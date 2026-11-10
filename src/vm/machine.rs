//! The virtual machine: executes compiled programs.
//!
//! All frames share one register stack. A frame is a window into it,
//! starting at `base`; a call slides the window up so that the callee's
//! parameters are the caller's argument registers, so arguments are never
//! copied. The stack only grows: a return resets the callee's registers to
//! `none`, which releases what they held at once, without reallocating. The dispatch loop runs without recursion: calls and returns
//! switch frames inside the same loop, so deep Vyrn recursion cannot overflow
//! the Rust stack.
//!
//! The common cases of the hottest instructions (integer and float
//! arithmetic, comparisons, list indexing, struct fields) are handled inline
//! in the loop; everything else goes through [`super::ops`].

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

use super::builtins;
use super::op::Op;
use super::ops::{self, err, type_error, Arith, Cmp, Ctx, VmResult};
use super::program::{Capture, GlobalInit, ParamSource, Program, TypeKind};
use super::tensor::{ReferenceBackend, TensorBackend};
use super::value::{Closure, Instance, Symbol, Value};
use crate::ast::{HttpMethod, PathSegment};

/// Calls nested deeper than this fail with `StackOverflow`.
const MAX_FRAMES: usize = 10_000;

struct Frame {
    closure: Rc<Closure>,
    ip: usize,
    base: usize,
    /// Absolute register that receives the return value.
    ret: usize,
}

struct Handler {
    /// Number of frames when the handler was installed.
    frame_depth: usize,
    target: usize,
    err_slot: usize,
    device_depth: usize,
}

/// One line of a runtime error's trace.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceLine {
    pub function: String,
    pub line: u32,
}

/// A failure nothing caught.
#[derive(Clone, Debug)]
pub struct RuntimeError {
    /// The error kind (`IndexError`, `DivisionByZero`, ...) or, for a value
    /// passed to `fail`, its type name.
    pub kind: String,
    pub message: String,
    /// Innermost call first.
    pub trace: Vec<TraceLine>,
    /// The value that was raised.
    pub value: Value,
}

