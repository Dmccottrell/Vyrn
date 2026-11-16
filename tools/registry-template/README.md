# The Vyrn package registry

The list of published Vyrn packages. `vyrn search`, `vyrn info` and
`vyrn add <name>` read `index.json` from this repository; packages
themselves stay in their own repositories and are fetched from there.

## Publishing a package

In your package's `vyrn.toml`, under `[package]`:

```toml
version = "1.0.0"
description = "What it does, in a line"
repository = "github.com/you/your-package"
keywords = "games, physics"
license = "MIT"
```

Tag that version and push the tag, then fork this repository, clone your
fork, and from your package's folder:

```bash
vyrn publish --to ../vyrn-registry
```

It checks and tests the package, then adds it to `index.json`. Commit
that, push, and open a pull request; `check.vyrn` runs on it.

## Rules

- Names are lower-case letters, digits and dashes, and first come, first
  served. A name stays with its repository.
- Each version must be a tag in the package's repository.
- Packages must build with `vyrn check` and pass their own `vyrn test`.

## Running your own registry

Copy this repository, and point `vyrn` at its index:

```bash
export VYRN_REGISTRY=https://raw.githubusercontent.com/you/your-registry/main/index.json
```

A file path works too, for a registry on a company drive.
