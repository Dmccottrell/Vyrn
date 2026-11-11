//! The everyday standard library: the Prelude functions a Python
//! programmer reaches for (`input`, `chr`, `any`, `format`, ...), files from
//! `open`, sets, and the modules `vyrn.os`, `vyrn.fs`, `vyrn.path`, `vyrn.time`,
//! `vyrn.random`, `vyrn.math`, `vyrn.json`, `vyrn.re`, `vyrn.csv` and the HTTP
//! client half of `vyrn.http`.
//!
//! Functions that take a callback (`sorted(xs, key = ...)`, `xs.map(f)`)
//! are written in Vyrn itself, in `src/prelude.vyrn`; the natives they lean
//! on are the `__`-prefixed ones here.

use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::io::{BufRead, Read, Write};
use std::rc::Rc;

use super::builtins::{integer, number, opt};
use super::foreign::Foreign;
use super::ops::{self, err, type_error, Arith, Ctx, VmResult};
use super::value::{Key, Map, Value};

fn text_arg<'a>(args: &'a [Value], i: usize, what: &str) -> VmResult<&'a str> {
    match args.get(i) {
        Some(Value::Str(s)) => Ok(s.as_str()),
        Some(other) => Err(type_error(format!(
            "{what} must be a string, not {}",
            other.type_name()
        ))),
        None => Err(err("CallError", format!("{what} is missing"))),
    }
}

fn io_error(what: &str, e: std::io::Error) -> Value {
    let kind = match e.kind() {
        std::io::ErrorKind::NotFound => "NotFound",
        std::io::ErrorKind::PermissionDenied => "PermissionDenied",
        _ => "IoError",
    };
    err(kind, format!("{what}: {e}"))
}

fn map_value(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::default();
    for (k, v) in entries {
        let key = Value::str(k);
        if let Some(hashed) = Key::from_value(&key) {
            map.insert(hashed, key, v);
        }
    }
    Value::Map(Rc::new(RefCell::new(map)))
}

fn bool_arg(args: &[Value], i: usize) -> bool {
    opt(args, i).is_some_and(Value::truthy)
}

// ==== Prelude ===============================================================

/// `input(prompt = "")`: one line from the keyboard, without its newline.
/// `none` at the end of input.
pub fn input(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if let Some(prompt) = opt(args, 0) {
        let _ = write!(ctx.out, "{}", ops::display(&ctx.program, prompt));
        let _ = ctx.out.flush();
    }
    if let Some(lines) = &mut ctx.scripted_input {
        return Ok(lines.pop_front().map_or(Value::None, Value::str));
    }
    let mut line = String::new();
    let read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| io_error("cannot read input", e))?;
    if read == 0 {
        return Ok(Value::None);
    }
    while line.ends_with('\n') || line.ends_with('\r') {
        line.pop();
    }
    Ok(Value::str(line))
}

pub fn to_bool(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(args[0].truthy()))
}

/// `typeof(x)`: the name of a value's type, as error messages spell it.
pub fn type_of(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(match &args[0] {
        Value::Struct(inst) => ctx
            .program
            .symbols
            .name(ctx.program.types[inst.ty as usize].name)
            .to_string(),
        Value::Object(obj) => ctx
            .program
            .symbols
            .name(ctx.program.types[obj.borrow().ty as usize].name)
            .to_string(),
        other => other.type_name().to_string(),
    }))
}

/// `repr(x)`: how a value is written in code: strings get their quotes.
pub fn repr(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::str(match &args[0] {
        Value::Str(s) => quote(s),
        other => ops::display(&ctx.program, other),
    }))
}

pub fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '\r' => out.push_str("\\r"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

pub fn chr(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let code = integer(&args[0], "the character code")?;
    u32::try_from(code)
        .ok()
        .and_then(char::from_u32)
        .map(|c| Value::str(c.to_string()))
        .ok_or_else(|| err("ValueError", format!("{code} is not a character code")))
}

pub fn ord(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let s = text_arg(args, 0, "`ord`'s argument")?;
    let mut chars = s.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) => Ok(Value::Int(c as i64)),
        _ => Err(err(
            "ValueError",
            format!("`ord` takes one character, not {}", quote(s)),
        )),
    }
}

fn radix(args: &[Value], base: u32, prefix: &str) -> VmResult<Value> {
    let n = integer(&args[0], "the number")?;
    let digits = match base {
        16 => format!("{:x}", n.unsigned_abs()),
        8 => format!("{:o}", n.unsigned_abs()),
        _ => format!("{:b}", n.unsigned_abs()),
    };
    Ok(Value::str(format!(
        "{}{prefix}{digits}",
        if n < 0 { "-" } else { "" }
    )))
}

pub fn hex(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    radix(args, 16, "0x")
}

pub fn oct(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    radix(args, 8, "0o")
}

pub fn bin(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    radix(args, 2, "0b")
}

pub fn divmod(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let q = ops::arith(ctx, Arith::FloorDiv, &args[0], &args[1])?;
    let r = ops::arith(ctx, Arith::Rem, &args[0], &args[1])?;
    Ok(Value::tuple(vec![q, r]))
}

pub fn pow(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let p = ops::arith(ctx, Arith::Pow, &args[0], &args[1])?;
    match opt(args, 2) {
        Some(m) => ops::arith(ctx, Arith::Rem, &p, m),
        None => Ok(p),
    }
}

pub fn floor(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(match &args[0] {
        Value::Int(i) => Value::Int(*i),
        v => Value::Int(number(v, "the number")?.floor() as i64),
    })
}

pub fn ceil(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(match &args[0] {
        Value::Int(i) => Value::Int(*i),
        v => Value::Int(number(v, "the number")?.ceil() as i64),
    })
}

pub fn any(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(
        ops::collect(&args[0])?.iter().any(Value::truthy),
    ))
}

pub fn all(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(
        ops::collect(&args[0])?.iter().all(Value::truthy),
    ))
}

pub fn reversed(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if let Value::Str(s) = &args[0] {
        return Ok(Value::str(s.chars().rev().collect::<String>()));
    }
    let mut items = ops::collect(&args[0])?;
    items.reverse();
    Ok(Value::list(items))
}

/// `list(x)`: a new list of the items of anything that can be looped over.
/// `list()` is an empty list.
pub fn list(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    match opt(args, 0) {
        None => Ok(Value::list(Vec::new())),
        Some(Value::Str(s)) => Ok(Value::list(
            s.chars().map(|c| Value::str(c.to_string())).collect(),
        )),
        Some(v) => Ok(Value::list(ops::collect(v)?)),
    }
}

/// `dict(pairs)`: a map from a list of `(key, value)` pairs. `dict()` is
/// an empty map.
pub fn dict(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut map = Map::default();
    if let Some(source) = opt(args, 0) {
        if let Value::Map(m) = source {
            return Ok(Value::Map(Rc::new(RefCell::new(m.borrow().clone()))));
        }
        for pair in ops::collect(source)? {
            let items = ops::collect(&pair)?;
            let [k, v] = items.as_slice() else {
                return Err(type_error("`dict` takes a list of (key, value) pairs"));
            };
            let hashed = Key::from_value(k)
                .ok_or_else(|| type_error(format!("{} cannot be a map key", k.type_name())))?;
            map.insert(hashed, k.clone(), v.clone());
        }
    }
    Ok(Value::Map(Rc::new(RefCell::new(map))))
}

/// `__sorted(xs, reverse)`: what `sorted` does without a key.
pub fn sorted_plain(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let result = super::builtins::sorted(ctx, &args[..1])?;
    if bool_arg(args, 1) {
        if let Value::List(items) = &result {
            items.borrow_mut().reverse();
        }
    }
    Ok(result)
}

/// `__sort_by(xs, keys, reverse)`: `xs` ordered by `keys`, which the Vyrn
/// side computed with the caller's key function (once per item). Stable.
pub fn sort_by(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let items = ops::collect(&args[0])?;
    let keys = ops::collect(&args[1])?;
    let mut order: Vec<usize> = (0..items.len()).collect();
    let mut failure = None;
    order.sort_by(|&a, &b| {
        ops::order(&keys[a], &keys[b]).unwrap_or_else(|e| {
            failure.get_or_insert(e);
            std::cmp::Ordering::Equal
        })
    });
    if let Some(e) = failure {
        return Err(e);
    }
    if bool_arg(args, 2) {
        // Reversed but still stable: equal keys keep their order.
        order.reverse();
        let mut start = 0;
        while start < order.len() {
            let mut end = start + 1;
            while end < order.len() && ops::equals(&keys[order[start]], &keys[order[end]]) {
                end += 1;
            }
            order[start..end].reverse();
            start = end;
        }
    }
    Ok(Value::list(
        order.into_iter().map(|i| items[i].clone()).collect(),
    ))
}