impl RuntimeError {
    fn new(kind: &str, message: impl Into<String>) -> RuntimeError {
        RuntimeError {
            kind: kind.to_string(),
            message: message.into(),
            trace: Vec::new(),
            value: Value::None,
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind, self.message)?;
        for line in &self.trace {
            write!(f, "\n  in {} at line {}", line.function, line.line)?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}

pub struct Vm {
    program: Rc<Program>,
    globals: Vec<Value>,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    /// Method lookup with inheritance already resolved: (type, name).
    methods: HashMap<(u16, Symbol), Rc<Closure>>,
    /// A capture-free closure for every function, shared by all calls.
    functions: Vec<Rc<Closure>>,
    ctx: Ctx,
    initialized: bool,
}

impl Vm {
    /// A VM that prints to standard output.
    pub fn new(program: Program) -> Vm {
        Vm::with_output(program, Box::new(std::io::stdout()))
    }

    pub fn with_output(program: Program, out: Box<dyn Write>) -> Vm {
        let program = Rc::new(program);
        let functions: Vec<Rc<Closure>> = program
            .protos
            .iter()
            .map(|proto| {
                Rc::new(Closure {
                    proto: proto.clone(),
                    captures: Vec::new(),
                })
            })
            .collect();
        let globals = program
            .globals
            .iter()
            .map(|global| match global.init {
                GlobalInit::Function(p) => Value::Function(functions[p].clone()),
                GlobalInit::Type(t) => Value::Type(t),
                GlobalInit::Const => Value::None,
            })
            .collect();
        let mut methods = HashMap::new();
        for ty in 0..program.types.len() {
            let mut chain = Vec::new();
            let mut current = Some(ty as u16);
            while let Some(t) = current {
                chain.push(t);
                current = program.types[t as usize].parent;
            }
            // Ancestors first, so that a subtype's method replaces theirs.
            for &t in chain.iter().rev() {
                for (&name, &proto) in &program.types[t as usize].methods {
                    methods.insert((ty as u16, name), functions[proto].clone());
                }
            }
        }
        let ctx = Ctx {
            program: program.clone(),
            out,
            devices: Vec::new(),
            cpu: Box::new(ReferenceBackend),
            gpu: Box::new(ReferenceBackend),
            rng: 0x9E37_79B9_7F4A_7C15,
        };
        Vm {
            program,
            globals,
            stack: Vec::with_capacity(1024),
            frames: Vec::new(),
            handlers: Vec::new(),
            methods,
            functions,
            ctx,
            initialized: false,
        }
    }

    /// Installs the backend that executes tensor instructions for `gpu[i]`
    /// devices.
    pub fn set_gpu_backend(&mut self, backend: Box<dyn TensorBackend>) {
        self.ctx.gpu = backend;
    }

    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Evaluates the program's constants. Runs once; the other entry points
    /// call it as needed.
    pub fn init(&mut self) -> Result<(), RuntimeError> {
        if self.initialized {
            return Ok(());
        }
        self.initialized = true;
        let init = Value::Function(self.functions[self.program.init].clone());
        self.call_value(init, Vec::new()).map(|_| ())
    }

    /// Runs the `main:` block. `args` is passed if `main` declares a
    /// parameter.
    pub fn run_main(&mut self, args: &[String]) -> Result<Value, RuntimeError> {
        self.init()?;
        let Some(main) = self.program.main else {
            return Err(RuntimeError::new(
                "NoMain",
                "this program has no `main:` block",
            ));
        };
        let wants_args = !self.program.protos[main].params.is_empty();
        let args = if wants_args {
            vec![Value::list(
                args.iter().map(|a| Value::str(a.clone())).collect(),
            )]
        } else {
            Vec::new()
        };
        let main = Value::Function(self.functions[main].clone());
        self.call_value(main, args)
    }

    /// Calls a top-level function by name.
    pub fn call_global(&mut self, name: &str, args: Vec<Value>) -> Result<Value, RuntimeError> {
        self.init()?;
        let Some(slot) = self.program.global(name) else {
            return Err(RuntimeError::new(
                "NameError",
                format!("`{name}` is not defined"),
            ));
        };
        let callee = self.globals[slot].clone();
        self.call_value(callee, args)
    }

    /// Dispatches a request to the matching `route`. Path parameters are
    /// converted to their declared types; every other parameter is looked
    /// up by name in `providers`. Returns `None` when no route matches.
    pub fn call_route(
        &mut self,
        method: HttpMethod,
        path: &str,
        providers: &HashMap<String, Value>,
    ) -> Result<Option<Value>, RuntimeError> {
        self.init()?;
        let program = self.program.clone();
        for route in &program.routes {
            if route.method != method {
                continue;
            }
            let Some(captured) = match_path(&route.segments, path) else {
                continue;
            };
            let mut args = Vec::with_capacity(route.params.len());
            for param in &route.params {
                let value = match param.source {
                    ParamSource::Path => {
                        let text = captured.get(&param.name).cloned().unwrap_or_default();
                        convert_path_param(&text, param.ty.as_deref()).ok_or_else(|| {
                            RuntimeError::new(
                                "BadRequest",
                                format!(
                                    "path parameter `{}` must be {}, got {text:?}",
                                    param.name,
                                    param.ty.as_deref().unwrap_or("text")
                                ),
                            )
                        })?
                    }
                    ParamSource::Provided => {
                        providers.get(&param.name).cloned().ok_or_else(|| {
                            RuntimeError::new(
                                "MissingProvider",
                                format!(
                                    "the route needs a value for `{}`; pass it in `providers`",
                                    param.name
                                ),
                            )
                        })?
                    }
                };
                args.push(value);
            }
            let handler = Value::Function(self.functions[route.proto].clone());
            return self.call_value(handler, args).map(Some);
        }
        Ok(None)
    }

    /// The text `print` would show for a value.
    pub fn display(&self, value: &Value) -> String {
        ops::display(&self.program, value)
    }

    /// A value as JSON, as a route would send it.
    pub fn to_json(&self, value: &Value) -> String {
        ops::to_json(&self.program, value)
    }

    // ==== calls ============================================================

    /// Calls `callee` from the host and runs it to completion.
    pub fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, RuntimeError> {
        let slot = self.stack.len();
        let argc = args.len();
        let depth = self.frames.len();
        let handlers = self.handlers.len();
        let devices = self.ctx.devices.len();
        self.stack.push(callee.clone());
        self.stack.extend(args);
        let result = match callee {
            Value::Function(closure) => match self.push_frame(closure, slot, slot + 1, argc) {
                Ok(()) => self.execute(depth + 1),
                Err(e) => Err(self.runtime_error(e, depth + 1)),
            },
            Value::Native(id) => builtins::call(id, &mut self.ctx, &self.stack[slot + 1..])
                .map_err(|e| self.runtime_error(e, depth + 1)),
            other => Err(RuntimeError::new(
                "TypeError",
                format!("{} is not callable", other.type_name()),
            )),
        };
        self.frames.truncate(depth);
        self.handlers.truncate(handlers);
        self.ctx.devices.truncate(devices);
        self.stack.truncate(slot);
        result
    }

    #[inline]
    fn push_frame(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        let proto = &closure.proto;
        let arity = proto.params.len();
        if argc > arity || argc < proto.required {
            let expected = if proto.required == arity {
                arity.to_string()
            } else {
                format!("{} to {arity}", proto.required)
            };
            return Err(err(
                "CallError",
                format!(
                    "`{}` takes {expected} argument{}, got {argc}",
                    proto.name,
                    if arity == 1 { "" } else { "s" }
                ),
            ));
        }
        if self.frames.len() >= MAX_FRAMES {
            return Err(err(
                "StackOverflow",
                format!(
                    "calls are nested more than {MAX_FRAMES} deep; is there unbounded recursion?"
                ),
            ));
        }
        let needed = base + proto.num_regs.max(arity);
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::None);
        }
        for slot in &mut self.stack[base + argc..base + arity] {
            *slot = Value::Missing;
        }
        self.frames.push(Frame {
            closure,
            ip: 0,
            base,
            ret,
        });
        Ok(())
    }

