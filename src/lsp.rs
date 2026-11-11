//! `vyrn lsp`: a language server, so editors get errors as you type,
//! completion, hover documentation, go to definition, an outline and
//! formatting. It speaks the Language Server Protocol over stdin and
//! stdout, so it works with VS Code, Neovim, Helix, Zed, Sublime and any
//! other editor with an LSP client.
//!
//! Everything comes from the compiler's own parser, checker and formatter,
//! so the editor and `vyrn run` never disagree. There is no type inference
//! of its own: completion after a `.` offers the members of `use`d std
//! modules, or the fields and methods that could apply.

use std::collections::HashMap;
use std::io::{BufRead, Read, Write};
use std::path::{Path, PathBuf};

use crate::ast::{Item, ItemKind, Module};
use crate::diagnostic::Diagnostic;
use crate::project::loader;
use crate::vm::json::{self, Json};

// ==== JSON output ===========================================================

fn s(text: &str) -> String {
    let mut out = String::from("\"");
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn obj(fields: &[(&str, String)]) -> String {
    let parts: Vec<String> = fields
        .iter()
        .map(|(k, v)| format!("{}:{v}", s(k)))
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn arr(items: Vec<String>) -> String {
    format!("[{}]", items.join(","))
}

fn id_json(id: &Json) -> String {
    match id {
        Json::Number(n) => format!("{}", *n as i64),
        Json::String(t) => s(t),
        _ => "null".into(),
    }
}

// ==== positions =============================================================

/// An LSP position: zero-based line and character.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

fn position_json(p: Position) -> String {
    obj(&[
        ("line", p.line.to_string()),
        ("character", p.character.to_string()),
    ])
}

fn range_json(start: Position, end: Position) -> String {
    obj(&[("start", position_json(start)), ("end", position_json(end))])
}

fn span_range(span: crate::span::Span) -> String {
    let p = |pos: crate::span::Pos| Position {
        line: pos.line.saturating_sub(1),
        character: pos.column.saturating_sub(1),
    };
    range_json(p(span.start), p(span.end))
}

fn uri_to_path(uri: &str) -> PathBuf {
    let raw = uri.strip_prefix("file://").unwrap_or(uri);
    let mut bytes = Vec::with_capacity(raw.len());
    let b = raw.as_bytes();
    let mut i = 0;
    while i < b.len() {
        if b[i] == b'%' && i + 2 < b.len() {
            if let Ok(v) = u8::from_str_radix(&raw[i + 1..i + 3], 16) {
                bytes.push(v);
                i += 3;
                continue;
            }
        }
        bytes.push(b[i]);
        i += 1;
    }
    let mut path = String::from_utf8_lossy(&bytes).into_owned();
    // `file:///C:/x` on Windows.
    if path.len() > 2 && path.as_bytes()[0] == b'/' && path.as_bytes()[2] == b':' {
        path.remove(0);
    }
    PathBuf::from(path)
}

fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut out = String::from("file://");
    if !text.starts_with('/') {
        out.push('/');
    }
    for c in text.chars() {
        match c {
            ' ' => out.push_str("%20"),
            '#' => out.push_str("%23"),
            '%' => out.push_str("%25"),
            c => out.push(c),
        }
    }
    out
}

/// The identifier around a position, and what comes before it: `math.` in
/// `math.sq|`.
fn word_at(text: &str, pos: Position) -> (String, Option<String>) {
    let Some(line) = text.lines().nth(pos.line as usize) else {
        return (String::new(), None);
    };
    let chars: Vec<char> = line.chars().collect();
    let at = (pos.character as usize).min(chars.len());
    let is_word = |c: char| c.is_alphanumeric() || c == '_';
    let mut start = at;
    while start > 0 && is_word(chars[start - 1]) {
        start -= 1;
    }
    let mut end = at;
    while end < chars.len() && is_word(chars[end]) {
        end += 1;
    }
    let word: String = chars[start..end].iter().collect();
    let receiver = if start > 0 && chars[start - 1] == '.' {
        let mut r = start - 1;
        while r > 0 && is_word(chars[r - 1]) {
            r -= 1;
        }
        Some(chars[r..start - 1].iter().collect::<String>())
    } else {
        None
    };
    (word, receiver)
}

// ==== what the language offers ==============================================

/// Keywords and a line about each, for completion and hover.
const KEYWORDS: &[(&str, &str)] = &[
    (
        "create",
        "Declares a function: `create name(params) -> Result:` ... `end`.",
    ),
    (
        "struct",
        "A value type: copied when assigned. `struct Point:` with fields, then `end`.",
    ),
    (
        "object",
        "A shared, reference type that may extend one base: `object Enemy(Actor):`.",
    ),
    (
        "enum",
        "A value that is one of several variants, each with its own data.",
    ),
    ("trait", "An interface: methods a type promises to have."),
    (
        "impl",
        "Gives a type the methods of a trait: `impl Shape for Circle:`.",
    ),
    ("main", "Where the program starts: `main:` ... `end`."),
    ("let", "A name that is fixed once set: `let limit = 10`."),
    ("var", "A name that may change: `var count = 0`."),
    ("const", "A value computed when the program is compiled."),
    (
        "if",
        "Runs a block when a condition is true; continue with `else if` and `else`.",
    ),
    (
        "else",
        "The branch taken when the conditions above are false.",
    ),
    (
        "each",
        "Runs a block once for each item: `each name in names:`.",
    ),
    (
        "loop",
        "Repeats while a condition holds (`loop count < 10:`), or forever (`loop:`) until `stop`.",
    ),
    ("stop", "Leaves the innermost loop (Python's `break`)."),
    (
        "skip",
        "Goes on to the next round of the loop (Python's `continue`).",
    ),
    (
        "when",
        "Picks a branch by the shape of a value: `when shape:` with `is Circle(r):` arms.",
    ),
    (
        "is",
        "One arm of a `when`: `is 0:`, `is Shape.Circle(r):`, `is _:`.",
    ),
    (
        "attempt",
        "Runs a block that might fail; handle failures with `catch` and clean up with `always`.",
    ),
    (
        "catch",
        "Handles a failure from `attempt`: `catch NotFound as e:`.",
    ),
    ("always", "Runs after `attempt` whether or not it failed."),
    ("fail", "Stops with an error: `fail NotFound(name)`."),
    (
        "using",
        "Uses a resource and closes it at `end`: `using open(path) as f:`.",
    ),
    ("return", "Leaves the function with a value."),
    (
        "use",
        "Brings in a module: `use std.math`, `use app.storage (Db)`, `use \"github.com/...@v1\"`.",
    ),
    (
        "async",
        "A function that runs as a task; calling it starts it.",
    ),
    ("await", "Waits for a task's result."),
    ("spawn", "Starts a task without waiting for it."),
    (
        "in",
        "Membership (`x in xs`) and the source of an `each` loop.",
    ),
    ("and", "True when both sides are true."),
    ("or", "True when either side is true."),
    ("not", "Turns true into false and false into true."),
    ("true", "The boolean true."),
    ("false", "The boolean false."),
    ("none", "No value. A type written `T?` may hold `none`."),
    ("to", "An inclusive range: `1 to 5` is 1, 2, 3, 4, 5."),
    (
        "until",
        "A range that stops before its end: `0 until 3` is 0, 1, 2.",
    ),
    ("by", "The step of a range: `0 to 10 by 2`."),
    (
        "on",
        "Runs a block on a device or places a value: `on gpu[0]:`.",
    ),
    (
        "strict",
        "A fully typed function, checked like Rust: `strict create f(x: int) -> int:`.",
    ),
    (
        "kernel",
        "A function on numbers that also runs on whole tensors, on the GPU.",
    ),
    (
        "route",
        "An HTTP route: `route GET \"/users/{id}\" (id: int):`.",
    ),
    (
        "mut",
        "A parameter the function may change in place, or `each mut x in xs`.",
    ),
    ("pub", "Visible outside its module."),
];

/// Methods of strings, lists and maps, with a line each.
const METHODS: &[(&str, &str, &str)] = &[
    ("str", "upper()", "In capitals."),
    ("str", "lower()", "In small letters."),
    ("str", "title()", "Each Word Capitalized."),
    (
        "str",
        "capitalize()",
        "First letter capital, the rest small.",
    ),
    (
        "str",
        "strip(chars?)",
        "Without spaces (or `chars`) at either end; also lstrip, rstrip.",
    ),
    (
        "str",
        "split(sep?, limit?)",
        "Pieces between separators (whitespace by default).",
    ),
    ("str", "rsplit(sep, limit?)", "Like split, from the right."),
    ("str", "lines()", "The lines, without line endings."),
    ("str", "chars()", "The characters, as a list."),
    (
        "str",
        "join(items)",
        "The items joined with this string between them.",
    ),
    ("str", "replace(old, new)", "Every `old` replaced by `new`."),
    (
        "str",
        "startswith(prefix)",
        "Whether it starts with `prefix`.",
    ),
    ("str", "endswith(suffix)", "Whether it ends with `suffix`."),
    ("str", "find(part)", "Where `part` first is, or -1."),
    (
        "str",
        "index(part)",
        "Where `part` first is; fails if it is not there.",
    ),
    ("str", "count(part)", "How many times `part` occurs."),
    (
        "str",
        "contains(part)",
        "Whether `part` occurs (same as `part in s`).",
    ),
    ("str", "isdigit()", "Whether every character is a digit."),
    ("str", "isalpha()", "Whether every character is a letter."),
    (
        "str",
        "partition(sep)",
        "(before, sep, after) around the first `sep`.",
    ),
    (
        "str",
        "removeprefix(prefix)",
        "Without `prefix` at the start.",
    ),
    (
        "str",
        "removesuffix(suffix)",
        "Without `suffix` at the end.",
    ),
    ("str", "zfill(width)", "Padded with zeros on the left."),
    (
        "str",
        "center(width, fill?)",
        "Centered in `width`; also ljust, rjust.",
    ),
    ("str", "repeat(n)", "The string `n` times."),
    ("str", "format(spec)", "As `\"{s:spec}\"` would show it."),
    ("list", "append(x)", "Adds `x` at the end."),
    ("list", "extend(xs)", "Adds every item of `xs`."),
    ("list", "insert(i, x)", "Puts `x` at position `i`."),
    (
        "list",
        "pop(i?)",
        "Removes and returns the last item (or item `i`).",
    ),
    ("list", "remove(x)", "Removes the first `x`."),
    ("list", "clear()", "Removes every item."),
    ("list", "sort(key?, reverse?)", "Sorts in place."),
    ("list", "reverse()", "Reverses in place."),
    ("list", "copy()", "A new list with the same items."),
    ("list", "index(x)", "Where `x` first is."),
    ("list", "count(x)", "How many items equal `x`."),
    ("list", "contains(x)", "Whether `x` is in the list."),
    ("list", "first()", "The first item, or none."),
    ("list", "last()", "The last item, or none."),
    ("list", "unique()", "The distinct items, in order."),
    (
        "list",
        "join(sep?)",
        "The items as text with `sep` between them.",
    ),
    ("list", "chunks(n)", "Lists of `n` items."),
    ("list", "sum()", "The total."),
    ("list", "map(f)", "A new list of `f(x)` for each item."),
    ("list", "filter(f)", "The items for which `f(x)` is true."),
    (
        "list",
        "reduce(f, start?)",
        "Combines the items from the left.",
    ),
    (
        "list",
        "find(f)",
        "The first item for which `f(x)` is true, or none.",
    ),
    ("list", "find_index(f)", "Where the first match is, or -1."),
    ("list", "any(f?)", "Whether some item passes."),
    ("list", "all(f?)", "Whether every item passes."),
    ("list", "count_if(f)", "How many items pass."),
    (
        "list",
        "min_by(f)",
        "The item with the smallest `f(x)`; also max_by.",
    ),
    ("list", "sum_by(f)", "The total of `f(x)`."),
    ("list", "group_by(f)", "A map from each key to its items."),
    ("list", "each(f)", "Calls `f` on every item."),
    ("list", "flatten()", "One level of nesting removed."),
    ("map", "keys()", "The keys, in insertion order."),
    ("map", "values()", "The values."),
    ("map", "items()", "(key, value) pairs."),
    (
        "map",
        "get(key, default?)",
        "The value for `key`, or `default` (none).",
    ),
    (
        "map",
        "pop(key, default?)",
        "Removes `key` and returns its value.",
    ),
    ("map", "update(other)", "Adds every entry of `other`."),
    (
        "map",
        "setdefault(key, value)",
        "The value for `key`, setting it to `value` first if missing.",
    ),
    ("map", "has(key)", "Whether `key` is in the map."),
    (
        "map",
        "map_values(f)",
        "The same keys with each value passed through `f`.",
    ),
    (
        "map",
        "filter(f)",
        "The entries for which `f(key, value)` is true.",
    ),
];

/// A line of documentation for the builtins people use most.
fn builtin_doc(name: &str) -> Option<&'static str> {
    Some(match name {
        "print" => "Shows values, separated by spaces, then a new line.",
        "input" => "Reads a line from the keyboard, after showing the prompt.",
        "len" => "How many items (or characters) a value has.",
        "str" => "A value as text.",
        "int" => "A whole number from a string or a float.",
        "float" => "A number with a fraction, from a string or an int.",
        "bool" => "Whether a value counts as true.",
        "range" => "Numbers from `start` up to `stop` (not included); Vyrn also has `0 until n`.",
        "enumerate" => "(position, item) pairs.",
        "zip" => "Tuples of the items at the same position in each list.",
        "sorted" => "A new list in order; `key = f` sorts by `f(x)`, `reverse = true` backwards.",
        "min" => "The smallest item; `key = f` compares `f(x)`.",
        "max" => "The largest item; `key = f` compares `f(x)`.",
        "sum" => "The total of the items.",
        "abs" => "The value without its sign.",
        "round" => "Rounded to `digits` decimals (to a whole number by default).",
        "open" => "Opens a file: \"r\" reads, \"w\" writes, \"a\" appends. Use it with `using`.",
        "read_text" => "A whole file as a string.",
        "write_text" => "Writes a string to a file, replacing it.",
        "parse_json" => "A value from JSON text.",
        "to_json" => "A value as JSON text.",
        "format" => "A value as `\"{value:spec}\"` would show it: `format(3.14159, \".2f\")`.",
        "set" => "The distinct items of a list. `{1, 2, 3}` is a set literal.",
        "list" => "A new list of the items of anything that can be looped over.",
        "dict" => "A map from (key, value) pairs.",
        "any" => "Whether any item is true.",
        "all" => "Whether every item is true.",
        "reversed" => "The items in reverse order.",
        "map" => "A new list of `f(x)` for each item.",
        "filter" => "The items for which `f(x)` is true.",
        "typeof" => "The name of a value's type.",
        "repr" => "A value as it would be written in code.",
        "assert" => "Fails with `AssertionError` when the condition is false.",
        "sleep" => "A task that finishes after some seconds: `await sleep(0.5)`.",
        "channel" => "A queue for passing values between tasks.",
        "tensor" => "A tensor from nested lists of numbers.",
        _ => return None,
    })
}

