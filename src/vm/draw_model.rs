//! `draw.load_model("soldier.glb")`: animated 3D models in glTF, the format
//! Blender, Mixamo and most 3D tools export.
//!
//! A model has meshes, a texture, and often a skeleton with animations
//! ("idle", "run", "shoot"). `model.mesh("run", t)` poses it at `t` seconds
//! into the animation: each joint is placed from the animation's keys, and
//! each point of the skin follows the joints it is weighted to (linear
//! blend skinning, done on the CPU). The result is an ordinary `Mesh`, so
//! it is lit, textured and drawn like any other: `win.model(...)` does both.

use std::rc::Rc;

use super::{normalize, wrap, Image, Mesh};
use crate::vm::foreign::Foreign;
use crate::vm::ops::{err, type_error, Ctx, VmResult};
use crate::vm::value::Value;

/// A 4-by-4 matrix, column by column (as glTF stores them).
type Mat = [f32; 16];

const IDENTITY: Mat = [
    1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1., 0., 0., 0., 0., 1.,
];

fn mul(a: &Mat, b: &Mat) -> Mat {
    let mut out = [0.0; 16];
    for c in 0..4 {
        for r in 0..4 {
            out[c * 4 + r] = (0..4).map(|k| a[k * 4 + r] * b[c * 4 + k]).sum();
        }
    }
    out
}

fn transform_point(m: &Mat, p: [f32; 3]) -> [f32; 3] {
    [
        m[0] * p[0] + m[4] * p[1] + m[8] * p[2] + m[12],
        m[1] * p[0] + m[5] * p[1] + m[9] * p[2] + m[13],
        m[2] * p[0] + m[6] * p[1] + m[10] * p[2] + m[14],
    ]
}

fn transform_dir(m: &Mat, d: [f32; 3]) -> [f32; 3] {
    [
        m[0] * d[0] + m[4] * d[1] + m[8] * d[2],
        m[1] * d[0] + m[5] * d[1] + m[9] * d[2],
        m[2] * d[0] + m[6] * d[1] + m[10] * d[2],
    ]
}

/// Translation, rotation (quaternion x, y, z, w) and scale.
#[derive(Clone, Copy, Debug)]
struct Trs {
    t: [f32; 3],
    r: [f32; 4],
    s: [f32; 3],
}

impl Trs {
    fn matrix(&self) -> Mat {
        let [x, y, z, w] = self.r;
        let (xx, yy, zz) = (x * x, y * y, z * z);
        let (xy, xz, yz) = (x * y, x * z, y * z);
        let (wx, wy, wz) = (w * x, w * y, w * z);
        let [sx, sy, sz] = self.s;
        [
            (1.0 - 2.0 * (yy + zz)) * sx,
            (2.0 * (xy + wz)) * sx,
            (2.0 * (xz - wy)) * sx,
            0.0,
            (2.0 * (xy - wz)) * sy,
            (1.0 - 2.0 * (xx + zz)) * sy,
            (2.0 * (yz + wx)) * sy,
            0.0,
            (2.0 * (xz + wy)) * sz,
            (2.0 * (yz - wx)) * sz,
            (1.0 - 2.0 * (xx + yy)) * sz,
            0.0,
            self.t[0],
            self.t[1],
            self.t[2],
            1.0,
        ]
    }
}

fn lerp3(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

/// Spherical interpolation between rotations, the short way round.
fn slerp(a: [f32; 4], b: [f32; 4], t: f32) -> [f32; 4] {
    let mut dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2] + a[3] * b[3];
    let b = if dot < 0.0 {
        dot = -dot;
        [-b[0], -b[1], -b[2], -b[3]]
    } else {
        b
    };
    let (wa, wb) = if dot > 0.9995 {
        (1.0 - t, t)
    } else {
        let theta = dot.acos();
        let sin = theta.sin();
        (((1.0 - t) * theta).sin() / sin, (t * theta).sin() / sin)
    };
    let q = [
        wa * a[0] + wb * b[0],
        wa * a[1] + wb * b[1],
        wa * a[2] + wb * b[2],
        wa * a[3] + wb * b[3],
    ];
    let n = (q[0] * q[0] + q[1] * q[1] + q[2] * q[2] + q[3] * q[3])
        .sqrt()
        .max(1e-9);
    [q[0] / n, q[1] / n, q[2] / n, q[3] / n]
}

