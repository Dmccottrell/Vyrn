//! The native tier: typed functions compiled to machine code.
//!
//! When a program is compiled, every top-level function whose parameters
//! are all `int`, `float` or `bool` is also translated to machine code
//! with Cranelift, if its body stays within what the native tier handles:
//! numbers and booleans, arithmetic, comparisons, `if`/`else if`/`else`,
//! `loop`, `each i in a until b`, `stop`, `skip`, `return`, calls to other
//! such functions (recursion included) and math builtins (`abs`, `sqrt`,
//! `min`, `max`, `vyrn.math`, ...). The interpreter then calls the machine
//! code whenever the arguments have the declared types, and runs the
//! bytecode otherwise, so a program means exactly the same either way.
//!
//! Semantics match the interpreter's: integer overflow, division by zero,
//! `//` and `%` rounding toward negative infinity, `/` giving a float, and
//! the limit on nested calls all behave alike, and failures come back as
//! the same errors. Anything outside the subset (strings, lists, `print`,
//! a variable that changes type, ...) leaves the function to the
//! interpreter. `VYRN_NATIVE=off` turns the tier off, and
//! `VYRN_NATIVE=verbose` says what was compiled and why the rest was not.

use std::collections::HashMap;
use std::rc::Rc;

use cranelift_codegen::ir::condcodes::{FloatCC, IntCC};
use cranelift_codegen::ir::{
    types, AbiParam, Block, FuncRef, InstBuilder, MemFlagsData, StackSlotData, StackSlotKind, Type,
    Value,
};
use cranelift_codegen::settings::{self, Configurable};
use cranelift_frontend::{FunctionBuilder, FunctionBuilderContext, Variable};
use cranelift_jit::{JITBuilder, JITModule};
use cranelift_module::{FuncId, Linkage, Module as _};

use crate::ast::{
    AssignOp, BinaryOp, BindTarget, Block as AstBlock, CompareOp, Condition, Expr, ExprKind,
    FnBody, FnDecl, ForMode, ItemKind, Module, PassMode, Stmt, StmtKind, TypeExpr, TypeExprKind,
    UnaryOp, UseKind,
};
use crate::vm::native_abi::{
    ErrState, NTy, NativeFn, ERR_DIV_ZERO, ERR_FLOAT_TO_INT, ERR_NO_RETURN, ERR_OVERFLOW,
    ERR_SHIFT, ERR_STACK, MAX_DEPTH,
};
use crate::vm::program::{GlobalInit, Program};

type Fail = String;
type Res<T> = Result<T, Fail>;

const OP_ADD: u32 = 0;
const OP_SUB: u32 = 1;
const OP_MUL: u32 = 2;
const OP_POW: u32 = 3;
const OP_FLOORDIV: u32 = 4;
const OP_REM: u32 = 5;
const OP_SHL: u32 = 6;
const OP_ABS: u32 = 7;
const OP_NEG: u32 = 8;

// ==== runtime helpers called from machine code ==============================

extern "C" fn rt_ipow(x: i64, y: i64, err: *mut ErrState) -> i64 {
    match u32::try_from(y).ok().and_then(|e| x.checked_pow(e)) {
        Some(v) => v,
        None => {
            // SAFETY: `err` is the state the VM passed to the entry point.
            unsafe {
                (*err).code = ERR_OVERFLOW;
                (*err).detail = OP_POW;
            }
            0
        }
    }
}

extern "C" fn rt_ishl(x: i64, y: i64, err: *mut ErrState) -> i64 {
    let shift = u32::try_from(y).ok().filter(|s| *s < 64);
    match shift {
        Some(s) => match x.checked_shl(s) {
            Some(v) => v,
            None => {
                // SAFETY: as above.
                unsafe {
                    (*err).code = ERR_OVERFLOW;
                    (*err).detail = OP_SHL;
                }
                0
            }
        },
        None => {
            // SAFETY: as above.
            unsafe {
                (*err).code = ERR_SHIFT;
                (*err).value = y;
            }
            0
        }
    }
}

extern "C" fn rt_powf(x: f64, y: f64) -> f64 {
    x.powf(y)
}
extern "C" fn rt_exp(x: f64) -> f64 {
    x.exp()
}
extern "C" fn rt_ln(x: f64) -> f64 {
    x.ln()
}
extern "C" fn rt_log2(x: f64) -> f64 {
    x.log2()
}
extern "C" fn rt_log10(x: f64) -> f64 {
    x.log10()
}
extern "C" fn rt_sin(x: f64) -> f64 {
    x.sin()
}
extern "C" fn rt_cos(x: f64) -> f64 {
    x.cos()
}
extern "C" fn rt_tan(x: f64) -> f64 {
    x.tan()
}
extern "C" fn rt_tanh(x: f64) -> f64 {
    x.tanh()
}
extern "C" fn rt_atan(x: f64) -> f64 {
    x.atan()
}
extern "C" fn rt_atan2(y: f64, x: f64) -> f64 {
    y.atan2(x)
}
extern "C" fn rt_hypot(x: f64, y: f64) -> f64 {
    x.hypot(y)
}
// Libcalls Cranelift may emit for rounding on CPUs without SSE4.1; given
// here so static (musl) builds, which cannot look them up, work too.
extern "C" fn rt_floor(x: f64) -> f64 {
    x.floor()
}
extern "C" fn rt_ceil(x: f64) -> f64 {
    x.ceil()
}
extern "C" fn rt_trunc(x: f64) -> f64 {
    x.trunc()
}
extern "C" fn rt_nearest(x: f64) -> f64 {
    x.round_ties_even()
}
extern "C" fn rt_floorf(x: f32) -> f32 {
    x.floor()
}
extern "C" fn rt_ceilf(x: f32) -> f32 {
    x.ceil()
}
extern "C" fn rt_truncf(x: f32) -> f32 {
    x.trunc()
}
extern "C" fn rt_nearestf(x: f32) -> f32 {
    x.round_ties_even()
}

// Lists of numbers. The VM passed a pointer to the list's contents (it
// keeps the list alive for the call) after checking every item's type, and
// native code keeps the types right, so the items read here are the
// declared type.

type ListPtr = *const std::cell::RefCell<Vec<crate::vm::value::Value>>;

fn list_index(list: &[crate::vm::value::Value], i: i64, err: *mut ErrState) -> Option<usize> {
    match crate::vm::tensor::normalize_index(i, list.len()) {
        Some(at) => Some(at),
        None => {
            // SAFETY: `err` is the state the VM passed to the entry point.
            unsafe {
                (*err).code = crate::vm::native_abi::ERR_INDEX;
                (*err).value = i;
                (*err).extra = list.len() as i64;
            }
            None
        }
    }
}

extern "C" fn rt_list_len(list: ListPtr) -> i64 {
    // SAFETY: a live list, see above.
    unsafe { (*list).borrow().len() as i64 }
}

extern "C" fn rt_list_get_i(list: ListPtr, i: i64, err: *mut ErrState) -> i64 {
    // SAFETY: as above.
    let items = unsafe { (*list).borrow() };
    match list_index(&items, i, err).map(|at| &items[at]) {
        Some(crate::vm::value::Value::Int(v)) => *v,
        _ => 0,
    }
}

extern "C" fn rt_list_get_f(list: ListPtr, i: i64, err: *mut ErrState) -> f64 {
    // SAFETY: as above.
    let items = unsafe { (*list).borrow() };
    match list_index(&items, i, err).map(|at| &items[at]) {
        Some(crate::vm::value::Value::Float(v)) => *v,
        _ => 0.0,
    }
}

extern "C" fn rt_list_set_i(list: ListPtr, i: i64, v: i64, err: *mut ErrState) -> i64 {
    // SAFETY: as above.
    let mut items = unsafe { (*list).borrow_mut() };
    if let Some(at) = list_index(&items, i, err) {
        items[at] = crate::vm::value::Value::Int(v);
    }
    0
}

extern "C" fn rt_list_set_f(list: ListPtr, i: i64, v: f64, err: *mut ErrState) -> i64 {
    // SAFETY: as above.
    let mut items = unsafe { (*list).borrow_mut() };
    if let Some(at) = list_index(&items, i, err) {
        items[at] = crate::vm::value::Value::Float(v);
    }
    0
}

extern "C" fn rt_list_push_i(list: ListPtr, v: i64) -> i64 {
    // SAFETY: as above.
    unsafe { (*list).borrow_mut().push(crate::vm::value::Value::Int(v)) };
    0
}

extern "C" fn rt_list_push_f(list: ListPtr, v: f64) -> i64 {
    // SAFETY: as above.
    unsafe { (*list).borrow_mut().push(crate::vm::value::Value::Float(v)) };
    0
}

fn arena<'e>(err: *mut ErrState) -> &'e mut crate::vm::native_abi::Arena {
    // SAFETY: the VM sets `arena` to an arena that outlives the call.
    unsafe { &mut *(*err).arena }
}

fn new_list(items: Vec<crate::vm::value::Value>, err: *mut ErrState) -> ListPtr {
    let list = Rc::new(std::cell::RefCell::new(items));
    let ptr = Rc::as_ptr(&list);
    arena(err).lists.push(list);
    ptr
}

extern "C" fn rt_list_new(cap: i64, err: *mut ErrState) -> ListPtr {
    new_list(Vec::with_capacity(cap.clamp(0, 1 << 20) as usize), err)
}

extern "C" fn rt_list_repeat(list: ListPtr, n: i64, err: *mut ErrState) -> ListPtr {
    // SAFETY: a live list.
    let items = unsafe { (*list).borrow() };
    let times = n.max(0) as usize;
    if items.len().saturating_mul(times) > (1 << 32) {
        // SAFETY: the VM's state.
        unsafe { (*err).code = crate::vm::native_abi::ERR_MEMORY };
        return list;
    }
    let mut out = Vec::with_capacity(items.len() * times);
    for _ in 0..times {
        out.extend(items.iter().cloned());
    }
    drop(items);
    new_list(out, err)
}

extern "C" fn rt_list_concat(a: ListPtr, b: ListPtr, err: *mut ErrState) -> ListPtr {
    // SAFETY: live lists.
    let mut out = unsafe { (*a).borrow().clone() };
    // SAFETY: as above.
    out.extend(unsafe { (*b).borrow().iter().cloned() });
    new_list(out, err)
}

extern "C" fn rt_list_copy(list: ListPtr, err: *mut ErrState) -> ListPtr {
    // SAFETY: a live list.
    let items = unsafe { (*list).borrow().clone() };
    new_list(items, err)
}

extern "C" fn rt_list_sum_i(list: ListPtr, err: *mut ErrState) -> i64 {
    // SAFETY: a live list of ints.
    let items = unsafe { (*list).borrow() };
    let mut total: i64 = 0;
    for v in items.iter() {
        if let crate::vm::value::Value::Int(x) = v {
            match total.checked_add(*x) {
                Some(t) => total = t,
                None => {
                    // SAFETY: the VM's state.
                    unsafe {
                        (*err).code = ERR_OVERFLOW;
                        (*err).detail = OP_ADD;
                    }
                    return 0;
                }
            }
        }
    }
    total
}

// ---- strings ----
//
// A string in native code is a pointer to the `String` inside one of the
// VM's `Rc<String>`s: an argument, a literal the machine code keeps, or one
// made during the call and kept in the arena. Operations whose exact
// behaviour the VM defines (methods, conversions, indexing, `print`) are
// done by the VM's own code, through its context, so they cannot differ.

use crate::vm::value::Value as VmValue;

type StrPtr = *const String;

/// Kinds of the words passed to the generic helpers.
const KIND_INT: i64 = 0;
const KIND_FLOAT: i64 = 1;
const KIND_BOOL: i64 = 2;
const KIND_STR: i64 = 3;

fn ctx_of<'c>(err: *mut ErrState) -> &'c mut crate::vm::ops::Ctx {
    // SAFETY: the VM sets `ctx` to its context for the call, and does not
    // touch it until the call returns.
    unsafe { &mut *((*err).ctx as *mut crate::vm::ops::Ctx) }
}

fn str_value(p: StrPtr) -> VmValue {
    // SAFETY: `p` points into a live `Rc<String>`; this takes a reference.
    unsafe {
        Rc::increment_strong_count(p);
        VmValue::Str(Rc::from_raw(p))
    }
}

fn keep_rc(err: *mut ErrState, text: Rc<String>) -> StrPtr {
    let p = Rc::as_ptr(&text);
    arena(err).strings.push(text);
    p
}

fn keep_str(err: *mut ErrState, text: String) -> StrPtr {
    keep_rc(err, Rc::new(text))
}

fn throw(err: *mut ErrState, e: VmValue) {
    // SAFETY: the VM's state.
    unsafe {
        (*err).code = crate::vm::native_abi::ERR_THROWN;
        (*err).thrown = Some(e);
    }
}

fn word_to_value(kind: i64, w: u64) -> VmValue {
    match kind {
        KIND_INT => VmValue::Int(w as i64),
        KIND_FLOAT => VmValue::Float(f64::from_bits(w)),
        KIND_BOOL => VmValue::Bool(w & 0xff != 0),
        _ => str_value(w as StrPtr),
    }
}

/// The word for a result of the expected kind; a result of another kind
/// (which the tables below rule out) fails rather than lie.
fn value_to_word(err: *mut ErrState, want: i64, v: VmValue) -> u64 {
    match (want, v) {
        (KIND_INT, VmValue::Int(x)) => x as u64,
        (KIND_FLOAT, VmValue::Float(x)) => x.to_bits(),
        (KIND_BOOL, VmValue::Bool(x)) => u64::from(x),
        (KIND_STR, VmValue::Str(t)) => keep_rc(err, t) as u64,
        (_, other) => {
            throw(
                err,
                crate::vm::ops::err(
                    "InternalError",
                    format!(
                        "native code expected another type than {}",
                        other.type_name()
                    ),
                ),
            );
            0
        }
    }
}

extern "C" fn rt_sb_new(err: *mut ErrState) -> *mut String {
    let mut b = Box::new(String::new());
    let p: *mut String = &mut *b;
    arena(err).builders.push(b);
    p
}

extern "C" fn rt_sb_str(sb: *mut String, s: StrPtr) -> i64 {
    // SAFETY: a builder from `rt_sb_new` and a live string.
    unsafe { (*sb).push_str(&*s) };
    0
}

extern "C" fn rt_sb_int(sb: *mut String, v: i64) -> i64 {
    // SAFETY: a builder from `rt_sb_new`.
    unsafe { (*sb).push_str(&v.to_string()) };
    0
}

extern "C" fn rt_sb_float(sb: *mut String, v: f64) -> i64 {
    // SAFETY: as above.
    unsafe { (*sb).push_str(&crate::vm::ops::fmt_float(v)) };
    0
}

extern "C" fn rt_sb_bool(sb: *mut String, v: i8) -> i64 {
    // SAFETY: as above.
    unsafe { (*sb).push_str(if v != 0 { "true" } else { "false" }) };
    0
}

extern "C" fn rt_sb_finish(sb: *mut String, err: *mut ErrState) -> StrPtr {
    // SAFETY: as above.
    let text = unsafe { std::mem::take(&mut *sb) };
    // Builders finish in reverse order of starting; drop this one.
    let builders = &mut arena(err).builders;
    if builders.last_mut().is_some_and(|b| std::ptr::eq(&**b, sb)) {
        builders.pop();
    }
    keep_str(err, text)
}

