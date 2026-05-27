use rsbinder::{ProcessState, hub};

mod logger;
mod vibrator;
mod richtap;
mod sysfs;

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

use crate::vendor::aac::hardware::richtap::vibrator::IRichtapVibrator::BnRichtapVibrator;
use crate::android::hardware::vibrator::IVibrator::BnVibrator;

const VIBRATOR_SERVICE: &str = "android.hardware.vibrator.IVibrator/default";

fn main() {
    hal_log!("Initializing Richtap Vibrator HAL...");
    ProcessState::init_default();
    ProcessState::start_thread_pool();

    let richtap_service = richtap::RichtapTranslator::new();
    let richtap_binder = BnRichtapVibrator::new_binder(richtap_service);

    let vibrator_service = vibrator::VibratorService::new();
    let vibrator_binder = BnVibrator::new_binder(vibrator_service);

    vibrator_binder.as_binder().set_extension(&mut richtap_binder.as_binder())
        .expect("Failed to attach Richtap extension");

    hub::add_service(VIBRATOR_SERVICE, vibrator_binder.as_binder())
        .expect("Failed to register unified Vibrator service");

    hal_log!("Service registered successfully. Joining thread pool...");
    ProcessState::join_thread_pool().unwrap();
}