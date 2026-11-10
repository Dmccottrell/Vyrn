//! Values that live outside the VM: Python objects (`use python.numpy`) and
//! C functions and pointers (`extern "c"`).
//!
//! The VM knows them only through this trait, so each bridge is one file
//! behind its own cargo feature and nothing else in the interpreter changes
//! when a bridge is left out of the build.

use std::any::Any;
use std::fmt;

use super::ops::{err, Arith, Ctx, VmResult};
use super::value::Value;

// `len` mirrors Python's `len()`; emptiness is `truthy`.
#[allow(clippy::len_without_is_empty)]
pub trait Foreign: Any {
    /// What `type(x)` and error messages call it: `"python object"`.
    fn type_name(&self) -> &'static str;

    /// What `print` shows.
    fn display(&self) -> String;

    /// `x.name`
    fn get_attr(&self, name: &str) -> VmResult<Value> {
        Err(err(
            "FieldError",
            format!("a {} has no field `{name}`", self.type_name()),
        ))
    }

    /// `x(args, name = value)`
    fn call(&self, ctx: &mut Ctx, args: &[Value], kwargs: &[(String, Value)]) -> VmResult<Value> {
        let _ = (ctx, args, kwargs);
        Err(err(
            "TypeError",
            format!("a {} is not callable", self.type_name()),
        ))
    }

    /// `x.name(args)`: by default the attribute, called.
    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        kwargs: &[(String, Value)],
    ) -> VmResult<Value> {
        match self.get_attr(name)? {
            Value::Foreign(f) => f.call(ctx, args, kwargs),
            other => Err(err(
                "TypeError",
                format!("`.{name}` is {}, which is not callable", other.type_name()),
            )),
        }
    }

    /// `x[key]`
    fn index(&self, key: &Value) -> VmResult<Value> {
        let _ = key;
        Err(err(
            "TypeError",
            format!("a {} cannot be indexed", self.type_name()),
        ))
    }

    /// `len(x)`
    fn len(&self) -> VmResult<usize> {
        Err(err(
            "TypeError",
            format!("a {} has no length", self.type_name()),
        ))
    }

    /// The items of `for item in x`.
    fn items(&self) -> VmResult<Vec<Value>> {
        Err(err(
            "TypeError",
            format!("cannot loop over a {}", self.type_name()),
        ))
    }

    /// `x op other`, or `other op x` when `flipped`. `None` when the
    /// operation is not defined, so the usual error is reported.
    fn binary(&self, op: Arith, other: &Value, flipped: bool) -> Option<VmResult<Value>> {
        let _ = (op, other, flipped);
        None
    }

    fn equals(&self, other: &Value) -> bool {
        let _ = other;
        false
    }

    fn truthy(&self) -> bool {
        true
    }

    fn as_any(&self) -> &dyn Any;
}

impl fmt::Debug for dyn Foreign {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<{}>", self.type_name())
    }
}
