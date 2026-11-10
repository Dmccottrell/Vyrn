//! A compact text form of the syntax tree, for tests and `vyrn parse`.
//!
//! Expressions print as S-expressions with the operator first, so the
//! grouping the parser chose is explicit: `a + b * c` prints as
//! `(+ a (* b c))`. Types and patterns print as they are written.
//! Declarations and statements print one per line, with bodies indented by
//! two spaces.

use crate::ast::*;

pub fn module(module: &Module) -> String {
    let mut printer = Printer::default();
    for item in &module.items {
        printer.item(item);
    }
    printer.out
}

pub fn stmt(stmt: &Stmt) -> String {
    let mut printer = Printer::default();
    printer.stmt(stmt);
    printer.out
}

pub fn expr(expr: &Expr) -> String {
    match &expr.kind {
        ExprKind::Int { value, suffix } => {
            format!("{value}{}", suffix.map(|s| s.as_str()).unwrap_or(""))
        }
        ExprKind::Float { value, suffix } => {
            format!("{value:?}{}", suffix.map(|s| s.as_str()).unwrap_or(""))
        }
        ExprKind::Str(segments) => {
            let mut out = String::from("\"");
            for segment in segments {
                match segment {
                    StrSegment::Text(text) => out.push_str(&escape(text)),
                    StrSegment::Interp(e) => {
                        out.push('{');
                        out.push_str(&self::expr(e));
                        out.push('}');
                    }
                }
            }
            out.push('"');
            out
        }
        ExprKind::Bool(value) => value.to_string(),
        ExprKind::None => "none".to_string(),
        ExprKind::Name(name) => name.name.clone(),
        ExprKind::Tuple(items) => list("tuple", items.iter().map(self::expr)),
        ExprKind::List(items) => list("list", items.iter().map(self::expr)),
        ExprKind::Map(entries) => list(
            "map",
            entries
                .iter()
                .map(|(k, v)| format!("({} {})", self::expr(k), self::expr(v))),
        ),
        ExprKind::ListComp { element, clauses } => {
            format!(
                "(list-comp {}{})",
                self::expr(element),
                comp_clauses(clauses)
            )
        }
        ExprKind::MapComp {
            key,
            value,
            clauses,
        } => format!(
            "(map-comp {} {}{})",
            self::expr(key),
            self::expr(value),
            comp_clauses(clauses)
        ),
        ExprKind::Unary { op, operand } => {
            let op = match op {
                UnaryOp::Neg => "neg",
                UnaryOp::Pos => "pos",
                UnaryOp::BitNot => "~",
                UnaryOp::Not => "not",
            };
            format!("({op} {})", self::expr(operand))
        }
        ExprKind::Binary { op, lhs, rhs } => {
            format!(
                "({} {} {})",
                binary_op(*op),
                self::expr(lhs),
                self::expr(rhs)
            )
        }
        ExprKind::Compare { operands, ops } => {
            if ops.len() == 1 {
                format!(
                    "({} {} {})",
                    compare_op(ops[0]),
                    self::expr(&operands[0]),
                    self::expr(&operands[1])
                )
            } else {
                let mut out = format!("(compare {}", self::expr(&operands[0]));
                for (op, operand) in ops.iter().zip(&operands[1..]) {
                    out.push_str(&format!(" {} {}", compare_op(*op), self::expr(operand)));
                }
                out.push(')');
                out
            }
        }
        ExprKind::Range {
            start,
            end,
            inclusive,
            step,
        } => format!(
            "({} {} {}{})",
            if *inclusive { "to" } else { "until" },
            self::expr(start),
            self::expr(end),
            step.as_ref()
                .map(|s| format!(" by {}", self::expr(s)))
                .unwrap_or_default()
        ),
        ExprKind::Coalesce { lhs, rhs } => format!("(?? {} {})", self::expr(lhs), self::expr(rhs)),
        ExprKind::Pipeline { lhs, rhs } => format!("(|> {} {})", self::expr(lhs), self::expr(rhs)),
        ExprKind::MatMul { lhs, rhs } => format!("(@ {} {})", self::expr(lhs), self::expr(rhs)),
        ExprKind::Placement { value, target, .. } => {
            format!("(on {} {})", self::expr(value), self::expr(target))
        }
        ExprKind::Index { base, subscripts } => format!(
            "(index {} {})",
            self::expr(base),
            subscripts
                .iter()
                .map(subscript)
                .collect::<Vec<_>>()
                .join(" ")
        ),
        ExprKind::Field {
            base,
            name,
            optional,
        } => format!(
            "({} {} {})",
            if *optional { "?." } else { "." },
            self::expr(base),
            name.name
        ),
        ExprKind::Call { callee, args } => {
            let mut out = format!("(call {}", self::expr(callee));
            for a in args {
                out.push(' ');
                out.push_str(&arg(a));
            }
            out.push(')');
            out
        }
        ExprKind::Await(e) => format!("(await {})", self::expr(e)),
        ExprKind::Spawn(e) => format!("(spawn {})", self::expr(e)),
        ExprKind::Try(e) => format!("(try {})", self::expr(e)),
        ExprKind::Move(e) => format!("(move {})", self::expr(e)),
        ExprKind::If {
            then_value,
            condition,
            else_value,
        } => format!(
            "(if {} {} {})",
            self::expr(condition),
            self::expr(then_value),
            self::expr(else_value)
        ),
        ExprKind::Lambda {
            is_async,
            params: ps,
            body,
        } => format!(
            "({}fn ({}) {})",
            if *is_async { "async " } else { "" },
            params(ps),
            self::expr(body)
        ),
    }
}

