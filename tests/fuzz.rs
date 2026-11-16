//! Fuzzing the front end: every program in the course, damaged in many
//! random ways (lines dropped, duplicated, swapped or cut short, tokens
//! inserted, characters changed), must get diagnostics or compile, never a
//! panic. Beginners type broken code all day; the parser and checker must
//! always answer with an error message.
//!
//! The randomness is seeded, so a failure is reproducible. `VYRN_FUZZ=N`
//! runs N mutations per program instead of the default (a few thousand in
//! all), for a longer hunt.

use std::panic::{catch_unwind, AssertUnwindSafe};

use vyrn::learn::course;
use vyrn::vm::compile_source;

/// xorshift64*: small, fast, and good enough to pick mutations.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// Fragments a beginner (or a cat on the keyboard) might add.
const TOKENS: &[&str] = &[
    "end",
    "main:",
    "create",
    "(",
    ")",
    "[",
    "]",
    "{",
    "}",
    ":",
    ",",
    ".",
    "=",
    "==",
    "->",
    "=>",
    "\"",
    "'",
    "\"\"\"",
    "#",
    "{x}",
    "let",
    "var",
    "if",
    "else",
    "else if",
    "each",
    "in",
    "loop",
    "when",
    "is",
    "return",
    "attempt:",
    "catch",
    "always:",
    "struct",
    "object",
    "enum",
    "trait",
    "impl",
    "self",
    "none",
    "true",
    "1.5e",
    "0x",
    "-",
    "*",
    "**",
    "/",
    "//",
    "%",
    "...",
    "?",
    "??",
    "async",
    "await",
    "yield",
    "use",
    "const",
    "    ",
    "\t",
    "\n",
    "\\",
    "@",
    "$",
    "é",
    "🙂",
    "0",
    "99999999999999999999999",
    "[x each x in",
    "f\"",
    "r\"",
    "<<",
    ">>",
    "&",
    "|",
    "not",
    "and",
];

fn mutate(src: &str, rng: &mut Rng) -> String {
    let mut lines: Vec<String> = src.lines().map(String::from).collect();
    if lines.is_empty() {
        lines.push(String::new());
    }
    for _ in 0..1 + rng.below(3) {
        let n = lines.len();
        let i = rng.below(n);
        match rng.below(9) {
            0 => {
                if n > 1 {
                    lines.remove(i);
                }
            }
            1 => {
                let l = lines[i].clone();
                lines.insert(i, l);
            }
            2 => {
                let j = rng.below(n);
                lines.swap(i, j);
            }
            3 => {
                let l = &lines[i];
                let cut = rng.below(l.len() + 1);
                let cut = (0..=cut)
                    .rev()
                    .find(|&c| l.is_char_boundary(c))
                    .unwrap_or(0);
                lines[i] = l[..cut].to_string();
            }
            4 => {
                let t = TOKENS[rng.below(TOKENS.len())];
                let l = &lines[i];
                let at = rng.below(l.len() + 1);
                let at = (0..=at).rev().find(|&c| l.is_char_boundary(c)).unwrap_or(0);
                lines[i] = format!("{}{}{}", &l[..at], t, &l[at..]);
            }
            5 => {
                // Indentation goes wrong.
                lines[i] = if rng.below(2) == 0 {
                    format!("    {}", lines[i])
                } else {
                    lines[i].trim_start().to_string()
                };
            }
            6 => {
                // One character changes.
                let chars: Vec<char> = lines[i].chars().collect();
                if !chars.is_empty() {
                    let k = rng.below(chars.len());
                    let pool = "()[]{}:\"'=,.+-*/#<>!?x0 \t";
                    let c = pool.chars().nth(rng.below(pool.chars().count())).unwrap();
                    lines[i] = chars
                        .iter()
                        .enumerate()
                        .map(|(j, &ch)| if j == k { c } else { ch })
                        .collect();
                }
            }
            7 => {
                // A line from elsewhere in the program.
                let j = rng.below(n);
                let l = lines[j].clone();
                lines.insert(i, l);
            }
            _ => {
                // The program stops part way through.
                lines.truncate(i.max(1));
            }
        }
    }
    let mut out = lines.join("\n");
    if rng.below(2) == 0 {
        out.push('\n');
    }
    out
}

#[test]
fn damaged_programs_get_errors_not_panics() {
    let per_program: usize = std::env::var("VYRN_FUZZ")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4);
    let mut programs: Vec<String> = Vec::new();
    for day in course() {
        for lesson in &day.lessons {
            for b in &lesson.blocks {
                if b.tag.starts_with("vyrn") {
                    programs.push(b.text.clone());
                }
            }
        }
    }
    assert!(programs.len() > 300, "the course has hundreds of programs");

    // Quiet the default panic message; failures are collected and shown.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let mut rng = Rng(0x5eed_1234_abcd_0042);
    let mut failures: Vec<(String, String)> = Vec::new();
    let mut tried = 0;
    for src in &programs {
        for _ in 0..per_program {
            let damaged = mutate(src, &mut rng);
            tried += 1;
            if let Err(e) = catch_unwind(AssertUnwindSafe(|| {
                let _ = compile_source(&damaged);
            })) {
                let why = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                if failures.len() < 5 {
                    failures.push((damaged, why));
                }
            }
        }
    }
    std::panic::set_hook(hook);
    assert!(
        failures.is_empty(),
        "{} of {tried} damaged programs crashed the compiler; first ones:\n{}",
        failures.len(),
        failures
            .iter()
            .map(|(src, why)| format!("--- panic: {why}\n{src}\n"))
            .collect::<String>()
    );
}
