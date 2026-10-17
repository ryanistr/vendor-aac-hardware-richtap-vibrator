use rsbinder::{ Interface, Strong, status::Result as BinderResult };
use crate::vendor::aac::hardware::richtap::vibrator::IRichtapVibrator::IRichtapVibrator;
use crate::vendor::aac::hardware::richtap::vibrator::IRichtapCallback::IRichtapCallback;
use crate::sysfs;
use crate::hal_log;
use std::sync::atomic::{ AtomicUsize, Ordering };
use std::thread;
use std::time::Duration;

/// Generation counter. Every new command bumps it; background threads compare
/// against the value they started with and bail out when it changed.
/// Shared with vibrator.rs so IVibrator::on/off/perform also cancel threads.
pub static SEQUENCE: AtomicUsize = AtomicUsize::new(0);

// RichTap HE (Java flat array) layout: 17 ints per event
// [0]=first-flag [1]=type [2]=rel_time [3]=intensity [4]=freq [5..16]=extra
const HE_STRIDE: usize = 17;
const HE_CONTINUOUS: i32 = 4096;
const HE_TRANSIENT: i32 = 4097;

const TRANSIENT_MS: u32 = 8; // default length of a transient event
const DEFAULT_TICK_MS: u32 = 10;
const MAX_FAST_MS: u32 = 100; // upper bound for single-event fast path
const MAX_EVENT_MS: u32 = 1000; // upper bound for one event inside a pattern
const MAX_ON_MS: u32 = 60_000; // upper bound for on(timeout)
const MAX_ENV_MS: u32 = 30_000; // upper bound for envelope timestamps
const MAX_LOOPS: usize = 1000;

/// Cancel every running background pattern/envelope and return the new generation.
pub fn cancel_all() -> usize {
    SEQUENCE.fetch_add(1, Ordering::SeqCst) + 1
}

fn is_current(seq: usize) -> bool {
    SEQUENCE.load(Ordering::SeqCst) == seq
}

/// Negative -> 0, huge -> `max`. Prevents `-1 as u32 == 4294967295`.
pub fn clamp_ms(v: i32, max: u32) -> u32 {
    if v <= 0 { 0 } else { (v as u32).min(max) }
}

pub fn max_on_ms() -> u32 {
    MAX_ON_MS
}

/// (rel_time_ms, duration_ms, intensity_0_100)
type HeEvent = (u32, u32, u32);

fn at(p: &[i32], i: usize) -> i32 {
    p.get(i).copied().unwrap_or(0)
}

fn event_duration(kind: i32, raw: i32) -> u32 {
    if kind == HE_CONTINUOUS {
        let d = clamp_ms(raw, MAX_EVENT_MS);
        if d == 0 { DEFAULT_TICK_MS } else { d }
    } else {
        TRANSIENT_MS
    }
}

/// Format A: flat array, fixed stride of 17 ints per event (matches the
/// patternHeInfo[event][field] dump seen in logcat).
fn parse_he_stride(p: &[i32]) -> Option<Vec<HeEvent>> {
    if p.len() < HE_STRIDE || p.len() % HE_STRIDE != 0 {
        return None;
    }
    let mut out = Vec::with_capacity(p.len() / HE_STRIDE);
    for chunk in p.chunks_exact(HE_STRIDE) {
        let kind = chunk[1];
        if kind != HE_CONTINUOUS && kind != HE_TRANSIENT {
            return None;
        }
        let rel = clamp_ms(chunk[2], MAX_ENV_MS);
        let intensity = clamp_ms(chunk[3], 100);
        out.push((rel, event_duration(kind, chunk[6]), intensity));
    }
    Some(out)
}

