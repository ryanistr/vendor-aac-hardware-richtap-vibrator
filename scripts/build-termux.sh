#!/data/data/com.termux/files/usr/bin/sh
# Build the HAL in Termux and copy it to /sdcard/Download.
#
# Usage:
#   sh scripts/build-termux.sh           normal build
#   sh scripts/build-termux.sh lowmem    for phones that run out of memory
#
# Do not run the binary in Termux. It registers with Binder and needs the ROM.

set -e

cd "$(dirname "$0")/.."

NAME=vendor-aac-hardware-richtap-vibrator
BIN=target/release/$NAME
OUT="${OUT:-/sdcard/Download/$NAME}"

if [ "$(uname -m)" != "aarch64" ]; then
    echo "Warning: uname -m is '$(uname -m)', not aarch64." >&2
    echo "The binary will not run on an arm64 ROM." >&2
fi

command -v cargo >/dev/null 2>&1 || {
    echo "cargo not found. Run: pkg install rust clang" >&2
    exit 1
}

echo "==> Build"
if [ "$1" = "lowmem" ]; then
    CARGO_PROFILE_RELEASE_LTO=off CARGO_BUILD_JOBS=2 cargo build --release --locked
else
    cargo build --release --locked
fi

echo "==> Check"
[ -s "$BIN" ] || { echo "Binary not found or empty: $BIN" >&2; exit 1; }
ls -l "$BIN"

echo "==> Copy"
if cp "$BIN" "$OUT" 2>/dev/null; then
    echo "Copied to $OUT"
else
    echo "Could not copy to $OUT. Run termux-setup-storage and try again." >&2
fi

echo "==> SHA-256"
sha256sum "$BIN"
