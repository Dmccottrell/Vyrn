//! `use vyrn.chart`: charts saved as PNG or SVG, as matplotlib gives
//! Python.
//!
//! ```vyrn
//! use vyrn.chart
//!
//! main:
//!     let c = chart.line([1, 2, 3, 4], {"sales": [10, 14, 9, 20], "costs": [8, 9, 9, 12]},
//!                        title = "This year", x_label = "quarter")
//!     c.save("sales.png")
//!     chart.bar(["a", "b", "c"], [3, 7, 5]).save("bars.svg")
//! end
//! ```
//!
//! `line`, `bar`, `scatter`, `hist` and `pie`, each with `title`,
//! `x_label`, `y_label`, `width` and `height`. A map of name to values
//! draws several series, with a legend.

use std::rc::Rc;

use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::Value;

/// What a chart is drawn from.
#[derive(Clone, Debug)]
pub enum Prim {
    Rect {
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        color: u32,
    },
    Line {
        x1: f64,
        y1: f64,
        x2: f64,
        y2: f64,
        color: u32,
        width: f64,
    },
    Circle {
        x: f64,
        y: f64,
        r: f64,
        color: u32,
    },
    Polygon {
        points: Vec<(f64, f64)>,
        color: u32,
    },
    /// Text, its left edge at `x` unless `centered`; the font is 8 pixels
    /// per character at size 1.
    Text {
        x: f64,
        y: f64,
        text: String,
        color: u32,
        size: f64,
        centered: bool,
    },
}

const PALETTE: &[u32] = &[
    0x1f77b4, 0xff7f0e, 0x2ca02c, 0xd62728, 0x9467bd, 0x8c564b, 0xe377c2, 0x7f7f7f,
];
const AXIS: u32 = 0x333333;
const GRID: u32 = 0xe5e5e5;

pub struct Chart {
    w: f64,
    h: f64,
    prims: Vec<Prim>,
}

struct Opts {
    title: String,
    x_label: String,
    y_label: String,
    w: f64,
    h: f64,
}

fn opts(named: &[(String, Value)]) -> VmResult<Opts> {
    let text = |n: &str| -> String {
        match named.iter().find(|(k, _)| k == n).map(|(_, v)| v) {
            Some(Value::Str(s)) => s.to_string(),
            _ => String::new(),
        }
    };
    let num = |n: &str, d: f64| -> VmResult<f64> {
        match named.iter().find(|(k, _)| k == n).map(|(_, v)| v) {
            Some(v) => super::builtins::number(v, n),
            None => Ok(d),
        }
    };
    Ok(Opts {
        title: text("title"),
        x_label: text("x_label"),
        y_label: text("y_label"),
        w: num("width", 640.0)?.clamp(200.0, 4000.0),
        h: num("height", 400.0)?.clamp(150.0, 4000.0),
    })
}

fn numbers(v: &Value, what: &str) -> VmResult<Vec<f64>> {
    ops::collect(v)?
        .iter()
        .map(|x| super::builtins::number(x, what))
        .collect()
}

fn labels(ctx: &Ctx, v: &Value) -> VmResult<Vec<String>> {
    Ok(ops::collect(v)?
        .iter()
        .map(|x| match x {
            Value::Str(s) => s.to_string(),
            other => ops::display(&ctx.program, other),
        })
        .collect())
}

/// The series to draw: a list of numbers, or a map of name to numbers.
fn series(ctx: &Ctx, v: &Value) -> VmResult<Vec<(String, Vec<f64>)>> {
    if let Value::Map(m) = v {
        let mut out = Vec::new();
        for (k, vals) in m.borrow().entries() {
            let name = match k {
                Value::Str(s) => s.to_string(),
                other => ops::display(&ctx.program, other),
            };
            out.push((name, numbers(vals, "a value to chart")?));
        }
        return Ok(out);
    }
    Ok(vec![(String::new(), numbers(v, "a value to chart")?)])
}

