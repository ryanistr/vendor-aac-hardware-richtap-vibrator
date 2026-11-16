use rsbinder::{ Interface, Strong, status::Result as BinderResult };
use crate::vendor::aac::hardware::richtap::vibrator::IRichtapVibrator::IRichtapVibrator;
use crate::vendor::aac::hardware::richtap::vibrator::IRichtapCallback::IRichtapCallback;
use crate::pls_vibra_config::PlsVibraConfig;
use crate::sysfs;
use crate::hal_log;
use std::sync::atomic::{ AtomicUsize, Ordering };
use std::thread;
use std::time::Duration;

static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

/// Upper bound on a single pulsed vibration. RTP/OEM ringtone entries in
/// effect_waveform.xml carry durations of 20-60 s; the framework stops the
/// vibration itself, so this only guards against a stuck motor.


pub struct RichtapTranslator {
    /// OEM effect definitions loaded from the OPlus VibratorService XML
    /// (effect_waveform.xml / inputmethod_duration_map.xml).
    oplus_cfg: &'static PlsVibraConfig,
}

impl RichtapTranslator {
    pub fn new(oplus_cfg: &'static PlsVibraConfig) -> Self {
        hal_log!(
            "RichtapTranslator::new() - oplus config loaded: {} effects, ime overrides: {}",
            oplus_cfg.effect_count(),
            oplus_cfg.has_ime_overrides()
        );
        Self { oplus_cfg }
    }

    /// Resolve an effect id to (sysfs index, duration) using the OEM XML map
    /// first and falling back to the Android prebaked table.
    fn resolve_effect(&self, effect: i32) -> Option<(u32, u32)> {
        if let Some(e) = self.oplus_cfg.get_effect(effect) {
            hal_log!(
                "RichtapTranslator::resolve_effect() - OEM id {} -> wave {} / {}ms ({}, usage={})",
                effect,
                e.waveform_id,
                e.duration_ms,
                e.typ,
                e.usage
            );
            if e.duration_ms == 0 {
                return None;
            }
            // The waveform id is what the AWINIC driver expects in the
            // `index` node; durations above the driver's RAM table are
            // replayed as a plain timed pulse.
            return Some((e.waveform_id.max(0) as u32, e.duration_ms));
        }

        // Android prebaked effects (AIDL Effect enum values).
        let mapped = match effect {
            2 => (1, 10),  // TICK
            0 => (2, 15),  // CLICK
            21 => (4, 20), // TEXTURE_TICK
            5 => (5, 30),  // HEAVY_CLICK
            1 => (6, 60),  // DOUBLE_CLICK
            3 => (7, 35),  // THUD
            4 => (1, 15),  // POP
            _ => (0, 0),
        };

        if mapped.0 > 0 { Some(mapped) } else { None }
    }
}

impl Interface for RichtapTranslator {}

