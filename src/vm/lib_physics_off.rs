//! `vyrn.physics` when Vyrn is built without the `physics` feature (such
//! as on WebAssembly).

use super::ops::{err, Ctx, VmResult};
use super::value::Value;

pub fn world(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Err(err(
        "Unavailable",
        "this Vyrn was built without `vyrn.physics` (the `physics` feature)",
    ))
}