#[derive(Clone, Copy, PartialEq)]
enum Property {
    Translation,
    Rotation,
    Scale,
}

/// One animated property of one node: key times and values.
struct Channel {
    node: usize,
    property: Property,
    times: Vec<f32>,
    /// Three or four numbers per key.
    values: Vec<[f32; 4]>,
    step: bool,
}

impl Channel {
    /// The value at time `t` (held before the first key and after the last).
    fn sample(&self, t: f32) -> [f32; 4] {
        let n = self.times.len();
        if n == 0 {
            return [0.0; 4];
        }
        if t <= self.times[0] || n == 1 {
            return self.values[0];
        }
        if t >= self.times[n - 1] {
            return self.values[n - 1];
        }
        let i = self
            .times
            .partition_point(|k| *k <= t)
            .saturating_sub(1)
            .min(n - 2);
        if self.step {
            return self.values[i];
        }
        let span = (self.times[i + 1] - self.times[i]).max(1e-9);
        let f = (t - self.times[i]) / span;
        let (a, b) = (self.values[i], self.values[i + 1]);
        match self.property {
            Property::Rotation => slerp(a, b, f),
            _ => {
                let v = lerp3([a[0], a[1], a[2]], [b[0], b[1], b[2]], f);
                [v[0], v[1], v[2], 0.0]
            }
        }
    }
}

struct Animation {
    name: String,
    duration: f32,
    channels: Vec<Channel>,
}

/// A piece of a mesh: its points, and how they hang on the skeleton.
struct Part {
    /// The node it is attached to, for models without a skin.
    node: usize,
    skin: Option<usize>,
    positions: Vec<[f32; 3]>,
    normals: Vec<[f32; 3]>,
    uvs: Vec<[f64; 2]>,
    joints: Vec<[u16; 4]>,
    weights: Vec<[f32; 4]>,
    indices: Vec<u32>,
}

struct Skin {
    /// The node of each joint, and the matrix from model space to it.
    joints: Vec<usize>,
    inverse_bind: Vec<Mat>,
}

/// A loaded model: see the module comment.
pub struct Model {
    rest: Vec<Trs>,
    parents: Vec<Option<usize>>,
    parts: Vec<Part>,
    skins: Vec<Skin>,
    animations: Vec<Animation>,
    texture: Option<Value>,
}

impl Model {
    /// Every node's place in the model at time `t` of `animation`.
    fn pose(&self, animation: Option<&Animation>, t: f32) -> Vec<Mat> {
        let mut local = self.rest.clone();
        if let Some(a) = animation {
            for ch in &a.channels {
                let v = ch.sample(t);
                let trs = &mut local[ch.node];
                match ch.property {
                    Property::Translation => trs.t = [v[0], v[1], v[2]],
                    Property::Rotation => trs.r = v,
                    Property::Scale => trs.s = [v[0], v[1], v[2]],
                }
            }
        }
        let mut global: Vec<Option<Mat>> = vec![None; local.len()];
        fn place(
            i: usize,
            local: &[Trs],
            parents: &[Option<usize>],
            global: &mut [Option<Mat>],
            depth: usize,
        ) -> Mat {
            if let Some(m) = global[i] {
                return m;
            }
            let own = local[i].matrix();
            let m = match parents[i] {
                Some(p) if depth < 256 => mul(&place(p, local, parents, global, depth + 1), &own),
                _ => own,
            };
            global[i] = Some(m);
            m
        }
        (0..local.len())
            .map(|i| place(i, &local, &self.parents, &mut global, 0))
            .collect()
    }