/// `__pick_by(xs, keys, greatest)`: the item with the smallest (or
/// greatest) key; the first one on ties, as in Python.
pub fn pick_by(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let items = ops::collect(&args[0])?;
    let keys = ops::collect(&args[1])?;
    let greatest = bool_arg(args, 2);
    let mut best: Option<usize> = None;
    for i in 0..items.len() {
        best = Some(match best {
            None => i,
            Some(b) => {
                let o = ops::order(&keys[i], &keys[b])?;
                if (greatest && o.is_gt()) || (!greatest && o.is_lt()) {
                    i
                } else {
                    b
                }
            }
        });
    }
    best.map(|i| items[i].clone()).ok_or_else(|| {
        err(
            "ValueError",
            format!(
                "`{}` of an empty sequence",
                if greatest { "max" } else { "min" }
            ),
        )
    })
}

/// `__call_error(name)`: the message for a bad argument to a Vyrn-written
/// Prelude function.
pub fn prelude_fail(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let kind = text_arg(args, 0, "the error kind")?;
    let message = text_arg(args, 1, "the message")?;
    let kind: &'static str = match kind {
        "ValueError" => "ValueError",
        "IndexError" => "IndexError",
        "KeyError" => "KeyError",
        _ => "TypeError",
    };
    Err(err(kind, message.to_string()))
}

/// `fail ValueError("age must be positive")`: a built-in kind of error,
/// caught by `catch ValueError` (or `catch Error`).
fn error_of(kind: &'static str, args: &[Value]) -> VmResult<Value> {
    let message = match args.first() {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => format!("{other:?}"),
        None => String::new(),
    };
    Ok(err(kind, message))
}

pub fn make_error(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    error_of("Error", args)
}

pub fn make_value_error(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    error_of("ValueError", args)
}

pub fn make_type_error(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    error_of("TypeError", args)
}

pub fn make_key_error(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    error_of("KeyError", args)
}

pub fn make_index_error(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    error_of("IndexError", args)
}

// ==== format specs ==========================================================

/// `format(value, spec)`: what `"{value:spec}"` shows. Specs follow Python:
/// `[[fill]align][sign][#][0][width][,|_][.precision][type]`, as in `.2f`,
/// `>8`, `08.3f`, `,`, `.1%`, `x`.
pub fn format(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let spec = match opt(args, 1) {
        Some(Value::Str(s)) => s.to_string(),
        Some(other) => {
            return Err(type_error(format!(
                "a format spec is a string, not {}",
                other.type_name()
            )))
        }
        None => String::new(),
    };
    format_value(ctx, &args[0], &spec).map(Value::str)
}

pub fn format_value(ctx: &Ctx, value: &Value, spec: &str) -> VmResult<String> {
    let bad = || err("ValueError", format!("`{spec}` is not a format spec"));
    let chars: Vec<char> = spec.chars().collect();
    let mut i = 0;
    let mut fill = ' ';
    let mut align = None;
    let is_align = |c: char| matches!(c, '<' | '>' | '^' | '=');
    if chars.len() >= 2 && is_align(chars[1]) {
        fill = chars[0];
        align = Some(chars[1]);
        i = 2;
    } else if !chars.is_empty() && is_align(chars[0]) {
        align = Some(chars[0]);
        i = 1;
    }
    let mut sign = '-';
    if i < chars.len() && matches!(chars[i], '+' | '-' | ' ') {
        sign = chars[i];
        i += 1;
    }
    let mut alternate = false;
    if i < chars.len() && chars[i] == '#' {
        alternate = true;
        i += 1;
    }
    if i < chars.len() && chars[i] == '0' && align.is_none() {
        fill = '0';
        align = Some('=');
        i += 1;
    }
    let mut width = 0usize;
    while i < chars.len() && chars[i].is_ascii_digit() {
        width = width * 10 + chars[i].to_digit(10).unwrap_or(0) as usize;
        i += 1;
    }
    let mut grouping = None;
    if i < chars.len() && matches!(chars[i], ',' | '_') {
        grouping = Some(chars[i]);
        i += 1;
    }
    let mut precision = None;
    if i < chars.len() && chars[i] == '.' {
        i += 1;
        let mut p = 0usize;
        let start = i;
        while i < chars.len() && chars[i].is_ascii_digit() {
            p = p * 10 + chars[i].to_digit(10).unwrap_or(0) as usize;
            i += 1;
        }
        if i == start {
            return Err(bad());
        }
        precision = Some(p);
    }
    let kind = if i < chars.len() {
        i += 1;
        Some(chars[i - 1])
    } else {
        None
    };
    if i != chars.len() {
        return Err(bad());
    }

    let as_float = |v: &Value| -> VmResult<f64> {
        match v {
            Value::Int(n) => Ok(*n as f64),
            Value::Float(f) => Ok(*f),
            Value::Bool(b) => Ok(f64::from(u8::from(*b))),
            other => Err(type_error(format!(
                "`{spec}` formats numbers, not {}",
                other.type_name()
            ))),
        }
    };
    let as_int = |v: &Value| -> VmResult<i64> {
        match v {
            Value::Int(n) => Ok(*n),
            Value::Bool(b) => Ok(i64::from(*b)),
            other => Err(type_error(format!(
                "`{spec}` formats integers, not {}",
                other.type_name()
            ))),
        }
    };
    // (sign, digits) for numbers; the text for everything else.
    let (negative, mut body, numeric) = match kind {
        Some('f' | 'F') => {
            let x = as_float(value)?;
            (
                x.is_sign_negative() && x != 0.0,
                format!("{:.*}", precision.unwrap_or(6), x.abs()),
                true,
            )
        }
        Some('e' | 'E') => {
            let x = as_float(value)?;
            let text = python_exp(x.abs(), precision.unwrap_or(6));
            (
                x.is_sign_negative() && x != 0.0,
                if kind == Some('E') {
                    text.to_uppercase()
                } else {
                    text
                },
                true,
            )
        }
        Some('%') => {
            let x = as_float(value)? * 100.0;
            (
                x.is_sign_negative() && x != 0.0,
                format!("{:.*}%", precision.unwrap_or(6), x.abs()),
                true,
            )
        }
        Some('g' | 'G') => {
            let x = as_float(value)?;
            (
                x.is_sign_negative() && x != 0.0,
                general(x.abs(), precision),
                true,
            )
        }
        Some('d' | 'n') => {
            let n = as_int(value)?;
            (n < 0, n.unsigned_abs().to_string(), true)
        }
        Some('x' | 'X' | 'o' | 'b') => {
            let n = as_int(value)?;
            let k = kind.unwrap_or('d');
            let digits = match k {
                'x' => format!("{:x}", n.unsigned_abs()),
                'X' => format!("{:X}", n.unsigned_abs()),
                'o' => format!("{:o}", n.unsigned_abs()),
                _ => format!("{:b}", n.unsigned_abs()),
            };
            let prefix = if alternate {
                match k {
                    'x' => "0x",
                    'X' => "0X",
                    'o' => "0o",
                    _ => "0b",
                }
            } else {
                ""
            };
            (n < 0, format!("{prefix}{digits}"), true)
        }
        Some('c') => {
            let n = as_int(value)?;
            let c = u32::try_from(n)
                .ok()
                .and_then(char::from_u32)
                .ok_or_else(|| err("ValueError", format!("{n} is not a character code")))?;
            (false, c.to_string(), false)
        }
        Some('s') | None => match value {
            Value::Int(n) if kind.is_none() => (*n < 0, n.unsigned_abs().to_string(), true),
            Value::Float(x) if kind.is_none() => {
                let text = match precision {
                    Some(p) => general(x.abs(), Some(p)),
                    None => ops::display(&ctx.program, &Value::Float(x.abs())),
                };
                (x.is_sign_negative() && *x != 0.0, text, true)
            }
            other => {
                let mut text = ops::display(&ctx.program, other);
                if let Some(p) = precision {
                    text = text.chars().take(p).collect();
                }
                (false, text, false)
            }
        },
        Some(_) => return Err(bad()),
    };
    if let (Some(sep), true) = (grouping, numeric) {
        body = group_digits(&body, sep);
    }
    let sign_text = if !numeric {
        ""
    } else if negative {
        "-"
    } else {
        match sign {
            '+' => "+",
            ' ' => " ",
            _ => "",
        }
    };
    let len = sign_text.chars().count() + body.chars().count();
    if len >= width {
        return Ok(format!("{sign_text}{body}"));
    }
    let pad = width - len;
    let padding = |n: usize| fill.to_string().repeat(n);
    let align = align.unwrap_or(if numeric { '>' } else { '<' });
    Ok(match align {
        '<' => format!("{sign_text}{body}{}", padding(pad)),
        '^' => format!(
            "{}{sign_text}{body}{}",
            padding(pad / 2),
            padding(pad - pad / 2)
        ),
        '=' => format!("{sign_text}{}{body}", padding(pad)),
        _ => format!("{}{sign_text}{body}", padding(pad)),
    })
}