/// How many strings and lists the arena holds, as a mark to collect back
/// to.
extern "C" fn rt_arena_mark(err: *mut ErrState) -> i64 {
    let a = arena(err);
    (a.strings.len() as i64) | ((a.lists.len() as i64) << 32)
}

/// At the end of a loop's round: of the strings and lists made since
/// `mark`, keeps the ones `live` (the variables still in scope) point at,
/// and frees the rest, as the interpreter would have.
extern "C" fn rt_arena_collect(err: *mut ErrState, mark: i64, live: *const u64, n: i64) -> i64 {
    let a = arena(err);
    let (ms, ml) = ((mark & 0xffff_ffff) as usize, (mark >> 32) as usize);
    if a.strings.len() <= ms && a.lists.len() <= ml {
        return 0;
    }
    // SAFETY: `n` words written by the caller.
    let live = unsafe { std::slice::from_raw_parts(live, n as usize) };
    if a.strings.len() > ms {
        let tail = a.strings.split_off(ms);
        a.strings.extend(
            tail.into_iter()
                .filter(|t| live.contains(&(Rc::as_ptr(t) as u64))),
        );
    }
    if a.lists.len() > ml {
        let tail = a.lists.split_off(ml);
        a.lists.extend(
            tail.into_iter()
                .filter(|l| live.contains(&(Rc::as_ptr(l) as u64))),
        );
    }
    0
}

extern "C" fn rt_print(err: *mut ErrState, s: StrPtr) -> i64 {
    use std::io::Write as _;
    let ctx = ctx_of(err);
    // SAFETY: a live string.
    let _ = writeln!(ctx.out, "{}", unsafe { &*s });
    0
}

extern "C" fn rt_str_len(s: StrPtr) -> i64 {
    // SAFETY: a live string.
    unsafe { (*s).chars().count() as i64 }
}

extern "C" fn rt_str_cmp(a: StrPtr, b: StrPtr) -> i64 {
    // SAFETY: live strings.
    match unsafe { (*a).cmp(&*b) } {
        std::cmp::Ordering::Less => -1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => 1,
    }
}

extern "C" fn rt_str_contains(hay: StrPtr, needle: StrPtr) -> i8 {
    // SAFETY: live strings.
    i8::from(unsafe { (*hay).contains((*needle).as_str()) })
}

extern "C" fn rt_str_concat(a: StrPtr, b: StrPtr, err: *mut ErrState) -> StrPtr {
    // SAFETY: live strings.
    let text = unsafe { format!("{}{}", *a, *b) };
    keep_str(err, text)
}

extern "C" fn rt_str_repeat(a: StrPtr, n: i64, err: *mut ErrState) -> StrPtr {
    // SAFETY: a live string.
    let text = unsafe { (*a).repeat(n.max(0) as usize) };
    keep_str(err, text)
}

/// `s[i]`, by the VM's own indexing.
extern "C" fn rt_str_index(err: *mut ErrState, s: StrPtr, i: i64) -> StrPtr {
    let ctx = ctx_of(err);
    match crate::vm::ops::get_index(ctx, &str_value(s), &VmValue::Int(i)) {
        Ok(VmValue::Str(t)) => keep_rc(err, t),
        Ok(other) => value_to_word(err, KIND_STR, other) as StrPtr,
        Err(e) => {
            throw(err, e);
            s
        }
    }
}

/// A builtin (`str`, `int`, `float`, `ord`, `format`, ...) on up to three
/// words, by the VM's own code.
#[allow(clippy::too_many_arguments)]
extern "C" fn rt_builtin(
    err: *mut ErrState,
    name: StrPtr,
    n: i64,
    kinds: i64,
    a0: u64,
    a1: u64,
    a2: u64,
    want: i64,
) -> u64 {
    let words = [a0, a1, a2];
    let args: Vec<VmValue> = (0..n as usize)
        .map(|i| word_to_value((kinds >> (4 * i)) & 0xf, words[i]))
        .collect();
    // SAFETY: a literal kept with the code.
    let name = unsafe { (*name).as_str() };
    let Some(id) = crate::vm::builtins::lookup(name) else {
        throw(
            err,
            crate::vm::ops::err("InternalError", format!("no builtin `{name}`")),
        );
        return 0;
    };
    match crate::vm::builtins::call(id, ctx_of(err), &args) {
        Ok(v) => value_to_word(err, want, v),
        Err(e) => {
            throw(err, e);
            0
        }
    }
}

/// A method of a string, by the VM's own code.
#[allow(clippy::too_many_arguments)]
extern "C" fn rt_str_method(
    err: *mut ErrState,
    name: StrPtr,
    recv: StrPtr,
    n: i64,
    kinds: i64,
    a0: u64,
    a1: u64,
    want: i64,
) -> u64 {
    let words = [a0, a1];
    let mut args = vec![str_value(recv)];
    args.extend((0..n as usize).map(|i| word_to_value((kinds >> (4 * i)) & 0xf, words[i])));
    // SAFETY: a literal kept with the code.
    let name = unsafe { (*name).as_str() };
    match crate::vm::builtins::call_method(ctx_of(err), name, &args) {
        Ok(v) => value_to_word(err, want, v),
        Err(e) => {
            throw(err, e);
            0
        }
    }
}

/// String methods the native tier calls, and the type each gives.
fn str_method_result(name: &str) -> Option<NTy> {
    Some(match name {
        "upper" | "lower" | "title" | "capitalize" | "swapcase" | "strip" | "lstrip" | "rstrip"
        | "replace" | "removeprefix" | "removesuffix" | "zfill" | "center" | "ljust" | "rjust"
        | "repeat" | "format" => NTy::Str,
        "startswith" | "endswith" | "contains" | "isdigit" | "isnumeric" | "isdecimal"
        | "isalpha" | "isalnum" | "isspace" | "isupper" | "islower" | "is_empty" => NTy::Bool,
        "find" | "rfind" | "index" | "count" => NTy::Int,
        _ => return None,
    })
}

/// Builtins the native tier calls through the VM, and the type each gives
/// for these argument types.
fn builtin_result(name: &str, args: &[NTy]) -> Option<NTy> {
    use NTy::*;
    Some(match (name, args) {
        ("str", [Int | Float | Bool | Str]) => Str,
        ("int", [Str]) => Int,
        ("float", [Str]) => Float,
        ("ord", [Str]) => Int,
        ("chr" | "hex" | "oct" | "bin", [Int]) => Str,
        ("repr", [Str]) => Str,
        ("format" | "__format", [Int | Float | Bool | Str, Str]) => Str,
        ("format", [Int | Float | Bool | Str]) => Str,
        _ => return None,
    })
}

/// Where a list's item pointer and length are inside the `RefCell<Vec<Value>>`
/// the VM passes, and where an `int`'s or `float`'s payload is inside a
/// `Value`. Rust does not promise these layouts, so they are measured once
/// and checked; if anything is ambiguous, list items go through the
/// runtime calls above instead.
#[derive(Clone, Copy, Debug)]
struct ListLayout {
    data: i32,
    len: i32,
    int_payload: i32,
    float_payload: i32,
}

fn list_layout() -> Option<ListLayout> {
    use crate::vm::value::Value as V;
    use std::cell::RefCell;
    static LAYOUT: std::sync::OnceLock<Option<ListLayout>> = std::sync::OnceLock::new();
    *LAYOUT.get_or_init(|| {
        if std::mem::size_of::<V>() != 16
            || std::mem::size_of::<usize>() != 8
            || std::mem::size_of::<Vec<V>>() != 24
        {
            return None;
        }
        // Where the `Vec` sits in the `RefCell`, from the cell's own
        // pointer to it; then which of the `Vec`'s three words (all of
        // them initialized) hold the item pointer and the length.
        let find = |len: usize, cap: usize| -> Option<(i32, i32)> {
            let mut v: Vec<V> = Vec::with_capacity(cap);
            for _ in 0..len {
                v.push(V::Int(0));
            }
            let data = v.as_ptr() as u64;
            let cell = RefCell::new(v);
            let base = &cell as *const RefCell<Vec<V>> as usize;
            let vec_at = cell.as_ptr() as usize - base;
            // SAFETY: a `Vec` is three initialized words.
            let words: [u64; 3] = unsafe { std::ptr::read(cell.as_ptr() as *const [u64; 3]) };
            let one = |x: u64| {
                let hits: Vec<usize> = (0..3).filter(|i| words[*i] == x).collect();
                (hits.len() == 1).then(|| (vec_at + hits[0] * 8) as i32)
            };
            Some((one(data)?, one(len as u64)?))
        };
        let (data, len) = find(3, 7)?;
        if find(5, 11)? != (data, len) || find(0, 4)?.1 != len {
            return None;
        }
        // The payloads, from references into the values themselves.
        let int_value = V::Int(0x0A0B_0C0D_0E0F_1011);
        let int_payload = match &int_value {
            V::Int(x) => x as *const i64 as usize - &int_value as *const V as usize,
            _ => return None,
        };
        let float_value = V::Float(-2.75);
        let float_payload = match &float_value {
            V::Float(x) => x as *const f64 as usize - &float_value as *const V as usize,
            _ => return None,
        };
        if int_payload % 8 != 0 || float_payload % 8 != 0 || int_payload > 8 || float_payload > 8 {
            return None;
        }
        Some(ListLayout {
            data,
            len,
            int_payload: int_payload as i32,
            float_payload: float_payload as i32,
        })
    })
}

/// Whether list items are read and written directly (the layout check
/// passed), rather than through runtime calls.
pub fn direct_list_access() -> bool {
    list_layout().is_some()
}

/// Float functions of one argument, by their Vyrn name.
const FLOAT1: &[(&str, &str)] = &[
    ("exp", "vyrn_exp"),
    ("log", "vyrn_ln"),
    ("log2", "vyrn_log2"),
    ("log10", "vyrn_log10"),
    ("sin", "vyrn_sin"),
    ("cos", "vyrn_cos"),
    ("tan", "vyrn_tan"),
    ("tanh", "vyrn_tanh"),
    ("atan", "vyrn_atan"),
];

fn helper_symbols() -> Vec<(&'static str, *const u8)> {
    vec![
        ("vyrn_ipow", rt_ipow as *const u8),
        ("vyrn_sb_new", rt_sb_new as *const u8),
        ("vyrn_arena_mark", rt_arena_mark as *const u8),
        ("vyrn_arena_collect", rt_arena_collect as *const u8),
        ("vyrn_sb_str", rt_sb_str as *const u8),
        ("vyrn_sb_int", rt_sb_int as *const u8),
        ("vyrn_sb_float", rt_sb_float as *const u8),
        ("vyrn_sb_bool", rt_sb_bool as *const u8),
        ("vyrn_sb_finish", rt_sb_finish as *const u8),
        ("vyrn_print", rt_print as *const u8),
        ("vyrn_str_len", rt_str_len as *const u8),
        ("vyrn_str_cmp", rt_str_cmp as *const u8),
        ("vyrn_str_contains", rt_str_contains as *const u8),
        ("vyrn_str_concat", rt_str_concat as *const u8),
        ("vyrn_str_repeat", rt_str_repeat as *const u8),
        ("vyrn_str_index", rt_str_index as *const u8),
        ("vyrn_builtin", rt_builtin as *const u8),
        ("vyrn_str_method", rt_str_method as *const u8),
        ("vyrn_list_len", rt_list_len as *const u8),
        ("vyrn_list_new", rt_list_new as *const u8),
        ("vyrn_list_repeat", rt_list_repeat as *const u8),
        ("vyrn_list_concat", rt_list_concat as *const u8),
        ("vyrn_list_copy", rt_list_copy as *const u8),
        ("vyrn_list_sum_i", rt_list_sum_i as *const u8),
        ("vyrn_list_get_i", rt_list_get_i as *const u8),
        ("vyrn_list_get_f", rt_list_get_f as *const u8),
        ("vyrn_list_set_i", rt_list_set_i as *const u8),
        ("vyrn_list_set_f", rt_list_set_f as *const u8),
        ("vyrn_list_push_i", rt_list_push_i as *const u8),
        ("vyrn_list_push_f", rt_list_push_f as *const u8),
        ("vyrn_ishl", rt_ishl as *const u8),
        ("vyrn_powf", rt_powf as *const u8),
        ("vyrn_exp", rt_exp as *const u8),
        ("vyrn_ln", rt_ln as *const u8),
        ("vyrn_log2", rt_log2 as *const u8),
        ("vyrn_log10", rt_log10 as *const u8),
        ("vyrn_sin", rt_sin as *const u8),
        ("vyrn_cos", rt_cos as *const u8),
        ("vyrn_tan", rt_tan as *const u8),
        ("vyrn_tanh", rt_tanh as *const u8),
        ("vyrn_atan", rt_atan as *const u8),
        ("vyrn_atan2", rt_atan2 as *const u8),
        ("vyrn_hypot", rt_hypot as *const u8),
        ("floor", rt_floor as *const u8),
        ("ceil", rt_ceil as *const u8),
        ("trunc", rt_trunc as *const u8),
        ("nearbyint", rt_nearest as *const u8),
        ("floorf", rt_floorf as *const u8),
        ("ceilf", rt_ceilf as *const u8),
        ("truncf", rt_truncf as *const u8),
        ("nearbyintf", rt_nearestf as *const u8),
    ]
}

// ==== choosing functions ====================================================

fn scalar_type(t: &TypeExpr) -> Option<NTy> {
    let TypeExprKind::Named { path, args } = &t.kind else {
        return None;
    };
    if !args.is_empty() || path.segments.len() != 1 {
        return None;
    }
    match path.segments[0].name.as_str() {
        "int" | "i64" => Some(NTy::Int),
        "float" | "f64" => Some(NTy::Float),
        "bool" => Some(NTy::Bool),
        _ => None,
    }
}

/// The structs the native tier handles: no generics, and every field an
/// `int`, `float` or `bool`.
#[derive(Default)]
struct Structs {
    by_name: HashMap<String, u16>,
    fields: HashMap<u16, (String, Vec<(String, NTy)>)>,
}

impl Structs {
    fn of(module: &Module, program: &Program) -> Structs {
        let mut out = Structs::default();
        for item in &module.items {
            let ItemKind::Aggregate(a) = &item.kind else {
                continue;
            };
            if !matches!(a.kind, crate::ast::AggregateKind::Struct)
                || !a.generics.is_empty()
                || a.base.is_some()
                || a.fields.is_empty()
                || a.fields.len() > 16
            {
                continue;
            }
            let fields: Option<Vec<(String, NTy)>> = a
                .fields
                .iter()
                .map(|f| {
                    let t = f.declared_ty.as_ref().and_then(scalar_type)?;
                    Some((f.name.name.clone(), t))
                })
                .collect();
            let Some(fields) = fields else { continue };
            let Some(id) = program.types.iter().position(|t| {
                matches!(t.kind, crate::vm::program::TypeKind::Struct)
                    && program.symbols.name(t.name) == a.name.name
            }) else {
                continue;
            };
            // The VM's field order must be the declared one.
            let vm_fields: Vec<&str> = program.types[id]
                .fields
                .iter()
                .map(|f| program.symbols.name(*f))
                .collect();
            if vm_fields != fields.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>() {
                continue;
            }
            out.by_name.insert(a.name.name.clone(), id as u16);
            out.fields.insert(id as u16, (a.name.name.clone(), fields));
        }
        out
    }

