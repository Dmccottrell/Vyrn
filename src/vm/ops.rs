//! Runtime semantics shared by the VM loop and the builtins: arithmetic,
//! comparison, indexing, conversion to text and JSON, and device placement.
//!
//! The VM's dispatch loop handles the common cases (two integers, two floats,
//! a list and an integer index) inline and calls these functions for
//! everything else.

use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::Write;
use std::rc::Rc;

use super::dtype::{DType, Storage};
use super::program::{Program, TypeKind};
use super::tensor::{
    normalize_index, reserve, slice_indices, widen, Device, ElemOp, MapOp, Sub, Tensor,
    TensorBackend, TensorError,
};
use super::value::{ErrorValue, Instance, Iter, Key, Map, RangeValue, SliceValue, Value};

/// A failure in flight: whatever value was passed to `fail`, or a runtime
/// error raised by the VM.
pub type VmResult<T> = Result<T, Value>;

/// Builds a runtime error value.
pub fn err(kind: &'static str, message: impl Into<String>) -> Value {
    Value::Error(Rc::new(ErrorValue {
        kind,
        message: message.into(),
    }))
}

pub fn type_error(message: impl Into<String>) -> Value {
    err("TypeError", message)
}

/// State the builtins and tensor operations need.
/// See [`Ctx::host`].
pub type HostFn = Rc<dyn Fn(&str, &str) -> Result<String, String>>;

pub struct Ctx {
    pub program: Rc<Program>,
    pub out: Box<dyn Write>,
    /// The `on device:` blocks being executed, innermost last.
    pub devices: Vec<Device>,
    pub cpu: Box<dyn TensorBackend>,
    pub gpu: Box<dyn TensorBackend>,
    pub rng: u64,
    /// Lines for `input()` to give instead of reading the keyboard (for
    /// tests and course checks).
    pub scripted_input: Option<std::collections::VecDeque<String>>,
    /// Builds another copy of the program, for `task.parallel`'s worker
    /// threads.
    pub rebuild: Option<super::parallel::Rebuild>,
    /// The program embedding Vyrn (such as a game engine), for
    /// `host.call(name, args...)`: takes a function name and its arguments
    /// as JSON, gives back the result as JSON or an error message.
    pub host: Option<HostFn>,
    /// Timers started by `sleep`, each finishing its task when it expires.
    pub timers: Vec<(std::time::Instant, Rc<super::task::TaskHandle>)>,
    /// Tasks made ready by builtins, such as a channel receiving a value.
    pub wakeups: Vec<super::task::TaskId>,
    /// The next task id to hand out.
    pub next_task: super::task::TaskId,
    /// Servers started by `web.serve`.
    pub servers: Vec<super::web::Server>,
    /// Values route handlers receive by parameter name (`web.provide`).
    pub providers: std::collections::HashMap<String, Value>,
    /// Set by a builtin that blocks, such as `web.serve`: the calling task
    /// waits for this task before continuing.
    pub block_on: Option<Rc<super::task::TaskHandle>>,
    /// The host delivers HTTP requests itself (an edge platform), so
    /// `web.serve` returns at once instead of listening.
    pub hosted: bool,
    /// Tasks a builtin asked to stop (`task.cancel`, a group's `cancel`).
    pub cancels: Vec<Rc<super::task::TaskHandle>>,
}

impl Ctx {
    pub fn current_device(&self) -> Device {
        self.devices.last().copied().unwrap_or(Device::Cpu)
    }

    pub fn backend(&self, device: Device) -> &dyn TensorBackend {
        match device {
            Device::Cpu => self.cpu.as_ref(),
            Device::Gpu(_) => self.gpu.as_ref(),
        }
    }

    pub fn new_task_id(&mut self) -> super::task::TaskId {
        self.next_task += 1;
        self.next_task
    }

    /// A pseudo-random number in [0, 1), from a fixed seed so runs are
    /// reproducible.
    pub fn random(&mut self) -> f32 {
        // xorshift64*
        let mut x = self.rng;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.rng = x;
        ((x.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 40) as f32) / (1u64 << 24) as f32
    }
}

pub fn tensor_error(error: TensorError) -> Value {
    match error {
        TensorError::Shape(message) => err("ShapeError", message),
        TensorError::Device(message) => err("DeviceError", message),
    }
}

// ---- arithmetic ------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arith {
    Add,
    Sub,
    Mul,
    Div,
    FloorDiv,
    Rem,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

impl Arith {
    fn symbol(self) -> &'static str {
        match self {
            Arith::Add => "+",
            Arith::Sub => "-",
            Arith::Mul => "*",
            Arith::Div => "/",
            Arith::FloorDiv => "//",
            Arith::Rem => "%",
            Arith::Pow => "**",
            Arith::BitAnd => "&",
            Arith::BitOr => "|",
            Arith::BitXor => "^",
            Arith::Shl => "<<",
            Arith::Shr => ">>",
        }
    }
}

fn overflow(op: Arith) -> Value {
    err("Overflow", format!("integer overflow in `{}`", op.symbol()))
}

fn as_float(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        Value::Bool(b) => Some(f64::from(u8::from(*b))),
        _ => None,
    }
}

