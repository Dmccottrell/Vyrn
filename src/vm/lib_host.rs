//! `use vyrn.host`: calling into the program Vyrn is embedded in.
//!
//! When Vyrn runs inside another program (a game engine through the
//! Unreal plugin, or any C or C++ program through `vyrn.h`), that program
//! registers functions, and scripts call them by name:
//!
//! ```vyrn
//! use vyrn.host
//!
//! create begin_play():
//!     host.call("log", "Hello from Vyrn")
//!     let pos = host.call("get_location")
//!     host.call("set_location", pos["x"], pos["y"], pos["z"] + 100)
//! end
//! ```
//!
//! Arguments and results travel as JSON, so they are plain data: numbers,
//! text, lists, maps (and structs, as maps).

use super::ops::{self, err, Ctx, VmResult};
use super::value::Value;

fn text<'a>(args: &'a [Value], what: &str) -> VmResult<&'a str> {
    match args.first() {
        Some(Value::Str(s)) => Ok(s.as_str()),
        _ => Err(err(
            "TypeError",
            format!("{what} is the host function's name, as text"),
        )),
    }
}

/// `host.call(name, args...)`: runs the host's function `name`.
pub fn call(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let name = text(args, "`host.call` takes a name first:")?.to_string();
    let Some(host) = ctx.host.clone() else {
        return Err(err(
            "HostError",
            format!("`host.call(\"{name}\")` works only when Vyrn runs inside another program, such as Unreal Engine"),
        ));
    };
    let json = ops::to_json(&ctx.program, &Value::list(args[1..].to_vec()));
    let answer = host(&name, &json).map_err(|e| err("HostError", format!("{name}: {e}")))?;
    if answer.trim().is_empty() {
        return Ok(Value::None);
    }
    let parsed = super::json::parse(&answer).map_err(|e| {
        err(
            "HostError",
            format!("{name} gave back something that is not JSON: {e}"),
        )
    })?;
    Ok(ops::from_json(&parsed))
}

/// `host.available()`: whether Vyrn is running inside a host program.
pub fn available(ctx: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(ctx.host.is_some()))
}