/// `1.5e+03`, as Python writes an exponent: a sign and at least two digits.
fn python_exp(x: f64, precision: usize) -> String {
    let text = format!("{:.*e}", precision, x);
    let (mantissa, exp) = text.split_once('e').unwrap_or((&text, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    format!(
        "{mantissa}e{}{:02}",
        if exp < 0 { '-' } else { '+' },
        exp.abs()
    )
}

/// Python's `g`: fixed or exponent, whichever is shorter, without trailing
/// zeros.
fn general(x: f64, precision: Option<usize>) -> String {
    let p = precision.unwrap_or(6).max(1);
    if x == 0.0 {
        return "0".into();
    }
    let exp = x.abs().log10().floor() as i32;
    let trim = |s: String| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    if exp < -4 || exp >= p as i32 {
        let text = python_exp(x, p - 1);
        let (m, e) = text.split_once('e').unwrap_or((&text, ""));
        format!("{}e{e}", trim(m.to_string()))
    } else {
        let decimals = (p as i32 - 1 - exp).max(0) as usize;
        trim(format!("{:.*}", decimals, x))
    }
}

fn group_digits(body: &str, sep: char) -> String {
    let (int_part, rest) = match body.find(|c: char| !c.is_ascii_digit()) {
        Some(i) => body.split_at(i),
        None => (body, ""),
    };
    let mut out = String::new();
    for (i, c) in int_part.chars().enumerate() {
        if i > 0 && (int_part.len() - i) % 3 == 0 {
            out.push(sep);
        }
        out.push(c);
    }
    out + rest
}

// ==== files =================================================================

/// An open file: what `open(path, mode)` returns.
pub struct File {
    path: String,
    mode: String,
    state: RefCell<FileState>,
}

enum FileState {
    Reading(std::io::BufReader<std::fs::File>),
    Writing(std::io::BufWriter<std::fs::File>),
    Closed,
}

/// `open(path, mode = "r")`: `"r"` reads, `"w"` replaces the file, `"a"`
/// adds to its end. Use it with `using`, which closes the file on every
/// way out of the block.
pub fn open(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?.to_string();
    let mode = match opt(args, 1) {
        Some(Value::Str(m)) => m.to_string(),
        Some(other) => {
            return Err(type_error(format!(
                "the mode must be a string such as \"r\" or \"w\", not {}",
                other.type_name()
            )))
        }
        None => "r".to_string(),
    };
    let what = format!("cannot open {path}");
    let state = match mode.trim_end_matches(['b', 't']) {
        "r" => FileState::Reading(std::io::BufReader::new(
            std::fs::File::open(&path).map_err(|e| io_error(&what, e))?,
        )),
        "w" => FileState::Writing(std::io::BufWriter::new(
            std::fs::File::create(&path).map_err(|e| io_error(&what, e))?,
        )),
        "a" => FileState::Writing(std::io::BufWriter::new(
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(&path)
                .map_err(|e| io_error(&what, e))?,
        )),
        other => {
            return Err(err(
                "ValueError",
                format!(
                    "unknown file mode {}: use \"r\", \"w\" or \"a\"",
                    quote(other)
                ),
            ))
        }
    };
    Ok(Value::Foreign(Rc::new(Box::new(File {
        path,
        mode,
        state: RefCell::new(state),
    }))))
}

impl File {
    fn reader<T>(
        &self,
        f: impl FnOnce(&mut std::io::BufReader<std::fs::File>) -> std::io::Result<T>,
    ) -> VmResult<T> {
        match &mut *self.state.borrow_mut() {
            FileState::Reading(r) => {
                f(r).map_err(|e| io_error(&format!("cannot read {}", self.path), e))
            }
            FileState::Writing(_) => Err(err(
                "IoError",
                format!(
                    "{} was opened for writing (mode \"{}\")",
                    self.path, self.mode
                ),
            )),
            FileState::Closed => Err(err("IoError", format!("{} is closed", self.path))),
        }
    }

    fn lines(&self) -> VmResult<Vec<Value>> {
        self.reader(|r| {
            let mut out = Vec::new();
            for line in r.lines() {
                let mut line = line?;
                if line.ends_with('\r') {
                    line.pop();
                }
                out.push(Value::str(line));
            }
            Ok(out)
        })
    }
}

impl Foreign for File {
    fn type_name(&self) -> &'static str {
        "File"
    }

    fn display(&self) -> String {
        format!("File({}, \"{}\")", quote(&self.path), self.mode)
    }

    fn get_attr(&self, name: &str) -> VmResult<Value> {
        match name {
            "path" => Ok(Value::str(self.path.clone())),
            "mode" => Ok(Value::str(self.mode.clone())),
            "closed" => Ok(Value::Bool(matches!(
                *self.state.borrow(),
                FileState::Closed
            ))),
            _ => Err(err("FieldError", format!("a File has no field `{name}`"))),
        }
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            "read" => self
                .reader(|r| {
                    let mut s = String::new();
                    r.read_to_string(&mut s)?;
                    Ok(s)
                })
                .map(Value::str),
            "readline" => self
                .reader(|r| {
                    let mut s = String::new();
                    r.read_line(&mut s)?;
                    Ok(s)
                })
                .map(Value::str),
            "lines" => self.lines().map(Value::list),
            "write" | "write_line" => {
                let mut text: String = args
                    .iter()
                    .map(|v| ops::display(&ctx.program, v))
                    .collect::<Vec<_>>()
                    .join("");
                if name == "write_line" {
                    text.push('\n');
                }
                match &mut *self.state.borrow_mut() {
                    FileState::Writing(w) => w
                        .write_all(text.as_bytes())
                        .map_err(|e| io_error(&format!("cannot write {}", self.path), e))?,
                    FileState::Reading(_) => {
                        return Err(err(
                            "IoError",
                            format!(
                                "{} was opened for reading; open it with \"w\" or \"a\" to write",
                                self.path
                            ),
                        ))
                    }
                    FileState::Closed => {
                        return Err(err("IoError", format!("{} is closed", self.path)))
                    }
                }
                Ok(Value::None)
            }
            "flush" => {
                if let FileState::Writing(w) = &mut *self.state.borrow_mut() {
                    w.flush()
                        .map_err(|e| io_error(&format!("cannot write {}", self.path), e))?;
                }
                Ok(Value::None)
            }
            "close" => {
                let old = std::mem::replace(&mut *self.state.borrow_mut(), FileState::Closed);
                if let FileState::Writing(mut w) = old {
                    w.flush()
                        .map_err(|e| io_error(&format!("cannot write {}", self.path), e))?;
                }
                Ok(Value::None)
            }
            _ => Err(err("FieldError", format!("a File has no method `{name}`"))),
        }
    }

    /// `each line in f`: the lines, without their line endings.
    fn items(&self) -> VmResult<Vec<Value>> {
        self.lines()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

impl Drop for File {
    fn drop(&mut self) {
        if let FileState::Writing(w) = &mut *self.state.borrow_mut() {
            let _ = w.flush();
        }
    }
}

// ==== sets ==================================================================

/// An unordered collection without duplicates: `{1, 2, 3}` or `set(xs)`.
/// Items keep the order they were first added in.
pub struct Set {
    items: RefCell<Map>,
}

impl Set {
    pub fn from_values(values: Vec<Value>) -> VmResult<Set> {
        let mut map = Map::default();
        for v in values {
            let key = Key::from_value(&v)
                .ok_or_else(|| type_error(format!("{} cannot be in a set", v.type_name())))?;
            map.insert(key, v, Value::None);
        }
        Ok(Set {
            items: RefCell::new(map),
        })
    }

    pub fn values(&self) -> Vec<Value> {
        self.items
            .borrow()
            .entries()
            .iter()
            .map(|(k, _)| k.clone())
            .collect()
    }

    fn has(&self, v: &Value) -> bool {
        Key::from_value(v).is_some_and(|k| self.items.borrow().contains(&k))
    }

    pub fn value(set: Set) -> Value {
        Value::Foreign(Rc::new(Box::new(set)))
    }

    pub fn of(value: &Value) -> Option<&Set> {
        match value {
            Value::Foreign(f) => f.as_any().downcast_ref::<Set>(),
            _ => None,
        }
    }

    fn combine(&self, other: &Value, op: &str) -> VmResult<Value> {
        let other_items = match Set::of(other) {
            Some(s) => s.values(),
            None => ops::collect(other)?,
        };
        let other_set = Set::from_values(other_items.clone())?;
        let mine = self.values();
        let result: Vec<Value> = match op {
            "union" => mine.into_iter().chain(other_items).collect(),
            "intersection" => mine.into_iter().filter(|v| other_set.has(v)).collect(),
            "difference" => mine.into_iter().filter(|v| !other_set.has(v)).collect(),
            _ => mine
                .iter()
                .filter(|v| !other_set.has(v))
                .cloned()
                .chain(other_items.into_iter().filter(|v| !self.has(v)))
                .collect(),
        };
        Ok(Set::value(Set::from_values(result)?))
    }
}

/// `set(xs)`: the distinct items of `xs`. `set()` is empty.
pub fn set(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let items = match opt(args, 0) {
        Some(v) => match Set::of(v) {
            Some(s) => s.values(),
            None => ops::collect(v)?,
        },
        None => Vec::new(),
    };
    Ok(Set::value(Set::from_values(items)?))
}

impl Foreign for Set {
    fn type_name(&self) -> &'static str {
        "set"
    }

    fn display(&self) -> String {
        let items = self.values();
        if items.is_empty() {
            return "set()".into();
        }
        let parts: Vec<String> = items.iter().map(display_plain).collect();
        format!("{{{}}}", parts.join(", "))
    }

    fn call_method(
        &self,
        _: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        let arg = |i: usize| {
            args.get(i)
                .ok_or_else(|| err("CallError", format!("`.{name}()` is missing an argument")))
        };
        match name {
            "add" => {
                let v = arg(0)?;
                let key = Key::from_value(v)
                    .ok_or_else(|| type_error(format!("{} cannot be in a set", v.type_name())))?;
                self.items.borrow_mut().insert(key, v.clone(), Value::None);
                Ok(Value::None)
            }
            "remove" | "discard" => {
                let v = arg(0)?;
                let removed = Key::from_value(v)
                    .and_then(|k| self.items.borrow_mut().remove(&k))
                    .is_some();
                if !removed && name == "remove" {
                    return Err(err(
                        "KeyError",
                        format!("{} is not in the set", display_plain(v)),
                    ));
                }
                Ok(Value::None)
            }
            "has" | "contains" => Ok(Value::Bool(self.has(arg(0)?))),
            "clear" => {
                *self.items.borrow_mut() = Map::default();
                Ok(Value::None)
            }
            "copy" => Ok(Set::value(Set::from_values(self.values())?)),
            "union" | "intersection" | "difference" | "symmetric_difference" => {
                self.combine(arg(0)?, name)
            }
            "issubset" => {
                let other = Set::from_values(ops::collect(arg(0)?).or_else(|_| {
                    Set::of(arg(0)?)
                        .map(Set::values)
                        .ok_or_else(|| type_error("expected a set"))
                })?)?;
                Ok(Value::Bool(self.values().iter().all(|v| other.has(v))))
            }
            "issuperset" => {
                let items = match Set::of(arg(0)?) {
                    Some(s) => s.values(),
                    None => ops::collect(arg(0)?)?,
                };
                Ok(Value::Bool(items.iter().all(|v| self.has(v))))
            }
            "list" => Ok(Value::list(self.values())),
            _ => Err(err("FieldError", format!("a set has no method `{name}`"))),
        }
    }

    fn len(&self) -> VmResult<usize> {
        Ok(self.items.borrow().len())
    }

    fn items(&self) -> VmResult<Vec<Value>> {
        Ok(self.values())
    }

    fn contains(&self, item: &Value) -> VmResult<bool> {
        Ok(self.has(item))
    }

    fn binary(&self, op: Arith, other: &Value, flipped: bool) -> Option<VmResult<Value>> {
        Set::of(other)?;
        let name = match op {
            Arith::BitOr => "union",
            Arith::BitAnd => "intersection",
            Arith::BitXor => "symmetric_difference",
            Arith::Sub if !flipped => "difference",
            _ => return None,
        };
        Some(self.combine(other, name))
    }

    fn equals(&self, other: &Value) -> bool {
        Set::of(other).is_some_and(|o| {
            o.items.borrow().len() == self.items.borrow().len()
                && self.values().iter().all(|v| o.has(v))
        })
    }

    fn truthy(&self) -> bool {
        !self.items.borrow().is_empty()
    }

    fn json(&self) -> Option<Value> {
        Some(Value::list(self.values()))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// A value shown without a program (sets hold only hashable values, which
/// display the same without one).
fn display_plain(v: &Value) -> String {
    match v {
        Value::Str(s) => quote(s),
        Value::Int(i) => i.to_string(),
        Value::Float(f) => {
            let text = f.to_string();
            if text.contains(['.', 'e', 'i', 'N']) {
                text
            } else {
                format!("{text}.0")
            }
        }
        Value::Bool(b) => b.to_string(),
        Value::None => "none".into(),
        Value::Tuple(items) => {
            let parts: Vec<String> = items.iter().map(display_plain).collect();
            if parts.len() == 1 {
                format!("({},)", parts[0])
            } else {
                format!("({})", parts.join(", "))
            }
        }
        other => other.type_name().to_string(),
    }
}

// ==== vyrn.os ================================================================

thread_local! {
    static ARGS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// The arguments after the script's name, for `os.args()`. Set by `vyrn run`.
pub fn set_args(args: Vec<String>) {
    ARGS.with(|a| *a.borrow_mut() = args);
}

pub fn os_args(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(ARGS.with(|a| Value::list(a.borrow().iter().map(|s| Value::str(s.clone())).collect())))
}

pub fn os_env(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let name = text_arg(args, 0, "the variable name")?;
    Ok(match std::env::var(name) {
        Ok(v) => Value::str(v),
        Err(_) => opt(args, 1).cloned().unwrap_or(Value::None),
    })
}

pub fn os_set_env(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let name = text_arg(args, 0, "the variable name")?;
    let value = ops::display(&ctx.program, &args[1]);
    std::env::set_var(name, value);
    Ok(Value::None)
}

pub fn os_environ(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    let mut vars: Vec<(String, String)> = std::env::vars().collect();
    vars.sort();
    let mut map = Map::default();
    for (k, v) in vars {
        let key = Value::str(k);
        if let Some(h) = Key::from_value(&key) {
            map.insert(h, key, Value::str(v));
        }
    }
    Ok(Value::Map(Rc::new(RefCell::new(map))))
}

pub fn os_cwd(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    std::env::current_dir()
        .map(|p| Value::str(p.to_string_lossy().into_owned()))
        .map_err(|e| io_error("cannot read the current directory", e))
}

pub fn os_chdir(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the directory")?;
    std::env::set_current_dir(path)
        .map_err(|e| io_error(&format!("cannot change to {path}"), e))?;
    Ok(Value::None)
}

pub fn os_exit(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let code = match opt(args, 0) {
        Some(v) => integer(v, "the exit code")?,
        None => 0,
    };
    let _ = ctx.out.flush();
    std::process::exit(code as i32)
}

pub fn os_platform(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(Value::str(std::env::consts::OS))
}

/// `os.run(command, input = none)`: runs a program and waits for it.
/// `command` is a list (`["git", "status"]`) or a string run by the shell.
/// The result has `code`, `out` and `err`; `ok` is true when `code` is 0.
pub fn os_run(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut command = match &args[0] {
        Value::Str(line) => {
            if cfg!(windows) {
                let mut c = std::process::Command::new("cmd");
                c.args(["/C", line.as_str()]);
                c
            } else {
                let mut c = std::process::Command::new("sh");
                c.args(["-c", line.as_str()]);
                c
            }
        }
        other => {
            let parts: Vec<String> = ops::collect(other)?
                .iter()
                .map(|v| ops::display(&ctx.program, v))
                .collect();
            let Some((program, rest)) = parts.split_first() else {
                return Err(err("ValueError", "`os.run` needs a command"));
            };
            let mut c = std::process::Command::new(program);
            c.args(rest);
            c
        }
    };
    let input = opt(args, 1).map(|v| ops::display(&ctx.program, v));
    command
        .stdin(if input.is_some() {
            std::process::Stdio::piped()
        } else {
            std::process::Stdio::inherit()
        })
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|e| io_error("cannot start the command", e))?;
    if let (Some(text), Some(mut stdin)) = (input, child.stdin.take()) {
        let _ = stdin.write_all(text.as_bytes());
    }
    let output = child
        .wait_with_output()
        .map_err(|e| io_error("the command failed", e))?;
    let code = output.status.code().unwrap_or(-1);
    Ok(map_value(vec![
        ("code", Value::Int(i64::from(code))),
        ("ok", Value::Bool(code == 0)),
        (
            "out",
            Value::str(String::from_utf8_lossy(&output.stdout).into_owned()),
        ),
        (
            "err",
            Value::str(String::from_utf8_lossy(&output.stderr).into_owned()),
        ),
    ]))
}

// ==== vyrn.fs and vyrn.path ===================================================

pub fn fs_exists(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(
        std::path::Path::new(text_arg(args, 0, "the path")?).exists(),
    ))
}

pub fn fs_is_file(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(
        std::path::Path::new(text_arg(args, 0, "the path")?).is_file(),
    ))
}

pub fn fs_is_dir(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(
        std::path::Path::new(text_arg(args, 0, "the path")?).is_dir(),
    ))
}

