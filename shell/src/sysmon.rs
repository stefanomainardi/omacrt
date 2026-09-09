//! What the machine is doing, read straight out of the kernel.
//!
//! Everything here comes from `/proc` and `/sys`: no daemon, no library, no
//! subprocess. A sample is cheap enough to take twice a second, and the rates
//! (processor time, network and disk throughput) are differences between two
//! samples, so the first one after start reports nothing moving.
//!
//! The drawing lives in the scene. This module only counts.

use std::collections::HashMap;
use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::Instant;

/// How many samples of history the graphs keep. At two samples a second this
/// is a minute and a half, which is about as much as 320 pixels can show.
pub const HISTORY: usize = 180;

/// One process in the list, as of the last sample.
#[derive(Clone, Debug, Default)]
pub struct Proc {
    pub pid: u32,
    pub name: String,
    /// Share of one processor, as a percentage: 250.0 is two and a half cores.
    pub cpu: f32,
    /// Resident memory in kibibytes.
    pub rss_kb: u64,
}

/// A reading of the whole machine.
#[derive(Clone, Debug, Default)]
pub struct Sample {
    /// Busy fraction per logical processor, 0 to 1.
    pub cores: Vec<f32>,
    /// Busy fraction over all of them, 0 to 1.
    pub cpu: f32,
    pub mem_used_kb: u64,
    pub mem_total_kb: u64,
    pub swap_used_kb: u64,
    pub swap_total_kb: u64,
    /// The one, five and fifteen minute load averages.
    pub load: [f32; 3],
    pub uptime_secs: u64,
    pub procs_running: u32,
    /// Processes the kernel has forked since boot.
    pub procs_total: u32,
    /// Processes alive at the last walk of `/proc`.
    pub procs_alive: u32,
    /// Degrees celsius, when the machine says.
    pub cpu_temp: Option<f32>,
    pub gpu_temp: Option<f32>,
    pub nvme_temp: Option<f32>,
    /// Fan speed in revolutions per minute, when there is a tachometer.
    pub fan_rpm: Option<u32>,
    /// Graphics: busy percentage and video memory, from amdgpu's own files.
    pub gpu_busy: Option<f32>,
    pub vram_used_kb: Option<u64>,
    pub vram_total_kb: Option<u64>,
    /// Bytes a second over every interface that is not the loopback.
    pub net_down: f64,
    pub net_up: f64,
    /// Bytes a second to and from every whole disk.
    pub disk_read: f64,
    pub disk_write: f64,
    /// The busiest processes, most processor time first.
    pub top: Vec<Proc>,
}

/// Counters from the previous sample, so a rate can be worked out.
#[derive(Default)]
struct Previous {
    cpu: Vec<(u64, u64)>,
    net: (u64, u64),
    disk: (u64, u64),
    at: Option<Instant>,
    /// The process list is walked less often than the counters, so its rates
    /// are worked out over its own interval.
    procs_at: Option<Instant>,
}

/// Takes samples and keeps the history the graphs draw.
pub struct Monitor {
    previous: Previous,
    pub last: Sample,
    /// Processor, memory, network down and up, as fractions of a full bar.
    pub cpu_history: Vec<f32>,
    pub mem_history: Vec<f32>,
    pub net_history: Vec<(f32, f32)>,
    /// The largest rate seen lately, so the network graph has a scale.
    pub net_peak: f64,
    /// Which card the graphics numbers come from, chosen once.
    gpu_dir: Option<String>,
    hwmon: Hwmon,
    /// Samples taken, so the process walk can happen every other one.
    tick: u64,
    /// The process walk, on a thread of its own. Opening two files for every
    /// process on the machine takes long enough to be seen as a dropped frame
    /// when it happens where the picture is drawn.
    walker: Option<Walker>,
    /// The last list the walker sent, held so a sample taken while it is
    /// still working has something to show.
    top: Vec<Proc>,
    procs_alive: u32,
}

/// The thread that walks `/proc`, and the two ends of the conversation with
/// it. It is asked for a list and answers when it has one; nothing waits.
struct Walker {
    ask: Sender<f64>,
    answer: Receiver<(Vec<Proc>, u32)>,
    /// True between the asking and the answer, so it is asked once.
    working: bool,
}

