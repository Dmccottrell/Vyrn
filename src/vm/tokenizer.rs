//! BPE tokenizers, read from Hugging Face `tokenizer.json` files.
//!
//! Two families are supported:
//!
//! - **Byte-level BPE** (GPT-2, Llama 3, Qwen, Mistral Nemo): text is split
//!   into words by a fixed pattern, each word's bytes are mapped to printable
//!   characters, and merges are applied within each word.
//! - **SentencePiece-style BPE** (Llama 2, Mistral 7B): spaces become `▁`,
//!   merges are applied, and characters with no token fall back to
//!   `<0xNN>` byte tokens.
//!
//! Special tokens such as `<|endoftext|>` are matched before anything else
//! and map straight to their ids.

use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap};

use super::json::{self, Json};

/// When a `▁` is put before the text (SentencePiece style).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Prepend {
    Never,
    /// Before the first piece of text only.
    First,
    /// Before every piece of text between special tokens.
    Always,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    ByteLevel,
    Metaspace,
}

/// How byte-level text is split into words before merging.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Split {
    /// GPT-2: `'s|'t|'re|'ve|'m|'ll|'d| ?\p{L}+| ?\p{N}+| ?[^\s\p{L}\p{N}]+|\s+(?!\S)|\s+`
    Gpt2,
    /// Llama 3 / Qwen 2: like GPT-2, but contractions ignore case, digits
    /// split into groups of up to three, and a punctuation run may carry a
    /// leading space and trailing newlines.
    Llama3,
}

#[derive(Debug)]
pub struct Tokenizer {
    mode: Mode,
    split: Split,
    vocab: HashMap<String, u32>,
    tokens: HashMap<u32, String>,
    /// (left id, right id) -> (rank, merged id).
    merges: HashMap<(u32, u32), (usize, u32)>,
    /// Whether a word found whole in the vocabulary skips merging.
    ignore_merges: bool,
    /// Whether consecutive unknown characters become one unknown token.
    fuse_unk: bool,
    /// Special tokens, longest first, so the longest match wins.
    added: Vec<(String, u32)>,
    unk: Option<u32>,
    byte_fallback: bool,
    /// Byte-level: the printable character standing for each byte.
    byte_chars: Vec<char>,
    char_bytes: HashMap<char, u8>,
    prepend: Prepend,
    /// Metaspace: whether text is split into words at each `▁`.
    metaspace_split: bool,
}

/// GPT-2's map from bytes to printable characters: printable bytes stand
/// for themselves, the rest are shifted past U+0100.
fn byte_chars() -> Vec<char> {
    let mut chars = vec!['\0'; 256];
    let mut extra = 0u32;
    for b in 0..=255u32 {
        let printable =
            (0x21..=0x7e).contains(&b) || (0xa1..=0xac).contains(&b) || (0xae..=0xff).contains(&b);
        chars[b as usize] = if printable {
            char::from_u32(b).unwrap_or('\0')
        } else {
            extra += 1;
            char::from_u32(255 + extra).unwrap_or('\0')
        };
    }
    chars
}

impl Tokenizer {
    pub fn from_file(path: &str) -> Result<Tokenizer, String> {
        let text = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
        Tokenizer::from_json(&text).map_err(|e| format!("{path}: {e}"))
    }

