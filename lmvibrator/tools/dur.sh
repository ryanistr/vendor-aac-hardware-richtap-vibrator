#!/system/bin/sh
# dur.sh - feel LM haptic pulse lengths on the X6739 (needs root)
#
#   dur.sh 50              one 50 ms pulse
#   dur.sh 20 50 80 100    play each in turn, 1 s apart   <- find your lineup
#   dur.sh -r3 50          three 50 ms pulses, 150 ms apart
#   dur.sh -g400 20 100    same list, 400 ms between pulses
#
# Intensity on this device comes from the pulse LENGTH: the driver picks the
# RAM wave from it. gain/brightness do not change the feel.
# Current HAL ladder: 20 click / 60 thud / 90 weighted thud / 200 heavy
# (taps for keyboard and touch are kept at 15..50 ms).

N=/sys/class/leds/vibrator_single

[ "$(id -u)" = "0" ] || exec su -c "sh $0 $*"
[ -d "$N" ] || { echo "error: $N missing"; exit 1; }

reps=1
gap=1000
while [ $# -gt 0 ]; do
    case "$1" in
        -r*) reps=${1#-r}; shift ;;
        -r)  reps=$2; shift 2 ;;
        -g*) gap=${1#-g}; shift ;;
        -g)  gap=$2; shift 2 ;;
        -h|--help) sed -n '2,13p' "$0"; exit 0 ;;
        *) break ;;
    esac
done

[ $# -eq 0 ] && { sed -n '2,13p' "$0"; exit 0; }

nap() {
    [ "$1" -le 0 ] && return
    sleep "$(printf '%d.%03d' $(($1 / 1000)) $(($1 % 1000)))"
}

for ms in "$@"; do
    case "$ms" in ''|*[!0-9]*) echo "skip '$ms' (not a number)"; continue ;; esac
    [ "$ms" -lt 1 ] && ms=1
    i=0
    while [ "$i" -lt "$reps" ]; do
        echo 255 > "$N/gain" 2>/dev/null
        echo "$ms" > "$N/duration" || { echo "error: cannot write duration"; exit 1; }
        echo 1 > "$N/activate"
        nap "$ms"
        echo 0 > "$N/activate"
        i=$((i + 1))
        [ "$i" -lt "$reps" ] && nap 150
    done
    echo "played ${ms} ms"
    nap "$gap"
done