    /// The model posed at `t` seconds into `animation` (or at rest), as
    /// one mesh.
    fn posed(&self, animation: Option<&Animation>, t: f32) -> Mesh {
        let nodes = self.pose(animation, t);
        let joint_mats: Vec<Vec<Mat>> = self
            .skins
            .iter()
            .map(|s| {
                s.joints
                    .iter()
                    .zip(&s.inverse_bind)
                    .map(|(j, ib)| mul(&nodes[*j], ib))
                    .collect()
            })
            .collect();
        let mut mesh = Mesh::new(Vec::new(), Vec::new());
        mesh.texture = self.texture.clone();
        let mut any_uv = false;
        for part in &self.parts {
            let base = mesh.verts.len();
            for (i, p) in part.positions.iter().enumerate() {
                let n = part.normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]);
                let (p, n) = match part.skin.and_then(|s| joint_mats.get(s)) {
                    Some(mats) if !part.joints.is_empty() => {
                        let (js, ws) = (part.joints[i], part.weights[i]);
                        let mut out_p = [0.0f32; 3];
                        let mut out_n = [0.0f32; 3];
                        let total: f32 = ws.iter().sum();
                        for k in 0..4 {
                            let w = if total > 0.0 { ws[k] / total } else { 0.0 };
                            if w == 0.0 {
                                continue;
                            }
                            let Some(m) = mats.get(js[k] as usize) else {
                                continue;
                            };
                            let q = transform_point(m, *p);
                            let d = transform_dir(m, n);
                            for c in 0..3 {
                                out_p[c] += q[c] * w;
                                out_n[c] += d[c] * w;
                            }
                        }
                        (out_p, out_n)
                    }
                    _ => (
                        transform_point(&nodes[part.node], *p),
                        transform_dir(&nodes[part.node], n),
                    ),
                };
                mesh.verts
                    .push([f64::from(p[0]), f64::from(p[1]), f64::from(p[2])]);
                mesh.normals.push(normalize([
                    f64::from(n[0]),
                    f64::from(n[1]),
                    f64::from(n[2]),
                ]));
                mesh.uvs
                    .push(part.uvs.get(i).copied().unwrap_or([0.0, 0.0]));
            }
            any_uv |= !part.uvs.is_empty();
            for tri in part.indices.chunks_exact(3) {
                mesh.faces.push([
                    base + tri[0] as usize,
                    base + tri[1] as usize,
                    base + tri[2] as usize,
                ]);
            }
        }
        if !any_uv {
            mesh.uvs.clear();
        }
        mesh
    }

    fn animation(&self, v: Option<&Value>) -> VmResult<Option<&Animation>> {
        match v {
            None | Some(Value::None) | Some(Value::Missing) => Ok(None),
            Some(Value::Str(name)) => self
                .animations
                .iter()
                .find(|a| a.name == **name)
                .map(Some)
                .ok_or_else(|| {
                    let names: Vec<&str> =
                        self.animations.iter().map(|a| a.name.as_str()).collect();
                    err(
                        "ValueError",
                        if names.is_empty() {
                            format!("this model has no animations (asked for {name:?})")
                        } else {
                            format!(
                                "this model has no animation {name:?}; it has {}",
                                names.join(", ")
                            )
                        },
                    )
                }),
            Some(Value::Int(i)) => self.animations.get(*i as usize).map(Some).ok_or_else(|| {
                err(
                    "IndexError",
                    format!("this model has {} animations", self.animations.len()),
                )
            }),
            Some(other) => Err(type_error(format!(
                "an animation is named by text, not {}",
                other.type_name()
            ))),
        }
    }

    /// `animation` at `time` seconds; looping by default.
    pub fn mesh_at(&self, animation: Option<&Value>, time: f64, looping: bool) -> VmResult<Mesh> {
        let anim = self.animation(animation)?;
        let mut t = time as f32;
        if let Some(a) = anim {
            if looping && a.duration > 0.0 {
                t = t.rem_euclid(a.duration);
            }
        }
        Ok(self.posed(anim, t))
    }
}