fn int_arith(op: Arith, x: i64, y: i64) -> VmResult<Value> {
    let checked = |r: Option<i64>| r.map(Value::Int).ok_or_else(|| overflow(op));
    match op {
        Arith::Add => checked(x.checked_add(y)),
        Arith::Sub => checked(x.checked_sub(y)),
        Arith::Mul => checked(x.checked_mul(y)),
        Arith::Div => {
            if y == 0 {
                Err(err("DivisionByZero", "division by zero"))
            } else {
                Ok(Value::Float(x as f64 / y as f64))
            }
        }
        Arith::FloorDiv | Arith::Rem if y == 0 => Err(err("DivisionByZero", "division by zero")),
        Arith::FloorDiv => {
            let q = x.checked_div(y).ok_or_else(|| overflow(op))?;
            Ok(Value::Int(if (x % y != 0) && ((x < 0) != (y < 0)) {
                q - 1
            } else {
                q
            }))
        }
        Arith::Rem => {
            let r = x.checked_rem(y).ok_or_else(|| overflow(op))?;
            Ok(Value::Int(if r != 0 && ((r < 0) != (y < 0)) {
                r + y
            } else {
                r
            }))
        }
        Arith::Pow => {
            if y < 0 {
                Ok(Value::Float((x as f64).powf(y as f64)))
            } else {
                let exponent = u32::try_from(y).map_err(|_| overflow(op))?;
                checked(x.checked_pow(exponent))
            }
        }
        Arith::BitAnd => Ok(Value::Int(x & y)),
        Arith::BitOr => Ok(Value::Int(x | y)),
        Arith::BitXor => Ok(Value::Int(x ^ y)),
        Arith::Shl | Arith::Shr => {
            let shift = u32::try_from(y)
                .ok()
                .filter(|s| *s < 64)
                .ok_or_else(|| err("ValueError", format!("cannot shift by {y}")))?;
            Ok(Value::Int(if op == Arith::Shl {
                x.checked_shl(shift).ok_or_else(|| overflow(op))?
            } else {
                x >> shift
            }))
        }
    }
}

fn float_arith(op: Arith, x: f64, y: f64) -> VmResult<Value> {
    Ok(Value::Float(match op {
        Arith::Add => x + y,
        Arith::Sub => x - y,
        Arith::Mul => x * y,
        Arith::Div | Arith::FloorDiv | Arith::Rem if y == 0.0 => {
            return Err(err("DivisionByZero", "division by zero"))
        }
        Arith::Div => x / y,
        Arith::FloorDiv => (x / y).floor(),
        Arith::Rem => x - y * (x / y).floor(),
        Arith::Pow => x.powf(y),
        _ => {
            return Err(type_error(format!(
                "`{}` needs integers, not floats",
                op.symbol()
            )))
        }
    }))
}

pub fn arith(ctx: &mut Ctx, op: Arith, a: &Value, b: &Value) -> VmResult<Value> {
    if let Value::Foreign(f) = a {
        if let Some(result) = f.binary(op, b, false) {
            return result;
        }
    }
    if let Value::Foreign(f) = b {
        if let Some(result) = f.binary(op, a, true) {
            return result;
        }
    }
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => int_arith(op, *x, *y),
        (Value::Tensor(x), Value::Tensor(y)) => tensor_arith(ctx, op, x, y),
        (Value::Tensor(x), other) => match as_float(other) {
            Some(s) => tensor_arith(ctx, op, x, &Tensor::scalar(s as f32, x.device)),
            None => Err(operand_error(op, a, b)),
        },
        (other, Value::Tensor(y)) => match as_float(other) {
            Some(s) => tensor_arith(ctx, op, &Tensor::scalar(s as f32, y.device), y),
            None => Err(operand_error(op, a, b)),
        },
        (Value::Str(x), Value::Str(y)) if op == Arith::Add => Ok(Value::str(format!("{x}{y}"))),
        (Value::Str(s), Value::Int(n)) | (Value::Int(n), Value::Str(s)) if op == Arith::Mul => {
            Ok(Value::str(s.repeat((*n).max(0) as usize)))
        }
        // `[0] * n`, as in Python: a negative count gives an empty list.
        (Value::List(x), Value::Int(n)) | (Value::Int(n), Value::List(x)) if op == Arith::Mul => {
            let x = x.borrow();
            let times = (*n).max(0) as usize;
            if (x.len() as u64).saturating_mul(times as u64) > (1u64 << 32) {
                return Err(err("MemoryError", "the list would be too large"));
            }
            let mut items = Vec::with_capacity(x.len() * times);
            for _ in 0..times {
                items.extend(x.iter().cloned());
            }
            Ok(Value::list(items))
        }
        (Value::List(x), Value::List(y)) if op == Arith::Add => {
            let mut items = x.borrow().clone();
            items.extend(y.borrow().iter().cloned());
            Ok(Value::list(items))
        }
        (Value::Tuple(x), Value::Tuple(y)) if op == Arith::Add => {
            let mut items = (**x).clone();
            items.extend(y.iter().cloned());
            Ok(Value::tuple(items))
        }
        _ => match (as_float(a), as_float(b)) {
            (Some(x), Some(y)) if !matches!(a, Value::Bool(_)) || !matches!(b, Value::Bool(_)) => {
                float_arith(op, x, y)
            }
            _ => Err(operand_error(op, a, b)),
        },
    }
}

fn operand_error(op: Arith, a: &Value, b: &Value) -> Value {
    type_error(format!(
        "cannot apply `{}` to {} and {}",
        op.symbol(),
        a.type_name(),
        b.type_name()
    ))
}

fn same_device(a: &Tensor, b: &Tensor) -> VmResult<Device> {
    if a.device == b.device {
        Ok(a.device)
    } else {
        Err(err(
            "DeviceError",
            format!(
                "cannot combine a tensor on {} with one on {}; move one first, for example `x on {}`",
                a.device, b.device, a.device
            ),
        ))
    }
}

fn tensor_arith(ctx: &mut Ctx, op: Arith, a: &Tensor, b: &Tensor) -> VmResult<Value> {
    let device = same_device(a, b)?;
    let elem = match op {
        Arith::Add => ElemOp::Add,
        Arith::Sub => ElemOp::Sub,
        Arith::Mul => ElemOp::Mul,
        Arith::Div => ElemOp::Div,
        Arith::Pow => ElemOp::Pow,
        _ => {
            return Err(type_error(format!(
                "`{}` is not defined for tensors",
                op.symbol()
            )))
        }
    };
    ctx.backend(device)
        .elementwise(elem, a, b)
        .map_err(tensor_error)
        .and_then(alloc)
}

pub fn matmul(ctx: &mut Ctx, a: &Value, b: &Value) -> VmResult<Value> {
    match (a, b) {
        (Value::Tensor(x), Value::Tensor(y)) => {
            let device = same_device(x, y)?;
            ctx.backend(device)
                .matmul(x, y)
                .map_err(tensor_error)
                .and_then(alloc)
        }
        _ => Err(type_error(format!(
            "`@` multiplies tensors, not {} and {}; create one with `tensor(...)`",
            a.type_name(),
            b.type_name()
        ))),
    }
}

