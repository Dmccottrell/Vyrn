//! `vyrn.toml`: a project's name, edition and main file.
//!
//! ```toml
//! [package]
//! name = "hello"
//! edition = "2026"
//! main = "main.vyrn"
//! ```
//!
//! Dependencies are not listed here. A module is declared once, by the
//! `use` line that needs it, and `vyrn.lock` pins its exact version: one
//! place to write it, one place that records it.

use std::path::{Path, PathBuf};

pub const MANIFEST: &str = "vyrn.toml";
pub const LOCKFILE: &str = "vyrn.lock";
/// The edition new projects start on.
pub const EDITION: &str = "2026";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Manifest {
    pub name: String,
    pub edition: String,
    /// The main file, relative to the project directory.
    pub main: String,
}

impl Manifest {
    pub fn new(name: &str) -> Manifest {
        Manifest {
            name: name.to_string(),
            edition: EDITION.to_string(),
            main: "main.vyrn".to_string(),
        }
    }

    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut m = Manifest::new("");
        let mut section = String::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                section = name.trim().to_string();
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("vyrn.toml line {}: expected `key = \"value\"`", n + 1))?;
            let value = value
                .trim()
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or_else(|| format!("vyrn.toml line {}: values are quoted strings", n + 1))?
                .to_string();
            match (section.as_str(), key.trim()) {
                ("package", "name") => m.name = value,
                ("package", "edition") => m.edition = value,
                ("package", "main") => m.main = value,
                (s, k) => {
                    return Err(format!(
                        "vyrn.toml line {}: unknown setting `{k}`{}",
                        n + 1,
                        if s == "dependencies" {
                            "; dependencies are declared by their `use` lines, and `vyrn add` locks them"
                        } else {
                            ""
                        }
                    ))
                }
            }
        }
        if m.name.is_empty() {
            return Err("vyrn.toml needs `name` under [package]".into());
        }
        if m.edition != EDITION {
            return Err(format!(
                "vyrn.toml asks for edition `{}`; this version of vyrn knows edition `{EDITION}`",
                m.edition
            ));
        }
        Ok(m)
    }

    pub fn render(&self) -> String {
        format!(
            "[package]\nname = \"{}\"\nedition = \"{}\"\nmain = \"{}\"\n",
            self.name, self.edition, self.main
        )
    }
}

/// The project directory containing `start`: the nearest ancestor with a
/// `vyrn.toml`.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let start = start.canonicalize().ok()?;
    let mut dir = if start.is_dir() {
        start
    } else {
        start.parent()?.to_path_buf()
    };
    loop {
        if dir.join(MANIFEST).is_file() {
            return Some(dir);
        }
        dir = dir.parent()?.to_path_buf();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manifests_round_trip() {
        let m = Manifest::new("hello");
        assert_eq!(Manifest::parse(&m.render()).unwrap(), m);
        let err = Manifest::parse(
            "[package]\nname = \"x\"\n[dependencies]\nneural = \"github.com/a/b@v1\"\n",
        )
        .unwrap_err();
        assert!(err.contains("declared by their `use` lines"), "{err}");
        assert!(Manifest::parse("[package]\nname = \"x\"\nedition = \"2019\"\n").is_err());
    }
}
