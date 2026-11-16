//! The interactive prompt behind `vyrn` (with no arguments) and
//! `vyrn repl`.
//!
//! Each entry is either declarations (`create`, `struct`, `object`, `enum`,
//! `use`, `const`, ...), which are kept for the rest of the session, or
//! statements, which run at once. Variables live in the session: a
//! statement entry is compiled into a function that loads every known
//! variable from the session's map, runs the entry, and stores the
//! variables back. An entry that fails keeps the changes it made to lists
//! and maps but not its assignments. A lone expression's value is shown,
//! as in Python.
//!
//! Every entry compiles the whole session afresh: the Prelude first, then
//! the declarations in the order they were entered, then the entry. Earlier
//! declarations keep their positions, so values made by earlier entries
//! (struct instances, functions stored in variables) stay valid.

use std::cell::RefCell;
use std::io::Write;
use std::rc::Rc;

use crate::ast::{ExprKind, ItemKind, Module, StmtKind};
use crate::diagnostic::Diagnostic;
use crate::span::{Pos, Span};
use crate::vm::value::{Key, Map, Value};
use crate::vm::{self, Vm};

/// Where the session's output goes: the terminal, or a buffer in tests.
pub enum Output {
    Stdout,
    Captured(vm::Captured),
}

pub struct Session {
    /// Declarations entered so far, as source text, in order.
    decls: Vec<String>,
    /// The session's variables, by name.
    vars: Rc<RefCell<Map>>,
    /// Variable names in the order they were first assigned.
    names: Vec<String>,
    /// The variables as they printed after the last entry.
    shown: Vec<(String, String)>,
    output: Output,
}

/// What came of one entry.
pub enum Outcome {
    /// Ran; the text is a value to show (for a lone expression).
    Done(Option<String>),
    /// The entry is an open block: read more lines and try again.
    NeedsMore,
    /// Errors, already rendered for the terminal.
    Failed(String),
}

const DECLARATION_WORDS: &[&str] = &[
    "create", "struct", "object", "enum", "use", "const", "trait", "impl", "type", "pub", "export",
    "extern", "route", "routes", "strict", "kernel", "@",
];

impl Default for Session {
    fn default() -> Self {
        Session::new(Output::Stdout)
    }
}

impl Session {
    pub fn new(output: Output) -> Session {
        Session {
            decls: Vec::new(),
            vars: Rc::new(RefCell::new(Map::default())),
            names: Vec::new(),
            shown: Vec::new(),
            output,
        }
    }

    /// The session's variables and how they print, for `:vars`.
    pub fn variables(&self) -> Vec<(String, String)> {
        self.shown.clone()
    }

    /// Runs one entry: one line, or several lines making up a block.
    pub fn eval(&mut self, input: &str) -> Outcome {
        let input = input.trim_end();
        if input.trim().is_empty() {
            return Outcome::Done(None);
        }
        let first_word = input
            .trim_start()
            .split(|c: char| !(c.is_alphanumeric() || c == '_' || c == '@'))
            .next()
            .unwrap_or("");
        let is_decl = DECLARATION_WORDS.contains(&first_word)
            || first_word.starts_with('@')
            || (first_word == "async"
                && input.trim_start()[5..].trim_start().starts_with("create"));
        // `create(x)` is a call, not a declaration.
        let is_decl = is_decl
            && !(first_word == "create"
                && !input.trim_start()[6..]
                    .trim_start()
                    .starts_with(|c: char| c.is_alphabetic() || c == '_'));
        if is_decl {
            self.declare(input)
        } else {
            self.run(input)
        }
    }

    fn declare(&mut self, input: &str) -> Outcome {
        let (module, errors) = crate::parser::parse_recovering(input);
        if !errors.is_empty() {
            return self.parse_failure(&errors, input, 0, 0);
        }
        // Check the whole session with this entry before keeping it.
        let mut decls = self.decls.clone();
        decls.push(input.to_string());
        let names: Vec<String> = crate::prelude::declared_names(&module)
            .into_iter()
            .collect();
        match self.compile(&decls, None) {
            Ok(_) => {
                self.decls = decls;
                let _ = names;
                Outcome::Done(None)
            }
            Err(errors) => Outcome::Failed(render_errors(
                &errors,
                &self.session_source(&decls),
                "session",
            )),
        }
    }

