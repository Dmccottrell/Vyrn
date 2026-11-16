#!/bin/sh
# Installs the latest Vyrn release on macOS or Linux:
#
#   curl -fsSL https://raw.githubusercontent.com/dmccottrell/vyrn/main/install.sh | sh
#
# Puts `vyrn` in ~/.vyrn/bin (or $VYRN_INSTALL), and says how to add it to
# PATH. Set VYRN_VERSION=v0.4.0 for a particular release.
set -eu

repo="dmccottrell/vyrn"
dir="${VYRN_INSTALL:-$HOME/.vyrn}/bin"
version="${VYRN_VERSION:-latest}"

case "$(uname -s)" in
  Linux) os="unknown-linux-gnu" ;;
  Darwin) os="apple-darwin" ;;
  *) echo "error: this installer is for macOS and Linux; on Windows use install.ps1" >&2; exit 1 ;;
esac
case "$(uname -m)" in
  x86_64 | amd64) arch="x86_64" ;;
  arm64 | aarch64) arch="aarch64" ;;
  *) echo "error: no prebuilt vyrn for $(uname -m); build it with: cargo install --git https://github.com/$repo" >&2; exit 1 ;;
esac
target="$arch-$os"

if [ "$version" = "latest" ]; then
  version=$(curl -fsSL "https://api.github.com/repos/$repo/releases/latest" |
    sed -n 's/.*"tag_name": *"\([^"]*\)".*/\1/p' | head -n 1)
  if [ -z "$version" ]; then
    echo "error: could not find the latest release of $repo" >&2
    exit 1
  fi
fi

name="vyrn-$version-$target"
url="https://github.com/$repo/releases/download/$version/$name.tar.gz"
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT

echo "downloading $name"
curl -fsSL "$url" -o "$tmp/vyrn.tar.gz"
if curl -fsSL "https://github.com/$repo/releases/download/$version/SHA256SUMS" -o "$tmp/SHA256SUMS" 2>/dev/null; then
  expected=$(grep " $name.tar.gz\$" "$tmp/SHA256SUMS" | cut -d' ' -f1)
  if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum "$tmp/vyrn.tar.gz" | cut -d' ' -f1)
  else
    actual=$(shasum -a 256 "$tmp/vyrn.tar.gz" | cut -d' ' -f1)
  fi
  if [ -n "$expected" ] && [ "$expected" != "$actual" ]; then
    echo "error: the download does not match its checksum" >&2
    exit 1
  fi
fi

tar xzf "$tmp/vyrn.tar.gz" -C "$tmp"
mkdir -p "$dir"
mv "$tmp/$name/vyrn" "$dir/vyrn"
chmod +x "$dir/vyrn"
echo "installed $("$dir/vyrn" --version) to $dir/vyrn"

case ":$PATH:" in
  *":$dir:"*) ;;
  *)
    echo
    echo "add it to your PATH, for example in ~/.bashrc or ~/.zshrc:"
    echo "  export PATH=\"$dir:\$PATH\""
    ;;
esac
echo
echo "then try:  vyrn        (the interactive prompt)"
echo "      or:  vyrn new hello && cd hello && vyrn run"
