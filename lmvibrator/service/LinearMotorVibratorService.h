/*
 * vendor.oplus.hardware.lmvibrator@1.0::ILinearMotorVibrator
 *
 * Source-built replacement for the shipped
 * /odm/bin/hw/vendor.oplus.hardware.lmvibrator@1.0-service, which drove
 * /sys/class/leds/vibrator/… — a node set that does not exist on the Infinix
 * X6739. This implementation resolves the node directory at runtime and uses
 * the one the platform actually exposes.
 */

#pragma once

#include <vendor/oplus/hardware/lmvibrator/1.0/ILinearMotorVibrator.h>

#include <atomic>
#include <cstdint>
#include <map>
#include <mutex>
#include <string>

namespace vendor::oplus::hardware::lmvibrator::implementation {

using ::android::hardware::Return;
using ::android::hardware::Void;
using ::android::hardware::hidl_string;

struct LinearMotorVibrator : public V1_0::ILinearMotorVibrator {
    LinearMotorVibrator();

    Return<void> linearmotorVibratorOn(int32_t waveformId, int16_t amplitude,
                                       bool isRtpMode) override;
    Return<void> linearmotorVibratorOff() override;
    Return<void> write_gun_type(int32_t gunType) override;
    Return<int32_t> getStatus() override;
    Return<void> setVmax(int32_t vmax) override;
    Return<void> write_bullet_num(int32_t bulletNum) override;
    Return<void> write_gun_mode(int32_t gunMode) override;
    Return<void> write_haptic_audio(const hidl_string& data) override;
    Return<void> setVibratorTouchStyle(int32_t touchStyle) override;
    Return<int32_t> getVibratorTouchStyle() override;

  private:
    // Writes "<root_>/<node>"; returns false when the node does not exist or
    // the write fails. Every failure is logged with the resolved path.
    bool writeNode(const char* node, const std::string& value);
    bool hasNode(const char* node) const;

    // Amplitude arrives as the framework's 0..2400 haptic strength, while the
    // driver's gain/brightness accept 0..255.
    static uint32_t scaleAmplitude(int32_t amplitude);

    // waveform id -> its entry in effect_waveform.xml. Durations are reused
    // across usages (id 2 is both a 45 ms touch effect and a 1044 ms
    // notification), so the longest entry wins and its usage drives the
    // synthesised pattern.
    struct WaveformInfo {
        uint32_t duration = 0;
        std::string usage;
        std::string type;
    };

    // One pulse of the real waveform envelope, extracted from the OOS15 RTP
    // firmware by tools/build_patterns.py.
    struct Segment {
        uint16_t startMs = 0;
        uint16_t durationMs = 0;
        uint8_t amplitude = 0;
        // Haptic character, from the source envelope: 0 tick, 1 click,
        // 2 thud, 3 heavy. The AW862xx has a handful of RAM waves, so this
        // selects which one is played instead of always using the same one.
        uint8_t kind = 0;
    };

    void loadWaveforms();
    void loadPatterns();
    uint32_t durationFor(int32_t waveformId) const;
    std::string usageFor(int32_t waveformId) const;
    void armSelfStop(uint32_t sequence, int32_t waveformId);

    // Replays the extracted envelope for this waveform. Returns false when the
    // id has no pattern, so the caller can fall back to a single pulse.
    bool playPattern(uint32_t sequence, int32_t waveformId, uint32_t amplitude);

    // Per-effect corrections measured on device (see NOTES.md). Returns a fixed
    // segment list whose durations are played verbatim, ignoring the intensity
    // ladder, or nullptr when the effect needs no correction.
    const std::vector<Segment>* fixedOverride(int32_t waveformId) const;
    void playFixed(uint32_t sequence, const std::vector<Segment>& segments);

    // Intensity is pulse duration: the driver's ram_config picks the wave from
    // the length, which is how stock/TWRP produce click -> thud -> heavy.
    static uint32_t scaleDuration(uint32_t baseMs, uint32_t strength);
    static uint32_t ladderDuration(uint32_t strength);

    // How a haptic is started on the AW862xx driver. `pattern` (default)
    // replays the waveform extracted from the OPLUS RTP firmware; `plain` is
    // one pulse only. `activate` and `index` are driver-level experiments kept
    // for probing the RAM waves.
    // Selected with `setprop persist.sys.lm.mode <pattern|plain|activate|index>`.
    static std::string triggerMode();

    std::string root_;
    std::map<int32_t, WaveformInfo> waveforms_;
    std::map<int32_t, std::vector<Segment>> patterns_;
    std::atomic<uint32_t> sequence_{0};
    std::mutex lock_;
    int32_t touchStyle_ = 10000;
};

}  // namespace vendor::oplus::hardware::lmvibrator::implementation
