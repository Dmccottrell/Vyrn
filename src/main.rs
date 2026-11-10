//! The `vyrn` command-line tool.
//!
//! - `vyrn lex <file>` prints the token stream.
//! - `vyrn parse <file>` prints the syntax tree in compact form.

use std::process::ExitCode;
use vyrn::{parse_recovering, pretty, Lexer};

const USAGE: &str = "usage: vyrn <lex|parse> <file.vyrn>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, file] = args.as_slice() else {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    };
    let src = match std::fs::read_to_string(file) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("cannot read {file}: {err}");
            return ExitCode::FAILURE;
        }
    };
    match command.as_str() {
        "lex" => lex(file, &src),
        "parse" => parse(file, &src),
        _ => {
            eprintln!("unknown command `{command}`\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn lex(file: &str, src: &str) -> ExitCode {
    let (tokens, errors) = Lexer::new(src).run();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("{}", error.render(file, src));
        }
        return ExitCode::FAILURE;
    }
    for token in &tokens {
        println!(
            "{:>4}:{:<3} {:?}",
            token.span.start.line, token.span.start.column, token.kind
        );
    }
    ExitCode::SUCCESS
}

fn parse(file: &str, src: &str) -> ExitCode {
    let (module, diagnostics) = parse_recovering(src);
    if !diagnostics.is_empty() {
        for diagnostic in &diagnostics {
            eprintln!("{}", diagnostic.render(file, src));
        }
        return ExitCode::FAILURE;
    }
    print!("{}", pretty::module(&module));
    ExitCode::SUCCESS
}
