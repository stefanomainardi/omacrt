//! Fitting modern video to a CRT television: standard (480i or 576i), frame
//! rate handling for film, aspect ratio, overscan, color, and the optional
//! 240p downscale for retro gameplay captures. Two paths share the same
//! decisions: mpv options applied live, and an ffmpeg conversion that writes a
//! CRT ready file next to the original with field based scaling.

use crate::settings::VideoFit;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// What ffprobe tells us about a source.
#[derive(Clone, Debug, Default)]
pub struct Probe {
    pub width: u32,
    pub height: u32,
    pub fps: f64,
    pub duration: f64,
    pub interlaced: bool,
    pub hdr: bool,
}

pub fn probe(file: &Path) -> Probe {
    let out = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,r_frame_rate,field_order,color_transfer:format=duration",
            "-of",
            "json",
        ])
        .arg(file)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output();
    let mut p = Probe::default();
    let Ok(out) = out else {
        return p;
    };
    let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) else {
        return p;
    };
    if let Some(s) = v.get("streams").and_then(|s| s.get(0)) {
        p.width = s.get("width").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        p.height = s.get("height").and_then(|x| x.as_u64()).unwrap_or(0) as u32;
        if let Some(r) = s.get("r_frame_rate").and_then(|x| x.as_str()) {
            if let Some((n, d)) = r.split_once('/') {
                let (n, d): (f64, f64) = (n.parse().unwrap_or(0.0), d.parse().unwrap_or(1.0));
                if d > 0.0 {
                    p.fps = n / d;
                }
            }
        }
        p.interlaced = matches!(
            s.get("field_order").and_then(|x| x.as_str()),
            Some("tt" | "bb" | "tb" | "bt")
        );
        p.hdr = matches!(
            s.get("color_transfer").and_then(|x| x.as_str()),
            Some("smpte2084" | "arib-std-b67")
        );
    }
    p.duration = v
        .get("format")
        .and_then(|f| f.get("duration"))
        .and_then(|d| d.as_str())
        .and_then(|d| d.parse().ok())
        .unwrap_or(0.0);
    p
}

/// The target the settings and the source resolve to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standard {
    Ntsc,
    Pal,
}

#[derive(Clone, Debug)]
pub struct Plan {
    pub standard: Standard,
    /// Output frame size before interlacing (fields are half the height).
    pub width: u32,
    pub height: u32,
    /// Film at 24 fps sped up by 25/24 (PAL style) instead of 3:2 pulldown.
    pub speedup: bool,
    /// Film at 24 fps telecined 3:2 to 59.94 fields.
    pub pulldown: bool,
    /// Source runs at field rate (50 or 60): every frame becomes one field.
    pub field_rate: bool,
    /// Retro capture: 320x240 progressive, no interlacing.
    pub retro: bool,
    pub aspect: String,
    pub overscan: bool,
    pub hdr: bool,
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.3
}

pub fn plan(p: &Probe, fit: &VideoFit) -> Plan {
    let fps = p.fps;
    let film = near(fps, 24.0) || near(fps, 23.976);
    let pal_source = near(fps, 25.0) || near(fps, 50.0);
    let standard = match fit.standard.as_str() {
        "ntsc" => Standard::Ntsc,
        "pal" => Standard::Pal,
        _ => {
            if pal_source || (film && fit.film24 == "speedup") {
                Standard::Pal
            } else {
                Standard::Ntsc
            }
        }
    };
    let four_three = p.height > 0 && ((p.width as f64 / p.height as f64) - 4.0 / 3.0).abs() < 0.08;
    let retro = fit.retro_240p && four_three && !p.interlaced;
    let (width, height) = if retro {
        (320, 240)
    } else {
        match standard {
            Standard::Ntsc => (720, 480),
            Standard::Pal => (720, 576),
        }
    };
    Plan {
        standard,
        width,
        height,
        speedup: film && standard == Standard::Pal,
        pulldown: film && standard == Standard::Ntsc,
        field_rate: near(fps, 50.0) || near(fps, 59.94) || near(fps, 60.0),
        retro,
        aspect: fit.aspect.clone(),
        overscan: fit.overscan,
        hdr: p.hdr,
    }
}

