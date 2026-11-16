//! `vyrn build --target=...`: one program, built for where it will run.
//!
//! | Target | Output |
//! |---|---|
//! | `native` (default) | one executable for this machine |
//! | `linux-gnu`, `linux-musl`, `linux-arm64`, `macos`, `windows`, or a Rust target triple | one executable for that system, from the `vyrn` runtime built for it |
//! | `wasm32-edge` | a directory to deploy on an edge platform: Vercel Edge Functions, Cloudflare Workers, Deno Deploy |
//!
//! Executables are the `vyrn` runtime with the program bundled in
//! (`bundle.rs`). The edge build is the WebAssembly runtime
//! (`runtime/wasm`), the same bundle, and the small amount of JavaScript
//! each platform needs to hand requests to it.

use std::path::{Path, PathBuf};

use super::bundle::Bundle;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Native,
    /// A Rust target triple, such as `x86_64-unknown-linux-musl`.
    Triple(String),
    WasmEdge,
}

impl Target {
    pub fn parse(text: &str) -> Result<Target, String> {
        Ok(match text {
            "" | "native" | "host" => Target::Native,
            "wasm32" | "wasm32-edge" | "edge" | "wasm" => Target::WasmEdge,
            "linux-gnu" | "linux" => Target::Triple("x86_64-unknown-linux-gnu".into()),
            "linux-musl" | "linux-static" => Target::Triple("x86_64-unknown-linux-musl".into()),
            "linux-arm64" => Target::Triple("aarch64-unknown-linux-gnu".into()),
            "macos" => Target::Triple("aarch64-apple-darwin".into()),
            "macos-intel" => Target::Triple("x86_64-apple-darwin".into()),
            "windows" => Target::Triple("x86_64-pc-windows-msvc".into()),
            t if t.matches('-').count() >= 2 && !t.contains(char::is_whitespace) => Target::Triple(t.into()),
            other => {
                return Err(format!(
                    "unknown target `{other}`; use native, linux-gnu, linux-musl, linux-arm64, macos, windows, wasm32-edge, or a Rust target triple"
                ))
            }
        })
    }
}

/// Where prebuilt runtimes for other targets live:
/// `$VYRN_RUNTIMES`, or `~/.vyrn/runtimes`.
pub fn runtimes_dir() -> PathBuf {
    std::env::var_os("VYRN_RUNTIMES")
        .map(PathBuf::from)
        .unwrap_or_else(|| super::resolve::vyrn_home().join("runtimes"))
}

/// The `vyrn` executable built for `triple`.
pub fn native_runtime(triple: &str) -> Result<PathBuf, String> {
    let dir = runtimes_dir().join(triple);
    for name in ["vyrn", "vyrn.exe"] {
        let p = dir.join(name);
        if p.is_file() {
            return Ok(p);
        }
    }
    Err(format!(
        "there is no vyrn runtime for `{triple}` in {}; build one with\n\n    cargo build --release --target {triple} --no-default-features\n\nand copy target/{triple}/release/vyrn there (a deployed program needs no module fetching, so the runtime leaves it out)",
        dir.display()
    ))
}

/// The WebAssembly runtime: `$VYRN_WASM_RUNTIME`, `vyrn.wasm` next to the
/// `vyrn` executable, or `~/.vyrn/runtimes/wasm32/vyrn.wasm`.
pub fn wasm_runtime() -> Result<PathBuf, String> {
    let mut candidates = Vec::new();
    if let Some(p) = std::env::var_os("VYRN_WASM_RUNTIME") {
        candidates.push(PathBuf::from(p));
    }
    if let Some(dir) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(Path::to_path_buf))
    {
        candidates.push(dir.join("vyrn.wasm"));
    }
    candidates.push(runtimes_dir().join("wasm32").join("vyrn.wasm"));
    candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            format!(
                "the WebAssembly runtime is missing; build it with\n\n    cargo build --release --target wasm32-unknown-unknown --manifest-path runtime/wasm/Cargo.toml\n\nand copy runtime/wasm/target/wasm32-unknown-unknown/release/vyrn_wasm.wasm to {}",
                runtimes_dir().join("wasm32").join("vyrn.wasm").display()
            )
        })
}

/// The JavaScript side of the WebAssembly runtime.
pub const EDGE_JS: &str = include_str!("../../runtime/wasm/vyrn-edge.mjs");

/// Writes an edge deployment of `bundle` into `out`, and returns the files
/// written.
pub fn write_edge(bundle: &Bundle, wasm: &Path, out: &Path) -> std::io::Result<Vec<PathBuf>> {
    std::fs::create_dir_all(out.join("api"))?;
    let program = base64(&bundle.encode());
    let files: Vec<(&str, Vec<u8>)> = vec![
        ("vyrn.wasm", std::fs::read(wasm)?),
        ("vyrn-edge.mjs", EDGE_JS.as_bytes().to_vec()),
        (
            "program.mjs",
            format!(
                "// The Vyrn program, as written by `vyrn build --target=wasm32-edge`.\nexport default Uint8Array.from(atob(\"{program}\"), (c) => c.charCodeAt(0));\n"
            )
            .into_bytes(),
        ),
        (
            "api/index.js",
            br#"// Vercel Edge Function: every request goes to the Vyrn program's routes.
import wasm from "../vyrn.wasm?module";
import { loadVyrn } from "../vyrn-edge.mjs";
import program from "../program.mjs";

export const config = { runtime: "edge" };

const handler = loadVyrn(wasm).then((vyrn) => vyrn.serve(program));

export default async function (request) {
  return (await handler)(request);
}
"#
            .to_vec(),
        ),
        (
            "vercel.json",
            br#"{
  "rewrites": [{ "source": "/(.*)", "destination": "/api" }]
}
"#
            .to_vec(),
        ),
        (
            "worker.mjs",
            br#"// Cloudflare Workers, Deno Deploy and Bun: a standard `fetch` handler.
import wasm from "./vyrn.wasm";
import { loadVyrn } from "./vyrn-edge.mjs";
import program from "./program.mjs";

const handler = loadVyrn(wasm).then((vyrn) => vyrn.serve(program));

export default {
  async fetch(request) {
    return (await handler)(request);
  },
};
"#
            .to_vec(),
        ),
    ];
    let mut written = Vec::new();
    for (name, data) in files {
        let path = out.join(name);
        std::fs::write(&path, data)?;
        written.push(path);
    }
    Ok(written)
}

fn base64(data: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        for i in 0..4 {
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - 6 * i)) as usize & 63] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_parse() {
        assert_eq!(Target::parse("native").unwrap(), Target::Native);
        assert_eq!(Target::parse("wasm32-edge").unwrap(), Target::WasmEdge);
        assert_eq!(
            Target::parse("linux-musl").unwrap(),
            Target::Triple("x86_64-unknown-linux-musl".into())
        );
        assert_eq!(
            Target::parse("aarch64-unknown-linux-musl").unwrap(),
            Target::Triple("aarch64-unknown-linux-musl".into())
        );
        assert!(Target::parse("mainframe").is_err());
    }

    #[test]
    fn base64_matches_the_standard() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
}
