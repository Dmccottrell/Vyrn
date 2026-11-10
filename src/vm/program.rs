//! Compiled programs: function prototypes, types, globals and routes.

use std::collections::HashMap;
use std::fmt::Write;
use std::rc::Rc;

use super::op::Op;
use super::value::{Interner, Symbol, Value};
use crate::ast::{HttpMethod, PathSegment};

/// A compiled function.
#[derive(Debug)]
pub struct Proto {
    pub name: String,
    pub code: Vec<Op>,
    /// Source line of each instruction, for error messages.
    pub lines: Vec<u32>,
    pub consts: Vec<Value>,
    /// Names used by field, method and type instructions.
    pub names: Vec<Symbol>,
    pub params: Vec<Symbol>,
    /// Parameters that must be passed; the rest have defaults.
    pub required: usize,
    /// Registers the frame needs.
    pub num_regs: usize,
    /// How to fill each captured variable when a closure is created.
    pub captures: Vec<Capture>,
    /// `async fn`: calling it starts a task and returns its handle.
    pub is_async: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capture {
    /// A register of the frame creating the closure.
    Local(u8),
    /// A variable that frame itself captured.
    Outer(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TypeKind {
    Struct,
    Class,
    Enum,
    Variant,
}

#[derive(Debug)]
pub struct TypeDef {
    /// For variants, the qualified name: `Shape.Circle`.
    pub name: Symbol,
    pub kind: TypeKind,
    pub fields: Vec<Symbol>,
    /// Whether fields are positional, as in `Rect(f32, f32)`.
    pub positional: bool,
    pub methods: HashMap<Symbol, usize>,
    /// The base class, or a variant's enum.
    pub parent: Option<u16>,
    /// An enum's variants: short name and type index.
    pub variants: Vec<(Symbol, u16)>,
    /// The constructor's prototype, for structs, classes and variants with
    /// fields.
    pub ctor: Option<usize>,
}

#[derive(Debug)]
pub enum GlobalInit {
    Function(usize),
    Type(u16),
    /// Set by the module initializer.
    Const,
}

#[derive(Debug)]
pub struct Global {
    pub name: String,
    pub init: GlobalInit,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ParamSource {
    /// A `{name}` hole in the path.
    Path,
    /// Supplied by the host: a database pool, configuration and so on.
    Provided,
}

#[derive(Debug)]
pub struct RouteParam {
    pub name: String,
    pub source: ParamSource,
    /// The declared type's name, used to convert path text.
    pub ty: Option<String>,
}

#[derive(Debug)]
pub struct RouteDef {
    pub method: HttpMethod,
    pub segments: Vec<PathSegment>,
    pub params: Vec<RouteParam>,
    pub proto: usize,
}

#[derive(Debug)]
pub struct Program {
    pub protos: Vec<Rc<Proto>>,
    pub types: Vec<TypeDef>,
    pub globals: Vec<Global>,
    pub symbols: Interner,
    /// Evaluates the constants. Runs once before anything else.
    pub init: usize,
    pub main: Option<usize>,
    pub routes: Vec<RouteDef>,
}

impl Program {
    pub fn global(&self, name: &str) -> Option<usize> {
        self.globals.iter().position(|g| g.name == name)
    }

    /// A readable listing of every function's bytecode.
    pub fn disassemble(&self) -> String {
        let mut out = String::new();
        for (index, proto) in self.protos.iter().enumerate() {
            let params: Vec<&str> = proto.params.iter().map(|p| self.symbols.name(*p)).collect();
            let _ = writeln!(
                out,
                "fn #{index} {}({}) registers={}",
                proto.name,
                params.join(", "),
                proto.num_regs
            );
            for (i, value) in proto.consts.iter().enumerate() {
                let _ = writeln!(out, "    k{i} = {value:?}");
            }
            for (i, op) in proto.code.iter().enumerate() {
                let _ = writeln!(out, "  {i:>4}  line {:<4} {op:?}", proto.lines[i]);
            }
            out.push('\n');
        }
        out
    }
}