pub fn type_expr(ty: &TypeExpr) -> String {
    match &ty.kind {
        TypeExprKind::Any => "any".to_string(),
        TypeExprKind::Named { path: p, args } => {
            if args.is_empty() {
                path(p)
            } else {
                let args: Vec<String> = args
                    .iter()
                    .map(|a| match a {
                        GenericArg::Type(t) => type_expr(t),
                        GenericArg::Int(value, _) => value.to_string(),
                    })
                    .collect();
                format!("{}[{}]", path(p), args.join(", "))
            }
        }
        TypeExprKind::Tensor(tensor) => {
            let dims: Vec<String> = tensor
                .dims
                .iter()
                .map(|d| match d {
                    Dim::Expr(e) => expr(e),
                    Dim::Unchecked(_) => "_".to_string(),
                    Dim::Variadic(_) => "...".to_string(),
                })
                .collect();
            if dims.is_empty() {
                format!("tensor[{}]", type_expr(&tensor.dtype))
            } else {
                format!("tensor[{}; {}]", type_expr(&tensor.dtype), dims.join(", "))
            }
        }
        TypeExprKind::List(element) => format!("[{}]", type_expr(element)),
        TypeExprKind::Array { element, len } => {
            format!("[{}; {}]", type_expr(element), expr(len))
        }
        TypeExprKind::Map { key, value } => format!("{{{}: {}}}", type_expr(key), type_expr(value)),
        TypeExprKind::Tuple(items) => format!("({})", join(items.iter().map(type_expr), ", ")),
        TypeExprKind::Fn { params, returns } => format!(
            "fn({}){}",
            join(params.iter().map(type_expr), ", "),
            returns
                .as_ref()
                .map(|r| format!(" -> {}", type_expr(r)))
                .unwrap_or_default()
        ),
        TypeExprKind::Optional(inner) => format!("{}?", type_expr(inner)),
        TypeExprKind::Union(members) => join(members.iter().map(type_expr), " | "),
        TypeExprKind::Owned { ownership, inner } => {
            let word = match ownership {
                Ownership::Ref => "ref",
                Ownership::RefMut => "ref mut",
                Ownership::Shared => "shared",
                Ownership::Weak => "weak",
                Ownership::Ptr => "ptr",
            };
            format!("{word} {}", type_expr(inner))
        }
        TypeExprKind::Placed { inner, target, .. } => {
            format!("{} on {}", type_expr(inner), expr(target))
        }
    }
}

pub fn pattern(pattern: &Pattern) -> String {
    match &pattern.kind {
        PatternKind::Wildcard => "_".to_string(),
        PatternKind::Literal(l) => literal(l),
        PatternKind::Range {
            start,
            end,
            inclusive,
        } => format!(
            "{} {} {}",
            literal(start),
            if *inclusive { "to" } else { "until" },
            literal(end)
        ),
        PatternKind::Binding(name) => name.name.clone(),
        PatternKind::Value(p) => path(p),
        PatternKind::Destructure { path: p, args } => format!(
            "{}({})",
            path(p),
            join(
                args.iter().map(|a| match &a.name {
                    Some(name) => format!("{} = {}", name.name, self::pattern(&a.pattern)),
                    None => self::pattern(&a.pattern),
                }),
                ", "
            )
        ),
        PatternKind::Tuple(items) => format!("({})", seq_patterns(items)),
        PatternKind::List(items) => format!("[{}]", seq_patterns(items)),
        PatternKind::Or(alternatives) => join(alternatives.iter().map(self::pattern), " or "),
        PatternKind::As { pattern: p, name } => format!("{} as {}", self::pattern(p), name.name),
    }
}

