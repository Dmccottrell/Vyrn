//! The bytecode interpreter that powers `vyrn run` and the REPL.
//!
//! - [`op`]: the instruction set
//! - [`compiler`]: syntax tree to bytecode
//! - [`machine`]: the virtual machine
//! - [`tensor`]: tensors and the backends tensor instructions run on
//! - [`value`], [`program`], [`ops`], [`builtins`]: runtime support

pub mod builtins;
pub mod clock;
pub mod compiler;
pub mod dtype;
mod fast;
#[cfg(feature = "ffi")]
pub mod ffi;
pub mod foreign;
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod json;
pub mod kernel;
pub mod lib_chart;
pub mod lib_db;
#[cfg(feature = "draw")]
pub mod lib_draw;
#[cfg(not(feature = "draw"))]
#[path = "lib_draw_off.rs"]
pub mod lib_draw;
pub mod lib_game;
pub mod lib_html;
pub mod lib_mail;
pub mod lib_table;
pub mod lib_template;
pub mod machine;
pub mod native_abi;
pub mod op;
pub mod ops;
pub mod program;
#[cfg(feature = "python")]
pub mod python;
#[cfg(feature = "python")]
pub mod python_tensor;
pub mod stdlib;
pub mod task;
pub mod tensor;
pub mod tokenizer;
pub mod value;
pub mod web;
pub mod weights;

use std::cell::RefCell;
use std::fmt;
use std::io::Write;
use std::rc::Rc;

pub use compiler::compile;
pub use machine::{RuntimeError, TraceLine, Vm};
pub use op::Op;
pub use program::Program;
pub use tensor::{Device, Tensor, TensorBackend};
pub use value::Value;

use crate::diagnostic::Diagnostic;

/// Why running source code failed.
#[derive(Debug)]
pub enum RunError {
    /// The source did not parse or compile.
    Compile(Vec<Diagnostic>),
    /// The program failed while running.
    Runtime(RuntimeError),
}

impl fmt::Display for RunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RunError::Compile(diagnostics) => {
                for d in diagnostics {
                    writeln!(f, "{d}")?;
                }
                Ok(())
            }
            RunError::Runtime(e) => write!(f, "{e}"),
        }
    }
}

/// Parses and compiles source code.
pub fn compile_source(src: &str) -> Result<Program, Vec<Diagnostic>> {
    let module = crate::parser::parse(src)?;
    compile_checked(&module)
}

/// Checks a parsed program (see [`crate::check`]) and compiles it.
pub fn compile_checked(module: &crate::ast::Module) -> Result<Program, Vec<Diagnostic>> {
    let problems = crate::check::check(module);
    if !problems.is_empty() {
        return Err(problems);
    }
    compile(module)
}

/// An output sink that keeps what is printed, for tests and embedding.
#[derive(Clone, Default)]
pub struct Captured(Rc<RefCell<Vec<u8>>>);

impl Captured {
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.0.borrow()).into_owned()
    }
}

impl Write for Captured {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

/// Runs a program's `main:` block and returns everything it printed.
/// Like [`run_source`], with `input()` reading the lines of `input`.
pub fn run_source_with_input(src: &str, input: &str) -> Result<String, RunError> {
    let program = compile_source(src).map_err(RunError::Compile)?;
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.set_input(input);
    vm.run_main(&[]).map_err(RunError::Runtime)?;
    Ok(out.text())
}

pub fn run_source(src: &str) -> Result<String, RunError> {
    let program = compile_source(src).map_err(RunError::Compile)?;
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.run_main(&[]).map_err(RunError::Runtime)?;
    Ok(out.text())
}
