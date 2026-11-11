//! `vyrn learn`: the course, in the terminal.
//!
//! The 100-day course in `docs/course/` is built into the toolchain. Each
//! day is a file of short lessons; each lesson explains one idea, shows it
//! running, and ends with an exercise: a starter file to finish, the output
//! it should print, hints, and a solution.
//!
//! ```text
//! vyrn learn              the days, and how far you are
//! vyrn learn 3            the lessons of day 3
//! vyrn learn 3.2          read lesson 3.2 and get its exercise file
//! vyrn learn check 3.2    run your answer and compare it with the goal
//! vyrn learn hint 3.2     a nudge, one more each time
//! vyrn learn solution 3.2 one way to solve it
//! vyrn learn next         the first lesson you have not finished
//! ```
//!
//! Exercise files and progress live in `vyrn-course/` in the current
//! directory, so a learner can open them in any editor.
//!
//! # Lesson format
//!
//! A day file starts with `# Day N: Title`. Each `## N.M Title` heading
//! starts a lesson. Fenced blocks are tagged:
//!
//! - `vyrn` and `text`: an example and the output it prints.
//! - `vyrn starter`: the exercise file a learner starts from
//!   (`vyrn starter broken` when it has mistakes to find on purpose).
//! - `text input`: what the exercise program reads with `input()`.
//! - `text expect`: what a correct answer prints.
//! - `vyrn solution`: one correct answer.
//!
//! Lines starting `> Hint:` are hints, given one at a time.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

/// Every day of the course, in order.
pub const DAYS: &[(&str, &str)] = &[
    ("day01", include_str!("../docs/course/day01.md")),
    ("day02", include_str!("../docs/course/day02.md")),
    ("day03", include_str!("../docs/course/day03.md")),
    ("day04", include_str!("../docs/course/day04.md")),
    ("day05", include_str!("../docs/course/day05.md")),
    ("day06", include_str!("../docs/course/day06.md")),
    ("day07", include_str!("../docs/course/day07.md")),
    ("day08", include_str!("../docs/course/day08.md")),
    ("day09", include_str!("../docs/course/day09.md")),
    ("day10", include_str!("../docs/course/day10.md")),
    ("day11", include_str!("../docs/course/day11.md")),
    ("day12", include_str!("../docs/course/day12.md")),
    ("day13", include_str!("../docs/course/day13.md")),
    ("day14", include_str!("../docs/course/day14.md")),
    ("day15", include_str!("../docs/course/day15.md")),
    ("day16", include_str!("../docs/course/day16.md")),
    ("day17", include_str!("../docs/course/day17.md")),
    ("day18", include_str!("../docs/course/day18.md")),
    ("day19", include_str!("../docs/course/day19.md")),
    ("day20", include_str!("../docs/course/day20.md")),
    ("day21", include_str!("../docs/course/day21.md")),
];

/// One fenced block: its tag (`vyrn`, `vyrn starter`, `text expect`, ...),
/// its text, and the line it starts on.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub tag: String,
    pub text: String,
    pub line: usize,
}