impl Walker {
    fn spawn() -> Option<Self> {
        let (ask, ask_rx) = channel::<f64>();
        let (answer_tx, answer) = channel::<(Vec<Proc>, u32)>();
        std::thread::Builder::new()
            .name("sysmon-procs".into())
            .spawn(move || {
                let mut previous: HashMap<u32, u64> = HashMap::new();
                while let Ok(elapsed) = ask_rx.recv() {
                    let procs = walk_processes(&mut previous, elapsed);
                    let alive = previous.len() as u32;
                    if answer_tx.send((procs, alive)).is_err() {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            ask,
            answer,
            working: false,
        })
    }
}

impl Default for Monitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Monitor {
    pub fn new() -> Self {
        Self {
            previous: Previous::default(),
            last: Sample::default(),
            cpu_history: vec![0.0; HISTORY],
            mem_history: vec![0.0; HISTORY],
            net_history: vec![(0.0, 0.0); HISTORY],
            net_peak: 1024.0 * 128.0,
            gpu_dir: pick_gpu(),
            hwmon: Hwmon::probe(),
            tick: 0,
            walker: Walker::spawn(),
            top: Vec::new(),
            procs_alive: 0,
        }
    }

    /// Read everything once. Call it a couple of times a second.
    pub fn sample(&mut self) {
        let now = Instant::now();
        let elapsed = self
            .previous
            .at
            .map(|t| now.duration_since(t).as_secs_f64())
            .unwrap_or(0.0);
        self.previous.at = Some(now);

        let mut s = Sample::default();
        let stat = read("/proc/stat");
        let cpu = parse_stat(&stat);
        s.cores = busy_fractions(&self.previous.cpu, &cpu.cores);
        s.cpu = if s.cores.is_empty() {
            0.0
        } else {
            s.cores.iter().sum::<f32>() / s.cores.len() as f32
        };
        self.previous.cpu = cpu.cores;
        s.procs_running = cpu.running;
        s.procs_total = cpu.total;

        let mem = parse_meminfo(&read("/proc/meminfo"));
        s.mem_total_kb = mem.total;
        s.mem_used_kb = mem.used();
        s.swap_total_kb = mem.swap_total;
        s.swap_used_kb = mem.swap_total.saturating_sub(mem.swap_free);

        s.load = parse_loadavg(&read("/proc/loadavg"));
        s.uptime_secs = read("/proc/uptime")
            .split_whitespace()
            .next()
            .and_then(|v| v.parse::<f64>().ok())
            .unwrap_or(0.0) as u64;

        let net = parse_net(&read("/proc/net/dev"));
        if elapsed > 0.0 && self.previous.net != (0, 0) {
            s.net_down = (net.0.saturating_sub(self.previous.net.0)) as f64 / elapsed;
            s.net_up = (net.1.saturating_sub(self.previous.net.1)) as f64 / elapsed;
        }
        self.previous.net = net;

        let disk = parse_diskstats(&read("/proc/diskstats"));
        if elapsed > 0.0 && self.previous.disk != (0, 0) {
            s.disk_read = (disk.0.saturating_sub(self.previous.disk.0)) as f64 * 512.0 / elapsed;
            s.disk_write = (disk.1.saturating_sub(self.previous.disk.1)) as f64 * 512.0 / elapsed;
        }
        self.previous.disk = disk;

        s.cpu_temp = self.hwmon.cpu_temp();
        s.gpu_temp = self.hwmon.gpu_temp();
        s.nvme_temp = self.hwmon.nvme_temp();
        s.fan_rpm = self.hwmon.fan_rpm();

        if let Some(dir) = &self.gpu_dir {
            s.gpu_busy = read(&format!("{dir}/gpu_busy_percent"))
                .trim()
                .parse::<f32>()
                .ok();
            s.vram_used_kb = read(&format!("{dir}/mem_info_vram_used"))
                .trim()
                .parse::<u64>()
                .ok()
                .map(|b| b / 1024);
            s.vram_total_kb = read(&format!("{dir}/mem_info_vram_total"))
                .trim()
                .parse::<u64>()
                .ok()
                .map(|b| b / 1024);
        }

        // Walking every process means opening a couple of files per process,
        // which is not something to do where the frame is drawn. The walker
        // thread is asked every other sample and answers when it can; until
        // then the last list it sent is what the screen shows.
        self.tick += 1;
        if let Some(w) = self.walker.as_mut() {
            match w.answer.try_recv() {
                Ok((top, alive)) => {
                    self.top = top;
                    self.procs_alive = alive;
                    w.working = false;
                }
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => {
                    // The thread is gone: the list stops moving, the rest of
                    // the screen does not.
                    self.walker = None;
                }
            }
        }
        if let Some(w) = self.walker.as_mut()
            && !w.working
            && (self.tick % 2 == 1 || self.top.is_empty())
        {
            let since = self
                .previous
                .procs_at
                .map(|t| now.duration_since(t).as_secs_f64())
                .unwrap_or(0.0);
            self.previous.procs_at = Some(now);
            w.working = w.ask.send(since).is_ok();
            if !w.working {
                self.walker = None;
            }
        }
        s.top = self.top.clone();
        s.procs_alive = self.procs_alive;

        // The graphs move one pixel per sample.
        let push = |v: &mut Vec<f32>, x: f32| {
            v.remove(0);
            v.push(x);
        };
        push(&mut self.cpu_history, s.cpu);
        push(
            &mut self.mem_history,
            if s.mem_total_kb == 0 {
                0.0
            } else {
                s.mem_used_kb as f32 / s.mem_total_kb as f32
            },
        );
        // The network scale follows the traffic, and falls back slowly so a
        // burst does not flatten the graph for the rest of the evening.
        let busiest = s.net_down.max(s.net_up);
        self.net_peak = (self.net_peak * 0.98).max(busiest).max(1024.0 * 32.0);
        self.net_history.remove(0);
        self.net_history.push((
            (s.net_down / self.net_peak) as f32,
            (s.net_up / self.net_peak) as f32,
        ));

        self.last = s;
    }
}

/// The busiest processes, by processor time used since the last walk. This
/// runs on the walker thread: `previous` is its own memory of the last walk.
fn walk_processes(previous: &mut HashMap<u32, u64>, elapsed: f64) -> Vec<Proc> {
    {
        let ticks = 100.0; // CONFIG_HZ on every kernel this runs on.
        let mut seen: HashMap<u32, u64> = HashMap::new();
        let mut out: Vec<Proc> = Vec::new();
        let dir = match std::fs::read_dir("/proc") {
            Ok(d) => d,
            Err(_) => return out,
        };
        for entry in dir.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            let pid: u32 = match name.parse() {
                Ok(p) => p,
                Err(_) => continue,
            };
            let stat = match std::fs::read_to_string(format!("/proc/{pid}/stat")) {
                Ok(s) => s,
                Err(_) => continue,
            };
            let (comm, used, rss_pages) = match parse_pid_stat(&stat) {
                Some(v) => v,
                None => continue,
            };
            seen.insert(pid, used);
            let before = previous.get(&pid).copied();
            let cpu = match (before, elapsed > 0.0) {
                (Some(b), true) => (used.saturating_sub(b) as f64 / ticks / elapsed * 100.0) as f32,
                _ => 0.0,
            };
            if cpu < 0.5 && before.is_some() {
                continue;
            }
            out.push(Proc {
                pid,
                name: comm,
                cpu,
                rss_kb: rss_pages * 4,
            });
        }
        *previous = seen;
        out.sort_by(|a, b| b.cpu.total_cmp(&a.cpu).then(b.rss_kb.cmp(&a.rss_kb)));
        out.truncate(8);
        out
    }
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

/// The graphics card the numbers should come from: the one with the most
/// video memory, which on a machine with an integrated and a discrete card is
/// always the discrete one.
fn pick_gpu() -> Option<String> {
    let mut best: Option<(u64, String)> = None;
    for entry in std::fs::read_dir("/sys/class/drm").ok()?.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with("card") || name.contains('-') {
            continue;
        }
        let dir = format!("/sys/class/drm/{name}/device");
        let total: u64 = match read(&format!("{dir}/mem_info_vram_total")).trim().parse() {
            Ok(v) => v,
            Err(_) => continue,
        };
        if best.as_ref().map(|(b, _)| total > *b).unwrap_or(true) {
            best = Some((total, dir));
        }
    }
    best.map(|(_, d)| d)
}

// ------------------------------------------------------------------ hwmon

/// The sensor files worth reading, found once at start. Every machine names
/// them differently, so the search is by the chip's own name.
#[derive(Default)]
struct Hwmon {
    cpu: Option<String>,
    gpu: Option<String>,
    nvme: Option<String>,
    fan: Option<String>,
}

impl Hwmon {
    fn probe() -> Self {
        let mut out = Self::default();
        let dirs = match std::fs::read_dir("/sys/class/hwmon") {
            Ok(d) => d,
            Err(_) => return out,
        };
        for entry in dirs.flatten() {
            let dir = entry.path().to_string_lossy().to_string();
            let chip = read(&format!("{dir}/name")).trim().to_string();
            match chip.as_str() {
                // Ryzen reports Tctl on temp1 and the die temperatures after it.
                "k10temp" | "coretemp" | "zenpower" => {
                    out.cpu.get_or_insert(format!("{dir}/temp1_input"));
                }
                "amdgpu" | "nouveau" => {
                    if std::path::Path::new(&format!("{dir}/temp1_input")).exists() {
                        out.gpu.get_or_insert(format!("{dir}/temp1_input"));
                    }
                    if std::path::Path::new(&format!("{dir}/fan1_input")).exists() {
                        out.fan.get_or_insert(format!("{dir}/fan1_input"));
                    }
                }
                "nvme" => {
                    out.nvme.get_or_insert(format!("{dir}/temp1_input"));
                }
                _ => {
                    // A motherboard chip is the only place a case fan shows up.
                    if out.fan.is_none()
                        && std::path::Path::new(&format!("{dir}/fan1_input")).exists()
                    {
                        out.fan = Some(format!("{dir}/fan1_input"));
                    }
                }
            }
        }
        out
    }

