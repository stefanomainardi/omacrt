//! Sega Model 3 through Supermodel: the one emulator on the tube that is not
//! RetroArch.
//!
//! Model 3 boards draw 496x384 for a 24 kHz monitor, which a 15 kHz set
//! cannot show. MAME reduces a Model 2 board's picture to the tube's 240
//! lines after drawing it; Supermodel can draw its 3D at whatever size it is
//! given, so here it is handed the tube's own frame and the scene comes out
//! drawn at 240 lines, with cleaner edges than a reduction gives. Only the 2D
//! layers, the text and the gauges, are still scaled.
//!
//! The Arch package installs the program as `supermodel-binary` and a desktop
//! wrapper as `supermodel` that forces X11 and wants its ROMs in a folder of
//! its own, so the binary is called directly and told where its list of
//! games is.

use std::path::{Path, PathBuf};
use std::process::Command;

/// What the window calls itself, for the compositor to raise it.
pub const APP_ID: &str = "omacrt-supermodel";

/// The board's own field rate, as `-true-hz` runs it.
pub const HZ: f32 = 57.524;

/// The program, preferring the package's real binary to its wrapper.
pub fn binary() -> Option<PathBuf> {
    let packaged = PathBuf::from("/usr/bin/supermodel-binary");
    if packaged.exists() {
        return Some(packaged);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|d| d.join("supermodel"))
        .find(|p| p.is_file() && !is_script(p))
}

/// The package's `supermodel` is a shell script; a build from source is not.
fn is_script(p: &Path) -> bool {
    std::fs::read(p).is_ok_and(|b| b.starts_with(b"#!"))
}

fn config_home() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| crate::library::home().join(".config"))
        .join("supermodel")
        .join("Config")
}

/// The list of games Supermodel needs: the user's copy, else the package's,
/// else the one beside a binary built from source.
fn games_xml(binary: &Path) -> Option<PathBuf> {
    let beside = binary
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("Config").join("Games.xml"));
    [
        Some(config_home().join("Games.xml")),
        Some(PathBuf::from("/usr/share/supermodel/Config/Games.xml")),
        beside,
    ]
    .into_iter()
    .flatten()
    .find(|p| p.exists())
}

/// Where Supermodel keeps what it writes, and looks for its Assets: it picks
/// `~/.local/share/supermodel` when the working directory has no `Config`
/// of its own and there is no `~/.supermodel`.
fn data_home() -> PathBuf {
    crate::library::home().join(".local/share/supermodel")
}

/// Copy Supermodel's Assets (the gun crosshairs it loads at start, and
/// refuses to start without) where it looks for them, the way the package's
/// own desktop wrapper does on a first run. Nothing is touched once they are
/// there.
pub fn ensure_assets(binary: &Path) -> std::io::Result<()> {
    let dest = data_home().join("Assets");
    if std::fs::read_dir(&dest).is_ok_and(|mut d| d.next().is_some()) {
        return Ok(());
    }
    let beside = binary
        .parent()
        .and_then(Path::parent)
        .map(|d| d.join("Assets"));
    let Some(src) = [Some(PathBuf::from("/usr/share/supermodel/Assets")), beside]
        .into_iter()
        .flatten()
        .find(|p| p.is_dir())
    else {
        return Ok(());
    };
    std::fs::create_dir_all(&dest)?;
    for entry in std::fs::read_dir(src)?.flatten() {
        if entry.path().is_file() {
            std::fs::copy(entry.path(), dest.join(entry.file_name()))?;
        }
    }
    Ok(())
}

/// Where Supermodel's log goes. It is also how the sweep for orphans knows a
/// Supermodel as one of ours: the path is on its command line.
pub fn log_path() -> PathBuf {
    crate::crt::state_dir().join("supermodel.log")
}