/// A lesson's exercise.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Exercise {
    pub starter: String,
    pub input: String,
    pub expect: String,
    pub solution: String,
    pub hints: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Lesson {
    /// `"3.2"`.
    pub id: String,
    pub title: String,
    /// The lesson as written, without the solution.
    pub text: String,
    pub blocks: Vec<Block>,
    pub exercise: Option<Exercise>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Day {
    pub number: usize,
    pub title: String,
    pub intro: String,
    pub lessons: Vec<Lesson>,
}

/// The fenced blocks in markdown text. Indented fences (inside lists or
/// `<details>`) count too.
pub fn blocks(text: &str) -> Vec<Block> {
    let mut out = Vec::new();
    let mut open: Option<(String, usize, String, usize)> = None;
    for (n, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        let indent = line.len() - trimmed.len();
        match &mut open {
            None => {
                if let Some(tag) = trimmed.strip_prefix("```") {
                    open = Some((tag.trim().to_string(), indent, String::new(), n + 1));
                }
            }
            Some((tag, fence_indent, body, start)) => {
                if trimmed.starts_with("```") {
                    out.push(Block {
                        tag: std::mem::take(tag),
                        text: std::mem::take(body),
                        line: *start,
                    });
                    open = None;
                } else {
                    let cut = line
                        .char_indices()
                        .take_while(|(i, c)| *i < *fence_indent && *c == ' ')
                        .count();
                    body.push_str(&line[cut..]);
                    body.push('\n');
                }
            }
        }
    }
    out
}

/// Reads one day's file.
pub fn parse_day(text: &str) -> Day {
    let mut title = String::new();
    let mut number = 0;
    let mut intro = String::new();
    let mut lessons: Vec<(String, String, String)> = Vec::new();
    let mut in_code = false;
    for line in text.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
        }
        let heading = !in_code && !line.trim_start().starts_with("```");
        if let Some(t) = line.strip_prefix("# ").filter(|_| heading) {
            title = t.trim().to_string();
            number = title
                .trim_start_matches("Day ")
                .split(|c: char| !c.is_ascii_digit())
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            continue;
        }
        if let Some(h) = line.strip_prefix("## ").filter(|_| heading) {
            let h = h.trim();
            let (id, rest) = h.split_once(' ').unwrap_or((h, ""));
            lessons.push((id.to_string(), rest.trim().to_string(), String::new()));
            continue;
        }
        match lessons.last_mut() {
            Some((_, _, body)) => {
                body.push_str(line);
                body.push('\n');
            }
            None => {
                intro.push_str(line);
                intro.push('\n');
            }
        }
    }
    Day {
        number,
        title,
        intro: intro.trim().to_string(),
        lessons: lessons
            .into_iter()
            .map(|(id, title, body)| lesson(id, title, &body))
            .collect(),
    }
}

fn lesson(id: String, title: String, body: &str) -> Lesson {
    let blocks = blocks(body);
    let find = |tag: &str| blocks.iter().find(|b| b.tag == tag).map(|b| b.text.clone());
    // `> Hint: ...`, continued on following `> ` lines.
    let mut hints: Vec<String> = Vec::new();
    let mut in_hint = false;
    for l in body.lines() {
        let t = l.trim_start();
        if let Some(h) = t.strip_prefix("> Hint:") {
            hints.push(h.trim().to_string());
            in_hint = true;
        } else if let (true, Some(more)) = (in_hint, t.strip_prefix('>')) {
            if let Some(last) = hints.last_mut() {
                last.push(' ');
                last.push_str(more.trim());
            }
        } else {
            in_hint = false;
        }
    }
    let starter = blocks
        .iter()
        .find(|b| b.tag.starts_with("vyrn starter"))
        .map(|b| b.text.clone());
    let exercise = find("text expect").map(|expect| Exercise {
        starter: starter.unwrap_or_default(),
        input: find("text input").unwrap_or_default(),
        expect,
        solution: find("vyrn solution").unwrap_or_default(),
        hints,
    });
    Lesson {
        id,
        title,
        text: without_solution(body),
        blocks,
        exercise,
    }
}

/// The lesson text with the `<details>` solution and hint lines taken out.
fn without_solution(body: &str) -> String {
    let mut out = String::new();
    let mut hidden = false;
    let mut in_hint = false;
    for line in body.lines() {
        let t = line.trim_start();
        if t.starts_with("<details") {
            hidden = true;
        }
        if t.starts_with("> Hint:") {
            in_hint = true;
        } else if !t.starts_with('>') {
            in_hint = false;
        }
        if !hidden && !in_hint {
            out.push_str(line);
            out.push('\n');
        }
        if t.starts_with("</details>") {
            hidden = false;
        }
    }
    out.trim().to_string()
}

/// All days of the built-in course.
pub fn course() -> Vec<Day> {
    DAYS.iter().map(|(_, text)| parse_day(text)).collect()
}

