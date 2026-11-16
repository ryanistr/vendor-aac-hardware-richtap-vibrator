#include "LinearMotorVibratorService.h"

#include <android/log.h>
#include <errno.h>
#include <fcntl.h>
#include <string.h>
#include <sys/system_properties.h>
#include <unistd.h>

#include <algorithm>
#include <chrono>
#include <cstring>
#include <iterator>
#include <fstream>
#include <sstream>
#include <thread>

#define LOG_TAG "LmVibratorHAL"
#define LOGI(...) __android_log_print(ANDROID_LOG_INFO, LOG_TAG, __VA_ARGS__)
#define LOGE(...) __android_log_print(ANDROID_LOG_ERROR, LOG_TAG, __VA_ARGS__)

namespace vendor::oplus::hardware::lmvibrator::implementation {

namespace {

// Candidate sysfs roots, in order. The shipped service only knew
// /sys/class/leds/vibrator; the X6739 exposes vibrator_single instead.
constexpr const char* kRoots[] = {
    "/sys/class/leds/vibrator_single",
    "/sys/class/leds/vibrator",
    "/sys/class/odm/vibrator_single",
};

// Directories that may hold the OPLUS waveform table.
constexpr const char* kWaveformDirs[] = {
    "/vendor/etc/vibrator",
    "/my_product/etc/vibrator",
    "/odm/etc/vibrator",
    "/vendor/odm/etc/vibrator",
    "/system/my_product/etc/vibrator",
};

constexpr uint32_t kMaxPulseMs = 60000;
// Used when a waveform id has no entry in effect_waveform.xml.
constexpr uint32_t kDefaultPulseMs = 40;
// Longest single LED-triggered pulse; below the LED core's max_brightness
// (255) so the driver never clamps a chunk.
constexpr uint32_t kMaxChunkMs = 200;

std::string attr(const std::string& tag, const char* key) {
    const std::string prefix = std::string(" ") + key + "=\"";
    size_t start = tag.find(prefix);
    if (start == std::string::npos) return {};
    start += prefix.size();
    size_t end = tag.find('"', start);
    if (end == std::string::npos) return {};
    return tag.substr(start, end - start);
}

}  // namespace

LinearMotorVibrator::LinearMotorVibrator() {
    for (const char* candidate : kRoots) {
        const std::string brightness = std::string(candidate) + "/brightness";
        if (access(brightness.c_str(), W_OK) == 0) {
            root_ = candidate;
            break;
        }
    }

    if (root_.empty()) {
        root_ = kRoots[0];
        LOGE("no writable vibrator node set found, defaulting to %s", root_.c_str());
    } else {
        LOGI("using node set %s", root_.c_str());
    }

    for (const char* node : {"brightness", "gain", "index", "waveform_index", "rtp",
                             "rtp_going", "vmax", "gun_type", "gun_mode", "bullet_nr",
                             "haptic_audio"}) {
        LOGI("node %-16s %s", node, hasNode(node) ? "present" : "absent");
    }

    loadWaveforms();
    loadPatterns();
}

bool LinearMotorVibrator::hasNode(const char* node) const {
    const std::string path = root_ + "/" + node;
    return access(path.c_str(), F_OK) == 0;
}

bool LinearMotorVibrator::writeNode(const char* node, const std::string& value) {
    const std::string path = root_ + "/" + node;
    int fd = open(path.c_str(), O_WRONLY | O_CLOEXEC);
    if (fd < 0) {
        LOGI("write %s='%s' skipped: %s", path.c_str(), value.c_str(), strerror(errno));
        return false;
    }
    ssize_t written = write(fd, value.data(), value.size());
    int savedErrno = errno;
    close(fd);
    if (written != static_cast<ssize_t>(value.size())) {
        LOGE("write %s='%s' failed: %s", path.c_str(), value.c_str(), strerror(savedErrno));
        return false;
    }
    return true;
}

uint32_t LinearMotorVibrator::scaleAmplitude(int32_t amplitude) {
    if (amplitude <= 0) return 0;
    // Already in the driver's gain domain (standard AIDL amplitude).
    if (amplitude <= 255) return static_cast<uint32_t>(amplitude);

    // OPLUS stepless scale: 700 (minimum) .. 2400 (maximum). The whole slider
    // range is mapped onto 1..255; scaling from 0 only ever used the top half,
    // which is why the intensity slider felt like it did nothing.
    const int32_t floor = 700;
    if (amplitude <= floor) return 1;

    int64_t scaled = static_cast<int64_t>(amplitude - floor) * 255 / (2400 - floor);
    if (scaled <= 0) scaled = 1;
    if (scaled > 255) scaled = 255;
    return static_cast<uint32_t>(scaled);
}

Return<void> LinearMotorVibrator::linearmotorVibratorOn(int32_t waveformId, int16_t amplitude,
                                                        bool isRtpMode) {
    const uint32_t gain = scaleAmplitude(amplitude);
    const uint32_t seq = ++sequence_;
    const std::string mode = triggerMode();
    uint32_t pulse = durationFor(waveformId);
    if (pulse == 0) pulse = kDefaultPulseMs;
    if (pulse > kMaxPulseMs) pulse = kMaxPulseMs;

    LOGI("on: waveform=%d amplitude=%d gain=%u rtp=%d duration=%u mode=%s", waveformId, amplitude,
         gain, isRtpMode, pulse, mode.c_str());

    // Amplitude always goes to `gain` first: it is the node the driver applies
    // at playback time (`ram_vbat_comp: ram vbat comp close, set_gain 0xNN`).
    writeNode("gain", std::to_string(gain));

    if (mode == "activate") {
        // `duration` + `activate` makes the driver pick the RAM wave itself
        // (`ram_config: duration %d, select index %d`) and, for anything at or
        // above the DT threshold, loop it with wavloop=15 for the whole effect
        // length. On this chip that turns a tap into a heavy thud and a
        // notification into a 1-4 s rumble, so it is opt-in only.
        writeNode("duration", std::to_string(pulse));
        if (!writeNode("activate", "1")) {
            LOGE("on: could not start the motor");
        }
        return Void();
    }

    // Default: replay the waveform extracted from the OPLUS firmware. Ids with
    // no extracted pattern (and <c>plain</c> mode) stay a single LED-triggered
    // pulse of the effect's own length.
    if (mode != "plain") {
        if (const std::vector<Segment>* override = fixedOverride(waveformId)) {
            playFixed(seq, *override);
            return Void();
        }
    }

    if (mode != "plain" && playPattern(seq, waveformId, amplitude)) {
        return Void();
    }

    // No extracted pattern: single pulse through duration + activate, with the
    // length scaled by intensity (TWRP does exactly this: 39 -> 120 -> 300 ms).
    //
    // Short effects (touch, keyboard, clicks) stay in a click-sized range:
    // letting them ride the full ladder turns a key press into a thud, which is
    // exactly what happened before. They get 15..50 ms so even full intensity
    // is a crisp tap, not a thump.
    uint32_t scaled;
    if (pulse <= 80) {
        int32_t s = static_cast<int32_t>(amplitude);
        if (s < 700) s = 700;
        if (s > 2400) s = 2400;
        scaled = 15 + static_cast<uint32_t>((s - 700) * 35 / 1700);
    } else {
        scaled = ladderDuration(amplitude);
    }
    LOGI("on: single pulse wave=%d strength=%u dur=%u->%u ms", waveformId, amplitude, pulse,
         scaled);
    writeNode("gain", "255");
    writeNode("duration", std::to_string(scaled));
    if (!writeNode("activate", "1")) {
        LOGE("on: could not start the motor");
    }
    armSelfStop(seq, waveformId);
    return Void();
}

Return<void> LinearMotorVibrator::linearmotorVibratorOff() {
    ++sequence_;
    LOGI("off");
    // `activate=0` runs vibrator_work in the stop branch and clears the
    // driver's timer; `brightness=0` covers the legacy LED path.
    writeNode("activate", "0");
    writeNode("brightness", "0");
    writeNode("rtp_going", "0");
    return Void();
}

Return<void> LinearMotorVibrator::write_gun_type(int32_t gunType) {
    LOGI("write_gun_type(%d)", gunType);
    writeNode("gun_type", std::to_string(gunType));
    return Void();
}

Return<int32_t> LinearMotorVibrator::getStatus() {
    // 0 == idle. The motor is only driven for the duration of a waveform, and
    // the framework tracks its own playback state.
    return 0;
}

Return<void> LinearMotorVibrator::setVmax(int32_t vmax) {
    LOGI("setVmax(%d)", vmax);
    if (!writeNode("vmax", std::to_string(vmax))) {
        writeNode("gain", std::to_string(scaleAmplitude(vmax)));
    }
    return Void();
}

Return<void> LinearMotorVibrator::write_bullet_num(int32_t bulletNum) {
    LOGI("write_bullet_num(%d)", bulletNum);
    writeNode("bullet_nr", std::to_string(bulletNum));
    return Void();
}

Return<void> LinearMotorVibrator::write_gun_mode(int32_t gunMode) {
    LOGI("write_gun_mode(%d)", gunMode);
    writeNode("gun_mode", std::to_string(gunMode));
    return Void();
}

Return<void> LinearMotorVibrator::write_haptic_audio(const hidl_string& data) {
    LOGI("write_haptic_audio(%zu bytes)", static_cast<size_t>(data.size()));
    writeNode("haptic_audio", std::string(data.c_str(), data.size()));
    return Void();
}

Return<void> LinearMotorVibrator::setVibratorTouchStyle(int32_t touchStyle) {
    LOGI("setVibratorTouchStyle(%d)", touchStyle);
    std::lock_guard<std::mutex> guard(lock_);
    touchStyle_ = touchStyle;
    return Void();
}

Return<int32_t> LinearMotorVibrator::getVibratorTouchStyle() {
    std::lock_guard<std::mutex> guard(lock_);
    return touchStyle_;
}

void LinearMotorVibrator::loadWaveforms() {
    for (const char* dir : kWaveformDirs) {
        const std::string path = std::string(dir) + "/effect_waveform.xml";
        std::ifstream file(path);
        if (!file.is_open()) continue;

        std::stringstream buffer;
        buffer << file.rdbuf();
        const std::string xml = buffer.str();

        size_t pos = 0;
        size_t count = 0;
        while ((pos = xml.find("<effect ", pos)) != std::string::npos) {
            size_t end = xml.find("/>", pos);
            if (end == std::string::npos) break;
            const std::string tag = xml.substr(pos, end - pos);

            const std::string waveformId = attr(tag, "waveform_id");
            const std::string duration = attr(tag, "duration");
            if (!waveformId.empty() && !duration.empty()) {
                const int32_t id = std::atoi(waveformId.c_str());
                const uint32_t ms = static_cast<uint32_t>(std::atoi(duration.c_str()));
                auto& entry = waveforms_[id];
                // Ids are reused across usages; the longest entry defines the
                // effect length and its usage drives the synthesised pattern.
                if (ms > entry.duration) {
                    entry.duration = ms;
                    entry.usage = attr(tag, "usage");
                    entry.type = attr(tag, "type");
                }
                ++count;
            }
            pos = end;
        }

        if (count > 0) {
            LOGI("loaded %zu waveform entries from %s", count, path.c_str());
            return;
        }
    }
    LOGI("no effect_waveform.xml found; falling back to a fixed pulse");
}

uint32_t LinearMotorVibrator::durationFor(int32_t waveformId) const {
    auto it = waveforms_.find(waveformId);
    return it == waveforms_.end() ? 0 : it->second.duration;
}

std::string LinearMotorVibrator::usageFor(int32_t waveformId) const {
    auto it = waveforms_.find(waveformId);
    return it == waveforms_.end() ? std::string() : it->second.usage;
}

void LinearMotorVibrator::loadPatterns() {
    static const char* kNames[] = {"patterns.bin", "vibrator_patterns.bin"};
    for (const char* dir : kWaveformDirs) {
        for (const char* name : kNames) {
            const std::string path = std::string(dir) + "/" + name;
            std::ifstream file(path, std::ios::binary);
            if (!file.is_open()) continue;

            std::string blob((std::istreambuf_iterator<char>(file)),
                             std::istreambuf_iterator<char>());
            const bool v2 = blob.size() >= 8 && blob.compare(0, 4, "LMP2") == 0;
            if (blob.size() < 8 || (!v2 && blob.compare(0, 4, "LMP1") != 0)) {
                LOGE("pattern table %s has a bad header", path.c_str());
                continue;
            }
            const size_t segmentSize = v2 ? 6 : 5;

            size_t offset = 8;
            uint32_t count = 0;
            memcpy(&count, blob.data() + 4, 4);
            for (uint32_t i = 0; i < count && offset + 8 <= blob.size(); ++i) {
                int32_t id = 0;
                uint32_t segments = 0;
                memcpy(&id, blob.data() + offset, 4);
                memcpy(&segments, blob.data() + offset + 4, 4);
                offset += 8;

                std::vector<Segment> list;
                for (uint32_t s = 0; s < segments && offset + segmentSize <= blob.size(); ++s) {
                    Segment segment;
                    memcpy(&segment.startMs, blob.data() + offset, 2);
                    memcpy(&segment.durationMs, blob.data() + offset + 2, 2);
                    segment.amplitude = static_cast<uint8_t>(blob[offset + 4]);
                    segment.kind = v2 ? static_cast<uint8_t>(blob[offset + 5]) : 0;
                    offset += segmentSize;
                    list.push_back(segment);
                }
                patterns_[id] = std::move(list);
            }
            LOGI("loaded %zu waveform patterns from %s", patterns_.size(), path.c_str());
            return;
        }
    }
    LOGI("no pattern table found; long effects fall back to a single pulse");
}

uint32_t LinearMotorVibrator::scaleDuration(uint32_t baseMs, uint32_t strength) {
    // Pattern segments keep their own rhythm; intensity leans them lighter or
    // heavier, continuously: 0.7x at the slider minimum, 1.0x at the middle,
    // 1.4x at the maximum. Capped at 300 ms so a long segment never falls into
    // the driver's looping "long vibration" bucket.
    int32_t s = static_cast<int32_t>(strength);
    if (s < 700) s = 700;
    if (s > 2400) s = 2400;
    // Pattern segments keep their own rhythm; the slider leans them from
    // light to heavy. Factors chosen so the mid slider value plays the
    // extracted pattern at its true timing.
    uint32_t factor;      // per mille
    if (s < 1100) factor = 400;
    else if (s < 1600) factor = 750;
    else if (s < 2100) factor = 1000;
    else factor = 1600;
    uint32_t ms = baseMs * factor / 1000;
    if (ms < 15) ms = 15;
    if (ms > 200) ms = 200;
    return ms;
}

uint32_t LinearMotorVibrator::ladderDuration(uint32_t strength) {
    // TWRP writes its slider value straight into `duration` (measured live:
    // 39 / 120 / 300 ms) and the driver turns that length into the wave, which
    // is the click -> thud -> heavy ladder stock has. These four steps were
    // picked by ear on this device and replace TWRP's range.
    int32_t s = static_cast<int32_t>(strength);
    if (s < 700) s = 700;
    if (s > 2400) s = 2400;
    // Measured on device with tools/dur.sh, one value per feel:
    //   20 ms -> click, 60 ms -> thud, 90 ms -> weighted thud, 200 ms -> heavy
    if (s < 1100) return 20;
    if (s < 1600) return 60;
    if (s < 2100) return 90;
    return 200;
}



const std::vector<LinearMotorVibrator::Segment>* LinearMotorVibrator::fixedOverride(
        int32_t waveformId) const {
    // Effects whose name/intent contradicts what the extracted data produces.
    // Verified by ear on device; durations are absolute and do not follow the
    // intensity ladder.
    static const std::map<int32_t, std::vector<Segment>> kOverrides = {
            // fingerprint enrollment arrives inverted: good scan -> click,
            // failure -> heavy (see NOTES.md)
            {3,     {{0, 20, 255, 1}}},
            {6,     {{0, 200, 255, 3}}},
            // name says light / short
            {254,   {{0, 60, 255, 1}}},
            {342,   {{0, 60, 255, 1}}},
            // "clear all notifications" reuses effect_customized_spread_out; a
            // dismissal should confirm crisply, not thump
            {47,    {{0, 40, 255, 1}}},
            // recents "clear all" (one_key_clear_recent_task_feedback)
            {363,   {{0, 40, 255, 1}}},
            // game "weak" must not be a 200 ms thud
            {414,   {{0, 50, 255, 1}}},
            // name says long press / long gesture
            {65537, {{0, 90, 255, 2}}},
            {171,   {{0, 90, 255, 2}}},
            {178,   {{0, 90, 255, 2}}},
            // "moderate short vibrate twice" = two moderate hits, not one blob
            {46,    {{0, 110, 255, 2}, {130, 110, 255, 2}}},
    };
    auto it = kOverrides.find(waveformId);
    return it == kOverrides.end() ? nullptr : &it->second;
}

void LinearMotorVibrator::playFixed(uint32_t sequence, const std::vector<Segment>& segments) {
    LOGI("override: %zu fixed segments", segments.size());
    std::thread([this, sequence, segments]() {
        uint32_t elapsed = 0;
        for (const Segment& segment : segments) {
            if (sequence_.load() != sequence) return;
            if (segment.startMs > elapsed) {
                std::this_thread::sleep_for(std::chrono::milliseconds(segment.startMs - elapsed));
                elapsed = segment.startMs;
                if (sequence_.load() != sequence) return;
            }
            writeNode("gain", "255");
            writeNode("duration", std::to_string(segment.durationMs));
            if (!writeNode("activate", "1")) {
                LOGE("override: activate failed");
                return;
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(segment.durationMs));
            elapsed += segment.durationMs;
            if (sequence_.load() != sequence) return;
            writeNode("activate", "0");
        }
    }).detach();
}

bool LinearMotorVibrator::playPattern(uint32_t sequence, int32_t waveformId, uint32_t amplitude) {
    auto it = patterns_.find(waveformId);
    if (it == patterns_.end() || it->second.empty()) return false;

    const std::vector<Segment> segments = it->second;
    uint32_t total = 0;
    for (const Segment& segment : segments) {
        const uint32_t end = static_cast<uint32_t>(segment.startMs) + segment.durationMs;
        if (end > total) total = end;
    }
    if (total > kMaxPulseMs) total = kMaxPulseMs;
    uint32_t kinds[4] = {0, 0, 0, 0};
    for (const Segment& segment : segments) {
        if (segment.kind < 4) ++kinds[segment.kind];
    }
    const uint32_t strength = amplitude;
    LOGI("pattern: waveform %d (%s) %zu segments, %u ms, strength=%u, "
         "tick=%u click=%u thud=%u heavy=%u",
         waveformId, usageFor(waveformId).c_str(), segments.size(), total, strength, kinds[0],
         kinds[1], kinds[2], kinds[3]);

    std::thread([this, sequence, segments, strength]() {
        uint32_t elapsed = 0;
        size_t index = 0;
        for (const Segment& segment : segments) {
            ++index;
            if (sequence_.load() != sequence) return;
            if (segment.startMs > elapsed) {
                std::this_thread::sleep_for(
                        std::chrono::milliseconds(segment.startMs - elapsed));
                elapsed = segment.startMs;
                if (sequence_.load() != sequence) return;
            }

            // Play through duration + activate, not brightness: that is the
            // path that runs the driver\'s ram_config, which selects the RAM
            // wave from the pulse length. It is also exactly what TWRP and the
            // stock HAL use, and it is where click/thud/heavy come from.
            // `gain` is inert for RAM playback, so intensity is length-scaled:
            // longer pulse -> the driver picks a heavier wave.
            uint32_t scaled = scaleDuration(segment.durationMs, strength);

            // Never let a scaled segment run into the next one: dense patterns
            // (the charging animation is segments every 20-50 ms) otherwise
            // smear into one machine-gun burst because intensity stretches the
            // pulse past the gap that separates it from the next.
            if (index < segments.size()) {
                const uint32_t space = segments[index].startMs - segment.startMs;
                if (space > 0 && scaled > space) {
                    scaled = space > 15 ? space : 15;
                }
            }

            LOGI("seg %zu/%zu kind=%u strength=%u dur=%u->%u ms", index, segments.size(),
                 segment.kind, strength, segment.durationMs, scaled);
            writeNode("gain", "255");
            writeNode("duration", std::to_string(scaled));
            if (!writeNode("activate", "1")) {
                LOGE("seg %zu: activate failed", index);
            }
            std::this_thread::sleep_for(std::chrono::milliseconds(scaled));
            elapsed += scaled;
            if (sequence_.load() != sequence) return;
            writeNode("activate", "0");
        }
    }).detach();
    return true;
}

std::string LinearMotorVibrator::triggerMode() {
    char value[PROP_VALUE_MAX] = {0};
    const int len = __system_property_get("persist.sys.lm.mode", value);
    if (len <= 0) return "pattern";
    return std::string(value, static_cast<size_t>(len));
}

void LinearMotorVibrator::armSelfStop(uint32_t sequence, int32_t waveformId) {
    uint32_t ms = durationFor(waveformId);

    if (ms == 0) {
        LOGI("waveform %d has no known duration; leaving the stop to off()", waveformId);
        return;
    }
    if (ms > kMaxPulseMs) ms = kMaxPulseMs;

    std::thread([this, sequence, ms]() {
        std::this_thread::sleep_for(std::chrono::milliseconds(ms));
        if (sequence_.load() != sequence) return;
        LOGI("self-stop after %u ms", ms);
        writeNode("activate", "0");
        writeNode("brightness", "0");
    }).detach();
}

}  // namespace vendor::oplus::hardware::lmvibrator::implementation