pub fn negate(ctx: &mut Ctx, value: &Value) -> VmResult<Value> {
    match value {
        Value::Int(i) => i
            .checked_neg()
            .map(Value::Int)
            .ok_or_else(|| err("Overflow", "integer overflow in negation")),
        Value::Float(f) => Ok(Value::Float(-f)),
        Value::Tensor(t) => alloc(ctx.backend(t.device).map(MapOp::Neg, t)),
        _ => Err(type_error(format!("cannot negate {}", value.type_name()))),
    }
}

pub fn map_tensor(ctx: &mut Ctx, op: MapOp, t: &Tensor) -> VmResult<Value> {
    alloc(ctx.backend(t.device).map(op, t))
}

/// Wraps a new tensor as a value, failing with `OutOfMemory` if its device
/// has a memory limit it would exceed.
pub fn alloc(tensor: Tensor) -> VmResult<Value> {
    reserve(tensor.device, tensor.nbytes()).map_err(|m| {
        err(
            "OutOfMemory",
            format!(
                "{m}; store weights at lower precision (`.to(q4)`) or split the model with `fit`"
            ),
        )
    })?;
    Ok(Value::tensor(tensor))
}

// ---- comparison ------------------------------------------------------------

/// How an operator is written, as in `create operator +(self, other)`.
pub fn arith_symbol(kind: Arith) -> &'static str {
    match kind {
        Arith::Add => "+",
        Arith::Sub => "-",
        Arith::Mul => "*",
        Arith::Div => "/",
        Arith::FloorDiv => "//",
        Arith::Rem => "%",
        Arith::Pow => "**",
        Arith::BitAnd => "&",
        Arith::BitOr => "|",
        Arith::BitXor => "^",
        Arith::Shl => "<<",
        Arith::Shr => ">>",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cmp {
    Lt,
    LtEq,
    Gt,
    GtEq,
}

pub fn equals(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Foreign(f), other) | (other, Value::Foreign(f)) => f.equals(other),
        (Value::None, Value::None) => true,
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Int(x), Value::Int(y)) => x == y,
        (Value::Str(x), Value::Str(y)) => x == y,
        (Value::List(x), Value::List(y)) => {
            Rc::ptr_eq(x, y) || seq_equals(&x.borrow(), &y.borrow())
        }
        (Value::Tuple(x), Value::Tuple(y)) => seq_equals(x, y),
        (Value::Map(x), Value::Map(y)) => {
            let (x, y) = (x.borrow(), y.borrow());
            x.len() == y.len()
                && x.entries().iter().all(|(k, v)| {
                    Key::from_value(k)
                        .and_then(|k| y.get(&k))
                        .is_some_and(|w| equals(v, w))
                })
        }
        (Value::Struct(x), Value::Struct(y)) => {
            Rc::ptr_eq(x, y) || (x.ty == y.ty && seq_equals(&x.fields, &y.fields))
        }
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        (Value::Range(x), Value::Range(y)) => x == y,
        (Value::Tensor(x), Value::Tensor(y)) => x.shape == y.shape && x.values() == y.values(),
        (Value::Device(x), Value::Device(y)) => x == y,
        (Value::Type(x), Value::Type(y)) => x == y,
        (Value::Function(x), Value::Function(y)) => Rc::ptr_eq(x, y),
        (Value::Native(x), Value::Native(y)) => x == y,
        _ => match (as_number(a), as_number(b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        },
    }
}

fn as_number(value: &Value) -> Option<f64> {
    match value {
        Value::Int(i) => Some(*i as f64),
        Value::Float(f) => Some(*f),
        _ => None,
    }
}

fn seq_equals(a: &[Value], b: &[Value]) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(x, y)| equals(x, y))
}

/// Orders two values, for `<` and sorting.
pub fn order(a: &Value, b: &Value) -> VmResult<std::cmp::Ordering> {
    use std::cmp::Ordering;
    match (a, b) {
        (Value::Int(x), Value::Int(y)) => Ok(x.cmp(y)),
        (Value::Str(x), Value::Str(y)) => Ok(x.cmp(y)),
        (Value::Bool(x), Value::Bool(y)) => Ok(x.cmp(y)),
        (Value::Tuple(x), Value::Tuple(y)) => seq_order(x, y),
        (Value::List(x), Value::List(y)) => seq_order(&x.borrow(), &y.borrow()),
        _ => match (as_number(a), as_number(b)) {
            (Some(x), Some(y)) => Ok(x.partial_cmp(&y).unwrap_or(Ordering::Equal)),
            _ => Err(type_error(format!(
                "cannot compare {} with {}",
                a.type_name(),
                b.type_name()
            ))),
        },
    }
}

fn seq_order(a: &[Value], b: &[Value]) -> VmResult<std::cmp::Ordering> {
    for (x, y) in a.iter().zip(b) {
        let ordering = order(x, y)?;
        if ordering != std::cmp::Ordering::Equal {
            return Ok(ordering);
        }
    }
    Ok(a.len().cmp(&b.len()))
}

pub fn compare(ctx: &mut Ctx, op: Cmp, a: &Value, b: &Value) -> VmResult<Value> {
    let elem = match op {
        Cmp::Lt => ElemOp::Lt,
        Cmp::LtEq => ElemOp::LtEq,
        Cmp::Gt => ElemOp::Gt,
        Cmp::GtEq => ElemOp::GtEq,
    };
    let tensors = match (a, b) {
        (Value::Tensor(x), Value::Tensor(y)) => Some(((***x).clone(), (***y).clone())),
        (Value::Tensor(x), other) => {
            as_float(other).map(|s| ((***x).clone(), Tensor::scalar(s as f32, x.device)))
        }
        (other, Value::Tensor(y)) => {
            as_float(other).map(|s| (Tensor::scalar(s as f32, y.device), (***y).clone()))
        }
        _ => None,
    };
    if let Some((x, y)) = tensors {
        let device = same_device(&x, &y)?;
        return ctx
            .backend(device)
            .elementwise(elem, &x, &y)
            .map_err(tensor_error)
            .and_then(alloc);
    }
    if let (Some(x), Some(y)) = (as_number(a), as_number(b)) {
        if x.is_nan() || y.is_nan() {
            return Ok(Value::Bool(false));
        }
    }
    let ordering = order(a, b)?;
    Ok(Value::Bool(match op {
        Cmp::Lt => ordering.is_lt(),
        Cmp::LtEq => ordering.is_le(),
        Cmp::Gt => ordering.is_gt(),
        Cmp::GtEq => ordering.is_ge(),
    }))
}