/// Whether a program's output matches the goal: the same lines, ignoring
/// spaces at line ends and blank lines at the end.
pub fn same_output(got: &str, expect: &str) -> bool {
    let clean = |s: &str| -> Vec<String> {
        let mut lines: Vec<String> = s.lines().map(|l| l.trim_end().to_string()).collect();
        while lines.last().is_some_and(|l| l.is_empty()) {
            lines.pop();
        }
        lines
    };
    clean(got) == clean(expect)
}

// ==== the command ===========================================================

/// Where exercise files and progress are kept.
fn workspace() -> PathBuf {
    PathBuf::from("vyrn-course")
}

fn exercise_path(id: &str) -> PathBuf {
    let (day, lesson) = id.split_once('.').unwrap_or((id, "0"));
    let day: usize = day.parse().unwrap_or(0);
    workspace().join(format!("day{day:02}_{lesson}.vyrn"))
}

fn progress_path() -> PathBuf {
    workspace().join("progress.txt")
}

/// Finished lessons, one id per line.
fn done() -> Vec<String> {
    std::fs::read_to_string(progress_path())
        .unwrap_or_default()
        .lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

fn mark_done(id: &str) {
    let mut all = done();
    if !all.iter().any(|d| d == id) {
        all.push(id.to_string());
    }
    let _ = std::fs::create_dir_all(workspace());
    let _ = std::fs::write(progress_path(), all.join("\n") + "\n");
}

fn hints_used(id: &str) -> usize {
    std::fs::read_to_string(workspace().join(format!(".hints-{id}")))
        .ok()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(0)
}

fn find_lesson<'a>(days: &'a [Day], id: &str) -> Option<&'a Lesson> {
    days.iter().flat_map(|d| &d.lessons).find(|l| l.id == id)
}

const USAGE: &str = "\
vyrn learn              the days, and how far you are
vyrn learn 3            the lessons of day 3
vyrn learn 3.2          read lesson 3.2 and get its exercise file
vyrn learn check 3.2    run your answer and compare it with the goal
vyrn learn hint 3.2     a nudge (ask again for the next one)
vyrn learn solution 3.2 one way to solve it
vyrn learn next         the first lesson you have not finished";

/// Runs `vyrn learn ...`. `run_file` runs a Vyrn file with the given input
/// and gives what it printed (stdout and stderr).
pub fn command(args: &[String], run_file: &dyn Fn(&Path, &str) -> (String, String)) -> i32 {
    let days = course();
    let mut out = String::new();
    let code = match args
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .as_slice()
    {
        [] => {
            overview(&days, &mut out);
            0
        }
        ["help" | "--help" | "-h"] => {
            out.push_str(USAGE);
            out.push('\n');
            0
        }
        ["next"] => {
            let finished = done();
            match days
                .iter()
                .flat_map(|d| &d.lessons)
                .find(|l| !finished.contains(&l.id))
            {
                Some(l) => open_lesson(l, &mut out),
                None => {
                    out.push_str("You have finished every lesson. Congratulations!\n");
                    0
                }
            }
        }
        ["check", id] => with_lesson(&days, id, &mut out, |l, out| check(l, run_file, &days, out)),
        ["hint", id] => with_lesson(&days, id, &mut out, hint),
        ["solution", id] => with_lesson(&days, id, &mut out, solution),
        [id] if !id.contains('.') => match id
            .parse::<usize>()
            .ok()
            .and_then(|n| days.iter().find(|d| d.number == n))
        {
            Some(day) => {
                day_overview(day, &mut out);
                0
            }
            None => {
                let _ = writeln!(out, "There is no day {id} yet. `vyrn learn` lists them.");
                1
            }
        },
        [id] => with_lesson(&days, id, &mut out, open_lesson),
        _ => {
            out.push_str(USAGE);
            out.push('\n');
            2
        }
    };
    print!("{out}");
    code
}

fn with_lesson(
    days: &[Day],
    id: &str,
    out: &mut String,
    f: impl FnOnce(&Lesson, &mut String) -> i32,
) -> i32 {
    match find_lesson(days, id) {
        Some(l) => f(l, out),
        None => {
            let _ = writeln!(
                out,
                "There is no lesson {id}. `vyrn learn` lists the days; `vyrn learn 1` lists day 1's lessons."
            );
            1
        }
    }
}

