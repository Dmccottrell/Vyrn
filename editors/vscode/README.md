# Vyrn for Visual Studio Code

- **Highlighting** for `.vyrn` files: keywords, types, string interpolation,
  numbers with suffixes, decorators, `route` lines and doc comments.
- **The Vyrn language server** (`vyrn lsp`): errors as you type (types,
  `let`, null safety, exhaustive `when`, borrows, with the compiler's
  hint), **completion** of keywords, builtins, your declarations and
  variables, `std` module members after `math.` and methods after `.`,
  **hover** documentation, **go to definition** (across your project's
  files), **signature help**, the **outline**, and **formatting**.
- Without the language client library installed, it falls back to running
  `vyrn check` and `vyrn fmt`.
- **Indentation** that follows Vyrn's blocks: a line ending in `:` indents
  the next, and `end`, `else if`, `else`, `catch`, `always` and `is` lines
  dedent. Blocks fold.
- **Commands**: *Vyrn: Run File*, *Vyrn: Run Tests* and *Vyrn: Open
  Interactive Prompt*.

It needs the `vyrn` command on the `PATH` (or set `vyrn.path`):

```bash
cargo install --path .
```

To package and install the extension from this directory, first fetch
the language client library:

```bash
npm install
```

```bash
npx @vscode/vsce package
```

```bash
code --install-extension vyrn-0.4.0.vsix
```

Other editors (Neovim, Helix, Zed, Sublime, Emacs) can use the same server:
point their LSP client at the command `vyrn lsp` for `.vyrn` files.

`node test.js` tests the parts that do not need VS Code: reading
`vyrn check` output, the grammar, the indentation rules, and the language
server answering `initialize`, diagnostics and hover.

## Debugging

Set a breakpoint by clicking left of a line number, then press **F5** (or
run **Vyrn: Debug File**). The program stops there, and the side bar shows
its variables and call stack. Step with F10 (over) and F11 (into). This
uses `vyrn debug --dap`; see `docs/debugging.md` for everything it can do.
