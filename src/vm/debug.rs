//! Stopping a running program to look at it: breakpoints, stepping, and
//! the stack and variables at a stop.
//!
//! The VM checks, before each instruction, whether it has reached a new
//! source line, but only while a debugger is attached (one `bool` test
//! otherwise). At a new line it asks the [`DebugHost`] (the terminal
//! debugger, or the Debug Adapter Protocol server for VS Code) whether to
//! stop; when stopped, the host inspects the VM and says how to go on.

use super::machine::Vm;
use super::value::Value;

/// Why the program stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// At the first line, when asked to stop on entry.
    Entry,
    Breakpoint,
    /// A step finished.
    Step,
    /// The user asked to pause.
    Pause,
}

impl Reason {
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::Entry => "entry",
            Reason::Breakpoint => "breakpoint",
            Reason::Step => "step",
            Reason::Pause => "pause",
        }
    }
}

/// How to go on after a stop.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Resume {
    /// Run to the next breakpoint.
    Continue,
    /// Stop at the next line, even inside a function it calls.
    StepIn,
    /// Stop at the next line of this function (or its caller).
    StepOver,
    /// Stop once this function returns.
    StepOut,
    /// End the program.
    Terminate,
}

/// What drives the debugger: decides where to stop, and what to do at a stop.
pub trait DebugHost {
    /// Whether there is a breakpoint at this line of this file.
    fn is_breakpoint(&mut self, file: u32, line: u32) -> bool;
    /// Whether the user asked to pause (checked at each new line).
    fn pause_requested(&mut self) -> bool {
        false
    }
    /// The program stopped: inspect `vm` and say how to go on.
    fn stopped(&mut self, vm: &Vm, reason: Reason) -> Resume;
}

/// One function call on the stack at a stop, innermost first.
#[derive(Clone, Debug)]
pub struct DebugFrame {
    pub function: String,
    pub file: u32,
    pub line: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Run,
    StepIn,
    /// Stop at a new line at this depth or shallower.
    StepOver(usize),
    /// Stop at a new line shallower than this depth.
    StepOut(usize),
}

pub(crate) struct Session {
    pub host: Box<dyn DebugHost>,
    mode: Mode,
    /// Files below this number are the program's own; the Prelude and
    /// library code are never stopped in.
    user_files: u32,
    /// The line each frame depth was last seen at (function, line), so a
    /// line is "new" only when its own frame moves on, not when a call it
    /// made returns.
    seen: Vec<(usize, u32)>,
}

impl Session {
    pub fn new(host: Box<dyn DebugHost>, user_files: u32, stop_on_entry: bool) -> Session {
        Session {
            host,
            mode: if stop_on_entry {
                Mode::StepIn
            } else {
                Mode::Run
            },
            user_files,
            seen: Vec::new(),
        }
    }

    /// Whether to stop before an instruction on `line` of function `proto`
    /// (by address), in file `file`, `depth` frames deep.
    pub fn should_stop(
        &mut self,
        proto: usize,
        file: u32,
        line: u32,
        depth: usize,
    ) -> Option<Reason> {
        if line == 0 || file >= self.user_files || depth == 0 {
            return None;
        }
        self.seen.truncate(depth);
        if self.seen.len() < depth {
            self.seen.resize(depth, (0, 0));
        }
        if self.seen[depth - 1] == (proto, line) {
            return None;
        }
        self.seen[depth - 1] = (proto, line);
        if self.host.is_breakpoint(file, line) {
            return Some(Reason::Breakpoint);
        }
        if self.host.pause_requested() {
            return Some(Reason::Pause);
        }
        let stepped = match self.mode {
            Mode::Run => false,
            Mode::StepIn => true,
            Mode::StepOver(d) => depth <= d,
            Mode::StepOut(d) => depth < d,
        };
        stepped.then_some(Reason::Step)
    }

    /// Sets up the next stop after the host said how to go on from `depth`.
    pub fn resume(&mut self, how: Resume, depth: usize) {
        self.mode = match how {
            Resume::Continue | Resume::Terminate => Mode::Run,
            Resume::StepIn => Mode::StepIn,
            Resume::StepOver => Mode::StepOver(depth),
            Resume::StepOut => Mode::StepOut(depth),
        };
    }
}

/// A path into a value, as typed in a watch: `player.pos.x`, `items[2]`,
/// `scores["ann"]`. Gives the parts.
pub fn parse_path(text: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = text.trim().chars().peekable();
    while let Some(c) = chars.next() {
        match c {
            '.' => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                } else if parts.is_empty() {
                    return None;
                }
            }
            '[' => {
                if !current.is_empty() {
                    parts.push(std::mem::take(&mut current));
                }
                let mut inside = String::new();
                for c in chars.by_ref() {
                    if c == ']' {
                        break;
                    }
                    inside.push(c);
                }
                let key = inside
                    .trim()
                    .trim_matches('"')
                    .trim_matches('\'')
                    .to_string();
                if key.is_empty() {
                    return None;
                }
                parts.push(key);
            }
            c if c.is_alphanumeric() || c == '_' => current.push(c),
            c if c.is_whitespace() => {}
            _ => return None,
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    (!parts.is_empty()).then_some(parts)
}

/// Looks up a watch path such as `player.pos.x` among a frame's variables.
pub fn lookup(vm: &Vm, frame: usize, text: &str) -> Option<Value> {
    let parts = parse_path(text)?;
    let first = &parts[0];
    let mut value = vm
        .debug_locals(frame)
        .into_iter()
        .rev()
        .find(|(n, _)| n == first)
        .map(|(_, v)| v)
        .or_else(|| {
            vm.debug_globals()
                .into_iter()
                .find(|(n, _)| n == first)
                .map(|(_, v)| v)
        })?;
    for part in &parts[1..] {
        value = vm
            .debug_children(&value)
            .into_iter()
            .find(|(n, _)| n == part || n.trim_matches('"') == part)
            .map(|(_, v)| v)?;
    }
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::parse_path;

    #[test]
    fn watch_paths() {
        assert_eq!(parse_path("player.pos.x").unwrap(), ["player", "pos", "x"]);
        assert_eq!(parse_path("items[2]").unwrap(), ["items", "2"]);
        assert_eq!(parse_path("m[\"ann\"].age").unwrap(), ["m", "ann", "age"]);
        assert_eq!(
            parse_path("players[1].name").unwrap(),
            ["players", "1", "name"]
        );
        assert!(parse_path("a + b").is_none());
        assert!(parse_path("").is_none());
    }
}
