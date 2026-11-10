//! Every program in `examples/` and `tests/fixtures/` must tokenize cleanly.

use std::path::{Path, PathBuf};
use vyrn::{tokenize, Keyword, Punct, TokenKind};

fn sources() -> Vec<PathBuf> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for dir in ["examples", "tests/fixtures"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|ext| ext == "vyrn") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn lex(path: &Path) -> Vec<TokenKind> {
    let src = std::fs::read_to_string(path).unwrap();
    match tokenize(&src) {
        Ok(tokens) => tokens.into_iter().map(|t| t.kind).collect(),
        Err(errors) => {
            let name = path.display().to_string();
            let report: String = errors.iter().map(|e| e.render(&name, &src)).collect();
            panic!("{report}");
        }
    }
}

fn count(kinds: &[TokenKind], wanted: &TokenKind) -> usize {
    kinds.iter().filter(|k| *k == wanted).count()
}

#[test]
fn all_sources_tokenize() {
    let files = sources();
    assert!(
        files.len() >= 5,
        "expected the examples and the fixture, found {files:?}"
    );
    for path in files {
        let kinds = lex(&path);
        assert_eq!(kinds.last(), Some(&TokenKind::Eof), "{}", path.display());
        assert_ne!(
            kinds.first(),
            Some(&TokenKind::Newline),
            "{}",
            path.display()
        );
        for pair in kinds.windows(2) {
            assert!(
                !(pair[0] == TokenKind::Newline && pair[1] == TokenKind::Newline),
                "two newlines in a row in {}",
                path.display()
            );
        }
    }
}

/// Block openers and `end` must balance in the example programs. This is a
/// cheap stand-in for parsing until the parser exists.
#[test]
fn examples_have_one_end_per_block() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    for (file, ends) in [
        ("grade.vyrn", 5),
        ("game.vyrn", 8),
        ("api.vyrn", 6),
        ("inference.vyrn", 5),
    ] {
        let kinds = lex(&root.join(file));
        assert_eq!(
            count(&kinds, &TokenKind::Keyword(Keyword::End)),
            ends,
            "{file}"
        );
    }
}

#[test]
fn inference_example_uses_both_products_and_placement() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let kinds = lex(&root.join("inference.vyrn"));
    assert_eq!(count(&kinds, &TokenKind::Punct(Punct::At)), 3);
    assert_eq!(count(&kinds, &TokenKind::Punct(Punct::Star)), 3);
    assert_eq!(count(&kinds, &TokenKind::Punct(Punct::Apostrophe)), 1);
    assert_eq!(count(&kinds, &TokenKind::Keyword(Keyword::On)), 3);
}

#[test]
fn game_example_places_scratch_data_on_the_frame_region() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
    let kinds = lex(&root.join("game.vyrn"));
    let on = kinds
        .iter()
        .position(|k| *k == TokenKind::Keyword(Keyword::On))
        .unwrap();
    assert_eq!(kinds[on + 1], TokenKind::Ident("frame".to_string()));
    // `in` appears only in the `for` loop.
    assert_eq!(count(&kinds, &TokenKind::Keyword(Keyword::In)), 1);
}