// ---- declarations and statements ------------------------------------------

#[derive(Default)]
struct Printer {
    out: String,
    depth: usize,
}

impl Printer {
    fn line(&mut self, text: impl AsRef<str>) {
        for _ in 0..self.depth {
            self.out.push_str("  ");
        }
        self.out.push_str(text.as_ref());
        self.out.push('\n');
    }

    fn nested(&mut self, f: impl FnOnce(&mut Printer)) {
        self.depth += 1;
        f(self);
        self.depth -= 1;
    }

    fn block(&mut self, block: &Block) {
        self.nested(|p| {
            for stmt in &block.stmts {
                p.stmt(stmt);
            }
        });
    }

    fn item(&mut self, item: &Item) {
        for line in &item.doc {
            self.line(format!("## {line}"));
        }
        for d in &item.decorators {
            if d.args.is_empty() {
                self.line(format!("@{}", path(&d.name)));
            } else {
                self.line(format!(
                    "@{}({})",
                    path(&d.name),
                    join(d.args.iter().map(arg), " ")
                ));
            }
        }
        let vis = match item.vis {
            Visibility::Public => "pub ",
            Visibility::Private => "",
        };
        match &item.kind {
            ItemKind::Use(u) => {
                let tail = match &u.kind {
                    UseKind::Module { alias: Some(a) } => format!(" as {}", a.name),
                    UseKind::Module { alias: None } => String::new(),
                    UseKind::Names(names) => format!(
                        " ({})",
                        join(
                            names.iter().map(|n| match &n.alias {
                                Some(a) => format!("{} as {}", n.name.name, a.name),
                                None => n.name.name.clone(),
                            }),
                            ", "
                        )
                    ),
                };
                self.line(format!("{vis}use {}{tail}", path(&u.path)));
            }
            ItemKind::Main(main) => {
                if main.params.is_empty() {
                    self.line("main");
                } else {
                    self.line(format!("main({})", params(&main.params)));
                }
                self.block(&main.body);
            }
            ItemKind::Const(c) => self.line(format!(
                "{vis}const {}{} = {}",
                c.name.name,
                annotation(&c.declared_ty),
                expr(&c.value)
            )),
            ItemKind::TypeAlias(t) => self.line(format!(
                "{vis}type {}{} = {}",
                t.name.name,
                generics(&t.generics),
                type_expr(&t.target)
            )),
            ItemKind::Fn(f) => self.fn_decl(vis, f),
            ItemKind::Aggregate(a) => {
                let keyword = match a.kind {
                    AggregateKind::Struct => "struct",
                    AggregateKind::Class => "class",
                };
                let base = a
                    .base
                    .as_ref()
                    .map(|b| format!("({})", type_expr(b)))
                    .unwrap_or_default();
                self.line(format!(
                    "{vis}{keyword} {}{}{base}",
                    a.name.name,
                    generics(&a.generics)
                ));
                self.nested(|p| {
                    for f in &a.fields {
                        let vis = match f.vis {
                            Visibility::Public => "pub ",
                            Visibility::Private => "",
                        };
                        p.line(format!(
                            "{vis}{}{}{}",
                            f.name.name,
                            annotation(&f.declared_ty),
                            default(&f.default)
                        ));
                    }
                    for m in &a.methods {
                        p.item(m);
                    }
                });
            }
            ItemKind::Enum(e) => {
                self.line(format!(
                    "{vis}enum {}{}",
                    e.name.name,
                    generics(&e.generics)
                ));
                self.nested(|p| {
                    for v in &e.variants {
                        let fields = if v.fields.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "({})",
                                join(
                                    v.fields.iter().map(|f| match &f.name {
                                        Some(n) => format!("{}: {}", n.name, type_expr(&f.ty)),
                                        None => type_expr(&f.ty),
                                    }),
                                    ", "
                                )
                            )
                        };
                        p.line(format!(
                            "{}{fields}{}",
                            v.name.name,
                            default(&v.discriminant)
                        ));
                    }
                    for m in &e.methods {
                        p.item(m);
                    }
                });
            }
            ItemKind::Trait(t) => {
                let supers = if t.supertraits.is_empty() {
                    String::new()
                } else {
                    format!("({})", join(t.supertraits.iter().map(type_expr), ", "))
                };
                self.line(format!(
                    "{vis}trait {}{}{supers}",
                    t.name.name,
                    generics(&t.generics)
                ));
                self.nested(|p| {
                    for m in &t.methods {
                        p.fn_decl("", m);
                    }
                });
            }
            ItemKind::Impl(i) => {
                let head = match &i.trait_ {
                    Some(t) => format!("{} for {}", type_expr(t), type_expr(&i.target)),
                    None => type_expr(&i.target),
                };
                self.line(format!("impl{} {head}", generics(&i.generics)));
                self.nested(|p| {
                    for m in &i.methods {
                        p.item(m);
                    }
                });
            }
            ItemKind::Route(r) => {
                self.line(format!(
                    "route {} {:?} ({}){}",
                    r.method.as_str(),
                    r.path.template(),
                    params(&r.params),
                    result(&r.result)
                ));
                self.block(&r.body);
            }
            ItemKind::RouteGroup(g) => {
                self.line(format!("routes {:?}", g.prefix.template()));
                self.nested(|p| {
                    for item in &g.items {
                        p.item(item);
                    }
                });
            }
            ItemKind::Extern(x) => {
                self.line(format!("extern {:?}", x.abi));
                self.nested(|p| {
                    for f in &x.functions {
                        p.fn_decl("", f);
                    }
                });
            }
        }
    }

    fn fn_decl(&mut self, vis: &str, f: &FnDecl) {
        let mut head = vis.to_string();
        if f.modifiers.is_async {
            head.push_str("async ");
        }
        if f.modifiers.is_strict {
            head.push_str("strict ");
        }
        if f.modifiers.is_kernel {
            head.push_str("kernel ");
        }
        match &f.modifiers.export {
            Some(Some(abi)) => head.push_str(&format!("export {abi:?} ")),
            Some(None) => head.push_str("export "),
            None => {}
        }
        head.push_str(&format!(
            "fn {}{}({}){}",
            f.name.name,
            generics(&f.generics),
            params(&f.params),
            result(&f.result)
        ));
        self.line(head);
        if let FnBody::Block(body) = &f.body {
            self.block(body);
        }
    }

    fn stmt(&mut self, stmt: &Stmt) {
        match &stmt.kind {
            StmtKind::Binding(b) => {
                let keyword = match b.kind {
                    BindingKind::Let => "let",
                    BindingKind::Var => "var",
                    BindingKind::Const => "const",
                };
                let value = b
                    .value
                    .as_ref()
                    .map(|v| format!(" = {}", expr(v)))
                    .unwrap_or_default();
                self.line(format!(
                    "{keyword} {}{}{value}",
                    bind_target(&b.target),
                    annotation(&b.declared_ty)
                ));
            }
            StmtKind::Expr(e) => self.line(expr(e)),
            StmtKind::Assign { target, op, value } => self.line(format!(
                "{} {} {}",
                expr(target),
                assign_op(*op),
                expr(value)
            )),
            StmtKind::Return(Some(value)) => self.line(format!("return {}", expr(value))),
            StmtKind::Return(None) => self.line("return"),
            StmtKind::Fail(e) => self.line(format!("fail {}", expr(e))),
            StmtKind::Defer(e) => self.line(format!("defer {}", expr(e))),
            StmtKind::Stop => self.line("stop"),
            StmtKind::Skip => self.line("skip"),
            StmtKind::If(i) => {
                for (n, (cond, body)) in i.branches.iter().enumerate() {
                    let keyword = if n == 0 { "if" } else { "elif" };
                    self.line(format!("{keyword} {}", condition(cond)));
                    self.block(body);
                }
                if let Some(body) = &i.else_body {
                    self.line("else");
                    self.block(body);
                }
            }
            StmtKind::While { condition: c, body } => {
                self.line(format!("while {}", condition(c)));
                self.block(body);
            }
            StmtKind::Loop(body) => {
                self.line("loop");
                self.block(body);
            }
            StmtKind::For(f) => {
                let mode = match f.mode {
                    ForMode::Sequential => "",
                    ForMode::Parallel => "par ",
                    ForMode::Async => "async ",
                };
                self.line(format!(
                    "{mode}for {}{} in {}",
                    pass_mode(f.pass),
                    bind_target(&f.target),
                    expr(&f.iter)
                ));
                self.block(&f.body);
            }
            StmtKind::Match(m) => {
                self.line(format!("match {}", expr(&m.scrutinee)));
                self.nested(|p| {
                    for arm in &m.arms {
                        let guard = arm
                            .guard
                            .as_ref()
                            .map(|g| format!(" if {}", expr(g)))
                            .unwrap_or_default();
                        p.line(format!("is {}{guard}", pattern(&arm.pattern)));
                        p.block(&arm.body);
                    }
                    if let Some(body) = &m.else_body {
                        p.line("else");
                        p.block(body);
                    }
                });
            }
            StmtKind::Try(t) => {
                self.line("try");
                self.block(&t.body);
                for c in &t.catches {
                    let ty = c
                        .error_ty
                        .as_ref()
                        .map(|t| format!(" {}", type_expr(t)))
                        .unwrap_or_default();
                    let binding = c
                        .binding
                        .as_ref()
                        .map(|b| format!(" as {}", b.name))
                        .unwrap_or_default();
                    self.line(format!("catch{ty}{binding}"));
                    self.block(&c.body);
                }
                if let Some(body) = &t.finally {
                    self.line("finally");
                    self.block(body);
                }
            }
            StmtKind::With {
                is_async,
                items,
                body,
            } => {
                let items = join(
                    items.iter().map(|i| match &i.binding {
                        Some(b) => format!("{} as {}", expr(&i.value), b.name),
                        None => expr(&i.value),
                    }),
                    ", ",
                );
                let prefix = if *is_async { "async " } else { "" };
                self.line(format!("{prefix}with {items}"));
                self.block(body);
            }
            StmtKind::On(on) => {
                self.line(format!("on {}", expr(&on.target)));
                self.block(&on.body);
            }
            StmtKind::Region(r) => {
                self.line(format!("region {}", r.name.name));
                self.block(&r.body);
            }
            StmtKind::Unsafe(body) => {
                self.line("unsafe");
                self.block(body);
            }
            StmtKind::Item(item) => self.item(item),
        }
    }
}