/// Format B: native header layout [type, size, _, rel_time, intensity, freq, duration, ...]
/// with variable event size (`idx += size + 2`). Kept for frameworks that use it.
fn parse_he_native(p: &[i32]) -> Vec<HeEvent> {
    let mut out = Vec::new();
    let mut idx = 0;
    while idx < p.len() {
        let kind = p[idx];
        if kind == HE_CONTINUOUS && idx + 6 < p.len() {
            let size = clamp_ms(p[idx + 1], 64) as usize;
            out.push((
                clamp_ms(p[idx + 3], MAX_ENV_MS),
                event_duration(kind, p[idx + 6]),
                clamp_ms(p[idx + 4], 100),
            ));
            idx += size + 2;
        } else if kind == HE_TRANSIENT && idx + 4 < p.len() {
            let size = clamp_ms(p[idx + 1], 64) as usize;
            out.push((
                clamp_ms(p[idx + 3], MAX_ENV_MS),
                TRANSIENT_MS,
                clamp_ms(p[idx + 4], 100).max(1),
            ));
            idx += size + 2;
        } else {
            idx += 1;
        }
    }
    out
}

fn parse_he(p: &[i32]) -> Vec<HeEvent> {
    match parse_he_stride(p) {
        Some(v) => v,
        None => parse_he_native(p),
    }
}

pub struct RichtapTranslator;

impl RichtapTranslator {
    pub fn new() -> Self {
        hal_log!("RichtapTranslator::new()");
        Self {}
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
        cancel_all();
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
        cancel_all();
        let amp_f32 = ((amplitude as f32) / 255.0).clamp(0.0, 1.0);
        let duration = match clamp_ms(interval, MAX_FAST_MS) {
            0 => DEFAULT_TICK_MS,
            d => d,
        };
        sysfs::set_amplitude(amp_f32);
        sysfs::on(duration);
        Ok(())
    }