pub fn contains(item: &Value, container: &Value) -> VmResult<bool> {
    match container {
        Value::List(items) => Ok(items.borrow().iter().any(|v| equals(v, item))),
        Value::Tuple(items) => Ok(items.iter().any(|v| equals(v, item))),
        Value::Str(text) => match item {
            Value::Str(part) => Ok(text.contains(part.as_str())),
            _ => Err(type_error("only a string can be found in a string")),
        },
        Value::Map(map) => Ok(Key::from_value(item).is_some_and(|k| map.borrow().contains(&k))),
        Value::Foreign(f) => f.contains(item),
        Value::Range(r) => Ok(match item {
            Value::Int(i) => {
                let inside = if r.step > 0 {
                    *i >= r.start && *i < r.stop
                } else {
                    *i <= r.start && *i > r.stop
                };
                inside && (i - r.start) % r.step == 0
            }
            Value::Float(f) => f.fract() == 0.0 && contains(&Value::Int(*f as i64), container)?,
            _ => false,
        }),
        _ => Err(type_error(format!(
            "cannot search inside {}",
            container.type_name()
        ))),
    }
}

// ---- collections -----------------------------------------------------------

pub fn len(value: &Value) -> VmResult<i64> {
    Ok(match value {
        Value::Str(s) => s.chars().count() as i64,
        Value::List(items) => items.borrow().len() as i64,
        Value::Tuple(items) => items.len() as i64,
        Value::Map(map) => map.borrow().len() as i64,
        Value::Range(r) => range_len(r),
        Value::Tensor(t) => t.shape.first().copied().unwrap_or(1) as i64,
        Value::Foreign(f) => f.len()? as i64,
        _ => return Err(type_error(format!("{} has no length", value.type_name()))),
    })
}

fn range_len(r: &RangeValue) -> i64 {
    let span = if r.step > 0 {
        r.stop - r.start
    } else {
        r.start - r.stop
    };
    if span <= 0 {
        0
    } else {
        (span + r.step.abs() - 1) / r.step.abs()
    }
}

pub fn make_range(start: &Value, end: &Value, step: &Value, inclusive: bool) -> VmResult<Value> {
    let int = |v: &Value| match v {
        Value::Int(i) => Ok(*i),
        Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
        _ => Err(type_error(format!(
            "range bounds must be integers, not {}",
            v.type_name()
        ))),
    };
    let (start, end, step) = (int(start)?, int(end)?, int(step)?);
    if step == 0 {
        return Err(err("ValueError", "a range step cannot be zero"));
    }
    let stop = if inclusive {
        end.saturating_add(step.signum())
    } else {
        end
    };
    Ok(Value::Range(Rc::new(RangeValue { start, stop, step })))
}

fn int_index(value: &Value) -> Option<i64> {
    match value {
        Value::Int(i) => Some(*i),
        Value::Bool(b) => Some(i64::from(*b)),
        _ => None,
    }
}

fn index_error(index: i64, len: usize, what: &str) -> Value {
    err(
        "IndexError",
        format!("index {index} is out of range for a {what} of length {len}"),
    )
}

/// The positions a slice or range selects from a sequence of length `len`.
fn selection(index: &Value, len: usize) -> Option<VmResult<Vec<usize>>> {
    match index {
        Value::Slice(s) => {
            Some(slice_indices(s.start, s.stop, s.step, len).map_err(|m| err("ValueError", m)))
        }
        Value::Range(r) => Some(Ok((0..range_len(r))
            .map(|i| r.start + i * r.step)
            .filter_map(|i| normalize_index(i, len))
            .collect())),
        _ => None,
    }
}

fn tensor_subs(index: &Value) -> VmResult<Vec<Sub>> {
    let one = |v: &Value| -> VmResult<Sub> {
        Ok(match v {
            Value::Int(i) => Sub::Index(*i),
            Value::Slice(s) => Sub::Slice {
                start: s.start,
                stop: s.stop,
                step: s.step,
            },
            Value::Range(r) => Sub::Slice {
                start: Some(r.start),
                stop: Some(r.stop),
                step: Some(r.step),
            },
            Value::Ellipsis => Sub::Ellipsis,
            Value::None => Sub::NewAxis,
            other => {
                return Err(type_error(format!(
                    "a tensor cannot be indexed by {}",
                    other.type_name()
                )))
            }
        })
    };
    match index {
        Value::Tuple(items) => items.iter().map(one).collect(),
        other => Ok(vec![one(other)?]),
    }
}

