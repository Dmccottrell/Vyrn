//! A small JSON parser, for model headers, tokenizer files and web requests.

use std::fmt::Write as _;

#[derive(Clone, Debug, PartialEq)]
pub enum Json {
    Null,
    Bool(bool),
    Number(f64),
    String(String),
    Array(Vec<Json>),
    /// Keys in file order.
    Object(Vec<(String, Json)>),
}

impl Json {
    pub fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(entries) => entries.iter().find(|(k, _)| k == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn as_str(&self) -> Option<&str> {
        match self {
            Json::String(s) => Some(s),
            _ => None,
        }
    }

    pub fn as_array(&self) -> Option<&[Json]> {
        match self {
            Json::Array(items) => Some(items),
            _ => None,
        }
    }

    pub fn as_u64(&self) -> Option<u64> {
        match self {
            Json::Number(n) if *n >= 0.0 && n.fract() == 0.0 && *n < 1.9e19 => Some(*n as u64),
            _ => None,
        }
    }

    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }
}

pub fn parse(text: &str) -> Result<Json, String> {
    let mut parser = Parser {
        bytes: text.as_bytes(),
        pos: 0,
    };
    parser.skip_whitespace();
    let value = parser.value(0)?;
    parser.skip_whitespace();
    if parser.pos != parser.bytes.len() {
        return Err(parser.error("unexpected text after the JSON value"));
    }
    Ok(value)
}

/// Writes `text` as a JSON string literal.
pub fn write_string(text: &str, out: &mut String) {
    out.push('"');
    for c in text.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
}

const MAX_DEPTH: usize = 512;

struct Parser<'a> {
    bytes: &'a [u8],
    pos: usize,
}