// ---- pieces ---------------------------------------------------------------

fn join(items: impl Iterator<Item = String>, separator: &str) -> String {
    items.collect::<Vec<_>>().join(separator)
}

fn list(head: &str, items: impl Iterator<Item = String>) -> String {
    let mut out = format!("({head}");
    for item in items {
        out.push(' ');
        out.push_str(&item);
    }
    out.push(')');
    out
}

fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            c => out.push(c),
        }
    }
    out
}

fn path(path: &Path) -> String {
    join(path.segments.iter().map(|s| s.name.clone()), ".")
}

fn pass_mode(pass: PassMode) -> &'static str {
    match pass {
        PassMode::Borrow => "",
        PassMode::Mut => "mut ",
        PassMode::Own => "own ",
    }
}

fn annotation(ty: &Option<TypeExpr>) -> String {
    ty.as_ref()
        .map(|t| format!(": {}", type_expr(t)))
        .unwrap_or_default()
}

fn default(value: &Option<Expr>) -> String {
    value
        .as_ref()
        .map(|v| format!(" = {}", expr(v)))
        .unwrap_or_default()
}

fn params(params: &[Param]) -> String {
    join(
        params.iter().map(|p| {
            format!(
                "{}{}{}{}",
                pass_mode(p.pass),
                p.name.name,
                annotation(&p.declared_ty),
                default(&p.default)
            )
        }),
        ", ",
    )
}

