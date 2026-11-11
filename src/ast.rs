//! Abstract syntax tree for Vyrn (grammar v0.4).
//!
//! The parser will build these nodes; nothing here parses anything. The tree
//! is organised in five layers, each mirroring a section of the grammar:
//!
//! | Layer | Root type | Grammar section |
//! |---|---|---|
//! | Declarations | [`Item`] | 1, 2 |
//! | Statements | [`Stmt`] | 3 |
//! | Types as written | [`TypeExpr`] | 4 |
//! | Expressions | [`Expr`] | 5 |
//! | Patterns | [`Pattern`] | 6 |
//!
//! # Type information
//!
//! Syntax and meaning are kept apart. A type the programmer *wrote* is a
//! [`TypeExpr`]. The type the compiler *decided on* goes into a [`TypeSlot`].
//! Every node that has a type carries a slot: expressions, bindings,
//! parameters, fields and function signatures. The parser leaves each slot
//! [`TypeSlot::Pending`]; semantic analysis fills it with either
//! [`TypeSlot::Static`] or, for code the programmer left untyped and the
//! compiler could not infer, [`TypeSlot::Dynamic`]. That last case is what
//! makes the typing gradual.
//!
//! Placement (`on gpu[0]`, `on frame`) follows the same pattern with
//! [`PlaceSlot`].

use crate::span::Span;
use crate::token::{FloatSuffix, IntSuffix, StrPart};

// ---------------------------------------------------------------------------
// Identity and semantic slots
// ---------------------------------------------------------------------------

/// Identifies a node within one module. Assigned by the parser, unique per
/// node, and used by later phases to key side tables.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

/// Index into the type table that semantic analysis will own.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// The resolved type of a node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum TypeSlot {
    /// Not analysed yet. The only value the parser produces.
    #[default]
    Pending,
    /// Known only at run time: the value is boxed and operations on it are
    /// dispatched dynamically.
    Dynamic,
    /// Known at compile time.
    Static(TypeId),
}

impl TypeSlot {
    pub fn is_resolved(self) -> bool {
        self != TypeSlot::Pending
    }

    pub fn static_type(self) -> Option<TypeId> {
        match self {
            TypeSlot::Static(id) => Some(id),
            _ => None,
        }
    }
}

/// Index into the table of devices and regions that semantic analysis owns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlaceId(pub u32);

/// Where a value is stored or a block executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PlaceSlot {
    /// Not analysed yet.
    #[default]
    Pending,
    /// No explicit placement; inherited from the enclosing `on` block.
    Inherited,
    /// A device or region known at compile time.
    Static(PlaceId),
    /// A device chosen at run time, as in `on layer.device`.
    Dynamic,
}

// ---------------------------------------------------------------------------
// Names
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Ident {
    pub name: String,
    pub span: Span,
}

/// A dotted name: `vyrn.web`, `Shape.Circle`.
#[derive(Clone, Debug, PartialEq)]
pub struct Path {
    pub segments: Vec<Ident>,
    pub span: Span,
}

// ---------------------------------------------------------------------------
// Module and items
// ---------------------------------------------------------------------------

/// One source file.
#[derive(Clone, Debug, PartialEq)]
pub struct Module {
    pub items: Vec<Item>,
    pub span: Span,
}

impl Module {
    /// The program entry point, if this file has one.
    pub fn main(&self) -> Option<&MainDecl> {
        self.items.iter().find_map(|item| match &item.kind {
            ItemKind::Main(main) => Some(main),
            _ => None,
        })
    }

    /// Every route in the file, including those nested in `routes` groups.
    pub fn routes(&self) -> Vec<&RouteDecl> {
        fn collect<'a>(items: &'a [Item], out: &mut Vec<&'a RouteDecl>) {
            for item in items {
                match &item.kind {
                    ItemKind::Route(route) => out.push(route),
                    ItemKind::RouteGroup(group) => collect(&group.items, out),
                    _ => {}
                }
            }
        }
        let mut out = Vec::new();
        collect(&self.items, &mut out);
        out
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum Visibility {
    #[default]
    Private,
    Public,
}

/// `@name(args)` on the line before a declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct Decorator {
    pub name: Path,
    pub args: Vec<Arg>,
    pub span: Span,
}

