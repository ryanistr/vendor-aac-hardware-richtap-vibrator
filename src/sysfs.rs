use std::env;
use std::fs;
use std::path::Path;
use crate::hal_log;
use std::sync::OnceLock;

// ── Configuration ────────────────────────────────────────────────────────────

const DEFAULT_SYSFS_PREFIX: &str = "/sys/class/leds/vibrator_single";
const FALLBACK_SYSFS_PREFIX: &str = "/sys/class/leds/vibrator";

// ERM pulse boost (LRA uses gain instead). Off unless set by init rc.
const DEFAULT_THRESHOLD_MS: u32 = 50;
const DEFAULT_TARGET_MS: u32 = 80;
const MAX_TARGET_MS: u32 = 1000;

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
    PREFIX.get_or_init(|| {
        if Path::new(DEFAULT_SYSFS_PREFIX).exists() {
            DEFAULT_SYSFS_PREFIX.to_string()
        } else {
            // Check for odm-managed path as well
            if Path::new("/sys/class/odm/vibrator_single").exists() {
                "/sys/class/odm/vibrator_single".to_string()
            } else {
                FALLBACK_SYSFS_PREFIX.to_string()
            }
        }
    }).as_str()
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

// ── Vibration primitives ─────────────────────────────────────────────────────

pub fn on(duration_ms: u32) {
    let actual_ms = boosted(duration_ms);
    if actual_ms != duration_ms {
        hal_log!("sysfs::on() - boost {} ms to {} ms", duration_ms, actual_ms);
    }
    let _ = set_node("activate", "0");
    let _ = set_node("duration", &actual_ms.to_string());
    let _ = set_node("activate", "1");
}

pub fn off() {
    let _ = set_node("activate", "0");
}

pub fn set_amplitude(amplitude: f32) {
    if has_node("gain") {
        let gain = (amplitude * 255.0).clamp(0.0, 255.0) as u32;
        let _ = set_node("gain", &gain.to_string());
    }
}

pub fn set_index(index: u32) {
    hal_log!("sysfs::set_index() - writing '{}' ", index);
    let _ = set_node("index", &index.to_string());
}

// ── OEM duration clamping ────────────────────────────────────────────────────
//
// effect_waveform.xml carries RTP/OEM patterns with durations of 20-60 s
// (ringtones, alarms). The driver's `duration` node takes a u32 in ms, but a
// single very long activate is unreliable, and VibratorManagerService stops
// the vibration itself. Clamping here keeps a stuck motor from running away
// if the framework never sends the stop.
pub const MAX_OEM_PULSE_MS: u32 = 60_000;

pub fn on_clamped(duration_ms: u32) -> u32 {
    let clamped = duration_ms.min(MAX_OEM_PULSE_MS);
    on(clamped);
    clamped
}
