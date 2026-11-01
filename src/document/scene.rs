//! 3D layers: a scene (an object, how it's turned and lit, how it's drawn) rendered into cells.
//!
//! Rays go straight into the canvas (an orthographic view), one or more per cell. Smooth
//! objects are distance fields, marched along the ray; extruded text and art are voxel columns,
//! walked cell by cell. A cell is twice as tall as wide everywhere, so nothing comes out squashed.

use serde::{Deserialize, Serialize};

use super::{Cell, Color};
use crate::tools::pixel::from_halves;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Object3D {
    Cube,
    Sphere,
    Torus,
    Cylinder,
    Cone,
    /// Letters of a FIGlet font (an index into `figlet::fonts()`), `depth` cells deep.
    Text { text: String, font: usize, depth: f32 },
    /// Cells (a selection's) pushed out into blocks of their own colours, `depth` cells deep.
    Art { cells: Vec<Vec<Cell>>, depth: f32 },
}

impl Object3D {
    pub fn name(&self) -> &'static str {
        match self {
            Object3D::Cube => "Cube",
            Object3D::Sphere => "Sphere",
            Object3D::Torus => "Torus",
            Object3D::Cylinder => "Cylinder",
            Object3D::Cone => "Cone",
            Object3D::Text { .. } => "Text",
            Object3D::Art { .. } => "Art",
        }
    }
}

/// How the lit object becomes cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Style3D {
    /// Glyphs of a density ramp, as in the famous spinning ASCII donut.
    Shade,
    /// Braille dots (2×4 per cell), dithered.
    Braille,
    /// Coloured half blocks (1×2 per cell).
    Blocks,
}

impl Style3D {
    pub const ALL: [Style3D; 3] = [Style3D::Shade, Style3D::Braille, Style3D::Blocks];

