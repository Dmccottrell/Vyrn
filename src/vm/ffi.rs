//! `extern "c":` blocks: calling C (and C++ through `extern "C"` headers)
//! from Vyrn. Built with `--features ffi`.
//!
//! ```vyrn
//! @link("m")
//! extern "c":
//!     fn cos(x: f64) -> f64
//! end
//! ```
//!
//! The compiler turns each declaration into a signature string such as
//! `f64(f64)`; at start-up this module opens the library, finds the symbol
//! and prepares a libffi call description for it. A call converts each
//! argument to its C type, calls through libffi, and converts the result
//! back. Everything `unsafe` in the interpreter's FFI is in this file.
//!
//! What may cross:
//!
//! | Vyrn | C |
//! |---|---|
//! | `int` | `i8` ... `u64`, range-checked |
//! | `float` | `f32`, `f64` |
//! | `bool` | `_Bool` |
//! | `str` | `const char*`, NUL-terminated, valid for the call |
//! | `tensor` (f32) | `float*` to its data, valid for the call |
//! | a C pointer, `none` | `void*`, `NULL` |
//!
//! Pointers into Vyrn memory last only for the call. The checker requires
//! calls that pass or return pointers to sit in an `unsafe:` block, and a
//! pointer returned by C is an opaque value that Vyrn never dereferences.

use std::any::Any;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_void, CString};
use std::rc::Rc;

use libffi::middle::{Arg, Cif, CodePtr, Type};
use libloading::Library;

use super::dtype::Storage;
use super::foreign::Foreign;
use super::ops::{err, Ctx, VmResult};
use super::value::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CType {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Bool,
    Str,
    Ptr,
    Void,
}

impl CType {
    fn parse(text: &str) -> Option<CType> {
        Some(match text {
            "i8" => CType::I8,
            "i16" => CType::I16,
            "i32" => CType::I32,
            "i64" => CType::I64,
            "u8" => CType::U8,
            "u16" => CType::U16,
            "u32" => CType::U32,
            "u64" => CType::U64,
            "f32" => CType::F32,
            "f64" => CType::F64,
            "bool" => CType::Bool,
            "str" => CType::Str,
            "ptr" => CType::Ptr,
            "void" => CType::Void,
            _ => return None,
        })
    }

    fn ffi(self) -> Type {
        match self {
            CType::I8 => Type::i8(),
            CType::I16 => Type::i16(),
            CType::I32 => Type::i32(),
            CType::I64 => Type::i64(),
            CType::U8 | CType::Bool => Type::u8(),
            CType::U16 => Type::u16(),
            CType::U32 => Type::u32(),
            CType::U64 => Type::u64(),
            CType::F32 => Type::f32(),
            CType::F64 => Type::f64(),
            CType::Str | CType::Ptr => Type::pointer(),
            CType::Void => Type::void(),
        }
    }
}

thread_local! {
    /// Libraries opened so far, by name; each stays loaded for the program.
    static LIBRARIES: RefCell<HashMap<String, Rc<Library>>> = RefCell::new(HashMap::new());
}

/// The file names tried for `@link("m")`: `libm.so`, `libm.so.6`, ...
fn candidates(name: &str) -> Vec<String> {
    if name.contains('/')
        || name.contains('\\')
        || name.contains(".so")
        || name.ends_with(".dylib")
        || name.ends_with(".dll")
    {
        return vec![name.to_string()];
    }
    let mut c = vec![libloading::library_filename(name)
        .to_string_lossy()
        .to_string()];
    for version in ["6", "1", "2", "3"] {
        c.push(format!("lib{name}.so.{version}"));
    }
    c.push(format!("{name}.dll"));
    c
}

fn open(library: &str) -> VmResult<Rc<Library>> {
    if let Some(lib) = LIBRARIES.with(|l| l.borrow().get(library).cloned()) {
        return Ok(lib);
    }
    let lib = if library.is_empty() {
        #[cfg(unix)]
        {
            Library::from(libloading::os::unix::Library::this())
        }
        #[cfg(windows)]
        {
            Library::from(
                libloading::os::windows::Library::this()
                    .map_err(|e| err("FfiError", format!("cannot open the program itself: {e}")))?,
            )
        }
    } else {
        let mut last = String::new();
        let mut found = None;
        for file in candidates(library) {
            // SAFETY: loading a library runs its initializers. The program
            // asked for this library with `@link`, which, like `extern`
            // itself, is an explicit request to run foreign code.
            match unsafe { Library::new(&file) } {
                Ok(l) => {
                    found = Some(l);
                    break;
                }
                Err(e) => last = e.to_string(),
            }
        }
        found.ok_or_else(|| {
            err(
                "FfiError",
                format!("cannot load the C library `{library}`: {last}; is it installed, and on the library path?"),
            )
        })?
    };
    let lib = Rc::new(lib);
    LIBRARIES.with(|l| l.borrow_mut().insert(library.to_string(), lib.clone()));
    Ok(lib)
}