/// The std modules and what each is for.
const MODULES: &[(&str, &str)] = &[
    (
        "os",
        "The program's arguments, environment variables, running commands.",
    ),
    (
        "fs",
        "Files and directories: exists, list, walk, mkdir, remove, copy.",
    ),
    ("path", "Paths: join, name, stem, ext, parent, absolute."),
    ("time", "The clock and dates: now, format, parse, date."),
    (
        "random",
        "Random numbers: int, float, choice, shuffle, sample, seed.",
    ),
    ("math", "sin, cos, log, floor, gcd, pi, inf and more."),
    ("json", "JSON: parse, dumps, load, save."),
    (
        "re",
        "Regular expressions: test, find, find_all, replace, split.",
    ),
    ("csv", "CSV files: read, parse, write, format."),
    (
        "http",
        "HTTP: a server (routes, serve) and a client (get, post).",
    ),
];

fn native_signature(name: &str, params: &[&str], variadic: bool) -> String {
    if variadic {
        format!("{name}(...)")
    } else {
        format!("{name}({})", params.join(", "))
    }
}

// ==== documents =============================================================

struct Server {
    docs: HashMap<String, String>,
    out: Box<dyn Write>,
    shutdown: bool,
}

/// Files on disk, except the ones open in the editor, which are read from
/// the editor's unsaved text.
struct Overlay<'a> {
    docs: &'a HashMap<PathBuf, String>,
}

