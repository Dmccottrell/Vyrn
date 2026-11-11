//! Programs made of several files.
//!
//! `use app.storage (Db)` loads `app/storage.vyrn` next to the main file;
//! `use "github.com/ann/neural@v1"` loads `lib.vyrn` from the fetched
//! module, whose own `use` lines are relative to the module's directory.
//!
//! Every file gets a namespace. Its top-level declarations are renamed into
//! it (`load` in `app/storage.vyrn` becomes `app.storage.load`), and each
//! reference is rewritten to the name it means: the file's own
//! declarations, names it imported, and `module.member` through a module it
//! used. Local variables are tracked, so a variable never gets mistaken for
//! an import of the same name. The result is one syntax tree, which the
//! checker and compiler handle as if it were one file, while two modules
//! can each have a `load` without clashing.
//!
//! Each file is parsed at its own base offset, so every span says which
//! file it came from, and errors name the right file.

use std::collections::{HashMap, HashSet};
use std::path::{Path as FsPath, PathBuf};

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::span::Span;

use super::resolve::Resolver;

/// Bits of a span offset that address a position within a file: files may
/// be up to 16 MB, and a program up to 256 files.
const FILE_BITS: u32 = 24;

pub struct SourceFile {
    pub path: PathBuf,
    pub text: String,
}

pub struct Loaded {
    pub module: Module,
    pub files: Vec<SourceFile>,
}

/// Labels each diagnostic with the file its span points into.
pub fn attach_files(diagnostics: &mut [Diagnostic], files: &[SourceFile]) {
    for d in diagnostics {
        if d.file.is_none() {
            let index = (d.span.start.offset >> FILE_BITS) as usize;
            if let Some(f) = files.get(index) {
                d.file = Some(f.path.display().to_string());
            }
        }
    }
}

struct FileInfo {
    module: Module,
    prefix: String,
    /// Use items that load files: item index and the file's key.
    uses: Vec<(usize, PathBuf)>,
}

/// Where the loader gets source text and URL modules: the file system and
/// the module cache, or the bundle inside an executable from `vyrn build`.
pub trait Source {
    fn read(&mut self, path: &FsPath) -> std::io::Result<String>;
    /// The directory of a URL module's source.
    fn resolve_url(&mut self, url: &str) -> Result<PathBuf, String>;
    /// Whether a module file exists.
    fn exists(&mut self, path: &FsPath) -> bool;
}

/// Files on disk; URL modules through a [`Resolver`], if there is one.
pub struct Disk<'r, 'f> {
    pub resolver: Option<&'r mut Resolver<'f>>,
}

impl Source for Disk<'_, '_> {
    fn read(&mut self, path: &FsPath) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }

    fn resolve_url(&mut self, url: &str) -> Result<PathBuf, String> {
        match self.resolver.as_deref_mut() {
            Some(r) => r.resolve(url),
            None => Err(format!(
                "`{url}` is a URL module; run the program with `vyrn run` so it can be fetched"
            )),
        }
    }

    fn exists(&mut self, path: &FsPath) -> bool {
        path.is_file()
    }
}

fn qualify(prefix: &str, name: &str) -> String {
    if prefix.is_empty() {
        name.to_string()
    } else {
        format!("{prefix}.{name}")
    }
}

fn key_of(path: &FsPath) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}

type LoadError = (Vec<Diagnostic>, Vec<SourceFile>);