    fn field_types(&self) -> HashMap<u16, Vec<NTy>> {
        self.fields
            .iter()
            .map(|(id, (_, f))| (*id, f.iter().map(|(_, t)| *t).collect()))
            .collect()
    }

    fn count(&self, id: u16) -> usize {
        self.fields.get(&id).map_or(0, |(_, f)| f.len())
    }

    fn field(&self, id: u16, name: &str) -> Option<(usize, NTy)> {
        let (_, fields) = self.fields.get(&id)?;
        fields
            .iter()
            .position(|(n, _)| n == name)
            .map(|i| (i, fields[i].1))
    }

    fn name(&self, id: u16) -> &str {
        self.fields.get(&id).map_or("?", |(n, _)| n.as_str())
    }
}

/// A parameter's type: a scalar, a list of `int` or `float`, or a struct
/// the tier handles.
fn param_type(t: &TypeExpr, structs: &Structs) -> Option<NTy> {
    if let TypeExprKind::Named { path, args } = &t.kind {
        if args.is_empty() && path.segments.len() == 1 {
            if let Some(id) = structs.by_name.get(&path.segments[0].name) {
                return Some(NTy::Struct(*id));
            }
            if path.segments[0].name == "str" {
                return Some(NTy::Str);
            }
        }
    }
    if let TypeExprKind::List(inner) = &t.kind {
        return match scalar_type(inner)? {
            NTy::Int => Some(NTy::ListInt),
            NTy::Float => Some(NTy::ListFloat),
            _ => None,
        };
    }
    scalar_type(t)
}

/// A candidate's signature, or why it cannot be one. For a method,
/// `self_ty` is its struct.
fn signature(f: &FnDecl, structs: &Structs, self_ty: Option<u16>) -> Res<(Vec<NTy>, NTy)> {
    let m = &f.modifiers;
    if m.is_async || m.is_kernel || m.export.is_some() || !f.generics.is_empty() {
        return Err("async, kernel, exported and generic functions stay interpreted".into());
    }
    if f.result.fails.is_some() {
        return Err("functions that declare `fails` stay interpreted".into());
    }
    let mut params = Vec::new();
    for (i, p) in f.params.iter().enumerate() {
        if let (0, Some(id), "self") = (i, self_ty, p.name.name.as_str()) {
            // `mut self` changes the caller's struct, which a copy cannot.
            if p.pass != PassMode::Borrow {
                return Err("`mut self` methods stay interpreted".into());
            }
            params.push(NTy::Struct(id));
            continue;
        }
        if p.default.is_some() || p.pass != PassMode::Borrow {
            return Err(format!(
                "parameter `{}` has a default or a pass mode",
                p.name.name
            ));
        }
        let ty = p
            .declared_ty
            .as_ref()
            .and_then(|t| param_type(t, structs))
            .ok_or_else(|| {
                format!(
                    "parameter `{}` is not declared int, float, bool, [int] or [float]",
                    p.name.name
                )
            })?;
        params.push(ty);
    }
    let ret = match &f.result.returns {
        None => NTy::Unit,
        Some(t) => param_type(t, structs)
            .ok_or("the result is not int, float, bool, [int], [float] or a struct of those")?,
    };
    Ok((params, ret))
}

struct Candidate<'a> {
    decl: &'a FnDecl,
    /// The name calls use: `name`, or `Type.method` for a method.
    key: String,
    proto: usize,
    params: Vec<NTy>,
    ret: NTy,
}

/// Compiles what it can of `module` and attaches the machine code to the
/// program's functions.
pub fn attach(module: &Module, program: &Program) {
    let mode = std::env::var("VYRN_NATIVE").unwrap_or_default();
    if mode == "off" || mode == "0" {
        return;
    }
    let verbose = mode == "verbose";
    let report = |name: &str, what: &str| {
        if verbose {
            eprintln!("native: {name}: {what}");
        }
    };

    // `use vyrn.math` (or `as m`): the local name of the math module.
    let mut math_names: Vec<String> = Vec::new();
    let mut all_names: Vec<String> = Vec::new();
    let mut candidates: Vec<Candidate> = Vec::new();
    let structs = Structs::of(module, program);
    for item in &module.items {
        match &item.kind {
            ItemKind::Use(u) => {
                let segs: Vec<&str> = u.path.segments.iter().map(|s| s.name.as_str()).collect();
                if segs == ["vyrn", "math"] {
                    if let UseKind::Module { alias } = &u.kind {
                        math_names.push(
                            alias
                                .as_ref()
                                .map_or("math".to_string(), |a| a.name.clone()),
                        );
                    }
                }
            }
            ItemKind::Fn(f) => {
                all_names.push(f.name.name.clone());
                if crate::prelude::is_prelude_offset(f.name.span.start.offset) {
                    continue;
                }
                if !item.decorators.is_empty() || !matches!(f.body, FnBody::Block(_)) {
                    continue;
                }
                let Some(GlobalInit::Function(proto)) = program
                    .global(&f.name.name)
                    .map(|g| &program.globals[g].init)
                else {
                    continue;
                };
                match signature(f, &structs, None) {
                    Ok((params, ret)) => candidates.push(Candidate {
                        decl: f,
                        key: f.name.name.clone(),
                        proto: *proto,
                        params,
                        ret,
                    }),
                    Err(why) => report(&f.name.name, &format!("interpreted: {why}")),
                }
            }
            ItemKind::Aggregate(a) => {
                all_names.push(a.name.name.clone());
                let Some(&id) = structs.by_name.get(&a.name.name) else {
                    continue;
                };
                for m in &a.methods {
                    let ItemKind::Fn(f) = &m.kind else { continue };
                    if !m.decorators.is_empty() || !matches!(f.body, FnBody::Block(_)) {
                        continue;
                    }
                    let Some(proto) = program
                        .symbols
                        .get(&f.name.name)
                        .and_then(|sym| program.types[id as usize].methods.get(&sym))
                    else {
                        continue;
                    };
                    let key = format!("{}.{}", a.name.name, f.name.name);
                    match signature(f, &structs, Some(id)) {
                        Ok((params, ret)) if params.first() == Some(&NTy::Struct(id)) => candidates
                            .push(Candidate {
                                decl: f,
                                key,
                                proto: *proto,
                                params,
                                ret,
                            }),
                        Ok(_) => report(&key, "interpreted: a method without `self`"),
                        Err(why) => report(&key, &format!("interpreted: {why}")),
                    }
                }
            }
            ItemKind::Enum(e) => all_names.push(e.name.name.clone()),
            ItemKind::Const(c) => all_names.push(c.name.name.clone()),
            _ => {}
        }
    }
    if candidates.is_empty() {
        return;
    }
    let struct_types = Rc::new(structs.field_types());
    let env = Env {
        math: math_names,
        globals: all_names,
        structs,
    };

    // Drop candidates that cannot be lowered, until the rest only call
    // each other.
    loop {
        let sigs: HashMap<String, (Vec<NTy>, NTy)> = candidates
            .iter()
            .map(|c| (c.key.clone(), (c.params.clone(), c.ret)))
            .collect();
        let mut jit = match Jit::new() {
            Ok(j) => j,
            Err(e) => {
                report("native tier", &format!("unavailable: {e}"));
                return;
            }
        };
        let ids = match jit.declare_all(&sigs) {
            Ok(ids) => ids,
            Err(e) => {
                report("native tier", &e);
                return;
            }
        };
        let mut failed = None;
        for (i, c) in candidates.iter().enumerate() {
            if let Err(why) = jit.define(c, &sigs, &ids, &env) {
                failed = Some((i, why));
                break;
            }
        }
        if let Some((i, why)) = failed {
            report(&candidates[i].key, &format!("interpreted: {why}"));
            candidates.remove(i);
            if candidates.is_empty() {
                return;
            }
            continue;
        }
        // Everything left compiles: add the entry points and finish.
        let mut entries = Vec::new();
        for c in &candidates {
            match jit.define_entry(&c.key, &c.params, c.ret, ids[&c.key], &env.structs) {
                Ok(id) => entries.push(id),
                Err(e) => {
                    report(&c.key, &e);
                    return;
                }
            }
        }
        if let Err(e) = jit.module.finalize_definitions() {
            report("native tier", &format!("could not finish: {e}"));
            return;
        }
        let pointers: Vec<*const u8> = entries
            .iter()
            .map(|id| jit.module.get_finalized_function(*id))
            .collect();
        let keep: Rc<dyn std::any::Any> = Rc::new(KeepAlive(Some(jit.module), jit.literals));
        for (c, ptr) in candidates.iter().zip(pointers) {
            // SAFETY: the entry was built with exactly the `Entry` signature.
            let entry: crate::vm::native_abi::Entry = unsafe { std::mem::transmute(ptr) };
            let native = NativeFn {
                name: c.key.clone(),
                entry,
                params: c.params.clone(),
                ret: c.ret,
                structs: struct_types.clone(),
                keep: keep.clone(),
            };
            let _ = program.protos[c.proto].native.set(Rc::new(native));
            report(&c.key, "compiled to machine code");
        }
        return;
    }
}

/// Owns the machine code. It is never freed while a program can call it;
/// programs are dropped at exit, or when the REPL compiles the next entry.
#[allow(dead_code)] // held only so the code is not freed
struct KeepAlive(Option<JITModule>, Vec<Rc<String>>);

/// What names mean to the lowering, besides locals and the candidates.
struct Env {
    /// Local names of `vyrn.math`.
    math: Vec<String>,
    /// Every top-level function, type and constant: a call to one that is
    /// not a candidate cannot be compiled.
    globals: Vec<String>,
    structs: Structs,
}

// ==== the code generator ====================================================

struct Jit {
    module: JITModule,
    ctx: cranelift_codegen::Context,
    fctx: FunctionBuilderContext,
    helpers: HashMap<&'static str, FuncId>,
    ptr: Type,
    /// String literals the machine code points at.
    literals: Vec<Rc<String>>,
}

fn cl_type(t: NTy) -> Type {
    match t {
        NTy::Int | NTy::Unit | NTy::ListInt | NTy::ListFloat | NTy::Struct(_) | NTy::Str => {
            types::I64
        }
        NTy::Float => types::F64,
        NTy::Bool => types::I8,
    }
}