/// About five round numbers covering `lo..hi`.
fn ticks(lo: f64, hi: f64) -> Vec<f64> {
    let (lo, hi) = if (hi - lo).abs() < 1e-12 {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    };
    let raw = (hi - lo) / 5.0;
    let mag = 10f64.powf(raw.log10().floor());
    let step = [1.0, 2.0, 2.5, 5.0, 10.0]
        .iter()
        .map(|m| m * mag)
        .find(|s| *s >= raw)
        .unwrap_or(10.0 * mag);
    let mut t = (lo / step).floor() * step;
    let mut out = Vec::new();
    while t <= hi + step * 0.001 {
        out.push(if t.abs() < step * 1e-9 { 0.0 } else { t });
        t += step;
    }
    if out.last().is_some_and(|l| *l < hi) {
        out.push(out.last().copied().unwrap_or(0.0) + step);
    }
    out
}

fn tick_text(v: f64) -> String {
    if v.fract().abs() < 1e-9 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        let s = format!("{v:.2}");
        s.trim_end_matches('0').trim_end_matches('.').to_string()
    }
}

/// The plot area and the drawing around it.
struct Frame {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
    prims: Vec<Prim>,
}

impl Frame {
    fn new(o: &Opts, legend: &[String]) -> Frame {
        let right = if legend.iter().any(|n| !n.is_empty()) {
            20.0 + 8.0 * legend.iter().map(|n| n.chars().count()).max().unwrap_or(0) as f64 + 30.0
        } else {
            20.0
        };
        let mut prims = vec![Prim::Rect {
            x: 0.0,
            y: 0.0,
            w: o.w,
            h: o.h,
            color: 0xffffff,
        }];
        if !o.title.is_empty() {
            prims.push(Prim::Text {
                x: o.w / 2.0,
                y: 12.0,
                text: o.title.clone(),
                color: AXIS,
                size: 2.0,
                centered: true,
            });
        }
        if !o.x_label.is_empty() {
            prims.push(Prim::Text {
                x: o.w / 2.0,
                y: o.h - 16.0,
                text: o.x_label.clone(),
                color: AXIS,
                size: 1.0,
                centered: true,
            });
        }
        if !o.y_label.is_empty() {
            prims.push(Prim::Text {
                x: 8.0,
                y: 40.0 - 14.0,
                text: o.y_label.clone(),
                color: AXIS,
                size: 1.0,
                centered: false,
            });
        }
        let mut f = Frame {
            x0: 70.0,
            y0: 44.0,
            x1: o.w - right,
            y1: o.h - 44.0,
            prims,
        };
        // The legend, top right.
        let mut ly = f.y0;
        for (i, n) in legend.iter().enumerate() {
            if n.is_empty() {
                continue;
            }
            let c = PALETTE[i % PALETTE.len()];
            f.prims.push(Prim::Rect {
                x: f.x1 + 14.0,
                y: ly,
                w: 12.0,
                h: 8.0,
                color: c,
            });
            f.prims.push(Prim::Text {
                x: f.x1 + 32.0,
                y: ly,
                text: n.clone(),
                color: AXIS,
                size: 1.0,
                centered: false,
            });
            ly += 16.0;
        }
        f
    }

    /// Horizontal grid lines and labels for values `lo..hi`; returns the
    /// mapping from a value to a y position.
    fn y_axis(&mut self, lo: f64, hi: f64) -> (f64, f64) {
        let t = ticks(lo, hi);
        let (tl, th) = (t[0], *t.last().unwrap_or(&hi));
        for v in &t {
            let y = self.y1 - (v - tl) / (th - tl) * (self.y1 - self.y0);
            self.prims.push(Prim::Line {
                x1: self.x0,
                y1: y,
                x2: self.x1,
                y2: y,
                color: GRID,
                width: 1.0,
            });
            let label = tick_text(*v);
            self.prims.push(Prim::Text {
                x: self.x0 - 8.0 - 8.0 * label.chars().count() as f64,
                y: y - 4.0,
                text: label,
                color: AXIS,
                size: 1.0,
                centered: false,
            });
        }
        (tl, th)
    }

