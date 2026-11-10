//! Error reports shared by every compiler phase.

use crate::span::Span;
use std::fmt;

/// A problem in the source: what is wrong, where, and, when there is an
/// obvious fix, how to fix it.
#[derive(Clone, Debug, PartialEq)]
pub struct Diagnostic {
    pub message: String,
    pub span: Span,
    pub hint: Option<String>,
}

impl Diagnostic {
    pub fn new(message: impl Into<String>, span: Span) -> Diagnostic {
        Diagnostic {
            message: message.into(),
            span,
            hint: None,
        }
    }

    pub fn with_hint(mut self, hint: impl Into<String>) -> Diagnostic {
        self.hint = Some(hint.into());
        self
    }

    /// Formats the diagnostic with the offending source line and a marker
    /// under it.
    ///
    /// ```text
    /// error: this string is never closed
    ///  --> game.vyrn:3:11
    ///   |
    /// 3 |     print("hello
    ///   |           ^
    ///   = help: add the closing quote; use `"""` for text that spans several lines
    /// ```
    pub fn render(&self, file: &str, src: &str) -> String {
        let start = self.span.start;
        let line_text = src.lines().nth(start.line as usize - 1).unwrap_or("");
        let gutter = start.line.to_string().len();
        let pad = " ".repeat(gutter);

        let width = if self.span.end.line == start.line {
            (self.span.end.column.saturating_sub(start.column)).max(1) as usize
        } else {
            1
        };
        // Keep tabs in the indent so the marker lines up in any tab width.
        let indent: String = line_text
            .chars()
            .take(start.column as usize - 1)
            .map(|c| if c == '\t' { '\t' } else { ' ' })
            .collect();

        let mut out = format!("error: {}\n", self.message);
        out.push_str(&format!("{pad}--> {file}:{start}\n"));
        out.push_str(&format!("{pad} |\n"));
        out.push_str(&format!("{} | {line_text}\n", start.line));
        out.push_str(&format!("{pad} | {indent}{}\n", "^".repeat(width)));
        if let Some(hint) = &self.hint {
            out.push_str(&format!("{pad} = help: {hint}\n"));
        }
        out
    }
}

impl fmt::Display for Diagnostic {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.span.start, self.message)
    }
}

impl std::error::Error for Diagnostic {}
