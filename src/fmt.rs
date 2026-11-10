//! `vyrn fmt`: one style for every Vyrn file, with no options.
//!
//! The formatter re-indents each line from the block structure (four
//! spaces per level), aligns `elif`, `else`, `catch`, `finally`, `is` arms
//! and `end` with the line that opened their block, indents lines inside
//! brackets one level past the line they started on, removes trailing
//! spaces and tabs, allows at most two blank lines in a row, and ends the
//! file with one newline. Comments are kept, and nothing inside a line is
//! rearranged, so formatting never changes what a program means; to be
//! sure, the result is parsed and compared with the original before it is
//! returned.

use crate::lexer::Lexer;
use crate::token::{Keyword as K, Punct as P, TokenKind as Tk};

const INDENT: &str = "    ";

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Frame {
    /// A block whose body is one level in: `fn`, `if`, `for`, ...
    Block(usize),
    /// `match`: its `is` arms are one level in, their bodies two.
    Match(usize),
    /// An `is` arm, at this depth.
    Arm(usize),
}

impl Frame {
    fn depth(self) -> usize {
        match self {
            Frame::Block(d) | Frame::Match(d) | Frame::Arm(d) => d,
        }
    }
}

/// What the formatter knows about one source line.
#[derive(Default, Clone)]
struct LineInfo {
    first: Option<Tk>,
    last: Option<Tk>,
    /// Inside a multi-line string: copied exactly.
    verbatim: bool,
    /// Open brackets at the start of the line.
    brackets_before: usize,
    /// Open brackets at the end of the line.
    brackets_after: usize,
    /// Continues the statement of the line before: inside brackets, or
    /// after a trailing operator. The lexer knows, because it ends
    /// statements with a `Newline` token.
    continues: bool,
}

