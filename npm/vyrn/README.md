# vyrn-lang

The [Vyrn](https://github.com/dmccottrell/vyrn) programming language,
installed with npm.

```bash
npm install -g vyrn-lang
```

```bash
vyrn run hello.vyrn
```

Or without installing:

```bash
npx vyrn-lang run hello.vyrn
```

Installing downloads the prebuilt `vyrn` for your computer (Windows x64,
macOS and Linux on x64 and ARM) from the GitHub release with the same
version. Everything `vyrn` does works the same: `vyrn` alone opens the
interactive prompt, `vyrn learn` starts the course, `vyrn debug` debugs.

Behind a proxy or offline, point the installer at a copy you already have:
`VYRN_BINARY_PATH=/path/to/vyrn npm install -g vyrn-lang`.
