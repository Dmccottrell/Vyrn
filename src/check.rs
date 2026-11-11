//! Semantic checks that run before code generation.
//!
//! - **Gradual types.** Annotated bindings, parameters, returns and struct
//!   fields are checked against the types the checker can infer. Anything
//!   it cannot infer is `any` and never reported, so unannotated code runs
//!   as dynamically as Python while annotated code gets static errors.
//! - **`let`.** A `let` binding cannot be reassigned, passed as `mut`, or
//!   changed through a field or a `mut self` method when it holds a struct
//!   (a value type).
//! - **`strict` and `kernel`.** Parameters need types, `any` is not allowed,
//!   variables are declared with `let` or `var`, and parameters not taken as
//!   `mut` are read-only.
//! - **Borrows.** A value cannot be used after `move`, a variable cannot be
//!   passed as `mut` together with any other use of it in the same call,
//!   arguments to a `mut` parameter must say `mut`, and the list a
//!   `for mut` loop is changing cannot be used inside that loop.
//!
//! Only definite errors are reported: when a rule depends on something the
//! checker does not know, it stays quiet and the program is checked at run
//! time as before.

use std::collections::{HashMap, HashSet};

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

/// Checks a parsed module. Returns every problem found, in source order.
pub fn check(module: &Module) -> Vec<Diagnostic> {
    let mut table = Table::default();
    table.collect(&module.items);
    let mut checker = Checker {
        table: &table,
        scopes: Vec::new(),
        fns: Vec::new(),
        borrowed: Vec::new(),
        unsafe_depth: 0,
        diagnostics: Vec::new(),
    };
    checker.items(&module.items);
    let mut seen = HashSet::new();
    let mut diagnostics: Vec<Diagnostic> = checker
        .diagnostics
        .into_iter()
        .filter(|d| seen.insert((d.span.start.offset, d.message.clone())))
        .collect();
    diagnostics.sort_by_key(|d| d.span.start.offset);
    diagnostics
}

// ==== types ================================================================

#[derive(Clone, Debug, PartialEq)]
pub enum Ty {
    /// Unknown or dynamic: compatible with everything.
    Any,
    Int,
    Float,
    Bool,
    Str,
    None,
    Tensor,
    Range,
    List(Box<Ty>),
    Map(Box<Ty>, Box<Ty>),
    Tuple(Vec<Ty>),
    Optional(Box<Ty>),
    /// A struct, class or enum declared in this module.
    Named(String),
    /// A pointer from C: an opaque handle.
    Ptr,
    /// A generic function's type parameter, `T` in `fn first[T](xs: [T]) -> T`.
    Var(String),
}

impl Ty {
    pub fn name(&self) -> String {
        match self {
            Ty::Any => "any".into(),
            Ty::Int => "int".into(),
            Ty::Float => "float".into(),
            Ty::Bool => "bool".into(),
            Ty::Str => "str".into(),
            Ty::None => "none".into(),
            Ty::Tensor => "tensor".into(),
            Ty::Range => "range".into(),
            Ty::List(t) => format!("[{}]", t.name()),
            Ty::Map(k, v) => format!("{{{}: {}}}", k.name(), v.name()),
            Ty::Tuple(items) => format!(
                "({})",
                items.iter().map(Ty::name).collect::<Vec<_>>().join(", ")
            ),
            Ty::Optional(t) => format!("{}?", t.name()),
            Ty::Named(n) => n.clone(),
            Ty::Ptr => "ptr".into(),
            Ty::Var(n) => n.clone(),
        }
    }

    fn is_scalar(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::Bool | Ty::Str)
    }
}

// ==== what the module declares =============================================

#[derive(Clone, Debug)]
struct ParamInfo {
    name: String,
    ty: Ty,
    pass: PassMode,
}

#[derive(Clone, Debug)]
struct FnSig {
    /// A `kernel fn`: its number parameters also take tensors, and then it
    /// returns a tensor.
    elementwise: bool,
    params: Vec<ParamInfo>,
    /// `None` when the return type is not written.
    ret: Option<Ty>,
}

#[derive(Debug, Default)]
struct TypeInfo {
    is_struct: bool,
    fields: Vec<(String, Ty)>,
    /// Method name and its signature, `self` included.
    methods: HashMap<String, FnSig>,
    parent: Option<String>,
}

#[derive(Default)]
struct Table {
    fns: HashMap<String, FnSig>,
    types: HashMap<String, TypeInfo>,
    enums: HashSet<String>,
    /// Each enum's variants, as patterns name them: `Shape.Circle`.
    variants: HashMap<String, Vec<String>>,
    consts: HashMap<String, Ty>,
    /// Type aliases and other names that are types the checker does not
    /// model.
    opaque: HashSet<String>,
    /// C functions that pass or return pointers, which only `unsafe:` code
    /// may call.
    raw_externs: HashSet<String>,
}

impl Table {
    fn collect(&mut self, items: &[Item]) {
        // Names first, so types can refer to types declared later.
        for item in items {
            match &item.kind {
                ItemKind::Aggregate(a) => {
                    self.types.insert(a.name.name.clone(), TypeInfo::default());
                }
                ItemKind::Enum(e) => {
                    self.enums.insert(e.name.name.clone());
                    self.variants.insert(
                        e.name.name.clone(),
                        e.variants
                            .iter()
                            .map(|v| format!("{}.{}", e.name.name, v.name.name))
                            .collect(),
                    );
                }
                ItemKind::TypeAlias(t) => {
                    self.opaque.insert(t.name.name.clone());
                }
                ItemKind::Trait(t) => {
                    self.opaque.insert(t.name.name.clone());
                }
                _ => {}
            }
        }
        for item in items {
            match &item.kind {
                ItemKind::Fn(f) => {
                    let sig = self.signature(f);
                    self.fns.insert(f.name.name.clone(), sig);
                }
                ItemKind::Aggregate(a) => {
                    let fields = a
                        .fields
                        .iter()
                        .map(|f| (f.name.name.clone(), self.ty_opt(f.declared_ty.as_ref())))
                        .collect();
                    let methods = a
                        .methods
                        .iter()
                        .filter_map(|m| match &m.kind {
                            ItemKind::Fn(f) => Some((f.name.name.clone(), self.signature(f))),
                            _ => None,
                        })
                        .collect();
                    let parent = a.base.as_ref().and_then(|b| match &b.kind {
                        TypeExprKind::Named { path, .. } => {
                            path.segments.last().map(|s| s.name.clone())
                        }
                        _ => None,
                    });
                    self.types.insert(
                        a.name.name.clone(),
                        TypeInfo {
                            is_struct: a.kind == AggregateKind::Struct,
                            fields,
                            methods,
                            parent,
                        },
                    );
                }
                ItemKind::Const(c) => {
                    let ty = match &c.declared_ty {
                        Some(t) => self.ty(t),
                        None => literal_ty(&c.value),
                    };
                    self.consts.insert(c.name.name.clone(), ty);
                }
                ItemKind::Impl(i) => {
                    if let TypeExprKind::Named { path, .. } = &i.target.kind {
                        let name = path.segments.last().map(|s| s.name.clone());
                        let sigs: Vec<(String, FnSig)> = i
                            .methods
                            .iter()
                            .filter_map(|m| match &m.kind {
                                ItemKind::Fn(f) => Some((f.name.name.clone(), self.signature(f))),
                                _ => None,
                            })
                            .collect();
                        if let Some(info) = name.and_then(|n| self.types.get_mut(&n)) {
                            info.methods.extend(sigs);
                        }
                    }
                }
                ItemKind::RouteGroup(g) => self.collect(&g.items),
                ItemKind::Extern(e) => {
                    for f in &e.functions {
                        let sig = self.signature(f);
                        let pointer = |t: &Ty| matches!(t, Ty::Ptr | Ty::Tensor | Ty::List(_));
                        let raw = sig.params.iter().any(|p| pointer(&p.ty))
                            || sig.ret.as_ref().is_some_and(pointer);
                        if raw {
                            self.raw_externs.insert(f.name.name.clone());
                        }
                        self.fns.insert(f.name.name.clone(), sig);
                    }
                }
                _ => {}
            }
        }
    }

    fn signature(&self, f: &FnDecl) -> FnSig {
        let generics: Vec<String> = f.generics.iter().map(|g| g.name.name.clone()).collect();
        FnSig {
            elementwise: f.modifiers.is_kernel && crate::vm::kernel::is_elementwise(f),
            params: f
                .params
                .iter()
                .map(|p| ParamInfo {
                    name: p.name.name.clone(),
                    ty: p
                        .declared_ty
                        .as_ref()
                        .map_or(Ty::Any, |t| self.ty_g(t, &generics)),
                    pass: p.pass,
                })
                .collect(),
            ret: f.result.returns.as_ref().map(|t| self.ty_g(t, &generics)),
        }
    }

    fn ty_opt(&self, t: Option<&TypeExpr>) -> Ty {
        t.map_or(Ty::Any, |t| self.ty(t))
    }

    /// The checker's view of a written type. Types it does not model are
    /// `any`.
    fn ty(&self, t: &TypeExpr) -> Ty {
        self.ty_g(t, &[])
    }