    pub fn name(self) -> &'static str {
        match self {
            Style3D::Shade => "Shaded ASCII",
            Style3D::Braille => "Braille",
            Style3D::Blocks => "Colour Blocks",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Scene3D {
    pub object: Object3D,
    pub style: Style3D,
    /// Density ramp for [`Style3D::Shade`], from empty to full.
    pub ramp: String,
    pub color: Color,
    /// Degrees around the x, y and z axes, in that order.
    pub turn: [f32; 3],
    /// Degrees more per animation frame (a turntable spins around y).
    pub spin: [f32; 3],
    /// Size: 1 fills the canvas's width or height.
    pub size: f32,
    /// Where the centre is, as a share of the canvas from its middle.
    pub position: [f32; 2],
    /// Where the light comes from: degrees to the side (negative: left) and up.
    pub light: [f32; 2],
    /// Light that reaches every side, 0..1.
    pub ambient: f32,
    /// A highlight where the light reflects towards the viewer.
    pub shine: bool,
}

impl Default for Scene3D {
    fn default() -> Self {
        Self {
            object: Object3D::Cube,
            style: Style3D::Shade,
            ramp: " .:-=+*#%@".into(),
            color: Color::new(167, 139, 250, 255),
            turn: [25.0, -35.0, 0.0],
            spin: [0.0; 3],
            size: 0.7,
            position: [0.0, 0.0],
            light: [-40.0, 35.0],
            ambient: 0.15,
            shine: true,
        }
    }
}

type Vec3 = [f32; 3];
type Mat3 = [[f32; 3]; 3];

fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn add(a: Vec3, b: Vec3) -> Vec3 {
    [a[0] + b[0], a[1] + b[1], a[2] + b[2]]
}

fn scale(a: Vec3, s: f32) -> Vec3 {
    [a[0] * s, a[1] * s, a[2] * s]
}

fn length(a: Vec3) -> f32 {
    dot(a, a).sqrt()
}

fn normalize(a: Vec3) -> Vec3 {
    let l = length(a);
    if l > 0.0 { scale(a, 1.0 / l) } else { a }
}

fn mul(m: &Mat3, v: Vec3) -> Vec3 {
    [dot(m[0], v), dot(m[1], v), dot(m[2], v)]
}

fn transpose(m: &Mat3) -> Mat3 {
    std::array::from_fn(|r| std::array::from_fn(|c| m[c][r]))
}

fn matmul(a: &Mat3, b: &Mat3) -> Mat3 {
    std::array::from_fn(|r| std::array::from_fn(|c| (0..3).map(|k| a[r][k] * b[k][c]).sum()))
}

/// Turns around x, then y, then z (degrees).
fn rotation(turn: Vec3) -> Mat3 {
    let [x, y, z] = turn.map(f32::to_radians);
    let rx = [[1.0, 0.0, 0.0], [0.0, x.cos(), -x.sin()], [0.0, x.sin(), x.cos()]];
    let ry = [[y.cos(), 0.0, y.sin()], [0.0, 1.0, 0.0], [-y.sin(), 0.0, y.cos()]];
    let rz = [[z.cos(), -z.sin(), 0.0], [z.sin(), z.cos(), 0.0], [0.0, 0.0, 1.0]];
    matmul(&rz, &matmul(&ry, &rx))
}

/// Where a ray met the object (in object space).
struct Hit {
    normal: Vec3,
    /// The object's own colour there, if it has one (art).
    color: Option<Color>,
}

/// The object, ready to be hit by rays.
enum Shape {
    Field(fn(Vec3) -> f32),
    Voxels(Voxels),
}

/// Columns of blocks: `solid[row][column]` with its colour, centred on the origin.
struct Voxels {
    solid: Vec<Vec<Option<Option<Color>>>>,
    /// Size of one voxel in object space: across, down, deep.
    cell: Vec3,
    /// Object-space corner of the grid at column 0, row 0, front.
    origin: Vec3,
}

impl Shape {
    fn new(object: &Object3D) -> Shape {
        match object {
            Object3D::Cube => Shape::Field(|p| {
                let q = p.map(|c| c.abs() - 0.56);
                length(q.map(|c| c.max(0.0))) + q[0].max(q[1]).max(q[2]).min(0.0) - 0.06
            }),
            Object3D::Sphere => Shape::Field(|p| length(p) - 0.95),
            Object3D::Torus => Shape::Field(|p| {
                let q = ((p[0] * p[0] + p[2] * p[2]).sqrt() - 0.66, p[1]);
                (q.0 * q.0 + q.1 * q.1).sqrt() - 0.3
            }),
            Object3D::Cylinder => Shape::Field(|p| {
                let d = ((p[0] * p[0] + p[2] * p[2]).sqrt() - 0.6, p[1].abs() - 0.8);
                d.0.max(d.1).min(0.0) + (d.0.max(0.0).powi(2) + d.1.max(0.0).powi(2)).sqrt()
            }),
            Object3D::Cone => Shape::Field(|p| {
                // A capped cone from radius 0.75 at the bottom to a point at the top (Inigo Quilez).
                let (h, r1, r2) = (0.8f32, 0.75f32, 0.0f32);
                let q = ((p[0] * p[0] + p[2] * p[2]).sqrt(), p[1]);
                let k1 = (r2, h);
                let k2 = (r2 - r1, 2.0 * h);
                let ca = (q.0 - q.0.min(if q.1 < 0.0 { r1 } else { r2 }), q.1.abs() - h);
                let t = (((k1.0 - q.0) * k2.0 + (k1.1 - q.1) * k2.1) / (k2.0 * k2.0 + k2.1 * k2.1)).clamp(0.0, 1.0);
                let cb = (q.0 - k1.0 + k2.0 * t, q.1 - k1.1 + k2.1 * t);
                let s = if cb.0 < 0.0 && ca.1 < 0.0 { -1.0 } else { 1.0 };
                s * (ca.0 * ca.0 + ca.1 * ca.1).min(cb.0 * cb.0 + cb.1 * cb.1).sqrt()
            }),
            Object3D::Text { text, font, depth } => {
                let rows = crate::figlet::fonts().get(*font).map(|f| f.render(text)).unwrap_or_default();
                let solid = rows.iter().map(|row| row.chars().map(|c| (c != ' ').then_some(None)).collect()).collect();
                Shape::Voxels(Voxels::new(solid, *depth))
            }
            Object3D::Art { cells, depth } => {
                let solid = cells
                    .iter()
                    .map(|row| {
                        row.iter()
                            .map(|c| (!c.is_empty()).then(|| Some(if c.bg.is_transparent() { c.fg.clone() } else { c.bg.clone() })))
                            .collect()
                    })
                    .collect();
                Shape::Voxels(Voxels::new(solid, *depth))
            }
        }
    }

    fn hit(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        match self {
            Shape::Field(field) => march(*field, origin, dir).map(|normal| Hit { normal, color: None }),
            Shape::Voxels(voxels) => voxels.hit(origin, dir),
        }
    }
}

/// Walks the ray through a distance field; the normal where it lands.
fn march(field: fn(Vec3) -> f32, origin: Vec3, dir: Vec3) -> Option<Vec3> {
    // Start where the ray enters the bounding sphere (radius 1.2).
    let b = dot(origin, dir);
    let c = dot(origin, origin) - 1.44;
    let disc = b * b - c;
    if disc < 0.0 { return None; }
    let (mut t, end) = (-b - disc.sqrt(), -b + disc.sqrt());
    for _ in 0..96 {
        let p = add(origin, scale(dir, t));
        let d = field(p);
        if d < 1e-3 {
            // The gradient from four samples on a tetrahedron (Inigo Quilez), not six.
            const E: f32 = 1e-3;
            const K: [Vec3; 4] = [[1.0, -1.0, -1.0], [-1.0, -1.0, 1.0], [-1.0, 1.0, -1.0], [1.0, 1.0, 1.0]];
            let n = K.iter().fold([0.0; 3], |n, k| add(n, scale(*k, field(add(p, scale(*k, E))))));
            return Some(normalize(n));
        }
        t += d;
        if t > end { return None; }
    }
    None
}

impl Voxels {
    fn new(solid: Vec<Vec<Option<Option<Color>>>>, depth: f32) -> Self {
        let rows = solid.len().max(1) as f32;
        let cols = solid.iter().map(Vec::len).max().unwrap_or(0).max(1) as f32;
        let depth = depth.max(0.5);
        // Fit the larger of width (cols) and height (rows × 2) into 2 units.
        let k = 2.0 / cols.max(rows * 2.0).max(depth);
        let cell = [k, 2.0 * k, depth * k];
        let origin = [-cols * k / 2.0, rows * k, -depth * k / 2.0];
        Self { solid, cell, origin }
    }

    fn solid(&self, col: i32, row: i32) -> Option<&Option<Color>> {
        if col < 0 || row < 0 { return None; }
        self.solid.get(row as usize)?.get(col as usize)?.as_ref()
    }

    /// Steps through the grid from voxel to voxel (Amanatides & Woo).
    fn hit(&self, origin: Vec3, dir: Vec3) -> Option<Hit> {
        // Grid space: x columns to the right, y rows downwards, z layers to the back.
        let to_grid = |p: Vec3| [(p[0] - self.origin[0]) / self.cell[0], (self.origin[1] - p[1]) / self.cell[1], (p[2] - self.origin[2]) / self.cell[2]];
        let o = to_grid(origin);
        let d = [dir[0] / self.cell[0], -dir[1] / self.cell[1], dir[2] / self.cell[2]];
        let size = [self.solid.iter().map(Vec::len).max().unwrap_or(0) as f32, self.solid.len() as f32, 1.0];
        // Where the ray enters the grid's box, and through which face.
        let (mut enter, mut exit, mut face) = (f32::NEG_INFINITY, f32::INFINITY, 0);
        for axis in 0..3 {
            if d[axis].abs() < 1e-9 {
                if o[axis] < 0.0 || o[axis] > size[axis] { return None; }
                continue;
            }
            let (a, b) = ((0.0 - o[axis]) / d[axis], (size[axis] - o[axis]) / d[axis]);
            let (near, far) = (a.min(b), a.max(b));
            if near > enter {
                enter = near;
                face = axis;
            }
            exit = exit.min(far);
        }
        if enter > exit || exit < 0.0 { return None; }
        let t = enter.max(0.0) + 1e-4;
        let p = add(o, scale(d, t));
        let mut cell = p.map(|c| c.floor() as i32);
        let step = d.map(|c| if c > 0.0 { 1 } else { -1 });
        let next = |axis: usize, c: i32| {
            let boundary = if step[axis] > 0 { c + 1 } else { c } as f32;
            if d[axis].abs() < 1e-9 { f32::INFINITY } else { (boundary - o[axis]) / d[axis] }
        };
        let mut t_max = [next(0, cell[0]), next(1, cell[1]), next(2, cell[2])];
        let t_delta = d.map(|c| if c.abs() < 1e-9 { f32::INFINITY } else { 1.0 / c.abs() });
        for _ in 0..4096 {
            if cell[2] == 0 && let Some(color) = self.solid(cell[0], cell[1]) {
                // The face the ray came through, back in object space (rows grow downwards).
                let mut normal = [0.0; 3];
                normal[face] = -(step[face] as f32);
                if face == 1 { normal[1] = -normal[1]; }
                return Some(Hit { normal, color: color.clone() });
            }
            let axis = if t_max[0] < t_max[1] { if t_max[0] < t_max[2] { 0 } else { 2 } } else if t_max[1] < t_max[2] { 1 } else { 2 };
            if t_max[axis] > exit { return None; }
            cell[axis] += step[axis];
            t_max[axis] += t_delta[axis];
            face = axis;
        }
        None
    }
}

/// What one ray saw.
#[derive(Clone, Copy)]
struct Sample {
    /// 0..1, with the ambient light.
    light: f32,
    color: [f32; 3],
}

impl Scene3D {
    /// The cells of a `width` × `height` canvas showing the scene in animation frame `frame`.
    pub fn render(&self, width: usize, height: usize, frame: usize) -> Vec<Cell> {
        let shape = Shape::new(&self.object);
        let turn: Vec3 = std::array::from_fn(|i| self.turn[i] + self.spin[i] * frame as f32);
        let r = rotation(turn);
        let r_inv = transpose(&r);
        let [az, el] = self.light.map(f32::to_radians);
        let light = [el.cos() * az.sin(), el.sin(), el.cos() * az.cos()];
        let half = normalize(add(light, [0.0, 0.0, 1.0]));
        let base = [self.color.r, self.color.g, self.color.b].map(|c| c as f32 / 255.0);
        let ramp: Vec<char> = self.ramp.chars().collect();

        // Canvas units are cell widths; a cell is 2 units tall.
        let (w, h) = (width as f32, height as f32 * 2.0);
        let centre = [w / 2.0 + self.position[0] * w, h / 2.0 + self.position[1] * h];
        let radius = (self.size * w.min(h) / 2.0).max(0.01);
        let ray = mul(&r_inv, [0.0, 0.0, -1.0]);
        let sample = |x: f32, y: f32| -> Option<Sample> {
            let view = [(x - centre[0]) / radius, -(y - centre[1]) / radius, 3.0];
            let hit = shape.hit(mul(&r_inv, view), ray)?;
            let n = mul(&r, hit.normal);
            let diffuse = dot(n, light).max(0.0);
            let lit = self.ambient + (1.0 - self.ambient) * diffuse;
            let spec = if self.shine { dot(n, half).max(0.0).powi(24) * 0.6 } else { 0.0 };
            let own = hit.color.map(|c| [c.r, c.g, c.b].map(|v| v as f32 / 255.0)).unwrap_or(base);
            Some(Sample { light: (lit + spec).min(1.0), color: own.map(|c| (c * lit + spec).min(1.0)) })
        };
        let to_color = |c: [f32; 3]| Color::new((c[0] * 255.0) as u8, (c[1] * 255.0) as u8, (c[2] * 255.0) as u8, 255);

        let (sx, sy) = match self.style {
            Style3D::Shade => (2, 2),
            Style3D::Braille => (2, 4),
            Style3D::Blocks => (1, 2),
        };
        let cell_at = |cx: usize, cy: usize| -> Cell {
            // At most 8 rays per cell, kept on the stack.
            let mut samples = [None; 8];
            let count = sx * sy;
            for (k, slot) in samples[..count].iter_mut().enumerate() {
                let (i, j) = (k % sx, k / sx);
                *slot = sample(cx as f32 + (i as f32 + 0.5) / sx as f32, (cy as f32 + (j as f32 + 0.5) / sy as f32) * 2.0);
            }
            let samples = &samples[..count];
            let hits = samples.iter().flatten().count();
            if hits == 0 {
                return Cell::empty();
            }
            let mean = |f: &dyn Fn(&Sample) -> f32| samples.iter().flatten().map(f).sum::<f32>() / hits as f32;
            let color = to_color([0, 1, 2].map(|k| mean(&|s| s.color[k])));
            match self.style {
                Style3D::Shade => {
                    // Misses count as dark, so edges fade out.
                    let value = samples.iter().flatten().map(|s| s.light).sum::<f32>() / count as f32;
                    let last = ramp.len().saturating_sub(1);
                    let ch = ramp.get(((value * last as f32).round() as usize).min(last)).copied().unwrap_or(' ');
                    // Dim glyphs stay visible: the colour only darkens so far.
                    let shade = to_color([0, 1, 2].map(|k| mean(&|s| s.color[k]).max(base[k] * 0.35)));
                    Cell::new(ch, shade, Color::TRANSPARENT)
                }
                Style3D::Braille => {
                    const ORDER: [[f32; 2]; 4] = [[0.0, 8.0], [12.0, 4.0], [3.0, 11.0], [15.0, 7.0]];
                    const DOT: [[u32; 2]; 4] = [[0, 3], [1, 4], [2, 5], [6, 7]];
                    let mut bits = 0;
                    for (k, s) in samples.iter().enumerate() {
                        let (i, j) = (k % 2, k / 2);
                        if let Some(s) = s
                            && s.light > (ORDER[j][i] + 0.5) / 16.0
                        {
                            bits |= 1 << DOT[j][i];
                        }
                    }
                    if bits == 0 { Cell::empty() } else { Cell::new(char::from_u32(0x2800 + bits).unwrap(), color, Color::TRANSPARENT) }
                }
                Style3D::Blocks => {
                    let pixel = |s: &Option<Sample>| s.as_ref().map(|s| to_color(s.color));
                    from_halves(pixel(&samples[0]), pixel(&samples[1]))
                }
            }
        };
        let rows = |from: usize, to: usize| -> Vec<Cell> {
            (from..to).flat_map(|cy| (0..width).map(move |cx| (cx, cy))).map(|(cx, cy)| cell_at(cx, cy)).collect()
        };

        // Bands of rows on every core; small pictures in one go.
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(16);
        if threads < 2 || width * height * sx * sy < 16_000 {
            return rows(0, height);
        }
        let band = height.div_ceil(threads);
        std::thread::scope(|scope| {
            let rows = &rows;
            let bands: Vec<_> = (0..height).step_by(band).map(|from| scope.spawn(move || rows(from, (from + band).min(height)))).collect();
            bands.into_iter().flat_map(|b| b.join().expect("a render thread")).collect()
        })
    }
}
