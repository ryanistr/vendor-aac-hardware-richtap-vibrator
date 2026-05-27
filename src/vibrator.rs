use rsbinder::{ Interface, status::Result as BinderResult, status::Status, SIBinder };
use crate::android::hardware::vibrator::{
    Effect::Effect,
    EffectStrength::EffectStrength,
    IVibrator::IVibrator,
    CompositePrimitive::CompositePrimitive,
    CompositeEffect::CompositeEffect,
    Braking::Braking,
    PrimitivePwle::PrimitivePwle,
};
use crate::sysfs;
use crate::hal_log;

pub struct VibratorService {
    support_gain: bool,
}

impl VibratorService {
    pub fn new() -> Self {
        let support_gain = sysfs::has_node("gain");
        hal_log!("VibratorService::new() - support_gain: {}", support_gain);
        Self { support_gain }
    }
}

impl Interface for VibratorService {}

impl IVibrator for VibratorService {
    fn getCapabilities(&self) -> BinderResult<i32> {
        let mut caps = 3;
        if self.support_gain {
            caps |= 4;
        }
        hal_log!("IVibrator::getCapabilities() - returning {}", caps);
        Ok(caps)
    }

    fn off(&self) -> BinderResult<()> {
        hal_log!("IVibrator::off()");
        sysfs::off();
        Ok(())
    }

    fn on(&self, timeout_ms: i32, _callback: &SIBinder) -> BinderResult<()> {
        hal_log!("IVibrator::on() - timeout_ms: {}", timeout_ms);
        sysfs::on(timeout_ms as u32);
        Ok(())
    }

    fn perform(
        &self,
        effect: Effect,
        _strength: EffectStrength,
        _callback: &SIBinder
    ) -> BinderResult<i32> {
        let (index, duration) = match effect {
            Effect::TICK => (1, 10),
            Effect::CLICK => (2, 15),
            Effect::TEXTURE_TICK => (4, 20),
            Effect::HEAVY_CLICK => (5, 30),
            Effect::DOUBLE_CLICK => (6, 60),
            Effect::THUD => (7, 35),
            Effect::POP => (1, 15),
            _ => (0, 0),
        };

        if index > 0 {
            hal_log!(
                "IVibrator::perform() - routing effect {:?} to hardware index {}",
                effect,
                index
            );
            sysfs::set_index(index);
            sysfs::on(duration);
            Ok(duration as i32)
        } else {
            hal_log!("IVibrator::perform() - unsupported effect {:?}", effect);
            Err(Status::new_service_specific_error(-1, None))
        }
    }

    fn getSupportedEffects(&self) -> BinderResult<Vec<Effect>> {
        hal_log!("IVibrator::getSupportedEffects()");
        Ok(
            vec![
                Effect::CLICK,
                Effect::DOUBLE_CLICK,
                Effect::TICK,
                Effect::THUD,
                Effect::POP,
                Effect::HEAVY_CLICK
            ]
        )
    }

    fn setAmplitude(&self, amplitude: f32) -> BinderResult<()> {
        hal_log!("IVibrator::setAmplitude() - amplitude: {}", amplitude);
        if self.support_gain {
            sysfs::set_amplitude(amplitude);
        }
        Ok(())
    }

    fn setExternalControl(&self, _enabled: bool) -> BinderResult<()> {
        hal_log!("IVibrator::setExternalControl() - Unsupported");
        Err(Status::new_service_specific_error(-1, None))
    }

    fn getCompositionDelayMax(&self) -> BinderResult<i32> {
        Ok(0)
    }
    fn getCompositionSizeMax(&self) -> BinderResult<i32> {
        Ok(0)
    }
    fn getSupportedPrimitives(&self) -> BinderResult<Vec<CompositePrimitive>> {
        Ok(vec![])
    }
    fn getPrimitiveDuration(&self, _primitive: CompositePrimitive) -> BinderResult<i32> {
        Ok(0)
    }

    fn compose(&self, _composite: &[CompositeEffect], _callback: &SIBinder) -> BinderResult<()> {
        Err(Status::new_service_specific_error(-1, None))
    }

    fn getSupportedAlwaysOnEffects(&self) -> BinderResult<Vec<Effect>> {
        Ok(vec![])
    }

    fn alwaysOnEnable(
        &self,
        _id: i32,
        _effect: Effect,
        _strength: EffectStrength
    ) -> BinderResult<()> {
        Err(Status::new_service_specific_error(-1, None))
    }

    fn alwaysOnDisable(&self, _id: i32) -> BinderResult<()> {
        Err(Status::new_service_specific_error(-1, None))
    }

    fn getResonantFrequency(&self) -> BinderResult<f32> {
        Ok(150.0)
    }
    fn getQFactor(&self) -> BinderResult<f32> {
        Ok(10.0)
    }
    fn getFrequencyResolution(&self) -> BinderResult<f32> {
        Ok(0.0)
    }
    fn getFrequencyMinimum(&self) -> BinderResult<f32> {
        Ok(0.0)
    }
    fn getBandwidthAmplitudeMap(&self) -> BinderResult<Vec<f32>> {
        Ok(vec![])
    }
    fn getPwlePrimitiveDurationMax(&self) -> BinderResult<i32> {
        Ok(0)
    }
    fn getPwleCompositionSizeMax(&self) -> BinderResult<i32> {
        Ok(0)
    }
    fn getSupportedBraking(&self) -> BinderResult<Vec<Braking>> {
        Ok(vec![])
    }

    fn composePwle(&self, _composite: &[PrimitivePwle], _callback: &SIBinder) -> BinderResult<()> {
        Err(Status::new_service_specific_error(-1, None))
    }
}
