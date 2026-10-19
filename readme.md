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
- **Optional ERM pulse extender** — off by default, extends sub-threshold pulses for ordinary ERM motors on MediaTek ROM ports (see [ERM motors](#erm-motors-mediatek-rom-ports))
- **Logging** — conditional Android logcat output via `persist.sys.richtap.debug` system property

## Building

This project uses [rsbinder](https://crates.io/crates/rsbinder) for Rust Binder IPC and requires the Android NDK for cross-compilation.

The built binary is not tracked in this repository — build it before installing. See
[BUILD.md](BUILD.md) for the Termux path and `scripts/build-termux.sh`.

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

## ERM motors (MediaTek ROM ports)

The HAL writes the short pulses an LRA wants: about 8 to 10 ms for a system effect. An ERM
motor needs tens of milliseconds to spin up, so on an ERM device those pulses barely move
it. For that case the HAL carries a pulse extender, merged from the standalone `erm-haptic`
daemon. It is off unless it is switched on in the rc, because on an LRA it is the wrong thing
to do — an LRA uses `gain` for amplitude and wants the pulse to stay short.

Switch it on in the service block of `vendor/etc/init/richtap-vibrator.rc`:

```
setenv ERM_HAPTIC_ENABLE 1
setenv ERM_HAPTIC_THRESHOLD_MS 50
setenv ERM_HAPTIC_TARGET_MS 80
```

Durations below the threshold are written as the target instead. `0` is never changed. The
values are read once when the HAL starts, so reboot after a change. The threshold must be at
least 1, the target must be above the threshold and at most 1000; anything else disables the
boost and logs why. With `ERM_HAPTIC_ENABLE` at 0, or unset, `on()` writes the duration it was
given and nothing else changes.

| Log line | Meaning |
|---|---|
| `erm boost config - below 50 ms becomes 80 ms` | The settings in use. |
| `erm boost config invalid (...), boost disabled` | The values were rejected. |
| `erm boost disabled - set ERM_HAPTIC_ENABLE=1 ...` | The extender is off. |
| `sysfs::on() - boost 10 ms to 80 ms` | A short pulse was extended. |

The rc also carries an `on property:sys.boot_completed=1` block for the ERM ports that expose
`/sys/class/leds/vibrator` through the `regulator_vibrator` driver with the `transient`
trigger. It attaches the trigger, sets `state` to 1 and fixes node ownership and mode, in that
order, because attaching the trigger recreates the nodes. On a device with only
`/sys/class/leds/vibrator_single` those paths do not exist: every line then logs one
`Unable to open ... for write` and nothing else happens. Do not repoint them at
`vibrator_single`.

Typing haptics too heavy? Lower both values, for example 30 and 40. Too weak? Raise the target.
On an ERM port you can also run `scripts/quick-test-hal.sh <binary> erm` to try it without
reflashing.

## Install in the ROM

The built binary is not tracked in this repository — build it first, see [BUILD.md](BUILD.md).

1. Copy the binary to `/vendor/bin/hw/vendor-aac-hardware-richtap-vibrator`. Keep the owner,
   mode and label of the old file. The usual label is `u:object_r:hal_vibrator_default_exec:s0`.
2. Copy `vendor/etc/init/richtap-vibrator.rc` to `/vendor/etc/init/richtap-vibrator.rc`.
3. The other files under `vendor/` (the library in `lib64`, `etc/richtapresources`,
   `etc/vintf/manifest`) go to the same paths in `/vendor`. Skip what the ROM already has.
4. If the standalone `erm-haptic` daemon was installed before, remove `/vendor/bin/hw/erm-haptic`
   and `/vendor/etc/init/erm-haptic.rc`. Do not run both.
5. No new SELinux rules are needed for the ERM additions. Old `erm_haptic` rules can stay, they
   are unused.

Keep a copy of the old binary and rc, so you can go back.

### Check after boot

| Command | Expected result |
|---|---|
| `pgrep -f richtap` | Prints a process id |
| `cat /sys/class/leds/vibrator/trigger` | Contains `[transient]` — ERM ports only |
| `cat /sys/class/leds/vibrator/state` | `1` — ERM ports only |
| `ls -l /sys/class/leds/vibrator/activate` | Owner `system system`, mode `-rw-rw----` — ERM ports only |
| `pgrep -x erm-haptic` | Prints nothing |
| `cmd vibrator_manager synced oneshot 300 255` | A short vibration that stops by itself |

On an ERM port the node ownership can go back to root (`state`, `activate` and `duration` are
recreated when the trigger is attached) — that is what the boot block fixes. A write of 0 to
`/sys/class/leds/vibrator/brightness` detaches the trigger; never do that.

## Credits

Based on Nothing OS 4.1 Pacman's RichTap NDK implementation. 
Uses [rsbinder](https://crates.io/crates/rsbinder) for Rust Binder IPC.
The ERM pulse extender, the ERM boot block and the scripts come from Dadobye's
`richtap-vibrator-erm` fork and its `erm-haptic` daemon, MIT licensed, see [LICENSE](LICENSE).
