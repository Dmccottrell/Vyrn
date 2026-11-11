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
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::io::Write;
use std::rc::Rc;

use super::builtins;
use super::fast;
use super::op::Op;
use super::ops::{self, err, type_error, Arith, Cmp, Ctx, VmResult};
use super::program::{Capture, GlobalInit, ParamSource, Program, RouteParam, TypeKind};
use super::task::{TaskHandle, TaskId, ROOT_TASK};
use super::tensor::{ReferenceBackend, TensorBackend};
use super::value::{Closure, Instance, Symbol, Value};
use super::web;
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
    /// The loaded file the function is in; see `Proto::file`.
    pub file: u32,
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
            if crate::prelude::is_prelude_offset(line.file << 24) {
                continue;
            }
            write!(f, "\n  in {} at line {}", line.function, line.line)?;
        }
        Ok(())
    }
}

impl std::error::Error for RuntimeError {}

fn err_value(kind: &'static str, message: String) -> Value {
    err(kind, message)
}

/// A number or boolean as the word native code takes, if it has the type.
fn scalar_word(want: super::native_abi::NTy, v: &Value) -> Option<u64> {
    use super::native_abi::NTy;
    Some(match (want, v) {
        (NTy::Int, Value::Int(x)) => *x as u64,
        (NTy::Float, Value::Float(x)) => x.to_bits(),
        (NTy::Bool, Value::Bool(x)) => u64::from(*x),
        _ => return None,
    })
}

/// The value a word from native code stands for.
fn word_value(t: super::native_abi::NTy, w: u64) -> Value {
    use super::native_abi::NTy;
    match t {
        NTy::Int => Value::Int(w as i64),
        NTy::Float => Value::Float(f64::from_bits(w)),
        NTy::Bool => Value::Bool(w != 0),
        _ => Value::None,
    }
}

impl RuntimeError {
    /// What usually fixes an error of this kind, for the report.
    pub fn hint(&self) -> Option<&'static str> {
        Some(match self.kind.as_str() {
            "IndexError" => "positions count from 0, so the last item of `xs` is `xs[len(xs) - 1]` (or `xs[-1]`)",
            "KeyError" => "check with `key in m` first, or use `m.get(key, default)`",
            "DivisionByZero" => "check that the number you divide by is not 0",
            "NotFound" => "check the path; `fs.exists(path)` (with `use vyrn.fs`) tells whether it is there",
            "PermissionDenied" => "this program is not allowed to use that file",
            "ValueError" if self.message.contains("is not an integer") || self.message.contains("is not a number") => {
                "convert text to a number only after checking it, as in `if s.isdigit(): n = int(s)`"
            }
            "FieldError" if self.message.contains("none") => {
                "the value is `none` here; check it with `if x != none:` or use `x?.field`"
            }
            "TypeError" if self.message.contains("none") => {
                "a value is `none` here; check it with `if x != none:` or give a default with `x ?? default`"
            }
            "Overflow" => "the result is too large for an `int`; use a `float` if precision can be lost",
            "AssertionError" => "the condition given to `assert` was false",
            _ => return None,
        })
    }
}

pub struct Vm {
    program: Rc<Program>,
    globals: Vec<Value>,
    stack: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    /// Method lookup with inheritance already resolved: (type, name).
    methods: HashMap<(u16, Symbol), Rc<Closure>>,
    /// Methods of lists, strings and maps written in Vyrn (the Prelude's
    /// `__list_map` and the like), by receiver kind and name.
    builtin_methods: HashMap<(u8, Symbol), Rc<Closure>>,
    /// A capture-free closure for every function, shared by all calls.
    functions: Vec<Rc<Closure>>,
    ctx: Ctx,
    initialized: bool,
    /// The task whose state is in `stack`, `frames` and `handlers`.
    current: TaskId,
    /// The current task's handle; `None` for the task the host called into.
    current_handle: Option<Rc<TaskHandle>>,
    /// Frames below this depth belong to whoever called into the current
    /// task, so a return or failure below it ends the task.
    entry_depth: usize,
    /// Tasks that are not running: ready to resume, or waiting.
    suspended: HashMap<TaskId, Suspended>,
    /// Tasks ready to resume, in order.
    ready: VecDeque<TaskId>,
    /// The last failure a handler caught, with where it was first raised:
    /// if a `catch` passes it on and nothing else catches it, the error
    /// names that place rather than the `catch`.
    caught: Option<(Value, Vec<TraceLine>)>,
}

/// The state of a task that is not running.
struct Suspended {
    stack: Vec<Value>,
    frames: Vec<Frame>,
    handlers: Vec<Handler>,
    devices: Vec<super::tensor::Device>,
    entry_depth: usize,
    handle: Option<Rc<TaskHandle>>,
}

/// A route handler and the arguments for one request.
type RouteCall = (Rc<Closure>, Vec<Value>);

/// How `await` came out.
enum Awaited {
    Ready(Value),
    Failed(Value),
    /// The current task was registered as a waiter and must suspend.
    Blocked,
}

