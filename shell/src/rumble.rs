//! Vibration, where the pad has it and the console had it.
//!
//! Rumble is not a setting to hunt for: a pad that can shake should shake in
//! the games whose consoles shook, and stay still in the ones that did not. A
//! Nintendo 64 needs its Rumble Pak in the controller slot, a Dreamcast its
//! Purupuru pack in the second slot of the port, a GameCube a flag, a
//! PlayStation a DualShock rather than the pad that came in the box.
//!
//! So the launcher looks at what is plugged in, and asks each emulator for the
//! part that makes it work. Nothing is turned on for a pad that cannot do it,
//! which would leave a memory card slot filled with a vibration pack for
//! nothing.

/// True when a gamepad with force feedback is connected.
///
/// The kernel lists every input device in `/proc/bus/input/devices`, one block
/// per device, and prints an `FF=` line only for the ones that can play a
/// force feedback effect. A block that has that line and gamepad buttons is a
/// pad that can shake.
pub fn pad_can_rumble() -> bool {
    let Ok(text) = std::fs::read_to_string("/proc/bus/input/devices") else {
        return false;
    };
    any_pad_rumbles(&text)
}

/// The same, over the text of that file, so it can be checked without one.
fn any_pad_rumbles(devices: &str) -> bool {
    devices.split("\n\n").any(|block| {
        let ff = block
            .lines()
            .filter_map(|l| l.strip_prefix("B: FF="))
            .any(|mask| {
                mask.split_whitespace()
                    .any(|w| !w.trim_matches('0').is_empty())
            });
        // A gamepad rather than a rumbling steering wheel or a phone: it has
        // absolute axes and its own button block.
        let pad = block.lines().any(|l| l.starts_with("B: ABS="))
            && block.lines().any(|l| l.starts_with("B: KEY="));
        ff && pad
    })
}

/// What to ask a system's emulator for so the pad shakes, as core options.
pub fn options(system: &str) -> &'static [(&'static str, &'static str)] {
    match system {
        // The Rumble Pak goes in the slot behind the controller.
        "n64" => &[
            ("mupen64plus-pak1", "rumble"),
            ("mupen64plus-pak2", "rumble"),
        ],
        // Port 1 slot 1 keeps the memory card; the pack goes in slot 2.
        "dreamcast" | "naomi" => &[
            ("reicast_device_port1_slot2", "Purupuru"),
            ("reicast_device_port2_slot2", "Purupuru"),
        ],
        "gamecube" | "wii" => &[("dolphin_enable_rumble", "enabled")],
        "psp" => &[("ppsspp_rumble_enabled", "enabled")],
        _ => &[],
    }
}

/// The RetroArch device a system needs before rumble means anything: a
/// PlayStation only shakes when the pad is a DualShock, which is a device
/// type rather than a core option. `port:id` as `--device` takes it.
pub fn devices(system: &str) -> &'static [&'static str] {
    match system {
        // 261: RETRO_DEVICE_ANALOG with the DualShock subclass, which is what
        // beetle-psx calls the pad with the motors in it.
        "psx" => &["1:261", "2:261"],
        _ => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WITH_FF: &str = "I: Bus=0003 Vendor=2dc8 Product=310a\n\
N: Name=\"8BitDo Ultimate 2C Wireless Controller\"\n\
H: Handlers=event2 js0\n\
B: EV=20000b\n\
B: KEY=7cdb000000000000 0 0 0 0\n\
B: ABS=3003f\n\
B: FF=107030000 0\n";

    const WITHOUT_FF: &str = "I: Bus=0003 Vendor=045e Product=028e\n\
N: Name=\"Some plain pad\"\n\
B: EV=b\n\
B: KEY=7cdb000000000000 0 0 0 0\n\
B: ABS=3003f\n";

    #[test]
    fn a_pad_with_force_feedback_is_recognised() {
        assert!(any_pad_rumbles(WITH_FF));
        assert!(!any_pad_rumbles(WITHOUT_FF));
        assert!(
            any_pad_rumbles(&format!("{WITHOUT_FF}\n{WITH_FF}")),
            "one pad out of two is enough"
        );
        assert!(!any_pad_rumbles(""));
    }

    #[test]
    fn an_ff_line_of_nothing_but_zeroes_is_not_force_feedback() {
        let flat = WITH_FF.replace("B: FF=107030000 0", "B: FF=0 0");
        assert!(!any_pad_rumbles(&flat));
    }

    #[test]
    fn the_consoles_that_shook_ask_for_the_right_part() {
        assert!(options("n64").iter().any(|(k, _)| *k == "mupen64plus-pak1"));
        assert!(options("dreamcast").iter().any(|(_, v)| *v == "Purupuru"));
        assert_eq!(options("gamecube"), &[("dolphin_enable_rumble", "enabled")]);
        assert!(options("snes").is_empty(), "a Super Nintendo did not shake");
        assert_eq!(devices("psx"), &["1:261", "2:261"]);
        assert!(devices("n64").is_empty());
    }
}
