//! The bootcamp in docs/course: every program followed by a `text` block
//! must print exactly that, and every `vyrn check` block (windows, the
//! network, the keyboard) must parse, type-check and compile.

use vyrn::vm::{compile_source, run_source, RunError};

#[test]
fn every_course_example_runs_as_shown() {
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
    let (mut ran, mut checked) = (0, 0);
    let mut failures = Vec::new();
    for page in &pages {
        let text = std::fs::read_to_string(page).unwrap();
        let name = page.file_name().unwrap().to_string_lossy().to_string();
        let mut blocks: Vec<(String, String, usize)> = Vec::new();
        let mut lines = text.lines().enumerate();
        while let Some((n, line)) = lines.next() {
            if let Some(info) = line.trim_start().strip_prefix("```") {
                let indent = line.len() - line.trim_start().len();
                let body: Vec<String> = lines
                    .by_ref()
                    .take_while(|(_, l)| l.trim() != "```")
                    .map(|(_, l)| l.get(indent..).unwrap_or(l.trim_start()).to_string())
                    .collect();
                blocks.push((info.trim().to_string(), body.join("\n") + "\n", n + 1));
            }
        }
        for (i, (info, body, line)) in blocks.iter().enumerate() {
            match info.as_str() {
                "vyrn" => {
                    let Some((next, expected, _)) = blocks.get(i + 1) else {
                        failures.push(format!("{name}:{line}: a program without its output"));
                        continue;
                    };
                    if next != "text" {
                        failures.push(format!("{name}:{line}: a program must be followed by its output (or be a `vyrn check` block)"));
                        continue;
                    }
                    match run_source(body) {
                        Ok(out) if &out == expected => {}
                        Ok(out) => failures.push(format!(
                            "{name}:{line}: printed\n{out}\nbut the page shows\n{expected}"
                        )),
                        Err(RunError::Compile(e)) => {
                            failures.push(format!("{name}:{line}: compile errors {e:?}"))
                        }
                        Err(RunError::Runtime(e)) => {
                            // A program may end in a failure on purpose; the
                            // page then shows the error after the output.
                            let shown = format!("error: {}: {}\n", e.kind, e.message);
                            if !expected.ends_with(&shown) {
                                failures.push(format!("{name}:{line}: runtime error {e}"));
                            }
                        }
                    }
                    ran += 1;
                }
                "vyrn check" => {
                    if let Err(e) = compile_source(body) {
                        let report: String = e.iter().map(|d| d.render(&name, body)).collect();
                        failures.push(format!("{name}:{line}: does not compile:\n{report}"));
                    }
                    checked += 1;
                }
                _ => {}
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
    assert!(ran + checked > 0, "no examples found");
    eprintln!("course: {ran} programs run, {checked} checked");
}