impl IRichtapVibrator for RichtapTranslator {
    fn init(&self, _callback: Option<&Strong<dyn IRichtapCallback>>) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::init()");
        Ok(())
    }

    fn setDynamicScale(
        &self,
        scale: i32,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::setDynamicScale() - scale: {}", scale);
        let amp_f32 = ((scale as f32) / 100.0).clamp(0.0, 1.0);
        sysfs::set_amplitude(amp_f32);
        Ok(())
    }

    fn setF0(&self, f0: i32, _callback: Option<&Strong<dyn IRichtapCallback>>) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::setF0() - f0: {}", f0);
        Ok(())
    }

    fn stop(&self, _callback: Option<&Strong<dyn IRichtapCallback>>) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::stop()");
        SEQUENCE.fetch_add(1, Ordering::SeqCst);
        sysfs::off();
        Ok(())
    }

    fn setAmplitude(
        &self,
        amplitude: i32,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::setAmplitude() - amplitude: {}", amplitude);
        let amp_f32 = ((amplitude as f32) / 255.0).clamp(0.0, 1.0);
        sysfs::set_amplitude(amp_f32);
        Ok(())
    }

    fn performHeParam(
        &self,
        interval: i32,
        amplitude: i32,
        freq: i32,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!(
            "IRichtapVibrator::performHeParam() - interval: {}, amplitude: {}, freq: {}",
            interval,
            amplitude,
            freq
        );
        let amp_f32 = ((amplitude as f32) / 255.0).clamp(0.0, 1.0);
        sysfs::set_amplitude(amp_f32);
        Ok(())
    }

    fn off(&self, _callback: Option<&Strong<dyn IRichtapCallback>>) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::off()");
        SEQUENCE.fetch_add(1, Ordering::SeqCst);
        sysfs::off();
        Ok(())
    }

    fn on(
        &self,
        timeout_ms: i32,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::on() - timeout_ms: {}", timeout_ms);
        SEQUENCE.fetch_add(1, Ordering::SeqCst);
        sysfs::on(timeout_ms.max(0) as u32);
        Ok(())
    }

    fn perform(
        &self,
        effect: i32,
        _strength: i8,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<i32> {
        SEQUENCE.fetch_add(1, Ordering::SeqCst);

        match self.resolve_effect(effect) {
            Some((index, duration)) => {
                hal_log!(
                    "IRichtapVibrator::perform() - effect {} -> index {} duration {}ms",
                    effect,
                    index,
                    duration
                );
                sysfs::set_index(index);
                sysfs::on(duration);
                let played = duration;
                Ok(played as i32)
            }
            None => {
                hal_log!(
                    "IRichtapVibrator::perform() - no mapping for effect {}, short fallback",
                    effect
                );
                sysfs::on(10);
                Ok(10)
            }
        }
    }

    fn performEnvelope(
        &self,
        env_info: &[i32],
        _steep_mode: bool,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        let seq = SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;

        if env_info.len() < 3 || env_info.len() % 3 != 0 {
            sysfs::on(10);
            return Ok(());
        }

        let mut events = Vec::with_capacity(env_info.len() / 3);
        for chunk in env_info.chunks_exact(3) {
            events.push((chunk[0].max(0) as u32, (chunk[1] as f32) / 255.0));
        }

        let total_time = events
            .last()
            .map(|e| e.0)
            .unwrap_or(0);

        if total_time <= 150 {
            hal_log!("IRichtapVibrator::performEnvelope() - compressing {}ms tap to 10ms click", total_time);
            sysfs::set_amplitude(0.7);
            sysfs::on(10);
            return Ok(());
        }

        hal_log!("IRichtapVibrator::performEnvelope() - playing threaded envelope: {}ms", total_time);

        thread::spawn(move || {
            let mut current_time = 0;

            for &(target_time, target_amp) in &events {
                if SEQUENCE.load(Ordering::SeqCst) != seq {
                    return;
                }

                if target_time > current_time {
                    let delay = target_time - current_time;

                    if target_amp > 0.0 {
                        crate::sysfs::set_amplitude(target_amp.clamp(0.0, 1.0));
                        crate::sysfs::on(delay);
                    } else {
                        crate::sysfs::off();
                    }

                    thread::sleep(Duration::from_millis(delay as u64));
                    current_time = target_time;
                }
            }
        });

        Ok(())
    }

    fn performRtp(
        &self,
        _pfd: &rsbinder::ParcelFileDescriptor,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::performRtp()");
        SEQUENCE.fetch_add(1, Ordering::SeqCst);
        sysfs::on(50);
        Ok(())
    }

    fn performHe(
        &self,
        looper: i32,
        interval: i32,
        amplitude: i32,
        freq: i32,
        pattern_info: &[i32],
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!(
            "IRichtapVibrator::performHe() - interval: {}, amp: {}, freq: {}, pattern_len: {}",
            interval,
            amplitude,
            freq,
            pattern_info.len()
        );

        let seq = SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1;
        let base_amplitude = if amplitude > 0 {
            ((amplitude as f32) / 255.0).clamp(0.0, 1.0)
        } else {
            1.0
        };

        if pattern_info.is_empty() || pattern_info.len() <= 16 {
            let mut duration = if interval > 0 { interval as u32 } else { 10 };

            if !pattern_info.is_empty() {
                if pattern_info[0] == 4096 && pattern_info.len() > 6 {
                    duration = pattern_info[6].max(0) as u32;
                } else if pattern_info[0] == 4097 && interval <= 0 {
                    duration = 8;
                }
            }

            hal_log!("IRichtapVibrator::performHe() - Fast path intercept, playing inline: {}ms", duration);
            crate::sysfs::set_amplitude(base_amplitude);
            crate::sysfs::on(duration);
            return Ok(());
        }

        let mut events = Vec::with_capacity(pattern_info.len() / 4);
        let mut idx = 0;
        let mut first_event = true;

        while idx < pattern_info.len() {
            if pattern_info[idx] == 4096 && idx + 6 < pattern_info.len() {
                let size = pattern_info[idx + 1].max(0) as usize;
                let mut rel_time = pattern_info[idx + 3].max(0) as u32;
                let intensity = pattern_info[idx + 4].max(0) as u32;
                let duration = pattern_info[idx + 6].max(0) as u32;
                if first_event {
                    rel_time = 0;
                    first_event = false;
                }

                events.push((rel_time, duration, intensity));
                idx += size + 2;
            } else if pattern_info[idx] == 4097 && idx + 3 < pattern_info.len() {
                let size = pattern_info[idx + 1].max(0) as usize;
                let mut rel_time = pattern_info[idx + 3].max(0) as u32;

                if first_event {
                    rel_time = 0;
                    first_event = false;
                }

                events.push((rel_time, 8, 100));
                idx += size + 2;
            } else {
                idx += 1;
            }
        }

        let looper_count = if looper > 0 { looper as usize } else { 1 };
        let pattern_duration = events
            .last()
            .map(|e| e.0 + e.1)
            .unwrap_or(0);

        thread::spawn(move || {
            for loop_idx in 0..looper_count {
                let time_offset = (loop_idx as u32) * pattern_duration;
                let mut current_time = time_offset;

                for &(rel_time, duration, intensity) in &events {
                    if SEQUENCE.load(Ordering::SeqCst) != seq {
                        return;
                    }

                    let target_time = time_offset + rel_time;
                    if target_time > current_time {
                        thread::sleep(Duration::from_millis((target_time - current_time) as u64));
                    }

                    if SEQUENCE.load(Ordering::SeqCst) != seq {
                        return;
                    }

                    let event_amp = ((intensity as f32) / 100.0) * base_amplitude;
                    crate::sysfs::set_amplitude(event_amp.clamp(0.0, 1.0));
                    crate::sysfs::on(duration);

                    current_time = target_time + duration;
                    thread::sleep(Duration::from_millis(duration as u64));
                }
            }
        });

        Ok(())
    }
}