/// Loads `entry` and everything it uses into one module. Local modules
/// (`use app.storage`) are found under `root`, the project directory.
pub fn load(entry: &FsPath, root: &FsPath, source: &mut dyn Source) -> Result<Loaded, LoadError> {
    let mut files: Vec<SourceFile> = Vec::new();
    let mut infos: Vec<FileInfo> = Vec::new();
    let mut by_key: HashMap<PathBuf, usize> = HashMap::new();
    let mut taken: HashSet<String> = HashSet::new();
    let mut diagnostics = Vec::new();
    let root_dir = root.to_path_buf();

    // ---- 1. load every file the program uses ----
    let mut queue = vec![(entry.to_path_buf(), String::new(), root_dir)];
    while let Some((path, prefix, root)) = queue.pop() {
        let key = key_of(&path);
        if by_key.contains_key(&key) {
            continue;
        }
        let text = match source.read(&path) {
            Ok(t) => t,
            Err(e) => {
                let mut d = Diagnostic::new(
                    format!("cannot read {}: {e}", path.display()),
                    Span::default(),
                );
                d.file = Some(path.display().to_string());
                diagnostics.push(d);
                continue;
            }
        };
        let index = files.len();
        if index >= 1 << (32 - FILE_BITS) || text.len() >= 1 << FILE_BITS {
            let mut d = Diagnostic::new(
                "a program can have at most 256 files of up to 16 MB each",
                Span::default(),
            );
            d.file = Some(path.display().to_string());
            diagnostics.push(d);
            continue;
        }
        let (module, mut errors) = crate::parser::parse_at(&text, (index as u32) << FILE_BITS);
        diagnostics.append(&mut errors);
        files.push(SourceFile {
            path: path.clone(),
            text,
        });
        by_key.insert(key, index);
        taken.insert(prefix.clone());

        let mut uses = Vec::new();
        for (i, item) in module.items.iter().enumerate() {
            let ItemKind::Use(u) = &item.kind else {
                continue;
            };
            match locate(u, &root, &prefix, source, &mut taken) {
                Ok(None) => {}
                Ok(Some((file, pre, r))) => {
                    uses.push((i, key_of(&file)));
                    queue.push((file, pre, r));
                }
                Err((message, hint)) => {
                    let mut d = Diagnostic::new(message, u.path.span);
                    if let Some(h) = hint {
                        d = d.with_hint(h);
                    }
                    diagnostics.push(d);
                }
            }
        }
        infos.push(FileInfo {
            module,
            prefix,
            uses,
        });
    }
    if !diagnostics.is_empty() {
        attach_files(&mut diagnostics, &files);
        return Err((diagnostics, files));
    }

    // ---- 2. what each file declares ----
    let exports: Vec<HashSet<String>> = infos.iter().map(|f| top_level_names(&f.module)).collect();
    let prefixes: Vec<String> = infos.iter().map(|f| f.prefix.clone()).collect();

    // ---- 3. rename each file into its namespace, and merge ----
    let mut merged: Vec<Vec<Item>> = Vec::new();
    for (index, info) in infos.iter_mut().enumerate() {
        let mut names: HashMap<String, String> = exports[index]
            .iter()
            .map(|n| (n.clone(), qualify(&info.prefix, n)))
            .collect();
        let mut modules: HashMap<String, usize> = HashMap::new();
        let loads: HashMap<usize, PathBuf> = info.uses.iter().cloned().collect();
        let mut kept = Vec::new();
        for (i, item) in std::mem::take(&mut info.module.items)
            .into_iter()
            .enumerate()
        {
            if index != 0 && matches!(item.kind, ItemKind::Main(_)) {
                // A module's `main:` runs only when it is the program.
                continue;
            }
            let (Some(key), ItemKind::Use(u)) = (loads.get(&i), &item.kind) else {
                kept.push(item);
                continue;
            };
            let target = by_key[key];
            match &u.kind {
                UseKind::Module { alias } => {
                    let local = alias
                        .as_ref()
                        .or(u.path.segments.last())
                        .map(|i| i.name.clone());
                    if let Some(local) = local {
                        modules.insert(local, target);
                    }
                }
                UseKind::Names(list) => {
                    for n in list {
                        if exports[target].contains(&n.name.name) {
                            let local = n.alias.as_ref().unwrap_or(&n.name);
                            names.insert(
                                local.name.clone(),
                                qualify(&prefixes[target], &n.name.name),
                            );
                        } else {
                            diagnostics.push(Diagnostic::new(
                                format!(
                                    "module `{}` has no `{}`",
                                    u.url.clone().unwrap_or_else(|| dotted(&u.path)),
                                    n.name.name
                                ),
                                n.name.span,
                            ));
                        }
                    }
                }
            }
        }
        let mut renamer = Renamer {
            names: &names,
            modules: modules
                .into_iter()
                .map(|(alias, target)| (alias, (prefixes[target].clone(), &exports[target])))
                .collect(),
            scopes: Vec::new(),
            errors: Vec::new(),
        };
        for item in &mut kept {
            renamer.item(item, true);
        }
        diagnostics.append(&mut renamer.errors);
        merged.push(kept);
    }
    if !diagnostics.is_empty() {
        attach_files(&mut diagnostics, &files);
        return Err((diagnostics, files));
    }
    // Modules first, in the reverse of the order they were found, so a
    // module's constants exist before the code that uses them.
    let mut items = Vec::new();
    for file in merged.iter_mut().skip(1).rev() {
        items.append(file);
    }
    items.append(&mut merged[0]);
    let span = infos[0].module.span;
    Ok(Loaded {
        module: Module { items, span },
        files,
    })
}