    fn millis(path: &Option<String>) -> Option<f32> {
        let raw: f32 = read(path.as_ref()?).trim().parse().ok()?;
        Some(raw / 1000.0)
    }

    fn cpu_temp(&self) -> Option<f32> {
        Self::millis(&self.cpu)
    }
    fn gpu_temp(&self) -> Option<f32> {
        Self::millis(&self.gpu)
    }
    fn nvme_temp(&self) -> Option<f32> {
        Self::millis(&self.nvme)
    }
    fn fan_rpm(&self) -> Option<u32> {
        read(self.fan.as_ref()?).trim().parse().ok()
    }
}

// ----------------------------------------------------------------- parsers

struct Cpu {
    /// Busy and total jiffies per logical processor.
    cores: Vec<(u64, u64)>,
    running: u32,
    total: u32,
}

/// `/proc/stat`: one `cpuN` line per logical processor, then the counters.
fn parse_stat(text: &str) -> Cpu {
    let mut cores = Vec::new();
    let mut running = 0;
    let mut total = 0;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix("cpu") {
            // The bare `cpu` line is the sum of the others; skip it.
            let (head, numbers) = match rest.split_once(' ') {
                Some(v) => v,
                None => continue,
            };
            if head.is_empty() || head.parse::<u32>().is_err() {
                continue;
            }
            let v: Vec<u64> = numbers
                .split_whitespace()
                .filter_map(|n| n.parse().ok())
                .collect();
            if v.len() < 4 {
                continue;
            }
            let idle = v[3] + v.get(4).copied().unwrap_or(0);
            let sum: u64 = v.iter().sum();
            cores.push((sum - idle, sum));
        } else if let Some(v) = line.strip_prefix("procs_running ") {
            running = v.trim().parse().unwrap_or(0);
        } else if let Some(v) = line.strip_prefix("processes ") {
            total = v.trim().parse().unwrap_or(0);
        }
    }
    Cpu {
        cores,
        running,
        total,
    }
}

