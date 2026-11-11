//! Converts source text into tokens (grammar v0.4, section 9).
//!
//! The lexer never stops at the first problem. It records an error, recovers,
//! and keeps going, so one run reports every lexical mistake in a file.
//!
//! Layout rules implemented here rather than in the parser:
//!
//! - A line break becomes [`TokenKind::Newline`] only where it can end a
//!   statement: not inside brackets, not after a binary operator, comma or
//!   dot, not at the start of the file, and never twice in a row.
//! - A word directly after `.` or `?.` is an identifier even if it is spelled
//!   like a keyword, so `range.end` and `player.move()` work.

use crate::diagnostic::Diagnostic;
use crate::span::{Pos, Span};
use crate::token::{FloatSuffix, IntSuffix, Keyword, Punct, StrPart, Token, TokenKind};
use std::fmt;

/// What went wrong while tokenizing.
#[derive(Clone, Debug, PartialEq)]
pub enum LexErrorKind {
    /// A character that cannot start any token.
    UnexpectedChar(char),
    /// An operator from another language that Vyrn spells differently.
    UnsupportedOperator {
        found: &'static str,
        hint: &'static str,
    },
    /// A string with no closing quote.
    UnterminatedString,
    /// Python's `f"..."`. Every Vyrn string already interpolates.
    NeedlessStringPrefix(char),
    /// A `{` inside a string with no matching `}`.
    UnterminatedInterpolation,
    /// `{}` inside a string.
    EmptyInterpolation,
    /// A backslash sequence that is not defined. Holds the offending text.
    InvalidEscape(String),
    /// A radix prefix (`0x`, `0b`, `0o`) with no digits after it.
    MissingDigits { prefix: &'static str },
    /// Letters or digits that do not belong in the number. Holds the tail.
    InvalidNumber(String),
    /// An integer literal that does not fit in 64 bits.
    IntOverflow,
    /// A closing bracket with nothing open.
    UnmatchedDelimiter(Punct),
    /// A closing bracket that does not match the innermost open one.
    MismatchedDelimiter {
        open: Punct,
        open_at: Pos,
        close: Punct,
    },
    /// An opening bracket that is never closed.
    UnclosedDelimiter(Punct),
}

/// A lexical error with its location.
#[derive(Clone, Debug, PartialEq)]
pub struct LexError {
    pub kind: LexErrorKind,
    pub span: Span,
}

impl LexError {
    /// One sentence saying what is wrong.
    pub fn message(&self) -> String {
        match &self.kind {
            LexErrorKind::UnexpectedChar(c) => format!("unexpected character `{c}`"),
            LexErrorKind::UnsupportedOperator { found, .. } => {
                format!("`{found}` is not a Vyrn operator")
            }
            LexErrorKind::UnterminatedString => "this string is never closed".to_string(),
            LexErrorKind::NeedlessStringPrefix(prefix) => {
                format!("strings do not take a `{prefix}` prefix")
            }
            LexErrorKind::UnterminatedInterpolation => {
                "this `{` inside a string is never closed".to_string()
            }
            LexErrorKind::EmptyInterpolation => "empty `{}` inside a string".to_string(),
            LexErrorKind::InvalidEscape(text) => format!("unknown escape sequence `{text}`"),
            LexErrorKind::MissingDigits { prefix } => {
                format!("expected digits after `{prefix}`")
            }
            LexErrorKind::InvalidNumber(tail) => {
                format!("`{tail}` is not valid at the end of a number")
            }
            LexErrorKind::IntOverflow => "this integer does not fit in 64 bits".to_string(),
            LexErrorKind::UnmatchedDelimiter(close) => {
                format!("`{close}` has no matching opening bracket")
            }
            LexErrorKind::MismatchedDelimiter {
                open,
                open_at,
                close,
            } => {
                format!("`{close}` does not close the `{open}` opened at {open_at}")
            }
            LexErrorKind::UnclosedDelimiter(open) => format!("this `{open}` is never closed"),
        }
    }

    /// A suggestion for fixing the error, when there is an obvious one.
    pub fn hint(&self) -> Option<String> {
        match &self.kind {
            LexErrorKind::UnsupportedOperator { hint, .. } => Some((*hint).to_string()),
            LexErrorKind::UnterminatedString => Some(
                "add the closing quote; use `\"\"\"` for text that spans several lines".to_string(),
            ),
            LexErrorKind::NeedlessStringPrefix(_) => {
                Some("every string interpolates `{expr}` already; remove the prefix".to_string())
            }
            LexErrorKind::UnterminatedInterpolation | LexErrorKind::EmptyInterpolation => {
                Some("write `\\{` for a literal brace".to_string())
            }
            LexErrorKind::InvalidEscape(_) => Some(
                "valid escapes are \\n \\t \\r \\0 \\\\ \\\" \\' \\{ \\} \\xHH \\u{HEX}"
                    .to_string(),
            ),
            LexErrorKind::InvalidNumber(_) => Some(
                "number suffixes are i8 i16 i32 i64 u8 u16 u32 u64 f16 f32 f64 bf16".to_string(),
            ),
            _ => None,
        }
    }

    /// Formats the error with the offending source line and a marker under it.
    ///
    /// ```text
    /// error: this string is never closed
    ///  --> game.vyrn:3:11
    ///   |
    /// 3 |     print("hello
    ///   |           ^
    ///   = help: add a closing `"`; use `"""` for text that spans several lines
    /// ```
    pub fn render(&self, file: &str, src: &str) -> String {
        Diagnostic::from(self).render(file, src)
    }
}

impl From<&LexError> for Diagnostic {
    fn from(error: &LexError) -> Diagnostic {
        Diagnostic {
            message: error.message(),
            span: error.span,
            hint: error.hint(),
            file: None,
        }
    }
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.span.start, self.message())
    }
}

impl std::error::Error for LexError {}

/// Tokenizes `src`. Returns every error found if there is at least one.
///
/// On success the token list always ends with [`TokenKind::Eof`].
pub fn tokenize(src: &str) -> Result<Vec<Token>, Vec<LexError>> {
    let (tokens, errors) = Lexer::new(src).run();
    if errors.is_empty() {
        Ok(tokens)
    } else {
        Err(errors)
    }
}

/// The tokenizer. Most callers want [`tokenize`].
pub struct Lexer<'src> {
    src: &'src str,
    /// Byte index into `src` of the next unread character.
    index: usize,
    /// Position reported for the next unread character.
    pos: Pos,
    tokens: Vec<Token>,
    errors: Vec<LexError>,
    /// Open brackets, innermost last.
    delimiters: Vec<(Punct, Span)>,
}