type Located = Option<(PathBuf, String, PathBuf)>;

/// The file a `use` line loads, its namespace, and its package root;
/// `None` for the standard library and Python modules.
fn locate(
    u: &UseDecl,
    root: &FsPath,
    prefix: &str,
    source: &mut dyn Source,
    taken: &mut HashSet<String>,
) -> Result<Located, (String, Option<String>)> {
    if let Some(url) = &u.url {
        let dir = source.resolve_url(url).map_err(|e| (e, None))?;
        let file = dir.join("lib.vyrn");
        if !source.exists(&file) {
            return Err((
                format!("`{url}` has no `lib.vyrn`, so it is not a Vyrn module"),
                Some(
                    "a module's entry point is lib.vyrn at the top of its repository or directory"
                        .into(),
                ),
            ));
        }
        let name = u.path.segments[0].name.clone();
        let mut unique = name.clone();
        let mut n = 2;
        while taken.contains(&unique) {
            unique = format!("{name}{n}");
            n += 1;
        }
        taken.insert(unique.clone());
        return Ok(Some((file, unique, dir)));
    }
    let segments: Vec<&str> = u.path.segments.iter().map(|s| s.name.as_str()).collect();
    if matches!(
        segments.first(),
        Some(&"vyrn") | Some(&"std") | Some(&"python")
    ) {
        return Ok(None);
    }
    let mut file = root.to_path_buf();
    for s in &segments {
        file.push(s);
    }
    file.set_extension("vyrn");
    if !source.exists(&file) {
        let name = segments.join(".");
        let relative = format!("{}.vyrn", segments.join("/"));
        return Err((
            format!("cannot find module `{name}`"),
            Some(format!(
                "`use {name}` loads {relative} from {}; create that file, or check the spelling",
                if root.as_os_str().is_empty() {
                    ".".to_string()
                } else {
                    root.display().to_string()
                }
            )),
        ));
    }
    Ok(Some((
        file,
        qualify(prefix, &segments.join(".")),
        root.to_path_buf(),
    )))
}

fn dotted(path: &Path) -> String {
    path.segments
        .iter()
        .map(|s| s.name.as_str())
        .collect::<Vec<_>>()
        .join(".")
}

/// The names a file declares at the top level, which other files can use.
fn top_level_names(module: &Module) -> HashSet<String> {
    let mut names = HashSet::new();
    for item in &module.items {
        match &item.kind {
            ItemKind::Fn(f) => {
                names.insert(f.name.name.clone());
            }
            ItemKind::Aggregate(a) => {
                names.insert(a.name.name.clone());
            }
            ItemKind::Enum(e) => {
                names.insert(e.name.name.clone());
            }
            ItemKind::Const(c) => {
                names.insert(c.name.name.clone());
            }
            ItemKind::TypeAlias(t) => {
                names.insert(t.name.name.clone());
            }
            ItemKind::Trait(t) => {
                names.insert(t.name.name.clone());
            }
            ItemKind::Extern(e) => {
                for f in &e.functions {
                    names.insert(f.name.name.clone());
                }
            }
            _ => {}
        }
    }
    names
}

