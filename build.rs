use std::path::PathBuf;
use std::env;

fn main() {
    let aidl_dir = PathBuf::from("aidl");
    println!("cargo:rerun-if-changed=aidl/");
    
    let out_dir = env::var("OUT_DIR").unwrap();
    let out_file = PathBuf::from(&out_dir).join("bindings.rs");
    
    rsbinder_aidl::Builder::new()
        .source(aidl_dir.join("vendor/aac/hardware/richtap/vibrator/IRichtapVibrator.aidl"))
        .source(aidl_dir.join("vendor/aac/hardware/richtap/vibrator/IRichtapCallback.aidl"))
        .source(aidl_dir.join("android/hardware/vibrator/IVibrator.aidl"))
        .include_dir(&aidl_dir)
        .output(&out_file)
        .generate()
        .expect("Failed to generate AIDL bindings");

    // Post-process to fix rsbinder 0.6.0 dyn compatibility bug on modern Rust
    let mut content = std::fs::read_to_string(&out_file).unwrap();
    let async_traits = [
        "IVibratorAsyncService",
        "IVibratorCallbackAsyncService",
        "IRichtapVibratorAsyncService",
        "IRichtapCallbackAsyncService",
    ];
    
    for t in async_traits {
        let target = format!("pub trait {}: rsbinder::Interface", t);
        let replacement = format!("#[async_trait::async_trait]\npub trait {}: rsbinder::Interface", t);
        content = content.replace(&target, &replacement);
    }
    
    std::fs::write(&out_file, content).unwrap();
}