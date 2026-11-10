//! Builtin functions and the methods of builtin types.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use super::dtype::DType;
use super::ops::{self, err, type_error, Ctx, VmResult};
use super::tensor::{widen, Device, MapOp, Reduce, Tensor};
use super::value::{Key, Map, Value};

pub type NativeFn = fn(&mut Ctx, &[Value]) -> VmResult<Value>;

pub struct Native {
    pub name: &'static str,
    /// Parameter names, for keyword arguments. Empty for variadic builtins.
    pub params: &'static [&'static str],
    /// Parameters that must be passed.
    pub required: usize,
    pub variadic: bool,
    pub func: NativeFn,
}

const fn fixed(
    name: &'static str,
    params: &'static [&'static str],
    required: usize,
    func: NativeFn,
) -> Native {
    Native {
        name,
        params,
        required,
        variadic: false,
        func,
    }
}

const fn variadic(name: &'static str, func: NativeFn) -> Native {
    Native {
        name,
        params: &[],
        required: 0,
        variadic: true,
        func,
    }
}

pub static NATIVES: &[Native] = &[
    variadic("print", print),
    fixed("len", &["x"], 1, len),
    fixed("str", &["x"], 1, to_str),
    fixed("int", &["x"], 1, to_int),
    fixed("float", &["x"], 1, to_float),
    fixed("abs", &["x"], 1, abs),
    variadic("min", min),
    variadic("max", max),
    fixed("sum", &["xs"], 1, sum),
    fixed("round", &["x", "digits"], 1, round),
    fixed("range", &["start", "stop", "step"], 1, range),
    fixed("enumerate", &["xs"], 1, enumerate),
    variadic("zip", zip),
    fixed("sorted", &["xs"], 1, sorted),
    fixed("sqrt", &["x"], 1, sqrt),
    fixed("exp", &["x"], 1, exp),
    fixed("tanh", &["x"], 1, tanh),
    fixed("relu", &["x"], 1, relu),
    fixed("gelu", &["x"], 1, gelu),
    fixed("softmax", &["x", "axis"], 1, softmax),
    fixed("argmax", &["x", "axis"], 1, argmax),
    fixed("tensor", &["data", "dtype"], 1, tensor),
    fixed("memory", &["x"], 1, memory),
    fixed("memory_used", &["device"], 1, memory_used),
    fixed(
        "set_memory_limit",
        &["device", "bytes"],
        2,
        set_memory_limit,
    ),
    fixed("fit", &["items", "device", "budget", "rest"], 2, fit),
    fixed("format_bytes", &["bytes"], 1, format_bytes),
    fixed("load_weights", &["path", "dtype"], 1, load_weights),
    fixed("save_weights", &["path", "tensors"], 2, save_weights),
    fixed("load_tokenizer", &["path"], 1, load_tokenizer),
    fixed("read_text", &["path"], 1, read_text),
    fixed("write_text", &["path", "text"], 2, write_text),
    fixed("parse_json", &["text"], 1, parse_json),
    fixed("to_json", &["value"], 1, to_json),
    variadic("zeros", zeros),
    variadic("ones", ones),
    variadic("rand", rand),
    fixed("arange", &["n"], 1, arange),
    fixed("eye", &["n"], 1, eye),
    variadic("vec2", vec2),
    variadic("vec3", vec3),
    variadic("vec4", vec4),
    variadic("join", join),
    fixed("sleep", &["seconds"], 1, sleep),
    fixed("channel", &[], 0, channel),
    fixed("now", &[], 0, now),
    fixed("assert", &["condition", "message"], 1, assert),
    fixed("Clock", &["hz", "frames"], 0, clock),
    fixed("web.serve", &["port", "host", "requests"], 0, web_serve),
    fixed("web.provide", &["name", "value"], 2, web_provide),
];

/// The modules of the standard library that `use std.<name>` brings in,
/// and the prefix of their builtins.
pub fn std_module(name: &str) -> Option<&'static str> {
    match name {
        "web" | "http" => Some("web"),
        _ => None,
    }
}

pub fn lookup(name: &str) -> Option<u16> {
    NATIVES
        .iter()
        .position(|n| n.name == name)
        .map(|i| i as u16)
}

/// Builtin names that are values rather than functions.
pub fn constant(name: &str) -> Option<Value> {
    match name {
        "cpu" => Some(Value::Device(Device::Cpu)),
        "gpu" => Some(Value::Gpus),
        "pi" => Some(Value::Float(std::f64::consts::PI)),
        // Element types are values, so they can be passed around:
        // `w.to(q4)`, `tensor(data, f16)`.
        "f32" | "f16" | "bf16" | "q8" | "q4" => Some(Value::str(name)),
        // Byte sizes, for memory budgets: `10 * GB`.
        "KB" => Some(Value::Int(1 << 10)),
        "MB" => Some(Value::Int(1 << 20)),
        "GB" => Some(Value::Int(1 << 30)),
        _ => None,
    }
}

