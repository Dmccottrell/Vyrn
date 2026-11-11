//! Builds the syntax tree from tokens (grammar v0.4).
//!
//! A hand-written recursive-descent parser. Each rule in `docs/grammar.ebnf`
//! has a method of the same name, and the expression methods follow the
//! precedence order of grammar section 5, lowest first.
//!
//! The parser keeps going after an error. A failed declaration or statement
//! is reported once and skipped: the rest of its line goes, and if the line
//! opened a block, the block goes too, up to its `end`. One mistake therefore
//! produces one message, not a cascade.

use std::collections::HashSet;

use crate::ast::*;
use crate::diagnostic::Diagnostic;
use crate::lexer::Lexer;
use crate::span::{Pos, Span};
use crate::token::{Keyword as K, Punct as P, StrPart, Token, TokenKind as Tk};

/// Parses a whole file. Returns the tree only if there were no errors.
pub fn parse(src: &str) -> Result<Module, Vec<Diagnostic>> {
    let (module, diagnostics) = parse_recovering(src);
    if diagnostics.is_empty() {
        Ok(module)
    } else {
        Err(diagnostics)
    }
}

/// Parses a whole file and returns the tree together with every error. The
/// tree leaves out the declarations and statements that failed to parse.
///
/// A syntax error on a line that already has a lexical error is almost always
/// a consequence of it, so only the lexical error is kept for that line.
pub fn parse_recovering(src: &str) -> (Module, Vec<Diagnostic>) {
    parse_at(src, 0)
}

/// Like [`parse_recovering`], with every offset starting at `base`. A
/// program of several files gives each its own range of offsets, so a span
/// identifies its file.
pub fn parse_at(src: &str, base: u32) -> (Module, Vec<Diagnostic>) {
    let start = Pos {
        offset: base,
        ..Pos::START
    };
    let (tokens, lex_errors) = Lexer::new_at(src, start).run();
    let (module, syntax_errors) = Parser::new(tokens).parse_module();
    let lex_lines: HashSet<u32> = lex_errors.iter().map(|e| e.span.start.line).collect();
    let mut diagnostics: Vec<Diagnostic> = lex_errors.iter().map(Diagnostic::from).collect();
    diagnostics.extend(
        syntax_errors
            .into_iter()
            .filter(|d| !lex_lines.contains(&d.span.start.line)),
    );
    diagnostics.sort_by_key(|d| d.span.start);
    (module, diagnostics)
}

/// Parses a single expression, such as a line typed into a REPL.
pub fn parse_expr(src: &str) -> Result<Expr, Vec<Diagnostic>> {
    let (tokens, lex_errors) = Lexer::new(src).run();
    if !lex_errors.is_empty() {
        return Err(lex_errors.iter().map(Diagnostic::from).collect());
    }
    let mut parser = Parser::new(tokens);
    let result = parser.expression();
    if result.is_ok() && !(parser.at_newline() || parser.at_eof()) {
        parser.expected("the end of the expression");
    }
    match result {
        Ok(expr) if parser.diagnostics.is_empty() => Ok(expr),
        _ => Err(parser.diagnostics),
    }
}

/// Marks a failed parse. The diagnostic was already recorded when this was
/// created, so callers only need to unwind to a recovery point.
struct Fail;

type PResult<X> = Result<X, Fail>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum FnMode {
    /// A function with a body.
    Body,
    /// A signature only, inside `trait` or `extern`.
    Signature,
}

enum Member {
    Field(FieldDecl),
    Method(Item),
}

enum Prefix {
    Unary(UnaryOp),
    Await,
    Spawn,
    Move,
}

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
    next_id: u32,
    diagnostics: Vec<Diagnostic>,
    /// Set once an unclosed block has been reported at the end of the file,
    /// so the blocks enclosing it are not reported as well.
    reported_unclosed: bool,
    /// End of the last token consumed, not counting line breaks, so spans
    /// never extend over a trailing newline.
    prev_end: Pos,
}

impl Parser {
    pub fn new(mut tokens: Vec<Token>) -> Parser {
        if !matches!(tokens.last().map(|t| &t.kind), Some(Tk::Eof)) {
            let end = tokens.last().map(|t| t.span.end).unwrap_or(Pos::START);
            tokens.push(Token::new(Tk::Eof, Span::at(end)));
        }
        let prev_end = tokens[0].span.start;
        Parser {
            tokens,
            pos: 0,
            next_id: 0,
            diagnostics: Vec::new(),
            reported_unclosed: false,
            prev_end,
        }
    }

