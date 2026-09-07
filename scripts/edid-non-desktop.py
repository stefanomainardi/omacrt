#!/usr/bin/env python3
"""Add a Microsoft "specialized display" VSDB to an EDID.

The kernel marks a connector non-desktop when its EDID carries a Microsoft
vendor block of version 1 or 2 (the head-mounted display convention). A
desktop compositor then leaves the output alone and offers it for DRM
leasing, which is how omarchy-crt drives the tube directly.

    edid-non-desktop.py /sys/class/drm/card1-HDMI-A-1/edid out.bin
"""
import sys
import uuid

src = open(sys.argv[1], "rb").read()
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
# Already overridden once? Keep the block list as it is.
j, has_ms = 0, False
while j < len(data):
    tag, ln = data[j] >> 5, data[j] & 0x1F
    if tag == 3 and data[j + 1 : j + 4] == oui:
        has_ms = True
    j += 1 + ln
container = uuid.uuid5(uuid.NAMESPACE_DNS, "omarchy-crt.display").bytes
vsdb = bytes([(3 << 5) | 21]) + oui + bytes([0x02, 0x00]) + container
body = data if has_ms else data + vsdb
if 4 + len(body) + len(dtds) > 127:
    sys.exit("no room in the CTA block")
new = bytearray(128)
new[0], new[1], new[3] = 0x02, cta[1], cta[3]
new[2] = 4 + len(body)
new[4 : 4 + len(body)] = body
new[new[2] : new[2] + len(dtds)] = dtds
new[127] = (-sum(new[:127])) & 0xFF
open(sys.argv[2], "wb").write(base + bytes(new))
print(f"wrote {sys.argv[2]}: {len(base) + 128} bytes, non-desktop")
