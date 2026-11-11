//! Walks the syntax tree and emits bytecode.
//!
//! Two passes. The first declares every top-level name (functions, types,
//! constants, routes) so that code can refer to anything in the file
//! regardless of order. The second compiles each body into a [`Proto`].
//!
//! Register allocation is a stack discipline: locals take the next free
//! register when they are declared and are released when their block ends;
//! temporaries sit above the locals and are released after each statement.
//! Expressions are compiled straight into the register that needs the
//! result, and a local used as an operand is read in place, so
//! `total = total + x` is a single `Add`.

use std::collections::{HashMap, HashSet};
use std::rc::Rc;

use super::builtins;
use super::op::Op;
use super::program::{
    Capture, Global, GlobalInit, ParamSource, Program, Proto, RouteDef, RouteParam, TypeDef,
    TypeKind,
};
use super::value::{Interner, Symbol, Value};
use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// Compiles a parsed module.
pub fn compile(module: &Module) -> Result<Program, Vec<Diagnostic>> {
    Compiler::new().compile_module(&crate::prelude::with_prelude(module))
}

struct Fail;
type CResult<T> = Result<T, Fail>;

const MAX_REGISTERS: usize = 256;

#[derive(Clone)]
struct Signature {
    params: Vec<String>,
    required: usize,
}

#[derive(Clone)]
enum GlobalKind {
    Function { slot: u16, sig: Signature },
    Struct { slot: u16, sig: Signature },
    Enum { slot: u16 },
    Const { slot: u16 },
}

impl GlobalKind {
    fn slot(&self) -> u16 {
        match self {
            GlobalKind::Function { slot, .. }
            | GlobalKind::Struct { slot, .. }
            | GlobalKind::Enum { slot }
            | GlobalKind::Const { slot } => *slot,
        }
    }
}

enum Resolved {
    Local(u8),
    /// A local held in a shared cell, because a closure captures it.
    Cell(u8),
    Capture(u8),
    /// A captured variable held in a shared cell.
    UpCell(u8),
    Global(u16),
    Native(u16),
    Constant(Value),
}

/// Work to do when control leaves a region early through `return`, `stop`
/// or `skip`.
#[derive(Clone)]
enum Cleanup {
    PopDevice,
    /// Leave a `try` that has no `finally`.
    Handler,
    /// Leave a `try` and run its `finally` block.
    Finally(Rc<Vec<Stmt>>),
    /// Leave an `each mut` loop: drop its handler and put the element being
    /// changed back into the list.
    WriteBack {
        list: u8,
        index: u8,
        item: u8,
    },
}

struct LoopCtx {
    breaks: Vec<usize>,
    continues: Vec<usize>,
    cleanup_depth: usize,
}

struct FnState {
    name: String,
    code: Vec<Op>,
    lines: Vec<u32>,
    consts: Vec<Value>,
    names: Vec<Symbol>,
    params: Vec<Symbol>,
    required: usize,
    /// Names in scope, innermost block last. Hidden registers (loop counters
    /// and the like) are recorded with an empty name.
    scopes: Vec<Vec<(String, u8, bool)>>,
    scope_starts: Vec<usize>,
    next_reg: usize,
    max_reg: usize,
    loops: Vec<LoopCtx>,
    cleanups: Vec<Cleanup>,
    captures: Vec<(String, Capture, bool)>,
    /// Names that closures inside this function refer to. Variables with
    /// these names live in shared cells.
    cells: HashSet<String>,
    /// Parameters with a shared cell: (parameter register, cell register).
    /// The cell's value is copied back before returning, so `mut`
    /// parameters still write back.
    param_cells: Vec<(u8, u8)>,
}

impl FnState {
    fn new(name: String) -> FnState {
        FnState {
            name,
            code: Vec::new(),
            lines: Vec::new(),
            consts: Vec::new(),
            names: Vec::new(),
            params: Vec::new(),
            required: 0,
            scopes: vec![Vec::new()],
            scope_starts: vec![0],
            next_reg: 0,
            max_reg: 0,
            loops: Vec::new(),
            cleanups: Vec::new(),
            captures: Vec::new(),
            cells: HashSet::new(),
            param_cells: Vec::new(),
        }
    }
}

/// What a declaration still needs compiled in the second pass.
enum Plan<'a> {
    Function {
        slot: usize,
        decl: &'a FnDecl,
    },
    Main {
        slot: usize,
        decl: &'a MainDecl,
    },
    Method {
        slot: usize,
        ty: u16,
        decl: &'a FnDecl,
    },
    Ctor {
        slot: usize,
        ty: u16,
    },
    Route {
        slot: usize,
        decl: &'a RouteDecl,
    },
}

/// A call argument after keyword arguments are matched to parameters.
#[derive(Clone, Copy)]
struct CallArg<'a> {
    name: Option<&'a str>,
    is_mut: bool,
    expr: &'a Expr,
}

enum IterKind {
    Counted { base: u8 },
    Generic,
}

struct IterLoop {
    kind: IterKind,
    top: usize,
    exit: usize,
}

enum CompBody<'a> {
    List {
        list: u8,
        element: &'a Expr,
    },
    Map {
        map: u8,
        key: &'a Expr,
        value: &'a Expr,
    },
}

struct Compiler {
    protos: Vec<Option<Rc<Proto>>>,
    types: Vec<TypeDef>,
    type_index: HashMap<String, u16>,
    globals: Vec<Global>,
    global_index: HashMap<String, GlobalKind>,
    /// Standard library modules in scope: the name used in the program,
    /// then the prefix of the module's builtins (`web` -> `web`).
    modules: HashMap<String, &'static str>,
    /// Builtins imported by name: `use std.web (serve)`.
    imports: HashMap<String, u16>,
    /// Globals filled by a builtin call before anything else runs: Python
    /// modules and C functions. Global slot, builtin, and its arguments.
    foreign_inits: Vec<(u16, u16, Vec<Value>)>,
    /// Constants with a literal number value, which kernels may read.
    numeric_consts: HashMap<String, f32>,
    symbols: Interner,
    routes: Vec<RouteDef>,
    main: Option<usize>,
    fns: Vec<FnState>,
    diagnostics: Vec<Diagnostic>,
    line: u32,
    span: Span,
}

fn collect_items<'a>(
    items: &'a [Item],
    prefix: &[PathSegment],
    out: &mut Vec<(&'a Item, Vec<PathSegment>)>,
) {
    for item in items {
        if let ItemKind::RouteGroup(group) = &item.kind {
            let mut nested = prefix.to_vec();
            nested.extend(group.prefix.segments.iter().cloned());
            collect_items(&group.items, &nested, out);
        } else {
            out.push((item, prefix.to_vec()));
        }
    }
}

/// Joins a group prefix and a route path, merging adjacent literal text.
fn join_segments(prefix: &[PathSegment], path: &[PathSegment]) -> Vec<PathSegment> {
    let mut out: Vec<PathSegment> = Vec::new();
    for segment in prefix.iter().chain(path) {
        match (out.last_mut(), segment) {
            (Some(PathSegment::Literal(text)), PathSegment::Literal(more)) => {
                if text.ends_with('/') && more.starts_with('/') {
                    text.push_str(&more[1..]);
                } else {
                    text.push_str(more);
                }
            }
            _ => out.push(segment.clone()),
        }
    }
    out
}

fn path_name(path: &Path) -> String {
    path.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

fn type_name(ty: &TypeExpr) -> Option<String> {
    match &ty.kind {
        TypeExprKind::Named { path, .. } => Some(path_name(path)),
        TypeExprKind::Placed { inner, .. } | TypeExprKind::Optional(inner) => type_name(inner),
        TypeExprKind::Any => Some("any".into()),
        _ => None,
    }
}

fn required_count(defaults: &[bool]) -> usize {
    defaults
        .iter()
        .rposition(|has_default| !has_default)
        .map(|i| i + 1)
        .unwrap_or(0)
}

/// Whether compiling `expr` into a register writes that register before it
/// has finished reading its operands. Such expressions are compiled into a
/// temporary first when the destination is a named variable.
fn writes_early(expr: &Expr) -> bool {
    match &expr.kind {
        ExprKind::Map(_) | ExprKind::ListComp { .. } | ExprKind::MapComp { .. } => true,
        ExprKind::Binary { op, .. } => matches!(op, BinaryOp::And | BinaryOp::Or),
        ExprKind::Coalesce { .. } => true,
        ExprKind::Compare { ops, .. } => ops.len() > 1,
        _ => false,
    }
}

fn pattern_binds(pattern: &Pattern) -> bool {
    match &pattern.kind {
        PatternKind::Binding(_) | PatternKind::As { .. } => true,
        PatternKind::Destructure { args, .. } => args.iter().any(|a| pattern_binds(&a.pattern)),
        PatternKind::Tuple(items) | PatternKind::List(items) => items.iter().any(|i| match i {
            SeqPattern::Item(p) => pattern_binds(p),
            SeqPattern::Rest(name, _) => name.is_some(),
        }),
        PatternKind::Or(alternatives) => alternatives.iter().any(pattern_binds),
        _ => false,
    }
}

fn edit_distance(a: &str, b: &str) -> usize {
    let b: Vec<char> = b.chars().collect();
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, ca) in a.chars().enumerate() {
        let mut current = vec![i + 1];
        for (j, cb) in b.iter().enumerate() {
            let cost = usize::from(ca != *cb);
            current.push(
                (previous[j] + cost)
                    .min(previous[j + 1] + 1)
                    .min(current[j] + 1),
            );
        }
        previous = current;
    }
    previous[b.len()]
}

impl Compiler {
    fn new() -> Compiler {
        Compiler {
            protos: Vec::new(),
            types: Vec::new(),
            type_index: HashMap::new(),
            globals: Vec::new(),
            global_index: HashMap::new(),
            modules: HashMap::new(),
            imports: HashMap::new(),
            foreign_inits: Vec::new(),
            numeric_consts: HashMap::new(),
            symbols: Interner::default(),
            routes: Vec::new(),
            main: None,
            fns: Vec::new(),
            diagnostics: Vec::new(),
            line: 0,
            span: Span::default(),
        }
    }

    // ==== module ===========================================================