impl Jit {
    fn new() -> Res<Jit> {
        let mut flags = settings::builder();
        flags
            .set("use_colocated_libcalls", "false")
            .map_err(|e| e.to_string())?;
        flags.set("is_pic", "false").map_err(|e| e.to_string())?;
        flags.set("opt_level", "speed").map_err(|e| e.to_string())?;
        let isa = cranelift_native::builder()
            .map_err(|e| e.to_string())?
            .finish(settings::Flags::new(flags))
            .map_err(|e| e.to_string())?;
        let mut builder = JITBuilder::with_isa(isa, cranelift_module::default_libcall_names());
        for (name, ptr) in helper_symbols() {
            builder.symbol(name, ptr);
        }
        let mut module = JITModule::new(builder);
        let ptr = module.target_config().pointer_type();
        let mut helpers = HashMap::new();
        let mut declare = |name: &'static str, params: &[Type], ret: Type| -> Res<()> {
            let mut sig = module.make_signature();
            for p in params {
                sig.params.push(AbiParam::new(*p));
            }
            sig.returns.push(AbiParam::new(ret));
            let id = module
                .declare_function(name, Linkage::Import, &sig)
                .map_err(|e| e.to_string())?;
            helpers.insert(name, id);
            Ok(())
        };
        declare("vyrn_ipow", &[types::I64, types::I64, ptr], types::I64)?;
        declare("vyrn_sb_new", &[ptr], ptr)?;
        declare("vyrn_arena_mark", &[ptr], types::I64)?;
        declare(
            "vyrn_arena_collect",
            &[ptr, types::I64, ptr, types::I64],
            types::I64,
        )?;
        declare("vyrn_sb_str", &[ptr, ptr], types::I64)?;
        declare("vyrn_sb_int", &[ptr, types::I64], types::I64)?;
        declare("vyrn_sb_float", &[ptr, types::F64], types::I64)?;
        declare("vyrn_sb_bool", &[ptr, types::I8], types::I64)?;
        declare("vyrn_sb_finish", &[ptr, ptr], ptr)?;
        declare("vyrn_print", &[ptr, ptr], types::I64)?;
        declare("vyrn_str_len", &[ptr], types::I64)?;
        declare("vyrn_str_cmp", &[ptr, ptr], types::I64)?;
        declare("vyrn_str_contains", &[ptr, ptr], types::I8)?;
        declare("vyrn_str_concat", &[ptr, ptr, ptr], ptr)?;
        declare("vyrn_str_repeat", &[ptr, types::I64, ptr], ptr)?;
        declare("vyrn_str_index", &[ptr, ptr, types::I64], ptr)?;
        declare(
            "vyrn_builtin",
            &[
                ptr,
                ptr,
                types::I64,
                types::I64,
                types::I64,
                types::I64,
                types::I64,
                types::I64,
            ],
            types::I64,
        )?;
        declare(
            "vyrn_str_method",
            &[
                ptr,
                ptr,
                ptr,
                types::I64,
                types::I64,
                types::I64,
                types::I64,
                types::I64,
            ],
            types::I64,
        )?;
        declare("vyrn_list_len", &[ptr], types::I64)?;
        declare("vyrn_list_new", &[types::I64, ptr], ptr)?;
        declare("vyrn_list_repeat", &[ptr, types::I64, ptr], ptr)?;
        declare("vyrn_list_concat", &[ptr, ptr, ptr], ptr)?;
        declare("vyrn_list_copy", &[ptr, ptr], ptr)?;
        declare("vyrn_list_sum_i", &[ptr, ptr], types::I64)?;
        declare("vyrn_list_get_i", &[ptr, types::I64, ptr], types::I64)?;
        declare("vyrn_list_get_f", &[ptr, types::I64, ptr], types::F64)?;
        declare(
            "vyrn_list_set_i",
            &[ptr, types::I64, types::I64, ptr],
            types::I64,
        )?;
        declare(
            "vyrn_list_set_f",
            &[ptr, types::I64, types::F64, ptr],
            types::I64,
        )?;
        declare("vyrn_list_push_i", &[ptr, types::I64], types::I64)?;
        declare("vyrn_list_push_f", &[ptr, types::F64], types::I64)?;
        declare("vyrn_ishl", &[types::I64, types::I64, ptr], types::I64)?;
        declare("vyrn_powf", &[types::F64, types::F64], types::F64)?;
        declare("vyrn_atan2", &[types::F64, types::F64], types::F64)?;
        declare("vyrn_hypot", &[types::F64, types::F64], types::F64)?;
        for (_, sym) in FLOAT1 {
            declare(sym, &[types::F64], types::F64)?;
        }
        let ctx = module.make_context();
        Ok(Jit {
            module,
            ctx,
            fctx: FunctionBuilderContext::new(),
            helpers,
            ptr,
            literals: Vec::new(),
        })
    }

    fn inner_signature(&self, params: &[NTy], ret: NTy) -> cranelift_codegen::ir::Signature {
        let mut sig = self.module.make_signature();
        for p in params {
            sig.params.push(AbiParam::new(cl_type(*p)));
        }
        // A struct result is written where the caller says.
        if matches!(ret, NTy::Struct(_)) {
            sig.params.push(AbiParam::new(self.ptr));
        }
        sig.params.push(AbiParam::new(self.ptr));
        sig.returns.push(AbiParam::new(cl_type(ret)));
        sig
    }

    fn declare_all(
        &mut self,
        sigs: &HashMap<String, (Vec<NTy>, NTy)>,
    ) -> Res<HashMap<String, FuncId>> {
        let mut ids = HashMap::new();
        for (name, (params, ret)) in sigs {
            let sig = self.inner_signature(params, *ret);
            let id = self
                .module
                .declare_function(&format!("vyrn_fn_{name}"), Linkage::Local, &sig)
                .map_err(|e| e.to_string())?;
            ids.insert(name.clone(), id);
        }
        Ok(ids)
    }

    fn define(
        &mut self,
        c: &Candidate,
        sigs: &HashMap<String, (Vec<NTy>, NTy)>,
        ids: &HashMap<String, FuncId>,
        env: &Env,
    ) -> Res<()> {
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = self.inner_signature(&c.params, c.ret);
        let FnBody::Block(body) = &c.decl.body else {
            return Err("no body".into());
        };
        let frontend = self.module.isa().frontend_config();
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let args: Vec<Value> = b.block_params(entry).to_vec();
            let has_sret = matches!(c.ret, NTy::Struct(_));
            let sret = has_sret.then(|| args[c.params.len()]);
            let err = args[c.params.len() + usize::from(has_sret)];
            let mut lower = Lower {
                b,
                module: &mut self.module,
                helpers: &self.helpers,
                literals: &mut self.literals,
                sigs,
                ids,
                env,
                refs: HashMap::new(),
                scopes: vec![Vec::new()],
                loops: Vec::new(),
                err,
                ret: c.ret,
                line: c.decl.name.span.start.line,
                layout: list_layout(),
                sret: None,
                allocs: 0,
            };
            lower.sret = sret;
            // Each parameter is the function's own; a struct is copied in.
            for (p, (v, ty)) in c.decl.params.iter().zip(args.iter().zip(&c.params)) {
                lower.declare(&p.name.name, *v, *ty);
            }
            // Count the call against the nesting limit.
            let depth = lower
                .b
                .ins()
                .load(types::I32, MemFlagsData::trusted(), err, 12);
            let depth = lower.b.ins().iadd_imm_s(depth, 1);
            lower.b.ins().store(MemFlagsData::trusted(), depth, err, 12);
            let too_deep =
                lower
                    .b
                    .ins()
                    .icmp_imm_s(IntCC::UnsignedGreaterThan, depth, i64::from(MAX_DEPTH));
            lower.fail_if(too_deep, ERR_STACK, 0);
            lower.block(body)?;
            // Falling off the end.
            if c.ret == NTy::Unit {
                let zero = lower.b.ins().iconst(types::I64, 0);
                lower.ret(zero);
            } else {
                lower.fail_now(ERR_NO_RETURN, 0);
            }
            lower.b.seal_all_blocks();
            lower.b.finalize(frontend);
        }
        let id = ids[&c.key];
        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| format!("code generation failed: {e:?}"))?;
        self.module.clear_context(&mut self.ctx);
        Ok(())
    }

    /// The entry the VM calls: arguments from an array of words, the
    /// result as a word.
    fn define_entry(
        &mut self,
        name: &str,
        params: &[NTy],
        ret: NTy,
        inner: FuncId,
        structs: &Structs,
    ) -> Res<FuncId> {
        let mut sig = self.module.make_signature();
        sig.params.push(AbiParam::new(self.ptr));
        sig.params.push(AbiParam::new(self.ptr));
        sig.returns.push(AbiParam::new(types::I64));
        let id = self
            .module
            .declare_function(&format!("vyrn_entry_{name}"), Linkage::Local, &sig)
            .map_err(|e| e.to_string())?;
        self.module.clear_context(&mut self.ctx);
        self.ctx.func.signature = sig;
        let frontend = self.module.isa().frontend_config();
        {
            let mut b = FunctionBuilder::new(&mut self.ctx.func, &mut self.fctx);
            let entry = b.create_block();
            b.append_block_params_for_function_params(entry);
            b.switch_to_block(entry);
            let (args_ptr, err) = (b.block_params(entry)[0], b.block_params(entry)[1]);
            let mut args = Vec::new();
            let mut at = 0usize;
            for p in params {
                if let NTy::Struct(id) = p {
                    // A struct's fields are consecutive words: pass where
                    // they start (the callee copies them).
                    args.push(b.ins().iadd_imm_s(args_ptr, (at * 8) as i64));
                    at += structs.count(*id);
                    continue;
                }
                let word = b.ins().load(
                    types::I64,
                    MemFlagsData::trusted(),
                    args_ptr,
                    (at * 8) as i32,
                );
                at += 1;
                args.push(match p {
                    NTy::Float => b.ins().bitcast(types::F64, MemFlagsData::new(), word),
                    NTy::Bool => b.ins().ireduce(types::I8, word),
                    _ => word,
                });
            }
            if matches!(ret, NTy::Struct(_)) {
                // Fields of a returned struct go in the state's `ret` words.
                args.push(b.ins().iadd_imm_s(err, 48));
            }
            args.push(err);
            let callee = self.module.declare_func_in_func(inner, b.func);
            let call = b.ins().call(callee, &args);
            let result = b.inst_results(call)[0];
            let word = match ret {
                NTy::Float => b.ins().bitcast(types::I64, MemFlagsData::new(), result),
                NTy::Bool => b.ins().uextend(types::I64, result),
                _ => result,
            };
            b.ins().return_(&[word]);
            b.seal_all_blocks();
            b.finalize(frontend);
        }
        self.module
            .define_function(id, &mut self.ctx)
            .map_err(|e| format!("code generation failed: {e:?}"))?;
        self.module.clear_context(&mut self.ctx);
        Ok(id)
    }
}

struct Lower<'a, 'f> {
    b: FunctionBuilder<'f>,
    module: &'a mut JITModule,
    helpers: &'a HashMap<&'static str, FuncId>,
    literals: &'a mut Vec<Rc<String>>,
    sigs: &'a HashMap<String, (Vec<NTy>, NTy)>,
    ids: &'a HashMap<String, FuncId>,
    env: &'a Env,
    refs: HashMap<FuncId, FuncRef>,
    scopes: Vec<Vec<(String, Variable, NTy)>>,
    /// For each loop: where `skip` and `stop` go.
    loops: Vec<(Block, Block)>,
    err: Value,
    ret: NTy,
    line: u32,
    layout: Option<ListLayout>,
    /// Where a struct result goes.
    sret: Option<Value>,
    /// Calls so far that may put a string or list in the arena.
    allocs: u32,
}

impl Lower<'_, '_> {
    fn func_ref(&mut self, id: FuncId) -> FuncRef {
        const ALLOCATING: &[&str] = &[
            "vyrn_list_new",
            "vyrn_list_repeat",
            "vyrn_list_concat",
            "vyrn_list_copy",
            "vyrn_sb_finish",
            "vyrn_str_concat",
            "vyrn_str_repeat",
            "vyrn_str_index",
            "vyrn_builtin",
            "vyrn_str_method",
        ];
        if ALLOCATING.iter().any(|n| self.helpers.get(n) == Some(&id))
            || self.ids.values().any(|v| *v == id)
        {
            // Helpers that make strings or lists, and compiled functions,
            // which may return one.
            self.allocs += 1;
        }
        if let Some(r) = self.refs.get(&id) {
            return *r;
        }
        let r = self.module.declare_func_in_func(id, self.b.func);
        self.refs.insert(id, r);
        r
    }

    fn lookup(&self, name: &str) -> Option<(Variable, NTy)> {
        for scope in self.scopes.iter().rev() {
            if let Some((_, v, t)) = scope.iter().rev().find(|(n, _, _)| n == name) {
                return Some((*v, *t));
            }
        }
        None
    }

    /// Starts a fresh block for code after a jump or return; it is
    /// unreachable unless something jumps to it.
    fn dead(&mut self) {
        let next = self.b.create_block();
        self.b.switch_to_block(next);
    }

    fn zero(&mut self, t: NTy) -> Value {
        match t {
            NTy::Float => self.b.ins().f64const(0.0),
            NTy::Bool => self.b.ins().iconst(types::I8, 0),
            _ => self.b.ins().iconst(types::I64, 0),
        }
    }

    fn ret(&mut self, v: Value) {
        let depth = self
            .b
            .ins()
            .load(types::I32, MemFlagsData::trusted(), self.err, 12);
        let depth = self.b.ins().iadd_imm_s(depth, -1);
        self.b
            .ins()
            .store(MemFlagsData::trusted(), depth, self.err, 12);
        self.b.ins().return_(&[v]);
        self.dead();
    }

    /// Records a failure and returns at once; the VM reads it.
    fn fail_now(&mut self, code: u32, detail: u32) {
        let c = self.b.ins().iconst(types::I32, i64::from(code));
        let d = self.b.ins().iconst(types::I32, i64::from(detail));
        let l = self.b.ins().iconst(types::I32, i64::from(self.line));
        self.b.ins().store(MemFlagsData::trusted(), c, self.err, 0);
        self.b.ins().store(MemFlagsData::trusted(), d, self.err, 4);
        self.b.ins().store(MemFlagsData::trusted(), l, self.err, 8);
        let z = self.zero(self.ret);
        self.b.ins().return_(&[z]);
        self.dead();
    }

    fn fail_if(&mut self, cond: Value, code: u32, detail: u32) {
        let bad = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(cond, bad, &[], ok, &[]);
        self.b.switch_to_block(bad);
        self.fail_now(code, detail);
        // `fail_now` left a dead block current; leave it terminated.
        let z = self.zero(self.ret);
        self.b.ins().return_(&[z]);
        self.b.switch_to_block(ok);
    }

    /// After a call into native code: pass a failure on.
    fn check_err(&mut self) {
        let code = self
            .b
            .ins()
            .load(types::I32, MemFlagsData::trusted(), self.err, 0);
        let bad = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(code, bad, &[], ok, &[]);
        self.b.switch_to_block(bad);
        let z = self.zero(self.ret);
        self.b.ins().return_(&[z]);
        self.b.switch_to_block(ok);
    }

    fn as_float(&mut self, v: Value, t: NTy) -> Value {
        match t {
            NTy::Int => self.b.ins().fcvt_from_sint(types::F64, v),
            _ => v,
        }
    }

    fn truthy(&mut self, v: Value, t: NTy) -> Res<Value> {
        Ok(match t {
            NTy::Bool => v,
            NTy::Int => self.b.ins().icmp_imm_s(IntCC::NotEqual, v, 0),
            NTy::Float => {
                let z = self.b.ins().f64const(0.0);
                self.b.ins().fcmp(FloatCC::NotEqual, v, z)
            }
            NTy::Str => {
                let len = self.helper("vyrn_str_len", &[v]);
                self.b.ins().icmp_imm_s(IntCC::NotEqual, len, 0)
            }
            _ => return Err("this value used as a condition".into()),
        })
    }

    // ---- statements ----

    fn block(&mut self, block: &AstBlock) -> Res<()> {
        self.scopes.push(Vec::new());
        for stmt in &block.stmts {
            self.stmt(stmt)?;
        }
        self.scopes.pop();
        Ok(())
    }

    fn assign(&mut self, name: &str, v: Value, t: NTy) -> Res<()> {
        match self.lookup(name) {
            Some((var, NTy::Struct(id))) if t == NTy::Struct(id) => {
                let slot = self.b.use_var(var);
                self.copy_struct(slot, v, id);
                Ok(())
            }
            Some((var, old)) if old == t => {
                self.b.def_var(var, v);
                Ok(())
            }
            Some((_, old)) => Err(format!(
                "`{name}` changes from {} to {}",
                ty_name(old),
                ty_name(t)
            )),
            None => {
                self.declare(name, v, t);
                Ok(())
            }
        }
    }

    fn declare(&mut self, name: &str, v: Value, t: NTy) {
        // A struct variable owns a slot of its own, and holds its address.
        let v = match t {
            NTy::Struct(id) => {
                let slot = self.struct_slot(id);
                self.copy_struct(slot, v, id);
                slot
            }
            _ => v,
        };
        let var = self.b.declare_var(cl_type(t));
        self.b.def_var(var, v);
        self.scopes
            .last_mut()
            .expect("a scope is open")
            .push((name.to_string(), var, t));
    }

