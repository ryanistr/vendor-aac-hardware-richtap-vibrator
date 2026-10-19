#!/system/bin/sh
# Run this HAL from /data/local/tmp to try a build without reflashing.
# Run as root. A reboot undoes everything, including the stopped service.
#
# Usage:
#   su -c "sh /sdcard/Download/quick-test-hal.sh"
#   su -c "sh /sdcard/Download/quick-test-hal.sh /path/to/binary"
#   su -c "sh /sdcard/Download/quick-test-hal.sh /path/to/binary erm"
#
# The second argument is "erm" to start it with ERM_HAPTIC_ENABLE=1, which extends
# pulses shorter than 50 ms to 80 ms. Only useful on an ERM motor.

BIN_SRC="${1:-/sdcard/Download/vendor-aac-hardware-richtap-vibrator}"
ERM="${2:-}"
BIN=/data/local/tmp/richtap-hal
LOG=/data/local/tmp/richtap-hal.log

[ "$(id -u)" = "0" ] || { echo "Root is required"; exit 1; }
[ -f "$BIN_SRC" ] || { echo "Binary not found: $BIN_SRC"; exit 1; }

L=/sys/class/leds/vibrator
[ -d "$L" ] || L=/sys/class/leds/vibrator_single
[ -d "$L" ] || { echo "No vibrator LED under /sys/class/leds"; exit 1; }

cp "$BIN_SRC" "$BIN" && chmod 755 "$BIN" || { echo "Copy failed"; exit 1; }

case "$L" in
*/vibrator)
    # ERM port: the same steps as the boot block in richtap-vibrator.rc
    echo transient > $L/trigger
    echo 1 > $L/state
    chown system:system $L/state $L/activate $L/duration
    chmod 0660 $L/state $L/activate $L/duration
    ;;
*)
    echo "LRA port $L: skipping the ERM trigger/state setup"
    ;;
esac

# Stop the vibrator HAL that is running now, so this one can take its name.
# A reboot starts it again.
pkill -x richtap-hal 2>/dev/null
for s in $(getprop | sed -n 's/^\[init\.svc\.\(.*vibrator.*\)\]: \[running\]$/\1/p'); do
    echo "stopping service: $s"
    stop "$s"
done
sleep 1

if [ "$ERM" = "erm" ]; then
    echo "starting with ERM_HAPTIC_ENABLE=1"
    ERM_HAPTIC_ENABLE=1 ERM_HAPTIC_THRESHOLD_MS=50 ERM_HAPTIC_TARGET_MS=80 nohup "$BIN" >"$LOG" 2>&1 &
else
    echo "starting without the ERM boost"
    nohup "$BIN" >"$LOG" 2>&1 &
fi
sleep 2

echo
echo "selinux : $(getenforce 2>/dev/null)"
echo "led     : $L"
echo "hal pid : $(pgrep -x richtap-hal | head -1)"
echo "service : $(service list 2>/dev/null | grep -i vibrator | head -3)"
echo "boost   : $(grep -m1 'erm boost' "$LOG" 2>/dev/null)"

if [ -z "$(pgrep -x richtap-hal | head -1)" ]; then
    echo
    echo "The HAL did not start. Last lines of its log:"
    tail -n 10 "$LOG"
    exit 1
fi

echo
echo "Long pulse, must stop by itself:"
cmd vibrator_manager synced oneshot 300 255
echo
echo "Now type on the keyboard. Without the boost the ticks stay short, with it"
echo "'boost 10 ms to 80 ms' appears in the log below."
echo "Reboot to go back to the normal setup."