pub fn call(id: u16, ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let native = &NATIVES[id as usize];
    if !native.variadic {
        let given = args.iter().filter(|a| !matches!(a, Value::Missing)).count();
        if args.len() > native.params.len() || given < native.required {
            return Err(err(
                "CallError",
                format!(
                    "`{}` takes {} argument{}, got {}",
                    native.name,
                    if native.required == native.params.len() {
                        native.required.to_string()
                    } else {
                        format!("{} to {}", native.required, native.params.len())
                    },
                    if native.params.len() == 1 { "" } else { "s" },
                    args.len()
                ),
            ));
        }
    }
    (native.func)(ctx, args)
}

/// An optional argument, treating a left-out one as absent.
fn opt(args: &[Value], i: usize) -> Option<&Value> {
    args.get(i).filter(|v| !matches!(v, Value::Missing))
}

fn number(value: &Value, what: &str) -> VmResult<f64> {
    match value {
        Value::Int(i) => Ok(*i as f64),
        Value::Float(f) => Ok(*f),
        other => Err(type_error(format!(
            "{what} must be a number, not {}",
            other.type_name()
        ))),
    }
}

fn integer(value: &Value, what: &str) -> VmResult<i64> {
    match value {
        Value::Int(i) => Ok(*i),
        Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
        other => Err(type_error(format!(
            "{what} must be an integer, not {}",
            other.type_name()
        ))),
    }
}

fn dims(args: &[Value]) -> VmResult<Vec<usize>> {
    let values: Vec<Value> = match args {
        [Value::List(items)] => items.borrow().clone(),
        [Value::Tuple(items)] => (**items).clone(),
        _ => args.to_vec(),
    };
    values
        .iter()
        .map(|v| {
            let d = integer(v, "a dimension")?;
            usize::try_from(d).map_err(|_| err("ValueError", format!("dimension {d} is negative")))
        })
        .collect()
}

fn axis(args: &[Value], i: usize) -> VmResult<Option<i64>> {
    opt(args, i).map(|v| integer(v, "`axis`")).transpose()
}

// ---- general ---------------------------------------------------------------

fn print(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let text: Vec<String> = args.iter().map(|v| ops::display(&ctx.program, v)).collect();
    let _ = writeln!(ctx.out, "{}", text.join(" "));
    Ok(Value::None)
}

fn len(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    ops::len(&args[0]).map(Value::Int)
}

fn to_str(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(ops::display(&ctx.program, &args[0])))
}

fn to_int(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    match &args[0] {
        Value::Int(i) => Ok(Value::Int(*i)),
        Value::Bool(b) => Ok(Value::Int(i64::from(*b))),
        Value::Float(f) if f.is_finite() => Ok(Value::Int(f.trunc() as i64)),
        Value::Str(s) => s
            .trim()
            .parse::<i64>()
            .map(Value::Int)
            .map_err(|_| err("ValueError", format!("{s:?} is not an integer"))),
        other => Err(type_error(format!(
            "cannot convert {} to int",
            other.type_name()
        ))),
    }
}

fn to_float(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    match &args[0] {
        Value::Int(i) => Ok(Value::Float(*i as f64)),
        Value::Float(f) => Ok(Value::Float(*f)),
        Value::Bool(b) => Ok(Value::Float(f64::from(u8::from(*b)))),
        Value::Str(s) => s
            .trim()
            .parse::<f64>()
            .map(Value::Float)
            .map_err(|_| err("ValueError", format!("{s:?} is not a number"))),
        other => Err(type_error(format!(
            "cannot convert {} to float",
            other.type_name()
        ))),
    }
}

fn abs(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    match &args[0] {
        Value::Int(i) => i
            .checked_abs()
            .map(Value::Int)
            .ok_or_else(|| err("Overflow", "integer overflow in `abs`")),
        Value::Float(f) => Ok(Value::Float(f.abs())),
        Value::Tensor(t) => ops::map_tensor(ctx, MapOp::Abs, t),
        other => Err(type_error(format!(
            "cannot take abs of {}",
            other.type_name()
        ))),
    }
}

fn extreme(args: &[Value], name: &str, pick_greater: bool) -> VmResult<Value> {
    let items: Vec<Value> = match args {
        [single] => ops::collect(single)?,
        _ => args.to_vec(),
    };
    let mut best: Option<Value> = None;
    for item in items {
        best = Some(match best {
            None => item,
            Some(current) => {
                let ordering = ops::order(&item, &current)?;
                if (pick_greater && ordering.is_gt()) || (!pick_greater && ordering.is_lt()) {
                    item
                } else {
                    current
                }
            }
        });
    }
    best.ok_or_else(|| err("ValueError", format!("`{name}` of an empty sequence")))
}

fn min(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if let [Value::Tensor(t)] = args {
        return reduce_scalar(t, Reduce::Min);
    }
    extreme(args, "min", false)
}

fn max(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if let [Value::Tensor(t)] = args {
        return reduce_scalar(t, Reduce::Max);
    }
    extreme(args, "max", true)
}