impl loader::Source for Overlay<'_> {
    fn read(&mut self, path: &Path) -> std::io::Result<String> {
        let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(text) = self.docs.get(&key) {
            return Ok(text.clone());
        }
        std::fs::read_to_string(path)
    }

    fn resolve_url(&mut self, url: &str) -> Result<PathBuf, String> {
        // Only what is already downloaded: the editor never fetches.
        let fetcher = crate::project::NoNetwork;
        let root = PathBuf::from(".");
        let lock = std::fs::read_to_string(root.join(crate::project::manifest::LOCKFILE))
            .ok()
            .and_then(|t| crate::project::lock::Lockfile::parse(&t).ok())
            .unwrap_or_default();
        let mut resolver = crate::project::resolve::Resolver::new(
            &fetcher,
            crate::project::resolve::vyrn_home().join("modules"),
            lock,
        );
        resolver.offline = true;
        resolver.resolve(url)
    }

    fn exists(&mut self, path: &Path) -> bool {
        let key = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        self.docs.contains_key(&key) || path.is_file()
    }
}

impl Server {
    fn send(&mut self, body: &str) {
        let _ = write!(self.out, "Content-Length: {}\r\n\r\n{body}", body.len());
        let _ = self.out.flush();
    }

    fn reply(&mut self, id: &Json, result: String) {
        let body = obj(&[
            ("jsonrpc", s("2.0")),
            ("id", id_json(id)),
            ("result", result),
        ]);
        self.send(&body);
    }