    fn run(&mut self, input: &str) -> Outcome {
        // Statements are parsed inside a function, so they are checked as
        // statements.
        let indented: String = input.lines().map(|l| format!("    {l}\n")).collect();
        let probe = format!("main:\n{indented}end\n");
        let (module, errors) = crate::parser::parse_recovering(&probe);
        if !errors.is_empty() {
            return self.parse_failure(&errors, input, 1, 4);
        }
        let Some(main) = module.main() else {
            return Outcome::Failed("error: could not read that entry".into());
        };
        let stmts = &main.body.stmts;

        // A lone expression is shown; `print(...)` and other calls that
        // give `none` show nothing.
        let shows_value = stmts.len() == 1 && matches!(stmts[0].kind, StmtKind::Expr(_));

        // Names this entry declares with `let`/`var` must not be preloaded,
        // or the declaration would collide with the preloaded variable.
        let mut declared = Vec::new();
        let mut assigned = Vec::new();
        for stmt in stmts {
            collect_names(stmt, &mut declared, &mut assigned);
        }
        let mut header = String::from("create __repl(__env):\n");
        let known: Vec<String> = self.names.clone();
        for name in &known {
            if !declared.contains(name) {
                header.push_str(&format!("    var {name} = __env[\"{name}\"]\n"));
            }
        }
        let mut all: Vec<String> = known.clone();
        for n in declared.iter().chain(&assigned) {
            if !all.contains(n) {
                all.push(n.clone());
            }
        }
        let header_lines = header.lines().count() as u32;
        let mut body = String::new();
        for (i, line) in input.lines().enumerate() {
            if i == 0 && shows_value {
                body.push_str(&format!("    __env[\"_\"] = {line}\n"));
            } else {
                body.push_str(&format!("    {line}\n"));
            }
        }
        for name in &all {
            body.push_str(&format!("    __env[\"{name}\"] = {name}\n"));
        }
        body.push_str("end\n");
        let entry = header + &body;

        let program = match self.compile(&self.decls, Some(&entry)) {
            Ok(p) => p,
            Err(errors) => {
                return Outcome::Failed(self.render_entry_errors(&errors, input, header_lines));
            }
        };
        let mut vm = match &self.output {
            Output::Stdout => Vm::new(program),
            Output::Captured(c) => Vm::with_output(program, Box::new(c.clone())),
        };
        let env = Value::Map(self.vars.clone());
        if shows_value {
            self.vars.borrow_mut().remove(&key("_"));
        }
        let result = vm.init().and_then(|_| vm.call_global("__repl", vec![env]));
        let _ = std::io::stdout().flush();
        // Variables first assigned in this entry join the session.
        for name in all {
            if !self.names.contains(&name) && self.vars.borrow().contains(&key(&name)) {
                self.names.push(name);
            }
        }
        self.shown = self
            .vars
            .borrow()
            .entries()
            .iter()
            .filter_map(|(k, v)| match k {
                Value::Str(name) if !name.starts_with('_') => {
                    Some((name.to_string(), vm.display(v)))
                }
                _ => None,
            })
            .collect();
        match result {
            Ok(_) => {
                if !shows_value {
                    return Outcome::Done(None);
                }
                let value = self.vars.borrow().get(&key("_")).cloned();
                Outcome::Done(match value {
                    None | Some(Value::None) => None,
                    Some(Value::Str(s)) => Some(crate::vm::stdlib::quote(&s)),
                    Some(v) => Some(vm.display(&v)),
                })
            }
            Err(e) => {
                let mut text = format!("error: {}: {}", e.kind, e.message);
                if let Some(hint) = e.hint() {
                    text.push_str(&format!("\n  = help: {hint}"));
                }
                Outcome::Failed(text)
            }
        }
    }

    /// The session's declarations as one source text, the Prelude excluded.
    fn session_source(&self, decls: &[String]) -> String {
        decls.join("\n")
    }

