//! `vyrn lex <file>` prints the token stream of a source file.
//!
//! This is a development aid for the front end. Other subcommands will be
//! added as later compiler phases are written.

use std::process::ExitCode;
use vyrn::Lexer;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, file] = args.as_slice() else {
        eprintln!("usage: vyrn lex <file.vyrn>");
        return ExitCode::from(2);
    };
    if command != "lex" {
        eprintln!("unknown command `{command}`\nusage: vyrn lex <file.vyrn>");
        return ExitCode::from(2);
    }
    let src = match std::fs::read_to_string(file) {
        Ok(src) => src,
        Err(err) => {
            eprintln!("cannot read {file}: {err}");
            return ExitCode::FAILURE;
        }
    };

    let (tokens, errors) = Lexer::new(&src).run();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("{}", error.render(file, &src));
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