impl Foreign for Model {
    fn type_name(&self) -> &'static str {
        "Model"
    }

    fn display(&self) -> String {
        let points: usize = self.parts.iter().map(|p| p.positions.len()).sum();
        format!(
            "Model({points} points, {} animations)",
            self.animations.len()
        )
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "animations" => Ok(Value::list(
                self.animations
                    .iter()
                    .map(|a| Value::str(a.name.clone()))
                    .collect(),
            )),
            "joints" => Ok(Value::Int(
                self.skins.iter().map(|s| s.joints.len()).sum::<usize>() as i64,
            )),
            "texture" => Ok(self.texture.clone().unwrap_or(Value::None)),
            _ => Err(err("FieldError", format!("a Model has no field `{name}`"))),
        }
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        args: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let get = |i: usize, key: &str| {
            named
                .iter()
                .find(|(n, _)| n == key)
                .map(|(_, v)| v)
                .or(args.get(i))
        };
        match name {
            // `model.duration("run")`: how long it lasts, in seconds.
            "duration" => Ok(Value::Float(
                self.animation(get(0, "animation"))?
                    .map_or(0.0, |a| f64::from(a.duration)),
            )),
            // `model.mesh("run", t, loop = true)`: posed, as a Mesh.
            "mesh" => {
                let time = match get(1, "time") {
                    Some(v) => crate::vm::builtins::number(v, "the time")?,
                    None => 0.0,
                };
                let looping = get(2, "loop").is_none_or(Value::truthy);
                Ok(wrap(self.mesh_at(get(0, "animation"), time, looping)?))
            }
            _ => Err(err("FieldError", format!("a Model has no method `{name}`"))),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `draw.load_model(path)`: a glTF model (`.gltf` with its files, or
/// one `.glb`), with its skeleton, animations and first texture.
pub fn load_model(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        _ => return Err(type_error("`draw.load_model` takes the file's path")),
    };
    let (doc, buffers, images) = gltf::import(&path).map_err(|e| {
        err(
            "ValueError",
            format!("{path} is not a glTF model this can read: {e}"),
        )
    })?;
    let nodes: Vec<gltf::Node> = doc.nodes().collect();
    let mut parents = vec![None; nodes.len()];
    for node in &nodes {
        for child in node.children() {
            parents[child.index()] = Some(node.index());
        }
    }
    let rest = nodes
        .iter()
        .map(|n| {
            let (t, r, s) = n.transform().decomposed();
            Trs { t, r, s }
        })
        .collect();
    let read = |b: gltf::Buffer| buffers.get(b.index()).map(|d| &d.0[..]);
    let mut parts = Vec::new();
    for node in &nodes {
        let Some(mesh) = node.mesh() else { continue };
        for prim in mesh.primitives() {
            if prim.mode() != gltf::mesh::Mode::Triangles {
                continue;
            }
            let r = prim.reader(read);
            let Some(positions) = r.read_positions() else {
                continue;
            };
            let positions: Vec<[f32; 3]> = positions.collect();
            let indices = match r.read_indices() {
                Some(i) => i.into_u32().collect(),
                None => (0..positions.len() as u32).collect(),
            };
            parts.push(Part {
                node: node.index(),
                skin: node.skin().map(|s| s.index()),
                normals: r.read_normals().map(|n| n.collect()).unwrap_or_default(),
                uvs: r
                    .read_tex_coords(0)
                    .map(|t| {
                        t.into_f32()
                            .map(|[u, v]| [f64::from(u), f64::from(v)])
                            .collect()
                    })
                    .unwrap_or_default(),
                joints: r
                    .read_joints(0)
                    .map(|j| j.into_u16().collect())
                    .unwrap_or_default(),
                weights: r
                    .read_weights(0)
                    .map(|w| w.into_f32().collect())
                    .unwrap_or_default(),
                indices,
                positions,
            });
        }
    }
    if parts.is_empty() {
        return Err(err(
            "ValueError",
            format!("{path} has no triangles to draw"),
        ));
    }
    let skins = doc
        .skins()
        .map(|s| {
            let joints: Vec<usize> = s.joints().map(|j| j.index()).collect();
            let inverse_bind = s
                .reader(read)
                .read_inverse_bind_matrices()
                .map(|m| {
                    m.map(|c| {
                        let mut out = [0.0; 16];
                        for (k, col) in c.iter().enumerate() {
                            out[k * 4..k * 4 + 4].copy_from_slice(col);
                        }
                        out
                    })
                    .collect()
                })
                .unwrap_or_else(|| vec![IDENTITY; joints.len()]);
            Skin {
                joints,
                inverse_bind,
            }
        })
        .collect();
    let animations = doc
        .animations()
        .enumerate()
        .map(|(i, a)| {
            let mut duration = 0.0f32;
            let channels = a
                .channels()
                .filter_map(|c| {
                    let r = c.reader(read);
                    let times: Vec<f32> = r.read_inputs()?.collect();
                    let cubic =
                        c.sampler().interpolation() == gltf::animation::Interpolation::CubicSpline;
                    use gltf::animation::util::ReadOutputs;
                    let (property, mut values): (Property, Vec<[f32; 4]>) = match r
                        .read_outputs()?
                    {
                        ReadOutputs::Translations(t) => (
                            Property::Translation,
                            t.map(|v| [v[0], v[1], v[2], 0.0]).collect(),
                        ),
                        ReadOutputs::Rotations(q) => (Property::Rotation, q.into_f32().collect()),
                        ReadOutputs::Scales(s) => (
                            Property::Scale,
                            s.map(|v| [v[0], v[1], v[2], 0.0]).collect(),
                        ),
                        ReadOutputs::MorphTargetWeights(_) => return None,
                    };
                    if cubic {
                        // In-tangent, value, out-tangent per key: keep the values.
                        values = values.chunks(3).filter_map(|c| c.get(1).copied()).collect();
                    }
                    if values.len() != times.len() {
                        return None;
                    }
                    duration = duration.max(times.last().copied().unwrap_or(0.0));
                    Some(Channel {
                        node: c.target().node().index(),
                        property,
                        times,
                        values,
                        step: c.sampler().interpolation() == gltf::animation::Interpolation::Step,
                    })
                })
                .collect();
            Animation {
                name: a
                    .name()
                    .map_or_else(|| format!("animation {i}"), str::to_string),
                duration,
                channels,
            }
        })
        .collect();
    // The first textured material's picture.
    let texture = doc
        .materials()
        .find_map(|m| m.pbr_metallic_roughness().base_color_texture())
        .and_then(|t| images.get(t.texture().source().index()))
        .and_then(image_of)
        .map(wrap);
    Ok(Value::Foreign(Rc::new(Box::new(Model {
        rest,
        parents,
        parts,
        skins,
        animations,
        texture,
    }))))
}

/// A decoded glTF picture as an `Image`.
fn image_of(data: &gltf::image::Data) -> Option<Image> {
    use gltf::image::Format;
    let (w, h) = (data.width as usize, data.height as usize);
    let channels = match data.format {
        Format::R8 => 1,
        Format::R8G8 => 2,
        Format::R8G8B8 => 3,
        Format::R8G8B8A8 => 4,
        _ => return None,
    };
    let px = data
        .pixels
        .chunks_exact(channels)
        .map(|p| {
            let (r, g, b, a) = match channels {
                1 => (p[0], p[0], p[0], 255),
                2 => (p[0], p[0], p[0], p[1]),
                3 => (p[0], p[1], p[2], 255),
                _ => (p[0], p[1], p[2], p[3]),
            };
            ((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b), a)
        })
        .collect();
    Some(Image::new(w, h, px))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotations_and_matrices() {
        // A quarter turn about y takes +x to -z.
        let s = std::f32::consts::FRAC_1_SQRT_2;
        let m = Trs {
            t: [1.0, 0.0, 0.0],
            r: [0.0, s, 0.0, s],
            s: [1.0; 3],
        }
        .matrix();
        let p = transform_point(&m, [1.0, 0.0, 0.0]);
        assert!(
            (p[0] - 1.0).abs() < 1e-5 && p[1].abs() < 1e-5 && (p[2] + 1.0).abs() < 1e-5,
            "{p:?}"
        );
        // Half way between no turn and a half turn about z is a quarter turn.
        let q = slerp([0.0, 0.0, 0.0, 1.0], [0.0, 0.0, 1.0, 0.0], 0.5);
        assert!((q[2] - s).abs() < 1e-5 && (q[3] - s).abs() < 1e-5, "{q:?}");
        assert_eq!(mul(&IDENTITY, &m), m);
    }

    #[test]
    fn channels_hold_and_interpolate() {
        let ch = Channel {
            node: 0,
            property: Property::Translation,
            times: vec![0.0, 1.0, 2.0],
            values: vec![[0.0; 4], [2.0, 0.0, 0.0, 0.0], [2.0, 4.0, 0.0, 0.0]],
            step: false,
        };
        assert_eq!(ch.sample(-1.0)[0], 0.0);
        assert_eq!(ch.sample(0.5)[0], 1.0);
        assert_eq!(ch.sample(1.5)[1], 2.0);
        assert_eq!(ch.sample(9.0)[1], 4.0);
    }
}