fn overview(days: &[Day], out: &mut String) {
    let finished = done();
    out.push_str("Learn Vyrn: 100 days of code\n\n");
    for day in days {
        let total = day.lessons.len();
        let n = day
            .lessons
            .iter()
            .filter(|l| finished.contains(&l.id))
            .count();
        let mark = if n == total && total > 0 { "✓" } else { " " };
        let _ = writeln!(out, " {mark} {:<44} {n}/{total}", day.title);
    }
    out.push_str("\n`vyrn learn next` opens the next lesson. `vyrn learn 1` lists day 1.\n");
}

fn day_overview(day: &Day, out: &mut String) {
    let finished = done();
    let _ = writeln!(out, "{}\n", day.title);
    if !day.intro.is_empty() {
        let _ = writeln!(out, "{}\n", day.intro);
    }
    for l in &day.lessons {
        let mark = if finished.contains(&l.id) { "✓" } else { " " };
        let _ = writeln!(out, " {mark} {:<5} {}", l.id, l.title);
    }
    let _ = writeln!(
        out,
        "\n`vyrn learn {}` opens the first lesson.",
        day.lessons.first().map_or("", |l| l.id.as_str())
    );
}

fn open_lesson(lesson: &Lesson, out: &mut String) -> i32 {
    let _ = writeln!(out, "── Lesson {}: {} ──\n", lesson.id, lesson.title);
    out.push_str(&lesson.text);
    out.push_str("\n\n");
    if let Some(ex) = &lesson.exercise {
        let path = exercise_path(&lesson.id);
        if !path.exists() {
            let _ = std::fs::create_dir_all(workspace());
            let starter = if ex.starter.is_empty() {
                "main:\n    # Your code here.\nend\n".to_string()
            } else {
                ex.starter.clone()
            };
            let _ = std::fs::write(&path, starter);
        }
        let _ = writeln!(
            out,
            "Your exercise file is {}. Open it in an editor, write your answer, then run:\n\n    vyrn learn check {}\n\nStuck? `vyrn learn hint {}` gives a nudge.",
            path.display(),
            lesson.id,
            lesson.id
        );
    } else {
        mark_done(&lesson.id);
        out.push_str("No exercise here: when you are ready, `vyrn learn next`.\n");
    }
    0
}

fn check(
    lesson: &Lesson,
    run_file: &dyn Fn(&Path, &str) -> (String, String),
    days: &[Day],
    out: &mut String,
) -> i32 {
    let Some(ex) = &lesson.exercise else {
        let _ = writeln!(out, "Lesson {} has no exercise to check.", lesson.id);
        return 0;
    };
    let path = exercise_path(&lesson.id);
    if !path.exists() {
        let _ = writeln!(
            out,
            "There is no {} yet: `vyrn learn {}` makes it.",
            path.display(),
            lesson.id
        );
        return 1;
    }
    let (got, errors) = run_file(&path, &ex.input);
    if same_output(&got, &ex.expect) && errors.trim().is_empty() {
        mark_done(&lesson.id);
        let _ = writeln!(out, "✓ Correct! Lesson {} is done.", lesson.id);
        let finished = done();
        if let Some(next) = days
            .iter()
            .flat_map(|d| &d.lessons)
            .find(|l| !finished.contains(&l.id))
        {
            let _ = writeln!(out, "\nNext: `vyrn learn {}` ({}).", next.id, next.title);
        }
        return 0;
    }
    let _ = writeln!(out, "✗ Not yet.\n");
    if !errors.trim().is_empty() {
        let _ = writeln!(
            out,
            "Your program stopped with an error:\n\n{}",
            errors.trim_end()
        );
        out.push_str(
            "\nRead the message from the top: it says what went wrong and on which line.\n",
        );
        return 1;
    }
    let want: Vec<&str> = ex.expect.lines().collect();
    let have: Vec<&str> = got.lines().collect();
    let first_diff = (0..want.len().max(have.len()))
        .find(|&i| want.get(i).map(|s| s.trim_end()) != have.get(i).map(|s| s.trim_end()))
        .unwrap_or(0);
    let _ = writeln!(out, "It should print:\n");
    for l in &want {
        let _ = writeln!(out, "    {l}");
    }
    let _ = writeln!(out, "\nIt printed:\n");
    if have.is_empty() {
        out.push_str("    (nothing)\n");
    }
    for l in &have {
        let _ = writeln!(out, "    {l}");
    }
    let _ = writeln!(
        out,
        "\nThe first difference is on line {}: expected {:?}, got {:?}.",
        first_diff + 1,
        want.get(first_diff).copied().unwrap_or("(nothing)"),
        have.get(first_diff).copied().unwrap_or("(nothing)")
    );
    let _ = writeln!(out, "`vyrn learn hint {}` gives a nudge.", lesson.id);
    1
}

