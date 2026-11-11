//! The Vyrn compiler and interpreter.
//!
//! Implemented so far, against `docs/grammar.ebnf` v0.4:
//!
//! - [`span`], [`diagnostic`]: source positions and error reports
//! - [`token`], [`lexer`]: source text to tokens
//! - [`ast`], [`parser`], [`pretty`]: tokens to a syntax tree, and back to text
//! - [`vm`]: a bytecode compiler and register-based virtual machine that runs
//!   programs, for `vyrn run` and the REPL
//!
//! - [`check`]: gradual type checking, `let`/`strict` rules and borrow checks
//!
//! The native backends come next.

pub mod ast;
pub mod check;
pub mod diagnostic;
pub mod fmt;
pub mod lexer;
pub mod parser;
pub mod prelude;
pub mod pretty;
pub mod project;
pub mod span;
pub mod token;
pub mod vm;

pub use diagnostic::Diagnostic;
pub use lexer::{tokenize, LexError, LexErrorKind, Lexer};
pub use parser::{parse, parse_expr, parse_recovering, Parser};
pub use span::{Pos, Span};
pub use token::{Keyword, Punct, StrPart, Token, TokenKind};