    /// Pops the finished frame. Returns the value if the frame was the
    /// entry point of the current `execute`.
    #[inline]
    fn finish_frame(&mut self, value: Value, entry_depth: usize) -> Option<Value> {
        let Some(frame) = self.frames.pop() else {
            return Some(value);
        };
        let depth = self.frames.len();
        while self.handlers.last().is_some_and(|h| h.frame_depth > depth) {
            self.handlers.pop();
        }
        if depth < entry_depth {
            return Some(value);
        }
        self.stack[frame.ret] = value;
        if let Some(caller) = self.frames.last() {
            // Release the registers only the finished frame used. Its
            // window starts inside the caller's, and the part that overlaps
            // holds the caller's dead temporaries, which can wait.
            let caller_end = caller.base + caller.closure.proto.num_regs;
            let callee_end = frame.base + frame.closure.proto.num_regs;
            if callee_end > caller_end {
                release(&mut self.stack[caller_end..callee_end]);
            }
        }
        None
    }

    /// Transfers control to the innermost handler, or reports the failure if
    /// no handler inside this `execute` call can take it.
    fn unwind(&mut self, thrown: Value, entry_depth: usize) -> Result<(), RuntimeError> {
        if self
            .handlers
            .last()
            .is_some_and(|h| h.frame_depth >= entry_depth)
        {
            if let Some(handler) = self.handlers.pop() {
                let unwound_end = self.frames[handler.frame_depth..]
                    .iter()
                    .map(|f| f.base + f.closure.proto.num_regs)
                    .max();
                self.frames.truncate(handler.frame_depth);
                if let Some(frame) = self.frames.last_mut() {
                    frame.ip = handler.target;
                    let end = frame.base + frame.closure.proto.num_regs;
                    if let Some(unwound_end) = unwound_end.filter(|e| *e > end) {
                        release(&mut self.stack[end..unwound_end]);
                    }
                }
                self.stack[handler.err_slot] = thrown;
                self.ctx.devices.truncate(handler.device_depth);
                return Ok(());
            }
        }
        Err(self.runtime_error(thrown, entry_depth))
    }

    fn runtime_error(&self, thrown: Value, entry_depth: usize) -> RuntimeError {
        let (kind, message) = match &thrown {
            Value::Error(e) => (e.kind.to_string(), e.message.clone()),
            Value::Struct(inst) => (self.type_name(inst.ty), self.display(&thrown)),
            Value::Object(obj) => (self.type_name(obj.borrow().ty), self.display(&thrown)),
            other => ("Failure".to_string(), self.display(other)),
        };
        let first = entry_depth.saturating_sub(1).min(self.frames.len());
        let trace = self.frames[first..]
            .iter()
            .rev()
            .map(|frame| TraceLine {
                function: frame.closure.proto.name.clone(),
                line: frame
                    .closure
                    .proto
                    .lines
                    .get(frame.ip.saturating_sub(1))
                    .copied()
                    .unwrap_or(0),
            })
            .collect();
        RuntimeError {
            kind,
            message,
            trace,
            value: thrown,
        }
    }

    fn type_name(&self, ty: u16) -> String {
        self.program
            .symbols
            .name(self.program.types[ty as usize].name)
            .to_string()
    }

    fn field_slot(&self, ty: u16, name: Symbol) -> Option<usize> {
        self.program.types[ty as usize]
            .fields
            .iter()
            .position(|f| *f == name)
    }

    fn no_field(&self, ty: u16, name: Symbol) -> Value {
        err(
            "FieldError",
            format!(
                "`{}` has no field `{}`",
                self.type_name(ty),
                self.program.symbols.name(name)
            ),
        )
    }

    /// `Shape.Circle` and `Shape.Empty`: a variant constructor or value.
    fn type_member(&self, ty: u16, name: Symbol) -> VmResult<Value> {
        let def = &self.program.types[ty as usize];
        let Some(&(_, variant)) = def.variants.iter().find(|(n, _)| *n == name) else {
            return Err(err(
                "FieldError",
                format!(
                    "`{}` has no variant `{}`",
                    self.type_name(ty),
                    self.program.symbols.name(name)
                ),
            ));
        };
        let vdef = &self.program.types[variant as usize];
        Ok(match vdef.ctor {
            Some(ctor) => Value::Function(self.functions[ctor].clone()),
            None => Value::Struct(Rc::new(Instance {
                ty: variant,
                fields: Vec::new(),
            })),
        })
    }

    // ==== the dispatch loop ================================================

