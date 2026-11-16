#!/usr/bin/env python3
"""Build the per-waveform pattern table used by the LM vibrator HAL.

The X6739's AW862xx cannot replay the OPLUS waveforms: its RAM firmware holds
only a few waves and the driver rejects the OPLUS waveform ids. The OOS15 tree
that this port was derived from ships the real per-effect waveforms as RTP
sample files:

    ROM_WORKDIR/oos15_old/erofs/odm/firmware/*_RTP_<waveform_id>[_<freq>Hz].bin

Each file is 8-bit unsigned amplitude at 24 kHz (verified against
effect_waveform.xml durations: id 41 = 65,633 B = 2729 ms = symphonic). This
script converts every waveform the port can ask for into an envelope table the
HAL replays as a pulse train on the AW862xx.

Output format (little endian), read by LinearMotorVibratorService:

    "LMP1" u32 entry_count
    per entry: i32 waveform_id, u32 segment_count,
               per segment: u16 start_ms, u16 duration_ms, u8 amplitude (0..255)

Usage:
    tools/build_patterns.py \
        --firmware ../../../../../oos15_old/erofs/odm/firmware \
        --xml       ../../../erofs/system/my_product/etc/vibrator/effect_waveform.xml \
        --out       ../../../erofs/vendor/etc/vibrator/patterns.bin
"""

import argparse
import os
import re
import struct
import sys

SAMPLE_RATE = 24000
# Envelope resolution before segmentation; 20 ms matches what the driver can
# start/stop without the LED trigger chattering.
BUCKET_MS = 10
# Buckets at or below this are silence.
SILENCE = 10
# A segment is split when the peak changes by more than this fraction, or when
# it has been running this long (the LED trigger is re-armed every <=255 ms).
AMP_JUMP = 0.28
MAX_SEGMENT_MS = 500
# Reject a file whose length disagrees with the XML duration by more than this.
MAX_DURATION_ERROR = 0.15


def parse_xml(path):
    text = open(path, encoding="utf-8", errors="replace").read()
    rows = [dict(re.findall(r'(\w+)="([^"]*)"', r))
            for r in re.findall(r"<effect ([^/]*)/>", text)]
    best = {}
    for row in rows:
        try:
            wid = int(row["waveform_id"])
            ms = int(row["duration"])
        except (KeyError, ValueError):
            continue
        if wid not in best or ms > best[wid][0]:
            best[wid] = (ms, row.get("usage", ""), row.get("desc", ""))
    return best


def index_firmware(directory):
    """Map waveform id -> files.

    Names come in two shapes and both must be indexed:
        aw8697_Joy_channel_RTP_13_175Hz.bin     (has _RTP_)
        aw8697_Newfeeling_98_165HZ.bin          (no _RTP_, id before the freq)
    """
    files = {}
    for name in os.listdir(directory):
        if not name.endswith(".bin"):
            continue
        stem = name[:-4]
        stem = re.sub(r"_\d+[Hh][Zz]$", "", stem)          # drop the frequency suffix
        match = re.search(r"_(\d+)$", stem)                # id is the trailing number
        if not match:
            continue
        files.setdefault(int(match.group(1)), []).append(os.path.join(directory, name))
    return files


