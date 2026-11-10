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
pub mod lexer;
pub mod span;
pub mod token;

pub use lexer::{tokenize, LexError, LexErrorKind, Lexer};
pub use span::{Pos, Span};
pub use token::{Keyword, Punct, StrPart, Token, TokenKind};