impl<'src> Lexer<'src> {
    pub fn new(src: &'src str) -> Lexer<'src> {
        Lexer::new_at(src, Pos::START)
    }

    /// Tokenizes a fragment that begins at `start` in some larger file, so
    /// that the spans produced refer to that file. Used for the expressions
    /// inside string interpolations.
    pub fn new_at(src: &'src str, start: Pos) -> Lexer<'src> {
        Lexer {
            src,
            index: 0,
            pos: start,
            tokens: Vec::new(),
            errors: Vec::new(),
            delimiters: Vec::new(),
        }
    }

    /// Consumes the lexer and returns all tokens and all errors. The tokens
    /// are usable even when there are errors, which lets a parser carry on
    /// and report more problems in one pass.
    pub fn run(mut self) -> (Vec<Token>, Vec<LexError>) {
        while let Some(c) = self.peek() {
            let start = self.pos;
            match c {
                '\n' => {
                    self.bump();
                    self.newline(start);
                }
                ' ' | '\t' | '\r' | '\u{c}' => {
                    self.bump();
                }
                '#' => self.comment(start),
                '"' | '\'' => self.string(start, false),
                'r' if matches!(self.peek_at(1), Some('"' | '\'')) => {
                    self.bump();
                    self.string(start, true);
                }
                'f' if matches!(self.peek_at(1), Some('"' | '\'')) => {
                    // Python habit. Report it, then lex the string normally
                    // so the rest of the line is unaffected.
                    self.bump();
                    self.error(
                        LexErrorKind::NeedlessStringPrefix('f'),
                        Span::new(start, self.pos),
                    );
                    self.string(start, false);
                }
                c if is_ident_start(c) => self.word(start),
                c if c.is_ascii_digit() => self.number(start),
                _ => self.punct(start, c),
            }
        }

        for (open, span) in std::mem::take(&mut self.delimiters) {
            self.errors.push(LexError {
                kind: LexErrorKind::UnclosedDelimiter(open),
                span,
            });
        }
        // Brackets may have swallowed line breaks; make sure the last
        // statement is terminated either way.
        let end = Span::at(self.pos);
        if !matches!(
            self.tokens.last().map(|t| &t.kind),
            None | Some(TokenKind::Newline)
        ) {
            self.tokens.push(Token::new(TokenKind::Newline, end));
        }
        self.tokens.push(Token::new(TokenKind::Eof, end));
        self.errors.sort_by_key(|e| e.span.start);
        (self.tokens, self.errors)
    }

    // ---- cursor ---------------------------------------------------------

    fn rest(&self) -> &'src str {
        &self.src[self.index..]
    }

    fn peek(&self) -> Option<char> {
        self.rest().chars().next()
    }

    fn peek_at(&self, n: usize) -> Option<char> {
        self.rest().chars().nth(n)
    }

    fn bump(&mut self) -> Option<char> {
        let c = self.peek()?;
        self.index += c.len_utf8();
        self.pos = self.pos.advance(c);
        Some(c)
    }

    fn bump_n(&mut self, n: usize) {
        for _ in 0..n {
            self.bump();
        }
    }

    /// Consumes characters while `keep` holds and returns them.
    fn take_while(&mut self, keep: impl Fn(char) -> bool) -> &'src str {
        let from = self.index;
        while self.peek().is_some_and(&keep) {
            self.bump();
        }
        &self.src[from..self.index]
    }

    fn push(&mut self, kind: TokenKind, start: Pos) {
        self.tokens
            .push(Token::new(kind, Span::new(start, self.pos)));
    }

    fn error(&mut self, kind: LexErrorKind, span: Span) {
        self.errors.push(LexError { kind, span });
    }

    // ---- layout ---------------------------------------------------------

    fn newline(&mut self, start: Pos) {
        if !self.delimiters.is_empty() {
            return;
        }
        match self.tokens.last().map(|t| &t.kind) {
            None | Some(TokenKind::Newline) | Some(TokenKind::DocComment(_)) => {}
            Some(kind) if kind.continues_line() => {}
            Some(_) => self.push(TokenKind::Newline, start),
        }
    }

    fn comment(&mut self, start: Pos) {
        let text = self.take_while(|c| c != '\n');
        // `##` is documentation when it starts a line. `###` and longer are
        // ordinary comments, so banner lines do not become docs. A `##` after
        // code on the same line is an ordinary comment too; otherwise it
        // would swallow that statement's line break. Docs inside brackets
        // are dropped.
        let starts_line = matches!(
            self.tokens.last().map(|t| &t.kind),
            None | Some(TokenKind::Newline) | Some(TokenKind::DocComment(_))
        );
        let is_doc = starts_line && text.starts_with("##") && !text.starts_with("###");
        if is_doc && self.delimiters.is_empty() {
            let body = &text[2..];
            let body = body.strip_prefix(' ').unwrap_or(body).trim_end();
            self.push(TokenKind::DocComment(body.to_string()), start);
        }
    }

    // ---- words ----------------------------------------------------------

    fn word(&mut self, start: Pos) {
        let text = self.take_while(is_ident_continue);
        let after_dot = matches!(
            self.tokens.last().map(|t| &t.kind),
            Some(TokenKind::Punct(Punct::Dot | Punct::QuestionDot))
        );
        let kind = if text == "_" {
            TokenKind::Punct(Punct::Underscore)
        } else if after_dot {
            TokenKind::Ident(text.to_string())
        } else if text == "create" && !self.declares_a_function() {
            // `create` is a keyword only where it declares a function
            // (`create greet(name):`), so `create(x)` stays an ordinary call.
            TokenKind::Ident(text.to_string())
        } else if let Some(keyword) =
            Keyword::lookup(text).filter(|_| !self.soft_word_is_name(text))
        {
            TokenKind::Keyword(keyword)
        } else {
            TokenKind::Ident(text.to_string())
        };
        self.push(kind, start);
    }