pub fn get_index(ctx: &mut Ctx, obj: &Value, index: &Value) -> VmResult<Value> {
    match obj {
        Value::Foreign(f) => f.index(index),
        Value::List(items) => {
            let items = items.borrow();
            if let Some(i) = int_index(index) {
                let at = normalize_index(i, items.len())
                    .ok_or_else(|| index_error(i, items.len(), "list"))?;
                return Ok(items[at].clone());
            }
            match selection(index, items.len()) {
                Some(picks) => Ok(Value::list(
                    picks?.into_iter().map(|i| items[i].clone()).collect(),
                )),
                None => Err(type_error(format!(
                    "a list index must be an integer, not {}",
                    index.type_name()
                ))),
            }
        }
        Value::Tuple(items) => {
            if let Some(i) = int_index(index) {
                let at = normalize_index(i, items.len())
                    .ok_or_else(|| index_error(i, items.len(), "tuple"))?;
                return Ok(items[at].clone());
            }
            match selection(index, items.len()) {
                Some(picks) => Ok(Value::tuple(
                    picks?.into_iter().map(|i| items[i].clone()).collect(),
                )),
                None => Err(type_error(format!(
                    "a tuple index must be an integer, not {}",
                    index.type_name()
                ))),
            }
        }
        Value::Str(text) => {
            let chars: Vec<char> = text.chars().collect();
            if let Some(i) = int_index(index) {
                let at = normalize_index(i, chars.len())
                    .ok_or_else(|| index_error(i, chars.len(), "string"))?;
                return Ok(Value::str(chars[at].to_string()));
            }
            match selection(index, chars.len()) {
                Some(picks) => Ok(Value::str(
                    picks?.into_iter().map(|i| chars[i]).collect::<String>(),
                )),
                None => Err(type_error(format!(
                    "a string index must be an integer, not {}",
                    index.type_name()
                ))),
            }
        }
        Value::Map(map) => {
            let key = Key::from_value(index)
                .ok_or_else(|| type_error(format!("{} cannot be a map key", index.type_name())))?;
            map.borrow().get(&key).cloned().ok_or_else(|| {
                err(
                    "KeyError",
                    format!("key {} is not in the map", repr(&ctx_program(ctx), index)),
                )
            })
        }
        Value::Range(r) => {
            let i =
                int_index(index).ok_or_else(|| type_error("a range index must be an integer"))?;
            let n = range_len(r);
            let at = normalize_index(i, n as usize)
                .ok_or_else(|| index_error(i, n as usize, "range"))?;
            Ok(Value::Int(r.start + at as i64 * r.step))
        }
        Value::Tensor(t) => {
            let selected = t.select(&tensor_subs(index)?).map_err(tensor_error)?;
            Ok(if selected.shape.is_empty() {
                Value::Float(widen(selected.value(0)))
            } else {
                alloc(selected)?
            })
        }
        Value::Gpus => match index {
            Value::Int(i) => u16::try_from(*i)
                .map(|i| Value::Device(Device::Gpu(i)))
                .map_err(|_| err("ValueError", format!("gpu[{i}] is not a valid device"))),
            Value::Range(r) => {
                let devices: Vec<Device> = (0..range_len(r))
                    .map(|i| r.start + i * r.step)
                    .filter_map(|i| u16::try_from(i).ok().map(Device::Gpu))
                    .collect();
                if devices.is_empty() {
                    return Err(err("ValueError", "an empty range selects no devices"));
                }
                Ok(Value::DeviceGroup(Rc::new(devices)))
            }
            _ => Err(type_error("pick devices with `gpu[0]` or `gpu[0 to 3]`")),
        },
        _ => Err(type_error(format!("{} cannot be indexed", obj.type_name()))),
    }
}

fn ctx_program(ctx: &Ctx) -> Rc<Program> {
    ctx.program.clone()
}

pub fn set_index(obj: &mut Value, index: &Value, value: Value) -> VmResult<()> {
    match obj {
        Value::List(items) => {
            let mut items = items.borrow_mut();
            let i = int_index(index).ok_or_else(|| {
                type_error(format!(
                    "a list index must be an integer, not {}",
                    index.type_name()
                ))
            })?;
            let len = items.len();
            let at = normalize_index(i, len).ok_or_else(|| index_error(i, len, "list"))?;
            items[at] = value;
            Ok(())
        }
        Value::Map(map) => {
            let key = Key::from_value(index)
                .ok_or_else(|| type_error(format!("{} cannot be a map key", index.type_name())))?;
            map.borrow_mut().insert(key, index.clone(), value);
            Ok(())
        }
        Value::Tensor(t) => {
            let number = as_float(&value).ok_or_else(|| {
                type_error(format!(
                    "a tensor element must be a number, not {}",
                    value.type_name()
                ))
            })?;
            let indices: Vec<i64> = match index {
                Value::Tuple(items) => items
                    .iter()
                    .map(|v| {
                        int_index(v)
                            .ok_or_else(|| type_error("tensor assignment needs integer indices"))
                    })
                    .collect::<VmResult<_>>()?,
                other => vec![int_index(other)
                    .ok_or_else(|| type_error("tensor assignment needs integer indices"))?],
            };
            Rc::make_mut(t)
                .set(&indices, number as f32)
                .map_err(tensor_error)
        }
        Value::Tuple(_) => Err(type_error("tuples cannot be changed; build a new one")),
        Value::Str(_) => Err(type_error("strings cannot be changed; build a new one")),
        other => Err(type_error(format!(
            "{} does not support item assignment",
            other.type_name()
        ))),
    }
}

/// Fields of builtin values: `t.T`, `t.shape`, `v.x` and so on.
pub fn get_attr(value: &Value, name: &str) -> VmResult<Value> {
    if let Value::Foreign(f) = value {
        return f.get_attr(name);
    }
    if let Value::Clock(clock) = value {
        let c = clock.borrow();
        return match name {
            "running" => Ok(Value::Bool(c.running())),
            "frame" => Ok(Value::Int(c.frame as i64)),
            "hz" => Ok(Value::Float(c.hz)),
            "dt" => Ok(Value::Float(c.step())),
            "real_dt" => Ok(Value::Float(c.real_dt)),
            "elapsed" => Ok(Value::Float(c.elapsed())),
            "fps" => Ok(Value::Float(c.fps())),
            _ => Err(err("FieldError", format!("a Clock has no field `{name}`"))),
        };
    }
    if let Value::Tensor(t) = value {
        let component = |i: usize| -> VmResult<Value> {
            if t.ndim() == 1 && i < t.shape[0] && t.shape[0] <= 4 {
                Ok(Value::Float(widen(t.value(i))))
            } else {
                Err(err(
                    "FieldError",
                    format!("`.{name}` needs a vector of up to 4 elements"),
                ))
            }
        };
        return match name {
            "T" | "mT" => alloc(t.transpose()),
            "shape" => Ok(Value::tuple(
                t.shape.iter().map(|&d| Value::Int(d as i64)).collect(),
            )),
            "ndim" => Ok(Value::Int(t.ndim() as i64)),
            "device" => Ok(Value::Device(t.device)),
            "dtype" => Ok(Value::str(t.dtype().name())),
            "nbytes" => Ok(Value::Int(t.nbytes() as i64)),
            "x" => component(0),
            "y" => component(1),
            "z" => component(2),
            "w" => component(3),
            _ => Err(err("FieldError", format!("a tensor has no field `{name}`"))),
        };
    }
    if let Value::Range(r) = value {
        match name {
            "start" => return Ok(Value::Int(r.start)),
            "end" => return Ok(Value::Int(r.stop)),
            "step" => return Ok(Value::Int(r.step)),
            _ => {}
        }
    }
    if let Value::Tokenizer(t) = value {
        if name == "vocab_size" {
            return Ok(Value::Int(t.vocab_size() as i64));
        }
    }
    if let Value::Error(e) = value {
        match name {
            "message" => return Ok(Value::str(e.message.clone())),
            "kind" => return Ok(Value::str(e.kind)),
            _ => {}
        }
    }
    Err(err(
        "FieldError",
        format!("{} has no field `{name}`", value.type_name()),
    ))
}

