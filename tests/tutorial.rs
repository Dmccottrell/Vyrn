//! Every example in docs/tutorial.md: programs followed by their output
//! must print exactly that output, and `vyrn check` blocks must compile.

use vyrn::vm::{compile_source, run_source, RunError};

#[test]
fn tutorial_examples_print_what_the_page_says() {
    let page =
        std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/docs/tutorial.md")).unwrap();
    let mut blocks: Vec<(String, String)> = Vec::new();
    let mut lines = page.lines();
    while let Some(line) = lines.next() {
        if let Some(info) = line.strip_prefix("```") {
            let body: Vec<&str> = lines.by_ref().take_while(|l| *l != "```").collect();
            blocks.push((info.to_string(), body.join("\n") + "\n"));
        }
    }
    let (mut ran, mut checked) = (0, 0);
    for (i, (info, body)) in blocks.iter().enumerate() {
        match info.as_str() {
            "vyrn" => {
                let (next_info, expected) = blocks.get(i + 1).expect("an output block");
                assert_eq!(
                    next_info, "text",
                    "a program in the tutorial is followed by its output"
                );
                match run_source(body) {
                    Ok(out) => assert_eq!(&out, expected, "for:\n{body}"),
                    Err(RunError::Compile(e)) => panic!("{body}\ncompile errors: {e:?}"),
                    Err(RunError::Runtime(e)) => panic!("{body}\nruntime error: {e}"),
                }
                ran += 1;
            }
            "vyrn check" => {
                // Programs that use other files or serve forever: they must
                // parse and check; a missing module is expected.
                let module = vyrn::parse(body).unwrap_or_else(|e| panic!("{body}\n{e:?}"));
                let problems = vyrn::check::check(&module);
                assert!(problems.is_empty(), "{body}\n{problems:?}");
                if !body.contains("use greeting") {
                    compile_source(body).unwrap_or_else(|e| panic!("{body}\n{e:?}"));
                }
                checked += 1;
            }
            _ => {}
        }
    }
    assert_eq!((ran, checked), (8, 2));
}