/// `fs.list(dir = ".")`: the names in a directory, sorted.
pub fn fs_list(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let dir = match opt(args, 0) {
        Some(_) => text_arg(args, 0, "the directory")?,
        None => ".",
    };
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| io_error(&format!("cannot list {dir}"), e))?
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    Ok(Value::list(names.into_iter().map(Value::str).collect()))
}

/// `fs.walk(dir)`: every file under a directory, as paths, sorted.
pub fn fs_walk(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let dir = text_arg(args, 0, "the directory")?;
    let mut out = Vec::new();
    let mut pending = vec![std::path::PathBuf::from(dir)];
    while let Some(d) = pending.pop() {
        let entries = std::fs::read_dir(&d)
            .map_err(|e| io_error(&format!("cannot list {}", d.display()), e))?;
        for entry in entries.filter_map(Result::ok) {
            let path = entry.path();
            if path.is_dir() {
                pending.push(path);
            } else {
                out.push(path.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    Ok(Value::list(out.into_iter().map(Value::str).collect()))
}

pub fn fs_mkdir(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the directory")?;
    std::fs::create_dir_all(path).map_err(|e| io_error(&format!("cannot create {path}"), e))?;
    Ok(Value::None)
}

pub fn fs_remove(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let p = std::path::Path::new(path);
    let result = if p.is_dir() {
        std::fs::remove_dir_all(p)
    } else {
        std::fs::remove_file(p)
    };
    result.map_err(|e| io_error(&format!("cannot remove {path}"), e))?;
    Ok(Value::None)
}

pub fn fs_rename(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let from = text_arg(args, 0, "the old path")?;
    let to = text_arg(args, 1, "the new path")?;
    std::fs::rename(from, to).map_err(|e| io_error(&format!("cannot move {from} to {to}"), e))?;
    Ok(Value::None)
}

pub fn fs_copy(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let from = text_arg(args, 0, "the source")?;
    let to = text_arg(args, 1, "the destination")?;
    std::fs::copy(from, to).map_err(|e| io_error(&format!("cannot copy {from} to {to}"), e))?;
    Ok(Value::None)
}

pub fn fs_size(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    std::fs::metadata(path)
        .map(|m| Value::Int(m.len() as i64))
        .map_err(|e| io_error(&format!("cannot read {path}"), e))
}

/// `fs.modified(path)`: when the file last changed, in seconds since 1970.
pub fn fs_modified(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let m = std::fs::metadata(path)
        .and_then(|m| m.modified())
        .map_err(|e| io_error(&format!("cannot read {path}"), e))?;
    Ok(Value::Float(
        m.duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0),
    ))
}

pub fn fs_read_lines(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let text =
        std::fs::read_to_string(path).map_err(|e| io_error(&format!("cannot read {path}"), e))?;
    Ok(Value::list(text.lines().map(Value::str).collect()))
}

pub fn fs_append_text(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let text = ops::display(&ctx.program, &args[1]);
    std::fs::OpenOptions::new()
        .append(true)
        .create(true)
        .open(path)
        .and_then(|mut f| f.write_all(text.as_bytes()))
        .map_err(|e| io_error(&format!("cannot write {path}"), e))?;
    Ok(Value::None)
}

/// `read_bytes("photo.png")`: the file's bytes, as a list of numbers from
/// 0 to 255.
pub fn fs_read_bytes(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let data = std::fs::read(path).map_err(|e| io_error(&format!("cannot read {path}"), e))?;
    Ok(Value::list(
        data.into_iter().map(|b| Value::Int(i64::from(b))).collect(),
    ))
}

/// `write_bytes("copy.png", data)`: `data` is a list of numbers from 0 to
/// 255 (or text, written as UTF-8).
pub fn fs_write_bytes(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let data: Vec<u8> = match args.get(1) {
        Some(Value::Str(s)) => s.as_bytes().to_vec(),
        Some(v) => ops::collect(v)?
            .iter()
            .map(|b| match b {
                Value::Int(i) if (0..=255).contains(i) => Ok(*i as u8),
                other => Err(err(
                    "ValueError",
                    format!("a byte is a number from 0 to 255, not {other:?}"),
                )),
            })
            .collect::<VmResult<_>>()?,
        None => return Err(err("CallError", "`write_bytes` needs the data to write")),
    };
    std::fs::write(path, data).map_err(|e| io_error(&format!("cannot write {path}"), e))?;
    Ok(Value::None)
}

/// `path.join("a", "b", "c.txt")`
pub fn path_join(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut p = std::path::PathBuf::new();
    for a in args {
        p.push(ops::display(&ctx.program, a));
    }
    Ok(Value::str(p.to_string_lossy().into_owned()))
}

fn path_part(args: &[Value], f: impl Fn(&std::path::Path) -> Option<String>) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    Ok(Value::str(
        f(std::path::Path::new(path)).unwrap_or_default(),
    ))
}

/// `path.name("a/b.txt")` is `"b.txt"`.
pub fn path_name(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    path_part(args, |p| {
        p.file_name().map(|s| s.to_string_lossy().into_owned())
    })
}

/// `path.stem("a/b.txt")` is `"b"`.
pub fn path_stem(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    path_part(args, |p| {
        p.file_stem().map(|s| s.to_string_lossy().into_owned())
    })
}

/// `path.ext("a/b.txt")` is `".txt"`.
pub fn path_ext(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    path_part(args, |p| {
        p.extension().map(|s| format!(".{}", s.to_string_lossy()))
    })
}

/// `path.parent("a/b.txt")` is `"a"`.
pub fn path_parent(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    path_part(args, |p| {
        p.parent().map(|s| s.to_string_lossy().into_owned())
    })
}

pub fn path_absolute(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let p = std::path::Path::new(path);
    let abs = if p.is_absolute() {
        p.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| io_error("cannot read the current directory", e))?
            .join(p)
    };
    Ok(Value::str(abs.to_string_lossy().into_owned()))
}

pub fn path_home(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .map(Value::str)
        .unwrap_or(Value::None))
}

