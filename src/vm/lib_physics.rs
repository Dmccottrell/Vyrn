//! `use vyrn.physics`: a physics engine for games, built on Rapier.
//!
//! ```vyrn
//! let world = physics.world()
//! let floor = world.box(0, -0.5, 0, 20, 1, 20, fixed = true)
//! let crate = world.box(0, 5, 0, 1, 1, 1, mass = 2)
//! loop win.open():
//!     world.step(dt)
//!     win.cube(cam, crate.x, crate.y, crate.z, 1, "brown", rotation = crate.rotation)
//! end
//! ```
//!
//! Bodies are boxes, balls and capsules. They fall, bounce, slide and
//! tumble; `fixed` ones (floors, walls) never move, and `kinematic` ones
//! move only where the program puts them (lifts, doors). A **character**
//! is a capsule that walks: `move` slides it along walls, climbs steps and
//! keeps it on the ground, which is what an FPS player needs.
//!
//! Units are metres, kilograms and seconds; y is up.

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::{Rc, Weak};

use rapier3d::control::{CharacterAutostep, CharacterLength, KinematicCharacterController};
use rapier3d::prelude::*;

use super::builtins::number;
use super::foreign::Foreign;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::value::Value;

type ForeignRc = Rc<Box<dyn Foreign>>;

struct Inner {
    world: PhysicsWorld,
    /// The Vyrn value of each body, to give the same one back from
    /// raycasts and collisions. The world owns its bodies; a body only
    /// points back at its world, so there is no cycle.
    bodies: HashMap<RigidBodyHandle, ForeignRc>,
    events: std::sync::mpsc::Receiver<CollisionEvent>,
    collector: ChannelEventCollector,
    /// Pairs that started touching during the last step.
    started: Vec<(RigidBodyHandle, RigidBodyHandle)>,
    /// Bodies were added, removed or moved by hand since collision
    /// detection last ran, so queries must refresh it first.
    dirty: bool,
    /// Bodies added or moved by hand since the last step. Refreshing
    /// collision detection early takes Rapier's note of their changes, so
    /// they are noted again for the step (or new bodies would never wake).
    touched: Vec<RigidBodyHandle>,
    _forces: std::sync::mpsc::Receiver<ContactForceEvent>,
    _tears: std::sync::mpsc::Receiver<SoftBodyTearEvent>,
}

impl Inner {
    /// Brings collision detection up to date with bodies added or moved
    /// since the last step, so walking and ray casts see them.
    fn refresh(&mut self) {
        if self.dirty {
            self.world.detect_collisions(&(), &());
            self.dirty = false;
            for h in &self.touched {
                // `get_mut` notes the body as changed again.
                let _ = self.world.bodies.get_mut(*h);
            }
        }
    }

    fn touch(&mut self, h: RigidBodyHandle) {
        self.dirty = true;
        if !self.touched.contains(&h) {
            self.touched.push(h);
        }
    }
}

/// `physics.world(gravity = -9.81)`: an empty world to add bodies to.
pub struct World {
    inner: Rc<RefCell<Inner>>,
}

/// A body in a world: a box, ball or capsule, or a character.
pub struct Body {
    world: Weak<RefCell<Inner>>,
    handle: RigidBodyHandle,
    collider: ColliderHandle,
    kind: &'static str,
    name: String,
    /// Half its width, height and depth (for drawing), or radius and half
    /// height for balls and capsules.
    size: [f64; 3],
    /// Characters only: how they walk.
    controller: Option<KinematicCharacterController>,
    grounded: std::cell::Cell<bool>,
}

fn v3(x: f64, y: f64, z: f64) -> Vector {
    Vector::new(x as f32, y as f32, z as f32)
}

fn list3(v: Vector) -> Value {
    Value::list(vec![
        Value::Float(round(v.x)),
        Value::Float(round(v.y)),
        Value::Float(round(v.z)),
    ])
}

