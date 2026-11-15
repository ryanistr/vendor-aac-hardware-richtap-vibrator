use rsbinder::{ Interface, Strong, status::Result as BinderResult, status::Status };

use crate::android::hardware::vibrator::IVibratorManager::IVibratorManager;
use crate::android::hardware::vibrator::IVibratorCallback::IVibratorCallback;
use crate::android::hardware::vibrator::IVibrator::{BnVibrator, IVibrator};
use crate::vibrator::VibratorService;
use crate::hal_log;

/// The single physical vibrator exposed by this HAL (sysfs `vibrator_single`).
const VIBRATOR_ID: i32 = 1;

/// `IVibratorManager` is mandatory alongside `IVibrator`: the framework's
/// VibratorManagerService talks to the manager to enumerate vibrators and to
/// obtain the `IVibrator` instance it then drives. The stock
/// `android.hardware.vibrator-service.mediatek` served both interfaces, so the
/// replacement must as well — otherwise replacing the service leaves the
/// framework without a vibrator manager and haptics stop entirely.
pub struct VibratorManagerService;

impl VibratorManagerService {
    pub fn new() -> Self {
        hal_log!("VibratorManagerService::new()");
        Self
    }
}

impl Interface for VibratorManagerService {}

impl IVibratorManager for VibratorManagerService {
    fn getCapabilities(&self) -> BinderResult<i32> {
        // No synced-vibration support: this is a single-actuator device.
        hal_log!("IVibratorManager::getCapabilities() - 0 (no sync support)");
        Ok(0)
    }

    fn getVibratorIds(&self) -> BinderResult<Vec<i32>> {
        hal_log!("IVibratorManager::getVibratorIds() - [{}]", VIBRATOR_ID);
        Ok(vec![VIBRATOR_ID])
    }

    fn getVibrator(&self, vibrator_id: i32) -> BinderResult<Strong<dyn IVibrator>> {
        hal_log!("IVibratorManager::getVibrator() - id {}", vibrator_id);
        if vibrator_id != VIBRATOR_ID {
            return Err(Status::new_service_specific_error(
                -1,
                Some(format!("unknown vibrator id {}", vibrator_id)),
            ));
        }
        Ok(BnVibrator::new_binder(VibratorService::new()))
    }

    fn prepareSynced(&self, _vibrator_ids: &[i32]) -> BinderResult<()> {
        hal_log!("IVibratorManager::prepareSynced() - unsupported");
        Err(Status::new_service_specific_error(-1, Some("synced vibration unsupported".to_string())))
    }

    fn triggerSynced(&self, _callback: &Strong<dyn IVibratorCallback>) -> BinderResult<()> {
        hal_log!("IVibratorManager::triggerSynced() - unsupported");
        Err(Status::new_service_specific_error(-1, Some("synced vibration unsupported".to_string())))
    }

    fn cancelSynced(&self) -> BinderResult<()> {
        hal_log!("IVibratorManager::cancelSynced() - nothing prepared");
        Ok(())
    }
}