    /// Whether a *soft* keyword is used as an ordinary name here. Words
    /// such as `to`, `by`, `move` and `type` are keywords only where they
    /// work as keywords, so they stay free for names: `create move(self)`,
    /// `bx, by = ...`, `pet.type`, `type = "cat"`.
    fn soft_word_is_name(&self, text: &str) -> bool {
        let rest = self.src[self.index..].trim_start_matches([' ', '\t']);
        let next_is_word = rest.chars().next().is_some_and(is_ident_start)
            && !rest.starts_with("and ")
            && !rest.starts_with("or ");
        let prev = self.tokens.last().map(|t| &t.kind);
        // After a value: `1 to 10`, `xs until n`, `x on gpu`.
        let after_value = matches!(
            prev,
            Some(
                TokenKind::Ident(_)
                    | TokenKind::Int { .. }
                    | TokenKind::Float { .. }
                    | TokenKind::Str(_)
                    | TokenKind::Keyword(Keyword::True | Keyword::False | Keyword::None)
                    | TokenKind::Punct(Punct::RParen | Punct::RBracket | Punct::RBrace)
            )
        );
        let line_start = matches!(
            prev,
            None | Some(
                TokenKind::Newline | TokenKind::DocComment(_) | TokenKind::Keyword(Keyword::Pub)
            )
        );
        match text {
            // `1 to 10 by 2`, `0 until n`
            "to" | "until" | "by" => !after_value,
            // `move x`, `own x`, `ref mut x`, `shared tensor[...]`,
            // `on gpu[0]:`, `region frame:`, `defer f.close()`, `par each`,
            // `route GET "/path"`
            "move" | "own" | "ref" | "shared" | "weak" | "ptr" | "on" | "region" | "defer"
            | "par" | "route" => !next_is_word,
            // `type Name = ...` at the start of a line
            "type" => !(line_start && next_is_word),
            // `routes "/api":`
            "routes" => !rest.starts_with('"'),
            _ => false,
        }
    }

    /// After the word `create`: whether a function name and its `(` or
    /// `[` follow on the same line.
    fn declares_a_function(&self) -> bool {
        let rest = self.src[self.index..].trim_start_matches([' ', '\t']);
        let Some(first) = rest.chars().next() else {
            return false;
        };
        if !is_ident_start(first) || rest.len() == self.src.len() - self.index {
            return false;
        }
        let name_len = rest
            .char_indices()
            .find(|&(_, c)| !is_ident_continue(c))
            .map_or(rest.len(), |(i, _)| i);
        let after = rest[name_len..].trim_start_matches([' ', '\t']);
        // `create operator +(self, other):`
        if &rest[..name_len] == "operator" {
            return OPERATORS.iter().any(|op| {
                after
                    .strip_prefix(op)
                    .is_some_and(|r| r.trim_start_matches([' ', '\t']).starts_with('('))
            });
        }
        after.starts_with('(') || after.starts_with('[')
    }

    // ---- numbers --------------------------------------------------------

    fn number(&mut self, start: Pos) {
        let radix = match (self.peek(), self.peek_at(1)) {
            (Some('0'), Some('x' | 'X')) => Some((16, "0x")),
            (Some('0'), Some('b' | 'B')) => Some((2, "0b")),
            (Some('0'), Some('o' | 'O')) => Some((8, "0o")),
            _ => None,
        };

        if let Some((radix, prefix)) = radix {
            self.bump_n(2);
            let digits = self
                .take_while(|c| c.is_digit(radix) || c == '_')
                .replace('_', "");
            if digits.is_empty() {
                self.take_while(is_ident_continue);
                self.error(
                    LexErrorKind::MissingDigits { prefix },
                    Span::new(start, self.pos),
                );
                self.push(
                    TokenKind::Int {
                        value: 0,
                        suffix: None,
                    },
                    start,
                );
                return;
            }
            let suffix = self.int_suffix();
            let value = self.parse_int(&digits, radix, start);
            self.push(TokenKind::Int { value, suffix }, start);
            return;
        }

        let mut text = self
            .take_while(|c| c.is_ascii_digit() || c == '_')
            .replace('_', "");
        let mut is_float = false;

        // A fraction needs a digit after the dot, so `1.max(2)` and a
        // trailing dot stay an integer followed by `.`.
        if self.peek() == Some('.') && self.peek_at(1).is_some_and(|c| c.is_ascii_digit()) {
            is_float = true;
            self.bump();
            text.push('.');
            text.push_str(
                &self
                    .take_while(|c| c.is_ascii_digit() || c == '_')
                    .replace('_', ""),
            );
        }

        if matches!(self.peek(), Some('e' | 'E')) {
            let sign = matches!(self.peek_at(1), Some('+' | '-'));
            let digit_at = if sign { 2 } else { 1 };
            if self.peek_at(digit_at).is_some_and(|c| c.is_ascii_digit()) {
                is_float = true;
                text.push('e');
                self.bump();
                if sign {
                    text.push(self.bump().unwrap_or('+'));
                }
                text.push_str(
                    &self
                        .take_while(|c| c.is_ascii_digit() || c == '_')
                        .replace('_', ""),
                );
            }
        }

        let tail_start = self.pos;
        let tail = self.take_while(is_ident_continue);
        let mut int_suffix = None;
        let mut float_suffix = None;
        if !tail.is_empty() {
            if let Some(suffix) = FloatSuffix::lookup(tail) {
                float_suffix = Some(suffix);
                is_float = true;
            } else if let (false, Some(suffix)) = (is_float, IntSuffix::lookup(tail)) {
                int_suffix = Some(suffix);
            } else {
                self.error(
                    LexErrorKind::InvalidNumber(tail.to_string()),
                    Span::new(tail_start, self.pos),
                );
            }
        }

        if is_float {
            // The text is digits, at most one dot and one exponent, so this
            // cannot fail; very large values become infinity.
            let value = text.parse::<f64>().unwrap_or(f64::INFINITY);
            self.push(
                TokenKind::Float {
                    value,
                    suffix: float_suffix,
                },
                start,
            );
        } else {
            let value = self.parse_int(&text, 10, start);
            self.push(
                TokenKind::Int {
                    value,
                    suffix: int_suffix,
                },
                start,
            );
        }
    }

    /// Reads the letters after a radix-prefixed integer. Anything other than
    /// an integer suffix is reported, which also catches digits that are out
    /// of range for the radix, as in `0b102`.
    fn int_suffix(&mut self) -> Option<IntSuffix> {
        let tail_start = self.pos;
        let tail = self.take_while(is_ident_continue);
        if tail.is_empty() {
            return None;
        }
        let suffix = IntSuffix::lookup(tail);
        if suffix.is_none() {
            self.error(
                LexErrorKind::InvalidNumber(tail.to_string()),
                Span::new(tail_start, self.pos),
            );
        }
        suffix
    }

