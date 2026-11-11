# Vyrn for Visual Studio Code

- **Highlighting** for `.vyrn` files: keywords, types, string interpolation,
  numbers with suffixes, decorators, `route` lines and doc comments.
- **Errors as you work**, from `vyrn check`: types, `let`, null safety,
  exhaustive `match`, borrows. They appear as you type (after a short
  pause) and on save, with the compiler's hint.
- **Formatting** with `vyrn fmt` (Format Document).
- **Indentation** that follows Vyrn's blocks: a line ending in `:` indents
  the next, and `end`, `elif`, `else`, `catch`, `finally` and `is` lines
  dedent. Blocks fold.
- **Commands**: *Vyrn: Run File* and *Vyrn: Run Tests*.

It needs the `vyrn` command on the `PATH` (or set `vyrn.path`):

```bash
cargo install --path .
```

To package and install the extension from this directory:

```bash
npx @vscode/vsce package
```

```bash
code --install-extension vyrn-0.3.0.vsix
```

`node test.js` tests the parts that do not need VS Code: reading
`vyrn check` output, the grammar, and the indentation rules.
