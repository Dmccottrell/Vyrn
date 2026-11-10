//! `vyrn.lock`: the exact commit and content hash of every URL module.
//!
//! ```text
//! # vyrn.lock: written by vyrn. Commit it; do not edit it.
//!
//! [[module]]
//! url = "github.com/ann/neural@v1"
//! version = "v1.10.0"
//! commit = "3f9c2e1c0d9a4b8e7f6a5b4c3d2e1f0a9b8c7d6e"
//! hash = "sha256-9b1f..."
//! ```
//!
//! With a lock file, every build fetches exactly the same bytes: a module
//! whose content no longer matches its hash is an error, never a silent
//! update.

use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Locked {
    /// The tag or branch the version request resolved to.
    pub version: String,
    pub commit: String,
    /// [`super::fetch::tree_hash`] of the module's files.
    pub hash: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lockfile {
    /// By module URL as written in `use`.
    pub modules: BTreeMap<String, Locked>,
}

impl Lockfile {
    pub fn parse(text: &str) -> Result<Lockfile, String> {
        let mut lock = Lockfile::default();
        let mut current: Option<BTreeMap<String, String>> = None;
        let finish =
            |entry: Option<BTreeMap<String, String>>, lock: &mut Lockfile| -> Result<(), String> {
                let Some(mut e) = entry else {
                    return Ok(());
                };
                let mut take = |k: &str| {
                    e.remove(k)
                        .ok_or_else(|| format!("a [[module]] entry has no `{k}`"))
                };
                let url = take("url")?;
                let locked = Locked {
                    version: take("version")?,
                    commit: take("commit")?,
                    hash: take("hash")?,
                };
                lock.modules.insert(url, locked);
                Ok(())
            };
        for (n, raw) in text.lines().enumerate() {
            let line = raw.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if line == "[[module]]" {
                finish(current.take(), &mut lock)?;
                current = Some(BTreeMap::new());
                continue;
            }
            let (key, value) = line
                .split_once('=')
                .ok_or_else(|| format!("vyrn.lock line {}: expected `key = \"value\"`", n + 1))?;
            let value = value.trim();
            let value = value
                .strip_prefix('"')
                .and_then(|v| v.strip_suffix('"'))
                .ok_or_else(|| format!("vyrn.lock line {}: values are quoted strings", n + 1))?;
            let entry = current
                .as_mut()
                .ok_or_else(|| format!("vyrn.lock line {}: a value outside [[module]]", n + 1))?;
            entry.insert(key.trim().to_string(), value.to_string());
        }
        finish(current, &mut lock)?;
        Ok(lock)
    }

    pub fn render(&self) -> String {
        let mut out = String::from("# vyrn.lock: written by vyrn. Commit it; do not edit it.\n");
        for (url, m) in &self.modules {
            out.push_str(&format!(
                "\n[[module]]\nurl = \"{url}\"\nversion = \"{}\"\ncommit = \"{}\"\nhash = \"{}\"\n",
                m.version, m.commit, m.hash
            ));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lock_files_round_trip() {
        let mut lock = Lockfile::default();
        lock.modules.insert(
            "github.com/ann/neural@v1".into(),
            Locked {
                version: "v1.10.0".into(),
                commit: "c".repeat(40),
                hash: "sha256-00".into(),
            },
        );
        let text = lock.render();
        assert_eq!(Lockfile::parse(&text).unwrap(), lock);
        assert!(Lockfile::parse("url = \"x\"").is_err());
        assert!(Lockfile::parse("[[module]]\nurl = \"x\"\n").is_err());
    }
}