    fn compile(
        &self,
        decls: &[String],
        entry: Option<&str>,
    ) -> Result<vm::Program, Vec<Diagnostic>> {
        // The Prelude first, so that the positions of its functions never
        // move as declarations are added.
        let (prelude, _) = crate::parser::parse_at(crate::prelude::SOURCE, crate::prelude::BASE);
        let mut module = Module {
            items: Vec::new(),
            span: prelude.span,
        };
        let mut user = Vec::new();
        for (i, d) in decls.iter().enumerate() {
            let (m, errors) = crate::parser::parse_at(d, (i as u32 + 1) << 24);
            if !errors.is_empty() {
                return Err(errors);
            }
            user.extend(m.items);
        }
        if let Some(entry) = entry {
            let (m, errors) = crate::parser::parse_at(entry, 0);
            if !errors.is_empty() {
                return Err(errors);
            }
            user.extend(m.items);
        }
        let mine = crate::prelude::declared_names(&Module {
            items: user.clone(),
            span: prelude.span,
        });
        for item in prelude.items {
            if let ItemKind::Fn(f) = &item.kind {
                if mine.contains(&f.name.name) {
                    continue;
                }
            }
            module.items.push(item);
        }
        module.items.extend(user);
        let problems = crate::check::check(&module);
        // The Prelude is not the user's to fix; the session's own code is.
        let problems: Vec<Diagnostic> = problems
            .into_iter()
            .filter(|d| !crate::prelude::is_prelude_offset(d.span.start.offset))
            .collect();
        if !problems.is_empty() {
            return Err(problems);
        }
        vm::compile(&module)
    }

    fn parse_failure(
        &self,
        errors: &[Diagnostic],
        input: &str,
        line_shift: u32,
        col_shift: u32,
    ) -> Outcome {
        let open = errors
            .iter()
            .any(|e| e.message.contains("is never closed") || e.message.contains("unterminated"));
        if open {
            return Outcome::NeedsMore;
        }
        let shifted: Vec<Diagnostic> = errors
            .iter()
            .map(|e| shift(e, line_shift, col_shift))
            .collect();
        Outcome::Failed(render_errors(&shifted, input, "input"))
    }

    fn render_entry_errors(&self, errors: &[Diagnostic], input: &str, header_lines: u32) -> String {
        let mut out = String::new();
        for e in errors {
            let file = e.span.start.offset >> 24;
            if file == 0 {
                // In the entry: header lines and four spaces of indent.
                let d = shift(e, header_lines, 4);
                out.push_str(&d.render("input", input));
            } else if let Some(text) = self.decls.get(file as usize - 1) {
                out.push_str(&e.render("earlier declaration", text));
            } else {
                out.push_str(&format!("error: {}\n", e.message));
            }
        }
        out.trim_end().to_string()
    }
}

fn key(name: &str) -> Key {
    Key::from_value(&Value::str(name)).expect("strings are keys")
}

fn shift(d: &Diagnostic, lines: u32, columns: u32) -> Diagnostic {
    let fix = |p: Pos| Pos {
        offset: p.offset,
        line: p.line.saturating_sub(lines).max(1),
        column: p.column.saturating_sub(columns).max(1),
    };
    let mut d = d.clone();
    d.span = Span {
        start: fix(d.span.start),
        end: fix(d.span.end),
    };
    d
}

fn render_errors(errors: &[Diagnostic], src: &str, file: &str) -> String {
    errors
        .iter()
        .map(|e| e.render(file, src))
        .collect::<String>()
        .trim_end()
        .to_string()
}

/// The variables a top-level statement declares (`let`, `var`) and assigns
/// (`x = 1`, `a, b = pair`).
fn collect_names(stmt: &crate::ast::Stmt, declared: &mut Vec<String>, assigned: &mut Vec<String>) {
    match &stmt.kind {
        StmtKind::Binding(b) => bind_names(&b.target, declared),
        StmtKind::Assign { target, op, .. } if *op == crate::ast::AssignOp::Assign => {
            target_names(target, assigned)
        }
        _ => {}
    }
}

fn target_names(target: &crate::ast::Expr, out: &mut Vec<String>) {
    match &target.kind {
        ExprKind::Name(id) => {
            if !out.contains(&id.name) {
                out.push(id.name.clone());
            }
        }
        ExprKind::Tuple(items) | ExprKind::List(items) => {
            for t in items {
                target_names(t, out);
            }
        }
        _ => {}
    }
}

fn bind_names(t: &crate::ast::BindTarget, out: &mut Vec<String>) {
    use crate::ast::BindTarget;
    match t {
        BindTarget::Name(id) => {
            if !out.contains(&id.name) {
                out.push(id.name.clone());
            }
        }
        BindTarget::Tuple(items, _) => {
            for i in items {
                bind_names(i, out);
            }
        }
        BindTarget::Discard(_) => {}
    }
}