impl Plan {
    pub fn label(&self) -> String {
        if self.retro {
            return "240p".into();
        }
        let mut s = match self.standard {
            Standard::Ntsc => "480i".to_string(),
            Standard::Pal => "576i".to_string(),
        };
        if self.speedup {
            s.push_str(" +4%");
        } else if self.pulldown {
            s.push_str(" 3:2");
        }
        s
    }

    /// Fit the picture into the target frame: letterbox, crop or squeeze,
    /// then the optional 5% overscan margin. Output is `w`x`h`.
    fn fit_chain(&self, w: u32, h: u32) -> String {
        let (iw, ih) = if self.overscan {
            (
                (w as f64 * 0.95) as u32 / 2 * 2,
                (h as f64 * 0.95) as u32 / 2 * 2,
            )
        } else {
            (w, h)
        };
        let core = match self.aspect.as_str() {
            "crop" => format!(
                "scale={iw}:{ih}:force_original_aspect_ratio=increase:flags=lanczos,crop={iw}:{ih}"
            ),
            "anamorphic" => format!("scale={iw}:{ih}:flags=lanczos"),
            _ => format!(
                "scale={iw}:{ih}:force_original_aspect_ratio=decrease:flags=lanczos,pad={iw}:{ih}:(ow-iw)/2:(oh-ih)/2"
            ),
        };
        if self.overscan {
            format!("{core},pad={w}:{h}:(ow-iw)/2:(oh-ih)/2")
        } else {
            core
        }
    }

    /// Extra mpv arguments for live playback under this plan.
    pub fn mpv_args(&self) -> Vec<String> {
        let mut a = Vec::new();
        let (w, h) = (self.width, self.height);
        let chain = if self.retro {
            format!("scale={w}:{h}:flags=area")
        } else {
            self.fit_chain(w, h)
        };
        a.push(format!("--vf=lavfi=[{chain}]"));
        a.push("--video-unscaled=no".into());
        a.push(format!("--video-aspect-override={}:{}", w, h));
        if self.speedup {
            a.push("--speed=1.04271".into());
            a.push("--audio-pitch-correction=yes".into());
        }
        // Color: SD primaries, tube gamma, HDR tone mapped.
        a.push(
            match self.standard {
                Standard::Ntsc => "--target-prim=bt.601-525",
                Standard::Pal => "--target-prim=bt.601-625",
            }
            .into(),
        );
        a.push("--target-trc=gamma2.4".into());
        a.push("--tone-mapping=bt.2390".into());
        // Audio and subtitles for a television.
        a.push("--audio-channels=stereo".into());
        a.push("--sub-font-size=44".into());
        a.push("--sub-use-margins=yes".into());
        a.push("--sub-margin-y=48".into());
        a
    }

