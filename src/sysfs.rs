use std::env;
use std::fs;
use std::path::Path;
use crate::hal_log;
use std::sync::atomic::{ AtomicU32, AtomicUsize, Ordering };
use std::sync::OnceLock;

// ── Nodes ────────────────────────────────────────────────────────────────────
//
// The AW22xxx vibrator on this platform is an LED-class device. The stock
// Mediatek HIDL service (`android.hardware.vibrator-service.mediatek`, backed
// up in ReconstructedHALSource/Oplus/stock-vibrator-backup/) drives it through:
//
//     /sys/class/leds/vibrator_single/brightness   <- start/stop + amplitude
//     /sys/class/leds/vibrator_single/gain
//     /sys/class/leds/vibrator_single/{loop,rtp,seq}
//
// and never references `activate`, `duration` or `index` at all (verified with
// `strings` on the stock binary: 0 hits for each). Writing `activate`/`duration`
// therefore does nothing on this driver — `brightness` is the trigger. The stock
// service also relies on the LED core's own timeout/loop handling, so this port
// runs a software timer to clear `brightness` after the requested duration.

const DEFAULT_SYSFS_PREFIX: &str = "/sys/class/leds/vibrator_single";
const FALLBACK_SYSFS_PREFIX: &str = "/sys/class/leds/vibrator";

/// Full-scale amplitude for the `gain`/`brightness` nodes.
const AMPLITUDE_MAX: u32 = 255;

// ERM pulse boost (LRA uses gain instead). Off unless set by init rc.
const DEFAULT_THRESHOLD_MS: u32 = 50;
const DEFAULT_TARGET_MS: u32 = 80;
const MAX_TARGET_MS: u32 = 1000;

/// Amplitude applied on the next `on()`, set by setAmplitude()/setDynamicScale().
static AMPLITUDE: AtomicU32 = AtomicU32::new(AMPLITUDE_MAX);
/// Generation counter so a superseded auto-off timer does not cut a new pulse.
static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

fn boost_config() -> Option<(u32, u32)> {
    static CFG: OnceLock<Option<(u32, u32)>> = OnceLock::new();
    *CFG.get_or_init(|| {
        let read = |name: &str, default: u32| -> u32 {
            env::var(name)
                .ok()
                .and_then(|s| s.trim().parse::<u32>().ok())
                .unwrap_or(default)
        };
        if read("ERM_HAPTIC_ENABLE", 0) != 1 {
            return None;
        }
        let threshold = read("ERM_HAPTIC_THRESHOLD_MS", DEFAULT_THRESHOLD_MS);
        let target = read("ERM_HAPTIC_TARGET_MS", DEFAULT_TARGET_MS);
        if threshold >= 1 && target > threshold && target <= MAX_TARGET_MS {
            Some((threshold, target))
        } else {
            None
        }
    })
}

fn boosted(duration_ms: u32) -> u32 {
    match boost_config() {
        Some((threshold, target)) if duration_ms > 0 && duration_ms < threshold => target,
        _ => duration_ms,
    }
}

fn get_vibrator_prefix() -> &'static str {
    static PREFIX: OnceLock<String> = OnceLock::new();
    PREFIX
        .get_or_init(|| {
            if Path::new(DEFAULT_SYSFS_PREFIX).exists() {
                DEFAULT_SYSFS_PREFIX.to_string()
            } else if Path::new("/sys/class/odm/vibrator_single").exists() {
                "/sys/class/odm/vibrator_single".to_string()
            } else {
                FALLBACK_SYSFS_PREFIX.to_string()
            }
        })
        .as_str()
}

// ── Core node operations ─────────────────────────────────────────────────────

pub fn set_node(node: &str, value: &str) -> std::io::Result<()> {
    let path = format!("{}/{}", get_vibrator_prefix(), node);
    hal_log!("sysfs::set_node() - writing '{}' to {}", value, path);
    fs::write(path, value)
}

#[allow(dead_code)]
pub fn get_node(node: &str) -> std::io::Result<String> {
    let path = format!("{}/{}", get_vibrator_prefix(), node);
    let val = fs::read_to_string(&path).map(|s| s.trim().to_string());
    hal_log!("sysfs::get_node() - reading from {}: {:?}", path, val);
    val
}

pub fn has_node(node: &str) -> bool {
    Path::new(&format!("{}/{}", get_vibrator_prefix(), node)).exists()
}

// ── Trigger path ─────────────────────────────────────────────────────────────

/// Same clamp as the OPLUS OEM path: very long RTP durations must not be able
/// to leave the motor running if the framework never sends the stop.
pub const MAX_OEM_PULSE_MS: u32 = 60_000;

fn apply_amplitude(amplitude: u32) {
    let amp = amplitude.min(AMPLITUDE_MAX);
    AMPLITUDE.store(amp, Ordering::SeqCst);
    // `gain` must be in place before `brightness` starts the motor.
    let _ = set_node("gain", &amp.to_string());
}

fn start_motor() {
    let amp = AMPLITUDE.load(Ordering::SeqCst).max(1);
    let _ = set_node("brightness", &amp.to_string());
}

fn stop_motor() {
    let _ = set_node("brightness", "0");
}

/// Start a timed vibration. Returns the duration actually applied.
pub fn on(duration_ms: u32) {
    let total = boosted(duration_ms.max(1)).min(MAX_OEM_PULSE_MS);
    let seq = SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;

    start_motor();

    if total == 0 || total == u32::MAX {
        return;
    }

    // The driver has no usable `duration` node, so schedule the stop ourselves.
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(total as u64));
        if SEQUENCE.load(Ordering::SeqCst) == seq {
            stop_motor();
        }
    });
}

pub fn off() {
    SEQUENCE.fetch_add(1, Ordering::SeqCst);
    stop_motor();
}

/// amplitude is 0.0..=1.0 for the AIDL IVibrator contract.
pub fn set_amplitude(amplitude: f32) {
    if !has_node("brightness") {
        return;
    }
    let a = (amplitude.clamp(0.0, 1.0) * AMPLITUDE_MAX as f32) as u32;
    apply_amplitude(a);
}

/// `index` is not a working node on this driver; kept for call-site
/// compatibility and logged so the mismatch is visible in a logcat trace.
pub fn set_index(index: u32) {
    hal_log!("sysfs::set_index() - ignored (AW22xxx has no index node), value {}", index);
}