    fn parse_int(&mut self, digits: &str, radix: u32, start: Pos) -> u64 {
        match u64::from_str_radix(digits, radix) {
            Ok(value) => value,
            Err(_) => {
                self.error(LexErrorKind::IntOverflow, Span::new(start, self.pos));
                0
            }
        }
    }

    // ---- strings --------------------------------------------------------

    /// Lexes a string whose opening quote (`"` or `'`) is the next
    /// character. `raw` strings have no escapes and no interpolation.
    fn string(&mut self, start: Pos, raw: bool) {
        let q = self.peek().unwrap_or('"');
        let quote = Span::new(self.pos, self.pos.advance(q));
        let triple_quote: &str = if q == '"' { "\"\"\"" } else { "\'\'\'" };
        let triple = self.rest().starts_with(triple_quote);
        self.bump_n(if triple { 3 } else { 1 });

        let mut parts = Vec::new();
        let mut text = String::new();
        loop {
            match self.peek() {
                None => {
                    self.error(LexErrorKind::UnterminatedString, quote);
                    break;
                }
                Some('\n') if !triple => {
                    // Leave the line break in place so the next line lexes
                    // normally and the damage stays on this line. Brackets
                    // opened on this line were most likely closed inside the
                    // lost text, so forget them instead of reporting each one
                    // and swallowing every later line break.
                    self.error(LexErrorKind::UnterminatedString, quote);
                    let line = self.pos.line;
                    self.delimiters.retain(|(_, span)| span.start.line != line);
                    break;
                }
                Some(c) if c == q => {
                    if !triple {
                        self.bump();
                        break;
                    }
                    if self.rest().starts_with(triple_quote) {
                        self.bump_n(3);
                        break;
                    }
                    self.bump();
                    text.push(c);
                }
                Some('\\') if !raw => self.escape(&mut text),
                Some('{') if !raw => {
                    if !text.is_empty() {
                        parts.push(StrPart::Text(std::mem::take(&mut text)));
                    }
                    if let Some(part) = self.interpolation(triple) {
                        parts.push(part);
                    }
                }
                Some(c) => {
                    self.bump();
                    text.push(c);
                }
            }
        }
        if !text.is_empty() {
            parts.push(StrPart::Text(text));
        }
        self.push(TokenKind::Str(parts), start);
    }

    /// Reads `{ ... }` inside a string and returns its source text. Braces
    /// and string literals nested inside the expression are skipped over.
    fn interpolation(&mut self, multiline: bool) -> Option<StrPart> {
        let open = Span::new(self.pos, self.pos.advance('{'));
        self.bump();
        let from = self.index;
        let inner_start = self.pos;
        let mut depth = 1usize;
        // The quote character of a string literal nested in the expression.
        let mut in_string: Option<char> = None;

        loop {
            match self.peek() {
                None => break,
                Some('\n') if !multiline => break,
                Some('\\') if in_string.is_some() => {
                    self.bump();
                    self.bump();
                }
                Some(c @ ('"' | '\'')) => {
                    match in_string {
                        None => in_string = Some(c),
                        Some(open) if open == c => in_string = None,
                        Some(_) => {}
                    }
                    self.bump();
                }
                Some('{') if in_string.is_none() => {
                    depth += 1;
                    self.bump();
                }
                Some('}') if in_string.is_none() => {
                    depth -= 1;
                    if depth == 0 {
                        let source = &self.src[from..self.index];
                        let span = Span::new(inner_start, self.pos);
                        self.bump();
                        if source.trim().is_empty() {
                            self.error(
                                LexErrorKind::EmptyInterpolation,
                                open.to(Span::at(self.pos)),
                            );
                            return None;
                        }
                        return Some(StrPart::Interp {
                            source: source.to_string(),
                            span,
                        });
                    }
                    self.bump();
                }
                Some(_) => {
                    self.bump();
                }
            }
        }
        self.error(LexErrorKind::UnterminatedInterpolation, open);
        None
    }

    /// Decodes one backslash escape into `out`.
    fn escape(&mut self, out: &mut String) {
        let start = self.pos;
        self.bump();
        let Some(c) = self.bump() else {
            return; // The caller reports the unterminated string.
        };
        match c {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            '0' => out.push('\0'),
            '\\' | '"' | '\'' | '{' | '}' => out.push(c),
            'x' => {
                let digits: String = self.rest().chars().take(2).collect();
                match u8::from_str_radix(&digits, 16) {
                    Ok(byte) if digits.len() == 2 && byte.is_ascii() => {
                        self.bump_n(2);
                        out.push(byte as char);
                    }
                    _ => self.invalid_escape(start),
                }
            }
            'u' => {
                let decoded = self.rest().strip_prefix('{').and_then(|rest| {
                    let digits = &rest[..rest.find('}')?];
                    let ch = char::from_u32(u32::from_str_radix(digits, 16).ok()?)?;
                    Some((ch, digits.len() + 2))
                });
                match decoded {
                    Some((ch, len)) => {
                        self.bump_n(len);
                        out.push(ch);
                    }
                    None => self.invalid_escape(start),
                }
            }
            _ => self.invalid_escape(start),
        }
    }

    fn invalid_escape(&mut self, start: Pos) {
        let span = Span::new(start, self.pos);
        let text = self.src[self.index - (self.pos.offset - start.offset) as usize..self.index]
            .to_string();
        self.error(LexErrorKind::InvalidEscape(text), span);
    }

    // ---- operators and punctuation ---------------------------------------