/// Formats `src`. Fails, without changing anything, on a file that does not
/// parse.
pub fn format(src: &str) -> Result<String, String> {
    let original = crate::parser::parse(src)
        .map_err(|e| format!("fix the syntax errors first ({} found)", e.len()))?;
    let (tokens, errors) = Lexer::new(src).run();
    if !errors.is_empty() {
        return Err("fix the syntax errors first".into());
    }
    let lines: Vec<&str> = src.lines().collect();
    let mut info = vec![LineInfo::default(); lines.len() + 1];
    let mut depth = 0usize;
    let mut last_line = 0usize;
    let mut statement_ended = true;
    for t in &tokens {
        let line = t.span.start.line as usize - 1;
        if line >= lines.len() {
            break;
        }
        if line != last_line {
            for li in &mut info[last_line + 1..=line] {
                li.brackets_before = depth;
            }
            info[last_line].brackets_after = depth;
            last_line = line;
        }
        // Doc comments sit on their own lines, like comments.
        if matches!(t.kind, Tk::Newline | Tk::Eof | Tk::DocComment(_)) {
            statement_ended = true;
            continue;
        }
        if info[line].first.is_none() {
            info[line].first = Some(t.kind.clone());
            info[line].brackets_before = depth;
            info[line].continues = !statement_ended;
            statement_ended = false;
        }
        info[line].last = Some(t.kind.clone());
        match t.kind {
            Tk::Punct(P::LParen | P::LBracket | P::LBrace) => depth += 1,
            Tk::Punct(P::RParen | P::RBracket | P::RBrace) => depth = depth.saturating_sub(1),
            _ => {}
        }
        let end_line = t.span.end.line as usize - 1;
        if end_line > line {
            for li in &mut info[line + 1..=end_line.min(lines.len() - 1)] {
                li.verbatim = true;
            }
        }
    }
    info[last_line].brackets_after = depth;

    // Indentation of each code line from the block structure.
    let mut stack: Vec<Frame> = Vec::new();
    let mut indent: Vec<Option<usize>> = vec![None; lines.len()];
    // The depth of the statement that opened the current brackets.
    let mut statement_depth = 0usize;
    for (n, line) in lines.iter().enumerate() {
        let li = &info[n];
        if li.verbatim || line.trim().is_empty() || li.first.is_none() {
            continue;
        }
        let first = li.first.clone().unwrap_or(Tk::Eof);
        if li.continues || li.brackets_before > 0 {
            let closes = matches!(first, Tk::Punct(P::RParen | P::RBracket | P::RBrace));
            indent[n] = Some(statement_depth + usize::from(!closes));
            continue;
        }
        let body = stack.last().map_or(0, |f| match f {
            Frame::Block(d) | Frame::Arm(d) => d + 1,
            Frame::Match(d) => d + 1,
        });
        let opens = matches!(li.last, Some(Tk::Punct(P::Colon))) && li.brackets_after == 0;
        let d = match first {
            Tk::Keyword(K::End) => {
                while let Some(Frame::Arm(_)) = stack.last() {
                    stack.pop();
                }
                stack.pop().map_or(0, Frame::depth)
            }
            // A match's `else` is one more arm.
            Tk::Keyword(K::Else)
                if matches!(stack.last(), Some(Frame::Arm(_) | Frame::Match(_))) =>
            {
                if let Some(Frame::Arm(_)) = stack.last() {
                    stack.pop();
                }
                let arm = stack.last().map_or(0, |f| f.depth() + 1);
                if opens {
                    stack.push(Frame::Arm(arm));
                }
                arm
            }
            Tk::Keyword(K::Elif | K::Else | K::Catch | K::Finally) => {
                stack.last().map_or(0, |f| f.depth())
            }
            Tk::Keyword(K::Is) if matches!(stack.last(), Some(Frame::Arm(_) | Frame::Match(_))) => {
                if let Some(Frame::Arm(_)) = stack.last() {
                    stack.pop();
                }
                let arm = stack.last().map_or(0, |f| f.depth() + 1);
                if opens {
                    stack.push(Frame::Arm(arm));
                }
                arm
            }
            Tk::Keyword(K::Match) if opens => {
                stack.push(Frame::Match(body));
                body
            }
            _ => {
                if opens {
                    stack.push(Frame::Block(body));
                }
                body
            }
        };
        indent[n] = Some(d);
        statement_depth = d;
    }

    // Comments and blank lines take the indentation of the next code line.
    let mut next = 0usize;
    for n in (0..lines.len()).rev() {
        match indent[n] {
            Some(d) => next = d,
            None if !info[n].verbatim => indent[n] = Some(next),
            None => {}
        }
    }

    let mut out = String::with_capacity(src.len());
    let mut blanks = 0;
    for (n, line) in lines.iter().enumerate() {
        if info[n].verbatim {
            out.push_str(line);
            out.push('\n');
            blanks = 0;
            continue;
        }
        let text = line.trim();
        if text.is_empty() {
            blanks += 1;
            if blanks <= 2 && !out.is_empty() {
                out.push('\n');
            }
            continue;
        }
        blanks = 0;
        for _ in 0..indent[n].unwrap_or(0) {
            out.push_str(INDENT);
        }
        out.push_str(text);
        out.push('\n');
    }
    while out.ends_with("\n\n") {
        out.pop();
    }

    // Formatting must not change the program.
    let formatted = crate::parser::parse(&out)
        .map_err(|_| "formatting would break this file; please report it".to_string())?;
    if crate::pretty::module(&formatted) != crate::pretty::module(&original) {
        return Err("formatting would change this file's meaning; please report it".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_are_reindented() {
        let src = "fn f(x):\n  if x > 1:   \n        return 1\n  elif x < 0:\n   return -1\n  else:\n     return 0\n  end\nend\n\n\n\n\nmain:\n\tprint(f(2))  # two\nend";
        let want = "fn f(x):\n    if x > 1:\n        return 1\n    elif x < 0:\n        return -1\n    else:\n        return 0\n    end\nend\n\n\nmain:\n    print(f(2))  # two\nend\n";
        assert_eq!(format(src).unwrap(), want);
        // Formatting is stable.
        assert_eq!(format(want).unwrap(), want);
    }

    #[test]
    fn match_arms_try_and_brackets() {
        let src = "main:\nmatch x:\nis 1:\nprint(\"one\")\nis _: print(\"other\")\nend\ntry:\nrisky()\ncatch:\n# handled\nprint(\"no\")\nend\nxs = [\n1,\n2,\n]\nend\n";
        let want = "main:\n    match x:\n        is 1:\n            print(\"one\")\n        is _: print(\"other\")\n    end\n    try:\n        risky()\n    catch:\n        # handled\n        print(\"no\")\n    end\n    xs = [\n        1,\n        2,\n    ]\nend\n";
        assert_eq!(format(src).unwrap(), want);
    }

    #[test]
    fn match_else_and_operator_continuations() {
        let src = "## Doc.\nfn f(x):\n match x:\n  is 1: return 1\n  else:\n   return 0\n end\nend\nmain:\n let total = 1 +\n         2 +\n 3\nend\n";
        let want = "## Doc.\nfn f(x):\n    match x:\n        is 1: return 1\n        else:\n            return 0\n    end\nend\nmain:\n    let total = 1 +\n        2 +\n        3\nend\n";
        assert_eq!(format(src).unwrap(), want);
    }

    #[test]
    fn multi_line_strings_are_left_alone() {
        let src = "main:\n  text = \"\"\"\n   keep   \n  this\"\"\"\nend\n";
        let out = format(src).unwrap();
        assert!(out.contains("\n   keep   \n  this\"\"\""), "{out}");
    }

    #[test]
    fn broken_files_are_refused() {
        assert!(format("main:\n    x = (\nend\n").is_err());
    }
}