def envelope(path):
    data = open(path, "rb").read()
    # 8-bit unsigned amplitude; bucket by peak so short transients survive.
    bucket_samples = max(1, SAMPLE_RATE * BUCKET_MS // 1000)
    buckets = []
    for start in range(0, len(data), bucket_samples):
        chunk = data[start:start + bucket_samples]
        buckets.append(max(chunk))
    return buckets


def classify(duration_ms, amplitude):
    """Haptic character of a segment, from its own envelope.

    0 tick   - very short, used for keys/clicks
    1 click  - short, slightly firmer
    2 thud   - medium, the body of a notification
    3 heavy  - long and loud, a deliberate "thump"
    """
    if duration_ms <= 30:
        return 0
    if duration_ms <= 80:
        return 1
    if duration_ms <= 250:
        return 2
    return 3


def collapse_chatter(segments, max_piece=60, max_gap=30, out_min=20, out_max=60):
    """Collapse a run of tiny, tightly spaced pulses into one click.

    Some OPLUS waveforms (the charging animation, for instance) encode one felt
    "click" as a burst of 10-20 ms segments 10-20 ms apart. Replaying them
    individually turns a single click into a machine-gun rattle, so such a run
    is merged back into one click-length pulse.
    """
    if not segments:
        return segments

    merged = []
    i = 0
    n = len(segments)
    while i < n:
        start, duration, amp = segments[i]
        if duration <= max_piece:
            j = i
            run_end = start + duration
            run_peak = amp
            while j + 1 < n:
                nstart, nduration, namp = segments[j + 1]
                if nduration <= max_piece and (nstart - run_end) <= max_gap:
                    run_end = max(run_end, nstart + nduration)
                    run_peak = max(run_peak, namp)
                    j += 1
                else:
                    break
            if j > i:
                span = run_end - start
                merged.append((start, max(out_min, min(out_max, span)), run_peak))
                i = j + 1
                continue
        merged.append((start, duration, amp))
        i += 1
    return merged


def segment(buckets):
    segments = []
    run_start = None
    run_peak = 0
    for index, amp in enumerate(buckets):
        if amp <= SILENCE:
            if run_start is not None:
                segments.append((run_start * BUCKET_MS,
                                 (index * BUCKET_MS) - (run_start * BUCKET_MS),
                                 run_peak))
                run_start = None
                run_peak = 0
            continue
        if run_start is None:
            run_start, run_peak = index, amp
            continue
        jump = abs(amp - run_peak) / 255.0
        length = (index * BUCKET_MS) - (run_start * BUCKET_MS)
        if jump > AMP_JUMP or length >= MAX_SEGMENT_MS:
            segments.append((run_start * BUCKET_MS, length, run_peak))
            run_start, run_peak = index, amp
        else:
            run_peak = max(run_peak, amp)
    if run_start is not None:
        segments.append((run_start * BUCKET_MS,
                         (len(buckets) * BUCKET_MS) - (run_start * BUCKET_MS),
                         run_peak))

    # Leading silence in the file exists to align with the sound; the HAL is
    # called at t=0, so shift the pattern to start immediately. Without this a
    # short effect plays nothing at all when its wave begins seconds in.
    if segments:
        lead = segments[0][0]
        if lead:
            segments = [(start - lead, duration, amp)
                        for start, duration, amp in segments]

    # Keep quiet patterns audible: lift the envelope so its peak reaches a
    # usable level, without letting a loud pattern get louder.
    peak = max((amp for _, _, amp in segments), default=0)
    if 0 < peak < 200:
        gain = min(255.0 / peak, 2.0)
        segments = [(start, duration, min(255, int(round(amp * gain))))
                    for start, duration, amp in segments]

    return segments


def main():
    here = os.path.dirname(os.path.abspath(__file__))
    ap = argparse.ArgumentParser()
    ap.add_argument("--firmware", default=os.path.join(
        here, "../../../../../oos15_old/erofs/odm/firmware"))
    ap.add_argument("--xml", default=os.path.join(
        here, "../../../../erofs/system/my_product/etc/vibrator/effect_waveform.xml"))
    ap.add_argument("--out", default=os.path.join(
        here, "../../../../erofs/vendor/etc/vibrator/patterns.bin"))
    ap.add_argument("--tsv", default=os.path.join(here, "../patterns/patterns.tsv"),
                    help="readable snapshot of the generated table")
    ap.add_argument("--from-tsv", default=None,
                    help="rebuild the binary from a previous --tsv snapshot "
                         "instead of the firmware tree")
    args = ap.parse_args()

    firmware = os.path.abspath(args.firmware)

    if args.from_tsv:
        entries = []
        with open(args.from_tsv, encoding="utf-8") as handle:
            for line in handle:
                line = line.rstrip("\n")
                if not line or line.startswith("#"):
                    continue
                wid, usage, segments = line.split("\t")
                parsed = []
                for item in segments.split(","):
                    fields = [int(v) for v in item.split(":")]
                    start, duration, amp = fields[:3]
                    kind = fields[3] if len(fields) > 3 else classify(duration, amp)
                    parsed.append((start, duration, amp))
                entries.append((int(wid), parsed))
    else:
        if not os.path.isdir(firmware):
            sys.exit(f"firmware directory not found: {firmware}")

        wanted = parse_xml(os.path.abspath(args.xml))
        files = index_firmware(firmware)

        entries = []
        for wid, (ms, usage, desc) in sorted(wanted.items()):
            candidates = files.get(wid)
            if not candidates:
                continue
            best = min(candidates, key=lambda p: abs(os.path.getsize(p) / SAMPLE_RATE * 1000.0 - ms))
            actual = os.path.getsize(best) / SAMPLE_RATE * 1000.0
            if abs(actual - ms) / max(ms, 1) > MAX_DURATION_ERROR:
                continue
            segments = collapse_chatter(segment(envelope(best)))
            if not segments:
                continue
            entries.append((wid, segments))
            print(f"  id {wid:4d} {ms:6d} ms  {len(segments):3d} segments  {usage:12s} {os.path.basename(best)}")

        usages = {wid: info[1] for wid, info in wanted.items()}
        os.makedirs(os.path.dirname(os.path.abspath(args.tsv)), exist_ok=True)
        with open(os.path.abspath(args.tsv), "w", encoding="utf-8") as handle:
            handle.write("# waveform_id\tusage\tstart_ms:duration_ms:amplitude:kind,...\n")
            for wid, segments in entries:
                packed = ",".join(f"{s}:{d}:{a}:{classify(d, a)}" for s, d, a in segments)
                handle.write(f"{wid}\t{usages.get(wid, '')}\t{packed}\n")
        print(f"readable snapshot -> {os.path.abspath(args.tsv)}")

    with open(os.path.abspath(args.out), "wb") as out:
        out.write(b"LMP2")
        out.write(struct.pack("<I", len(entries)))
        for wid, segments in entries:
            out.write(struct.pack("<iI", wid, len(segments)))
            for start, duration, amp in segments:
                out.write(struct.pack("<HHBB", start, min(duration, 0xFFFF), amp,
                                      classify(duration, amp)))

    total = os.path.getsize(os.path.abspath(args.out))
    print(f"\n{len(entries)} waveforms, {total} bytes -> {os.path.abspath(args.out)}")
    if not args.from_tsv:
        missing = [w for w in parse_xml(os.path.abspath(args.xml)) if w not in index_firmware(firmware)]
        print(f"{len(missing)} waveform ids have no firmware file (single-pulse fallback)")


if __name__ == "__main__":
    main()