/// f32 positions shown as the decimals people expect (0.1, not
/// 0.10000000149).
fn round(x: f32) -> f64 {
    let x = f64::from(x);
    (x * 1e5).round() / 1e5
}

fn point(v: &Value, what: &str) -> VmResult<Vector> {
    let items = ops::collect(v)?;
    match items.as_slice() {
        [x, y, z] => Ok(v3(number(x, what)?, number(y, what)?, number(z, what)?)),
        _ => Err(type_error(format!("{what} is [x, y, z]"))),
    }
}

/// Keyword or positional argument `i`.
fn arg<'a>(
    pos: &'a [Value],
    named: &'a [(String, Value)],
    i: usize,
    name: &str,
) -> Option<&'a Value> {
    named
        .iter()
        .find(|(n, _)| n == name)
        .map(|(_, v)| v)
        .or_else(|| pos.get(i))
        .filter(|v| !matches!(v, Value::Missing))
}

fn num(
    pos: &[Value],
    named: &[(String, Value)],
    i: usize,
    name: &str,
    default: Option<f64>,
) -> VmResult<f64> {
    match arg(pos, named, i, name) {
        Some(v) if !matches!(v, Value::None) => number(v, &format!("`{name}`")),
        _ => default.ok_or_else(|| err("CallError", format!("needs `{name}`"))),
    }
}

pub fn world(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let gravity = match args.first() {
        Some(Value::None) | Some(Value::Missing) | None => -9.81,
        Some(v) => number(v, "`gravity`")?,
    };
    let mut world = PhysicsWorld::new();
    world.gravity = v3(0.0, gravity, 0.0);
    let (send, events) = std::sync::mpsc::channel();
    let (force_send, forces) = std::sync::mpsc::channel();
    let (tear_send, tears) = std::sync::mpsc::channel();
    let inner = Inner {
        world,
        bodies: HashMap::new(),
        events,
        collector: ChannelEventCollector::new(send, force_send, tear_send),
        started: Vec::new(),
        dirty: true,
        touched: Vec::new(),
        _forces: forces,
        _tears: tears,
    };
    Ok(Value::Foreign(Rc::new(Box::new(World {
        inner: Rc::new(RefCell::new(inner)),
    }))))
}

impl World {
    /// Adds a body made from a shape at (x, y, z).
    #[allow(clippy::too_many_arguments)]
    fn add(
        &self,
        kind: &'static str,
        shape: ColliderBuilder,
        size: [f64; 3],
        at: Vector,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let flag = |name: &str| arg(pos, named, usize::MAX, name).is_some_and(Value::truthy);
        let option = |name: &str, default: f64| -> VmResult<f64> {
            num(pos, named, usize::MAX, name, Some(default))
        };
        let body = if flag("fixed") {
            RigidBodyBuilder::fixed()
        } else if flag("kinematic") {
            RigidBodyBuilder::kinematic_position_based()
        } else {
            RigidBodyBuilder::dynamic().ccd_enabled(true)
        }
        .translation(at)
        .linear_damping(option("damping", 0.0)? as f32)
        .angular_damping(option("spin_damping", 0.05)? as f32);
        let body = if flag("upright") {
            body.lock_rotations()
        } else {
            body
        };
        let mut collider = shape
            .restitution(option("bounce", 0.0)? as f32)
            .friction(option("friction", 0.6)? as f32)
            .active_events(ActiveEvents::COLLISION_EVENTS);
        if let Some(mass) =
            arg(pos, named, usize::MAX, "mass").filter(|v| !matches!(v, Value::None))
        {
            let mass = number(mass, "`mass`")?;
            if mass <= 0.0 {
                return Err(err("ValueError", "`mass` must be more than 0"));
            }
            collider = collider.mass(mass as f32);
        }
        if flag("sensor") {
            collider = collider.sensor(true);
        }
        let name = match arg(pos, named, usize::MAX, "name") {
            Some(Value::Str(s)) => s.to_string(),
            _ => String::new(),
        };
        let (handle, collider) = self.inner.borrow_mut().world.insert(body, collider);
        let value: ForeignRc = Rc::new(Box::new(Body {
            world: Rc::downgrade(&self.inner),
            handle,
            collider,
            kind,
            name,
            size,
            controller: None,
            grounded: std::cell::Cell::new(false),
        }));
        let mut inner = self.inner.borrow_mut();
        inner.bodies.insert(handle, value.clone());
        inner.touch(handle);
        Ok(Value::Foreign(value))
    }