/// A C function ready to call.
pub struct CFunction {
    name: String,
    params: Vec<CType>,
    result: CType,
    cif: Cif,
    code: CodePtr,
    /// Keeps the library loaded while the function exists.
    _library: Rc<Library>,
}

/// `ffi.function(library, name, "f64(f64)")`
pub fn function(library: &str, name: &str, signature: &str) -> VmResult<Value> {
    let bad = || {
        err(
            "FfiError",
            format!("bad C signature `{signature}` for `{name}`"),
        )
    };
    let (result, rest) = signature.split_once('(').ok_or_else(bad)?;
    let params = rest.strip_suffix(')').ok_or_else(bad)?;
    let result = CType::parse(result).ok_or_else(bad)?;
    let params: Vec<CType> = if params.is_empty() {
        Vec::new()
    } else {
        params
            .split(',')
            .map(CType::parse)
            .collect::<Option<_>>()
            .ok_or_else(bad)?
    };
    let lib = open(library)?;
    // SAFETY: the symbol is only used as an address, handed to libffi
    // together with the signature the program declared.
    let symbol = unsafe { lib.get::<*mut c_void>(name.as_bytes()) }.map_err(|e| {
        err(
            "FfiError",
            format!(
                "`{name}` is not in {}: {e}",
                if library.is_empty() {
                    "this program".to_string()
                } else {
                    format!("`{library}`")
                }
            ),
        )
    })?;
    let code = CodePtr(*symbol);
    let cif = Cif::new(params.iter().map(|p| p.ffi()), result.ffi());
    Ok(Value::Foreign(Rc::new(Box::new(CFunction {
        name: name.to_string(),
        params,
        result,
        cif,
        code,
        _library: lib,
    }))))
}

/// One argument in C form, owned for the duration of the call.
enum Slot {
    I8(i8),
    I16(i16),
    I32(i32),
    I64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    F32(f32),
    F64(f64),
    Ptr(*mut c_void),
}

impl Slot {
    fn arg(&self) -> Arg<'_> {
        match self {
            Slot::I8(v) => Arg::new(v),
            Slot::I16(v) => Arg::new(v),
            Slot::I32(v) => Arg::new(v),
            Slot::I64(v) => Arg::new(v),
            Slot::U8(v) => Arg::new(v),
            Slot::U16(v) => Arg::new(v),
            Slot::U32(v) => Arg::new(v),
            Slot::U64(v) => Arg::new(v),
            Slot::F32(v) => Arg::new(v),
            Slot::F64(v) => Arg::new(v),
            Slot::Ptr(v) => Arg::new(v),
        }
    }
}

impl CFunction {
    fn convert(
        &self,
        i: usize,
        ty: CType,
        value: &Value,
        strings: &mut Vec<CString>,
    ) -> VmResult<Slot> {
        let wrong = |want: &str| {
            err(
                "TypeError",
                format!(
                    "argument {} of C function `{}` must be {want}, not {}",
                    i + 1,
                    self.name,
                    value.type_name()
                ),
            )
        };
        let int = |value: &Value| match value {
            Value::Int(n) => Ok(*n),
            Value::Bool(b) => Ok(i64::from(*b)),
            _ => Err(wrong("an int")),
        };
        let range = |n: i64, what: &str| {
            err(
                "OverflowError",
                format!(
                    "{n} does not fit in a C {what} (argument {} of `{}`)",
                    i + 1,
                    self.name
                ),
            )
        };
        Ok(match ty {
            CType::I8 => {
                Slot::I8(int(value).and_then(|n| i8::try_from(n).map_err(|_| range(n, "i8")))?)
            }
            CType::I16 => {
                Slot::I16(int(value).and_then(|n| i16::try_from(n).map_err(|_| range(n, "i16")))?)
            }
            CType::I32 => {
                Slot::I32(int(value).and_then(|n| i32::try_from(n).map_err(|_| range(n, "i32")))?)
            }
            CType::I64 => Slot::I64(int(value)?),
            CType::U8 => {
                Slot::U8(int(value).and_then(|n| u8::try_from(n).map_err(|_| range(n, "u8")))?)
            }
            CType::U16 => {
                Slot::U16(int(value).and_then(|n| u16::try_from(n).map_err(|_| range(n, "u16")))?)
            }
            CType::U32 => {
                Slot::U32(int(value).and_then(|n| u32::try_from(n).map_err(|_| range(n, "u32")))?)
            }
            CType::U64 => {
                Slot::U64(int(value).and_then(|n| u64::try_from(n).map_err(|_| range(n, "u64")))?)
            }
            CType::Bool => match value {
                Value::Bool(b) => Slot::U8(u8::from(*b)),
                _ => return Err(wrong("a bool")),
            },
            CType::F32 => match value {
                Value::Float(f) => Slot::F32(*f as f32),
                Value::Int(n) => Slot::F32(*n as f32),
                _ => return Err(wrong("a float")),
            },
            CType::F64 => match value {
                Value::Float(f) => Slot::F64(*f),
                Value::Int(n) => Slot::F64(*n as f64),
                _ => return Err(wrong("a float")),
            },
            CType::Str => match value {
                Value::Str(s) => {
                    let c = CString::new(s.as_bytes()).map_err(|_| {
                        err(
                            "ValueError",
                            format!(
                                "argument {} of `{}` contains a NUL character",
                                i + 1,
                                self.name
                            ),
                        )
                    })?;
                    let ptr = c.as_ptr() as *mut c_void;
                    strings.push(c);
                    Slot::Ptr(ptr)
                }
                _ => return Err(wrong("a str")),
            },
            CType::Ptr => match value {
                Value::None => Slot::Ptr(std::ptr::null_mut()),
                Value::Tensor(t) => match &t.storage {
                    Storage::F32(data) => Slot::Ptr(data.as_ptr() as *mut c_void),
                    _ => return Err(wrong("an f32 tensor (convert with `x.to(\"f32\")`)")),
                },
                Value::Foreign(f) => match f.as_any().downcast_ref::<CPointer>() {
                    Some(p) => Slot::Ptr(p.0 as *mut c_void),
                    None => return Err(wrong("a pointer")),
                },
                _ => return Err(wrong("a pointer, a tensor or none")),
            },
            CType::Void => return Err(wrong("nothing")),
        })
    }
}