/// Rewrites one file's names to the namespaced names they refer to.
struct Renamer<'a> {
    /// Unqualified name in this file -> namespaced name.
    names: &'a HashMap<String, String>,
    /// Module alias -> (namespace, its exported names).
    modules: HashMap<String, (String, &'a HashSet<String>)>,
    /// Local variables in scope.
    scopes: Vec<HashSet<String>>,
    errors: Vec<Diagnostic>,
}

impl Renamer<'_> {
    fn is_local(&self, name: &str) -> bool {
        self.scopes.iter().any(|s| s.contains(name))
    }

    fn declare(&mut self, name: &str) {
        if let Some(s) = self.scopes.last_mut() {
            s.insert(name.to_string());
        }
    }

    fn ident(&mut self, id: &mut Ident) {
        if self.is_local(&id.name) {
            return;
        }
        if let Some(q) = self.names.get(&id.name) {
            id.name = q.clone();
        }
    }

    /// `module.member` as the namespaced name it refers to.
    fn member(&mut self, module: &str, member: &Ident) -> Option<Ident> {
        if self.is_local(module) {
            return None;
        }
        let (prefix, exports) = self.modules.get(module)?;
        if !exports.contains(&member.name) {
            self.errors.push(Diagnostic::new(
                format!("module `{module}` has no `{}`", member.name),
                member.span,
            ));
            return None;
        }
        Some(Ident {
            name: qualify(prefix, &member.name),
            span: member.span,
        })
    }

    /// A path in a type, a pattern or a decorator: `Db`, `storage.Db`,
    /// `storage.Shape.Circle`.
    fn path(&mut self, path: &mut Path) {
        if path.segments.len() >= 2 {
            let first = path.segments[0].name.clone();
            if let Some(joined) = self.member(&first, &path.segments[1].clone()) {
                path.segments.splice(0..2, [joined]);
                return;
            }
        }
        if let Some(first) = path.segments.first_mut() {
            if let Some(q) = self.names.get(&first.name) {
                first.name = q.clone();
            }
        }
    }

    fn ty(&mut self, t: &mut TypeExpr) {
        match &mut t.kind {
            TypeExprKind::Named { path, args } => {
                self.path(path);
                for a in args {
                    if let GenericArg::Type(t) = a {
                        self.ty(t);
                    }
                }
            }
            TypeExprKind::Tensor(tt) => {
                self.ty(&mut tt.dtype);
                for d in &mut tt.dims {
                    if let Dim::Expr(e) = d {
                        self.expr(e);
                    }
                }
            }
            TypeExprKind::List(inner) | TypeExprKind::Optional(inner) => self.ty(inner),
            TypeExprKind::Array { element, len } => {
                self.ty(element);
                self.expr(len);
            }
            TypeExprKind::Map { key, value } => {
                self.ty(key);
                self.ty(value);
            }
            TypeExprKind::Tuple(items) | TypeExprKind::Union(items) => {
                for t in items {
                    self.ty(t);
                }
            }
            TypeExprKind::Fn { params, returns } => {
                for t in params {
                    self.ty(t);
                }
                if let Some(r) = returns {
                    self.ty(r);
                }
            }
            TypeExprKind::Owned { inner, .. } => self.ty(inner),
            TypeExprKind::Placed { inner, target, .. } => {
                self.ty(inner);
                self.expr(target);
            }
            TypeExprKind::Any => {}
        }
    }

    fn item(&mut self, item: &mut Item, top: bool) {
        for d in &mut item.decorators {
            self.path(&mut d.name);
            for a in &mut d.args {
                self.expr(&mut a.value);
            }
        }
        match &mut item.kind {
            ItemKind::Fn(f) => self.function(f, top),
            ItemKind::Main(m) => {
                self.scopes.push(HashSet::new());
                self.params(&mut m.params);
                self.block(&mut m.body);
                self.scopes.pop();
            }
            ItemKind::Const(c) => {
                self.ident(&mut c.name);
                if let Some(t) = &mut c.declared_ty {
                    self.ty(t);
                }
                self.expr(&mut c.value);
            }
            ItemKind::TypeAlias(t) => {
                self.ident(&mut t.name);
                self.ty(&mut t.target);
            }
            ItemKind::Aggregate(a) => {
                self.ident(&mut a.name);
                if let Some(b) = &mut a.base {
                    self.ty(b);
                }
                for f in &mut a.fields {
                    if let Some(t) = &mut f.declared_ty {
                        self.ty(t);
                    }
                    if let Some(d) = &mut f.default {
                        self.expr(d);
                    }
                }
                for m in &mut a.methods {
                    self.item(m, false);
                }
            }
            ItemKind::Enum(e) => {
                self.ident(&mut e.name);
                for v in &mut e.variants {
                    for f in &mut v.fields {
                        self.ty(&mut f.ty);
                    }
                    if let Some(d) = &mut v.discriminant {
                        self.expr(d);
                    }
                }
                for m in &mut e.methods {
                    self.item(m, false);
                }
            }
            ItemKind::Trait(t) => {
                self.ident(&mut t.name);
                for s in &mut t.supertraits {
                    self.ty(s);
                }
                for m in &mut t.methods {
                    self.function(m, false);
                }
            }
            ItemKind::Impl(i) => {
                if let Some(t) = &mut i.trait_ {
                    self.ty(t);
                }
                self.ty(&mut i.target);
                for m in &mut i.methods {
                    self.item(m, false);
                }
            }
            ItemKind::Route(r) => {
                self.scopes.push(HashSet::new());
                self.params(&mut r.params);
                self.result(&mut r.result);
                self.block(&mut r.body);
                self.scopes.pop();
            }
            ItemKind::RouteGroup(g) => {
                for i in &mut g.items {
                    self.item(i, true);
                }
            }
            ItemKind::Extern(e) => {
                for f in &mut e.functions {
                    self.function(f, true);
                }
            }
            ItemKind::Use(_) => {}
        }
    }

    fn result(&mut self, r: &mut ResultSpec) {
        if let Some(t) = &mut r.returns {
            self.ty(t);
        }
        if let Some(t) = &mut r.fails {
            self.ty(t);
        }
    }

    fn params(&mut self, params: &mut [Param]) {
        for p in params {
            if let Some(t) = &mut p.declared_ty {
                self.ty(t);
            }
            if let Some(d) = &mut p.default {
                self.expr(d);
            }
            self.declare(&p.name.name);
        }
    }

    /// A function; `top` when it is a top-level declaration whose name is
    /// renamed (methods keep theirs).
    fn function(&mut self, f: &mut FnDecl, top: bool) {
        if top {
            self.ident(&mut f.name);
        }
        self.scopes.push(HashSet::new());
        for g in &f.generics {
            self.declare(&g.name.name);
        }
        self.params(&mut f.params);
        self.result(&mut f.result);
        if let FnBody::Block(b) = &mut f.body {
            self.block(b);
        }
        self.scopes.pop();
    }

    fn block(&mut self, b: &mut Block) {
        self.scopes.push(HashSet::new());
        for s in &mut b.stmts {
            self.stmt(s);
        }
        self.scopes.pop();
    }

    fn target(&mut self, t: &BindTarget) {
        match t {
            BindTarget::Name(n) => self.declare(&n.name),
            BindTarget::Discard(_) => {}
            BindTarget::Tuple(items, _) => {
                for i in items {
                    self.target(i);
                }
            }
        }
    }

    fn stmt(&mut self, s: &mut Stmt) {
        match &mut s.kind {
            StmtKind::Binding(b) => {
                if let Some(t) = &mut b.declared_ty {
                    self.ty(t);
                }
                if let Some(v) = &mut b.value {
                    self.expr(v);
                }
                self.target(&b.target);
            }
            StmtKind::Expr(e) | StmtKind::Fail(e) | StmtKind::Defer(e) => self.expr(e),
            StmtKind::Assign { target, value, op } => {
                self.expr(value);
                self.assign_target(target, *op);
            }
            StmtKind::Return(v) => {
                if let Some(v) = v {
                    self.expr(v);
                }
            }
            StmtKind::Stop | StmtKind::Skip => {}
            StmtKind::If(i) => {
                for (c, b) in &mut i.branches {
                    self.scopes.push(HashSet::new());
                    self.condition(c);
                    self.block(b);
                    self.scopes.pop();
                }
                if let Some(b) = &mut i.else_body {
                    self.block(b);
                }
            }
            StmtKind::While { condition, body } => {
                self.scopes.push(HashSet::new());
                self.condition(condition);
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::Loop(b) | StmtKind::Unsafe(b) => self.block(b),
            StmtKind::For(f) => {
                self.expr(&mut f.iter);
                self.scopes.push(HashSet::new());
                self.target(&f.target);
                self.block(&mut f.body);
                self.scopes.pop();
            }
            StmtKind::Match(m) => {
                self.expr(&mut m.scrutinee);
                for arm in &mut m.arms {
                    self.scopes.push(HashSet::new());
                    self.pattern(&mut arm.pattern);
                    if let Some(g) = &mut arm.guard {
                        self.expr(g);
                    }
                    self.block(&mut arm.body);
                    self.scopes.pop();
                }
                if let Some(b) = &mut m.else_body {
                    self.block(b);
                }
            }
            StmtKind::Try(t) => {
                self.block(&mut t.body);
                for c in &mut t.catches {
                    self.scopes.push(HashSet::new());
                    if let Some(ty) = &mut c.error_ty {
                        self.ty(ty);
                    }
                    if let Some(b) = &c.binding {
                        self.declare(&b.name);
                    }
                    self.block(&mut c.body);
                    self.scopes.pop();
                }
                if let Some(f) = &mut t.finally {
                    self.block(f);
                }
            }
            StmtKind::With { items, body, .. } => {
                self.scopes.push(HashSet::new());
                for i in items.iter_mut() {
                    self.expr(&mut i.value);
                    if let Some(b) = &i.binding {
                        self.declare(&b.name);
                    }
                }
                self.block(body);
                self.scopes.pop();
            }
            StmtKind::On(o) => {
                self.expr(&mut o.target);
                self.block(&mut o.body);
            }
            StmtKind::Region(r) => {
                self.scopes.push(HashSet::new());
                self.declare(&r.name.name);
                self.block(&mut r.body);
                self.scopes.pop();
            }
            StmtKind::Item(item) => {
                if let ItemKind::Fn(f) = &item.kind {
                    let name = f.name.name.clone();
                    self.declare(&name);
                }
                self.item(item, false);
            }
        }
    }

    /// Assigning to a name that is neither local nor imported declares a
    /// local, as in Python.
    fn assign_target(&mut self, target: &mut Expr, op: AssignOp) {
        match &mut target.kind {
            ExprKind::Name(n) if op == AssignOp::Assign => {
                if !self.is_local(&n.name) && !self.names.contains_key(&n.name) {
                    self.declare(&n.name);
                } else {
                    self.ident(n);
                }
            }
            ExprKind::Tuple(items) => {
                for i in items {
                    self.assign_target(i, op);
                }
            }
            _ => self.expr(target),
        }
    }

    fn condition(&mut self, c: &mut Condition) {
        match c {
            Condition::Expr(e) => self.expr(e),
            Condition::Let { pattern, value } => {
                self.expr(value);
                self.pattern(pattern);
            }
        }
    }

    fn pattern(&mut self, p: &mut Pattern) {
        match &mut p.kind {
            PatternKind::Binding(n) => self.declare(&n.name),
            PatternKind::Value(path) => self.path(path),
            PatternKind::Destructure { path, args } => {
                self.path(path);
                for a in args {
                    self.pattern(&mut a.pattern);
                }
            }
            PatternKind::Tuple(items) | PatternKind::List(items) => {
                for i in items {
                    match i {
                        SeqPattern::Item(p) => self.pattern(p),
                        SeqPattern::Rest(Some(n), _) => self.declare(&n.name),
                        SeqPattern::Rest(None, _) => {}
                    }
                }
            }
            PatternKind::Or(ps) => {
                for p in ps {
                    self.pattern(p);
                }
            }
            PatternKind::As { pattern, name } => {
                self.pattern(pattern);
                self.declare(&name.name);
            }
            PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Range { .. } => {}
        }
    }

    fn expr(&mut self, e: &mut Expr) {
        // `module.member` collapses to the member's namespaced name.
        if let ExprKind::Field { base, name, .. } = &e.kind {
            if let ExprKind::Name(module) = &base.kind {
                if let Some(joined) = self.member(&module.name.clone(), &name.clone()) {
                    e.kind = ExprKind::Name(joined);
                    return;
                }
            }
        }
        match &mut e.kind {
            ExprKind::Name(n) => self.ident(n),
            ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::Bool(_) | ExprKind::None => {}
            ExprKind::Str(segments) => {
                for s in segments {
                    if let StrSegment::Interp(inner) = s {
                        self.expr(inner);
                    }
                }
            }
            ExprKind::Tuple(items) | ExprKind::List(items) => {
                for i in items {
                    self.expr(i);
                }
            }
            ExprKind::Map(entries) => {
                for (k, v) in entries {
                    self.expr(k);
                    self.expr(v);
                }
            }
            ExprKind::ListComp { element, clauses } => {
                self.scopes.push(HashSet::new());
                self.clauses(clauses);
                self.expr(element);
                self.scopes.pop();
            }
            ExprKind::MapComp {
                key,
                value,
                clauses,
            } => {
                self.scopes.push(HashSet::new());
                self.clauses(clauses);
                self.expr(key);
                self.expr(value);
                self.scopes.pop();
            }
            ExprKind::Unary { operand, .. } => self.expr(operand),
            ExprKind::Binary { lhs, rhs, .. }
            | ExprKind::Coalesce { lhs, rhs }
            | ExprKind::Pipeline { lhs, rhs }
            | ExprKind::MatMul { lhs, rhs } => {
                self.expr(lhs);
                self.expr(rhs);
            }
            ExprKind::Compare { operands, .. } => {
                for o in operands {
                    self.expr(o);
                }
            }
            ExprKind::Range {
                start, end, step, ..
            } => {
                self.expr(start);
                self.expr(end);
                if let Some(s) = step {
                    self.expr(s);
                }
            }
            ExprKind::Placement { value, target, .. } => {
                self.expr(value);
                self.expr(target);
            }
            ExprKind::Index { base, subscripts } => {
                self.expr(base);
                for s in subscripts {
                    match s {
                        Subscript::Index(i) => self.expr(i),
                        Subscript::Slice {
                            start, stop, step, ..
                        } => {
                            for part in [start, stop, step].into_iter().flatten() {
                                self.expr(part);
                            }
                        }
                        Subscript::Ellipsis(_) => {}
                    }
                }
            }
            ExprKind::Field { base, .. } => self.expr(base),
            ExprKind::Call { callee, args } => {
                self.expr(callee);
                for a in args {
                    self.expr(&mut a.value);
                }
            }
            ExprKind::Await(i) | ExprKind::Spawn(i) | ExprKind::Try(i) | ExprKind::Move(i) => {
                self.expr(i)
            }
            ExprKind::If {
                then_value,
                condition,
                else_value,
            } => {
                self.expr(condition);
                self.expr(then_value);
                self.expr(else_value);
            }
            ExprKind::Lambda { params, body, .. } => {
                self.scopes.push(HashSet::new());
                self.params(params);
                self.expr(body);
                self.scopes.pop();
            }
        }
    }

    fn clauses(&mut self, clauses: &mut [CompClause]) {
        for c in clauses {
            match c {
                CompClause::For { target, iter } => {
                    self.expr(iter);
                    self.target(target);
                }
                CompClause::If(e) => self.expr(e),
            }
        }
    }
}
