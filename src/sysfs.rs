use std::fs;
use std::path::Path;
use crate::hal_log;

use std::sync::OnceLock;

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
    let _ = set_node("activate", "0");
    let _ = set_node("duration", &duration_ms.to_string());
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