    pub fn from_json(text: &str) -> Result<Tokenizer, String> {
        let root = json::parse(text)?;
        let model = root.get("model").ok_or("no `model` section")?;
        if let Some(kind) = model.get("type").and_then(Json::as_str) {
            if kind != "BPE" {
                return Err(format!("only BPE tokenizers are supported, not {kind}"));
            }
        }
        let Some(Json::Object(entries)) = model.get("vocab") else {
            return Err("no vocabulary".into());
        };
        let mut vocab = HashMap::with_capacity(entries.len());
        for (token, id) in entries {
            let id = id.as_u64().ok_or("a vocabulary id is not a number")? as u32;
            vocab.insert(token.clone(), id);
        }
        let mut merges = HashMap::new();
        for (rank, merge) in model
            .get("merges")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .enumerate()
        {
            // Either "a b" or ["a", "b"], depending on the file's age.
            let pair = match merge {
                Json::String(s) => s
                    .split_once(' ')
                    .map(|(a, b)| (a.to_string(), b.to_string())),
                Json::Array(parts) => match parts.as_slice() {
                    [Json::String(a), Json::String(b)] => Some((a.clone(), b.clone())),
                    _ => None,
                },
                _ => None,
            };
            let (a, b) = pair.ok_or("a merge is not a pair of tokens")?;
            if let (Some(&left), Some(&right), Some(&merged)) =
                (vocab.get(&a), vocab.get(&b), vocab.get(&format!("{a}{b}")))
            {
                merges.entry((left, right)).or_insert((rank, merged));
            }
        }
        let mut added: Vec<(String, u32)> = root
            .get("added_tokens")
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|t| {
                Some((
                    t.get("content")?.as_str()?.to_string(),
                    t.get("id")?.as_u64()? as u32,
                ))
            })
            .collect();
        added.sort_by_key(|a| std::cmp::Reverse(a.0.len()));

        // The file says which family it is through its pre-tokenizer,
        // normalizer and decoder.
        let described = |key: &str| root.get(key).map(json_text).unwrap_or_default();
        let pre = described("pre_tokenizer");
        let decoder = described("decoder");
        let normalizer = described("normalizer");
        let mode = if pre.contains("ByteLevel") || decoder.contains("ByteLevel") {
            Mode::ByteLevel
        } else if pre.contains("Metaspace") || normalizer.contains('▁') || decoder.contains('▁')
        {
            Mode::Metaspace
        } else if vocab.contains_key("Ġ") {
            Mode::ByteLevel
        } else {
            Mode::Metaspace
        };
        let split = if pre.contains("{1,3}") {
            Split::Llama3
        } else {
            Split::Gpt2
        };
        let prepend = if mode != Mode::Metaspace {
            Prepend::Never
        } else if normalizer.contains("\"Prepend\"")
            || pre.contains("\"prepend_scheme\":\"always\"")
            || pre.contains("\"add_prefix_space\":true")
        {
            Prepend::Always
        } else if pre.contains("\"prepend_scheme\":\"first\"") {
            Prepend::First
        } else {
            Prepend::Never
        };
        let metaspace_split = pre.contains("Metaspace") && !pre.contains("\"split\":false");
        let flag = |key: &str| model.get(key).and_then(Json::as_bool).unwrap_or(false);