/// Busy fraction per processor between two readings of `/proc/stat`.
fn busy_fractions(before: &[(u64, u64)], now: &[(u64, u64)]) -> Vec<f32> {
    now.iter()
        .enumerate()
        .map(|(i, (busy, total))| match before.get(i) {
            Some((b0, t0)) if total > t0 => {
                ((busy.saturating_sub(*b0)) as f32 / (total - t0) as f32).clamp(0.0, 1.0)
            }
            _ => 0.0,
        })
        .collect()
}

#[derive(Default)]
struct Mem {
    total: u64,
    available: u64,
    swap_total: u64,
    swap_free: u64,
}

impl Mem {
    /// What the machine would call used: everything but what it can hand out
    /// without swapping, which is the number `free -h` shows.
    fn used(&self) -> u64 {
        self.total.saturating_sub(self.available)
    }
}

fn parse_meminfo(text: &str) -> Mem {
    let mut m = Mem::default();
    for line in text.lines() {
        let (key, rest) = match line.split_once(':') {
            Some(v) => v,
            None => continue,
        };
        let value: u64 = rest
            .split_whitespace()
            .next()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        match key {
            "MemTotal" => m.total = value,
            "MemAvailable" => m.available = value,
            "SwapTotal" => m.swap_total = value,
            "SwapFree" => m.swap_free = value,
            _ => {}
        }
    }
    m
}

