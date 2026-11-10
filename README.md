# Vyrn

A gradually typed, garbage-collector-free language for machine learning, web
services and WebAssembly, and games and systems code.

Status: grammar v0.4, with a lexer, parser and bytecode interpreter in Rust.
The type checker, borrow checker and native backends do not exist yet.

## Documents

- [`docs/syntax.md`](docs/syntax.md): the syntax at a glance, and how it maps from Python
- [`docs/grammar.ebnf`](docs/grammar.ebnf): the complete EBNF grammar (v0.4)
- [`docs/architecture.md`](docs/architecture.md): compiler tiers, memory model, GPU bridge and concurrency design

## Examples

- [`examples/grade.vyrn`](examples/grade.vyrn): beginner, a grade calculator
- [`examples/game.vyrn`](examples/game.vyrn): game dev, a fixed-size entity pool and a per-frame memory region
- [`examples/api.vyrn`](examples/api.vyrn): web, async routes returning JSON
- [`examples/inference.vyrn`](examples/inference.vyrn): ML, tensor layers spread across GPUs

## Compiler

```bash
cargo test
```

```bash
cargo run --release -- run examples/grade.vyrn
```

```bash
cargo run -- bytecode examples/grade.vyrn
```

`vyrn parse` prints the syntax tree and `vyrn lex` the tokens.

| Module | Contents |
|---|---|
| `src/span.rs` | Line, column and byte-offset positions |
| `src/token.rs` | Token, keyword and operator definitions |
| `src/lexer.rs` | Source text to tokens, with error recovery and rendered diagnostics |
| `src/ast.rs` | Syntax tree, with empty type and placement slots for semantic analysis |
| `src/parser.rs` | Recursive-descent parser with error recovery and hints |
| `src/pretty.rs` | Compact text form of the syntax tree |
| `src/vm/` | Bytecode compiler, register VM, tensor backends and builtins (see `docs/architecture.md` section 2.4) |

Only `examples/grade.vyrn` runs as is: the other examples call library
functions (`load_level`, `load_model`, `web.serve`, ...) that do not exist yet.