        let unk = model
            .get("unk_token")
            .and_then(Json::as_str)
            .and_then(|t| vocab.get(t).copied());
        let byte_fallback = flag("byte_fallback");
        let tokens = vocab
            .iter()
            .map(|(t, &id)| (id, t.clone()))
            .chain(added.iter().map(|(t, id)| (*id, t.clone())))
            .collect();
        let byte_chars = byte_chars();
        let char_bytes = byte_chars
            .iter()
            .enumerate()
            .map(|(b, &c)| (c, b as u8))
            .collect();
        Ok(Tokenizer {
            mode,
            split,
            vocab,
            tokens,
            merges,
            ignore_merges: flag("ignore_merges"),
            fuse_unk: flag("fuse_unk"),
            added,
            unk,
            byte_fallback,
            byte_chars,
            char_bytes,
            prepend,
            metaspace_split,
        })
    }

    pub fn vocab_size(&self) -> usize {
        self.tokens.len()
    }

    pub fn token_id(&self, token: &str) -> Option<u32> {
        self.vocab.get(token).copied().or_else(|| {
            self.added
                .iter()
                .find(|(t, _)| t == token)
                .map(|(_, id)| *id)
        })
    }

    pub fn encode(&self, text: &str) -> Vec<u32> {
        let mut ids = Vec::new();
        let mut rest = text;
        let mut first = true;
        while !rest.is_empty() {
            // The next special token, if any.
            let next = self
                .added
                .iter()
                .filter_map(|(t, id)| rest.find(t.as_str()).map(|at| (at, t.len(), *id)))
                .min_by_key(|&(at, len, _)| (at, std::cmp::Reverse(len)));
            let (plain, special) = match next {
                Some((at, len, id)) => (&rest[..at], Some((len, id, at))),
                None => (rest, None),
            };
            self.encode_plain(plain, first, &mut ids);
            first = false;
            match special {
                Some((len, id, at)) => {
                    ids.push(id);
                    rest = &rest[at + len..];
                }
                None => break,
            }
        }
        ids
    }

    fn encode_plain(&self, text: &str, at_start: bool, ids: &mut Vec<u32>) {
        if text.is_empty() {
            return;
        }
        match self.mode {
            Mode::ByteLevel => {
                for word in split_words(text, self.split) {
                    let mapped: String =
                        word.bytes().map(|b| self.byte_chars[b as usize]).collect();
                    self.merge(&mapped, ids);
                }
            }
            Mode::Metaspace => {
                let mut normalized = String::new();
                if self.prepend == Prepend::Always || (self.prepend == Prepend::First && at_start) {
                    normalized.push('▁');
                }
                normalized.push_str(&text.replace(' ', "▁"));
                if !self.metaspace_split {
                    self.merge(&normalized, ids);
                    return;
                }
                // Split before each `▁` that starts a word.
                let mut start = 0;
                for (at, c) in normalized.char_indices() {
                    if c == '▁' && at > start {
                        self.merge(&normalized[start..at], ids);
                        start = at;
                    }
                }
                self.merge(&normalized[start..], ids);
            }
        }
    }

    /// Splits a word into its starting symbols: each character's token, or
    /// its byte tokens, or the unknown token.
    fn symbols(&self, word: &str) -> Vec<u32> {
        let mut symbols = Vec::with_capacity(word.len());
        let mut buffer = [0u8; 4];
        let mut last_unknown = false;
        for c in word.chars() {
            let text = c.encode_utf8(&mut buffer);
            if let Some(&id) = self.vocab.get(&*text) {
                symbols.push(id);
                last_unknown = false;
                continue;
            }
            if self.byte_fallback {
                let bytes: Option<Vec<u32>> = text
                    .bytes()
                    .map(|b| self.vocab.get(&format!("<0x{b:02X}>")).copied())
                    .collect();
                if let Some(bytes) = bytes {
                    symbols.extend(bytes);
                    last_unknown = false;
                    continue;
                }
            }
            if let Some(unk) = self.unk {
                if !(self.fuse_unk && last_unknown) {
                    symbols.push(unk);
                }
                last_unknown = true;
            }
        }
        symbols
    }

    /// Applies the merges to one word and appends its token ids. Merges are
    /// applied lowest rank first, leftmost first among equals, using a
    /// priority queue, so long inputs take n log n steps.
    fn merge(&self, word: &str, ids: &mut Vec<u32>) {
        if self.ignore_merges {
            if let Some(&id) = self.vocab.get(word) {
                ids.push(id);
                return;
            }
        }
        let mut symbols = self.symbols(word);
        let n = symbols.len();
        const NONE: usize = usize::MAX;
        let mut next: Vec<usize> = (1..=n).map(|i| if i < n { i } else { NONE }).collect();
        let mut prev: Vec<usize> = (0..n).map(|i| if i == 0 { NONE } else { i - 1 }).collect();
        let mut alive = vec![true; n];
        let mut queue = BinaryHeap::new();
        for i in 0..n.saturating_sub(1) {
            if let Some(&(rank, _)) = self.merges.get(&(symbols[i], symbols[i + 1])) {
                queue.push(Reverse((rank, i)));
            }
        }
        while let Some(Reverse((rank, i))) = queue.pop() {
            let j = next[i];
            if !alive[i] || j == NONE {
                continue;
            }
            // Skip entries made stale by an earlier merge.
            let Some(&(current, merged)) = self.merges.get(&(symbols[i], symbols[j])) else {
                continue;
            };
            if current != rank {
                continue;
            }
            symbols[i] = merged;
            alive[j] = false;
            next[i] = next[j];
            if next[j] != NONE {
                prev[next[j]] = i;
            }
            if prev[i] != NONE {
                let p = prev[i];
                if let Some(&(r, _)) = self.merges.get(&(symbols[p], symbols[i])) {
                    queue.push(Reverse((r, p)));
                }
            }
            if next[i] != NONE {
                if let Some(&(r, _)) = self.merges.get(&(symbols[i], symbols[next[i]])) {
                    queue.push(Reverse((r, i)));
                }
            }
        }
        let mut i = if n == 0 { NONE } else { 0 };
        while i != NONE {
            ids.push(symbols[i]);
            i = next[i];
        }
    }

    pub fn decode(&self, ids: &[u32]) -> String {
        let mut bytes = Vec::new();
        for id in ids {
            let Some(token) = self.tokens.get(id) else {
                continue;
            };
            if self.mode == Mode::Metaspace {
                if let Some(byte) = byte_token(token) {
                    bytes.push(byte);
                    continue;
                }
            }
            if self.added.iter().any(|(t, i)| i == id && t == token) {
                bytes.extend_from_slice(token.as_bytes());
                continue;
            }
            match self.mode {
                Mode::ByteLevel => {
                    for c in token.chars() {
                        match self.char_bytes.get(&c) {
                            Some(&b) => bytes.push(b),
                            None => bytes.extend_from_slice(c.to_string().as_bytes()),
                        }
                    }
                }
                Mode::Metaspace => bytes.extend_from_slice(token.replace('▁', " ").as_bytes()),
            }
        }
        let text = String::from_utf8_lossy(&bytes).into_owned();
        if self.mode == Mode::Metaspace && self.prepend != Prepend::Never {
            text.strip_prefix(' ').map(str::to_string).unwrap_or(text)
        } else {
            text
        }
    }
}

