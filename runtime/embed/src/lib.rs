//! Vyrn inside other programs: the C API declared in `include/vyrn.h`.
//!
//! A host program (a game engine, a tool, any C or C++ program) makes an
//! engine, loads a script, and calls its functions with JSON arguments.
//! Scripts call back into the host with `host.call(name, args...)`, and
//! their `print` goes wherever the host wants (Unreal's Output Log, say).
//!
//! Every function catches panics, so a bug in Vyrn becomes an error
//! message for the host instead of crashing the game.

#![allow(clippy::missing_safety_doc)]

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_void, CStr, CString};
use std::io::Write;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::rc::Rc;

use vyrn::vm::{self, ops, Vm};

pub type VyrnPrintFn = extern "C" fn(user: *mut c_void, line: *const c_char);
pub type VyrnHostFn = extern "C" fn(user: *mut c_void, args_json: *const c_char) -> *const c_char;

/// Where printed text goes: the host's function, or standard output.
type PrintTarget = Rc<RefCell<Option<(VyrnPrintFn, *mut c_void)>>>;

/// The functions scripts can call with `host.call`.
type HostTable = Rc<RefCell<HashMap<String, (VyrnHostFn, *mut c_void)>>>;

/// The handle C holds. It is only ever borrowed shared, so callbacks from
/// inside a running script (print, host functions, vyrn_host_error) never
/// alias the running VM, which lives in its own allocation.
pub struct VyrnEngine {
    shared: Shared,
    state: *mut State,
}

struct Shared {
    last_error: RefCell<CString>,
    print: PrintTarget,
    host_fns: HostTable,
    /// Set by vyrn_host_error() inside a host function.
    host_error: Rc<RefCell<Option<String>>>,
    /// Whether a call is running, to refuse calls from inside it.
    busy: Cell<bool>,
}

struct State {
    vm: Option<Vm>,
}

/// Collects printed text and hands it over a line at a time.
struct PrintSink {
    target: PrintTarget,
    pending: Vec<u8>,
}

impl PrintSink {
    fn emit(&self, line: &[u8]) {
        let text = String::from_utf8_lossy(line).into_owned();
        // Copied out, so the callback may call vyrn_set_print.
        let target = *self.target.borrow();
        match target {
            Some((print, user)) => {
                let c = CString::new(text.replace('\0', " ")).unwrap_or_default();
                print(user, c.as_ptr());
            }
            None => println!("{text}"),
        }
    }
}

impl Write for PrintSink {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(buf);
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.pending.drain(..=end).collect();
            self.emit(&line[..line.len() - 1]);
        }
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

impl Drop for PrintSink {
    fn drop(&mut self) {
        if !self.pending.is_empty() {
            let rest = std::mem::take(&mut self.pending);
            self.emit(&rest);
        }
    }
}

unsafe fn text<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

impl Shared {
    fn fail(&self, message: impl Into<String>) -> c_int {
        *self.last_error.borrow_mut() =
            CString::new(message.into().replace('\0', " ")).unwrap_or_default();
        -1
    }

    /// A VM for `program`, wired to this engine's print and host functions.
    fn start(&self, state: &mut State, program: vm::Program) -> c_int {
        let sink = PrintSink {
            target: self.print.clone(),
            pending: Vec::new(),
        };
        let mut machine = Vm::with_output(program, Box::new(sink));
        let table = self.host_fns.clone();
        let host_error = self.host_error.clone();
        machine.set_host(Rc::new(move |name: &str, args: &str| {
            let entry = table.borrow().get(name).copied();
            let Some((func, user)) = entry else {
                let mut known: Vec<String> = table.borrow().keys().cloned().collect();
                known.sort();
                return Err(format!(
                    "the host has no function `{name}` (it has: {})",
                    known.join(", ")
                ));
            };
            *host_error.borrow_mut() = None;
            let c_args = CString::new(args).unwrap_or_default();
            let answer = func(user, c_args.as_ptr());
            if let Some(e) = host_error.borrow_mut().take() {
                return Err(e);
            }
            if answer.is_null() {
                return Ok(String::new());
            }
            // SAFETY: the host promises a valid string until it returns.
            Ok(unsafe { CStr::from_ptr(answer) }
                .to_string_lossy()
                .into_owned())
        }));
        // Evaluate the script's constants now, so their mistakes show at
        // load time.
        if let Err(e) = machine.init() {
            return self.fail(e.to_string());
        }
        state.vm = Some(machine);
        0
    }
}

