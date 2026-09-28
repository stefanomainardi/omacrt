//! Small voxel models, drawn as pixel art.
//!
//! A console is built in voxels from its real measurements, then drawn by
//! casting one orthographic ray per pixel through the grid: the first solid
//! cell a ray meets decides the colour, from the face it hit and a lamp above
//! and to the left. Shading comes in steps of the material's ramp with the
//! Bayer dither between them, the way the rest of the launcher shades, and
//! the picture gets a dark outline and a rim of light on its upper edges.
//!
//! Coordinates: x runs across the front, y from the front (0) to the back, z
//! up from the floor. A model turns about its vertical centre line.

use crate::art::Image;
use crate::fb::{Color, lerp_color};

#[derive(Clone, Copy, Debug)]
pub struct Mat {
    pub color: Color,
    /// Lamps and lit screens: drawn in their own colour whatever the light.
    pub glow: bool,
}

pub struct Model {
    pub w: i32,
    pub d: i32,
    pub h: i32,
    cells: Vec<u8>,
    mats: Vec<Mat>,
}

impl Model {
    pub fn new(w: i32, d: i32, h: i32) -> Self {
        Model {
            w,
            d,
            h,
            cells: vec![0; (w * d * h) as usize],
            // Index 0 is empty space.
            mats: vec![Mat {
                color: 0,
                glow: false,
            }],
        }
    }

    pub fn mat(&mut self, color: Color) -> u8 {
        self.mats.push(Mat { color, glow: false });
        (self.mats.len() - 1) as u8
    }

    pub fn glow(&mut self, color: Color) -> u8 {
        self.mats.push(Mat { color, glow: true });
        (self.mats.len() - 1) as u8
    }

    fn inside(&self, x: i32, y: i32, z: i32) -> bool {
        x >= 0 && y >= 0 && z >= 0 && x < self.w && y < self.d && z < self.h
    }

    pub fn get(&self, x: i32, y: i32, z: i32) -> u8 {
        if self.inside(x, y, z) {
            self.cells[((z * self.d + y) * self.w + x) as usize]
        } else {
            0
        }
    }

    pub fn set(&mut self, x: i32, y: i32, z: i32, m: u8) {
        if self.inside(x, y, z) {
            let i = ((z * self.d + y) * self.w + x) as usize;
            self.cells[i] = m;
        }
    }

    /// A box of material `m` (0 carves).
    #[allow(clippy::too_many_arguments)]
    pub fn cube(&mut self, x: i32, y: i32, z: i32, w: i32, d: i32, h: i32, m: u8) {
        for k in z..z + h {
            for j in y..y + d {
                for i in x..x + w {
                    self.set(i, j, k, m);
                }
            }
        }
    }

    /// A box with its vertical edges rounded to radius `r`.
    #[allow(clippy::too_many_arguments)]
    pub fn rounded(&mut self, x: i32, y: i32, z: i32, w: i32, d: i32, h: i32, r: i32, m: u8) {
        let r = r.min(w / 2).min(d / 2).max(0);
        for j in y..y + d {
            for i in x..x + w {
                let cx = if i < x + r {
                    x + r
                } else if i >= x + w - r {
                    x + w - r - 1
                } else {
                    i
                };
                let cy = if j < y + r {
                    y + r
                } else if j >= y + d - r {
                    y + d - r - 1
                } else {
                    j
                };
                let (dx, dy) = ((i - cx) as f32, (j - cy) as f32);
                if dx * dx + dy * dy <= r as f32 * r as f32 + r as f32 * 0.5 {
                    for k in z..z + h {
                        self.set(i, j, k, m);
                    }
                }
            }
        }
    }