pub fn set_attr(value: &mut Value, name: &str, new: Value) -> VmResult<()> {
    // A method call on `x.field` stores the field back afterwards; for a
    // foreign value that is the very same object, so there is nothing to do.
    if let (Value::Foreign(f), Value::Foreign(n)) = (&*value, &new) {
        if let Ok(Value::Foreign(current)) = f.get_attr(name) {
            if Rc::ptr_eq(&current, n) {
                return Ok(());
            }
        }
    }
    if let Value::Tensor(t) = value {
        let slot = match name {
            "x" => 0,
            "y" => 1,
            "z" => 2,
            "w" => 3,
            _ => {
                return Err(err(
                    "FieldError",
                    format!("a tensor has no field `{name}` to set"),
                ))
            }
        };
        let number = as_float(&new).ok_or_else(|| {
            type_error(format!(
                "`.{name}` must be a number, not {}",
                new.type_name()
            ))
        })?;
        if t.ndim() != 1 || slot >= t.shape[0] {
            return Err(err(
                "FieldError",
                format!("`.{name}` needs a vector of up to 4 elements"),
            ));
        }
        Rc::make_mut(t).data_mut().map_err(tensor_error)?[slot] = number as f32;
        return Ok(());
    }
    Err(err(
        "FieldError",
        format!("{} has no field `{name}` to set", value.type_name()),
    ))
}

pub fn make_iter(value: &Value) -> VmResult<Value> {
    let iter = match value {
        Value::Foreign(f) => Iter::Values {
            items: f.items()?,
            next: 0,
        },
        Value::List(items) => Iter::List {
            items: items.clone(),
            next: 0,
        },
        Value::Tuple(items) => Iter::Values {
            items: (**items).clone(),
            next: 0,
        },
        Value::Str(text) => Iter::Values {
            items: text.chars().map(|c| Value::str(c.to_string())).collect(),
            next: 0,
        },
        Value::Map(map) => Iter::Values {
            items: map
                .borrow()
                .entries()
                .iter()
                .map(|(k, _)| k.clone())
                .collect(),
            next: 0,
        },
        Value::Range(r) => Iter::Range {
            next: r.start,
            stop: r.stop,
            step: r.step,
        },
        Value::Tensor(t) => Iter::Values {
            items: tensor_rows(t),
            next: 0,
        },
        Value::Iter(_) => return Ok(value.clone()),
        other => {
            return Err(type_error(format!(
                "cannot loop over {}",
                other.type_name()
            )))
        }
    };
    Ok(Value::Iter(Rc::new(RefCell::new(iter))))
}

/// The elements along a tensor's first dimension.
pub fn tensor_rows(t: &Tensor) -> Vec<Value> {
    let values = t.values();
    if t.ndim() <= 1 {
        return values.iter().map(|&x| Value::Float(widen(x))).collect();
    }
    let row: usize = t.shape[1..].iter().product();
    values
        .chunks(row.max(1))
        .map(|chunk| {
            Value::tensor(Tensor {
                shape: t.shape[1..].to_vec(),
                storage: Storage::F32(chunk.to_vec()),
                device: t.device,
            })
        })
        .collect()
}

/// Bytes of tensor memory a value holds, counting every tensor inside
/// lists, tuples, maps and fields. Tensors shared between places are
/// counted each time they appear.
pub fn tensor_bytes(value: &Value) -> usize {
    match value {
        Value::Tensor(t) => t.nbytes(),
        Value::List(items) => items.borrow().iter().map(tensor_bytes).sum(),
        Value::Tuple(items) => items.iter().map(tensor_bytes).sum(),
        Value::Map(map) => map
            .borrow()
            .entries()
            .iter()
            .map(|(_, v)| tensor_bytes(v))
            .sum(),
        Value::Struct(inst) => inst.fields.iter().map(tensor_bytes).sum(),
        Value::Object(obj) => obj.borrow().fields.iter().map(tensor_bytes).sum(),
        _ => 0,
    }
}

/// Collects any iterable into a vector.
pub fn collect(value: &Value) -> VmResult<Vec<Value>> {
    let iter = make_iter(value)?;
    let Value::Iter(iter) = iter else {
        return Err(type_error("cannot iterate"));
    };
    let mut items = Vec::new();
    while let Some(v) = iter.borrow_mut().next_value() {
        items.push(v);
    }
    Ok(items)
}

// ---- types -----------------------------------------------------------------

/// Whether `value` has the type `name`, or a type derived from it.
pub fn is_type(program: &Program, value: &Value, name: &str) -> bool {
    let mut ty = match value {
        Value::Struct(inst) => Some(inst.ty),
        Value::Object(obj) => Some(obj.borrow().ty),
        Value::Error(e) => return e.kind == name || name == "Error",
        other => return other.type_name() == name,
    };
    while let Some(t) = ty {
        let def = &program.types[t as usize];
        if program.symbols.name(def.name) == name {
            return true;
        }
        ty = def.parent;
    }
    false
}

