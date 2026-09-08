//! What the machine is doing, drawn the way a 16 bit game drew a status
//! screen: bevelled plates, segmented meters, and a bank of little bars for
//! the processors that moves like the meters on an amplifier.

use super::*;

impl Scene {
    /// Green while there is room, then yellow, orange and red.
    fn heat(&self, level: f32) -> Color {
        let t = level.clamp(0.0, 1.0);
        if t < 0.5 {
            lerp_color(self.theme.green, self.theme.yellow, t / 0.5)
        } else if t < 0.8 {
            lerp_color(self.theme.yellow, self.theme.orange, (t - 0.5) / 0.3)
        } else {
            lerp_color(self.theme.orange, self.theme.red, (t - 0.8) / 0.2)
        }
    }

    /// A plate with a lit top left edge and a shadowed bottom right one, the
    /// way every menu of the era framed a box, with its title cut into the
    /// top edge.
    fn plate(&self, fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, title: &str) {
        let face = lerp_color(self.theme.bg, self.theme.paper, 0.06);
        fb.rect(x, y, w, h, face);
        // A dither of one lit pixel in four: at 240p this reads as a texture
        // rather than as a pattern, and it stops the plate looking painted.
        let speck = lerp_color(face, self.theme.paper, 0.07);
        let mut yy = y + 1;
        while yy < y + h - 1 {
            let mut xx = x + 1 + ((yy - y) % 4);
            while xx < x + w - 1 {
                fb.put(xx, yy, speck);
                xx += 4;
            }
            yy += 2;
        }
        let light = lerp_color(face, self.theme.paper, 0.30);
        let dark = lerp_color(face, self.theme.bg, 0.85);
        fb.rect(x, y, w, 1, light);
        fb.rect(x, y, 1, h, light);
        fb.rect(x, y + h - 1, w, 1, dark);
        fb.rect(x + w - 1, y, 1, h, dark);
        if !title.is_empty() {
            let tw = Framebuffer::text_width(title, 1);
            fb.rect(x + 5, y, tw + 4, 1, face);
            fb.text(x + 7, y - 4, title, self.theme.cyan, 1);
        }
    }

    /// A horizontal meter in blocks. Unlit blocks stay visible, so the meter
    /// has a length even when nothing is happening.
    fn meter(&self, fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, level: f32, hot: bool) {
        const BLOCK: i32 = 4;
        let lit = if hot {
            self.heat(level)
        } else {
            self.theme.cyan
        };
        let off = lerp_color(self.theme.bg, self.theme.paper, 0.10);
        let blocks = (w / BLOCK).max(1);
        let on = (level.clamp(0.0, 1.0) * blocks as f32).round() as i32;
        for i in 0..blocks {
            let bx = x + i * BLOCK;
            let c = if i < on { lit } else { off };
            fb.rect(bx, y, BLOCK - 1, h, c);
            if i < on && h >= 4 {
                // The highlight along the top is what makes a bar look moulded.
                fb.rect(bx, y, BLOCK - 1, 1, lerp_color(c, self.theme.paper, 0.45));
            }
        }
    }

    /// The bank of processor meters: one narrow vertical bar per logical
    /// processor, lit from the bottom, with the last peak left behind.
    fn core_bank(&mut self, fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32) {
        let cores = self.sysmon.last.cores.clone();
        if cores.is_empty() {
            return;
        }
        if self.sysmon_bars.len() != cores.len() {
            self.sysmon_bars = cores.clone();
        }
        // A meter that jumps to every sample is unreadable; one that eases up
        // fast and falls slowly reads like a needle.
        for (i, target) in cores.iter().enumerate() {
            let cur = self.sysmon_bars[i];
            let k = if *target > cur { 0.55 } else { 0.20 };
            self.sysmon_bars[i] = cur + (target - cur) * k;
        }
        let n = cores.len() as i32;
        let step = (w / n).max(2);
        let bar = (step - 2).max(1);
        let off = lerp_color(self.theme.bg, self.theme.paper, 0.13);
        const SEG: i32 = 3;
        let segments = (h / SEG).max(1);
        for i in 0..n {
            let level = self.sysmon_bars[i as usize].clamp(0.0, 1.0);
            let on = (level * segments as f32).round() as i32;
            let bx = x + i * step;
            for s in 0..segments {
                let sy = y + h - (s + 1) * SEG;
                let c = if s < on {
                    self.heat(s as f32 / segments as f32)
                } else {
                    off
                };
                fb.rect(bx, sy, bar, SEG - 1, c);
            }
        }
    }

