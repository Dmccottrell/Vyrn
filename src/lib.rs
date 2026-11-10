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
//! Type checking, borrow checking and the native backends come next.

pub mod ast;
pub mod diagnostic;
pub mod lexer;
pub mod parser;
pub mod pretty;
pub mod span;
pub mod token;
pub mod vm;

pub use diagnostic::Diagnostic;
pub use lexer::{tokenize, LexError, LexErrorKind, Lexer};
pub use parser::{parse, parse_expr, parse_recovering, Parser};
pub use span::{Pos, Span};
pub use token::{Keyword, Punct, StrPart, Token, TokenKind};
