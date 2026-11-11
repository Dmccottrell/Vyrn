//! `vyrn.draw` in builds without the `draw` feature (such as WebAssembly):
//! every function explains how to get it.

use super::ops::{err, Ctx, VmResult};
use super::value::Value;

fn unavailable() -> VmResult<Value> {
    Err(err(
        "Unavailable",
        "`vyrn.draw` needs a build with drawing (the `draw` feature, on by default)",
    ))
}

macro_rules! stubs {
    ($($name:ident),*) => {
        $(pub fn $name(_: &mut Ctx, _: &[Value]) -> VmResult<Value> { unavailable() })*
    };
}

stubs!(
    canvas,
    window,
    turtle,
    image,
    camera,
    mesh,
    overlaps,
    rgb,
    text_width,
    cube_mesh_fn,
    sphere_mesh_fn
);