    /// An upright elliptic cylinder centred on (cx, cy).
    #[allow(clippy::too_many_arguments)]
    pub fn cylinder(&mut self, cx: f32, cy: f32, z: i32, rx: f32, ry: f32, h: i32, m: u8) {
        let (x0, x1) = ((cx - rx).floor() as i32, (cx + rx).ceil() as i32);
        let (y0, y1) = ((cy - ry).floor() as i32, (cy + ry).ceil() as i32);
        for j in y0..=y1 {
            for i in x0..=x1 {
                let (u, v) = ((i as f32 + 0.5 - cx) / rx, (j as f32 + 0.5 - cy) / ry);
                if u * u + v * v <= 1.0 {
                    for k in z..z + h {
                        self.set(i, j, k, m);
                    }
                }
            }
        }
    }

    /// Recolour the topmost solid cell of every column in a rectangle.
    pub fn paint_top(&mut self, x: i32, y: i32, w: i32, d: i32, m: u8) {
        for j in y..y + d {
            for i in x..x + w {
                if let Some(k) = (0..self.h).rev().find(|&k| self.get(i, j, k) != 0) {
                    self.set(i, j, k, m);
                }
            }
        }
    }

    /// Recolour the frontmost solid cell of every row in a rectangle of the
    /// front face (x across, z up).
    pub fn paint_front(&mut self, x: i32, z: i32, w: i32, h: i32, m: u8) {
        for k in z..z + h {
            for i in x..x + w {
                if let Some(j) = (0..self.d).find(|&j| self.get(i, j, k) != 0) {
                    self.set(i, j, k, m);
                }
            }
        }
    }

    /// Recolour the rightmost solid cell of every row in a rectangle of the
    /// right side (y front to back, z up).
    pub fn paint_right(&mut self, y: i32, z: i32, d: i32, h: i32, m: u8) {
        for k in z..z + h {
            for j in y..y + d {
                if let Some(i) = (0..self.w).rev().find(|&i| self.get(i, j, k) != 0) {
                    self.set(i, j, k, m);
                }
            }
        }
    }
}

/// How a model is looked at.
#[derive(Clone, Copy, Debug)]
pub struct View {
    /// Turn about the vertical axis, radians. 0 shows the front square on.
    pub yaw: f32,
    /// How far above the model the eye is, radians.
    pub pitch: f32,
    /// Pixels per voxel.
    pub scale: f32,
}

type V3 = [f32; 3];

