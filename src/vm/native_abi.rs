//! What the VM needs to know about functions compiled to machine code (see
//! `src/native`): how to call them and how to read their failures. This
//! file does not depend on the code generator, so the VM builds the same
//! with or without the `native` feature.

use std::rc::Rc;

/// The type of a native parameter, local or result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NTy {
    Int,
    Float,
    Bool,
    /// No value: a function without `-> T`. Returns `none`.
    Unit,
    /// `[int]` and `[float]`: a list, passed as a pointer to its contents.
    /// Every item is checked to have the type when the call starts.
    ListInt,
    ListFloat,
    /// A struct whose fields are all `int`, `float` or `bool`, by type
    /// index. Passed as its fields, one word each.
    Struct(u16),
    /// `str`: a pointer to the text inside the VM's reference-counted
    /// string.
    Str,
}

impl NTy {
    pub fn is_list(self) -> bool {
        matches!(self, NTy::ListInt | NTy::ListFloat)
    }
}

/// What lists and strings made by native code live in until the VM's call
/// returns. A returned one is handed to the VM with its own reference.
#[derive(Default)]
pub struct Arena {
    pub lists: Vec<Rc<std::cell::RefCell<Vec<super::value::Value>>>>,
    pub strings: Vec<Rc<String>>,
    /// Text being built (interpolation, `print`), before it becomes a
    /// string.
    pub builders: Vec<Box<String>>,
}

/// Filled in by native code when it fails, and read by the VM. Also
/// carries what native code needs from the VM for one call.
#[repr(C)]
#[derive(Debug)]
pub struct ErrState {
    /// 0 when nothing failed; otherwise one of the `ERR_` codes.
    pub code: u32,
    /// For `ERR_OVERFLOW`, which operator (an index into `OPERATORS`).
    pub detail: u32,
    /// The source line that failed.
    pub line: u32,
    /// Native calls in progress, counted from the VM's own depth.
    pub depth: u32,
    /// A value the message names, such as a shift amount or an index.
    pub value: i64,
    /// A second one: the length of the list an index was out of range for.
    pub extra: i64,
    /// Where new lists and strings go (offset 32).
    pub arena: *mut Arena,
    /// The VM's context, for `print` (offset 40).
    pub ctx: *mut std::ffi::c_void,
    /// The fields of a returned struct (offset 48).
    pub ret: [u64; 16],
    /// For `ERR_THROWN`: the error a VM operation raised, as is.
    pub thrown: Option<super::value::Value>,
}

impl Default for ErrState {
    fn default() -> Self {
        ErrState {
            code: 0,
            detail: 0,
            line: 0,
            depth: 0,
            value: 0,
            extra: 0,
            arena: std::ptr::null_mut(),
            ctx: std::ptr::null_mut(),
            ret: [0; 16],
            thrown: None,
        }
    }
}

pub const ERR_OVERFLOW: u32 = 1;
pub const ERR_DIV_ZERO: u32 = 2;
pub const ERR_STACK: u32 = 3;
pub const ERR_FLOAT_TO_INT: u32 = 4;
pub const ERR_SHIFT: u32 = 5;
pub const ERR_NO_RETURN: u32 = 6;
pub const ERR_INDEX: u32 = 7;
pub const ERR_MEMORY: u32 = 8;
/// A VM operation called from native code (a string method, `print`,
/// indexing a string) failed; the error is in `thrown`.
pub const ERR_THROWN: u32 = 9;

/// Operators named in overflow messages, by `ErrState::detail`.
pub const OPERATORS: &[&str] = &["+", "-", "*", "**", "//", "%", "<<", "abs", "neg"];

/// The same limit the interpreter has on nested calls.
pub const MAX_DEPTH: u32 = 10_000;

/// The entry of a compiled function: arguments as 64-bit words (an `int`'s
/// bits, a `float`'s bits, or 0/1 for a `bool`), and the result the same way.
pub type Entry = unsafe extern "C" fn(args: *const u64, err: *mut ErrState) -> u64;

/// A function compiled to machine code.
pub struct NativeFn {
    pub name: String,
    pub entry: Entry,
    pub params: Vec<NTy>,
    pub ret: NTy,
    /// The field types of every struct the native tier handles.
    pub structs: Rc<std::collections::HashMap<u16, Vec<NTy>>>,
    /// Keeps the machine code alive.
    pub keep: Rc<dyn std::any::Any>,
}

impl std::fmt::Debug for NativeFn {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "<native {}({:?}) -> {:?}>",
            self.name, self.params, self.ret
        )
    }
}

impl ErrState {
    /// The error kind and message for a failure.
    pub fn describe(&self, function: &str) -> (&'static str, String) {
        let place = format!(" (in `{function}` at line {})", self.line);
        match self.code {
            ERR_OVERFLOW => (
                "Overflow",
                match OPERATORS.get(self.detail as usize).copied().unwrap_or("?") {
                    "neg" => format!("integer overflow in negation{place}"),
                    op => format!("integer overflow in `{op}`{place}"),
                },
            ),
            ERR_DIV_ZERO => ("DivisionByZero", format!("division by zero{place}")),
            ERR_STACK => (
                "StackOverflow",
                format!("calls are nested more than {MAX_DEPTH} deep; is there unbounded recursion?{place}"),
            ),
            ERR_FLOAT_TO_INT => ("TypeError", format!("cannot convert float to int{place}")),
            ERR_SHIFT => ("ValueError", format!("cannot shift by {}{place}", self.value)),
            ERR_INDEX => (
                "IndexError",
                format!(
                    "index {} is out of range for a list of length {}{place}",
                    self.value, self.extra
                ),
            ),
            ERR_MEMORY => ("MemoryError", format!("the list would be too large{place}")),
            ERR_NO_RETURN => ("TypeError", format!("the function ended without returning a value{place}")),
            _ => ("InternalError", format!("native code failed with code {}{place}", self.code)),
        }
    }
}