    /// A filled graph of one history line, newest at the right.
    fn graph(&self, fb: &mut Framebuffer, x: i32, y: i32, w: i32, h: i32, data: &[f32], c: Color) {
        let fill = lerp_color(self.theme.bg, c, 0.35);
        let from = data.len().saturating_sub(w as usize);
        for (i, v) in data[from..].iter().enumerate() {
            let px = x + i as i32;
            let level = v.clamp(0.0, 1.0);
            let top = y + h - (level * h as f32).round() as i32;
            if top < y + h {
                fb.rect(px, top, 1, y + h - top, fill);
            }
            fb.put(px, top.clamp(y, y + h - 1), c);
        }
    }

    /// The system monitor. Page 0 is the machine, page 1 the processes.
    pub(super) fn draw_monitor(&mut self, fb: &mut Framebuffer, page: usize) {
        // The counters are read twice a second, whatever the frame rate.
        if self.now - self.sysmon_at >= 0.5 {
            self.sysmon_at = self.now;
            self.sysmon.sample();
        }
        let w = fb.w as i32;
        let h = fb.h as i32;
        let left = (w as f32 * 0.05) as i32;
        let width = w - 2 * left;
        fb.clear(self.theme.bg);

        // ---------------------------------------------------- the name plate
        let s = self.sysmon.last.clone();
        let plate_h = 13;
        self.plate(fb, left, 4, width, plate_h, "");
        let right = format!(
            "UP {}  {}",
            crate::sysmon::uptime(s.uptime_secs),
            chrono::Local::now().format("%H:%M")
        );
        let right_w = Framebuffer::text_width(&right, 1);
        fb.text(w - left - 4 - right_w, 6, &right, self.theme.dim, 1);
        fb.text(left + 4, 6, "SYSTEM", self.theme.accent, 1);
        let host_x = left + 4 + 8 * 7;
        let room = ((w - left - 8 - right_w - host_x) / 8).max(0) as usize;
        let host: String = self.info.host.to_uppercase().chars().take(room).collect();
        fb.text(host_x, 6, &host, self.theme.paper, 1);

        if page == 1 {
            self.draw_monitor_processes(fb, left, width, 4 + plate_h + 8, h);
            let hint = self.hint(&[("<>", "page"), ("B", "back")]);
            fb.text(left, h - 12, &hint, scale(self.theme.dim, 0.7), 1);
            return;
        }

        // ------------------------------------------------------- processors
        let cpu_y = 4 + plate_h + 6;
        let cpu_h = 52;
        self.plate(fb, left, cpu_y, width, cpu_h, "CPU");
        let pct = format!("{:>3.0}%", s.cpu * 100.0);
        fb.text(left + 6, cpu_y + 6, &pct, self.heat(s.cpu), 2);
        let load = format!("{:.2} {:.2} {:.2}", s.load[0], s.load[1], s.load[2]);
        let load_x = left + 6 + Framebuffer::text_width(&pct, 2) + 10;
        fb.text(load_x, cpu_y + 9, &load, self.theme.dim, 1);
        let mut badge = String::new();
        if let Some(t) = s.cpu_temp {
            badge.push_str(&format!("{t:.0}C"));
        }
        // An idle fan that has stopped is not worth a line of its own.
        if let Some(rpm) = s.fan_rpm.filter(|r| *r > 0) {
            badge.push_str(&format!("  {rpm}RPM"));
        }
        if !badge.is_empty() {
            let temp_level = s.cpu_temp.map(|t| (t - 40.0) / 50.0).unwrap_or(0.0);
            fb.text(
                w - left - 6 - Framebuffer::text_width(&badge, 1),
                cpu_y + 9,
                &badge,
                self.heat(temp_level),
                1,
            );
        }
        self.core_bank(fb, left + 6, cpu_y + 24, width - 12, 24);

        // ---------------------------------------------- memory and graphics
        let half = (width - 6) / 2;
        let mem_y = cpu_y + cpu_h + 6;
        let mem_h = 36;
        self.plate(fb, left, mem_y, half, mem_h, "MEM");
        let mem_level = if s.mem_total_kb == 0 {
            0.0
        } else {
            s.mem_used_kb as f32 / s.mem_total_kb as f32
        };
        self.meter(fb, left + 6, mem_y + 7, half - 12, 7, mem_level, true);
        let mem_text = format!(
            "{} / {}",
            crate::sysmon::gib(s.mem_used_kb),
            crate::sysmon::gib(s.mem_total_kb)
        );
        fb.text(left + 6, mem_y + 18, &mem_text, self.theme.paper, 1);
        if s.swap_total_kb > 0 {
            let swap = s.swap_used_kb as f32 / s.swap_total_kb as f32;
            let bar = ((half - 12) as f32 * swap.clamp(0.0, 1.0)) as i32;
            let track = lerp_color(self.theme.bg, self.theme.paper, 0.10);
            fb.rect(left + 6, mem_y + mem_h - 6, half - 12, 2, track);
            fb.rect(left + 6, mem_y + mem_h - 6, bar, 2, self.theme.magenta);
        }

        let gpu_x = left + half + 6;
        self.plate(fb, gpu_x, mem_y, half, mem_h, "GPU");
        match s.gpu_busy {
            Some(busy) => {
                self.meter(fb, gpu_x + 6, mem_y + 7, half - 12, 7, busy / 100.0, true);
                fb.text(
                    gpu_x + 6,
                    mem_y + 18,
                    &format!("{busy:.0}%"),
                    self.theme.paper,
                    1,
                );
                if let Some(t) = s.gpu_temp {
                    let badge = format!("{t:.0}C");
                    fb.text(
                        gpu_x + half - 6 - Framebuffer::text_width(&badge, 1),
                        mem_y + 18,
                        &badge,
                        self.heat((t - 40.0) / 50.0),
                        1,
                    );
                }
                if let (Some(used), Some(total)) = (s.vram_used_kb, s.vram_total_kb) {
                    let vram = format!(
                        "{} / {}",
                        crate::sysmon::gib(used),
                        crate::sysmon::gib(total)
                    );
                    fb.text(gpu_x + 6, mem_y + 28, &vram, self.theme.dim, 1);
                }
            }
            None => fb.text(gpu_x + 6, mem_y + 12, "no counters", self.theme.dim, 1),
        }

        // --------------------------------------------------------- graphs
        let graph_y = mem_y + mem_h + 6;
        let graph_h = 40;
        self.plate(fb, left, graph_y, half, graph_h, "LOAD");
        let cpu_history = self.sysmon.cpu_history.clone();
        self.graph(
            fb,
            left + 3,
            graph_y + 4,
            half - 6,
            graph_h - 8,
            &cpu_history,
            self.theme.green,
        );
        let mem_history = self.sysmon.mem_history.clone();
        self.graph(
            fb,
            left + 3,
            graph_y + 4,
            half - 6,
            graph_h - 8,
            &mem_history,
            self.theme.magenta,
        );

        self.plate(fb, gpu_x, graph_y, half, graph_h, "NET");
        let net: Vec<f32> = self
            .sysmon
            .net_history
            .iter()
            .map(|(d, _)| *d)
            .collect::<Vec<_>>();
        let up: Vec<f32> = self.sysmon.net_history.iter().map(|(_, u)| *u).collect();
        self.graph(
            fb,
            gpu_x + 3,
            graph_y + 4,
            half - 6,
            graph_h - 8,
            &net,
            self.theme.cyan,
        );
        self.graph(
            fb,
            gpu_x + 3,
            graph_y + 4,
            half - 6,
            graph_h - 8,
            &up,
            self.theme.yellow,
        );

        // ----------------------------------------------------- the bottom line
        let io_y = graph_y + graph_h + 5;
        let io = format!(
            "NET {} {}  DISK {} {}",
            crate::sysmon::bytes(s.net_down),
            crate::sysmon::bytes(s.net_up),
            crate::sysmon::bytes(s.disk_read),
            crate::sysmon::bytes(s.disk_write),
        );
        fb.text(left, io_y, &io, self.theme.paper, 1);
        let procs = format!("{}R {}P", s.procs_running, s.procs_alive);
        fb.text(
            w - left - Framebuffer::text_width(&procs, 1),
            io_y,
            &procs,
            self.theme.dim,
            1,
        );

        // The three busiest processes, so the page answers "what is it doing"
        // without turning to the next one.
        let list_y = io_y + 14;
        fb.text(left, list_y, "BUSIEST", self.theme.cyan, 1);
        let top = s.top.clone();
        for (i, p) in top.iter().take(3).enumerate() {
            let y = list_y + 11 + i as i32 * 11;
            let cpu = format!("{:>5.1}%", p.cpu);
            let cpu_w = Framebuffer::text_width(&cpu, 1);
            let mem = crate::sysmon::bytes(p.rss_kb as f64 * 1024.0);
            let mem_w = Framebuffer::text_width(&mem, 1);
            let room = ((width - cpu_w - mem_w - 20) / 8).max(4) as usize;
            let name: String = p.name.chars().take(room).collect();
            fb.text(left, y, &name, self.theme.paper, 1);
            fb.text(
                w - left - mem_w - cpu_w - 8,
                y,
                &cpu,
                self.heat(p.cpu / 400.0),
                1,
            );
            fb.text(w - left - mem_w, y, &mem, self.theme.dim, 1);
        }

        let hint = self.hint(&[("<>", "processes"), ("B", "back")]);
        fb.text(left, h - 12, &hint, scale(self.theme.dim, 0.7), 1);
    }