/// The controls for a pad, written once if the user has no Supermodel.ini.
/// Supermodel counts buttons from one, so on an XInput pad A is BUTTON1,
/// Select BUTTON7 and Start BUTTON8; its own defaults put Start and the coin
/// on 9 and 10, which a pad does not have. The triggers are the pedals and
/// the shoulders the gears, with A and B as pedals too for anyone who
/// prefers buttons. The d-pad does what the left stick does: it is a hat,
/// which Supermodel reads apart from the stick. A trigger rests at the bottom of its axis rather than in
/// the middle, so its off value is moved there: otherwise the first half of
/// the travel does nothing.
const INI: &str = r#"; Written by omacrt for a pad on the television. Edit freely: omacrt only
; writes this file when there is none.
[ Global ]
InputStart1 = "KEY_1,JOY1_BUTTON8"
InputCoin1 = "KEY_3,JOY1_BUTTON7"
InputServiceA = "KEY_5"
InputTestA = "KEY_6"
InputSteering = "JOY1_XAXIS"
InputSteeringLeft = "KEY_LEFT,JOY1_POV1_LEFT"
InputSteeringRight = "KEY_RIGHT,JOY1_POV1_RIGHT"
InputJoyUp = "KEY_UP,JOY1_UP,JOY1_POV1_UP"
InputJoyDown = "KEY_DOWN,JOY1_DOWN,JOY1_POV1_DOWN"
InputJoyLeft = "KEY_LEFT,JOY1_LEFT,JOY1_POV1_LEFT"
InputJoyRight = "KEY_RIGHT,JOY1_RIGHT,JOY1_POV1_RIGHT"
InputAnalogJoyUp = "KEY_UP,JOY1_POV1_UP"
InputAnalogJoyDown = "KEY_DOWN,JOY1_POV1_DOWN"
InputAnalogJoyLeft = "KEY_LEFT,JOY1_POV1_LEFT"
InputAnalogJoyRight = "KEY_RIGHT,JOY1_POV1_RIGHT"
InputSkiUp = "KEY_UP,JOY1_POV1_UP"
InputSkiDown = "KEY_DOWN,JOY1_POV1_DOWN"
InputSkiLeft = "KEY_LEFT,JOY1_POV1_LEFT"
InputSkiRight = "KEY_RIGHT,JOY1_POV1_RIGHT"
InputFishingRodUp = "KEY_UP,JOY1_POV1_UP"
InputFishingRodDown = "KEY_DOWN,JOY1_POV1_DOWN"
InputFishingRodLeft = "KEY_LEFT,JOY1_POV1_LEFT"
InputFishingRodRight = "KEY_RIGHT,JOY1_POV1_RIGHT"
InputAccelerator = "KEY_UP,JOY1_BUTTON1,JOY1_RZAXIS_POS"
InputBrake = "KEY_DOWN,JOY1_BUTTON2,JOY1_ZAXIS_POS"
InputGearShiftUp = "KEY_Y,JOY1_BUTTON6"
InputGearShiftDown = "KEY_H,JOY1_BUTTON5"
InputHandBrake = "KEY_S,JOY1_BUTTON3"
InputViewChange = "KEY_A,JOY1_BUTTON4"
InputVR1 = "KEY_A,JOY1_BUTTON4"
InputJoy1ZOffVal = -32768
InputJoy1RZOffVal = -32768
"#;

/// Write the controls file if there is none, or if the one there is the
/// defaults Supermodel writes for itself on a first run, which put Start and
/// the coin on buttons a pad does not have. A file the user has changed, by
/// hand or through Supermodel's own input setup, is left alone.
pub fn ensure_ini() -> std::io::Result<bool> {
    let ini = config_home().join("Supermodel.ini");
    if std::fs::read_to_string(&ini).is_ok_and(|s| !is_stock(&s)) {
        return Ok(false);
    }
    std::fs::create_dir_all(config_home())?;
    std::fs::write(&ini, INI)?;
    Ok(true)
}

/// Supermodel's own first run file opens with this comment; once its input
/// setup has saved anything the comment says so instead.
fn is_stock(ini: &str) -> bool {
    ini.lines()
        .take(4)
        .any(|l| l.trim() == ";; Default settings.")
}

// ------------------------------------------------------- twin cabinets

/// Sets built for two cabinets linked by a network board. Out of the box
/// each is the master of a pair and waits for the other cabinet: without a
/// network board it stops at "network board not present", with Supermodel's
/// simulated one at "network checking", for ever. The operator's fix is
/// LINK ID = SINGLE in the test menu. Each family is listed whole, because
/// the NVRAM is named after the set Supermodel finds in the zip, which need
/// not be the file's name: a `scud.zip` holding the Australian set saves
/// `scudau.nv`.
const TWIN: &[&[&str]] = &[
    &["scud", "scudau", "scudplus", "scudplusa"],
    &["daytona2", "dayto2pe"],
];