    fn stmt(&mut self, stmt: &Stmt) -> Res<()> {
        self.line = stmt.span.start.line;
        match &stmt.kind {
            StmtKind::Binding(binding) => {
                let BindTarget::Name(name) = &binding.target else {
                    return Err("only plain names can be bound".into());
                };
                let value = binding.value.as_ref().ok_or("a binding without a value")?;
                let want = binding
                    .declared_ty
                    .as_ref()
                    .and_then(|t| param_type(t, &self.env.structs));
                let (v, t) = self.expr_as(value, want)?;
                if let Some(declared) = &binding.declared_ty {
                    if param_type(declared, &self.env.structs) != Some(t) {
                        return Err(format!(
                            "`{}`'s declared type differs from its value",
                            name.name
                        ));
                    }
                }
                if t == NTy::Unit {
                    return Err("a variable holding `none`".into());
                }
                self.declare(&name.name, v, t);
            }
            StmtKind::Assign { target, op, value } => {
                if let ExprKind::Index { base, subscripts } = &target.kind {
                    return self.assign_item(base, subscripts, *op, value);
                }
                if let ExprKind::Field {
                    base,
                    name,
                    optional: false,
                } = &target.kind
                {
                    return self.assign_field(base, &name.name, *op, value);
                }
                let ExprKind::Name(name) = &target.kind else {
                    return Err("only variables and list items can be assigned".into());
                };
                let (v, t) = match op {
                    AssignOp::Assign => {
                        let want = self.lookup(&name.name).map(|(_, t)| t);
                        self.expr_as(value, want)?
                    }
                    op => {
                        let (var, old) = self
                            .lookup(&name.name)
                            .ok_or_else(|| format!("`{}` is not a local number", name.name))?;
                        let current = self.b.use_var(var);
                        let (rhs, rt) = self.expr(value)?;
                        let bop = assign_op(*op).ok_or("this compound assignment")?;
                        if bop == BinaryOp::Pow
                            && old == NTy::Int
                            && rt == NTy::Int
                            && !matches!(value.kind, ExprKind::Int { .. })
                        {
                            return Err(
                                "`int **= int` with an exponent that is not a literal".into()
                            );
                        }
                        self.binary(bop, current, old, rhs, rt)?
                    }
                };
                if t == NTy::Unit {
                    return Err("a variable holding `none`".into());
                }
                self.assign(&name.name, v, t)?;
            }
            StmtKind::Expr(e) => {
                self.expr(e)?;
            }
            StmtKind::Return(value) => match (value, self.ret) {
                (None, NTy::Unit) => {
                    let z = self.b.ins().iconst(types::I64, 0);
                    self.ret(z);
                }
                (Some(e), want) if want != NTy::Unit => {
                    let (v, t) = self.expr_as(e, Some(want))?;
                    if t != want {
                        return Err(format!(
                            "returns {} where {} is declared",
                            ty_name(t),
                            ty_name(want)
                        ));
                    }
                    if let (NTy::Struct(id), Some(sret)) = (want, self.sret) {
                        self.copy_struct(sret, v, id);
                        let z = self.b.ins().iconst(types::I64, 0);
                        self.ret(z);
                    } else {
                        self.ret(v);
                    }
                }
                _ => return Err("the `return` does not match the declared result".into()),
            },
            StmtKind::If(s) => {
                let merge = self.b.create_block();
                for (cond, body) in &s.branches {
                    let Condition::Expr(c) = cond else {
                        return Err("`if let`".into());
                    };
                    let (v, t) = self.expr(c)?;
                    let c = self.truthy(v, t)?;
                    let then = self.b.create_block();
                    let next = self.b.create_block();
                    self.b.ins().brif(c, then, &[], next, &[]);
                    self.b.switch_to_block(then);
                    self.block(body)?;
                    self.b.ins().jump(merge, &[]);
                    self.b.switch_to_block(next);
                }
                if let Some(body) = &s.else_body {
                    self.block(body)?;
                }
                self.b.ins().jump(merge, &[]);
                self.b.switch_to_block(merge);
            }
            StmtKind::While { condition, body } => {
                let Condition::Expr(c) = condition else {
                    return Err("`loop let`".into());
                };
                let mark = self.arena_mark();
                let header = self.b.create_block();
                let inside = self.b.create_block();
                let next = self.b.create_block();
                let exit = self.b.create_block();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                let (v, t) = self.expr(c)?;
                let c = self.truthy(v, t)?;
                self.b.ins().brif(c, inside, &[], exit, &[]);
                self.b.switch_to_block(inside);
                self.loops.push((next, exit));
                let before = self.allocs;
                self.block(body)?;
                let allocates = self.allocs != before;
                self.loops.pop();
                self.b.ins().jump(next, &[]);
                self.b.switch_to_block(next);
                if allocates {
                    self.arena_collect(mark);
                }
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(exit);
            }
            StmtKind::Loop(body) => {
                let mark = self.arena_mark();
                let header = self.b.create_block();
                let next = self.b.create_block();
                let exit = self.b.create_block();
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(header);
                self.loops.push((next, exit));
                let before = self.allocs;
                self.block(body)?;
                let allocates = self.allocs != before;
                self.loops.pop();
                self.b.ins().jump(next, &[]);
                self.b.switch_to_block(next);
                if allocates {
                    self.arena_collect(mark);
                }
                self.b.ins().jump(header, &[]);
                self.b.switch_to_block(exit);
            }
            StmtKind::For(f) => self.each(f)?,
            StmtKind::Stop => {
                let (_, exit) = *self.loops.last().ok_or("`stop` outside a loop")?;
                self.b.ins().jump(exit, &[]);
                self.dead();
            }
            StmtKind::Skip => {
                let (next, _) = *self.loops.last().ok_or("`skip` outside a loop")?;
                self.b.ins().jump(next, &[]);
                self.dead();
            }
            _ => return Err(format!("this kind of statement (line {})", self.line)),
        }
        Ok(())
    }

    /// `each i in a until b by step`, `a to b`, `range(...)`, or a list.
    fn each(&mut self, f: &crate::ast::ForStmt) -> Res<()> {
        if f.mode != ForMode::Sequential || f.pass != PassMode::Borrow {
            return Err("`par each`, `async each` and `each mut` loops".into());
        }
        let BindTarget::Name(var_name) = &f.target else {
            return Err("loops that unpack items".into());
        };
        let body = &f.body;
        self.loop_over(&var_name.name, &f.iter, &mut |s: &mut Self| {
            for stmt in &body.stmts {
                s.stmt(stmt)?;
            }
            Ok(())
        })
    }

    /// Runs `body` once per item of `iter` (a range or a list of numbers),
    /// with `name` bound to the item in a scope of its own.
    fn loop_over(
        &mut self,
        name: &str,
        iter: &Expr,
        body: &mut dyn FnMut(&mut Self) -> Res<()>,
    ) -> Res<()> {
        let range_call = matches!(&iter.kind, ExprKind::Call { callee, args }
            if matches!(&callee.kind, ExprKind::Name(n) if n.name == "range"
                && self.lookup("range").is_none()
                && !self.env.globals.iter().any(|g| g == "range"))
                && (1..=3).contains(&args.len())
                && args.iter().all(|a| a.name.is_none()));
        let (s, e, inclusive, step) = match &iter.kind {
            ExprKind::Range {
                start,
                end,
                inclusive,
                step,
            } => {
                let step = step.as_deref().map(literal_int).transpose()?.unwrap_or(1);
                let (s, st) = self.expr(start)?;
                let (e, et) = self.expr(end)?;
                if st != NTy::Int || et != NTy::Int {
                    return Err("ranges of floats".into());
                }
                (s, e, *inclusive, step)
            }
            ExprKind::Call { args, .. } if range_call => {
                let step = match args.get(2) {
                    Some(a) => literal_int(&a.value)?,
                    None => 1,
                };
                let (s, e) = if args.len() == 1 {
                    let (e, et) = self.expr(&args[0].value)?;
                    if et != NTy::Int {
                        return Err("ranges of floats".into());
                    }
                    (self.b.ins().iconst(types::I64, 0), e)
                } else {
                    let (s, st) = self.expr(&args[0].value)?;
                    let (e, et) = self.expr(&args[1].value)?;
                    if st != NTy::Int || et != NTy::Int {
                        return Err("ranges of floats".into());
                    }
                    (s, e)
                };
                (s, e, false, step)
            }
            _ => {
                let (list, lt) = self.expr(iter)?;
                if !lt.is_list() {
                    return Err("loops over anything but a range or a list of numbers".into());
                }
                return self.each_item(name, list, lt, body);
            }
        };
        if step == 0 {
            return Err("a range with step 0".into());
        }
        self.counted_loop(name, s, e, inclusive, step, body)
    }

    fn counted_loop(
        &mut self,
        name: &str,
        start: Value,
        end: Value,
        inclusive: bool,
        step: i64,
        body: &mut dyn FnMut(&mut Self) -> Res<()>,
    ) -> Res<()> {
        let counter = self.b.declare_var(types::I64);
        self.b.def_var(counter, start);
        let mark = self.arena_mark();
        let header = self.b.create_block();
        let inside = self.b.create_block();
        let next = self.b.create_block();
        let exit = self.b.create_block();
        self.b.ins().jump(header, &[]);
        self.b.switch_to_block(header);
        let i = self.b.use_var(counter);
        let cc = match (step > 0, inclusive) {
            (true, false) => IntCC::SignedLessThan,
            (true, true) => IntCC::SignedLessThanOrEqual,
            (false, false) => IntCC::SignedGreaterThan,
            (false, true) => IntCC::SignedGreaterThanOrEqual,
        };
        let go = self.b.ins().icmp(cc, i, end);
        self.b.ins().brif(go, inside, &[], exit, &[]);
        self.b.switch_to_block(inside);
        self.scopes.push(Vec::new());
        let i = self.b.use_var(counter);
        self.declare(name, i, NTy::Int);
        self.loops.push((next, exit));
        let before = self.allocs;
        body(self)?;
        let allocates = self.allocs != before;
        self.loops.pop();
        self.scopes.pop();
        self.b.ins().jump(next, &[]);
        self.b.switch_to_block(next);
        if allocates {
            self.arena_collect(mark);
        }
        let i = self.b.use_var(counter);
        let step_v = self.b.ins().iconst(types::I64, step);
        let (n, overflow) = self.b.ins().sadd_overflow(i, step_v);
        self.b.def_var(counter, n);
        // Past the largest integer: the range has ended.
        self.b.ins().brif(overflow, exit, &[], header, &[]);
        self.b.switch_to_block(exit);
        Ok(())
    }

    // ---- freeing what loops make ----

    fn arena_mark(&mut self) -> Value {
        self.helper("vyrn_arena_mark", &[self.err])
    }

