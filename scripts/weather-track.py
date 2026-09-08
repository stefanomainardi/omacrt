#!/usr/bin/env python3
"""Build the sound for a video of the weather page.

`--dump` writes frames and no audio, and the ambient page's own sound is a
looping voice rather than an event, so `--record` does not catch it either.
This makes the track out of the launcher's own loops instead, which is why
the sound on a video of the page is the sound on the television.

    omarchy-crt-shell --dump-audio sfx
    scripts/weather-track.py loop rain 8.0 out.wav
    scripts/weather-track.py night 6.0 out.wav
    scripts/weather-track.py storm 6.0 out.wav --thunder-at 4.0
    scripts/weather-track.py dawn  8.0 out.wav

The storm phases its loop so the roll of thunder lands a beat after the
lightning in the picture, and `dawn` crossfades the crickets into the birds
at the moment the sun comes up, which is what the launcher itself would do.
Pass the directory `--dump-audio` wrote as SFX if it is not `sfx2`.
"""
import os
import struct
import sys
import wave

RATE = 48000
SFX = os.environ.get("SFX", "sfx2")


def read(name):
    w = wave.open(f"{SFX}/weather-{name}.wav")
    n = w.getnframes()
    return list(struct.unpack(f"<{n}h", w.readframes(n)))


def looped(name, seconds, phase=0.0):
    """The loop, repeated to fill `seconds`, starting `phase` seconds in."""
    src = read(name)
    total = int(seconds * RATE)
    start = int(phase * RATE) % len(src)
    return [src[(start + i) % len(src)] for i in range(total)]


def crossfade(a, b, at, over):
    """`a` until `at`, then `over` seconds of fade into `b`."""
    out = list(a)
    start = int(at * RATE)
    n = int(over * RATE)
    for i in range(n):
        if start + i >= len(out):
            break
        k = i / n
        out[start + i] = int(a[start + i] * (1 - k) + b[start + i] * k)
    for i in range(start + n, len(out)):
        out[i] = b[i]
    return out


def write(path, data):
    with wave.open(path, "w") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(struct.pack(f"<{len(data)}h", *data))


def main():
    scene = sys.argv[1]
    seconds = float(sys.argv[2]) if scene != "loop" else 0.0
    out = sys.argv[3] if scene != "loop" else ""
    if scene == "loop":
        # Any of the loops, straight: `loop rain 8.0 out.wav`.
        which = sys.argv[2]
        seconds = float(sys.argv[3])
        out = sys.argv[4]
        data = looped(which, seconds)
    elif scene == "night":
        data = looped("night", seconds)
    elif scene == "storm":
        # The near roll sits 0.6 s into the loop; put it a beat after the
        # lightning, the way thunder actually arrives.
        strike = float(sys.argv[sys.argv.index("--thunder-at") + 1])
        data = looped("storm", seconds, phase=-(strike + 0.35 - 0.6))
    elif scene == "dawn":
        # Crickets until the sun is up, then birds: the same two loops the
        # ambient page would fade between, at the moment it would do it.
        night = looped("night", seconds)
        calm = looped("calm", seconds)
        data = crossfade(night, calm, at=3.0, over=1.5)
    else:
        raise SystemExit(f"no such scene: {scene}")
    # Everything fades in and out over a third of a second, because a clip
    # that starts mid-hiss sounds like a fault.
    edge = int(0.33 * RATE)
    for i in range(edge):
        k = i / edge
        data[i] = int(data[i] * k)
        data[-1 - i] = int(data[-1 - i] * k)
    write(out, data)
    print(out, len(data) / RATE, "s")


main()