    fn punct(&mut self, start: Pos, c: char) {
        let next = self.peek_at(1);
        let third = self.peek_at(2);

        // Spellings from other languages get a specific message.
        let foreign = match (c, next) {
            ('&', Some('&')) => Some(("&&", 2, "use `and`")),
            ('|', Some('|')) => Some(("||", 2, "use `or`")),
            ('.', Some('.')) if third != Some('.') => Some((
                "..",
                2,
                "use `a to b` (inclusive) or `a until b` (exclusive)",
            )),
            ('!', n) if n != Some('=') => Some(("!", 1, "use `not`")),
            _ => None,
        };
        if let Some((found, len, hint)) = foreign {
            self.bump_n(len);
            self.error(
                LexErrorKind::UnsupportedOperator { found, hint },
                Span::new(start, self.pos),
            );
            return;
        }

        // Longest match first.
        let (punct, len) = match (c, next, third) {
            ('.', Some('.'), Some('.')) => (Punct::Ellipsis, 3),
            ('/', Some('/'), Some('=')) => (Punct::SlashSlashEq, 3),
            ('*', Some('*'), Some('=')) => (Punct::StarStarEq, 3),
            ('<', Some('<'), Some('=')) => (Punct::ShlEq, 3),
            ('>', Some('>'), Some('=')) => (Punct::ShrEq, 3),
            ('?', Some('?'), Some('=')) => (Punct::QuestionQuestionEq, 3),

            ('*', Some('*'), _) => (Punct::StarStar, 2),
            ('/', Some('/'), _) => (Punct::SlashSlash, 2),
            ('-', Some('>'), _) => (Punct::Arrow, 2),
            ('<', Some('<'), _) => (Punct::Shl, 2),
            ('>', Some('>'), _) => (Punct::Shr, 2),
            ('?', Some('?'), _) => (Punct::QuestionQuestion, 2),
            ('?', Some('.'), _) => (Punct::QuestionDot, 2),
            ('=', Some('>'), _) => (Punct::FatArrow, 2),
            ('|', Some('>'), _) => (Punct::PipeGt, 2),
            ('=', Some('='), _) => (Punct::EqEq, 2),
            ('!', Some('='), _) => (Punct::NotEq, 2),
            ('<', Some('='), _) => (Punct::LtEq, 2),
            ('>', Some('='), _) => (Punct::GtEq, 2),
            ('+', Some('='), _) => (Punct::PlusEq, 2),
            ('-', Some('='), _) => (Punct::MinusEq, 2),
            ('*', Some('='), _) => (Punct::StarEq, 2),
            ('/', Some('='), _) => (Punct::SlashEq, 2),
            ('%', Some('='), _) => (Punct::PercentEq, 2),
            ('@', Some('='), _) => (Punct::AtEq, 2),
            ('&', Some('='), _) => (Punct::AmpEq, 2),
            ('|', Some('='), _) => (Punct::PipeEq, 2),
            ('^', Some('='), _) => (Punct::CaretEq, 2),

            ('(', ..) => (Punct::LParen, 1),
            (')', ..) => (Punct::RParen, 1),
            ('[', ..) => (Punct::LBracket, 1),
            (']', ..) => (Punct::RBracket, 1),
            ('{', ..) => (Punct::LBrace, 1),
            ('}', ..) => (Punct::RBrace, 1),
            (',', ..) => (Punct::Comma, 1),
            (':', ..) => (Punct::Colon, 1),
            (';', ..) => (Punct::Semicolon, 1),
            ('.', ..) => (Punct::Dot, 1),
            ('?', ..) => (Punct::Question, 1),
            ('+', ..) => (Punct::Plus, 1),
            ('-', ..) => (Punct::Minus, 1),
            ('*', ..) => (Punct::Star, 1),
            ('/', ..) => (Punct::Slash, 1),
            ('%', ..) => (Punct::Percent, 1),
            ('@', ..) => (Punct::At, 1),
            ('&', ..) => (Punct::Amp, 1),
            ('|', ..) => (Punct::Pipe, 1),
            ('^', ..) => (Punct::Caret, 1),
            ('~', ..) => (Punct::Tilde, 1),
            ('=', ..) => (Punct::Eq, 1),
            ('<', ..) => (Punct::Lt, 1),
            ('>', ..) => (Punct::Gt, 1),
            _ => {
                self.bump();
                self.error(LexErrorKind::UnexpectedChar(c), Span::new(start, self.pos));
                return;
            }
        };
        self.bump_n(len);
        let span = Span::new(start, self.pos);

        match punct {
            Punct::LParen | Punct::LBracket | Punct::LBrace => self.delimiters.push((punct, span)),
            Punct::RParen | Punct::RBracket | Punct::RBrace => self.close_delimiter(punct, span),
            _ => {}
        }
        self.tokens.push(Token::new(TokenKind::Punct(punct), span));
    }

    fn close_delimiter(&mut self, close: Punct, span: Span) {
        let expected_open = match close {
            Punct::RParen => Punct::LParen,
            Punct::RBracket => Punct::LBracket,
            _ => Punct::LBrace,
        };
        match self.delimiters.pop() {
            None => self.error(LexErrorKind::UnmatchedDelimiter(close), span),
            Some((open, _)) if open == expected_open => {}
            Some((open, open_span)) => self.error(
                LexErrorKind::MismatchedDelimiter {
                    open,
                    open_at: open_span.start,
                    close,
                },
                span,
            ),
        }
    }
}

/// Letters and `_`. `char::is_alphabetic` stands in for Unicode XID_Start
/// until the compiler takes a dependency that provides the exact tables.
fn is_ident_start(c: char) -> bool {
    c == '_' || c.is_alphabetic()
}

fn is_ident_continue(c: char) -> bool {
    c == '_' || c.is_alphanumeric()
}