    fn y_of(&self, v: f64, (tl, th): (f64, f64)) -> f64 {
        self.y1 - (v - tl) / (th - tl) * (self.y1 - self.y0)
    }

    fn axes(&mut self) {
        self.prims.push(Prim::Line {
            x1: self.x0,
            y1: self.y1,
            x2: self.x1,
            y2: self.y1,
            color: AXIS,
            width: 1.0,
        });
        self.prims.push(Prim::Line {
            x1: self.x0,
            y1: self.y0,
            x2: self.x0,
            y2: self.y1,
            color: AXIS,
            width: 1.0,
        });
    }

    fn x_ticks(&mut self, lo: f64, hi: f64) -> (f64, f64) {
        let t = ticks(lo, hi);
        let (tl, th) = (t[0], *t.last().unwrap_or(&hi));
        for v in &t {
            let x = self.x0 + (v - tl) / (th - tl) * (self.x1 - self.x0);
            self.prims.push(Prim::Line {
                x1: x,
                y1: self.y1,
                x2: x,
                y2: self.y1 + 4.0,
                color: AXIS,
                width: 1.0,
            });
            self.prims.push(Prim::Text {
                x,
                y: self.y1 + 8.0,
                text: tick_text(*v),
                color: AXIS,
                size: 1.0,
                centered: true,
            });
        }
        (tl, th)
    }

    /// Text labels at 1, 2, 3, ... along the x axis.
    fn x_labels(&mut self, labels: &[String]) -> (f64, f64) {
        let (tl, th) = (1.0, (labels.len() as f64).max(2.0));
        for (i, text) in labels.iter().enumerate() {
            let x = self.x_of(i as f64 + 1.0, (tl, th));
            self.prims.push(Prim::Line {
                x1: x,
                y1: self.y1,
                x2: x,
                y2: self.y1 + 4.0,
                color: AXIS,
                width: 1.0,
            });
            self.prims.push(Prim::Text {
                x,
                y: self.y1 + 8.0,
                text: text.clone(),
                color: AXIS,
                size: 1.0,
                centered: true,
            });
        }
        (tl, th)
    }

    fn x_of(&self, v: f64, (tl, th): (f64, f64)) -> f64 {
        self.x0 + (v - tl) / (th - tl) * (self.x1 - self.x0)
    }
}

fn range(vals: impl Iterator<Item = f64>) -> (f64, f64) {
    let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
    for v in vals.filter(|v| v.is_finite()) {
        lo = lo.min(v);
        hi = hi.max(v);
    }
    if lo > hi {
        (0.0, 1.0)
    } else {
        (lo, hi)
    }
}

fn wrap(c: Chart) -> Value {
    Value::Foreign(Rc::new(Box::new(c)))
}

