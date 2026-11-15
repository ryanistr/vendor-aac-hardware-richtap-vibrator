//! OPlus VibratorService effect mapping adapter.
//!
//! Parses the XML configs used by OPlus VibratorManagerService so RichTap can
//! route OEM‑custom effect IDs to the correct sysfs commands instead of falling
//! through to a generic 10 ms pulse.
//!
//! Expected config locations (checked in order):
//!   /vendor/odm/etc/vibrat/effect_waveform.xml          (ODM vibra maps)
//!   /system/my_product/etc/vibrat/effect_waveform.xml    (merged my_product)
//!
//! Per‑package IME duration overrides live in:
//!   /vendor/odm/etc/vibrat_service_config/inputmethod_duration_map.xml
//!
//! NOTE: RichTap does not see the Binder UID of the caller, so we cannot
//! enforce per‑IME min/max durations at the HAL layer.  That logic belongs
//! in VibratorManagerService.  We only store the table so it remains
//! available for future extensions or debugging.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use crate::hal_log;

// ── Data structures ────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct VibraEffect {
    pub effect_id: i32,
    pub waveform_id: i32,
    pub duration_ms: u32,       // effective duration after IME override (if known)
    pub typ: String,            // "RAM" | "RTP"
    pub usage: String,          // TOUCH | RINGTONE | NOTIFICATION | ALARM …
}

#[derive(Debug, Default)]
pub struct PlsVibraConfig {
    /// effect_id → VibraEffect
    effects: HashMap<i32, VibraEffect>,
    /// IME package → (min_ms, max_ms)
    ime_durations: HashMap<String, (u32, u32)>,
}

impl PlsVibraConfig {
    // ── Public API ────────────────────────────────────────────────────────

    pub fn new() -> Self {
        let mut cfg = Self::default();
        cfg.load_configs();
        hal_log!(
            "OPlus vbr: loaded {} effects{}, {} IME packages",
            cfg.effects.len(),
            if cfg.has_rtp_effects() { ", incl RTP" } else { "" },
            cfg.ime_durations.len()
        );
        cfg
    }

    /// Look up an OEM effect by its numeric id (the value VibratorManagerService
    /// passes when it calls perform(effect_id)).  Falls back to `(0,0)` which
    /// signals "no mapping found".
    pub fn get_effect(&self, effect_id: i32) -> Option<&VibraEffect> {
        self.effects.get(&effect_id)
    }

    /// Check whether there are any RTP‑type effects loaded.
    pub fn has_rtp_effects(&self) -> bool {
        self.effects.values().any(|e| e.typ == "RTP")
    }

    /// Number of OEM effects loaded from the XML configs.
    pub fn effect_count(&self) -> usize {
        self.effects.len()
    }

    /// Whether any IME duration overrides were loaded (for diagnostics).
    pub fn has_ime_overrides(&self) -> bool {
        !self.ime_durations.is_empty()
    }
}

// ── Loading ─────────────────────────────────────────────────────────────────

/// Candidate directories holding `effect_waveform.xml`, in lookup order.
/// On this port `my_product` is merged into the system image but mounted/visible
/// at `/my_product`; OPLUS devices also ship a copy under `/odm/etc`.
fn waveform_dirs() -> Vec<&'static str> {
    vec![
        "/my_product/etc/vibrator",
        "/odm/etc/vibrator",
        "/system/my_product/etc/vibrator",
        "/vendor/odm/etc/vibrator",
    ]
}

/// Candidate directories holding `inputmethod_duration_map.xml`.
fn service_config_dirs() -> Vec<&'static str> {
    vec![
        "/my_product/etc/vibrator_service_config",
        "/odm/etc/vibrator_service_config",
        "/system/my_product/etc/vibrator_service_config",
    ]
}

fn load_xml_text(base: &str, filename: &str) -> Option<String> {
    let p = format!("{}/{filename}", base);
    if Path::new(&p).exists() {
        Some(fs::read_to_string(&p).unwrap_or_default())
    } else {
        None
    }
}