fn reduce_scalar(t: &Tensor, op: Reduce) -> VmResult<Value> {
    use super::tensor::{ReferenceBackend, TensorBackend};
    let r = ReferenceBackend
        .reduce(op, t, None)
        .map_err(ops::tensor_error)?;
    Ok(Value::Float(widen(r.value(0))))
}

fn sum(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if let Value::Tensor(t) = &args[0] {
        let r = ctx
            .backend(t.device)
            .reduce(Reduce::Sum, t, None)
            .map_err(ops::tensor_error)?;
        return Ok(Value::Float(widen(r.value(0))));
    }
    let mut total = Value::Int(0);
    for item in ops::collect(&args[0])? {
        total = ops::arith(ctx, ops::Arith::Add, &total, &item)?;
    }
    Ok(total)
}

fn round(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let x = number(&args[0], "the value to round")?;
    match opt(args, 1) {
        None => Ok(Value::Int(x.round_ties_even() as i64)),
        Some(d) => {
            let factor = 10f64.powi(integer(d, "`digits`")? as i32);
            Ok(Value::Float((x * factor).round_ties_even() / factor))
        }
    }
}

fn range(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (start, stop) = match opt(args, 1) {
        Some(stop) => (args[0].clone(), stop.clone()),
        None => (Value::Int(0), args[0].clone()),
    };
    let step = opt(args, 2).cloned().unwrap_or(Value::Int(1));
    ops::make_range(&start, &stop, &step, false)
}

fn enumerate(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::list(
        ops::collect(&args[0])?
            .into_iter()
            .enumerate()
            .map(|(i, v)| Value::tuple(vec![Value::Int(i as i64), v]))
            .collect(),
    ))
}

fn zip(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let columns: Vec<Vec<Value>> = args.iter().map(ops::collect).collect::<VmResult<_>>()?;
    let n = columns.iter().map(Vec::len).min().unwrap_or(0);
    Ok(Value::list(
        (0..n)
            .map(|i| Value::tuple(columns.iter().map(|c| c[i].clone()).collect()))
            .collect(),
    ))
}

fn sorted(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut items = ops::collect(&args[0])?;
    let mut failure = None;
    items.sort_by(|a, b| {
        ops::order(a, b).unwrap_or_else(|e| {
            failure.get_or_insert(e);
            std::cmp::Ordering::Equal
        })
    });
    match failure {
        Some(e) => Err(e),
        None => Ok(Value::list(items)),
    }
}

/// `join(a, b, ...)` groups tasks so one `await` waits for all of them:
/// `x, y = await join(fetch(1), fetch(2))`.
fn join(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::tuple(args.to_vec()))
}

/// `sleep(seconds)`: a task that finishes after the time has passed.
/// `await sleep(0.5)` pauses the current task and lets others run.
fn sleep(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let seconds = number(&args[0], "the number of seconds")?;
    if !(seconds >= 0.0 && seconds.is_finite()) {
        return Err(err(
            "ValueError",
            "`sleep` needs a non-negative number of seconds",
        ));
    }
    let id = ctx.new_task_id();
    let handle = Rc::new(super::task::TaskHandle::new(id, "sleep"));
    let until = std::time::Instant::now() + std::time::Duration::from_secs_f64(seconds);
    ctx.timers.push((until, handle.clone()));
    Ok(Value::Task(handle))
}

/// `Clock(hz = 60, frames = none)`: a fixed-rate clock for game loops.
/// `frames` stops it after that many ticks.
fn clock(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let hz = match opt(args, 0) {
        Some(v) => number(v, "`hz`")?,
        None => 60.0,
    };
    if !(hz > 0.0 && hz.is_finite()) {
        return Err(err("ValueError", "a Clock needs a positive `hz`"));
    }
    let limit = match opt(args, 1) {
        Some(v) => {
            let n = integer(v, "`frames`")?;
            Some(u64::try_from(n).map_err(|_| err("ValueError", "`frames` must not be negative"))?)
        }
        None => None,
    };
    Ok(Value::Clock(Rc::new(RefCell::new(
        super::clock::Clock::new(hz, limit),
    ))))
}

/// `web.serve(port = 8080, host = "127.0.0.1", requests = none)`: answers
/// HTTP requests with the program's routes. The calling task waits while
/// the server runs; other tasks keep running. `requests` stops the server
/// after that many requests, which is handy in tests.
fn web_serve(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let port = match opt(args, 0) {
        Some(v) => integer(v, "`port`")?,
        None => 8080,
    };
    let port =
        u16::try_from(port).map_err(|_| err("ValueError", format!("{port} is not a port")))?;
    let host = match opt(args, 1) {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => {
            return Err(type_error(format!(
                "`host` must be a string, not {}",
                other.type_name()
            )))
        }
        None => "127.0.0.1".to_string(),
    };
    let remaining = match opt(args, 2) {
        Some(v) => Some(
            u64::try_from(integer(v, "`requests`")?)
                .map_err(|_| err("ValueError", "`requests` must not be negative"))?,
        ),
        None => None,
    };
    let listener = std::net::TcpListener::bind((host.as_str(), port))
        .map_err(|e| err("IoError", format!("cannot listen on {host}:{port}: {e}")))?;
    listener
        .set_nonblocking(true)
        .map_err(|e| err("IoError", e.to_string()))?;
    let bound = listener.local_addr().map(|a| a.port()).unwrap_or(port);
    let _ = writeln!(ctx.out, "serving on http://{host}:{bound}");
    let _ = ctx.out.flush();
    let id = ctx.new_task_id();
    let handle = Rc::new(super::task::TaskHandle::new(id, "web.serve"));
    handle.observed.set(true);
    ctx.servers.push(super::web::Server {
        listener,
        handle: handle.clone(),
        remaining,
        pending: Vec::new(),
    });
    ctx.block_on = Some(handle);
    Ok(Value::None)
}