// ---- devices ---------------------------------------------------------------

/// `value on target`.
pub fn place(ctx: &mut Ctx, value: &Value, target: &Value) -> VmResult<Value> {
    let devices: Vec<Device> = match target {
        Value::Device(d) => vec![*d],
        Value::DeviceGroup(group) => (**group).clone(),
        Value::Region => return Ok(value.clone()),
        Value::Gpus => {
            return Err(type_error(
                "pick a device to place on: `gpu[0]`, or a group such as `gpu[0 to 3]`",
            ))
        }
        other => {
            return Err(type_error(format!(
                "cannot place a value on {}",
                other.type_name()
            )))
        }
    };
    place_on(ctx, value, &devices)
}

/// Moves tensors to `devices[0]`. A list placed on several devices is split
/// into contiguous shards, one per device: the layers of a model are spread
/// across GPUs in order, which is what pipeline-parallel inference needs.
fn place_on(ctx: &mut Ctx, value: &Value, devices: &[Device]) -> VmResult<Value> {
    Ok(match value {
        Value::Tensor(t) => {
            if t.device == devices[0] {
                value.clone()
            } else {
                alloc(
                    ctx.backend(devices[0])
                        .transfer(t, devices[0])
                        .map_err(tensor_error)?,
                )?
            }
        }
        Value::List(items) => {
            let items = items.borrow();
            let shard = items.len().div_ceil(devices.len()).max(1);
            let mut placed = Vec::with_capacity(items.len());
            for (i, item) in items.iter().enumerate() {
                let device = devices[(i / shard).min(devices.len() - 1)];
                placed.push(place_on(ctx, item, &[device])?);
            }
            Value::list(placed)
        }
        Value::Tuple(items) => Value::tuple(
            items
                .iter()
                .map(|item| place_on(ctx, item, devices))
                .collect::<VmResult<_>>()?,
        ),
        Value::Struct(inst) => {
            let fields = inst
                .fields
                .iter()
                .map(|f| place_on(ctx, f, devices))
                .collect::<VmResult<_>>()?;
            Value::Struct(Rc::new(Instance {
                ty: inst.ty,
                fields,
            }))
        }
        Value::Object(obj) => {
            let fields: Vec<Value> = obj.borrow().fields.clone();
            let placed = fields
                .iter()
                .map(|f| place_on(ctx, f, devices))
                .collect::<VmResult<_>>()?;
            obj.borrow_mut().fields = placed;
            value.clone()
        }
        Value::Map(map) => {
            let mut placed = Map::default();
            for (k, v) in map.borrow().entries() {
                if let Some(key) = Key::from_value(k) {
                    placed.insert(key, k.clone(), place_on(ctx, v, devices)?);
                }
            }
            Value::Map(Rc::new(RefCell::new(placed)))
        }
        other => other.clone(),
    })
}

// ---- text ------------------------------------------------------------------

pub fn fmt_float(f: f64) -> String {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e16 {
        format!("{f:.1}")
    } else if f.is_nan() {
        "nan".to_string()
    } else if f.is_infinite() {
        if f > 0.0 { "inf" } else { "-inf" }.to_string()
    } else {
        format!("{f}")
    }
}

/// The text `print` shows.
pub fn display(program: &Program, value: &Value) -> String {
    let mut out = String::new();
    write_value(program, value, false, &mut out);
    out
}

/// The text shown for a value inside a collection: strings are quoted.
pub fn repr(program: &Program, value: &Value) -> String {
    let mut out = String::new();
    write_value(program, value, true, &mut out);
    out
}

fn write_seq(program: &Program, items: &[Value], open: &str, close: &str, out: &mut String) {
    out.push_str(open);
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        write_value(program, item, true, out);
    }
    if open == "(" && items.len() == 1 {
        out.push(',');
    }
    out.push_str(close);
}

fn write_tensor_data(t: &Tensor, dim: usize, offset: usize, out: &mut String) {
    if dim == t.ndim() {
        out.push_str(&fmt_float(widen(t.value(offset))));
        return;
    }
    let stride: usize = t.shape[dim + 1..].iter().product();
    out.push('[');
    for i in 0..t.shape[dim] {
        if i > 0 {
            out.push_str(", ");
        }
        write_tensor_data(t, dim + 1, offset + i * stride, out);
    }
    out.push(']');
}

