//! `use vyrn.draw`: pictures, games and 3D, with nothing to install.
//!
//! ```vyrn
//! use vyrn.draw
//!
//! main:
//!     let win = draw.window(640, 480, "Hello")
//!     var x = 0.0
//!     loop win.open():
//!         win.clear("black")
//!         win.circle(x, 240, 20, "orange")
//!         win.text("Press arrows", 10, 10, "white")
//!         if win.key_down("right"): x += 4
//!         win.show()
//!     end
//! end
//! ```
//!
//! A `canvas` is the same drawing surface without a window: draw on it and
//! `save` it as a PNG. Everything is drawn in software, so it works the
//! same on every machine; the window only shows the result. There is also
//! a turtle for drawing by moving (`draw.turtle(surface)`), images
//! (`draw.image("ship.png")`), and simple 3D: a camera, meshes (cubes,
//! spheres, planes, or your own) and filled, shaded triangles with depth.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use super::builtins::number;
use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::Value;

// ==== arguments by position or by name ======================================

struct Args<'a> {
    pos: &'a [Value],
    named: &'a [(String, Value)],
    what: &'a str,
}

impl<'a> Args<'a> {
    fn new(what: &'a str, pos: &'a [Value], named: &'a [(String, Value)]) -> Args<'a> {
        Args { pos, named, what }
    }

    fn get(&self, i: usize, name: &str) -> Option<&'a Value> {
        self.named
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, v)| v)
            .or_else(|| self.pos.get(i))
            .filter(|v| !matches!(v, Value::Missing))
    }

    fn num(&self, i: usize, name: &str) -> VmResult<f64> {
        match self.get(i, name) {
            Some(v) => number(v, &format!("`{name}` of `{}`", self.what)),
            None => Err(err("CallError", format!("`{}` needs `{name}`", self.what))),
        }
    }

    fn num_or(&self, i: usize, name: &str, default: f64) -> VmResult<f64> {
        match self.get(i, name) {
            Some(v) => number(v, &format!("`{name}` of `{}`", self.what)),
            None => Ok(default),
        }
    }

    fn color_or(&self, i: usize, name: &str, default: u32) -> VmResult<Color> {
        match self.get(i, name) {
            Some(v) => parse_color(v),
            None => Ok(Color::opaque(default)),
        }
    }

    fn flag_or(&self, i: usize, name: &str, default: bool) -> bool {
        self.get(i, name).map_or(default, Value::truthy)
    }

    fn text(&self, i: usize, name: &str) -> VmResult<String> {
        match self.get(i, name) {
            Some(Value::Str(s)) => Ok(s.to_string()),
            Some(v) => Ok(super::stdlib::quote(&format!("{v:?}"))),
            None => Err(err("CallError", format!("`{}` needs `{name}`", self.what))),
        }
    }
}

// ==== colours ===============================================================

#[derive(Clone, Copy, Debug, PartialEq)]
struct Color {
    rgb: u32,
    alpha: u8,
}

impl Color {
    fn opaque(rgb: u32) -> Color {
        Color { rgb, alpha: 255 }
    }
}

const NAMES: &[(&str, u32)] = &[
    ("black", 0x000000),
    ("white", 0xffffff),
    ("red", 0xe53935),
    ("green", 0x43a047),
    ("blue", 0x1e88e5),
    ("yellow", 0xfdd835),
    ("orange", 0xfb8c00),
    ("purple", 0x8e24aa),
    ("pink", 0xf06292),
    ("brown", 0x795548),
    ("gray", 0x9e9e9e),
    ("grey", 0x9e9e9e),
    ("darkgray", 0x424242),
    ("lightgray", 0xe0e0e0),
    ("cyan", 0x00bcd4),
    ("magenta", 0xd81b60),
    ("lime", 0xc0ca33),
    ("navy", 0x1a237e),
    ("teal", 0x00897b),
    ("gold", 0xffc107),
    ("sky", 0x81d4fa),
    ("darkgreen", 0x1b5e20),
    ("skyblue", 0x87ceeb),
    ("lightblue", 0xadd8e6),
    ("darkblue", 0x0d47a1),
    ("darkred", 0x8b0000),
    ("lightgreen", 0x90ee90),
    ("olive", 0x808000),
    ("maroon", 0x800000),
    ("violet", 0xee82ee),
    ("indigo", 0x4b0082),
    ("silver", 0xc0c0c0),
    ("beige", 0xf5f5dc),
    ("coral", 0xff7f50),
    ("salmon", 0xfa8072),
    ("tan", 0xd2b48c),
    ("turquoise", 0x40e0d0),
    ("crimson", 0xdc143c),
    ("chocolate", 0xd2691e),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("mint", 0x98ff98),
    ("peach", 0xffcba4),
    ("ivory", 0xfffff0),
    ("aqua", 0x00ffff),
    ("forestgreen", 0x228b22),
    ("tomato", 0xff6347),
    ("orchid", 0xda70d6),
];

fn parse_color(v: &Value) -> VmResult<Color> {
    let bad = |what: String| {
        type_error(format!(
            "{what} is not a colour; use a name such as \"red\", \"#ff8800\", or [255, 136, 0]"
        ))
    };
    match v {
        Value::Str(s) => {
            // "Sky Blue", "sky_blue" and "skyblue" are the same colour.
            let s: String = s
                .trim()
                .to_lowercase()
                .chars()
                .filter(|c| !matches!(c, ' ' | '_' | '-'))
                .collect();
            if let Some(hex) = s.strip_prefix('#') {
                let full: String = match hex.len() {
                    3 => hex.chars().flat_map(|c| [c, c]).collect(),
                    6 | 8 => hex.to_string(),
                    _ => return Err(bad(format!("{s:?}"))),
                };
                let n = u32::from_str_radix(&full[..6], 16).map_err(|_| bad(format!("{s:?}")))?;
                let alpha = if full.len() == 8 {
                    u8::from_str_radix(&full[6..8], 16).map_err(|_| bad(format!("{s:?}")))?
                } else {
                    255
                };
                return Ok(Color { rgb: n, alpha });
            }
            NAMES
                .iter()
                .find(|(n, _)| *n == s)
                .map(|(_, c)| Color::opaque(*c))
                .ok_or_else(|| bad(format!("{s:?}")))
        }
        Value::List(_) | Value::Tuple(_) => {
            let parts = ops::collect(v)?;
            if !(3..=4).contains(&parts.len()) {
                return Err(bad("a list without 3 or 4 numbers".into()));
            }
            let ch = |i: usize| -> VmResult<u32> {
                Ok(number(&parts[i], "a colour channel")?.clamp(0.0, 255.0) as u32)
            };
            Ok(Color {
                rgb: (ch(0)? << 16) | (ch(1)? << 8) | ch(2)?,
                alpha: if parts.len() == 4 { ch(3)? as u8 } else { 255 },
            })
        }
        other => Err(bad(other.type_name().to_string())),
    }
}

fn color_text(rgb: u32) -> String {
    format!("#{:06x}", rgb & 0xff_ffff)
}

// ==== pixels ================================================================

/// What every surface draws into.
pub struct Pixels {
    pub w: usize,
    pub h: usize,
    pub buf: Vec<u32>,
    /// Depth of what is drawn in 3D, nearest wins.
    depth: Vec<f32>,
    /// Which surface this is, for its GPU resources.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    id: u64,
    /// 3D triangles waiting to be filled on the GPU, grouped by texture.
    #[cfg(feature = "gpu")]
    gpu: Vec<super::draw_gpu::Batch>,
    /// The GPU depth buffer must start over (after `clear`).
    #[cfg(feature = "gpu")]
    depth_reset: bool,
    /// How 3D shapes are lit.
    lights: Lights,
}

/// The lights of a 3D scene. The defaults light everything from above and
/// behind the camera, as Vyrn always has.
#[derive(Clone)]
struct Lights {
    /// Towards the sun, length 1.
    sun: [f64; 3],
    sun_color: [f64; 3],
    ambient: [f64; 3],
    /// Lamps: position, colour (times strength) and how far they reach.
    /// `clear` turns them off, so a game adds this frame's lamps each frame.
    points: Vec<([f64; 3], [f64; 3], f64)>,
    /// Things fade to this colour from `start` to `end` away.
    fog: Option<([f64; 3], f64, f64)>,
}

impl Default for Lights {
    fn default() -> Lights {
        Lights {
            sun: normalize([0.4, 0.8, -0.45]),
            sun_color: [0.65; 3],
            ambient: [0.35; 3],
            points: Vec::new(),
            fog: None,
        }
    }
}