fn nvram_dir() -> PathBuf {
    data_home().join("NVRAM")
}

/// Set every twin set of this file's family that has an NVRAM to a single
/// cabinet, and the names of those that needed it. A first run has no
/// NVRAM yet: the game comes up with its own defaults and stops, and the
/// file it saves on the way out is set right here when it ends.
pub fn single_cabinet(rom: &Path) -> Vec<String> {
    let stem = rom.file_stem().and_then(|s| s.to_str()).unwrap_or("");
    let Some(family) = TWIN.iter().find(|f| f.contains(&stem)) else {
        return Vec::new();
    };
    let mut changed = Vec::new();
    for name in family.iter() {
        let path = nvram_dir().join(format!("{name}.nv"));
        let Ok(mut bytes) = std::fs::read(&path) else {
            continue;
        };
        if set_single(&mut bytes) && crate::store::save(&path, &bytes).is_ok() {
            changed.push(name.to_string());
        }
    }
    changed
}

/// Where the 93C46 EEPROM's 64 words start in Supermodel's NVRAM file: a
/// chain of blocks, each a length, a name and a comment ahead of its data.
fn eeprom_offset(file: &[u8]) -> Option<usize> {
    let mut pos = 0;
    while pos + 12 <= file.len() {
        let dword = |i: usize| u32::from_le_bytes(file[i..i + 4].try_into().unwrap()) as usize;
        let (len, name_len, comment_len) = (dword(pos), dword(pos + 4), dword(pos + 8));
        let name = file.get(pos + 12..pos + 12 + name_len.saturating_sub(1))?;
        let data = pos + 12 + name_len + comment_len;
        if name == b"93C46" {
            return (data + 128 <= file.len()).then_some(data);
        }
        if len == 0 {
            return None;
        }
        pos += len;
    }
    None
}

/// LINK ID = SINGLE in a twin set's EEPROM, the way its test menu sets it.
/// The settings are 29 words behind a "M3SEGA" header and a checksum, kept
/// twice over. Their first word is the link, 1 for the factory's master and
/// 2 for single; the tenth is a flag the menu clears along with it. Watched
/// on Scud Race and Daytona USA 2, where the menu changes exactly those two
/// words in both copies and the checksum, a CRC-16/XMODEM over the first
/// copy. False when the file is not laid out that way, or is already set.
fn set_single(file: &mut [u8]) -> bool {
    let Some(at) = eeprom_offset(file) else {
        return false;
    };
    let mut w: Vec<u16> = file[at..at + 128]
        .chunks(2)
        .map(|b| u16::from_le_bytes([b[0], b[1]]))
        .collect();
    let Some(h) = (0..2).find(|&h| w[h..h + 3] == [0x4d33, 0x5345, 0x4741]) else {
        return false;
    };
    let (sum, block) = (h + 3, h + 6);
    let copy = block + 29;
    let mirrored = (0..29)
        .filter(|i| copy + i < 64)
        .all(|i| w[block + i] == w[copy + i]);
    if !mirrored || w[sum] != xmodem(&w[block..block + 29]) || !(1..=3).contains(&w[block]) {
        return false;
    }
    if w[block] == 2 && w[block + 9] == 0 {
        return false;
    }
    for start in [block, copy] {
        w[start] = 2;
        if start + 9 < 64 {
            w[start + 9] = 0;
        }
    }
    w[sum] = xmodem(&w[block..block + 29]);
    for (i, word) in w.iter().enumerate() {
        file[at + 2 * i..at + 2 * i + 2].copy_from_slice(&word.to_le_bytes());
    }
    true
}

/// CRC-16/XMODEM over words read high byte first, as the board reads them.
fn xmodem(words: &[u16]) -> u16 {
    let mut c: u16 = 0;
    for b in words.iter().flat_map(|w| w.to_be_bytes()) {
        c ^= (b as u16) << 8;
        for _ in 0..8 {
            c = if c & 0x8000 != 0 {
                (c << 1) ^ 0x1021
            } else {
                c << 1
            };
        }
    }
    c
}