/// `web.provide("db", value)`: route parameters named `db` receive `value`.
fn web_provide(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let Value::Str(name) = &args[0] else {
        return Err(type_error(
            "`web.provide` takes the parameter name as a string",
        ));
    };
    ctx.providers.insert(name.to_string(), args[1].clone());
    Ok(Value::None)
}

/// `assert(condition, message)`: fails with `AssertionError` when the
/// condition is false.
fn assert(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if args[0].truthy() {
        return Ok(Value::None);
    }
    let message = match opt(args, 1) {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => format!("{other:?}"),
        None => "the assertion is false".to_string(),
    };
    Err(err("AssertionError", message))
}

/// `channel()`: a queue for passing values between tasks.
fn channel(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(Value::Channel(Rc::new(RefCell::new(
        super::task::Channel::default(),
    ))))
}

/// Seconds on a monotonic clock, for measuring time.
fn now(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    thread_local! {
        static START: std::time::Instant = std::time::Instant::now();
    }
    Ok(Value::Float(START.with(|s| s.elapsed().as_secs_f64())))
}

// ---- math on scalars and tensors -------------------------------------------

fn unary(
    ctx: &mut Ctx,
    value: &Value,
    op: MapOp,
    scalar: fn(f64) -> f64,
    name: &str,
) -> VmResult<Value> {
    match value {
        Value::Tensor(t) => ops::map_tensor(ctx, op, t),
        other => Ok(Value::Float(scalar(number(
            other,
            &format!("the argument of `{name}`"),
        )?))),
    }
}

fn sqrt(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    unary(ctx, &args[0], MapOp::Sqrt, f64::sqrt, "sqrt")
}

fn exp(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    unary(ctx, &args[0], MapOp::Exp, f64::exp, "exp")
}

fn tanh(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    unary(ctx, &args[0], MapOp::Tanh, f64::tanh, "tanh")
}

fn relu(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    unary(ctx, &args[0], MapOp::Relu, |x| x.max(0.0), "relu")
}

fn gelu(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    unary(
        ctx,
        &args[0],
        MapOp::Gelu,
        |x| {
            0.5 * x
                * (1.0 + ((2.0 / std::f64::consts::PI).sqrt() * (x + 0.044_715 * x.powi(3))).tanh())
        },
        "gelu",
    )
}

fn expect_tensor<'a>(value: &'a Value, name: &str) -> VmResult<&'a Tensor> {
    match value {
        Value::Tensor(t) => Ok(t),
        other => Err(type_error(format!(
            "`{name}` needs a tensor, not {}",
            other.type_name()
        ))),
    }
}

fn softmax(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let t = expect_tensor(&args[0], "softmax")?;
    let axis = axis(args, 1)?.unwrap_or(-1);
    ctx.backend(t.device)
        .softmax(t, axis)
        .map_err(ops::tensor_error)
        .and_then(ops::alloc)
}

fn argmax(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let t = expect_tensor(&args[0], "argmax")?;
    let axis = axis(args, 1)?;
    let r = ctx
        .backend(t.device)
        .reduce(Reduce::ArgMax, t, axis)
        .map_err(ops::tensor_error)?;
    Ok(if r.shape.is_empty() {
        Value::Int(r.value(0) as i64)
    } else {
        Value::tensor(r)
    })
}

// ---- creating tensors ------------------------------------------------------

fn flatten_into(
    value: &Value,
    depth: usize,
    shape: &mut Vec<usize>,
    data: &mut Vec<f32>,
) -> VmResult<()> {
    let items: Vec<Value> = match value {
        Value::List(items) => items.borrow().clone(),
        Value::Tuple(items) => (**items).clone(),
        Value::Tensor(t) => {
            if shape.len() == depth {
                shape.extend_from_slice(&t.shape);
            }
            data.extend_from_slice(&t.values());
            return Ok(());
        }
        other => {
            if shape.len() != depth {
                return Err(err(
                    "ValueError",
                    "rows of a tensor must all have the same length",
                ));
            }
            data.push(number(other, "a tensor element")? as f32);
            return Ok(());
        }
    };
    if shape.len() == depth {
        shape.push(items.len());
    } else if shape.get(depth) != Some(&items.len()) {
        return Err(err(
            "ValueError",
            "rows of a tensor must all have the same length",
        ));
    }
    for item in &items {
        flatten_into(item, depth + 1, shape, data)?;
    }
    Ok(())
}

