//! Front end of the Vyrn compiler.
//!
//! Implemented so far, against `docs/grammar.ebnf` v0.4:
//!
//! - [`span`]: source positions with line and column tracking
//! - [`token`]: the token vocabulary
//! - [`lexer`]: source text to tokens
//! - [`ast`]: the tree the parser will build
//!
//! The parser and everything after it do not exist yet.

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod pretty;
pub mod span;
pub mod token;

pub use diagnostic::Diagnostic;
pub use lexer::{tokenize, LexError, LexErrorKind, Lexer};
pub use parser::{parse, parse_expr, parse_recovering, Parser};
pub use span::{Pos, Span};
pub use token::{Keyword, Punct, StrPart, Token, TokenKind};