// ==== vyrn.time ==============================================================

fn unix_now() -> f64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// `time.now()`: seconds since 1970-01-01 UTC, with a fraction.
pub fn time_now(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    if cfg!(target_arch = "wasm32") {
        return Err(err("Unavailable", "this platform has no clock"));
    }
    Ok(Value::Float(unix_now()))
}

/// Days since 1970-01-01 to (year, month, day). Howard Hinnant's algorithm.
fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (if m <= 2 { y + 1 } else { y }, m, d)
}

fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

struct Parts {
    year: i64,
    month: i64,
    day: i64,
    hour: i64,
    minute: i64,
    second: i64,
    micro: i64,
    weekday: i64,
}

fn parts(ts: f64) -> Parts {
    let secs = ts.floor() as i64;
    let micro = ((ts - ts.floor()) * 1e6).round() as i64;
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    Parts {
        year,
        month,
        day,
        hour: rem / 3600,
        minute: rem % 3600 / 60,
        second: rem % 60,
        micro,
        // 1970-01-01 was a Thursday; Monday is 0, as in Python.
        weekday: (days + 3).rem_euclid(7),
    }
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];

/// `time.format(ts, pattern = "%Y-%m-%d %H:%M:%S")`, in UTC. Supports
/// `%Y %m %d %H %M %S %f %j %A %a %B %b %y %p %I %%`.
pub fn time_format(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let ts = match opt(args, 0) {
        Some(v) => number(v, "the time")?,
        None => unix_now(),
    };
    let pattern = match opt(args, 1) {
        Some(_) => text_arg(args, 1, "the pattern")?,
        None => "%Y-%m-%d %H:%M:%S",
    };
    let p = parts(ts);
    let mut out = String::new();
    let mut chars = pattern.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        let _ = match chars.next() {
            Some('Y') => write!(out, "{:04}", p.year),
            Some('y') => write!(out, "{:02}", p.year.rem_euclid(100)),
            Some('m') => write!(out, "{:02}", p.month),
            Some('d') => write!(out, "{:02}", p.day),
            Some('H') => write!(out, "{:02}", p.hour),
            Some('I') => write!(out, "{:02}", (p.hour + 11) % 12 + 1),
            Some('p') => write!(out, "{}", if p.hour < 12 { "AM" } else { "PM" }),
            Some('M') => write!(out, "{:02}", p.minute),
            Some('S') => write!(out, "{:02}", p.second),
            Some('f') => write!(out, "{:06}", p.micro),
            Some('j') => write!(
                out,
                "{:03}",
                days_from_civil(p.year, p.month, p.day) - days_from_civil(p.year, 1, 1) + 1
            ),
            Some('A') => write!(out, "{}", DAYS[p.weekday as usize]),
            Some('a') => write!(out, "{}", &DAYS[p.weekday as usize][..3]),
            Some('B') => write!(out, "{}", MONTHS[p.month as usize - 1]),
            Some('b') => write!(out, "{}", &MONTHS[p.month as usize - 1][..3]),
            Some('%') => write!(out, "%"),
            Some(other) => {
                return Err(err(
                    "ValueError",
                    format!("`%{other}` is not a time format code"),
                ))
            }
            None => write!(out, "%"),
        };
    }
    Ok(Value::str(out))
}

/// `time.date(ts = now)`: the calendar parts of a time, in UTC.
pub fn time_date(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let ts = match opt(args, 0) {
        Some(v) => number(v, "the time")?,
        None => unix_now(),
    };
    let p = parts(ts);
    Ok(map_value(vec![
        ("year", Value::Int(p.year)),
        ("month", Value::Int(p.month)),
        ("day", Value::Int(p.day)),
        ("hour", Value::Int(p.hour)),
        ("minute", Value::Int(p.minute)),
        ("second", Value::Int(p.second)),
        ("weekday", Value::str(DAYS[p.weekday as usize])),
    ]))
}

