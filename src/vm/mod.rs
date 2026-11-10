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
#[cfg(feature = "gpu")]
pub mod gpu;
pub mod json;
pub mod machine;
pub mod op;
pub mod ops;
pub mod program;
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
    compile(&module)
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
pub fn run_source(src: &str) -> Result<String, RunError> {
    let program = compile_source(src).map_err(RunError::Compile)?;
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.run_main(&[]).map_err(RunError::Runtime)?;
    Ok(out.text())
}
