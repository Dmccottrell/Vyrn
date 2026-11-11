# Vyrn

A gradually typed, garbage-collector-free language for machine learning, web
services and WebAssembly, and games and systems code.

Status: grammar v0.4, with a lexer, parser and bytecode interpreter in Rust.
The type checker, borrow checker and native backends do not exist yet.

## Documents

- [`docs/tutorial.md`](docs/tutorial.md): learn Vyrn in an hour, every example tested
- [`docs/syntax.md`](docs/syntax.md): the syntax at a glance, and how it maps from Python
- [`docs/grammar.ebnf`](docs/grammar.ebnf): the complete EBNF grammar (v0.4)
- [`docs/architecture.md`](docs/architecture.md): compiler tiers, memory model, GPU bridge and concurrency design
- [`docs/stdlib.md`](docs/stdlib.md): the standard library: the Prelude, `std.http`, `std.fs`, `std.task` and `std.nn`
- [`docs/principles.md`](docs/principles.md): the five rules every feature is checked against, and what Vyrn takes from each language
- [`docs/ffi.md`](docs/ffi.md): the Python and C bridges, zero-copy tensors, and the decentralized module resolver
- [`docs/nn.md`](docs/nn.md): `kernel fn` on tensors, its lowering to WGSL, and tensor memory layout
- [`docs/deploy.md`](docs/deploy.md): `vyrn build --target=...` for static Linux binaries and edge WebAssembly, and the LLVM backend design

## Examples

- [`examples/grade.vyrn`](examples/grade.vyrn): beginner, a grade calculator
- [`examples/game.vyrn`](examples/game.vyrn): game dev, a fixed-size entity pool and a per-frame memory region
- [`examples/bounce.vyrn`](examples/bounce.vyrn): a terminal game loop with physics, drawn at 30 frames a second
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
| `vyrn build [--target=T]` | Makes one executable file that runs anywhere, with nothing to install; `--target=linux-musl` for a static Linux binary, `--target=wasm32-edge` for Vercel, Cloudflare and Deno |

Files of a project use each other with `use geo.shapes` (loads `geo/shapes.vyrn`),
and modules from any git host with `use "github.com/ann/neural@v1"`. There is no
central registry: `docs/ffi.md` describes the resolver.

## Editor

`editors/vscode` is the VS Code extension: highlighting, errors from `vyrn check`
as you type, formatting with `vyrn fmt`, and run and test commands. Its README
says how to package and install it.

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

`examples/grade.vyrn`, `examples/api.vyrn` (a web API on port 8080, with its
database module in `examples/app/storage.vyrn`) and `examples/bounce.vyrn` (a
terminal game loop) run as they are. `examples/game.vyrn` and
`examples/inference.vyrn` show larger programs and call functions from
packages that do not exist yet (`load_level`, `load_model`).

## Playground

`playground/index.html` runs Vyrn in the browser: the whole toolchain as one
WebAssembly module, with the tutorial's examples. Serve it next to
`vyrn.wasm` (built from `runtime/wasm`), or open the published copy, which
has the module built in.
