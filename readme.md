# AAC RichTap Vibrator HAL

A Rust implementation of the AAC RichTap vibrator AIDL HAL (Hardware Abstraction Layer) service for Android devices. This is a translator layer based on Nothing OS 4.1 Pacman's RichTap NDK, enabling RichTap haptic effects on devices that do not natively support the RichTap protocol.

## Overview

The service implements both the standard Android `android.hardware.vibrator.IVibrator` interface and the vendor-specific `vendor.aac.hardware.richtap.vibrator.IRichtapVibrator` interface. It translates RichTap haptic commands into generic sysfs operations, making AAC RichTap linear actuators work on any device with a standard vibrator sysfs interface.

### Key Features

- **Standard IVibrator HAL** — implements `on`/`off`, `perform`, `setAmplitude`, `getCapabilities`, and `getSupportedEffects`
- **RichTap IRichtapVibrator** — supports `performEnvelope`, `performHe` (haptic effect), `performRtp` (real-time playback), `setDynamicScale`, `setAmplitude`, `performHeParam`, and `perform`
- **Envelope-based haptics** — threaded playback of amplitude-over-time envelopes for rich, expressive haptic patterns
- **HE (Haptic Effect) pattern support** — parses complex ringtone/media haptic patterns with looping, delay compensation, and fast-path shortcuts for short taps
- **Sysfs abstraction** — automatically detects the correct sysfs vibrator node (`/sys/class/leds/vibrator_single/` or `/sys/class/leds/vibrator/`)
- **Logging** — conditional Android logcat output via `persist.sys.richtap.debug` system property

## Building

This project uses [rsbinder](https://crates.io/crates/rsbinder) for Rust Binder IPC and requires the Android NDK for cross-compilation.

### Build

```bash
# For Android (aarch64)
cargo ndk -t arm64-v8a -o <output_dir> build --release

# Or with standard cross-compilation
cargo build --release --target aarch64-linux-android
```

### AIDL Bindings

The `build.rs` script automatically generates Rust bindings from the AIDL files during compilation using `rsbinder-aidl`. It also applies a workaround for a `rsbinder 0.6.0` async trait compatibility issue on modern Rust versions.

### Build Profile

The release profile is optimized for size and performance:
- Optimization level: `-s` (size-optimized)
- LTO: `fat` (full link-time optimization)
- Codegen units: `1`
- Panic: `abort`
- Strip symbols

## Debugging

Enable verbose logging via system property:

```bash
adb shell setprop persist.sys.richtap.debug 1
adb logcat -s RichtapHAL
```

Disable when done:

```bash
adb shell setprop persist.sys.richtap.debug 0
```

### Sysfs Nodes

The driver interacts with the following sysfs nodes under `vibrator_single` or `vibrator`:

| Node | Purpose |
|------|---------|
| `activate` | Start (write `1`) / stop (write `0`) vibration |
| `duration` | Vibration duration in milliseconds |
| `gain` | Amplitude control (0–255), if supported |
| `index` | Predefined haptic effect index |

## RichTap Features

### Amplitude Translation

The RichTap interface uses integer amplitude values (0–255 or 0–100%) which are translated to the 0.0–1.0 float range expected by sysfs:

```rust
// setAmplitude: 0-255 → 0.0-1.0
let amp_f32 = ((amplitude as f32) / 255.0).clamp(0.0, 1.0);

// setDynamicScale: 0-100 → 0.0-1.0
let amp_f32 = ((scale as f32) / 100.0).clamp(0.0, 1.0);
```

### performHe (Haptic Effect)

The `performHe` method supports two paths:

1. **Fast Path** — For short, immediate patterns (e.g., keyboard taps) with ≤16 entries. Plays inline without thread spawning.
2. **Complex Path** — For long haptic patterns (ringtones, media haptics). Parses `4096` (waveform segment) and `4097` (prebaked effect) markers from `pattern_info`, then plays them on a background thread with amplitude scaling and optional looping.

### performEnvelope

Plays amplitude-over-time envelopes on a background thread. Short envelopes (≤150ms) are compressed to a 10ms click for immediacy.

### Effect Mapping

Standard Android effects are mapped to hardware effect indices:

| Android Effect | HW Index | Duration (ms) |
|---------------|----------|---------------|
| `TICK` | 1 | 10 |
| `CLICK` | 2 | 15 |
| `TEXTURE_TICK` | 4 | 20 |
| `HEAVY_CLICK` | 5 | 30 |
| `DOUBLE_CLICK` | 6 | 60 |
| `THUD` | 7 | 35 |
| `POP` | 1 | 15 |

### Capabilities

The service reports the following IVibrator capabilities:
- `CAP_ON_CALLBACK` (bit 0)
- `CAP_PERFORM_CALLBACK` (bit 1)
- `CAP_AMPLITUDE_CONTROL` (bit 2) — only if the `gain` sysfs node exists

## Credits

Based on Nothing OS 4.1 Pacman's RichTap NDK implementation. 
Uses [rsbinder](https://crates.io/crates/rsbinder) for Rust Binder IPC.