/// The operators a type can define with `create operator +(self, other)`,
/// longest first.
pub const OPERATORS: &[&str] = &[
    "//", "**", "==", "!=", "<=", ">=", "[]", "+", "-", "*", "/", "%", "<", ">",
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::Keyword as K;
    use crate::token::Punct as P;
    use crate::token::TokenKind as T;

    fn kinds(src: &str) -> Vec<TokenKind> {
        match tokenize(src) {
            Ok(tokens) => tokens.into_iter().map(|t| t.kind).collect(),
            Err(errors) => panic!("unexpected lex errors: {errors:?}"),
        }
    }

    fn errors(src: &str) -> Vec<LexError> {
        tokenize(src).expect_err("expected lex errors")
    }

    fn ident(name: &str) -> TokenKind {
        T::Ident(name.to_string())
    }

    fn int(value: u64) -> TokenKind {
        T::Int {
            value,
            suffix: None,
        }
    }

    fn text(s: &str) -> StrPart {
        StrPart::Text(s.to_string())
    }

    #[test]
    fn empty_source_is_just_eof() {
        assert_eq!(kinds(""), vec![T::Eof]);
        assert_eq!(kinds("\n\n  # only a comment\n"), vec![T::Eof]);
    }

    #[test]
    fn end_and_on_are_keywords() {
        assert_eq!(
            kinds("on gpu\nend"),
            vec![
                T::Keyword(K::On),
                ident("gpu"),
                T::Newline,
                T::Keyword(K::End),
                T::Newline,
                T::Eof
            ]
        );
        // Longer words that merely start with a keyword are identifiers.
        assert_eq!(
            kinds("ending online")[..2],
            [ident("ending"), ident("online")]
        );
    }

    #[test]
    fn every_keyword_round_trips() {
        for &keyword in K::ALL {
            // `create` is a keyword only before a function's name, and the
            // soft keywords only where they work as keywords.
            let src = match keyword {
                K::Fn => "create f()".to_string(),
                K::To | K::Until | K::By => format!("x {}", keyword.as_str()),
                K::Route => "route GET".to_string(),
                K::Routes => "routes \"/x\"".to_string(),
                K::Move
                | K::Own
                | K::Ref
                | K::Shared
                | K::Weak
                | K::Ptr
                | K::On
                | K::Region
                | K::Defer
                | K::Par
                | K::Type => format!("{} x", keyword.as_str()),
                _ => keyword.as_str().to_string(),
            };
            assert!(
                kinds(&src).contains(&T::Keyword(keyword)),
                "{src:?} should hold the keyword {keyword:?}"
            );
        }
        assert_eq!(kinds("create(x)")[0], ident("create"));
        // Soft keywords used as names.
        assert_eq!(kinds("by = 2")[0], ident("by"));
        assert_eq!(kinds("f(to, by)")[2], ident("to"));
        assert_eq!(kinds("create move(self)")[1], ident("move"));
        assert_eq!(kinds("type = 1")[0], ident("type"));
        assert_eq!(kinds("route = []")[0], ident("route"));
        assert_eq!(kinds("1 to 9 by 2")[1], T::Keyword(K::To));
        assert_eq!(kinds("1 to 9 by 2")[3], T::Keyword(K::By));
        assert_eq!(kinds("create = 1")[0], ident("create"));
    }

    #[test]
    fn every_punct_round_trips() {
        for &punct in P::ALL {
            let (tokens, _) = Lexer::new(punct.as_str()).run();
            assert_eq!(tokens[0].kind, T::Punct(punct), "lexing `{punct}`");
        }
    }

    #[test]
    fn star_is_elementwise_and_at_is_matmul() {
        assert_eq!(
            kinds("a * b @ c")[..5],
            [
                ident("a"),
                T::Punct(P::Star),
                ident("b"),
                T::Punct(P::At),
                ident("c")
            ]
        );
        assert_eq!(
            kinds("a *= b\na @= b\na ** b\na **= b")
                .into_iter()
                .filter(|k| matches!(k, T::Punct(_)))
                .collect::<Vec<_>>(),
            vec![
                T::Punct(P::StarEq),
                T::Punct(P::AtEq),
                T::Punct(P::StarStar),
                T::Punct(P::StarStarEq)
            ]
        );
    }

    #[test]
    fn python_arithmetic_operators() {
        assert_eq!(
            kinds("a // b % c\na //= 2\na %= 3")
                .into_iter()
                .filter(|k| matches!(k, T::Punct(_)))
                .collect::<Vec<_>>(),
            vec![
                T::Punct(P::SlashSlash),
                T::Punct(P::Percent),
                T::Punct(P::SlashSlashEq),
                T::Punct(P::PercentEq),
            ]
        );
    }

    #[test]
    fn header_lines_end_with_a_colon_and_a_newline() {
        assert_eq!(
            kinds("create area(w, h) -> f32:\n    return w * h\nend"),
            vec![
                T::Keyword(K::Fn),
                ident("area"),
                T::Punct(P::LParen),
                ident("w"),
                T::Punct(P::Comma),
                ident("h"),
                T::Punct(P::RParen),
                T::Punct(P::Arrow),
                ident("f32"),
                T::Punct(P::Colon),
                T::Newline,
                T::Keyword(K::Return),
                ident("w"),
                T::Punct(P::Star),
                ident("h"),
                T::Newline,
                T::Keyword(K::End),
                T::Newline,
                T::Eof,
            ]
        );
    }

    #[test]
    fn each_is_a_keyword_and_then_is_not() {
        assert_eq!(
            kinds("each then")[..2],
            [T::Keyword(K::Each), ident("then")]
        );
    }

    #[test]
    fn python_slices() {
        assert_eq!(
            kinds("h[:, -1]")[..7],
            [
                ident("h"),
                T::Punct(P::LBracket),
                T::Punct(P::Colon),
                T::Punct(P::Comma),
                T::Punct(P::Minus),
                int(1),
                T::Punct(P::RBracket),
            ]
        );
    }

    #[test]
    fn tracks_line_and_column() {
        let tokens = tokenize("main\n    let x = 42\nend\n").unwrap();
        let at = |i: usize| (tokens[i].span.start.line, tokens[i].span.start.column);
        assert_eq!(at(0), (1, 1)); // main
        assert_eq!(at(2), (2, 5)); // let
        assert_eq!(at(3), (2, 9)); // x
        assert_eq!(at(5), (2, 13)); // 42
        assert_eq!(tokens[5].span.end.column, 15);
        assert_eq!(at(7), (3, 1)); // end
    }

    #[test]
    fn columns_count_characters_not_bytes() {
        let src = "let s = \"héllo\" + x";
        let tokens = tokenize(src).unwrap();
        let x = tokens.iter().find(|t| t.kind == ident("x")).unwrap();
        assert_eq!(x.span.start.column, 19);
        assert_eq!(x.lexeme(src), "x");
    }

    #[test]
    fn newlines_collapse_and_never_lead() {
        assert_eq!(
            kinds("\n\n a \n\n\n b # trailing\n\n"),
            vec![ident("a"), T::Newline, ident("b"), T::Newline, T::Eof]
        );
    }

    #[test]
    fn final_newline_is_added_when_missing() {
        assert_eq!(kinds("end"), vec![T::Keyword(K::End), T::Newline, T::Eof]);
    }

    #[test]
    fn no_newline_inside_brackets() {
        let k = kinds("f(\n  a,\n  b\n)\n[1,\n 2]\n{\n\"k\": 1\n}");
        assert_eq!(k.iter().filter(|k| **k == T::Newline).count(), 3);
    }

    #[test]
    fn no_newline_after_a_binary_operator() {
        assert_eq!(
            kinds("let total = a +\n    b @\n    c\nend"),
            vec![
                T::Keyword(K::Let),
                ident("total"),
                T::Punct(P::Eq),
                ident("a"),
                T::Punct(P::Plus),
                ident("b"),
                T::Punct(P::At),
                ident("c"),
                T::Newline,
                T::Keyword(K::End),
                T::Newline,
                T::Eof,
            ]
        );
        assert_eq!(
            kinds("a and\nb"),
            vec![
                ident("a"),
                T::Keyword(K::And),
                ident("b"),
                T::Newline,
                T::Eof
            ]
        );
    }

    #[test]
    fn transpose_and_optional_do_end_a_line() {
        assert_eq!(
            kinds("w.T\nx: str?\ny"),
            vec![
                ident("w"),
                T::Punct(P::Dot),
                ident("T"),
                T::Newline,
                ident("x"),
                T::Punct(P::Colon),
                ident("str"),
                T::Punct(P::Question),
                T::Newline,
                ident("y"),
                T::Newline,
                T::Eof,
            ]
        );
    }

    #[test]
    fn keywords_after_a_dot_are_identifiers() {
        assert_eq!(
            kinds("range.end + player?.move")[..7],
            [
                ident("range"),
                T::Punct(P::Dot),
                ident("end"),
                T::Punct(P::Plus),
                ident("player"),
                T::Punct(P::QuestionDot),
                ident("move"),
            ]
        );
    }

    #[test]
    fn underscore_alone_is_punctuation() {
        assert_eq!(
            kinds("_ _x x_")[..3],
            [T::Punct(P::Underscore), ident("_x"), ident("x_")]
        );
    }

    #[test]
    fn contextual_words_stay_identifiers() {
        assert_eq!(
            kinds("GET POST self")[..3],
            [ident("GET"), ident("POST"), ident("self")]
        );
    }

    #[test]
    fn integers() {
        assert_eq!(
            kinds("0 42 1_000_000")[..3],
            [int(0), int(42), int(1_000_000)]
        );
        assert_eq!(kinds("0xFF 0b1010 0o17")[..3], [int(255), int(10), int(15)]);
        assert_eq!(
            kinds("255u8 0xFF_FFu32")[..2],
            [
                T::Int {
                    value: 255,
                    suffix: Some(IntSuffix::U8)
                },
                T::Int {
                    value: 0xFFFF,
                    suffix: Some(IntSuffix::U32)
                },
            ]
        );
    }

    #[test]
    fn floats() {
        let f = |value: f64| T::Float {
            value,
            suffix: None,
        };
        assert_eq!(
            kinds("1.5 0.25 1e3 2.5e-3")[..4],
            [f(1.5), f(0.25), f(1000.0), f(0.0025)]
        );
        assert_eq!(
            kinds("100.0f32 2f64")[..2],
            [
                T::Float {
                    value: 100.0,
                    suffix: Some(FloatSuffix::F32)
                },
                T::Float {
                    value: 2.0,
                    suffix: Some(FloatSuffix::F64)
                },
            ]
        );
    }

    #[test]
    fn a_dot_needs_a_digit_to_make_a_float() {
        assert_eq!(
            kinds("1.max")[..3],
            [int(1), T::Punct(P::Dot), ident("max")]
        );
        assert_eq!(kinds("0 to 3")[..3], [int(0), T::Keyword(K::To), int(3)]);
    }

    #[test]
    fn single_and_double_quotes_are_equivalent() {
        assert_eq!(kinds("'it\\'s'")[0], T::Str(vec![text("it's")]));
        assert_eq!(kinds("'say \"hi\"'")[0], T::Str(vec![text("say \"hi\"")]));
        assert_eq!(kinds("\"it's\"")[0], T::Str(vec![text("it's")]));
        let T::Str(parts) = &kinds("'{name}!'")[0] else {
            panic!()
        };
        assert!(matches!(&parts[0], StrPart::Interp { source, .. } if source == "name"));
        assert_eq!(kinds("r'C:\\{x}'")[0], T::Str(vec![text("C:\\{x}")]));
    }

    #[test]
    fn triple_single_quoted_strings() {
        let tokens = tokenize("'''a 'b' \"c\"\nd'''").unwrap();
        assert_eq!(tokens[0].kind, T::Str(vec![text("a 'b' \"c\"\nd")]));
    }

    #[test]
    fn interpolation_may_hold_the_other_quote() {
        let T::Str(parts) = &kinds("\"{d['k']}\"")[0] else {
            panic!()
        };
        assert!(matches!(&parts[0], StrPart::Interp { source, .. } if source == "d['k']"));
    }

    #[test]
    fn python_f_prefix_is_reported_but_the_string_still_lexes() {
        let (tokens, errs) = Lexer::new("print(f\"x = {x}\")").run();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, LexErrorKind::NeedlessStringPrefix('f'));
        assert!(errs[0].hint().is_some());
        assert!(matches!(&tokens[2].kind, T::Str(parts) if parts.len() == 2));
        // A name `f` followed by a space is fine.
        assert_eq!(kinds("f \"x\"")[0], ident("f"));
    }

    #[test]
    fn plain_and_empty_strings() {
        assert_eq!(kinds("\"hello\"")[0], T::Str(vec![text("hello")]));
        assert_eq!(kinds("\"\"")[0], T::Str(vec![]));
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(
            kinds(r#""a\n\t\"\\\{x\}\x41\u{1F600}""#)[0],
            T::Str(vec![text("a\n\t\"\\{x}A\u{1F600}")])
        );
    }

    #[test]
    fn interpolation_keeps_source_and_position() {
        let tokens =
            tokenize("print(\"Average: {average}, grade {letter_for(average)}\")").unwrap();
        let T::Str(parts) = &tokens[2].kind else {
            panic!("expected a string")
        };
        assert_eq!(parts.len(), 4);
        assert_eq!(parts[0], text("Average: "));
        let StrPart::Interp { source, span } = &parts[1] else {
            panic!("expected interpolation")
        };
        assert_eq!(source, "average");
        assert_eq!((span.start.line, span.start.column), (1, 18));
        let StrPart::Interp { source, .. } = &parts[3] else {
            panic!("expected interpolation")
        };
        assert_eq!(source, "letter_for(average)");
    }

    #[test]
    fn interpolation_may_contain_braces_and_strings() {
        let T::Str(parts) = &kinds(r#""{ {"a": "}"}["a"] }""#)[0] else {
            panic!()
        };
        assert_eq!(parts.len(), 1);
        let StrPart::Interp { source, .. } = &parts[0] else {
            panic!()
        };
        assert_eq!(source, r#" {"a": "}"}["a"] "#);
    }

    #[test]
    fn interpolated_source_can_be_lexed_in_place() {
        let tokens = tokenize("x = \"{a @ b}\"").unwrap();
        let T::Str(parts) = &tokens[2].kind else {
            panic!()
        };
        let StrPart::Interp { source, span } = &parts[0] else {
            panic!()
        };
        let (inner, errs) = Lexer::new_at(source, span.start).run();
        assert!(errs.is_empty());
        assert_eq!(inner[1].kind, T::Punct(P::At));
        assert_eq!(inner[1].span.start.column, 9);
    }

    #[test]
    fn route_path_holes_are_interpolation_parts() {
        let k = kinds("route GET \"/users/{id}/summary\" (id: u64)");
        assert_eq!(k[0], T::Keyword(K::Route));
        assert_eq!(k[1], ident("GET"));
        let T::Str(parts) = &k[2] else { panic!() };
        assert_eq!(parts[0], text("/users/"));
        assert!(matches!(&parts[1], StrPart::Interp { source, .. } if source == "id"));
        assert_eq!(parts[2], text("/summary"));
    }

    #[test]
    fn raw_strings_keep_everything() {
        assert_eq!(
            kinds(r#"r"C:\dir\{x}""#)[0],
            T::Str(vec![text(r"C:\dir\{x}")])
        );
        // A name that merely starts with r is still a name.
        assert_eq!(kinds("r")[0], ident("r"));
    }

    #[test]
    fn triple_quoted_strings_span_lines() {
        let src = "\"\"\"one\n\"two\"\n{n}\"\"\"\nx";
        let tokens = tokenize(src).unwrap();
        let T::Str(parts) = &tokens[0].kind else {
            panic!()
        };
        assert_eq!(parts[0], text("one\n\"two\"\n"));
        assert!(matches!(&parts[1], StrPart::Interp { source, .. } if source == "n"));
        assert_eq!(tokens[2].span.start.line, 4);
    }

    #[test]
    fn doc_comments_attach_to_the_next_line() {
        assert_eq!(
            kinds("## Moves one step.\ncreate step()\n### banner ###\n# plain"),
            vec![
                T::DocComment("Moves one step.".to_string()),
                T::Keyword(K::Fn),
                ident("step"),
                T::Punct(P::LParen),
                T::Punct(P::RParen),
                T::Newline,
                T::Eof,
            ]
        );
    }

    #[test]
    fn a_doc_marker_after_code_is_an_ordinary_comment() {
        assert_eq!(
            kinds("x = 1 ## note\ny")[..5],
            [ident("x"), T::Punct(P::Eq), int(1), T::Newline, ident("y"),]
        );
    }

    #[test]
    fn crlf_line_endings() {
        assert_eq!(
            kinds("a\r\nb\r\n"),
            vec![ident("a"), T::Newline, ident("b"), T::Newline, T::Eof]
        );
    }

    #[test]
    fn unterminated_string_points_at_the_opening_quote() {
        let errs = errors("main\n    print(\"hello\nend");
        // Only the string is reported, not the `(` it left open.
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, LexErrorKind::UnterminatedString);
        assert_eq!(
            (errs[0].span.start.line, errs[0].span.start.column),
            (2, 11)
        );
        // The following line still ends with a statement terminator.
        let (tokens, _) = Lexer::new("main\n    print(\"hello\nend").run();
        let kinds: Vec<_> = tokens.into_iter().map(|t| t.kind).rev().take(3).collect();
        assert_eq!(kinds, vec![T::Eof, T::Newline, T::Keyword(K::End)]);
    }

    #[test]
    fn lexing_continues_after_an_error() {
        let (tokens, errs) = Lexer::new("let a = $\nlet b = 2").run();
        assert_eq!(errs.len(), 1);
        assert_eq!(errs[0].kind, LexErrorKind::UnexpectedChar('$'));
        assert!(tokens.iter().any(|t| t.kind == ident("b")));
    }

    #[test]
    fn foreign_operators_get_hints() {
        for (src, found) in [
            ("a && b", "&&"),
            ("a || b", "||"),
            ("!a", "!"),
            ("0..3", ".."),
        ] {
            let errs = errors(src);
            assert_eq!(errs.len(), 1, "{src}");
            assert!(
                matches!(errs[0].kind, LexErrorKind::UnsupportedOperator { found: f, .. } if f == found),
                "{src}: {:?}",
                errs[0]
            );
            assert!(errs[0].hint().is_some());
        }
        // `!=` and `...` are real tokens.
        assert_eq!(kinds("a != b")[1], T::Punct(P::NotEq));
        assert_eq!(kinds("x[..., 0]")[2], T::Punct(P::Ellipsis));
    }

    #[test]
    fn bad_numbers() {
        assert_eq!(
            errors("0x")[0].kind,
            LexErrorKind::MissingDigits { prefix: "0x" }
        );
        assert_eq!(
            errors("0b102")[0].kind,
            LexErrorKind::InvalidNumber("2".to_string())
        );
        assert_eq!(
            errors("12abc")[0].kind,
            LexErrorKind::InvalidNumber("abc".to_string())
        );
        assert_eq!(
            errors("1.5u8")[0].kind,
            LexErrorKind::InvalidNumber("u8".to_string())
        );
        assert_eq!(
            errors("99999999999999999999")[0].kind,
            LexErrorKind::IntOverflow
        );
    }

    #[test]
    fn bad_escapes_and_interpolations() {
        assert_eq!(
            errors(r#""\q""#)[0].kind,
            LexErrorKind::InvalidEscape("\\q".to_string())
        );
        assert_eq!(
            errors(r#""a {} b""#)[0].kind,
            LexErrorKind::EmptyInterpolation
        );
        assert!(errors("\"a {b\n")
            .iter()
            .any(|e| e.kind == LexErrorKind::UnterminatedInterpolation));
    }

    #[test]
    fn bracket_mismatches() {
        assert_eq!(
            errors("a)")[0].kind,
            LexErrorKind::UnmatchedDelimiter(P::RParen)
        );
        assert!(matches!(
            errors("f(a]")[0].kind,
            LexErrorKind::MismatchedDelimiter {
                open: P::LParen,
                close: P::RBracket,
                ..
            }
        ));
        let errs = errors("x = [1, 2\nend");
        assert_eq!(errs[0].kind, LexErrorKind::UnclosedDelimiter(P::LBracket));
        assert_eq!(errs[0].span.start.column, 5);
    }

    #[test]
    fn rendered_error_shows_the_line_and_a_marker() {
        let src = "main:\n    if passed && on_time: print(\"ok\")\nend\n";
        let rendered = errors(src)[0].render("grade.vyrn", src);
        let expected = "\
error: `&&` is not a Vyrn operator
 --> grade.vyrn:2:15
  |
2 |     if passed && on_time: print(\"ok\")
  |               ^^
  = help: use `and`
";
        assert_eq!(rendered, expected);
    }
}