    /// ffmpeg command writing a CRT ready file: field based scaling for
    /// field rate sources, 3:2 telecine or PAL speed-up for film, proper
    /// interlaced flags, SD color, stereo loudness normalized audio.
    pub fn ffmpeg(&self, src: &Path, dst: &Path, progress: &Path) -> Command {
        let (w, h) = (self.width, self.height);
        let mut vf: Vec<String> = Vec::new();
        if self.hdr {
            vf.push("zscale=t=linear:npl=100,format=gbrpf32le,zscale=p=bt709,tonemap=hable:desat=0,zscale=t=bt709:m=bt709:r=tv,format=yuv420p".into());
        }
        let out_rate = match self.standard {
            Standard::Ntsc => "30000/1001",
            Standard::Pal => "25",
        };
        let mut af: Vec<String> = Vec::new();
        if self.retro {
            vf.push(format!("scale={w}:{h}:flags=area"));
        } else if self.field_rate {
            // One source frame per field: scale to field height, weave two per frame.
            let field_rate = match self.standard {
                Standard::Ntsc => "60000/1001",
                Standard::Pal => "50",
            };
            vf.push(format!("fps={field_rate}"));
            vf.push(self.fit_chain(w, h / 2));
            vf.push("tinterlace=merge,setfield=tff".into());
        } else if self.pulldown {
            vf.push("fps=24000/1001".into());
            vf.push(self.fit_chain(w, h));
            vf.push("telecine=pattern=32,setfield=tff".into());
        } else if self.speedup {
            vf.push("setpts=PTS/1.04271,fps=25".into());
            vf.push(self.fit_chain(w, h));
            vf.push("setfield=tff".into());
            af.push("atempo=1.04271".into());
        } else {
            vf.push(format!("fps={out_rate}"));
            vf.push(self.fit_chain(w, h));
            vf.push("setfield=tff".into());
        }
        let matrix = match self.standard {
            Standard::Ntsc => "smpte170m",
            Standard::Pal => "bt470bg",
        };
        vf.push(format!(
            "scale=in_range=tv:out_range=tv:out_color_matrix={matrix},format=yuv420p"
        ));
        af.push("loudnorm=I=-16:TP=-1.5:LRA=11".into());
        let mut cmd = Command::new("ffmpeg");
        cmd.arg("-y")
            .arg("-nostats")
            .arg("-loglevel")
            .arg("error")
            .arg("-progress")
            .arg(progress)
            .arg("-i")
            .arg(src)
            .arg("-vf")
            .arg(vf.join(","))
            .arg("-af")
            .arg(af.join(","))
            .arg("-c:v")
            .arg("libx264")
            .arg("-preset")
            .arg("medium")
            .arg("-crf")
            .arg("18")
            .arg("-pix_fmt")
            .arg("yuv420p")
            .arg("-colorspace")
            .arg(matrix)
            .arg("-color_primaries")
            .arg(matrix)
            .arg("-color_trc")
            .arg("bt709")
            .arg("-aspect")
            .arg("4:3");
        if !self.retro {
            cmd.arg("-flags")
                .arg("+ilme+ildct")
                .arg("-x264-params")
                .arg("tff=1");
        }
        cmd.arg("-c:a")
            .arg("aac")
            .arg("-b:a")
            .arg("192k")
            .arg("-ac")
            .arg("2")
            .arg("-movflags")
            .arg("+faststart")
            .arg(dst)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        cmd
    }
}

/// Name of the CRT ready sibling of a source file.
pub fn crt_path(src: &Path) -> PathBuf {
    let stem = src.file_stem().and_then(|s| s.to_str()).unwrap_or("video");
    src.with_file_name(format!("{stem}.crt.mp4"))
}

pub fn is_crt_file(p: &Path) -> bool {
    p.file_name()
        .and_then(|n| n.to_str())
        .map(|n| n.ends_with(".crt.mp4"))
        .unwrap_or(false)
}

/// A running conversion.
pub struct Conversion {
    pub child: Child,
    pub src: PathBuf,
    pub dst: PathBuf,
    pub title: String,
    pub duration: f64,
    progress_file: PathBuf,
    pub done_secs: f64,
}

impl Conversion {
    pub fn start(
        src: &Path,
        title: &str,
        fit: &VideoFit,
        config_dir: &Path,
    ) -> std::io::Result<Self> {
        let p = probe(src);
        let plan = plan(&p, fit);
        let dst = crt_path(src);
        let progress_file = config_dir.join("convert.progress");
        let _ = std::fs::remove_file(&progress_file);
        let child = plan.ffmpeg(src, &dst, &progress_file).spawn()?;
        Ok(Self {
            child,
            src: src.to_path_buf(),
            dst,
            title: title.to_string(),
            duration: p.duration,
            progress_file,
            done_secs: 0.0,
        })
    }

    /// Returns Some(success) when finished.
    pub fn poll(&mut self) -> Option<bool> {
        if let Ok(text) = std::fs::read_to_string(&self.progress_file) {
            if let Some(us) = text.lines().rev().find_map(|l| {
                l.strip_prefix("out_time_us=")
                    .or(l.strip_prefix("out_time_ms="))
            }) {
                if let Ok(v) = us.trim().parse::<f64>() {
                    self.done_secs = v / 1_000_000.0;
                }
            }
        }
        match self.child.try_wait() {
            Ok(Some(status)) => {
                let _ = std::fs::remove_file(&self.progress_file);
                if !status.success() {
                    let _ = std::fs::remove_file(&self.dst);
                }
                Some(status.success())
            }
            Ok(None) => None,
            Err(_) => Some(false),
        }
    }

    pub fn percent(&self) -> u32 {
        if self.duration <= 0.0 {
            return 0;
        }
        ((self.done_secs / self.duration) * 100.0).clamp(0.0, 100.0) as u32
    }
}
