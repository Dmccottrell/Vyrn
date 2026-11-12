# Vyrn

A gradually typed, garbage-collector-free language for machine learning, web
services and WebAssembly, and games and systems code.

Status: v0.4. A complete toolchain in Rust: parser, gradual type and borrow
checker, bytecode interpreter, a native tier that compiles typed functions
to machine code (10-25 times faster than CPython on numeric code, within
1.3-3.5 times of C), formatter, interactive prompt, language server,
package resolver and single-file builds. Native code covers numbers,
strings, lists of numbers and structs of numbers; maps and objects still
run on the interpreter (`docs/deploy.md` section 3).

## Documents

- [`docs/tutorial.md`](docs/tutorial.md): learn Vyrn in an hour, every example tested
- [`docs/syntax.md`](docs/syntax.md): the syntax at a glance, and how it maps from Python
- [`docs/grammar.ebnf`](docs/grammar.ebnf): the complete EBNF grammar (v0.4)
- [`docs/architecture.md`](docs/architecture.md): compiler tiers, memory model, GPU bridge and concurrency design
- [`docs/stdlib.md`](docs/stdlib.md): the standard library: the Prelude, `vyrn.http`, `vyrn.fs`, `vyrn.task` and `vyrn.nn`
- [`docs/principles.md`](docs/principles.md): the five rules every feature is checked against, and what Vyrn takes from each language
- [`docs/ffi.md`](docs/ffi.md): the Python and C bridges, zero-copy tensors, and the decentralized module resolver
- [`docs/nn.md`](docs/nn.md): `kernel` function on tensors, its lowering to WGSL, and tensor memory layout
- [`docs/deploy.md`](docs/deploy.md): `vyrn build --target=...` for static Linux binaries and edge WebAssembly, and the LLVM backend design
- [`unreal/README.md`](unreal/README.md): gameplay scripting in Unreal Engine 5 with the VyrnScripting plugin, and `vyrn.dll` for embedding Vyrn in any C or C++ program

## Examples

- [`examples/grade.vyrn`](examples/grade.vyrn): beginner, a grade calculator
- [`examples/game.vyrn`](examples/game.vyrn): game dev, a fixed-size entity pool and a per-frame memory region
- [`examples/bounce.vyrn`](examples/bounce.vyrn): a terminal game loop with physics, drawn at 30 frames a second
- [`examples/api.vyrn`](examples/api.vyrn): web, async routes returning JSON
- [`examples/inference.vyrn`](examples/inference.vyrn): ML, tensor layers spread across GPUs
- [`examples/snake.vyrn`](examples/snake.vyrn): a Snake game in a window
- [`examples/cube3d.vyrn`](examples/cube3d.vyrn): a 3D scene you can move through
- [`examples/tips.vyrn`](examples/tips.vyrn): a desktop app with text fields and buttons
- [`examples/settings.vyrn`](examples/settings.vyrn): a settings window laid out automatically, with a slider

## Install

macOS and Linux:

```bash
curl -fsSL https://raw.githubusercontent.com/dmccottrell/vyrn/main/install.sh | sh
```

Windows (PowerShell):

```powershell
irm https://raw.githubusercontent.com/dmccottrell/vyrn/main/install.ps1 | iex
```

Or from source, with Rust installed:

```bash
cargo install --path .
```

On Linux, building from source needs the sound library's development
files first (`sudo apt-get install libasound2-dev` on Debian and Ubuntu).

The installers download a release built by `.github/workflows/release.yml`;
releases appear once a version tag such as `v0.4.0` is pushed.

## Getting started

Type `vyrn` for the interactive prompt:

```text
>>> prices = [4.5, 12.0, 7.25]
>>> "{sum(prices):.2f}"
"23.75"
>>> sorted(prices, reverse = true)
[12.0, 7.25, 4.5]
```

Run a file with `vyrn hello.vyrn`, or start a project:

```bash
vyrn new hello
```

