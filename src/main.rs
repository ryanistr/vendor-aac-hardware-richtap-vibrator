use rsbinder::{ProcessState, hub};

mod logger;
mod vibrator;
mod manager;
mod richtap;
mod lmvibrator;
mod sysfs;
mod pls_vibra_config;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use crate::vendor::aac::hardware::richtap::vibrator::IRichtapVibrator::BnRichtapVibrator;
use crate::android::hardware::vibrator::IVibrator::BnVibrator;
use crate::android::hardware::vibrator::IVibratorManager::BnVibratorManager;
use crate::vendor::oplus::hardware::lmvibrator::ILinearMotorVibrator::BnLinearMotorVibrator;

const VIBRATOR_SERVICE: &str = "android.hardware.vibrator.IVibrator/default";
const VIBRATOR_MANAGER_SERVICE: &str = "android.hardware.vibrator.IVibratorManager/default";
const LM_VIBRATOR_SERVICE: &str = "vendor.oplus.hardware.lmvibrator.ILinearMotorVibrator/default";

fn main() {
    hal_log!("Initializing OPlus-adapted RichTap Vibrator HAL...");
    ProcessState::init_default();
    ProcessState::start_thread_pool();

    // Load OEM effect definitions from the OPlus VibratorService XML configs
    // so perform() can resolve OEM effect ids instead of using raw defaults.
    let oplus_cfg = pls_vibra_config::PlsVibraConfig::new();

    let richtap_service = richtap::RichtapTranslator::new(oplus_cfg);
    let richtap_binder = BnRichtapVibrator::new_binder(richtap_service);

    let vibrator_service = vibrator::VibratorService::new();
    let vibrator_binder = BnVibrator::new_binder(vibrator_service);

    // IVibratorManager/default is what VibratorManagerService enumerates.
    // The stock mediatek service provided it too; without it the framework
    // has no vibrator at all.
    let manager_binder = BnVibratorManager::new_binder(manager::VibratorManagerService::new());

    // OPLUS linear-motor vibrator, reimplemented in AIDL + Rust from the
    // reverse-engineered HIDL interface (vendor.oplus.hardware.lmvibrator@1.0).
    let lm_binder = BnLinearMotorVibrator::new_binder(lmvibrator::LmVibratorService::new());

    // The Richtap extension is attached to the IVibrator binder so a single
    // process serves both the standard and the vendor haptic interfaces.
    vibrator_binder.as_binder().set_extension(&mut richtap_binder.as_binder())
        .expect("Failed to attach Richtap extension");

    // All binders are constructed first so every implementation's start-up
    // logging happens before the first registration can fail.
    hub::add_service(VIBRATOR_SERVICE, vibrator_binder.as_binder())
        .expect("Failed to register Vibrator service");
    hub::add_service(VIBRATOR_MANAGER_SERVICE, manager_binder.as_binder())
        .expect("Failed to register VibratorManager service");
    hub::add_service(LM_VIBRATOR_SERVICE, lm_binder.as_binder())
        .expect("Failed to register lmvibrator service");

    hal_log!("Services registered successfully. Joining thread pool...");
    ProcessState::join_thread_pool().unwrap();
}