    fn compile_module(mut self, module: &Module) -> Result<Program, Vec<Diagnostic>> {
        let mut items = Vec::new();
        collect_items(&module.items, &[], &mut items);

        // Pass 1: declare every top-level name.
        let mut plans = Vec::new();
        let mut own_fields: HashMap<u16, Vec<(String, Option<&Expr>)>> = HashMap::new();
        let mut bases: Vec<(u16, &TypeExpr, Span)> = Vec::new();
        let mut impls: Vec<(&ImplDecl, Span)> = Vec::new();
        let mut consts: Vec<&ConstDecl> = Vec::new();
        for (item, prefix) in &items {
            self.set_pos(item.span);
            self.declare(
                item,
                prefix,
                &mut plans,
                &mut own_fields,
                &mut bases,
                &mut impls,
                &mut consts,
            );
        }
        self.resolve_bases(&bases, &mut own_fields);
        for (decl, span) in impls {
            self.set_pos(span);
            let Some(name) = type_name(&decl.target) else {
                self.error("methods can only be added to a named type", span);
                continue;
            };
            let Some(&ty) = self.type_index.get(&name) else {
                self.error(
                    format!(
                        "cannot add methods to `{name}`: no struct, class or enum has that name"
                    ),
                    span,
                );
                continue;
            };
            for method in &decl.methods {
                if let ItemKind::Fn(f) = &method.kind {
                    let slot = self.reserve_proto();
                    let symbol = self.symbols.intern(&f.name.name);
                    self.types[ty as usize].methods.insert(symbol, slot);
                    plans.push(Plan::Method { slot, ty, decl: f });
                }
            }
        }
        self.finish_struct_signatures(&own_fields);

        // Pass 2: compile.
        let init = self.reserve_proto();
        self.begin_fn("<module>".into(), &[], HashSet::new());
        for (slot, native, args) in std::mem::take(&mut self.foreign_inits) {
            if let Ok(regs) = self.alloc_n(args.len() + 1) {
                let _ = self.load_const(regs[0], Value::Native(native));
                let argc = args.len() as u8;
                for (value, &r) in args.into_iter().zip(&regs[1..]) {
                    let _ = self.load_const(r, value);
                }
                self.emit(Op::Call {
                    func: regs[0],
                    argc,
                });
                self.emit(Op::SetGlobal {
                    src: regs[0],
                    idx: slot,
                });
                self.release(0);
            }
        }
        for c in consts {
            self.set_pos(c.value.span);
            if let Some(GlobalKind::Const { slot }) = self.global_index.get(&c.name.name).cloned() {
                if let Ok(r) = self.alloc() {
                    if self.expr_to(&c.value, r).is_ok() {
                        if let Some(ty) = &c.declared_ty {
                            let _ = self.apply_placement(Some(ty), r);
                        }
                        self.emit(Op::SetGlobal { src: r, idx: slot });
                    }
                    self.release(0);
                }
            }
        }
        let proto = self.end_fn();
        self.set_proto(init, proto);

        for plan in plans {
            self.compile_plan(plan, &own_fields);
        }

        if !self.diagnostics.is_empty() {
            self.diagnostics.sort_by_key(|d| d.span.start);
            return Err(self.diagnostics);
        }
        let protos = self
            .protos
            .into_iter()
            .map(|p| {
                p.unwrap_or_else(|| {
                    Rc::new(Proto {
                        name: "<missing>".into(),
                        code: vec![Op::ReturnNone],
                        lines: vec![0],
                        file: 0,
                        consts: Vec::new(),
                        names: Vec::new(),
                        params: Vec::new(),
                        required: 0,
                        num_regs: 1,
                        captures: Vec::new(),
                        is_async: false,
                        kernel: None,
                    })
                })
            })
            .collect();
        Ok(Program {
            protos,
            types: self.types,
            globals: self.globals,
            symbols: self.symbols,
            init,
            main: self.main,
            routes: self.routes,
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn declare<'a>(
        &mut self,
        item: &'a Item,
        prefix: &[PathSegment],
        plans: &mut Vec<Plan<'a>>,
        own_fields: &mut HashMap<u16, Vec<(String, Option<&'a Expr>)>>,
        bases: &mut Vec<(u16, &'a TypeExpr, Span)>,
        impls: &mut Vec<(&'a ImplDecl, Span)>,
        consts: &mut Vec<&'a ConstDecl>,
    ) {
        match &item.kind {
            ItemKind::Use(decl) => self.use_decl(decl),
            ItemKind::Extern(e) => self.extern_decl(e, item),
            ItemKind::Trait(_) | ItemKind::TypeAlias(_) => {}
            ItemKind::RouteGroup(_) => {}
            ItemKind::Main(decl) => {
                let slot = self.reserve_proto();
                self.main = Some(slot);
                plans.push(Plan::Main { slot, decl });
            }
            ItemKind::Const(c) => {
                let number = match &c.value.kind {
                    ExprKind::Int { value, .. } => Some(*value as f32),
                    ExprKind::Float { value, .. } => Some(*value as f32),
                    ExprKind::Unary {
                        op: UnaryOp::Neg,
                        operand,
                    } => match &operand.kind {
                        ExprKind::Int { value, .. } => Some(-(*value as f32)),
                        ExprKind::Float { value, .. } => Some(-(*value as f32)),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(n) = number {
                    self.numeric_consts.insert(c.name.name.clone(), n);
                }
                if let Some(slot) = self.add_global(&c.name.name, GlobalInit::Const, item.span) {
                    self.global_index
                        .insert(c.name.name.clone(), GlobalKind::Const { slot });
                    consts.push(c);
                }
            }
            ItemKind::Fn(f) => {
                let proto = self.reserve_proto();
                if let Some(slot) =
                    self.add_global(&f.name.name, GlobalInit::Function(proto), item.span)
                {
                    let sig = Signature {
                        params: f.params.iter().map(|p| p.name.name.clone()).collect(),
                        required: required_count(
                            &f.params
                                .iter()
                                .map(|p| p.default.is_some())
                                .collect::<Vec<_>>(),
                        ),
                    };
                    self.global_index
                        .insert(f.name.name.clone(), GlobalKind::Function { slot, sig });
                    plans.push(Plan::Function {
                        slot: proto,
                        decl: f,
                    });
                }
            }
            ItemKind::Aggregate(a) => {
                let kind = match a.kind {
                    AggregateKind::Struct => TypeKind::Struct,
                    AggregateKind::Class => TypeKind::Class,
                };
                let Some(ty) = self.add_type(&a.name.name, kind, item.span) else {
                    return;
                };
                let ctor = self.reserve_proto();
                self.types[ty as usize].ctor = Some(ctor);
                own_fields.insert(
                    ty,
                    a.fields
                        .iter()
                        .map(|f| (f.name.name.clone(), f.default.as_ref()))
                        .collect(),
                );
                if let Some(base) = &a.base {
                    bases.push((ty, base, item.span));
                }
                if let Some(slot) =
                    self.add_global(&a.name.name, GlobalInit::Function(ctor), item.span)
                {
                    self.global_index.insert(
                        a.name.name.clone(),
                        GlobalKind::Struct {
                            slot,
                            sig: Signature {
                                params: Vec::new(),
                                required: 0,
                            },
                        },
                    );
                }
                plans.push(Plan::Ctor { slot: ctor, ty });
                for method in &a.methods {
                    if let ItemKind::Fn(f) = &method.kind {
                        let slot = self.reserve_proto();
                        let symbol = self.symbols.intern(&f.name.name);
                        self.types[ty as usize].methods.insert(symbol, slot);
                        plans.push(Plan::Method { slot, ty, decl: f });
                    }
                }
            }
            ItemKind::Enum(e) => {
                let Some(ty) = self.add_type(&e.name.name, TypeKind::Enum, item.span) else {
                    return;
                };
                for v in &e.variants {
                    let qualified = format!("{}.{}", e.name.name, v.name.name);
                    let Some(vty) = self.add_type(&qualified, TypeKind::Variant, v.span) else {
                        continue;
                    };
                    let positional = v.fields.iter().all(|f| f.name.is_none());
                    let fields: Vec<String> = v
                        .fields
                        .iter()
                        .enumerate()
                        .map(|(i, f)| {
                            f.name
                                .as_ref()
                                .map(|n| n.name.clone())
                                .unwrap_or(format!("_{i}"))
                        })
                        .collect();
                    {
                        let symbols: Vec<Symbol> =
                            fields.iter().map(|f| self.symbols.intern(f)).collect();
                        let def = &mut self.types[vty as usize];
                        def.fields = symbols;
                        def.positional = positional;
                        def.parent = Some(ty);
                    }
                    let short = self.symbols.intern(&v.name.name);
                    self.types[ty as usize].variants.push((short, vty));
                    if !v.fields.is_empty() {
                        let ctor = self.reserve_proto();
                        self.types[vty as usize].ctor = Some(ctor);
                        own_fields.insert(vty, fields.into_iter().map(|f| (f, None)).collect());
                        plans.push(Plan::Ctor {
                            slot: ctor,
                            ty: vty,
                        });
                    }
                }
                for method in &e.methods {
                    if let ItemKind::Fn(f) = &method.kind {
                        let slot = self.reserve_proto();
                        let symbol = self.symbols.intern(&f.name.name);
                        self.types[ty as usize].methods.insert(symbol, slot);
                        plans.push(Plan::Method { slot, ty, decl: f });
                    }
                }
                if let Some(slot) = self.add_global(&e.name.name, GlobalInit::Type(ty), item.span) {
                    self.global_index
                        .insert(e.name.name.clone(), GlobalKind::Enum { slot });
                }
            }
            ItemKind::Impl(decl) => impls.push((decl, item.span)),
            ItemKind::Route(r) => {
                let slot = self.reserve_proto();
                let holes: Vec<&str> = r.path.params().map(|p| p.name.as_str()).collect();
                let params = r
                    .params
                    .iter()
                    .map(|p| RouteParam {
                        name: p.name.name.clone(),
                        source: if holes.contains(&p.name.name.as_str()) {
                            ParamSource::Path
                        } else {
                            ParamSource::Provided
                        },
                        ty: p.declared_ty.as_ref().and_then(type_name),
                    })
                    .collect();
                self.routes.push(RouteDef {
                    method: r.method,
                    segments: join_segments(prefix, &r.path.segments),
                    params,
                    proto: slot,
                });
                plans.push(Plan::Route { slot, decl: r });
            }
        }
    }

    /// Puts each class's inherited fields before its own.
    fn resolve_bases(
        &mut self,
        bases: &[(u16, &TypeExpr, Span)],
        own_fields: &mut HashMap<u16, Vec<(String, Option<&Expr>)>>,
    ) {
        for &(ty, base, span) in bases {
            let Some(name) = type_name(base) else {
                self.error("a base class must be named", span);
                continue;
            };
            match self.type_index.get(&name).copied() {
                Some(parent) if self.types[parent as usize].kind == TypeKind::Class => {
                    self.types[ty as usize].parent = Some(parent);
                }
                Some(_) => {
                    self.error(
                        format!("`{name}` is not a class; a class can only extend a class"),
                        span,
                    );
                }
                None => {
                    self.error(format!("unknown base class `{name}`"), span);
                }
            }
        }
        // Check for cycles, then compute full field lists.
        let class_types: Vec<u16> = own_fields.keys().copied().collect();
        let mut resolved: HashMap<u16, Vec<(String, Option<&Expr>)>> = HashMap::new();
        for ty in class_types {
            let mut chain = vec![ty];
            let mut current = self.types[ty as usize].parent;
            let mut cyclic = false;
            while let Some(p) = current {
                if chain.contains(&p) {
                    cyclic = true;
                    break;
                }
                chain.push(p);
                current = self.types[p as usize].parent;
            }
            if cyclic {
                let name = self.symbols.name(self.types[ty as usize].name).to_string();
                self.error(format!("object `{name}` inherits from itself"), self.span);
                self.types[ty as usize].parent = None;
                chain.truncate(1);
            }
            let mut fields = Vec::new();
            for t in chain.iter().rev() {
                if let Some(own) = own_fields.get(t) {
                    fields.extend(own.iter().cloned());
                }
            }
            resolved.insert(ty, fields);
        }
        for (ty, fields) in resolved {
            let symbols: Vec<Symbol> = fields.iter().map(|(f, _)| self.symbols.intern(f)).collect();
            self.types[ty as usize].fields = symbols;
            own_fields.insert(ty, fields);
        }
    }

    fn finish_struct_signatures(
        &mut self,
        all_fields: &HashMap<u16, Vec<(String, Option<&Expr>)>>,
    ) {
        let names: Vec<(String, u16)> = self
            .type_index
            .iter()
            .map(|(name, &ty)| (name.clone(), ty))
            .collect();
        for (name, ty) in names {
            let Some(fields) = all_fields.get(&ty) else {
                continue;
            };
            if let Some(GlobalKind::Struct { sig, .. }) = self.global_index.get_mut(&name) {
                sig.params = fields.iter().map(|(f, _)| f.clone()).collect();
                sig.required =
                    required_count(&fields.iter().map(|(_, d)| d.is_some()).collect::<Vec<_>>());
            }
        }
    }

    fn compile_plan(
        &mut self,
        plan: Plan,
        all_fields: &HashMap<u16, Vec<(String, Option<&Expr>)>>,
    ) {
        match plan {
            Plan::Function { slot, decl } => {
                self.set_pos(decl.name.span);
                let mut proto = self.function(decl.name.name.clone(), &decl.params, &decl.body);
                proto.is_async = decl.modifiers.is_async;
                if decl.modifiers.is_kernel && super::kernel::is_elementwise(decl) {
                    proto.kernel = Some(Rc::new(super::kernel::lower(decl, &self.numeric_consts)));
                }
                self.set_proto(slot, proto);
            }
            Plan::Main { slot, decl } => {
                self.set_pos(decl.body.span);
                let params: Vec<(String, Option<&Expr>)> = decl
                    .params
                    .iter()
                    .map(|p| (p.name.name.clone(), p.default.as_ref()))
                    .collect();
                let cells = nested_names(&decl.body.stmts, &params);
                self.begin_fn("main".into(), &params, cells);
                self.stmts(&decl.body.stmts);
                let proto = self.end_fn();
                self.set_proto(slot, proto);
            }
            Plan::Method { slot, ty, decl } => {
                self.set_pos(decl.name.span);
                if decl.params.first().map(|p| p.name.name.as_str()) != Some("self") {
                    self.error(
                        format!(
                            "method `{}` must take `self` as its first parameter",
                            decl.name.name
                        ),
                        decl.name.span,
                    );
                }
                let type_name = self.symbols.name(self.types[ty as usize].name).to_string();
                let mut proto = self.function(
                    format!("{type_name}.{}", decl.name.name),
                    &decl.params,
                    &decl.body,
                );
                proto.is_async = decl.modifiers.is_async;
                self.set_proto(slot, proto);
            }
            Plan::Ctor { slot, ty } => {
                let fields = all_fields.get(&ty).cloned().unwrap_or_default();
                let name = self.symbols.name(self.types[ty as usize].name).to_string();
                self.begin_fn(name, &fields, HashSet::new());
                if let Ok(r) = self.alloc() {
                    self.emit(Op::MakeInstance { dst: r, ty });
                    self.emit(Op::Return { src: r });
                }
                let proto = self.end_fn();
                self.set_proto(slot, proto);
            }
            Plan::Route { slot, decl } => {
                self.set_pos(decl.method_span);
                let params: Vec<(String, Option<&Expr>)> = decl
                    .params
                    .iter()
                    .map(|p| (p.name.name.clone(), p.default.as_ref()))
                    .collect();
                let cells = nested_names(&decl.body.stmts, &params);
                self.begin_fn(
                    format!("{} {}", decl.method.as_str(), decl.path.template()),
                    &params,
                    cells,
                );
                self.stmts(&decl.body.stmts);
                let proto = self.end_fn();
                self.set_proto(slot, proto);
            }
        }
    }

    fn function(&mut self, name: String, params: &[Param], body: &FnBody) -> Proto {
        let list: Vec<(String, Option<&Expr>)> = params
            .iter()
            .map(|p| (p.name.name.clone(), p.default.as_ref()))
            .collect();
        let cells = match body {
            FnBody::Block(block) => nested_names(&block.stmts, &list),
            FnBody::Signature => HashSet::new(),
        };
        self.begin_fn(name, &list, cells);
        if let FnBody::Block(block) = body {
            self.stmts(&block.stmts);
        }
        self.end_fn()
    }

    // ==== tables ===========================================================

    fn reserve_proto(&mut self) -> usize {
        self.protos.push(None);
        self.protos.len() - 1
    }

    fn set_proto(&mut self, slot: usize, proto: Proto) {
        self.protos[slot] = Some(Rc::new(proto));
    }

    fn add_proto(&mut self, proto: Proto) -> CResult<u16> {
        self.protos.push(Some(Rc::new(proto)));
        u16::try_from(self.protos.len() - 1).map_err(|_| {
            self.error(
                "the program has too many functions for the interpreter",
                self.span,
            )
        })
    }

    fn add_global(&mut self, name: &str, init: GlobalInit, span: Span) -> Option<u16> {
        if self.global_index.contains_key(name) {
            self.error(format!("`{name}` is already defined in this file"), span);
            return None;
        }
        let Ok(slot) = u16::try_from(self.globals.len()) else {
            self.error("too many top-level names for the interpreter", span);
            return None;
        };
        self.globals.push(Global {
            name: name.to_string(),
            init,
        });
        Some(slot)
    }

    fn add_type(&mut self, name: &str, kind: TypeKind, span: Span) -> Option<u16> {
        if self.type_index.contains_key(name) {
            self.error(format!("the type `{name}` is already defined"), span);
            return None;
        }
        let Ok(ty) = u16::try_from(self.types.len()) else {
            self.error("too many types for the interpreter", span);
            return None;
        };
        let symbol = self.symbols.intern(name);
        self.types.push(TypeDef {
            name: symbol,
            kind,
            fields: Vec::new(),
            positional: false,
            methods: HashMap::new(),
            parent: None,
            variants: Vec::new(),
            ctor: None,
        });
        self.type_index.insert(name.to_string(), ty);
        Some(ty)
    }

    // ==== diagnostics and positions ========================================

    fn error(&mut self, message: impl Into<String>, span: Span) -> Fail {
        self.diagnostics.push(Diagnostic::new(message, span));
        Fail
    }

    fn error_hint(
        &mut self,
        message: impl Into<String>,
        span: Span,
        hint: impl Into<String>,
    ) -> Fail {
        self.diagnostics
            .push(Diagnostic::new(message, span).with_hint(hint));
        Fail
    }

    fn set_pos(&mut self, span: Span) {
        self.span = span;
        self.line = span.start.line;
    }

    // ==== function state ===================================================

    fn f(&mut self) -> &mut FnState {
        let last = self.fns.len() - 1;
        &mut self.fns[last]
    }

    fn begin_fn(
        &mut self,
        name: String,
        params: &[(String, Option<&Expr>)],
        cells: HashSet<String>,
    ) {
        let mut state = FnState::new(name);
        state.cells = cells;
        state.params = params.iter().map(|(n, _)| self.symbols.intern(n)).collect();
        state.required =
            required_count(&params.iter().map(|(_, d)| d.is_some()).collect::<Vec<_>>());
        self.fns.push(state);
        if params.len() > MAX_REGISTERS - 2 {
            self.error("too many parameters", self.span);
            return;
        }
        for (i, (name, _)) in params.iter().enumerate() {
            if params[..i].iter().any(|(n, _)| n == name) {
                self.error(format!("the parameter `{name}` appears twice"), self.span);
            }
            if let Ok(r) = self.alloc() {
                self.bind_plain(name, r);
            }
        }
        for (i, (_, default)) in params.iter().enumerate() {
            if let Some(default) = default {
                let skip = self.emit(Op::JumpIfNotMissing {
                    src: i as u8,
                    offset: 0,
                });
                let mark = self.mark();
                let _ = self.expr_to(default, i as u8);
                self.release(mark);
                self.patch_here(skip);
            }
        }
        // A parameter that closures capture gets a cell of its own.
        for (i, (name, _)) in params.iter().enumerate() {
            if self.f().cells.contains(name) {
                if let Ok(cell) = self.alloc() {
                    self.emit(Op::Move {
                        dst: cell,
                        src: i as u8,
                    });
                    self.emit(Op::MakeCell { reg: cell });
                    self.bind_entry(name, cell, true);
                    self.f().param_cells.push((i as u8, cell));
                }
            }
        }
    }

    /// Copies parameter cells back into the parameters before a return.
    fn sync_param_cells(&mut self) {
        let pairs = self.f().param_cells.clone();
        for (param, cell) in pairs {
            self.emit(Op::GetCell { dst: param, cell });
        }
    }

    fn end_fn(&mut self) -> Proto {
        self.sync_param_cells();
        self.emit(Op::ReturnNone);
        let f = self.fns.pop().expect("a function is being compiled");
        Proto {
            name: f.name,
            code: f.code,
            lines: f.lines,
            file: self.span.start.offset >> 24,
            consts: f.consts,
            names: f.names,
            params: f.params,
            required: f.required,
            num_regs: f.max_reg.max(1),
            captures: f.captures.into_iter().map(|(_, c, _)| c).collect(),
            is_async: false,
            kernel: None,
        }
    }

    fn emit(&mut self, op: Op) -> usize {
        let line = self.line;
        let f = self.f();
        f.code.push(op);
        f.lines.push(line);
        f.code.len() - 1
    }

    fn here(&mut self) -> usize {
        self.f().code.len()
    }

    /// Points the jump at `at` to `target`.
    fn patch_to(&mut self, at: usize, target: usize) {
        let delta = target as isize - (at as isize + 1);
        let Ok(offset) = i16::try_from(delta) else {
            self.error(
                "this function is too long for the interpreter's jumps; split it into smaller functions",
                self.span,
            );
            return;
        };
        match &mut self.f().code[at] {
            Op::Jump { offset: o }
            | Op::JumpIfFalse { offset: o, .. }
            | Op::JumpIfTrue { offset: o, .. }
            | Op::JumpIfNotNone { offset: o, .. }
            | Op::JumpIfNotMissing { offset: o, .. }
            | Op::ForPrep { offset: o, .. }
            | Op::ForLoop { offset: o, .. }
            | Op::TryBegin { offset: o, .. } => *o = offset,
            _ => {}
        }
    }

    fn patch_here(&mut self, at: usize) {
        let target = self.here();
        self.patch_to(at, target);
    }

    fn jump_to(&mut self, target: usize) {
        let at = self.emit(Op::Jump { offset: 0 });
        self.patch_to(at, target);
    }

    fn constant(&mut self, value: Value) -> CResult<u16> {
        let existing = self.f().consts.iter().position(|c| match (c, &value) {
            (Value::Int(a), Value::Int(b)) => a == b,
            (Value::Float(a), Value::Float(b)) => a.to_bits() == b.to_bits(),
            (Value::Str(a), Value::Str(b)) => a == b,
            (Value::Native(a), Value::Native(b)) => a == b,
            _ => false,
        });
        if let Some(i) = existing {
            return Ok(i as u16);
        }
        let f = self.f();
        f.consts.push(value);
        let index = f.consts.len() - 1;
        u16::try_from(index)
            .map_err(|_| self.error("too many constants in one function", self.span))
    }

    fn load_const(&mut self, dst: u8, value: Value) -> CResult<()> {
        let k = self.constant(value)?;
        self.emit(Op::LoadConst { dst, k });
        Ok(())
    }

    fn name_index(&mut self, name: &str) -> CResult<u8> {
        let symbol = self.symbols.intern(name);
        if let Some(i) = self.f().names.iter().position(|s| *s == symbol) {
            return Ok(i as u8);
        }
        let f = self.f();
        f.names.push(symbol);
        let index = f.names.len() - 1;
        u8::try_from(index).map_err(|_| {
            self.error(
                "too many distinct field and method names in one function",
                self.span,
            )
        })
    }

    // ==== registers and scopes =============================================

    fn alloc(&mut self) -> CResult<u8> {
        let f = self.f();
        if f.next_reg >= MAX_REGISTERS {
            return Err(self.error(
                "this function needs more than 256 registers; split it into smaller functions",
                self.span,
            ));
        }
        let r = f.next_reg as u8;
        f.next_reg += 1;
        f.max_reg = f.max_reg.max(f.next_reg);
        Ok(r)
    }

    /// Allocates `n` consecutive registers.
    fn alloc_n(&mut self, n: usize) -> CResult<Vec<u8>> {
        (0..n).map(|_| self.alloc()).collect()
    }

    fn mark(&mut self) -> usize {
        self.f().next_reg
    }

    fn release(&mut self, mark: usize) {
        let f = self.f();
        f.next_reg = mark;
    }

    fn push_scope(&mut self) {
        let f = self.f();
        f.scope_starts.push(f.next_reg);
        f.scopes.push(Vec::new());
    }

    fn pop_scope(&mut self) {
        let f = self.f();
        f.scopes.pop();
        if let Some(start) = f.scope_starts.pop() {
            f.next_reg = start;
        }
    }

    /// Declares `name` as the variable in `reg`, which already holds its
    /// value. A variable that closures capture is moved into a shared cell.
    fn bind_name(&mut self, name: &str, reg: u8) {
        let cell = self.f().cells.contains(name);
        if cell {
            self.emit(Op::MakeCell { reg });
        }
        self.bind_entry(name, reg, cell);
    }

    /// Declares `name` without a cell: for loop variables, which are new in
    /// each iteration, so a closure keeps the value of its own iteration.
    fn bind_plain(&mut self, name: &str, reg: u8) {
        self.bind_entry(name, reg, false);
    }

    fn bind_entry(&mut self, name: &str, reg: u8, cell: bool) {
        if let Some(scope) = self.f().scopes.last_mut() {
            scope.push((name.to_string(), reg, cell));
        }
    }

    fn hide(&mut self, reg: u8) {
        self.bind_plain("", reg);
    }

    /// Whether `reg` holds a variable or a hidden value such as a loop
    /// counter, rather than a temporary.
    fn is_bound(&mut self, reg: u8) -> bool {
        self.f()
            .scopes
            .iter()
            .any(|s| s.iter().any(|(_, r, _)| *r == reg))
    }

    fn is_named_local(&mut self, reg: u8) -> bool {
        self.f()
            .scopes
            .iter()
            .any(|s| s.iter().any(|(n, r, _)| *r == reg && !n.is_empty()))
    }

    /// Frees temporaries after a statement, keeping the variables it
    /// declared.
    fn settle(&mut self, before: usize) {
        let f = self.f();
        let top = f
            .scopes
            .last()
            .and_then(|s| s.iter().map(|(_, r, _)| *r as usize + 1).max())
            .unwrap_or(0);
        f.next_reg = before.max(top);
    }

    // ==== names ============================================================

    /// `use std.web` brings the module in as `web`; `use std.web (serve)`
    /// brings in the names listed. Other modules belong to the package
    /// system, which the interpreter leaves to the release toolchain.
    fn use_decl(&mut self, decl: &UseDecl) {
        let segments: Vec<&str> = decl.path.segments.iter().map(|s| s.name.as_str()).collect();
        if segments.first() == Some(&"python") && decl.url.is_none() {
            self.python_use(decl, &segments[1..]);
            return;
        }
        let [std, name] = segments[..] else {
            return;
        };
        if std != "std" {
            return;
        }
        let Some(prefix) = builtins::std_module(name) else {
            self.error_hint(
                format!("the standard library has no module `{name}`"),
                decl.path.span,
                "the standard library has `std.os`, `std.fs`, `std.path`, `std.time`, `std.random`, `std.math`, `std.json`, `std.re`, `std.csv` and `std.http`",
            );
            return;
        };
        match &decl.kind {
            UseKind::Module { alias } => {
                let local = alias.as_ref().map_or(name, |a| a.name.as_str());
                self.modules.insert(local.to_string(), prefix);
            }
            UseKind::Names(names) => {
                for item in names {
                    match builtins::lookup(&format!("{prefix}.{}", item.name.name)) {
                        Some(id) => {
                            let local = item.alias.as_ref().unwrap_or(&item.name);
                            self.imports.insert(local.name.clone(), id);
                        }
                        None => {
                            self.error(
                                format!("`std.{name}` has no `{}`", item.name.name),
                                item.name.span,
                            );
                        }
                    }
                }
            }
        }
    }

    /// A global filled at start-up by calling builtin `native` with `args`.
    fn foreign_global(&mut self, name: &str, span: Span, native: &str, args: Vec<Value>) {
        let Some(id) = builtins::lookup(native) else {
            return;
        };
        if let Some(slot) = self.add_global(name, GlobalInit::Const, span) {
            self.global_index
                .insert(name.to_string(), GlobalKind::Const { slot });
            self.foreign_inits.push((slot, id, args));
        }
    }

    /// `use python.numpy as np`, `use python.os.path (join)`.
    fn python_use(&mut self, decl: &UseDecl, path: &[&str]) {
        if path.is_empty() {
            self.error_hint(
                "`use python` needs a module name",
                decl.path.span,
                "for example `use python.numpy as np`",
            );
            return;
        }
        let module = Value::str(path.join("."));
        match &decl.kind {
            UseKind::Module { alias } => {
                let local = alias
                    .as_ref()
                    .map_or(path[path.len() - 1], |a| a.name.as_str())
                    .to_string();
                self.foreign_global(&local, decl.path.span, "python.import", vec![module]);
            }
            UseKind::Names(names) => {
                for n in names {
                    let local = n.alias.as_ref().unwrap_or(&n.name).name.clone();
                    self.foreign_global(
                        &local,
                        n.name.span,
                        "python.import",
                        vec![module.clone(), Value::str(n.name.name.clone())],
                    );
                }
            }
        }
    }

    /// `extern "c":` functions, from the library named by `@link("m")`, or
    /// from the program itself (libc) when there is none.
    fn extern_decl(&mut self, decl: &ExternDecl, item: &Item) {
        if decl.abi != "c" {
            self.error_hint(
                format!(
                    "`extern \"{}\"` is not supported by the interpreter",
                    decl.abi
                ),
                item.span,
                "the interpreter calls C libraries: `extern \"c\":`",
            );
            return;
        }
        let mut library = String::new();
        for d in &item.decorators {
            let name: Vec<&str> = d.name.segments.iter().map(|s| s.name.as_str()).collect();
            if name == ["link"] {
                match d.args.first().map(|a| &a.value.kind) {
                    Some(ExprKind::Str(parts)) => {
                        if let [StrSegment::Text(t)] = parts.as_slice() {
                            library = t.clone();
                        }
                    }
                    _ => {
                        self.error("`@link` takes the library's name: `@link(\"m\")`", d.span);
                    }
                }
            }
        }
        for f in &decl.functions {
            match c_signature(f) {
                Ok(signature) => self.foreign_global(
                    &f.name.name,
                    f.name.span,
                    "ffi.function",
                    vec![
                        Value::str(library.clone()),
                        Value::str(
                            f.name
                                .name
                                .rsplit('.')
                                .next()
                                .unwrap_or(&f.name.name)
                                .to_string(),
                        ),
                        Value::str(signature),
                    ],
                ),
                Err((message, span)) => {
                    self.error_hint(
                        message,
                        span,
                        "C functions take and return numbers (i8 to u64, f32, f64), bool, str, `ptr T`, and tensors or lists of numbers as pointers",
                    );
                }
            }
        }
    }

    /// The module a name refers to, unless a variable hides it.
    fn module_prefix(&mut self, name: &str) -> Option<&'static str> {
        let prefix = *self.modules.get(name)?;
        let level = self.fns.len() - 1;
        if self.resolve_in(level, name).is_some() || self.global_index.contains_key(name) {
            return None;
        }
        Some(prefix)
    }

    /// A constant that `module.name` refers to, as in `math.pi`.
    fn module_constant(&mut self, expr: &Expr) -> Option<Value> {
        let ExprKind::Field { base, name, .. } = &expr.kind else {
            return None;
        };
        let ExprKind::Name(module) = &base.kind else {
            return None;
        };
        let prefix = self.module_prefix(&module.name)?;
        builtins::constant(&format!("{prefix}.{}", name.name))
    }

    /// The builtin that `module.name` refers to, as in `web.serve`.
    fn module_member(&mut self, expr: &Expr) -> Option<Result<u16, (String, Span)>> {
        let ExprKind::Field { base, name, .. } = &expr.kind else {
            return None;
        };
        let ExprKind::Name(module) = &base.kind else {
            return None;
        };
        let prefix = self.module_prefix(&module.name)?;
        Some(
            builtins::lookup(&format!("{prefix}.{}", name.name))
                .ok_or_else(|| (format!("`std.{prefix}` has no `{}`", name.name), name.span)),
        )
    }

    fn resolve(&mut self, name: &str) -> Option<Resolved> {
        let level = self.fns.len() - 1;
        if let Some(found) = self.resolve_in(level, name) {
            return Some(found);
        }
        if let Some(global) = self.global_index.get(name) {
            return Some(Resolved::Global(global.slot()));
        }
        if let Some(&id) = self.imports.get(name) {
            return Some(Resolved::Native(id));
        }
        if let Some(value) = builtins::constant(name) {
            return Some(Resolved::Constant(value));
        }
        builtins::lookup(name).map(Resolved::Native)
    }

    fn resolve_in(&mut self, level: usize, name: &str) -> Option<Resolved> {
        if name.is_empty() {
            return None;
        }
        let f = &self.fns[level];
        for scope in f.scopes.iter().rev() {
            if let Some((_, r, cell)) = scope.iter().rev().find(|(n, _, _)| n == name) {
                return Some(if *cell {
                    Resolved::Cell(*r)
                } else {
                    Resolved::Local(*r)
                });
            }
        }
        if let Some(i) = f.captures.iter().position(|(n, _, _)| n == name) {
            let cell = f.captures[i].2;
            return Some(if cell {
                Resolved::UpCell(i as u8)
            } else {
                Resolved::Capture(i as u8)
            });
        }
        if level == 0 {
            return None;
        }
        let (capture, cell) = match self.resolve_in(level - 1, name)? {
            Resolved::Local(r) => (Capture::Local(r), false),
            Resolved::Cell(r) => (Capture::Local(r), true),
            Resolved::Capture(i) => (Capture::Outer(i), false),
            Resolved::UpCell(i) => (Capture::Outer(i), true),
            _ => return None,
        };
        let f = &mut self.fns[level];
        if f.captures.len() >= 255 {
            return None;
        }
        f.captures.push((name.to_string(), capture, cell));
        let index = (f.captures.len() - 1) as u8;
        Some(if cell {
            Resolved::UpCell(index)
        } else {
            Resolved::Capture(index)
        })
    }

    fn local_of(&mut self, expr: &Expr) -> Option<u8> {
        match &expr.kind {
            ExprKind::Name(id) => match self.resolve(&id.name) {
                Some(Resolved::Local(r)) => Some(r),
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether `expr` names storage that an assignment could write back to.
    fn is_place(&mut self, expr: &Expr) -> bool {
        match &expr.kind {
            ExprKind::Name(id) => matches!(
                self.resolve(&id.name),
                Some(Resolved::Local(_) | Resolved::Cell(_) | Resolved::UpCell(_))
            ),
            ExprKind::Field {
                base,
                optional: false,
                ..
            } => self.is_place(base),
            ExprKind::Index { base, .. } => self.is_place(base),
            _ => false,
        }
    }

    fn unknown_name(&mut self, name: &str, span: Span) -> Fail {
        let mut candidates: Vec<String> = self.global_index.keys().cloned().collect();
        candidates.extend(builtins::NATIVES.iter().map(|n| n.name.to_string()));
        for scope in &self.f().scopes {
            candidates.extend(
                scope
                    .iter()
                    .filter(|(n, _, _)| !n.is_empty())
                    .map(|(n, _, _)| n.clone()),
            );
        }
        let best = candidates
            .iter()
            .map(|c| (edit_distance(name, c), c))
            .filter(|(d, _)| *d <= 2 && *d < name.len())
            .min();
        match best {
            Some((_, suggestion)) => {
                let hint = format!("did you mean `{suggestion}`?");
                self.error_hint(format!("`{name}` is not defined"), span, hint)
            }
            None => self.error(format!("`{name}` is not defined"), span),
        }
    }

    // ==== statements =======================================================

    fn stmts(&mut self, stmts: &[Stmt]) {
        for (i, stmt) in stmts.iter().enumerate() {
            if let StmtKind::Defer(expr) = &stmt.kind {
                // `defer e` runs `e` when the enclosing block ends, however
                // it ends: the rest of the block becomes a `try` body with
                // `e` as its `finally`.
                self.set_pos(stmt.span);
                let finally = Rc::new(vec![Stmt {
                    id: stmt.id,
                    span: stmt.span,
                    kind: StmtKind::Expr(expr.clone()),
                }]);
                let _ = self.try_finally(&stmts[i + 1..], &[], Some(finally));
                return;
            }
            let (before, scopes, loops, cleanups) = {
                let f = self.f();
                (f.next_reg, f.scopes.len(), f.loops.len(), f.cleanups.len())
            };
            if self.stmt(stmt).is_err() {
                // Undo whatever the failed statement left half done, so the
                // rest of the function compiles normally.
                let f = self.f();
                f.scopes.truncate(scopes);
                f.scope_starts.truncate(scopes);
                f.loops.truncate(loops);
                f.cleanups.truncate(cleanups);
                f.next_reg = before;
            }
            self.settle(before);
        }
    }

    fn block(&mut self, block: &Block) {
        self.push_scope();
        self.stmts(&block.stmts);
        self.pop_scope();
    }

    fn stmt(&mut self, stmt: &Stmt) -> CResult<()> {
        self.set_pos(stmt.span);
        match &stmt.kind {
            StmtKind::Binding(b) => self.binding(b),
            StmtKind::Expr(e) => {
                let mark = self.mark();
                self.expr_any(e)?;
                self.release(mark);
                Ok(())
            }
            StmtKind::Assign { target, op, value } => self.assign(target, *op, value),
            StmtKind::Return(value) => self.return_stmt(value.as_ref()),
            StmtKind::Fail(e) => {
                let r = self.expr_any(e)?;
                self.emit(Op::Fail { src: r });
                Ok(())
            }
            StmtKind::Defer(_) => Ok(()),
            StmtKind::Stop => self.loop_exit(true),
            StmtKind::Skip => self.loop_exit(false),
            StmtKind::If(i) => self.if_stmt(i),
            StmtKind::While { condition, body } => self.while_stmt(condition, body),
            StmtKind::Loop(body) => self.loop_stmt(body),
            StmtKind::For(f) => self.for_stmt(f),
            StmtKind::Match(m) => self.match_stmt(m),
            StmtKind::Try(t) => {
                let finally = t.finally.as_ref().map(|b| Rc::new(b.stmts.clone()));
                self.try_finally(&t.body.stmts, &t.catches, finally)
            }
            StmtKind::With { items, body, .. } => self.with_stmt(items, body, stmt.span),
            StmtKind::On(on) => {
                let mark = self.mark();
                let target = self.expr_any(&on.target)?;
                self.emit(Op::PushDevice { src: target });
                self.release(mark);
                self.f().cleanups.push(Cleanup::PopDevice);
                self.block(&on.body);
                self.f().cleanups.pop();
                self.emit(Op::PopDevice);
                Ok(())
            }
            StmtKind::Region(region) => {
                self.push_scope();
                let r = self.alloc()?;
                self.load_const(r, Value::Region)?;
                self.bind_name(&region.name.name, r);
                self.stmts(&region.body.stmts);
                self.pop_scope();
                Ok(())
            }
            StmtKind::Unsafe(body) => {
                self.block(body);
                Ok(())
            }
            StmtKind::Item(item) => self.nested_item(item),
        }
    }

    /// `with a as x, b:` holds each resource in a hidden variable and runs
    /// the body as a `try` whose `finally` calls `close()` on each, last
    /// opened first, however the body ends.
    fn with_stmt(&mut self, items: &[WithItem], body: &Block, span: Span) -> CResult<()> {
        self.push_scope();
        let mut closes = Vec::with_capacity(items.len());
        for (i, item) in items.iter().enumerate() {
            let hidden = format!("$with{}_{i}", self.f().code.len());
            let r = self.alloc()?;
            self.expr_to(&item.value, r)?;
            self.bind_name(&hidden, r);
            if let Some(binding) = &item.binding {
                let named = self.alloc()?;
                self.emit(Op::Move { dst: named, src: r });
                self.bind_name(&binding.name, named);
            }
            let resource = Expr::new(
                NodeId(0),
                ExprKind::Name(Ident {
                    name: hidden,
                    span: item.value.span,
                }),
                item.value.span,
            );
            let close = Expr::new(
                NodeId(0),
                ExprKind::Call {
                    callee: Box::new(Expr::new(
                        NodeId(0),
                        ExprKind::Field {
                            base: Box::new(resource),
                            name: Ident {
                                name: "close".into(),
                                span: item.value.span,
                            },
                            optional: false,
                        },
                        item.value.span,
                    )),
                    args: Vec::new(),
                },
                item.value.span,
            );
            closes.push(Stmt {
                id: NodeId(0),
                span,
                kind: StmtKind::Expr(close),
            });
        }
        closes.reverse();
        let result = self.try_finally(&body.stmts, &[], Some(Rc::new(closes)));
        self.pop_scope();
        result
    }

    fn nested_item(&mut self, item: &Item) -> CResult<()> {
        let ItemKind::Fn(f) = &item.kind else {
            return Err(self.error(
                "only functions can be declared inside a function",
                item.span,
            ));
        };
        let FnBody::Block(body) = &f.body else {
            return Ok(());
        };
        let r = self.alloc()?;
        // Declared before its body is compiled, so the function can call
        // itself through the shared cell.
        let recursive = self.f().cells.contains(&f.name.name);
        if recursive {
            self.emit(Op::LoadNone { dst: r });
            self.bind_name(&f.name.name, r);
        }
        let params: Vec<(String, Option<&Expr>)> = f
            .params
            .iter()
            .map(|p| (p.name.name.clone(), p.default.as_ref()))
            .collect();
        let cells = nested_names(&body.stmts, &params);
        self.begin_fn(f.name.name.clone(), &params, cells);
        self.stmts(&body.stmts);
        let mut proto = self.end_fn();
        proto.is_async = f.modifiers.is_async;
        let index = self.add_proto(proto)?;
        if recursive {
            let closure = self.alloc()?;
            self.emit(Op::MakeClosure {
                dst: closure,
                proto: index,
            });
            self.emit(Op::SetCell {
                cell: r,
                src: closure,
            });
        } else {
            self.emit(Op::MakeClosure {
                dst: r,
                proto: index,
            });
            self.bind_name(&f.name.name, r);
        }
        Ok(())
    }

    fn apply_placement(&mut self, ty: Option<&TypeExpr>, reg: u8) -> CResult<()> {
        if let Some(TypeExpr {
            kind: TypeExprKind::Placed { target, .. },
            ..
        }) = ty
        {
            let mark = self.mark();
            let t = self.expr_any(target)?;
            self.emit(Op::Place {
                dst: reg,
                src: reg,
                target: t,
            });
            self.release(mark);
        }
        Ok(())
    }

    fn binding(&mut self, b: &Binding) -> CResult<()> {
        match &b.target {
            BindTarget::Name(ident) => {
                let r = self.alloc()?;
                match &b.value {
                    Some(value) => self.expr_to(value, r)?,
                    None => {
                        self.emit(Op::LoadNone { dst: r });
                    }
                }
                self.apply_placement(b.declared_ty.as_ref(), r)?;
                self.release(r as usize + 1);
                self.bind_name(&ident.name, r);
                Ok(())
            }
            target => {
                let Some(value) = &b.value else {
                    return Err(self.error("unpacking into several names needs a value", self.span));
                };
                let v = self.expr_any(value)?;
                self.destructure(target, v)
            }
        }
    }

    /// Declares the names in `target` and fills them from the sequence in
    /// `value`.
    fn destructure(&mut self, target: &BindTarget, value: u8) -> CResult<()> {
        match target {
            BindTarget::Name(ident) => {
                let r = self.alloc()?;
                self.emit(Op::Move { dst: r, src: value });
                self.bind_name(&ident.name, r);
            }
            BindTarget::Discard(_) => {}
            BindTarget::Tuple(items, _) => {
                for (i, item) in items.iter().enumerate() {
                    let index = self.alloc()?;
                    self.emit(Op::LoadInt {
                        dst: index,
                        value: i as i16,
                    });
                    let element = self.alloc()?;
                    self.emit(Op::GetIndex {
                        dst: element,
                        obj: value,
                        index,
                    });
                    match item {
                        BindTarget::Name(ident) => self.bind_name(&ident.name, element),
                        other => self.destructure(other, element)?,
                    }
                }
            }
        }
        Ok(())
    }

    fn assign(&mut self, target: &Expr, op: AssignOp, value: &Expr) -> CResult<()> {
        if op == AssignOp::Assign {
            if let ExprKind::Name(ident) = &target.kind {
                match self.resolve(&ident.name) {
                    Some(Resolved::Local(r)) => {
                        let mark = self.mark();
                        self.expr_to(value, r)?;
                        self.release(mark);
                        return Ok(());
                    }
                    Some(Resolved::Cell(_) | Resolved::UpCell(_)) => {
                        let v = self.expr_any(value)?;
                        return self.store_place(target, v);
                    }
                    Some(Resolved::Capture(_)) => return Err(self.error_hint(
                        format!("cannot assign to `{}` here", ident.name),
                        target.span,
                        "a loop variable is new in each iteration, so a closure keeps its own copy",
                    )),
                    Some(Resolved::Global(_)) => {
                        return Err(self.error(
                            format!(
                                "cannot assign to `{}`: it is a top-level constant or declaration",
                                ident.name
                            ),
                            target.span,
                        ))
                    }
                    _ => {
                        // A new variable, as in Python.
                        let r = self.alloc()?;
                        self.expr_to(value, r)?;
                        self.release(r as usize + 1);
                        self.bind_name(&ident.name, r);
                        return Ok(());
                    }
                }
            }
            if let ExprKind::Tuple(items) = &target.kind {
                let v = self.expr_any(value)?;
                for (i, item) in items.iter().enumerate() {
                    let index = self.alloc()?;
                    self.emit(Op::LoadInt {
                        dst: index,
                        value: i as i16,
                    });
                    let element = self.alloc()?;
                    self.emit(Op::GetIndex {
                        dst: element,
                        obj: v,
                        index,
                    });
                    self.store_place(item, element)?;
                }
                return Ok(());
            }
            let v = self.expr_any(value)?;
            return self.store_place(target, v);
        }

        if let Some(r) = self.local_of(target) {
            if op == AssignOp::Coalesce {
                let skip = self.emit(Op::JumpIfNotNone { src: r, offset: 0 });
                self.expr_to(value, r)?;
                self.patch_here(skip);
            } else if let (Some(imm), AssignOp::Add | AssignOp::Sub) = (small_int(value), op) {
                self.emit(if op == AssignOp::Add {
                    Op::AddI { dst: r, a: r, imm }
                } else {
                    Op::SubI { dst: r, a: r, imm }
                });
            } else {
                let rhs = self.expr_any(value)?;
                self.emit_assign_op(op, r, r, rhs);
            }
            return Ok(());
        }
        if let ExprKind::Name(ident) = &target.kind {
            if self.resolve(&ident.name).is_none() {
                return Err(self.unknown_name(&ident.name, target.span));
            }
        }
        let current = self.expr_any(target)?;
        if op == AssignOp::Coalesce {
            let skip = self.emit(Op::JumpIfNotNone {
                src: current,
                offset: 0,
            });
            let v = self.expr_any(value)?;
            self.store_place(target, v)?;
            self.patch_here(skip);
        } else {
            let rhs = self.expr_any(value)?;
            let result = self.alloc()?;
            self.emit_assign_op(op, result, current, rhs);
            self.store_place(target, result)?;
        }
        Ok(())
    }

    fn emit_assign_op(&mut self, op: AssignOp, dst: u8, a: u8, b: u8) {
        let op = match op {
            AssignOp::Add => Op::Add { dst, a, b },
            AssignOp::Sub => Op::Sub { dst, a, b },
            AssignOp::Mul => Op::Mul { dst, a, b },
            AssignOp::Div => Op::Div { dst, a, b },
            AssignOp::FloorDiv => Op::FloorDiv { dst, a, b },
            AssignOp::Rem => Op::Rem { dst, a, b },
            AssignOp::MatMul => Op::MatMul { dst, a, b },
            AssignOp::Pow => Op::Pow { dst, a, b },
            AssignOp::BitAnd => Op::BitAnd { dst, a, b },
            AssignOp::BitOr => Op::BitOr { dst, a, b },
            AssignOp::BitXor => Op::BitXor { dst, a, b },
            AssignOp::Shl => Op::Shl { dst, a, b },
            AssignOp::Shr => Op::Shr { dst, a, b },
            AssignOp::Assign | AssignOp::Coalesce => Op::Move { dst, src: b },
        };
        self.emit(op);
    }

    /// Writes register `src` to the place `target` names. Writing to a
    /// field or element of a value type writes the updated value back to
    /// its own place in turn: `e.pos.y = 0` updates `e`.
    fn store_place(&mut self, target: &Expr, src: u8) -> CResult<()> {
        match &target.kind {
            ExprKind::Name(ident) => match self.resolve(&ident.name) {
                Some(Resolved::Local(r)) => {
                    if r != src {
                        self.emit(Op::Move { dst: r, src });
                    }
                    Ok(())
                }
                Some(Resolved::Cell(cell)) => {
                    self.emit(Op::SetCell { cell, src });
                    Ok(())
                }
                Some(Resolved::UpCell(idx)) => {
                    self.emit(Op::SetUpval { idx, src });
                    Ok(())
                }
                Some(Resolved::Capture(_)) | Some(Resolved::Global(_)) => {
                    Err(self.error(format!("cannot assign to `{}`", ident.name), target.span))
                }
                _ => {
                    let r = self.alloc()?;
                    self.emit(Op::Move { dst: r, src });
                    self.bind_name(&ident.name, r);
                    Ok(())
                }
            },
            ExprKind::Field {
                base,
                name,
                optional: false,
            } => {
                let n = self.name_index(&name.name)?;
                if let Some(r) = self.local_of(base) {
                    self.emit(Op::SetField {
                        obj: r,
                        name: n,
                        src,
                    });
                    return Ok(());
                }
                let holder = self.alloc()?;
                self.expr_to(base, holder)?;
                self.emit(Op::SetField {
                    obj: holder,
                    name: n,
                    src,
                });
                if self.is_place(base) {
                    self.store_place(base, holder)?;
                }
                Ok(())
            }
            ExprKind::Index { base, subscripts } => {
                let key = self.index_key(subscripts)?;
                if let Some(r) = self.local_of(base) {
                    self.emit(Op::SetIndex {
                        obj: r,
                        index: key,
                        src,
                    });
                    return Ok(());
                }
                let holder = self.alloc()?;
                self.expr_to(base, holder)?;
                self.emit(Op::SetIndex {
                    obj: holder,
                    index: key,
                    src,
                });
                if self.is_place(base) {
                    self.store_place(base, holder)?;
                }
                Ok(())
            }
            _ => Err(self.error("cannot assign to this expression", target.span)),
        }
    }

    fn emit_cleanups(&mut self, down_to: usize) {
        let saved = self.f().cleanups.clone();
        for i in (down_to..saved.len()).rev() {
            self.f().cleanups.truncate(i);
            match &saved[i] {
                Cleanup::PopDevice => {
                    self.emit(Op::PopDevice);
                }
                Cleanup::Handler => {
                    self.emit(Op::TryEnd);
                }
                Cleanup::Finally(stmts) => {
                    self.emit(Op::TryEnd);
                    self.push_scope();
                    self.stmts(stmts);
                    self.pop_scope();
                }
                Cleanup::WriteBack { list, index, item } => {
                    self.emit(Op::TryEnd);
                    self.emit(Op::SetIndex {
                        obj: *list,
                        index: *index,
                        src: *item,
                    });
                }
            }
        }
        self.f().cleanups = saved;
    }

    fn return_stmt(&mut self, value: Option<&Expr>) -> CResult<()> {
        let has_cleanups = !self.f().cleanups.is_empty();
        let src = match value {
            Some(e) => {
                let mut r = self.expr_any(e)?;
                if has_cleanups && self.is_named_local(r) {
                    // A `finally` block may change the variable; return the
                    // value it had at `return`.
                    let copy = self.alloc()?;
                    self.emit(Op::Move { dst: copy, src: r });
                    r = copy;
                }
                Some(r)
            }
            None => None,
        };
        self.emit_cleanups(0);
        self.sync_param_cells();
        match src {
            Some(src) => self.emit(Op::Return { src }),
            None => self.emit(Op::ReturnNone),
        };
        Ok(())
    }

    fn loop_exit(&mut self, is_stop: bool) -> CResult<()> {
        let Some(depth) = self.f().loops.last().map(|l| l.cleanup_depth) else {
            let word = if is_stop { "stop" } else { "skip" };
            return Err(self.error(
                format!("`{word}` can only be used inside a loop"),
                self.span,
            ));
        };
        self.emit_cleanups(depth);
        let jump = self.emit(Op::Jump { offset: 0 });
        if let Some(lp) = self.f().loops.last_mut() {
            if is_stop {
                lp.breaks.push(jump);
            } else {
                lp.continues.push(jump);
            }
        }
        Ok(())
    }

    fn push_loop(&mut self) {
        let depth = self.f().cleanups.len();
        self.f().loops.push(LoopCtx {
            breaks: Vec::new(),
            continues: Vec::new(),
            cleanup_depth: depth,
        });
    }

    fn pop_loop(&mut self) -> LoopCtx {
        self.f().loops.pop().unwrap_or(LoopCtx {
            breaks: Vec::new(),
            continues: Vec::new(),
            cleanup_depth: 0,
        })
    }

    fn condition(&mut self, condition: &Condition, fails: &mut Vec<usize>) -> CResult<()> {
        match condition {
            Condition::Expr(e) => {
                let mark = self.mark();
                let r = self.expr_any(e)?;
                fails.push(self.emit(Op::JumpIfFalse { cond: r, offset: 0 }));
                self.release(mark);
            }
            Condition::Let { pattern, value } => {
                let v = self.expr_any(value)?;
                self.hide(v);
                self.pattern(pattern, v, fails)?;
            }
        }
        Ok(())
    }

    fn if_stmt(&mut self, stmt: &IfStmt) -> CResult<()> {
        let mut ends = Vec::new();
        let count = stmt.branches.len();
        for (i, (condition, body)) in stmt.branches.iter().enumerate() {
            self.push_scope();
            let mut fails = Vec::new();
            self.condition(condition, &mut fails)?;
            self.stmts(&body.stmts);
            self.pop_scope();
            if i + 1 < count || stmt.else_body.is_some() {
                ends.push(self.emit(Op::Jump { offset: 0 }));
            }
            for f in fails {
                self.patch_here(f);
            }
        }
        if let Some(body) = &stmt.else_body {
            self.block(body);
        }
        for end in ends {
            self.patch_here(end);
        }
        Ok(())
    }

    fn while_stmt(&mut self, condition: &Condition, body: &Block) -> CResult<()> {
        let top = self.here();
        self.push_scope();
        let mut fails = Vec::new();
        self.condition(condition, &mut fails)?;
        self.push_loop();
        self.stmts(&body.stmts);
        let lp = self.pop_loop();
        self.pop_scope();
        for c in lp.continues {
            self.patch_to(c, top);
        }
        self.jump_to(top);
        for f in fails.into_iter().chain(lp.breaks) {
            self.patch_here(f);
        }
        Ok(())
    }

    fn loop_stmt(&mut self, body: &Block) -> CResult<()> {
        let top = self.here();
        self.push_loop();
        self.block(body);
        let lp = self.pop_loop();
        for c in lp.continues {
            self.patch_to(c, top);
        }
        self.jump_to(top);
        for b in lp.breaks {
            self.patch_here(b);
        }
        Ok(())
    }

    fn begin_iteration(&mut self, target: &BindTarget, iter: &Expr) -> CResult<IterLoop> {
        self.begin_iteration_mode(target, iter, false)
    }

    fn begin_iteration_mode(
        &mut self,
        target: &BindTarget,
        iter: &Expr,
        is_async: bool,
    ) -> CResult<IterLoop> {
        self.push_scope();
        if let (
            false,
            ExprKind::Range {
                start,
                end,
                inclusive,
                step,
            },
        ) = (is_async, &iter.kind)
        {
            if matches!(target, BindTarget::Name(_) | BindTarget::Discard(_)) {
                // A counted loop: two instructions of overhead per iteration.
                let regs = self.alloc_n(4)?;
                let base = regs[0];
                for &r in &regs[..3] {
                    self.hide(r);
                }
                match target {
                    BindTarget::Name(ident) => self.bind_plain(&ident.name, regs[3]),
                    _ => self.hide(regs[3]),
                }
                let mark = self.mark();
                let bounds = self.alloc_n(3)?;
                self.expr_to(start, bounds[0])?;
                self.expr_to(end, bounds[1])?;
                match step {
                    Some(step) => self.expr_to(step, bounds[2])?,
                    None => {
                        self.emit(Op::LoadInt {
                            dst: bounds[2],
                            value: 1,
                        });
                    }
                }
                self.emit(Op::MakeRange {
                    dst: base,
                    base: bounds[0],
                    inclusive: *inclusive,
                });
                self.release(mark);
                let exit = self.emit(Op::ForPrep { base, offset: 0 });
                let top = self.here();
                return Ok(IterLoop {
                    kind: IterKind::Counted { base },
                    top,
                    exit,
                });
            }
        }
        let it = self.alloc()?;
        self.hide(it);
        let mark = self.mark();
        let src = self.expr_any(iter)?;
        if is_async {
            self.emit(Op::AsyncIterInit { dst: it, src });
        } else {
            self.emit(Op::IterInit { dst: it, src });
        }
        self.release(mark);
        let item = self.alloc()?;
        match target {
            BindTarget::Name(ident) => self.bind_plain(&ident.name, item),
            _ => self.hide(item),
        }
        let top = self.here();
        if is_async {
            self.emit(Op::AsyncIterNext {
                iter: it,
                dst: item,
            });
        } else {
            self.emit(Op::IterNext {
                iter: it,
                dst: item,
            });
        }
        let exit = self.emit(Op::Jump { offset: 0 });
        if let BindTarget::Tuple(..) = target {
            self.destructure(target, item)?;
        }
        Ok(IterLoop {
            kind: IterKind::Generic,
            top,
            exit,
        })
    }

    fn end_iteration(&mut self, lp: IterLoop) {
        match lp.kind {
            IterKind::Counted { base } => {
                let at = self.emit(Op::ForLoop { base, offset: 0 });
                self.patch_to(at, lp.top);
            }
            IterKind::Generic => self.jump_to(lp.top),
        }
        self.patch_here(lp.exit);
        self.pop_scope();
    }

    fn for_stmt(&mut self, stmt: &ForStmt) -> CResult<()> {
        // `par for` runs its iterations one after another here; the result
        // is the same, because iterations of a `par for` are independent.
        let is_async = stmt.mode == ForMode::Async;
        if stmt.pass == PassMode::Mut && !is_async {
            return self.for_mut(stmt);
        }
        let lp = self.begin_iteration_mode(&stmt.target, &stmt.iter, is_async)?;
        self.push_loop();
        self.stmts(&stmt.body.stmts);
        let ctx = self.pop_loop();
        let next = self.here();
        for c in ctx.continues {
            self.patch_to(c, next);
        }
        self.end_iteration(lp);
        for b in ctx.breaks {
            self.patch_here(b);
        }
        Ok(())
    }

    /// `each mut item in list`: each element is moved out of the list,
    /// changed by the body, and moved back. Moving rather than copying
    /// means a struct element is changed in place, with no copy on write;
    /// the loop has the list to itself, so nothing else can observe the
    /// gap. If the body fails, a handler puts the element back first.
    fn for_mut(&mut self, stmt: &ForStmt) -> CResult<()> {
        let BindTarget::Name(ident) = &stmt.target else {
            return Err(self.error("`each mut` takes a single name", self.span));
        };
        self.push_scope();
        let regs = self.alloc_n(7)?;
        let (list, index, len, one, cond, err, item) = (
            regs[0], regs[1], regs[2], regs[3], regs[4], regs[5], regs[6],
        );
        for &r in &regs[..6] {
            self.hide(r);
        }
        self.expr_to(&stmt.iter, list)?;
        self.emit(Op::LoadInt {
            dst: index,
            value: 0,
        });
        self.emit(Op::LoadInt { dst: one, value: 1 });
        self.emit(Op::Len {
            dst: len,
            src: list,
        });
        self.emit(Op::LoadMissing { dst: item });
        let handler = self.emit(Op::TryBegin { err, offset: 0 });
        let top = self.here();
        self.emit(Op::Lt {
            dst: cond,
            a: index,
            b: len,
        });
        let exit = self.emit(Op::JumpIfFalse { cond, offset: 0 });
        self.emit(Op::TakeIndex {
            dst: item,
            obj: list,
            index,
        });
        self.bind_plain(&ident.name, item);
        self.f()
            .cleanups
            .push(Cleanup::WriteBack { list, index, item });
        self.push_loop();
        self.stmts(&stmt.body.stmts);
        let ctx = self.pop_loop();
        self.f().cleanups.pop();
        let next = self.here();
        for c in ctx.continues {
            self.patch_to(c, next);
        }
        self.emit(Op::SetIndex {
            obj: list,
            index,
            src: item,
        });
        self.emit(Op::Add {
            dst: index,
            a: index,
            b: one,
        });
        self.jump_to(top);
        // `stop` puts the current element back before leaving.
        let stop = self.here();
        for b in ctx.breaks {
            self.patch_to(b, stop);
        }
        self.emit(Op::SetIndex {
            obj: list,
            index,
            src: item,
        });
        self.patch_here(exit);
        self.emit(Op::TryEnd);
        let done = self.emit(Op::Jump { offset: 0 });
        // The body failed: restore the element it held, then pass the
        // failure on. Before the first element is taken, `item` is missing.
        self.patch_here(handler);
        let restore = self.emit(Op::JumpIfNotMissing {
            src: item,
            offset: 0,
        });
        let rethrow = self.emit(Op::Jump { offset: 0 });
        self.patch_here(restore);
        self.emit(Op::SetIndex {
            obj: list,
            index,
            src: item,
        });
        self.patch_here(rethrow);
        self.emit(Op::Fail { src: err });
        self.patch_here(done);
        self.pop_scope();
        Ok(())
    }

    fn match_stmt(&mut self, stmt: &MatchStmt) -> CResult<()> {
        let subject = self.expr_any(&stmt.scrutinee)?;
        self.hide(subject);
        let mut ends = Vec::new();
        for arm in &stmt.arms {
            self.set_pos(arm.span);
            self.push_scope();
            let mut fails = Vec::new();
            self.pattern(&arm.pattern, subject, &mut fails)?;
            if let Some(guard) = &arm.guard {
                let mark = self.mark();
                let r = self.expr_any(guard)?;
                fails.push(self.emit(Op::JumpIfFalse { cond: r, offset: 0 }));
                self.release(mark);
            }
            self.stmts(&arm.body.stmts);
            self.pop_scope();
            ends.push(self.emit(Op::Jump { offset: 0 }));
            for f in fails {
                self.patch_here(f);
            }
        }
        if let Some(body) = &stmt.else_body {
            self.block(body);
        }
        for end in ends {
            self.patch_here(end);
        }
        Ok(())
    }

    fn try_finally(
        &mut self,
        body: &[Stmt],
        catches: &[CatchClause],
        finally: Option<Rc<Vec<Stmt>>>,
    ) -> CResult<()> {
        self.push_scope();
        let err = self.alloc()?;
        self.hide(err);
        let begin = self.emit(Op::TryBegin { err, offset: 0 });
        let cleanup = match &finally {
            Some(f) => Cleanup::Finally(f.clone()),
            None => Cleanup::Handler,
        };
        self.f().cleanups.push(cleanup.clone());
        self.push_scope();
        self.stmts(body);
        self.pop_scope();
        self.f().cleanups.pop();
        self.emit(Op::TryEnd);
        self.finally_body(&finally);
        let mut ends = vec![self.emit(Op::Jump { offset: 0 })];

        // Failure path. With both `catch` and `finally`, a failure inside a
        // `catch` must still run `finally`, so the catches get a handler of
        // their own.
        self.patch_here(begin);
        let guard = if finally.is_some() && !catches.is_empty() {
            let inner = self.alloc()?;
            self.hide(inner);
            let at = self.emit(Op::TryBegin {
                err: inner,
                offset: 0,
            });
            Some((inner, at))
        } else {
            None
        };
        for clause in catches {
            self.set_pos(clause.span);
            let next = match &clause.error_ty {
                Some(ty) => {
                    let Some(name) = type_name(ty) else {
                        return Err(self.error("`catch` needs a type name", clause.span));
                    };
                    let n = self.name_index(&name)?;
                    let t = self.alloc()?;
                    self.emit(Op::IsType {
                        dst: t,
                        src: err,
                        name: n,
                    });
                    Some(self.emit(Op::JumpIfFalse { cond: t, offset: 0 }))
                }
                None => None,
            };
            self.push_scope();
            if let Some(binding) = &clause.binding {
                let r = self.alloc()?;
                self.emit(Op::Move { dst: r, src: err });
                self.bind_name(&binding.name, r);
            }
            if guard.is_some() {
                self.f().cleanups.push(cleanup.clone());
            }
            self.stmts(&clause.body.stmts);
            if guard.is_some() {
                self.f().cleanups.pop();
                self.emit(Op::TryEnd);
            }
            self.pop_scope();
            self.finally_body(&finally);
            ends.push(self.emit(Op::Jump { offset: 0 }));
            if let Some(next) = next {
                self.patch_here(next);
            }
        }
        // No `catch` matched: run `finally` and pass the failure on.
        if guard.is_some() {
            self.emit(Op::TryEnd);
        }
        self.finally_body(&finally);
        self.emit(Op::Fail { src: err });
        if let Some((inner, at)) = guard {
            self.patch_here(at);
            self.finally_body(&finally);
            self.emit(Op::Fail { src: inner });
        }
        for end in ends {
            self.patch_here(end);
        }
        self.pop_scope();
        Ok(())
    }

    fn finally_body(&mut self, finally: &Option<Rc<Vec<Stmt>>>) {
        if let Some(stmts) = finally {
            self.push_scope();
            self.stmts(stmts);
            self.pop_scope();
        }
    }

    // ==== patterns =========================================================

    fn literal_reg(&mut self, literal: &LiteralPattern) -> CResult<u8> {
        let r = self.alloc()?;
        match literal {
            LiteralPattern::Int {
                negative, value, ..
            } => {
                let v = i64::try_from(*value)
                    .map_err(|_| self.error("integer literal is too large", self.span))?;
                self.load_int(r, if *negative { -v } else { v })?;
            }
            LiteralPattern::Float {
                negative, value, ..
            } => {
                self.load_const(r, Value::Float(if *negative { -value } else { *value }))?;
            }
            LiteralPattern::Str(text) => self.load_const(r, Value::str(text.clone()))?,
            LiteralPattern::Bool(b) => {
                self.emit(Op::LoadBool { dst: r, value: *b });
            }
            LiteralPattern::None => {
                self.emit(Op::LoadNone { dst: r });
            }
        }
        Ok(r)
    }

    fn fail_unless(&mut self, cond: u8, fails: &mut Vec<usize>) {
        fails.push(self.emit(Op::JumpIfFalse { cond, offset: 0 }));
    }

    /// Emits a test of the value in `value` against `pattern`. Failed tests
    /// jump to the addresses collected in `fails`; names the pattern binds
    /// are declared in the current scope.
    fn pattern(&mut self, pattern: &Pattern, value: u8, fails: &mut Vec<usize>) -> CResult<()> {
        match &pattern.kind {
            PatternKind::Wildcard => {}
            PatternKind::Binding(ident) => {
                let r = self.alloc()?;
                self.emit(Op::Move { dst: r, src: value });
                self.bind_name(&ident.name, r);
            }
            PatternKind::Literal(literal) => {
                let k = self.literal_reg(literal)?;
                let t = self.alloc()?;
                self.emit(Op::Eq {
                    dst: t,
                    a: value,
                    b: k,
                });
                self.fail_unless(t, fails);
            }
            PatternKind::Range {
                start,
                end,
                inclusive,
            } => {
                // A value of another type does not match; it is not an error.
                let t = self.alloc()?;
                if let LiteralPattern::Str(_) = start {
                    let n = self.name_index("str")?;
                    self.emit(Op::IsType {
                        dst: t,
                        src: value,
                        name: n,
                    });
                    self.fail_unless(t, fails);
                } else {
                    let int = self.name_index("int")?;
                    let float = self.name_index("float")?;
                    self.emit(Op::IsType {
                        dst: t,
                        src: value,
                        name: int,
                    });
                    let numeric = self.emit(Op::JumpIfTrue { cond: t, offset: 0 });
                    self.emit(Op::IsType {
                        dst: t,
                        src: value,
                        name: float,
                    });
                    self.fail_unless(t, fails);
                    self.patch_here(numeric);
                }
                let lo = self.literal_reg(start)?;
                self.emit(Op::GtEq {
                    dst: t,
                    a: value,
                    b: lo,
                });
                self.fail_unless(t, fails);
                let hi = self.literal_reg(end)?;
                if *inclusive {
                    self.emit(Op::LtEq {
                        dst: t,
                        a: value,
                        b: hi,
                    });
                } else {
                    self.emit(Op::Lt {
                        dst: t,
                        a: value,
                        b: hi,
                    });
                }
                self.fail_unless(t, fails);
            }
            PatternKind::Value(path) => {
                let r = self.path_value(path)?;
                let t = self.alloc()?;
                self.emit(Op::Eq {
                    dst: t,
                    a: value,
                    b: r,
                });
                self.fail_unless(t, fails);
            }
            PatternKind::Destructure { path, args } => {
                let name = path_name(path);
                let Some(&ty) = self.type_index.get(&name) else {
                    return Err(self.error(
                        format!("`{name}` is not a struct, class or enum variant"),
                        path.span,
                    ));
                };
                let fields: Vec<String> = self.types[ty as usize]
                    .fields
                    .iter()
                    .map(|s| self.symbols.name(*s).to_string())
                    .collect();
                let n = self.name_index(&name)?;
                let t = self.alloc()?;
                self.emit(Op::IsType {
                    dst: t,
                    src: value,
                    name: n,
                });
                self.fail_unless(t, fails);
                if args.len() > fields.len() {
                    return Err(self.error(
                        format!(
                            "`{name}` has {} fields, but the pattern lists {}",
                            fields.len(),
                            args.len()
                        ),
                        pattern.span,
                    ));
                }
                for (i, arg) in args.iter().enumerate() {
                    let slot = match &arg.name {
                        Some(field) => match fields.iter().position(|f| *f == field.name) {
                            Some(slot) => slot,
                            None => {
                                return Err(self.error(
                                    format!("`{name}` has no field `{}`", field.name),
                                    field.span,
                                ))
                            }
                        },
                        None => i,
                    };
                    let f = self.alloc()?;
                    self.emit(Op::GetSlot {
                        dst: f,
                        obj: value,
                        slot: slot as u8,
                    });
                    self.pattern(&arg.pattern, f, fails)?;
                }
            }
            PatternKind::Tuple(items) | PatternKind::List(items) => {
                let kind = if matches!(pattern.kind, PatternKind::Tuple(_)) {
                    "tuple"
                } else {
                    "list"
                };
                let n = self.name_index(kind)?;
                let t = self.alloc()?;
                self.emit(Op::IsType {
                    dst: t,
                    src: value,
                    name: n,
                });
                self.fail_unless(t, fails);
                let rest = items.iter().position(|i| matches!(i, SeqPattern::Rest(..)));
                let fixed = items.len() - usize::from(rest.is_some());
                let len = self.alloc()?;
                self.emit(Op::Len {
                    dst: len,
                    src: value,
                });
                let count = self.alloc()?;
                self.load_int(count, fixed as i64)?;
                if rest.is_some() {
                    self.emit(Op::GtEq {
                        dst: t,
                        a: len,
                        b: count,
                    });
                } else {
                    self.emit(Op::Eq {
                        dst: t,
                        a: len,
                        b: count,
                    });
                }
                self.fail_unless(t, fails);
                let before = rest.unwrap_or(items.len());
                let after = items.len() - before - usize::from(rest.is_some());
                for (i, item) in items.iter().enumerate() {
                    match item {
                        SeqPattern::Item(p) => {
                            let index = if i < before {
                                i as i64
                            } else {
                                -((items.len() - i) as i64)
                            };
                            let k = self.alloc()?;
                            self.load_int(k, index)?;
                            let e = self.alloc()?;
                            self.emit(Op::GetIndex {
                                dst: e,
                                obj: value,
                                index: k,
                            });
                            self.pattern(p, e, fails)?;
                        }
                        SeqPattern::Rest(Some(name), _) => {
                            let bounds = self.alloc_n(3)?;
                            self.load_int(bounds[0], before as i64)?;
                            if after > 0 {
                                self.load_int(bounds[1], -(after as i64))?;
                            } else {
                                self.emit(Op::LoadNone { dst: bounds[1] });
                            }
                            self.emit(Op::LoadNone { dst: bounds[2] });
                            let slice = self.alloc()?;
                            self.emit(Op::MakeSlice {
                                dst: slice,
                                base: bounds[0],
                            });
                            let r = self.alloc()?;
                            self.emit(Op::GetIndex {
                                dst: r,
                                obj: value,
                                index: slice,
                            });
                            self.bind_name(&name.name, r);
                        }
                        SeqPattern::Rest(None, _) => {}
                    }
                }
            }
            PatternKind::Or(alternatives) => {
                if alternatives.iter().any(pattern_binds) {
                    return Err(
                        self.error("patterns joined with `or` cannot bind names", pattern.span)
                    );
                }
                let mut matched = Vec::new();
                for (i, alternative) in alternatives.iter().enumerate() {
                    if i + 1 == alternatives.len() {
                        self.pattern(alternative, value, fails)?;
                    } else {
                        let mut next = Vec::new();
                        self.pattern(alternative, value, &mut next)?;
                        matched.push(self.emit(Op::Jump { offset: 0 }));
                        for f in next {
                            self.patch_here(f);
                        }
                    }
                }
                for m in matched {
                    self.patch_here(m);
                }
            }
            PatternKind::As {
                pattern: inner,
                name,
            } => {
                self.pattern(inner, value, fails)?;
                let r = self.alloc()?;
                self.emit(Op::Move { dst: r, src: value });
                self.bind_name(&name.name, r);
            }
        }
        Ok(())
    }

    /// Loads a dotted name such as `Shape.Empty`.
    fn path_value(&mut self, path: &Path) -> CResult<u8> {
        let first = &path.segments[0];
        let r = self.alloc()?;
        self.name_to(&first.name, first.span, r)?;
        for segment in &path.segments[1..] {
            let n = self.name_index(&segment.name)?;
            self.emit(Op::GetField {
                dst: r,
                obj: r,
                name: n,
            });
        }
        Ok(r)
    }

    // ==== expressions ======================================================

    fn load_int(&mut self, dst: u8, value: i64) -> CResult<()> {
        match i16::try_from(value) {
            Ok(small) => {
                self.emit(Op::LoadInt { dst, value: small });
                Ok(())
            }
            Err(_) => self.load_const(dst, Value::Int(value)),
        }
    }

    /// Compiles `expr` and returns the register holding its value. A local
    /// variable is returned as is, without a copy.
    fn expr_any(&mut self, expr: &Expr) -> CResult<u8> {
        if let Some(r) = self.local_of(expr) {
            return Ok(r);
        }
        let r = self.alloc()?;
        self.expr_to(expr, r)?;
        Ok(r)
    }

    fn name_to(&mut self, name: &str, span: Span, dst: u8) -> CResult<()> {
        match self.resolve(name) {
            Some(Resolved::Local(r)) => {
                if r != dst {
                    self.emit(Op::Move { dst, src: r });
                }
            }
            Some(Resolved::Capture(idx)) => {
                self.emit(Op::GetCapture { dst, idx });
            }
            Some(Resolved::Cell(cell)) => {
                self.emit(Op::GetCell { dst, cell });
            }
            Some(Resolved::UpCell(idx)) => {
                self.emit(Op::GetUpval { dst, idx });
            }
            Some(Resolved::Global(idx)) => {
                self.emit(Op::GetGlobal { dst, idx });
            }
            Some(Resolved::Native(id)) => self.load_const(dst, Value::Native(id))?,
            Some(Resolved::Constant(value)) => self.load_const(dst, value)?,
            None => return Err(self.unknown_name(name, span)),
        }
        Ok(())
    }

    /// Compiles `items` into consecutive registers and returns the first.
    fn sequence(&mut self, items: &[Expr]) -> CResult<(u8, u8)> {
        if items.len() > 250 {
            return Err(self.error(
                "too many items in one literal for the interpreter",
                self.span,
            ));
        }
        let regs = self.alloc_n(items.len())?;
        for (item, &r) in items.iter().zip(&regs) {
            self.expr_to(item, r)?;
        }
        Ok((regs.first().copied().unwrap_or(0), items.len() as u8))
    }

    fn expr_to(&mut self, expr: &Expr, dst: u8) -> CResult<()> {
        if writes_early(expr) && self.is_named_local(dst) {
            let temp = self.alloc()?;
            self.expr_to(expr, temp)?;
            self.emit(Op::Move { dst, src: temp });
            return Ok(());
        }
        let mark = self.mark();
        self.expr_inner(expr, dst)?;
        self.release(mark);
        Ok(())
    }

    fn expr_inner(&mut self, expr: &Expr, dst: u8) -> CResult<()> {
        match &expr.kind {
            ExprKind::Int { value, .. } => {
                let v = i64::try_from(*value)
                    .map_err(|_| self.error("integer literal is too large", expr.span))?;
                self.load_int(dst, v)?;
            }
            ExprKind::Float { value, .. } => self.load_const(dst, Value::Float(*value))?,
            ExprKind::Str(segments) => match segments.as_slice() {
                [] => self.load_const(dst, Value::str(""))?,
                [StrSegment::Text(text)] => self.load_const(dst, Value::str(text.clone()))?,
                _ => {
                    let regs = self.alloc_n(segments.len())?;
                    for (segment, &r) in segments.iter().zip(&regs) {
                        match segment {
                            StrSegment::Text(text) => {
                                self.load_const(r, Value::str(text.clone()))?
                            }
                            StrSegment::Interp(e) => self.expr_to(e, r)?,
                        }
                    }
                    self.emit(Op::Concat {
                        dst,
                        start: regs[0],
                        count: segments.len() as u8,
                    });
                }
            },
            ExprKind::Bool(value) => {
                self.emit(Op::LoadBool { dst, value: *value });
            }
            ExprKind::None => {
                self.emit(Op::LoadNone { dst });
            }
            ExprKind::Name(ident) => self.name_to(&ident.name, expr.span, dst)?,
            ExprKind::Tuple(items) => {
                let (start, count) = self.sequence(items)?;
                self.emit(Op::NewTuple { dst, start, count });
            }
            ExprKind::List(items) => {
                let (start, count) = self.sequence(items)?;
                self.emit(Op::NewList { dst, start, count });
            }
            ExprKind::Map(entries) => {
                self.emit(Op::NewMap { dst });
                for (key, value) in entries {
                    let mark = self.mark();
                    let k = self.expr_any(key)?;
                    let v = self.expr_any(value)?;
                    self.emit(Op::SetIndex {
                        obj: dst,
                        index: k,
                        src: v,
                    });
                    self.release(mark);
                }
            }
            ExprKind::ListComp { element, clauses } => {
                self.emit(Op::NewList {
                    dst,
                    start: 0,
                    count: 0,
                });
                self.comprehension(clauses, 0, &CompBody::List { list: dst, element })?;
            }
            ExprKind::MapComp {
                key,
                value,
                clauses,
            } => {
                self.emit(Op::NewMap { dst });
                self.comprehension(
                    clauses,
                    0,
                    &CompBody::Map {
                        map: dst,
                        key,
                        value,
                    },
                )?;
            }
            ExprKind::Unary { op, operand } => {
                if let (UnaryOp::Neg, ExprKind::Int { value, .. }) = (op, &operand.kind) {
                    if let Ok(v) = i64::try_from(*value) {
                        return self.load_int(dst, -v);
                    }
                }
                let src = self.expr_any(operand)?;
                self.emit(match op {
                    UnaryOp::Neg => Op::Neg { dst, src },
                    UnaryOp::Pos => Op::Pos { dst, src },
                    UnaryOp::BitNot => Op::BitNot { dst, src },
                    UnaryOp::Not => Op::Not { dst, src },
                });
            }
            ExprKind::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                lhs,
                rhs,
            } => {
                self.expr_to(lhs, dst)?;
                let skip = if *op == BinaryOp::And {
                    self.emit(Op::JumpIfFalse {
                        cond: dst,
                        offset: 0,
                    })
                } else {
                    self.emit(Op::JumpIfTrue {
                        cond: dst,
                        offset: 0,
                    })
                };
                self.expr_to(rhs, dst)?;
                self.patch_here(skip);
            }
            ExprKind::Binary {
                op: op @ (BinaryOp::Add | BinaryOp::Sub),
                lhs,
                rhs,
            } if small_int(rhs).is_some() => {
                let imm = small_int(rhs).unwrap_or_default();
                let a = self.expr_any(lhs)?;
                self.emit(if *op == BinaryOp::Add {
                    Op::AddI { dst, a, imm }
                } else {
                    Op::SubI { dst, a, imm }
                });
            }
            ExprKind::Compare { operands, ops }
                if ops.len() == 1
                    && small_int(&operands[1]).is_some()
                    && !matches!(ops[0], CompareOp::In | CompareOp::NotIn) =>
            {
                let imm = small_int(&operands[1]).unwrap_or_default();
                let a = self.expr_any(&operands[0])?;
                self.emit(match ops[0] {
                    CompareOp::Lt => Op::LtI { dst, a, imm },
                    CompareOp::LtEq => Op::LtEqI { dst, a, imm },
                    CompareOp::Gt => Op::GtI { dst, a, imm },
                    CompareOp::GtEq => Op::GtEqI { dst, a, imm },
                    CompareOp::Eq => Op::EqI { dst, a, imm },
                    _ => Op::NotEqI { dst, a, imm },
                });
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.expr_any(lhs)?;
                let b = self.expr_any(rhs)?;
                self.emit(match op {
                    BinaryOp::Add => Op::Add { dst, a, b },
                    BinaryOp::Sub => Op::Sub { dst, a, b },
                    BinaryOp::Mul => Op::Mul { dst, a, b },
                    BinaryOp::Div => Op::Div { dst, a, b },
                    BinaryOp::FloorDiv => Op::FloorDiv { dst, a, b },
                    BinaryOp::Rem => Op::Rem { dst, a, b },
                    BinaryOp::Pow => Op::Pow { dst, a, b },
                    BinaryOp::BitAnd => Op::BitAnd { dst, a, b },
                    BinaryOp::BitOr => Op::BitOr { dst, a, b },
                    BinaryOp::BitXor => Op::BitXor { dst, a, b },
                    BinaryOp::Shl => Op::Shl { dst, a, b },
                    BinaryOp::Shr => Op::Shr { dst, a, b },
                    BinaryOp::And | BinaryOp::Or => unreachable!("handled above"),
                });
            }
            ExprKind::Compare { operands, ops } => {
                let mut a = self.expr_any(&operands[0])?;
                let mut ends = Vec::new();
                for (i, op) in ops.iter().enumerate() {
                    let b = self.expr_any(&operands[i + 1])?;
                    self.emit_compare(*op, dst, a, b);
                    if i + 1 < ops.len() {
                        ends.push(self.emit(Op::JumpIfFalse {
                            cond: dst,
                            offset: 0,
                        }));
                    }
                    a = b;
                }
                for end in ends {
                    self.patch_here(end);
                }
            }
            ExprKind::Range {
                start,
                end,
                inclusive,
                step,
            } => {
                let regs = self.alloc_n(3)?;
                self.expr_to(start, regs[0])?;
                self.expr_to(end, regs[1])?;
                match step {
                    Some(step) => self.expr_to(step, regs[2])?,
                    None => {
                        self.emit(Op::LoadInt {
                            dst: regs[2],
                            value: 1,
                        });
                    }
                }
                self.emit(Op::MakeRange {
                    dst,
                    base: regs[0],
                    inclusive: *inclusive,
                });
            }
            ExprKind::Coalesce { lhs, rhs } => {
                self.expr_to(lhs, dst)?;
                let skip = self.emit(Op::JumpIfNotNone {
                    src: dst,
                    offset: 0,
                });
                self.expr_to(rhs, dst)?;
                self.patch_here(skip);
            }
            ExprKind::Pipeline { lhs, rhs } => {
                // `x |> f(a)` calls `f(x, a)`; `x |> f` calls `f(x)`.
                let first = CallArg {
                    name: None,
                    is_mut: false,
                    expr: lhs,
                };
                match &rhs.kind {
                    ExprKind::Call { callee, args } => {
                        let mut list = vec![first];
                        list.extend(args.iter().map(|a| CallArg {
                            name: a.name.as_ref().map(|n| n.name.as_str()),
                            is_mut: a.is_mut,
                            expr: &a.value,
                        }));
                        self.call(callee, list, dst)?;
                    }
                    _ => self.call(rhs, vec![first], dst)?,
                }
            }
            ExprKind::MatMul { lhs, rhs } => {
                let a = self.expr_any(lhs)?;
                let b = self.expr_any(rhs)?;
                self.emit(Op::MatMul { dst, a, b });
            }
            ExprKind::Placement { value, target, .. } => {
                let src = self.expr_any(value)?;
                let target = self.expr_any(target)?;
                self.emit(Op::Place { dst, src, target });
            }
            ExprKind::Index { base, subscripts } => {
                let obj = self.expr_any(base)?;
                let index = self.index_key(subscripts)?;
                self.emit(Op::GetIndex { dst, obj, index });
            }
            ExprKind::Field { .. } if self.module_constant(expr).is_some() => {
                let value = self.module_constant(expr).unwrap_or(Value::None);
                self.load_const(dst, value)?;
            }
            ExprKind::Field { .. } if self.module_member(expr).is_some() => {
                match self.module_member(expr) {
                    Some(Ok(id)) => self.load_const(dst, Value::Native(id))?,
                    Some(Err((message, span))) => return Err(self.error(message, span)),
                    None => {}
                }
            }
            ExprKind::Field {
                base,
                name,
                optional,
            } => {
                let obj = self.expr_any(base)?;
                let n = self.name_index(&name.name)?;
                if *optional {
                    let present = self.emit(Op::JumpIfNotNone {
                        src: obj,
                        offset: 0,
                    });
                    self.emit(Op::LoadNone { dst });
                    let done = self.emit(Op::Jump { offset: 0 });
                    self.patch_here(present);
                    self.emit(Op::GetField { dst, obj, name: n });
                    self.patch_here(done);
                } else {
                    self.emit(Op::GetField { dst, obj, name: n });
                }
            }
            ExprKind::Call { callee, args } => {
                let list = args
                    .iter()
                    .map(|a| CallArg {
                        name: a.name.as_ref().map(|n| n.name.as_str()),
                        is_mut: a.is_mut,
                        expr: &a.value,
                    })
                    .collect();
                self.call(callee, list, dst)?;
            }
            ExprKind::Await(inner) => {
                let src = self.expr_any(inner)?;
                self.emit(Op::Await { dst, src });
            }
            ExprKind::Spawn(inner) => {
                let src = self.expr_any(inner)?;
                self.emit(Op::Spawn { dst, src });
            }
            // A failure already travels to the nearest handler, so `try e`
            // only marks the call; `move` matters to the borrow checker,
            // not to the interpreter.
            ExprKind::Try(inner) | ExprKind::Move(inner) => self.expr_to(inner, dst)?,
            ExprKind::If {
                then_value,
                condition,
                else_value,
            } => {
                let c = self.expr_any(condition)?;
                let otherwise = self.emit(Op::JumpIfFalse { cond: c, offset: 0 });
                self.expr_to(then_value, dst)?;
                let done = self.emit(Op::Jump { offset: 0 });
                self.patch_here(otherwise);
                self.expr_to(else_value, dst)?;
                self.patch_here(done);
            }
            ExprKind::Lambda {
                params,
                body,
                is_async,
            } => {
                let list: Vec<(String, Option<&Expr>)> = params
                    .iter()
                    .map(|p| (p.name.name.clone(), p.default.as_ref()))
                    .collect();
                let mut cells = HashSet::new();
                scan_expr(body, false, &mut cells);
                for (_, default) in &list {
                    if let Some(d) = default {
                        scan_expr(d, false, &mut cells);
                    }
                }
                self.begin_fn("<lambda>".into(), &list, cells);
                if let Ok(r) = self.expr_any(body) {
                    self.emit(Op::Return { src: r });
                }
                let mut proto = self.end_fn();
                proto.is_async = *is_async;
                let index = self.add_proto(proto)?;
                self.emit(Op::MakeClosure { dst, proto: index });
            }
        }
        Ok(())
    }

    fn emit_compare(&mut self, op: CompareOp, dst: u8, a: u8, b: u8) {
        match op {
            CompareOp::Eq => self.emit(Op::Eq { dst, a, b }),
            CompareOp::NotEq => self.emit(Op::NotEq { dst, a, b }),
            CompareOp::Lt => self.emit(Op::Lt { dst, a, b }),
            CompareOp::LtEq => self.emit(Op::LtEq { dst, a, b }),
            CompareOp::Gt => self.emit(Op::Gt { dst, a, b }),
            CompareOp::GtEq => self.emit(Op::GtEq { dst, a, b }),
            CompareOp::In => self.emit(Op::Contains {
                dst,
                item: a,
                container: b,
            }),
            CompareOp::NotIn => {
                self.emit(Op::Contains {
                    dst,
                    item: a,
                    container: b,
                });
                self.emit(Op::Not { dst, src: dst })
            }
        };
    }

    /// Compiles subscripts into a single index value: the value itself for
    /// one subscript, a tuple for several.
    fn index_key(&mut self, subscripts: &[Subscript]) -> CResult<u8> {
        if let [Subscript::Index(e)] = subscripts {
            return self.expr_any(e);
        }
        let regs = self.alloc_n(subscripts.len())?;
        for (subscript, &r) in subscripts.iter().zip(&regs) {
            match subscript {
                Subscript::Index(e) => self.expr_to(e, r)?,
                Subscript::Slice {
                    start, stop, step, ..
                } => {
                    let mark = self.mark();
                    let bounds = self.alloc_n(3)?;
                    for (part, &b) in [start, stop, step].into_iter().zip(&bounds) {
                        match part {
                            Some(e) => self.expr_to(e, b)?,
                            None => {
                                self.emit(Op::LoadNone { dst: b });
                            }
                        }
                    }
                    self.emit(Op::MakeSlice {
                        dst: r,
                        base: bounds[0],
                    });
                    self.release(mark);
                }
                Subscript::Ellipsis(_) => self.load_const(r, Value::Ellipsis)?,
            }
        }
        if regs.len() == 1 {
            return Ok(regs[0]);
        }
        let key = self.alloc()?;
        self.emit(Op::NewTuple {
            dst: key,
            start: regs[0],
            count: regs.len() as u8,
        });
        Ok(key)
    }

    fn comprehension(&mut self, clauses: &[CompClause], i: usize, body: &CompBody) -> CResult<()> {
        let Some(clause) = clauses.get(i) else {
            let mark = self.mark();
            match body {
                CompBody::List { list, element } => {
                    let v = self.expr_any(element)?;
                    self.emit(Op::ListPush {
                        list: *list,
                        src: v,
                    });
                }
                CompBody::Map { map, key, value } => {
                    let k = self.expr_any(key)?;
                    let v = self.expr_any(value)?;
                    self.emit(Op::SetIndex {
                        obj: *map,
                        index: k,
                        src: v,
                    });
                }
            }
            self.release(mark);
            return Ok(());
        };
        match clause {
            CompClause::For { target, iter } => {
                let lp = self.begin_iteration(target, iter)?;
                self.comprehension(clauses, i + 1, body)?;
                self.end_iteration(lp);
            }
            CompClause::If(condition) => {
                let mark = self.mark();
                let c = self.expr_any(condition)?;
                let skip = self.emit(Op::JumpIfFalse { cond: c, offset: 0 });
                self.release(mark);
                self.comprehension(clauses, i + 1, body)?;
                self.patch_here(skip);
            }
        }
        Ok(())
    }

    // ==== calls ============================================================

    fn signature_of(&mut self, callee: &Expr) -> Option<Signature> {
        match &callee.kind {
            ExprKind::Name(ident) => match self.resolve(&ident.name)? {
                Resolved::Global(_) => match self.global_index.get(&ident.name)? {
                    GlobalKind::Function { sig, .. } | GlobalKind::Struct { sig, .. } => {
                        Some(sig.clone())
                    }
                    _ => None,
                },
                Resolved::Native(id) => {
                    let native = &builtins::NATIVES[id as usize];
                    (!native.variadic).then(|| Signature {
                        params: native.params.iter().map(|p| p.to_string()).collect(),
                        required: native.required,
                    })
                }
                _ => None,
            },
            ExprKind::Field { .. } if matches!(self.module_member(callee), Some(Ok(_))) => {
                let Some(Ok(id)) = self.module_member(callee) else {
                    return None;
                };
                let native = &builtins::NATIVES[id as usize];
                (!native.variadic).then(|| Signature {
                    params: native.params.iter().map(|p| p.to_string()).collect(),
                    required: native.required,
                })
            }
            ExprKind::Field { base, name, .. } => {
                let enum_name = match &base.kind {
                    ExprKind::Name(ident) => ident.name.clone(),
                    _ => return None,
                };
                if !self.is_enum(&enum_name) {
                    return None;
                }
                let ty = *self.type_index.get(&format!("{enum_name}.{}", name.name))?;
                let fields: Vec<String> = self.types[ty as usize]
                    .fields
                    .iter()
                    .map(|s| self.symbols.name(*s).to_string())
                    .collect();
                let required = fields.len();
                Some(Signature {
                    params: fields,
                    required,
                })
            }
            _ => None,
        }
    }

    fn is_enum(&mut self, name: &str) -> bool {
        matches!(self.resolve(name), Some(Resolved::Global(_)))
            && matches!(self.global_index.get(name), Some(GlobalKind::Enum { .. }))
    }

    fn callee_name(callee: &Expr) -> String {
        match &callee.kind {
            ExprKind::Name(ident) => ident.name.clone(),
            ExprKind::Field { name, .. } => name.name.clone(),
            _ => "this function".into(),
        }
    }

    /// Matches positional and keyword arguments to parameters. Parameters
    /// left out are `None`, which compiles to `LoadMissing` so the callee
    /// uses its default.
    fn order_args<'a>(
        &mut self,
        callee: &Expr,
        args: Vec<CallArg<'a>>,
        sig: Option<Signature>,
    ) -> CResult<Vec<Option<CallArg<'a>>>> {
        let has_keywords = args.iter().any(|a| a.name.is_some());
        let name = Self::callee_name(callee);
        let Some(sig) = sig else {
            if has_keywords {
                return Err(self.error_hint(
                    "keyword arguments need a function the compiler can see",
                    callee.span,
                    "call a top-level function, a struct or a builtin by name, or pass the arguments by position",
                ));
            }
            return Ok(args.into_iter().map(Some).collect());
        };
        let mut slots: Vec<Option<CallArg>> = vec![None; sig.params.len()];
        let mut seen_keyword = false;
        let mut position = 0;
        for arg in args {
            match arg.name {
                None => {
                    if seen_keyword {
                        return Err(self.error(
                            "positional arguments must come before keyword arguments",
                            arg.expr.span,
                        ));
                    }
                    if position >= sig.params.len() {
                        return Err(self.error(
                            format!(
                                "`{name}` takes {} argument{}, but more were given",
                                sig.params.len(),
                                if sig.params.len() == 1 { "" } else { "s" }
                            ),
                            arg.expr.span,
                        ));
                    }
                    slots[position] = Some(arg);
                    position += 1;
                }
                Some(keyword) => {
                    seen_keyword = true;
                    let Some(index) = sig.params.iter().position(|p| p == keyword) else {
                        let hint = format!("its parameters are: {}", sig.params.join(", "));
                        return Err(self.error_hint(
                            format!("`{name}` has no parameter `{keyword}`"),
                            arg.expr.span,
                            hint,
                        ));
                    };
                    if slots[index].is_some() {
                        return Err(
                            self.error(format!("`{keyword}` is given twice"), arg.expr.span)
                        );
                    }
                    slots[index] = Some(arg);
                }
            }
        }
        if let Some(missing) = (0..sig.required).find(|&i| slots[i].is_none()) {
            return Err(self.error(
                format!("`{name}` is missing the argument `{}`", sig.params[missing]),
                callee.span,
            ));
        }
        while matches!(slots.last(), Some(None)) {
            slots.pop();
        }
        Ok(slots)
    }

    fn call(&mut self, callee: &Expr, args: Vec<CallArg>, dst: u8) -> CResult<()> {
        if let ExprKind::Field {
            base,
            name,
            optional,
        } = &callee.kind
        {
            if *optional {
                // `x?.method()` is `none` when `x` is, without calling.
                let receiver = self.expr_any(base)?;
                let present = self.emit(Op::JumpIfNotNone {
                    src: receiver,
                    offset: 0,
                });
                self.emit(Op::LoadNone { dst });
                let done = self.emit(Op::Jump { offset: 0 });
                self.patch_here(present);
                self.method_call(base, Some(receiver), &name.name, args, dst)?;
                self.patch_here(done);
                return Ok(());
            }
            let is_namespace = matches!(&base.kind, ExprKind::Name(ident) if self.is_enum(&ident.name))
                || self.module_member(callee).is_some();
            if !is_namespace {
                return self.method_call(base, None, &name.name, args, dst);
            }
        }
        let sig = self.signature_of(callee);
        if sig.is_none() && args.iter().any(|a| a.name.is_some()) {
            return self.call_with_keywords(callee, None, None, args, dst);
        }
        let ordered = self.order_args(callee, args, sig)?;
        if ordered.len() > 250 {
            return Err(self.error("too many arguments for the interpreter", callee.span));
        }
        // A result headed for the newest temporary can be computed in place:
        // the function goes there and the arguments above it, saving a move.
        // Never for a variable, whose old value the arguments may read.
        let in_place = dst as usize + 1 == self.f().next_reg && !self.is_bound(dst);
        let func = if in_place { dst } else { self.alloc()? };
        let regs = self.alloc_n(ordered.len())?;
        self.expr_to(callee, func)?;
        for (arg, &r) in ordered.iter().zip(&regs) {
            match arg {
                Some(arg) => self.expr_to(arg.expr, r)?,
                None => {
                    self.emit(Op::LoadMissing { dst: r });
                }
            }
        }
        self.emit(Op::Call {
            func,
            argc: ordered.len() as u8,
        });
        // A `mut` argument may have been changed by the callee, which works
        // on the very registers the arguments were passed in.
        for (arg, &r) in ordered.iter().zip(&regs) {
            if let Some(arg) = arg {
                if arg.is_mut && self.is_place(arg.expr) {
                    self.store_place(arg.expr, r)?;
                }
            }
        }
        if dst != func {
            self.emit(Op::Move { dst, src: func });
        }
        Ok(())
    }

    /// Calls method `name` on `receiver`. `evaluated` holds the receiver if
    /// the caller has already computed it.
    /// A call whose keyword arguments are matched to parameters when it
    /// runs, because the compiler cannot see the callee: a closure, a value
    /// passed in, or a method. Positional arguments come first, then the
    /// keyword arguments, then a tuple of their names.
    fn call_with_keywords(
        &mut self,
        callee: &Expr,
        receiver: Option<(&Expr, Option<u8>)>,
        method: Option<&str>,
        args: Vec<CallArg>,
        dst: u8,
    ) -> CResult<()> {
        let mut seen_keyword = false;
        for arg in &args {
            if arg.name.is_some() {
                seen_keyword = true;
            } else if seen_keyword {
                return Err(self.error(
                    "positional arguments must come before keyword arguments",
                    arg.expr.span,
                ));
            }
        }
        if args.len() > 249 {
            return Err(self.error("too many arguments for the interpreter", callee.span));
        }
        let names: Vec<Value> = args.iter().filter_map(|a| a.name.map(Value::str)).collect();
        let func = self.alloc()?;
        let recv = match receiver {
            Some(_) => Some(self.alloc()?),
            None => None,
        };
        let regs = self.alloc_n(args.len() + 1)?;
        match (receiver, recv) {
            (Some((expr, evaluated)), Some(r)) => match evaluated {
                Some(value) => {
                    self.emit(Op::Move { dst: r, src: value });
                }
                None => self.expr_to(expr, r)?,
            },
            _ => self.expr_to(callee, func)?,
        }
        for (arg, &r) in args.iter().zip(&regs) {
            self.expr_to(arg.expr, r)?;
        }
        self.load_const(regs[args.len()], Value::tuple(names))?;
        let argc = args.len() as u8;
        match method {
            Some(name) => {
                let n = self.name_index(name)?;
                self.emit(Op::CallMethodKw {
                    base: func,
                    argc,
                    name: n,
                });
            }
            None => {
                self.emit(Op::CallKw { func, argc });
            }
        }
        if let (Some((expr, _)), Some(r)) = (receiver, recv) {
            if self.is_place(expr) {
                self.store_place(expr, r)?;
            }
        }
        // `mut` is only allowed on positional arguments, which keep their
        // registers.
        for (arg, &r) in args.iter().zip(&regs) {
            if arg.is_mut && arg.name.is_none() && self.is_place(arg.expr) {
                self.store_place(arg.expr, r)?;
            }
        }
        if dst != func {
            self.emit(Op::Move { dst, src: func });
        }
        Ok(())
    }

    fn method_call(
        &mut self,
        receiver: &Expr,
        evaluated: Option<u8>,
        name: &str,
        args: Vec<CallArg>,
        dst: u8,
    ) -> CResult<()> {
        if args.iter().any(|a| a.name.is_some()) {
            return self.call_with_keywords(
                receiver,
                Some((receiver, evaluated)),
                Some(name),
                args,
                dst,
            );
        }
        if args.len() > 250 {
            return Err(self.error("too many arguments for the interpreter", receiver.span));
        }
        let base = self.alloc()?;
        let recv = self.alloc()?;
        let regs = self.alloc_n(args.len())?;
        match evaluated {
            Some(r) => {
                self.emit(Op::Move { dst: recv, src: r });
            }
            None => self.expr_to(receiver, recv)?,
        }
        for (arg, &r) in args.iter().zip(&regs) {
            self.expr_to(arg.expr, r)?;
        }
        let n = self.name_index(name)?;
        self.emit(Op::CallMethod {
            base,
            argc: args.len() as u8,
            name: n,
        });
        // A method with `mut self` changes the receiver in place; write it
        // back to where it came from. For reference types this is a no-op.
        if self.is_place(receiver) {
            self.store_place(receiver, recv)?;
        }
        for (arg, &r) in args.iter().zip(&regs) {
            if arg.is_mut && self.is_place(arg.expr) {
                self.store_place(arg.expr, r)?;
            }
        }
        if dst != base {
            self.emit(Op::Move { dst, src: base });
        }
        Ok(())
    }
}

// ==== variables captured by closures ===========================================

/// Names used inside the closures and nested functions of a function body.
/// Variables of the function with these names are held in shared cells.
/// Over-approximating is harmless: a cell that is never shared costs one
/// indirection.
fn nested_names(stmts: &[Stmt], params: &[(String, Option<&Expr>)]) -> HashSet<String> {
    let mut names = HashSet::new();
    for stmt in stmts {
        scan_stmt(stmt, false, &mut names);
    }
    for (_, default) in params {
        if let Some(d) = default {
            scan_expr(d, false, &mut names);
        }
    }
    names
}

fn scan_block(block: &Block, inside: bool, names: &mut HashSet<String>) {
    for stmt in &block.stmts {
        scan_stmt(stmt, inside, names);
    }
}

fn scan_condition(condition: &Condition, inside: bool, names: &mut HashSet<String>) {
    match condition {
        Condition::Expr(e) => scan_expr(e, inside, names),
        Condition::Let { pattern, value } => {
            scan_pattern(pattern, inside, names);
            scan_expr(value, inside, names);
        }
    }
}

fn scan_pattern(pattern: &Pattern, inside: bool, names: &mut HashSet<String>) {
    match &pattern.kind {
        PatternKind::Value(path) if inside => {
            names.insert(path.segments[0].name.clone());
        }
        PatternKind::Destructure { args, .. } => {
            for arg in args {
                scan_pattern(&arg.pattern, inside, names);
            }
        }
        PatternKind::Tuple(items) | PatternKind::List(items) => {
            for item in items {
                if let SeqPattern::Item(p) = item {
                    scan_pattern(p, inside, names);
                }
            }
        }
        PatternKind::Or(alternatives) => {
            for p in alternatives {
                scan_pattern(p, inside, names);
            }
        }
        PatternKind::As { pattern, .. } => scan_pattern(pattern, inside, names),
        _ => {}
    }
}

fn scan_stmt(stmt: &Stmt, inside: bool, names: &mut HashSet<String>) {
    match &stmt.kind {
        StmtKind::Binding(b) => {
            if let Some(v) = &b.value {
                scan_expr(v, inside, names);
            }
        }
        StmtKind::Expr(e) | StmtKind::Fail(e) | StmtKind::Defer(e) => scan_expr(e, inside, names),
        StmtKind::Assign { target, value, .. } => {
            scan_expr(target, inside, names);
            scan_expr(value, inside, names);
        }
        StmtKind::Return(v) => {
            if let Some(v) = v {
                scan_expr(v, inside, names);
            }
        }
        StmtKind::Stop | StmtKind::Skip => {}
        StmtKind::If(i) => {
            for (condition, body) in &i.branches {
                scan_condition(condition, inside, names);
                scan_block(body, inside, names);
            }
            if let Some(body) = &i.else_body {
                scan_block(body, inside, names);
            }
        }
        StmtKind::While { condition, body } => {
            scan_condition(condition, inside, names);
            scan_block(body, inside, names);
        }
        StmtKind::Loop(body) | StmtKind::Unsafe(body) => scan_block(body, inside, names),
        StmtKind::For(f) => {
            scan_expr(&f.iter, inside, names);
            scan_block(&f.body, inside, names);
        }
        StmtKind::Match(m) => {
            scan_expr(&m.scrutinee, inside, names);
            for arm in &m.arms {
                scan_pattern(&arm.pattern, inside, names);
                if let Some(g) = &arm.guard {
                    scan_expr(g, inside, names);
                }
                scan_block(&arm.body, inside, names);
            }
            if let Some(body) = &m.else_body {
                scan_block(body, inside, names);
            }
        }
        StmtKind::Try(t) => {
            scan_block(&t.body, inside, names);
            for c in &t.catches {
                scan_block(&c.body, inside, names);
            }
            if let Some(body) = &t.finally {
                scan_block(body, inside, names);
            }
        }
        StmtKind::With { items, body, .. } => {
            for item in items {
                scan_expr(&item.value, inside, names);
            }
            scan_block(body, inside, names);
        }
        StmtKind::On(on) => {
            scan_expr(&on.target, inside, names);
            scan_block(&on.body, inside, names);
        }
        StmtKind::Region(region) => scan_block(&region.body, inside, names),
        StmtKind::Item(item) => {
            if let ItemKind::Fn(f) = &item.kind {
                for p in &f.params {
                    if let Some(d) = &p.default {
                        scan_expr(d, true, names);
                    }
                }
                if let FnBody::Block(body) = &f.body {
                    scan_block(body, true, names);
                }
            }
        }
    }
}

fn scan_expr(expr: &Expr, inside: bool, names: &mut HashSet<String>) {
    match &expr.kind {
        ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::Bool(_) | ExprKind::None => {}
        ExprKind::Name(ident) => {
            if inside {
                names.insert(ident.name.clone());
            }
        }
        ExprKind::Str(segments) => {
            for segment in segments {
                if let StrSegment::Interp(e) = segment {
                    scan_expr(e, inside, names);
                }
            }
        }
        ExprKind::Tuple(items) | ExprKind::List(items) => {
            for item in items {
                scan_expr(item, inside, names);
            }
        }
        ExprKind::Map(entries) => {
            for (k, v) in entries {
                scan_expr(k, inside, names);
                scan_expr(v, inside, names);
            }
        }
        ExprKind::ListComp { element, clauses } => {
            scan_expr(element, inside, names);
            scan_clauses(clauses, inside, names);
        }
        ExprKind::MapComp {
            key,
            value,
            clauses,
        } => {
            scan_expr(key, inside, names);
            scan_expr(value, inside, names);
            scan_clauses(clauses, inside, names);
        }
        ExprKind::Unary { operand, .. } => scan_expr(operand, inside, names),
        ExprKind::Binary { lhs, rhs, .. }
        | ExprKind::Coalesce { lhs, rhs }
        | ExprKind::Pipeline { lhs, rhs }
        | ExprKind::MatMul { lhs, rhs } => {
            scan_expr(lhs, inside, names);
            scan_expr(rhs, inside, names);
        }
        ExprKind::Compare { operands, .. } => {
            for operand in operands {
                scan_expr(operand, inside, names);
            }
        }
        ExprKind::Range {
            start, end, step, ..
        } => {
            scan_expr(start, inside, names);
            scan_expr(end, inside, names);
            if let Some(s) = step {
                scan_expr(s, inside, names);
            }
        }
        ExprKind::Placement { value, target, .. } => {
            scan_expr(value, inside, names);
            scan_expr(target, inside, names);
        }
        ExprKind::Index { base, subscripts } => {
            scan_expr(base, inside, names);
            for sub in subscripts {
                match sub {
                    Subscript::Index(e) => scan_expr(e, inside, names),
                    Subscript::Slice {
                        start, stop, step, ..
                    } => {
                        for part in [start, stop, step].into_iter().flatten() {
                            scan_expr(part, inside, names);
                        }
                    }
                    Subscript::Ellipsis(_) => {}
                }
            }
        }
        ExprKind::Field { base, .. } => scan_expr(base, inside, names),
        ExprKind::Call { callee, args } => {
            scan_expr(callee, inside, names);
            for a in args {
                scan_expr(&a.value, inside, names);
            }
        }
        ExprKind::Await(e) | ExprKind::Spawn(e) | ExprKind::Try(e) | ExprKind::Move(e) => {
            scan_expr(e, inside, names)
        }
        ExprKind::If {
            then_value,
            condition,
            else_value,
        } => {
            scan_expr(then_value, inside, names);
            scan_expr(condition, inside, names);
            scan_expr(else_value, inside, names);
        }
        ExprKind::Lambda { params, body, .. } => {
            for p in params {
                if let Some(d) = &p.default {
                    scan_expr(d, true, names);
                }
            }
            scan_expr(body, true, names);
        }
    }
}

fn scan_clauses(clauses: &[CompClause], inside: bool, names: &mut HashSet<String>) {
    for clause in clauses {
        match clause {
            CompClause::For { iter, .. } => scan_expr(iter, inside, names),
            CompClause::If(e) => scan_expr(e, inside, names),
        }
    }
}

/// An integer literal small enough to be an instruction's operand.
fn small_int(expr: &Expr) -> Option<i8> {
    match &expr.kind {
        ExprKind::Int { value, .. } => i8::try_from(*value).ok(),
        _ => None,
    }
}

/// A C function's signature in the form the FFI builtin reads:
/// `f64(f64,ptr)`. Types are spelled as C sees them.
fn c_signature(f: &FnDecl) -> Result<String, (String, Span)> {
    fn c_type(t: &TypeExpr) -> Option<&'static str> {
        match &t.kind {
            TypeExprKind::Named { path, .. } if path.segments.len() == 1 => {
                Some(match path.segments[0].name.as_str() {
                    "i8" => "i8",
                    "i16" => "i16",
                    "i32" => "i32",
                    "i64" | "int" | "isize" => "i64",
                    "u8" => "u8",
                    "u16" => "u16",
                    "u32" => "u32",
                    "u64" | "usize" => "u64",
                    "f32" => "f32",
                    "f64" | "float" => "f64",
                    "bool" => "bool",
                    "str" => "str",
                    _ => return None,
                })
            }
            TypeExprKind::Owned {
                ownership: Ownership::Ptr,
                ..
            } => Some("ptr"),
            TypeExprKind::Tensor(_) | TypeExprKind::List(_) | TypeExprKind::Array { .. } => {
                Some("ptr")
            }
            TypeExprKind::Tuple(items) if items.is_empty() => Some("void"),
            _ => None,
        }
    }
    let mut params = Vec::new();
    for p in &f.params {
        let Some(t) = &p.declared_ty else {
            return Err((
                format!(
                    "parameter `{}` of C function `{}` needs a type",
                    p.name.name, f.name.name
                ),
                p.name.span,
            ));
        };
        match c_type(t) {
            Some("void") | None => {
                return Err((
                    format!(
                        "parameter `{}` has a type that cannot be passed to C",
                        p.name.name
                    ),
                    t.span,
                ))
            }
            Some(c) => params.push(c),
        }
    }
    let result = match &f.result.returns {
        None => "void",
        Some(t) => match c_type(t) {
            Some(c) if c != "str" => c,
            _ => {
                return Err((
                    format!(
                        "`{}` returns a type that cannot come back from C",
                        f.name.name
                    ),
                    t.span,
                ))
            }
        },
    };
    Ok(format!("{result}({})", params.join(",")))
}
