# The Vyrn package registry

The list of published Vyrn packages: `index.json` here. `vyrn search`,
`vyrn info` and `vyrn add <name>` read it from the Learn Vyrn website
(<https://learn-vyrn.vercel.app/registry/index.json>), which publishes this
folder, and the website's **Packages** page lists it. Packages themselves
stay in their own repositories and are fetched from there.

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
fork next to your package, and from your package's folder:

```bash
vyrn publish --to ../Vyrn/registry
```

It checks and tests the package, then adds it to `index.json`. Commit
that, push, and open a pull request; CI runs `check.vyrn` on it.

## Rules

- Names are lower-case letters, digits and dashes, and first come, first
  served.
- `url` is the package's repository, `host/owner/repo`; versions are its
  git tags (`v1.2.3`).
- A package may be removed if it is harmful, or abandoned and its name is
  wanted.