fn xy_chart(ctx: &Ctx, args: &[Value], named: &[(String, Value)], points: bool) -> VmResult<Value> {
    let o = opts(named)?;
    let first = args
        .first()
        .ok_or_else(|| err("CallError", "a chart needs x values"))?;
    // Text on the x axis (`["Jan", "Feb", ...]`): the points go at 1, 2,
    // 3, ... and each is labelled with its text.
    let x_labels: Option<Vec<String>> = {
        let items = ops::collect(first)?;
        (!items.is_empty() && items.iter().all(|v| matches!(v, Value::Str(_))))
            .then(|| labels(ctx, first))
            .transpose()?
    };
    let xs: Vec<f64> = match &x_labels {
        Some(l) => (1..=l.len()).map(|i| i as f64).collect(),
        None => numbers(first, "an x value")?,
    };
    let ys = series(
        ctx,
        args.get(1)
            .ok_or_else(|| err("CallError", "a chart needs y values"))?,
    )?;
    for (n, y) in &ys {
        if y.len() != xs.len() {
            return Err(err(
                "ValueError",
                format!(
                    "{} x values but {} y values{}",
                    xs.len(),
                    y.len(),
                    if n.is_empty() {
                        String::new()
                    } else {
                        format!(" in {n:?}")
                    }
                ),
            ));
        }
    }
    let names: Vec<String> = ys.iter().map(|(n, _)| n.clone()).collect();
    let mut f = Frame::new(&o, &names);
    let yr = range(ys.iter().flat_map(|(_, y)| y.iter().copied()));
    let ymap = f.y_axis(yr.0.min(if points { yr.0 } else { yr.0.min(0.0) }), yr.1);
    let xmap = match &x_labels {
        Some(l) => f.x_labels(l),
        None => f.x_ticks(range(xs.iter().copied()).0, range(xs.iter().copied()).1),
    };
    f.axes();
    for (i, (_, y)) in ys.iter().enumerate() {
        let c = PALETTE[i % PALETTE.len()];
        let pts: Vec<(f64, f64)> = xs
            .iter()
            .zip(y)
            .map(|(x, y)| (f.x_of(*x, xmap), f.y_of(*y, ymap)))
            .collect();
        if points {
            for (x, y) in pts {
                f.prims.push(Prim::Circle {
                    x,
                    y,
                    r: 3.5,
                    color: c,
                });
            }
        } else {
            for w in pts.windows(2) {
                f.prims.push(Prim::Line {
                    x1: w[0].0,
                    y1: w[0].1,
                    x2: w[1].0,
                    y2: w[1].1,
                    color: c,
                    width: 2.0,
                });
            }
            for (x, y) in pts {
                f.prims.push(Prim::Circle {
                    x,
                    y,
                    r: 2.5,
                    color: c,
                });
            }
        }
    }
    Ok(wrap(Chart {
        w: o.w,
        h: o.h,
        prims: f.prims,
    }))
}

/// `chart.line(xs, ys)`: `ys` is a list, or a map of name to list.
pub fn line(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (pos, named) = named(args, 2, XY);
    xy_chart(ctx, &pos, &named, false)
}

/// `chart.scatter(xs, ys)`
pub fn scatter(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (pos, named) = named(args, 2, XY);
    xy_chart(ctx, &pos, &named, true)
}

fn bars(o: &Opts, names: &[String], values: &[(String, Vec<f64>)]) -> Chart {
    let legend: Vec<String> = values.iter().map(|(n, _)| n.clone()).collect();
    let mut f = Frame::new(o, &legend);
    let yr = range(values.iter().flat_map(|(_, v)| v.iter().copied()));
    let ymap = f.y_axis(yr.0.min(0.0), yr.1.max(0.0));
    f.axes();
    let n = names.len().max(1) as f64;
    let slot = (f.x1 - f.x0) / n;
    let groups = values.len().max(1) as f64;
    let bar = slot * 0.7 / groups;
    let zero = f.y_of(0.0, ymap);
    for (i, label) in names.iter().enumerate() {
        let left = f.x0 + slot * i as f64 + slot * 0.15;
        for (s, (_, vals)) in values.iter().enumerate() {
            let v = vals.get(i).copied().unwrap_or(0.0);
            let y = f.y_of(v, ymap);
            f.prims.push(Prim::Rect {
                x: left + bar * s as f64,
                y: y.min(zero),
                w: bar - 1.0,
                h: (zero - y).abs(),
                color: PALETTE[s % PALETTE.len()],
            });
        }
        let short: String = label.chars().take(((slot / 8.0) as usize).max(1)).collect();
        f.prims.push(Prim::Text {
            x: left + slot * 0.35,
            y: f.y1 + 8.0,
            text: short,
            color: AXIS,
            size: 1.0,
            centered: true,
        });
    }
    Chart {
        w: o.w,
        h: o.h,
        prims: f.prims,
    }
}