fn write_value(program: &Program, value: &Value, quoted: bool, out: &mut String) {
    match value {
        Value::None => out.push_str("none"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => out.push_str(&fmt_float(*f)),
        Value::Str(s) => {
            if quoted {
                let _ = write!(out, "{:?}", s.as_str());
            } else {
                out.push_str(s);
            }
        }
        Value::List(items) => write_seq(program, &items.borrow(), "[", "]", out),
        Value::Tuple(items) => write_seq(program, items, "(", ")", out),
        Value::Map(map) => {
            out.push('{');
            for (i, (k, v)) in map.borrow().entries().iter().enumerate() {
                if i > 0 {
                    out.push_str(", ");
                }
                write_value(program, k, true, out);
                out.push_str(": ");
                write_value(program, v, true, out);
            }
            out.push('}');
        }
        Value::Range(r) => {
            let _ = write!(out, "{} until {}", r.start, r.stop);
            if r.step != 1 {
                let _ = write!(out, " by {}", r.step);
            }
        }
        Value::Slice(s) => {
            let part = |p: Option<i64>| p.map(|v| v.to_string()).unwrap_or_default();
            let _ = write!(out, "[{}:{}:{}]", part(s.start), part(s.stop), part(s.step));
        }
        Value::Ellipsis => out.push_str("..."),
        Value::Struct(inst) => write_instance(program, inst, out),
        Value::Object(obj) => write_instance(program, &obj.borrow(), out),
        Value::Type(t) => {
            let _ = write!(
                out,
                "<type {}>",
                program.symbols.name(program.types[*t as usize].name)
            );
        }
        Value::Function(c) => {
            let _ = write!(out, "<fn {}>", c.proto.name);
        }
        Value::Native(id) => {
            let _ = write!(
                out,
                "<builtin {}>",
                super::builtins::NATIVES[*id as usize].name
            );
        }
        Value::Tensor(t) => {
            out.push_str("tensor(");
            if t.numel() > 1000 {
                let _ = write!(out, "shape={:?}", t.shape);
            } else {
                write_tensor_data(t, 0, 0, out);
            }
            if t.dtype() != DType::F32 {
                let _ = write!(out, ", dtype={}", t.dtype());
            }
            if t.device != Device::Cpu {
                let _ = write!(out, ", device={}", t.device);
            }
            out.push(')');
        }
        Value::Device(d) => {
            let _ = write!(out, "{d}");
        }
        Value::Gpus => out.push_str("gpu"),
        Value::DeviceGroup(group) => {
            let names: Vec<String> = group.iter().map(Device::to_string).collect();
            let _ = write!(out, "[{}]", names.join(", "));
        }
        Value::Region => out.push_str("<region>"),
        Value::Iter(_) => out.push_str("<iterator>"),
        Value::Task(t) => {
            let state = if t.is_done() { "done" } else { "running" };
            let _ = write!(out, "<task {} {state}>", t.name);
        }
        Value::Channel(c) => {
            let _ = write!(out, "<channel with {} waiting>", c.borrow().queue.len());
        }
        Value::Foreign(f) => out.push_str(&f.display()),
        Value::Clock(c) => {
            let c = c.borrow();
            let _ = write!(out, "<Clock {} Hz, frame {}>", c.hz, c.frame);
        }
        Value::Error(e) => {
            let _ = write!(out, "{}: {}", e.kind, e.message);
        }
        Value::Missing => out.push_str("<missing>"),
        Value::Cell(cell) => write_value(program, &cell.borrow(), quoted, out),
        Value::Tokenizer(t) => {
            let _ = write!(out, "<tokenizer with {} tokens>", t.vocab_size());
        }
    }
}

fn write_instance(program: &Program, inst: &Instance, out: &mut String) {
    let def = &program.types[inst.ty as usize];
    out.push_str(program.symbols.name(def.name));
    if def.fields.is_empty() {
        return;
    }
    out.push('(');
    for (i, (name, value)) in def.fields.iter().zip(&inst.fields).enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        if !def.positional {
            out.push_str(program.symbols.name(*name));
            out.push('=');
        }
        write_value(program, value, true, out);
    }
    out.push(')');
}

/// Converts parsed JSON to a value: objects become maps, arrays lists, and
/// whole numbers integers.
pub fn from_json(json: &super::json::Json) -> Value {
    use super::json::Json;
    match json {
        Json::Null => Value::None,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) if n.fract() == 0.0 && n.abs() < 9.0e15 => Value::Int(*n as i64),
        Json::Number(n) => Value::Float(*n),
        Json::String(s) => Value::str(s.clone()),
        Json::Array(items) => Value::list(items.iter().map(from_json).collect()),
        Json::Object(entries) => {
            let mut map = Map::default();
            for (k, v) in entries {
                let key = Value::str(k.clone());
                if let Some(hashed) = Key::from_value(&key) {
                    map.insert(hashed, key, from_json(v));
                }
            }
            Value::Map(Rc::new(RefCell::new(map)))
        }
    }
}

/// Serializes a value as JSON, the default response format of routes.
pub fn to_json(program: &Program, value: &Value) -> String {
    let mut out = String::new();
    write_json(program, value, &mut out);
    out
}

fn write_json_str(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

fn write_json(program: &Program, value: &Value, out: &mut String) {
    if let Value::Foreign(f) = value {
        if let Some(v) = f.json() {
            return write_json(program, &v, out);
        }
    }
    match value {
        Value::None | Value::Missing => out.push_str("null"),
        Value::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Value::Int(i) => {
            let _ = write!(out, "{i}");
        }
        Value::Float(f) => {
            if f.is_finite() {
                let _ = write!(out, "{f}");
            } else {
                out.push_str("null");
            }
        }
        Value::Str(s) => write_json_str(s, out),
        Value::List(items) => write_json_array(program, &items.borrow(), out),
        Value::Tuple(items) => write_json_array(program, items, out),
        Value::Map(map) => {
            out.push('{');
            for (i, (k, v)) in map.borrow().entries().iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_json_str(&display(program, k), out);
                out.push(':');
                write_json(program, v, out);
            }
            out.push('}');
        }
        Value::Struct(inst) => write_json_instance(program, inst, out),
        Value::Object(obj) => write_json_instance(program, &obj.borrow(), out),
        Value::Tensor(t) => {
            let mut nested = String::new();
            write_tensor_data(t, 0, 0, &mut nested);
            out.push_str(&nested);
        }
        other => write_json_str(&display(program, other), out),
    }
}

fn write_json_array(program: &Program, items: &[Value], out: &mut String) {
    out.push('[');
    for (i, item) in items.iter().enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_json(program, item, out);
    }
    out.push(']');
}

fn write_json_instance(program: &Program, inst: &Instance, out: &mut String) {
    let def = &program.types[inst.ty as usize];
    if def.fields.is_empty() && def.kind == TypeKind::Variant {
        write_json_str(program.symbols.name(def.name), out);
        return;
    }
    if def.positional {
        write_json_array(program, &inst.fields, out);
        return;
    }
    out.push('{');
    for (i, (name, value)) in def.fields.iter().zip(&inst.fields).enumerate() {
        if i > 0 {
            out.push(',');
        }
        write_json_str(program.symbols.name(*name), out);
        out.push(':');
        write_json(program, value, out);
    }
    out.push('}');
}

pub fn slice_value(start: &Value, stop: &Value, step: &Value) -> VmResult<Value> {
    let part = |v: &Value| match v {
        Value::None => Ok(None),
        Value::Int(i) => Ok(Some(*i)),
        other => Err(type_error(format!(
            "slice bounds must be integers, not {}",
            other.type_name()
        ))),
    };
    Ok(Value::Slice(Rc::new(SliceValue {
        start: part(start)?,
        stop: part(stop)?,
        step: part(step)?,
    })))
}
