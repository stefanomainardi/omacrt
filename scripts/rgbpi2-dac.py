#!/usr/bin/env python3
"""Control the sync combiner of an RGB-Pi 2 HDMI to SCART DAC.

The DAC is configured over the HDMI DDC bus (I2C) at 7-bit address 0x78.
Without configuration it outputs separated H and V sync, which a SCART TV
cannot lock to, so the host has to select composite sync once per plug.

    rgbpi2-dac.py csync and|xor|separate [CONNECTOR]
    rgbpi2-dac.py reset [CONNECTOR]
    rgbpi2-dac.py status [CONNECTOR]
    rgbpi2-dac.py watch [CONNECTOR]     # reset the DAC whenever it loses lock

CONNECTOR is a DRM connector name (card1-HDMI-A-1 or HDMI-A-1); the default is
the first connected HDMI connector whose EDID names a Mortaca device. The I2C
device node is usually root only: add yourself to the `i2c` group or run with
sudo.

Register map, page select is register 0x00:
    page 4, reg 0xB5: 0x06 = csync AND, 0x0C = csync XOR, 0x00 = separated H/V
    page 0, reg 0x60: 0x00 asserts reset, 0xFF releases it
    page 0, reg 0x61: lock status, 0xFF locked, 0xEF after a signal loss
"""

import fcntl
import glob
import os
import sys
import time

ADDR = 0x78
I2C_SLAVE_FORCE = 0x0706
CSYNC = {"and": 0x06, "xor": 0x0C, "separate": 0x00}


def connectors():
    for path in sorted(glob.glob("/sys/class/drm/card*-HDMI-A-*")):
        try:
            with open(os.path.join(path, "status")) as f:
                if f.read().strip() != "connected":
                    continue
        except OSError:
            continue
        yield path


def pick_connector(name):
    if name:
        if not name.startswith("card"):
            hits = glob.glob(f"/sys/class/drm/card*-{name}")
            if not hits:
                sys.exit(f"no connector {name}")
            return hits[0]
        return f"/sys/class/drm/{name}"
    for path in connectors():
        try:
            with open(os.path.join(path, "edid"), "rb") as f:
                if b"MORTACA" in f.read():
                    return path
        except OSError:
            pass
    for path in connectors():
        return path
    sys.exit("no connected HDMI connector")


def bus_of(connector):
    ddc = os.path.realpath(os.path.join(connector, "ddc"))
    return "/dev/" + os.path.basename(ddc)


class Dac:
    def __init__(self, bus):
        self.bus = bus
        try:
            self.fd = os.open(bus, os.O_RDWR)
        except PermissionError:
            sys.exit(f"{bus}: permission denied (join the i2c group or use sudo)")
        fcntl.ioctl(self.fd, I2C_SLAVE_FORCE, ADDR)

    def write(self, reg, value):
        if os.write(self.fd, bytes([reg, value])) != 2:
            raise OSError(f"short write to reg 0x{reg:02X}")

    def read(self, reg):
        if os.write(self.fd, bytes([reg])) != 1:
            raise OSError(f"cannot select reg 0x{reg:02X}")
        return os.read(self.fd, 1)[0]

    def page(self, n):
        self.write(0x00, n)

    def present(self):
        try:
            self.page(0)
            return True
        except OSError:
            return False

    def csync(self, mode):
        self.page(4)
        self.write(0xB5, CSYNC[mode])

    def csync_value(self):
        self.page(4)
        return self.read(0xB5)

    def reset(self, hold=2.0):
        # A reset clears the csync selection, so restore it afterwards.
        keep = self.csync_value()
        self.page(0)
        self.write(0x60, 0x00)
        time.sleep(hold)
        self.write(0x60, 0xFF)
        time.sleep(0.2)
        self.page(4)
        self.write(0xB5, keep)

    def status(self):
        self.page(0)
        return self.read(0x61)


def main(argv):
    if len(argv) < 2 or argv[1] in ("-h", "--help"):
        print((__doc__ or "").strip())
        return 0
    cmd = argv[1]
    if cmd == "csync":
        if len(argv) < 3 or argv[2] not in CSYNC:
            sys.exit("csync needs one of: " + ", ".join(CSYNC))
        name = argv[3] if len(argv) > 3 else None
    else:
        name = argv[2] if len(argv) > 2 else None

    connector = pick_connector(name)
    bus = bus_of(connector)
    dac = Dac(bus)
    if not dac.present():
        sys.exit(f"no RGB-Pi 2 at 0x{ADDR:02X} on {bus} ({os.path.basename(connector)})")

    if cmd == "csync":
        dac.csync(argv[2])
        print(f"{os.path.basename(connector)} {bus}: csync {argv[2]}")
    elif cmd == "reset":
        dac.reset()
        print(f"{os.path.basename(connector)} {bus}: reset done")
    elif cmd == "status":
        v = dac.status()
        state = "locked" if v == 0xFF else "lost" if v == 0xEF else "unknown"
        names = {v: k for k, v in CSYNC.items()}
        c = dac.csync_value()
        print(f"{os.path.basename(connector)} {bus}: reg 0x61 = 0x{v:02X} ({state}), csync {names.get(c, hex(c))}")
    elif cmd == "watch":
        last = dac.status()
        print(f"watching {bus}, reg 0x61 = 0x{last:02X}")
        while True:
            time.sleep(0.001)
            try:
                v = dac.status()
            except OSError:
                continue
            if last == 0xFF and v == 0xEF:
                print("lock lost, resetting")
                dac.reset()
                v = dac.status()
            last = v
    else:
        sys.exit(f"unknown command {cmd}")
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main(sys.argv))
    except KeyboardInterrupt:
        pass
