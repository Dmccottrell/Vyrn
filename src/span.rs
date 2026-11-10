//! Source positions.

use std::fmt;

/// A position in a source file.
///
/// `line` and `column` are 1-based and are what error messages show. `column`
/// counts Unicode scalar values, not bytes. `offset` is a 0-based byte offset,
/// suitable for slicing the source string.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Pos {
    pub offset: u32,
    pub line: u32,
    pub column: u32,
}

impl Pos {
    /// The first character of a file.
    pub const START: Pos = Pos {
        offset: 0,
        line: 1,
        column: 1,
    };

    /// Returns the position after `c`, which must be the character at `self`.
    pub fn advance(self, c: char) -> Pos {
        let offset = self.offset + c.len_utf8() as u32;
        if c == '\n' {
            Pos {
                offset,
                line: self.line + 1,
                column: 1,
            }
        } else {
            Pos {
                offset,
                line: self.line,
                column: self.column + 1,
            }
        }
    }
}

impl Default for Pos {
    fn default() -> Self {
        Pos::START
    }
}

impl fmt::Display for Pos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.line, self.column)
    }
}

/// A half-open range of source text: `start` is included, `end` is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
pub struct Span {
    pub start: Pos,
    pub end: Pos,
}

impl Span {
    pub fn new(start: Pos, end: Pos) -> Span {
        Span { start, end }
    }

    /// A zero-width span at `pos`.
    pub fn at(pos: Pos) -> Span {
        Span {
            start: pos,
            end: pos,
        }
    }

    /// The smallest span covering both `self` and `other`.
    pub fn to(self, other: Span) -> Span {
        Span {
            start: self.start.min(other.start),
            end: self.end.max(other.end),
        }
    }

    /// The source text this span covers.
    pub fn text(self, src: &str) -> &str {
        &src[self.start.offset as usize..self.end.offset as usize]
    }
}

impl fmt::Display for Span {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.start)
    }
}
