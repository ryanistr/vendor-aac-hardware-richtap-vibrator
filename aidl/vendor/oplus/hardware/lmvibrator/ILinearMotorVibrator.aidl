// ILinearMotorVibrator.aidl
//
// AIDL port of the OPLUS HIDL interface
//   vendor.oplus.hardware.lmvibrator@1.0::ILinearMotorVibrator
//
// The original is HIDL. Reimplementing it in AIDL + Rust lets this project
// build it with the NDK/cargo toolchain instead of requiring hidl-gen and an
// AOSP/Soong tree. Method order mirrors the recovered HIDL transaction order
// (see ReconstructedHALSource/Oplus/OplusLmVibrator/NOTES.md).
package vendor.oplus.hardware.lmvibrator;

@VintfStability
interface ILinearMotorVibrator {
    // tx 1 — timeoutMs, amplitude (0..32767; the HIDL original uses `short`,
    // widened to `int` here because rsbinder-aidl 0.6 mishandles the AIDL
    // `short` primitive), gun-mode flag
    int linearmotorVibratorOn(int timeoutMs, int amplitude, boolean isGunMode);

    // tx 2
    int setVmax(int vmax);

    // tx 3
    int getVibratorTouchStyle();

    // tx 4
    int write_gun_type(int type);

    // tx 5
    int linearmotorVibratorOff();

    // tx 6
    int getStatus();

    // tx 7
    int write_haptic_audio(String data);

    // tx 8
    int write_bullet_num(int num);

    // tx 9
    int write_gun_mode(int mode);

    // tx 10
    int setVibratorTouchStyle(int style);
}