fn parse_loadavg(text: &str) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (i, field) in text.split_whitespace().take(3).enumerate() {
        out[i] = field.parse().unwrap_or(0.0);
    }
    out
}

/// Total bytes received and sent, over every interface but the loopback.
fn parse_net(text: &str) -> (u64, u64) {
    let mut down = 0;
    let mut up = 0;
    for line in text.lines().skip(2) {
        let (name, rest) = match line.split_once(':') {
            Some(v) => v,
            None => continue,
        };
        let name = name.trim();
        if name == "lo" || name.starts_with("veth") || name.starts_with("docker") {
            continue;
        }
        let v: Vec<u64> = rest
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();
        if v.len() < 9 {
            continue;
        }
        down += v[0];
        up += v[8];
    }
    (down, up)
}

/// Sectors read and written, counting whole disks only: the partitions of a
/// disk count the same sectors again.
fn parse_diskstats(text: &str) -> (u64, u64) {
    let mut read_sectors = 0;
    let mut written = 0;
    for line in text.lines() {
        let f: Vec<&str> = line.split_whitespace().collect();
        if f.len() < 10 {
            continue;
        }
        let name = f[2];
        let whole = match name {
            n if n.starts_with("nvme") => !n.contains('p'),
            n if n.starts_with("sd") || n.starts_with("hd") || n.starts_with("vd") => {
                !n.chars().last().is_some_and(|c| c.is_ascii_digit())
            }
            n if n.starts_with("mmcblk") => !n.contains('p'),
            _ => false,
        };
        if !whole {
            continue;
        }
        read_sectors += f[5].parse::<u64>().unwrap_or(0);
        written += f[9].parse::<u64>().unwrap_or(0);
    }
    (read_sectors, written)
}

/// `/proc/<pid>/stat`: the command, the jiffies it has used, its resident
/// pages. The command is in brackets and may itself contain a bracket, so
/// the fields after it are counted from the last one.
fn parse_pid_stat(text: &str) -> Option<(String, u64, u64)> {
    let open = text.find('(')?;
    let close = text.rfind(')')?;
    let name = text.get(open + 1..close)?.to_string();
    let rest: Vec<&str> = text.get(close + 2..)?.split_whitespace().collect();
    // After the command the fields are state, ppid, pgrp, session, tty,
    // tpgid, flags, minflt, cminflt, majflt, cmajflt, utime, stime, ...
    let utime: u64 = rest.get(11)?.parse().ok()?;
    let stime: u64 = rest.get(12)?.parse().ok()?;
    let rss: u64 = rest.get(21).and_then(|v| v.parse().ok()).unwrap_or(0);
    Some((name, utime + stime, rss))
}

/// A byte count in as few characters as a 320 pixel screen can spare.
pub fn bytes(n: f64) -> String {
    const UNITS: [&str; 5] = ["B", "K", "M", "G", "T"];
    let mut v = n.max(0.0);
    let mut unit = 0;
    while v >= 1024.0 && unit + 1 < UNITS.len() {
        v /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{v:.0}{}", UNITS[unit])
    } else if v < 10.0 {
        format!("{v:.1}{}", UNITS[unit])
    } else {
        format!("{v:.0}{}", UNITS[unit])
    }
}