    /// Frees the strings and lists made since `mark` that no variable in
    /// scope holds. Every string and list a function can reach is in a
    /// variable (lists hold only numbers, structs only numbers), so the
    /// variables are all that can keep one alive.
    fn arena_collect(&mut self, mark: Value) {
        let live: Vec<Variable> = self
            .scopes
            .iter()
            .flatten()
            .filter(|(_, _, t)| matches!(t, NTy::Str | NTy::ListInt | NTy::ListFloat))
            .map(|(_, v, _)| *v)
            .collect();
        let size = (live.len().max(1) * 8) as u32;
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            3,
        ));
        let base = self.b.ins().stack_addr(types::I64, slot, 0);
        for (i, v) in live.iter().enumerate() {
            let p = self.b.use_var(*v);
            self.b
                .ins()
                .store(MemFlagsData::trusted(), p, base, (i * 8) as i32);
        }
        let n = self.b.ins().iconst(types::I64, live.len() as i64);
        self.helper("vyrn_arena_collect", &[self.err, mark, base, n]);
    }

    // ---- strings ----

    /// Calls a runtime helper and gives its result.
    fn helper(&mut self, name: &str, args: &[Value]) -> Value {
        let f = self.func_ref(self.helpers[name]);
        let call = self.b.ins().call(f, args);
        self.b.inst_results(call)[0]
    }

    /// A string literal: kept with the machine code, so it costs nothing
    /// to use.
    fn literal(&mut self, text: &str) -> Value {
        let rc = Rc::new(text.to_string());
        let p = Rc::as_ptr(&rc) as i64;
        self.literals.push(rc);
        self.b.ins().iconst(types::I64, p)
    }

    /// `"text"` and `"text {value} more"`.
    fn string(&mut self, segments: &[crate::ast::StrSegment]) -> Res<(Value, NTy)> {
        use crate::ast::StrSegment;
        if let [StrSegment::Text(t)] = segments {
            return Ok((self.literal(t), NTy::Str));
        }
        if segments.is_empty() {
            return Ok((self.literal(""), NTy::Str));
        }
        let sb = self.helper("vyrn_sb_new", &[self.err]);
        for seg in segments {
            match seg {
                StrSegment::Text(t) => {
                    let lit = self.literal(t);
                    self.helper("vyrn_sb_str", &[sb, lit]);
                }
                StrSegment::Interp(e) => {
                    let (v, t) = self.expr(e)?;
                    self.append(sb, v, t)?;
                }
            }
        }
        Ok((self.helper("vyrn_sb_finish", &[sb, self.err]), NTy::Str))
    }

    /// Adds a value to text being built, as `print` and `"{x}"` show it.
    fn append(&mut self, sb: Value, v: Value, t: NTy) -> Res<()> {
        let helper = match t {
            NTy::Int => "vyrn_sb_int",
            NTy::Float => "vyrn_sb_float",
            NTy::Bool => "vyrn_sb_bool",
            NTy::Str => "vyrn_sb_str",
            _ => return Err(format!("showing {} as text", ty_name(t))),
        };
        self.helper(helper, &[sb, v]);
        Ok(())
    }

    fn as_word(&mut self, v: Value, t: NTy) -> Value {
        match t {
            NTy::Float => self.b.ins().bitcast(types::I64, MemFlagsData::new(), v),
            NTy::Bool => self.b.ins().uextend(types::I64, v),
            _ => v,
        }
    }

    fn word_to(&mut self, w: Value, t: NTy) -> Value {
        match t {
            NTy::Float => self.b.ins().bitcast(types::F64, MemFlagsData::new(), w),
            NTy::Bool => self.b.ins().ireduce(types::I8, w),
            _ => w,
        }
    }

    /// A builtin the VM runs for us (`str(x)`, `int(text)`, `format`, ...).
    fn generic_builtin(&mut self, name: &str, args: &[(Value, NTy)]) -> Res<(Value, NTy)> {
        let types: Vec<NTy> = args.iter().map(|(_, t)| *t).collect();
        let want = builtin_result(name, &types).ok_or_else(|| format!("calls `{name}`"))?;
        let mut kinds = 0i64;
        let mut words = Vec::new();
        for (i, (v, t)) in args.iter().enumerate() {
            kinds |= kind_of(*t)? << (4 * i);
            words.push(self.as_word(*v, *t));
        }
        while words.len() < 3 {
            words.push(self.b.ins().iconst(types::I64, 0));
        }
        let name_v = self.literal(name);
        let n = self.b.ins().iconst(types::I64, args.len() as i64);
        let kinds = self.b.ins().iconst(types::I64, kinds);
        let want_k = self.b.ins().iconst(types::I64, kind_of(want)?);
        let w = self.helper(
            "vyrn_builtin",
            &[
                self.err, name_v, n, kinds, words[0], words[1], words[2], want_k,
            ],
        );
        self.check_err_with_line();
        Ok((self.word_to(w, want), want))
    }

    // ---- structs ----

    /// A fresh stack slot for a struct: one 8-byte word per field.
    fn struct_slot(&mut self, id: u16) -> Value {
        let size = (self.env.structs.count(id).max(1) * 8) as u32;
        let slot = self.b.create_sized_stack_slot(StackSlotData::new(
            StackSlotKind::ExplicitSlot,
            size,
            3,
        ));
        self.b.ins().stack_addr(types::I64, slot, 0)
    }

    fn copy_struct(&mut self, to: Value, from: Value, id: u16) {
        let fields: Vec<NTy> = self
            .env
            .structs
            .fields
            .get(&id)
            .map(|(_, f)| f.iter().map(|(_, t)| *t).collect())
            .unwrap_or_default();
        for (i, t) in fields.into_iter().enumerate() {
            let off = (i * 8) as i32;
            let v = self
                .b
                .ins()
                .load(cl_type(t), MemFlagsData::trusted(), from, off);
            self.b.ins().store(MemFlagsData::trusted(), v, to, off);
        }
    }

    /// Calls a compiled function or method with `values` (its arguments,
    /// `err` added here); a struct result lands in a new slot.
    fn call_compiled(&mut self, key: &str, mut values: Vec<Value>, ret: NTy) -> (Value, NTy) {
        let sret = match ret {
            NTy::Struct(id) => {
                let slot = self.struct_slot(id);
                values.push(slot);
                Some(slot)
            }
            _ => None,
        };
        values.push(self.err);
        let f = self.func_ref(self.ids[key]);
        let call = self.b.ins().call(f, &values);
        let v = self.b.inst_results(call)[0];
        self.check_err();
        (sret.unwrap_or(v), ret)
    }

    /// `Point(1.0, 2.0)` or `Point(x = 1.0, y = 2.0)`: every field given,
    /// each with exactly its declared type.
    fn construct(&mut self, id: u16, args: &[crate::ast::Arg]) -> Res<(Value, NTy)> {
        let fields: Vec<(String, NTy)> = self.env.structs.fields[&id].1.clone();
        if args.len() != fields.len() {
            return Err(format!(
                "`{}(...)` without every field",
                self.env.structs.name(id)
            ));
        }
        let slot = self.struct_slot(id);
        let named = args.iter().any(|a| a.name.is_some());
        for (k, a) in args.iter().enumerate() {
            let i = match (&a.name, named) {
                (Some(n), _) => fields
                    .iter()
                    .position(|(f, _)| *f == n.name)
                    .ok_or_else(|| format!("no field `{}`", n.name))?,
                (None, false) => k,
                (None, true) => {
                    return Err("a constructor mixing named and positional fields".into())
                }
            };
            let (v, t) = self.expr(&a.value)?;
            if t != fields[i].1 {
                return Err(format!(
                    "field `{}` given {} where {} is declared",
                    fields[i].0,
                    ty_name(t),
                    ty_name(fields[i].1)
                ));
            }
            self.b
                .ins()
                .store(MemFlagsData::trusted(), v, slot, (i * 8) as i32);
        }
        Ok((slot, NTy::Struct(id)))
    }

    /// `p.x = v` and `p.x += v` on a struct variable.
    fn assign_field(&mut self, base: &Expr, field: &str, op: AssignOp, value: &Expr) -> Res<()> {
        let ExprKind::Name(var) = &base.kind else {
            return Err("assigning a field of something that is not a variable".into());
        };
        let (v, t) = self
            .lookup(&var.name)
            .ok_or_else(|| format!("`{}` is not a local struct", var.name))?;
        let NTy::Struct(id) = t else {
            return Err("assigning a field of something that is not a struct".into());
        };
        let slot = self.b.use_var(v);
        let (i, ft) = self
            .env
            .structs
            .field(id, field)
            .ok_or_else(|| format!("no field `{field}`"))?;
        let off = (i * 8) as i32;
        let (nv, nt) = match op {
            AssignOp::Assign => self.expr(value)?,
            op => {
                let current = self
                    .b
                    .ins()
                    .load(cl_type(ft), MemFlagsData::trusted(), slot, off);
                let (rhs, rt) = self.expr(value)?;
                let bop = assign_op(op).ok_or("this compound assignment")?;
                if bop == BinaryOp::Pow
                    && rt == NTy::Int
                    && !matches!(value.kind, ExprKind::Int { .. })
                {
                    return Err("`**=` with an exponent that is not a literal".into());
                }
                self.binary(bop, current, ft, rhs, rt)?
            }
        };
        if nt != ft {
            return Err(format!(
                "field `{field}` given {} where {} is declared",
                ty_name(nt),
                ty_name(ft)
            ));
        }
        self.b.ins().store(MemFlagsData::trusted(), nv, slot, off);
        Ok(())
    }

    // ---- lists ----

    fn list_and_index(
        &mut self,
        base: &Expr,
        subscripts: &[crate::ast::Subscript],
    ) -> Res<(Value, NTy, Value)> {
        let [crate::ast::Subscript::Index(index)] = subscripts else {
            return Err("slices and multi-dimensional indexing".into());
        };
        let (list, lt) = self.expr(base)?;
        if !lt.is_list() && lt != NTy::Str {
            return Err("indexing something that is not a list of numbers or a string".into());
        }
        let (i, it) = self.expr(index)?;
        if it != NTy::Int {
            return Err("a list index that is not an int".into());
        }
        Ok((list, lt, i))
    }

    fn list_len(&mut self, list: Value) -> Value {
        if let Some(l) = self.layout {
            return self
                .b
                .ins()
                .load(types::I64, MemFlagsData::trusted(), list, l.len);
        }
        let f = self.func_ref(self.helpers["vyrn_list_len"]);
        let call = self.b.ins().call(f, &[list]);
        self.b.inst_results(call)[0]
    }

    /// The address of item `i` (negative counts from the end), failing
    /// with the interpreter's `IndexError` when it is out of range.
    fn item_address(&mut self, list: Value, i: Value, l: ListLayout, elem: NTy) -> Value {
        let len = self
            .b
            .ins()
            .load(types::I64, MemFlagsData::trusted(), list, l.len);
        let negative = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, i, 0);
        let wrapped = self.b.ins().iadd(i, len);
        let at = self.b.ins().select(negative, wrapped, i);
        // One unsigned comparison covers both ends.
        let inside = self.b.ins().icmp(IntCC::UnsignedLessThan, at, len);
        let bad = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(inside, ok, &[], bad, &[]);
        self.b.switch_to_block(bad);
        self.b.ins().store(MemFlagsData::trusted(), i, self.err, 16);
        self.b
            .ins()
            .store(MemFlagsData::trusted(), len, self.err, 24);
        self.fail_now(crate::vm::native_abi::ERR_INDEX, 0);
        let z = self.zero(self.ret);
        self.b.ins().return_(&[z]);
        self.b.switch_to_block(ok);
        let data = self
            .b
            .ins()
            .load(types::I64, MemFlagsData::trusted(), list, l.data);
        let offset = self.b.ins().ishl_imm_s(at, 4);
        let item = self.b.ins().iadd(data, offset);
        let payload = if elem == NTy::Int {
            l.int_payload
        } else {
            l.float_payload
        };
        self.b.ins().iadd_imm_s(item, i64::from(payload))
    }

    fn list_get(&mut self, list: Value, i: Value, elem: NTy) -> Value {
        if let Some(l) = self.layout {
            let at = self.item_address(list, i, l, elem);
            return self
                .b
                .ins()
                .load(cl_type(elem), MemFlagsData::trusted(), at, 0);
        }
        let get = if elem == NTy::Int {
            "vyrn_list_get_i"
        } else {
            "vyrn_list_get_f"
        };
        let f = self.func_ref(self.helpers[get]);
        let call = self.b.ins().call(f, &[list, i, self.err]);
        let v = self.b.inst_results(call)[0];
        self.check_err_with_line();
        v
    }

    fn list_set(&mut self, list: Value, i: Value, v: Value, elem: NTy) {
        if let Some(l) = self.layout {
            let at = self.item_address(list, i, l, elem);
            self.b.ins().store(MemFlagsData::trusted(), v, at, 0);
            return;
        }
        let set = if elem == NTy::Int {
            "vyrn_list_set_i"
        } else {
            "vyrn_list_set_f"
        };
        let f = self.func_ref(self.helpers[set]);
        self.b.ins().call(f, &[list, i, v, self.err]);
        self.check_err_with_line();
    }

    fn assign_item(
        &mut self,
        base: &Expr,
        subscripts: &[crate::ast::Subscript],
        op: AssignOp,
        value: &Expr,
    ) -> Res<()> {
        let (list, lt, i) = self.list_and_index(base, subscripts)?;
        if lt == NTy::Str {
            return Err("assigning into a string".into());
        }
        let elem = if lt == NTy::ListInt {
            NTy::Int
        } else {
            NTy::Float
        };
        let (v, t) = match op {
            AssignOp::Assign => self.expr(value)?,
            op => {
                let current = self.list_get(list, i, elem);
                let (rhs, rt) = self.expr(value)?;
                let bop = assign_op(op).ok_or("this compound assignment")?;
                if bop == BinaryOp::Pow
                    && rt == NTy::Int
                    && !matches!(value.kind, ExprKind::Int { .. })
                {
                    return Err("`**=` with an exponent that is not a literal".into());
                }
                self.binary(bop, current, elem, rhs, rt)?
            }
        };
        if t != elem {
            return Err(format!(
                "putting {} into a list of {}",
                ty_name(t),
                ty_name(elem)
            ));
        }
        self.list_set(list, i, v, elem);
        Ok(())
    }

    /// `xs.append(x)`, and `len` as a method.
    fn method(&mut self, base: &Expr, name: &str, args: &[crate::ast::Arg]) -> Res<(Value, NTy)> {
        let (list, lt) = self.expr(base)?;
        if lt == NTy::Str {
            let want =
                str_method_result(name).ok_or_else(|| format!("the string method `{name}`"))?;
            if args.len() > 2 || args.iter().any(|a| a.name.is_some()) {
                return Err(format!("`.{name}()` with these arguments"));
            }
            let mut words = Vec::new();
            let mut kinds = 0i64;
            for (i, a) in args.iter().enumerate() {
                let (v, t) = self.expr(&a.value)?;
                kinds |= kind_of(t)? << (4 * i);
                words.push(self.as_word(v, t));
            }
            while words.len() < 2 {
                words.push(self.b.ins().iconst(types::I64, 0));
            }
            let name_v = self.literal(name);
            let n = self.b.ins().iconst(types::I64, args.len() as i64);
            let kinds = self.b.ins().iconst(types::I64, kinds);
            let want_k = self.b.ins().iconst(types::I64, kind_of(want)?);
            let w = self.helper(
                "vyrn_str_method",
                &[self.err, name_v, list, n, kinds, words[0], words[1], want_k],
            );
            self.check_err_with_line();
            return Ok((self.word_to(w, want), want));
        }
        if let NTy::Struct(id) = lt {
            let key = format!("{}.{name}", self.env.structs.name(id));
            let (params, ret) = self
                .sigs
                .get(&key)
                .cloned()
                .ok_or_else(|| format!("calls `{key}`, which stays interpreted"))?;
            if args.len() + 1 != params.len() {
                return Err(format!("`{key}` called with {} arguments", args.len()));
            }
            let mut values = vec![list];
            for (a, want) in args.iter().zip(&params[1..]) {
                let (v, t) = self.expr_as(&a.value, Some(*want))?;
                if t != *want {
                    return Err(format!(
                        "`{key}` receives {} where it declares {}",
                        ty_name(t),
                        ty_name(*want)
                    ));
                }
                values.push(v);
            }
            return Ok(self.call_compiled(&key, values, ret));
        }
        if !lt.is_list() {
            return Err("method calls on values that are not lists of numbers".into());
        }
        let elem = if lt == NTy::ListInt {
            NTy::Int
        } else {
            NTy::Float
        };
        match name {
            "append" => {
                let a = self.args(args, 1)?;
                if a[0].1 != elem {
                    return Err(format!(
                        "appending {} to a list of {}",
                        ty_name(a[0].1),
                        ty_name(elem)
                    ));
                }
                let push = if elem == NTy::Int {
                    "vyrn_list_push_i"
                } else {
                    "vyrn_list_push_f"
                };
                let f = self.func_ref(self.helpers[push]);
                self.b.ins().call(f, &[list, a[0].0]);
                Ok((self.b.ins().iconst(types::I64, 0), NTy::Unit))
            }
            "copy" => {
                self.args(args, 0)?;
                let f = self.func_ref(self.helpers["vyrn_list_copy"]);
                let call = self.b.ins().call(f, &[list, self.err]);
                Ok((self.b.inst_results(call)[0], lt))
            }
            "sum" if lt == NTy::ListInt => {
                self.args(args, 0)?;
                self.list_sum(list)
            }
            _ => Err(format!("the list method `{name}`")),
        }
    }

    fn list_sum(&mut self, list: Value) -> Res<(Value, NTy)> {
        let f = self.func_ref(self.helpers["vyrn_list_sum_i"]);
        let call = self.b.ins().call(f, &[list, self.err]);
        let v = self.b.inst_results(call)[0];
        self.check_err_with_line();
        Ok((v, NTy::Int))
    }

    /// `each x in xs`: by position, reading the length each time round, as
    /// the interpreter does, so items appended in the loop are visited.
    fn each_item(
        &mut self,
        name: &str,
        list: Value,
        lt: NTy,
        body: &mut dyn FnMut(&mut Self) -> Res<()>,
    ) -> Res<()> {
        let elem = if lt == NTy::ListInt {
            NTy::Int
        } else {
            NTy::Float
        };
        let counter = self.b.declare_var(types::I64);
        let zero = self.b.ins().iconst(types::I64, 0);
        self.b.def_var(counter, zero);
        let mark = self.arena_mark();
        let header = self.b.create_block();
        let inside = self.b.create_block();
        let next = self.b.create_block();
        let exit = self.b.create_block();
        self.b.ins().jump(header, &[]);
        self.b.switch_to_block(header);
        let i = self.b.use_var(counter);
        let len = self.list_len(list);
        let go = self.b.ins().icmp(IntCC::SignedLessThan, i, len);
        self.b.ins().brif(go, inside, &[], exit, &[]);
        self.b.switch_to_block(inside);
        let item = self.list_get(list, i, elem);
        self.scopes.push(Vec::new());
        self.declare(name, item, elem);
        self.loops.push((next, exit));
        let before = self.allocs;
        body(self)?;
        let allocates = self.allocs != before;
        self.loops.pop();
        self.scopes.pop();
        self.b.ins().jump(next, &[]);
        self.b.switch_to_block(next);
        if allocates {
            self.arena_collect(mark);
        }
        let i = self.b.use_var(counter);
        let n = self.b.ins().iadd_imm_s(i, 1);
        self.b.def_var(counter, n);
        self.b.ins().jump(header, &[]);
        self.b.switch_to_block(exit);
        Ok(())
    }

    // ---- expressions ----

    /// An expression where the type wanted is known, which is what gives
    /// an empty `[]` its type.
    fn expr_as(&mut self, e: &Expr, want: Option<NTy>) -> Res<(Value, NTy)> {
        if let (ExprKind::List(items), Some(t)) = (&e.kind, want) {
            if items.is_empty() && t.is_list() {
                let zero = self.b.ins().iconst(types::I64, 0);
                return Ok((self.new_list(zero), t));
            }
        }
        self.expr(e)
    }

    fn new_list(&mut self, capacity: Value) -> Value {
        let f = self.func_ref(self.helpers["vyrn_list_new"]);
        let call = self.b.ins().call(f, &[capacity, self.err]);
        self.b.inst_results(call)[0]
    }

    fn list_push(&mut self, list: Value, v: Value, elem: NTy) {
        let push = if elem == NTy::Int {
            "vyrn_list_push_i"
        } else {
            "vyrn_list_push_f"
        };
        let f = self.func_ref(self.helpers[push]);
        self.b.ins().call(f, &[list, v]);
    }

    /// `[a, b, c]`: every item an `int`, or every item a `float`.
    fn list_literal(&mut self, items: &[Expr]) -> Res<(Value, NTy)> {
        if items.is_empty() {
            return Err("an empty list whose item type is not known here".into());
        }
        let mut values = Vec::new();
        for item in items {
            values.push(self.expr(item)?);
        }
        let elem = values[0].1;
        if !matches!(elem, NTy::Int | NTy::Float) || values.iter().any(|(_, t)| *t != elem) {
            return Err("a list whose items are not all ints or all floats".into());
        }
        let cap = self.b.ins().iconst(types::I64, items.len() as i64);
        let list = self.new_list(cap);
        for (v, _) in values {
            self.list_push(list, v, elem);
        }
        Ok((
            list,
            if elem == NTy::Int {
                NTy::ListInt
            } else {
                NTy::ListFloat
            },
        ))
    }

    /// `[element each x in iter if cond]`.
    fn comprehension(
        &mut self,
        element: &Expr,
        clauses: &[crate::ast::CompClause],
    ) -> Res<(Value, NTy)> {
        use crate::ast::CompClause;
        let Some((CompClause::For { target, iter }, rest)) = clauses.split_first() else {
            return Err("a comprehension that does not start with `each`".into());
        };
        let BindTarget::Name(name) = target else {
            return Err("a comprehension that unpacks items".into());
        };
        let mut conditions = Vec::new();
        for c in rest {
            match c {
                CompClause::If(e) => conditions.push(e),
                CompClause::For { .. } => return Err("a comprehension with two `each`s".into()),
            }
        }
        let zero = self.b.ins().iconst(types::I64, 0);
        let list = self.new_list(zero);
        let mut elem: Option<NTy> = None;
        self.loop_over(&name.name, iter, &mut |s: &mut Self| {
            for c in &conditions {
                let (v, t) = s.expr(c)?;
                let truth = s.truthy(v, t)?;
                let keep = s.b.create_block();
                let (next, _) = *s.loops.last().expect("inside the loop");
                s.b.ins().brif(truth, keep, &[], next, &[]);
                s.b.switch_to_block(keep);
            }
            let (v, t) = s.expr(element)?;
            if !matches!(t, NTy::Int | NTy::Float) {
                return Err("a comprehension whose items are not numbers".into());
            }
            if elem.is_some_and(|e| e != t) {
                return Err("a comprehension whose items change type".into());
            }
            elem = Some(t);
            s.list_push(list, v, t);
            Ok(())
        })?;
        let elem = elem.ok_or("an empty comprehension")?;
        Ok((
            list,
            if elem == NTy::Int {
                NTy::ListInt
            } else {
                NTy::ListFloat
            },
        ))
    }

    fn expr(&mut self, e: &Expr) -> Res<(Value, NTy)> {
        let saved = self.line;
        if e.span.start.line > 0 && e.span.start.offset < (0xFE << 24) {
            self.line = e.span.start.line;
        }
        let result = self.expr_inner(e);
        self.line = saved;
        result
    }

    fn expr_inner(&mut self, e: &Expr) -> Res<(Value, NTy)> {
        Ok(match &e.kind {
            ExprKind::Int {
                value,
                suffix: None,
            } => {
                let v = i64::try_from(*value).map_err(|_| "an integer literal too large")?;
                (self.b.ins().iconst(types::I64, v), NTy::Int)
            }
            ExprKind::Float {
                value,
                suffix: None,
            } => (self.b.ins().f64const(*value), NTy::Float),
            ExprKind::Bool(v) => (self.b.ins().iconst(types::I8, i64::from(*v)), NTy::Bool),
            ExprKind::Name(n) => {
                let (var, t) = self
                    .lookup(&n.name)
                    .ok_or_else(|| format!("`{}` is not a local number", n.name))?;
                (self.b.use_var(var), t)
            }
            ExprKind::Unary { op, operand } => {
                let (v, t) = self.expr(operand)?;
                match (op, t) {
                    (UnaryOp::Neg, NTy::Int) => {
                        let z = self.b.ins().iconst(types::I64, 0);
                        let (r, of) = self.b.ins().ssub_overflow(z, v);
                        self.fail_if(of, ERR_OVERFLOW, OP_NEG);
                        (r, NTy::Int)
                    }
                    (UnaryOp::Neg, NTy::Float) => (self.b.ins().fneg(v), NTy::Float),
                    (UnaryOp::Pos, NTy::Int | NTy::Float) => (v, t),
                    (UnaryOp::BitNot, NTy::Int) => (self.b.ins().bnot(v), NTy::Int),
                    (UnaryOp::Not, _) => {
                        let truth = self.truthy(v, t)?;
                        (self.b.ins().icmp_imm_s(IntCC::Equal, truth, 0), NTy::Bool)
                    }
                    _ => return Err("this unary operator on this type".into()),
                }
            }
            ExprKind::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                lhs,
                rhs,
            } => {
                let (l, lt) = self.expr(lhs)?;
                if lt != NTy::Bool {
                    return Err("`and`/`or` on values that are not booleans".into());
                }
                let rhs_block = self.b.create_block();
                let done = self.b.create_block();
                self.b.append_block_param(done, types::I8);
                if *op == BinaryOp::And {
                    self.b.ins().brif(l, rhs_block, &[], done, &[l.into()]);
                } else {
                    self.b.ins().brif(l, done, &[l.into()], rhs_block, &[]);
                }
                self.b.switch_to_block(rhs_block);
                let (r, rt) = self.expr(rhs)?;
                if rt != NTy::Bool {
                    return Err("`and`/`or` on values that are not booleans".into());
                }
                self.b.ins().jump(done, &[r.into()]);
                self.b.switch_to_block(done);
                (self.b.block_params(done)[0], NTy::Bool)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let (l, lt) = self.expr(lhs)?;
                let (r, rt) = self.expr(rhs)?;
                // `x ** n` with an integer `n` gives a float when `n` is
                // negative, so only a literal exponent has a known type.
                if *op == BinaryOp::Pow
                    && lt == NTy::Int
                    && rt == NTy::Int
                    && !matches!(rhs.kind, ExprKind::Int { .. })
                {
                    return Err("`int ** int` with an exponent that is not a literal".into());
                }
                self.binary(*op, l, lt, r, rt)?
            }
            ExprKind::Compare { operands, ops } => self.compare(operands, ops)?,
            ExprKind::If {
                then_value,
                condition,
                else_value,
            } => {
                let (c, ct) = self.expr(condition)?;
                let c = self.truthy(c, ct)?;
                let then = self.b.create_block();
                let other = self.b.create_block();
                let done = self.b.create_block();
                self.b.ins().brif(c, then, &[], other, &[]);
                self.b.switch_to_block(then);
                let (a, at) = self.expr(then_value)?;
                self.b.append_block_param(done, cl_type(at));
                self.b.ins().jump(done, &[a.into()]);
                self.b.switch_to_block(other);
                let (b, bt) = self.expr(else_value)?;
                if at != bt {
                    return Err("the two sides of `a if c else b` have different types".into());
                }
                self.b.ins().jump(done, &[b.into()]);
                self.b.switch_to_block(done);
                (self.b.block_params(done)[0], at)
            }
            ExprKind::Call { callee, args } => self.call(callee, args)?,
            ExprKind::List(items) => self.list_literal(items)?,
            ExprKind::Str(segments) => self.string(segments)?,
            ExprKind::Field {
                base,
                name,
                optional: false,
            } => {
                let (p, t) = self.expr(base)?;
                let NTy::Struct(id) = t else {
                    return Err("a field of something that is not a struct".into());
                };
                let (i, ft) = self.env.structs.field(id, &name.name).ok_or_else(|| {
                    format!(
                        "`{}` has no field `{}`",
                        self.env.structs.name(id),
                        name.name
                    )
                })?;
                (
                    self.b
                        .ins()
                        .load(cl_type(ft), MemFlagsData::trusted(), p, (i * 8) as i32),
                    ft,
                )
            }
            ExprKind::ListComp { element, clauses } => self.comprehension(element, clauses)?,
            ExprKind::Index { base, subscripts } => {
                let (list, lt, i) = self.list_and_index(base, subscripts)?;
                if lt == NTy::Str {
                    let v = self.helper("vyrn_str_index", &[self.err, list, i]);
                    self.check_err_with_line();
                    return Ok((v, NTy::Str));
                }
                let t = if lt == NTy::ListInt {
                    NTy::Int
                } else {
                    NTy::Float
                };
                (self.list_get(list, i, t), t)
            }
            _ => return Err("this kind of expression".into()),
        })
    }

    fn binary(&mut self, op: BinaryOp, l: Value, lt: NTy, r: Value, rt: NTy) -> Res<(Value, NTy)> {
        use BinaryOp as B;
        match (op, lt, rt) {
            (B::Mul, list, NTy::Int) | (B::Mul, NTy::Int, list) if list.is_list() => {
                let (list_v, n) = if lt.is_list() { (l, r) } else { (r, l) };
                let f = self.func_ref(self.helpers["vyrn_list_repeat"]);
                let call = self.b.ins().call(f, &[list_v, n, self.err]);
                let v = self.b.inst_results(call)[0];
                self.check_err_with_line();
                return Ok((v, list));
            }
            (B::Add, NTy::Str, NTy::Str) => {
                let v = self.helper("vyrn_str_concat", &[l, r, self.err]);
                return Ok((v, NTy::Str));
            }
            (B::Mul, NTy::Str, NTy::Int) | (B::Mul, NTy::Int, NTy::Str) => {
                let (text, n) = if lt == NTy::Str { (l, r) } else { (r, l) };
                let v = self.helper("vyrn_str_repeat", &[text, n, self.err]);
                return Ok((v, NTy::Str));
            }
            (B::Add, a, b) if a.is_list() && a == b => {
                let f = self.func_ref(self.helpers["vyrn_list_concat"]);
                let call = self.b.ins().call(f, &[l, r, self.err]);
                return Ok((self.b.inst_results(call)[0], a));
            }
            _ => {}
        }
        match (lt, rt) {
            (NTy::Int, NTy::Int) => self.int_binary(op, l, r),
            (NTy::Int | NTy::Float, NTy::Int | NTy::Float) => {
                if matches!(op, B::BitAnd | B::BitOr | B::BitXor | B::Shl | B::Shr) {
                    return Err("bit operators on floats".into());
                }
                let x = self.as_float(l, lt);
                let y = self.as_float(r, rt);
                self.float_binary(op, x, y)
            }
            _ => Err(format!("`{op:?}` on {} and {}", ty_name(lt), ty_name(rt))),
        }
    }

    fn int_binary(&mut self, op: BinaryOp, x: Value, y: Value) -> Res<(Value, NTy)> {
        use BinaryOp as B;
        let checked = |s: &mut Self, (v, of): (Value, Value), detail| {
            s.fail_if(of, ERR_OVERFLOW, detail);
            (v, NTy::Int)
        };
        Ok(match op {
            B::Add => {
                let r = self.b.ins().sadd_overflow(x, y);
                checked(self, r, OP_ADD)
            }
            B::Sub => {
                let r = self.b.ins().ssub_overflow(x, y);
                checked(self, r, OP_SUB)
            }
            B::Mul => {
                let r = self.b.ins().smul_overflow(x, y);
                checked(self, r, OP_MUL)
            }
            B::Div => {
                let zero = self.b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                self.fail_if(zero, ERR_DIV_ZERO, 0);
                let fx = self.b.ins().fcvt_from_sint(types::F64, x);
                let fy = self.b.ins().fcvt_from_sint(types::F64, y);
                (self.b.ins().fdiv(fx, fy), NTy::Float)
            }
            B::FloorDiv | B::Rem => {
                let zero = self.b.ins().icmp_imm_s(IntCC::Equal, y, 0);
                self.fail_if(zero, ERR_DIV_ZERO, 0);
                // i64::MIN by -1 overflows.
                let min = self.b.ins().icmp_imm_s(IntCC::Equal, x, i64::MIN);
                let neg1 = self.b.ins().icmp_imm_s(IntCC::Equal, y, -1);
                let both = self.b.ins().band(min, neg1);
                self.fail_if(
                    both,
                    ERR_OVERFLOW,
                    if op == B::FloorDiv {
                        OP_FLOORDIV
                    } else {
                        OP_REM
                    },
                );
                let q = self.b.ins().sdiv(x, y);
                let r = self.b.ins().srem(x, y);
                // Round toward negative infinity: adjust when the remainder
                // is nonzero and its sign differs from the divisor's.
                let r_nonzero = self.b.ins().icmp_imm_s(IntCC::NotEqual, r, 0);
                let xor = self.b.ins().bxor(r, y);
                let signs_differ = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, xor, 0);
                let adjust = self.b.ins().band(r_nonzero, signs_differ);
                if op == B::FloorDiv {
                    let q1 = self.b.ins().iadd_imm_s(q, -1);
                    (self.b.ins().select(adjust, q1, q), NTy::Int)
                } else {
                    let r1 = self.b.ins().iadd(r, y);
                    (self.b.ins().select(adjust, r1, r), NTy::Int)
                }
            }
            B::Pow => {
                // The exponent is a literal (checked by the caller); a
                // negative one fails here as the interpreter's would not, so
                // it is refused there too.
                let f = self.func_ref(self.helpers["vyrn_ipow"]);
                let call = self.b.ins().call(f, &[x, y, self.err]);
                let v = self.b.inst_results(call)[0];
                self.check_err_with_line();
                (v, NTy::Int)
            }
            B::BitAnd => (self.b.ins().band(x, y), NTy::Int),
            B::BitOr => (self.b.ins().bor(x, y), NTy::Int),
            B::BitXor => (self.b.ins().bxor(x, y), NTy::Int),
            B::Shl => {
                let f = self.func_ref(self.helpers["vyrn_ishl"]);
                let call = self.b.ins().call(f, &[x, y, self.err]);
                let v = self.b.inst_results(call)[0];
                self.check_err_with_line();
                (v, NTy::Int)
            }
            B::Shr => {
                let ok = self.b.ins().icmp_imm_s(IntCC::UnsignedLessThan, y, 64);
                let bad = self.b.ins().icmp_imm_s(IntCC::Equal, ok, 0);
                // The message names the amount.
                self.b.ins().store(MemFlagsData::trusted(), y, self.err, 16);
                self.fail_if(bad, ERR_SHIFT, 0);
                (self.b.ins().sshr(x, y), NTy::Int)
            }
            B::And | B::Or => unreachable!("handled with short-circuiting"),
        })
    }

    /// After a helper that may have failed: set the line and return.
    fn check_err_with_line(&mut self) {
        let code = self
            .b
            .ins()
            .load(types::I32, MemFlagsData::trusted(), self.err, 0);
        let bad = self.b.create_block();
        let ok = self.b.create_block();
        self.b.ins().brif(code, bad, &[], ok, &[]);
        self.b.switch_to_block(bad);
        let l = self.b.ins().iconst(types::I32, i64::from(self.line));
        self.b.ins().store(MemFlagsData::trusted(), l, self.err, 8);
        let z = self.zero(self.ret);
        self.b.ins().return_(&[z]);
        self.b.switch_to_block(ok);
    }

    fn float_binary(&mut self, op: BinaryOp, x: Value, y: Value) -> Res<(Value, NTy)> {
        use BinaryOp as B;
        let zero_check = |s: &mut Self| {
            let z = s.b.ins().f64const(0.0);
            let is_zero = s.b.ins().fcmp(FloatCC::Equal, y, z);
            s.fail_if(is_zero, ERR_DIV_ZERO, 0);
        };
        Ok((
            match op {
                B::Add => self.b.ins().fadd(x, y),
                B::Sub => self.b.ins().fsub(x, y),
                B::Mul => self.b.ins().fmul(x, y),
                B::Div => {
                    zero_check(self);
                    self.b.ins().fdiv(x, y)
                }
                B::FloorDiv => {
                    zero_check(self);
                    let q = self.b.ins().fdiv(x, y);
                    self.b.ins().floor(q)
                }
                B::Rem => {
                    zero_check(self);
                    let q = self.b.ins().fdiv(x, y);
                    let f = self.b.ins().floor(q);
                    let m = self.b.ins().fmul(y, f);
                    self.b.ins().fsub(x, m)
                }
                B::Pow => {
                    let f = self.func_ref(self.helpers["vyrn_powf"]);
                    let call = self.b.ins().call(f, &[x, y]);
                    self.b.inst_results(call)[0]
                }
                _ => return Err("this operator on floats".into()),
            },
            NTy::Float,
        ))
    }

    fn compare(&mut self, operands: &[Expr], ops: &[CompareOp]) -> Res<(Value, NTy)> {
        let done = self.b.create_block();
        self.b.append_block_param(done, types::I8);
        let (mut left, mut lt) = self.expr(&operands[0])?;
        for (i, op) in ops.iter().enumerate() {
            let (right, rt) = self.expr(&operands[i + 1])?;
            let result = self.compare_pair(*op, left, lt, right, rt)?;
            if i + 1 == ops.len() {
                self.b.ins().jump(done, &[result.into()]);
            } else {
                let next = self.b.create_block();
                let f = self.b.ins().iconst(types::I8, 0);
                self.b.ins().brif(result, next, &[], done, &[f.into()]);
                self.b.switch_to_block(next);
            }
            left = right;
            lt = rt;
        }
        self.b.switch_to_block(done);
        Ok((self.b.block_params(done)[0], NTy::Bool))
    }

    fn compare_pair(&mut self, op: CompareOp, l: Value, lt: NTy, r: Value, rt: NTy) -> Res<Value> {
        use CompareOp as C;
        if (lt, rt) == (NTy::Str, NTy::Str) {
            return Ok(match op {
                C::In | C::NotIn => {
                    let found = self.helper("vyrn_str_contains", &[r, l]);
                    let cc = if op == C::In {
                        IntCC::NotEqual
                    } else {
                        IntCC::Equal
                    };
                    self.b.ins().icmp_imm_s(cc, found, 0)
                }
                _ => {
                    let order = self.helper("vyrn_str_cmp", &[l, r]);
                    let cc = match op {
                        C::Eq => IntCC::Equal,
                        C::NotEq => IntCC::NotEqual,
                        C::Lt => IntCC::SignedLessThan,
                        C::LtEq => IntCC::SignedLessThanOrEqual,
                        C::Gt => IntCC::SignedGreaterThan,
                        _ => IntCC::SignedGreaterThanOrEqual,
                    };
                    self.b.ins().icmp_imm_s(cc, order, 0)
                }
            });
        }
        match (lt, rt) {
            (NTy::Int, NTy::Int) | (NTy::Bool, NTy::Bool) => {
                if lt == NTy::Bool && !matches!(op, C::Eq | C::NotEq) {
                    return Err("ordering booleans".into());
                }
                let cc = match op {
                    C::Eq => IntCC::Equal,
                    C::NotEq => IntCC::NotEqual,
                    C::Lt => IntCC::SignedLessThan,
                    C::LtEq => IntCC::SignedLessThanOrEqual,
                    C::Gt => IntCC::SignedGreaterThan,
                    C::GtEq => IntCC::SignedGreaterThanOrEqual,
                    _ => return Err("`in` on numbers".into()),
                };
                Ok(self.b.ins().icmp(cc, l, r))
            }
            (NTy::Int | NTy::Float, NTy::Int | NTy::Float) => {
                let x = self.as_float(l, lt);
                let y = self.as_float(r, rt);
                let cc = match op {
                    C::Eq => FloatCC::Equal,
                    C::NotEq => FloatCC::NotEqual,
                    C::Lt => FloatCC::LessThan,
                    C::LtEq => FloatCC::LessThanOrEqual,
                    C::Gt => FloatCC::GreaterThan,
                    C::GtEq => FloatCC::GreaterThanOrEqual,
                    _ => return Err("`in` on numbers".into()),
                };
                Ok(self.b.ins().fcmp(cc, x, y))
            }
            _ => Err(format!("comparing {} with {}", ty_name(lt), ty_name(rt))),
        }
    }

    fn call(&mut self, callee: &Expr, args: &[crate::ast::Arg]) -> Res<(Value, NTy)> {
        // Constructors take fields by name too.
        if let ExprKind::Name(n) = &callee.kind {
            if let Some(&id) = self.env.structs.by_name.get(&n.name) {
                if self.lookup(&n.name).is_none() && !self.sigs.contains_key(&n.name) {
                    if args.iter().any(|a| a.is_mut) {
                        return Err("a constructor with `mut` arguments".into());
                    }
                    return self.construct(id, args);
                }
            }
        }
        if args.iter().any(|a| a.name.is_some() || a.is_mut) {
            return Err("calls with keyword or `mut` arguments".into());
        }
        // `math.sqrt(x)` when `use vyrn.math`.
        let name = match &callee.kind {
            ExprKind::Name(n) => {
                if self.lookup(&n.name).is_some() {
                    return Err("calling a variable".into());
                }
                n.name.clone()
            }
            ExprKind::Field {
                base,
                name,
                optional: false,
            } => match &base.kind {
                ExprKind::Name(m)
                    if self.env.math.contains(&m.name) && self.lookup(&m.name).is_none() =>
                {
                    return self.math_call(&name.name, args);
                }
                _ => return self.method(base, &name.name, args),
            },
            _ => return Err("this kind of call".into()),
        };
        if let Some((params, ret)) = self.sigs.get(&name).cloned() {
            if args.len() != params.len() {
                return Err(format!("`{name}` called with {} arguments", args.len()));
            }
            let mut values = Vec::new();
            for (a, want) in args.iter().zip(&params) {
                let (v, t) = self.expr_as(&a.value, Some(*want))?;
                if t != *want {
                    return Err(format!(
                        "`{name}` receives {} where it declares {}",
                        ty_name(t),
                        ty_name(*want)
                    ));
                }
                values.push(v);
            }
            return Ok(self.call_compiled(&name, values, ret));
        }
        if let Some(&id) = self.env.structs.by_name.get(&name) {
            return self.construct(id, args);
        }
        if self.env.globals.contains(&name) {
            return Err(format!("calls `{name}`, which stays interpreted"));
        }
        self.builtin(&name, args)
    }

    fn args(&mut self, args: &[crate::ast::Arg], n: usize) -> Res<Vec<(Value, NTy)>> {
        if args.len() != n {
            return Err("a builtin with an unexpected number of arguments".into());
        }
        args.iter().map(|a| self.expr(&a.value)).collect()
    }

    fn builtin(&mut self, name: &str, args: &[crate::ast::Arg]) -> Res<(Value, NTy)> {
        Ok(match name {
            // A float list's sum is an int when the list is empty, so only
            // int lists are summed here.
            "sum" => {
                let a = self.args(args, 1)?;
                if a[0].1 != NTy::ListInt {
                    return Err("`sum` of something that is not a list of ints".into());
                }
                return self.list_sum(a[0].0);
            }
            "list" => {
                let a = self.args(args, 1)?;
                if !a[0].1.is_list() {
                    return Err("`list` of something that is not a list of numbers".into());
                }
                let f = self.func_ref(self.helpers["vyrn_list_copy"]);
                let call = self.b.ins().call(f, &[a[0].0, self.err]);
                (self.b.inst_results(call)[0], a[0].1)
            }
            "print" => {
                let mut values = Vec::new();
                for a in args {
                    if a.name.is_some() {
                        return Err("`print` with keyword arguments".into());
                    }
                    values.push(self.expr(&a.value)?);
                }
                let sb = self.helper("vyrn_sb_new", &[self.err]);
                for (i, (v, t)) in values.into_iter().enumerate() {
                    if i > 0 {
                        let space = self.literal(" ");
                        self.helper("vyrn_sb_str", &[sb, space]);
                    }
                    self.append(sb, v, t)?;
                }
                let line = self.helper("vyrn_sb_finish", &[sb, self.err]);
                self.helper("vyrn_print", &[self.err, line]);
                (self.b.ins().iconst(types::I64, 0), NTy::Unit)
            }
            "len" if args.len() == 1 => {
                let (v, t) = self.expr(&args[0].value)?;
                if t == NTy::Str {
                    return Ok((self.helper("vyrn_str_len", &[v]), NTy::Int));
                }
                if !t.is_list() {
                    return Err("`len` of something that is not a list or a string".into());
                }
                (self.list_len(v), NTy::Int)
            }
            "len" => {
                let a = self.args(args, 1)?;
                if !a[0].1.is_list() {
                    return Err("`len` of something that is not a list".into());
                }
                (self.list_len(a[0].0), NTy::Int)
            }
            "abs" => {
                let a = self.args(args, 1)?;
                let (v, t) = a[0];
                match t {
                    NTy::Int => {
                        let min = self.b.ins().icmp_imm_s(IntCC::Equal, v, i64::MIN);
                        self.fail_if(min, ERR_OVERFLOW, OP_ABS);
                        let neg = self.b.ins().ineg(v);
                        let is_neg = self.b.ins().icmp_imm_s(IntCC::SignedLessThan, v, 0);
                        (self.b.ins().select(is_neg, neg, v), NTy::Int)
                    }
                    NTy::Float => (self.b.ins().fabs(v), NTy::Float),
                    _ => return Err("abs of a boolean".into()),
                }
            }
            "float" => {
                let a = self.args(args, 1)?;
                match a[0].1 {
                    NTy::Str => return self.generic_builtin(name, &a),
                    NTy::Int | NTy::Float => (self.as_float(a[0].0, a[0].1), NTy::Float),
                    _ => return Err("float of a boolean".into()),
                }
            }
            "int" => {
                let a = self.args(args, 1)?;
                match a[0].1 {
                    NTy::Int => a[0],
                    NTy::Str => return self.generic_builtin(name, &a),
                    NTy::Float => {
                        let v = a[0].0;
                        // Not finite: the interpreter refuses it.
                        let inf = self.b.ins().f64const(f64::INFINITY);
                        let abs = self.b.ins().fabs(v);
                        let finite = self.b.ins().fcmp(FloatCC::LessThan, abs, inf);
                        let bad = self.b.ins().icmp_imm_s(IntCC::Equal, finite, 0);
                        self.fail_if(bad, ERR_FLOAT_TO_INT, 0);
                        let t = self.b.ins().trunc(v);
                        (self.b.ins().fcvt_to_sint_sat(types::I64, t), NTy::Int)
                    }
                    _ => return Err("int of a boolean".into()),
                }
            }
            "sqrt" => {
                let a = self.args(args, 1)?;
                let x = self.float_arg(a[0])?;
                (self.b.ins().sqrt(x), NTy::Float)
            }
            "exp" | "tanh" => return self.math_call(name, args),
            "floor" | "ceil" => {
                let a = self.args(args, 1)?;
                match a[0].1 {
                    NTy::Int => a[0],
                    NTy::Float => {
                        let r = if name == "floor" {
                            self.b.ins().floor(a[0].0)
                        } else {
                            self.b.ins().ceil(a[0].0)
                        };
                        (self.b.ins().fcvt_to_sint_sat(types::I64, r), NTy::Int)
                    }
                    _ => return Err("floor of a boolean".into()),
                }
            }
            "min" | "max" => {
                let a = self.args(args, 2)?;
                let ((x, xt), (y, yt)) = (a[0], a[1]);
                if xt != yt || xt == NTy::Bool {
                    return Err("`min`/`max` of values with different types".into());
                }
                // Ties keep the first, as the interpreter does.
                let pick_second = match (xt, name) {
                    (NTy::Int, "min") => self.b.ins().icmp(IntCC::SignedLessThan, y, x),
                    (NTy::Int, _) => self.b.ins().icmp(IntCC::SignedGreaterThan, y, x),
                    (_, "min") => self.b.ins().fcmp(FloatCC::LessThan, y, x),
                    _ => self.b.ins().fcmp(FloatCC::GreaterThan, y, x),
                };
                (self.b.ins().select(pick_second, y, x), xt)
            }
            _ => {
                if args.iter().any(|a| a.name.is_some()) {
                    return Err(format!("calls `{name}` with keyword arguments"));
                }
                let values: Vec<(Value, NTy)> = args
                    .iter()
                    .map(|a| self.expr(&a.value))
                    .collect::<Res<_>>()?;
                return self.generic_builtin(name, &values);
            }
        })
    }

    fn float_arg(&mut self, (v, t): (Value, NTy)) -> Res<Value> {
        match t {
            NTy::Int | NTy::Float => Ok(self.as_float(v, t)),
            _ => Err("a math function of a boolean".into()),
        }
    }

    /// `vyrn.math` functions, and `exp`/`tanh`.
    fn math_call(&mut self, name: &str, args: &[crate::ast::Arg]) -> Res<(Value, NTy)> {
        if let Some((_, sym)) = FLOAT1.iter().find(|(n, _)| *n == name) {
            if name == "log" && args.len() != 1 {
                return Err("`log` with a base".into());
            }
            let a = self.args(args, 1)?;
            let x = self.float_arg(a[0])?;
            let f = self.func_ref(self.helpers[sym]);
            let call = self.b.ins().call(f, &[x]);
            return Ok((self.b.inst_results(call)[0], NTy::Float));
        }
        match name {
            "sqrt" | "floor" | "ceil" => self.builtin(name, args),
            "atan2" | "hypot" => {
                let a = self.args(args, 2)?;
                let x = self.float_arg(a[0])?;
                let y = self.float_arg(a[1])?;
                let f = self.func_ref(
                    self.helpers[if name == "atan2" {
                        "vyrn_atan2"
                    } else {
                        "vyrn_hypot"
                    }],
                );
                let call = self.b.ins().call(f, &[x, y]);
                Ok((self.b.inst_results(call)[0], NTy::Float))
            }
            _ => Err(format!("calls `math.{name}`")),
        }
    }
}

