#!/system/bin/sh
# haptic_tune.sh - play with LM haptic timing on the X6739 (AW862xx)
#
# Needs root:
#   adb push haptic_tune.sh /data/local/tmp/
#   adb shell su -c 'sh /data/local/tmp/haptic_tune.sh help'
#
# How the actuator actually works on this device (measured):
#   - intensity / character comes from the PULSE LENGTH, not from gain.
#     The driver's ram_config() picks the RAM wave from that length.
#   - `gain` and `brightness` do not change how hard it feels.
#   - the play path that our HAL (and TWRP) use is:
#         duration=<ms>; activate=1; sleep <ms>; activate=0
#
# So this script only varies `duration`:
#       20  - crisp click
#       50  - firm tap
#      100  - heavy tap (this is the HAL's current maximum)
#      >300  - the driver switches to its looping "long vibration" wave
#
# It also reproduces the HAL's slider mapping so you can audition the exact
# durations the HAL would write for a given OPLUS strength (700..2400).

NODE=/sys/class/leds/vibrator_single
MIN_MS=5
MAX_MS=320

die() { echo "error: $*" >&2; exit 1; }

need_root() {
    [ "$(id -u)" = "0" ] || die "run as root:  su -c 'sh $0 $*'"
    [ -d "$NODE" ] || die "$NODE not found (is aw862xx_haptic.ko loaded?)"
}

# play <duration_ms> [repeats] [gap_ms]
play() {
    ms=${1:-50}
    reps=${2:-1}
    gap=${3:-120}

    case "$ms" in ''|*[!0-9]*) die "duration must be a number of ms" ;; esac
    [ "$ms" -lt "$MIN_MS" ] && ms=$MIN_MS
    [ "$ms" -gt "$MAX_MS" ] && ms=$MAX_MS

    i=0
    while [ "$i" -lt "$reps" ]; do
        echo 255 > "$NODE/gain" 2>/dev/null
        echo "$ms" > "$NODE/duration" || die "cannot write duration"
        echo 1 > "$NODE/activate"   || die "cannot write activate"
        sleep_frac "$ms"
        echo 0 > "$NODE/activate"
        i=$((i + 1))
        [ "$i" -lt "$reps" ] && sleep_frac "$gap"
    done
    echo "played ${reps}x ${ms} ms (gap ${gap} ms)"
}

# sleep with millisecond precision; toybox sleep accepts fractional seconds
sleep_frac() {
    ms=$1
    [ "$ms" -le 0 ] && return
    sec=$(printf "%d.%03d" $((ms / 1000)) $((ms % 1000)))
    sleep "$sec"
}

# sweep <from_ms> <to_ms> <step_ms> [repeats_per_step]
sweep() {
    from=${1:-20}; to=${2:-200}; step=${3:-20}; reps=${4:-2}
    [ "$step" -le 0 ] && die "step must be > 0"
    ms=$from
    while [ "$ms" -le "$to" ]; do
        echo "--- ${ms} ms"
        play "$ms" "$reps" 150
        sleep_frac 600
        ms=$((ms + step))
    done
}

# strength <700..2400> - what the HAL writes for this OPLUS strength
strength() {
    s=${1:-1700}
    case "$s" in ''|*[!0-9]*) die "strength must be a number 700..2400" ;; esac
    [ "$s" -lt 700 ] && s=700
    [ "$s" -gt 2400 ] && s=2400
    # HAL: short effects (<=80 ms) use 15 + (s-700)*35/1700
    short=$((15 + (s - 700) * 35 / 1700))
    # HAL: longer pulses use 20 + (s-700)*80/1700
    long=$((20 + (s - 700) * 80 / 1700))
    echo "strength $s -> click path ${short} ms | long path ${long} ms"
    play "$short" 2 150
    sleep_frac 500
    play "$long" 2 150
}

# ladder - audition the slider range the HAL produces
ladder() {
    for s in 700 1000 1200 1500 1700 2000 2200 2400; do
        printf "strength %-5s " "$s"
        strength "$s"
        sleep_frac 800
    done
}

info() {
    echo "node dir: $NODE"
    for n in duration activate activate_mode index gain state brightness cont_drv_lvl; do
        printf "  %-14s " "$n"
        cat "$NODE/$n" 2>/dev/null | tr '\n' ' '
        echo
    done
    echo
    echo "last driver decisions:"
    dmesg | grep -E "ram_config: duration|set_gain|index_store" | tail -6
}

usage() {
    cat <<EOF
haptic_tune.sh - LM haptic timing bench (needs root)

  play <ms> [repeats] [gap_ms]   one pulse length, repeated
                                 e.g. play 50 3 120
  sweep <from> <to> <step> [n]   walk the length range
                                 e.g. sweep 20 200 20 2
  strength <700..2400>           play what the HAL writes for this slider value
  ladder                         audition 700..2400 (the slider range)
  info                           node state + last driver decisions

Typical values on this device (current HAL tuning):
   20 ms  crisp click        50 ms  firm tap        100 ms  heavy tap
  >300 ms  driver loops its "long vibration" wave (avoid)

Notes:
  - intensity comes from the pulse length; `gain` does nothing audible here
  - run everything through su, e.g.
        adb shell su -c 'sh /data/local/tmp/haptic_tune.sh ladder'
EOF
}

[ "$(id -u)" = "0" ] || exec su -c "sh $0 $*"

cmd=${1:-help}
shift 2>/dev/null
case "$cmd" in
    play)     need_root; play "$@" ;;
    sweep)    need_root; sweep "$@" ;;
    strength) need_root; strength "$@" ;;
    ladder)   need_root; ladder ;;
    info)     need_root; info ;;
    help|-h|--help) usage ;;
    *) usage ;;
esac