/// `chart.bar(labels, values)`: `values` is a list, or a map of name to
/// list for grouped bars.
pub fn bar(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (pos, named) = named(args, 2, BAR);
    let o = opts(&named)?;
    let names = labels(
        ctx,
        pos.first()
            .ok_or_else(|| err("CallError", "`chart.bar` needs labels"))?,
    )?;
    let values = series(
        ctx,
        pos.get(1)
            .ok_or_else(|| err("CallError", "`chart.bar` needs values"))?,
    )?;
    for (_, v) in &values {
        if v.len() != names.len() {
            return Err(err(
                "ValueError",
                format!("{} labels but {} values", names.len(), v.len()),
            ));
        }
    }
    Ok(wrap(bars(&o, &names, &values)))
}

/// `chart.hist(values, bins = 10)`: how many values fall in each range.
pub fn hist(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (pos, named) = named(args, 1, HIST);
    let _ = ctx;
    let o = opts(&named)?;
    let vals = numbers(
        pos.first()
            .ok_or_else(|| err("CallError", "`chart.hist` needs values"))?,
        "a value",
    )?;
    let bins = match named.iter().find(|(k, _)| k == "bins").map(|(_, v)| v) {
        Some(v) => super::builtins::integer(v, "the number of bins")?.clamp(1, 200) as usize,
        None => 10,
    };
    let (lo, hi) = range(vals.iter().copied());
    let width = if hi > lo {
        (hi - lo) / bins as f64
    } else {
        1.0
    };
    let mut counts = vec![0f64; bins];
    for v in &vals {
        let i = (((v - lo) / width) as usize).min(bins - 1);
        counts[i] += 1.0;
    }
    let names: Vec<String> = (0..bins)
        .map(|i| tick_text(lo + width * i as f64))
        .collect();
    Ok(wrap(bars(&o, &names, &[(String::new(), counts)])))
}

/// `chart.pie(labels, values)`
pub fn pie(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (pos, named) = named(args, 2, BAR);
    let o = opts(&named)?;
    let names = labels(
        ctx,
        pos.first()
            .ok_or_else(|| err("CallError", "`chart.pie` needs labels"))?,
    )?;
    let vals = numbers(
        pos.get(1)
            .ok_or_else(|| err("CallError", "`chart.pie` needs values"))?,
        "a value",
    )?;
    if vals.iter().any(|v| *v < 0.0) {
        return Err(err(
            "ValueError",
            "a pie chart needs values that are not negative",
        ));
    }
    let total: f64 = vals.iter().sum();
    let legend: Vec<String> = names
        .iter()
        .zip(&vals)
        .map(|(n, v)| {
            format!(
                "{n} ({:.0}%)",
                if total > 0.0 { v / total * 100.0 } else { 0.0 }
            )
        })
        .collect();
    let f = Frame::new(&o, &legend);
    let mut prims = f.prims;
    let (cx, cy) = ((f.x0 + f.x1) / 2.0, (f.y0 + f.y1) / 2.0);
    let r = ((f.x1 - f.x0).min(f.y1 - f.y0) / 2.0 - 4.0).max(10.0);
    let mut angle = -std::f64::consts::FRAC_PI_2;
    for (i, v) in vals.iter().enumerate() {
        let sweep = if total > 0.0 {
            v / total * std::f64::consts::TAU
        } else {
            0.0
        };
        let steps = ((sweep / 0.05).ceil() as usize).max(1);
        let mut points = vec![(cx, cy)];
        for s in 0..=steps {
            let a = angle + sweep * s as f64 / steps as f64;
            points.push((cx + r * a.cos(), cy + r * a.sin()));
        }
        prims.push(Prim::Polygon {
            points,
            color: PALETTE[i % PALETTE.len()],
        });
        angle += sweep;
    }
    Ok(wrap(Chart {
        w: o.w,
        h: o.h,
        prims,
    }))
}