fn hint(lesson: &Lesson, out: &mut String) -> i32 {
    let Some(ex) = &lesson.exercise else {
        let _ = writeln!(out, "Lesson {} has no exercise.", lesson.id);
        return 0;
    };
    if ex.hints.is_empty() {
        let _ = writeln!(
            out,
            "No hints for this one. Reread the example in the lesson; `vyrn learn solution {}` shows an answer.",
            lesson.id
        );
        return 0;
    }
    let used = hints_used(&lesson.id);
    let i = used.min(ex.hints.len() - 1);
    let _ = writeln!(out, "Hint {} of {}: {}", i + 1, ex.hints.len(), ex.hints[i]);
    let _ = std::fs::create_dir_all(workspace());
    let _ = std::fs::write(
        workspace().join(format!(".hints-{}", lesson.id)),
        (used + 1).to_string(),
    );
    if i + 1 == ex.hints.len() {
        let _ = writeln!(
            out,
            "\nThat was the last hint. `vyrn learn solution {}` shows an answer.",
            lesson.id
        );
    }
    0
}

fn solution(lesson: &Lesson, out: &mut String) -> i32 {
    match &lesson.exercise {
        Some(ex) if !ex.solution.is_empty() => {
            let _ = writeln!(out, "One way to solve lesson {}:\n", lesson.id);
            for l in ex.solution.lines() {
                let _ = writeln!(out, "    {l}");
            }
            let _ = writeln!(
                out,
                "\nType it in yourself rather than copying: that is how it sticks. Then `vyrn learn check {}`.",
                lesson.id
            );
        }
        _ => {
            let _ = writeln!(out, "Lesson {} has no written solution.", lesson.id);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: &str = "# Day 1: Hello\n\nIntro.\n\n## 1.1 Printing\n\nText.\n\n```vyrn\n# not a heading\nmain:\n    print(1)\nend\n```\n\n```text\n1\n```\n\n### Exercise\n\nPrint 2.\n\n```vyrn starter\nmain:\nend\n```\n\n```text expect\n2\n```\n\n> Hint: use\n> print\n\n<details><summary>Solution</summary>\n\n```vyrn solution\nmain:\n    print(2)\nend\n```\n\n</details>\n\n## 1.2 Next\n\nMore.\n";

    #[test]
    fn days_split_into_lessons_with_exercises() {
        let day = parse_day(DAY);
        assert_eq!(day.number, 1);
        assert_eq!(day.title, "Day 1: Hello");
        assert_eq!(day.lessons.len(), 2);
        let l = &day.lessons[0];
        assert_eq!((l.id.as_str(), l.title.as_str()), ("1.1", "Printing"));
        let ex = l.exercise.as_ref().unwrap();
        assert_eq!(ex.expect, "2\n");
        assert_eq!(ex.solution, "main:\n    print(2)\nend\n");
        assert_eq!(ex.hints, ["use print"]);
        assert!(!l.text.contains("print(2)") && !l.text.contains("Hint"));
        assert!(day.lessons[1].exercise.is_none());
    }

    #[test]
    fn output_comparison_ignores_trailing_space() {
        assert!(same_output("a \nb\n\n", "a\nb"));
        assert!(!same_output("a\nc\n", "a\nb\n"));
    }
}