/// `time.parse("2026-11-11", pattern = "%Y-%m-%d")`: the time a text
/// names, in seconds since 1970 UTC. Understands `%Y %m %d %H %M %S`.
pub fn time_parse(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let text = text_arg(args, 0, "the text")?;
    let pattern = match opt(args, 1) {
        Some(_) => text_arg(args, 1, "the pattern")?,
        None => {
            if text.len() > 10 {
                "%Y-%m-%d %H:%M:%S"
            } else {
                "%Y-%m-%d"
            }
        }
    };
    let bad = || {
        err(
            "ValueError",
            format!("{} does not match {}", quote(text), quote(pattern)),
        )
    };
    let (mut y, mut mo, mut d, mut h, mut mi, mut s) = (1970, 1, 1, 0, 0, 0);
    let t: Vec<char> = text.chars().collect();
    let mut i = 0;
    let mut pat = pattern.chars();
    while let Some(c) = pat.next() {
        if c != '%' {
            if t.get(i) != Some(&c) && !(c == ' ' && t.get(i) == Some(&'T')) {
                return Err(bad());
            }
            i += 1;
            continue;
        }
        let code = pat.next().ok_or_else(bad)?;
        let max = if code == 'Y' { 4 } else { 2 };
        let start = i;
        while i < t.len() && i - start < max && t[i].is_ascii_digit() {
            i += 1;
        }
        let n: i64 = t[start..i]
            .iter()
            .collect::<String>()
            .parse()
            .map_err(|_| bad())?;
        match code {
            'Y' => y = n,
            'm' => mo = n,
            'd' => d = n,
            'H' => h = n,
            'M' => mi = n,
            'S' => s = n,
            _ => return Err(bad()),
        }
    }
    if i != t.len() || !(1..=12).contains(&mo) || !(1..=31).contains(&d) {
        return Err(bad());
    }
    Ok(Value::Float(
        (days_from_civil(y, mo, d) * 86_400 + h * 3600 + mi * 60 + s) as f64,
    ))
}

// ==== vyrn.random ============================================================

thread_local! {
    static RNG: RefCell<u64> = const { RefCell::new(0) };
}

fn next_u64() -> u64 {
    RNG.with(|r| {
        let mut x = *r.borrow();
        if x == 0 {
            x = (unix_now() * 1e9) as u64 ^ 0x9E37_79B9_7F4A_7C15;
            x |= 1;
        }
        // xorshift64*
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *r.borrow_mut() = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    })
}

fn next_f64() -> f64 {
    (next_u64() >> 11) as f64 / (1u64 << 53) as f64
}

fn below(n: u64) -> u64 {
    if n == 0 {
        return 0;
    }
    // Rejection sampling, so every value is equally likely.
    let zone = u64::MAX - u64::MAX % n;
    loop {
        let x = next_u64();
        if x < zone {
            return x % n;
        }
    }
}

/// `random.seed(n)`: the same seed gives the same numbers.
pub fn random_seed(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = integer(&args[0], "the seed")? as u64;
    RNG.with(|r| *r.borrow_mut() = n.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1);
    Ok(Value::None)
}

/// `random.float()`: in [0, 1).
pub fn random_float(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    match (opt(args, 0), opt(args, 1)) {
        (Some(a), Some(b)) => {
            let (a, b) = (number(a, "the low end")?, number(b, "the high end")?);
            Ok(Value::Float(a + (b - a) * next_f64()))
        }
        _ => Ok(Value::Float(next_f64())),
    }
}

/// `random.int(low, high)`: a whole number from low to high, both included.
pub fn random_int(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let a = integer(&args[0], "the low end")?;
    let b = integer(&args[1], "the high end")?;
    if b < a {
        return Err(err(
            "ValueError",
            format!("random.int({a}, {b}): the high end is below the low end"),
        ));
    }
    Ok(Value::Int(a + below((b - a) as u64 + 1) as i64))
}

pub fn random_choice(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let items = ops::collect(&args[0])?;
    if items.is_empty() {
        return Err(err("IndexError", "`random.choice` of an empty list"));
    }
    Ok(items[below(items.len() as u64) as usize].clone())
}

/// `random.shuffle(xs)`: puts a list in random order, in place.
pub fn random_shuffle(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let Value::List(items) = &args[0] else {
        return Err(type_error("`random.shuffle` takes a list"));
    };
    let mut items = items.borrow_mut();
    for i in (1..items.len()).rev() {
        let j = below(i as u64 + 1) as usize;
        items.swap(i, j);
    }
    Ok(Value::None)
}

/// `random.sample(xs, k)`: k different items.
pub fn random_sample(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let mut items = ops::collect(&args[0])?;
    let k = integer(&args[1], "the count")?;
    let k = usize::try_from(k).map_err(|_| err("ValueError", "the count must not be negative"))?;
    if k > items.len() {
        return Err(err(
            "ValueError",
            format!("cannot take {k} items from {}", items.len()),
        ));
    }
    for i in 0..k {
        let j = i + below((items.len() - i) as u64) as usize;
        items.swap(i, j);
    }
    items.truncate(k);
    Ok(Value::list(items))
}

// ==== vyrn.math ==============================================================

fn float_fn(args: &[Value], f: fn(f64) -> f64) -> VmResult<Value> {
    Ok(Value::Float(f(number(&args[0], "the number")?)))
}

pub fn math_log(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let x = number(&args[0], "the number")?;
    if x <= 0.0 {
        return Err(err(
            "ValueError",
            format!("log of {x}: the number must be positive"),
        ));
    }
    Ok(Value::Float(match opt(args, 1) {
        Some(b) => x.ln() / number(b, "the base")?.ln(),
        None => x.ln(),
    }))
}

pub fn math_log2(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::log2)
}
pub fn math_log10(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::log10)
}
pub fn math_sin(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::sin)
}
pub fn math_cos(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::cos)
}
pub fn math_tan(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::tan)
}
pub fn math_asin(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::asin)
}
pub fn math_acos(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::acos)
}
pub fn math_atan(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::atan)
}
pub fn math_degrees(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::to_degrees)
}
pub fn math_radians(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    float_fn(args, f64::to_radians)
}

pub fn math_atan2(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Float(
        number(&args[0], "y")?.atan2(number(&args[1], "x")?),
    ))
}

pub fn math_hypot(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Float(
        number(&args[0], "x")?.hypot(number(&args[1], "y")?),
    ))
}

pub fn math_isnan(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(number(&args[0], "the number")?.is_nan()))
}

pub fn math_isinf(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(number(&args[0], "the number")?.is_infinite()))
}

pub fn math_gcd(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (mut a, mut b) = (
        integer(&args[0], "a")?.unsigned_abs(),
        integer(&args[1], "b")?.unsigned_abs(),
    );
    while b != 0 {
        (a, b) = (b, a % b);
    }
    Ok(Value::Int(a as i64))
}

pub fn math_factorial(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let n = integer(&args[0], "the number")?;
    if n < 0 {
        return Err(err("ValueError", "factorial of a negative number"));
    }
    let mut r: i64 = 1;
    for i in 2..=n {
        r = r
            .checked_mul(i)
            .ok_or_else(|| err("Overflow", format!("{n}! is too large for an int")))?;
    }
    Ok(Value::Int(r))
}

pub fn math_clamp(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let (x, lo, hi) = (&args[0], &args[1], &args[2]);
    let _ = ctx;
    Ok(if ops::order(x, lo)?.is_lt() {
        lo.clone()
    } else if ops::order(x, hi)?.is_gt() {
        hi.clone()
    } else {
        x.clone()
    })
}

// ==== vyrn.json ==============================================================

/// `json.dumps(value, indent = none)`
pub fn json_dumps(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let text = ops::to_json(&ctx.program, &args[0]);
    match opt(args, 1) {
        Some(v) => {
            let indent = integer(v, "the indent")?.max(0) as usize;
            let json = super::json::parse(&text).map_err(|e| err("ValueError", e))?;
            let mut out = String::new();
            pretty_json(&json, indent, 0, &mut out);
            Ok(Value::str(out))
        }
        None => Ok(Value::str(text)),
    }
}