/// What became of a failure.
enum Unwound {
    /// A handler in the current task took it.
    Handled,
    /// Nothing in the current task handles it.
    Unhandled(Value),
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
                    bound: None,
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
        let mut builtin_methods = HashMap::new();
        for (i, global) in program.globals.iter().enumerate() {
            let GlobalInit::Function(p) = global.init else {
                continue;
            };
            let _ = i;
            for (prefix, kind) in [("__list_", 0u8), ("__str_", 1), ("__map_", 2)] {
                if let Some(method) = global.name.strip_prefix(prefix) {
                    if let Some(symbol) = program.symbols.get(method) {
                        builtin_methods.insert((kind, symbol), functions[p].clone());
                    }
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
            timers: Vec::new(),
            wakeups: Vec::new(),
            next_task: super::task::ROOT_TASK,
            servers: Vec::new(),
            providers: HashMap::new(),
            block_on: None,
            hosted: false,
            cancels: Vec::new(),
        };
        Vm {
            program,
            globals,
            stack: Vec::with_capacity(1024),
            frames: Vec::new(),
            handlers: Vec::new(),
            caught: None,
            methods,
            builtin_methods,
            functions,
            ctx,
            initialized: false,
            current: ROOT_TASK,
            current_handle: None,
            entry_depth: 1,
            suspended: HashMap::new(),
            ready: VecDeque::new(),
        }
    }

    /// Marks the program as hosted: the host delivers HTTP requests through
    /// [`Vm::handle_request`], and `web.serve` returns at once.
    pub fn set_hosted(&mut self, hosted: bool) {
        self.ctx.hosted = hosted;
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
        let result = self.call_value(main, args);
        // The program ends with `main`; tasks still running are cancelled.
        self.cancel_tasks();
        result
    }

    /// Drops every task other than the current one.
    pub fn cancel_tasks(&mut self) {
        self.suspended.clear();
        self.ready.clear();
        self.ctx.timers.clear();
        self.ctx.wakeups.clear();
        self.ctx.servers.clear();
    }

    /// Runs tasks left by earlier calls until none can make progress, for
    /// hosts that keep a program alive between requests.
    pub fn run_pending(&mut self) -> Result<(), RuntimeError> {
        loop {
            self.apply_cancels();
            self.ready.extend(self.ctx.wakeups.drain(..));
            self.fire_timers();
            if self.ready.is_empty() {
                if let Some(server) = self.ctx.servers.first() {
                    // Serve until that server stops.
                    let handle = server.handle.clone();
                    handle.waiters.borrow_mut().push(self.current);
                    let root = self.current;
                    let saved = self.save_current();
                    self.suspended.insert(root, saved);
                    self.resume_next()?;
                    if self.current != root {
                        self.execute(root)?;
                    }
                    continue;
                }
                let Some(when) = self.ctx.timers.iter().map(|(t, _)| *t).min() else {
                    // Tasks still suspended wait on something only the host
                    // can provide; they stay until then.
                    return Ok(());
                };
                let now = std::time::Instant::now();
                if when > now {
                    std::thread::sleep(when - now);
                }
                continue;
            }
            // Queue the host's task behind the others: by the time it is
            // resumed, every task ahead of it has had a turn.
            let root = self.current;
            let saved = self.save_current();
            self.suspended.insert(root, saved);
            self.ready.push_back(root);
            self.resume_next()?;
            if self.current != root {
                self.execute(root)?;
            }
        }
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
        match self.resolve_route(method, path, &HashMap::new(), providers, None)? {
            Some((handler, args)) => self.call_value(Value::Function(handler), args).map(Some),
            None => Ok(None),
        }
    }

    /// Finds the route for a request and builds its arguments. Path
    /// parameters are converted to their declared types. Every other
    /// parameter comes from the query string, then `body` from the request
    /// body, then `providers`.
    fn resolve_route(
        &self,
        method: HttpMethod,
        path: &str,
        query: &HashMap<String, String>,
        providers: &HashMap<String, Value>,
        body: Option<&Value>,
    ) -> Result<Option<RouteCall>, RuntimeError> {
        for route in &self.program.routes {
            if route.method != method {
                continue;
            }
            let Some(captured) = match_path(&route.segments, path) else {
                continue;
            };
            let convert = |text: &str, param: &RouteParam, what: &str| {
                convert_path_param(text, param.ty.as_deref()).ok_or_else(|| {
                    RuntimeError::new(
                        "BadRequest",
                        format!(
                            "{what} `{}` must be {}, got {text:?}",
                            param.name,
                            param.ty.as_deref().unwrap_or("text")
                        ),
                    )
                })
            };
            let mut args = Vec::with_capacity(route.params.len());
            for param in &route.params {
                let value = match param.source {
                    ParamSource::Path => {
                        let text = captured.get(&param.name).cloned().unwrap_or_default();
                        convert(&text, param, "path parameter")?
                    }
                    ParamSource::Provided => {
                        if let Some(text) = query.get(&param.name) {
                            convert(text, param, "query parameter")?
                        } else if let (Some(body), "body") = (body, param.name.as_str()) {
                            self.body_as(body, param.ty.as_deref())?
                        } else {
                            providers.get(&param.name).cloned().ok_or_else(|| {
                                RuntimeError::new(
                                    "MissingProvider",
                                    format!(
                                        "the route needs a value for `{}`; pass it as a query parameter or with `web.provide`",
                                        param.name
                                    ),
                                )
                            })?
                        }
                    }
                };
                args.push(value);
            }
            return Ok(Some((self.functions[route.proto].clone(), args)));
        }
        Ok(None)
    }

    /// A request body as the route declared it: a JSON object becomes the
    /// struct or class named by the parameter's type, field by field.
    fn body_as(&self, body: &Value, ty: Option<&str>) -> Result<Value, RuntimeError> {
        let Some(name) = ty else {
            return Ok(body.clone());
        };
        let found = self.program.types.iter().position(|t| {
            matches!(t.kind, TypeKind::Struct | TypeKind::Class)
                && self.program.symbols.name(t.name) == name
        });
        let (Some(index), Value::Map(map)) = (found, body) else {
            return Ok(body.clone());
        };
        let def = &self.program.types[index];
        let map = map.borrow();
        let mut fields = Vec::with_capacity(def.fields.len());
        for &field in &def.fields {
            let key = super::value::Key::Str(Rc::new(self.program.symbols.name(field).to_string()));
            match map.get(&key) {
                Some(v) => fields.push(v.clone()),
                None => {
                    return Err(RuntimeError::new(
                        "BadRequest",
                        format!(
                            "the body is missing field `{}` of `{name}`",
                            self.program.symbols.name(field)
                        ),
                    ))
                }
            }
        }
        let instance = Instance {
            ty: index as u16,
            fields,
        };
        Ok(if def.kind == TypeKind::Class {
            Value::Object(Rc::new(RefCell::new(instance)))
        } else {
            Value::Struct(Rc::new(instance))
        })
    }

    /// Answers one HTTP request with the program's routes, without a
    /// socket: for hosts that receive requests themselves, such as edge
    /// platforms running Vyrn as WebAssembly. The handler runs to
    /// completion; parameters and responses work as with `web.serve`.
    pub fn handle_request(
        &mut self,
        method: &str,
        path: &str,
        query: &str,
        body: &[u8],
        content_type: Option<&str>,
    ) -> web::Response {
        if let Err(e) = self.init() {
            return web::Response::error(500, &e.kind, &e.message);
        }
        let Some(m) = HttpMethod::from_ident(&method.to_ascii_uppercase()) else {
            return web::Response::error(
                405,
                "MethodNotAllowed",
                &format!("unknown method {method}"),
            );
        };
        let body = match request_body(body, content_type) {
            Ok(b) => b,
            Err(response) => return response,
        };
        let query = web::parse_query(query);
        let path = web::percent_decode(path);
        let providers = self.ctx.providers.clone();
        let resolved = match self.resolve_route(m, &path, &query, &providers, body.as_ref()) {
            Ok(r) => r,
            Err(e) => {
                return web::Response::error(
                    web::status_for(&e.kind),
                    web::short_kind(&e.kind),
                    &e.message,
                )
            }
        };
        let Some((handler, args)) = resolved else {
            return web::Response::error(404, "NotFound", &format!("no route for {method} {path}"));
        };
        match self.call_value(Value::Function(handler), args) {
            Ok(v) => self.response_for(Ok(v)),
            Err(e) => self.response_for(Err(e.value)),
        }
    }

    /// Calls a function's machine code when the arguments have exactly the
    /// declared types; `None` means the bytecode must run instead.
    fn call_native(
        &mut self,
        native: &super::native_abi::NativeFn,
        args_at: usize,
        argc: usize,
    ) -> Option<VmResult<Value>> {
        use super::native_abi::{ErrState, NTy};
        if argc != native.params.len() {
            return None;
        }
        // Each argument as words: one for a number or a list, one per
        // field for a struct.
        let mut words = [0u64; 64];
        let mut n = 0usize;
        let mut list_words: [(usize, NTy); 16] = [(0, NTy::Unit); 16];
        let mut lists = 0usize;
        for (i, want) in native.params.iter().enumerate() {
            let arg = &self.stack[args_at + i];
            if let NTy::Struct(ty) = want {
                let Value::Struct(inst) = arg else {
                    return None;
                };
                let fields = native.structs.get(ty)?;
                if inst.ty != *ty || inst.fields.len() != fields.len() || n + fields.len() > 64 {
                    return None;
                }
                for (f, ft) in inst.fields.iter().zip(fields) {
                    words[n] = scalar_word(*ft, f)?;
                    n += 1;
                }
                continue;
            }
            if n >= 64 {
                return None;
            }
            words[n] = match (want, arg) {
                (NTy::ListInt, Value::List(items))
                    if items.borrow().iter().all(|v| matches!(v, Value::Int(_))) =>
                {
                    Rc::as_ptr(items) as u64
                }
                (NTy::ListFloat, Value::List(items))
                    if items.borrow().iter().all(|v| matches!(v, Value::Float(_))) =>
                {
                    Rc::as_ptr(items) as u64
                }
                (NTy::Str, Value::Str(text)) => Rc::as_ptr(text) as u64,
                _ => scalar_word(*want, arg)?,
            };
            if want.is_list() {
                if lists == 16 {
                    return None;
                }
                list_words[lists] = (n, *want);
                lists += 1;
            }
            n += 1;
        }
        // One list given as both `[int]` and `[float]` (possible only while
        // it is empty) could end up holding both: leave that to the
        // interpreter.
        for (i, (wi, a)) in list_words[..lists].iter().enumerate() {
            for (wj, b) in &list_words[i + 1..lists] {
                if a != b && words[*wi] == words[*wj] {
                    return None;
                }
            }
        }
        let mut arena = super::native_abi::Arena::default();
        let mut err = ErrState {
            depth: self.frames.len() as u32,
            arena: &mut arena,
            ctx: &mut self.ctx as *mut super::ops::Ctx as *mut std::ffi::c_void,
            ..ErrState::default()
        };
        // SAFETY: the entry was compiled for exactly these parameter types,
        // and `words` holds at least as many words as it reads.
        let word = unsafe { (native.entry)(words.as_ptr(), &mut err) };
        if err.code == super::native_abi::ERR_THROWN {
            if let Some(thrown) = err.thrown.take() {
                return Some(Err(thrown));
            }
        }
        if err.code != 0 {
            let (kind, message) = err.describe(&native.name);
            return Some(Err(err_value(kind, message)));
        }
        Some(Ok(match native.ret {
            NTy::Int => Value::Int(word as i64),
            NTy::Float => Value::Float(f64::from_bits(word)),
            NTy::Bool => Value::Bool(word != 0),
            NTy::Unit => Value::None,
            NTy::Str => {
                let text = word as *const String;
                // SAFETY: native code returns strings that live in an
                // `Rc`: its arena, a literal it keeps, or an argument.
                unsafe {
                    Rc::increment_strong_count(text);
                    Value::Str(Rc::from_raw(text))
                }
            }
            NTy::Struct(ty) => {
                // Always known: the tier only returns structs it describes.
                let fields = native.structs.get(&ty).cloned().unwrap_or_default();
                let values = fields
                    .iter()
                    .zip(err.ret.iter())
                    .map(|(t, w)| word_value(*t, *w))
                    .collect();
                Value::Struct(Rc::new(super::value::Instance { ty, fields: values }))
            }
            NTy::ListInt | NTy::ListFloat => {
                let list = word as *const RefCell<Vec<Value>>;
                // SAFETY: native code returns lists that live in an `Rc`:
                // its arena, or an argument the VM still holds. Taking a
                // reference of our own keeps it once the arena is dropped.
                unsafe {
                    Rc::increment_strong_count(list);
                    Value::List(Rc::from_raw(list))
                }
            }
        }))
    }

    /// A Vyrn-written method of a list, string or map (from the Prelude).
    fn builtin_method(&self, receiver: &Value, symbol: Symbol) -> Option<Rc<Closure>> {
        let kind = match receiver {
            Value::List(_) => 0,
            Value::Str(_) => 1,
            Value::Map(_) => 2,
            _ => return None,
        };
        self.builtin_methods.get(&(kind, symbol)).cloned()
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
        let saved_entry = self.entry_depth;
        self.entry_depth = depth + 1;
        let root = self.current;
        let result = match callee {
            Value::Function(closure) => match self.push_frame(closure, slot, slot + 1, argc) {
                Ok(()) => self.execute(root),
                Err(e) => Err(self.runtime_error(e)),
            },
            Value::Native(id) => {
                let result = builtins::call(id, &mut self.ctx, &self.stack[slot + 1..])
                    .map_err(|e| self.runtime_error(e));
                // Nothing at host level can wait; `run_pending` drives a
                // server started this way.
                self.ctx.block_on = None;
                result
            }
            other => Err(RuntimeError::new(
                "TypeError",
                format!("{} is not callable", other.type_name()),
            )),
        };
        self.entry_depth = saved_entry;
        self.frames.truncate(depth);
        self.handlers.truncate(handlers);
        self.ctx.devices.truncate(devices);
        self.stack.truncate(slot);
        result
    }

    // ==== tasks ============================================================

    /// Takes the current task's state out of the VM.
    fn save_current(&mut self) -> Suspended {
        Suspended {
            stack: std::mem::take(&mut self.stack),
            frames: std::mem::take(&mut self.frames),
            handlers: std::mem::take(&mut self.handlers),
            devices: std::mem::take(&mut self.ctx.devices),
            entry_depth: self.entry_depth,
            handle: self.current_handle.take(),
        }
    }

    fn load(&mut self, id: TaskId, state: Suspended) {
        self.stack = state.stack;
        self.frames = state.frames;
        self.handlers = state.handlers;
        self.ctx.devices = state.devices;
        self.entry_depth = state.entry_depth;
        self.current_handle = state.handle;
        self.current = id;
    }

    /// Calls `closure`: in the current task, or, for an `async fn`, as a new
    /// task. A new task starts at once and runs until it first waits; the
    /// caller then resumes with the task's handle in `ret`.
    #[inline]
    fn enter(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        if closure.proto.is_async || closure.proto.kernel.is_some() {
            return self.enter_special(closure, ret, base, argc);
        }
        self.push_frame(closure, ret, base, argc)
    }

    /// Calls an `async fn` (as a task) or a `kernel fn` (on tensors, element
    /// by element, when any argument is a tensor).
    #[cold]
    #[inline(never)]
    fn enter_special(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        if let Some(kernel) = closure.proto.kernel.clone() {
            let args = &self.stack[base..base + argc];
            if args.iter().any(|a| matches!(a, Value::Tensor(_))) {
                let value = self.run_kernel(&kernel, &closure.proto.name, base, argc)?;
                self.stack[ret] = value;
                return Ok(());
            }
        }
        if closure.proto.is_async {
            return self.start_task(closure, ret, base, argc);
        }
        self.push_frame(closure, ret, base, argc)
    }

    /// Applies a kernel to tensor arguments (and number arguments, the same
    /// for every element), on the tensors' device.
    fn run_kernel(
        &mut self,
        kernel: &Result<super::kernel::Kernel, String>,
        name: &str,
        base: usize,
        argc: usize,
    ) -> VmResult<Value> {
        use super::kernel::Input;
        use super::tensor::KernelArg;
        let kernel = kernel.as_ref().map_err(|why| {
            err(
                "KernelError",
                format!(
                    "kernel `{name}` works on numbers, but cannot run on tensors because {why}"
                ),
            )
        })?;
        if argc != kernel.params.len() {
            return Err(err(
                "CallError",
                format!(
                    "`{name}` takes {} arguments, got {argc}",
                    kernel.params.len()
                ),
            ));
        }
        let args: Vec<Value> = self.stack[base..base + argc].to_vec();
        let mut shape: Option<(Vec<usize>, super::tensor::Device)> = None;
        for a in &args {
            match a {
                Value::Tensor(t) => match &shape {
                    None => shape = Some((t.shape.clone(), t.device)),
                    Some((s, d)) => {
                        if *s != t.shape {
                            return Err(err(
                                "ShapeError",
                                format!("kernel `{name}` needs tensors of one shape, got {s:?} and {:?}", t.shape),
                            ));
                        }
                        if *d != t.device {
                            return Err(err(
                                "DeviceError",
                                format!(
                                    "kernel `{name}` got tensors on {d} and {}; move one first",
                                    t.device
                                ),
                            ));
                        }
                    }
                },
                Value::Int(_) | Value::Float(_) => {}
                other => {
                    return Err(type_error(format!(
                        "kernel `{name}` takes tensors and numbers, not {}",
                        other.type_name()
                    )))
                }
            }
        }
        let Some((shape, device)) = shape else {
            return Err(err("InternalError", "no tensor argument"));
        };
        let len: usize = shape.iter().product();
        let scalar = |v: &Value| match v {
            Value::Int(i) => *i as f32,
            Value::Float(f) => *f as f32,
            _ => 0.0,
        };
        let backend_args: Vec<KernelArg> = args
            .iter()
            .map(|a| match a {
                Value::Tensor(t) => KernelArg::Tensor(t),
                other => KernelArg::Scalar(scalar(other)),
            })
            .collect();
        let data = match self.ctx.backend(device).kernel(kernel, &backend_args, len) {
            Some(data) => data,
            None => {
                let values: Vec<Option<std::borrow::Cow<[f32]>>> = args
                    .iter()
                    .map(|a| match a {
                        Value::Tensor(t) => Some(t.values()),
                        _ => None,
                    })
                    .collect();
                let inputs: Vec<Input> = args
                    .iter()
                    .zip(&values)
                    .map(|(a, v)| match v {
                        Some(v) => Input::Values(v),
                        None => Input::Scalar(scalar(a)),
                    })
                    .collect();
                kernel.run_cpu(&inputs, len)
            }
        };
        let tensor = super::tensor::Tensor::new(shape, data, device).map_err(ops::tensor_error)?;
        ops::alloc(tensor)
    }

    #[cold]
    #[inline(never)]
    fn start_task(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        let id = self.ctx.new_task_id();
        let handle = Rc::new(TaskHandle::new(id, closure.proto.name.clone()));
        let mut stack = Vec::with_capacity(argc + closure.proto.num_regs + 1);
        stack.push(Value::Function(closure.clone()));
        stack.extend(self.stack[base..base + argc].iter().cloned());
        self.stack[ret] = Value::Task(handle.clone());
        // New tasks start on the device the caller is using.
        let devices = self.ctx.devices.clone();
        let caller = self.current;
        let state = self.save_current();
        self.suspended.insert(caller, state);
        self.ready.push_front(caller);
        self.load(
            id,
            Suspended {
                stack,
                frames: Vec::new(),
                handlers: Vec::new(),
                devices,
                entry_depth: 1,
                handle: Some(handle),
            },
        );
        self.push_frame(closure, 0, 1, argc)
    }

    /// Ends the current task with `result` and wakes the tasks awaiting it.
    fn complete_task(&mut self, result: Result<Value, Value>) {
        if let Some(handle) = self.current_handle.take() {
            let waiters = handle.finish(result);
            self.ready.extend(waiters);
        }
        self.stack.clear();
        self.frames.clear();
        self.handlers.clear();
        self.ctx.devices.clear();
    }

    /// Switches to the next task that can run, waiting for timers if
    /// nothing is ready. The current task's state must already be saved or
    /// finished.
    fn resume_next(&mut self) -> Result<(), RuntimeError> {
        loop {
            self.apply_cancels();
            self.ready.extend(self.ctx.wakeups.drain(..));
            self.fire_timers();
            if !self.ctx.servers.is_empty() {
                self.service_servers();
            }
            while let Some(id) = self.ready.pop_front() {
                if let Some(state) = self.suspended.remove(&id) {
                    self.load(id, state);
                    return Ok(());
                }
            }
            let next_timer = self.ctx.timers.iter().map(|(t, _)| *t).min();
            if self.ctx.servers.iter().any(|s| s.accepting()) {
                // Wait for a connection, or for the next timer if sooner.
                let mut wait = std::time::Duration::from_millis(2);
                if let Some(when) = next_timer {
                    wait = wait.min(when.saturating_duration_since(std::time::Instant::now()));
                }
                std::thread::sleep(wait);
                continue;
            }
            match next_timer {
                Some(when) => {
                    let now = std::time::Instant::now();
                    if when > now {
                        std::thread::sleep(when - now);
                    }
                }
                None => {
                    // Nothing can run and nothing will wake: report it from
                    // the task the host is waiting on.
                    if let Some(state) = self.suspended.remove(&ROOT_TASK) {
                        self.load(ROOT_TASK, state);
                    }
                    self.cancel_tasks();
                    let mut error = RuntimeError::new(
                        "Deadlock",
                        "every task is waiting for another, so none can continue",
                    );
                    error.trace = self.trace();
                    return Err(error);
                }
            }
        }
    }

    /// Sends the responses of finished handlers, starts a task for each
    /// new request, and stops servers that are done. The current task's
    /// state must already be saved, as for `resume_next`.
    fn service_servers(&mut self) {
        let mut servers = std::mem::take(&mut self.ctx.servers);
        for server in &mut servers {
            let mut i = 0;
            while i < server.pending.len() {
                if server.pending[i].0.is_done() {
                    let (handle, mut stream) = server.pending.swap_remove(i);
                    let result = handle.result.borrow().clone();
                    if let Some(result) = result {
                        self.response_for(result).write_to(&mut stream);
                    }
                } else {
                    i += 1;
                }
            }
            if !server.accepting() {
                continue;
            }
            for mut stream in web::accept_ready(&server.listener, server.remaining) {
                if let Some(n) = &mut server.remaining {
                    *n -= 1;
                }
                match self.start_request(&mut stream) {
                    Ok(handle) => server.pending.push((handle, stream)),
                    Err(response) => response.write_to(&mut stream),
                }
            }
        }
        servers.retain(|server| {
            if server.finished() {
                let waiters = server.handle.finish(Ok(Value::None));
                self.ready.extend(waiters);
                false
            } else {
                true
            }
        });
        // Servers started by the handlers just run come after these.
        servers.append(&mut self.ctx.servers);
        self.ctx.servers = servers;
    }

    /// Reads a request and starts its handler as a task, or returns the
    /// response to send at once.
    fn start_request(
        &mut self,
        stream: &mut std::net::TcpStream,
    ) -> Result<Rc<TaskHandle>, web::Response> {
        let request =
            web::read_request(stream).map_err(|e| web::Response::error(400, "BadRequest", &e))?;
        let Some(method) = HttpMethod::from_ident(&request.method) else {
            return Err(web::Response::error(
                405,
                "MethodNotAllowed",
                &format!("unknown method {}", request.method),
            ));
        };
        let body = request_body(
            &request.body,
            request.headers.get("content-type").map(String::as_str),
        )?;
        let resolved = self
            .resolve_route(
                method,
                &request.path,
                &request.query,
                &self.ctx.providers,
                body.as_ref(),
            )
            .map_err(|e| web::Response::error(web::status_for(&e.kind), &e.kind, &e.message))?;
        let Some((handler, args)) = resolved else {
            return Err(web::Response::error(
                404,
                "NotFound",
                &format!("no route for {} {}", request.method, request.path),
            ));
        };
        let id = self.ctx.new_task_id();
        let handle = Rc::new(TaskHandle::new(id, handler.proto.name.clone()));
        // The server reports the outcome, so it never goes unobserved.
        handle.observed.set(true);
        let argc = args.len();
        let mut stack = Vec::with_capacity(argc + handler.proto.num_regs + 1);
        stack.push(Value::Function(handler.clone()));
        stack.extend(args);
        self.load(
            id,
            Suspended {
                stack,
                frames: Vec::new(),
                handlers: Vec::new(),
                devices: Vec::new(),
                entry_depth: 1,
                handle: Some(handle.clone()),
            },
        );
        let pushed = self.push_frame(handler, 0, 1, argc);
        let state = self.save_current();
        match pushed {
            Ok(()) => {
                self.suspended.insert(id, state);
                self.ready.push_back(id);
                Ok(handle)
            }
            Err(e) => {
                let e = self.runtime_error(e);
                Err(web::Response::error(500, &e.kind, &e.message))
            }
        }
    }

    /// The response for what a route handler returned or raised.
    fn response_for(&self, result: Result<Value, Value>) -> web::Response {
        match result {
            Ok(value) => {
                let (status, body) = match &value {
                    Value::Tuple(items) if items.len() == 2 => match &items[0] {
                        Value::Int(code) if (100..600).contains(code) => {
                            (*code as u16, items[1].clone())
                        }
                        _ => (200, value.clone()),
                    },
                    _ => (200, value.clone()),
                };
                match body {
                    Value::None => {
                        web::Response::text(if status == 200 { 204 } else { status }, String::new())
                    }
                    Value::Str(text) => web::Response::text(status, text.to_string()),
                    other => web::Response::json(status, self.to_json(&other)),
                }
            }
            Err(failure) => {
                let (kind, message) = match &failure {
                    Value::Error(e) => (e.kind.to_string(), e.message.clone()),
                    Value::Struct(inst) => (self.type_name(inst.ty), self.display(&failure)),
                    Value::Object(obj) => (self.type_name(obj.borrow().ty), self.display(&failure)),
                    other => ("Failure".to_string(), self.display(other)),
                };
                let message = message.replace(&kind, web::short_kind(&kind));
                web::Response::error(web::status_for(&kind), web::short_kind(&kind), &message)
            }
        }
    }

    fn fire_timers(&mut self) {
        // Only read the clock when there is a timer: some hosts (WebAssembly
        // without imports) have none.
        if self.ctx.timers.is_empty() {
            return;
        }
        let now = std::time::Instant::now();
        let mut i = 0;
        while i < self.ctx.timers.len() {
            if self.ctx.timers[i].0 <= now {
                let (_, handle) = self.ctx.timers.swap_remove(i);
                let waiters = handle.finish(Ok(Value::None));
                self.ready.extend(waiters);
            } else {
                i += 1;
            }
        }
    }

    /// Waits for `value`: a task, a channel, or a tuple or list of tasks.
    fn await_value(&mut self, value: &Value) -> Awaited {
        match value {
            Value::Task(handle) => match &*handle.result.borrow() {
                Some(Ok(v)) => {
                    handle.observed.set(true);
                    Awaited::Ready(v.clone())
                }
                Some(Err(e)) => {
                    handle.observed.set(true);
                    Awaited::Failed(e.clone())
                }
                None => {
                    handle.waiters.borrow_mut().push(self.current);
                    Awaited::Blocked
                }
            },
            Value::Channel(ch) => {
                let mut ch = ch.borrow_mut();
                match ch.queue.pop_front() {
                    Some(v) => Awaited::Ready(v),
                    None if ch.closed => Awaited::Ready(Value::None),
                    None => {
                        ch.waiters.push(self.current);
                        Awaited::Blocked
                    }
                }
            }
            Value::Tuple(items) => match self.await_all(items) {
                Ok(Some(values)) => Awaited::Ready(Value::tuple(values)),
                Ok(None) => Awaited::Blocked,
                Err(e) => Awaited::Failed(e),
            },
            Value::List(items) => {
                let items = items.borrow().clone();
                match self.await_all(&items) {
                    Ok(Some(values)) => Awaited::Ready(Value::list(values)),
                    Ok(None) => Awaited::Blocked,
                    Err(e) => Awaited::Failed(e),
                }
            }
            Value::Foreign(f) if f.as_any().is::<super::stdlib::Race>() => {
                let race = f
                    .as_any()
                    .downcast_ref::<super::stdlib::Race>()
                    .expect("checked above");
                self.await_race(race)
            }
            Value::Foreign(f) if f.as_any().is::<super::stdlib::Group>() => {
                match self.await_all(&ops::collect(value).unwrap_or_default()) {
                    Ok(Some(values)) => Awaited::Ready(Value::list(values)),
                    Ok(None) => Awaited::Blocked,
                    Err(e) => Awaited::Failed(e),
                }
            }
            other => Awaited::Ready(other.clone()),
        }
    }

    /// The first ready item of a `task.select` or `task.timeout`; otherwise
    /// waits on all of them. A task woken by a loser later just finds its
    /// `await` still blocked and waits again.
    fn await_race(&mut self, race: &super::stdlib::Race) -> Awaited {
        for (i, item) in race.items.iter().enumerate() {
            let ready = match item {
                Value::Task(h) => h.is_done(),
                Value::Channel(ch) => {
                    let ch = ch.borrow();
                    !ch.queue.is_empty() || ch.closed
                }
                _ => true,
            };
            if !ready {
                continue;
            }
            return match self.await_value(item) {
                Awaited::Ready(v) => {
                    if race.timer == Some(i) {
                        // The work lost: stop it.
                        for (j, other) in race.items.iter().enumerate() {
                            if let (true, Value::Task(h)) = (j != i, other) {
                                if !h.is_done() {
                                    self.ctx.cancels.push(h.clone());
                                }
                            }
                        }
                        Awaited::Failed(err(
                            "Timeout",
                            format!("the work took longer than {} seconds", race.seconds),
                        ))
                    } else if race.timer.is_some() {
                        Awaited::Ready(v)
                    } else {
                        Awaited::Ready(Value::tuple(vec![Value::Int(i as i64), v]))
                    }
                }
                other => other,
            };
        }
        for item in &race.items {
            match item {
                Value::Task(h) => h.waiters.borrow_mut().push(self.current),
                Value::Channel(ch) => ch.borrow_mut().waiters.push(self.current),
                _ => {}
            }
        }
        Awaited::Blocked
    }

    /// Stops the tasks builtins asked to cancel: they never run again, and
    /// awaiting them fails with `Cancelled`.
    fn apply_cancels(&mut self) {
        for handle in std::mem::take(&mut self.ctx.cancels) {
            if handle.is_done() || handle.id == self.current {
                continue;
            }
            self.suspended.remove(&handle.id);
            self.ready.retain(|id| *id != handle.id);
            let waiters = handle.finish(Err(err(
                "Cancelled",
                format!("the task `{}` was cancelled", handle.name),
            )));
            // Nobody has to look at a cancellation.
            handle.observed.set(true);
            self.ready.extend(waiters);
        }
    }

    /// Results of every task in `items`, or `None` after registering as a
    /// waiter on the first unfinished one.
    fn await_all(&mut self, items: &[Value]) -> Result<Option<Vec<Value>>, Value> {
        for item in items {
            if let Value::Task(handle) = item {
                if !handle.is_done() {
                    handle.waiters.borrow_mut().push(self.current);
                    return Ok(None);
                }
            }
        }
        let mut values = Vec::with_capacity(items.len());
        for item in items {
            match self.await_value(item) {
                Awaited::Ready(v) => values.push(v),
                Awaited::Failed(e) => return Err(e),
                Awaited::Blocked => return Ok(None),
            }
        }
        Ok(Some(values))
    }

    #[inline]
    /// `create f(a, rest...)`: the arguments after the fixed ones become
    /// one list.
    #[cold]
    fn push_variadic_frame(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        let proto = &closure.proto;
        let fixed = proto.params.len() - 1;
        if argc < proto.required {
            return Err(err(
                "CallError",
                format!(
                    "`{}` takes at least {} argument{}, got {argc}",
                    proto.name,
                    proto.required,
                    if proto.required == 1 { "" } else { "s" }
                ),
            ));
        }
        let needed = base + proto.num_regs.max(fixed + 1).max(argc);
        if self.stack.len() < needed {
            self.stack.resize(needed, Value::None);
        }
        let rest: Vec<Value> = if argc > fixed {
            self.stack[base + fixed..base + argc].to_vec()
        } else {
            for slot in &mut self.stack[base + argc..base + fixed] {
                *slot = Value::Missing;
            }
            Vec::new()
        };
        self.stack[base + fixed] = Value::list(rest);
        self.push_frame_unchecked(closure, ret, base, fixed + 1)
    }

    fn push_frame(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        let mut argc = argc;
        if let Some(receiver) = &closure.bound {
            // The object goes first, as `self`.
            if self.stack.len() < base + argc + 1 {
                self.stack.resize(base + argc + 1, Value::None);
            }
            self.stack[base..base + argc + 1].rotate_right(1);
            self.stack[base] = receiver.clone();
            argc += 1;
        }
        let proto = &closure.proto;
        let arity = proto.params.len();
        if proto.variadic {
            return self.push_variadic_frame(closure, ret, base, argc);
        }
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
        self.push_frame_unchecked(closure, ret, base, argc)
    }

    #[inline]
    fn push_frame_unchecked(
        &mut self,
        closure: Rc<Closure>,
        ret: usize,
        base: usize,
        argc: usize,
    ) -> VmResult<()> {
        let proto = &closure.proto;
        let arity = proto.params.len();
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
    /// Puts keyword arguments in parameter order. The `argc` arguments
    /// start at `start`; the tuple of keyword names is in the register after
    /// them and names the last ones. Returns the number of arguments now in
    /// place, with left-out ones marked missing so defaults apply.
    fn match_keywords(
        &mut self,
        start: usize,
        argc: usize,
        params: &[String],
        required: usize,
        name: &str,
    ) -> VmResult<usize> {
        let names: Vec<String> = match &self.stack[start + argc] {
            Value::Tuple(items) => items
                .iter()
                .map(|n| match n {
                    Value::Str(s) => (**s).clone(),
                    _ => String::new(),
                })
                .collect(),
            _ => return Err(err("InternalError", "keyword names are missing")),
        };
        let positional = argc - names.len();
        if positional > params.len() {
            return Err(err(
                "CallError",
                format!(
                    "`{name}` takes {} argument{}, got {positional}",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" }
                ),
            ));
        }
        let mut slots = vec![Value::Missing; params.len()];
        for (i, slot) in slots.iter_mut().enumerate().take(positional) {
            *slot = std::mem::replace(&mut self.stack[start + i], Value::None);
        }
        for (k, keyword) in names.iter().enumerate() {
            let Some(index) = params.iter().position(|p| p == keyword) else {
                return Err(err(
                    "CallError",
                    format!(
                        "`{name}` has no parameter `{keyword}`; its parameters are: {}",
                        params.join(", ")
                    ),
                ));
            };
            if !matches!(slots[index], Value::Missing) {
                return Err(err("CallError", format!("`{keyword}` is given twice")));
            }
            slots[index] = std::mem::replace(&mut self.stack[start + positional + k], Value::None);
        }
        if let Some(missing) =
            (0..required.min(slots.len())).find(|&i| matches!(slots[i], Value::Missing))
        {
            return Err(err(
                "CallError",
                format!("`{name}` is missing the argument `{}`", params[missing]),
            ));
        }
        while matches!(slots.last(), Some(Value::Missing)) {
            slots.pop();
        }
        let count = slots.len();
        if self.stack.len() < start + count + 1 {
            self.stack.resize(start + count + 1, Value::None);
        }
        for (i, value) in slots.into_iter().enumerate() {
            self.stack[start + i] = value;
        }
        Ok(count)
    }

    #[inline]
    fn finish_frame(&mut self, value: Value) -> Option<Value> {
        let entry_depth = self.entry_depth;
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
        fast::store(&mut self.stack[frame.ret], value);
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
    fn unwind(&mut self, thrown: Value) -> Unwound {
        let entry_depth = self.entry_depth;
        if self
            .handlers
            .last()
            .is_some_and(|h| h.frame_depth >= entry_depth)
        {
            let trace = self.trace();
            if let Some(handler) = self.handlers.pop() {
                self.caught = Some((thrown.clone(), trace));
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
                return Unwound::Handled;
            }
        }
        Unwound::Unhandled(thrown)
    }

    fn trace(&self) -> Vec<TraceLine> {
        let first = self.entry_depth.saturating_sub(1).min(self.frames.len());
        self.frames[first..]
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
                file: frame.closure.proto.file,
            })
            .collect()
    }

    fn runtime_error(&self, thrown: Value) -> RuntimeError {
        let (kind, message) = match &thrown {
            Value::Error(e) => (e.kind.to_string(), e.message.clone()),
            Value::Struct(inst) => (self.type_name(inst.ty), self.display(&thrown)),
            Value::Object(obj) => (self.type_name(obj.borrow().ty), self.display(&thrown)),
            other => ("Failure".to_string(), self.display(other)),
        };
        let trace = match &self.caught {
            Some((value, trace)) if same_failure(value, &thrown) => trace.clone(),
            _ => self.trace(),
        };
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

    /// `obj.method` without a call: a function that remembers `obj`.
    fn bound_method(&self, receiver: &Value, ty: u16, name: Symbol) -> VmResult<Value> {
        match self.methods.get(&(ty, name)) {
            Some(method) => Ok(Value::Function(Rc::new(Closure {
                proto: method.proto.clone(),
                captures: method.captures.clone(),
                bound: Some(receiver.clone()),
            }))),
            None => Err(self.no_field(ty, name)),
        }
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

    /// Runs until task `root`, the one the host called into, returns.
    /// Other tasks run whenever `root` waits for something.
    fn execute(&mut self, root: TaskId) -> Result<Value, RuntimeError> {
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
            // Stores into a register. Overwriting a value that owns no heap
            // memory (a number, a bool) needs no destructor call.
            macro_rules! put {
                ($r:expr, $v:expr) => {{
                    let value = $v;
                    let slot = &mut self.stack[base + $r as usize];
                    if slot.owns_heap() {
                        *slot = value;
                    } else {
                        std::mem::forget(std::mem::replace(slot, value));
                    }
                }};
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
                    match self.unwind(error) {
                        Unwound::Handled => continue 'frames,
                        Unwound::Unhandled(failure) => {
                            if self.current == root {
                                return Err(self.runtime_error(failure));
                            }
                            // A spawned task failed: whoever awaits it gets
                            // the failure.
                            self.complete_task(Err(failure));
                            self.resume_next()?;
                            continue 'frames;
                        }
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
            // A builtin such as `web.serve` asked the task to wait.
            macro_rules! block_on {
                () => {
                    if let Some(handle) = self.ctx.block_on.take() {
                        if self.entry_depth > 1 {
                            throw!(waiting_in_callback());
                        }
                        save_ip!();
                        handle.waiters.borrow_mut().push(self.current);
                        let id = self.current;
                        let state = self.save_current();
                        self.suspended.insert(id, state);
                        self.resume_next()?;
                        continue 'frames;
                    }
                };
            }
            // A call of the value in stack[at] with `argc` arguments after it.
            macro_rules! call_at {
                ($at:expr, $argc:expr) => {{
                    let at: usize = $at;
                    let argc: usize = $argc;
                        match &self.stack[at] {
                            Value::Function(c)
                                if c.bound.is_none() && c.proto.native.get().is_some() =>
                            {
                                let native = c.proto.native.get().cloned().expect("checked");
                                match self.call_native(&native, at + 1, argc) {
                                    Some(result) => {
                                        let v = check!(result);
                                        self.stack[at] = v;
                                    }
                                    None => {
                                        let Value::Function(callee) =
                                            std::mem::replace(&mut self.stack[at], Value::None)
                                        else {
                                            unreachable!()
                                        };
                                        save_ip!();
                                        check!(self.enter(callee, at, at + 1, argc));
                                        continue 'frames;
                                    }
                                }
                            }
                            Value::Function(_) => {
                                // The return value replaces the function,
                                // so the frame can take it over.
                                let Value::Function(callee) =
                                    std::mem::replace(&mut self.stack[at], Value::None)
                                else {
                                    unreachable!()
                                };
                                save_ip!();
                                check!(self.enter(callee, at, at + 1, argc));
                                continue 'frames;
                            }
                            Value::Native(id) => {
                                let id = *id;
                                let result = check!(builtins::call(
                                    id,
                                    &mut self.ctx,
                                    &self.stack[at + 1..at + 1 + argc]
                                ));
                                self.stack[at] = result;
                                block_on!();
                            }
                            Value::Foreign(f) => {
                                let f = f.clone();
                                let args: Vec<Value> =
                                    self.stack[at + 1..at + 1 + argc].to_vec();
                                let result = check!(f.call(&mut self.ctx, &args, &[]));
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
                                    }};
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
                    put!($dst, value);
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
                    put!($dst, value);
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
                    put!($dst, value);
                }};
            }

            macro_rules! compare_imm {
                ($dst:expr, $a:expr, $imm:expr, $kind:expr, $op:tt) => {{
                    let fast = match &reg!($a) {
                        Value::Int(x) => Some(*x $op i64::from($imm)),
                        Value::Float(x) => Some(*x $op f64::from($imm)),
                        _ => None,
                    };
                    let value = match fast {
                        Some(b) => Value::Bool(b),
                        None => check!(ops::compare(
                            &mut self.ctx,
                            $kind,
                            &self.stack[base + $a as usize],
                            &Value::Int(i64::from($imm))
                        )),
                    };
                    put!($dst, value);
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
                        let value = fast::copy(&reg!(src));
                        put!(dst, value);
                    }
                    Op::LoadConst { dst, k } => put!(dst, fast::copy(&proto.consts[k as usize])),
                    Op::LoadInt { dst, value } => put!(dst, Value::Int(i64::from(value))),
                    Op::LoadNone { dst } => put!(dst, Value::None),
                    Op::LoadBool { dst, value } => put!(dst, Value::Bool(value)),
                    Op::LoadMissing { dst } => put!(dst, Value::Missing),
                    Op::GetGlobal { dst, idx } => {
                        let value = self.globals[idx as usize].clone();
                        put!(dst, value);
                    }
                    Op::SetGlobal { src, idx } => {
                        let value = reg!(src).clone();
                        self.globals[idx as usize] = value;
                    }
                    Op::GetCapture { dst, idx } => {
                        put!(dst, closure.captures[idx as usize].clone());
                    }
                    Op::MakeCell { reg } => {
                        let value = std::mem::replace(&mut reg!(reg), Value::None);
                        put!(reg, Value::Cell(Rc::new(RefCell::new(value))));
                    }
                    Op::GetCell { dst, cell } => {
                        let value = match &reg!(cell) {
                            Value::Cell(c) => c.borrow().clone(),
                            other => other.clone(),
                        };
                        put!(dst, value);
                    }
                    Op::SetCell { cell, src } => {
                        let value = reg!(src).clone();
                        match &reg!(cell) {
                            Value::Cell(c) => *c.borrow_mut() = value,
                            _ => put!(cell, value),
                        }
                    }
                    Op::GetUpval { dst, idx } => {
                        let value = match &closure.captures[idx as usize] {
                            Value::Cell(c) => c.borrow().clone(),
                            other => other.clone(),
                        };
                        put!(dst, value);
                    }
                    Op::SetUpval { idx, src } => {
                        let value = reg!(src).clone();
                        if let Value::Cell(c) = &closure.captures[idx as usize] {
                            *c.borrow_mut() = value;
                        }
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
                            bound: None,
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
                    Op::FloorDiv { dst, a, b } => arith!(
                        dst,
                        a,
                        b,
                        Arith::FloorDiv,
                        |x, y| fast::floor_div(x, y).map(Value::Int),
                        |_x, _y| None
                    ),
                    Op::Rem { dst, a, b } => arith!(
                        dst,
                        a,
                        b,
                        Arith::Rem,
                        |x, y| fast::modulo(x, y).map(Value::Int),
                        |_x, _y| None
                    ),
                    Op::Pow { dst, a, b } => slow_arith!(dst, a, b, Arith::Pow),
                    Op::BitAnd { dst, a, b } => slow_arith!(dst, a, b, Arith::BitAnd),
                    Op::BitOr { dst, a, b } => slow_arith!(dst, a, b, Arith::BitOr),
                    Op::BitXor { dst, a, b } => slow_arith!(dst, a, b, Arith::BitXor),
                    Op::Shl { dst, a, b } => slow_arith!(dst, a, b, Arith::Shl),
                    Op::Shr { dst, a, b } => slow_arith!(dst, a, b, Arith::Shr),
                    Op::AddI { dst, a, imm } => {
                        let value = match &reg!(a) {
                            Value::Int(x) => x.checked_add(i64::from(imm)).map(Value::Int),
                            Value::Float(x) => Some(Value::Float(x + f64::from(imm))),
                            _ => None,
                        };
                        let value = match value {
                            Some(v) => v,
                            None => check!(ops::arith(
                                &mut self.ctx,
                                Arith::Add,
                                &self.stack[base + a as usize],
                                &Value::Int(i64::from(imm))
                            )),
                        };
                        put!(dst, value);
                    }
                    Op::SubI { dst, a, imm } => {
                        let value = match &reg!(a) {
                            Value::Int(x) => x.checked_sub(i64::from(imm)).map(Value::Int),
                            Value::Float(x) => Some(Value::Float(x - f64::from(imm))),
                            _ => None,
                        };
                        let value = match value {
                            Some(v) => v,
                            None => check!(ops::arith(
                                &mut self.ctx,
                                Arith::Sub,
                                &self.stack[base + a as usize],
                                &Value::Int(i64::from(imm))
                            )),
                        };
                        put!(dst, value);
                    }
                    Op::LtI { dst, a, imm } => compare_imm!(dst, a, imm, Cmp::Lt, <),
                    Op::LtEqI { dst, a, imm } => compare_imm!(dst, a, imm, Cmp::LtEq, <=),
                    Op::GtI { dst, a, imm } => compare_imm!(dst, a, imm, Cmp::Gt, >),
                    Op::GtEqI { dst, a, imm } => compare_imm!(dst, a, imm, Cmp::GtEq, >=),
                    Op::EqI { dst, a, imm } => {
                        let equal = match &reg!(a) {
                            Value::Int(x) => *x == i64::from(imm),
                            other => ops::equals(other, &Value::Int(i64::from(imm))),
                        };
                        put!(dst, Value::Bool(equal));
                    }
                    Op::NotEqI { dst, a, imm } => {
                        let equal = match &reg!(a) {
                            Value::Int(x) => *x == i64::from(imm),
                            other => ops::equals(other, &Value::Int(i64::from(imm))),
                        };
                        put!(dst, Value::Bool(!equal));
                    }
                    Op::Neg { dst, src } => {
                        let value = match &reg!(src) {
                            Value::Float(x) => Value::Float(-x),
                            other => check!(ops::negate(&mut self.ctx, &other.clone())),
                        };
                        put!(dst, value);
                    }
                    Op::Pos { dst, src } => {
                        let value = match &reg!(src) {
                            v @ (Value::Int(_) | Value::Float(_) | Value::Tensor(_)) => v.clone(),
                            other => throw!(type_error(format!(
                                "unary `+` needs a number, not {}",
                                other.type_name()
                            ))),
                        };
                        put!(dst, value);
                    }
                    Op::BitNot { dst, src } => {
                        let value = match &reg!(src) {
                            Value::Int(x) => Value::Int(!x),
                            other => throw!(type_error(format!(
                                "`~` needs an integer, not {}",
                                other.type_name()
                            ))),
                        };
                        put!(dst, value);
                    }
                    Op::Not { dst, src } => {
                        let value = !reg!(src).truthy();
                        put!(dst, Value::Bool(value));
                    }

                    // ---- comparison ----
                    Op::Eq { dst, a, b } => {
                        let equal = match (&reg!(a), &reg!(b)) {
                            (Value::Int(x), Value::Int(y)) => x == y,
                            (x, y) => ops::equals(x, y),
                        };
                        put!(dst, Value::Bool(equal));
                    }
                    Op::NotEq { dst, a, b } => {
                        let equal = match (&reg!(a), &reg!(b)) {
                            (Value::Int(x), Value::Int(y)) => x == y,
                            (x, y) => ops::equals(x, y),
                        };
                        put!(dst, Value::Bool(!equal));
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
                        put!(dst, Value::Bool(found));
                    }

                    // ---- tensors and devices ----
                    Op::MatMul { dst, a, b } => {
                        let value = check!(ops::matmul(
                            &mut self.ctx,
                            &self.stack[base + a as usize],
                            &self.stack[base + b as usize]
                        ));
                        put!(dst, value);
                    }
                    Op::Place { dst, src, target } => {
                        let value = check!(ops::place(
                            &mut self.ctx,
                            &self.stack[base + src as usize],
                            &self.stack[base + target as usize]
                        ));
                        put!(dst, value);
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
                        put!(dst, iter);
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
                            put!(dst, value);
                            ip += 1;
                        }
                    }

                    // ---- calls ----
                    Op::Call { func, argc } => {
                        call_at!(base + func as usize, argc as usize);
                    }
                    // `f(a, xs...)`: the arguments arrive as one list.
                    Op::CallSpread { func } => {
                        let at = base + func as usize;
                        let args = check!(ops::collect(&self.stack[at + 1]));
                        let needed = at + 1 + args.len() + 1;
                        if self.stack.len() < needed {
                            self.stack.resize(needed, Value::None);
                        }
                        let n = args.len();
                        for (i, a) in args.into_iter().enumerate() {
                            self.stack[at + 1 + i] = a;
                        }
                        call_at!(at, n);
                    }
                    Op::CallKw { func, argc } => {
                        let at = base + func as usize;
                        let callee = self.stack[at].clone();
                        if let Value::Foreign(f) = &callee {
                            let (args, kwargs) =
                                check!(split_keywords(&self.stack[at + 1..], argc as usize));
                            let result = check!(f.call(&mut self.ctx, &args, &kwargs));
                            self.stack[at] = result;
                            continue;
                        }
                        let (params, required, name): (Vec<String>, usize, String) = match &callee {
                            Value::Function(c) => (
                                c.proto
                                    .params
                                    .iter()
                                    .map(|p| self.program.symbols.name(*p).to_string())
                                    .collect(),
                                c.proto.required,
                                c.proto.name.clone(),
                            ),
                            Value::Native(id) => {
                                let native = &builtins::NATIVES[*id as usize];
                                if native.variadic && matches!(native.name, "min" | "max") {
                                    // `max(xs, key = f)`: the Prelude's
                                    // `__vyrn_max_by(items, f)` does it.
                                    let (args, kwargs) = check!(split_keywords(
                                        &self.stack[at + 1..],
                                        argc as usize
                                    ));
                                    let key = match kwargs.as_slice() {
                                        [(k, v)] if k == "key" => v.clone(),
                                        _ => throw!(type_error(format!(
                                            "`{}` takes one keyword argument, `key`",
                                            native.name
                                        ))),
                                    };
                                    let items = match args.as_slice() {
                                        [single] => check!(ops::collect(single)),
                                        _ => args,
                                    };
                                    let helper = format!("__vyrn_{}_by", native.name);
                                    let Some(Value::Function(f)) = self
                                        .program
                                        .global(&helper)
                                        .map(|g| self.globals[g].clone())
                                    else {
                                        throw!(type_error(format!(
                                            "`{}` takes no keyword arguments here",
                                            native.name
                                        )));
                                    };
                                    self.stack[at + 1] = Value::list(items);
                                    self.stack[at + 2] = key;
                                    save_ip!();
                                    check!(self.enter(f, at, at + 1, 2));
                                    continue 'frames;
                                }
                                if native.variadic {
                                    throw!(type_error(format!(
                                        "`{}` takes no keyword arguments",
                                        native.name
                                    )));
                                }
                                (
                                    native.params.iter().map(|p| p.to_string()).collect(),
                                    native.required,
                                    native.name.to_string(),
                                )
                            }
                            other => {
                                throw!(type_error(format!("{} is not callable", other.type_name())))
                            }
                        };
                        let count = check!(self.match_keywords(
                            at + 1,
                            argc as usize,
                            &params,
                            required,
                            &name
                        ));
                        match callee {
                            Value::Function(c) => {
                                save_ip!();
                                check!(self.enter(c, at, at + 1, count));
                                continue 'frames;
                            }
                            Value::Native(id) => {
                                let result = check!(builtins::call(
                                    id,
                                    &mut self.ctx,
                                    &self.stack[at + 1..at + 1 + count]
                                ));
                                self.stack[at] = result;
                                block_on!();
                            }
                            _ => {}
                        }
                    }
                    Op::CallMethodKw {
                        base: b,
                        argc,
                        name,
                    } => {
                        let result_at = base + b as usize;
                        let receiver = result_at + 1;
                        let symbol = proto.names[name as usize];
                        if let Value::Foreign(f) = &self.stack[receiver] {
                            let f = f.clone();
                            let (args, kwargs) =
                                check!(split_keywords(&self.stack[receiver + 1..], argc as usize));
                            let method = self.program.symbols.name(symbol).to_string();
                            let value =
                                check!(f.call_method(&mut self.ctx, &method, &args, &kwargs));
                            self.stack[result_at] = value;
                            continue;
                        }
                        let ty = match &self.stack[receiver] {
                            Value::Struct(inst) => Some(inst.ty),
                            Value::Object(obj) => Some(obj.borrow().ty),
                            _ => None,
                        };
                        let method = ty
                            .and_then(|ty| self.methods.get(&(ty, symbol)).cloned())
                            .or_else(|| self.builtin_method(&self.stack[receiver], symbol));
                        let Some(method) = method else {
                            throw!(type_error(format!(
                                "`.{}()` takes no keyword arguments here; pass them by position",
                                self.program.symbols.name(symbol)
                            )));
                        };
                        // `self` is matched by position, the rest by name.
                        let params: Vec<String> = method.proto.params[1..]
                            .iter()
                            .map(|p| self.program.symbols.name(*p).to_string())
                            .collect();
                        let required = method.proto.required.saturating_sub(1);
                        let name = method.proto.name.clone();
                        let count = check!(self.match_keywords(
                            receiver + 1,
                            argc as usize,
                            &params,
                            required,
                            &name
                        ));
                        save_ip!();
                        check!(self.enter(method, result_at, receiver, count + 1));
                        continue 'frames;
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
                                Some(method) if method.proto.native.get().is_some() => {
                                    let native =
                                        method.proto.native.get().cloned().expect("checked");
                                    match self.call_native(&native, receiver, argc as usize + 1) {
                                        Some(result) => {
                                            let v = check!(result);
                                            self.stack[result_at] = v;
                                            continue;
                                        }
                                        None => {
                                            save_ip!();
                                            check!(self.enter(
                                                method,
                                                result_at,
                                                receiver,
                                                argc as usize + 1
                                            ));
                                            continue 'frames;
                                        }
                                    }
                                }
                                Some(method) => {
                                    save_ip!();
                                    check!(self.enter(
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
                        if let Some(method) = self.builtin_method(&self.stack[receiver], symbol) {
                            save_ip!();
                            check!(self.enter(method, result_at, receiver, argc as usize + 1));
                            continue 'frames;
                        }
                        let result = check!(builtins::call_method(
                            &mut self.ctx,
                            self.program.symbols.name(symbol),
                            &self.stack[receiver..receiver + 1 + argc as usize]
                        ));
                        self.stack[result_at] = result;
                    }
                    Op::Return { src } => {
                        // The frame ends here, so its value can be moved out
                        // rather than copied; parameters stay, as callers
                        // read `mut` arguments back from them.
                        let value = if (src as usize) < proto.params.len() {
                            reg!(src).clone()
                        } else {
                            std::mem::replace(&mut reg!(src), Value::None)
                        };
                        if let Some(value) = self.finish_frame(value) {
                            if self.current == root {
                                return Ok(value);
                            }
                            self.complete_task(Ok(value));
                            self.resume_next()?;
                        }
                        continue 'frames;
                    }
                    Op::ReturnNone => {
                        if let Some(value) = self.finish_frame(Value::None) {
                            if self.current == root {
                                return Ok(value);
                            }
                            self.complete_task(Ok(value));
                            self.resume_next()?;
                        }
                        continue 'frames;
                    }

                    // ---- collections and objects ----
                    Op::NewList { dst, start, count } => {
                        let from = base + start as usize;
                        let items = self.stack[from..from + count as usize].to_vec();
                        put!(dst, Value::list(items));
                    }
                    Op::NewTuple { dst, start, count } => {
                        let from = base + start as usize;
                        let items = self.stack[from..from + count as usize].to_vec();
                        put!(dst, Value::tuple(items));
                    }
                    Op::NewMap { dst } => put!(dst, builtins::new_map()),
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
                        put!(dst, range);
                    }
                    Op::MakeSlice { dst, base: b } => {
                        let at = base + b as usize;
                        let slice = check!(ops::slice_value(
                            &self.stack[at],
                            &self.stack[at + 1],
                            &self.stack[at + 2]
                        ));
                        put!(dst, slice);
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
                        put!(dst, value);
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
                        put!(dst, value);
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
                                None => self.bound_method(&reg!(obj), inst.ty, symbol),
                            },
                            Value::Object(o) => {
                                let ty = o.borrow().ty;
                                match self.field_slot(ty, symbol) {
                                    Some(i) => Ok(o.borrow().fields[i].clone()),
                                    None => self.bound_method(&reg!(obj), ty, symbol),
                                }
                            }
                            Value::Type(t) => self.type_member(*t, symbol),
                            other => ops::get_attr(other, self.program.symbols.name(symbol)),
                        };
                        let value = check!(result);
                        put!(dst, value);
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
                            Some(v) => put!(dst, v),
                            None => throw!(type_error("pattern field out of range")),
                        }
                    }
                    Op::Len { dst, src } => {
                        let n = check!(ops::len(&reg!(src)));
                        put!(dst, Value::Int(n));
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
                        put!(dst, Value::str(text));
                    }

                    // ---- patterns and failures ----
                    Op::IsType { dst, src, name } => {
                        let type_name = self.program.symbols.name(proto.names[name as usize]);
                        let matches = ops::is_type(&self.program, &reg!(src), type_name);
                        put!(dst, Value::Bool(matches));
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
                    // Calling an `async fn` already started a task; `spawn`
                    // makes that explicit, and wraps any other value in a
                    // finished task.
                    Op::Spawn { dst, src } => {
                        let value = match &reg!(src) {
                            task @ Value::Task(_) => task.clone(),
                            other => Value::Task(Rc::new(TaskHandle::done(other.clone()))),
                        };
                        put!(dst, value);
                    }
                    Op::Await { dst, src } => {
                        let value = reg!(src).clone();
                        match self.await_value(&value) {
                            Awaited::Ready(v) => put!(dst, v),
                            Awaited::Failed(e) => throw!(e),
                            Awaited::Blocked => {
                                if self.entry_depth > 1 {
                                    throw!(waiting_in_callback());
                                }
                                // Suspend, and run this `await` again when woken.
                                ip -= 1;
                                save_ip!();
                                let id = self.current;
                                let state = self.save_current();
                                self.suspended.insert(id, state);
                                self.resume_next()?;
                                continue 'frames;
                            }
                        }
                    }
                    Op::AsyncIterInit { dst, src } => {
                        let iter = match &reg!(src) {
                            ch @ Value::Channel(_) => ch.clone(),
                            other => check!(ops::make_iter(other)),
                        };
                        put!(dst, iter);
                    }
                    Op::AsyncIterNext { iter, dst } => {
                        let channel = match &reg!(iter) {
                            Value::Channel(ch) => Some(ch.clone()),
                            _ => None,
                        };
                        match channel {
                            Some(ch) => {
                                let next = {
                                    let mut ch = ch.borrow_mut();
                                    match ch.queue.pop_front() {
                                        Some(v) => Some(Some(v)),
                                        None if ch.closed => Some(None),
                                        None => {
                                            ch.waiters.push(self.current);
                                            None
                                        }
                                    }
                                };
                                match next {
                                    Some(Some(v)) => {
                                        put!(dst, v);
                                        ip += 1;
                                    }
                                    // Closed and empty: fall through to the exit.
                                    Some(None) => {}
                                    None => {
                                        if self.entry_depth > 1 {
                                            throw!(waiting_in_callback());
                                        }
                                        ip -= 1;
                                        save_ip!();
                                        let id = self.current;
                                        let state = self.save_current();
                                        self.suspended.insert(id, state);
                                        self.resume_next()?;
                                        continue 'frames;
                                    }
                                }
                            }
                            None => {
                                let next = match &reg!(iter) {
                                    Value::Iter(it) => it.borrow_mut().next_value(),
                                    other => throw!(type_error(format!(
                                        "cannot loop over {}",
                                        other.type_name()
                                    ))),
                                };
                                if let Some(value) = next {
                                    put!(dst, value);
                                    ip += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

/// A task can only be suspended at its own level: a function called back
/// by a built-in (such as the one passed to `map`) runs on the host's
/// stack, which cannot be saved.
fn waiting_in_callback() -> Value {
    ops::err(
        "AsyncError",
        "cannot wait inside a function called by a built-in; await outside the callback",
    )
}

/// A request body as a value: JSON when it parses (an error when it was
/// declared JSON and does not), otherwise text.
fn request_body(body: &[u8], content_type: Option<&str>) -> Result<Option<Value>, web::Response> {
    if body.is_empty() {
        return Ok(None);
    }
    let text = String::from_utf8_lossy(body).into_owned();
    let is_json = content_type.is_some_and(|t| t.contains("json"));
    Ok(Some(match super::json::parse(&text) {
        Ok(json) => ops::from_json(&json),
        Err(e) if is_json => {
            return Err(web::Response::error(
                400,
                "BadRequest",
                &format!("invalid JSON: {e}"),
            ))
        }
        Err(_) => Value::str(text),
    }))
}

/// Splits `argc` call arguments into positional ones and `name = value`
/// pairs: the tuple of keyword names follows the arguments and names the
/// last ones.
type Keywords = Vec<(String, Value)>;

fn split_keywords(slots: &[Value], argc: usize) -> VmResult<(Vec<Value>, Keywords)> {
    let names: Vec<String> = match slots.get(argc) {
        Some(Value::Tuple(items)) => items
            .iter()
            .map(|n| match n {
                Value::Str(s) => (**s).clone(),
                _ => String::new(),
            })
            .collect(),
        _ => return Err(err("InternalError", "keyword names are missing")),
    };
    let positional = argc - names.len();
    let args = slots[..positional].to_vec();
    let kwargs = names
        .into_iter()
        .zip(slots[positional..argc].iter().cloned())
        .collect();
    Ok((args, kwargs))
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

/// Whether two failures are the same value, passed on rather than raised
/// again.
fn same_failure(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Error(x), Value::Error(y)) => Rc::ptr_eq(x, y),
        (Value::Struct(x), Value::Struct(y)) => Rc::ptr_eq(x, y),
        (Value::Object(x), Value::Object(y)) => Rc::ptr_eq(x, y),
        _ => false,
    }
}
