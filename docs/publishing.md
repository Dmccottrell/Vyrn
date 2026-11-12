# Publishing Vyrn

Everything is built and published by GitHub Actions when you push a
version tag. You set up three accounts once; after that, a release is two
commands.

## What a release makes

Pushing a tag such as `v0.5.0` runs `.github/workflows/release.yml`, which
builds and attaches to a GitHub release:

| File | Who uses it |
|---|---|
| `vyrn-v0.5.0-<platform>.tar.gz` / `.zip` | `install.sh` and `install.ps1` |
| `vyrn-v0.5.0-<platform>` / `.exe` (the bare program) | the npm package `vyrn-lang` |
| `VyrnScripting-v0.5.0-<platform>.zip` | Unreal Engine projects (`unreal/README.md`) |
| `vyrn-vscode-v0.5.0.vsix` | VS Code, by hand: Extensions > `...` > Install from VSIX |
| `SHA256SUMS` | checking downloads |

Then, if the secrets below are set, it publishes:

- the VS Code extension to the **Visual Studio Marketplace** (`VSCE_PAT`),
- the same extension to **Open VSX**, for VSCodium, Cursor and other
  editors (`OVSX_PAT`),
- the **npm** package `vyrn-lang` (`NPM_TOKEN`).

A secret that isn't set skips its step; the rest still runs.

## One-time setup

Add each token in GitHub: the repository's **Settings > Secrets and
variables > Actions > New repository secret**.

### npm (`NPM_TOKEN`)

1. Make an account at npmjs.com and turn on two-factor authentication.
2. Avatar > **Access Tokens** > **Generate New Token** > *Granular Access
   Token*: Read and write for packages, with "Bypass 2FA" allowed for
   automation.
3. Save it as the secret `NPM_TOKEN`.

The package is named `vyrn-lang` (in `npm/vyrn/package.json`). If you
prefer a scope such as `@yourname/vyrn`, change `name` there; the command
is still `vyrn`.

### VS Code Marketplace (`VSCE_PAT`)

1. Sign in at https://marketplace.visualstudio.com/manage and **Create
   publisher**. The ID must match `"publisher"` in
   `editors/vscode/package.json` (it says `vyrn`; change one or the other).
2. At https://dev.azure.com make an organization if you have none, then
   User settings > **Personal access tokens** > **New Token**: Organization
   *All accessible organizations*, Scopes *Custom defined* > **Marketplace:
   Manage**.
3. Save it as the secret `VSCE_PAT`.

### Open VSX (`OVSX_PAT`)

1. Sign in at https://open-vsx.org with GitHub, and sign the publisher
   agreement in your profile.
2. **Access Tokens** > Generate, then make the namespace once:
   `npx ovsx create-namespace vyrn -p <token>`.
3. Save the token as the secret `OVSX_PAT`.

## Making a release

Put the new version in `Cargo.toml` (and `runtime/*/Cargo.toml`), commit,
then tag and push:

```bash
git tag v0.5.0
```

```bash
git push origin v0.5.0
```

The npm package and the extension take their version from the tag, so
their `package.json` files need no edit.

## Trying it before publishing

The npm package, with a `vyrn` you built:

```bash
VYRN_BINARY_PATH=target/release/vyrn node npm/vyrn/test.js
```

The extension, as the Marketplace would get it:

```bash
cd editors/vscode && npm install --omit=dev && npx @vscode/vsce package
```

That writes `vyrn-<version>.vsix`; install it in VS Code with Extensions >
`...` > **Install from VSIX**.

In PowerShell on Windows, set the variable on its own line first:

```powershell
$env:VYRN_BINARY_PATH = "target\release\vyrn.exe"
```

```powershell
node npm\vyrn\test.js
```
