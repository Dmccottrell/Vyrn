//! The token vocabulary of Vyrn (grammar v0.3, sections 7 and 9).

use crate::span::Span;
use std::fmt;

/// Defines an enum whose variants each have a fixed spelling.
macro_rules! spelled_enum {
    (
        $(#[$meta:meta])*
        $name:ident { $( $(#[$vmeta:meta])* $variant:ident => $text:literal ),* $(,)? }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $( $(#[$vmeta])* $variant ),* }

        impl $name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [$name] = &[ $( $name::$variant ),* ];

            /// The variant spelled `text`, if any.
            pub fn lookup(text: &str) -> Option<$name> {
                match text {
                    $( $text => Some($name::$variant), )*
                    _ => None,
                }
            }

            /// The source spelling.
            pub fn as_str(self) -> &'static str {
                match self { $( $name::$variant => $text ),* }
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(self.as_str())
            }
        }
    };
}

spelled_enum! {
    /// Hard keywords. These are never identifiers, except directly after `.`
    /// or `?.`, where the lexer produces [`TokenKind::Ident`] instead.
    ///
    /// The HTTP method names (`GET`, `POST`, ...) and the subscript words
    /// `all` and `last` are contextual. The lexer emits them as identifiers
    /// and the parser gives them meaning by position.
    Keyword {
        And => "and",
        Any => "any",
        As => "as",
        Async => "async",
        Await => "await",
        By => "by",
        Catch => "catch",
        Class => "class",
        Const => "const",
        Defer => "defer",
        Div => "div",
        Else => "else",
        /// Closes every block.
        End => "end",
        Enum => "enum",
        Export => "export",
        Extern => "extern",
        Fail => "fail",
        Fails => "fails",
        False => "false",
        Finally => "finally",
        Fn => "fn",
        For => "for",
        If => "if",
        Impl => "impl",
        /// Iteration and membership only.
        In => "in",
        Is => "is",
        Kernel => "kernel",
        Let => "let",
        Loop => "loop",
        Main => "main",
        Match => "match",
        Mod => "mod",
        Move => "move",
        Mut => "mut",
        None => "none",
        Not => "not",
        /// Placement on a device or a memory region.
        On => "on",
        Or => "or",
        Own => "own",
        Par => "par",
        Ptr => "ptr",
        Pub => "pub",
        Ref => "ref",
        Region => "region",
        Return => "return",
        Returns => "returns",
        Route => "route",
        Routes => "routes",
        Shared => "shared",
        Skip => "skip",
        Spawn => "spawn",
        Stop => "stop",
        Strict => "strict",
        Struct => "struct",
        Tensor => "tensor",
        Then => "then",
        To => "to",
        Trait => "trait",
        True => "true",
        Try => "try",
        Type => "type",
        Unsafe => "unsafe",
        Until => "until",
        Use => "use",
        Var => "var",
        Weak => "weak",
        While => "while",
        With => "with",
    }
}

impl Keyword {
    /// True for keywords that act as binary operators. A line ending in one
    /// of these continues on the next line.
    pub fn continues_line(self) -> bool {
        matches!(
            self,
            Keyword::And
                | Keyword::Or
                | Keyword::Not
                | Keyword::To
                | Keyword::Until
                | Keyword::By
                | Keyword::Div
                | Keyword::Mod
                | Keyword::In
        )
    }
}

spelled_enum! {
    /// Operators and punctuation.
    Punct {
        LParen => "(",
        RParen => ")",
        LBracket => "[",
        RBracket => "]",
        LBrace => "{",
        RBrace => "}",
        Comma => ",",
        Colon => ":",
        Semicolon => ";",
        Dot => ".",
        QuestionDot => "?.",
        Question => "?",
        QuestionQuestion => "??",
        Ellipsis => "...",
        Underscore => "_",
        /// Postfix transpose: `A'`.
        Apostrophe => "'",
        FatArrow => "=>",
        PipeGt => "|>",
        Plus => "+",
        Minus => "-",
        /// Element-wise multiplication.
        Star => "*",
        StarStar => "**",
        Slash => "/",
        /// Matrix multiplication. Also introduces a decorator at the start of
        /// a line; the parser tells the two apart by position.
        At => "@",
        Amp => "&",
        Pipe => "|",
        Caret => "^",
        Tilde => "~",
        Shl => "<<",
        Shr => ">>",
        Eq => "=",
        EqEq => "==",
        NotEq => "!=",
        Lt => "<",
        LtEq => "<=",
        Gt => ">",
        GtEq => ">=",
        PlusEq => "+=",
        MinusEq => "-=",
        StarEq => "*=",
        SlashEq => "/=",
        AtEq => "@=",
        StarStarEq => "**=",
        AmpEq => "&=",
        PipeEq => "|=",
        CaretEq => "^=",
        ShlEq => "<<=",
        ShrEq => ">>=",
        QuestionQuestionEq => "??=",
    }
}

impl Punct {
    /// True for the compound assignment operators (`+=`, `@=`, ...). Plain
    /// `=` is not included.
    pub fn is_assign_op(self) -> bool {
        matches!(
            self,
            Punct::PlusEq
                | Punct::MinusEq
                | Punct::StarEq
                | Punct::SlashEq
                | Punct::AtEq
                | Punct::StarStarEq
                | Punct::AmpEq
                | Punct::PipeEq
                | Punct::CaretEq
                | Punct::ShlEq
                | Punct::ShrEq
                | Punct::QuestionQuestionEq
        )
    }

    /// True if a statement cannot end with this token, so a following line
    /// break is not a statement terminator.
    pub fn continues_line(self) -> bool {
        self.is_assign_op()
            || matches!(
                self,
                Punct::Comma
                    | Punct::Dot
                    | Punct::QuestionDot
                    | Punct::QuestionQuestion
                    | Punct::FatArrow
                    | Punct::PipeGt
                    | Punct::Plus
                    | Punct::Minus
                    | Punct::Star
                    | Punct::StarStar
                    | Punct::Slash
                    | Punct::At
                    | Punct::Amp
                    | Punct::Pipe
                    | Punct::Caret
                    | Punct::Tilde
                    | Punct::Shl
                    | Punct::Shr
                    | Punct::Eq
                    | Punct::EqEq
                    | Punct::NotEq
                    | Punct::Lt
                    | Punct::LtEq
                    | Punct::Gt
                    | Punct::GtEq
            )
    }
}

spelled_enum! {
    /// Width suffix on an integer literal: `255u8`.
    IntSuffix {
        I8 => "i8",
        I16 => "i16",
        I32 => "i32",
        I64 => "i64",
        U8 => "u8",
        U16 => "u16",
        U32 => "u32",
        U64 => "u64",
    }
}

spelled_enum! {
    /// Width suffix on a float literal: `1.0f32`.
    FloatSuffix {
        F16 => "f16",
        F32 => "f32",
        F64 => "f64",
        Bf16 => "bf16",
    }
}

/// One piece of a string literal.
#[derive(Clone, Debug, PartialEq)]
pub enum StrPart {
    /// Literal text, with escapes already decoded.
    Text(String),
    /// The source text between `{` and `}`, not yet tokenized.
    ///
    /// In an ordinary string this is an expression; the parser lexes it with
    /// [`crate::lexer::Lexer::new_at`] so that spans stay absolute. In a route
    /// path it is the name of a path parameter.
    Interp { source: String, span: Span },
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    /// Integer literal. `value` is the magnitude; a leading `-` is a separate
    /// token.
    Int {
        value: u64,
        suffix: Option<IntSuffix>,
    },
    Float {
        value: f64,
        suffix: Option<FloatSuffix>,
    },
    /// String literal. An empty string has no parts.
    Str(Vec<StrPart>),
    Ident(String),
    Keyword(Keyword),
    Punct(Punct),
    /// A `##` comment, without the marker. The line break after it is not
    /// emitted, so it sits directly before the declaration it documents.
    DocComment(String),
    /// End of a statement. Never emitted twice in a row, at the start of a
    /// file, inside brackets, or after a token that continues the line.
    Newline,
    /// Always the last token.
    Eof,
}

impl TokenKind {
    pub fn is_keyword(&self, kw: Keyword) -> bool {
        matches!(self, TokenKind::Keyword(k) if *k == kw)
    }

    pub fn is_punct(&self, p: Punct) -> bool {
        matches!(self, TokenKind::Punct(q) if *q == p)
    }

    /// See [`Punct::continues_line`] and [`Keyword::continues_line`].
    pub fn continues_line(&self) -> bool {
        match self {
            TokenKind::Punct(p) => p.continues_line(),
            TokenKind::Keyword(k) => k.continues_line(),
            _ => false,
        }
    }

    /// A short description for error messages, such as "`end`" or "a string".
    pub fn describe(&self) -> String {
        match self {
            TokenKind::Int { .. } => "an integer".to_string(),
            TokenKind::Float { .. } => "a number".to_string(),
            TokenKind::Str(_) => "a string".to_string(),
            TokenKind::Ident(name) => format!("the name `{name}`"),
            TokenKind::Keyword(k) => format!("`{k}`"),
            TokenKind::Punct(p) => format!("`{p}`"),
            TokenKind::DocComment(_) => "a doc comment".to_string(),
            TokenKind::Newline => "the end of the line".to_string(),
            TokenKind::Eof => "the end of the file".to_string(),
        }
    }
}

/// A token and where it came from.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Token {
        Token { kind, span }
    }

    /// The exact source text of this token.
    pub fn lexeme<'src>(&self, src: &'src str) -> &'src str {
        self.span.text(src)
    }
}
