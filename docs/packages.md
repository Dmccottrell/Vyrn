# Packages

A Vyrn package is a git repository with `.vyrn` files. Using one is a
`use` line; finding one is the registry.

## Using a package

```bash
vyrn search physics
```

```bash
vyrn add physics-extra
```

`vyrn add` finds the package in the registry, fetches its newest release,
and pins it in `vyrn.lock`. It prints the `use` line to write:

```vyrn
use "github.com/ann/physics-extra@v1"
```

`@v1` means "the newest v1.x.y", so fixes arrive with `vyrn add` again
without breaking changes; `@v1.4.2` pins one exact release. Packages that
are not in the registry are added by URL the same way:
`vyrn add github.com/ann/neural@v1`.

| Command | Does |
|---|---|
| `vyrn search [words]` | packages whose name, keywords or description have every word |
| `vyrn info <name>` | its versions, source and `use` line |
| `vyrn add <name>`, `vyrn add <name>@v2` | fetches and locks it |
| `vyrn fetch` | downloads everything in `vyrn.lock`, to work offline |

## Publishing a package

1. Fill in the registry fields of `vyrn.toml`:

   ```toml
   [package]
   name = "greeter"
   edition = "2026"
   main = "main.vyrn"
   version = "1.0.0"
   description = "Friendly greetings"
   repository = "github.com/you/greeter"
   keywords = "greeting, example"
   license = "MIT"
   ```

2. Commit, tag the version, and push both:

   ```bash
   git tag v1.0.0
   ```

   ```bash
   git push origin main v1.0.0
   ```

3. Fork the registry (`github.com/dmccottrell/vyrn-registry`), clone your
   fork next to your package, and from the package's folder:

   ```bash
   vyrn publish --to ../vyrn-registry
   ```

   It runs `vyrn check` and your tests, makes sure the tag exists, and adds
   the entry to the registry's `index.json`. Commit that and open a pull
   request. Without `--to`, it writes `registry-entry.json` for you to
   paste in.

## How the registry works

The registry is one file, `index.json`, in a git repository. It maps names
to repositories and lists their versions; no code is stored there, and no
server runs. Every pull request is checked by `check.vyrn` (names, URLs and
version tags). `tools/registry-template/` in this repository is the
registry's starting point.

`VYRN_REGISTRY` points `vyrn` at another index, a URL or a file, for a
private or company registry. Without a network, `vyrn search` uses the copy
it downloaded last.