fn generics(generics: &[GenericParam]) -> String {
    if generics.is_empty() {
        return String::new();
    }
    format!(
        "[{}]",
        join(
            generics.iter().map(|g| {
                if g.bounds.is_empty() {
                    g.name.name.clone()
                } else {
                    format!(
                        "{}: {}",
                        g.name.name,
                        join(g.bounds.iter().map(type_expr), " + ")
                    )
                }
            }),
            ", "
        )
    )
}

fn result(result: &ResultSpec) -> String {
    let mut out = String::new();
    if let Some(t) = &result.returns {
        out.push_str(&format!(" -> {}", type_expr(t)));
    }
    if let Some(t) = &result.fails {
        out.push_str(&format!(" fails {}", type_expr(t)));
    }
    out
}

fn arg(arg: &Arg) -> String {
    match (&arg.name, arg.is_mut) {
        (Some(name), _) => format!("{}={}", name.name, expr(&arg.value)),
        (None, true) => format!("(mut {})", expr(&arg.value)),
        (None, false) => expr(&arg.value),
    }
}

fn subscript(subscript: &Subscript) -> String {
    let part = |e: &Option<Box<Expr>>| e.as_ref().map(|e| expr(e)).unwrap_or_else(|| "_".into());
    match subscript {
        Subscript::Index(e) => expr(e),
        Subscript::Slice {
            start, stop, step, ..
        } => format!("(slice {} {} {})", part(start), part(stop), part(step)),
        Subscript::Ellipsis(_) => "...".to_string(),
    }
}

