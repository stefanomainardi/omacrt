//! Pixel assets: the mark, and the wordmark decoded from a block drawing
//! (block characters become a 2-pixel-tall grid).

/// The wordmark, in the shape of Omarchy's own `logo.txt` (MIT, Omacom).
pub const WORDMARK_TXT: &str = include_str!("../assets/wordmark.txt");

/// The mark: four horizontal bars and the dark cut of the beam's return.
///
/// A tube draws its picture as a stack of lines and between two lines the
/// beam runs back with the gun switched off. The mark is that return, so the
/// bars stand still and the cut is what moves.
///
/// The cut steps once per bar, and the step is the bars' PITCH rather than
/// their thickness: only then is its edge a true 45 degrees. Its width is not
/// less than the pitch either, or the steps stop overlapping and the diagonal
/// reads as a dashed line rather than a cut.
///
/// One shape at every size. Four bars of eight units with four between them
/// fill the 44 unit grid exactly, and at 24 pixels, the narrowest anything
/// uses, that is still 4.4 pixels of bar and 2.2 of gap: both above what a
/// phosphor's bloom closes. Five bars cannot shrink that far, and a mark that
/// changes shape when it shrinks is a patch rather than a system.
pub struct Retrace {
    pub grid: i32,
    pub bars: i32,
    pub thick: i32,
    pub gap: i32,
    pub cut: i32,
    /// The shortest fragment a bar may be left with. A 45 degree cut near the
    /// end of a bar would otherwise leave a sliver one pixel wide, which the
    /// bloom swallows; where a step would do that, the cut runs to the edge
    /// instead and the bar simply starts later.
    pub minseg: i32,
    /// Where the cut rests. The most asymmetric of the thirty positions that
    /// pass, which is the only thing that keeps the mark from reading as a
    /// diagram.
    pub rest: i32,
}

pub const RETRACE: Retrace = Retrace {
    grid: 44,
    bars: 4,
    thick: 8,
    gap: 4,
    cut: 12,
    minseg: 3,
    rest: -2,
};

/// One solid piece of a bar, in framebuffer pixels.
pub struct Seg {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// The edge the cut has just left. On a tube it stays a shade brighter
    /// for a frame or two, which is the phosphor and not a decision.
    pub hot: bool,
}

impl Retrace {
    pub fn pitch(&self) -> i32 {
        self.thick + self.gap
    }

    /// How far the cut travels to leave the mark, come back in the other
    /// side and land where it started.
    pub fn span(&self) -> i32 {
        self.grid + self.cut + 16
    }

    pub fn low(&self) -> i32 {
        -16
    }

    /// The pieces to draw, in pixels, with the cut at `base`.
    ///
    /// Pitch, thickness, cut width and the step per bar are worked out once
    /// and then repeated. Rounding each coordinate on its own gave bars six
    /// and seven pixels tall in turn, with gaps that did not match: the mark
    /// looked out of true. The stack is also made to fit the box it is given,
    /// because rounding a pitch up can make four bars taller than the size
    /// they were asked for, and the last bar was being cut off.
    pub fn segments(&self, size: f32, base: i32) -> Vec<Seg> {
        let k = size / self.grid as f32;
        let at = |u: i32| (u as f32 * k).round() as i32;
        let side = size.round() as i32;
        let mut pitch = at(self.pitch()).max(2);
        let mut thick = at(self.thick).clamp(1, pitch - 1);
        while (self.bars - 1) * pitch + thick > side && pitch > 2 {
            pitch -= 1;
            thick = thick.min(pitch - 1);
        }
        let width = at(self.grid);
        let cut = at(self.cut).max(1);
        let minseg = at(self.minseg).max(2);
        let stack = (self.bars - 1) * pitch + thick;
        let top = ((side - stack) / 2).max(0);
        let base_px = (base as f32 * k).round() as i32;
        let mut out = Vec::new();
        for i in 0..self.bars {
            let y = top + i * pitch;
            let mut a = base_px + (self.bars - 1 - i) * pitch;
            let mut b = a + cut - 1;
            if a < minseg {
                a = 0;
            }
            if b > width - 1 - minseg {
                b = width - 1;
            }
            if b < 0 || a > width - 1 {
                out.push(Seg { x: 0, y, w: width, h: thick, hot: false });
                continue;
            }
            if a > 0 {
                out.push(Seg { x: 0, y, w: a, h: thick, hot: false });
            }
            if b < width - 1 {
                out.push(Seg { x: b + 1, y, w: width - 1 - b, h: thick, hot: true });
            }
        }
        out
    }
}