/// The tube's frame, out of the keys the launcher writes for RetroArch: the
/// size the emulator lays its picture out for. Nothing on a desktop, where
/// the game opens in a window.
pub fn frame_from_keys(extra: &str) -> Option<(u32, u32)> {
    let value = |key: &str| {
        extra.lines().find_map(|l| {
            let (k, v) = l.split_once('=')?;
            (k.trim() == key).then(|| v.trim().trim_matches('"').parse::<u32>().ok())?
        })
    };
    Some((value("video_fullscreen_x")?, value("video_fullscreen_y")?))
}

/// The command for one game.
pub fn command(binary: &Path, rom: &Path, frame: Option<(u32, u32)>) -> Command {
    let mut cmd = Command::new(binary);
    // Its own data folder as the working directory: a `Config` folder in
    // whatever directory the launcher runs in would switch it to keeping
    // everything there instead.
    let _ = std::fs::create_dir_all(data_home());
    cmd.current_dir(data_home());
    cmd.arg(rom);
    match frame {
        // The whole frame, the 3D drawn at its size and stretched across it:
        // the frame is thousands of pixels wide and the set shows it as 4:3.
        Some((w, h)) => {
            cmd.arg("-fullscreen")
                .arg(format!("-res={w},{h}"))
                .arg("-stretch");
        }
        None => {
            cmd.arg("-window");
        }
    }
    cmd.arg("-true-hz")
        .arg(format!("-log-output={}", log_path().display()));
    if let Some(xml) = games_xml(binary) {
        cmd.arg(format!("-game-xml-file={}", xml.display()));
    }
    // On the tube the compositor is flyback, which hosts RetroArch and mpv
    // as Wayland clients; Supermodel goes the same way, and names itself so
    // it can be raised.
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        cmd.env("SDL_VIDEODRIVER", "wayland");
    }
    // The pad is read whether or not the window has the keyboard: a display
    // process from before Supermodel was known does not give it the focus,
    // and SDL drops a joystick's events in a window without it.
    cmd.env("SDL_JOYSTICK_ALLOW_BACKGROUND_EVENTS", "1");
    // And its sound stream, so the launcher can move it with the others.
    // SDL opens the desktop's sink by name whatever PULSE_SINK says, so the
    // launcher moves the stream once it is open. A stream from SDL's
    // PipeWire backend is put back where it asked to go; one through the
    // PulseAudio backend stays where it is moved.
    cmd.env("SDL_AUDIODRIVER", "pulseaudio");
    cmd.env("SDL_APP_NAME", "Supermodel")
        .env("SDL_AUDIO_DEVICE_APP_NAME", "Supermodel");
    cmd.env("SDL_VIDEO_WAYLAND_WMCLASS", APP_ID)
        .env("SDL_VIDEO_X11_WMCLASS", APP_ID)
        .env("SDL_APP_ID", APP_ID);
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scud Race's EEPROM as the factory leaves it, and as its test menu
    /// leaves it after LINK ID = SINGLE, read off Supermodel's NVRAM files.
    const SCUD_MASTER: [u16; 64] = [
        0x4d33, 0x5345, 0x4741, 0x593b, 0xa218, 0x0104, 0x0001, 0x0001, 0xffff, 0xff01, 0x0101,
        0x0001, 0x0200, 0x0000, 0x0000, 0x0100, 0x0101, 0x0100, 0x8080, 0x0300, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0001, 0x0001, 0xffff, 0xff01, 0x0101, 0x0001, 0x0200, 0x0000, 0x0000,
        0x0100, 0x0101, 0x0100, 0x8080, 0x0300, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ];
    const SCUD_SINGLE: [u16; 64] = [
        0x4d33, 0x5345, 0x4741, 0x0791, 0xa218, 0x0104, 0x0002, 0x0001, 0xffff, 0xff01, 0x0101,
        0x0001, 0x0200, 0x0000, 0x0000, 0x0000, 0x0101, 0x0100, 0x8080, 0x0300, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0002, 0x0001, 0xffff, 0xff01, 0x0101, 0x0001, 0x0200, 0x0000, 0x0000,
        0x0000, 0x0101, 0x0100, 0x8080, 0x0300, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
        0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000, 0x0000,
    ];

    /// An NVRAM file the way Supermodel writes one: a header block, the
    /// EEPROM's block, then the backup RAM's.
    fn nvram(words: &[u16; 64]) -> Vec<u8> {
        let block = |name: &str, comment: &str, data: &[u8]| {
            let mut b = Vec::new();
            let len = 12 + name.len() + 1 + comment.len() + 1 + data.len();
            for v in [len, name.len() + 1, comment.len() + 1] {
                b.extend_from_slice(&(v as u32).to_le_bytes());
            }
            for s in [name, comment] {
                b.extend_from_slice(s.as_bytes());
                b.push(0);
            }
            b.extend_from_slice(data);
            b
        };
        let eeprom: Vec<u8> = words
            .iter()
            .flat_map(|w| w.to_le_bytes())
            .chain([0; 38])
            .collect();
        let mut f = block(
            "Supermodel NVRAM State",
            "Supermodel Version 0.3a",
            b"\x05\0\0\0scudau\0",
        );
        f.extend(block("93C46", "Src/Model3/93C46.cpp", &eeprom));
        f.extend(block("Backup RAM", "Src/Model3/Model3.cpp", &[0; 64]));
        f
    }

    #[test]
    fn a_twin_set_is_set_to_one_cabinet_as_its_test_menu_does() {
        assert_eq!(xmodem(&SCUD_MASTER[6..35]), SCUD_MASTER[3]);
        let mut file = nvram(&SCUD_MASTER);
        assert!(set_single(&mut file));
        assert_eq!(file, nvram(&SCUD_SINGLE));
        // Once set, it is left alone.
        assert!(!set_single(&mut file));
    }

    #[test]
    fn an_eeprom_laid_out_otherwise_is_not_touched() {
        let mut other = SCUD_MASTER;
        other[0] = 0x1234;
        let mut file = nvram(&other);
        assert!(!set_single(&mut file));
        // A checksum that does not add up means the layout is not the one
        // this was watched on.
        let mut bad = SCUD_MASTER;
        bad[3] ^= 1;
        assert!(!set_single(&mut nvram(&bad)));
        assert!(!set_single(&mut b"not an nvram file".to_vec()));
    }

    #[test]
    fn the_frame_is_read_from_the_launch_keys() {
        let keys = "aspect_ratio_index = \"24\"\nvideo_fullscreen_x = \"3520\"\nvideo_fullscreen_y = \"240\"\n";
        assert_eq!(frame_from_keys(keys), Some((3520, 240)));
        assert_eq!(frame_from_keys("video_fullscreen_x = \"3520\"\n"), None);
        assert_eq!(frame_from_keys(""), None);
    }

    #[test]
    fn on_the_tube_the_game_fills_the_frame_at_its_size() {
        let cmd = command(
            Path::new("/usr/bin/supermodel-binary"),
            Path::new("/roms/srally2.zip"),
            Some((3520, 240)),
        );
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args[0], "/roms/srally2.zip");
        for want in ["-fullscreen", "-res=3520,240", "-stretch", "-true-hz"] {
            assert!(args.iter().any(|a| a == want), "{want} in {args:?}");
        }
        assert!(
            args.iter()
                .any(|a| a.starts_with("-log-output=") && a.ends_with("supermodel.log"))
        );
    }

    #[test]
    fn only_supermodels_own_defaults_are_replaced() {
        let stock = ";;\n;; Supermodel Configuration File\n;; Default settings.\n;;\n";
        assert!(is_stock(stock));
        let saved = ";;\n;; Supermodel Configuration File\n;; Updated from input configuration.\n";
        assert!(!is_stock(saved));
        assert!(!is_stock(INI));
    }

    #[test]
    fn on_a_desktop_it_opens_in_a_window() {
        let cmd = command(Path::new("supermodel"), Path::new("/roms/scud.zip"), None);
        let args: Vec<String> = cmd
            .get_args()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert!(args.iter().any(|a| a == "-window"));
        assert!(!args.iter().any(|a| a == "-fullscreen"));
    }
}