fn comp_clauses(clauses: &[CompClause]) -> String {
    let mut out = String::new();
    for clause in clauses {
        match clause {
            CompClause::For { target, iter } => {
                out.push_str(&format!(" (for {} {})", bind_target(target), expr(iter)))
            }
            CompClause::If(cond) => out.push_str(&format!(" (if {})", expr(cond))),
        }
    }
    out
}

fn bind_target(target: &BindTarget) -> String {
    match target {
        BindTarget::Name(name) => name.name.clone(),
        BindTarget::Discard(_) => "_".to_string(),
        BindTarget::Tuple(items, _) => format!("({})", join(items.iter().map(bind_target), ", ")),
    }
}

fn condition(condition: &Condition) -> String {
    match condition {
        Condition::Expr(e) => expr(e),
        Condition::Let { pattern: p, value } => format!("let {} = {}", pattern(p), expr(value)),
    }
}

fn literal(literal: &LiteralPattern) -> String {
    match literal {
        LiteralPattern::Int {
            negative,
            value,
            suffix,
        } => format!(
            "{}{value}{}",
            if *negative { "-" } else { "" },
            suffix.map(|s| s.as_str()).unwrap_or("")
        ),
        LiteralPattern::Float {
            negative,
            value,
            suffix,
        } => format!(
            "{}{value:?}{}",
            if *negative { "-" } else { "" },
            suffix.map(|s| s.as_str()).unwrap_or("")
        ),
        LiteralPattern::Str(text) => format!("{text:?}"),
        LiteralPattern::Bool(value) => value.to_string(),
        LiteralPattern::None => "none".to_string(),
    }
}

fn seq_patterns(items: &[SeqPattern]) -> String {
    join(
        items.iter().map(|item| match item {
            SeqPattern::Item(p) => pattern(p),
            SeqPattern::Rest(Some(name), _) => format!("...{}", name.name),
            SeqPattern::Rest(None, _) => "...".to_string(),
        }),
        ", ",
    )
}

fn binary_op(op: BinaryOp) -> &'static str {
    match op {
        BinaryOp::Add => "+",
        BinaryOp::Sub => "-",
        BinaryOp::Mul => "*",
        BinaryOp::Div => "/",
        BinaryOp::FloorDiv => "//",
        BinaryOp::Rem => "%",
        BinaryOp::Pow => "**",
        BinaryOp::BitAnd => "&",
        BinaryOp::BitOr => "|",
        BinaryOp::BitXor => "^",
        BinaryOp::Shl => "<<",
        BinaryOp::Shr => ">>",
        BinaryOp::And => "and",
        BinaryOp::Or => "or",
    }
}

fn compare_op(op: CompareOp) -> &'static str {
    match op {
        CompareOp::Eq => "==",
        CompareOp::NotEq => "!=",
        CompareOp::Lt => "<",
        CompareOp::LtEq => "<=",
        CompareOp::Gt => ">",
        CompareOp::GtEq => ">=",
        CompareOp::In => "in",
        CompareOp::NotIn => "not in",
    }
}

fn assign_op(op: AssignOp) -> &'static str {
    match op {
        AssignOp::Assign => "=",
        AssignOp::Add => "+=",
        AssignOp::Sub => "-=",
        AssignOp::Mul => "*=",
        AssignOp::Div => "/=",
        AssignOp::FloorDiv => "//=",
        AssignOp::Rem => "%=",
        AssignOp::MatMul => "@=",
        AssignOp::Pow => "**=",
        AssignOp::BitAnd => "&=",
        AssignOp::BitOr => "|=",
        AssignOp::BitXor => "^=",
        AssignOp::Shl => "<<=",
        AssignOp::Shr => ">>=",
        AssignOp::Coalesce => "??=",
    }
}