    fn off(&self, _callback: Option<&Strong<dyn IRichtapCallback>>) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::off()");
        cancel_all();
        sysfs::off();
        Ok(())
    }

    fn on(
        &self,
        timeout_ms: i32,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        hal_log!("IRichtapVibrator::on() - timeout_ms: {}", timeout_ms);
        cancel_all();
        match clamp_ms(timeout_ms, MAX_ON_MS) {
            0 => sysfs::off(),
            t => sysfs::on(t),
        }
        Ok(())
    }

    fn perform(
        &self,
        effect: i32,
        _strength: i8,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<i32> {
        cancel_all();
        let (index, duration) = match effect {
            2 => (1, 10), // Effect::TICK
            0 => (2, 15), // Effect::CLICK
            21 => (4, 20), // Effect::TEXTURE_TICK
            5 => (5, 30), // Effect::HEAVY_CLICK
            1 => (6, 60), // Effect::DOUBLE_CLICK
            3 => (7, 35), // Effect::THUD
            4 => (1, 15), // Effect::POP
            _ => (0, 0),
        };

        if index > 0 {
            hal_log!(
                "IRichtapVibrator::perform() - routing effect {} to hardware index {}",
                effect,
                index
            );
            sysfs::set_index(index);
            sysfs::on(duration);
            Ok(duration as i32)
        } else {
            hal_log!("IRichtapVibrator::perform() - raw fallback for effect {}", effect);
            sysfs::on(DEFAULT_TICK_MS);
            Ok(DEFAULT_TICK_MS as i32)
        }
    }

    fn performEnvelope(
        &self,
        env_info: &[i32],
        _steep_mode: bool,
        _callback: Option<&Strong<dyn IRichtapCallback>>
    ) -> BinderResult<()> {
        let seq = cancel_all();

        if env_info.len() < 3 || env_info.len() % 3 != 0 {
            sysfs::on(DEFAULT_TICK_MS);
            return Ok(());
        }

        // chunk = (time_ms, amplitude_0_255, <unused third field>)
        let mut events: Vec<(u32, f32)> = Vec::with_capacity(env_info.len() / 3);
        for chunk in env_info.chunks_exact(3) {
            events.push((
                clamp_ms(chunk[0], MAX_ENV_MS),
                ((chunk[1] as f32) / 255.0).clamp(0.0, 1.0),
            ));
        }

        let total_time = events.last().map(|e| e.0).unwrap_or(0);

        if total_time <= 150 {
            hal_log!("IRichtapVibrator::performEnvelope() - compressing {}ms tap to 10ms click", total_time);
            sysfs::set_amplitude(0.7);
            sysfs::on(DEFAULT_TICK_MS);
            return Ok(());
        }

        hal_log!("IRichtapVibrator::performEnvelope() - playing threaded envelope: {}ms", total_time);

        thread::spawn(move || {
            let mut current_time = 0;

            for &(target_time, target_amp) in &events {
                if !is_current(seq) {
                    return;
                }

                if target_time > current_time {
                    let delay = target_time - current_time;

                    if target_amp > 0.0 {
                        sysfs::set_amplitude(target_amp);
                        sysfs::on(delay);
                    } else {
                        sysfs::off();
                    }

                    thread::sleep(Duration::from_millis(delay as u64));
                    current_time = target_time;
                }
            }

            // Only stop the motor if nobody started something newer meanwhile.
            if is_current(seq) {
                sysfs::off();
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
        cancel_all();
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
            "IRichtapVibrator::performHe() - looper: {}, interval: {}, amp: {}, freq: {}, pattern_len: {}, head: {:?}",
            looper,
            interval,
            amplitude,
            freq,
            pattern_info.len(),
            &pattern_info[..pattern_info.len().min(8)]
        );

        let seq = cancel_all();
        let base_amplitude = if amplitude > 0 {
            ((amplitude as f32) / 255.0).clamp(0.0, 1.0)
        } else {
            1.0
        };

        let events = parse_he(pattern_info);

        // 1. FAST PATH: empty pattern or a single event (keyboard taps, clicks).
        //    Plays inline, no thread, always bounded.
        if events.len() <= 1 {
            let mut duration = match clamp_ms(interval, MAX_FAST_MS) {
                0 => DEFAULT_TICK_MS,
                d => d,
            };
            let mut amp = base_amplitude;

            if let Some(&(_, ev_dur, ev_int)) = events.first() {
                // Single continuous event with explicit duration wins over `interval`.
                if ev_dur > TRANSIENT_MS {
                    duration = ev_dur;
                }
                if ev_int > 0 {
                    amp = (((ev_int as f32) / 100.0) * base_amplitude).clamp(0.0, 1.0);
                }
            }

            hal_log!("IRichtapVibrator::performHe() - fast path, playing inline: {}ms", duration);
            sysfs::set_amplitude(amp);
            sysfs::on(duration);
            return Ok(());
        }

        // 2. COMPLEX PATH: multi-event patterns (double click, ringtones, media).
        let mut events = events;
        // Kill initial framework latency on the very first event.
        events[0].0 = 0;

        let looper_count = if looper > 0 { (looper as usize).min(MAX_LOOPS) } else { 1 };
        let pattern_duration = events
            .last()
            .map(|e| e.0 + e.1)
            .unwrap_or(0);

        thread::spawn(move || {
            for loop_idx in 0..looper_count {
                let time_offset = (loop_idx as u32) * pattern_duration;
                let mut current_time = time_offset;

                for &(rel_time, duration, intensity) in &events {
                    if !is_current(seq) {
                        return;
                    }

                    let target_time = time_offset + rel_time;
                    if target_time > current_time {
                        thread::sleep(Duration::from_millis((target_time - current_time) as u64));
                    }

                    if !is_current(seq) {
                        return;
                    }

                    let event_amp = ((intensity as f32) / 100.0) * base_amplitude;
                    sysfs::set_amplitude(event_amp.clamp(0.0, 1.0));
                    sysfs::on(duration);

                    current_time = target_time + duration;
                    thread::sleep(Duration::from_millis(duration as u64));
                }
            }

            // Guarantee the motor is stopped, but never kill a newer vibration.
            if is_current(seq) {
                sysfs::off();
            }
        });

        Ok(())
    }
}