fn dot(a: V3, b: V3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn yawed(v: V3, a: f32) -> V3 {
    let (s, c) = a.sin_cos();
    [v[0] * c - v[1] * s, v[0] * s + v[1] * c, v[2]]
}

/// What a ray found: the cell's material and the axis and sign of the face.
struct Hit {
    mat: u8,
    normal: V3,
}

fn cast(m: &Model, o: V3, dir: V3) -> Option<Hit> {
    // Where the ray enters the model's box, if it does.
    let size = [m.w as f32, m.d as f32, m.h as f32];
    let (mut t0, mut t1) = (f32::NEG_INFINITY, f32::INFINITY);
    let mut enter_axis = 0;
    for a in 0..3 {
        if dir[a].abs() < 1e-6 {
            if o[a] < 0.0 || o[a] > size[a] {
                return None;
            }
            continue;
        }
        let (mut ta, mut tb) = ((0.0 - o[a]) / dir[a], (size[a] - o[a]) / dir[a]);
        if ta > tb {
            std::mem::swap(&mut ta, &mut tb);
        }
        if ta > t0 {
            t0 = ta;
            enter_axis = a;
        }
        t1 = t1.min(tb);
    }
    if t0 > t1 || t1 < 0.0 {
        return None;
    }
    let t = t0.max(0.0) + 1e-4;
    let p = [o[0] + dir[0] * t, o[1] + dir[1] * t, o[2] + dir[2] * t];
    let mut cell = [
        (p[0].floor() as i32).clamp(0, m.w - 1),
        (p[1].floor() as i32).clamp(0, m.d - 1),
        (p[2].floor() as i32).clamp(0, m.h - 1),
    ];
    let step = [
        if dir[0] > 0.0 { 1 } else { -1 },
        if dir[1] > 0.0 { 1 } else { -1 },
        if dir[2] > 0.0 { 1 } else { -1 },
    ];
    let mut tmax = [0.0f32; 3];
    let mut tdelta = [0.0f32; 3];
    for a in 0..3 {
        if dir[a].abs() < 1e-6 {
            tmax[a] = f32::INFINITY;
            tdelta[a] = f32::INFINITY;
        } else {
            let next = if step[a] > 0 {
                cell[a] as f32 + 1.0
            } else {
                cell[a] as f32
            };
            tmax[a] = (next - p[a]) / dir[a];
            tdelta[a] = 1.0 / dir[a].abs();
        }
    }
    let mut axis = enter_axis;
    loop {
        let mat = m.get(cell[0], cell[1], cell[2]);
        if mat != 0 {
            let mut normal = [0.0; 3];
            normal[axis] = -step[axis] as f32;
            return Some(Hit { mat, normal });
        }
        axis = if tmax[0] < tmax[1] {
            if tmax[0] < tmax[2] { 0 } else { 2 }
        } else if tmax[1] < tmax[2] {
            1
        } else {
            2
        };
        cell[axis] += step[axis];
        tmax[axis] += tdelta[axis];
        if cell[axis] < 0 || cell[axis] >= [m.w, m.d, m.h][axis] {
            return None;
        }
    }
}

/// Draw a model to an image with straight alpha, lit from `lamp` (a
/// direction in view space, towards the light) and rimmed in `rim`.
pub fn render(m: &Model, v: &View, rim: Color, outline: Color) -> Image {
    let centre = [m.w as f32 / 2.0, m.d as f32 / 2.0, m.h as f32 / 2.0];
    // Camera basis in world space: right, up, forward (into the scene).
    let (sp, cp) = v.pitch.sin_cos();
    let right: V3 = [1.0, 0.0, 0.0];
    let fwd: V3 = [0.0, cp, -sp];
    let up: V3 = [0.0, sp, cp];
    // The same basis in model space, turned against the model's yaw.
    let (r, u, f) = (yawed(right, -v.yaw), yawed(up, -v.yaw), yawed(fwd, -v.yaw));
    // The picture's extent: the eight corners of the box, projected.
    let (mut umin, mut umax, mut vmin, mut vmax) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for &cx in &[0.0, m.w as f32] {
        for &cy in &[0.0, m.d as f32] {
            for &cz in &[0.0, m.h as f32] {
                let p = [cx - centre[0], cy - centre[1], cz - centre[2]];
                let (a, b) = (dot(p, r), dot(p, u));
                umin = umin.min(a);
                umax = umax.max(a);
                vmin = vmin.min(b);
                vmax = vmax.max(b);
            }
        }
    }
    let iw = ((umax - umin) * v.scale).ceil() as usize + 3;
    let ih = ((vmax - vmin) * v.scale).ceil() as usize + 3;
    // The lamp, above, to the left and in front, in world space.
    let lamp = {
        let l: V3 = [-0.55, -0.45, 0.70];
        let n = dot(l, l).sqrt();
        [l[0] / n, l[1] / n, l[2] / n]
    };
    let ramps: Vec<[Color; 5]> = m
        .mats
        .iter()
        .map(|mat| crate::stage::ramp(mat.color))
        .collect();
    let mut hits: Vec<Option<(u8, V3)>> = vec![None; iw * ih];
    for py in 0..ih {
        for px in 0..iw {
            let a = umin + (px as f32 - 1.0 + 0.5) / v.scale;
            let b = vmax - (py as f32 - 1.0 + 0.5) / v.scale;
            let o = [
                centre[0] + r[0] * a + u[0] * b - f[0] * 1000.0,
                centre[1] + r[1] * a + u[1] * b - f[1] * 1000.0,
                centre[2] + r[2] * a + u[2] * b - f[2] * 1000.0,
            ];
            if let Some(h) = cast(m, o, f) {
                hits[py * iw + px] = Some((h.mat, yawed(h.normal, v.yaw)));
            }
        }
    }
    let mut px = vec![0u32; iw * ih];
    for y in 0..ih {
        for x in 0..iw {
            let Some((mat, n)) = hits[y * iw + x] else {
                continue;
            };
            let c = if m.mats[mat as usize].glow {
                m.mats[mat as usize].color
            } else {
                // One step of the ramp per face, so a flat face is one colour
                // as a face of a sprite was: the material's own colour on top,
                // one step down on a face the lamp reaches, two on one it
                // does not. The steps above are for the rim of light.
                let lit = dot(n, lamp).max(0.0);
                let step = if n[2] > 0.5 {
                    2
                } else if lit > 0.2 {
                    1
                } else {
                    0
                };
                ramps[mat as usize][step]
            };
            // An edge between two faces reads as an edge: a pixel whose
            // neighbour above hit a face turned another way takes one
            // step of shade.
            let edge = y > 0 && matches!(hits[(y - 1) * iw + x], Some((_, na)) if dot(na, n) < 0.5);
            let c = if edge && !m.mats[mat as usize].glow {
                lerp_color(c, 0x000000, 0.22)
            } else {
                c
            };
            let top = y == 0 || hits[(y - 1) * iw + x].is_none();
            let c = if top { lerp_color(c, rim, 0.55) } else { c };
            px[y * iw + x] = 0xff00_0000 | c;
        }
    }
    // The outline: empty pixels touching the model.
    let solid: Vec<bool> = px.iter().map(|&p| p >> 24 != 0).collect();
    for y in 0..ih {
        for x in 0..iw {
            if solid[y * iw + x] {
                continue;
            }
            let near = [(1i32, 0i32), (-1, 0), (0, 1), (0, -1)]
                .iter()
                .any(|&(dx, dy)| {
                    let (nx, ny) = (x as i32 + dx, y as i32 + dy);
                    nx >= 0
                        && ny >= 0
                        && (nx as usize) < iw
                        && (ny as usize) < ih
                        && solid[ny as usize * iw + nx as usize]
                });
            if near {
                px[y * iw + x] = 0xff00_0000 | outline;
            }
        }
    }
    Image { w: iw, h: ih, px }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube_model() -> Model {
        let mut m = Model::new(8, 8, 8);
        let grey = m.mat(0x808080);
        m.cube(0, 0, 0, 8, 8, 8, grey);
        m
    }

    fn solid(img: &Image) -> usize {
        img.px.iter().filter(|&&p| p >> 24 != 0).count()
    }

    #[test]
    fn a_cube_square_on_is_a_square_and_turned_is_wider() {
        let m = cube_model();
        let flat = render(
            &m,
            &View {
                yaw: 0.0,
                pitch: 0.0,
                scale: 2.0,
            },
            0xffffff,
            0,
        );
        let turned = render(
            &m,
            &View {
                yaw: std::f32::consts::FRAC_PI_4,
                pitch: 0.0,
                scale: 2.0,
            },
            0xffffff,
            0,
        );
        assert!(
            turned.w > flat.w,
            "at 45 degrees the diagonal faces the eye"
        );
        assert!(solid(&flat) >= 14 * 14);
    }

    #[test]
    fn drawing_the_same_model_twice_gives_the_same_picture() {
        let m = cube_model();
        let v = View {
            yaw: 0.6,
            pitch: 0.5,
            scale: 1.5,
        };
        assert_eq!(
            render(&m, &v, 0xffffff, 0).px,
            render(&m, &v, 0xffffff, 0).px
        );
    }

    #[test]
    fn carving_empties_cells_and_rounding_keeps_the_middle() {
        let mut m = cube_model();
        m.cube(2, 2, 2, 4, 4, 4, 0);
        assert_eq!(m.get(3, 3, 3), 0);
        assert_ne!(m.get(0, 0, 0), 0);
        let mut r = Model::new(10, 10, 1);
        let g = r.mat(0x808080);
        r.rounded(0, 0, 0, 10, 10, 1, 3, g);
        assert_eq!(r.get(0, 0, 0), 0, "a rounded corner is empty");
        assert_ne!(r.get(5, 5, 0), 0);
        assert_ne!(r.get(5, 0, 0), 0, "the middle of an edge is not rounded");
    }
}