```bash
cd hello && vyrn run
```

| Command | Does |
|---|---|
| `vyrn` | The interactive prompt (also `vyrn repl`) |
| `vyrn file.vyrn [args]` | Runs one file (same as `vyrn run file.vyrn`) |
| `vyrn new <name>` | Creates a project: `vyrn.toml`, `main.vyrn`, a module and a test |
| `vyrn run [file]` | Runs the project (or one file) |
| `vyrn check [file]` | Finds type, `let`, `strict` and borrow errors without running |
| `vyrn test` | Runs every `create test_...()` in `tests/` |
| `vyrn fmt [--check]` | Formats every file in the one Vyrn style |
| `vyrn add github.com/ann/neural@v1` | Fetches a module and locks its exact commit and content hash in `vyrn.lock` |
| `vyrn fetch` | Downloads every locked module, to work offline |
| `vyrn lsp` | The language server, for editors |
| `vyrn build [--target=T]` | Makes one executable file that runs anywhere, with nothing to install; `--target=linux-musl` for a static Linux binary, `--target=wasm32-edge` for Vercel, Cloudflare and Deno |

Files of a project use each other with `use geo.shapes` (loads `geo/shapes.vyrn`),
and modules from any git host with `use "github.com/ann/neural@v1"`. There is no
central registry: `docs/ffi.md` describes the resolver.

## Standard library

What a Python programmer uses every day is there, with the same names where
Python's are good ones (`docs/stdlib.md` has the full list):

- **Prelude** (no `use` needed): `print`, `input`, `len`, `open` for files,
  `sorted`/`min`/`max` with `key =`, `map`, `filter`, `any`, `all`, sets
  (`{1, 2, 3}`), format specs (`"{price:.2f}"`, `"{n:,}"`), and the methods
  of strings, lists and maps, including `xs.map(f)`, `xs.filter(f)`,
  `xs.group_by(f)`.
- **Modules**: `vyrn.os`, `vyrn.fs`, `vyrn.path`, `vyrn.time`, `vyrn.random`,
  `vyrn.math`, `vyrn.json`, `vyrn.re`, `vyrn.csv`, `vyrn.task` (select, timeouts,
  task groups) and `vyrn.http` (a server and a client).
- **Applications**: `vyrn.db` (SQLite), `vyrn.draw` (2D and 3D graphics,
  windows with keyboard, mouse and widgets, turtle, images), `vyrn.html`
  (reading web pages), `vyrn.template` (pages from data), `vyrn.table`
  (data analysis), `vyrn.chart` (charts to PNG or SVG), `vyrn.mail`
  (email), `vyrn.game`, `vyrn.sound` and `vyrn.net` (games and
  multiplayer), and `vyrn.host` (scripts inside Unreal Engine or another
  program). No Python needed.

Runtime errors show the file, the line, the code and a hint, like compile
errors do.

## Editor

`vyrn lsp` is a language server: errors as you type, completion, hover
documentation, go to definition, signature help, an outline and formatting.
`editors/vscode` is the VS Code extension that uses it; its README says how to
package and install it. Neovim, Helix, Zed, Sublime and Emacs can run
`vyrn lsp` through their own LSP clients.

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
| `src/prelude.vyrn`, `src/prelude.rs` | The part of the Prelude written in Vyrn: everything that takes a function |
| `src/repl.rs` | The interactive prompt |
| `src/lsp.rs` | The language server |
| `src/project/` | Multi-file programs, the URL module resolver, `vyrn.lock`, single-file builds |
| `src/native/` | The native tier: typed functions to machine code with Cranelift |
| `src/vm/` | Bytecode compiler, register VM, tensor backends and builtins (see `docs/architecture.md` section 2.4); `src/vm/stdlib.rs` holds files, sets, format specs and the `vyrn.*` modules |
| `.github/workflows/` | CI on Linux, macOS and Windows; release builds for the installers |

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