    fn body_value(inner: &Inner, handle: RigidBodyHandle) -> Value {
        inner
            .bodies
            .get(&handle)
            .cloned()
            .map_or(Value::None, Value::Foreign)
    }
}

fn body_of(v: Option<&Value>, what: &str) -> VmResult<RigidBodyHandle> {
    match v {
        Some(Value::Foreign(f)) => f
            .as_any()
            .downcast_ref::<Body>()
            .map(|b| b.handle)
            .ok_or_else(|| type_error(format!("{what} takes a body"))),
        _ => Err(type_error(format!("{what} takes a body"))),
    }
}

impl Foreign for World {
    fn type_name(&self) -> &'static str {
        "PhysicsWorld"
    }

    fn display(&self) -> String {
        format!(
            "<physics world with {} bodies>",
            self.inner.borrow().world.bodies.len()
        )
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        let inner = self.inner.borrow();
        match name {
            "gravity" => Ok(Value::Float(round(inner.world.gravity.y))),
            "bodies" => {
                // In the order they were added.
                let mut all: Vec<_> = inner.bodies.iter().collect();
                all.sort_by_key(|(h, _)| h.into_raw_parts());
                Ok(Value::list(
                    all.into_iter()
                        .map(|(_, v)| Value::Foreign(v.clone()))
                        .collect(),
                ))
            }
            _ => Err(err(
                "FieldError",
                format!("a physics world has no `{name}`"),
            )),
        }
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let n = |i: usize, name: &str, default: Option<f64>| num(pos, named, i, name, default);
        match name {
            // `world.box(x, y, z, width, height, depth, mass = 1, fixed = false, ...)`
            "box" => {
                let at = v3(n(0, "x", None)?, n(1, "y", None)?, n(2, "z", None)?);
                let (w, h, d) = (
                    n(3, "width", Some(1.0))?,
                    n(4, "height", Some(1.0))?,
                    n(5, "depth", Some(1.0))?,
                );
                if w <= 0.0 || h <= 0.0 || d <= 0.0 {
                    return Err(err(
                        "ValueError",
                        "a box needs a width, height and depth above 0",
                    ));
                }
                let shape =
                    ColliderBuilder::cuboid((w / 2.0) as f32, (h / 2.0) as f32, (d / 2.0) as f32);
                self.add("box", shape, [w / 2.0, h / 2.0, d / 2.0], at, pos, named)
            }
            // `world.ball(x, y, z, radius = 0.5, ...)`
            "ball" | "sphere" => {
                let at = v3(n(0, "x", None)?, n(1, "y", None)?, n(2, "z", None)?);
                let r = n(3, "radius", Some(0.5))?;
                if r <= 0.0 {
                    return Err(err("ValueError", "a ball needs a radius above 0"));
                }
                self.add(
                    "ball",
                    ColliderBuilder::ball(r as f32),
                    [r, r, r],
                    at,
                    pos,
                    named,
                )
            }
            // `world.capsule(x, y, z, radius = 0.4, height = 1.8, ...)`: upright.
            "capsule" => {
                let at = v3(n(0, "x", None)?, n(1, "y", None)?, n(2, "z", None)?);
                let r = n(3, "radius", Some(0.4))?;
                let h = n(4, "height", Some(1.8))?;
                let half = ((h / 2.0) - r).max(0.01);
                self.add(
                    "capsule",
                    ColliderBuilder::capsule_y(half as f32, r as f32),
                    [r, h / 2.0, r],
                    at,
                    pos,
                    named,
                )
            }
            // `world.character(x, y, z, radius = 0.4, height = 1.8, step = 0.3)`:
            // a player or enemy that walks with `move`.
            "character" => {
                let at = v3(n(0, "x", None)?, n(1, "y", None)?, n(2, "z", None)?);
                let r = n(3, "radius", Some(0.4))?;
                let h = n(4, "height", Some(1.8))?;
                let step = n(5, "step", Some(0.3))?;
                let half = ((h / 2.0) - r).max(0.01);
                let shape = ColliderBuilder::capsule_y(half as f32, r as f32);
                let body = RigidBodyBuilder::kinematic_position_based().translation(at);
                let (handle, collider) = self
                    .inner
                    .borrow_mut()
                    .world
                    .insert(body, shape.active_events(ActiveEvents::COLLISION_EVENTS));
                let controller = KinematicCharacterController {
                    autostep: Some(CharacterAutostep {
                        max_height: CharacterLength::Absolute(step as f32),
                        min_width: CharacterLength::Absolute(0.2),
                        include_dynamic_bodies: true,
                    }),
                    snap_to_ground: Some(CharacterLength::Absolute(0.2)),
                    max_slope_climb_angle: 50f32.to_radians(),
                    ..Default::default()
                };
                let name = match arg(pos, named, usize::MAX, "name") {
                    Some(Value::Str(s)) => s.to_string(),
                    _ => String::new(),
                };
                let value: ForeignRc = Rc::new(Box::new(Body {
                    world: Rc::downgrade(&self.inner),
                    handle,
                    collider,
                    kind: "character",
                    name,
                    size: [r, h / 2.0, r],
                    controller: Some(controller),
                    grounded: std::cell::Cell::new(false),
                }));
                let mut inner = self.inner.borrow_mut();
                inner.bodies.insert(handle, value.clone());
                inner.touch(handle);
                Ok(Value::Foreign(value))
            }
            // `world.step(dt)`: moves time on by `dt` seconds (in small
            // steps, so fast things don't pass through walls).
            "step" => {
                let dt = n(0, "dt", Some(1.0 / 60.0))?.clamp(0.0, 0.25);
                let mut inner = self.inner.borrow_mut();
                inner.started.clear();
                let steps = (dt / (1.0 / 60.0)).ceil().max(1.0) as usize;
                let each = (dt / steps as f64) as f32;
                inner.world.integration_parameters.dt = each;
                inner.dirty = false;
                inner.touched.clear();
                for _ in 0..steps {
                    let Inner {
                        world, collector, ..
                    } = &mut *inner;
                    world.step_with_events(&(), &*collector);
                }
                while let Ok(event) = inner.events.try_recv() {
                    if let CollisionEvent::Started(a, b, _) = event {
                        let parent = |c: ColliderHandle| {
                            inner.world.colliders.get(c).and_then(|c| c.parent())
                        };
                        if let (Some(a), Some(b)) = (parent(a), parent(b)) {
                            inner.started.push((a, b));
                        }
                    }
                }
                Ok(Value::None)
            }
            // `world.collisions()`: [a, b] pairs that started touching in
            // the last step.
            "collisions" => {
                let inner = self.inner.borrow();
                Ok(Value::list(
                    inner
                        .started
                        .iter()
                        .map(|(a, b)| {
                            Value::list(vec![
                                World::body_value(&inner, *a),
                                World::body_value(&inner, *b),
                            ])
                        })
                        .collect(),
                ))
            }
            // `world.raycast([x, y, z], [dx, dy, dz], max = 100, ignore = body)`:
            // the first body a line hits, as a map with `body`, `point`,
            // `normal` and `distance`, or none.
            "raycast" => {
                let origin = point(
                    arg(pos, named, 0, "origin").unwrap_or(&Value::None),
                    "the origin",
                )?;
                let dir = point(
                    arg(pos, named, 1, "direction").unwrap_or(&Value::None),
                    "the direction",
                )?;
                let max = n(2, "max", Some(100.0))? as f32;
                let len = dir.length();
                if len == 0.0 {
                    return Err(err("ValueError", "the direction must not be [0, 0, 0]"));
                }
                let dir = dir / len;
                let ignore = match arg(pos, named, 3, "ignore") {
                    Some(v @ Value::Foreign(_)) => Some(body_of(Some(v), "`ignore`")?),
                    _ => None,
                };
                self.inner.borrow_mut().refresh();
                let inner = self.inner.borrow();
                let mut filter = QueryFilter::default();
                if let Some(h) = ignore {
                    filter = filter.exclude_rigid_body(h);
                }
                let ray = Ray::new(origin, dir);
                let Some((collider, hit)) =
                    inner.world.cast_ray_and_get_normal(&ray, max, true, filter)
                else {
                    return Ok(Value::None);
                };
                let body = inner
                    .world
                    .colliders
                    .get(collider)
                    .and_then(|c| c.parent())
                    .map_or(Value::None, |h| World::body_value(&inner, h));
                let at = origin + dir * hit.time_of_impact;
                Ok(super::stdlib::map_value(vec![
                    ("body", body),
                    ("point", list3(at)),
                    ("normal", list3(hit.normal)),
                    ("distance", Value::Float(round(hit.time_of_impact))),
                ]))
            }
            // `world.remove(body)`
            "remove" => {
                let handle = body_of(arg(pos, named, 0, "body"), "`world.remove`")?;
                let mut inner = self.inner.borrow_mut();
                inner.world.remove_body_with_colliders(handle, true);
                inner.bodies.remove(&handle);
                inner.dirty = true;
                Ok(Value::None)
            }
            _ => Err(err(
                "FieldError",
                format!("a physics world has no method `{name}`"),
            )),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Body {
    fn world(&self) -> VmResult<Rc<RefCell<Inner>>> {
        self.world
            .upgrade()
            .ok_or_else(|| err("PhysicsError", "this body's world is gone"))
    }

    fn with<T>(&self, f: impl FnOnce(&RigidBody) -> T) -> VmResult<T> {
        let world = self.world()?;
        let inner = world.borrow();
        inner
            .world
            .bodies
            .get(self.handle)
            .map(f)
            .ok_or_else(|| err("PhysicsError", "this body was removed from its world"))
    }

    fn with_mut<T>(&self, f: impl FnOnce(&mut RigidBody) -> T) -> VmResult<T> {
        let world = self.world()?;
        let mut inner = world.borrow_mut();
        inner
            .world
            .bodies
            .get_mut(self.handle)
            .map(f)
            .ok_or_else(|| err("PhysicsError", "this body was removed from its world"))
    }

    /// Its turn as [x, y, z] degrees, in the order `vyrn.draw` turns
    /// shapes (about x, then y, then z).
    fn rotation_degrees(&self) -> VmResult<Value> {
        self.with(|b| {
            let (z, y, x) = b.rotation().to_euler(glam_euler());
            Value::list(vec![
                Value::Float(round(x.to_degrees())),
                Value::Float(round(y.to_degrees())),
                Value::Float(round(z.to_degrees())),
            ])
        })
    }

    /// Walks a character by an offset: slides along walls, climbs steps,
    /// stays on the ground. The walk across and the move up or down are
    /// done one after the other, so falling while walking still climbs
    /// steps.
    fn walk(&self, by: Vector, dt: f32) -> VmResult<Value> {
        if self.controller.is_none() {
            return Err(type_error(
                "`move` is for characters, from `world.character(...)`",
            ));
        }
        let across = Vector::new(by.x, 0.0, by.z);
        let up = Vector::new(0.0, by.y, 0.0);
        let mut total = Vector::ZERO;
        let mut grounded = false;
        if across != Vector::ZERO {
            let (moved, on_ground) = self.sweep(across, dt)?;
            total += moved;
            grounded = on_ground;
        }
        // Walking up a step or a slope lifts the character; falling in the
        // same frame would undo that, so it waits for a frame on the flat.
        let climbed = total.y > 1e-4;
        if up != Vector::ZERO && !(climbed && up.y < 0.0) {
            let (moved, on_ground) = self.sweep(up, dt)?;
            total += moved;
            grounded = on_ground;
        }
        if by == Vector::ZERO {
            // Standing still: just check the ground.
            let (_, on_ground) = self.sweep(Vector::new(0.0, -0.01, 0.0), dt)?;
            grounded = on_ground;
        }
        self.grounded.set(grounded);
        Ok(list3(total))
    }

    /// One move of the character's shape; gives how far it went and
    /// whether it ended on the ground.
    fn sweep(&self, by: Vector, dt: f32) -> VmResult<(Vector, bool)> {
        let controller = self.controller.as_ref().expect("checked by walk");
        let world = self.world()?;
        let mut inner = world.borrow_mut();
        inner.refresh();
        let w = &inner.world;
        let Some(collider) = w.colliders.get(self.collider) else {
            return Err(err(
                "PhysicsError",
                "this character was removed from its world",
            ));
        };
        let shape = collider.shared_shape().clone();
        let pose = *collider.position();
        let filter = QueryFilter::default().exclude_rigid_body(self.handle);
        let queries = w.query_pipeline_with_filter(filter);
        let moved = controller.move_shape(dt, &queries, &*shape, &pose, by, |_| {});
        if let Some(body) = inner.world.bodies.get_mut(self.handle) {
            let to = body.translation() + moved.translation;
            body.set_next_kinematic_translation(to);
            // Also now, so the position reads back before the next step.
            body.set_translation(to, true);
        }
        if let Some(c) = inner.world.colliders.get_mut(self.collider) {
            let mut p = *c.position();
            p.translation += moved.translation;
            c.set_position(p);
        }
        Ok((moved.translation, moved.grounded))
    }
}

/// The Euler order that matches `vyrn.draw`'s rotation.
fn glam_euler() -> rapier3d::glamx::EulerRot {
    rapier3d::glamx::EulerRot::ZYX
}

impl Foreign for Body {
    fn type_name(&self) -> &'static str {
        "Body"
    }

    fn display(&self) -> String {
        let at = self.with(|b| b.translation()).unwrap_or(Vector::ZERO);
        let name = if self.name.is_empty() {
            String::new()
        } else {
            format!(" {:?}", self.name)
        };
        format!(
            "<{}{name} at ({}, {}, {})>",
            self.kind,
            round(at.x),
            round(at.y),
            round(at.z)
        )
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "x" => self.with(|b| Value::Float(round(b.translation().x))),
            "y" => self.with(|b| Value::Float(round(b.translation().y))),
            "z" => self.with(|b| Value::Float(round(b.translation().z))),
            "position" => self.with(|b| list3(b.translation())),
            "velocity" => self.with(|b| list3(b.linvel())),
            "spin" => self.with(|b| list3(b.angvel())),
            "rotation" => self.rotation_degrees(),
            "kind" => Ok(Value::str(self.kind)),
            "name" => Ok(Value::str(self.name.clone())),
            // [width, height, depth], to draw it the size it is.
            "size" => Ok(Value::list(
                self.size.iter().map(|s| Value::Float(s * 2.0)).collect(),
            )),
            "mass" => self.with(|b| Value::Float(round(b.mass()))),
            "sleeping" => self.with(|b| Value::Bool(b.is_sleeping())),
            "grounded" => Ok(Value::Bool(self.grounded.get())),
            // [x, y, z, width, height, depth]: the box lists `vyrn.draw`
            // and `vyrn.game` use.
            "box" => self.with(|b| {
                let t = b.translation();
                Value::list(vec![
                    Value::Float(round(t.x)),
                    Value::Float(round(t.y)),
                    Value::Float(round(t.z)),
                    Value::Float(self.size[0] * 2.0),
                    Value::Float(self.size[1] * 2.0),
                    Value::Float(self.size[2] * 2.0),
                ])
            }),
            _ => Err(err("FieldError", format!("a body has no `{name}`"))),
        }
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        pos: &[Value],
        named: &[(String, Value)],
    ) -> VmResult<Value> {
        let vec_arg = || -> VmResult<Vector> {
            match (pos.first(), pos.len()) {
                (Some(v @ (Value::List(_) | Value::Tuple(_))), 1) => point(v, "it"),
                _ => Ok(v3(
                    num(pos, named, 0, "x", Some(0.0))?,
                    num(pos, named, 1, "y", Some(0.0))?,
                    num(pos, named, 2, "z", Some(0.0))?,
                )),
            }
        };
        match name {
            // `body.push(x, y, z)` or `body.push([x, y, z])`: a sudden shove
            // (an impulse, in kilogram metres per second).
            "push" => {
                let v = vec_arg()?;
                self.with_mut(|b| b.apply_impulse(v, true))?;
                Ok(Value::None)
            }
            // `body.spin_by(x, y, z)`: a sudden twist.
            "spin_by" => {
                let v = vec_arg()?;
                self.with_mut(|b| b.apply_torque_impulse(v, true))?;
                Ok(Value::None)
            }
            "set_velocity" => {
                let v = vec_arg()?;
                self.with_mut(|b| b.set_linvel(v, true))?;
                Ok(Value::None)
            }
            // `body.move_to(x, y, z)`: puts it somewhere at once.
            "move_to" => {
                let v = vec_arg()?;
                self.with_mut(|b| {
                    if b.is_kinematic() {
                        b.set_next_kinematic_translation(v);
                    }
                    b.set_translation(v, true);
                })?;
                // The shape goes too, so a character's next `move` starts here.
                let world = self.world()?;
                let mut inner = world.borrow_mut();
                if let Some(c) = inner.world.colliders.get_mut(self.collider) {
                    let mut p = *c.position();
                    p.translation = v;
                    c.set_position(p);
                }
                inner.touch(self.handle);
                Ok(Value::None)
            }
            // `body.turn_to(x, y, z)`: sets its rotation, in degrees.
            "turn_to" => {
                let v = vec_arg()?;
                let q = rapier3d::glamx::Quat::from_euler(
                    glam_euler(),
                    v.z.to_radians(),
                    v.y.to_radians(),
                    v.x.to_radians(),
                );
                self.with_mut(|b| b.set_rotation(q, true))?;
                self.world()?.borrow_mut().touch(self.handle);
                Ok(Value::None)
            }
            // `character.move(x, y, z, dt = 1/60)`: walk by that much.
            "move" => {
                let v = vec_arg()?;
                let dt = num(pos, named, usize::MAX, "dt", Some(1.0 / 60.0))? as f32;
                self.walk(v, dt)
            }
            // `body.touching()`: the bodies it touches now.
            "touching" => {
                let world = self.world()?;
                let inner = world.borrow();
                let mut out = Vec::new();
                for pair in inner.world.narrow_phase.contact_pairs_with(self.collider) {
                    if !pair.has_any_active_contact() {
                        continue;
                    }
                    let other = if pair.collider1 == self.collider {
                        pair.collider2
                    } else {
                        pair.collider1
                    };
                    if let Some(h) = inner.world.colliders.get(other).and_then(|c| c.parent()) {
                        out.push(World::body_value(&inner, h));
                    }
                }
                for (a, b, touching) in inner
                    .world
                    .narrow_phase
                    .intersection_pairs_with(self.collider)
                {
                    if !touching {
                        continue;
                    }
                    let other = if a == self.collider { b } else { a };
                    if let Some(h) = inner.world.colliders.get(other).and_then(|c| c.parent()) {
                        out.push(World::body_value(&inner, h));
                    }
                }
                Ok(Value::list(out))
            }
            "wake" => {
                self.with_mut(|b| b.wake_up(true))?;
                Ok(Value::None)
            }
            _ => Err(err("FieldError", format!("a body has no method `{name}`"))),
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}