/// Kibibytes as gibibytes, the way a memory bar wants them.
pub fn gib(kb: u64) -> String {
    format!("{:.1}G", kb as f64 / 1024.0 / 1024.0)
}

/// Seconds as a length of time somebody would say out loud.
pub fn uptime(secs: u64) -> String {
    let days = secs / 86_400;
    let hours = (secs % 86_400) / 3_600;
    let minutes = (secs % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STAT: &str = "cpu  100 0 100 800 0 0 0 0 0 0
cpu0 50 0 50 400 0 0 0 0 0 0
cpu1 50 0 50 400 0 0 0 0 0 0
intr 1
processes 4321
procs_running 3
procs_blocked 0
";

    #[test]
    fn the_summary_line_is_not_a_core() {
        let cpu = parse_stat(STAT);
        assert_eq!(cpu.cores.len(), 2);
        assert_eq!(cpu.cores[0], (100, 500));
        assert_eq!(cpu.running, 3);
        assert_eq!(cpu.total, 4321);
    }

    #[test]
    fn busy_is_the_difference_between_two_readings() {
        let before = [(100, 500), (100, 500)];
        // One core spent every new jiffy busy, the other spent none.
        let now = [(200, 600), (100, 600)];
        let f = busy_fractions(&before, &now);
        assert_eq!(f, vec![1.0, 0.0]);
        // With nothing to compare against, nothing is busy.
        assert_eq!(busy_fractions(&[], &now), vec![0.0, 0.0]);
    }

    #[test]
    fn memory_used_is_what_free_would_say() {
        let m = parse_meminfo(
            "MemTotal:       32000000 kB
MemFree:         1000000 kB
MemAvailable:   20000000 kB
SwapTotal:       8000000 kB
SwapFree:        7000000 kB
",
        );
        assert_eq!(m.total, 32_000_000);
        assert_eq!(m.used(), 12_000_000);
        assert_eq!(m.swap_total - m.swap_free, 1_000_000);
    }

    #[test]
    fn the_loopback_is_not_traffic() {
        let text = "Inter-|   Receive                        |  Transmit
 face |bytes packets errs drop fifo frame compressed multicast|bytes packets errs drop fifo colls carrier compressed
    lo: 999 1 0 0 0 0 0 0 999 1 0 0 0 0 0 0
  eth0: 100 1 0 0 0 0 0 0 200 1 0 0 0 0 0 0
docker0: 500 1 0 0 0 0 0 0 500 1 0 0 0 0 0 0
";
        assert_eq!(parse_net(text), (100, 200));
    }

    #[test]
    fn a_partition_is_not_a_disk() {
        let text = " 259 0 nvme0n1 10 0 100 0 20 0 200 0 0 0 0
 259 1 nvme0n1p1 5 0 50 0 10 0 100 0 0 0 0
   8 0 sda 1 0 10 0 2 0 20 0 0 0 0
   8 1 sda1 1 0 10 0 2 0 20 0 0 0 0
";
        assert_eq!(parse_diskstats(text), (110, 220));
    }

    #[test]
    fn a_command_with_a_bracket_in_its_name_still_parses() {
        let line = "42 (weird ) name) S 1 42 42 0 -1 4194304 100 0 0 0 \
             30 12 0 0 20 0 1 0 900 123456 4096 18446744073709551615";
        let (name, used, rss) = parse_pid_stat(line).unwrap();
        assert_eq!(name, "weird ) name");
        assert_eq!(used, 42);
        assert_eq!(rss, 4096);
    }

    #[test]
    fn sizes_read_the_way_a_person_says_them() {
        assert_eq!(bytes(0.0), "0B");
        assert_eq!(bytes(1536.0), "1.5K");
        assert_eq!(bytes(20.0 * 1024.0 * 1024.0), "20M");
        assert_eq!(uptime(90), "1m");
        assert_eq!(uptime(3 * 3600 + 25 * 60), "3h 25m");
        assert_eq!(uptime(2 * 86_400 + 3600), "2d 1h");
    }
}