    fn execute(&mut self, entry_depth: usize) -> Result<Value, RuntimeError> {
        'frames: loop {
            let Some(frame) = self.frames.last() else {
                return Ok(Value::None);
            };
            let closure = frame.closure.clone();
            let base = frame.base;
            let mut ip = frame.ip;
            let proto = &*closure.proto;
            let code: &[Op] = &proto.code;

            macro_rules! reg {
                ($r:expr) => {
                    self.stack[base + $r as usize]
                };
            }
            macro_rules! save_ip {
                () => {
                    if let Some(f) = self.frames.last_mut() {
                        f.ip = ip;
                    }
                };
            }
            macro_rules! throw {
                ($error:expr) => {{
                    let error = $error;
                    save_ip!();
                    match self.unwind(error, entry_depth) {
                        Ok(()) => continue 'frames,
                        Err(e) => return Err(e),
                    }
                }};
            }
            macro_rules! check {
                ($result:expr) => {
                    match $result {
                        Ok(v) => v,
                        Err(e) => throw!(e),
                    }
                };
            }
            macro_rules! jump {
                ($offset:expr) => {
                    ip = (ip as isize + $offset as isize) as usize
                };
            }
            macro_rules! arith {
                ($dst:expr, $a:expr, $b:expr, $kind:expr,
                 |$x:ident, $y:ident| $int:expr, |$fx:ident, $fy:ident| $float:expr) => {{
                    let fast = match (&reg!($a), &reg!($b)) {
                        (Value::Int($x), Value::Int($y)) => {
                            let ($x, $y) = (*$x, *$y);
                            $int
                        }
                        (Value::Float($fx), Value::Float($fy)) => {
                            let ($fx, $fy) = (*$fx, *$fy);
                            $float
                        }
                        _ => None,
                    };
                    let value = match fast {
                        Some(v) => v,
                        None => check!(ops::arith(
                            &mut self.ctx,
                            $kind,
                            &self.stack[base + $a as usize],
                            &self.stack[base + $b as usize]
                        )),
                    };
                    reg!($dst) = value;
                }};
            }
            macro_rules! slow_arith {
                ($dst:expr, $a:expr, $b:expr, $kind:expr) => {{
                    let value = check!(ops::arith(
                        &mut self.ctx,
                        $kind,
                        &self.stack[base + $a as usize],
                        &self.stack[base + $b as usize]
                    ));
                    reg!($dst) = value;
                }};
            }
            macro_rules! compare {
                ($dst:expr, $a:expr, $b:expr, $kind:expr, $op:tt) => {{
                    let fast = match (&reg!($a), &reg!($b)) {
                        (Value::Int(x), Value::Int(y)) => Some(x $op y),
                        (Value::Float(x), Value::Float(y)) => Some(x $op y),
                        _ => None,
                    };
                    let value = match fast {
                        Some(b) => Value::Bool(b),
                        None => check!(ops::compare(
                            &mut self.ctx,
                            $kind,
                            &self.stack[base + $a as usize],
                            &self.stack[base + $b as usize]
                        )),
                    };
                    reg!($dst) = value;
                }};
            }

            loop {
                debug_assert!(
                    self.stack.len() >= base + proto.num_regs,
                    "frame `{}` lost its registers before {ip}: {:?}",
                    proto.name,
                    code[ip]
                );
                let op = code[ip];
                ip += 1;
                match op {
                    // ---- loads and moves ----
                    Op::Move { dst, src } => {
                        let value = match &reg!(src) {
                            Value::Int(i) => Value::Int(*i),
                            Value::Float(f) => Value::Float(*f),
                            Value::Bool(b) => Value::Bool(*b),
                            other => other.clone(),
                        };
                        reg!(dst) = value;
                    }
                    Op::LoadConst { dst, k } => reg!(dst) = proto.consts[k as usize].clone(),
                    Op::LoadInt { dst, value } => reg!(dst) = Value::Int(i64::from(value)),
                    Op::LoadNone { dst } => reg!(dst) = Value::None,
                    Op::LoadBool { dst, value } => reg!(dst) = Value::Bool(value),
                    Op::LoadMissing { dst } => reg!(dst) = Value::Missing,
                    Op::GetGlobal { dst, idx } => {
                        let value = self.globals[idx as usize].clone();
                        reg!(dst) = value;
                    }
                    Op::SetGlobal { src, idx } => {
                        let value = reg!(src).clone();
                        self.globals[idx as usize] = value;
                    }
                    Op::GetCapture { dst, idx } => {
                        reg!(dst) = closure.captures[idx as usize].clone();
                    }
                    Op::MakeClosure { dst, proto: index } => {
                        let target = self.program.protos[index as usize].clone();
                        let captures = target
                            .captures
                            .iter()
                            .map(|capture| match *capture {
                                Capture::Local(r) => self.stack[base + r as usize].clone(),
                                Capture::Outer(i) => closure.captures[i as usize].clone(),
                            })
                            .collect();
                        reg!(dst) = Value::Function(Rc::new(Closure {
                            proto: target,
                            captures,
                        }));
                    }

                    // ---- arithmetic ----
                    Op::Add { dst, a, b } => arith!(
                        dst,
                        a,
                        b,
                        Arith::Add,
                        |x, y| x.checked_add(y).map(Value::Int),
                        |x, y| Some(Value::Float(x + y))
                    ),
                    Op::Sub { dst, a, b } => arith!(
                        dst,
                        a,
                        b,
                        Arith::Sub,
                        |x, y| x.checked_sub(y).map(Value::Int),
                        |x, y| Some(Value::Float(x - y))
                    ),
                    Op::Mul { dst, a, b } => arith!(
                        dst,
                        a,
                        b,
                        Arith::Mul,
                        |x, y| x.checked_mul(y).map(Value::Int),
                        |x, y| Some(Value::Float(x * y))
                    ),
                    Op::Div { dst, a, b } => arith!(
                        dst,
                        a,
                        b,
                        Arith::Div,
                        |x, y| (y != 0).then(|| Value::Float(x as f64 / y as f64)),
                        |x, y| (y != 0.0).then(|| Value::Float(x / y))
                    ),
                    Op::FloorDiv { dst, a, b } => slow_arith!(dst, a, b, Arith::FloorDiv),
                    Op::Rem { dst, a, b } => slow_arith!(dst, a, b, Arith::Rem),
                    Op::Pow { dst, a, b } => slow_arith!(dst, a, b, Arith::Pow),
                    Op::BitAnd { dst, a, b } => slow_arith!(dst, a, b, Arith::BitAnd),
                    Op::BitOr { dst, a, b } => slow_arith!(dst, a, b, Arith::BitOr),
                    Op::BitXor { dst, a, b } => slow_arith!(dst, a, b, Arith::BitXor),
                    Op::Shl { dst, a, b } => slow_arith!(dst, a, b, Arith::Shl),
                    Op::Shr { dst, a, b } => slow_arith!(dst, a, b, Arith::Shr),
                    Op::Neg { dst, src } => {
                        let value = match &reg!(src) {
                            Value::Float(x) => Value::Float(-x),
                            other => check!(ops::negate(&mut self.ctx, &other.clone())),
                        };
                        reg!(dst) = value;
                    }
                    Op::Pos { dst, src } => {
                        let value = match &reg!(src) {
                            v @ (Value::Int(_) | Value::Float(_) | Value::Tensor(_)) => v.clone(),
                            other => throw!(type_error(format!(
                                "unary `+` needs a number, not {}",
                                other.type_name()
                            ))),
                        };
                        reg!(dst) = value;
                    }
                    Op::BitNot { dst, src } => {
                        let value = match &reg!(src) {
                            Value::Int(x) => Value::Int(!x),
                            other => throw!(type_error(format!(
                                "`~` needs an integer, not {}",
                                other.type_name()
                            ))),
                        };
                        reg!(dst) = value;
                    }
                    Op::Not { dst, src } => {
                        let value = !reg!(src).truthy();
                        reg!(dst) = Value::Bool(value);
                    }

                    // ---- comparison ----
                    Op::Eq { dst, a, b } => {
                        let equal = match (&reg!(a), &reg!(b)) {
                            (Value::Int(x), Value::Int(y)) => x == y,
                            (x, y) => ops::equals(x, y),
                        };
                        reg!(dst) = Value::Bool(equal);
                    }
                    Op::NotEq { dst, a, b } => {
                        let equal = match (&reg!(a), &reg!(b)) {
                            (Value::Int(x), Value::Int(y)) => x == y,
                            (x, y) => ops::equals(x, y),
                        };
                        reg!(dst) = Value::Bool(!equal);
                    }
                    Op::Lt { dst, a, b } => compare!(dst, a, b, Cmp::Lt, <),
                    Op::LtEq { dst, a, b } => compare!(dst, a, b, Cmp::LtEq, <=),
                    Op::Gt { dst, a, b } => compare!(dst, a, b, Cmp::Gt, >),
                    Op::GtEq { dst, a, b } => compare!(dst, a, b, Cmp::GtEq, >=),
                    Op::Contains {
                        dst,
                        item,
                        container,
                    } => {
                        let found = check!(ops::contains(&reg!(item), &reg!(container)));
                        reg!(dst) = Value::Bool(found);
                    }

                    // ---- tensors and devices ----
                    Op::MatMul { dst, a, b } => {
                        let value = check!(ops::matmul(
                            &mut self.ctx,
                            &self.stack[base + a as usize],
                            &self.stack[base + b as usize]
                        ));
                        reg!(dst) = value;
                    }
                    Op::Place { dst, src, target } => {
                        let value = check!(ops::place(
                            &mut self.ctx,
                            &self.stack[base + src as usize],
                            &self.stack[base + target as usize]
                        ));
                        reg!(dst) = value;
                    }
                    Op::PushDevice { src } => {
                        let device = match &reg!(src) {
                            Value::Device(d) => *d,
                            Value::DeviceGroup(group) => group[0],
                            Value::Region => self.ctx.current_device(),
                            other => throw!(type_error(format!(
                                "`on` needs a device such as `gpu[0]` or a region, not {}",
                                other.type_name()
                            ))),
                        };
                        self.ctx.devices.push(device);
                    }
                    Op::PopDevice => {
                        self.ctx.devices.pop();
                    }

                    // ---- control flow ----
                    Op::Jump { offset } => jump!(offset),
                    Op::JumpIfFalse { cond, offset } => {
                        let truthy = match &reg!(cond) {
                            Value::Bool(b) => *b,
                            other => other.truthy(),
                        };
                        if !truthy {
                            jump!(offset);
                        }
                    }
                    Op::JumpIfTrue { cond, offset } => {
                        let truthy = match &reg!(cond) {
                            Value::Bool(b) => *b,
                            other => other.truthy(),
                        };
                        if truthy {
                            jump!(offset);
                        }
                    }
                    Op::JumpIfNotNone { src, offset } => {
                        if !matches!(reg!(src), Value::None) {
                            jump!(offset);
                        }
                    }
                    Op::JumpIfNotMissing { src, offset } => {
                        if !matches!(reg!(src), Value::Missing) {
                            jump!(offset);
                        }
                    }
                    Op::ForPrep { base: b, offset } => {
                        let at = base + b as usize;
                        let (start, stop, step) = match &self.stack[at] {
                            Value::Range(r) => (r.start, r.stop, r.step),
                            other => throw!(type_error(format!(
                                "cannot count over {}",
                                other.type_name()
                            ))),
                        };
                        self.stack[at] = Value::Int(start);
                        self.stack[at + 1] = Value::Int(stop);
                        self.stack[at + 2] = Value::Int(step);
                        if (step > 0 && start < stop) || (step < 0 && start > stop) {
                            self.stack[at + 3] = Value::Int(start);
                        } else {
                            jump!(offset);
                        }
                    }
                    Op::ForLoop { base: b, offset } => {
                        let at = base + b as usize;
                        let (Value::Int(i), Value::Int(stop), Value::Int(step)) =
                            (&self.stack[at], &self.stack[at + 1], &self.stack[at + 2])
                        else {
                            throw!(err("InternalError", "corrupted loop counter"));
                        };
                        let (i, stop, step) = (*i, *stop, *step);
                        if let Some(next) = i.checked_add(step) {
                            if (step > 0 && next < stop) || (step < 0 && next > stop) {
                                self.stack[at] = Value::Int(next);
                                self.stack[at + 3] = Value::Int(next);
                                jump!(offset);
                            }
                        }
                    }
                    Op::IterInit { dst, src } => {
                        let iter = check!(ops::make_iter(&reg!(src)));
                        reg!(dst) = iter;
                    }
                    Op::IterNext { iter, dst } => {
                        let next = match &reg!(iter) {
                            Value::Iter(it) => it.borrow_mut().next_value(),
                            other => throw!(type_error(format!(
                                "cannot loop over {}",
                                other.type_name()
                            ))),
                        };
                        if let Some(value) = next {
                            reg!(dst) = value;
                            ip += 1;
                        }
                    }

                    // ---- calls ----
                    Op::Call { func, argc } => {
                        let at = base + func as usize;
                        match &self.stack[at] {
                            Value::Function(callee) => {
                                let callee = callee.clone();
                                save_ip!();
                                check!(self.push_frame(callee, at, at + 1, argc as usize));
                                continue 'frames;
                            }
                            Value::Native(id) => {
                                let id = *id;
                                let result = check!(builtins::call(
                                    id,
                                    &mut self.ctx,
                                    &self.stack[at + 1..at + 1 + argc as usize]
                                ));
                                self.stack[at] = result;
                            }
                            Value::Type(t) => {
                                let name = self.type_name(*t);
                                throw!(type_error(format!(
                                    "`{name}` is an enum; create a value with one of its variants, as in `{name}.Variant(...)`"
                                )));
                            }
                            other => {
                                throw!(type_error(format!("{} is not callable", other.type_name())))
                            }
                        }
                    }
                    Op::CallMethod {
                        base: b,
                        argc,
                        name,
                    } => {
                        let result_at = base + b as usize;
                        let receiver = result_at + 1;
                        let symbol = proto.names[name as usize];
                        let ty = match &self.stack[receiver] {
                            Value::Struct(inst) => Some(inst.ty),
                            Value::Object(obj) => Some(obj.borrow().ty),
                            _ => None,
                        };
                        if let Some(ty) = ty {
                            match self.methods.get(&(ty, symbol)).cloned() {
                                Some(method) => {
                                    save_ip!();
                                    check!(self.push_frame(
                                        method,
                                        result_at,
                                        receiver,
                                        argc as usize + 1
                                    ));
                                    continue 'frames;
                                }
                                None => throw!(err(
                                    "FieldError",
                                    format!(
                                        "`{}` has no method `{}`",
                                        self.type_name(ty),
                                        self.program.symbols.name(symbol)
                                    )
                                )),
                            }
                        }
                        let result = check!(builtins::call_method(
                            &mut self.ctx,
                            self.program.symbols.name(symbol),
                            &self.stack[receiver..receiver + 1 + argc as usize]
                        ));
                        self.stack[result_at] = result;
                    }
                    Op::Return { src } => {
                        let value = reg!(src).clone();
                        if let Some(value) = self.finish_frame(value, entry_depth) {
                            return Ok(value);
                        }
                        continue 'frames;
                    }
                    Op::ReturnNone => {
                        if let Some(value) = self.finish_frame(Value::None, entry_depth) {
                            return Ok(value);
                        }
                        continue 'frames;
                    }

                    // ---- collections and objects ----
                    Op::NewList { dst, start, count } => {
                        let from = base + start as usize;
                        let items = self.stack[from..from + count as usize].to_vec();
                        reg!(dst) = Value::list(items);
                    }
                    Op::NewTuple { dst, start, count } => {
                        let from = base + start as usize;
                        let items = self.stack[from..from + count as usize].to_vec();
                        reg!(dst) = Value::tuple(items);
                    }
                    Op::NewMap { dst } => reg!(dst) = builtins::new_map(),
                    Op::ListPush { list, src } => {
                        let value = reg!(src).clone();
                        match &reg!(list) {
                            Value::List(items) => items.borrow_mut().push(value),
                            other => throw!(type_error(format!(
                                "cannot append to {}",
                                other.type_name()
                            ))),
                        }
                    }
                    Op::MakeRange {
                        dst,
                        base: b,
                        inclusive,
                    } => {
                        let at = base + b as usize;
                        let range = check!(ops::make_range(
                            &self.stack[at],
                            &self.stack[at + 1],
                            &self.stack[at + 2],
                            inclusive
                        ));
                        reg!(dst) = range;
                    }
                    Op::MakeSlice { dst, base: b } => {
                        let at = base + b as usize;
                        let slice = check!(ops::slice_value(
                            &self.stack[at],
                            &self.stack[at + 1],
                            &self.stack[at + 2]
                        ));
                        reg!(dst) = slice;
                    }
                    Op::GetIndex { dst, obj, index } => {
                        let fast = match (&reg!(obj), &reg!(index)) {
                            (Value::List(items), Value::Int(i)) => {
                                let items = items.borrow();
                                let n = items.len() as i64;
                                let i = if *i < 0 { i + n } else { *i };
                                (0..n).contains(&i).then(|| items[i as usize].clone())
                            }
                            _ => None,
                        };
                        let value = match fast {
                            Some(v) => v,
                            None => check!(ops::get_index(
                                &mut self.ctx,
                                &self.stack[base + obj as usize],
                                &self.stack[base + index as usize]
                            )),
                        };
                        reg!(dst) = value;
                    }
                    Op::TakeIndex { dst, obj, index } => {
                        let taken = match (&reg!(obj), &reg!(index)) {
                            (Value::List(items), Value::Int(i)) => {
                                let mut items = items.borrow_mut();
                                let n = items.len() as i64;
                                let i = if *i < 0 { i + n } else { *i };
                                (0..n)
                                    .contains(&i)
                                    .then(|| std::mem::replace(&mut items[i as usize], Value::None))
                            }
                            _ => None,
                        };
                        let value = match taken {
                            Some(v) => v,
                            None => check!(ops::get_index(
                                &mut self.ctx,
                                &self.stack[base + obj as usize],
                                &self.stack[base + index as usize]
                            )),
                        };
                        reg!(dst) = value;
                    }
                    Op::SetIndex { obj, index, src } => {
                        let key = reg!(index).clone();
                        let value = reg!(src).clone();
                        check!(ops::set_index(&mut reg!(obj), &key, value));
                    }
                    Op::GetField { dst, obj, name } => {
                        let symbol = proto.names[name as usize];
                        let result: VmResult<Value> = match &reg!(obj) {
                            Value::Struct(inst) => match self.field_slot(inst.ty, symbol) {
                                Some(i) => Ok(inst.fields[i].clone()),
                                None => Err(self.no_field(inst.ty, symbol)),
                            },
                            Value::Object(o) => {
                                let o = o.borrow();
                                match self.field_slot(o.ty, symbol) {
                                    Some(i) => Ok(o.fields[i].clone()),
                                    None => Err(self.no_field(o.ty, symbol)),
                                }
                            }
                            Value::Type(t) => self.type_member(*t, symbol),
                            other => ops::get_attr(other, self.program.symbols.name(symbol)),
                        };
                        let value = check!(result);
                        reg!(dst) = value;
                    }
                    Op::SetField { obj, name, src } => {
                        let symbol = proto.names[name as usize];
                        let value = reg!(src).clone();
                        let slot = match &reg!(obj) {
                            Value::Struct(inst) => {
                                Some((inst.ty, self.field_slot(inst.ty, symbol)))
                            }
                            Value::Object(o) => {
                                let ty = o.borrow().ty;
                                Some((ty, self.field_slot(ty, symbol)))
                            }
                            _ => None,
                        };
                        let result: VmResult<()> = match slot {
                            Some((ty, None)) => Err(self.no_field(ty, symbol)),
                            Some((_, Some(i))) => {
                                match &mut self.stack[base + obj as usize] {
                                    // Copy on write: a struct shared with
                                    // another variable is cloned first.
                                    Value::Struct(inst) => Rc::make_mut(inst).fields[i] = value,
                                    Value::Object(o) => o.borrow_mut().fields[i] = value,
                                    _ => {}
                                }
                                Ok(())
                            }
                            None => ops::set_attr(
                                &mut self.stack[base + obj as usize],
                                self.program.symbols.name(symbol),
                                value,
                            ),
                        };
                        check!(result);
                    }
                    Op::GetSlot { dst, obj, slot } => {
                        let value = match &reg!(obj) {
                            Value::Struct(inst) => inst.fields.get(slot as usize).cloned(),
                            Value::Object(o) => o.borrow().fields.get(slot as usize).cloned(),
                            _ => None,
                        };
                        match value {
                            Some(v) => reg!(dst) = v,
                            None => throw!(type_error("pattern field out of range")),
                        }
                    }
                    Op::Len { dst, src } => {
                        let n = check!(ops::len(&reg!(src)));
                        reg!(dst) = Value::Int(n);
                    }
                    Op::MakeInstance { dst, ty } => {
                        let def = &self.program.types[ty as usize];
                        let fields = self.stack[base..base + def.fields.len()].to_vec();
                        let instance = Instance { ty, fields };
                        reg!(dst) = if def.kind == TypeKind::Class {
                            Value::Object(Rc::new(RefCell::new(instance)))
                        } else {
                            Value::Struct(Rc::new(instance))
                        };
                    }
                    Op::Concat { dst, start, count } => {
                        let from = base + start as usize;
                        let mut text = String::new();
                        for value in &self.stack[from..from + count as usize] {
                            match value {
                                Value::Str(s) => text.push_str(s),
                                other => text.push_str(&ops::display(&self.program, other)),
                            }
                        }
                        reg!(dst) = Value::str(text);
                    }

                    // ---- patterns and failures ----
                    Op::IsType { dst, src, name } => {
                        let type_name = self.program.symbols.name(proto.names[name as usize]);
                        let matches = ops::is_type(&self.program, &reg!(src), type_name);
                        reg!(dst) = Value::Bool(matches);
                    }
                    Op::Fail { src } => {
                        let value = reg!(src).clone();
                        throw!(value);
                    }
                    Op::TryBegin { err: e, offset } => {
                        let target = (ip as isize + offset as isize) as usize;
                        self.handlers.push(Handler {
                            frame_depth: self.frames.len(),
                            target,
                            err_slot: base + e as usize,
                            device_depth: self.ctx.devices.len(),
                        });
                    }
                    Op::TryEnd => {
                        self.handlers.pop();
                    }

                    // ---- tasks ----
                    // The interpreter runs a spawned call to completion
                    // immediately; `await` then just takes its result.
                    Op::Spawn { dst, src } => {
                        let value = reg!(src).clone();
                        reg!(dst) = Value::Task(Rc::new(value));
                    }
                    Op::Await { dst, src } => {
                        let value = match &reg!(src) {
                            Value::Task(result) => (**result).clone(),
                            other => other.clone(),
                        };
                        reg!(dst) = value;
                    }
                }
            }
        }
    }
}