    fn ty_g(&self, t: &TypeExpr, generics: &[String]) -> Ty {
        match &t.kind {
            TypeExprKind::Any => Ty::Any,
            TypeExprKind::Named { path, .. } => {
                if path.segments.len() != 1 {
                    return Ty::Any;
                }
                let name = path.segments[0].name.as_str();
                if generics.iter().any(|g| g == name) {
                    return Ty::Var(name.to_string());
                }
                match name {
                    "int" | "i8" | "i16" | "i32" | "i64" | "u8" | "u16" | "u32" | "u64"
                    | "isize" | "usize" => Ty::Int,
                    "float" | "f16" | "bf16" | "f32" | "f64" => Ty::Float,
                    "bool" => Ty::Bool,
                    "str" => Ty::Str,
                    "none" => Ty::None,
                    "tensor" | "Tensor" | "vec2" | "vec3" | "vec4" | "mat3" | "mat4" => Ty::Tensor,
                    _ if self.types.contains_key(name) || self.enums.contains(name) => {
                        Ty::Named(name.to_string())
                    }
                    _ => Ty::Any,
                }
            }
            TypeExprKind::Tensor(_) => Ty::Tensor,
            TypeExprKind::List(inner) => Ty::List(Box::new(self.ty_g(inner, generics))),
            TypeExprKind::Array { element, .. } => Ty::List(Box::new(self.ty_g(element, generics))),
            TypeExprKind::Map { key, value } => Ty::Map(
                Box::new(self.ty_g(key, generics)),
                Box::new(self.ty_g(value, generics)),
            ),
            TypeExprKind::Tuple(items) if items.is_empty() => Ty::None,
            TypeExprKind::Tuple(items) => {
                Ty::Tuple(items.iter().map(|t| self.ty_g(t, generics)).collect())
            }
            TypeExprKind::Optional(inner) => match self.ty_g(inner, generics) {
                Ty::Any => Ty::Any,
                t => Ty::Optional(Box::new(t)),
            },
            TypeExprKind::Owned {
                ownership: Ownership::Ptr,
                ..
            } => Ty::Ptr,
            TypeExprKind::Owned { inner, .. } => self.ty_g(inner, generics),
            TypeExprKind::Placed { inner, .. } => self.ty_g(inner, generics),
            _ => Ty::Any,
        }
    }

    fn type_info(&self, name: &str) -> Option<&TypeInfo> {
        self.types.get(name)
    }

    /// A field's type, looking through base classes. `None` when the type
    /// has no such field.
    fn field(&self, ty: &str, name: &str) -> Option<Ty> {
        let mut current = Some(ty.to_string());
        let mut hops = 0;
        while let Some(t) = current {
            let info = self.types.get(&t)?;
            if let Some((_, ty)) = info.fields.iter().find(|(f, _)| f == name) {
                return Some(ty.clone());
            }
            current = info.parent.clone();
            hops += 1;
            if hops > 64 {
                break;
            }
        }
        None
    }

    fn method(&self, ty: &str, name: &str) -> Option<&FnSig> {
        let mut current = Some(ty.to_string());
        let mut hops = 0;
        while let Some(t) = current {
            let info = self.types.get(&t)?;
            if let Some(sig) = info.methods.get(name) {
                return Some(sig);
            }
            current = info.parent.clone();
            hops += 1;
            if hops > 64 {
                break;
            }
        }
        None
    }

    /// Whether every ancestor of `ty` is declared here, so a missing field
    /// is certain.
    fn fully_known(&self, ty: &str) -> bool {
        let mut current = Some(ty.to_string());
        let mut hops = 0;
        while let Some(t) = current {
            let Some(info) = self.types.get(&t) else {
                return false;
            };
            current = info.parent.clone();
            hops += 1;
            if hops > 64 {
                return false;
            }
        }
        true
    }

