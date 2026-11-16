//! The tokenizer must match Hugging Face `tokenizers` exactly. The fixtures
//! were produced by `tools/make_tokenizer_fixtures.py` with the reference
//! implementation.

use vyrn::vm::json::{self, Json};
use vyrn::vm::tokenizer::Tokenizer;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/tokenizers");

fn check(family: &str) {
    let tokenizer = Tokenizer::from_file(&format!("{DIR}/{family}.json")).unwrap();
    let expected = std::fs::read_to_string(format!("{DIR}/expected.json")).unwrap();
    let expected = json::parse(&expected).unwrap();
    let cases = expected.get(family).and_then(Json::as_array).unwrap();
    let mut failures = Vec::new();
    for case in cases {
        let text = case.get("text").and_then(Json::as_str).unwrap();
        let ids: Vec<u32> = case
            .get("ids")
            .and_then(Json::as_array)
            .unwrap()
            .iter()
            .map(|id| id.as_u64().unwrap() as u32)
            .collect();
        let decoded = case.get("decoded").and_then(Json::as_str).unwrap();
        let ours = tokenizer.encode(text);
        if ours != ids {
            failures.push(format!(
                "encode {text:?}\n  ours: {ours:?}\n  hf:   {ids:?}"
            ));
        }
        let back = tokenizer.decode(&ids);
        if back != decoded {
            failures.push(format!(
                "decode {text:?}\n  ours: {back:?}\n  hf:   {decoded:?}"
            ));
        }
    }
    assert!(failures.is_empty(), "{family}:\n{}", failures.join("\n"));
}

#[test]
fn gpt2_style_matches_hugging_face() {
    check("gpt2");
}

#[test]
fn llama3_style_matches_hugging_face() {
    check("llama3");
}

#[test]
fn sentencepiece_style_matches_hugging_face() {
    check("spm");
}