    /// Parses every declaration in the file.
    pub fn parse_module(mut self) -> (Module, Vec<Diagnostic>) {
        let start = self.start();
        let mut items = Vec::new();
        let mut main_span: Option<Span> = None;
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.item() {
                Ok(item) => {
                    if matches!(item.kind, ItemKind::Main(_)) {
                        match main_span {
                            Some(first) => {
                                self.error(
                                    format!(
                                        "a program has one `main`; the first is on line {}",
                                        first.start.line
                                    ),
                                    item.span,
                                    Some("merge the two blocks, or move this code into a function"),
                                );
                            }
                            None => main_span = Some(item.span),
                        }
                    }
                    items.push(item);
                }
                Err(Fail) => self.recover(before),
            }
            if self.pos == before {
                self.bump();
            }
        }
        let span = self.span_from(start);
        (Module { items, span }, self.diagnostics)
    }

    // ==== cursor ===========================================================

    fn kind(&self) -> &Tk {
        &self.tokens[self.pos].kind
    }

    fn kind_at(&self, n: usize) -> &Tk {
        &self.tokens[(self.pos + n).min(self.tokens.len() - 1)].kind
    }

    fn span(&self) -> Span {
        self.tokens[self.pos].span
    }

    fn start(&self) -> Pos {
        self.span().start
    }

    fn span_from(&self, start: Pos) -> Span {
        if self.prev_end < start {
            Span::at(start)
        } else {
            Span::new(start, self.prev_end)
        }
    }

    fn at_kw(&self, keyword: K) -> bool {
        self.kind().is_keyword(keyword)
    }

    fn at_p(&self, punct: P) -> bool {
        self.kind().is_punct(punct)
    }

    fn at_newline(&self) -> bool {
        matches!(self.kind(), Tk::Newline)
    }

    fn at_eof(&self) -> bool {
        matches!(self.kind(), Tk::Eof)
    }

    fn bump(&mut self) -> Token {
        let token = self.tokens[self.pos].clone();
        if !matches!(token.kind, Tk::Eof) {
            self.pos += 1;
            if !matches!(token.kind, Tk::Newline) {
                self.prev_end = token.span.end;
            }
        }
        token
    }

    fn eat_kw(&mut self, keyword: K) -> bool {
        let found = self.at_kw(keyword);
        if found {
            self.bump();
        }
        found
    }

    fn eat_p(&mut self, punct: P) -> bool {
        let found = self.at_p(punct);
        if found {
            self.bump();
        }
        found
    }

    fn eat_newline(&mut self) -> bool {
        let found = self.at_newline();
        if found {
            self.bump();
        }
        found
    }

    fn skip_newlines(&mut self) {
        while self.at_newline() {
            self.bump();
        }
    }

    fn id(&mut self) -> NodeId {
        let id = NodeId(self.next_id);
        self.next_id += 1;
        id
    }

    // ==== diagnostics ======================================================

    fn error(&mut self, message: impl Into<String>, span: Span, hint: Option<&str>) -> Fail {
        let mut diagnostic = Diagnostic::new(message, span);
        if let Some(hint) = hint {
            diagnostic = diagnostic.with_hint(hint);
        }
        self.diagnostics.push(diagnostic);
        Fail
    }

    fn expected(&mut self, what: &str) -> Fail {
        // `[x for x in xs]`: a comprehension written the Python way.
        if self.at_kw(K::For) {
            return self.expected_with(
                what,
                Some("Vyrn comprehensions use `each`: `[x * 2 each x in xs]`"),
            );
        }
        self.expected_with(what, None)
    }

    /// Whether a statement starting with the name `word` is a Python header
    /// (`while x < 3:`, `def f():`, `try:`) rather than code using a
    /// variable that happens to have that name (`match = 1`, `with(x)`).
    fn is_python_header(&self, word: &str) -> bool {
        if python_replacement(word).is_none() {
            return false;
        }
        match self.kind_at(1) {
            // Used as a value: `match = 3`, `with.x`, `try(1)`, `pass[0]`.
            Tk::Punct(
                P::Eq
                | P::Dot
                | P::LParen
                | P::LBracket
                | P::Comma
                | P::RParen
                | P::PlusEq
                | P::MinusEq
                | P::StarEq
                | P::SlashEq,
            ) => false,
            Tk::Newline | Tk::Eof => matches!(word, "pass" | "break" | "continue"),
            _ => true,
        }
    }

    /// Explains the Vyrn word for a Python habit.
    fn python_word(&mut self, word: &str) -> Fail {
        let span = self.span();
        let (vyrn, example) = python_replacement(word).unwrap_or(("", ""));
        let _ = vyrn;
        self.error(
            format!("`{word}` is not a Vyrn keyword"),
            span,
            Some(example),
        )
    }

    fn expected_hint(&mut self, what: &str, hint: &str) -> Fail {
        self.expected_with(what, Some(hint))
    }

    fn expected_with(&mut self, what: &str, hint: Option<&str>) -> Fail {
        let found = self.kind().describe();
        let span = self.span();
        let keyword_hint;
        let hint = match (hint, self.kind()) {
            (Some(hint), _) => Some(hint),
            (None, Tk::Keyword(k)) if what.contains("name") => {
                keyword_hint = format!("`{k}` is a keyword and cannot be used as a name");
                Some(keyword_hint.as_str())
            }
            (None, _) => None,
        };
        self.error(format!("expected {what}, found {found}"), span, hint)
    }

    fn expect_p(&mut self, punct: P, what: &str) -> PResult<Span> {
        if self.at_p(punct) {
            Ok(self.bump().span)
        } else {
            Err(self.expected(what))
        }
    }

    fn expect_kw(&mut self, keyword: K, what: &str) -> PResult<Span> {
        if self.at_kw(keyword) {
            Ok(self.bump().span)
        } else {
            Err(self.expected(what))
        }
    }

    fn expect_ident(&mut self, what: &str) -> PResult<Ident> {
        if let Tk::Ident(name) = self.kind() {
            let name = name.clone();
            let span = self.bump().span;
            Ok(Ident { name, span })
        } else {
            Err(self.expected(what))
        }
    }

    fn expect_newline(&mut self) -> PResult<()> {
        if self.eat_newline() || self.at_eof() {
            Ok(())
        } else {
            Err(self.expected("the end of the line"))
        }
    }

    /// The `:` that ends every header line.
    fn header_colon(&mut self, what: &str) -> PResult<()> {
        if self.eat_p(P::Colon) {
            return Ok(());
        }
        let hint = if self.at_p(P::Eq) {
            "use `==` to compare; `=` assigns"
        } else {
            "a header line ends with `:`, followed by one statement or by a block closed with `end`"
        };
        Err(self.expected_hint(&format!("`:` to end the `{what}` line"), hint))
    }

    /// The line break after a header whose body must be a block.
    fn expect_block_start(&mut self, what: &str) -> PResult<()> {
        if self.eat_newline() {
            Ok(())
        } else {
            Err(self.expected_hint(
                &format!("a new line after `{what} ...:`"),
                "the contents go on the following lines, closed with `end`",
            ))
        }
    }

    fn expect_end(&mut self, what: &str, header: Span) -> PResult<()> {
        if self.eat_kw(K::End) {
            return self.expect_newline();
        }
        if self.at_eof() {
            if self.reported_unclosed {
                return Err(Fail);
            }
            self.reported_unclosed = true;
            return Err(self.error(
                format!("this `{what}` block is never closed"),
                header,
                Some("add `end` on its own line after the last line of the block"),
            ));
        }
        Err(self.expected_hint(
            &format!(
                "`end` to close the `{what}` block from line {}",
                header.start.line
            ),
            "every block closes with `end`",
        ))
    }

    /// Recognises Python spellings at the start of a statement or
    /// declaration and reports them with the Vyrn equivalent.
    fn python_habit(&mut self) -> Option<Fail> {
        let Tk::Ident(word) = self.kind() else {
            return None;
        };
        let next = self.kind_at(1);
        let line_ends = matches!(next, Tk::Newline | Tk::Eof);
        let names_something = matches!(next, Tk::Ident(_));
        let hint = match word.as_str() {
            "def" | "fn" if names_something => {
                "functions start with `create`: `create name(args):` ... `end`"
            }
            "import" if names_something => "write `use module`",
            "from" if names_something => "write `use module (name)`",
            "raise" if names_something => "write `fail error`",
            "lambda" if names_something || next.is_punct(P::Colon) => "write `(x) => expression`",
            "except" if names_something || next.is_punct(P::Colon) => {
                "write `catch ErrorType as e:`"
            }
            "case"
                if names_something
                    || matches!(
                        next,
                        Tk::Int { .. }
                            | Tk::Float { .. }
                            | Tk::Str(_)
                            | Tk::Punct(P::Underscore)
                            | Tk::Keyword(K::True | K::False | K::None)
                    ) =>
            {
                "match arms start with `is`: `is 0: ...`"
            }
            "break" if line_ends => "write `stop` to leave a loop",
            "continue" if line_ends => "write `skip` to go to the next iteration",
            "pass" if line_ends => "nothing is needed here: `end` closes an empty block",
            _ => return None,
        };
        let message = format!("`{word}` is not a Vyrn keyword");
        let span = self.span();
        Some(self.error(message, span, Some(hint)))
    }

    // ==== recovery =========================================================

    /// Skips the rest of the current line.
    fn recover_line(&mut self) {
        while !self.at_newline() && !self.at_eof() {
            self.bump();
        }
        self.eat_newline();
    }

    /// Skips past a failed declaration or statement that began at token
    /// index `start`: the rest of the current line, then any blocks opened
    /// since `start` that are not yet closed.
    fn recover(&mut self, start: usize) {
        self.recover_line();
        let mut depth = 0;
        let mut line_start = start;
        for i in start..self.pos {
            if matches!(self.tokens[i].kind, Tk::Newline) {
                depth += line_effect(&self.tokens[line_start..i]);
                line_start = i + 1;
            }
        }
        if line_start < self.pos {
            depth += line_effect(&self.tokens[line_start..self.pos]);
        }
        while depth > 0 && !self.at_eof() {
            let from = self.pos;
            while !self.at_newline() && !self.at_eof() {
                self.bump();
            }
            depth += line_effect(&self.tokens[from..self.pos]);
            self.eat_newline();
        }
    }

    // ==== documentation ====================================================

    fn docs(&mut self) -> Vec<String> {
        let mut docs = Vec::new();
        while let Tk::DocComment(text) = self.kind() {
            docs.push(text.clone());
            self.bump();
        }
        docs
    }

    fn index_past_docs(&self) -> usize {
        let mut i = self.pos;
        while matches!(self.tokens[i].kind, Tk::DocComment(_)) {
            i += 1;
        }
        i
    }

    /// Drops doc comments that have nothing after them to document.
    fn drop_stray_docs(&mut self) {
        let i = self.index_past_docs();
        if i != self.pos && is_closer(&self.tokens[i].kind) {
            while self.pos < i {
                self.bump();
            }
        }
    }

    // ==== declarations =====================================================

    fn fn_decl_starts_at(&self, i: usize) -> bool {
        let at = |n: usize| &self.tokens[(i + n).min(self.tokens.len() - 1)].kind;
        match at(0) {
            Tk::Keyword(K::Strict | K::Kernel | K::Export) => true,
            Tk::Keyword(K::Async) => match at(1) {
                Tk::Keyword(K::Fn) => matches!(at(2), Tk::Ident(_)),
                Tk::Keyword(K::Strict | K::Kernel | K::Export) => true,
                _ => false,
            },
            Tk::Keyword(K::Fn) => matches!(at(1), Tk::Ident(_) | Tk::Keyword(K::Main)),
            _ => false,
        }
    }

    fn at_fn_decl(&self) -> bool {
        self.fn_decl_starts_at(self.pos)
    }

    fn item(&mut self) -> PResult<Item> {
        let start = self.start();
        let doc = self.docs();
        let mut decorators = Vec::new();
        while self.at_p(P::At) {
            decorators.push(self.decorator()?);
        }
        let vis = if self.eat_kw(K::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let kind = match self.kind() {
            Tk::Keyword(K::Use) => ItemKind::Use(self.use_decl()?),
            Tk::Keyword(K::Main) => ItemKind::Main(self.main_decl()?),
            Tk::Keyword(K::Const) => ItemKind::Const(self.const_decl()?),
            Tk::Keyword(K::Type) => ItemKind::TypeAlias(self.type_alias()?),
            Tk::Keyword(K::Struct | K::Object) => ItemKind::Aggregate(self.aggregate_decl()?),
            Tk::Keyword(K::Enum) => ItemKind::Enum(self.enum_decl()?),
            Tk::Keyword(K::Trait) => ItemKind::Trait(self.trait_decl()?),
            Tk::Keyword(K::Impl) => ItemKind::Impl(self.impl_decl()?),
            Tk::Keyword(K::Route) => ItemKind::Route(self.route_decl()?),
            Tk::Keyword(K::Routes) => ItemKind::RouteGroup(self.route_group()?),
            Tk::Keyword(K::Extern) => ItemKind::Extern(self.extern_decl()?),
            _ if self.at_fn_decl() => ItemKind::Fn(self.fn_decl(FnMode::Body)?),
            _ => return Err(self.not_an_item()),
        };
        Ok(Item {
            id: self.id(),
            span: self.span_from(start),
            doc,
            decorators,
            vis,
            kind,
        })
    }

    fn not_an_item(&mut self) -> Fail {
        if let Some(fail) = self.python_habit() {
            return fail;
        }
        if self.at_kw(K::End) {
            let span = self.span();
            return self.error(
                "`end` has no open block to close",
                span,
                Some("remove it, or look for an extra `end` in the block above"),
            );
        }
        if let Tk::Ident(word) = self.kind() {
            if self.is_python_header(word) {
                let word = word.clone();
                return self.python_word(&word);
            }
        }
        let statement_like = matches!(
            self.kind(),
            Tk::Ident(_)
                | Tk::Keyword(
                    K::Let
                        | K::Var
                        | K::If
                        | K::Each
                        | K::Par
                        | K::Loop
                        | K::When
                        | K::Return
                        | K::Attempt
                        | K::Using
                        | K::On
                        | K::Region
                        | K::Await
                )
        );
        if statement_like {
            self.expected_hint(
                "a declaration",
                "statements run inside `main:` or a function; the top level of a file holds declarations only",
            )
        } else {
            self.expected("a declaration such as `create`, `struct`, `route` or `main:`")
        }
    }

    fn decorator(&mut self) -> PResult<Decorator> {
        let start = self.start();
        self.bump();
        let name = self.dotted_name("a decorator name")?;
        let args = if self.eat_p(P::LParen) {
            self.arguments(P::RParen)?
        } else {
            Vec::new()
        };
        let span = self.span_from(start);
        self.expect_newline()?;
        Ok(Decorator { name, args, span })
    }

    fn dotted_name(&mut self, what: &str) -> PResult<Path> {
        let start = self.start();
        let mut segments = vec![self.expect_ident(what)?];
        while self.at_p(P::Dot) && matches!(self.kind_at(1), Tk::Ident(_)) {
            self.bump();
            segments.push(self.expect_ident("a name")?);
        }
        Ok(Path {
            segments,
            span: self.span_from(start),
        })
    }

    fn use_decl(&mut self) -> PResult<UseDecl> {
        self.bump();
        let mut url = None;
        let path = if let Tk::Str(parts) = self.kind() {
            let parts = parts.clone();
            let span = self.span();
            self.bump();
            let text = match parts.as_slice() {
                [StrPart::Text(t)] => t.clone(),
                _ => {
                    return Err(self.error(
                        "a module URL cannot contain `{...}`".to_string(),
                        span,
                        None,
                    ))
                }
            };
            let name = url_module_name(&text).ok_or_else(|| {
                self.error(
                    format!("`{text}` is not a module URL"),
                    span,
                    Some("write it as \"host/owner/repo@version\", for example \"github.com/ann/neural@v1\""),
                )
            })?;
            url = Some(text);
            Path {
                segments: vec![Ident { name, span }],
                span,
            }
        } else {
            self.dotted_name("a module name after `use`")?
        };
        let kind = if self.eat_kw(K::As) {
            UseKind::Module {
                alias: Some(self.expect_ident("a name after `as`")?),
            }
        } else if self.eat_p(P::LParen) {
            let mut names = Vec::new();
            loop {
                if self.eat_p(P::RParen) {
                    break;
                }
                let name = self.expect_ident("a name to import")?;
                let alias = if self.eat_kw(K::As) {
                    Some(self.expect_ident("a name after `as`")?)
                } else {
                    None
                };
                names.push(UseName { name, alias });
                if !self.eat_p(P::Comma) {
                    self.expect_p(P::RParen, "`,` or `)`")?;
                    break;
                }
            }
            UseKind::Names(names)
        } else {
            UseKind::Module { alias: None }
        };
        self.expect_newline()?;
        Ok(UseDecl { path, kind, url })
    }

    fn main_decl(&mut self) -> PResult<MainDecl> {
        let header = self.bump().span;
        let params = if self.eat_p(P::LParen) {
            self.params()?
        } else {
            Vec::new()
        };
        let body = self.suite("main", header)?;
        Ok(MainDecl { params, body })
    }

    fn const_decl(&mut self) -> PResult<ConstDecl> {
        self.bump();
        let name = self.expect_ident("a constant name")?;
        let declared_ty = if self.eat_p(P::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        if !self.eat_p(P::Eq) {
            return Err(self.expected_hint(
                "`=` and a value",
                "a constant needs a value: `const MAX = 10`",
            ));
        }
        let value = self.expression()?;
        self.expect_newline()?;
        Ok(ConstDecl {
            name,
            declared_ty,
            value,
            ty: TypeSlot::Pending,
        })
    }

    fn type_alias(&mut self) -> PResult<TypeAliasDecl> {
        self.bump();
        let name = self.expect_ident("a type name")?;
        let generics = self.opt_generics()?;
        self.expect_p(P::Eq, "`=` and the type it names")?;
        let target = self.type_expr()?;
        self.expect_newline()?;
        Ok(TypeAliasDecl {
            name,
            generics,
            target,
        })
    }

    fn opt_generics(&mut self) -> PResult<Vec<GenericParam>> {
        if self.at_p(P::LBracket) {
            self.generic_params()
        } else {
            Ok(Vec::new())
        }
    }

    fn generic_params(&mut self) -> PResult<Vec<GenericParam>> {
        self.bump();
        let mut params = Vec::new();
        loop {
            if self.at_p(P::RBracket) {
                break;
            }
            let start = self.start();
            let name = self.expect_ident("a type parameter name")?;
            let mut bounds = Vec::new();
            if self.eat_p(P::Colon) {
                bounds.push(self.type_expr()?);
                while self.eat_p(P::Plus) {
                    bounds.push(self.type_expr()?);
                }
            }
            params.push(GenericParam {
                name,
                bounds,
                span: self.span_from(start),
            });
            if !self.eat_p(P::Comma) {
                break;
            }
        }
        self.expect_p(P::RBracket, "`,` or `]` after the type parameters")?;
        Ok(params)
    }

    /// In `impl [T] Foo[T]:` the brackets are type parameters; in
    /// `impl [Entity]:` they are a list type.
    fn brackets_are_generics(&self) -> bool {
        let mut depth = 0;
        let mut i = self.pos;
        loop {
            match &self.tokens[i].kind {
                Tk::Punct(P::LBracket) => depth += 1,
                Tk::Punct(P::RBracket) => {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                Tk::Newline | Tk::Eof => return false,
                _ => {}
            }
            i += 1;
        }
        !matches!(
            self.tokens[(i + 1).min(self.tokens.len() - 1)].kind,
            Tk::Punct(P::Colon) | Tk::Keyword(K::For)
        )
    }

    fn fn_decl(&mut self, mode: FnMode) -> PResult<FnDecl> {
        let header = self.span();
        let mut modifiers = FnModifiers::default();
        loop {
            match self.kind() {
                Tk::Keyword(K::Async) => modifiers.is_async = true,
                Tk::Keyword(K::Strict) => modifiers.is_strict = true,
                Tk::Keyword(K::Kernel) => modifiers.is_kernel = true,
                Tk::Keyword(K::Export) => {
                    self.bump();
                    let abi = match self.kind() {
                        Tk::Str(parts) => {
                            let abi = plain_text(parts);
                            self.bump();
                            Some(abi)
                        }
                        _ => None,
                    };
                    modifiers.export = Some(abi);
                    continue;
                }
                _ => break,
            }
            self.bump();
        }
        self.expect_kw(K::Fn, "`create`")?;
        if self.at_kw(K::Main) {
            let span = self.span();
            return Err(self.error(
                "`main` is not a function name",
                span,
                Some("the program starts at a `main:` block: `main:` ... `end`"),
            ));
        }
        let name = self.expect_ident("a function name")?;
        let generics = self.opt_generics()?;
        self.expect_p(P::LParen, "`(` to start the parameter list")?;
        let params = self.params()?;
        let result = self.result_spec()?;
        let body = match mode {
            FnMode::Body => FnBody::Block(self.suite("create", header)?),
            FnMode::Signature => {
                if self.at_p(P::Colon) {
                    let span = self.span();
                    return Err(self.error(
                        "this function cannot have a body",
                        span,
                        Some("functions in a `trait` or `extern` block are signatures only; remove the `:` and the body"),
                    ));
                }
                self.expect_newline()?;
                FnBody::Signature
            }
        };
        Ok(FnDecl {
            name,
            modifiers,
            generics,
            params,
            result,
            body,
            ty: TypeSlot::Pending,
        })
    }

    /// Parameters up to and including the closing `)`; the `(` is consumed.
    fn params(&mut self) -> PResult<Vec<Param>> {
        let mut params = Vec::new();
        loop {
            if self.eat_p(P::RParen) {
                break;
            }
            params.push(self.param()?);
            if !self.eat_p(P::Comma) {
                self.expect_p(P::RParen, "`,` or `)` in the parameter list")?;
                break;
            }
        }
        Ok(params)
    }

    fn param(&mut self) -> PResult<Param> {
        let start = self.start();
        let pass = self.pass_mode();
        let name = self.expect_ident("a parameter name")?;
        let declared_ty = if self.eat_p(P::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        let default = if self.eat_p(P::Eq) {
            Some(self.expression()?)
        } else {
            None
        };
        Ok(Param {
            id: self.id(),
            span: self.span_from(start),
            pass,
            name,
            declared_ty,
            default,
            ty: TypeSlot::Pending,
        })
    }

    fn pass_mode(&mut self) -> PassMode {
        if self.eat_kw(K::Mut) {
            PassMode::Mut
        } else if self.eat_kw(K::Own) {
            PassMode::Own
        } else {
            PassMode::Borrow
        }
    }

    fn result_spec(&mut self) -> PResult<ResultSpec> {
        let returns = if self.eat_p(P::Arrow) {
            Some(self.type_expr()?)
        } else {
            None
        };
        let fails = if self.eat_kw(K::Fails) {
            Some(self.type_expr()?)
        } else {
            None
        };
        Ok(ResultSpec { returns, fails })
    }

    fn aggregate_decl(&mut self) -> PResult<AggregateDecl> {
        let header = self.span();
        let kind = if self.bump().kind.is_keyword(K::Struct) {
            AggregateKind::Struct
        } else {
            AggregateKind::Class
        };
        let what = match kind {
            AggregateKind::Struct => "struct",
            AggregateKind::Class => "object",
        };
        let name = self.expect_ident(&format!("a {what} name"))?;
        let generics = self.opt_generics()?;
        let mut base = None;
        if self.at_p(P::LParen) {
            let open = self.span();
            self.bump();
            let ty = self.type_expr()?;
            self.expect_p(P::RParen, "`)` after the base class")?;
            if kind == AggregateKind::Struct {
                let span = open.to(Span::at(self.prev_end));
                self.error(
                    "a struct cannot extend another type",
                    span,
                    Some("only an `object` can have a base; give the struct a field of that type instead"),
                );
            } else {
                base = Some(ty);
            }
        }
        self.header_colon(what)?;
        self.expect_block_start(what)?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if self.at_kw(K::End) || self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.member() {
                Ok(Member::Field(field)) => fields.push(field),
                Ok(Member::Method(method)) => methods.push(method),
                Err(Fail) => self.recover(before),
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect_end(what, header)?;
        Ok(AggregateDecl {
            kind,
            name,
            generics,
            base,
            fields,
            methods,
        })
    }

    fn member(&mut self) -> PResult<Member> {
        if self.fn_decl_starts_at(self.index_past_docs())
            || matches!(self.tokens[self.index_past_docs()].kind, Tk::Punct(P::At))
            || (self.at_kw(K::Pub) && self.fn_decl_starts_at(self.pos + 1))
        {
            return Ok(Member::Method(self.method_item()?));
        }
        self.docs();
        let start = self.start();
        let vis = if self.eat_kw(K::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };
        let name = self.expect_ident("a field name or `create`")?;
        let declared_ty = if self.eat_p(P::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        let default = if self.eat_p(P::Eq) {
            Some(self.expression()?)
        } else {
            None
        };
        if declared_ty.is_none() && default.is_none() {
            return Err(self.expected_hint(
                "`:` and a type, or `=` and a default value",
                "a field is written `name: Type` or `name = default`",
            ));
        }
        let span = self.span_from(start);
        self.expect_newline()?;
        Ok(Member::Field(FieldDecl {
            id: self.id(),
            span,
            vis,
            name,
            declared_ty,
            default,
            ty: TypeSlot::Pending,
        }))
    }

    /// A function with its docs, decorators and visibility, as an item.
    fn method_item(&mut self) -> PResult<Item> {
        let start = self.start();
        let doc = self.docs();
        let mut decorators = Vec::new();
        while self.at_p(P::At) {
            decorators.push(self.decorator()?);
        }
        let vis = if self.eat_kw(K::Pub) {
            Visibility::Public
        } else {
            Visibility::Private
        };
        if !self.at_fn_decl() {
            return Err(self.expected("`create`"));
        }
        let decl = self.fn_decl(FnMode::Body)?;
        Ok(Item {
            id: self.id(),
            span: self.span_from(start),
            doc,
            decorators,
            vis,
            kind: ItemKind::Fn(decl),
        })
    }

    fn enum_decl(&mut self) -> PResult<EnumDecl> {
        let header = self.bump().span;
        let name = self.expect_ident("an enum name")?;
        let generics = self.opt_generics()?;
        self.header_colon("enum")?;
        self.expect_block_start("enum")?;
        let mut variants = Vec::new();
        let mut methods = Vec::new();
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if self.at_kw(K::End) || self.at_eof() {
                break;
            }
            let before = self.pos;
            let is_variant = matches!(self.tokens[self.index_past_docs()].kind, Tk::Ident(_));
            let result = if is_variant {
                self.docs();
                self.variant().map(|v| variants.push(v))
            } else {
                self.method_item().map(|m| methods.push(m))
            };
            if result.is_err() {
                self.recover(before);
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect_end("enum", header)?;
        Ok(EnumDecl {
            name,
            generics,
            variants,
            methods,
        })
    }

    fn variant(&mut self) -> PResult<Variant> {
        let start = self.start();
        let name = self.expect_ident("a variant name")?;
        let mut fields = Vec::new();
        let mut discriminant = None;
        if self.eat_p(P::LParen) {
            loop {
                if self.eat_p(P::RParen) {
                    break;
                }
                let field_name =
                    if matches!(self.kind(), Tk::Ident(_)) && self.kind_at(1).is_punct(P::Colon) {
                        let name = self.expect_ident("a field name")?;
                        self.bump();
                        Some(name)
                    } else {
                        None
                    };
                let ty = self.type_expr()?;
                fields.push(VariantField {
                    name: field_name,
                    ty,
                });
                if !self.eat_p(P::Comma) {
                    self.expect_p(P::RParen, "`,` or `)`")?;
                    break;
                }
            }
        } else if self.eat_p(P::Eq) {
            discriminant = Some(self.expression()?);
        }
        let span = self.span_from(start);
        self.expect_newline()?;
        Ok(Variant {
            id: self.id(),
            span,
            name,
            fields,
            discriminant,
        })
    }

    /// Types up to and including `close`; the opening bracket is consumed.
    fn type_list(&mut self, close: P) -> PResult<Vec<TypeExpr>> {
        let mut types = Vec::new();
        loop {
            if self.eat_p(close) {
                break;
            }
            types.push(self.type_expr()?);
            if !self.eat_p(P::Comma) {
                self.expect_p(close, &format!("`,` or `{close}`"))?;
                break;
            }
        }
        Ok(types)
    }

    fn trait_decl(&mut self) -> PResult<TraitDecl> {
        let header = self.bump().span;
        let name = self.expect_ident("a trait name")?;
        let generics = self.opt_generics()?;
        let supertraits = if self.eat_p(P::LParen) {
            self.type_list(P::RParen)?
        } else {
            Vec::new()
        };
        self.header_colon("trait")?;
        self.expect_block_start("trait")?;
        let methods = self.signatures()?;
        self.expect_end("trait", header)?;
        Ok(TraitDecl {
            name,
            generics,
            supertraits,
            methods,
        })
    }

    /// Function signatures, one per line, up to `end`.
    fn signatures(&mut self) -> PResult<Vec<FnDecl>> {
        let mut functions = Vec::new();
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if self.at_kw(K::End) || self.at_eof() {
                break;
            }
            self.docs();
            let before = self.pos;
            match self.fn_decl(FnMode::Signature) {
                Ok(f) => functions.push(f),
                Err(Fail) => self.recover_line(),
            }
            if self.pos == before {
                self.bump();
            }
        }
        Ok(functions)
    }

    fn impl_decl(&mut self) -> PResult<ImplDecl> {
        let header = self.bump().span;
        let generics = if self.at_p(P::LBracket) && self.brackets_are_generics() {
            self.generic_params()?
        } else {
            Vec::new()
        };
        let first = self.type_expr()?;
        let (trait_, target) = if self.eat_kw(K::For) {
            (Some(first), self.type_expr()?)
        } else {
            (None, first)
        };
        self.header_colon("impl")?;
        self.expect_block_start("impl")?;
        let mut methods = Vec::new();
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if self.at_kw(K::End) || self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.method_item() {
                Ok(m) => methods.push(m),
                Err(Fail) => self.recover(before),
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect_end("impl", header)?;
        Ok(ImplDecl {
            generics,
            trait_,
            target,
            methods,
        })
    }

    fn http_method(&mut self) -> PResult<(HttpMethod, Span)> {
        let span = self.span();
        let name = match self.kind() {
            Tk::Ident(name) => Some(name.clone()),
            _ => None,
        };
        if let Some(name) = name {
            if let Some(method) = HttpMethod::from_ident(&name) {
                self.bump();
                return Ok((method, span));
            }
            let upper = name.to_uppercase();
            if let Some(method) = HttpMethod::from_ident(&upper) {
                self.error(
                    format!("HTTP methods are written in capitals: `{upper}`"),
                    span,
                    None,
                );
                self.bump();
                return Ok((method, span));
            }
        }
        Err(self.expected_hint(
            "an HTTP method",
            "one of GET, POST, PUT, PATCH, DELETE, HEAD, OPTIONS, WS or SSE",
        ))
    }

    fn route_path(&mut self) -> PResult<RoutePath> {
        let span = self.span();
        let parts = match self.kind() {
            Tk::Str(parts) => parts.clone(),
            _ => {
                return Err(self.expected_hint(
                    "a quoted path",
                    "route paths are strings: `route GET \"/users/{id}\" (id: u64):`",
                ))
            }
        };
        self.bump();
        RoutePath::from_str_parts(&parts, span).map_err(|e| self.error(e.message, e.span, None))
    }

    fn route_decl(&mut self) -> PResult<RouteDecl> {
        let header = self.bump().span;
        let (method, method_span) = self.http_method()?;
        let path = self.route_path()?;
        let params = if self.eat_p(P::LParen) {
            self.params()?
        } else {
            Vec::new()
        };
        let result = self.result_spec()?;
        for hole in path.params() {
            if !params.iter().any(|p| p.name.name == hole.name) {
                let hint = format!("add `{}: Type` to the route's parameter list", hole.name);
                self.error(
                    format!(
                        "path parameter `{{{}}}` has no matching parameter",
                        hole.name
                    ),
                    hole.span,
                    Some(&hint),
                );
            }
        }
        let body = self.suite("route", header)?;
        Ok(RouteDecl {
            method,
            method_span,
            path,
            params,
            result,
            body,
            response_ty: TypeSlot::Pending,
        })
    }

    fn route_group(&mut self) -> PResult<RouteGroup> {
        let header = self.bump().span;
        let prefix = self.route_path()?;
        self.header_colon("routes")?;
        self.expect_block_start("routes")?;
        let mut items = Vec::new();
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if self.at_kw(K::End) || self.at_eof() {
                break;
            }
            let before = self.pos;
            match self.route_item() {
                Ok(item) => items.push(item),
                Err(Fail) => self.recover(before),
            }
            if self.pos == before {
                self.bump();
            }
        }
        self.expect_end("routes", header)?;
        Ok(RouteGroup { prefix, items })
    }

    fn route_item(&mut self) -> PResult<Item> {
        let start = self.start();
        let doc = self.docs();
        let mut decorators = Vec::new();
        while self.at_p(P::At) {
            decorators.push(self.decorator()?);
        }
        let kind = match self.kind() {
            Tk::Keyword(K::Route) => ItemKind::Route(self.route_decl()?),
            Tk::Keyword(K::Routes) => ItemKind::RouteGroup(self.route_group()?),
            _ => {
                return Err(
                    self.expected_hint("`route` or `routes`", "a `routes` group holds routes only")
                )
            }
        };
        Ok(Item {
            id: self.id(),
            span: self.span_from(start),
            doc,
            decorators,
            vis: Visibility::Private,
            kind,
        })
    }

    fn extern_decl(&mut self) -> PResult<ExternDecl> {
        let header = self.bump().span;
        let abi = match self.kind() {
            Tk::Str(parts) => {
                let abi = plain_text(parts);
                self.bump();
                abi
            }
            _ => {
                return Err(self.expected_hint(
                    "the ABI name as a string",
                    "for example `extern \"c\"` or `extern \"js\"`",
                ))
            }
        };
        let functions = if self.eat_p(P::Colon) {
            self.expect_block_start("extern")?;
            let functions = self.signatures()?;
            self.expect_end("extern", header)?;
            functions
        } else {
            vec![self.fn_decl(FnMode::Signature)?]
        };
        Ok(ExternDecl { abi, functions })
    }

    // ==== blocks and statements ============================================

    /// `:` followed by one statement, or by a line break, a block and `end`.
    fn suite(&mut self, what: &str, header: Span) -> PResult<Block> {
        self.header_colon(what)?;
        if self.eat_newline() {
            let block = self.block();
            self.expect_end(what, header)?;
            Ok(block)
        } else {
            self.inline_body()
        }
    }

    /// The body of a clause in a chain (`elif`, `else`, `is`, `catch`,
    /// `finally`), which shares the chain's single `end`.
    fn clause_body(&mut self) -> PResult<Block> {
        if self.eat_newline() {
            Ok(self.block())
        } else {
            self.inline_body()
        }
    }

    fn inline_body(&mut self) -> PResult<Block> {
        let start = self.start();
        let kind = self.simple_stmt()?;
        let span = self.span_from(start);
        self.expect_newline()?;
        let stmt = Stmt {
            id: self.id(),
            span,
            kind,
        };
        Ok(Block {
            stmts: vec![stmt],
            span,
        })
    }

    /// Statements up to the next `end`, `elif`, `else`, `is`, `catch`,
    /// `finally` or the end of the file.
    fn block(&mut self) -> Block {
        let start = self.start();
        let mut stmts = Vec::new();
        loop {
            self.skip_newlines();
            self.drop_stray_docs();
            if is_closer(self.kind()) {
                break;
            }
            let before = self.pos;
            match self.statement() {
                Ok(stmt) => stmts.push(stmt),
                Err(Fail) => self.recover(before),
            }
            if self.pos == before {
                self.bump();
            }
        }
        Block {
            stmts,
            span: self.span_from(start),
        }
    }

    fn statement(&mut self) -> PResult<Stmt> {
        let after_docs = self.index_past_docs();
        if self.tokens[after_docs].kind.is_punct(P::At) || self.fn_decl_starts_at(after_docs) {
            let item = self.method_item()?;
            return Ok(Stmt {
                id: self.id(),
                span: item.span,
                kind: StmtKind::Item(Box::new(item)),
            });
        }
        self.docs();
        let start = self.start();
        let kind = match self.kind() {
            Tk::Keyword(K::If) => self.if_stmt()?,
            // `loop:` runs until `stop`; `loop condition:` while it holds.
            Tk::Keyword(K::Loop) if self.kind_at(1).is_punct(P::Colon) => {
                let header = self.bump().span;
                StmtKind::Loop(self.suite("loop", header)?)
            }
            Tk::Keyword(K::Loop) => self.while_stmt()?,
            Tk::Keyword(K::Each | K::Par) => self.for_stmt()?,
            Tk::Keyword(K::Async) if self.kind_at(1).is_keyword(K::Each) => self.for_stmt()?,
            Tk::Keyword(K::Async) if self.kind_at(1).is_keyword(K::Using) => self.with_stmt()?,
            Tk::Keyword(K::Using) => self.with_stmt()?,
            Tk::Keyword(K::When) => self.match_stmt()?,
            Tk::Keyword(K::Attempt) if self.kind_at(1).is_punct(P::Colon) => self.try_stmt()?,
            Tk::Keyword(K::For) => return Err(self.python_word("for")),
            Tk::Ident(word) if self.is_python_header(word) => {
                let word = word.clone();
                return Err(self.python_word(&word));
            }
            Tk::Keyword(K::On) => {
                let header = self.bump().span;
                let target = self.coalesce()?;
                let body = self.suite("on", header)?;
                StmtKind::On(OnBlock {
                    target,
                    body,
                    place: PlaceSlot::Pending,
                })
            }
            Tk::Keyword(K::Region) => {
                let header = self.bump().span;
                let name = self.expect_ident("a name for the region")?;
                let body = self.suite("region", header)?;
                StmtKind::Region(RegionBlock {
                    name,
                    body,
                    place: PlaceSlot::Pending,
                })
            }
            Tk::Keyword(K::Unsafe) => {
                let header = self.bump().span;
                StmtKind::Unsafe(self.suite("unsafe", header)?)
            }
            _ => {
                let kind = self.simple_stmt()?;
                let span = self.span_from(start);
                self.expect_newline()?;
                return Ok(Stmt {
                    id: self.id(),
                    span,
                    kind,
                });
            }
        };
        Ok(Stmt {
            id: self.id(),
            span: self.span_from(start),
            kind,
        })
    }

    fn simple_stmt(&mut self) -> PResult<StmtKind> {
        if let Some(fail) = self.python_habit() {
            return Err(fail);
        }
        match self.kind() {
            Tk::Keyword(K::Let | K::Var | K::Const) => self.binding(),
            Tk::Keyword(K::Return) => {
                self.bump();
                if self.at_newline() || self.at_eof() {
                    Ok(StmtKind::Return(None))
                } else {
                    Ok(StmtKind::Return(Some(self.expression_list()?)))
                }
            }
            Tk::Keyword(K::Fail) => {
                self.bump();
                Ok(StmtKind::Fail(self.expression()?))
            }
            Tk::Keyword(K::Defer) => {
                self.bump();
                Ok(StmtKind::Defer(self.expression()?))
            }
            Tk::Keyword(K::Stop) => {
                self.bump();
                Ok(StmtKind::Stop)
            }
            Tk::Keyword(K::Skip) => {
                self.bump();
                Ok(StmtKind::Skip)
            }
            Tk::Keyword(
                keyword @ (K::If
                | K::Loop
                | K::Each
                | K::Par
                | K::When
                | K::Using
                | K::On
                | K::Region
                | K::Unsafe),
            ) => {
                let keyword = *keyword;
                let span = self.span();
                Err(self.error(
                    format!("`{keyword}` cannot follow `:` on the same line"),
                    span,
                    Some("a one-line body holds one simple statement; start a new line and close the block with `end`"),
                ))
            }
            Tk::Keyword(K::Attempt) if self.kind_at(1).is_punct(P::Colon) => {
                let span = self.span();
                Err(self.error(
                    "`attempt` cannot follow `:` on the same line",
                    span,
                    Some("a one-line body holds one simple statement; start a new line and close the block with `end`"),
                ))
            }
            _ => self.expr_stmt(),
        }
    }

    fn binding(&mut self) -> PResult<StmtKind> {
        let kind = match self.bump().kind {
            Tk::Keyword(K::Let) => BindingKind::Let,
            Tk::Keyword(K::Var) => BindingKind::Var,
            _ => BindingKind::Const,
        };
        let target = self.bind_target()?;
        let declared_ty = if self.eat_p(P::Colon) {
            Some(self.type_expr()?)
        } else {
            None
        };
        let value = if self.eat_p(P::Eq) {
            Some(self.expression_list()?)
        } else {
            None
        };
        Ok(StmtKind::Binding(Binding {
            kind,
            target,
            declared_ty,
            value,
            ty: TypeSlot::Pending,
            place: PlaceSlot::Pending,
        }))
    }

    fn expr_stmt(&mut self) -> PResult<StmtKind> {
        let target = self.expression_list()?;
        let op = match self.kind() {
            Tk::Punct(p) => assign_op(*p),
            _ => None,
        };
        if let Some(op) = op {
            self.bump();
            let value = self.expression_list()?;
            if !assignable(&target, op == AssignOp::Assign) {
                let hint = if matches!(target.kind, ExprKind::Tuple(_)) {
                    "a compound assignment such as `+=` takes a single target"
                } else {
                    "the left side must be a name, a field (`a.b`) or an element (`a[i]`)"
                };
                self.error("cannot assign to this expression", target.span, Some(hint));
            }
            return Ok(StmtKind::Assign { target, op, value });
        }
        if self.at_p(P::Colon) && matches!(target.kind, ExprKind::Name(_)) {
            let span = self.span();
            return Err(self.error(
                "a typed variable is declared with `let` or `var`",
                span,
                Some("write `var name: Type = value`"),
            ));
        }
        Ok(StmtKind::Expr(target))
    }

    fn condition(&mut self) -> PResult<Condition> {
        if self.eat_kw(K::Let) {
            let pattern = self.pattern()?;
            self.expect_p(P::Eq, "`=` after the pattern")?;
            let value = self.expression()?;
            Ok(Condition::Let { pattern, value })
        } else {
            Ok(Condition::Expr(self.expression()?))
        }
    }

    fn if_stmt(&mut self) -> PResult<StmtKind> {
        let header = self.bump().span;
        let condition = self.condition()?;
        self.header_colon("if")?;

        if !self.at_newline() {
            let body = self.inline_body()?;
            let mut branches = vec![(condition, body)];
            let mut else_body = None;
            if self.at_kw(K::Else) {
                let span = self.span();
                self.error(
                    "a one-line `if` cannot continue with `else`",
                    span,
                    Some("start the chain with a block (`if condition:` and a new line) closed by `end`, or use `a if condition else b`"),
                );
                // Read the rest of the chain anyway, so that one mistake
                // produces one message.
                let mut needs_end = false;
                while self.eat_kw(K::Else) {
                    if self.eat_kw(K::If) {
                        let condition = self.condition()?;
                        self.header_colon("else if")?;
                        needs_end |= self.at_newline();
                        branches.push((condition, self.clause_body()?));
                    } else {
                        self.header_colon("else")?;
                        needs_end |= self.at_newline();
                        else_body = Some(self.clause_body()?);
                        break;
                    }
                }
                if needs_end {
                    self.expect_end("if", header)?;
                }
            }
            return Ok(StmtKind::If(IfStmt {
                branches,
                else_body,
            }));
        }

        self.bump();
        let mut branches = vec![(condition, self.block())];
        let mut else_body = None;
        while self.eat_kw(K::Else) {
            if self.eat_kw(K::If) {
                let condition = self.condition()?;
                self.header_colon("else if")?;
                branches.push((condition, self.clause_body()?));
                continue;
            }
            self.header_colon("else")?;
            else_body = Some(self.clause_body()?);
            break;
        }
        self.expect_end("if", header)?;
        Ok(StmtKind::If(IfStmt {
            branches,
            else_body,
        }))
    }

    /// `loop condition:`: repeats while the condition holds.
    fn while_stmt(&mut self) -> PResult<StmtKind> {
        let header = self.bump().span;
        let condition = self.condition()?;
        let body = self.suite("loop", header)?;
        Ok(StmtKind::While { condition, body })
    }

    fn for_stmt(&mut self) -> PResult<StmtKind> {
        let header = self.span();
        let mode = if self.eat_kw(K::Par) {
            ForMode::Parallel
        } else if self.eat_kw(K::Async) {
            ForMode::Async
        } else {
            ForMode::Sequential
        };
        self.expect_kw(K::Each, "`each`")?;
        let pass = self.pass_mode();
        let target = self.bind_target()?;
        if !self.eat_kw(K::In) {
            return Err(self.expected_hint("`in`", "a loop is written `each item in items:`"));
        }
        let iter = self.expression()?;
        let body = self.suite("each", header)?;
        Ok(StmtKind::For(ForStmt {
            mode,
            pass,
            target,
            iter,
            body,
            item_ty: TypeSlot::Pending,
        }))
    }

    fn with_stmt(&mut self) -> PResult<StmtKind> {
        let header = self.span();
        let is_async = self.eat_kw(K::Async);
        self.expect_kw(K::Using, "`using`")?;
        let mut items = Vec::new();
        loop {
            let value = self.expression()?;
            let binding = if self.eat_kw(K::As) {
                Some(self.expect_ident("a name after `as`")?)
            } else {
                None
            };
            items.push(WithItem { value, binding });
            if !self.eat_p(P::Comma) {
                break;
            }
        }
        let body = self.suite("using", header)?;
        Ok(StmtKind::With {
            is_async,
            items,
            body,
        })
    }

    fn match_stmt(&mut self) -> PResult<StmtKind> {
        let header = self.bump().span;
        let scrutinee = self.expression()?;
        self.header_colon("when")?;
        self.expect_block_start("when")?;
        let mut arms = Vec::new();
        let mut else_body = None;
        loop {
            self.skip_newlines();
            self.docs();
            if self.at_kw(K::Is) {
                arms.push(self.match_arm()?);
            } else if self.at_kw(K::Else) {
                self.bump();
                self.header_colon("else")?;
                else_body = Some(self.clause_body()?);
                break;
            } else if self.at_kw(K::End) || self.at_eof() {
                break;
            } else if let Some(fail) = self.python_habit() {
                return Err(fail);
            } else {
                return Err(self.expected_hint(
                    "`is`, `else` or `end`",
                    "each arm of a `when` starts with `is pattern:`",
                ));
            }
        }
        self.expect_end("when", header)?;
        Ok(StmtKind::Match(MatchStmt {
            scrutinee,
            arms,
            else_body,
        }))
    }

    fn match_arm(&mut self) -> PResult<MatchArm> {
        let start = self.start();
        self.bump();
        let pattern = self.pattern()?;
        let guard = if self.eat_kw(K::If) {
            Some(self.expression()?)
        } else {
            None
        };
        self.header_colon("is")?;
        let body = self.clause_body()?;
        Ok(MatchArm {
            span: self.span_from(start),
            pattern,
            guard,
            body,
        })
    }

    fn try_stmt(&mut self) -> PResult<StmtKind> {
        let header = self.bump().span;
        self.header_colon("attempt")?;
        self.expect_block_start("attempt")?;
        let body = self.block();
        let mut catches = Vec::new();
        while self.at_kw(K::Catch) {
            let start = self.start();
            self.bump();
            let error_ty = if self.at_p(P::Colon) {
                None
            } else {
                Some(self.type_expr()?)
            };
            let binding = if self.eat_kw(K::As) {
                Some(self.expect_ident("a name after `as`")?)
            } else {
                None
            };
            self.header_colon("catch")?;
            let body = self.clause_body()?;
            catches.push(CatchClause {
                span: self.span_from(start),
                error_ty,
                binding,
                body,
            });
        }
        let finally = if self.eat_kw(K::Always) {
            self.header_colon("always")?;
            Some(self.clause_body()?)
        } else {
            None
        };
        if catches.is_empty() && finally.is_none() {
            self.error(
                "an `attempt` block needs a `catch` or an `always`",
                header,
                None,
            );
        }
        self.expect_end("attempt", header)?;
        Ok(StmtKind::Try(TryStmt {
            body,
            catches,
            finally,
        }))
    }

    fn bind_target(&mut self) -> PResult<BindTarget> {
        let start = self.start();
        let first = self.bind_name()?;
        if !self.at_p(P::Comma) {
            return Ok(first);
        }
        let mut names = vec![first];
        while self.eat_p(P::Comma) {
            names.push(self.bind_name()?);
        }
        Ok(BindTarget::Tuple(names, self.span_from(start)))
    }

    fn bind_name(&mut self) -> PResult<BindTarget> {
        match self.kind() {
            Tk::Ident(_) => Ok(BindTarget::Name(self.expect_ident("a name")?)),
            Tk::Punct(P::Underscore) => Ok(BindTarget::Discard(self.bump().span)),
            Tk::Punct(P::LParen) => {
                self.bump();
                let inner = self.bind_target()?;
                self.expect_p(P::RParen, "`)`")?;
                Ok(inner)
            }
            _ => Err(self.expected("a name to bind")),
        }
    }

    // ==== expressions ======================================================

    fn mk(&mut self, kind: ExprKind, start: Pos) -> Expr {
        let span = self.span_from(start);
        Expr::new(self.id(), kind, span)
    }

    /// `a, b, c` as a tuple, or a single expression.
    fn expression_list(&mut self) -> PResult<Expr> {
        let start = self.start();
        let first = self.expression()?;
        if !self.at_p(P::Comma) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat_p(P::Comma) {
            if self.at_list_end() {
                break;
            }
            items.push(self.expression()?);
        }
        Ok(self.mk(ExprKind::Tuple(items), start))
    }

    fn at_list_end(&self) -> bool {
        match self.kind() {
            Tk::Newline | Tk::Eof => true,
            Tk::Punct(p) => {
                matches!(p, P::Eq | P::Colon | P::RParen | P::RBracket | P::RBrace)
                    || p.is_assign_op()
            }
            _ => false,
        }
    }

    fn expression(&mut self) -> PResult<Expr> {
        let first = usize::from(self.at_kw(K::Async));
        if self.lambda_at(first) {
            self.lambda()
        } else {
            self.conditional()
        }
    }

    /// Whether a lambda starts `offset` tokens ahead: `x => ...` or
    /// `(params) => ...`, found by looking past the matching `)`.
    fn lambda_at(&self, offset: usize) -> bool {
        match self.kind_at(offset) {
            Tk::Ident(_) => self.kind_at(offset + 1).is_punct(P::FatArrow),
            Tk::Punct(P::LParen) => {
                let mut depth = 0usize;
                let mut n = offset;
                loop {
                    match self.kind_at(n) {
                        Tk::Punct(P::LParen | P::LBracket | P::LBrace) => depth += 1,
                        Tk::Punct(P::RParen | P::RBracket | P::RBrace) => {
                            depth -= 1;
                            if depth == 0 {
                                return self.kind_at(n + 1).is_punct(P::FatArrow);
                            }
                        }
                        Tk::Eof => return false,
                        _ => {}
                    }
                    n += 1;
                }
            }
            _ => false,
        }
    }

    fn lambda(&mut self) -> PResult<Expr> {
        let start = self.start();
        let is_async = self.eat_kw(K::Async);
        let params = if self.eat_p(P::LParen) {
            self.params()?
        } else {
            vec![self.param()?]
        };
        if !self.eat_p(P::FatArrow) {
            return Err(self.expected_hint(
                "`=>` after the parameters",
                "a lambda is written `(x) => x * 2`",
            ));
        }
        let body = self.expression()?;
        Ok(self.mk(
            ExprKind::Lambda {
                is_async,
                params,
                body: Box::new(body),
            },
            start,
        ))
    }

    fn conditional(&mut self) -> PResult<Expr> {
        let start = self.start();
        let value = self.pipeline()?;
        if !self.eat_kw(K::If) {
            return Ok(value);
        }
        let condition = self.pipeline()?;
        if !self.eat_kw(K::Else) {
            return Err(self.expected_hint(
                "`else`",
                "a conditional expression is written `a if condition else b`",
            ));
        }
        let else_value = self.expression()?;
        Ok(self.mk(
            ExprKind::If {
                then_value: Box::new(value),
                condition: Box::new(condition),
                else_value: Box::new(else_value),
            },
            start,
        ))
    }

    fn pipeline(&mut self) -> PResult<Expr> {
        let start = self.start();
        let mut lhs = self.placement()?;
        while self.eat_p(P::PipeGt) {
            let rhs = self.placement()?;
            lhs = self.mk(
                ExprKind::Pipeline {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                start,
            );
        }
        Ok(lhs)
    }

    fn placement(&mut self) -> PResult<Expr> {
        let start = self.start();
        let value = self.coalesce()?;
        if !self.eat_kw(K::On) {
            return Ok(value);
        }
        let target = self.coalesce()?;
        Ok(self.mk(
            ExprKind::Placement {
                value: Box::new(value),
                target: Box::new(target),
                place: PlaceSlot::Pending,
            },
            start,
        ))
    }

    fn coalesce(&mut self) -> PResult<Expr> {
        let start = self.start();
        let mut lhs = self.or_expr()?;
        while self.eat_p(P::QuestionQuestion) {
            let rhs = self.or_expr()?;
            lhs = self.mk(
                ExprKind::Coalesce {
                    lhs: Box::new(lhs),
                    rhs: Box::new(rhs),
                },
                start,
            );
        }
        Ok(lhs)
    }

    fn or_expr(&mut self) -> PResult<Expr> {
        let start = self.start();
        let mut lhs = self.and_expr()?;
        while self.eat_kw(K::Or) {
            let rhs = self.and_expr()?;
            lhs = self.binary(BinaryOp::Or, lhs, rhs, start);
        }
        Ok(lhs)
    }

    fn and_expr(&mut self) -> PResult<Expr> {
        let start = self.start();
        let mut lhs = self.not_expr()?;
        while self.eat_kw(K::And) {
            let rhs = self.not_expr()?;
            lhs = self.binary(BinaryOp::And, lhs, rhs, start);
        }
        Ok(lhs)
    }

    fn not_expr(&mut self) -> PResult<Expr> {
        let start = self.start();
        if self.eat_kw(K::Not) {
            let operand = self.not_expr()?;
            return Ok(self.mk(
                ExprKind::Unary {
                    op: UnaryOp::Not,
                    operand: Box::new(operand),
                },
                start,
            ));
        }
        self.comparison()
    }

    fn comparison(&mut self) -> PResult<Expr> {
        let start = self.start();
        let first = self.range_expr()?;
        let mut operands = vec![first];
        let mut ops = Vec::new();
        loop {
            let op = match self.kind() {
                Tk::Punct(P::EqEq) => CompareOp::Eq,
                Tk::Punct(P::NotEq) => CompareOp::NotEq,
                Tk::Punct(P::Lt) => CompareOp::Lt,
                Tk::Punct(P::LtEq) => CompareOp::LtEq,
                Tk::Punct(P::Gt) => CompareOp::Gt,
                Tk::Punct(P::GtEq) => CompareOp::GtEq,
                Tk::Keyword(K::In) => CompareOp::In,
                Tk::Keyword(K::Not) if self.kind_at(1).is_keyword(K::In) => {
                    self.bump();
                    CompareOp::NotIn
                }
                _ => break,
            };
            self.bump();
            ops.push(op);
            operands.push(self.range_expr()?);
        }
        if ops.is_empty() {
            return Ok(operands.remove(0));
        }
        Ok(self.mk(ExprKind::Compare { operands, ops }, start))
    }

    fn range_expr(&mut self) -> PResult<Expr> {
        let start = self.start();
        let lo = self.bit_or()?;
        let inclusive = if self.at_kw(K::To) {
            true
        } else if self.at_kw(K::Until) {
            false
        } else {
            return Ok(lo);
        };
        self.bump();
        let hi = self.bit_or()?;
        let step = if self.eat_kw(K::By) {
            Some(Box::new(self.bit_or()?))
        } else {
            None
        };
        Ok(self.mk(
            ExprKind::Range {
                start: Box::new(lo),
                end: Box::new(hi),
                inclusive,
                step,
            },
            start,
        ))
    }

    fn binary(&mut self, op: BinaryOp, lhs: Expr, rhs: Expr, start: Pos) -> Expr {
        self.mk(
            ExprKind::Binary {
                op,
                lhs: Box::new(lhs),
                rhs: Box::new(rhs),
            },
            start,
        )
    }

    /// One left-associative level of binary operators.
    fn binary_left(
        &mut self,
        next: fn(&mut Parser) -> PResult<Expr>,
        table: &[(P, BinaryOp)],
    ) -> PResult<Expr> {
        let start = self.start();
        let mut lhs = next(self)?;
        while let Tk::Punct(punct) = self.kind() {
            let Some(&(_, op)) = table.iter().find(|(p, _)| p == punct) else {
                break;
            };
            self.bump();
            let rhs = next(self)?;
            lhs = self.binary(op, lhs, rhs, start);
        }
        Ok(lhs)
    }

    fn bit_or(&mut self) -> PResult<Expr> {
        self.binary_left(Parser::bit_xor, &[(P::Pipe, BinaryOp::BitOr)])
    }

    fn bit_xor(&mut self) -> PResult<Expr> {
        self.binary_left(Parser::bit_and, &[(P::Caret, BinaryOp::BitXor)])
    }

    fn bit_and(&mut self) -> PResult<Expr> {
        self.binary_left(Parser::shift, &[(P::Amp, BinaryOp::BitAnd)])
    }

    fn shift(&mut self) -> PResult<Expr> {
        self.binary_left(
            Parser::sum,
            &[(P::Shl, BinaryOp::Shl), (P::Shr, BinaryOp::Shr)],
        )
    }

    fn sum(&mut self) -> PResult<Expr> {
        self.binary_left(
            Parser::term,
            &[(P::Plus, BinaryOp::Add), (P::Minus, BinaryOp::Sub)],
        )
    }

    /// `*` `/` `//` `%` and `@`. All share one precedence level, as in
    /// Python; `@` builds its own node.
    fn term(&mut self) -> PResult<Expr> {
        let start = self.start();
        let mut lhs = self.unary()?;
        loop {
            let op = match self.kind() {
                Tk::Punct(P::Star) => Some(BinaryOp::Mul),
                Tk::Punct(P::Slash) => Some(BinaryOp::Div),
                Tk::Punct(P::SlashSlash) => Some(BinaryOp::FloorDiv),
                Tk::Punct(P::Percent) => Some(BinaryOp::Rem),
                Tk::Punct(P::At) => None,
                _ => break,
            };
            self.bump();
            let rhs = self.unary()?;
            lhs = match op {
                Some(op) => self.binary(op, lhs, rhs, start),
                None => self.mk(
                    ExprKind::MatMul {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    },
                    start,
                ),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> PResult<Expr> {
        let start = self.start();
        let prefix = match self.kind() {
            Tk::Punct(P::Minus) => Prefix::Unary(UnaryOp::Neg),
            Tk::Punct(P::Plus) => Prefix::Unary(UnaryOp::Pos),
            Tk::Punct(P::Tilde) => Prefix::Unary(UnaryOp::BitNot),
            Tk::Keyword(K::Await) => Prefix::Await,
            Tk::Keyword(K::Spawn) => Prefix::Spawn,
            Tk::Keyword(K::Move) => Prefix::Move,
            _ => return self.power(),
        };
        self.bump();
        let operand = Box::new(self.unary()?);
        let kind = match prefix {
            Prefix::Unary(op) => ExprKind::Unary { op, operand },
            Prefix::Await => ExprKind::Await(operand),
            Prefix::Spawn => ExprKind::Spawn(operand),
            Prefix::Move => ExprKind::Move(operand),
        };
        Ok(self.mk(kind, start))
    }

    fn power(&mut self) -> PResult<Expr> {
        let start = self.start();
        let base = self.postfix()?;
        if !self.eat_p(P::StarStar) {
            return Ok(base);
        }
        let exponent = self.unary()?;
        Ok(self.binary(BinaryOp::Pow, base, exponent, start))
    }

    fn postfix(&mut self) -> PResult<Expr> {
        let start = self.start();
        let mut expr = self.atom()?;
        loop {
            match self.kind() {
                Tk::Punct(P::Dot | P::QuestionDot) => {
                    let optional = self.bump().kind.is_punct(P::QuestionDot);
                    let name = self.expect_ident("a field or method name")?;
                    expr = self.mk(
                        ExprKind::Field {
                            base: Box::new(expr),
                            name,
                            optional,
                        },
                        start,
                    );
                }
                Tk::Punct(P::LParen) => {
                    self.bump();
                    let args = self.arguments(P::RParen)?;
                    expr = self.mk(
                        ExprKind::Call {
                            callee: Box::new(expr),
                            args,
                        },
                        start,
                    );
                }
                Tk::Punct(P::LBracket) => {
                    self.bump();
                    let subscripts = self.subscripts()?;
                    expr = self.mk(
                        ExprKind::Index {
                            base: Box::new(expr),
                            subscripts,
                        },
                        start,
                    );
                }
                _ => break,
            }
        }
        Ok(expr)
    }

    /// Arguments up to and including `close`; the opening bracket is
    /// consumed.
    fn arguments(&mut self, close: P) -> PResult<Vec<Arg>> {
        let mut args = Vec::new();
        loop {
            if self.eat_p(close) {
                break;
            }
            args.push(self.argument()?);
            if !self.eat_p(P::Comma) {
                self.expect_p(close, "`,` or `)` in the argument list")?;
                break;
            }
        }
        Ok(args)
    }

    fn argument(&mut self) -> PResult<Arg> {
        let start = self.start();
        // `name = value`. Any word can name an argument there, keywords
        // included (`mail.send(to = ...)`), since `=` follows it.
        let word = match self.kind() {
            Tk::Ident(n) => Some(n.clone()),
            Tk::Keyword(k) => Some(k.as_str().to_string()),
            _ => None,
        };
        if let (Some(word), true) = (word, self.kind_at(1).is_punct(P::Eq)) {
            let span = self.bump().span;
            let name = Ident { name: word, span };
            self.bump();
            let value = self.expression()?;
            return Ok(Arg {
                span: self.span_from(start),
                name: Some(name),
                is_mut: false,
                value,
            });
        }
        let is_mut = self.eat_kw(K::Mut);
        let value = self.expression()?;
        Ok(Arg {
            span: self.span_from(start),
            name: None,
            is_mut,
            value,
        })
    }

    fn subscripts(&mut self) -> PResult<Vec<Subscript>> {
        if self.at_p(P::RBracket) {
            return Err(self.expected("an index"));
        }
        let mut subscripts = Vec::new();
        loop {
            if self.eat_p(P::RBracket) {
                break;
            }
            subscripts.push(self.subscript()?);
            if !self.eat_p(P::Comma) {
                self.expect_p(P::RBracket, "`,` or `]`")?;
                break;
            }
        }
        Ok(subscripts)
    }

    fn at_slice_part_end(&self) -> bool {
        matches!(self.kind(), Tk::Punct(P::Colon | P::Comma | P::RBracket))
    }

    fn subscript(&mut self) -> PResult<Subscript> {
        let start = self.start();
        if self.eat_p(P::Ellipsis) {
            return Ok(Subscript::Ellipsis(self.span_from(start)));
        }
        let first = if self.at_p(P::Colon) {
            None
        } else {
            let index = self.expression()?;
            if !self.at_p(P::Colon) {
                return Ok(Subscript::Index(index));
            }
            Some(Box::new(index))
        };
        self.bump();
        let stop = if self.at_slice_part_end() {
            None
        } else {
            Some(Box::new(self.expression()?))
        };
        let step = if self.eat_p(P::Colon) && !self.at_slice_part_end() {
            Some(Box::new(self.expression()?))
        } else {
            None
        };
        Ok(Subscript::Slice {
            start: first,
            stop,
            step,
            span: self.span_from(start),
        })
    }

    fn atom(&mut self) -> PResult<Expr> {
        let start = self.start();
        let kind = match self.kind().clone() {
            Tk::Int { value, suffix } => {
                self.bump();
                ExprKind::Int { value, suffix }
            }
            Tk::Float { value, suffix } => {
                self.bump();
                ExprKind::Float { value, suffix }
            }
            Tk::Str(parts) => {
                self.bump();
                ExprKind::Str(self.str_segments(&parts))
            }
            Tk::Keyword(K::True) => {
                self.bump();
                ExprKind::Bool(true)
            }
            Tk::Keyword(K::False) => {
                self.bump();
                ExprKind::Bool(false)
            }
            Tk::Keyword(K::None) => {
                self.bump();
                ExprKind::None
            }
            // `any` is a type, but `any(xs)` calls the Prelude function.
            Tk::Keyword(K::Any) if self.kind_at(1).is_punct(P::LParen) => {
                let span = self.bump().span;
                ExprKind::Name(Ident {
                    name: "any".to_string(),
                    span,
                })
            }
            Tk::Ident(name) => {
                let span = self.bump().span;
                let python = match name.as_str() {
                    "True" => Some(("true", ExprKind::Bool(true))),
                    "False" => Some(("false", ExprKind::Bool(false))),
                    "None" => Some(("none", ExprKind::None)),
                    _ => None,
                };
                match python {
                    Some((spelling, kind)) => {
                        self.error(
                            format!("`{name}` is written `{spelling}` in Vyrn"),
                            span,
                            None,
                        );
                        kind
                    }
                    None => ExprKind::Name(Ident { name, span }),
                }
            }
            // `tensor` names a type, and in an expression it is the builtin
            // that builds one: `tensor([[1, 2], [3, 4]])`.
            Tk::Keyword(K::Tensor) => {
                let span = self.bump().span;
                ExprKind::Name(Ident {
                    name: "tensor".to_string(),
                    span,
                })
            }
            Tk::Punct(P::LParen) => return self.paren(),
            Tk::Punct(P::LBracket) => return self.list(),
            Tk::Punct(P::LBrace) => return self.map(),
            _ => return Err(self.expected("an expression")),
        };
        Ok(self.mk(kind, start))
    }

    fn paren(&mut self) -> PResult<Expr> {
        let start = self.start();
        self.bump();
        if self.eat_p(P::RParen) {
            return Ok(self.mk(ExprKind::Tuple(Vec::new()), start));
        }
        let first = self.expression()?;
        if self.eat_p(P::RParen) {
            return Ok(first);
        }
        let mut items = vec![first];
        while self.eat_p(P::Comma) {
            if self.at_p(P::RParen) {
                break;
            }
            items.push(self.expression()?);
        }
        self.expect_p(P::RParen, "`,` or `)`")?;
        Ok(self.mk(ExprKind::Tuple(items), start))
    }

    fn list(&mut self) -> PResult<Expr> {
        let start = self.start();
        self.bump();
        if self.eat_p(P::RBracket) {
            return Ok(self.mk(ExprKind::List(Vec::new()), start));
        }
        let first = self.expression()?;
        if self.at_kw(K::Each) {
            let clauses = self.comp_clauses()?;
            self.expect_p(P::RBracket, "`]` to close the comprehension")?;
            return Ok(self.mk(
                ExprKind::ListComp {
                    element: Box::new(first),
                    clauses,
                },
                start,
            ));
        }
        let mut items = vec![first];
        while self.eat_p(P::Comma) {
            if self.at_p(P::RBracket) {
                break;
            }
            items.push(self.expression()?);
        }
        self.expect_p(P::RBracket, "`,` or `]`")?;
        Ok(self.mk(ExprKind::List(items), start))
    }

    /// The rest of a set literal, after its first item: a call to the
    /// Prelude's `__set` with a list.
    fn set_literal(&mut self, start: Pos, first: Expr) -> PResult<Expr> {
        let list = if self.at_kw(K::Each) {
            let clauses = self.comp_clauses()?;
            self.expect_p(P::RBrace, "`}` to close the set comprehension")?;
            self.mk(
                ExprKind::ListComp {
                    element: Box::new(first),
                    clauses,
                },
                start,
            )
        } else {
            let mut items = vec![first];
            while self.eat_p(P::Comma) {
                if self.at_p(P::RBrace) {
                    break;
                }
                items.push(self.expression()?);
            }
            self.expect_p(P::RBrace, "`,` or `}` in the set")?;
            self.mk(ExprKind::List(items), start)
        };
        let span = list.span;
        let callee = Expr::new(
            self.id(),
            ExprKind::Name(Ident {
                name: "__set".to_string(),
                span,
            }),
            span,
        );
        let args = vec![Arg {
            span,
            name: None,
            is_mut: false,
            value: list,
        }];
        Ok(self.mk(
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            start,
        ))
    }

    fn map(&mut self) -> PResult<Expr> {
        let start = self.start();
        self.bump();
        if self.eat_p(P::RBrace) {
            return Ok(self.mk(ExprKind::Map(Vec::new()), start));
        }
        let key = self.expression()?;
        // `{1, 2, 3}` and `{x each x in xs}` are sets.
        if self.at_p(P::Comma) || self.at_p(P::RBrace) || self.at_kw(K::Each) {
            return self.set_literal(start, key);
        }
        self.expect_p(P::Colon, "`:` between a key and its value")?;
        let value = self.expression()?;
        if self.at_kw(K::Each) {
            let clauses = self.comp_clauses()?;
            self.expect_p(P::RBrace, "`}` to close the comprehension")?;
            return Ok(self.mk(
                ExprKind::MapComp {
                    key: Box::new(key),
                    value: Box::new(value),
                    clauses,
                },
                start,
            ));
        }
        let mut entries = vec![(key, value)];
        while self.eat_p(P::Comma) {
            if self.at_p(P::RBrace) {
                break;
            }
            let key = self.expression()?;
            self.expect_p(P::Colon, "`:` between a key and its value")?;
            let value = self.expression()?;
            entries.push((key, value));
        }
        self.expect_p(P::RBrace, "`,` or `}`")?;
        Ok(self.mk(ExprKind::Map(entries), start))
    }

    fn comp_clauses(&mut self) -> PResult<Vec<CompClause>> {
        let mut clauses = Vec::new();
        while self.eat_kw(K::Each) {
            let target = self.bind_target()?;
            self.expect_kw(K::In, "`in`")?;
            let iter = self.pipeline()?;
            clauses.push(CompClause::For { target, iter });
            while self.eat_kw(K::If) {
                clauses.push(CompClause::If(self.pipeline()?));
            }
        }
        Ok(clauses)
    }

    fn str_segments(&mut self, parts: &[StrPart]) -> Vec<StrSegment> {
        let mut segments = Vec::with_capacity(parts.len());
        for part in parts {
            match part {
                StrPart::Text(text) => segments.push(StrSegment::Text(text.clone())),
                StrPart::Interp { source, span } => {
                    if let Some(expr) = self.interpolation(source, *span) {
                        segments.push(StrSegment::Interp(expr));
                    }
                }
            }
        }
        segments
    }

    /// Parses the expression inside `{...}` in a string. Positions stay
    /// relative to the whole file, and node ids continue from this parser.
    fn interpolation(&mut self, source: &str, span: Span) -> Option<Expr> {
        // `"{{name}}"`: template syntax in an ordinary string.
        if source.trim_start().starts_with('{') {
            self.error(
                "`{{` inside a string starts an interpolation",
                span,
                Some("for template text such as `{{name}}`, use a raw string `r\"...\"` or a template file; for a literal brace write `\\{`"),
            );
            return None;
        }
        // `{price:.2f}`: a format spec after a `:` outside brackets.
        let (source, spec) = split_format_spec(source);
        let expr = self.interpolation_expr(source, span)?;
        let Some(spec) = spec else {
            return Some(expr);
        };
        let at = expr.span;
        let callee = Expr::new(
            self.id(),
            ExprKind::Name(Ident {
                name: "__format".to_string(),
                span: at,
            }),
            at,
        );
        let spec = Expr::new(
            self.id(),
            ExprKind::Str(vec![StrSegment::Text(spec.to_string())]),
            at,
        );
        let arg = |value: Expr| Arg {
            span: value.span,
            name: None,
            is_mut: false,
            value,
        };
        let args = vec![arg(expr), arg(spec)];
        Some(Expr::new(
            self.id(),
            ExprKind::Call {
                callee: Box::new(callee),
                args,
            },
            at,
        ))
    }

    fn interpolation_expr(&mut self, source: &str, span: Span) -> Option<Expr> {
        let (tokens, lex_errors) = Lexer::new_at(source, span.start).run();
        if !lex_errors.is_empty() {
            self.diagnostics
                .extend(lex_errors.iter().map(Diagnostic::from));
            return None;
        }
        let mut inner = Parser::new(tokens);
        inner.next_id = self.next_id;
        let result = inner.expression();
        let complete = result.is_ok() && (inner.at_newline() || inner.at_eof());
        if result.is_ok() && !complete {
            inner.expected_hint(
                "the end of the `{...}` expression",
                "a `{...}` inside a string holds one expression; write `\\{` for a literal brace",
            );
        }
        self.next_id = inner.next_id;
        self.diagnostics.append(&mut inner.diagnostics);
        result.ok().filter(|_| complete)
    }

    // ==== types ============================================================

    fn mk_ty(&mut self, kind: TypeExprKind, start: Pos) -> TypeExpr {
        let span = self.span_from(start);
        TypeExpr {
            id: self.id(),
            span,
            kind,
            resolved: TypeSlot::Pending,
        }
    }

    fn type_expr(&mut self) -> PResult<TypeExpr> {
        let start = self.start();
        let inner = self.union_type()?;
        if !self.eat_kw(K::On) {
            return Ok(inner);
        }
        let target = self.coalesce()?;
        Ok(self.mk_ty(
            TypeExprKind::Placed {
                inner: Box::new(inner),
                target: Box::new(target),
                place: PlaceSlot::Pending,
            },
            start,
        ))
    }

    fn union_type(&mut self) -> PResult<TypeExpr> {
        let start = self.start();
        let first = self.optional_type()?;
        if !self.at_p(P::Pipe) {
            return Ok(first);
        }
        let mut members = vec![first];
        while self.eat_p(P::Pipe) {
            members.push(self.optional_type()?);
        }
        Ok(self.mk_ty(TypeExprKind::Union(members), start))
    }

    fn optional_type(&mut self) -> PResult<TypeExpr> {
        let start = self.start();
        let mut ty = self.primary_type()?;
        while self.eat_p(P::Question) {
            ty = self.mk_ty(TypeExprKind::Optional(Box::new(ty)), start);
        }
        Ok(ty)
    }

    fn primary_type(&mut self) -> PResult<TypeExpr> {
        let start = self.start();
        let kind = match self.kind() {
            Tk::Keyword(K::Any) => {
                self.bump();
                TypeExprKind::Any
            }
            Tk::Keyword(K::Tensor) => self.tensor_type()?,
            Tk::Punct(P::LBracket) => {
                self.bump();
                let element = Box::new(self.type_expr()?);
                if self.eat_p(P::Semicolon) {
                    let len = Box::new(self.expression()?);
                    self.expect_p(P::RBracket, "`]` after the array length")?;
                    TypeExprKind::Array { element, len }
                } else {
                    self.expect_p(P::RBracket, "`]`, or `;` and a length")?;
                    TypeExprKind::List(element)
                }
            }
            Tk::Punct(P::LBrace) => {
                self.bump();
                let key = Box::new(self.type_expr()?);
                self.expect_p(P::Colon, "`:` between the key and value types")?;
                let value = Box::new(self.type_expr()?);
                self.expect_p(P::RBrace, "`}`")?;
                TypeExprKind::Map { key, value }
            }
            // `(int, str)` is a tuple; `(int) -> str` is a function.
            Tk::Punct(P::LParen) => {
                self.bump();
                let items = self.type_list(P::RParen)?;
                if self.eat_p(P::Arrow) {
                    let returns = Some(Box::new(self.type_expr()?));
                    TypeExprKind::Fn {
                        params: items,
                        returns,
                    }
                } else {
                    TypeExprKind::Tuple(items)
                }
            }
            Tk::Keyword(K::Ref | K::Shared | K::Weak | K::Ptr) => {
                let ownership = match self.bump().kind {
                    Tk::Keyword(K::Ref) if self.eat_kw(K::Mut) => Ownership::RefMut,
                    Tk::Keyword(K::Ref) => Ownership::Ref,
                    Tk::Keyword(K::Shared) => Ownership::Shared,
                    Tk::Keyword(K::Weak) => Ownership::Weak,
                    _ => Ownership::Ptr,
                };
                let inner = Box::new(self.primary_type()?);
                TypeExprKind::Owned { ownership, inner }
            }
            Tk::Ident(_) => {
                let path = self.dotted_name("a type name")?;
                let args = if self.eat_p(P::LBracket) {
                    self.generic_args()?
                } else {
                    Vec::new()
                };
                TypeExprKind::Named { path, args }
            }
            _ => return Err(self.expected("a type")),
        };
        Ok(self.mk_ty(kind, start))
    }

    fn generic_args(&mut self) -> PResult<Vec<GenericArg>> {
        let mut args = Vec::new();
        loop {
            if self.eat_p(P::RBracket) {
                break;
            }
            let arg = if let Tk::Int { value, .. } = self.kind() {
                let value = *value;
                GenericArg::Int(value, self.bump().span)
            } else {
                GenericArg::Type(self.type_expr()?)
            };
            args.push(arg);
            if !self.eat_p(P::Comma) {
                self.expect_p(P::RBracket, "`,` or `]`")?;
                break;
            }
        }
        Ok(args)
    }

    fn tensor_type(&mut self) -> PResult<TypeExprKind> {
        self.bump();
        self.expect_p(P::LBracket, "`[` after `tensor`")?;
        let dtype = Box::new(self.type_expr()?);
        let mut dims = Vec::new();
        if self.eat_p(P::Semicolon) {
            loop {
                let start = self.start();
                let dim = if self.eat_p(P::Underscore) {
                    Dim::Unchecked(self.span_from(start))
                } else if self.eat_p(P::Ellipsis) {
                    Dim::Variadic(self.span_from(start))
                } else {
                    Dim::Expr(self.sum()?)
                };
                dims.push(dim);
                if !self.eat_p(P::Comma) {
                    break;
                }
            }
        }
        self.expect_p(P::RBracket, "`]` to close the tensor type")?;
        Ok(TypeExprKind::Tensor(TensorType { dtype, dims }))
    }

    // ==== patterns =========================================================

    fn mk_pat(&mut self, kind: PatternKind, start: Pos) -> Pattern {
        let span = self.span_from(start);
        Pattern {
            id: self.id(),
            span,
            kind,
            ty: TypeSlot::Pending,
        }
    }

    fn pattern(&mut self) -> PResult<Pattern> {
        let start = self.start();
        let first = self.primary_pattern()?;
        let pattern = if self.at_kw(K::Or) {
            let mut alternatives = vec![first];
            while self.eat_kw(K::Or) {
                alternatives.push(self.primary_pattern()?);
            }
            self.mk_pat(PatternKind::Or(alternatives), start)
        } else {
            first
        };
        if self.eat_kw(K::As) {
            let name = self.expect_ident("a name after `as`")?;
            return Ok(self.mk_pat(
                PatternKind::As {
                    pattern: Box::new(pattern),
                    name,
                },
                start,
            ));
        }
        Ok(pattern)
    }

    fn primary_pattern(&mut self) -> PResult<Pattern> {
        let start = self.start();
        let kind = match self.kind() {
            Tk::Punct(P::Underscore) => {
                self.bump();
                PatternKind::Wildcard
            }
            Tk::Punct(P::Minus)
            | Tk::Int { .. }
            | Tk::Float { .. }
            | Tk::Str(_)
            | Tk::Keyword(K::True | K::False | K::None) => {
                let low = self.literal_pattern()?;
                let inclusive = if self.at_kw(K::To) {
                    true
                } else if self.at_kw(K::Until) {
                    false
                } else {
                    return Ok(self.mk_pat(PatternKind::Literal(low), start));
                };
                self.bump();
                let high = self.literal_pattern()?;
                PatternKind::Range {
                    start: low,
                    end: high,
                    inclusive,
                }
            }
            Tk::Ident(_) => {
                let path = self.dotted_name("a pattern")?;
                if self.eat_p(P::LParen) {
                    let mut args = Vec::new();
                    loop {
                        if self.eat_p(P::RParen) {
                            break;
                        }
                        let name = if matches!(self.kind(), Tk::Ident(_))
                            && self.kind_at(1).is_punct(P::Eq)
                        {
                            let name = self.expect_ident("a field name")?;
                            self.bump();
                            Some(name)
                        } else {
                            None
                        };
                        let pattern = self.pattern()?;
                        args.push(PatternArg { name, pattern });
                        if !self.eat_p(P::Comma) {
                            self.expect_p(P::RParen, "`,` or `)`")?;
                            break;
                        }
                    }
                    PatternKind::Destructure { path, args }
                } else if path.segments.len() == 1 {
                    let mut segments = path.segments;
                    PatternKind::Binding(segments.remove(0))
                } else {
                    PatternKind::Value(path)
                }
            }
            Tk::Punct(P::LParen) => {
                self.bump();
                PatternKind::Tuple(self.seq_patterns(P::RParen)?)
            }
            Tk::Punct(P::LBracket) => {
                self.bump();
                PatternKind::List(self.seq_patterns(P::RBracket)?)
            }
            _ => return Err(self.expected("a pattern")),
        };
        Ok(self.mk_pat(kind, start))
    }

    fn literal_pattern(&mut self) -> PResult<LiteralPattern> {
        let negative = self.eat_p(P::Minus);
        let literal = match self.kind().clone() {
            Tk::Int { value, suffix } => LiteralPattern::Int {
                negative,
                value,
                suffix,
            },
            Tk::Float { value, suffix } => LiteralPattern::Float {
                negative,
                value,
                suffix,
            },
            Tk::Str(parts) if !negative => {
                if parts.iter().any(|p| matches!(p, StrPart::Interp { .. })) {
                    let span = self.span();
                    self.error(
                        "a string pattern cannot contain `{...}`",
                        span,
                        Some("write `\\{` for a literal brace"),
                    );
                }
                LiteralPattern::Str(plain_text(&parts))
            }
            Tk::Keyword(K::True) if !negative => LiteralPattern::Bool(true),
            Tk::Keyword(K::False) if !negative => LiteralPattern::Bool(false),
            Tk::Keyword(K::None) if !negative => LiteralPattern::None,
            _ => {
                return Err(self.expected(if negative {
                    "a number after `-`"
                } else {
                    "a literal"
                }))
            }
        };
        self.bump();
        Ok(literal)
    }

    fn seq_patterns(&mut self, close: P) -> PResult<Vec<SeqPattern>> {
        let mut items = Vec::new();
        loop {
            if self.eat_p(close) {
                break;
            }
            let start = self.start();
            let item = if self.eat_p(P::Ellipsis) {
                let name = if matches!(self.kind(), Tk::Ident(_)) {
                    Some(self.expect_ident("a name")?)
                } else {
                    None
                };
                SeqPattern::Rest(name, self.span_from(start))
            } else {
                SeqPattern::Item(self.pattern()?)
            };
            items.push(item);
            if !self.eat_p(P::Comma) {
                self.expect_p(close, &format!("`,` or `{close}`"))?;
                break;
            }
        }
        Ok(items)
    }
}

// ==== helpers ==============================================================

/// Python's words that Vyrn spells differently: the Vyrn word, and an
/// example for the error message.
/// Splits `price:.2f` into the expression and the format spec. A `:`
/// inside brackets or quotes (`m["a:b"]`, `xs[1:3]`) is not a spec.
fn split_format_spec(source: &str) -> (&str, Option<&str>) {
    let mut depth = 0i32;
    let mut quote: Option<char> = None;
    let mut escaped = false;
    for (i, c) in source.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => quote = Some(c),
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth -= 1,
            ':' if depth == 0 => return (&source[..i], Some(&source[i + 1..])),
            _ => {}
        }
    }
    (source, None)
}

fn python_replacement(word: &str) -> Option<(&'static str, &'static str)> {
    Some(match word {
        "def" | "fn" => (
            "create",
            "declare a function with `create name(args):` and close it with `end`",
        ),
        "elif" => ("else if", "write `else if condition:`"),
        "while" => (
            "loop",
            "write `loop condition:`; plain `loop:` repeats until `stop`",
        ),
        "for" => ("each", "write `each item in items:`"),
        "match" => ("when", "write `when value:` with `is pattern:` arms"),
        "with" => ("using", "write `using resource as name:`"),
        "try" => (
            "attempt",
            "write `attempt:`, then `catch Error as e:` and `always:`, closed by `end`",
        ),
        "except" => ("catch", "write `catch Error as e:`"),
        "finally" => ("always", "write `always:`"),
        "class" => (
            "object",
            "write `object Name:` (or `struct Name:` for a value that is copied)",
        ),
        "import" | "from" => ("use", "write `use module` or `use module (name, other)`"),
        "raise" => ("fail", "write `fail error`"),
        "break" => ("stop", "write `stop` to leave a loop"),
        "continue" => ("skip", "write `skip` to go to the next round of a loop"),
        "lambda" => ("=>", "write `(x) => x * 2`"),
        "pass" => ("", "an empty block is just its header and `end`"),
        _ => return None,
    })
}

/// Tokens that end a block: `end`, the clause keywords of a chain, and the
/// end of the file.
fn is_closer(kind: &Tk) -> bool {
    matches!(
        kind,
        Tk::Eof | Tk::Keyword(K::End | K::Else | K::Is | K::Catch | K::Always)
    )
}

/// The text of a string token, with interpolations kept as `{source}`.
fn plain_text(parts: &[StrPart]) -> String {
    parts
        .iter()
        .map(|part| match part {
            StrPart::Text(text) => text.clone(),
            StrPart::Interp { source, .. } => format!("{{{source}}}"),
        })
        .collect()
}

fn assign_op(punct: P) -> Option<AssignOp> {
    Some(match punct {
        P::Eq => AssignOp::Assign,
        P::PlusEq => AssignOp::Add,
        P::MinusEq => AssignOp::Sub,
        P::StarEq => AssignOp::Mul,
        P::SlashEq => AssignOp::Div,
        P::SlashSlashEq => AssignOp::FloorDiv,
        P::PercentEq => AssignOp::Rem,
        P::AtEq => AssignOp::MatMul,
        P::StarStarEq => AssignOp::Pow,
        P::AmpEq => AssignOp::BitAnd,
        P::PipeEq => AssignOp::BitOr,
        P::CaretEq => AssignOp::BitXor,
        P::ShlEq => AssignOp::Shl,
        P::ShrEq => AssignOp::Shr,
        P::QuestionQuestionEq => AssignOp::Coalesce,
        _ => return None,
    })
}

/// Whether `expr` can appear on the left of an assignment.
fn assignable(expr: &Expr, allow_tuple: bool) -> bool {
    match &expr.kind {
        ExprKind::Name(_) | ExprKind::Index { .. } => true,
        ExprKind::Field { optional, .. } => !optional,
        ExprKind::Tuple(items) if allow_tuple => {
            !items.is_empty() && items.iter().all(|item| assignable(item, false))
        }
        _ => false,
    }
}

/// How a line changes block depth, for error recovery: `+1` if it opens a
/// block, `-1` if it is an `end`, `0` otherwise. A header that is missing
/// its `:` still counts as opening a block, so that its body is skipped
/// along with it.
fn line_effect(line: &[Token]) -> i32 {
    let kinds: Vec<&Tk> = line
        .iter()
        .map(|t| &t.kind)
        .filter(|k| !matches!(k, Tk::DocComment(_)))
        .collect();
    let Some(first) = kinds.first() else {
        return 0;
    };
    match first {
        Tk::Keyword(K::End) => return -1,
        Tk::Keyword(K::Else | K::Is | K::Catch | K::Always) => return 0,
        _ => {}
    }
    if kinds.last().is_some_and(|k| k.is_punct(P::Colon)) {
        return 1;
    }
    let mut depth = 0;
    for kind in &kinds {
        match kind {
            Tk::Punct(P::LParen | P::LBracket | P::LBrace) => depth += 1,
            Tk::Punct(P::RParen | P::RBracket | P::RBrace) => depth -= 1,
            // A colon outside brackets with more after it: a one-line body.
            Tk::Punct(P::Colon) if depth == 0 => return 0,
            _ => {}
        }
    }
    let second = kinds.get(1).copied();
    let opens = match first {
        Tk::Keyword(
            K::If
            | K::Each
            | K::Par
            | K::Loop
            | K::When
            | K::Using
            | K::On
            | K::Region
            | K::Unsafe
            | K::Main
            | K::Struct
            | K::Object
            | K::Enum
            | K::Trait
            | K::Impl
            | K::Routes
            | K::Route
            | K::Strict
            | K::Kernel
            | K::Export,
        ) => true,
        Tk::Keyword(K::Fn) => matches!(second, Some(Tk::Ident(_))),
        Tk::Keyword(K::Async) => {
            matches!(second, Some(Tk::Keyword(K::Each | K::Using | K::Fn)))
        }
        Tk::Keyword(K::Pub) => matches!(
            second,
            Some(Tk::Keyword(
                K::Fn
                    | K::Struct
                    | K::Object
                    | K::Enum
                    | K::Trait
                    | K::Strict
                    | K::Kernel
                    | K::Async
            ))
        ),
        _ => false,
    };
    i32::from(opens)
}

/// The name a URL import is bound to: the last path segment before the
/// version, as a valid identifier (`neural-net` becomes `neural_net`).
/// `None` when the text is not `host/owner/repo...@version`.
pub fn url_module_name(url: &str) -> Option<String> {
    let (path, version) = url.split_once('@')?;
    if version.is_empty()
        || path.split('/').filter(|s| !s.is_empty()).count() < 3
        || !path.contains('.')
    {
        return None;
    }
    let last = path.trim_end_matches('/').rsplit('/').next()?;
    let name: String = last
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if name.is_empty() || name.starts_with(|c: char| c.is_ascii_digit()) {
        return None;
    }
    Some(name)
}
