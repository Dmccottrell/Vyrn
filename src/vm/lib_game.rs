//! `use vyrn.game`: the maths of 3D games, independent of drawing.
//!
//! ```vyrn
//! use vyrn.game
//!
//! main:
//!     let walls = [[0, 1, 10, 4, 2, 1], [5, 1, 3, 1, 2, 6]]
//!     let hit = game.raycast([0, 1, 0], [0, 0, 1], walls)
//!     print(hit)    # {"index": 0, "distance": 9.5, "point": [0.0, 1.0, 9.5]}
//! end
//! ```
//!
//! Points and directions are `[x, y, z]` lists. A box is
//! `[x, y, z, width, height, depth]`: its centre and its size, the same way
//! `win.cube` places a cube.

use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::Value;

fn num(v: &Value, what: &str) -> VmResult<f64> {
    match v {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_error(format!(
            "{what} must be a number, not {}",
            other.type_name()
        ))),
    }
}

fn numbers<const N: usize>(v: Option<&Value>, what: &str) -> VmResult<[f64; N]> {
    let items = ops::collect(v.unwrap_or(&Value::None))
        .map_err(|_| type_error(format!("{what} is a list of {N} numbers")))?;
    if items.len() != N {
        return Err(type_error(format!(
            "{what} is a list of {N} numbers, not {}",
            items.len()
        )));
    }
    let mut out = [0.0; N];
    for (o, v) in out.iter_mut().zip(&items) {
        *o = num(v, what)?;
    }
    Ok(out)
}

fn point(p: [f64; 3]) -> Value {
    Value::list(p.iter().map(|v| Value::Float(*v)).collect())
}

/// Where a ray `origin + t * dir` first enters a box, as `t`, if it does.
fn ray_box(origin: [f64; 3], dir: [f64; 3], b: [f64; 6]) -> Option<f64> {
    let (mut near, mut far) = (f64::NEG_INFINITY, f64::INFINITY);
    for axis in 0..3 {
        let lo = b[axis] - b[axis + 3] / 2.0;
        let hi = b[axis] + b[axis + 3] / 2.0;
        if dir[axis].abs() < 1e-12 {
            if origin[axis] < lo || origin[axis] > hi {
                return None;
            }
            continue;
        }
        let (mut t1, mut t2) = (
            (lo - origin[axis]) / dir[axis],
            (hi - origin[axis]) / dir[axis],
        );
        if t1 > t2 {
            std::mem::swap(&mut t1, &mut t2);
        }
        near = near.max(t1);
        far = far.min(t2);
        if near > far {
            return None;
        }
    }
    if far < 0.0 {
        return None;
    }
    Some(near.max(0.0))
}

/// `game.raycast(origin, direction, boxes, max_distance)`: the first box a
/// ray hits, as `{"index", "distance", "point"}`, or `none`. This is how
/// a shot finds what it hits, or a character what's in front of it.
pub fn raycast(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let origin: [f64; 3] = numbers(args.first(), "the origin")?;
    let mut dir: [f64; 3] = numbers(args.get(1), "the direction")?;
    let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt();
    if len == 0.0 {
        return Err(err("ValueError", "the direction cannot be [0, 0, 0]"));
    }
    for d in &mut dir {
        *d /= len;
    }
    let boxes = ops::collect(args.get(2).unwrap_or(&Value::None))?;
    let max = match args.get(3) {
        None | Some(Value::None | Value::Missing) => f64::INFINITY,
        Some(v) => num(v, "the maximum distance")?,
    };
    let mut best: Option<(usize, f64)> = None;
    for (i, b) in boxes.iter().enumerate() {
        let b: [f64; 6] = numbers(Some(b), "a box ([x, y, z, width, height, depth])")?;
        if let Some(t) = ray_box(origin, dir, b) {
            if t <= max && best.is_none_or(|(_, bt)| t < bt) {
                best = Some((i, t));
            }
        }
    }
    let Some((index, t)) = best else {
        return Ok(Value::None);
    };
    let hit = [
        origin[0] + dir[0] * t,
        origin[1] + dir[1] * t,
        origin[2] + dir[2] * t,
    ];
    let mut map = super::value::Map::default();
    for (k, v) in [
        ("index", Value::Int(index as i64)),
        ("distance", Value::Float(t)),
        ("point", point(hit)),
    ] {
        let key = Value::str(k);
        if let Some(h) = super::value::Key::from_value(&key) {
            map.insert(h, key, v);
        }
    }
    Ok(Value::Map(std::rc::Rc::new(std::cell::RefCell::new(map))))
}

/// `game.overlaps3d(a, b)`: whether two boxes touch.
pub fn overlaps3d(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let a: [f64; 6] = numbers(args.first(), "a box ([x, y, z, width, height, depth])")?;
    let b: [f64; 6] = numbers(args.get(1), "a box ([x, y, z, width, height, depth])")?;
    Ok(Value::Bool(
        (0..3).all(|i| (a[i] - b[i]).abs() * 2.0 < a[i + 3] + b[i + 3]),
    ))
}

/// `game.distance(a, b)`: between two points.
pub fn distance(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let a: [f64; 3] = numbers(args.first(), "a point")?;
    let b: [f64; 3] = numbers(args.get(1), "a point")?;
    Ok(Value::Float(
        ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt(),
    ))
}

/// `game.normalize(v)`: the same direction with length 1.
pub fn normalize(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let v: [f64; 3] = numbers(args.first(), "a direction")?;
    let len = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    if len == 0.0 {
        return Ok(point([0.0, 0.0, 0.0]));
    }
    Ok(point([v[0] / len, v[1] / len, v[2] / len]))
}

/// `game.slide(position, move, boxes, size)`: where something of `size`
/// ([width, height, depth]) ends up moving by `move` from `position`
/// without passing through any box. Each axis is tried on its own, so
/// walking into a wall at an angle slides along it, as players expect.
pub fn slide(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut pos: [f64; 3] = numbers(args.first(), "the position")?;
    let step: [f64; 3] = numbers(args.get(1), "the movement")?;
    let boxes: Vec<[f64; 6]> = ops::collect(args.get(2).unwrap_or(&Value::None))?
        .iter()
        .map(|b| numbers(Some(b), "a box ([x, y, z, width, height, depth])"))
        .collect::<VmResult<_>>()?;
    let size: [f64; 3] = match args.get(3) {
        None | Some(Value::None | Value::Missing) => [0.5, 1.8, 0.5],
        v => numbers(v, "the size")?,
    };
    for axis in 0..3 {
        let mut next = pos;
        next[axis] += step[axis];
        let me = [next[0], next[1], next[2], size[0], size[1], size[2]];
        let blocked = boxes
            .iter()
            .any(|b| (0..3).all(|i| (me[i] - b[i]).abs() * 2.0 < me[i + 3] + b[i + 3]));
        if !blocked {
            pos = next;
        }
    }
    Ok(point(pos))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rays_hit_the_nearest_face() {
        let b = [0.0, 0.0, 10.0, 2.0, 2.0, 2.0];
        assert_eq!(ray_box([0.0, 0.0, 0.0], [0.0, 0.0, 1.0], b), Some(9.0));
        assert_eq!(ray_box([0.0, 0.0, 0.0], [0.0, 0.0, -1.0], b), None);
        assert_eq!(ray_box([5.0, 0.0, 0.0], [0.0, 0.0, 1.0], b), None);
        assert_eq!(ray_box([0.0, 0.0, 10.0], [1.0, 0.0, 0.0], b), Some(0.0));
    }
}