/// Drops what dead registers refer to. Registers holding scalars are left
/// alone: they own nothing, and skipping them keeps returns cheap.
fn release(registers: &mut [Value]) {
    for register in registers {
        if register.owns_heap() {
            *register = Value::None;
        }
    }
}

/// Matches a request path against a route's segments, returning the text of
/// each `{param}`.
fn match_path(segments: &[PathSegment], path: &str) -> Option<HashMap<String, String>> {
    let mut rest = path;
    let mut captured = HashMap::new();
    for (i, segment) in segments.iter().enumerate() {
        match segment {
            PathSegment::Literal(text) => rest = rest.strip_prefix(text.as_str())?,
            PathSegment::Param(name) => {
                let end = match segments.get(i + 1) {
                    Some(PathSegment::Literal(next)) if !next.starts_with('/') => {
                        rest.find(next.as_str())?
                    }
                    _ => rest.find('/').unwrap_or(rest.len()),
                };
                if end == 0 {
                    return None;
                }
                captured.insert(name.name.clone(), rest[..end].to_string());
                rest = &rest[end..];
            }
        }
    }
    rest.is_empty().then_some(captured)
}

fn convert_path_param(text: &str, ty: Option<&str>) -> Option<Value> {
    match ty {
        Some("int" | "i8" | "i16" | "i32" | "i64") => text.parse().ok().map(Value::Int),
        Some("u8" | "u16" | "u32" | "u64") => {
            text.parse::<i64>().ok().filter(|v| *v >= 0).map(Value::Int)
        }
        Some("float" | "f32" | "f64") => text.parse().ok().map(Value::Float),
        Some("bool") => match text {
            "true" => Some(Value::Bool(true)),
            "false" => Some(Value::Bool(false)),
            _ => None,
        },
        _ => Some(Value::str(text)),
    }
}