fn tensor(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut shape = Vec::new();
    let mut data = Vec::new();
    flatten_into(&args[0], 0, &mut shape, &mut data)?;
    let device = ctx.current_device();
    let t = Tensor::new(shape, data, device).map_err(ops::tensor_error)?;
    ops::alloc(match opt(args, 1) {
        Some(dtype) => t.to_dtype(dtype_arg(dtype)?),
        None => t,
    })
}

/// An element type given as `f16`, `q4` and so on.
fn dtype_arg(value: &Value) -> VmResult<DType> {
    let name = match value {
        Value::Str(s) => s.as_str(),
        other => {
            return Err(type_error(format!(
                "expected an element type such as `f16` or `q4`, not {}",
                other.type_name()
            )))
        }
    };
    DType::from_name(name).ok_or_else(|| {
        err(
            "ValueError",
            format!("unknown element type `{name}`; use f32, f16, bf16, q8 or q4"),
        )
    })
}

fn string_arg<'a>(value: &'a Value, what: &str) -> VmResult<&'a str> {
    match value {
        Value::Str(s) => Ok(s.as_str()),
        other => Err(type_error(format!(
            "{what} must be a string, not {}",
            other.type_name()
        ))),
    }
}

fn io_error(message: String) -> Value {
    err("IoError", message)
}

/// `load_weights(path, dtype?)`: the tensors of a safetensors file, as a
/// map from name to tensor, created on the current device and optionally
/// converted while loading: `load_weights("model.safetensors", q4)`.
fn load_weights(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = string_arg(&args[0], "the path")?;
    let dtype = opt(args, 1).map(dtype_arg).transpose()?;
    let tensors = super::weights::load(path, ctx.current_device(), dtype).map_err(io_error)?;
    let mut map = Map::default();
    for (name, tensor) in tensors {
        let key = Value::str(name);
        if let Some(hashed) = Key::from_value(&key) {
            map.insert(hashed, key, ops::alloc(tensor)?);
        }
    }
    Ok(Value::Map(Rc::new(RefCell::new(map))))
}

/// `save_weights(path, tensors)`: writes a map of name to tensor.
fn save_weights(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = string_arg(&args[0], "the path")?;
    let Value::Map(map) = &args[1] else {
        return Err(type_error("`save_weights` takes a map from name to tensor"));
    };
    let map = map.borrow();
    let mut tensors = Vec::with_capacity(map.len());
    for (k, v) in map.entries() {
        match (k, v) {
            (Value::Str(name), Value::Tensor(t)) => tensors.push(((**name).clone(), &***t)),
            _ => return Err(type_error("`save_weights` takes a map from name to tensor")),
        }
    }
    super::weights::save(path, &tensors).map_err(io_error)?;
    Ok(Value::None)
}

fn load_tokenizer(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = string_arg(&args[0], "the path")?;
    let tokenizer = super::tokenizer::Tokenizer::from_file(path).map_err(io_error)?;
    Ok(Value::Tokenizer(Rc::new(tokenizer)))
}

fn read_text(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = string_arg(&args[0], "the path")?;
    std::fs::read_to_string(path)
        .map(Value::str)
        .map_err(|e| io_error(format!("cannot read {path}: {e}")))
}

fn write_text(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = string_arg(&args[0], "the path")?;
    let text = ops::display(&ctx.program, &args[1]);
    std::fs::write(path, text).map_err(|e| io_error(format!("cannot write {path}: {e}")))?;
    Ok(Value::None)
}

fn parse_json(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let text = string_arg(&args[0], "the JSON text")?;
    let json = super::json::parse(text).map_err(|e| err("ValueError", e))?;
    Ok(ops::from_json(&json))
}

fn to_json(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(ops::to_json(&ctx.program, &args[0])))
}

fn device_arg(value: &Value) -> VmResult<Device> {
    match value {
        Value::Device(d) => Ok(*d),
        Value::Gpus => Err(type_error("pick one device, such as `gpu[0]`")),
        other => Err(type_error(format!(
            "expected a device such as `cpu` or `gpu[0]`, not {}",
            other.type_name()
        ))),
    }
}

fn byte_count(value: &Value) -> VmResult<usize> {
    let n = integer(value, "a size in bytes")?;
    usize::try_from(n).map_err(|_| err("ValueError", "a size in bytes cannot be negative"))
}

fn memory_used(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let device = device_arg(&args[0])?;
    Ok(Value::Int(super::tensor::memory_used(device) as i64))
}

/// `set_memory_limit(gpu[0], 12 * GB)` makes the device behave as if it had
/// that much memory: going over is an `OutOfMemory` error. `none` removes
/// the limit.
fn set_memory_limit(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let device = device_arg(&args[0])?;
    let limit = match &args[1] {
        Value::None => None,
        other => Some(byte_count(other)?),
    };
    super::tensor::set_memory_limit(device, limit);
    Ok(Value::None)
}