fn normalize(v: [f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if n == 0.0 {
        v
    } else {
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

fn rgb_unit(c: u32) -> [f64; 3] {
    [
        ((c >> 16) & 0xff) as f64 / 255.0,
        ((c >> 8) & 0xff) as f64 / 255.0,
        (c & 0xff) as f64 / 255.0,
    ]
}

impl Lights {
    /// The colour of `base` at point `p` facing `n` (length 1), seen from
    /// `eye`, as 0..255 channels and alpha. Surfaces are lit on both sides.
    fn shade(&self, base: Color, p: [f64; 3], n: [f64; 3], eye: [f64; 3]) -> [f64; 4] {
        let d = (n[0] * self.sun[0] + n[1] * self.sun[1] + n[2] * self.sun[2]).abs();
        let mut light = [
            self.ambient[0] + self.sun_color[0] * d,
            self.ambient[1] + self.sun_color[1] * d,
            self.ambient[2] + self.sun_color[2] * d,
        ];
        for (at, color, range) in &self.points {
            let to = [at[0] - p[0], at[1] - p[1], at[2] - p[2]];
            let dist = (to[0] * to[0] + to[1] * to[1] + to[2] * to[2]).sqrt();
            if dist >= *range || dist == 0.0 {
                continue;
            }
            let fall = (1.0 - dist / range).powi(2);
            let facing = ((n[0] * to[0] + n[1] * to[1] + n[2] * to[2]) / dist).abs();
            for k in 0..3 {
                light[k] += color[k] * fall * facing;
            }
        }
        let ch = |shift: u32, k: usize| {
            (((base.rgb >> shift) & 0xff) as f64 * light[k])
                .clamp(0.0, 255.0)
                .trunc()
        };
        let mut out = [ch(16, 0), ch(8, 1), ch(0, 2), f64::from(base.alpha)];
        if let Some((fog, start, end)) = self.fog {
            let dist =
                ((p[0] - eye[0]).powi(2) + (p[1] - eye[1]).powi(2) + (p[2] - eye[2]).powi(2))
                    .sqrt();
            let f = if end > start {
                ((dist - start) / (end - start)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            for k in 0..3 {
                out[k] = (out[k] * (1.0 - f) + fog[k] * 255.0 * f).trunc();
            }
        }
        out
    }
}

static NEXT_SURFACE: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

#[cfg(feature = "gpu")]
impl Drop for Pixels {
    fn drop(&mut self) {
        super::draw_gpu::forget(self.id);
    }
}

impl Pixels {
    fn new(w: usize, h: usize) -> Pixels {
        Pixels {
            w,
            h,
            buf: vec![0; w * h],
            depth: vec![f32::INFINITY; w * h],
            id: NEXT_SURFACE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            #[cfg(feature = "gpu")]
            gpu: Vec::new(),
            #[cfg(feature = "gpu")]
            depth_reset: true,
            lights: Lights::default(),
        }
    }

    /// Fills the 3D triangles waiting for the GPU, before anything reads or
    /// draws over the pixels.
    #[inline]
    fn sync(&mut self) {
        #[cfg(feature = "gpu")]
        if !self.gpu.is_empty() {
            let tris = std::mem::take(&mut self.gpu);
            super::draw_gpu::render(
                self.id,
                &mut self.buf,
                self.w,
                self.h,
                &tris,
                self.depth_reset,
            );
            self.depth_reset = false;
        }
    }

    fn clear(&mut self, c: Color) {
        #[cfg(feature = "gpu")]
        {
            self.gpu.clear();
            self.depth_reset = true;
        }
        // Lamps are placed again each frame.
        self.lights.points.clear();
        self.buf.fill(c.rgb);
        self.depth.fill(f32::INFINITY);
    }

    fn put(&mut self, x: i64, y: i64, c: Color) {
        if x < 0 || y < 0 || x >= self.w as i64 || y >= self.h as i64 {
            return;
        }
        self.sync();
        let i = y as usize * self.w + x as usize;
        self.buf[i] = blend(self.buf[i], c);
    }

    fn hline(&mut self, x0: i64, x1: i64, y: i64, c: Color) {
        if y < 0 || y >= self.h as i64 {
            return;
        }
        let (a, b) = (x0.max(0), x1.min(self.w as i64 - 1));
        self.sync();
        for x in a..=b {
            let i = y as usize * self.w + x as usize;
            self.buf[i] = blend(self.buf[i], c);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn rect(&mut self, x: f64, y: f64, w: f64, h: f64, c: Color, fill: bool, width: f64) {
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        let (x1, y1) = ((x + w).round() as i64 - 1, (y + h).round() as i64 - 1);
        if fill {
            for yy in y0.max(0)..=y1.min(self.h as i64 - 1) {
                self.hline(x0, x1, yy, c);
            }
        } else {
            let t = width.max(1.0);
            self.rect(x, y, w, t, c, true, 1.0);
            self.rect(x, y + h - t, w, t, c, true, 1.0);
            self.rect(x, y + t, t, h - 2.0 * t, c, true, 1.0);
            self.rect(x + w - t, y + t, t, h - 2.0 * t, c, true, 1.0);
        }
    }

    fn circle(&mut self, cx: f64, cy: f64, r: f64, c: Color, fill: bool, width: f64) {
        let ri = r.ceil() as i64;
        let (cxi, cyi) = (cx.round() as i64, cy.round() as i64);
        let inner = if fill {
            -1.0
        } else {
            (r - width.max(1.0)).max(0.0)
        };
        for dy in -ri..=ri {
            for dx in -ri..=ri {
                let d = ((dx * dx + dy * dy) as f64).sqrt();
                if d <= r && d > inner {
                    self.put(cxi + dx, cyi + dy, c);
                }
            }
        }
    }

    fn line(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, c: Color, width: f64) {
        let (mut x, mut y) = (x0.round() as i64, y0.round() as i64);
        let (xe, ye) = (x1.round() as i64, y1.round() as i64);
        let (dx, dy) = ((xe - x).abs(), -(ye - y).abs());
        let (sx, sy) = (if x < xe { 1 } else { -1 }, if y < ye { 1 } else { -1 });
        let mut e = dx + dy;
        let r = (width.max(1.0) - 1.0) / 2.0;
        loop {
            if r <= 0.0 {
                self.put(x, y, c);
            } else {
                self.circle(x as f64, y as f64, r + 0.5, c, true, 1.0);
            }
            if x == xe && y == ye {
                break;
            }
            let e2 = 2 * e;
            if e2 >= dy {
                e += dy;
                x += sx;
            }
            if e2 <= dx {
                e += dx;
                y += sy;
            }
        }
    }

    /// A filled polygon, by scanlines (even-odd rule).
    fn polygon(&mut self, pts: &[(f64, f64)], c: Color) {
        if pts.len() < 3 {
            return;
        }
        let ymin = pts
            .iter()
            .map(|p| p.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let ymax = pts
            .iter()
            .map(|p| p.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(self.h as f64 - 1.0) as i64;
        for y in ymin..=ymax {
            let yc = y as f64 + 0.5;
            let mut xs = Vec::new();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= yc && b.1 > yc) || (b.1 <= yc && a.1 > yc) {
                    xs.push(a.0 + (yc - a.1) / (b.1 - a.1) * (b.0 - a.0));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            for pair in xs.chunks(2) {
                if let [a, b] = pair {
                    self.hline(a.round() as i64, b.round() as i64 - 1, y, c);
                }
            }
        }
    }

    /// A 3D triangle already projected: screen x, y and depth per corner.
    /// Fills a 3D triangle given in screen x, y and distance, with a colour
    /// (0..255 channels and alpha) and texture coordinates per corner,
    /// nearest wins. Texture coordinates follow the perspective.
    fn triangle_shaded(
        &mut self,
        p: [(f64, f64, f64); 3],
        col: [[f64; 4]; 3],
        uv: [[f64; 2]; 3],
        tex: Option<&Image>,
    ) {
        #[cfg(feature = "gpu")]
        if super::draw_gpu::active() {
            let (w, h) = (self.w as f64, self.h as f64);
            let texture = tex.map(Image::texture);
            let id = texture.as_ref().map(|t| t.id);
            if self
                .gpu
                .last()
                .is_none_or(|b| b.texture.as_ref().map(|t| t.id) != id)
            {
                self.gpu.push(super::draw_gpu::Batch {
                    texture,
                    verts: Vec::new(),
                });
            }
            let batch = self.gpu.last_mut().expect("pushed above");
            for (k, q) in p.iter().enumerate() {
                let z = q.2.max(NEAR);
                batch.verts.push(super::draw_gpu::Vertex {
                    pos: [
                        ((q.0 / w * 2.0 - 1.0) * z) as f32,
                        ((1.0 - q.1 / h * 2.0) * z) as f32,
                        // Nearer is smaller, from 0 at the near plane to 1.
                        ((1.0 - NEAR / z) * z) as f32,
                        z as f32,
                    ],
                    color: [
                        (col[k][0] / 255.0) as f32,
                        (col[k][1] / 255.0) as f32,
                        (col[k][2] / 255.0) as f32,
                        (col[k][3] / 255.0) as f32,
                    ],
                    uv: [uv[k][0] as f32, uv[k][1] as f32],
                });
            }
            return;
        }
        let minx = p
            .iter()
            .map(|q| q.0)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let maxx = p
            .iter()
            .map(|q| q.0)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(self.w as f64 - 1.0) as i64;
        let miny = p
            .iter()
            .map(|q| q.1)
            .fold(f64::INFINITY, f64::min)
            .floor()
            .max(0.0) as i64;
        let maxy = p
            .iter()
            .map(|q| q.1)
            .fold(f64::NEG_INFINITY, f64::max)
            .ceil()
            .min(self.h as f64 - 1.0) as i64;
        let area = (p[1].0 - p[0].0) * (p[2].1 - p[0].1) - (p[2].0 - p[0].0) * (p[1].1 - p[0].1);
        if area.abs() < 1e-9 {
            return;
        }
        // One colour for the whole triangle unless its corners differ.
        let flat = col[0] == col[1] && col[1] == col[2];
        let color_of = |c: [f64; 4]| Color {
            rgb: ((c[0] as u32) << 16) | ((c[1] as u32) << 8) | c[2] as u32,
            alpha: c[3] as u8,
        };
        let inv = [
            1.0 / p[0].2.max(NEAR),
            1.0 / p[1].2.max(NEAR),
            1.0 / p[2].2.max(NEAR),
        ];
        for y in miny..=maxy {
            for x in minx..=maxx {
                let (px, py) = (x as f64 + 0.5, y as f64 + 0.5);
                let w0 = ((p[1].0 - px) * (p[2].1 - py) - (p[2].0 - px) * (p[1].1 - py)) / area;
                let w1 = ((p[2].0 - px) * (p[0].1 - py) - (p[0].0 - px) * (p[2].1 - py)) / area;
                let w2 = 1.0 - w0 - w1;
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = (w0 * p[0].2 + w1 * p[1].2 + w2 * p[2].2) as f32;
                let i = y as usize * self.w + x as usize;
                if z >= self.depth[i] {
                    continue;
                }
                let mut c = if flat {
                    col[0]
                } else {
                    let mut c = [0.0; 4];
                    for (k, v) in c.iter_mut().enumerate() {
                        *v = (w0 * col[0][k] + w1 * col[1][k] + w2 * col[2][k]).clamp(0.0, 255.0);
                    }
                    c
                };
                if let Some(img) = tex {
                    // Perspective-correct: interpolate uv/z and 1/z.
                    let (a, b, d) = (w0 * inv[0], w1 * inv[1], w2 * inv[2]);
                    let sum = a + b + d;
                    let u = (a * uv[0][0] + b * uv[1][0] + d * uv[2][0]) / sum;
                    let v = (a * uv[0][1] + b * uv[1][1] + d * uv[2][1]) / sum;
                    let (rgb, alpha) = img.sample(u, v);
                    let t = rgb_unit(rgb);
                    for k in 0..3 {
                        c[k] = (c[k] * t[k]).trunc();
                    }
                    c[3] = (c[3] * f64::from(alpha) / 255.0).trunc();
                }
                if c[3] <= 0.0 {
                    continue;
                }
                self.depth[i] = z;
                self.buf[i] = blend(self.buf[i], color_of(c));
            }
        }
    }

    fn text(&mut self, text: &str, x: f64, y: f64, c: Color, size: f64) {
        use font8x8::UnicodeFonts;
        let scale = size.max(1.0).round() as i64;
        let (mut cx, mut cy) = (x.round() as i64, y.round() as i64);
        for ch in text.chars() {
            if ch == '\n' {
                cx = x.round() as i64;
                cy += 9 * scale;
                continue;
            }
            // ASCII, then accented letters and signs such as °, é and ñ,
            // then Greek and box drawing; anything else shows as `?`.
            let glyph = font8x8::BASIC_FONTS
                .get(ch)
                .or_else(|| font8x8::LATIN_FONTS.get(ch))
                .or_else(|| font8x8::GREEK_FONTS.get(ch))
                .or_else(|| font8x8::BOX_FONTS.get(ch))
                .or_else(|| font8x8::BASIC_FONTS.get('?'))
                .unwrap_or([0; 8]);
            for (row, bits) in glyph.iter().enumerate() {
                for col in 0..8 {
                    if bits & (1 << col) != 0 {
                        for sy in 0..scale {
                            for sx in 0..scale {
                                self.put(cx + col * scale + sx, cy + row as i64 * scale + sy, c);
                            }
                        }
                    }
                }
            }
            cx += 8 * scale;
        }
    }

    fn image(&mut self, img: &Image, x: f64, y: f64, scale: f64) {
        let s = scale.max(0.01);
        let (w, h) = (
            (img.w as f64 * s).round() as i64,
            (img.h as f64 * s).round() as i64,
        );
        let (x0, y0) = (x.round() as i64, y.round() as i64);
        for dy in 0..h {
            for dx in 0..w {
                let sx = ((dx as f64 / s) as usize).min(img.w - 1);
                let sy = ((dy as f64 / s) as usize).min(img.h - 1);
                let (rgb, a) = img.px[sy * img.w + sx];
                if a > 0 {
                    self.put(x0 + dx, y0 + dy, Color { rgb, alpha: a });
                }
            }
        }
    }

    fn save_png(&self, path: &str) -> VmResult<()> {
        let file = std::fs::File::create(path)
            .map_err(|e| err("IoError", format!("cannot write {path}: {e}")))?;
        let mut enc =
            png::Encoder::new(std::io::BufWriter::new(file), self.w as u32, self.h as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc
            .write_header()
            .map_err(|e| err("IoError", format!("cannot write {path}: {e}")))?;
        let mut data = Vec::with_capacity(self.w * self.h * 3);
        for p in &self.buf {
            data.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
        }
        writer
            .write_image_data(&data)
            .map_err(|e| err("IoError", format!("cannot write {path}: {e}")))
    }
}

fn blend(dst: u32, c: Color) -> u32 {
    if c.alpha == 255 {
        return c.rgb;
    }
    let a = u32::from(c.alpha);
    let mix = |shift: u32| {
        let s = (c.rgb >> shift) & 0xff;
        let d = (dst >> shift) & 0xff;
        ((s * a + d * (255 - a)) / 255) << shift
    };
    mix(16) | mix(8) | mix(0)
}

// ==== images ================================================================

/// A picture loaded from a PNG file.
pub struct Image {
    w: usize,
    h: usize,
    /// (rgb, alpha) per pixel.
    px: Vec<(u32, u8)>,
    /// Unique to this image, so the GPU uploads it once.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    id: u64,
    /// RGBA bytes, made the first time the image is used as a texture.
    #[cfg_attr(not(feature = "gpu"), allow(dead_code))]
    rgba: std::cell::OnceCell<Rc<Vec<u8>>>,
}

impl Image {
    fn new(w: usize, h: usize, px: Vec<(u32, u8)>) -> Image {
        Image {
            w,
            h,
            px,
            id: NEXT_SURFACE.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
            rgba: std::cell::OnceCell::new(),
        }
    }

    /// The texel at texture coordinates (u, v), from the top left,
    /// repeating past the edges; nearest pixel, as the GPU samples.
    fn sample(&self, u: f64, v: f64) -> (u32, u8) {
        if self.w == 0 || self.h == 0 {
            return (0xffffff, 255);
        }
        let x = ((u - u.floor()) * self.w as f64) as usize;
        let y = ((v - v.floor()) * self.h as f64) as usize;
        self.px[y.min(self.h - 1) * self.w + x.min(self.w - 1)]
    }

    #[cfg(feature = "gpu")]
    fn texture(&self) -> super::draw_gpu::TextureData {
        let rgba = self.rgba.get_or_init(|| {
            Rc::new(
                self.px
                    .iter()
                    .flat_map(|(rgb, a)| [(rgb >> 16) as u8, (rgb >> 8) as u8, *rgb as u8, *a])
                    .collect(),
            )
        });
        super::draw_gpu::TextureData {
            id: self.id,
            w: self.w as u32,
            h: self.h as u32,
            rgba: rgba.clone(),
        }
    }
}

fn load_png(path: &str) -> VmResult<Image> {
    let bytes = std::fs::read(path).map_err(|e| {
        err(
            if e.kind() == std::io::ErrorKind::NotFound {
                "NotFound"
            } else {
                "IoError"
            },
            format!("cannot read {path}: {e}"),
        )
    })?;
    let bad = |e: png::DecodingError| err("ValueError", format!("{path} is not a PNG image: {e}"));
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = dec.read_info().map_err(bad)?;
    let mut buf = vec![0; reader.output_buffer_size().unwrap_or(0)];
    let info = reader.next_frame(&mut buf).map_err(bad)?;
    let (w, h) = (info.width as usize, info.height as usize);
    let channels = match info.color_type {
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Indexed => {
            return Err(err(
                "ValueError",
                format!("{path}: unsupported PNG colours"),
            ))
        }
    };
    let mut px = Vec::with_capacity(w * h);
    for i in 0..w * h {
        let p = &buf[i * channels..(i + 1) * channels];
        let (r, g, b, a) = match channels {
            1 => (p[0], p[0], p[0], 255),
            2 => (p[0], p[0], p[0], p[1]),
            3 => (p[0], p[1], p[2], 255),
            _ => (p[0], p[1], p[2], p[3]),
        };
        px.push(((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b), a));
    }
    Ok(Image::new(w, h, px))
}

impl Foreign for Image {
    fn type_name(&self) -> &'static str {
        "Image"
    }

    fn display(&self) -> String {
        format!("Image({}x{})", self.w, self.h)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "width" => Ok(Value::Int(self.w as i64)),
            "height" => Ok(Value::Int(self.h as i64)),
            _ => Err(err("FieldError", format!("an Image has no field `{name}`"))),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== 3D ====================================================================

/// Where the 3D scene is seen from.
pub struct Camera {
    pos: Cell<[f64; 3]>,
    /// Turning left and right, and up and down, in degrees.
    yaw: Cell<f64>,
    pitch: Cell<f64>,
    fov: Cell<f64>,
}

impl Camera {
    /// The way it looks, length 1: yaw turns right, pitch looks up.
    fn direction(&self) -> [f64; 3] {
        let (sy, cy) = self.yaw.get().to_radians().sin_cos();
        let (sp, cp) = self.pitch.get().to_radians().sin_cos();
        [sy * cp, sp, cy * cp]
    }

    /// A world point in camera space: x right, y up, z forward.
    fn view(&self, p: [f64; 3]) -> [f64; 3] {
        let c = self.pos.get();
        let (x, y, z) = (p[0] - c[0], p[1] - c[1], p[2] - c[2]);
        let (sy, cy) = self.yaw.get().to_radians().sin_cos();
        let (x, z) = (x * cy - z * sy, x * sy + z * cy);
        let (sp, cp) = self.pitch.get().to_radians().sin_cos();
        let (y, z) = (y * cp - z * sp, y * sp + z * cp);
        [x, y, z]
    }

    fn project(&self, v: [f64; 3], w: usize, h: usize) -> Option<(f64, f64, f64)> {
        if v[2] < NEAR - 1e-9 {
            return None;
        }
        let f = (h as f64 / 2.0) / (self.fov.get().to_radians() / 2.0).tan();
        Some((
            w as f64 / 2.0 + v[0] * f / v[2],
            h as f64 / 2.0 - v[1] * f / v[2],
            v[2],
        ))
    }
}

impl Foreign for Camera {
    fn type_name(&self) -> &'static str {
        "Camera"
    }

    fn display(&self) -> String {
        let p = self.pos.get();
        format!(
            "Camera(x={}, y={}, z={}, yaw={}, pitch={})",
            p[0],
            p[1],
            p[2],
            self.yaw.get(),
            self.pitch.get()
        )
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        let p = self.pos.get();
        Ok(Value::Float(match name {
            "x" => p[0],
            "y" => p[1],
            "z" => p[2],
            "yaw" => self.yaw.get(),
            "pitch" => self.pitch.get(),
            // Where it looks, as a direction of length 1: for aiming.
            "direction" => {
                return Ok(Value::list(
                    self.direction().iter().map(|v| Value::Float(*v)).collect(),
                ))
            }
            "position" => {
                return Ok(Value::list(
                    self.pos.get().iter().map(|v| Value::Float(*v)).collect(),
                ))
            }
            "fov" => self.fov.get(),
            _ => return Err(err("FieldError", format!("a Camera has no field `{name}`"))),
        }))
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let a = Args::new(name, pos, named);
        match name {
            // `cam.move_to(x, y, z)`
            "move_to" => self
                .pos
                .set([a.num(0, "x")?, a.num(1, "y")?, a.num(2, "z")?]),
            // `cam.move(dx, dy, dz)`: in the world's axes.
            "move" => {
                let p = self.pos.get();
                self.pos.set([
                    p[0] + a.num_or(0, "dx", 0.0)?,
                    p[1] + a.num_or(1, "dy", 0.0)?,
                    p[2] + a.num_or(2, "dz", 0.0)?,
                ]);
            }
            // `cam.forward(d)`: along where it looks, on the ground plane.
            "forward" => {
                let d = a.num(0, "distance")?;
                let (s, c) = self.yaw.get().to_radians().sin_cos();
                let p = self.pos.get();
                self.pos.set([p[0] + d * s, p[1], p[2] + d * c]);
            }
            // `cam.strafe(d)`: sideways, to the right for positive `d`.
            "strafe" => {
                let d = a.num(0, "distance")?;
                let (s, c) = self.yaw.get().to_radians().sin_cos();
                let p = self.pos.get();
                self.pos.set([p[0] + d * c, p[1], p[2] - d * s]);
            }
            // `cam.fly(d)`: along exactly where it looks, up or down too.
            "fly" => {
                let d = a.num(0, "distance")?;
                let v = self.direction();
                let p = self.pos.get();
                self.pos
                    .set([p[0] + d * v[0], p[1] + d * v[1], p[2] + d * v[2]]);
            }
            "turn" => {
                self.yaw.set(self.yaw.get() + a.num_or(0, "yaw", 0.0)?);
                self.pitch
                    .set((self.pitch.get() + a.num_or(1, "pitch", 0.0)?).clamp(-89.0, 89.0));
            }
            "look_at" => {
                let p = self.pos.get();
                let (dx, dy, dz) = (
                    a.num(0, "x")? - p[0],
                    a.num(1, "y")? - p[1],
                    a.num(2, "z")? - p[2],
                );
                self.yaw.set(dx.atan2(dz).to_degrees());
                // Positive pitch looks up.
                self.pitch
                    .set(dy.atan2((dx * dx + dz * dz).sqrt()).to_degrees());
            }
            "set_fov" => self.fov.set(a.num(0, "degrees")?.clamp(10.0, 170.0)),
            _ => {
                return Err(err(
                    "FieldError",
                    format!("a Camera has no method `{name}`"),
                ))
            }
        }
        Ok(Value::None)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// Points and the triangles between them, with optional texture
/// coordinates and normals per point, and a texture.
pub struct Mesh {
    verts: Vec<[f64; 3]>,
    faces: Vec<[usize; 3]>,
    /// Texture coordinates per point (from the top left), or empty.
    uvs: Vec<[f64; 2]>,
    /// The way the surface faces at each point, for smooth shading; empty
    /// for flat triangles.
    normals: Vec<[f64; 3]>,
    /// An `Image` to paint it with.
    texture: Option<Value>,
}

impl Mesh {
    fn new(verts: Vec<[f64; 3]>, faces: Vec<[usize; 3]>) -> Mesh {
        Mesh {
            verts,
            faces,
            uvs: Vec::new(),
            normals: Vec::new(),
            texture: None,
        }
    }

    /// Smooth normals: each point faces the average way of its triangles.
    fn smooth(mut self) -> Mesh {
        let mut normals = vec![[0.0; 3]; self.verts.len()];
        for f in &self.faces {
            let (a, b, c) = (self.verts[f[0]], self.verts[f[1]], self.verts[f[2]]);
            let n = cross(
                [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
                [c[0] - a[0], c[1] - a[1], c[2] - a[2]],
            );
            for &i in f {
                for k in 0..3 {
                    normals[i][k] += n[k];
                }
            }
        }
        self.normals = normals.into_iter().map(normalize).collect();
        self
    }
}

fn cross(u: [f64; 3], v: [f64; 3]) -> [f64; 3] {
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

impl Foreign for Mesh {
    fn type_name(&self) -> &'static str {
        "Mesh"
    }

    fn display(&self) -> String {
        format!(
            "Mesh({} points, {} triangles)",
            self.verts.len(),
            self.faces.len()
        )
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "points" => Ok(Value::Int(self.verts.len() as i64)),
            "triangles" => Ok(Value::Int(self.faces.len() as i64)),
            "texture" => Ok(self.texture.clone().unwrap_or(Value::None)),
            "smooth" => Ok(Value::Bool(!self.normals.is_empty())),
            _ => Err(err("FieldError", format!("a Mesh has no field `{name}`"))),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

fn cube_mesh() -> Mesh {
    // Four corners per side, so each side has its own texture coordinates.
    // Corners of each side in order (u, v) = (0,0) (1,0) (1,1) (0,1).
    let sides: [[[f64; 3]; 4]; 6] = [
        [[0., 1., 0.], [1., 1., 0.], [1., 0., 0.], [0., 0., 0.]], // front (z = 0)
        [[1., 1., 1.], [0., 1., 1.], [0., 0., 1.], [1., 0., 1.]], // back
        [[0., 1., 1.], [0., 1., 0.], [0., 0., 0.], [0., 0., 1.]], // left
        [[1., 1., 0.], [1., 1., 1.], [1., 0., 1.], [1., 0., 0.]], // right
        [[0., 1., 1.], [1., 1., 1.], [1., 1., 0.], [0., 1., 0.]], // top
        [[0., 0., 0.], [1., 0., 0.], [1., 0., 1.], [0., 0., 1.]], // bottom
    ];
    let mut m = Mesh::new(Vec::new(), Vec::new());
    for side in sides {
        let at = m.verts.len();
        for (k, c) in side.iter().enumerate() {
            m.verts.push([c[0] - 0.5, c[1] - 0.5, c[2] - 0.5]);
            m.uvs
                .push([[0.0, 1.0, 1.0, 0.0][k], [0.0, 0.0, 1.0, 1.0][k]]);
        }
        m.faces.push([at, at + 1, at + 2]);
        m.faces.push([at, at + 2, at + 3]);
    }
    m
}

fn sphere_mesh(detail: usize) -> Mesh {
    let (rings, segs) = (detail.clamp(3, 64), (detail * 2).clamp(6, 128));
    let mut verts = Vec::new();
    for r in 0..=rings {
        let phi = std::f64::consts::PI * r as f64 / rings as f64;
        for s in 0..segs {
            let th = 2.0 * std::f64::consts::PI * s as f64 / segs as f64;
            verts.push([
                0.5 * phi.sin() * th.cos(),
                0.5 * phi.cos(),
                0.5 * phi.sin() * th.sin(),
            ]);
        }
    }
    let mut faces = Vec::new();
    for r in 0..rings {
        for s in 0..segs {
            let (a, b) = (r * segs + s, r * segs + (s + 1) % segs);
            let (c, d) = (a + segs, b + segs);
            faces.push([a, b, d]);
            faces.push([a, d, c]);
        }
    }
    let uvs = (0..=rings)
        .flat_map(|r| (0..segs).map(move |s| [s as f64 / segs as f64, r as f64 / rings as f64]))
        .collect();
    let normals = verts.iter().map(|v| normalize(*v)).collect();
    Mesh {
        verts,
        faces,
        uvs,
        normals,
        texture: None,
    }
}

fn plane_mesh() -> Mesh {
    let mut m = Mesh::new(
        vec![
            [-0.5, 0.0, -0.5],
            [0.5, 0.0, -0.5],
            [0.5, 0.0, 0.5],
            [-0.5, 0.0, 0.5],
        ],
        vec![[0, 2, 1], [0, 3, 2]],
    );
    m.uvs = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
    m
}

/// Draws a mesh placed at `pos`, turned by `rot` (degrees about x, y, z)
/// and scaled, with one shade per triangle from a light above and behind
/// the camera.
/// Nearest distance in front of the camera that is drawn.
const NEAR: f64 = 0.06;

/// An `Image` argument (`texture = img`), if given.
fn image_arg(v: Option<&Value>) -> VmResult<Option<*const Image>> {
    match v {
        None | Some(Value::None) => Ok(None),
        Some(Value::Foreign(f)) => match f.as_any().downcast_ref::<Image>() {
            Some(img) => Ok(Some(img as *const Image)),
            None => Err(type_error(
                "`texture` takes an image, from `draw.image(path)` or `canvas.image()`",
            )),
        },
        Some(other) => Err(type_error(format!(
            "`texture` takes an image, from `draw.image(path)` or `canvas.image()`, not {}",
            other.type_name()
        ))),
    }
}

/// Repeats a cube's texture once per unit of its size on each side.
fn tile_cube(cube: &mut Mesh, size: [f64; 3]) {
    // Sides in cube_mesh order: front, back, left, right, top, bottom; how
    // many units wide and high each is.
    let reps = [
        (size[0], size[1]),
        (size[0], size[1]),
        (size[2], size[1]),
        (size[2], size[1]),
        (size[0], size[2]),
        (size[0], size[2]),
    ];
    for (side, (w, h)) in reps.iter().enumerate() {
        for k in 0..4 {
            let uv = &mut cube.uvs[side * 4 + k];
            uv[0] *= w.abs().max(1e-6);
            uv[1] *= h.abs().max(1e-6);
        }
    }
}

/// A triangle corner on its way to the screen: in camera space, with its
/// colour (0..255 and alpha) and texture coordinates.
#[derive(Clone, Copy)]
struct Corner {
    v: [f64; 3],
    c: [f64; 4],
    uv: [f64; 2],
}

/// The part of a triangle (in camera space) in front of the near plane:
/// none, three or four corners.
fn clip_near(tri: [Corner; 3]) -> Vec<Corner> {
    let mut out = Vec::with_capacity(4);
    for i in 0..3 {
        let a = tri[i];
        let b = tri[(i + 1) % 3];
        let (ina, inb) = (a.v[2] >= NEAR, b.v[2] >= NEAR);
        if ina {
            out.push(a);
        }
        if ina != inb {
            let t = (NEAR - a.v[2]) / (b.v[2] - a.v[2]);
            let lerp = |x: f64, y: f64| x + (y - x) * t;
            out.push(Corner {
                v: [lerp(a.v[0], b.v[0]), lerp(a.v[1], b.v[1]), NEAR],
                c: [
                    lerp(a.c[0], b.c[0]),
                    lerp(a.c[1], b.c[1]),
                    lerp(a.c[2], b.c[2]),
                    lerp(a.c[3], b.c[3]),
                ],
                uv: [lerp(a.uv[0], b.uv[0]), lerp(a.uv[1], b.uv[1])],
            });
        }
    }
    out
}

/// Draws a mesh placed at `pos`, turned by `rot` (degrees about x, y, z)
/// and scaled, lit by the surface's lights, and painted with `tex` (or
/// the mesh's own texture) when it has texture coordinates.
#[allow(clippy::too_many_arguments)]
fn draw_mesh(
    px: &mut Pixels,
    cam: &Camera,
    m: &Mesh,
    pos: [f64; 3],
    rot: [f64; 3],
    scale: [f64; 3],
    c: Color,
    tex: Option<&Image>,
) {
    let (sx, cx) = rot[0].to_radians().sin_cos();
    let (sy, cy) = rot[1].to_radians().sin_cos();
    let (sz, cz) = rot[2].to_radians().sin_cos();
    let turn = |x: f64, y: f64, z: f64| {
        let (y, z) = (y * cx - z * sx, y * sx + z * cx);
        let (x, z) = (x * cy + z * sy, -x * sy + z * cy);
        let (x, y) = (x * cz - y * sz, x * sz + y * cz);
        [x, y, z]
    };
    let world: Vec<[f64; 3]> = m
        .verts
        .iter()
        .map(|v| {
            let w = turn(v[0] * scale[0], v[1] * scale[1], v[2] * scale[2]);
            [w[0] + pos[0], w[1] + pos[1], w[2] + pos[2]]
        })
        .collect();
    // Normals turn with the mesh; stretching divides them by the scale.
    let normals: Vec<[f64; 3]> = m
        .normals
        .iter()
        .map(|n| {
            let safe = |s: f64| if s == 0.0 { 1.0 } else { s };
            normalize(turn(
                n[0] / safe(scale[0]),
                n[1] / safe(scale[1]),
                n[2] / safe(scale[2]),
            ))
        })
        .collect();
    let mesh_texture = m.texture.as_ref().and_then(|v| match v {
        Value::Foreign(f) => f
            .as_any()
            .downcast_ref::<Image>()
            .map(|i| i as *const Image),
        _ => None,
    });
    // SAFETY: the mesh, which holds the image, outlives the call.
    let tex = tex.or_else(|| mesh_texture.map(|p| unsafe { &*p }));
    let tex = if m.uvs.len() == m.verts.len() {
        tex
    } else {
        None
    };
    let lights = px.lights.clone();
    let eye = cam.pos.get();
    for f in &m.faces {
        let (a, b, d) = (world[f[0]], world[f[1]], world[f[2]]);
        let face = normalize(cross(
            [b[0] - a[0], b[1] - a[1], b[2] - a[2]],
            [d[0] - a[0], d[1] - a[1], d[2] - a[2]],
        ));
        let corner = |k: usize, p: [f64; 3]| {
            let n = if normals.is_empty() {
                face
            } else {
                normals[f[k]]
            };
            Corner {
                v: cam.view(p),
                c: lights.shade(c, p, n, eye),
                uv: if tex.is_some() { m.uvs[f[k]] } else { [0.0; 2] },
            }
        };
        // Cut away the part behind the camera, so a wall right in front
        // of you is drawn instead of vanishing.
        let visible = clip_near([corner(0, a), corner(1, b), corner(2, d)]);
        let pts: Option<Vec<(f64, f64, f64)>> = visible
            .iter()
            .map(|k| cam.project(k.v, px.w, px.h))
            .collect();
        if let Some(p) = pts {
            for k in 1..p.len().saturating_sub(1) {
                let (i, j) = (k, k + 1);
                px.triangle_shaded(
                    [p[0], p[i], p[j]],
                    [visible[0].c, visible[i].c, visible[j].c],
                    [visible[0].uv, visible[i].uv, visible[j].uv],
                    tex,
                );
            }
        }
    }
}

// ==== surfaces: canvas and window ===========================================

/// A drawing surface without a window.
pub struct Canvas {
    pub px: Rc<RefCell<Pixels>>,
}

/// Drawing that canvases and windows share. `None`: not a drawing method.
fn surface_method(
    px: &Rc<RefCell<Pixels>>,
    name: &str,
    pos: &[Value],
    named: &[(String, Value)],
) -> Option<VmResult<Value>> {
    let a = Args::new(name, pos, named);
    let r = (|| -> VmResult<Value> {
        let mut p = px.borrow_mut();
        match name {
            "clear" => p.clear(a.color_or(0, "color", 0)?),
            "pixel" => {
                let c = a.color_or(2, "color", 0xffffff)?;
                p.put(
                    a.num(0, "x")?.round() as i64,
                    a.num(1, "y")?.round() as i64,
                    c,
                )
            }
            "get_pixel" => {
                let (x, y) = (a.num(0, "x")? as i64, a.num(1, "y")? as i64);
                if x < 0 || y < 0 || x >= p.w as i64 || y >= p.h as i64 {
                    return Ok(Value::None);
                }
                p.sync();
                return Ok(Value::str(color_text(p.buf[y as usize * p.w + x as usize])));
            }
            "line" => {
                let c = a.color_or(4, "color", 0xffffff)?;
                let w = a.num_or(5, "width", 1.0)?;
                p.line(
                    a.num(0, "x1")?,
                    a.num(1, "y1")?,
                    a.num(2, "x2")?,
                    a.num(3, "y2")?,
                    c,
                    w,
                )
            }
            "rect" => {
                let c = a.color_or(4, "color", 0xffffff)?;
                let fill = a.flag_or(5, "fill", true);
                let w = a.num_or(6, "width", 1.0)?;
                p.rect(
                    a.num(0, "x")?,
                    a.num(1, "y")?,
                    a.num(2, "w")?,
                    a.num(3, "h")?,
                    c,
                    fill,
                    w,
                )
            }
            "circle" => {
                let c = a.color_or(3, "color", 0xffffff)?;
                let fill = a.flag_or(4, "fill", true);
                let w = a.num_or(5, "width", 1.0)?;
                p.circle(a.num(0, "x")?, a.num(1, "y")?, a.num(2, "r")?, c, fill, w)
            }
            "triangle" => {
                let c = a.color_or(6, "color", 0xffffff)?;
                let pts = [
                    (a.num(0, "x1")?, a.num(1, "y1")?),
                    (a.num(2, "x2")?, a.num(3, "y2")?),
                    (a.num(4, "x3")?, a.num(5, "y3")?),
                ];
                p.polygon(&pts, c)
            }
            "polygon" => {
                let c = a.color_or(1, "color", 0xffffff)?;
                let fill = a.flag_or(2, "fill", true);
                let points = ops::collect(a.get(0, "points").unwrap_or(&Value::None))?;
                let pts: Vec<(f64, f64)> = points
                    .iter()
                    .map(|pt| {
                        let xy = ops::collect(pt)?;
                        match xy.as_slice() {
                            [x, y] => Ok((number(x, "a point's x")?, number(y, "a point's y")?)),
                            _ => Err(type_error("each point is [x, y]")),
                        }
                    })
                    .collect::<VmResult<_>>()?;
                if fill {
                    p.polygon(&pts, c);
                } else {
                    for i in 0..pts.len() {
                        let (s, e) = (pts[i], pts[(i + 1) % pts.len()]);
                        p.line(s.0, s.1, e.0, e.1, c, 1.0);
                    }
                }
            }
            "text" => {
                let t = match a.get(0, "text") {
                    Some(Value::Str(s)) => s.to_string(),
                    Some(other) => super::stdlib::quote(other.type_name()),
                    None => a.text(0, "text")?,
                };
                let c = a.color_or(3, "color", 0xffffff)?;
                let size = a.num_or(4, "size", 2.0)?;
                p.text(&t, a.num(1, "x")?, a.num(2, "y")?, c, size)
            }
            "image" => {
                let img = match a.get(0, "image") {
                    Some(Value::Foreign(f)) => f
                        .as_any()
                        .downcast_ref::<Image>()
                        .map(|i| i as *const Image),
                    _ => None,
                }
                .ok_or_else(|| type_error("`image` takes an image from `draw.image(path)`"))?;
                let scale = a.num_or(3, "scale", 1.0)?;
                // SAFETY: the image is held by the argument for the call.
                p.image(unsafe { &*img }, a.num(1, "x")?, a.num(2, "y")?, scale)
            }
            "save" => {
                let path = a.text(0, "path")?;
                p.sync();
                p.save_png(&path)?
            }
            // `img = canvas.to_image()`: what is drawn so far, as an image,
            // for example to paint 3D shapes with (`texture = img`).
            "to_image" => {
                p.sync();
                let px = p.buf.iter().map(|c| (*c, 255u8)).collect();
                return Ok(wrap(Image::new(p.w, p.h, px)));
            }
            // `win.light(direction = [x, y, z], color = "white", strength = 0.65)`:
            // the sun, shining from `direction` (towards the light).
            "light" => {
                let dir = match a.get(0, "direction") {
                    Some(v) => {
                        let d = ops::collect(v)?;
                        let [x, y, z] = d.as_slice() else {
                            return Err(type_error("`direction` is [x, y, z]"));
                        };
                        normalize([number(x, "x")?, number(y, "y")?, number(z, "z")?])
                    }
                    None => p.lights.sun,
                };
                let color = rgb_unit(a.color_or(1, "color", 0xffffff)?.rgb);
                let strength = a.num_or(2, "strength", 0.65)?;
                p.lights.sun = dir;
                p.lights.sun_color = [
                    color[0] * strength,
                    color[1] * strength,
                    color[2] * strength,
                ];
            }
            // `win.ambient(0.35, color = "white")`: light that reaches everywhere.
            "ambient" => {
                let level = a.num_or(0, "level", 0.35)?;
                let color = rgb_unit(a.color_or(1, "color", 0xffffff)?.rgb);
                p.lights.ambient = [color[0] * level, color[1] * level, color[2] * level];
            }
            // `win.point_light(x, y, z, color = "orange", strength = 1, range = 8)`:
            // a lamp for this frame (`clear` turns lamps off).
            "point_light" => {
                let at = [a.num(0, "x")?, a.num(1, "y")?, a.num(2, "z")?];
                let color = rgb_unit(a.color_or(3, "color", 0xffffff)?.rgb);
                let strength = a.num_or(4, "strength", 1.0)?;
                let range = a.num_or(5, "range", 8.0)?.max(0.001);
                if p.lights.points.len() >= 64 {
                    return Err(err(
                        "ValueError",
                        "a scene can have at most 64 point lights",
                    ));
                }
                p.lights.points.push((
                    at,
                    [
                        color[0] * strength,
                        color[1] * strength,
                        color[2] * strength,
                    ],
                    range,
                ));
            }
            // `win.fog("#9fb4c7", start = 10, end = 40)`, or `win.fog(none)`.
            "fog" => match a.get(0, "color") {
                None | Some(Value::None) => p.lights.fog = None,
                Some(_) => {
                    let color = rgb_unit(a.color_or(0, "color", 0xffffff)?.rgb);
                    let start = a.num_or(1, "start", 10.0)?;
                    let end = a.num_or(2, "end", 40.0)?;
                    p.lights.fog = Some((color, start, end));
                }
            },
            // `win.box(cam, [x, y, z, width, height, depth], colour)`: the
            // same box lists `vyrn.game` collides and ray-casts with.
            "box" => {
                let cam = match a.get(0, "camera") {
                    Some(Value::Foreign(f)) => f
                        .as_any()
                        .downcast_ref::<Camera>()
                        .map(|c| c as *const Camera),
                    _ => None,
                }
                .ok_or_else(|| type_error("`box` takes a camera first, from `draw.camera()`"))?;
                // SAFETY: held by the argument for the call.
                let cam = unsafe { &*cam };
                let b = ops::collect(a.get(1, "box").unwrap_or(&Value::None))?;
                if b.len() != 6 {
                    return Err(type_error("a box is [x, y, z, width, height, depth]"));
                }
                let n: Vec<f64> = b
                    .iter()
                    .map(|v| number(v, "a box"))
                    .collect::<VmResult<_>>()?;
                let c = a.color_or(2, "color", 0xffffff)?;
                let tex = image_arg(a.get(3, "texture"))?;
                // SAFETY: held by the argument for the call.
                let tex = tex.map(|t| unsafe { &*t });
                // A texture repeats once per unit, so a long wall shows
                // many bricks rather than one stretched.
                let mut cube = cube_mesh();
                if tex.is_some() {
                    tile_cube(&mut cube, [n[3], n[4], n[5]]);
                }
                draw_mesh(
                    &mut p,
                    cam,
                    &cube,
                    [n[0], n[1], n[2]],
                    [0.0; 3],
                    [n[3], n[4], n[5]],
                    c,
                    tex,
                )
            }
            "cube" | "sphere" | "plane" | "mesh" => {
                let cam = match a.get(0, "camera") {
                    Some(Value::Foreign(f)) => f
                        .as_any()
                        .downcast_ref::<Camera>()
                        .map(|c| c as *const Camera),
                    _ => None,
                }
                .ok_or_else(|| {
                    type_error(format!(
                        "`{name}` takes a camera first, from `draw.camera()`"
                    ))
                })?;
                // SAFETY: held by the argument for the call.
                let cam = unsafe { &*cam };
                let (mesh_owned, at) = match name {
                    "cube" => (Some(cube_mesh()), 1),
                    "sphere" => (Some(sphere_mesh(a.num_or(7, "detail", 12.0)? as usize)), 1),
                    "plane" => (Some(plane_mesh()), 1),
                    _ => (None, 2),
                };
                let mesh_ref: &Mesh = match &mesh_owned {
                    Some(m) => m,
                    None => match a.get(1, "mesh") {
                        Some(Value::Foreign(f)) => {
                            f.as_any().downcast_ref::<Mesh>().ok_or_else(|| {
                                type_error("`mesh` takes a mesh from `draw.mesh(...)`")
                            })?
                        }
                        _ => return Err(type_error("`mesh` takes a mesh from `draw.mesh(...)`")),
                    },
                };
                let position = [a.num(at, "x")?, a.num(at + 1, "y")?, a.num(at + 2, "z")?];
                // One number, or [width, height, depth].
                let size = match a.get(at + 3, "size") {
                    Some(v @ (Value::List(_) | Value::Tuple(_))) => {
                        let s = ops::collect(v)?;
                        if s.len() != 3 {
                            return Err(type_error("`size` is a number or [width, height, depth]"));
                        }
                        [
                            number(&s[0], "size")?,
                            number(&s[1], "size")?,
                            number(&s[2], "size")?,
                        ]
                    }
                    _ => [a.num_or(at + 3, "size", 1.0)?; 3],
                };
                let c = a.color_or(at + 4, "color", 0xffffff)?;
                let rot = match a.get(at + 5, "rotation") {
                    Some(v) => {
                        let r = ops::collect(v)?;
                        if r.len() != 3 {
                            return Err(type_error("`rotation` is [x, y, z] in degrees"));
                        }
                        [
                            number(&r[0], "rotation")?,
                            number(&r[1], "rotation")?,
                            number(&r[2], "rotation")?,
                        ]
                    }
                    None => [0.0; 3],
                };
                let tex = image_arg(a.get(at + 6, "texture"))?;
                // SAFETY: held by the argument for the call.
                let tex = tex.map(|t| unsafe { &*t });
                draw_mesh(&mut p, cam, mesh_ref, position, rot, size, c, tex)
            }
            "line3d" => {
                let cam = match a.get(0, "camera") {
                    Some(Value::Foreign(f)) => f
                        .as_any()
                        .downcast_ref::<Camera>()
                        .map(|c| c as *const Camera),
                    _ => None,
                }
                .ok_or_else(|| type_error("`line3d` takes a camera first"))?;
                // SAFETY: held by the argument for the call.
                let cam = unsafe { &*cam };
                let pt = |v: Option<&Value>| -> VmResult<[f64; 3]> {
                    let r = ops::collect(v.unwrap_or(&Value::None))?;
                    match r.as_slice() {
                        [x, y, z] => Ok([number(x, "x")?, number(y, "y")?, number(z, "z")?]),
                        _ => Err(type_error("a 3D point is [x, y, z]")),
                    }
                };
                let (s, e) = (pt(a.get(1, "start"))?, pt(a.get(2, "end"))?);
                let c = a.color_or(3, "color", 0xffffff)?;
                let (w, h) = (p.w, p.h);
                if let (Some(s), Some(e)) = (
                    cam.project(cam.view(s), w, h),
                    cam.project(cam.view(e), w, h),
                ) {
                    p.line(s.0, s.1, e.0, e.1, c, 1.0);
                }
            }
            _ => return Err(err("NotADrawingMethod", "")),
        }
        Ok(Value::None)
    })();
    match r {
        Err(Value::Error(e)) if e.kind == "NotADrawingMethod" => None,
        other => Some(other),
    }
}

fn surface_attr(px: &Rc<RefCell<Pixels>>, name: &str) -> Option<Value> {
    let p = px.borrow();
    Some(match name {
        "width" => Value::Int(p.w as i64),
        "height" => Value::Int(p.h as i64),
        _ => return None,
    })
}

impl Foreign for Canvas {
    fn type_name(&self) -> &'static str {
        "Canvas"
    }

    fn display(&self) -> String {
        let p = self.px.borrow();
        format!("Canvas({}x{})", p.w, p.h)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        surface_attr(&self.px, name)
            .ok_or_else(|| err("FieldError", format!("a Canvas has no field `{name}`")))
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        surface_method(&self.px, name, pos, named).unwrap_or_else(|| {
            Err(err(
                "FieldError",
                format!("a Canvas has no method `{name}`"),
            ))
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== widgets ===============================================================

/// Buttons, text fields and checkboxes, drawn each frame where they are
/// asked for (immediate mode): `if win.button("Go", 10, 10): ...`. The
/// window gives this the mouse and the typed text at the start of each
/// frame; the widgets only read them, so they can be tested without one.
#[derive(Default)]
pub struct Ui {
    mouse: (f64, f64),
    down: bool,
    was_down: bool,
    /// Characters typed during the last frame; `\u{8}` for Backspace.
    typed: String,
    fields: std::collections::HashMap<String, String>,
    focus: Option<String>,
    /// Where the next widget goes when it is given no position.
    flow: Option<Flow>,
    /// The slider being dragged.
    dragging: Option<String>,
}

/// The layout cursor: widgets without a position go one under another, or
/// side by side inside `win.row()` ... `win.end_row()`.
#[derive(Clone, Copy)]
struct Flow {
    left: f64,
    x: f64,
    y: f64,
    gap: f64,
    in_row: bool,
    row_height: f64,
}

impl Flow {
    fn at(x: f64, y: f64, gap: f64) -> Flow {
        Flow {
            left: x,
            x,
            y,
            gap,
            in_row: false,
            row_height: 0.0,
        }
    }
}

const BUTTON: u32 = 0x3949ab;
const BUTTON_HOVER: u32 = 0x5c6bc0;
const FIELD: u32 = 0xffffff;
const FIELD_EDGE: u32 = 0x9e9e9e;
const FOCUS_EDGE: u32 = 0x3949ab;
const INK: u32 = 0x212121;

impl Ui {
    /// The input for the frame about to be drawn.
    pub fn next_frame(&mut self, mouse: (f64, f64), down: bool, typed: String) {
        self.was_down = self.down;
        self.down = down;
        self.mouse = mouse;
        self.typed = typed;
        // Each frame lays out from the top again.
        self.flow = None;
        if !down {
            self.dragging = None;
        }
    }

    /// Starts laying out widgets at (x, y), `gap` pixels apart.
    pub fn layout(&mut self, x: f64, y: f64, gap: f64) {
        self.flow = Some(Flow::at(x, y, gap));
    }

    fn flow(&mut self) -> &mut Flow {
        self.flow.get_or_insert(Flow::at(16.0, 16.0, 10.0))
    }

    /// From here to `end_row`, widgets go side by side.
    pub fn row(&mut self) {
        let f = self.flow();
        if !f.in_row {
            f.in_row = true;
            f.row_height = 0.0;
        }
    }

    pub fn end_row(&mut self) {
        let f = self.flow();
        if f.in_row {
            f.in_row = false;
            f.y += f.row_height + f.gap;
            f.x = f.left;
        }
    }

    /// Empty space: down in a column, across in a row.
    pub fn space(&mut self, pixels: f64) {
        let f = self.flow();
        if f.in_row {
            f.x += pixels;
        } else {
            f.y += pixels;
        }
    }

    /// The position for a widget of this size, moving the cursor past it.
    pub fn place(&mut self, w: f64, h: f64) -> (f64, f64) {
        let f = self.flow();
        let at = (f.x, f.y);
        if f.in_row {
            f.x += w + f.gap;
            f.row_height = f.row_height.max(h);
        } else {
            f.y += h + f.gap;
        }
        at
    }

    /// A slider named `name` from `low` to `high`; gives its value, which
    /// follows the mouse while it is dragged.
    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        &mut self,
        px: &mut Pixels,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        low: f64,
        high: f64,
        value: f64,
    ) -> f64 {
        let h = 24.0;
        if self.down && !self.was_down && self.inside(x, y, w, h) {
            self.dragging = Some(name.to_string());
        }
        let mut value = value;
        if self.down && self.dragging.as_deref() == Some(name) && w > 0.0 {
            let t = ((self.mouse.0 - x) / w).clamp(0.0, 1.0);
            value = low + t * (high - low);
        }
        let t = if high == low {
            0.0
        } else {
            ((value - low) / (high - low)).clamp(0.0, 1.0)
        };
        px.rect(
            x,
            y + h / 2.0 - 3.0,
            w,
            6.0,
            Color::opaque(FIELD_EDGE),
            true,
            1.0,
        );
        px.rect(
            x,
            y + h / 2.0 - 3.0,
            w * t,
            6.0,
            Color::opaque(BUTTON),
            true,
            1.0,
        );
        let knob = x + w * t;
        px.circle(
            knob,
            y + h / 2.0,
            9.0,
            Color::opaque(BUTTON_HOVER),
            true,
            1.0,
        );
        value
    }

    /// The mouse button went down this frame.
    pub fn clicked(&self) -> bool {
        self.down && !self.was_down
    }

    fn inside(&self, x: f64, y: f64, w: f64, h: f64) -> bool {
        let (mx, my) = self.mouse;
        mx >= x && mx < x + w && my >= y && my < y + h
    }

    /// Draws a button; true on the frame it is clicked.
    pub fn button(
        &mut self,
        px: &mut Pixels,
        label: &str,
        x: f64,
        y: f64,
        w: Option<f64>,
        h: f64,
    ) -> bool {
        let w = w.unwrap_or(16.0 * label.chars().count() as f64 + 24.0);
        let hover = self.inside(x, y, w, h);
        px.rect(
            x,
            y,
            w,
            h,
            Color::opaque(if hover { BUTTON_HOVER } else { BUTTON }),
            true,
            1.0,
        );
        let tw = 16.0 * label.chars().count() as f64;
        px.text(
            label,
            x + (w - tw) / 2.0,
            y + (h - 16.0) / 2.0,
            Color::opaque(0xffffff),
            2.0,
        );
        hover && self.clicked()
    }

    /// Draws a one-line text field named `name` and gives its text. Click
    /// it to type into it; Enter or a click elsewhere leaves it.
    pub fn input(
        &mut self,
        px: &mut Pixels,
        name: &str,
        x: f64,
        y: f64,
        w: f64,
        hint: &str,
    ) -> String {
        let h = 28.0;
        // Only a field focused before this frame takes this frame's typing.
        let was_focused = self.focus.as_deref() == Some(name);
        if self.clicked() {
            if self.inside(x, y, w, h) {
                self.focus = Some(name.to_string());
            } else if self.focus.as_deref() == Some(name) {
                self.focus = None;
            }
        }
        let focused = self.focus.as_deref() == Some(name);
        let text = self.fields.entry(name.to_string()).or_default();
        if focused && was_focused {
            for c in self.typed.chars() {
                match c {
                    '\u{8}' => {
                        text.pop();
                    }
                    '\n' | '\r' => self.focus = None,
                    c if !c.is_control() => text.push(c),
                    _ => {}
                }
            }
        }
        let text = text.clone();
        px.rect(x, y, w, h, Color::opaque(FIELD), true, 1.0);
        px.rect(
            x,
            y,
            w,
            h,
            Color::opaque(if focused { FOCUS_EDGE } else { FIELD_EDGE }),
            false,
            if focused { 2.0 } else { 1.0 },
        );
        let room = ((w - 12.0) / 16.0).max(1.0) as usize;
        let shown: String = if text.is_empty() && !focused {
            hint.chars().take(room).collect()
        } else {
            let n = text.chars().count();
            text.chars()
                .skip(n.saturating_sub(room - usize::from(focused)))
                .collect()
        };
        let ink = if text.is_empty() && !focused {
            FIELD_EDGE
        } else {
            INK
        };
        px.text(&shown, x + 6.0, y + 6.0, Color::opaque(ink), 2.0);
        if focused {
            let cx = x + 6.0 + 16.0 * shown.chars().count() as f64;
            px.rect(cx, y + 5.0, 2.0, 18.0, Color::opaque(INK), true, 1.0);
        }
        text
    }

    /// Sets a field's text (to clear it, or fill it in).
    pub fn set_field(&mut self, name: &str, text: &str) {
        self.fields.insert(name.to_string(), text.to_string());
    }

    /// Draws a checkbox; gives the new state (flipped when clicked).
    pub fn checkbox(
        &mut self,
        px: &mut Pixels,
        label: &str,
        x: f64,
        y: f64,
        checked: bool,
    ) -> bool {
        let size = 20.0;
        let w = size + 8.0 + 16.0 * label.chars().count() as f64;
        let now = if self.inside(x, y, w, size) && self.clicked() {
            !checked
        } else {
            checked
        };
        px.rect(x, y, size, size, Color::opaque(FIELD), true, 1.0);
        px.rect(x, y, size, size, Color::opaque(FIELD_EDGE), false, 2.0);
        if now {
            px.rect(
                x + 5.0,
                y + 5.0,
                size - 10.0,
                size - 10.0,
                Color::opaque(BUTTON),
                true,
                1.0,
            );
        }
        px.text(label, x + size + 8.0, y + 2.0, Color::opaque(0xffffff), 2.0);
        now
    }
}

/// Collects typed characters from the window.
struct Typed(Rc<RefCell<String>>);

impl minifb::InputCallback for Typed {
    fn add_char(&mut self, c: u32) {
        if let Some(ch) = char::from_u32(c) {
            self.0.borrow_mut().push(ch);
        }
    }
}

/// A window on the screen, with keyboard, mouse and widgets.
pub struct Window {
    px: Rc<RefCell<Pixels>>,
    win: RefCell<minifb::Window>,
    last: Cell<std::time::Instant>,
    ui: RefCell<Ui>,
    typed: Rc<RefCell<String>>,
    /// Where the mouse was last frame, and how far it moved since.
    last_mouse: Cell<Option<(f64, f64)>>,
    mouse_delta: Cell<(f64, f64)>,
}

impl Window {
    /// Reads the mouse and what was typed, for the frame about to start.
    /// A widget's position: x and y if given (at `i` and `i + 1`),
    /// otherwise the next place in the layout.
    fn position(&self, a: &Args<'_>, i: usize, w: f64, h: f64) -> VmResult<(f64, f64)> {
        if a.get(i, "x").is_some() {
            return Ok((a.num(i, "x")?, a.num(i + 1, "y")?));
        }
        Ok(self.ui.borrow_mut().place(w, h))
    }

    fn poll(&self) {
        let w = self.win.borrow();
        let mouse = w
            .get_mouse_pos(minifb::MouseMode::Clamp)
            .map_or((-1.0, -1.0), |(x, y)| (f64::from(x), f64::from(y)));
        let moved = match self.last_mouse.get() {
            Some((x, y)) if mouse.0 >= 0.0 => (mouse.0 - x, mouse.1 - y),
            _ => (0.0, 0.0),
        };
        self.mouse_delta.set(moved);
        if mouse.0 >= 0.0 {
            self.last_mouse.set(Some(mouse));
        }
        let down = w.get_mouse_down(minifb::MouseButton::Left);
        let mut typed = std::mem::take(&mut *self.typed.borrow_mut());
        for k in w.get_keys_pressed(minifb::KeyRepeat::Yes) {
            match k {
                minifb::Key::Backspace => typed.push('\u{8}'),
                minifb::Key::Enter | minifb::Key::NumPadEnter => typed.push('\n'),
                _ => {}
            }
        }
        // Control characters arrive by key above, not as text.
        let typed: String = typed
            .chars()
            .filter(|c| !c.is_control() || matches!(c, '\u{8}' | '\n'))
            .collect();
        self.ui.borrow_mut().next_frame(mouse, down, typed);
    }
}

fn key(name: &str) -> Option<minifb::Key> {
    use minifb::Key as K;
    let lower = name.to_lowercase();
    Some(match lower.as_str() {
        "left" => K::Left,
        "right" => K::Right,
        "up" => K::Up,
        "down" => K::Down,
        "space" => K::Space,
        "enter" | "return" => K::Enter,
        "escape" | "esc" => K::Escape,
        "shift" => K::LeftShift,
        "ctrl" | "control" => K::LeftCtrl,
        "alt" => K::LeftAlt,
        "tab" => K::Tab,
        "backspace" => K::Backspace,
        s if s.len() == 1 => {
            let c = s.chars().next()?;
            match c {
                'a' => K::A,
                'b' => K::B,
                'c' => K::C,
                'd' => K::D,
                'e' => K::E,
                'f' => K::F,
                'g' => K::G,
                'h' => K::H,
                'i' => K::I,
                'j' => K::J,
                'k' => K::K,
                'l' => K::L,
                'm' => K::M,
                'n' => K::N,
                'o' => K::O,
                'p' => K::P,
                'q' => K::Q,
                'r' => K::R,
                's' => K::S,
                't' => K::T,
                'u' => K::U,
                'v' => K::V,
                'w' => K::W,
                'x' => K::X,
                'y' => K::Y,
                'z' => K::Z,
                '0' => K::Key0,
                '1' => K::Key1,
                '2' => K::Key2,
                '3' => K::Key3,
                '4' => K::Key4,
                '5' => K::Key5,
                '6' => K::Key6,
                '7' => K::Key7,
                '8' => K::Key8,
                '9' => K::Key9,
                _ => return None,
            }
        }
        _ => return None,
    })
}

impl Foreign for Window {
    fn type_name(&self) -> &'static str {
        "Window"
    }

    fn display(&self) -> String {
        let p = self.px.borrow();
        format!("Window({}x{})", p.w, p.h)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        if let Some(v) = surface_attr(&self.px, name) {
            return Ok(v);
        }
        let pos = self
            .win
            .borrow()
            .get_mouse_pos(minifb::MouseMode::Clamp)
            .unwrap_or((0.0, 0.0));
        match name {
            // How far the mouse moved since the last frame: for looking
            // around in a first-person game.
            "mouse_dx" => Ok(Value::Float(self.mouse_delta.get().0)),
            "mouse_dy" => Ok(Value::Float(self.mouse_delta.get().1)),
            "mouse_x" => Ok(Value::Float(f64::from(pos.0))),
            "mouse_y" => Ok(Value::Float(f64::from(pos.1))),
            _ => Err(err("FieldError", format!("a Window has no field `{name}`"))),
        }
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let _ = ctx;
        if let Some(r) = surface_method(&self.px, name, pos, named) {
            return r;
        }
        let a = Args::new(name, pos, named);
        match name {
            // `loop win.open():` runs until the window is closed (or Escape).
            "open" => {
                let w = self.win.borrow();
                Ok(Value::Bool(
                    w.is_open() && !w.is_key_down(minifb::Key::Escape),
                ))
            }
            // `win.show()`: puts the frame on screen, waits for the next
            // (60 a second by default), and gives the seconds since the last.
            "show" => {
                self.px.borrow_mut().sync();
                let p = self.px.borrow();
                self.win
                    .borrow_mut()
                    .update_with_buffer(&p.buf, p.w, p.h)
                    .map_err(|e| err("DrawError", e.to_string()))?;
                let now = std::time::Instant::now();
                let dt = now.duration_since(self.last.get()).as_secs_f64();
                self.last.set(now);
                drop(p);
                self.poll();
                Ok(Value::Float(dt))
            }
            // `if win.button("Save", x, y): ...` (optional `w`, `h`). Without
            // x and y, it goes where the layout puts it.
            "button" => {
                let label = a.text(0, "label")?;
                let w = match a.get(3, "w") {
                    Some(v) => number(v, "the width")?,
                    None => 16.0 * label.chars().count() as f64 + 24.0,
                };
                let h = a.num_or(4, "h", 32.0)?;
                let (x, y) = self.position(&a, 1, w, h)?;
                let clicked = self.ui.borrow_mut().button(
                    &mut self.px.borrow_mut(),
                    &label,
                    x,
                    y,
                    Some(w),
                    h,
                );
                Ok(Value::Bool(clicked))
            }
            // `name = win.input("name", x, y, w, hint = "Your name")`
            "input" => {
                let name = a.text(0, "name")?;
                let w = a.num_or(3, "w", 240.0)?;
                let (x, y) = self.position(&a, 1, w, 28.0)?;
                let hint = match a.get(4, "hint") {
                    Some(Value::Str(s)) => s.to_string(),
                    _ => String::new(),
                };
                let text =
                    self.ui
                        .borrow_mut()
                        .input(&mut self.px.borrow_mut(), &name, x, y, w, &hint);
                Ok(Value::str(text))
            }
            // `win.set_input("name", "")`: clears (or fills) a text field.
            "set_input" => {
                let name = a.text(0, "name")?;
                let text = a.text(1, "text")?;
                self.ui.borrow_mut().set_field(&name, &text);
                Ok(Value::None)
            }
            // `done = win.checkbox("Done", x, y, done)`, or with the layout
            // `done = win.checkbox("Done", checked = done)`.
            "checkbox" => {
                let label = a.text(0, "label")?;
                let checked = a.flag_or(3, "checked", false);
                let w = 28.0 + 16.0 * label.chars().count() as f64;
                let (x, y) = self.position(&a, 1, w, 20.0)?;
                let now =
                    self.ui
                        .borrow_mut()
                        .checkbox(&mut self.px.borrow_mut(), &label, x, y, checked);
                Ok(Value::Bool(now))
            }
            // `win.label("Name:", x, y, color = "white", size = 2)`
            "label" => {
                let text = match a.get(0, "text") {
                    Some(Value::Str(s)) => s.to_string(),
                    Some(v) => super::ops::display(&ctx.program, v),
                    None => a.text(0, "text")?,
                };
                let size = a.num_or(4, "size", 2.0)?;
                let color = a.color_or(3, "color", 0xffffff)?;
                let w = 8.0 * size * text.chars().count() as f64;
                let (x, y) = self.position(&a, 1, w, 8.0 * size)?;
                self.px.borrow_mut().text(&text, x, y, color, size);
                Ok(Value::None)
            }
            // `volume = win.slider("volume", 0, 100, volume, x, y, w = 240)`
            "slider" => {
                let name = a.text(0, "name")?;
                let low = a.num_or(1, "low", 0.0)?;
                let high = a.num_or(2, "high", 1.0)?;
                let value = a.num_or(3, "value", low)?;
                let w = a.num_or(6, "w", 240.0)?;
                let (x, y) = self.position(&a, 4, w, 24.0)?;
                let v = self.ui.borrow_mut().slider(
                    &mut self.px.borrow_mut(),
                    &name,
                    x,
                    y,
                    w,
                    low,
                    high,
                    value,
                );
                Ok(Value::Float(v))
            }
            // `win.layout(x, y, gap = 10)`: where widgets without a position
            // start this frame; `row()` ... `end_row()` puts them side by
            // side, `space(pixels)` leaves a gap.
            "layout" => {
                let (x, y) = (a.num_or(0, "x", 16.0)?, a.num_or(1, "y", 16.0)?);
                let gap = a.num_or(2, "gap", 10.0)?;
                self.ui.borrow_mut().layout(x, y, gap);
                Ok(Value::None)
            }
            "row" => {
                self.ui.borrow_mut().row();
                Ok(Value::None)
            }
            "end_row" => {
                self.ui.borrow_mut().end_row();
                Ok(Value::None)
            }
            "space" => {
                let pixels = a.num_or(0, "pixels", 10.0)?;
                self.ui.borrow_mut().space(pixels);
                Ok(Value::None)
            }
            "clicked" => Ok(Value::Bool(self.ui.borrow().clicked())),
            // What was typed during the last frame.
            "typed" => Ok(Value::str(
                self.ui
                    .borrow()
                    .typed
                    .chars()
                    .filter(|c| !c.is_control())
                    .collect::<String>(),
            )),
            "key_down" => {
                let k = a.text(0, "key")?;
                let code =
                    key(&k).ok_or_else(|| err("ValueError", format!("unknown key {k:?}")))?;
                Ok(Value::Bool(self.win.borrow().is_key_down(code)))
            }
            "key_pressed" => {
                let k = a.text(0, "key")?;
                let code =
                    key(&k).ok_or_else(|| err("ValueError", format!("unknown key {k:?}")))?;
                Ok(Value::Bool(
                    self.win
                        .borrow()
                        .is_key_pressed(code, minifb::KeyRepeat::No),
                ))
            }
            "mouse_down" => {
                let b = match a.get(0, "button") {
                    Some(Value::Str(s)) if s.as_str() == "right" => minifb::MouseButton::Right,
                    Some(Value::Str(s)) if s.as_str() == "middle" => minifb::MouseButton::Middle,
                    _ => minifb::MouseButton::Left,
                };
                Ok(Value::Bool(self.win.borrow().get_mouse_down(b)))
            }
            "set_title" => {
                self.win.borrow_mut().set_title(&a.text(0, "title")?);
                Ok(Value::None)
            }
            // For first-person games: the mouse turns the view instead of
            // pointing at things.
            "hide_cursor" | "show_cursor" => {
                self.win
                    .borrow_mut()
                    .set_cursor_visibility(name == "show_cursor");
                Ok(Value::None)
            }
            "set_fps" => {
                self.win
                    .borrow_mut()
                    .set_target_fps(a.num(0, "fps")?.max(1.0) as usize);
                Ok(Value::None)
            }
            "close" => Ok(Value::None),
            _ => Err(err(
                "FieldError",
                format!("a Window has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== turtle ================================================================

/// Drawing by moving: `t.forward(100)`, `t.left(90)`. The turtle starts in
/// the middle, facing right; y goes up, as in school geometry.
pub struct Turtle {
    px: Rc<RefCell<Pixels>>,
    state: RefCell<TurtleState>,
}

struct TurtleState {
    x: f64,
    y: f64,
    heading: f64,
    down: bool,
    color: Color,
    width: f64,
    fill: Option<Vec<(f64, f64)>>,
}

impl Turtle {
    fn screen(&self, x: f64, y: f64) -> (f64, f64) {
        let p = self.px.borrow();
        (p.w as f64 / 2.0 + x, p.h as f64 / 2.0 - y)
    }

    fn go(&self, x: f64, y: f64) {
        let (x0, y0, down, c, w) = {
            let s = self.state.borrow();
            (s.x, s.y, s.down, s.color, s.width)
        };
        if down {
            let (a, b) = (self.screen(x0, y0), self.screen(x, y));
            self.px.borrow_mut().line(a.0, a.1, b.0, b.1, c, w);
        }
        let mut s = self.state.borrow_mut();
        s.x = x;
        s.y = y;
        if let Some(f) = &mut s.fill {
            f.push((x, y));
        }
    }
}

impl Foreign for Turtle {
    fn type_name(&self) -> &'static str {
        "Turtle"
    }

    fn display(&self) -> String {
        let s = self.state.borrow();
        format!("Turtle(x={}, y={}, heading={})", s.x, s.y, s.heading)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        let s = self.state.borrow();
        Ok(Value::Float(match name {
            "x" => s.x,
            "y" => s.y,
            // `+ 0.0` turns -0.0 into 0.0.
            "heading" => s.heading.rem_euclid(360.0) + 0.0,
            _ => return Err(err("FieldError", format!("a Turtle has no field `{name}`"))),
        }))
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let a = Args::new(name, pos, named);
        match name {
            "forward" | "back" => {
                let d = a.num(0, "distance")? * if name == "back" { -1.0 } else { 1.0 };
                let (x, y, h) = {
                    let s = self.state.borrow();
                    (s.x, s.y, s.heading.to_radians())
                };
                self.go(x + d * h.cos(), y + d * h.sin());
            }
            "left" => self.state.borrow_mut().heading += a.num(0, "degrees")?,
            "right" => self.state.borrow_mut().heading -= a.num(0, "degrees")?,
            "goto" => self.go(a.num(0, "x")?, a.num(1, "y")?),
            "home" => {
                self.go(0.0, 0.0);
                self.state.borrow_mut().heading = 0.0;
            }
            "set_heading" => self.state.borrow_mut().heading = a.num(0, "degrees")?,
            "pen_up" => self.state.borrow_mut().down = false,
            "pen_down" => self.state.borrow_mut().down = true,
            "color" => self.state.borrow_mut().color = a.color_or(0, "color", 0xffffff)?,
            "width" => self.state.borrow_mut().width = a.num(0, "width")?,
            // `t.circle(r)`: a circle to the turtle's left, as Python's does.
            "circle" => {
                let r = a.num(0, "radius")?;
                let steps = 72;
                let step = 2.0 * std::f64::consts::PI * r.abs() / steps as f64;
                for _ in 0..steps {
                    let (x, y, h) = {
                        let s = self.state.borrow();
                        (s.x, s.y, s.heading)
                    };
                    let rad = (h + 2.5_f64.copysign(r)).to_radians();
                    self.go(x + step * rad.cos(), y + step * rad.sin());
                    self.state.borrow_mut().heading = h + 5.0_f64.copysign(r);
                }
            }
            "dot" => {
                let size = a.num_or(0, "size", 6.0)?;
                let c = match a.get(1, "color") {
                    Some(v) => parse_color(v)?,
                    None => self.state.borrow().color,
                };
                let (x, y) = {
                    let s = self.state.borrow();
                    (s.x, s.y)
                };
                let (sx, sy) = self.screen(x, y);
                self.px
                    .borrow_mut()
                    .circle(sx, sy, size / 2.0, c, true, 1.0);
            }
            "write" => {
                let t = a.text(0, "text")?;
                let size = a.num_or(1, "size", 2.0)?;
                let (x, y, c) = {
                    let s = self.state.borrow();
                    (s.x, s.y, s.color)
                };
                let (sx, sy) = self.screen(x, y);
                self.px.borrow_mut().text(&t, sx, sy, c, size);
            }
            "begin_fill" => {
                let mut s = self.state.borrow_mut();
                let start = (s.x, s.y);
                s.fill = Some(vec![start]);
            }
            "end_fill" => {
                let (pts, c) = {
                    let mut s = self.state.borrow_mut();
                    (s.fill.take().unwrap_or_default(), s.color)
                };
                let screen: Vec<(f64, f64)> =
                    pts.iter().map(|(x, y)| self.screen(*x, *y)).collect();
                self.px.borrow_mut().polygon(&screen, c);
            }
            _ => {
                return Err(err(
                    "FieldError",
                    format!("a Turtle has no method `{name}`"),
                ))
            }
        }
        Ok(Value::None)
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

// ==== the module ============================================================

fn size_args(args: &[Value]) -> VmResult<(usize, usize)> {
    let w = number(args.first().unwrap_or(&Value::Int(640)), "the width")?;
    let h = number(args.get(1).unwrap_or(&Value::Int(480)), "the height")?;
    if !(1.0..=8192.0).contains(&w) || !(1.0..=8192.0).contains(&h) {
        return Err(err(
            "ValueError",
            format!("a {w}x{h} picture is too large or too small"),
        ));
    }
    Ok((w as usize, h as usize))
}

fn wrap(f: impl Foreign) -> Value {
    Value::Foreign(Rc::new(Box::new(f)))
}

/// `draw.canvas(width, height)`: a picture to draw on and `save`.
pub fn canvas(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (w, h) = size_args(args)?;
    Ok(wrap(Canvas {
        px: Rc::new(RefCell::new(Pixels::new(w, h))),
    }))
}

/// `draw.window(width, height, title)`: a window on the screen.
pub fn window(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (w, h) = size_args(args)?;
    let title = match args.get(2) {
        Some(Value::Str(s)) => s.to_string(),
        _ => "Vyrn".to_string(),
    };
    let mut win = minifb::Window::new(&title, w, h, minifb::WindowOptions::default()).map_err(|e| {
        err(
            "DrawError",
            format!("cannot open a window ({e}); on a machine without a screen, draw on `draw.canvas(w, h)` and `save` it"),
        )
    })?;
    win.set_target_fps(60);
    let typed = Rc::new(RefCell::new(String::new()));
    win.set_input_callback(Box::new(Typed(typed.clone())));
    let window = Window {
        px: Rc::new(RefCell::new(Pixels::new(w, h))),
        win: RefCell::new(win),
        last: Cell::new(std::time::Instant::now()),
        ui: RefCell::new(Ui::default()),
        typed,
        last_mouse: Cell::new(None),
        mouse_delta: Cell::new((0.0, 0.0)),
    };
    window.poll();
    Ok(wrap(window))
}

fn pixels_of(v: &Value) -> Option<Rc<RefCell<Pixels>>> {
    let Value::Foreign(f) = v else { return None };
    if let Some(c) = f.as_any().downcast_ref::<Canvas>() {
        return Some(c.px.clone());
    }
    f.as_any().downcast_ref::<Window>().map(|w| w.px.clone())
}

/// `draw.turtle(surface)`: a turtle drawing on a canvas or window.
pub fn turtle(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let px = args
        .first()
        .and_then(pixels_of)
        .ok_or_else(|| type_error("`draw.turtle` takes a canvas or a window"))?;
    Ok(wrap(Turtle {
        px,
        state: RefCell::new(TurtleState {
            x: 0.0,
            y: 0.0,
            heading: 0.0,
            down: true,
            color: Color::opaque(0xffffff),
            width: 1.0,
            fill: None,
        }),
    }))
}

/// `draw.image(path)`: a PNG picture.
pub fn image(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let Some(Value::Str(path)) = args.first() else {
        return Err(type_error("`draw.image` takes the path of a PNG file"));
    };
    load_png(path).map(wrap)
}

/// `draw.camera(x = 0, y = 0, z = -5)`: a 3D viewpoint, looking along +z.
pub fn camera(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = |i: usize, d: f64| -> VmResult<f64> {
        match args.get(i).filter(|v| !matches!(v, Value::Missing)) {
            Some(v) => number(v, "a camera coordinate"),
            None => Ok(d),
        }
    };
    Ok(wrap(Camera {
        pos: Cell::new([n(0, 0.0)?, n(1, 0.0)?, n(2, -5.0)?]),
        yaw: Cell::new(0.0),
        pitch: Cell::new(0.0),
        fov: Cell::new(60.0),
    }))
}

/// `draw.mesh(points, triangles)`: a shape of your own: `points` is a list
/// of [x, y, z], `triangles` a list of [a, b, c] positions in `points`.
pub fn mesh(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let pts = ops::collect(args.first().unwrap_or(&Value::None))?;
    let tris = ops::collect(args.get(1).unwrap_or(&Value::None))?;
    let mut verts = Vec::new();
    for p in &pts {
        let c = ops::collect(p)?;
        let [x, y, z] = c.as_slice() else {
            return Err(type_error("each point is [x, y, z]"));
        };
        verts.push([number(x, "x")?, number(y, "y")?, number(z, "z")?]);
    }
    let mut faces = Vec::new();
    for t in &tris {
        let c = ops::collect(t)?;
        let [a, b, d] = c.as_slice() else {
            return Err(type_error("each triangle is [a, b, c]"));
        };
        let idx = |v: &Value| -> VmResult<usize> {
            let i = super::builtins::integer(v, "a point's position")?;
            usize::try_from(i)
                .ok()
                .filter(|i| *i < verts.len())
                .ok_or_else(|| {
                    err(
                        "IndexError",
                        format!(
                            "triangle point {i} is not in the list of {} points",
                            verts.len()
                        ),
                    )
                })
        };
        faces.push([idx(a)?, idx(b)?, idx(d)?]);
    }
    let mut mesh = Mesh::new(verts, faces);
    // `uvs = [[u, v], ...]`: texture coordinates per point.
    if let Some(uvs) = opt_arg(args, 2) {
        let uvs = ops::collect(uvs)?;
        if uvs.len() != mesh.verts.len() {
            return Err(type_error(format!(
                "`uvs` needs one [u, v] per point: {} points, {} given",
                mesh.verts.len(),
                uvs.len()
            )));
        }
        for uv in &uvs {
            let c = ops::collect(uv)?;
            let [u, v] = c.as_slice() else {
                return Err(type_error("each texture coordinate is [u, v]"));
            };
            mesh.uvs.push([number(u, "u")?, number(v, "v")?]);
        }
    }
    if opt_arg(args, 3).is_some_and(Value::truthy) {
        mesh = mesh.smooth();
    }
    if let Some(t) = opt_arg(args, 4) {
        image_arg(Some(t))?;
        mesh.texture = Some(t.clone());
    }
    Ok(wrap(mesh))
}

fn opt_arg(args: &[Value], i: usize) -> Option<&Value> {
    args.get(i)
        .filter(|v| !matches!(v, Value::None | Value::Missing))
}

/// `draw.overlaps(x1, y1, w1, h1, x2, y2, w2, h2)`: whether two rectangles
/// touch, for collisions in games.
pub fn overlaps(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let v: Vec<f64> = args
        .iter()
        .map(|a| number(a, "a rectangle side"))
        .collect::<VmResult<_>>()?;
    let [x1, y1, w1, h1, x2, y2, w2, h2] = v.as_slice() else {
        return Err(type_error(
            "`draw.overlaps` takes two rectangles: x, y, w, h, x, y, w, h",
        ));
    };
    Ok(Value::Bool(
        x1 < &(x2 + w2) && x2 < &(x1 + w1) && y1 < &(y2 + h2) && y2 < &(y1 + h1),
    ))
}

/// `draw.rgb(r, g, b)`: a colour from its parts.
pub fn rgb(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let ch = |i: usize| -> VmResult<u32> {
        Ok(
            number(args.get(i).unwrap_or(&Value::Int(0)), "a colour channel")?.clamp(0.0, 255.0)
                as u32,
        )
    };
    Ok(Value::str(color_text(
        (ch(0)? << 16) | (ch(1)? << 8) | ch(2)?,
    )))
}

/// `draw.text_width(text, size = 2)`: how wide text is drawn, in pixels.
pub fn text_width(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = match args.first() {
        Some(Value::Str(s)) => s.lines().map(|l| l.chars().count()).max().unwrap_or(0),
        _ => 0,
    };
    let size = match args.get(1) {
        Some(v) if !matches!(v, Value::Missing) => number(v, "the size")?.max(1.0).round(),
        _ => 2.0,
    };
    Ok(Value::Int((n as f64 * 8.0 * size) as i64))
}

/// `draw.use_gpu(true)`: fill 3D shapes on the GPU (false: in software).
/// Without it, a real GPU is used when there is one. Gives whether the GPU
/// is now in use.
pub fn use_gpu(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let on = args.first().is_some_and(Value::truthy);
    #[cfg(feature = "gpu")]
    {
        super::draw_gpu::choose(if on {
            super::draw_gpu::Choice::On
        } else {
            super::draw_gpu::Choice::Off
        });
        Ok(Value::Bool(super::draw_gpu::active()))
    }
    #[cfg(not(feature = "gpu"))]
    {
        let _ = on;
        Ok(Value::Bool(false))
    }
}

/// `draw.renderer()`: what fills 3D shapes, "gpu: <name>" or "software".
pub fn renderer(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    #[cfg(feature = "gpu")]
    {
        Ok(Value::str(super::draw_gpu::description()))
    }
    #[cfg(not(feature = "gpu"))]
    {
        Ok(Value::str("software"))
    }
}

/// `draw.load_obj("ship.obj")`: a model saved as a Wavefront OBJ file (the
/// format Blender and most 3D tools export), for `win.mesh(...)`. Faces
/// with more than three corners are split into triangles.
pub fn load_obj(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => return Err(type_error("`draw.load_obj` takes the file's path")),
    };
    let text = std::fs::read_to_string(&path)
        .map_err(|e| err("IoError", format!("cannot read {path}: {e}")))?;
    let (mut mesh, library) = parse_obj(&text).map_err(|why| {
        err(
            "ValueError",
            format!("{path} is not a model this can read: {why}"),
        )
    })?;
    // The texture its material file names (`map_Kd`), if it is a PNG.
    let dir = std::path::Path::new(&path)
        .parent()
        .unwrap_or(std::path::Path::new(""));
    if let Some(library) = library {
        if let Ok(mtl) = std::fs::read_to_string(dir.join(&library)) {
            let map = mtl
                .lines()
                .filter_map(|l| l.trim().strip_prefix("map_Kd"))
                .map(|rest| rest.split_whitespace().last().unwrap_or("").to_string())
                .next();
            if let Some(map) = map.filter(|m| m.to_lowercase().ends_with(".png")) {
                let image = load_png(&dir.join(&map).to_string_lossy())?;
                mesh.texture = Some(wrap(image));
            }
        }
    }
    Ok(wrap(mesh))
}

/// The mesh in an OBJ file, and the material file it names. Corners with
/// different texture coordinates or normals become separate points, as a
/// GPU needs.
fn parse_obj(text: &str) -> Result<(Mesh, Option<String>), String> {
    let mut positions = Vec::new();
    let mut tex_coords: Vec<[f64; 2]> = Vec::new();
    let mut normals_in: Vec<[f64; 3]> = Vec::new();
    let mut mesh = Mesh::new(Vec::new(), Vec::new());
    let mut seen: std::collections::HashMap<(usize, Option<usize>, Option<usize>), usize> =
        std::collections::HashMap::new();
    let mut any_uv = false;
    let mut any_normal = false;
    let mut library = None;
    let numbers = |parts: std::str::SplitWhitespace<'_>, n: usize| -> Vec<f64> {
        parts.take(n).filter_map(|p| p.parse().ok()).collect()
    };
    for (n, line) in text.lines().enumerate() {
        let mut parts = line.split_whitespace();
        match parts.next() {
            Some("v") => {
                let c = numbers(parts, 3);
                let [x, y, z] = c[..] else {
                    return Err(format!("line {} has a point without x, y and z", n + 1));
                };
                positions.push([x, y, z]);
            }
            Some("vt") => {
                let c = numbers(parts, 2);
                let u = c.first().copied().unwrap_or(0.0);
                let v = c.get(1).copied().unwrap_or(0.0);
                // OBJ counts v from the bottom; images from the top.
                tex_coords.push([u, 1.0 - v]);
            }
            Some("vn") => {
                let c = numbers(parts, 3);
                let [x, y, z] = c[..] else {
                    return Err(format!("line {} has a normal without x, y and z", n + 1));
                };
                normals_in.push(normalize([x, y, z]));
            }
            Some("mtllib") => library = parts.next().map(str::to_string),
            Some("f") => {
                // `f 1 2 3`, `f 1/1 2/2 3/3` or `f 1/1/1 2//2 ...`; counted
                // from 1, or from the end when negative.
                let index = |field: Option<&str>,
                             count: usize,
                             what: &str|
                 -> Result<Option<usize>, String> {
                    let Some(field) = field.filter(|f| !f.is_empty()) else {
                        return Ok(None);
                    };
                    let i: i64 = field
                        .parse()
                        .map_err(|_| format!("line {} has a face corner {field:?}", n + 1))?;
                    let i = if i < 0 { count as i64 + i } else { i - 1 };
                    usize::try_from(i)
                        .ok()
                        .filter(|i| *i < count)
                        .map(Some)
                        .ok_or_else(|| format!("line {} uses a {what} that is not there", n + 1))
                };
                let mut corners = Vec::new();
                for p in parts {
                    let mut fields = p.split('/');
                    let v = index(fields.next(), positions.len(), "point")?
                        .ok_or_else(|| format!("line {} has an empty face corner", n + 1))?;
                    let t = index(fields.next(), tex_coords.len(), "texture coordinate")?;
                    let nn = index(fields.next(), normals_in.len(), "normal")?;
                    any_uv |= t.is_some();
                    any_normal |= nn.is_some();
                    let key = (v, t, nn);
                    let at = *seen.entry(key).or_insert_with(|| {
                        mesh.verts.push(positions[v]);
                        mesh.uvs.push(t.map_or([0.0; 2], |t| tex_coords[t]));
                        mesh.normals.push(nn.map_or([0.0; 3], |nn| normals_in[nn]));
                        mesh.verts.len() - 1
                    });
                    corners.push(at);
                }
                for k in 1..corners.len().saturating_sub(1) {
                    mesh.faces.push([corners[0], corners[k], corners[k + 1]]);
                }
            }
            _ => {}
        }
    }
    if mesh.faces.is_empty() {
        return Err("it has no faces".into());
    }
    if !any_uv {
        mesh.uvs.clear();
    }
    if !any_normal {
        mesh.normals.clear();
    }
    Ok((mesh, library))
}

/// `draw.cube_mesh()` and `draw.sphere_mesh(detail)`: ready-made shapes
/// one unit across, for `win.mesh(...)`.
pub fn cube_mesh_fn(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(wrap(cube_mesh()))
}

pub fn sphere_mesh_fn(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let detail = match args.first() {
        Some(v) if !matches!(v, Value::Missing) => number(v, "the detail")? as usize,
        _ => 12,
    };
    Ok(wrap(sphere_mesh(detail)))
}

/// Draws a chart's shapes (see `lib_chart`) and saves them as a PNG.
pub fn save_prims(
    w: usize,
    h: usize,
    prims: &[super::lib_chart::Prim],
    path: &str,
) -> VmResult<()> {
    use super::lib_chart::Prim;
    let mut p = Pixels::new(w, h);
    for prim in prims {
        match prim {
            Prim::Rect { x, y, w, h, color } => {
                p.rect(*x, *y, *w, *h, Color::opaque(*color), true, 1.0)
            }
            Prim::Line {
                x1,
                y1,
                x2,
                y2,
                color,
                width,
            } => p.line(*x1, *y1, *x2, *y2, Color::opaque(*color), *width),
            Prim::Circle { x, y, r, color } => {
                p.circle(*x, *y, *r, Color::opaque(*color), true, 1.0)
            }
            Prim::Polygon { points, color } => p.polygon(points, Color::opaque(*color)),
            Prim::Text {
                x,
                y,
                text,
                color,
                size,
                centered,
            } => {
                let width = 8.0 * size * text.chars().count() as f64;
                let left = if *centered { x - width / 2.0 } else { *x };
                p.text(text, left, *y, Color::opaque(*color), *size)
            }
        }
    }
    p.save_png(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widgets_respond_to_input() {
        let mut px = Pixels::new(300, 200);
        let mut ui = Ui::default();
        // Hovering is not clicking.
        ui.next_frame((20.0, 20.0), false, String::new());
        assert!(!ui.button(&mut px, "Go", 10.0, 10.0, None, 32.0));
        // Pressing inside clicks once, not again while held.
        ui.next_frame((20.0, 20.0), true, String::new());
        assert!(ui.button(&mut px, "Go", 10.0, 10.0, None, 32.0));
        ui.next_frame((20.0, 20.0), true, String::new());
        assert!(!ui.button(&mut px, "Go", 10.0, 10.0, None, 32.0));
        // A click elsewhere misses.
        ui.next_frame((250.0, 150.0), false, String::new());
        ui.next_frame((250.0, 150.0), true, String::new());
        assert!(!ui.button(&mut px, "Go", 10.0, 10.0, None, 32.0));

        // Typing goes to the field clicked, with Backspace.
        ui.next_frame((20.0, 60.0), false, String::new());
        ui.next_frame((20.0, 60.0), true, "ignored".into());
        assert_eq!(ui.input(&mut px, "name", 10.0, 50.0, 200.0, "Name"), "");
        ui.next_frame((20.0, 60.0), false, "Adx".into());
        assert_eq!(ui.input(&mut px, "name", 10.0, 50.0, 200.0, "Name"), "Adx");
        ui.next_frame((20.0, 60.0), false, "\u{8}a".into());
        assert_eq!(ui.input(&mut px, "name", 10.0, 50.0, 200.0, "Name"), "Ada");
        // Enter leaves the field; later typing does not reach it.
        ui.next_frame((20.0, 60.0), false, "\n".into());
        ui.input(&mut px, "name", 10.0, 50.0, 200.0, "Name");
        ui.next_frame((20.0, 60.0), false, "zz".into());
        assert_eq!(ui.input(&mut px, "name", 10.0, 50.0, 200.0, "Name"), "Ada");
        ui.set_field("name", "");
        assert_eq!(ui.input(&mut px, "name", 10.0, 50.0, 200.0, "Name"), "");

        // A checkbox flips when clicked.
        ui.next_frame((15.0, 105.0), false, String::new());
        assert!(!ui.checkbox(&mut px, "Done", 10.0, 100.0, false));
        ui.next_frame((15.0, 105.0), true, String::new());
        assert!(ui.checkbox(&mut px, "Done", 10.0, 100.0, false));
    }

    #[test]
    fn obj_files_with_texture_coordinates_and_normals() {
        let text = "mtllib crate.mtl\nv 0 0 0\nv 1 0 0\nv 1 1 0\nv 0 1 0\n\
                    vt 0 0\nvt 1 0\nvt 1 1\nvt 0 1\nvn 0 0 2\n\
                    f 1/1/1 2/2/1 3/3/1 4/4/1\n# the same corner, another uv\nf 1/3/1 3/3/1 4/4/1\n";
        let (mesh, library) = parse_obj(text).unwrap();
        assert_eq!(library.as_deref(), Some("crate.mtl"));
        // Point 1 with two different uvs becomes two points.
        assert_eq!((mesh.verts.len(), mesh.faces.len()), (5, 3));
        // v counts from the bottom in OBJ, from the top in images.
        assert_eq!(mesh.uvs[0], [0.0, 1.0]);
        assert_eq!(mesh.uvs[2], [1.0, 0.0]);
        assert_eq!(mesh.normals[0], [0.0, 0.0, 1.0]);
        // Without them, both stay empty.
        let (plain, _) = parse_obj("v 0 0 0\nv 1 0 0\nv 0 1 0\nf 1 2 3\n").unwrap();
        assert!(plain.uvs.is_empty() && plain.normals.is_empty());
        assert!(parse_obj("v 0 0 0\nf 1 2 3\n").is_err());
    }

    #[test]
    fn layout_places_widgets_in_columns_and_rows() {
        let mut ui = Ui::default();
        ui.next_frame((0.0, 0.0), false, String::new());
        // Without a layout, widgets start at (16, 16), 10 apart.
        assert_eq!(ui.place(100.0, 32.0), (16.0, 16.0));
        assert_eq!(ui.place(100.0, 28.0), (16.0, 58.0));
        ui.layout(40.0, 40.0, 8.0);
        assert_eq!(ui.place(50.0, 20.0), (40.0, 40.0));
        ui.row();
        assert_eq!(ui.place(50.0, 32.0), (40.0, 68.0));
        assert_eq!(ui.place(30.0, 20.0), (98.0, 68.0));
        ui.end_row();
        ui.space(12.0);
        assert_eq!(ui.place(10.0, 10.0), (40.0, 120.0));
        // The next frame starts at the top again.
        ui.next_frame((0.0, 0.0), false, String::new());
        assert_eq!(ui.place(10.0, 10.0), (16.0, 16.0));
    }

    #[test]
    fn sliders_follow_a_drag() {
        let mut px = Pixels::new(300, 100);
        let mut ui = Ui::default();
        ui.next_frame((60.0, 22.0), false, String::new());
        assert_eq!(
            ui.slider(&mut px, "v", 10.0, 10.0, 200.0, 0.0, 100.0, 30.0),
            30.0
        );
        // Pressing on the track picks up the slider; dragging moves it,
        // even past the end.
        ui.next_frame((60.0, 22.0), true, String::new());
        assert_eq!(
            ui.slider(&mut px, "v", 10.0, 10.0, 200.0, 0.0, 100.0, 30.0),
            25.0
        );
        ui.next_frame((400.0, 90.0), true, String::new());
        assert_eq!(
            ui.slider(&mut px, "v", 10.0, 10.0, 200.0, 0.0, 100.0, 25.0),
            100.0
        );
        ui.next_frame((400.0, 90.0), false, String::new());
        assert_eq!(
            ui.slider(&mut px, "v", 10.0, 10.0, 200.0, 0.0, 100.0, 100.0),
            100.0
        );
    }
}