/// A declaration together with what may precede any declaration.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub id: NodeId,
    pub span: Span,
    /// Text of the `##` comments directly above, one entry per line.
    pub doc: Vec<String>,
    pub decorators: Vec<Decorator>,
    pub vis: Visibility,
    pub kind: ItemKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ItemKind {
    Use(UseDecl),
    Main(MainDecl),
    Const(ConstDecl),
    TypeAlias(TypeAliasDecl),
    Fn(FnDecl),
    /// Both `struct` and `class`; see [`AggregateKind`].
    Aggregate(AggregateDecl),
    Enum(EnumDecl),
    Trait(TraitDecl),
    Impl(ImplDecl),
    Route(RouteDecl),
    RouteGroup(RouteGroup),
    Extern(ExternDecl),
}

/// `use vyrn.web`, `use vyrn.web as w`, `use app.storage (Db, Cache as C)`.
#[derive(Clone, Debug, PartialEq)]
pub struct UseDecl {
    /// The module path. For a URL import this is the name it is bound
    /// to: `use "github.com/ann/neural@v1"` gives `neural`.
    pub path: Path,
    pub kind: UseKind,
    /// `use "github.com/ann/neural@v1"`: a module fetched from a URL.
    pub url: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum UseKind {
    /// Brings the module itself into scope, optionally renamed.
    Module { alias: Option<Ident> },
    /// Brings the listed names into scope.
    Names(Vec<UseName>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct UseName {
    pub name: Ident,
    pub alias: Option<Ident>,
}

/// The `main ... end` block. `params` is empty for plain `main`.
#[derive(Clone, Debug, PartialEq)]
pub struct MainDecl {
    pub params: Vec<Param>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ConstDecl {
    pub name: Ident,
    pub declared_ty: Option<TypeExpr>,
    pub value: Expr,
    pub ty: TypeSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TypeAliasDecl {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub target: TypeExpr,
}

/// `T` or `T: Ord + Hash` in a generic parameter list.
#[derive(Clone, Debug, PartialEq)]
pub struct GenericParam {
    pub name: Ident,
    pub bounds: Vec<TypeExpr>,
    pub span: Span,
}

// ---- functions ------------------------------------------------------------

/// The modifiers that may precede `fn`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FnModifiers {
    pub is_async: bool,
    /// Every binding must be statically typed.
    pub is_strict: bool,
    /// Strict, and compiled through the tensor backend.
    pub is_kernel: bool,
    /// `None`: not exported. `Some(None)`: `export fn`. `Some(Some(abi))`:
    /// `export "wasm" fn`.
    pub export: Option<Option<String>>,
}

/// How an argument is passed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum PassMode {
    /// Shared read-only borrow. The default.
    #[default]
    Borrow,
    /// `mut`: exclusive mutable borrow.
    Mut,
    /// `own`: ownership moves to the callee.
    Own,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Param {
    pub id: NodeId,
    pub span: Span,
    pub pass: PassMode,
    pub name: Ident,
    /// `None` when the parameter is untyped.
    pub declared_ty: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub ty: TypeSlot,
    /// `args...`: takes every remaining argument, as a list.
    pub variadic: bool,
}

/// `-> T`, `fails E`, or `-> T fails E`.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ResultSpec {
    pub returns: Option<TypeExpr>,
    pub fails: Option<TypeExpr>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum FnBody {
    /// `fn f(): stmt` or `fn f():` ... `end`. The one-line form is a block
    /// holding a single statement.
    Block(Block),
    /// A signature with no body, in a `trait` or `extern` declaration.
    Signature,
}

#[derive(Clone, Debug, PartialEq)]
pub struct FnDecl {
    pub name: Ident,
    pub modifiers: FnModifiers,
    pub generics: Vec<GenericParam>,
    pub params: Vec<Param>,
    pub result: ResultSpec,
    pub body: FnBody,
    /// The function's own type, once resolved.
    pub ty: TypeSlot,
}

// ---- aggregates -----------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AggregateKind {
    /// Value type, statically sized, stored inline.
    Struct,
    /// Reference type, heap allocated, reference counted.
    Class,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AggregateDecl {
    pub kind: AggregateKind,
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    /// The base class in `class Enemy(Actor):`. Always `None` for a struct.
    pub base: Option<TypeExpr>,
    pub fields: Vec<FieldDecl>,
    /// Each is an [`ItemKind::Fn`].
    pub methods: Vec<Item>,
    /// Fields and methods marked `private`: only the type's own methods
    /// can use them.
    pub private: Vec<String>,
}

/// `name: T`, `name: T = default`, or `name = default`.
#[derive(Clone, Debug, PartialEq)]
pub struct FieldDecl {
    pub id: NodeId,
    pub span: Span,
    pub vis: Visibility,
    pub name: Ident,
    pub declared_ty: Option<TypeExpr>,
    pub default: Option<Expr>,
    pub ty: TypeSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EnumDecl {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub variants: Vec<Variant>,
    /// Each is an [`ItemKind::Fn`].
    pub methods: Vec<Item>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Variant {
    pub id: NodeId,
    pub span: Span,
    pub name: Ident,
    pub fields: Vec<VariantField>,
    /// `Name = expr`, for enums with explicit values.
    pub discriminant: Option<Expr>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct VariantField {
    pub name: Option<Ident>,
    pub ty: TypeExpr,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TraitDecl {
    pub name: Ident,
    pub generics: Vec<GenericParam>,
    pub supertraits: Vec<TypeExpr>,
    /// Signatures ([`FnBody::Signature`]), or default methods with a body.
    pub methods: Vec<FnDecl>,
}

/// `impl Type` or `impl Trait for Type`.
#[derive(Clone, Debug, PartialEq)]
pub struct ImplDecl {
    pub generics: Vec<GenericParam>,
    pub trait_: Option<TypeExpr>,
    pub target: TypeExpr,
    /// Each is an [`ItemKind::Fn`].
    pub methods: Vec<Item>,
}

/// `extern "abi" fn ...` or an `extern "abi" ... end` block.
#[derive(Clone, Debug, PartialEq)]
pub struct ExternDecl {
    pub abi: String,
    /// Signatures only: every body is [`FnBody::Signature`].
    pub functions: Vec<FnDecl>,
}

// ---- web routing ----------------------------------------------------------

/// The HTTP methods a route can bind to. `Ws` and `Sse` are upgrade-style
/// endpoints (WebSocket, server-sent events) rather than HTTP verbs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum HttpMethod {
    Get,
    Post,
    Put,
    Patch,
    Delete,
    Head,
    Options,
    Ws,
    Sse,
}

impl HttpMethod {
    /// Recognises the contextual keyword that follows `route`. The lexer
    /// delivers these as ordinary identifiers.
    pub fn from_ident(name: &str) -> Option<HttpMethod> {
        Some(match name {
            "GET" => HttpMethod::Get,
            "POST" => HttpMethod::Post,
            "PUT" => HttpMethod::Put,
            "PATCH" => HttpMethod::Patch,
            "DELETE" => HttpMethod::Delete,
            "HEAD" => HttpMethod::Head,
            "OPTIONS" => HttpMethod::Options,
            "WS" => HttpMethod::Ws,
            "SSE" => HttpMethod::Sse,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            HttpMethod::Get => "GET",
            HttpMethod::Post => "POST",
            HttpMethod::Put => "PUT",
            HttpMethod::Patch => "PATCH",
            HttpMethod::Delete => "DELETE",
            HttpMethod::Head => "HEAD",
            HttpMethod::Options => "OPTIONS",
            HttpMethod::Ws => "WS",
            HttpMethod::Sse => "SSE",
        }
    }
}

/// One piece of a route path.
#[derive(Clone, Debug, PartialEq)]
pub enum PathSegment {
    /// Fixed text, such as `/users/`.
    Literal(String),
    /// A `{name}` hole. It must match a parameter of the route.
    Param(Ident),
}

/// A route path such as `"/users/{id}/summary"`.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutePath {
    pub segments: Vec<PathSegment>,
    pub span: Span,
}

/// Why a string is not a valid route path.
#[derive(Clone, Debug, PartialEq)]
pub struct RoutePathError {
    pub message: String,
    pub span: Span,
}

impl RoutePath {
    /// Builds a path from the parts of a string token. Each interpolation
    /// hole must contain exactly one identifier.
    pub fn from_str_parts(parts: &[StrPart], span: Span) -> Result<RoutePath, RoutePathError> {
        let mut segments = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                StrPart::Text(text) => segments.push(PathSegment::Literal(text.clone())),
                StrPart::Interp { source, span } => {
                    let name = source.trim();
                    let mut chars = name.chars();
                    let valid = chars.next().is_some_and(|c| c == '_' || c.is_alphabetic())
                        && chars.all(|c| c == '_' || c.is_alphanumeric());
                    if !valid {
                        return Err(RoutePathError {
                            message: format!(
                                "`{{{source}}}` is not a path parameter; write a single name, as in `{{id}}`"
                            ),
                            span: *span,
                        });
                    }
                    segments.push(PathSegment::Param(Ident {
                        name: name.to_string(),
                        span: *span,
                    }));
                }
            }
        }
        match segments.first() {
            Some(PathSegment::Literal(text)) if text.starts_with('/') => {
                Ok(RoutePath { segments, span })
            }
            _ => Err(RoutePathError {
                message: "a route path must start with `/`".to_string(),
                span,
            }),
        }
    }

    /// The names of the `{param}` holes, in order.
    pub fn params(&self) -> impl Iterator<Item = &Ident> {
        self.segments.iter().filter_map(|segment| match segment {
            PathSegment::Param(name) => Some(name),
            PathSegment::Literal(_) => None,
        })
    }

    /// The path as written, with holes shown as `{name}`.
    pub fn template(&self) -> String {
        self.segments
            .iter()
            .map(|segment| match segment {
                PathSegment::Literal(text) => text.clone(),
                PathSegment::Param(name) => format!("{{{}}}", name.name),
            })
            .collect()
    }
}

/// `route GET "/users/{id}" (id: u64, db: Db) -> User fails NotFound:`.
///
/// A route body is implicitly async.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteDecl {
    pub method: HttpMethod,
    pub method_span: Span,
    pub path: RoutePath,
    /// Path parameters, the request body, and injected providers. Semantic
    /// analysis classifies each one and records it in a side table.
    pub params: Vec<Param>,
    pub result: ResultSpec,
    pub body: Block,
    /// The type of the response payload, once resolved.
    pub response_ty: TypeSlot,
}

/// `routes "/admin" ... end`. Nested routes inherit the prefix.
#[derive(Clone, Debug, PartialEq)]
pub struct RouteGroup {
    pub prefix: RoutePath,
    /// Each is an [`ItemKind::Route`] or [`ItemKind::RouteGroup`]. Decorators
    /// on the items carry middleware such as `@require(role = "admin")`.
    pub items: Vec<Item>,
}

// ---------------------------------------------------------------------------
// Statements
// ---------------------------------------------------------------------------

/// The statements between a header and its `end`.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stmt {
    pub id: NodeId,
    pub span: Span,
    pub kind: StmtKind,
}

#[derive(Clone, Debug, PartialEq)]
pub enum StmtKind {
    Binding(Binding),
    /// An expression evaluated for its effect.
    Expr(Expr),
    /// `target = value` or `target op= value`. A multiple assignment has
    /// [`ExprKind::Tuple`] on either side. As in Python, assigning to a
    /// name not yet in scope declares it; semantic analysis decides which
    /// case applies and rejects it in `strict` code.
    Assign {
        target: Expr,
        op: AssignOp,
        value: Expr,
    },
    Return(Option<Expr>),
    Fail(Expr),
    /// `yield value`: makes the function a generator.
    Yield(Expr),
    Defer(Expr),
    /// Leaves the innermost loop.
    Stop,
    /// Continues with the next iteration of the innermost loop.
    Skip,
    If(IfStmt),
    While {
        condition: Condition,
        body: Block,
    },
    Loop(Block),
    For(ForStmt),
    Match(MatchStmt),
    Try(TryStmt),
    With {
        is_async: bool,
        items: Vec<WithItem>,
        body: Block,
    },
    On(OnBlock),
    Region(RegionBlock),
    Unsafe(Block),
    /// A function declared inside another function.
    Item(Box<Item>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BindingKind {
    /// `let`: cannot be reassigned.
    Let,
    /// `var`: can be reassigned.
    Var,
    /// `const`: evaluated at compile time.
    Const,
}

/// `let x = 1`, `var world: [Entity; N] = ...`, `let a: [T] on frame = ...`.
#[derive(Clone, Debug, PartialEq)]
pub struct Binding {
    pub kind: BindingKind,
    pub target: BindTarget,
    /// The annotation as written, including any `on` placement.
    pub declared_ty: Option<TypeExpr>,
    pub value: Option<Expr>,
    pub ty: TypeSlot,
    pub place: PlaceSlot,
}

/// The names introduced by a binding or an `each` loop.
#[derive(Clone, Debug, PartialEq)]
pub enum BindTarget {
    Name(Ident),
    /// `_`
    Discard(Span),
    /// `a, b` or `(a, (b, c))`
    Tuple(Vec<BindTarget>, Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AssignOp {
    /// `=`
    Assign,
    Add,
    Sub,
    /// `*=`: element-wise.
    Mul,
    Div,
    /// `//=`
    FloorDiv,
    /// `%=`
    Rem,
    /// `@=`: matrix product in place.
    MatMul,
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    /// `??=`: assign only if the target is `none`.
    Coalesce,
}

/// What follows `if`, `elif` or `while`.
#[derive(Clone, Debug, PartialEq)]
pub enum Condition {
    Expr(Expr),
    /// `if let pattern = value`
    Let {
        pattern: Pattern,
        value: Expr,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct IfStmt {
    /// The `if` branch followed by each `elif`, in source order.
    pub branches: Vec<(Condition, Block)>,
    pub else_body: Option<Block>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub enum ForMode {
    #[default]
    Sequential,
    /// `par for`: iterations may run in parallel.
    Parallel,
    /// `async each`: each item is awaited from a stream.
    Async,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ForStmt {
    pub mode: ForMode,
    /// How each element is bound: read-only, `mut` or `own`.
    pub pass: PassMode,
    pub target: BindTarget,
    pub iter: Expr,
    pub body: Block,
    /// The element type, once resolved.
    pub item_ty: TypeSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MatchStmt {
    pub scrutinee: Expr,
    pub arms: Vec<MatchArm>,
    pub else_body: Option<Block>,
}

/// `is pattern [if guard]: stmt`, or the same header followed by a block.
#[derive(Clone, Debug, PartialEq)]
pub struct MatchArm {
    pub span: Span,
    pub pattern: Pattern,
    pub guard: Option<Expr>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TryStmt {
    pub body: Block,
    pub catches: Vec<CatchClause>,
    pub finally: Option<Block>,
}

/// `catch`, `catch IoError`, or `catch IoError as e`.
#[derive(Clone, Debug, PartialEq)]
pub struct CatchClause {
    pub span: Span,
    /// `None` catches every failure.
    pub error_ty: Option<TypeExpr>,
    pub binding: Option<Ident>,
    pub body: Block,
}

#[derive(Clone, Debug, PartialEq)]
pub struct WithItem {
    pub value: Expr,
    pub binding: Option<Ident>,
}

/// `on target ... end`: allocations and tensor kernels inside the block use
/// the given device or region.
#[derive(Clone, Debug, PartialEq)]
pub struct OnBlock {
    pub target: Expr,
    pub body: Block,
    pub place: PlaceSlot,
}

/// `region name ... end`: declares a memory arena that is released in one
/// step when the block ends.
#[derive(Clone, Debug, PartialEq)]
pub struct RegionBlock {
    pub name: Ident,
    pub body: Block,
    pub place: PlaceSlot,
}

// ---------------------------------------------------------------------------
// Types as written
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct TypeExpr {
    pub id: NodeId,
    pub span: Span,
    pub kind: TypeExprKind,
    /// What this annotation resolves to.
    pub resolved: TypeSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TypeExprKind {
    /// `any`: explicitly dynamic.
    Any,
    /// `Name` or `Name[args]`
    Named {
        path: Path,
        args: Vec<GenericArg>,
    },
    Tensor(TensorType),
    /// `[T]`: growable list.
    List(Box<TypeExpr>),
    /// `[T; N]`: fixed-size array stored inline.
    Array {
        element: Box<TypeExpr>,
        len: Box<Expr>,
    },
    /// `{K: V}`
    Map {
        key: Box<TypeExpr>,
        value: Box<TypeExpr>,
    },
    /// `(A, B)`; `()` is the unit type.
    Tuple(Vec<TypeExpr>),
    /// `fn(A, B) -> C`
    Fn {
        params: Vec<TypeExpr>,
        returns: Option<Box<TypeExpr>>,
    },
    /// `T?`
    Optional(Box<TypeExpr>),
    /// `A | B`
    Union(Vec<TypeExpr>),
    /// `ref T`, `shared T`, ...
    Owned {
        ownership: Ownership,
        inner: Box<TypeExpr>,
    },
    /// `T on target`: storage pinned to a device or region.
    Placed {
        inner: Box<TypeExpr>,
        target: Box<Expr>,
        place: PlaceSlot,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum GenericArg {
    Type(TypeExpr),
    /// An integer argument, as in `vec[f32, 3]`.
    Int(u64, Span),
}

/// `tensor[dtype; d0, d1, ...]`. With no dimensions the rank is unchecked.
#[derive(Clone, Debug, PartialEq)]
pub struct TensorType {
    pub dtype: Box<TypeExpr>,
    pub dims: Vec<Dim>,
}

/// One dimension of a tensor shape.
#[derive(Clone, Debug, PartialEq)]
pub enum Dim {
    /// A constant expression or a shape variable: `768`, `T`, `4 * D`.
    Expr(Expr),
    /// `_`: this dimension is not checked.
    Unchecked(Span),
    /// `...`: any number of leading dimensions.
    Variadic(Span),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ownership {
    /// `ref T`: borrowed view.
    Ref,
    /// `ref mut T`: exclusive borrowed view.
    RefMut,
    /// `shared T`: reference counted.
    Shared,
    /// `weak T`: non-owning reference to a shared value.
    Weak,
    /// `ptr T`: raw pointer.
    Ptr,
}

// ---------------------------------------------------------------------------
// Expressions
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Expr {
    pub id: NodeId,
    pub span: Span,
    pub kind: ExprKind,
    /// Filled in by semantic analysis.
    pub ty: TypeSlot,
}

impl Expr {
    /// A new expression with its type still pending.
    pub fn new(id: NodeId, kind: ExprKind, span: Span) -> Expr {
        Expr {
            id,
            span,
            kind,
            ty: TypeSlot::Pending,
        }
    }

    /// True for the node kinds that exist specifically for tensor code.
    pub fn is_tensor_op(&self) -> bool {
        matches!(
            self.kind,
            ExprKind::MatMul { .. } | ExprKind::Placement { .. }
        )
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum ExprKind {
    // ---- literals and names ----
    Int {
        value: u64,
        suffix: Option<IntSuffix>,
    },
    Float {
        value: f64,
        suffix: Option<FloatSuffix>,
    },
    Str(Vec<StrSegment>),
    Bool(bool),
    None,
    Name(Ident),

    // ---- collections ----
    /// `(a, b)`, or a bare `a, b` in a return or assignment. `()` is unit.
    Tuple(Vec<Expr>),
    List(Vec<Expr>),
    Map(Vec<(Expr, Expr)>),
    /// `[element for x in xs if cond]`
    ListComp {
        element: Box<Expr>,
        clauses: Vec<CompClause>,
    },
    /// `{key: value for k, v in pairs}`
    MapComp {
        key: Box<Expr>,
        value: Box<Expr>,
        clauses: Vec<CompClause>,
    },

    // ---- operators ----
    Unary {
        op: UnaryOp,
        operand: Box<Expr>,
    },
    /// Scalar and element-wise arithmetic, bitwise and logical operators.
    /// `*` on tensors is the element-wise product.
    Binary {
        op: BinaryOp,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `a < b <= c`: `operands` has one more entry than `ops`.
    Compare {
        operands: Vec<Expr>,
        ops: Vec<CompareOp>,
    },
    /// `a to b`, `a until b`, optionally `by step`.
    Range {
        start: Box<Expr>,
        end: Box<Expr>,
        inclusive: bool,
        step: Option<Box<Expr>>,
    },
    /// `a ?? b`
    Coalesce {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `x |> f`
    Pipeline {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    // ---- tensor primitives ----
    /// `a @ b`: matrix product, batched over leading dimensions. Kept apart
    /// from [`ExprKind::Binary`] because it has its own shape rule and is the
    /// seed from which tensor regions are grown.
    MatMul {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    /// `value on target`: the value placed on a device or in a region.
    Placement {
        value: Box<Expr>,
        target: Box<Expr>,
        place: PlaceSlot,
    },
    /// `base[subscripts]`: indexing, slicing, masking and gathering.
    Index {
        base: Box<Expr>,
        subscripts: Vec<Subscript>,
    },

    // ---- access and calls ----
    /// `base.name`, or `base?.name` when `optional` is set.
    Field {
        base: Box<Expr>,
        name: Ident,
        optional: bool,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Arg>,
    },

    // ---- effects ----
    Await(Box<Expr>),
    Spawn(Box<Expr>),
    /// `try e`: unwrap, passing a failure to the caller.
    Try(Box<Expr>),
    Move(Box<Expr>),

    // ---- control ----
    /// `a if cond else b`
    If {
        then_value: Box<Expr>,
        condition: Box<Expr>,
        else_value: Box<Expr>,
    },
    /// `fn(x, y) => body`
    Lambda {
        is_async: bool,
        params: Vec<Param>,
        body: Box<Expr>,
    },
}

/// One piece of a string literal after its interpolations are parsed.
#[derive(Clone, Debug, PartialEq)]
pub enum StrSegment {
    Text(String),
    Interp(Expr),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Pos,
    BitNot,
    Not,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinaryOp {
    Add,
    Sub,
    /// `*`: element-wise on tensors.
    Mul,
    /// `/`: true division.
    Div,
    /// `//`: floor division.
    FloorDiv,
    /// `%`
    Rem,
    /// `**`
    Pow,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
    And,
    Or,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CompareOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    In,
    NotIn,
}

/// One entry between `[` and `]`.
#[derive(Clone, Debug, PartialEq)]
pub enum Subscript {
    /// An index (negative counts from the end), a boolean mask, an index
    /// tensor, or `none` to insert a new axis. Which one is decided by the
    /// type of the expression.
    Index(Expr),
    /// `start:stop:step`, any part optional: `:`, `1:`, `:-1`, `::2`.
    Slice {
        start: Option<Box<Expr>>,
        stop: Option<Box<Expr>>,
        step: Option<Box<Expr>>,
        span: Span,
    },
    /// `...`: all remaining leading dimensions.
    Ellipsis(Span),
}

/// A call argument: `x`, `mut x`, or `name = x`.
#[derive(Clone, Debug, PartialEq)]
pub struct Arg {
    pub span: Span,
    pub name: Option<Ident>,
    /// The caller wrote `mut`, acknowledging that the callee may modify it.
    pub is_mut: bool,
    pub value: Expr,
    /// `xs...`: the items of a list become separate arguments.
    pub spread: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompClause {
    For { target: BindTarget, iter: Expr },
    If(Expr),
}

// ---------------------------------------------------------------------------
// Patterns
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    pub id: NodeId,
    pub span: Span,
    pub kind: PatternKind,
    /// The type of the value this pattern matches against.
    pub ty: TypeSlot,
}

#[derive(Clone, Debug, PartialEq)]
pub enum PatternKind {
    /// `_`
    Wildcard,
    Literal(LiteralPattern),
    /// `1 to 9` or `0 until 10`
    Range {
        start: LiteralPattern,
        end: LiteralPattern,
        inclusive: bool,
    },
    /// A bare name: binds the matched value.
    Binding(Ident),
    /// A dotted name: compared by value, as in `Shape.Empty`.
    Value(Path),
    /// `Name(...)`: a struct or an enum variant taken apart.
    Destructure {
        path: Path,
        args: Vec<PatternArg>,
    },
    Tuple(Vec<SeqPattern>),
    List(Vec<SeqPattern>),
    /// `a or b`
    Or(Vec<Pattern>),
    /// `pattern as name`
    As {
        pattern: Box<Pattern>,
        name: Ident,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub enum LiteralPattern {
    Int {
        negative: bool,
        value: u64,
        suffix: Option<IntSuffix>,
    },
    Float {
        negative: bool,
        value: f64,
        suffix: Option<FloatSuffix>,
    },
    /// Text only; a string pattern may not interpolate.
    Str(String),
    Bool(bool),
    None,
}

/// `pattern` or `field = pattern` inside a destructuring pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct PatternArg {
    pub name: Option<Ident>,
    pub pattern: Pattern,
}

#[derive(Clone, Debug, PartialEq)]
pub enum SeqPattern {
    Item(Pattern),
    /// `...` or `...rest`: the remaining elements.
    Rest(Option<Ident>, Span),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;
    use crate::token::TokenKind;

    fn ident(name: &str) -> Ident {
        Ident {
            name: name.to_string(),
            span: Span::default(),
        }
    }

    fn name(id: u32, text: &str) -> Expr {
        Expr::new(NodeId(id), ExprKind::Name(ident(text)), Span::default())
    }

    #[test]
    fn slots_start_pending() {
        let expr = name(0, "x");
        assert_eq!(expr.ty, TypeSlot::Pending);
        assert!(!expr.ty.is_resolved());
        assert!(TypeSlot::Dynamic.is_resolved());
        assert_eq!(TypeSlot::Static(TypeId(7)).static_type(), Some(TypeId(7)));
        assert_eq!(PlaceSlot::default(), PlaceSlot::Pending);
    }

    /// `(h[:, -1] @ w.T) on cpu`, built by hand the way the parser will
    /// build it.
    #[test]
    fn tensor_nodes_compose() {
        let minus_one = Expr::new(
            NodeId(2),
            ExprKind::Unary {
                op: UnaryOp::Neg,
                operand: Box::new(Expr::new(
                    NodeId(1),
                    ExprKind::Int {
                        value: 1,
                        suffix: None,
                    },
                    Span::default(),
                )),
            },
            Span::default(),
        );
        let last_row = Expr::new(
            NodeId(3),
            ExprKind::Index {
                base: Box::new(name(0, "h")),
                subscripts: vec![
                    Subscript::Slice {
                        start: None,
                        stop: None,
                        step: None,
                        span: Span::default(),
                    },
                    Subscript::Index(minus_one),
                ],
            },
            Span::default(),
        );
        let transpose = Expr::new(
            NodeId(5),
            ExprKind::Field {
                base: Box::new(name(4, "w")),
                name: ident("T"),
                optional: false,
            },
            Span::default(),
        );
        let matmul = Expr::new(
            NodeId(6),
            ExprKind::MatMul {
                lhs: Box::new(last_row),
                rhs: Box::new(transpose),
            },
            Span::default(),
        );
        let placed = Expr::new(
            NodeId(5),
            ExprKind::Placement {
                value: Box::new(matmul.clone()),
                target: Box::new(name(4, "cpu")),
                place: PlaceSlot::Pending,
            },
            Span::default(),
        );
        assert!(matmul.is_tensor_op());
        assert!(placed.is_tensor_op());
        assert!(!name(6, "h").is_tensor_op());
    }

    #[test]
    fn http_methods_round_trip() {
        for text in [
            "GET", "POST", "PUT", "PATCH", "DELETE", "HEAD", "OPTIONS", "WS", "SSE",
        ] {
            assert_eq!(
                HttpMethod::from_ident(text).map(HttpMethod::as_str),
                Some(text)
            );
        }
        assert_eq!(HttpMethod::from_ident("get"), None);
        assert_eq!(HttpMethod::from_ident("FETCH"), None);
    }

    fn path_from(src: &str) -> Result<RoutePath, RoutePathError> {
        let tokens = tokenize(src).unwrap();
        let TokenKind::Str(parts) = &tokens[0].kind else {
            panic!("expected a string")
        };
        RoutePath::from_str_parts(parts, tokens[0].span)
    }

    #[test]
    fn route_path_from_a_string_token() {
        let path = path_from("\"/users/{id}/orders/{order_id}\"").unwrap();
        assert_eq!(path.segments.len(), 4);
        assert_eq!(
            path.params().map(|p| p.name.as_str()).collect::<Vec<_>>(),
            ["id", "order_id"]
        );
        assert_eq!(path.template(), "/users/{id}/orders/{order_id}");
        // The parameter remembers where it was written.
        assert_eq!(path.params().next().unwrap().span.start.column, 10);
    }

    #[test]
    fn route_path_rejects_expressions_and_relative_paths() {
        assert!(path_from("\"/users/{id + 1}\"")
            .unwrap_err()
            .message
            .contains("not a path parameter"));
        assert!(path_from("\"users\"")
            .unwrap_err()
            .message
            .contains("must start with `/`"));
        assert!(path_from("\"{id}\"").is_err());
    }

    #[test]
    fn module_finds_main_and_nested_routes() {
        let block = Block {
            stmts: Vec::new(),
            span: Span::default(),
        };
        let route = |id: u32, method: HttpMethod, path: &str| Item {
            id: NodeId(id),
            span: Span::default(),
            doc: Vec::new(),
            decorators: Vec::new(),
            vis: Visibility::Private,
            kind: ItemKind::Route(RouteDecl {
                method,
                method_span: Span::default(),
                path: path_from(path).unwrap(),
                params: Vec::new(),
                result: ResultSpec::default(),
                body: block.clone(),
                response_ty: TypeSlot::Pending,
            }),
        };
        let item = |id: u32, kind: ItemKind| Item {
            id: NodeId(id),
            span: Span::default(),
            doc: Vec::new(),
            decorators: Vec::new(),
            vis: Visibility::Private,
            kind,
        };
        let module = Module {
            items: vec![
                route(0, HttpMethod::Get, "\"/health\""),
                item(
                    1,
                    ItemKind::RouteGroup(RouteGroup {
                        prefix: path_from("\"/admin\"").unwrap(),
                        items: vec![route(2, HttpMethod::Post, "\"/users\"")],
                    }),
                ),
                item(
                    3,
                    ItemKind::Main(MainDecl {
                        params: Vec::new(),
                        body: block.clone(),
                    }),
                ),
            ],
            span: Span::default(),
        };
        assert!(module.main().is_some());
        let methods: Vec<_> = module.routes().iter().map(|r| r.method).collect();
        assert_eq!(methods, [HttpMethod::Get, HttpMethod::Post]);
    }
}