/// The options after the first `start` arguments, named as the builtin's
/// parameters (the VM puts keyword arguments in their places).
fn named(args: &[Value], start: usize, names: &[&str]) -> (Vec<Value>, Vec<(String, Value)>) {
    let pos = args[..start.min(args.len())].to_vec();
    let named = args
        .iter()
        .enumerate()
        .skip(start)
        .filter(|(_, v)| !matches!(v, Value::Missing | Value::None))
        .filter_map(|(i, v)| names.get(i).map(|n| (n.to_string(), v.clone())))
        .collect();
    (pos, named)
}

pub const XY: &[&str] = &["x", "y", "title", "x_label", "y_label", "width", "height"];
pub const BAR: &[&str] = &[
    "labels", "values", "title", "x_label", "y_label", "width", "height",
];
pub const HIST: &[&str] = &[
    "values", "bins", "title", "x_label", "y_label", "width", "height",
];

fn svg_color(c: u32) -> String {
    format!("#{:06x}", c & 0xffffff)
}

fn to_svg(c: &Chart) -> String {
    let esc = super::lib_html::escape_text;
    let mut out = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {w} {h}\" font-family=\"monospace\">\n",
        w = c.w,
        h = c.h
    );
    for p in &c.prims {
        let line = match p {
            Prim::Rect { x, y, w, h, color } => format!("<rect x=\"{x:.1}\" y=\"{y:.1}\" width=\"{w:.1}\" height=\"{h:.1}\" fill=\"{}\"/>", svg_color(*color)),
            Prim::Line { x1, y1, x2, y2, color, width } => format!(
                "<line x1=\"{x1:.1}\" y1=\"{y1:.1}\" x2=\"{x2:.1}\" y2=\"{y2:.1}\" stroke=\"{}\" stroke-width=\"{width}\"/>",
                svg_color(*color)
            ),
            Prim::Circle { x, y, r, color } => format!("<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"{r}\" fill=\"{}\"/>", svg_color(*color)),
            Prim::Polygon { points, color } => format!(
                "<polygon points=\"{}\" fill=\"{}\"/>",
                points.iter().map(|(x, y)| format!("{x:.1},{y:.1}")).collect::<Vec<_>>().join(" "),
                svg_color(*color)
            ),
            Prim::Text { x, y, text, color, size, centered } => format!(
                "<text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"{:.0}\" fill=\"{}\"{}>{}</text>",
                y + 8.0 * size,
                11.0 * size,
                svg_color(*color),
                if *centered { " text-anchor=\"middle\"" } else { "" },
                esc(text)
            ),
        };
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str("</svg>\n");
    out
}

impl Foreign for Chart {
    fn type_name(&self) -> &'static str {
        "Chart"
    }

    fn display(&self) -> String {
        format!("Chart({}x{})", self.w, self.h)
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            // `c.save("chart.png")` or `c.save("chart.svg")`
            "save" => {
                let Some(Value::Str(path)) = args.first() else {
                    return Err(type_error("`save` takes a path ending in .png or .svg"));
                };
                let lower = path.to_lowercase();
                if lower.ends_with(".svg") {
                    std::fs::write(path.as_str(), to_svg(self))
                        .map_err(|e| err("IoError", format!("cannot write {path}: {e}")))?;
                    return Ok(Value::None);
                }
                if !lower.ends_with(".png") {
                    return Err(err(
                        "ValueError",
                        format!("{path:?}: save a chart as .png or .svg"),
                    ));
                }
                #[cfg(feature = "draw")]
                {
                    super::lib_draw::save_prims(
                        self.w as usize,
                        self.h as usize,
                        &self.prims,
                        path,
                    )?;
                    Ok(Value::None)
                }
                #[cfg(not(feature = "draw"))]
                {
                    Err(err(
                        "Unavailable",
                        "PNG charts need the `draw` feature; save as .svg",
                    ))
                }
            }
            "svg" => Ok(Value::str(to_svg(self))),
            _ => Err(err("FieldError", format!("a Chart has no method `{name}`"))),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
