//! The course in docs/course, checked the way a learner meets it:
//!
//! - every `vyrn` example prints exactly the `text` block after it (or
//!   ends in the error that block shows);
//! - every `vyrn check` block (windows, the network) compiles;
//! - every exercise's starter compiles, and its solution, given the
//!   exercise's input, prints what `vyrn learn check` expects;
//! - every day file is built into `vyrn learn`.

use vyrn::learn::{self, Block};
use vyrn::vm::{compile_source, run_source, run_source_with_input, RunError};

fn run_example(name: &str, block: &Block, next: Option<&Block>, failures: &mut Vec<String>) {
    let line = block.line;
    let Some(next) = next.filter(|n| n.tag == "text") else {
        failures.push(format!(
            "{name}:{line}: an example must be followed by a `text` block with its output (or be tagged `vyrn check`)"
        ));
        return;
    };
    match run_source(&block.text) {
        Ok(out) if out == next.text => {}
        Ok(out) => failures.push(format!(
            "{name}:{line}: printed\n{out}\nbut the page shows\n{}",
            next.text
        )),
        Err(RunError::Compile(e)) => {
            let report: String = e.iter().map(|d| d.render(name, &block.text)).collect();
            failures.push(format!("{name}:{line}: does not compile:\n{report}"))
        }
        Err(RunError::Runtime(e)) => {
            // An example may end in a failure on purpose; the page then
            // shows the error after the output.
            let shown = format!("error: {}: {}\n", e.kind, e.message);
            if !next.text.ends_with(&shown) {
                failures.push(format!("{name}:{line}: runtime error {e}"));
            }
        }
    }
}

fn compiles(name: &str, block: &Block, failures: &mut Vec<String>) {
    if let Err(e) = compile_source(&block.text) {
        let report: String = e.iter().map(|d| d.render(name, &block.text)).collect();
        failures.push(format!(
            "{name}:{}: does not compile:\n{report}",
            block.line
        ));
    }
}

#[test]
fn every_lesson_runs_as_shown() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/course");
    let mut pages: Vec<_> = std::fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "md"))
        .collect();
    pages.sort();
    // Programs write files (notes, databases, pictures): keep them out of
    // the repository.
    let work = std::env::temp_dir().join(format!("vyrn-course-{}", std::process::id()));
    std::fs::create_dir_all(&work).unwrap();
    std::env::set_current_dir(&work).unwrap();
    let mut failures = Vec::new();
    let (mut examples, mut exercises, mut day_files) = (0, 0, 0);
    for page in &pages {
        let text = std::fs::read_to_string(page).unwrap();
        let name = page.file_name().unwrap().to_string_lossy().to_string();
        if name.starts_with("day") {
            day_files += 1;
            let stem = name.trim_end_matches(".md");
            if !learn::DAYS.iter().any(|(d, _)| *d == stem) {
                failures.push(format!(
                    "{name}: not listed in learn::DAYS, so `vyrn learn` cannot show it"
                ));
            }
        }
        let blocks = learn::blocks(&text);
        for (i, block) in blocks.iter().enumerate() {
            match block.tag.as_str() {
                "vyrn" => {
                    run_example(&name, block, blocks.get(i + 1), &mut failures);
                    examples += 1;
                }
                "vyrn check" | "vyrn starter" => compiles(&name, block, &mut failures),
                _ => {}
            }
        }
        if !name.starts_with("day") {
            continue;
        }
        let day = learn::parse_day(&text);
        if day.lessons.is_empty() {
            failures.push(format!("{name}: no lessons (`## N.M Title`)"));
        }
        for lesson in &day.lessons {
            let Some(ex) = &lesson.exercise else { continue };
            exercises += 1;
            if ex.solution.is_empty() {
                failures.push(format!(
                    "{name}: lesson {} has no `vyrn solution`",
                    lesson.id
                ));
                continue;
            }
            match run_source_with_input(&ex.solution, &ex.input) {
                Ok(out) if learn::same_output(&out, &ex.expect) => {}
                Ok(out) => failures.push(format!(
                    "{name}: lesson {}'s solution printed\n{out}\nbut the exercise expects\n{}",
                    lesson.id, ex.expect
                )),
                Err(e) => failures.push(format!(
                    "{name}: lesson {}'s solution fails: {e:?}",
                    lesson.id
                )),
            }
        }
    }
    let _ = std::fs::remove_dir_all(&work);
    assert!(
        failures.is_empty(),
        "{} problems:\n\n{}",
        failures.len(),
        failures.join("\n\n")
    );
    assert_eq!(
        day_files,
        learn::DAYS.len(),
        "learn::DAYS lists a day with no file"
    );
    eprintln!("course: {examples} examples, {exercises} exercises");
}
