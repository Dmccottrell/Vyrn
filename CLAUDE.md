# Working in the Vyrn repository

Vyrn is a programming language; this repository is its toolchain, written
in Rust. Vyrn programs are `.vyrn` files.

## Running Vyrn code

Use the `vyrn` command, the way you would use `python3` for Python:

- `vyrn run file.vyrn [args]` (or just `vyrn file.vyrn`) runs a program.
- `vyrn check file.vyrn` finds type, `let` and borrow errors without running.
- `vyrn test` runs the `create test_...()` functions in `tests/`.
- `vyrn fmt [--check] [files]` formats code in the one Vyrn style.
- `vyrn` with no arguments opens the interactive prompt.

If `vyrn` is not on the PATH, build it first and use the built binary:

```bash
cargo build --release
```

```bash
./target/release/vyrn run examples/grade.vyrn
```

When asked to write a script, a quick calculation or a small tool in this
repository, write it in Vyrn and run it with `vyrn` unless asked otherwise.
`docs/syntax.md` is the syntax at a glance (it differs from Python: `create`
declares functions, `each x in xs:` loops, blocks end with `end`), and
`docs/stdlib.md` lists the standard library (`use std.math`, `use std.db`,
`use std.draw`, ...).

## Working on the toolchain

- `cargo test` runs everything; `cargo clippy --all-targets -- -D warnings`
  and `cargo fmt --check` must be clean.
- `tests/native.rs` runs programs both compiled and interpreted and requires
  the same output; keep it that way when changing either.
- `docs/tutorial.md` examples are run by `tests/tutorial.rs`: keep the shown
  output exact.