impl Parser<'_> {
    fn error(&self, message: &str) -> String {
        format!("invalid JSON at byte {}: {message}", self.pos)
    }

    fn skip_whitespace(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.bytes.get(self.pos) == Some(&byte) {
            self.pos += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }

    fn literal(&mut self, word: &str, value: Json) -> Result<Json, String> {
        if self.bytes[self.pos..].starts_with(word.as_bytes()) {
            self.pos += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown word"))
        }
    }

    fn value(&mut self, depth: usize) -> Result<Json, String> {
        if depth > MAX_DEPTH {
            return Err(self.error("nested too deeply"));
        }
        match self.bytes.get(self.pos) {
            None => Err(self.error("unexpected end of input")),
            Some(b'n') => self.literal("null", Json::Null),
            Some(b't') => self.literal("true", Json::Bool(true)),
            Some(b'f') => self.literal("false", Json::Bool(false)),
            Some(b'"') => self.string().map(Json::String),
            Some(b'[') => {
                self.pos += 1;
                let mut items = Vec::new();
                self.skip_whitespace();
                if self.bytes.get(self.pos) == Some(&b']') {
                    self.pos += 1;
                    return Ok(Json::Array(items));
                }
                loop {
                    self.skip_whitespace();
                    items.push(self.value(depth + 1)?);
                    self.skip_whitespace();
                    match self.bytes.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b']') => {
                            self.pos += 1;
                            return Ok(Json::Array(items));
                        }
                        _ => return Err(self.error("expected `,` or `]`")),
                    }
                }
            }
            Some(b'{') => {
                self.pos += 1;
                let mut entries = Vec::new();
                self.skip_whitespace();
                if self.bytes.get(self.pos) == Some(&b'}') {
                    self.pos += 1;
                    return Ok(Json::Object(entries));
                }
                loop {
                    self.skip_whitespace();
                    if self.bytes.get(self.pos) != Some(&b'"') {
                        return Err(self.error("expected a key in quotes"));
                    }
                    let key = self.string()?;
                    self.skip_whitespace();
                    self.expect(b':')?;
                    self.skip_whitespace();
                    let value = self.value(depth + 1)?;
                    entries.push((key, value));
                    self.skip_whitespace();
                    match self.bytes.get(self.pos) {
                        Some(b',') => self.pos += 1,
                        Some(b'}') => {
                            self.pos += 1;
                            return Ok(Json::Object(entries));
                        }
                        _ => return Err(self.error("expected `,` or `}`")),
                    }
                }
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn number(&mut self) -> Result<Json, String> {
        let start = self.pos;
        while let Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') = self.bytes.get(self.pos) {
            self.pos += 1;
        }
        let text = std::str::from_utf8(&self.bytes[start..self.pos]).unwrap_or("");
        text.parse::<f64>()
            .map(Json::Number)
            .map_err(|_| self.error("invalid number"))
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .bytes
            .get(self.pos..self.pos + 4)
            .and_then(|d| std::str::from_utf8(d).ok())
            .and_then(|d| u32::from_str_radix(d, 16).ok())
            .ok_or_else(|| self.error("invalid \\u escape"))?;
        self.pos += 4;
        Ok(digits)
    }

    fn string(&mut self) -> Result<String, String> {
        self.pos += 1;
        let mut out = String::new();
        loop {
            let start = self.pos;
            while let Some(&b) = self.bytes.get(self.pos) {
                if b == b'"' || b == b'\\' {
                    break;
                }
                self.pos += 1;
            }
            out.push_str(
                std::str::from_utf8(&self.bytes[start..self.pos])
                    .map_err(|_| self.error("invalid UTF-8"))?,
            );
            match self.bytes.get(self.pos) {
                None => return Err(self.error("unterminated string")),
                Some(b'"') => {
                    self.pos += 1;
                    return Ok(out);
                }
                _ => {
                    self.pos += 1;
                    let escape = *self
                        .bytes
                        .get(self.pos)
                        .ok_or_else(|| self.error("unterminated escape"))?;
                    self.pos += 1;
                    match escape {
                        b'"' => out.push('"'),
                        b'\\' => out.push('\\'),
                        b'/' => out.push('/'),
                        b'b' => out.push('\u{8}'),
                        b'f' => out.push('\u{c}'),
                        b'n' => out.push('\n'),
                        b'r' => out.push('\r'),
                        b't' => out.push('\t'),
                        b'u' => {
                            let first = self.hex4()?;
                            let code = if (0xd800..0xdc00).contains(&first)
                                && self.bytes[self.pos..].starts_with(b"\\u")
                            {
                                self.pos += 2;
                                let second = self.hex4()?;
                                0x10000
                                    + ((first - 0xd800) << 10)
                                    + (second.wrapping_sub(0xdc00) & 0x3ff)
                            } else {
                                first
                            };
                            out.push(char::from_u32(code).unwrap_or('\u{fffd}'));
                        }
                        _ => return Err(self.error("unknown escape")),
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_values() {
        let j = parse(r#" {"a": [1, -2.5e3, true, null], "b": {"c": "x\nyé😀"}} "#).unwrap();
        assert_eq!(
            j.get("a"),
            Some(&Json::Array(vec![
                Json::Number(1.0),
                Json::Number(-2500.0),
                Json::Bool(true),
                Json::Null
            ]))
        );
        assert_eq!(
            j.get("b").and_then(|b| b.get("c")).and_then(Json::as_str),
            Some("x\nyé😀")
        );
        assert_eq!(parse("[]").unwrap(), Json::Array(vec![]));
        assert_eq!(parse("{}").unwrap(), Json::Object(vec![]));
    }

    #[test]
    fn rejects_bad_input() {
        for bad in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "tru",
            "\"open",
            "1 2",
            "{1: 2}",
        ] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        let deep = "[".repeat(1000) + &"]".repeat(1000);
        assert!(parse(&deep).is_err());
    }

    #[test]
    fn writes_strings() {
        let mut out = String::new();
        write_string("a\"b\\c\n\u{1}", &mut out);
        assert_eq!(out, r#""a\"b\\c\n\u0001""#);
    }
}