fn pretty_json(json: &super::json::Json, indent: usize, depth: usize, out: &mut String) {
    use super::json::Json;
    let pad = |d: usize| " ".repeat(indent * d);
    match json {
        Json::Array(items) if !items.is_empty() => {
            out.push_str("[\n");
            for (i, item) in items.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                pretty_json(item, indent, depth + 1, out);
                out.push_str(if i + 1 < items.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push(']');
        }
        Json::Object(entries) if !entries.is_empty() => {
            out.push_str("{\n");
            for (i, (k, v)) in entries.iter().enumerate() {
                out.push_str(&pad(depth + 1));
                out.push_str(&json_string(k));
                out.push_str(": ");
                pretty_json(v, indent, depth + 1, out);
                out.push_str(if i + 1 < entries.len() { ",\n" } else { "\n" });
            }
            out.push_str(&pad(depth));
            out.push('}');
        }
        Json::Array(_) => out.push_str("[]"),
        Json::Object(_) => out.push_str("{}"),
        Json::Null => out.push_str("null"),
        Json::Bool(b) => out.push_str(if *b { "true" } else { "false" }),
        Json::Number(n) => {
            if n.fract() == 0.0 && n.abs() < 9.0e15 {
                let _ = write!(out, "{}", *n as i64);
            } else {
                let _ = write!(out, "{n}");
            }
        }
        Json::String(s) => out.push_str(&json_string(s)),
    }
}

fn json_string(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
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
    out
}

pub fn json_load(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let text =
        std::fs::read_to_string(path).map_err(|e| io_error(&format!("cannot read {path}"), e))?;
    let json = super::json::parse(&text).map_err(|e| err("ValueError", format!("{path}: {e}")))?;
    Ok(ops::from_json(&json))
}

pub fn json_save(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?.to_string();
    let text = json_dumps(ctx, &[args[1].clone(), Value::Int(2)])?;
    let text = ops::display(&ctx.program, &text);
    std::fs::write(&path, text + "\n").map_err(|e| io_error(&format!("cannot write {path}"), e))?;
    Ok(Value::None)
}

// ==== vyrn.re ================================================================

thread_local! {
    static PATTERNS: RefCell<HashMap<String, Rc<regex::Regex>>> = RefCell::new(HashMap::new());
}

fn regex(pattern: &str) -> VmResult<Rc<regex::Regex>> {
    PATTERNS.with(|cache| {
        if let Some(r) = cache.borrow().get(pattern) {
            return Ok(r.clone());
        }
        let compiled = regex::Regex::new(pattern)
            .map_err(|e| err("ValueError", format!("bad pattern {}: {e}", quote(pattern))))?;
        let compiled = Rc::new(compiled);
        let mut cache = cache.borrow_mut();
        if cache.len() > 256 {
            cache.clear();
        }
        cache.insert(pattern.to_string(), compiled.clone());
        Ok(compiled)
    })
}

/// A match as a value: `text`, `start`, `end`, and `groups` (a list, with
/// `none` for groups that did not take part), plus named groups in `named`.
fn match_value(re: &regex::Regex, caps: &regex::Captures, text: &str) -> Value {
    let whole = caps.get(0).expect("group 0 always matches");
    let char_index = |byte: usize| text[..byte].chars().count() as i64;
    let groups: Vec<Value> = (1..caps.len())
        .map(|i| {
            caps.get(i)
                .map(|m| Value::str(m.as_str()))
                .unwrap_or(Value::None)
        })
        .collect();
    let mut named = Vec::new();
    for name in re.capture_names().flatten() {
        named.push((
            name,
            caps.name(name)
                .map(|m| Value::str(m.as_str()))
                .unwrap_or(Value::None),
        ));
    }
    map_value(vec![
        ("text", Value::str(whole.as_str())),
        ("start", Value::Int(char_index(whole.start()))),
        ("end", Value::Int(char_index(whole.end()))),
        ("groups", Value::list(groups)),
        ("named", map_value(named)),
    ])
}

/// `re.test(pattern, text)`: whether the pattern occurs anywhere.
pub fn re_test(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let re = regex(text_arg(args, 0, "the pattern")?)?;
    Ok(Value::Bool(re.is_match(text_arg(args, 1, "the text")?)))
}

/// `re.find(pattern, text)`: the first match, or `none`.
pub fn re_find(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let re = regex(text_arg(args, 0, "the pattern")?)?;
    let text = text_arg(args, 1, "the text")?;
    Ok(re
        .captures(text)
        .map(|c| match_value(&re, &c, text))
        .unwrap_or(Value::None))
}

/// `re.find_all(pattern, text)`: every match's text, or with groups, every
/// match's groups (one group: just its text), as Python's `findall`.
pub fn re_find_all(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let re = regex(text_arg(args, 0, "the pattern")?)?;
    let text = text_arg(args, 1, "the text")?;
    let groups = re.captures_len() - 1;
    Ok(Value::list(
        re.captures_iter(text)
            .map(|c| {
                let get = |i: usize| {
                    c.get(i)
                        .map(|m| Value::str(m.as_str()))
                        .unwrap_or_else(|| Value::str(""))
                };
                match groups {
                    0 => get(0),
                    1 => get(1),
                    n => Value::tuple((1..=n).map(get).collect()),
                }
            })
            .collect(),
    ))
}

/// `re.replace(pattern, text, with)`: every match replaced; `$1` or
/// `${name}` in `with` inserts a group.
pub fn re_replace(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let re = regex(text_arg(args, 0, "the pattern")?)?;
    let text = text_arg(args, 1, "the text")?;
    let replacement = text_arg(args, 2, "the replacement")?;
    Ok(Value::str(re.replace_all(text, replacement).into_owned()))
}

pub fn re_split(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let re = regex(text_arg(args, 0, "the pattern")?)?;
    let text = text_arg(args, 1, "the text")?;
    Ok(Value::list(re.split(text).map(Value::str).collect()))
}

// ==== vyrn.csv ===============================================================

/// Splits CSV text into rows of fields. Handles quotes, `""` inside quotes,
/// and line breaks inside quoted fields.
pub fn csv_rows_pub(text: &str, sep: char) -> Vec<Vec<String>> {
    csv_rows(text, sep)
}

fn csv_rows(text: &str, sep: char) -> Vec<Vec<String>> {
    let mut rows = Vec::new();
    let mut row = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut any = false;
    while let Some(c) = chars.next() {
        any = true;
        if quoted {
            if c == '"' {
                if chars.peek() == Some(&'"') {
                    field.push('"');
                    chars.next();
                } else {
                    quoted = false;
                }
            } else {
                field.push(c);
            }
        } else if c == '"' {
            quoted = true;
        } else if c == sep {
            row.push(std::mem::take(&mut field));
        } else if c == '\n' || c == '\r' {
            if c == '\r' && chars.peek() == Some(&'\n') {
                chars.next();
            }
            row.push(std::mem::take(&mut field));
            rows.push(std::mem::take(&mut row));
            any = false;
        } else {
            field.push(c);
        }
    }
    if any {
        row.push(field);
        rows.push(row);
    }
    rows
}

fn sep_arg(args: &[Value], i: usize) -> VmResult<char> {
    match opt(args, i) {
        Some(Value::Str(s)) if s.chars().count() == 1 => Ok(s.chars().next().unwrap_or(',')),
        Some(_) => Err(type_error("the separator is one character, such as \",\"")),
        None => Ok(','),
    }
}

/// `csv.parse(text, header = true, sep = ",")`: with a header, a list of
/// maps from column name to text; without, a list of lists.
pub fn csv_parse(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let text = text_arg(args, 0, "the CSV text")?;
    csv_value(
        text,
        opt(args, 1).is_none_or(Value::truthy),
        sep_arg(args, 2)?,
    )
}

fn csv_value(text: &str, header: bool, sep: char) -> VmResult<Value> {
    let rows = csv_rows(text, sep);
    if !header {
        return Ok(Value::list(
            rows.into_iter()
                .map(|r| Value::list(r.into_iter().map(Value::str).collect()))
                .collect(),
        ));
    }
    let mut rows = rows.into_iter();
    let Some(names) = rows.next() else {
        return Ok(Value::list(Vec::new()));
    };
    Ok(Value::list(
        rows.map(|r| {
            let mut map = Map::default();
            for (i, name) in names.iter().enumerate() {
                let key = Value::str(name.clone());
                if let Some(h) = Key::from_value(&key) {
                    map.insert(h, key, Value::str(r.get(i).cloned().unwrap_or_default()));
                }
            }
            Value::Map(Rc::new(RefCell::new(map)))
        })
        .collect(),
    ))
}

/// `csv.read(path, header = true, sep = ",")`
pub fn csv_read(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?;
    let text =
        std::fs::read_to_string(path).map_err(|e| io_error(&format!("cannot read {path}"), e))?;
    csv_value(
        &text,
        opt(args, 1).is_none_or(Value::truthy),
        sep_arg(args, 2)?,
    )
}

fn csv_field(text: &str, sep: char) -> String {
    if text.contains([sep, '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_string()
    }
}

/// `csv.format(rows, sep = ",")`: rows of maps (a header is written from
/// the first row's keys) or rows of lists, as CSV text.
pub fn csv_format(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let rows = ops::collect(&args[0])?;
    let sep = sep_arg(args, 1)?;
    let show = |v: &Value| csv_field(&ops::display(&ctx.program, v), sep);
    let mut out = String::new();
    let mut names: Option<Vec<Value>> = None;
    for row in &rows {
        let fields: Vec<String> = match row {
            Value::Map(m) => {
                let m = m.borrow();
                let names = names
                    .get_or_insert_with(|| m.entries().iter().map(|(k, _)| k.clone()).collect());
                if out.is_empty() {
                    let header: Vec<String> = names.iter().map(&show).collect();
                    out.push_str(&header.join(&sep.to_string()));
                    out.push('\n');
                }
                names
                    .iter()
                    .map(|k| {
                        Key::from_value(k)
                            .and_then(|h| m.get(&h).cloned())
                            .map(|v| show(&v))
                            .unwrap_or_default()
                    })
                    .collect()
            }
            other => ops::collect(other)?.iter().map(&show).collect(),
        };
        out.push_str(&fields.join(&sep.to_string()));
        out.push('\n');
    }
    Ok(Value::str(out))
}

pub fn csv_write(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let path = text_arg(args, 0, "the path")?.to_string();
    let rest: Vec<Value> = args[1..].to_vec();
    let text = csv_format(ctx, &rest)?;
    let text = ops::display(&ctx.program, &text);
    std::fs::write(&path, text).map_err(|e| io_error(&format!("cannot write {path}"), e))?;
    Ok(Value::None)
}

// ==== vyrn.http client =======================================================

/// `http.get(url, headers = {})` and `http.post(url, body, headers = {})`:
/// a response with `status`, `ok`, `text`, `headers` and, for JSON
/// replies, `json`. Maps and lists sent as a body go as JSON.
pub fn http_get(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    http_request(ctx, "GET", &args[0], None, opt(args, 1))
}

pub fn http_post(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    http_request(ctx, "POST", &args[0], opt(args, 1), opt(args, 2))
}

pub fn http_put(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    http_request(ctx, "PUT", &args[0], opt(args, 1), opt(args, 2))
}

pub fn http_delete(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    http_request(ctx, "DELETE", &args[0], None, opt(args, 1))
}

#[cfg(all(feature = "net", not(target_arch = "wasm32")))]
fn http_request(
    ctx: &mut Ctx,
    method: &str,
    url: &Value,
    body: Option<&Value>,
    headers: Option<&Value>,
) -> VmResult<Value> {
    let Value::Str(url) = url else {
        return Err(type_error("the URL must be a string"));
    };
    let mut header_list: Vec<(String, String)> = Vec::new();
    if let Some(Value::Map(h)) = headers {
        for (k, v) in h.borrow().entries() {
            header_list.push((ops::display(&ctx.program, k), ops::display(&ctx.program, v)));
        }
    }
    let (payload, content_type) = match body {
        None => (None, None),
        Some(Value::Str(s)) => (
            Some(s.as_bytes().to_vec()),
            Some("text/plain; charset=utf-8"),
        ),
        Some(v) => (
            Some(ops::to_json(&ctx.program, v).into_bytes()),
            Some("application/json"),
        ),
    };
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .http_status_as_error(false)
        .build()
        .into();
    let fail = |e: ureq::Error| err("HttpError", format!("{method} {url}: {e}"));
    let mut response = match payload {
        None => {
            let mut req = match method {
                "DELETE" => agent.delete(url.as_str()),
                _ => agent.get(url.as_str()),
            };
            for (k, v) in &header_list {
                req = req.header(k, v);
            }
            req.call().map_err(fail)?
        }
        Some(bytes) => {
            let mut req = match method {
                "PUT" => agent.put(url.as_str()),
                _ => agent.post(url.as_str()),
            };
            if let Some(ct) = content_type {
                if !header_list
                    .iter()
                    .any(|(k, _)| k.eq_ignore_ascii_case("content-type"))
                {
                    req = req.header("content-type", ct);
                }
            }
            for (k, v) in &header_list {
                req = req.header(k, v);
            }
            req.send(&bytes[..]).map_err(fail)?
        }
    };
    let status = response.status().as_u16();
    let mut headers_out = Vec::new();
    for (k, v) in response.headers() {
        headers_out.push((
            k.as_str().to_string(),
            Value::str(v.to_str().unwrap_or("").to_string()),
        ));
    }
    let text = response
        .body_mut()
        .read_to_string()
        .map_err(|e| err("HttpError", format!("{method} {url}: {e}")))?;
    let json = super::json::parse(&text)
        .ok()
        .map(|j| ops::from_json(&j))
        .unwrap_or(Value::None);
    let headers_value = map_value(
        headers_out
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect(),
    );
    Ok(map_value(vec![
        ("status", Value::Int(i64::from(status))),
        ("ok", Value::Bool((200..300).contains(&status))),
        ("text", Value::str(text)),
        ("json", json),
        ("headers", headers_value),
    ]))
}

#[cfg(not(all(feature = "net", not(target_arch = "wasm32"))))]
fn http_request(
    _: &mut Ctx,
    method: &str,
    _: &Value,
    _: Option<&Value>,
    _: Option<&Value>,
) -> VmResult<Value> {
    Err(err(
        "Unavailable",
        format!(
            "`http.{}` needs a build with networking (the `net` feature)",
            method.to_lowercase()
        ),
    ))
}

// ==== vyrn.task ==============================================================

/// What `task.select(...)` and `task.timeout(...)` return: awaiting it waits
/// for the first of several tasks or channels to be ready.
pub struct Race {
    pub items: Vec<Value>,
    /// For `task.timeout`: the position of the timer. Winning it fails with
    /// `Timeout`; any other winner gives just its value.
    pub timer: Option<usize>,
    pub seconds: f64,
}

impl Foreign for Race {
    fn type_name(&self) -> &'static str {
        "Race"
    }

    fn display(&self) -> String {
        if self.timer.is_some() {
            format!("<timeout after {}s>", self.seconds)
        } else {
            format!("<select of {}>", self.items.len())
        }
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `task.select(a, b, ...)`: `await` it to get `(index, value)` of the first
/// task or channel that is ready.
pub fn task_select(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let items = match args {
        [Value::List(items)] => items.borrow().clone(),
        _ => args.to_vec(),
    };
    if items.is_empty() {
        return Err(err("ValueError", "`task.select` needs at least one task"));
    }
    Ok(Value::Foreign(Rc::new(Box::new(Race {
        items,
        timer: None,
        seconds: 0.0,
    }))))
}

/// `task.timeout(seconds, work)`: `await` it for `work`'s value, or a
/// `Timeout` failure if `seconds` pass first.
pub fn task_timeout(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    let seconds = number(&args[0], "the number of seconds")?;
    let timer = super::builtins::sleep_task(ctx, seconds)?;
    Ok(Value::Foreign(Rc::new(Box::new(Race {
        items: vec![args[1].clone(), timer],
        timer: Some(1),
        seconds,
    }))))
}

/// `task.group()`: tasks started together. `g.spawn(work)` adds one,
/// `await g.wait()` waits for all and gives their results, `g.cancel()`
/// stops the unfinished ones. Used with `using`, the tasks never outlive
/// the block: any still running at its `end` are cancelled.
pub struct Group {
    tasks: RefCell<Vec<Value>>,
}

pub fn task_group(_: &mut Ctx, _: &[Value]) -> VmResult<Value> {
    Ok(Value::Foreign(Rc::new(Box::new(Group {
        tasks: RefCell::new(Vec::new()),
    }))))
}

impl Group {
    fn cancel_all(&self, ctx: &mut Ctx) {
        for t in self.tasks.borrow().iter() {
            if let Value::Task(h) = t {
                if !h.is_done() {
                    ctx.cancels.push(h.clone());
                }
            }
        }
    }
}

impl Foreign for Group {
    fn type_name(&self) -> &'static str {
        "TaskGroup"
    }

    fn display(&self) -> String {
        format!("<task group of {}>", self.tasks.borrow().len())
    }

    fn call_method(
        &self,
        ctx: &mut Ctx,
        name: &str,
        args: &[Value],
        _: &[(String, Value)],
    ) -> VmResult<Value> {
        match name {
            "spawn" | "add" => {
                let task = args.first().ok_or_else(|| {
                    err(
                        "CallError",
                        "`g.spawn` takes a task, as in `g.spawn(fetch(url))`",
                    )
                })?;
                if !matches!(task, Value::Task(_)) {
                    return Err(type_error(format!(
                        "`g.spawn` takes a task (the call of an `async` function), not {}",
                        task.type_name()
                    )));
                }
                self.tasks.borrow_mut().push(task.clone());
                Ok(task.clone())
            }
            "wait" => Ok(Value::list(self.tasks.borrow().clone())),
            "tasks" => Ok(Value::list(self.tasks.borrow().clone())),
            "cancel" | "close" => {
                self.cancel_all(ctx);
                Ok(Value::None)
            }
            _ => Err(err(
                "FieldError",
                format!("a task group has no method `{name}`"),
            )),
        }
    }

    fn len(&self) -> VmResult<usize> {
        Ok(self.tasks.borrow().len())
    }

    fn items(&self) -> VmResult<Vec<Value>> {
        Ok(self.tasks.borrow().clone())
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// `task.done(t)`: whether a task has finished.
pub fn task_done(_: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    Ok(Value::Bool(match &args[0] {
        Value::Task(h) => h.is_done(),
        _ => true,
    }))
}

/// `task.cancel(t)`: stops a task; awaiting it then fails with `Cancelled`.
pub fn task_cancel(ctx: &mut Ctx, args: &[Value]) -> VmResult<Value> {
    if let Value::Task(h) = &args[0] {
        if !h.is_done() {
            ctx.cancels.push(h.clone());
        }
    }
    Ok(Value::None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn calendar_round_trips() {
        for days in [-1000, 0, 1, 59, 365, 10_957, 20_768, 100_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
        assert_eq!(civil_from_days(0), (1970, 1, 1));
        assert_eq!(civil_from_days(20_768), (2026, 11, 11));
    }

    #[test]
    fn csv_handles_quotes() {
        let rows = csv_rows("a,b\n\"x, y\",\"say \"\"hi\"\"\"\n", ',');
        assert_eq!(rows, vec![vec!["a", "b"], vec!["x, y", "say \"hi\""]]);
    }

    #[test]
    fn digits_are_grouped() {
        assert_eq!(group_digits("1234567.891", ','), "1,234,567.891");
        assert_eq!(group_digits("123", ','), "123");
    }
}