fn kind_of(t: NTy) -> Res<i64> {
    Ok(match t {
        NTy::Int => KIND_INT,
        NTy::Float => KIND_FLOAT,
        NTy::Bool => KIND_BOOL,
        NTy::Str => KIND_STR,
        _ => return Err(format!("passing {} to the runtime", ty_name(t))),
    })
}

fn literal_int(e: &Expr) -> Res<i64> {
    match &e.kind {
        ExprKind::Int {
            value,
            suffix: None,
        } => i64::try_from(*value).map_err(|_| "a step too large".into()),
        ExprKind::Unary {
            op: UnaryOp::Neg,
            operand,
        } => literal_int(operand).map(|v| -v),
        _ => Err("a range whose step is not a literal".into()),
    }
}

fn assign_op(op: AssignOp) -> Option<BinaryOp> {
    Some(match op {
        AssignOp::Add => BinaryOp::Add,
        AssignOp::Sub => BinaryOp::Sub,
        AssignOp::Mul => BinaryOp::Mul,
        AssignOp::Div => BinaryOp::Div,
        AssignOp::FloorDiv => BinaryOp::FloorDiv,
        AssignOp::Rem => BinaryOp::Rem,
        AssignOp::Pow => BinaryOp::Pow,
        AssignOp::BitAnd => BinaryOp::BitAnd,
        AssignOp::BitOr => BinaryOp::BitOr,
        AssignOp::BitXor => BinaryOp::BitXor,
        AssignOp::Shl => BinaryOp::Shl,
        AssignOp::Shr => BinaryOp::Shr,
        _ => return None,
    })
}

fn ty_name(t: NTy) -> &'static str {
    match t {
        NTy::Int => "int",
        NTy::Float => "float",
        NTy::Bool => "bool",
        NTy::Unit => "none",
        NTy::ListInt => "[int]",
        NTy::ListFloat => "[float]",
        NTy::Struct(_) => "a struct",
        NTy::Str => "str",
    }
}
