#!/usr/bin/env python3
"""Add a Microsoft "specialized display" VSDB to an EDID, and optionally a
FreeSync range.

The kernel marks a connector non-desktop when its EDID carries a Microsoft
vendor block of version 1 or 2 (the head-mounted display convention). A
desktop compositor then leaves the output alone and offers it for DRM
leasing, which is how omacrt drives the tube directly.

    edid-non-desktop.py /sys/class/drm/card1-HDMI-A-1/edid out.bin
    edid-non-desktop.py IN OUT --freesync 55:66

`--freesync` adds the AMD vendor block that makes amdgpu call the connector
`vrr_capable`, which is what lets the refresh rate be changed by moving the
vertical blanking alone instead of by a mode change. On this chain the
horizontal rate never moves, so every refresh the emulation asks for is
reachable that way; whether the television's vertical circuit follows is the
thing to find out.

The block's shape is taken from real monitors rather than from a
specification: nine bytes, `68 1a 00 00 01 01 <min> <max> 00`, which
edid-decode reads back as "Vendor-Specific Data Block (AMD), Version 1.1".
The last byte is the flags, and it is left at zero on purpose: monitors that
set 0xe6 there are asking for an MCCS conversation over DDC, and amdgpu
withdraws the capability when the sink does not answer it.

amdgpu also wants the range to be wider than ten hertz, and the mode in use
to fall inside it, or it leaves `vrr_capable` at zero.
"""
import sys
import uuid


def freesync_vsdb(spec):
    """`min:max` in hertz as the nine bytes of an AMD VSDB."""
    lo, hi = (int(x) for x in spec.split(":"))
    if not 0 < lo < hi < 256:
        sys.exit(f"freesync range out of order or out of a byte: {spec}")
    if hi - lo <= 10:
        sys.exit(f"amdgpu ignores a range of {hi - lo} Hz: it wants more than 10")
    return bytes([(3 << 5) | 8, 0x1A, 0x00, 0x00, 0x01, 0x01, lo, hi, 0x00])

args = [a for a in sys.argv[1:] if not a.startswith("--")]
freesync = None
if "--freesync" in sys.argv:
    freesync = freesync_vsdb(sys.argv[sys.argv.index("--freesync") + 1])
    args = [a for a in args if a != sys.argv[sys.argv.index("--freesync") + 1]]

src = open(args[0], "rb").read()
if len(src) < 256 or src[128] != 0x02:
    sys.exit("need an EDID with a CTA-861 extension block")
base, cta = src[:128], bytearray(src[128:256])
dtd_off = cta[2]
data = bytes(cta[4:dtd_off])
i, dtds = dtd_off, b""
while i + 18 <= 127 and not (cta[i] == 0 and cta[i + 1] == 0):
    dtds += bytes(cta[i : i + 18])
    i += 18
oui = bytes([0x5C, 0x12, 0xCA])  # Microsoft, little endian
amd = bytes([0x1A, 0x00, 0x00])
# Already overridden once? Keep the block list as it is.
j, has_ms, has_amd = 0, False, False
while j < len(data):
    tag, ln = data[j] >> 5, data[j] & 0x1F
    if tag == 3 and data[j + 1 : j + 4] == oui:
        has_ms = True
    if tag == 3 and data[j + 1 : j + 4] == amd:
        has_amd = True
    j += 1 + ln
container = uuid.uuid5(uuid.NAMESPACE_DNS, "omacrt.display").bytes
vsdb = bytes([(3 << 5) | 21]) + oui + bytes([0x02, 0x00]) + container
body = data if has_ms else data + vsdb
if freesync and not has_amd:
    body += freesync
if 4 + len(body) + len(dtds) > 127:
    sys.exit("no room in the CTA block")
new = bytearray(128)
new[0], new[1], new[3] = 0x02, cta[1], cta[3]
new[2] = 4 + len(body)
new[4 : 4 + len(body)] = body
new[new[2] : new[2] + len(dtds)] = dtds
new[127] = (-sum(new[:127])) & 0xFF
open(args[1], "wb").write(base + bytes(new))
print(
    f"wrote {args[1]}: {len(base) + 128} bytes, non-desktop"
    + (", freesync" if freesync else "")
)