impl Foreign for CFunction {
    fn type_name(&self) -> &'static str {
        "C function"
    }

    fn display(&self) -> String {
        format!("<C function {}>", self.name)
    }

    fn call(&self, _: &mut Ctx, args: &[Value], kwargs: &[(String, Value)]) -> VmResult<Value> {
        if !kwargs.is_empty() {
            return Err(err(
                "CallError",
                format!("C function `{}` takes no keyword arguments", self.name),
            ));
        }
        if args.len() != self.params.len() {
            return Err(err(
                "CallError",
                format!(
                    "C function `{}` takes {} arguments, got {}",
                    self.name,
                    self.params.len(),
                    args.len()
                ),
            ));
        }
        let mut strings = Vec::new();
        let slots: Vec<Slot> = self
            .params
            .iter()
            .zip(args)
            .enumerate()
            .map(|(i, (&ty, v))| self.convert(i, ty, v, &mut strings))
            .collect::<VmResult<_>>()?;
        let ffi_args: Vec<Arg> = slots.iter().map(Slot::arg).collect();
        // SAFETY: `cif` describes the declared signature, every argument was
        // converted to exactly that C type, and the pointers in `slots`
        // (strings, tensor data) stay alive until the call returns. Whether
        // the declaration matches the real C function is the program's
        // promise, made by writing `extern`.
        unsafe {
            Ok(match self.result {
                CType::Void => {
                    self.cif.call::<()>(self.code, &ffi_args);
                    Value::None
                }
                CType::I8 => Value::Int(i64::from(self.cif.call::<i8>(self.code, &ffi_args))),
                CType::I16 => Value::Int(i64::from(self.cif.call::<i16>(self.code, &ffi_args))),
                CType::I32 => Value::Int(i64::from(self.cif.call::<i32>(self.code, &ffi_args))),
                CType::I64 => Value::Int(self.cif.call::<i64>(self.code, &ffi_args)),
                CType::U8 => Value::Int(i64::from(self.cif.call::<u8>(self.code, &ffi_args))),
                CType::U16 => Value::Int(i64::from(self.cif.call::<u16>(self.code, &ffi_args))),
                CType::U32 => Value::Int(i64::from(self.cif.call::<u32>(self.code, &ffi_args))),
                CType::U64 => {
                    let v = self.cif.call::<u64>(self.code, &ffi_args);
                    Value::Int(i64::try_from(v).map_err(|_| {
                        err(
                            "OverflowError",
                            format!("`{}` returned {v}, which does not fit in an int", self.name),
                        )
                    })?)
                }
                CType::Bool => Value::Bool(self.cif.call::<u8>(self.code, &ffi_args) != 0),
                CType::F32 => Value::Float(f64::from(self.cif.call::<f32>(self.code, &ffi_args))),
                CType::F64 => Value::Float(self.cif.call::<f64>(self.code, &ffi_args)),
                CType::Ptr => {
                    let p = self.cif.call::<*mut c_void>(self.code, &ffi_args);
                    if p.is_null() {
                        Value::None
                    } else {
                        Value::Foreign(Rc::new(Box::new(CPointer(p as usize))))
                    }
                }
                CType::Str => Value::None,
            })
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// A pointer returned by C: an opaque handle Vyrn passes back but never
/// reads through.
pub struct CPointer(pub usize);

impl Foreign for CPointer {
    fn type_name(&self) -> &'static str {
        "C pointer"
    }

    fn display(&self) -> String {
        format!("<C pointer {:#x}>", self.0)
    }

    fn equals(&self, other: &Value) -> bool {
        matches!(other, Value::Foreign(f) if f.as_any().downcast_ref::<CPointer>().is_some_and(|p| p.0 == self.0))
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
