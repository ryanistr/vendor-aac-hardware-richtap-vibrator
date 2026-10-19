# Build guide

Two ways: on the workstation with the Android NDK, or on the phone in Termux.

## On the workstation

Needs `cargo`, the `aarch64-linux-android` target and an NDK.

```sh
rustup target add aarch64-linux-android
export ANDROID_NDK_HOME=/path/to/android-ndk
cargo ndk -t arm64-v8a build --release
```

The binary is `target/aarch64-linux-android/release/vendor-aac-hardware-richtap-vibrator`.
Without `cargo-ndk`, set `CC_aarch64-linux-android` and `CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER`
to the NDK's clang and build with `--target aarch64-linux-android`.

## On the phone, in Termux

The first build downloads crates, so the phone needs internet.

### Before you start

- Use Termux from F-Droid or the Termux GitHub releases. The Play Store version is outdated.
- The phone must be arm64. Run `uname -m`. The answer must be `aarch64`.
- Keep at least 3 GB of free storage.

### Quick way

```sh
pkg install -y rust clang unzip
sh scripts/build-termux.sh           # normal
sh scripts/build-termux.sh lowmem    # phones that run out of memory
```

The binary is copied to `/sdcard/Download/vendor-aac-hardware-richtap-vibrator`.
Set `OUT=/other/path` to change that.

### By hand

```sh
termux-setup-storage
pkg update -y
pkg install -y rust clang
cargo build --release --locked
ls -l target/release/vendor-aac-hardware-richtap-vibrator
cp target/release/vendor-aac-hardware-richtap-vibrator /sdcard/Download/
sha256sum target/release/vendor-aac-hardware-richtap-vibrator
```

Check that the ERM extender is in the tree. This must print lines with `ERM_HAPTIC`:

```sh
grep -n ERM_HAPTIC src/sysfs.rs vendor/etc/init/richtap-vibrator.rc
```

Do not run the binary in Termux. It registers with Binder and needs the ROM.
To test it, use `scripts/quick-test-hal.sh`, see the [README](readme.md).

## Problems

| Message | What to do |
|---|---|
| `cargo: command not found` | Run `pkg install rust clang` again. |
| `Killed` during the build | Out of memory. Close other apps and use the low memory command. |
| `unable to find library -llog` | Run `RUSTFLAGS="-L /system/lib64" cargo build --release --locked`. |
| Download of crates fails | Check the internet connection and run the build again. |
| `the lock file needs to be updated` | Run the build without `--locked`. |
| `No such file` for `/sdcard/Download` | Run `termux-setup-storage` and allow access. |
| `error: could not compile` | Copy the last 20 lines of the output and report them. |

## Notes

- Always build again after you change any file in `src`.
- A change of the `ERM_HAPTIC_*` values in the rc does not need a build. Only a reboot.
- The built binary is not tracked in this repository. Build it and copy it in at install
  time, see the install steps in the [README](readme.md).