    fn notify(&mut self, method: &str, params: String) {
        let body = obj(&[
            ("jsonrpc", s("2.0")),
            ("method", s(method)),
            ("params", params),
        ]);
        self.send(&body);
    }

    fn handle(&mut self, message: &Json) {
        let method = message.get("method").and_then(Json::as_str).unwrap_or("");
        let params = message.get("params").cloned().unwrap_or(Json::Null);
        let id = message.get("id");
        let text_doc = params.get("textDocument");
        let uri = text_doc
            .and_then(|d| d.get("uri"))
            .and_then(Json::as_str)
            .unwrap_or("")
            .to_string();
        let pos = params.get("position").map(|p| Position {
            line: p.get("line").and_then(Json::as_u64).unwrap_or(0) as u32,
            character: p.get("character").and_then(Json::as_u64).unwrap_or(0) as u32,
        });
        match method {
            "initialize" => {
                let caps = obj(&[
                    ("textDocumentSync", "1".into()),
                    ("hoverProvider", "true".into()),
                    ("definitionProvider", "true".into()),
                    ("documentSymbolProvider", "true".into()),
                    ("documentFormattingProvider", "true".into()),
                    (
                        "completionProvider",
                        obj(&[("triggerCharacters", arr(vec![s(".")]))]),
                    ),
                    (
                        "signatureHelpProvider",
                        obj(&[("triggerCharacters", arr(vec![s("("), s(",")]))]),
                    ),
                ]);
                let result = obj(&[
                    ("capabilities", caps),
                    (
                        "serverInfo",
                        obj(&[
                            ("name", s("vyrn")),
                            ("version", s(env!("CARGO_PKG_VERSION"))),
                        ]),
                    ),
                ]);
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            "shutdown" => {
                self.shutdown = true;
                if let Some(id) = id {
                    self.reply(id, "null".into());
                }
            }
            "exit" => std::process::exit(if self.shutdown { 0 } else { 1 }),
            "textDocument/didOpen" => {
                let text = text_doc
                    .and_then(|d| d.get("text"))
                    .and_then(Json::as_str)
                    .unwrap_or("")
                    .to_string();
                self.docs.insert(uri.clone(), text);
                self.publish(&uri);
            }
            "textDocument/didChange" => {
                if let Some(text) = params
                    .get("contentChanges")
                    .and_then(Json::as_array)
                    .and_then(|c| c.last())
                    .and_then(|c| c.get("text"))
                    .and_then(Json::as_str)
                {
                    self.docs.insert(uri.clone(), text.to_string());
                }
                self.publish(&uri);
            }
            "textDocument/didSave" => self.publish(&uri),
            "textDocument/didClose" => {
                self.docs.remove(&uri);
                self.notify(
                    "textDocument/publishDiagnostics",
                    obj(&[("uri", s(&uri)), ("diagnostics", "[]".into())]),
                );
            }
            "textDocument/completion" => {
                let result = pos.map_or("[]".into(), |p| self.completion(&uri, p));
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            "textDocument/hover" => {
                let result = pos
                    .and_then(|p| self.hover(&uri, p))
                    .unwrap_or("null".into());
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            "textDocument/definition" => {
                let result = pos
                    .and_then(|p| self.definition(&uri, p))
                    .unwrap_or("null".into());
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            "textDocument/signatureHelp" => {
                let result = pos
                    .and_then(|p| self.signature_help(&uri, p))
                    .unwrap_or("null".into());
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            "textDocument/documentSymbol" => {
                let result = self.symbols(&uri);
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            "textDocument/formatting" => {
                let result = self.format(&uri);
                if let Some(id) = id {
                    self.reply(id, result);
                }
            }
            _ => {
                // Requests we do not answer get an empty result, so the
                // editor does not wait; notifications are ignored.
                if let Some(id) = id {
                    if !method.starts_with("$/") {
                        self.reply(id, "null".into());
                    }
                }
            }
        }
    }

    fn text(&self, uri: &str) -> String {
        self.docs
            .get(uri)
            .cloned()
            .or_else(|| std::fs::read_to_string(uri_to_path(uri)).ok())
            .unwrap_or_default()
    }

    /// Parses, loads the modules it uses, and checks a document. Returns
    /// the problems by file.
    fn problems(&self, uri: &str) -> HashMap<PathBuf, Vec<Diagnostic>> {
        let path = uri_to_path(uri);
        let open: HashMap<PathBuf, String> = self
            .docs
            .iter()
            .map(|(u, t)| {
                let p = uri_to_path(u);
                (p.canonicalize().unwrap_or(p), t.clone())
            })
            .collect();
        let root = crate::project::manifest::find_root(&path).unwrap_or_else(|| {
            path.parent()
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from("."))
        });
        let mut by_file: HashMap<PathBuf, Vec<Diagnostic>> = HashMap::new();
        let (diagnostics, files) = match loader::load(&path, &root, &mut Overlay { docs: &open }) {
            Ok(loaded) => {
                let mut problems = crate::check::check(&loaded.module);
                if problems.is_empty() {
                    if let Err(errors) = crate::vm::compile(&loaded.module) {
                        problems = errors;
                    }
                }
                (problems, loaded.files)
            }
            Err((diagnostics, files)) => (diagnostics, files),
        };
        for d in diagnostics {
            if crate::prelude::is_prelude_offset(d.span.start.offset) {
                continue;
            }
            let file = files
                .get((d.span.start.offset >> 24) as usize)
                .map(|f| f.path.clone())
                .unwrap_or_else(|| path.clone());
            by_file.entry(file).or_default().push(d);
        }
        by_file
    }

    fn publish(&mut self, uri: &str) {
        let path = uri_to_path(uri);
        let mut problems = self.problems(uri);
        let mine = problems
            .remove(&path)
            .or_else(|| {
                let key = path.canonicalize().ok()?;
                let found = problems
                    .keys()
                    .find(|k| k.canonicalize().ok().as_ref() == Some(&key))?
                    .clone();
                problems.remove(&found)
            })
            .unwrap_or_default();
        let list: Vec<String> = mine
            .iter()
            .map(|d| {
                let message = match &d.hint {
                    Some(h) => format!("{}\n{h}", d.message),
                    None => d.message.clone(),
                };
                obj(&[
                    ("range", span_range(d.span)),
                    ("severity", "1".into()),
                    ("source", s("vyrn")),
                    ("message", s(&message)),
                ])
            })
            .collect();
        self.notify(
            "textDocument/publishDiagnostics",
            obj(&[("uri", s(uri)), ("diagnostics", arr(list))]),
        );
    }

    fn module(&self, uri: &str) -> Module {
        crate::parser::parse_recovering(&self.text(uri)).0
    }

    // ---- completion ----

    fn completion(&self, uri: &str, pos: Position) -> String {
        let text = self.text(uri);
        let (prefix, receiver) = word_at(
            &text,
            Position {
                line: pos.line,
                character: pos.character,
            },
        );
        let _ = prefix;
        let mut items: Vec<String> = Vec::new();
        let item = |label: &str, kind: u32, detail: &str, doc: &str| {
            let mut fields = vec![
                ("label", s(label)),
                ("kind", kind.to_string()),
                ("detail", s(detail)),
            ];
            if !doc.is_empty() {
                fields.push((
                    "documentation",
                    obj(&[("kind", s("markdown")), ("value", s(doc))]),
                ));
            }
            obj(&fields)
        };
        let modules = used_modules(&text);
        if let Some(receiver) = receiver {
            if let Some(prefix) = modules.get(&receiver) {
                for n in crate::vm::builtins::NATIVES {
                    if let Some(member) = n.name.strip_prefix(&format!("{prefix}.")) {
                        items.push(item(
                            member,
                            3,
                            &native_signature(member, n.params, n.variadic),
                            "",
                        ));
                    }
                }
                for c in ["pi", "e", "tau", "inf", "nan"] {
                    if crate::vm::builtins::constant(&format!("{prefix}.{c}")).is_some() {
                        items.push(item(c, 21, "constant", ""));
                    }
                }
                return arr(items);
            }
            // Fields and methods of the program's types, then those of
            // strings, lists and maps.
            let module = self.module(uri);
            for it in &module.items {
                if let ItemKind::Aggregate(a) = &it.kind {
                    for f in &a.fields {
                        items.push(item(
                            &f.name.name,
                            5,
                            &format!("field of {}", a.name.name),
                            "",
                        ));
                    }
                    for m in &a.methods {
                        if let ItemKind::Fn(f) = &m.kind {
                            items.push(item(
                                &f.name.name,
                                2,
                                &format!("method of {}", a.name.name),
                                &m.doc.join("\n"),
                            ));
                        }
                    }
                }
            }
            for (owner, sig, doc) in METHODS {
                let name = sig.split('(').next().unwrap_or(sig);
                items.push(item(name, 2, &format!("{owner}.{sig}"), doc));
            }
            return arr(items);
        }
        for (k, doc) in KEYWORDS {
            items.push(item(k, 14, "keyword", doc));
        }
        for n in crate::vm::builtins::NATIVES {
            if n.name.contains('.') || n.name.starts_with("__") {
                continue;
            }
            items.push(item(
                n.name,
                3,
                &native_signature(n.name, n.params, n.variadic),
                builtin_doc(n.name).unwrap_or(""),
            ));
        }
        for (name, doc) in [("sorted", "sorted"), ("map", "map"), ("filter", "filter")] {
            if !items
                .iter()
                .any(|i| i.contains(&format!("\"label\":\"{name}\"")))
            {
                items.push(item(name, 3, name, builtin_doc(doc).unwrap_or("")));
            }
        }
        for alias in modules.keys() {
            items.push(item(alias, 9, "module", ""));
        }
        let module = self.module(uri);
        for it in &module.items {
            if let Some((name, kind, detail)) = describe_item(it, &text) {
                items.push(item(&name, kind, &detail, &it.doc.join("\n")));
            }
        }
        // Names assigned anywhere in the file: variables and parameters.
        let mut seen = std::collections::HashSet::new();
        for line in text.lines() {
            let t = line.trim_start();
            let t = t
                .strip_prefix("let ")
                .or_else(|| t.strip_prefix("var "))
                .or_else(|| t.strip_prefix("each "))
                .unwrap_or(t);
            let name: String = t
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect();
            let rest = t[name.len()..].trim_start();
            if !name.is_empty()
                && (rest.starts_with('=') && !rest.starts_with("==")
                    || rest.starts_with(" in")
                    || rest.starts_with("in "))
                && seen.insert(name.clone())
            {
                items.push(item(&name, 6, "variable", ""));
            }
        }
        arr(items)
    }

    // ---- hover ----

    fn hover(&self, uri: &str, pos: Position) -> Option<String> {
        let text = self.text(uri);
        let (word, receiver) = word_at(&text, pos);
        if word.is_empty() {
            return None;
        }
        let markdown = |value: String| {
            obj(&[(
                "contents",
                obj(&[("kind", s("markdown")), ("value", s(&value))]),
            )])
        };
        let modules = used_modules(&text);
        if let Some(receiver) = &receiver {
            if let Some(prefix) = modules.get(receiver) {
                let full = format!("{prefix}.{word}");
                if let Some(n) = crate::vm::builtins::NATIVES.iter().find(|n| n.name == full) {
                    return Some(markdown(format!(
                        "```vyrn\n{receiver}.{}\n```",
                        native_signature(&word, n.params, n.variadic)
                    )));
                }
            }
            let docs: Vec<String> = METHODS
                .iter()
                .filter(|(_, sig, _)| sig.split('(').next() == Some(word.as_str()))
                .map(|(owner, sig, doc)| format!("`{owner}.{sig}`: {doc}"))
                .collect();
            if !docs.is_empty() {
                return Some(markdown(docs.join("\n\n")));
            }
        }
        let module = self.module(uri);
        for it in &module.items {
            if let Some((name, _, detail)) = describe_item(it, &text) {
                if name == word {
                    let mut value = format!("```vyrn\n{detail}\n```");
                    if !it.doc.is_empty() {
                        value.push_str("\n\n");
                        value.push_str(&it.doc.join("\n"));
                    }
                    return Some(markdown(value));
                }
            }
        }
        if let Some((_, doc)) = KEYWORDS.iter().find(|(k, _)| *k == word) {
            return Some(markdown(format!("**{word}**: {doc}")));
        }
        if let Some(n) = crate::vm::builtins::NATIVES.iter().find(|n| n.name == word) {
            let mut value = format!(
                "```vyrn\n{}\n```",
                native_signature(&word, n.params, n.variadic)
            );
            if let Some(doc) = builtin_doc(&word) {
                value.push_str(&format!("\n\n{doc}"));
            }
            return Some(markdown(value));
        }
        if let Some(doc) = builtin_doc(&word) {
            return Some(markdown(format!("**{word}**: {doc}")));
        }
        if let Some((name, doc)) = MODULES.iter().find(|(m, _)| {
            modules.get(&word).is_some_and(|p| p == m) || **m == word && modules.contains_key(&word)
        }) {
            return Some(markdown(format!("module `std.{name}`: {doc}")));
        }
        None
    }

    // ---- go to definition ----

    fn definition(&self, uri: &str, pos: Position) -> Option<String> {
        let text = self.text(uri);
        let (word, receiver) = word_at(&text, pos);
        if word.is_empty() {
            return None;
        }
        let location = |file_uri: &str, span: crate::span::Span| {
            obj(&[("uri", s(file_uri)), ("range", span_range(span))])
        };
        // Declarations in this file.
        let module = self.module(uri);
        if receiver.is_none() {
            for it in &module.items {
                if let Some(span) = item_name_span(it, &word) {
                    return Some(location(uri, span));
                }
            }
            // A local variable or parameter: its first binding above, in
            // the same function.
            if let Some(span) = local_binding(&text, &word, pos.line) {
                return Some(location(uri, span));
            }
        } else {
            for it in &module.items {
                if let ItemKind::Aggregate(a) = &it.kind {
                    for f in &a.fields {
                        if f.name.name == word {
                            return Some(location(uri, f.name.span));
                        }
                    }
                    for m in &a.methods {
                        if let Some(span) = item_name_span(m, &word) {
                            return Some(location(uri, span));
                        }
                    }
                }
            }
        }
        // Declarations in the modules this file uses.
        let path = uri_to_path(uri);
        let root = crate::project::manifest::find_root(&path)
            .unwrap_or_else(|| path.parent().map(PathBuf::from).unwrap_or_default());
        let open: HashMap<PathBuf, String> = self
            .docs
            .iter()
            .map(|(u, t)| {
                let p = uri_to_path(u);
                (p.canonicalize().unwrap_or(p), t.clone())
            })
            .collect();
        let loaded = loader::load(&path, &root, &mut Overlay { docs: &open }).ok()?;
        for it in &loaded.module.items {
            let file = (it.span.start.offset >> 24) as usize;
            if file == 0 {
                continue;
            }
            let name = item_names(it);
            if name
                .iter()
                .any(|n| n == &word || n.ends_with(&format!(".{word}")))
            {
                let f = loaded.files.get(file)?;
                let span = item_name_span_any(it)?;
                return Some(location(&path_to_uri(&f.path), span));
            }
        }
        None
    }

    // ---- signature help ----

    fn signature_help(&self, uri: &str, pos: Position) -> Option<String> {
        let text = self.text(uri);
        let line = text.lines().nth(pos.line as usize)?;
        let before: String = line.chars().take(pos.character as usize).collect();
        // The call whose `(` is still open, and which argument we are on.
        let mut depth = 0i32;
        let mut commas = 0u32;
        let chars: Vec<char> = before.chars().collect();
        let mut open_at = None;
        for i in (0..chars.len()).rev() {
            match chars[i] {
                ')' | ']' | '}' => depth += 1,
                '(' | '[' | '{' if depth > 0 => depth -= 1,
                '(' => {
                    open_at = Some(i);
                    break;
                }
                ',' if depth == 0 => commas += 1,
                _ => {}
            }
        }
        let open_at = open_at?;
        let head: String = chars[..open_at].iter().collect();
        let name: String = head
            .chars()
            .rev()
            .take_while(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        let (receiver, word) = match name.rsplit_once('.') {
            Some((r, w)) => (Some(r.to_string()), w.to_string()),
            None => (None, name.clone()),
        };
        let mut params: Vec<String> = Vec::new();
        let mut label = String::new();
        let modules = used_modules(&text);
        if let Some(prefix) = receiver.as_ref().and_then(|r| modules.get(r)) {
            let full = format!("{prefix}.{word}");
            let n = crate::vm::builtins::NATIVES
                .iter()
                .find(|n| n.name == full)?;
            params = n.params.iter().map(|p| p.to_string()).collect();
            label = format!(
                "{}.{}",
                receiver.as_deref().unwrap_or(""),
                native_signature(&word, n.params, n.variadic)
            );
        } else if receiver.is_none() {
            let module = self.module(uri);
            for it in &module.items {
                if let ItemKind::Fn(f) = &it.kind {
                    if f.name.name == word {
                        params = f.params.iter().map(|p| p.name.name.clone()).collect();
                        label = format!("{word}({})", params.join(", "));
                    }
                }
            }
            if label.is_empty() {
                let n = crate::vm::builtins::NATIVES
                    .iter()
                    .find(|n| n.name == word)?;
                params = n.params.iter().map(|p| p.to_string()).collect();
                label = native_signature(&word, n.params, n.variadic);
            }
        } else {
            return None;
        }
        let parameters: Vec<String> = params.iter().map(|p| obj(&[("label", s(p))])).collect();
        Some(obj(&[
            (
                "signatures",
                arr(vec![obj(&[
                    ("label", s(&label)),
                    ("parameters", arr(parameters)),
                ])]),
            ),
            ("activeSignature", "0".into()),
            ("activeParameter", commas.to_string()),
        ]))
    }

    // ---- outline and formatting ----

    fn symbols(&self, uri: &str) -> String {
        let text = self.text(uri);
        let module = self.module(uri);
        let mut out = Vec::new();
        for it in &module.items {
            let Some((name, kind, detail)) = describe_item(it, &text) else {
                if matches!(it.kind, ItemKind::Main(_)) {
                    out.push(obj(&[
                        ("name", s("main")),
                        ("kind", "12".into()),
                        ("range", span_range(it.span)),
                        ("selectionRange", span_range(it.span)),
                    ]));
                }
                continue;
            };
            let symbol_kind = match kind {
                3 => 12,  // function
                22 => 23, // struct
                7 => 5,   // class
                13 => 10, // enum
                21 => 14, // constant
                9 => 2,   // module
                _ => 13,  // variable
            };
            let mut children = Vec::new();
            if let ItemKind::Aggregate(a) = &it.kind {
                for f in &a.fields {
                    children.push(obj(&[
                        ("name", s(&f.name.name)),
                        ("kind", "8".into()),
                        ("range", span_range(f.span)),
                        ("selectionRange", span_range(f.name.span)),
                    ]));
                }
                for m in &a.methods {
                    if let ItemKind::Fn(f) = &m.kind {
                        children.push(obj(&[
                            ("name", s(&f.name.name)),
                            ("kind", "6".into()),
                            ("range", span_range(m.span)),
                            ("selectionRange", span_range(f.name.span)),
                        ]));
                    }
                }
            }
            let selection = item_name_span_any(it).unwrap_or(it.span);
            out.push(obj(&[
                ("name", s(&name)),
                ("detail", s(&detail)),
                ("kind", symbol_kind.to_string()),
                ("range", span_range(it.span)),
                ("selectionRange", span_range(selection)),
                ("children", arr(children)),
            ]));
        }
        arr(out)
    }

    fn format(&self, uri: &str) -> String {
        let text = self.text(uri);
        match crate::fmt::format(&text) {
            Ok(formatted) if formatted != text => {
                let lines = text.lines().count() as u32 + 1;
                arr(vec![obj(&[
                    (
                        "range",
                        range_json(
                            Position {
                                line: 0,
                                character: 0,
                            },
                            Position {
                                line: lines,
                                character: 0,
                            },
                        ),
                    ),
                    ("newText", s(&formatted)),
                ])])
            }
            _ => "[]".into(),
        }
    }
}

/// `use std.math` and `use std.math as m`: local name to builtin prefix.
fn used_modules(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    for line in text.lines() {
        let Some(rest) = line.trim().strip_prefix("use std.") else {
            continue;
        };
        let mut parts = rest.split_whitespace();
        let Some(name) = parts.next() else { continue };
        let name = name.trim_end_matches(|c: char| !c.is_alphanumeric());
        let Some(prefix) = crate::vm::builtins::std_module(name) else {
            continue;
        };
        let local = match (parts.next(), parts.next()) {
            (Some("as"), Some(alias)) => alias.to_string(),
            _ => name.to_string(),
        };
        out.insert(local, prefix.to_string());
    }
    out
}

/// A declaration's name, completion kind, and header line.
fn describe_item(it: &Item, text: &str) -> Option<(String, u32, String)> {
    let line = item_name_span_any(it).map_or(it.span.start.line, |s| s.start.line);
    let header = || {
        text.lines()
            .nth(line.saturating_sub(1) as usize)
            .unwrap_or("")
            .trim()
            .trim_end_matches(':')
            .to_string()
    };
    let (name, kind) = match &it.kind {
        ItemKind::Fn(f) => (f.name.name.clone(), 3),
        ItemKind::Aggregate(a) => (
            a.name.name.clone(),
            if matches!(a.kind, crate::ast::AggregateKind::Struct) {
                22
            } else {
                7
            },
        ),
        ItemKind::Enum(e) => (e.name.name.clone(), 13),
        ItemKind::Const(c) => (c.name.name.clone(), 21),
        ItemKind::Trait(t) => (t.name.name.clone(), 8),
        ItemKind::TypeAlias(t) => (t.name.name.clone(), 22),
        _ => return None,
    };
    Some((name, kind, header()))
}

fn item_names(it: &Item) -> Vec<String> {
    match &it.kind {
        ItemKind::Fn(f) => vec![f.name.name.clone()],
        ItemKind::Aggregate(a) => vec![a.name.name.clone()],
        ItemKind::Enum(e) => vec![e.name.name.clone()],
        ItemKind::Const(c) => vec![c.name.name.clone()],
        ItemKind::Trait(t) => vec![t.name.name.clone()],
        ItemKind::TypeAlias(t) => vec![t.name.name.clone()],
        _ => Vec::new(),
    }
}

fn item_name_span_any(it: &Item) -> Option<crate::span::Span> {
    Some(match &it.kind {
        ItemKind::Fn(f) => f.name.span,
        ItemKind::Aggregate(a) => a.name.span,
        ItemKind::Enum(e) => e.name.span,
        ItemKind::Const(c) => c.name.span,
        ItemKind::Trait(t) => t.name.span,
        ItemKind::TypeAlias(t) => t.name.span,
        _ => return None,
    })
}

fn item_name_span(it: &Item, word: &str) -> Option<crate::span::Span> {
    if item_names(it).iter().any(|n| n == word) {
        return item_name_span_any(it);
    }
    // Enum variants.
    if let ItemKind::Enum(e) = &it.kind {
        for v in &e.variants {
            if v.name.name == word {
                return Some(v.name.span);
            }
        }
    }
    None
}

/// The first line at or above `line`, within the enclosing function, that
/// binds `word`: `word = ...`, `let word`, `var word`, `each word in`, or a
/// parameter of the function's header.
fn local_binding(text: &str, word: &str, line: u32) -> Option<crate::span::Span> {
    let lines: Vec<&str> = text.lines().collect();
    let mut found = None;
    let mut i = line.min(lines.len().saturating_sub(1) as u32) as i64;
    while i >= 0 {
        let l = lines[i as usize];
        let t = l.trim_start();
        let indent = (l.len() - t.len()) as u32;
        let body = t
            .strip_prefix("let ")
            .or_else(|| t.strip_prefix("var "))
            .or_else(|| t.strip_prefix("each "))
            .or_else(|| t.strip_prefix("each mut "))
            .unwrap_or(t);
        let offset = (t.len() - body.len()) as u32;
        if let Some(rest) = body.strip_prefix(word) {
            let rest = rest.trim_start();
            let binds = (rest.starts_with('=') && !rest.starts_with("=="))
                || rest.starts_with(':') && !rest.starts_with("::") && offset > 0
                || rest.starts_with("in ")
                || rest.starts_with(',');
            if binds {
                found = Some((i as u32, indent + offset));
            }
        }
        // A function header: its parameters, and the end of the search.
        let is_header = t.starts_with("create ")
            || t.starts_with("async create ")
            || t.starts_with("main")
            || t.starts_with("strict create ")
            || t.starts_with("kernel create ");
        if is_header {
            if let (Some(open), Some(close)) = (l.find('('), l.rfind(')')) {
                let params = &l[open + 1..close];
                let mut at = open + 1;
                for p in params.split(',') {
                    let name = p
                        .trim()
                        .trim_start_matches("mut ")
                        .trim_start_matches("own ");
                    let name: String = name
                        .chars()
                        .take_while(|c| c.is_alphanumeric() || *c == '_')
                        .collect();
                    if name == word {
                        let col = at + p.find(&name).unwrap_or(0);
                        found = Some((i as u32, col as u32));
                    }
                    at += p.len() + 1;
                }
            }
            break;
        }
        i -= 1;
    }
    let (l, c) = found?;
    let pos = |col: u32| crate::span::Pos {
        offset: 0,
        line: l + 1,
        column: col + 1,
    };
    Some(crate::span::Span {
        start: pos(c),
        end: pos(c + word.chars().count() as u32),
    })
}

/// Runs the language server on stdin and stdout until the editor says
/// `exit`.
pub fn run_stdio() {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut server = Server {
        docs: HashMap::new(),
        out: Box::new(std::io::stdout()),
        shutdown: false,
    };
    loop {
        let mut length = None;
        loop {
            let mut header = String::new();
            match input.read_line(&mut header) {
                Ok(0) => return,
                Ok(_) => {}
                Err(_) => return,
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
        if input.read_exact(&mut body).is_err() {
            return;
        }
        let Ok(message) = json::parse(&String::from_utf8_lossy(&body)) else {
            continue;
        };
        server.handle(&message);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn words_and_receivers() {
        let text = "main:\n    x = math.sq\nend\n";
        assert_eq!(
            word_at(
                text,
                Position {
                    line: 1,
                    character: 15
                }
            ),
            ("sq".to_string(), Some("math".to_string()))
        );
        assert_eq!(
            word_at(
                text,
                Position {
                    line: 1,
                    character: 5
                }
            )
            .0,
            "x"
        );
    }

    #[test]
    fn uris_round_trip() {
        let p = PathBuf::from("/tmp/my project/a.vyrn");
        assert_eq!(uri_to_path(&path_to_uri(&p)), p);
    }

    #[test]
    fn locals_are_found() {
        let text = "create f(a, b):\n    total = a + b\n    return total\nend\n";
        let span = local_binding(text, "total", 2).unwrap();
        assert_eq!((span.start.line, span.start.column), (2, 5));
        let span = local_binding(text, "b", 2).unwrap();
        assert_eq!((span.start.line, span.start.column), (1, 13));
    }
}