/// The byte a `<0xNN>` fallback token stands for.
fn byte_token(token: &str) -> Option<u8> {
    token
        .strip_prefix("<0x")
        .and_then(|t| t.strip_suffix('>'))
        .filter(|h| h.len() == 2)
        .and_then(|h| u8::from_str_radix(h, 16).ok())
}

/// Compact text of a JSON value, used to recognise tokenizer settings.
fn json_text(value: &Json) -> String {
    match value {
        Json::Null => "null".into(),
        Json::Bool(b) => b.to_string(),
        Json::Number(n) => n.to_string(),
        Json::String(s) => {
            let mut out = String::new();
            json::write_string(s, &mut out);
            out
        }
        Json::Array(items) => format!(
            "[{}]",
            items.iter().map(json_text).collect::<Vec<_>>().join(",")
        ),
        Json::Object(entries) => {
            let inner: Vec<String> = entries
                .iter()
                .map(|(k, v)| {
                    let mut key = String::new();
                    json::write_string(k, &mut key);
                    format!("{key}:{}", json_text(v))
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

fn is_letter(c: char) -> bool {
    c.is_alphabetic()
}

fn is_number(c: char) -> bool {
    c.is_numeric()
}

fn is_other(c: char) -> bool {
    !c.is_whitespace() && !is_letter(c) && !is_number(c)
}

/// Splits text into words the way the byte-level pre-tokenizers do.
fn split_words(text: &str, split: Split) -> Vec<&str> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let end_byte = text.len();
    let byte_at = |i: usize| chars.get(i).map(|&(b, _)| b).unwrap_or(end_byte);
    let mut words = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let start = i;
        let c = chars[i].1;

        // Contractions: 's 't 're 've 'm 'll 'd
        if c == '\'' {
            let rest = &text[byte_at(i)..];
            let lower;
            let candidate = if split == Split::Llama3 {
                lower = rest.chars().take(3).collect::<String>().to_lowercase();
                lower.as_str()
            } else {
                rest
            };
            let found = ["'re", "'ve", "'ll", "'s", "'t", "'m", "'d"]
                .iter()
                .find(|s| candidate.starts_with(**s));
            if let Some(s) = found {
                let n = s.chars().count();
                words.push(&text[byte_at(i)..byte_at(i + n)]);
                i += n;
                continue;
            }
        }

        if split == Split::Llama3 {
            // [^\r\n\p{L}\p{N}]?\p{L}+ : one optional non-letter, then letters.
            let lead = !matches!(c, '\r' | '\n') && !is_letter(c) && !is_number(c);
            let letters_from = if lead { i + 1 } else { i };
            if chars.get(letters_from).is_some_and(|&(_, l)| is_letter(l)) {
                let mut j = letters_from;
                while chars.get(j).is_some_and(|&(_, l)| is_letter(l)) {
                    j += 1;
                }
                words.push(&text[byte_at(start)..byte_at(j)]);
                i = j;
                continue;
            }
            // \p{N}{1,3}
            if is_number(c) {
                let mut j = i;
                while j < i + 3 && chars.get(j).is_some_and(|&(_, d)| is_number(d)) {
                    j += 1;
                }
                words.push(&text[byte_at(i)..byte_at(j)]);
                i = j;
                continue;
            }
            // ` ?[^\s\p{L}\p{N}]+[\r\n]*`
            let other_from = if c == ' ' { i + 1 } else { i };
            if chars.get(other_from).is_some_and(|&(_, o)| is_other(o)) {
                let mut j = other_from;
                while chars.get(j).is_some_and(|&(_, o)| is_other(o)) {
                    j += 1;
                }
                while chars.get(j).is_some_and(|&(_, o)| matches!(o, '\r' | '\n')) {
                    j += 1;
                }
                words.push(&text[byte_at(start)..byte_at(j)]);
                i = j;
                continue;
            }
            // `\s*[\r\n]+`
            if c.is_whitespace() {
                let mut j = i;
                while chars.get(j).is_some_and(|&(_, w)| w.is_whitespace()) {
                    j += 1;
                }
                let last_newline = (i..j).rev().find(|&k| matches!(chars[k].1, '\r' | '\n'));
                if let Some(k) = last_newline {
                    words.push(&text[byte_at(i)..byte_at(k + 1)]);
                    i = k + 1;
                    continue;
                }
            }
        } else {
            // ` ?\p{L}+`, ` ?\p{N}+`, ` ?[^\s\p{L}\p{N}]+`
            let body = if c == ' ' { i + 1 } else { i };
            if let Some(&(_, first)) = chars.get(body) {
                let class: Option<fn(char) -> bool> = if is_letter(first) {
                    Some(is_letter)
                } else if is_number(first) {
                    Some(is_number)
                } else if is_other(first) {
                    Some(is_other)
                } else {
                    None
                };
                if let Some(class) = class {
                    let mut j = body;
                    while chars.get(j).is_some_and(|&(_, x)| class(x)) {
                        j += 1;
                    }
                    words.push(&text[byte_at(start)..byte_at(j)]);
                    i = j;
                    continue;
                }
            }
        }

        // `\s+(?!\S)|\s+`: whitespace, leaving the last space for the next
        // word when one follows.
        let mut j = i;
        while chars.get(j).is_some_and(|&(_, w)| w.is_whitespace()) {
            j += 1;
        }
        if j == i {
            // Not reachable for valid classes; take one character.
            j = i + 1;
        } else if j < chars.len() && j - i > 1 {
            j -= 1;
        }
        words.push(&text[byte_at(i)..byte_at(j)]);
        i = j;
    }
    words
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpt2_word_splitting() {
        assert_eq!(
            split_words("Hello world, it's 2024!  Ok\n\tgo  ", Split::Gpt2),
            [
                "Hello", " world", ",", " it", "'s", " 2024", "!", " ", " Ok", "\n", "\t", "go",
                "  "
            ]
        );
    }

    #[test]
    fn llama3_word_splitting() {
        assert_eq!(
            split_words("I'LL pay 12345 now!\n\nYes", Split::Llama3),
            ["I", "'LL", " pay", " ", "123", "45", " now", "!\n\n", "Yes"]
        );
    }

    #[test]
    fn byte_map_is_a_bijection() {
        let chars = byte_chars();
        let unique: std::collections::HashSet<char> = chars.iter().copied().collect();
        assert_eq!(unique.len(), 256);
        assert_eq!(chars[b' ' as usize], 'Ġ');
        assert_eq!(chars[b'A' as usize], 'A');
    }
}
