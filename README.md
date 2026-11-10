# Vyrn

A gradually typed, garbage-collector-free language for machine learning, web
services and WebAssembly, and games and systems code.

Status: grammar v0.3, with a lexer and AST definitions in Rust. There is no
parser yet.

## Documents

- [`docs/grammar.ebnf`](docs/grammar.ebnf): the complete EBNF grammar (v0.3)
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
cargo run -- lex examples/inference.vyrn
```

| Module | Contents |
|---|---|
| `src/span.rs` | Line, column and byte-offset positions |
| `src/token.rs` | Token, keyword and operator definitions |
| `src/lexer.rs` | Source text to tokens, with error recovery and rendered diagnostics |
| `src/ast.rs` | Syntax tree, with empty type and placement slots for semantic analysis |
