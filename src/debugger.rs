//! `vyrn debug`: stopping a program to look inside it.
//!
//! Two front ends share the VM's debugger hook (`vm::debug`):
//!
//! - **the terminal**: `vyrn debug game.vyrn` stops at the first line;
//!   type `next`, `step`, `break 12`, `print player.x`, `continue`...
//! - **the Debug Adapter Protocol**: `vyrn debug --dap`, spoken over
//!   standard input and output, which VS Code (and other editors) use to
//!   show breakpoints, the call stack and variables in the editor.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;

use crate::project::{self, loader, SourceFile};
use crate::vm::debug::{self, DebugHost, Reason, Resume};
use crate::vm::json::{self, Json};
use crate::vm::machine::Vm;
use crate::vm::value::Value;

/// Loads and compiles a program, or gives its errors as text.
fn compile(entry: &Path) -> Result<(crate::vm::Program, Vec<SourceFile>), String> {
    let fetcher = project::default_fetcher();
    let render = |d: &[crate::Diagnostic]| -> String {
        d.iter()
            .map(|d| {
                let file = d
                    .file
                    .clone()
                    .unwrap_or_else(|| entry.display().to_string());
                format!(
                    "{file}:{}:{}: {}",
                    d.span.start.line, d.span.start.column, d.message
                )
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    let loaded =
        project::load_program(entry, fetcher.as_ref(), false).map_err(|(d, _)| render(&d))?;
    match crate::vm::compile_checked(&loaded.module) {
        Ok(program) => Ok((program, loaded.files)),
        Err(mut d) => {
            loader::attach_files(&mut d, &loaded.files);
            Err(render(&d))
        }
    }
}

fn same_file(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

fn source_line(files: &[SourceFile], file: u32, line: u32) -> String {
    files
        .get(file as usize)
        .and_then(|f| f.text.lines().nth(line.saturating_sub(1) as usize))
        .unwrap_or("")
        .to_string()
}

fn file_name(files: &[SourceFile], file: u32) -> String {
    files
        .get(file as usize)
        .map(|f| {
            f.path
                .file_name()
                .map_or(f.path.display().to_string(), |n| n.to_string_lossy().into())
        })
        .unwrap_or_else(|| "?".into())
}

/// Writes into a shared sink, so the program's output and the debugger's
/// come out in order.
struct Shared(Rc<RefCell<Box<dyn Write>>>);

impl Write for Shared {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().write(buf)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.0.borrow_mut().flush()
    }
}

// ==== the terminal ===========================================================

const TERMINAL_HELP: &str = "\
  next (n)          run to the next line, stepping over calls
  step (s)          run to the next line, going into calls
  out (o)           run until this function returns
  continue (c)      run to the next breakpoint
  break 12 (b)      stop at line 12 (or `break other.vyrn:12`)
  delete 12         remove that breakpoint
  print x (p)       show a variable: `p player`, `p player.pos.x`, `p items[0]`
  vars (v)          every variable here
  where (w)         the calls that led here
  list (l)          the code around this line
  quit (q)          stop the program
  Enter             repeat the last command";

struct Terminal {
    files: Rc<Vec<SourceFile>>,
    breakpoints: HashSet<(u32, u32)>,
    input: Box<dyn BufRead>,
    out: Rc<RefCell<Box<dyn Write>>>,
    last: String,
}

impl Terminal {
    fn say(&self, text: &str) {
        let mut out = self.out.borrow_mut();
        let _ = writeln!(out, "{text}");
        let _ = out.flush();
    }

    /// `12` or `file.vyrn:12` to (file, line).
    fn place(&self, text: &str, here: u32) -> Option<(u32, u32)> {
        match text.rsplit_once(':') {
            Some((name, line)) => {
                let line = line.trim().parse().ok()?;
                let file = self.files.iter().position(|f| {
                    f.path.ends_with(name.trim())
                        || f.path.file_name().is_some_and(|n| n == name.trim())
                })?;
                Some((file as u32, line))
            }
            None => Some((here, text.trim().parse().ok()?)),
        }
    }

    fn show_value(&self, vm: &Vm, name: &str, value: &Value) -> String {
        let children = vm.debug_children(value);
        if children.is_empty() || children.len() > 20 {
            return format!("{name} = {}", vm.display_repr(value));
        }
        let mut out = format!("{name} = {}", vm.display_repr(value));
        for (k, v) in children {
            out.push_str(&format!("\n    {k}: {}", vm.display_repr(&v)));
        }
        out
    }
}

impl DebugHost for Terminal {
    fn is_breakpoint(&mut self, file: u32, line: u32) -> bool {
        self.breakpoints.contains(&(file, line))
    }

    fn stopped(&mut self, vm: &Vm, reason: Reason) -> Resume {
        let stack = vm.debug_stack();
        let Some(top) = stack.first().cloned() else {
            return Resume::Continue;
        };
        let why = match reason {
            Reason::Breakpoint => "breakpoint, ",
            _ => "",
        };
        self.say(&format!(
            "-> {why}{}:{} in {}\n   {} | {}",
            file_name(&self.files, top.file),
            top.line,
            top.function,
            top.line,
            source_line(&self.files, top.file, top.line).trim_end()
        ));
        loop {
            {
                let mut out = self.out.borrow_mut();
                let _ = write!(out, "(vyrn) ");
                let _ = out.flush();
            }
            let mut line = String::new();
            match self.input.read_line(&mut line) {
                // The end of the input: run the rest of the program.
                Ok(0) | Err(_) => {
                    self.breakpoints.clear();
                    return Resume::Continue;
                }
                Ok(_) => {}
            }
            let mut line = line.trim().to_string();
            if line.is_empty() {
                line = self.last.clone();
            }
            self.last = line.clone();
            let (command, rest) = line.split_once(' ').unwrap_or((&line, ""));
            let rest = rest.trim();
            match command {
                "n" | "next" | "" => return Resume::StepOver,
                "s" | "step" => return Resume::StepIn,
                "o" | "out" | "finish" => return Resume::StepOut,
                "c" | "continue" => return Resume::Continue,
                "q" | "quit" | "exit" => return Resume::Terminate,
                "b" | "break" => match self.place(rest, top.file) {
                    Some(p) => {
                        self.breakpoints.insert(p);
                        self.say(&format!(
                            "breakpoint at {}:{}",
                            file_name(&self.files, p.0),
                            p.1
                        ));
                    }
                    None => self.say("which line? `break 12` or `break file.vyrn:12`"),
                },
                "d" | "delete" => match self.place(rest, top.file) {
                    Some(p) if self.breakpoints.remove(&p) => self.say("removed"),
                    _ => self.say("there is no breakpoint there"),
                },
                "p" | "print" => match debug::lookup(vm, 0, rest) {
                    Some(v) => self.say(&self.show_value(vm, rest, &v)),
                    None => self.say(&format!("there is no `{rest}` here")),
                },
                "v" | "vars" | "locals" => {
                    let locals = vm.debug_locals(0);
                    if locals.is_empty() {
                        self.say("no variables yet");
                    }
                    for (n, v) in locals {
                        self.say(&format!("{n} = {}", vm.display_repr(&v)));
                    }
                }
                "w" | "where" | "bt" => {
                    for (i, f) in stack.iter().enumerate() {
                        self.say(&format!(
                            "{} {} at {}:{}",
                            if i == 0 { "->" } else { "  " },
                            f.function,
                            file_name(&self.files, f.file),
                            f.line
                        ));
                    }
                }
                "l" | "list" => {
                    let from = top.line.saturating_sub(3).max(1);
                    for n in from..top.line + 4 {
                        if let Some(text) = self
                            .files
                            .get(top.file as usize)
                            .and_then(|f| f.text.lines().nth(n as usize - 1))
                        {
                            let mark = if n == top.line { "->" } else { "  " };
                            self.say(&format!("{mark} {n:>3} | {text}"));
                        }
                    }
                }
                "h" | "help" | "?" => self.say(TERMINAL_HELP),
                other => self.say(&format!("`{other}` is not a debugger command; type `help`")),
            }
        }
    }
}

/// `vyrn debug file.vyrn`: runs the program in the terminal debugger,
/// stopped at its first line. Gives the process exit code.
pub fn run_terminal(
    entry: &Path,
    args: &[String],
    input: Box<dyn BufRead>,
    out: Box<dyn Write>,
) -> i32 {
    let out = Rc::new(RefCell::new(out));
    let say = |text: &str| {
        let _ = writeln!(out.borrow_mut(), "{text}");
    };
    let (program, files) = match compile(entry) {
        Ok(p) => p,
        Err(e) => {
            say(&e);
            return 1;
        }
    };
    say("Debugging. Type `help` for the commands.");
    crate::vm::stdlib::set_args(args.to_vec());
    let files = Rc::new(files);
    let mut machine = Vm::with_output(program, Box::new(Shared(out.clone())));
    let host = Terminal {
        files: files.clone(),
        breakpoints: HashSet::new(),
        input,
        out: out.clone(),
        last: "next".into(),
    };
    machine.attach_debugger(Box::new(host), files.len() as u32, true);
    let code = match machine.run_main(args) {
        Ok(_) => {
            say("The program finished.");
            0
        }
        Err(e) if e.kind == "Stopped" => {
            say("Stopped.");
            0
        }
        Err(e) => {
            let place = e
                .trace
                .iter()
                .find(|t| (t.file as usize) < files.len() && t.line > 0)
                .map(|t| format!(" at {}:{}", file_name(&files, t.file), t.line))
                .unwrap_or_default();
            say(&format!("error{place}: {}: {}", e.kind, e.message));
            1
        }
    };
    let _ = out.borrow_mut().flush();
    code
}

// ==== the Debug Adapter Protocol =============================================

/// One message to the editor, as JSON text.
fn quote(text: &str) -> String {
    let mut out = String::new();
    json::write_string(text, &mut out);
    out
}

/// Sends protocol messages, numbered, to the editor.
#[derive(Clone)]
struct Wire {
    out: std::sync::Arc<std::sync::Mutex<Box<dyn Write + Send>>>,
    seq: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

impl Wire {
    fn send(&self, kind_and_body: &str) {
        let seq = self.seq.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
        let message = format!("{{\"seq\":{seq},{kind_and_body}}}");
        if let Ok(mut out) = self.out.lock() {
            let _ = write!(out, "Content-Length: {}\r\n\r\n{message}", message.len());
            let _ = out.flush();
        }
    }

    fn respond(&self, request: &Json, body: &str) {
        let command = request.get("command").and_then(Json::as_str).unwrap_or("");
        let seq = request.get("seq").and_then(Json::as_u64).unwrap_or(0);
        self.send(&format!(
            "\"type\":\"response\",\"request_seq\":{seq},\"success\":true,\"command\":{},\"body\":{body}",
            quote(command)
        ));
    }

    fn fail(&self, request: &Json, message: &str) {
        let command = request.get("command").and_then(Json::as_str).unwrap_or("");
        let seq = request.get("seq").and_then(Json::as_u64).unwrap_or(0);
        self.send(&format!(
            "\"type\":\"response\",\"request_seq\":{seq},\"success\":false,\"command\":{},\"message\":{}",
            quote(command),
            quote(message)
        ));
    }

    fn event(&self, name: &str, body: &str) {
        self.send(&format!(
            "\"type\":\"event\",\"event\":{},\"body\":{body}",
            quote(name)
        ));
    }

    fn output(&self, category: &str, text: &str) {
        self.event(
            "output",
            &format!(
                "{{\"category\":{},\"output\":{}}}",
                quote(category),
                quote(text)
            ),
        );
    }
}

/// The program's `print`, sent to the editor's debug console.
struct ConsoleOut {
    wire: Wire,
    pending: Vec<u8>,
}

impl Write for ConsoleOut {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.pending.extend_from_slice(buf);
        if let Some(end) = self.pending.iter().rposition(|&b| b == b'\n') {
            let text: Vec<u8> = self.pending.drain(..=end).collect();
            self.wire.output("stdout", &String::from_utf8_lossy(&text));
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        if !self.pending.is_empty() {
            let text = std::mem::take(&mut self.pending);
            self.wire.output("stdout", &String::from_utf8_lossy(&text));
        }
        Ok(())
    }
}

impl Drop for ConsoleOut {
    fn drop(&mut self) {
        let _ = self.flush();
    }
}

/// Reads protocol messages from the editor.
fn read_message(input: &mut dyn BufRead) -> Option<Json> {
    loop {
        let mut length = None;
        loop {
            let mut header = String::new();
            if input.read_line(&mut header).ok()? == 0 {
                return None;
            }
            let header = header.trim_end();
            if header.is_empty() {
                break;
            }
            if let Some(v) = header.strip_prefix("Content-Length:") {
                length = v.trim().parse::<usize>().ok();
            }
        }
        let Some(length) = length else { continue };
        let mut body = vec![0u8; length];
        input.read_exact(&mut body).ok()?;
        if let Ok(message) = json::parse(&String::from_utf8_lossy(&body)) {
            return Some(message);
        }
    }
}

/// What the editor set: breakpoints by file path.
#[derive(Default)]
struct Settings {
    breakpoints: HashMap<PathBuf, Vec<u32>>,
}

impl Settings {
    /// Answers `setBreakpoints`; every line can hold one.
    fn set_breakpoints(&mut self, wire: &Wire, request: &Json) {
        let args = request.get("arguments");
        let path = args
            .and_then(|a| a.get("source"))
            .and_then(|s| s.get("path"))
            .and_then(Json::as_str)
            .unwrap_or("");
        let lines: Vec<u32> = args
            .and_then(|a| a.get("breakpoints"))
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|b| b.get("line").and_then(Json::as_u64))
            .map(|l| l as u32)
            .collect();
        let shown: Vec<String> = lines
            .iter()
            .map(|l| format!("{{\"verified\":true,\"line\":{l}}}"))
            .collect();
        self.breakpoints.insert(PathBuf::from(path), lines);
        wire.respond(
            request,
            &format!("{{\"breakpoints\":[{}]}}", shown.join(",")),
        );
    }
}

/// The debugger while the program runs: answers the editor at each stop.
struct Dap {
    wire: Wire,
    requests: mpsc::Receiver<Json>,
    settings: Settings,
    files: Rc<Vec<SourceFile>>,
    /// (file, line) pairs with a breakpoint, rebuilt when the editor
    /// changes them.
    active: HashSet<(u32, u32)>,
    pause: bool,
    /// Values the editor can expand, by `variablesReference`, for this stop.
    handles: Vec<Value>,
    /// Requests that arrived while running, to answer at the next stop.
    queued: VecDeque<Json>,
    terminated: bool,
}

/// `variablesReference`s: globals, then one per stack frame's locals, then
/// one per value that can be expanded.
const GLOBALS_REF: u64 = 1;
const LOCALS_REF: u64 = 10;
const FIRST_VALUE_REF: u64 = 100_000;

impl Dap {
    fn rebuild_breakpoints(&mut self) {
        self.active.clear();
        for (path, lines) in &self.settings.breakpoints {
            if let Some(file) = self.files.iter().position(|f| same_file(&f.path, path)) {
                for line in lines {
                    self.active.insert((file as u32, *line));
                }
            }
        }
    }

    /// A variable for the editor, with a handle if it can be expanded.
    fn variable(&mut self, vm: &Vm, name: &str, value: &Value) -> String {
        let children = vm.debug_children(value);
        let reference = if children.is_empty() {
            0
        } else {
            self.handles.push(value.clone());
            FIRST_VALUE_REF + self.handles.len() as u64 - 1
        };
        format!(
            "{{\"name\":{},\"value\":{},\"type\":{},\"variablesReference\":{reference}}}",
            quote(name),
            quote(&vm.display_repr(value)),
            quote(value.type_name())
        )
    }

    /// Handles one request at a stop; gives how to go on, if it says.
    fn at_stop(&mut self, vm: &Vm, request: &Json) -> Option<Resume> {
        let command = request.get("command").and_then(Json::as_str).unwrap_or("");
        let args = request.get("arguments");
        match command {
            "threads" => self
                .wire
                .respond(request, "{\"threads\":[{\"id\":1,\"name\":\"main\"}]}"),
            "stackTrace" => {
                let frames: Vec<String> = vm
                    .debug_stack()
                    .iter()
                    .enumerate()
                    .map(|(i, f)| {
                        let path = self
                            .files
                            .get(f.file as usize)
                            .map(|s| s.path.display().to_string())
                            .unwrap_or_default();
                        format!(
                            "{{\"id\":{i},\"name\":{},\"line\":{},\"column\":1,\"source\":{{\"name\":{},\"path\":{}}}}}",
                            quote(&f.function),
                            f.line,
                            quote(&file_name(&self.files, f.file)),
                            quote(&path)
                        )
                    })
                    .collect();
                self.wire.respond(
                    request,
                    &format!(
                        "{{\"stackFrames\":[{}],\"totalFrames\":{}}}",
                        frames.join(","),
                        frames.len()
                    ),
                );
            }
            "scopes" => {
                let frame = args
                    .and_then(|a| a.get("frameId"))
                    .and_then(Json::as_u64)
                    .unwrap_or(0);
                self.wire.respond(
                    request,
                    &format!(
                        "{{\"scopes\":[{{\"name\":\"Locals\",\"variablesReference\":{},\"expensive\":false}},{{\"name\":\"Globals\",\"variablesReference\":{GLOBALS_REF},\"expensive\":false}}]}}",
                        LOCALS_REF + frame
                    ),
                );
            }
            "variables" => {
                let reference = args
                    .and_then(|a| a.get("variablesReference"))
                    .and_then(Json::as_u64)
                    .unwrap_or(0);
                let pairs: Vec<(String, Value)> = if reference == GLOBALS_REF {
                    vm.debug_globals()
                } else if reference >= FIRST_VALUE_REF {
                    match self.handles.get((reference - FIRST_VALUE_REF) as usize) {
                        Some(v) => vm.debug_children(&v.clone()),
                        None => Vec::new(),
                    }
                } else if reference >= LOCALS_REF {
                    vm.debug_locals((reference - LOCALS_REF) as usize)
                } else {
                    Vec::new()
                };
                let shown: Vec<String> =
                    pairs.iter().map(|(n, v)| self.variable(vm, n, v)).collect();
                self.wire
                    .respond(request, &format!("{{\"variables\":[{}]}}", shown.join(",")));
            }
            "evaluate" => {
                let expression = args
                    .and_then(|a| a.get("expression"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let frame = args
                    .and_then(|a| a.get("frameId"))
                    .and_then(Json::as_u64)
                    .unwrap_or(0);
                match debug::lookup(vm, frame as usize, expression) {
                    Some(value) => {
                        let reference = if vm.debug_children(&value).is_empty() {
                            0
                        } else {
                            self.handles.push(value.clone());
                            FIRST_VALUE_REF + self.handles.len() as u64 - 1
                        };
                        self.wire.respond(
                            request,
                            &format!(
                                "{{\"result\":{},\"type\":{},\"variablesReference\":{reference}}}",
                                quote(&vm.display_repr(&value)),
                                quote(value.type_name())
                            ),
                        );
                    }
                    None => self
                        .wire
                        .fail(request, &format!("there is no `{expression}` here")),
                }
            }
            "setBreakpoints" => {
                self.settings.set_breakpoints(&self.wire, request);
                self.rebuild_breakpoints();
            }
            "continue" => {
                self.wire.respond(request, "{\"allThreadsContinued\":true}");
                return Some(Resume::Continue);
            }
            "next" => {
                self.wire.respond(request, "{}");
                return Some(Resume::StepOver);
            }
            "stepIn" => {
                self.wire.respond(request, "{}");
                return Some(Resume::StepIn);
            }
            "stepOut" => {
                self.wire.respond(request, "{}");
                return Some(Resume::StepOut);
            }
            "pause" => self.wire.respond(request, "{}"),
            "disconnect" | "terminate" => {
                self.wire.respond(request, "{}");
                self.terminated = true;
                return Some(Resume::Terminate);
            }
            _ => self.wire.respond(request, "{}"),
        }
        None
    }

    /// Requests that arrive while the program runs: breakpoints and pause
    /// are handled at once, the rest wait for the next stop.
    fn drain_running(&mut self) {
        while let Ok(request) = self.requests.try_recv() {
            match request.get("command").and_then(Json::as_str).unwrap_or("") {
                "setBreakpoints" => {
                    self.settings.set_breakpoints(&self.wire, &request);
                    self.rebuild_breakpoints();
                }
                "pause" => {
                    self.wire.respond(&request, "{}");
                    self.pause = true;
                }
                "disconnect" | "terminate" => {
                    self.wire.respond(&request, "{}");
                    self.terminated = true;
                }
                "threads" => self
                    .wire
                    .respond(&request, "{\"threads\":[{\"id\":1,\"name\":\"main\"}]}"),
                _ => self.queued.push_back(request),
            }
        }
    }
}

impl DebugHost for Dap {
    fn is_breakpoint(&mut self, file: u32, line: u32) -> bool {
        self.active.contains(&(file, line))
    }

    fn pause_requested(&mut self) -> bool {
        self.drain_running();
        std::mem::take(&mut self.pause) || self.terminated
    }

    fn stopped(&mut self, vm: &Vm, reason: Reason) -> Resume {
        if self.terminated {
            return Resume::Terminate;
        }
        self.handles.clear();
        self.wire.event(
            "stopped",
            &format!(
                "{{\"reason\":{},\"threadId\":1,\"allThreadsStopped\":true}}",
                quote(reason.as_str())
            ),
        );
        loop {
            let request = match self.queued.pop_front() {
                Some(r) => r,
                None => match self.requests.recv() {
                    Ok(r) => r,
                    // The editor went away.
                    Err(_) => return Resume::Terminate,
                },
            };
            if let Some(how) = self.at_stop(vm, &request) {
                return how;
            }
        }
    }
}

/// `vyrn debug --dap`: the Debug Adapter Protocol over `input` and
/// `output` (standard input and output when run by an editor).
pub fn run_dap(input: Box<dyn BufRead + Send>, output: Box<dyn Write + Send>) {
    let wire = Wire {
        out: std::sync::Arc::new(std::sync::Mutex::new(output)),
        seq: Default::default(),
    };
    // A thread reads the editor's messages, so pause works while the
    // program runs.
    let (send, requests) = mpsc::channel::<Json>();
    std::thread::spawn(move || {
        let mut input = input;
        while let Some(message) = read_message(&mut *input) {
            if send.send(message).is_err() {
                return;
            }
        }
    });

    // Before the program starts: initialize, launch, breakpoints.
    let mut settings = Settings::default();
    let mut launch: Option<(PathBuf, bool, Vec<String>)> = None;
    while let Ok(request) = requests.recv() {
        let command = request.get("command").and_then(Json::as_str).unwrap_or("");
        let args = request.get("arguments");
        match command {
            "initialize" => {
                wire.respond(
                    &request,
                    "{\"supportsConfigurationDoneRequest\":true,\"supportsEvaluateForHovers\":true,\"supportsTerminateRequest\":true}",
                );
                wire.event("initialized", "{}");
            }
            "launch" => {
                let program = args
                    .and_then(|a| a.get("program"))
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let stop = args
                    .and_then(|a| a.get("stopOnEntry"))
                    .and_then(Json::as_bool)
                    .unwrap_or(false);
                let program_args = args
                    .and_then(|a| a.get("args"))
                    .and_then(Json::as_array)
                    .unwrap_or(&[])
                    .iter()
                    .filter_map(|a| a.as_str().map(str::to_string))
                    .collect();
                launch = Some((PathBuf::from(program), stop, program_args));
                wire.respond(&request, "{}");
            }
            "setBreakpoints" => settings.set_breakpoints(&wire, &request),
            "setExceptionBreakpoints" => wire.respond(&request, "{\"breakpoints\":[]}"),
            "threads" => wire.respond(&request, "{\"threads\":[{\"id\":1,\"name\":\"main\"}]}"),
            "configurationDone" => {
                wire.respond(&request, "{}");
                break;
            }
            "disconnect" | "terminate" => {
                wire.respond(&request, "{}");
                return;
            }
            _ => wire.respond(&request, "{}"),
        }
    }
    let Some((program_path, stop_on_entry, program_args)) = launch else {
        wire.output(
            "stderr",
            "nothing to debug: the launch request named no program\n",
        );
        wire.event("terminated", "{}");
        return;
    };

    let (program, files) = match compile(&program_path) {
        Ok(p) => p,
        Err(e) => {
            wire.output("stderr", &format!("{e}\n"));
            wire.event("exited", "{\"exitCode\":1}");
            wire.event("terminated", "{}");
            return;
        }
    };
    crate::vm::stdlib::set_args(program_args.clone());
    let files = Rc::new(files);
    let console = ConsoleOut {
        wire: wire.clone(),
        pending: Vec::new(),
    };
    let mut machine = Vm::with_output(program, Box::new(console));
    let mut host = Dap {
        wire: wire.clone(),
        requests,
        settings,
        files: files.clone(),
        active: HashSet::new(),
        pause: false,
        handles: Vec::new(),
        queued: VecDeque::new(),
        terminated: false,
    };
    host.rebuild_breakpoints();
    machine.attach_debugger(Box::new(host), files.len() as u32, stop_on_entry);
    let result = machine.run_main(&program_args);
    drop(machine);
    let code = match result {
        Ok(_) => 0,
        Err(e) if e.kind == "Stopped" => 0,
        Err(e) => {
            let place = e
                .trace
                .iter()
                .find(|t| (t.file as usize) < files.len() && t.line > 0)
                .map(|t| format!(" at {}:{}", file_name(&files, t.file), t.line))
                .unwrap_or_default();
            wire.output(
                "stderr",
                &format!("error{place}: {}: {}\n", e.kind, e.message),
            );
            1
        }
    };
    wire.event("exited", &format!("{{\"exitCode\":{code}}}"));
    wire.event("terminated", "{}");
}
