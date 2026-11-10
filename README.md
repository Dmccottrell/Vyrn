# Vyrn

A gradually typed, garbage-collector-free language for machine learning, web
services and WebAssembly, and games and systems code.

Status: grammar v0.4, with a lexer, parser and bytecode interpreter in Rust.
The type checker, borrow checker and native backends do not exist yet.

## Documents

- [`docs/syntax.md`](docs/syntax.md): the syntax at a glance, and how it maps from Python
- [`docs/grammar.ebnf`](docs/grammar.ebnf): the complete EBNF grammar (v0.4)
- [`docs/architecture.md`](docs/architecture.md): compiler tiers, memory model, GPU bridge and concurrency design
- [`docs/stdlib.md`](docs/stdlib.md): the standard library: the Prelude, `std.http`, `std.fs`, `std.task` and `std.nn`
- [`docs/principles.md`](docs/principles.md): the five rules every feature is checked against, and what Vyrn takes from each language
- [`docs/ffi.md`](docs/ffi.md): the Python and C bridges, zero-copy tensors, and the decentralized module resolver

## Examples

- [`examples/grade.vyrn`](examples/grade.vyrn): beginner, a grade calculator
- [`examples/game.vyrn`](examples/game.vyrn): game dev, a fixed-size entity pool and a per-frame memory region
- [`examples/api.vyrn`](examples/api.vyrn): web, async routes returning JSON
- [`examples/inference.vyrn`](examples/inference.vyrn): ML, tensor layers spread across GPUs

## Getting started

```bash
cargo install --path .
```

```bash
vyrn new hello
```

```bash
cd hello && vyrn run
```

| Command | Does |
|---|---|
| `vyrn new <name>` | Creates a project: `vyrn.toml`, `main.vyrn`, a module and a test |
| `vyrn run [file]` | Runs the project (or one file) |
| `vyrn check [file]` | Finds type, `let`, `strict` and borrow errors without running |
| `vyrn test` | Runs every `fn test_...()` in `tests/` |
| `vyrn fmt [--check]` | Formats every file in the one Vyrn style |
| `vyrn add github.com/ann/neural@v1` | Fetches a module and locks its exact commit and content hash in `vyrn.lock` |
| `vyrn fetch` | Downloads every locked module, to work offline |
| `vyrn build` | Makes one executable file that runs anywhere, with nothing to install |

Files of a project use each other with `use geo.shapes` (loads `geo/shapes.vyrn`),
and modules from any git host with `use "github.com/ann/neural@v1"`. There is no
central registry: `docs/ffi.md` describes the resolver.

## Compiler

```bash
cargo test
```

To run `gpu[...]` blocks on a real GPU (Vulkan, Metal or DirectX 12):

```bash
cargo run --release --features gpu -- run program.vyrn
```

| Module | Contents |
|---|---|
| `src/span.rs` | Line, column and byte-offset positions |
| `src/token.rs` | Token, keyword and operator definitions |
| `src/lexer.rs` | Source text to tokens, with error recovery and rendered diagnostics |
| `src/ast.rs` | Syntax tree, with empty type and placement slots for semantic analysis |
| `src/parser.rs` | Recursive-descent parser with error recovery and hints |
| `src/pretty.rs` | Compact text form of the syntax tree |
| `src/check.rs` | Gradual type checks, `let`/`strict` rules and borrow checks (`vyrn check <file>`) |
| `src/fmt.rs` | The formatter (`vyrn fmt`) |
| `src/project/` | Multi-file programs, the URL module resolver, `vyrn.lock`, single-file builds |
| `src/vm/` | Bytecode compiler, register VM, tensor backends and builtins (see `docs/architecture.md` section 2.4) |

Only `examples/grade.vyrn` runs as is: the other examples call functions
from packages that do not exist yet (`load_level`, `load_model`, `app.storage`).