    /// Binds the type parameters in `param` to the parts of `arg` they
    /// stand for. Fails with the parameter and both types when a parameter
    /// is used for two types that do not fit each other.
    fn unify(
        &self,
        param: &Ty,
        arg: &Ty,
        at: Span,
        bindings: &mut HashMap<String, (Ty, Span)>,
    ) -> Result<(), (String, Ty, Ty)> {
        match (param, arg) {
            (_, Ty::Any) => Ok(()),
            (Ty::Var(v), a) => match bindings.get(v) {
                None => {
                    bindings.insert(v.clone(), (a.clone(), at));
                    Ok(())
                }
                Some((bound, _)) if self.assignable(bound, a) => Ok(()),
                // `T` was an int and now a float: it is a float.
                Some((bound, _)) if self.assignable(a, bound) => {
                    bindings.insert(v.clone(), (a.clone(), at));
                    Ok(())
                }
                Some((bound, _)) => Err((v.clone(), bound.clone(), a.clone())),
            },
            (Ty::List(p), Ty::List(a)) | (Ty::Optional(p), Ty::Optional(a)) => {
                self.unify(p, a, at, bindings)
            }
            (Ty::Optional(p), a) if *a != Ty::None => self.unify(p, a, at, bindings),
            (Ty::Map(pk, pv), Ty::Map(ak, av)) => {
                self.unify(pk, ak, at, bindings)?;
                self.unify(pv, av, at, bindings)
            }
            (Ty::Tuple(ps), Ty::Tuple(as_)) if ps.len() == as_.len() => {
                for (p, a) in ps.iter().zip(as_) {
                    self.unify(p, a, at, bindings)?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn is_subtype(&self, sub: &str, sup: &str) -> bool {
        let mut current = Some(sub.to_string());
        let mut hops = 0;
        while let Some(t) = current {
            if t == sup {
                return true;
            }
            current = self.types.get(&t).and_then(|i| i.parent.clone());
            hops += 1;
            if hops > 64 {
                break;
            }
        }
        false
    }

    fn assignable(&self, expected: &Ty, actual: &Ty) -> bool {
        match (expected, actual) {
            (Ty::Any, _) | (_, Ty::Any) | (Ty::Var(_), _) | (_, Ty::Var(_)) => true,
            (a, b) if a == b => true,
            (Ty::Float, Ty::Int) => true,
            (Ty::Optional(_), Ty::None) => true,
            // `none` is C's NULL.
            (Ty::Ptr, Ty::None) => true,
            (Ty::Optional(e), Ty::Optional(a)) => self.assignable(e, a),
            (Ty::Optional(e), a) => self.assignable(e, a),
            (Ty::List(e), Ty::List(a)) => self.assignable(e, a),
            (Ty::Map(ek, ev), Ty::Map(ak, av)) => {
                self.assignable(ek, ak) && self.assignable(ev, av)
            }
            (Ty::Tuple(e), Ty::Tuple(a)) => {
                e.len() == a.len() && e.iter().zip(a).all(|(e, a)| self.assignable(e, a))
            }
            (Ty::Named(e), Ty::Named(a)) => self.is_subtype(a, e),
            _ => false,
        }
    }
}

/// The type of a literal constant, or `any`.
fn literal_ty(e: &Expr) -> Ty {
    match &e.kind {
        ExprKind::Int { .. } => Ty::Int,
        ExprKind::Float { .. } => Ty::Float,
        ExprKind::Str(_) => Ty::Str,
        ExprKind::Bool(_) => Ty::Bool,
        _ => Ty::Any,
    }
}

// ==== checking bodies ======================================================

#[derive(Clone, Copy, Debug, PartialEq)]
enum Kind {
    Let,
    Var,
    /// Declared by assignment, as in Python.
    Implicit,
    Param(PassMode),
    Loop {
        is_mut: bool,
    },
    /// Bound by a pattern, `catch ... as e`, `with ... as f`.
    Bound,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct Moved {
    at: Span,
    /// Moved on every path, rather than on some.
    surely: bool,
}

#[derive(Clone, Debug)]
struct Var {
    name: String,
    /// The declared type (or a `let`'s inferred one).
    ty: Ty,
    /// A narrower type known at this point: `x: int?` after
    /// `if x != none:` is `int`.
    narrowed: Option<Ty>,
    kind: Kind,
    moved: Option<Moved>,
}

impl Var {
    /// The type of the variable where it is read.
    fn current(&self) -> Ty {
        self.narrowed.clone().unwrap_or_else(|| self.ty.clone())
    }
}

struct FnCtx {
    name: String,
    ret: Option<Ty>,
    strict: bool,
    /// Scope depth where the function's own variables start.
    depth: usize,
}

struct Checker<'t> {
    table: &'t Table,
    scopes: Vec<Vec<Var>>,
    fns: Vec<FnCtx>,
    /// Lists being changed by `for mut` loops: name and the loop's span.
    borrowed: Vec<(String, Span)>,
    /// How many `unsafe:` blocks enclose the code being checked.
    unsafe_depth: usize,
    diagnostics: Vec<Diagnostic>,
}

type MoveState = Vec<Vec<Option<Moved>>>;

impl Checker<'_> {
    fn error(&mut self, message: impl Into<String>, span: Span) {
        self.diagnostics.push(Diagnostic::new(message, span));
    }

    fn error_hint(&mut self, message: impl Into<String>, span: Span, hint: impl Into<String>) {
        self.diagnostics
            .push(Diagnostic::new(message, span).with_hint(hint));
    }

    fn strict(&self) -> bool {
        self.fns.last().is_some_and(|f| f.strict)
    }

    // ---- scopes ------------------------------------------------------------

    fn push(&mut self) {
        self.scopes.push(Vec::new());
    }

    fn pop(&mut self) {
        self.scopes.pop();
    }

    fn declare(&mut self, name: &str, ty: Ty, kind: Kind) {
        if name.is_empty() || name == "_" {
            return;
        }
        if let Some(scope) = self.scopes.last_mut() {
            scope.push(Var {
                name: name.to_string(),
                ty,
                narrowed: None,
                kind,
                moved: None,
            });
        }
    }

    fn lookup(&self, name: &str) -> Option<&Var> {
        self.scopes
            .iter()
            .rev()
            .flat_map(|s| s.iter().rev())
            .find(|v| v.name == name)
    }

    fn lookup_mut(&mut self, name: &str) -> Option<&mut Var> {
        self.scopes
            .iter_mut()
            .rev()
            .flat_map(|s| s.iter_mut().rev())
            .find(|v| v.name == name)
    }

    fn move_state(&self) -> MoveState {
        self.scopes
            .iter()
            .map(|s| s.iter().map(|v| v.moved).collect())
            .collect()
    }

    fn set_move_state(&mut self, state: &MoveState) {
        for (scope, saved) in self.scopes.iter_mut().zip(state) {
            for (var, moved) in scope.iter_mut().zip(saved) {
                var.moved = *moved;
            }
        }
    }

    /// Joins the states at the end of several paths: a variable moved on
    /// every path is surely moved, on some paths maybe moved.
    fn join(states: &[MoveState]) -> MoveState {
        let Some(first) = states.first() else {
            return Vec::new();
        };
        let mut out = first.clone();
        for (i, scope) in out.iter_mut().enumerate() {
            for (j, slot) in scope.iter_mut().enumerate() {
                let all: Vec<Option<Moved>> = states
                    .iter()
                    .map(|s| s.get(i).and_then(|sc| sc.get(j)).copied().flatten())
                    .collect();
                let any = all.iter().flatten().next().copied();
                *slot = any.map(|m| Moved {
                    at: m.at,
                    surely: all.iter().all(|m| m.is_some_and(|m| m.surely)),
                });
            }
        }
        out
    }

    // ---- items -------------------------------------------------------------

    fn items(&mut self, items: &[Item]) {
        for item in items {
            match &item.kind {
                ItemKind::Fn(f) => self.function(f, None),
                ItemKind::Main(m) => {
                    self.begin_fn("main", None, false);
                    for p in &m.params {
                        self.param(p);
                    }
                    self.block(&m.body);
                    self.end_fn();
                }
                ItemKind::Aggregate(a) => {
                    for field in &a.fields {
                        if let (Some(t), Some(d)) = (&field.declared_ty, &field.default) {
                            let expected = self.table.ty(t);
                            self.push();
                            let actual = self.expr(d);
                            self.pop();
                            self.expect(&expected, &actual, d.span, || {
                                format!("the default of field `{}`", field.name.name)
                            });
                        }
                    }
                    for m in &a.methods {
                        if let ItemKind::Fn(f) = &m.kind {
                            self.function(f, Some(&a.name.name));
                        }
                    }
                }
                ItemKind::Enum(e) => {
                    for m in &e.methods {
                        if let ItemKind::Fn(f) = &m.kind {
                            self.function(f, Some(&e.name.name));
                        }
                    }
                }
                ItemKind::Impl(i) => {
                    let owner = match &i.target.kind {
                        TypeExprKind::Named { path, .. } => {
                            path.segments.last().map(|s| s.name.clone())
                        }
                        _ => None,
                    };
                    for m in &i.methods {
                        if let ItemKind::Fn(f) = &m.kind {
                            self.function(f, owner.as_deref());
                        }
                    }
                }
                ItemKind::Const(c) => {
                    if let Some(t) = &c.declared_ty {
                        let expected = self.table.ty(t);
                        self.push();
                        let actual = self.expr(&c.value);
                        self.pop();
                        self.expect(&expected, &actual, c.value.span, || {
                            format!("constant `{}`", c.name.name)
                        });
                    }
                }
                ItemKind::Route(r) => {
                    let ret = r.result.returns.as_ref().map(|t| self.table.ty(t));
                    self.begin_fn("route", ret, false);
                    for p in &r.params {
                        self.param(p);
                    }
                    self.block(&r.body);
                    self.end_fn();
                }
                ItemKind::RouteGroup(g) => self.items(&g.items),
                _ => {}
            }
        }
    }

    fn begin_fn(&mut self, name: &str, ret: Option<Ty>, strict: bool) {
        self.fns.push(FnCtx {
            name: name.to_string(),
            ret,
            strict,
            depth: self.scopes.len(),
        });
        self.push();
    }

    fn end_fn(&mut self) {
        self.pop();
        self.fns.pop();
    }

    fn param(&mut self, p: &Param) {
        if self.strict() {
            match &p.declared_ty {
                None if p.name.name != "self" => self.error_hint(
                    format!("parameter `{}` needs a type in strict code", p.name.name),
                    p.name.span,
                    format!("write `{}: T`", p.name.name),
                ),
                Some(t) if t.kind == TypeExprKind::Any => self.error(
                    format!("parameter `{}` cannot be `any` in strict code", p.name.name),
                    t.span,
                ),
                _ => {}
            }
        }
        let ty = self.table.ty_opt(p.declared_ty.as_ref());
        if let (Some(d), false) = (&p.default, ty == Ty::Any) {
            let actual = self.expr(d);
            self.expect(&ty, &actual, d.span, || {
                format!("the default of parameter `{}`", p.name.name)
            });
        }
        self.declare(&p.name.name, ty, Kind::Param(p.pass));
    }

    fn function(&mut self, f: &FnDecl, owner: Option<&str>) {
        let FnBody::Block(body) = &f.body else {
            return;
        };
        let strict = f.modifiers.is_strict || f.modifiers.is_kernel;
        let ret = f.result.returns.as_ref().map(|t| self.table.ty(t));
        if strict {
            if let Some(t) = &f.result.returns {
                if t.kind == TypeExprKind::Any {
                    self.error(
                        format!("`{}` cannot return `any` in strict code", f.name.name),
                        t.span,
                    );
                }
            }
        }
        self.begin_fn(&f.name.name, ret.clone(), strict);
        for p in &f.params {
            if p.name.name == "self" {
                let ty = owner.map_or(Ty::Any, |o| Ty::Named(o.to_string()));
                self.declare("self", ty, Kind::Param(p.pass));
            } else {
                self.param(p);
            }
        }
        self.block(body);
        if let Some(ret) = &ret {
            let needs_value = !matches!(ret, Ty::None | Ty::Any | Ty::Optional(_));
            if needs_value && !f.modifiers.is_async && !block_always_exits(&body.stmts) {
                self.error_hint(
                    format!(
                        "`{}` can reach its end without returning a {}",
                        f.name.name,
                        ret.name()
                    ),
                    f.name.span,
                    "add a `return` on every path, or `fail` where there is no value",
                );
            }
        }
        self.end_fn();
    }

    // ---- statements --------------------------------------------------------

    fn block(&mut self, block: &Block) {
        self.push();
        self.stmts(&block.stmts);
        self.pop();
    }

    fn stmts(&mut self, stmts: &[Stmt]) {
        for stmt in stmts {
            self.stmt(stmt);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Binding(b) => self.binding(b, stmt.span),
            StmtKind::Expr(e) => {
                self.expr(e);
            }
            StmtKind::Assign { target, op, value } => self.assign(target, *op, value),
            StmtKind::Return(value) => {
                let actual = match value {
                    Some(v) => self.expr(v),
                    None => Ty::None,
                };
                let ret = self.fns.last().and_then(|f| f.ret.clone());
                let name = self.fns.last().map(|f| f.name.clone()).unwrap_or_default();
                if let Some(expected) = ret {
                    let span = value.as_ref().map_or(stmt.span, |v| v.span);
                    let expected = match (&expected, &actual) {
                        // A function returning `T?` may return `none`.
                        (Ty::Optional(_), _) => expected.clone(),
                        _ => expected,
                    };
                    if let (Ty::Optional(inner), false) =
                        (&actual, matches!(expected, Ty::Optional(_)))
                    {
                        if self.table.assignable(&expected, inner) {
                            self.error_hint(
                                format!(
                                    "`{name}` returns {}, but this may be none",
                                    expected.name()
                                ),
                                span,
                                format!(
                                    "check it first, give a default with `??`, or declare `-> {}?`",
                                    expected.name()
                                ),
                            );
                            return;
                        }
                    }
                    if !self.table.assignable(&expected, &actual) {
                        self.error(
                            format!(
                                "`{name}` returns {}, but this is {}",
                                expected.name(),
                                actual.name()
                            ),
                            span,
                        );
                    }
                }
            }
            StmtKind::Fail(e) | StmtKind::Defer(e) => {
                self.expr(e);
            }
            StmtKind::Stop | StmtKind::Skip => {}
            StmtKind::If(s) => {
                let mut ends = Vec::new();
                // Narrowing at the end of each path that falls through.
                let mut narrow_ends = Vec::new();
                let before = self.narrow_state();
                for (condition, body) in &s.branches {
                    self.push();
                    self.condition(condition);
                    // The condition runs on every path through the rest.
                    let after_condition = self.move_state();
                    let (when_true, when_false) = match condition {
                        Condition::Expr(e) => self.facts(e),
                        Condition::Let { .. } => (Vec::new(), Vec::new()),
                    };
                    let outside = self.narrow_state();
                    self.narrow(&when_true);
                    self.block(body);
                    if !block_always_exits(&body.stmts) {
                        narrow_ends.push(self.narrow_state());
                    }
                    ends.push(self.move_state());
                    self.pop();
                    self.set_move_state(&after_condition);
                    // Later branches run only when this condition was false.
                    let len = outside.len().min(self.scopes.len());
                    self.set_narrow_state(&outside[..len]);
                    self.narrow(&when_false);
                }
                match &s.else_body {
                    Some(body) => {
                        self.block(body);
                        if !block_always_exits(&body.stmts) {
                            narrow_ends.push(self.narrow_state());
                        }
                        ends.push(self.move_state());
                    }
                    None => {
                        narrow_ends.push(self.narrow_state());
                        ends.push(self.move_state());
                    }
                }
                let joined = Self::join(&ends);
                self.set_move_state(&joined);
                let narrowed = if narrow_ends.is_empty() {
                    before
                } else {
                    Self::join_narrow(&narrow_ends)
                };
                self.set_narrow_state(&narrowed);
            }
            StmtKind::While { condition, body } => {
                self.push();
                self.condition(condition);
                self.pop();
                self.loop_body(|c| {
                    c.push();
                    c.condition(condition);
                    c.block(body);
                    c.pop();
                });
            }
            StmtKind::Loop(body) => self.loop_body(|c| c.block(body)),
            StmtKind::For(f) => self.for_stmt(f),
            StmtKind::Match(m) => {
                let scrutinee = self.expr(&m.scrutinee);
                let start = self.move_state();
                let mut ends = Vec::new();
                for arm in &m.arms {
                    self.set_move_state(&start);
                    self.push();
                    self.pattern(&arm.pattern, &scrutinee);
                    if let Some(g) = &arm.guard {
                        self.expr(g);
                    }
                    self.block(&arm.body);
                    self.pop();
                    ends.push(self.move_state());
                }
                self.set_move_state(&start);
                if let Some(body) = &m.else_body {
                    self.block(body);
                } else {
                    self.exhaustive(m, &scrutinee);
                }
                ends.push(self.move_state());
                let joined = Self::join(&ends);
                self.set_move_state(&joined);
            }
            StmtKind::Try(t) => {
                let start = self.move_state();
                self.block(&t.body);
                let mut ends = vec![self.move_state()];
                for c in &t.catches {
                    // A failure can happen anywhere in the body.
                    let partial = Self::join(&[start.clone(), ends[0].clone()]);
                    self.set_move_state(&partial);
                    self.push();
                    if let Some(b) = &c.binding {
                        let ty = c.error_ty.as_ref().map_or(Ty::Any, |t| self.table.ty(t));
                        self.declare(&b.name, ty, Kind::Bound);
                    }
                    self.block(&c.body);
                    self.pop();
                    ends.push(self.move_state());
                }
                let joined = Self::join(&ends);
                self.set_move_state(&joined);
                if let Some(f) = &t.finally {
                    self.block(f);
                }
            }
            StmtKind::With { items, body, .. } => {
                self.push();
                for item in items {
                    let ty = self.expr(&item.value);
                    if let Some(b) = &item.binding {
                        self.declare(&b.name, ty, Kind::Bound);
                    }
                }
                self.block(body);
                self.pop();
            }
            StmtKind::On(o) => {
                self.expr(&o.target);
                self.block(&o.body);
            }
            StmtKind::Region(r) => {
                self.push();
                self.declare(&r.name.name, Ty::Any, Kind::Bound);
                self.block(&r.body);
                self.pop();
            }
            StmtKind::Unsafe(b) => {
                self.unsafe_depth += 1;
                self.block(b);
                self.unsafe_depth -= 1;
            }
            StmtKind::Item(item) => {
                if let ItemKind::Fn(f) = &item.kind {
                    // Declared first, so it can call itself.
                    self.declare(&f.name.name, Ty::Any, Kind::Bound);
                    self.function(f, None);
                }
            }
        }
    }

    /// Checks a loop body as if it ran twice, so a move in one iteration is
    /// reported where a later iteration uses the value.
    fn loop_body(&mut self, mut body: impl FnMut(&mut Self)) {
        let start = self.move_state();
        body(self);
        let after_one = self.move_state();
        if after_one != start {
            // A later iteration may or may not run.
            let again = Self::join(&[start.clone(), after_one.clone()]);
            self.set_move_state(&again);
            body(self);
        }
        let end = self.move_state();
        let joined = Self::join(&[start, after_one, end]);
        self.set_move_state(&joined);
    }

    fn condition(&mut self, condition: &Condition) {
        match condition {
            Condition::Expr(e) => {
                self.expr(e);
            }
            Condition::Let { pattern, value } => {
                let ty = self.expr(value);
                let inner = match ty {
                    Ty::Optional(t) => *t,
                    other => other,
                };
                self.pattern(pattern, &inner);
            }
        }
    }

    fn binding(&mut self, b: &Binding, span: Span) {
        if self.strict() {
            if let Some(t) = &b.declared_ty {
                if t.kind == TypeExprKind::Any {
                    self.error("`any` is not allowed in strict code", t.span);
                }
            }
        }
        let declared = b.declared_ty.as_ref().map(|t| self.table.ty(t));
        let actual = match &b.value {
            Some(v) => self.expr(v),
            None => Ty::Any,
        };
        if let (Some(expected), Some(v)) = (&declared, &b.value) {
            let what = match &b.target {
                BindTarget::Name(n) => format!("`{}`", n.name),
                _ => "this binding".into(),
            };
            self.expect(expected, &actual, v.span, || what.clone());
        }
        if b.kind == BindingKind::Let && b.value.is_none() {
            self.error_hint(
                "a `let` binding needs a value",
                span,
                "use `var` to assign it later",
            );
        }
        let kind = match b.kind {
            BindingKind::Let | BindingKind::Const => Kind::Let,
            BindingKind::Var => Kind::Var,
        };
        // An unannotated `var` may later hold anything, so only a `let`
        // keeps the type of its value.
        let ty = match (declared, kind) {
            (Some(t), _) => t,
            (None, Kind::Let) => actual,
            _ => Ty::Any,
        };
        self.bind_target(&b.target, ty, kind);
    }

    fn bind_target(&mut self, target: &BindTarget, ty: Ty, kind: Kind) {
        match target {
            BindTarget::Name(n) => self.declare(&n.name, ty, kind),
            BindTarget::Discard(_) => {}
            BindTarget::Tuple(items, _) => {
                let parts = match &ty {
                    Ty::Tuple(parts) if parts.len() == items.len() => parts.clone(),
                    _ => vec![Ty::Any; items.len()],
                };
                for (item, ty) in items.iter().zip(parts) {
                    self.bind_target(item, ty, kind);
                }
            }
        }
    }

    fn for_stmt(&mut self, f: &ForStmt) {
        let iter_ty = self.expr(&f.iter);
        if matches!(iter_ty, Ty::Optional(_)) {
            self.maybe_none(&f.iter, "a loop");
        }
        let item_ty = match &iter_ty {
            Ty::List(t) => (**t).clone(),
            Ty::Range => Ty::Int,
            Ty::Str => Ty::Str,
            Ty::Map(k, _) => (**k).clone(),
            _ => Ty::Any,
        };
        let is_mut = f.pass == PassMode::Mut;
        let mut borrowed = None;
        if is_mut {
            if let ExprKind::Name(n) = &f.iter.kind {
                self.check_changeable(n, f.iter.span, "change the elements of");
                borrowed = Some((n.name.clone(), f.iter.span));
            }
        }
        if let Some(b) = borrowed.clone() {
            self.borrowed.push(b);
        }
        self.loop_body(|c| {
            c.push();
            c.bind_target(&f.target, item_ty.clone(), Kind::Loop { is_mut });
            c.block(&f.body);
            c.pop();
        });
        if borrowed.is_some() {
            self.borrowed.pop();
        }
    }

    fn pattern(&mut self, pattern: &Pattern, ty: &Ty) {
        match &pattern.kind {
            PatternKind::Binding(n) => self.declare(&n.name, ty.clone(), Kind::Bound),
            PatternKind::As {
                pattern: inner,
                name,
            } => {
                self.pattern(inner, ty);
                self.declare(&name.name, ty.clone(), Kind::Bound);
            }
            PatternKind::Destructure { args, .. } => {
                for a in args {
                    self.pattern(&a.pattern, &Ty::Any);
                }
            }
            PatternKind::Tuple(items) | PatternKind::List(items) => {
                for item in items {
                    match item {
                        SeqPattern::Item(p) => self.pattern(p, &Ty::Any),
                        SeqPattern::Rest(Some(n), _) => {
                            self.declare(&n.name, Ty::List(Box::new(Ty::Any)), Kind::Bound)
                        }
                        SeqPattern::Rest(None, _) => {}
                    }
                }
            }
            PatternKind::Or(alternatives) => {
                for p in alternatives {
                    self.pattern(p, ty);
                }
            }
            _ => {}
        }
    }

    // ---- assignment and mutation ----------------------------------------

    /// Reports a change to variable `n` that its declaration forbids.
    fn check_changeable(&mut self, n: &Ident, span: Span, action: &str) {
        let Some(var) = self.lookup(&n.name).cloned() else {
            return;
        };
        let in_this_fn = self
            .fns
            .last()
            .map(|f| f.depth)
            .is_some_and(|d| self.scope_of(&n.name).is_some_and(|s| s >= d));
        match var.kind {
            Kind::Let => self.error_hint(
                format!("cannot {action} `{}`: it is declared with `let`", n.name),
                span,
                format!("declare it with `var {}` to change it", n.name),
            ),
            Kind::Param(PassMode::Borrow) if self.strict() && in_this_fn && n.name != "self" => {
                self.error_hint(
                    format!(
                        "cannot {action} `{}`: strict code takes parameters read-only",
                        n.name
                    ),
                    span,
                    format!("take it as `mut {}` to change it", n.name),
                )
            }
            _ => {}
        }
    }

    fn scope_of(&self, name: &str) -> Option<usize> {
        self.scopes
            .iter()
            .enumerate()
            .rev()
            .find(|(_, s)| s.iter().any(|v| v.name == name))
            .map(|(i, _)| i)
    }

    /// The variable a place expression (`x`, `x.f`, `x[i].g`) changes.
    fn root_of(place: &Expr) -> Option<&Ident> {
        match &place.kind {
            ExprKind::Name(n) => Some(n),
            ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => Self::root_of(base),
            _ => None,
        }
    }

    fn assign(&mut self, target: &Expr, op: AssignOp, value: &Expr) {
        let actual = self.expr(value);
        match &target.kind {
            ExprKind::Name(n) => {
                if op != AssignOp::Assign {
                    self.use_name(n, target.span);
                }
                self.check_borrowed(n, target.span);
                match self.lookup(&n.name).cloned() {
                    None => {
                        let global = self.table.fns.contains_key(&n.name)
                            || self.table.consts.contains_key(&n.name)
                            || self.table.types.contains_key(&n.name);
                        if self.strict() && op == AssignOp::Assign && !global {
                            self.error_hint(
                                format!("`{}` is not declared", n.name),
                                target.span,
                                format!(
                                    "strict code declares variables: `let {0} = ...` or `var {0} = ...`",
                                    n.name
                                ),
                            );
                        }
                        if op == AssignOp::Assign && !global {
                            self.declare(&n.name, Ty::Any, Kind::Implicit);
                        }
                    }
                    Some(var) => {
                        self.check_changeable(n, target.span, "assign to");
                        if op == AssignOp::Assign {
                            self.expect(&var.ty, &actual, value.span, || format!("`{}`", n.name));
                            // Assigning gives a moved variable a value again,
                            // and tells whether it is `none` now.
                            let narrowed = match (&var.ty, &actual) {
                                (Ty::Optional(inner), a)
                                    if *a != Ty::None && self.table.assignable(inner, a) =>
                                {
                                    Some((**inner).clone())
                                }
                                _ => None,
                            };
                            if let Some(v) = self.lookup_mut(&n.name) {
                                v.moved = None;
                                v.narrowed = narrowed;
                            }
                        }
                    }
                }
            }
            ExprKind::Tuple(items) => {
                for item in items {
                    if let ExprKind::Name(n) = &item.kind {
                        self.check_borrowed(n, item.span);
                        match self.lookup(&n.name) {
                            Some(_) => {
                                self.check_changeable(n, item.span, "assign to");
                                if let Some(v) = self.lookup_mut(&n.name) {
                                    v.moved = None;
                                }
                            }
                            None => {
                                if self.strict() {
                                    self.error(format!("`{}` is not declared", n.name), item.span);
                                }
                                self.declare(&n.name, Ty::Any, Kind::Implicit);
                            }
                        }
                    } else {
                        self.mutate_place(item);
                    }
                }
            }
            _ => {
                let expected = self.place_ty(target);
                self.mutate_place(target);
                if op == AssignOp::Assign {
                    if let ExprKind::Field { name, .. } = &target.kind {
                        self.expect(&expected, &actual, value.span, || {
                            format!("field `{}`", name.name)
                        });
                    }
                }
            }
        }
    }

    /// Checks a write through a field or index of `place`.
    fn mutate_place(&mut self, place: &Expr) {
        let Some(root) = Self::root_of(place) else {
            self.expr(place);
            return;
        };
        let root = root.clone();
        self.use_name(&root, place.span);
        self.check_borrowed(&root, place.span);
        if let Some(var) = self.lookup(&root.name).cloned() {
            let is_value_type = matches!(&var.ty, Ty::Named(t)
                if self.table.type_info(t).is_some_and(|i| i.is_struct));
            if var.kind == Kind::Let && is_value_type {
                self.check_changeable(&root, place.span, "change a field of");
            }
            if self.strict() && matches!(var.kind, Kind::Param(PassMode::Borrow)) {
                self.check_changeable(&root, place.span, "change");
            }
            if var.kind == (Kind::Loop { is_mut: false })
                && is_value_type
                && matches!(place.kind, ExprKind::Field { .. })
            {
                self.error_hint(
                    format!(
                        "this changes a copy: `{}` is a copy of the element",
                        root.name
                    ),
                    place.span,
                    format!(
                        "loop with `for mut {} in ...` to change the elements",
                        root.name
                    ),
                );
            }
        }
        // Evaluate the indexes and inner bases for uses.
        if let ExprKind::Index { subscripts, .. } = &place.kind {
            self.subscripts(subscripts);
        }
    }

    fn place_ty(&mut self, place: &Expr) -> Ty {
        match &place.kind {
            ExprKind::Field { base, name, .. } => match self.peek_ty(base) {
                Ty::Named(t) => self.table.field(&t, &name.name).unwrap_or(Ty::Any),
                _ => Ty::Any,
            },
            _ => Ty::Any,
        }
    }

    /// The type of a simple place expression, without reporting anything.
    fn peek_ty(&self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Name(n) => self.lookup(&n.name).map_or(Ty::Any, |v| v.ty.clone()),
            ExprKind::Field { base, name, .. } => match self.peek_ty(base) {
                Ty::Named(t) => self.table.field(&t, &name.name).unwrap_or(Ty::Any),
                _ => Ty::Any,
            },
            ExprKind::Index { base, .. } => match self.peek_ty(base) {
                Ty::List(t) => *t,
                Ty::Map(_, v) => *v,
                _ => Ty::Any,
            },
            _ => Ty::Any,
        }
    }

    fn check_borrowed(&mut self, n: &Ident, span: Span) {
        if let Some((_, loop_span)) = self.borrowed.iter().find(|(b, _)| *b == n.name).cloned() {
            self.error_hint(
                format!(
                    "`{}` cannot be used here: a `for mut` loop is changing its elements",
                    n.name
                ),
                span,
                format!(
                    "the loop over `{}` on line {} has the list to itself until it ends",
                    n.name, loop_span.start.line
                ),
            );
        }
    }

    /// A read of variable `n`.
    fn use_name(&mut self, n: &Ident, span: Span) {
        self.check_borrowed(n, span);
        if let Some(moved) = self.lookup(&n.name).and_then(|v| v.moved) {
            let (message, hint) = if moved.surely {
                (
                    format!("`{}` is used after it was moved", n.name),
                    format!(
                        "it was moved on line {}; use a copy, or move it later",
                        moved.at.start.line
                    ),
                )
            } else {
                (
                    format!("`{}` may have been moved", n.name),
                    format!(
                        "it is moved on line {} on some paths (or in an earlier iteration)",
                        moved.at.start.line
                    ),
                )
            };
            self.error_hint(message, span, hint);
        }
    }

    /// Reports the cases a `match` without `else` does not handle, for the
    /// types whose cases are known: enums declared here, `bool`, and `T?`.
    fn exhaustive(&mut self, m: &MatchStmt, ty: &Ty) {
        let cases: Vec<String> = match ty {
            Ty::Named(e) => match self.table.variants.get(e) {
                Some(v) => v.clone(),
                None => return,
            },
            Ty::Bool => vec!["true".into(), "false".into()],
            Ty::Optional(_) => vec!["none".into(), "a value".into()],
            _ => return,
        };
        let mut covered: HashSet<String> = HashSet::new();
        for arm in m.arms.iter().filter(|a| a.guard.is_none()) {
            if covers(&arm.pattern, ty, &cases, &mut covered) {
                return;
            }
        }
        let missing: Vec<&String> = cases.iter().filter(|c| !covered.contains(*c)).collect();
        if missing.is_empty() {
            return;
        }
        let list = missing
            .iter()
            .take(3)
            .map(|c| format!("`{c}`"))
            .collect::<Vec<_>>()
            .join(", ");
        let more = if missing.len() > 3 {
            format!(" and {} more", missing.len() - 3)
        } else {
            String::new()
        };
        let example = match missing[0].as_str() {
            "a value" => "is v: ...".to_string(),
            c => format!("is {c}: ..."),
        };
        self.error_hint(
            format!("this `match` does not handle {list}{more}"),
            m.scrutinee.span,
            format!("add `{example}` for each, or an `else:` arm"),
        );
    }

    /// Reports a use of a value that may be `none`.
    fn maybe_none(&mut self, e: &Expr, use_: &str) {
        let text = match place_text(e) {
            Some(t) => format!("`{t}`"),
            None => "this value".to_string(),
        };
        let fix = match place_text(e) {
            Some(t) => format!(
                "check it first (`if {t} != none:`), use `{t}?.field` to get none through, or give a default with `{t} ?? ...`"
            ),
            None => "check it for none first, or give a default with `??`".to_string(),
        };
        self.error_hint(
            format!("{text} may be none, so it cannot be used in {use_}"),
            e.span,
            fix,
        );
    }

    /// What a condition establishes: the variables known not to be `none`
    /// when it is true, and when it is false.
    fn facts(&self, e: &Expr) -> (Vec<String>, Vec<String>) {
        let optional = |n: &Ident| {
            self.lookup(&n.name)
                .is_some_and(|v| matches!(v.current(), Ty::Optional(_)))
        };
        match &e.kind {
            ExprKind::Compare { operands, ops } if ops.len() == 1 => {
                let name = match (&operands[0].kind, &operands[1].kind) {
                    (ExprKind::Name(n), ExprKind::None) | (ExprKind::None, ExprKind::Name(n)) => n,
                    _ => return (Vec::new(), Vec::new()),
                };
                if !optional(name) {
                    return (Vec::new(), Vec::new());
                }
                match ops[0] {
                    CompareOp::NotEq => (vec![name.name.clone()], Vec::new()),
                    CompareOp::Eq => (Vec::new(), vec![name.name.clone()]),
                    _ => (Vec::new(), Vec::new()),
                }
            }
            ExprKind::Name(n) if optional(n) => (vec![n.name.clone()], Vec::new()),
            ExprKind::Unary {
                op: UnaryOp::Not,
                operand,
            } => {
                let (t, f) = self.facts(operand);
                (f, t)
            }
            ExprKind::Binary {
                op: BinaryOp::And,
                lhs,
                rhs,
            } => {
                let (mut t, _) = self.facts(lhs);
                t.extend(self.facts(rhs).0);
                (t, Vec::new())
            }
            ExprKind::Binary {
                op: BinaryOp::Or,
                lhs,
                rhs,
            } => {
                let (_, mut f) = self.facts(lhs);
                f.extend(self.facts(rhs).1);
                (Vec::new(), f)
            }
            _ => (Vec::new(), Vec::new()),
        }
    }

    /// Marks variables as not `none` from here on.
    fn narrow(&mut self, names: &[String]) {
        for name in names {
            if let Some(v) = self.lookup_mut(name) {
                if let Ty::Optional(inner) = v.current() {
                    v.narrowed = Some(*inner);
                }
            }
        }
    }

    fn narrow_state(&self) -> Vec<Vec<Option<Ty>>> {
        self.scopes
            .iter()
            .map(|s| s.iter().map(|v| v.narrowed.clone()).collect())
            .collect()
    }

    fn set_narrow_state(&mut self, state: &[Vec<Option<Ty>>]) {
        for (scope, saved) in self.scopes.iter_mut().zip(state) {
            for (var, n) in scope.iter_mut().zip(saved) {
                var.narrowed = n.clone();
            }
        }
    }

    /// After paths join, a variable stays narrowed only if it is on every
    /// path that reaches the join.
    fn join_narrow(states: &[Vec<Vec<Option<Ty>>>]) -> Vec<Vec<Option<Ty>>> {
        let Some(first) = states.first() else {
            return Vec::new();
        };
        let mut out = first.clone();
        for (i, scope) in out.iter_mut().enumerate() {
            for (j, slot) in scope.iter_mut().enumerate() {
                let same = states
                    .iter()
                    .all(|s| s.get(i).and_then(|sc| sc.get(j)) == Some(&*slot));
                if !same {
                    *slot = None;
                }
            }
        }
        out
    }

    fn expect(&mut self, expected: &Ty, actual: &Ty, span: Span, what: impl FnOnce() -> String) {
        if !self.table.assignable(expected, actual) {
            let what = what();
            if let Ty::Optional(inner) = actual {
                if self.table.assignable(expected, inner) {
                    self.error_hint(
                        format!("{what} is {}, but this may be none", expected.name()),
                        span,
                        "check it first (`if x != none:`), or give a default with `??`",
                    );
                    return;
                }
            }
            self.error(
                format!(
                    "{what} is {}, but this is {}",
                    expected.name(),
                    actual.name()
                ),
                span,
            );
        }
    }

    // ---- expressions ---------------------------------------------------------

    fn subscripts(&mut self, subscripts: &[Subscript]) {
        for s in subscripts {
            match s {
                Subscript::Index(e) => {
                    self.expr(e);
                }
                Subscript::Slice {
                    start, stop, step, ..
                } => {
                    for e in [start, stop, step].into_iter().flatten() {
                        self.expr(e);
                    }
                }
                Subscript::Ellipsis(_) => {}
            }
        }
    }

    fn expr(&mut self, e: &Expr) -> Ty {
        match &e.kind {
            ExprKind::Int { .. } => Ty::Int,
            ExprKind::Float { .. } => Ty::Float,
            ExprKind::Bool(_) => Ty::Bool,
            ExprKind::None => Ty::None,
            ExprKind::Str(segments) => {
                for s in segments {
                    if let StrSegment::Interp(inner) = s {
                        self.expr(inner);
                    }
                }
                Ty::Str
            }
            ExprKind::Name(n) => {
                self.use_name(n, e.span);
                if let Some(v) = self.lookup(&n.name) {
                    return v.current();
                }
                self.table.consts.get(&n.name).cloned().unwrap_or(Ty::Any)
            }
            ExprKind::Tuple(items) => Ty::Tuple(items.iter().map(|i| self.expr(i)).collect()),
            ExprKind::List(items) => {
                let tys: Vec<Ty> = items.iter().map(|i| self.expr(i)).collect();
                Ty::List(Box::new(common(&tys)))
            }
            ExprKind::Map(entries) => {
                let mut keys = Vec::new();
                let mut values = Vec::new();
                for (k, v) in entries {
                    keys.push(self.expr(k));
                    values.push(self.expr(v));
                }
                Ty::Map(Box::new(common(&keys)), Box::new(common(&values)))
            }
            ExprKind::ListComp { element, clauses } => {
                self.push();
                self.clauses(clauses);
                let t = self.expr(element);
                self.pop();
                Ty::List(Box::new(t))
            }
            ExprKind::MapComp {
                key,
                value,
                clauses,
            } => {
                self.push();
                self.clauses(clauses);
                let k = self.expr(key);
                let v = self.expr(value);
                self.pop();
                Ty::Map(Box::new(k), Box::new(v))
            }
            ExprKind::Unary { op, operand } => {
                let t = self.expr(operand);
                match op {
                    UnaryOp::Not => Ty::Bool,
                    UnaryOp::Neg | UnaryOp::Pos => match t {
                        Ty::Int | Ty::Float | Ty::Tensor => t,
                        Ty::Str | Ty::Bool => {
                            self.error(
                                format!("unary `-` needs a number, not {}", t.name()),
                                e.span,
                            );
                            Ty::Any
                        }
                        _ => Ty::Any,
                    },
                    UnaryOp::BitNot => match t {
                        Ty::Int => Ty::Int,
                        _ => Ty::Any,
                    },
                }
            }
            ExprKind::Binary {
                op: op @ (BinaryOp::And | BinaryOp::Or),
                lhs,
                rhs,
            } => {
                // `x != none and x.f`: the right side knows what the left
                // side established.
                let a = self.expr(lhs);
                let (when_true, when_false) = self.facts(lhs);
                let saved = self.narrow_state();
                self.narrow(if *op == BinaryOp::And {
                    &when_true
                } else {
                    &when_false
                });
                let b = self.expr(rhs);
                self.set_narrow_state(&saved);
                self.binary(*op, &a, &b, e.span)
            }
            ExprKind::Binary { op, lhs, rhs } => {
                let a = self.expr(lhs);
                let b = self.expr(rhs);
                for (side, t) in [(lhs, &a), (rhs, &b)] {
                    if matches!(t, Ty::Optional(_)) {
                        self.maybe_none(side, "arithmetic");
                        return Ty::Any;
                    }
                }
                self.binary(*op, &a, &b, e.span)
            }
            ExprKind::Compare { operands, .. } => {
                for o in operands {
                    self.expr(o);
                }
                Ty::Bool
            }
            ExprKind::Range {
                start, end, step, ..
            } => {
                self.expr(start);
                self.expr(end);
                if let Some(s) = step {
                    self.expr(s);
                }
                Ty::Range
            }
            ExprKind::Coalesce { lhs, rhs } => {
                let a = self.expr(lhs);
                let b = self.expr(rhs);
                match a {
                    Ty::Optional(t) if self.table.assignable(&t, &b) => *t,
                    _ => Ty::Any,
                }
            }
            ExprKind::Pipeline { lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
                Ty::Any
            }
            ExprKind::MatMul { lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
                Ty::Tensor
            }
            ExprKind::Placement { value, target, .. } => {
                let t = self.expr(value);
                self.expr(target);
                t
            }
            ExprKind::Index { base, subscripts } => {
                let t = self.expr(base);
                self.subscripts(subscripts);
                if matches!(t, Ty::Optional(_)) {
                    self.maybe_none(base, "indexing");
                    return Ty::Any;
                }
                let is_slice = subscripts
                    .iter()
                    .any(|s| matches!(s, Subscript::Slice { .. }));
                match (t, is_slice) {
                    (t @ (Ty::List(_) | Ty::Str), true) => t,
                    (Ty::List(inner), false) => *inner,
                    (Ty::Map(_, v), false) => *v,
                    (Ty::Str, false) => Ty::Str,
                    (Ty::Tensor, _) => Ty::Any,
                    (Ty::Tuple(items), false) => match subscripts.first() {
                        Some(Subscript::Index(Expr {
                            kind: ExprKind::Int { value, .. },
                            ..
                        })) if subscripts.len() == 1 => {
                            items.get(*value as usize).cloned().unwrap_or(Ty::Any)
                        }
                        _ => Ty::Any,
                    },
                    _ => Ty::Any,
                }
            }
            ExprKind::Field {
                base,
                name,
                optional,
            } => {
                // `Enum.Variant` and module members are not values here.
                if let ExprKind::Name(n) = &base.kind {
                    if self.lookup(&n.name).is_none() {
                        return Ty::Any;
                    }
                }
                let t = self.expr(base);
                match t {
                    Ty::Optional(inner) if *optional => match self.field(&inner, name) {
                        Ty::Any => Ty::Any,
                        Ty::Optional(t) => Ty::Optional(t),
                        t => Ty::Optional(Box::new(t)),
                    },
                    Ty::Optional(_) => {
                        self.maybe_none(base, &format!("`.{}`", name.name));
                        Ty::Any
                    }
                    t => self.field(&t, name),
                }
            }
            ExprKind::Call { callee, args } => self.call(callee, args),
            ExprKind::Await(inner) | ExprKind::Spawn(inner) | ExprKind::Try(inner) => {
                self.expr(inner);
                Ty::Any
            }
            ExprKind::Move(inner) => {
                let t = self.expr(inner);
                if let ExprKind::Name(n) = &inner.kind {
                    if let Some(v) = self.lookup_mut(&n.name) {
                        v.moved = Some(Moved {
                            at: e.span,
                            surely: true,
                        });
                    }
                }
                t
            }
            ExprKind::If {
                then_value,
                condition,
                else_value,
            } => {
                self.expr(condition);
                let start = self.move_state();
                let a = self.expr(then_value);
                let after_then = self.move_state();
                self.set_move_state(&start);
                let b = self.expr(else_value);
                let after_else = self.move_state();
                let joined = Self::join(&[after_then, after_else]);
                self.set_move_state(&joined);
                if a == b {
                    a
                } else {
                    Ty::Any
                }
            }
            ExprKind::Lambda { params, body, .. } => {
                self.begin_fn("<lambda>", None, false);
                for p in params {
                    self.param(p);
                }
                self.expr(body);
                self.end_fn();
                Ty::Any
            }
        }
    }

    fn clauses(&mut self, clauses: &[CompClause]) {
        for c in clauses {
            match c {
                CompClause::For { target, iter } => {
                    let t = self.expr(iter);
                    let item = match t {
                        Ty::List(t) => *t,
                        Ty::Range => Ty::Int,
                        _ => Ty::Any,
                    };
                    self.bind_target(target, item, Kind::Loop { is_mut: false });
                }
                CompClause::If(cond) => {
                    self.expr(cond);
                }
            }
        }
    }

    fn binary(&mut self, op: BinaryOp, a: &Ty, b: &Ty, span: Span) -> Ty {
        use BinaryOp as B;
        if matches!(op, B::And | B::Or) {
            return if a == b { a.clone() } else { Ty::Any };
        }
        let symbol = match op {
            B::Add => "+",
            B::Sub => "-",
            B::Mul => "*",
            B::Div => "/",
            B::FloorDiv => "//",
            B::Rem => "%",
            B::Pow => "**",
            B::BitAnd => "&",
            B::BitOr => "|",
            B::BitXor => "^",
            B::Shl => "<<",
            B::Shr => ">>",
            B::And | B::Or => unreachable!(),
        };
        let numeric = |t: &Ty| matches!(t, Ty::Int | Ty::Float);
        match (a, b) {
            (Ty::Int, Ty::Int) => match op {
                B::Div => Ty::Float,
                B::Pow => Ty::Any,
                _ => Ty::Int,
            },
            (x, y) if numeric(x) && numeric(y) => match op {
                B::BitAnd | B::BitOr | B::BitXor | B::Shl | B::Shr => Ty::Any,
                _ => Ty::Float,
            },
            (Ty::Str, Ty::Str) if op == B::Add => Ty::Str,
            (Ty::Str, Ty::Int) | (Ty::Int, Ty::Str) if op == B::Mul => Ty::Str,
            (Ty::Tensor, Ty::Tensor | Ty::Int | Ty::Float) | (Ty::Int | Ty::Float, Ty::Tensor) => {
                Ty::Tensor
            }
            (Ty::List(x), Ty::List(y)) if op == B::Add => {
                if x == y {
                    a.clone()
                } else {
                    Ty::List(Box::new(Ty::Any))
                }
            }
            (x, y) if x.is_scalar() && y.is_scalar() => {
                self.error(
                    format!("cannot apply `{symbol}` to {} and {}", x.name(), y.name()),
                    span,
                );
                Ty::Any
            }
            _ => Ty::Any,
        }
    }

    fn field(&mut self, t: &Ty, name: &Ident) -> Ty {
        let Ty::Named(ty) = t else {
            return Ty::Any;
        };
        if let Some(f) = self.table.field(ty, &name.name) {
            return f;
        }
        if self.table.method(ty, &name.name).is_some() {
            return Ty::Any;
        }
        if self.table.fully_known(ty) && !self.table.enums.contains(ty) {
            self.error(format!("`{ty}` has no field `{}`", name.name), name.span);
        }
        Ty::Any
    }

    fn call(&mut self, callee: &Expr, args: &[Arg]) -> Ty {
        // What is being called, and its signature if known.
        let mut receiver: Option<(&Expr, Ty)> = None;
        let mut sig: Option<FnSig> = None;
        let mut name = String::new();
        let mut result = Ty::Any;
        match &callee.kind {
            ExprKind::Name(n) if self.lookup(&n.name).is_none() => {
                name = n.name.clone();
                if self.table.raw_externs.contains(&n.name) && self.unsafe_depth == 0 {
                    self.error_hint(
                        format!("`{}` passes raw pointers to C, so it can only be called in an `unsafe:` block", n.name),
                        callee.span,
                        "put the call in `unsafe:` ... `end`: you promise the C code reads and writes only what it was given, and keeps no pointer to Vyrn memory after it returns",
                    );
                }
                if let Some(s) = self.table.fns.get(&n.name) {
                    sig = Some(s.clone());
                    result = s.ret.clone().unwrap_or(Ty::Any);
                } else if let Some(info) = self.table.type_info(&n.name) {
                    result = Ty::Named(n.name.clone());
                    if self.table.fully_known(&n.name) && info.parent.is_none() {
                        sig = Some(FnSig {
                            params: info
                                .fields
                                .iter()
                                .map(|(f, t)| ParamInfo {
                                    name: f.clone(),
                                    ty: t.clone(),
                                    pass: PassMode::Borrow,
                                })
                                .collect(),
                            ret: Some(result.clone()),
                            elementwise: false,
                        });
                    }
                } else {
                    result = builtin_result(&n.name);
                }
            }
            ExprKind::Field {
                base,
                name: m,
                optional,
            } => {
                let is_value = match &base.kind {
                    ExprKind::Name(n) => self.lookup(&n.name).is_some(),
                    _ => true,
                };
                if is_value {
                    let mut t = self.expr(base);
                    name = m.name.clone();
                    if let Ty::Optional(inner) = &t {
                        if !*optional {
                            self.maybe_none(base, &format!("`.{}()`", m.name));
                        }
                        t = (**inner).clone();
                    }
                    if let Ty::Named(ty) = &t {
                        if let Some(s) = self.table.method(ty, &m.name) {
                            result = s.ret.clone().unwrap_or(Ty::Any);
                            sig = Some(s.clone());
                        } else if self.table.field(ty, &m.name).is_none()
                            && self.table.fully_known(ty)
                            && !self.table.enums.contains(ty)
                        {
                            self.error(format!("`{ty}` has no method `{}`", m.name), m.span);
                        }
                    }
                    receiver = Some((base, t));
                }
            }
            _ => {
                self.expr(callee);
            }
        }

        // A method taking `mut self` changes its receiver.
        let mut params: &[ParamInfo] = sig.as_ref().map_or(&[], |s| &s.params);
        if let Some((base, _)) = &receiver {
            if let Some(first) = params.first().filter(|p| p.name == "self") {
                if first.pass == PassMode::Mut {
                    if let Some(root) = Self::root_of(base) {
                        let root = root.clone();
                        let var = self.lookup(&root.name).cloned();
                        let is_value_type = var.as_ref().is_some_and(|v| {
                            matches!(&v.ty, Ty::Named(t)
                            if self.table.type_info(t).is_some_and(|i| i.is_struct))
                        });
                        if var.is_some_and(|v| v.kind == Kind::Let) && is_value_type {
                            self.check_changeable(
                                &root,
                                base.span,
                                &format!("call `mut self` method `{name}` on"),
                            );
                        }
                    }
                }
                params = &params[1..];
            }
        }

        // Arguments.
        let mut mut_roots: Vec<(String, Span)> = Vec::new();
        let mut other_roots: Vec<String> = Vec::new();
        if let Some((base, _)) = &receiver {
            if let Some(r) = Self::root_of(base) {
                other_roots.push(r.name.clone());
            }
        }
        let mut positional = 0;
        // What each type parameter stands for in this call.
        let mut bindings: HashMap<String, (Ty, Span)> = HashMap::new();
        let elementwise = sig.as_ref().is_some_and(|s| s.elementwise);
        let mut on_tensors = false;
        for arg in args {
            let t = self.expr(&arg.value);
            let param = match &arg.name {
                Some(n) => params.iter().find(|p| p.name == n.name),
                None => {
                    positional += 1;
                    params.get(positional - 1)
                }
            };
            let root = Self::root_of(&arg.value).cloned();
            if arg.is_mut {
                if let Some(r) = &root {
                    self.check_changeable(r, arg.value.span, "pass as `mut`");
                    mut_roots.push((r.name.clone(), arg.value.span));
                }
            } else if let Some(r) = &root {
                other_roots.push(r.name.clone());
            }
            if let Some(p) = param {
                if let Err((var, earlier, now)) =
                    self.table.unify(&p.ty, &t, arg.value.span, &mut bindings)
                {
                    self.error_hint(
                        format!(
                            "`{var}` in `{name}` stands for {} from an earlier argument, but this is {}",
                            earlier.name(),
                            now.name()
                        ),
                        arg.value.span,
                        format!("every `{var}` in one call must be the same type"),
                    );
                    // One error is enough for this argument.
                } else if elementwise && t == Ty::Tensor && matches!(p.ty, Ty::Int | Ty::Float) {
                    // A kernel applied to a tensor, element by element.
                    on_tensors = true;
                } else {
                    let expected = subst(&p.ty, &bindings);
                    self.expect(&expected, &t, arg.value.span, || {
                        format!("argument `{}` of `{name}`", p.name)
                    });
                }
                let is_place = root.is_some()
                    && matches!(
                        arg.value.kind,
                        ExprKind::Name(_) | ExprKind::Field { .. } | ExprKind::Index { .. }
                    );
                if p.pass == PassMode::Mut && !arg.is_mut && is_place {
                    let text = root.as_ref().map(|r| r.name.clone()).unwrap_or_default();
                    self.error_hint(
                        format!(
                            "`{name}` changes its argument `{}`; pass it with `mut`",
                            p.name
                        ),
                        arg.value.span,
                        format!("write `mut {text}` to show the call changes it"),
                    );
                }
            }
        }
        for (i, (root, at)) in mut_roots.iter().enumerate() {
            let twice = mut_roots[..i].iter().any(|(r, _)| r == root);
            let shared = other_roots.contains(root);
            if twice || shared {
                self.error_hint(
                    format!("`{root}` is passed as `mut` and also used elsewhere in this call"),
                    *at,
                    "a `mut` argument must be the only way the call can reach that value",
                );
            }
        }
        if on_tensors {
            return Ty::Tensor;
        }
        subst(&result, &bindings)
    }
}

/// The type of a list or map built from values of these types.
fn common(tys: &[Ty]) -> Ty {
    match tys.split_first() {
        None => Ty::Any,
        Some((first, rest)) if rest.iter().all(|t| t == first) => first.clone(),
        Some(_) => {
            if tys.iter().all(|t| matches!(t, Ty::Int | Ty::Float)) {
                Ty::Float
            } else {
                Ty::Any
            }
        }
    }
}

/// What a builtin function returns, when it is always the same.
fn builtin_result(name: &str) -> Ty {
    match name {
        "len" | "int" => Ty::Int,
        "str" | "format_bytes" | "read_text" | "to_json" => Ty::Str,
        "float" | "sqrt" | "now" => Ty::Float,
        "range" => Ty::Range,
        "zeros" | "ones" | "rand" | "arange" | "eye" | "tensor" | "vec2" | "vec3" | "vec4"
        | "softmax" | "relu" | "gelu" => Ty::Tensor,
        "print" => Ty::None,
        _ => Ty::Any,
    }
}

/// Whether running `stmts` always ends in `return` or `fail` (or never
/// ends), so control cannot fall off the end.
fn block_always_exits(stmts: &[Stmt]) -> bool {
    stmts.iter().any(stmt_always_exits)
}

fn stmt_always_exits(stmt: &Stmt) -> bool {
    match &stmt.kind {
        StmtKind::Return(_) | StmtKind::Fail(_) => true,
        StmtKind::If(s) => {
            s.else_body
                .as_ref()
                .is_some_and(|b| block_always_exits(&b.stmts))
                && s.branches.iter().all(|(_, b)| block_always_exits(&b.stmts))
        }
        StmtKind::Match(m) => {
            m.arms.iter().all(|a| block_always_exits(&a.body.stmts))
                && m.else_body
                    .as_ref()
                    .is_none_or(|b| block_always_exits(&b.stmts))
        }
        // `loop` without `stop` never finishes.
        StmtKind::Loop(body) => !contains_stop(&body.stmts),
        StmtKind::Try(t) => {
            block_always_exits(&t.body.stmts)
                && t.catches.iter().all(|c| block_always_exits(&c.body.stmts))
        }
        StmtKind::With { body, .. } | StmtKind::Unsafe(body) => block_always_exits(&body.stmts),
        StmtKind::On(o) => block_always_exits(&o.body.stmts),
        StmtKind::Region(r) => block_always_exits(&r.body.stmts),
        _ => false,
    }
}

/// Whether a `stop` in `stmts` leaves the loop they are the body of.
fn contains_stop(stmts: &[Stmt]) -> bool {
    stmts.iter().any(|s| match &s.kind {
        StmtKind::Stop => true,
        StmtKind::If(i) => {
            i.branches.iter().any(|(_, b)| contains_stop(&b.stmts))
                || i.else_body
                    .as_ref()
                    .is_some_and(|b| contains_stop(&b.stmts))
        }
        StmtKind::Match(m) => {
            m.arms.iter().any(|a| contains_stop(&a.body.stmts))
                || m.else_body
                    .as_ref()
                    .is_some_and(|b| contains_stop(&b.stmts))
        }
        StmtKind::Try(t) => {
            contains_stop(&t.body.stmts)
                || t.catches.iter().any(|c| contains_stop(&c.body.stmts))
                || t.finally.as_ref().is_some_and(|b| contains_stop(&b.stmts))
        }
        StmtKind::With { body, .. } | StmtKind::Unsafe(body) => contains_stop(&body.stmts),
        StmtKind::On(o) => contains_stop(&o.body.stmts),
        StmtKind::Region(r) => contains_stop(&r.body.stmts),
        // A nested loop's `stop` leaves that loop.
        _ => false,
    })
}

/// Adds the cases `pattern` handles to `covered`. Returns true when it
/// handles everything, as a name or `_` does.
fn covers(pattern: &Pattern, ty: &Ty, cases: &[String], covered: &mut HashSet<String>) -> bool {
    let irrefutable =
        |p: &Pattern| matches!(p.kind, PatternKind::Wildcard | PatternKind::Binding(_));
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Binding(_) => {
            if matches!(ty, Ty::Optional(_)) {
                // A name matches `none` too.
                covered.insert("none".into());
                covered.insert("a value".into());
            }
            true
        }
        PatternKind::As { pattern, .. } => covers(pattern, ty, cases, covered),
        PatternKind::Or(alternatives) => {
            let mut all = false;
            for p in alternatives {
                all |= covers(p, ty, cases, covered);
            }
            all
        }
        PatternKind::Literal(LiteralPattern::Bool(b)) => {
            covered.insert(b.to_string());
            false
        }
        PatternKind::Literal(LiteralPattern::None) => {
            covered.insert("none".into());
            false
        }
        PatternKind::Value(path) => {
            let name = path_text(path);
            if cases.contains(&name) {
                covered.insert(name);
            }
            false
        }
        PatternKind::Destructure { path, args } => {
            let name = path_text(path);
            // `Shape.Rect(w, h)` handles every Rect; `Shape.Rect(0, h)` does not.
            if cases.contains(&name) && args.iter().all(|a| irrefutable(&a.pattern)) {
                covered.insert(name);
            }
            false
        }
        _ => false,
    }
}

fn path_text(path: &Path) -> String {
    path.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// `u.email` for a place expression, for messages.
fn place_text(e: &Expr) -> Option<String> {
    match &e.kind {
        ExprKind::Name(n) => Some(n.name.clone()),
        ExprKind::Field { base, name, .. } => Some(format!("{}.{}", place_text(base)?, name.name)),
        _ => None,
    }
}

/// `t` with each type parameter replaced by what it stands for; unknown
/// ones become `any`.
fn subst(t: &Ty, bindings: &HashMap<String, (Ty, Span)>) -> Ty {
    match t {
        Ty::Var(v) => bindings.get(v).map_or(Ty::Any, |(t, _)| t.clone()),
        Ty::List(i) => Ty::List(Box::new(subst(i, bindings))),
        Ty::Optional(i) => match subst(i, bindings) {
            Ty::Any => Ty::Any,
            t => Ty::Optional(Box::new(t)),
        },
        Ty::Map(k, v) => Ty::Map(Box::new(subst(k, bindings)), Box::new(subst(v, bindings))),
        Ty::Tuple(items) => Ty::Tuple(items.iter().map(|i| subst(i, bindings)).collect()),
        other => other.clone(),
    }
}
