//! The `vyrn` command-line tool.
//!
//! - `vyrn lex <file>` prints the token stream.
//! - `vyrn parse <file>` prints the syntax tree in compact form.
//! - `vyrn bytecode <file>` prints the compiled bytecode.
//! - `vyrn run <file>` compiles the file and runs its `main:` block.

use std::process::ExitCode;
use vyrn::vm::{self, Vm};
use vyrn::{parse_recovering, pretty, Lexer};

const USAGE: &str = "usage: vyrn <run|bytecode|parse|lex> <file.vyrn>";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [command, file, rest @ ..] = args.as_slice() else {
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
        "bytecode" => bytecode(file, &src),
        "run" => run(file, &src, rest),
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

fn compile(file: &str, src: &str) -> Option<vm::Program> {
    match vm::compile_source(src) {
        Ok(program) => Some(program),
        Err(diagnostics) => {
            for diagnostic in &diagnostics {
                eprintln!("{}", diagnostic.render(file, src));
            }
            None
        }
    }
}

fn bytecode(file: &str, src: &str) -> ExitCode {
    match compile(file, src) {
        Some(program) => {
            print!("{}", program.disassemble());
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}

fn run(file: &str, src: &str, args: &[String]) -> ExitCode {
    let Some(program) = compile(file, src) else {
        return ExitCode::FAILURE;
    };
    let mut machine = Vm::new(program);
    let result = machine.run_main(args);
    for warning in vyrn::vm::task::take_unobserved() {
        eprintln!("warning: {warning}");
    }
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}