/// `fit(layers, gpu[0], 10 * GB)`: places items on `device`, in order, for
/// as long as their tensors fit in the budget, and the rest on `rest`
/// (the CPU unless given). Without a budget, the device's free memory under
/// its limit is used. Returns the placed items as a new list.
fn fit(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let items = ops::collect(&args[0])?;
    let device = device_arg(&args[1])?;
    let budget = match opt(args, 2) {
        Some(Value::None) | None => match super::tensor::memory_limit(device) {
            Some(limit) => limit.saturating_sub(super::tensor::memory_used(device)),
            None => {
                return Err(err(
                    "ValueError",
                    format!(
                        "give `fit` a budget, or set a limit for {device} with `set_memory_limit`"
                    ),
                ))
            }
        },
        Some(budget) => byte_count(budget)?,
    };
    let rest = match opt(args, 3) {
        Some(v) => v.clone(),
        None => Value::Device(Device::Cpu),
    };
    let target = Value::Device(device);
    let mut used = 0usize;
    let mut spilled = false;
    let mut placed = Vec::with_capacity(items.len());
    for item in &items {
        let size = ops::tensor_bytes(item);
        // Layers stay in order: once one spills, the rest follow it.
        if !spilled && used + size <= budget {
            used += size;
            placed.push(ops::place(ctx, item, &target)?);
        } else {
            spilled = true;
            placed.push(ops::place(ctx, item, &rest)?);
        }
    }
    Ok(Value::list(placed))
}

fn format_bytes(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(super::tensor::format_bytes(byte_count(
        &args[0],
    )?)))
}

/// Bytes of tensor memory inside a value.
fn memory(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Int(ops::tensor_bytes(&args[0]) as i64))
}

fn zeros(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    ops::alloc(Tensor::filled(dims(args)?, 0.0, ctx.current_device()))
}

fn ones(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    ops::alloc(Tensor::filled(dims(args)?, 1.0, ctx.current_device()))
}

fn rand(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let shape = dims(args)?;
    let n: usize = shape.iter().product();
    let data = (0..n).map(|_| ctx.random()).collect();
    let device = ctx.current_device();
    Tensor::new(shape, data, device)
        .map_err(ops::tensor_error)
        .and_then(ops::alloc)
}

fn arange(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = usize::try_from(integer(&args[0], "`n`")?)
        .map_err(|_| err("ValueError", "`arange` needs a non-negative length"))?;
    let device = ctx.current_device();
    Tensor::new(vec![n], (0..n).map(|i| i as f32).collect(), device)
        .map_err(ops::tensor_error)
        .and_then(ops::alloc)
}

fn eye(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = usize::try_from(integer(&args[0], "`n`")?)
        .map_err(|_| err("ValueError", "`eye` needs a non-negative size"))?;
    let mut data = vec![0.0; n * n];
    for i in 0..n {
        data[i * n + i] = 1.0;
    }
    Tensor::new(vec![n, n], data, ctx.current_device())
        .map_err(ops::tensor_error)
        .and_then(ops::alloc)
}

fn vector(ctx: &mut Ctx, args: &[Value], n: usize) -> VmResult<Value> {
    if args.len() != n {
        return Err(err(
            "CallError",
            format!("`vec{n}` takes {n} numbers, got {}", args.len()),
        ));
    }
    let data = args
        .iter()
        .map(|v| number(v, "a vector component").map(|x| x as f32))
        .collect::<VmResult<_>>()?;
    Tensor::new(vec![n], data, ctx.current_device())
        .map_err(ops::tensor_error)
        .and_then(ops::alloc)
}

fn vec2(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    vector(ctx, args, 2)
}

fn vec3(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    vector(ctx, args, 3)
}

fn vec4(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    vector(ctx, args, 4)
}

// ---- methods of builtin types ----------------------------------------------

