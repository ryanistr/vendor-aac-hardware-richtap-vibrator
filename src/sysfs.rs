use std::env;
use std::fs;
use std::path::Path;
use crate::hal_log;

use std::sync::OnceLock;

// ERM pulse boost, merged from the erm-haptic daemon. LRA motors want the 8-10 ms
// pulses this HAL writes; an ERM needs tens of ms to spin up. Off unless the service
// block sets ERM_HAPTIC_ENABLE=1 - an LRA must never enable this, it uses gain instead.
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
            hal_log!("erm boost disabled - set ERM_HAPTIC_ENABLE=1 to extend short pulses");
            return None;
        }
        let threshold = read("ERM_HAPTIC_THRESHOLD_MS", DEFAULT_THRESHOLD_MS);
        let target = read("ERM_HAPTIC_TARGET_MS", DEFAULT_TARGET_MS);
        if threshold >= 1 && target > threshold && target <= MAX_TARGET_MS {
            hal_log!(
                "erm boost config - below {} ms becomes {} ms",
                threshold,
                target
            );
            Some((threshold, target))
        } else {
            hal_log!(
                "erm boost config invalid ({}, {}), boost disabled",
                threshold,
                target
            );
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
    static PREFIX: OnceLock<&'static str> = OnceLock::new();
    *PREFIX.get_or_init(|| {
        if Path::new("/sys/class/leds/vibrator_single").exists() {
            "/sys/class/leds/vibrator_single/"
        } else {
            "/sys/class/leds/vibrator/"
        }
    })
}

pub fn set_node(node: &str, value: &str) -> std::io::Result<()> {
    let path = format!("{}{}", get_vibrator_prefix(), node);
    hal_log!("sysfs::set_node() - writing '{}' to {}", value, path);
    fs::write(path, value)
}

#[allow(dead_code)]
pub fn get_node(node: &str) -> std::io::Result<String> {
    let path = format!("{}{}", get_vibrator_prefix(), node);
    let val = fs::read_to_string(&path).map(|s| s.trim().to_string());
    hal_log!("sysfs::get_node() - reading from {}: {:?}", path, val);
    val
}

pub fn has_node(node: &str) -> bool {
    let exists = Path::new(&format!("{}{}", get_vibrator_prefix(), node)).exists();
    hal_log!("sysfs::has_node() - node '{}' exists: {}", node, exists);
    exists
}

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
    crate::hal_log!("sysfs::set_index() - writing '{}' ", index);
    let _ = set_node("index", &index.to_string());
}