impl PlsVibraConfig {
    fn load_configs(&mut self) {
        for dir in waveform_dirs() {
            self.load_effect_waveform(dir);
        }
        for dir in service_config_dirs() {
            if self.load_inputmethod_duration(dir) {
                break;
            }
        }
    }

    fn load_effect_waveform(&mut self, dir: &str) {
        let text = match load_xml_text(dir, "effect_waveform.xml") {
            Some(t) => t,
            None => return,           // no waveform file – nothing to load
        };
        let parsed = parse_effect_waveform(&text);
        let count = parsed.len();
        for eff in parsed {
            self.effects.insert(eff.effect_id, eff);
        }
        if count > 0 {
            hal_log!("OPlus vbr: loaded {} effects from {}", count, dir);
        }
    }

    /// Returns true when a duration map was found and parsed.
    fn load_inputmethod_duration(&mut self, dir: &str) -> bool {
        let text = match load_xml_text(dir, "inputmethod_duration_map.xml") {
            Some(t) => t,
            None => return false,
        };
        let before = self.ime_durations.len();
        self.parse_ime_map(dir, &text);
        if self.ime_durations.len() > before {
            hal_log!(
                "OPlus vbr: {} IME duration overrides from {}",
                self.ime_durations.len() - before,
                dir
            );
        }
        true
    }

    fn parse_ime_map(&mut self, _base: &str, text: &str) {
        // Extract <durationmap packagename="…"  min="…"  max="…"/>
        for line in text.lines() {
            let trimmed = line.trim();
            if !trimmed.starts_with("<durationmap ") { continue; }

            fn attr(line: &str, key: &str) -> Option<u32> {
                let prefix = format!("{key}=\"");
                if let Some(pos) = line.find(&prefix) {
                    let rest = &line[pos + prefix.len()..];
                    if let Some(end) = rest.find('"') {
                        rest[..end].parse().ok()
                    } else {
                        None
                    }
                } else {
                    None
                }
            }

            if let Some(pkg_pos) = trimmed.find("packagename=\"") {
                let after_pkg = &trimmed[pkg_pos + 13..];
                if let Some(pkg_end) = after_pkg.find('"') {
                    let pkg = &after_pkg[..pkg_end];
                    let mn = attr(trimmed, "min").unwrap_or(0);
                    let mx = attr(trimmed, "max").unwrap_or(u32::MAX);
                    self.ime_durations.insert(pkg.to_string(), (mn, mx));
                }
            }
        }
    }
}

// ── Minimal XML parsing ────────────────────────────────────────────────────

/// Parse `<effect effect_id="…" waveform_id="…" duration="…" type="…" usage="…"/>`
/// entries from *one* XML string.  Returns a vector of VibraEffect structs.
fn parse_effect_waveform(text: &str) -> Vec<VibraEffect> {
    let mut result = Vec::new();

    for line in text.lines() {
        let trimmed = line.trim();
        if !trimmed.starts_with("<effect ") { continue; }

        fn extract_attr<'a>(line: &'a str, key: &str) -> Option<&'a str> {
            let prefix = format!("{key}=\"");
            if let Some(start) = line.find(&prefix) {
                let after = &line[start + prefix.len()..];
                if let Some(end) = after.find('"') {
                    return Some(&after[..end]);
                }
            }
            None
        }

        if let (Some(eid_str), Some(wid_str), Some(dur_str), Some(typ), Some(usage)) = (
            extract_attr(trimmed, "effect_id"),
            extract_attr(trimmed, "waveform_id"),
            extract_attr(trimmed, "duration"),
            extract_attr(trimmed, "type"),
            extract_attr(trimmed, "usage"),
        ) {
            if let (Ok(eid), Ok(wid), Ok(dur)) =
                (eid_str.parse::<i32>(), wid_str.parse::<i32>(), dur_str.parse::<u32>())
            {
                result.push(VibraEffect {
                    effect_id: eid,
                    waveform_id: wid,
                    duration_ms: dur,
                    typ: typ.to_string(),
                    usage: usage.to_string(),
                });
            }
        }
    }

    result
}