    /// Page two: the processes using the machine, most demanding first.
    fn draw_monitor_processes(
        &mut self,
        fb: &mut Framebuffer,
        left: i32,
        width: i32,
        y0: i32,
        h: i32,
    ) {
        let top = self.sysmon.last.top.clone();
        let rows = ((h - y0 - 20) / 12).clamp(1, 12) as usize;
        fb.text(left + 6, y0, "PROCESS", self.theme.cyan, 1);
        let cpu_x = left + width - 6 - 8 * 11;
        fb.text(cpu_x, y0, "CPU", self.theme.cyan, 1);
        fb.text(left + width - 6 - 8 * 5, y0, "MEM", self.theme.cyan, 1);
        fb.rect(
            left,
            y0 + 10,
            width,
            1,
            lerp_color(self.theme.bg, self.theme.paper, 0.25),
        );
        for (i, p) in top.iter().take(rows).enumerate() {
            let y = y0 + 14 + i as i32 * 12;
            let pid = format!("{:>7}", p.pid);
            fb.text(left + 6, y, &pid, scale(self.theme.dim, 0.8), 1);
            let name_x = left + 6 + 8 * 8;
            let room = ((cpu_x - name_x - 4) / 8).max(4) as usize;
            let name: String = p.name.chars().take(room).collect();
            fb.text(name_x, y, &name, self.theme.paper, 1);
            let cpu = format!("{:>5.1}%", p.cpu);
            fb.text(cpu_x, y, &cpu, self.heat(p.cpu / 400.0), 1);
            let mem = crate::sysmon::bytes(p.rss_kb as f64 * 1024.0);
            fb.text(
                left + width - 6 - Framebuffer::text_width(&mem, 1),
                y,
                &mem,
                self.theme.dim,
                1,
            );
        }
        if top.is_empty() {
            fb.text(left + 6, y0 + 20, "reading...", self.theme.dim, 1);
        }
    }
}