/// Calls method `name` on `args[0]` with the remaining arguments.
pub fn call_method(ctx: &mut Ctx, name: &str, args: &[Value]) -> VmResult<Value> {
    let (receiver, rest) = (&args[0], &args[1..]);
    let arity = |n: usize| -> VmResult<()> {
        if rest.len() == n {
            Ok(())
        } else {
            Err(err(
                "CallError",
                format!(
                    "`.{name}()` takes {n} argument{}, got {}",
                    if n == 1 { "" } else { "s" },
                    rest.len()
                ),
            ))
        }
    };
    match receiver {
        Value::List(items) => match name {
            "append" => {
                arity(1)?;
                items.borrow_mut().push(rest[0].clone());
                Ok(Value::None)
            }
            "extend" => {
                arity(1)?;
                let more = ops::collect(&rest[0])?;
                items.borrow_mut().extend(more);
                Ok(Value::None)
            }
            "insert" => {
                arity(2)?;
                let mut items = items.borrow_mut();
                let i = integer(&rest[0], "the position")?;
                let len = items.len() as i64;
                let at = if i < 0 { (i + len).max(0) } else { i.min(len) };
                items.insert(at as usize, rest[1].clone());
                Ok(Value::None)
            }
            "pop" => {
                let mut items = items.borrow_mut();
                match rest {
                    [] => items
                        .pop()
                        .ok_or_else(|| err("IndexError", "pop from an empty list")),
                    [i] => {
                        let i = integer(i, "the position")?;
                        let len = items.len();
                        let at = super::tensor::normalize_index(i, len).ok_or_else(|| {
                            err(
                                "IndexError",
                                format!("index {i} is out of range for a list of length {len}"),
                            )
                        })?;
                        Ok(items.remove(at))
                    }
                    _ => Err(err("CallError", "`.pop()` takes at most one argument")),
                }
            }
            "clear" => {
                arity(0)?;
                items.borrow_mut().clear();
                Ok(Value::None)
            }
            "reverse" => {
                arity(0)?;
                items.borrow_mut().reverse();
                Ok(Value::None)
            }
            "sort" => {
                arity(0)?;
                let sorted_items = sorted(ctx, std::slice::from_ref(receiver))?;
                if let Value::List(new) = sorted_items {
                    let new = new.borrow().clone();
                    *items.borrow_mut() = new;
                }
                Ok(Value::None)
            }
            "copy" => {
                arity(0)?;
                Ok(Value::list(items.borrow().clone()))
            }
            "index" => {
                arity(1)?;
                items
                    .borrow()
                    .iter()
                    .position(|v| ops::equals(v, &rest[0]))
                    .map(|i| Value::Int(i as i64))
                    .ok_or_else(|| err("ValueError", "the value is not in the list"))
            }
            _ => Err(no_method(receiver, name)),
        },
        Value::Str(text) => {
            let text = text.as_str();
            let string_arg = |i: usize| -> VmResult<String> {
                match rest.get(i) {
                    Some(Value::Str(s)) => Ok((**s).clone()),
                    Some(other) => Err(type_error(format!(
                        "expected a string, not {}",
                        other.type_name()
                    ))),
                    None => Err(err(
                        "CallError",
                        format!("`.{name}()` is missing an argument"),
                    )),
                }
            };
            match name {
                "upper" => Ok(Value::str(text.to_uppercase())),
                "lower" => Ok(Value::str(text.to_lowercase())),
                "strip" => Ok(Value::str(text.trim())),
                "split" => {
                    let parts: Vec<Value> = if rest.is_empty() {
                        text.split_whitespace().map(Value::str).collect()
                    } else {
                        let sep = string_arg(0)?;
                        if sep.is_empty() {
                            return Err(err("ValueError", "cannot split on an empty separator"));
                        }
                        text.split(sep.as_str()).map(Value::str).collect()
                    };
                    Ok(Value::list(parts))
                }
                "join" => {
                    arity(1)?;
                    let parts: Vec<String> = ops::collect(&rest[0])?
                        .iter()
                        .map(|v| ops::display(&ctx.program, v))
                        .collect();
                    Ok(Value::str(parts.join(text)))
                }
                "replace" => {
                    arity(2)?;
                    Ok(Value::str(text.replace(&string_arg(0)?, &string_arg(1)?)))
                }
                "startswith" => Ok(Value::Bool(text.starts_with(&string_arg(0)?))),
                "endswith" => Ok(Value::Bool(text.ends_with(&string_arg(0)?))),
                "find" => Ok(Value::Int(
                    text.find(&string_arg(0)?)
                        .map(|byte| text[..byte].chars().count() as i64)
                        .unwrap_or(-1),
                )),
                _ => Err(no_method(receiver, name)),
            }
        }
        Value::Clock(clock) => match name {
            "tick" => {
                arity(0)?;
                Ok(Value::Float(clock.borrow_mut().tick()))
            }
            "stop" => {
                arity(0)?;
                clock.borrow_mut().stopped = true;
                Ok(Value::None)
            }
            _ => Err(no_method(receiver, name)),
        },
        Value::Channel(ch) => match name {
            "send" => {
                arity(1)?;
                let mut ch = ch.borrow_mut();
                if ch.closed {
                    return Err(err("ChannelClosed", "cannot send on a closed channel"));
                }
                ch.queue.push_back(rest[0].clone());
                let waiters = std::mem::take(&mut ch.waiters);
                ctx.wakeups.extend(waiters);
                Ok(Value::None)
            }
            "close" => {
                arity(0)?;
                let mut ch = ch.borrow_mut();
                ch.closed = true;
                let waiters = std::mem::take(&mut ch.waiters);
                ctx.wakeups.extend(waiters);
                Ok(Value::None)
            }
            "len" => Ok(Value::Int(ch.borrow().queue.len() as i64)),
            _ => Err(no_method(receiver, name)),
        },
        Value::Tokenizer(tokenizer) => match name {
            "encode" => {
                arity(1)?;
                let text = string_arg(&rest[0], "the text")?;
                Ok(Value::list(
                    tokenizer
                        .encode(text)
                        .into_iter()
                        .map(|id| Value::Int(i64::from(id)))
                        .collect(),
                ))
            }
            "decode" => {
                arity(1)?;
                let ids = match &rest[0] {
                    Value::Int(id) => vec![*id],
                    other => ops::collect(other)?
                        .iter()
                        .map(|v| match v {
                            Value::Int(i) => Ok(*i),
                            Value::Float(f) if f.fract() == 0.0 => Ok(*f as i64),
                            other => Err(type_error(format!(
                                "a token id must be an integer, not {}",
                                other.type_name()
                            ))),
                        })
                        .collect::<VmResult<_>>()?,
                };
                let ids: Vec<u32> = ids
                    .into_iter()
                    .filter_map(|i| u32::try_from(i).ok())
                    .collect();
                Ok(Value::str(tokenizer.decode(&ids)))
            }
            "token_id" => {
                arity(1)?;
                let token = string_arg(&rest[0], "the token")?;
                Ok(tokenizer
                    .token_id(token)
                    .map(|id| Value::Int(i64::from(id)))
                    .unwrap_or(Value::None))
            }
            _ => Err(no_method(receiver, name)),
        },
        Value::Map(map) => match name {
            "keys" => Ok(Value::list(
                map.borrow()
                    .entries()
                    .iter()
                    .map(|(k, _)| k.clone())
                    .collect(),
            )),
            "values" => Ok(Value::list(
                map.borrow()
                    .entries()
                    .iter()
                    .map(|(_, v)| v.clone())
                    .collect(),
            )),
            "items" => Ok(Value::list(
                map.borrow()
                    .entries()
                    .iter()
                    .map(|(k, v)| Value::tuple(vec![k.clone(), v.clone()]))
                    .collect(),
            )),
            "get" => {
                let key = rest
                    .first()
                    .ok_or_else(|| err("CallError", "`.get()` needs a key"))?;
                let key = Key::from_value(key).ok_or_else(|| {
                    type_error(format!("{} cannot be a map key", key.type_name()))
                })?;
                Ok(map
                    .borrow()
                    .get(&key)
                    .cloned()
                    .unwrap_or_else(|| rest.get(1).cloned().unwrap_or(Value::None)))
            }
            _ => Err(no_method(receiver, name)),
        },
        Value::Tensor(t) => {
            let backend_reduce = |ctx: &mut Ctx, op: Reduce| -> VmResult<Value> {
                let r = ctx
                    .backend(t.device)
                    .reduce(op, t, axis(rest, 0)?)
                    .map_err(ops::tensor_error)?;
                Ok(if r.shape.is_empty() {
                    if op == Reduce::ArgMax {
                        Value::Int(r.value(0) as i64)
                    } else {
                        Value::Float(widen(r.value(0)))
                    }
                } else {
                    Value::tensor(r)
                })
            };
            match name {
                "sum" => backend_reduce(ctx, Reduce::Sum),
                "mean" => backend_reduce(ctx, Reduce::Mean),
                "max" => backend_reduce(ctx, Reduce::Max),
                "min" => backend_reduce(ctx, Reduce::Min),
                "argmax" => backend_reduce(ctx, Reduce::ArgMax),
                "length" => {
                    arity(0)?;
                    let squares: f32 = t.values().iter().map(|x| x * x).sum();
                    Ok(Value::Float(widen(squares.sqrt())))
                }
                "reshape" => t
                    .reshape(dims(rest)?)
                    .map_err(ops::tensor_error)
                    .and_then(ops::alloc),
                "to" => {
                    arity(1)?;
                    let dtype = dtype_arg(&rest[0])?;
                    Ok(Value::tensor(t.to_dtype(dtype)))
                }
                "tolist" => {
                    arity(0)?;
                    Ok(tensor_to_list(t, 0, 0))
                }
                "item" => {
                    arity(0)?;
                    if t.numel() == 1 {
                        Ok(Value::Float(widen(t.value(0))))
                    } else {
                        Err(err(
                            "ValueError",
                            "`.item()` needs a tensor with one element",
                        ))
                    }
                }
                _ => Err(no_method(receiver, name)),
            }
        }
        _ => Err(no_method(receiver, name)),
    }
}

fn tensor_to_list(t: &Tensor, dim: usize, offset: usize) -> Value {
    if dim == t.ndim() {
        return Value::Float(widen(t.value(offset)));
    }
    let stride: usize = t.shape[dim + 1..].iter().product();
    Value::list(
        (0..t.shape[dim])
            .map(|i| tensor_to_list(t, dim + 1, offset + i * stride))
            .collect(),
    )
}

fn no_method(receiver: &Value, name: &str) -> Value {
    err(
        "FieldError",
        format!("{} has no method `{name}`", receiver.type_name()),
    )
}

/// An empty map value.
pub fn new_map() -> Value {
    Value::Map(Rc::new(RefCell::new(Map::default())))
}
