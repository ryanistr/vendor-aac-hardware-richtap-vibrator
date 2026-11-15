//! AIDL reimplementation of the OPLUS linear-motor vibrator HAL.
//!
//! The original is the HIDL binary
//! `vendor.oplus.hardware.lmvibrator@1.0-service`, shipped prebuilt and stripped
//! (38296 B). Its behaviour was reverse engineered and is recorded in
//! `ReconstructedHALSource/Oplus/OplusLmVibrator/NOTES.md`:
//!
//!   nodes   /sys/class/leds/vibrator/{vmax,gun_mode,gun_type,haptic_audio,
//!                                     bullet_nr,brightness,waveform_index,
//!                                     rtp,rtp_going}
//!           /proc/vibrator/touch_style
//!   logs    "<awinic debug> open dev  fail"
//!           "node write error,ret = %d" / "node write success,ret = %d"
//!
//! This port keeps the same method set and return convention (the byte count
//! of the write, or a negative errno), but adds a fallback for devices that do
//! not expose the AW8697 node set. The X6739 is one of those: it only has
//! `/sys/class/leds/vibrator_single` with `activate`/`duration`/`gain`/`index`,
//! so in fallback mode the calls are translated onto those nodes and the
//! HAL actually drives the motor instead of failing every write.

use rsbinder::{ Interface, status::Result as BinderResult };

use crate::vendor::oplus::hardware::lmvibrator::ILinearMotorVibrator::ILinearMotorVibrator;
use crate::hal_log;

use std::fs;
use std::path::{ Path, PathBuf };
use std::sync::atomic::{ AtomicI32, Ordering };

/// AW8697 node directory used by the original OPLUS HAL.
const AW8697_DIR: &str = "/sys/class/leds/vibrator";
/// AW22xxx single-node driver this device actually exposes.
const SINGLE_DIR: &str = "/sys/class/leds/vibrator_single";
/// Touch-style pseudo file used by OPLUS.
const TOUCH_STYLE: &str = "/proc/vibrator/touch_style";

/// Amplitude domain of `linearmotorVibratorOn` (HIDL passes a signed short).
const AMPLITUDE_MAX: i32 = 32767;

pub struct LmVibratorService {
    /// True when the AW8697 node set is present, false for the single-node fallback.
    aw8697: bool,
    /// Last touch style, used when /proc/vibrator/touch_style does not exist.
    touch_style: AtomicI32,
    /// Last amplitude applied, reported by getStatus().
    last_status: AtomicI32,
}

impl LmVibratorService {
    pub fn new() -> Self {
        let aw8697 = Path::new(AW8697_DIR).exists();
        hal_log!(
            "LmVibratorService::new() - {} mode ({})",
            if aw8697 { "AW8697" } else { "vibrator_single fallback" },
            if aw8697 { AW8697_DIR } else { SINGLE_DIR }
        );
        Self {
            aw8697,
            touch_style: AtomicI32::new(0),
            last_status: AtomicI32::new(0),
        }
    }

    fn node(&self, name: &str) -> PathBuf {
        let base = if self.aw8697 { AW8697_DIR } else { SINGLE_DIR };
        Path::new(base).join(name)
    }

    /// Mirrors the original's "node write success/error,ret = %d" logging and
    /// its return convention: bytes written, or a negative errno.
    fn write_node(&self, name: &str, value: &str) -> i32 {
        let path = self.node(name);
        match fs::write(&path, value) {
            Ok(()) => {
                let ret = value.len() as i32;
                hal_log!("node write success,ret = {} ({})", ret, path.display());
                ret
            }
            Err(e) => {
                hal_log!(
                    "<awinic debug> open dev  fail: {} ({})",
                    path.display(),
                    e
                );
                let ret = -e.raw_os_error().unwrap_or(1);
                hal_log!("node write error,ret = {}", ret);
                ret
            }
        }
    }

    fn read_touch_style(&self) -> i32 {
        match fs::read_to_string(TOUCH_STYLE) {
            Ok(s) => s.trim().parse::<i32>().unwrap_or(-1),
            Err(_) => self.touch_style.load(Ordering::SeqCst),
        }
    }

    fn write_touch_style(&self, style: i32) -> i32 {
        self.touch_style.store(style, Ordering::SeqCst);
        if Path::new(TOUCH_STYLE).exists() {
            self.write_node_abs(TOUCH_STYLE, &style.to_string())
        } else {
            style
        }
    }

    fn write_node_abs(&self, path: &str, value: &str) -> i32 {
        match fs::write(path, value) {
            Ok(()) => value.len() as i32,
            Err(e) => {
                let ret = -e.raw_os_error().unwrap_or(1);
                hal_log!("node write error,ret = {} ({})", ret, path);
                ret
            }
        }
    }
}

impl Interface for LmVibratorService {}

impl ILinearMotorVibrator for LmVibratorService {
    fn linearmotorVibratorOn(
        &self,
        timeout_ms: i32,
        amplitude: i32,
        is_gun_mode: bool,
    ) -> BinderResult<i32> {
        hal_log!(
            "ILinearMotorVibrator::linearmotorVibratorOn() - {}ms, amp {}, gun {}",
            timeout_ms,
            amplitude,
            is_gun_mode
        );
        self.last_status.store(amplitude, Ordering::SeqCst);

        if self.aw8697 {
            // Original AW8697 trigger path.
            self.write_node("brightness", &amplitude.to_string());
            self.write_node("waveform_index", "0");
            Ok(self.write_node("vmax", &timeout_ms.to_string()))
        } else {
            // Fallback: map the 0..32767 amplitude onto the 0..255 gain node and
            // trigger through `brightness` — that is the node the stock service
            // used on this AW22xxx driver (`activate`/`duration` do nothing).
            let gain = ((amplitude.max(0) as i64 * 255) / AMPLITUDE_MAX as i64)
                .clamp(0, 255);
            self.write_node("gain", &gain.to_string());
            Ok(self.write_node("brightness", &gain.to_string()))
        }
    }

    fn setVmax(&self, vmax: i32) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::setVmax() - {}", vmax);
        Ok(self.write_node("vmax", &vmax.to_string()))
    }

    fn getVibratorTouchStyle(&self) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::getVibratorTouchStyle()");
        Ok(self.read_touch_style())
    }

    fn write_gun_type(&self, r#type: i32) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::write_gun_type() - {}", r#type);
        Ok(self.write_node("gun_type", &r#type.to_string()))
    }

    fn linearmotorVibratorOff(&self) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::linearmotorVibratorOff()");
        self.last_status.store(0, Ordering::SeqCst);
        if self.aw8697 {
            Ok(self.write_node("brightness", "0"))
        } else {
            Ok(self.write_node("brightness", "0"))
        }
    }

    fn getStatus(&self) -> BinderResult<i32> {
        Ok(self.last_status.load(Ordering::SeqCst))
    }

    fn write_haptic_audio(&self, data: &str) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::write_haptic_audio() - {} bytes", data.len());
        Ok(self.write_node("haptic_audio", data))
    }

    fn write_bullet_num(&self, num: i32) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::write_bullet_num() - {}", num);
        Ok(self.write_node("bullet_nr", &num.to_string()))
    }

    fn write_gun_mode(&self, mode: i32) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::write_gun_mode() - {}", mode);
        Ok(self.write_node("gun_mode", &mode.to_string()))
    }

    fn setVibratorTouchStyle(&self, style: i32) -> BinderResult<i32> {
        hal_log!("ILinearMotorVibrator::setVibratorTouchStyle() - {}", style);
        Ok(self.write_touch_style(style))
    }
}