fn diagnostics_text(d: &[vyrn::Diagnostic]) -> String {
    d.iter()
        .map(|d| {
            let file = d.file.clone().unwrap_or_else(|| "script".into());
            let hint = d.hint.as_ref().map_or(String::new(), |h| format!(" ({h})"));
            format!(
                "{file}:{}:{}: {}{hint}",
                d.span.start.line, d.span.start.column, d.message
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Runs `f` on the engine's VM, turning a panic into an error, and refusing
/// a call made from inside one already running on this engine.
unsafe fn guarded<T>(
    engine: *mut VyrnEngine,
    failed: T,
    f: impl FnOnce(&Shared, &mut State) -> T,
) -> T {
    let Some(handle) = engine.as_ref() else {
        return failed;
    };
    let shared = &handle.shared;
    if shared.busy.get() {
        shared.fail(
            "this engine is already running a script function; calls back into it from a host \
             function or print callback are not allowed (queue the work and do it afterwards)",
        );
        return failed;
    }
    shared.busy.set(true);
    let state = &mut *handle.state;
    let result = catch_unwind(AssertUnwindSafe(|| f(shared, state)));
    shared.busy.set(false);
    match result {
        Ok(v) => v,
        Err(_) => {
            shared.fail(
                "Vyrn hit an internal error; please report it with the script that caused it",
            );
            failed
        }
    }
}

#[no_mangle]
pub extern "C" fn vyrn_new() -> *mut VyrnEngine {
    Box::into_raw(Box::new(VyrnEngine {
        shared: Shared {
            last_error: RefCell::new(CString::default()),
            print: Rc::new(RefCell::new(None)),
            host_fns: Rc::new(RefCell::new(HashMap::new())),
            host_error: Rc::new(RefCell::new(None)),
            busy: Cell::new(false),
        },
        state: Box::into_raw(Box::new(State { vm: None })),
    }))
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_free(engine: *mut VyrnEngine) {
    let Some(handle) = engine.as_ref() else {
        return;
    };
    if handle.shared.busy.get() {
        // Freed from inside its own call: too late to stop it safely.
        handle
            .shared
            .fail("vyrn_free was called while the engine was running");
        return;
    }
    let handle = Box::from_raw(engine);
    drop(Box::from_raw(handle.state));
}

#[no_mangle]
pub extern "C" fn vyrn_version() -> *const c_char {
    concat!(env!("CARGO_PKG_VERSION"), "\0").as_ptr().cast()
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_load_file(engine: *mut VyrnEngine, path: *const c_char) -> c_int {
    guarded(engine, -1, |e, state| {
        let Some(path) = text(path) else {
            return e.fail("vyrn_load_file needs a path");
        };
        let fetcher = vyrn::project::default_fetcher();
        let loaded = match vyrn::project::load_program(
            std::path::Path::new(path),
            fetcher.as_ref(),
            false,
        ) {
            Ok(l) => l,
            Err((d, _)) => return e.fail(diagnostics_text(&d)),
        };
        match vm::compile_checked(&loaded.module) {
            Ok(program) => e.start(state, program),
            Err(mut d) => {
                vyrn::project::loader::attach_files(&mut d, &loaded.files);
                e.fail(diagnostics_text(&d))
            }
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_load_source(
    engine: *mut VyrnEngine,
    name: *const c_char,
    source: *const c_char,
) -> c_int {
    guarded(engine, -1, |e, state| {
        let name = text(name).unwrap_or("script");
        let Some(source) = text(source) else {
            return e.fail("vyrn_load_source needs the script's text, in UTF-8");
        };
        match vm::compile_source(source) {
            Ok(program) => e.start(state, program),
            Err(mut d) => {
                for x in &mut d {
                    x.file = Some(name.to_string());
                }
                e.fail(diagnostics_text(&d))
            }
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_has_function(engine: *mut VyrnEngine, name: *const c_char) -> c_int {
    guarded(engine, 0, |_, state| {
        let (Some(vm), Some(name)) = (state.vm.as_ref(), text(name)) else {
            return 0;
        };
        let program = vm.program();
        c_int::from(program.global(name).is_some() && program.main.is_none_or(|_| name != "main"))
    })
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_call(
    engine: *mut VyrnEngine,
    name: *const c_char,
    args_json: *const c_char,
) -> *mut c_char {
    guarded(engine, std::ptr::null_mut(), |e, state| {
        let Some(name) = text(name) else {
            e.fail("vyrn_call needs a function name");
            return std::ptr::null_mut();
        };
        let args = match text(args_json).map(str::trim) {
            None | Some("") => Vec::new(),
            Some(json) => match vm::json::parse(json) {
                Ok(parsed) => match ops::from_json(&parsed) {
                    vm::value::Value::List(items) => items.borrow().clone(),
                    single => vec![single],
                },
                Err(why) => {
                    e.fail(format!("the arguments to `{name}` are not JSON: {why}"));
                    return std::ptr::null_mut();
                }
            },
        };
        let Some(machine) = state.vm.as_mut() else {
            e.fail("no script is loaded");
            return std::ptr::null_mut();
        };
        match machine.call_named(name, args) {
            Ok(result) => {
                let json = ops::to_json(machine.program(), &result);
                CString::new(json).map_or(std::ptr::null_mut(), CString::into_raw)
            }
            Err(err) => {
                e.fail(format!("{err}"));
                std::ptr::null_mut()
            }
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_run_main(engine: *mut VyrnEngine) -> c_int {
    guarded(engine, -1, |e, state| {
        let Some(machine) = state.vm.as_mut() else {
            return e.fail("no script is loaded");
        };
        match machine.run_main(&[]) {
            Ok(_) => 0,
            Err(err) => e.fail(format!("{err}")),
        }
    })
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_last_error(engine: *mut VyrnEngine) -> *const c_char {
    match engine.as_ref() {
        // The CString's buffer stays put until the next error replaces it.
        Some(e) => e.shared.last_error.borrow().as_ptr(),
        None => c"no engine".as_ptr(),
    }
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_string_free(text: *mut c_char) {
    if !text.is_null() {
        drop(CString::from_raw(text));
    }
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_set_print(
    engine: *mut VyrnEngine,
    print: Option<VyrnPrintFn>,
    user: *mut c_void,
) {
    if let Some(e) = engine.as_ref() {
        if let Ok(mut target) = e.shared.print.try_borrow_mut() {
            *target = print.map(|p| (p, user));
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_register(
    engine: *mut VyrnEngine,
    name: *const c_char,
    func: Option<VyrnHostFn>,
    user: *mut c_void,
) {
    let (Some(e), Some(name)) = (engine.as_ref(), text(name)) else {
        return;
    };
    let Ok(mut table) = e.shared.host_fns.try_borrow_mut() else {
        return;
    };
    match func {
        Some(f) => {
            table.insert(name.to_string(), (f, user));
        }
        None => {
            table.remove(name);
        }
    }
}

#[no_mangle]
pub unsafe extern "C" fn vyrn_host_error(engine: *mut VyrnEngine, message: *const c_char) {
    if let Some(e) = engine.as_ref() {
        *e.shared.host_error.borrow_mut() = Some(
            text(message)
                .unwrap_or("the host function failed")
                .to_string(),
        );
    }
}
