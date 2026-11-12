//! The package registry: an index of published Vyrn packages, so they can
//! be found by name (`vyrn search`, `vyrn info`) and added by name
//! (`vyrn add neural`).
//!
//! The index is one JSON file in a git repository, as Homebrew's and
//! crates.io's are. Packages are still fetched from their own repositories
//! by URL, so the registry only maps names to those URLs; it stores no
//! code and needs no server. Publishing is a pull request adding an entry,
//! which `vyrn publish` writes.
//!
//! ```json
//! {"packages": [
//!   {"name": "neural", "description": "Small neural networks",
//!    "url": "github.com/ann/neural", "versions": ["v1.0.0", "v1.1.0"],
//!    "keywords": ["ml", "tensor"], "license": "MIT"}
//! ]}
//! ```
//!
//! `VYRN_REGISTRY` names another index: a URL, or a file path (for a
//! private registry, or a clone of the public one).

use std::path::PathBuf;

use crate::vm::json::{self, Json};

/// The public index.
pub const DEFAULT_REGISTRY: &str =
    "https://raw.githubusercontent.com/dmccottrell/vyrn-registry/main/index.json";

/// The index's repository, where entries are added.
pub const DEFAULT_REGISTRY_REPO: &str = "https://github.com/dmccottrell/vyrn-registry";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    pub name: String,
    pub description: String,
    /// The module URL without a version: `github.com/ann/neural`.
    pub url: String,
    /// Tags, oldest first: `v1.0.0`.
    pub versions: Vec<String>,
    pub keywords: Vec<String>,
    pub license: String,
}

impl Entry {
    /// The newest version, comparing numbers (`v1.10.0` after `v1.9.0`).
    pub fn latest(&self) -> Option<&str> {
        self.versions
            .iter()
            .max_by_key(|v| version_key(v))
            .map(String::as_str)
    }

    /// What `vyrn add` locks for this entry: its newest major version,
    /// such as `github.com/ann/neural@v1`, so fixes come along later.
    pub fn module_url(&self, version: Option<&str>) -> String {
        let pick = version.map(str::to_string).or_else(|| {
            self.latest()
                .map(|v| v.split('.').next().unwrap_or(v).to_string())
        });
        match pick {
            Some(v) => format!("{}@{v}", self.url),
            None => format!("{}@main", self.url),
        }
    }

    fn json(&self) -> String {
        let s = |t: &str| {
            let mut out = String::new();
            json::write_string(t, &mut out);
            out
        };
        let list = |xs: &[String]| xs.iter().map(|x| s(x)).collect::<Vec<_>>().join(", ");
        format!(
            "    {{\"name\": {}, \"description\": {}, \"url\": {}, \"versions\": [{}], \"keywords\": [{}], \"license\": {}}}",
            s(&self.name),
            s(&self.description),
            s(&self.url),
            list(&self.versions),
            list(&self.keywords),
            s(&self.license)
        )
    }
}

fn version_key(v: &str) -> Vec<u64> {
    v.trim_start_matches('v')
        .split('.')
        .map(|p| p.parse().unwrap_or(0))
        .collect()
}

/// Reads an index.
pub fn parse_index(text: &str) -> Result<Vec<Entry>, String> {
    let root =
        json::parse(text).map_err(|e| format!("the registry index is not valid JSON: {e}"))?;
    let packages = root
        .get("packages")
        .and_then(Json::as_array)
        .ok_or("the registry index has no `packages` list")?;
    let text_of = |j: &Json, key: &str| j.get(key).and_then(Json::as_str).unwrap_or("").to_string();
    let list_of = |j: &Json, key: &str| -> Vec<String> {
        j.get(key)
            .and_then(Json::as_array)
            .unwrap_or(&[])
            .iter()
            .filter_map(|x| x.as_str().map(str::to_string))
            .collect()
    };
    packages
        .iter()
        .map(|p| {
            let entry = Entry {
                name: text_of(p, "name"),
                description: text_of(p, "description"),
                url: text_of(p, "url"),
                versions: list_of(p, "versions"),
                keywords: list_of(p, "keywords"),
                license: text_of(p, "license"),
            };
            if entry.name.is_empty() || entry.url.is_empty() {
                Err("a registry entry needs a `name` and a `url`".to_string())
            } else {
                Ok(entry)
            }
        })
        .collect()
}

/// An index as text, one package per line, sorted by name.
pub fn render_index(entries: &[Entry]) -> String {
    let mut sorted: Vec<&Entry> = entries.iter().collect();
    sorted.sort_by(|a, b| a.name.cmp(&b.name));
    let lines: Vec<String> = sorted.iter().map(|e| e.json()).collect();
    format!("{{\"packages\": [\n{}\n]}}\n", lines.join(",\n"))
}

/// Adds or updates `entry` in an index: a new version joins the list; the
/// description, keywords and license are replaced. A name that belongs to
/// another repository is refused.
pub fn merge(entries: &mut Vec<Entry>, entry: Entry) -> Result<(), String> {
    match entries.iter_mut().find(|e| e.name == entry.name) {
        Some(existing) if existing.url != entry.url => Err(format!(
            "the name `{}` is taken by {}; choose another `name` in vyrn.toml",
            entry.name, existing.url
        )),
        Some(existing) => {
            for v in entry.versions {
                if !existing.versions.contains(&v) {
                    existing.versions.push(v);
                }
            }
            existing.versions.sort_by_key(|v| version_key(v));
            existing.description = entry.description;
            existing.keywords = entry.keywords;
            existing.license = entry.license;
            Ok(())
        }
        None => {
            entries.push(entry);
            Ok(())
        }
    }
}

/// Packages matching every search word, best first: a word in the name
/// counts most, then in the keywords, then in the description.
pub fn search<'e>(entries: &'e [Entry], words: &[String]) -> Vec<&'e Entry> {
    let mut scored: Vec<(u32, &Entry)> = entries
        .iter()
        .filter_map(|e| {
            let mut score = 0;
            for w in words {
                let w = w.to_lowercase();
                let s = if e.name.to_lowercase() == w {
                    10
                } else if e.name.to_lowercase().contains(&w) {
                    6
                } else if e.keywords.iter().any(|k| k.to_lowercase() == w) {
                    4
                } else if e.description.to_lowercase().contains(&w) {
                    2
                } else {
                    return None;
                };
                score += s;
            }
            Some((score, e))
        })
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.name.cmp(&b.1.name)));
    scored.into_iter().map(|(_, e)| e).collect()
}

/// Where the index is: `VYRN_REGISTRY`, or the public one.
pub fn location() -> String {
    std::env::var("VYRN_REGISTRY").unwrap_or_else(|_| DEFAULT_REGISTRY.to_string())
}

fn cache_path() -> PathBuf {
    super::resolve::vyrn_home().join("registry-index.json")
}

/// The index's text: read from a file, or downloaded (and kept, for when
/// there is no network). Gives a note when the kept copy was used.
pub fn fetch_index(offline: bool) -> Result<(String, Option<String>), String> {
    let at = location();
    if !at.starts_with("http://") && !at.starts_with("https://") {
        return std::fs::read_to_string(&at)
            .map(|t| (t, None))
            .map_err(|e| format!("cannot read the registry index {at}: {e}"));
    }
    let cached = || std::fs::read_to_string(cache_path()).ok();
    if offline {
        return cached()
            .map(|t| {
                (
                    t,
                    Some("offline: using the registry index downloaded earlier".to_string()),
                )
            })
            .ok_or_else(|| "offline, and the registry index was never downloaded".to_string());
    }
    match download(&at) {
        Ok(text) => {
            let _ = std::fs::create_dir_all(super::resolve::vyrn_home());
            let _ = std::fs::write(cache_path(), &text);
            Ok((text, None))
        }
        Err(e) => cached()
            .map(|t| {
                (
                    t,
                    Some(format!("{e}; using the registry index downloaded earlier")),
                )
            })
            .ok_or(e),
    }
}

#[cfg(feature = "net")]
fn download(url: &str) -> Result<String, String> {
    let agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(30)))
        .user_agent("vyrn")
        .build()
        .new_agent();
    agent
        .get(url)
        .call()
        .map_err(|e| format!("could not fetch the registry index {url}: {e}"))?
        .body_mut()
        .read_to_string()
        .map_err(|e| format!("could not read the registry index {url}: {e}"))
}

#[cfg(not(feature = "net"))]
fn download(url: &str) -> Result<String, String> {
    Err(format!(
        "this vyrn was built without networking, so it cannot fetch {url}; set VYRN_REGISTRY to a file"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(name: &str, url: &str, versions: &[&str]) -> Entry {
        Entry {
            name: name.into(),
            description: format!("the {name} package"),
            url: url.into(),
            versions: versions.iter().map(|v| v.to_string()).collect(),
            keywords: vec!["games".into()],
            license: "MIT".into(),
        }
    }

    #[test]
    fn versions_compare_as_numbers() {
        let e = entry("a", "github.com/x/a", &["v1.9.0", "v1.10.0", "v0.3.0"]);
        assert_eq!(e.latest(), Some("v1.10.0"));
        assert_eq!(e.module_url(None), "github.com/x/a@v1");
        assert_eq!(e.module_url(Some("v0.3")), "github.com/x/a@v0.3");
        assert_eq!(
            entry("b", "github.com/x/b", &[]).module_url(None),
            "github.com/x/b@main"
        );
    }

    #[test]
    fn indexes_round_trip_and_merge() {
        let mut all = vec![entry("zeta", "github.com/x/zeta", &["v1.0.0"])];
        merge(&mut all, entry("alpha", "github.com/x/alpha", &["v0.1.0"])).unwrap();
        merge(&mut all, entry("zeta", "github.com/x/zeta", &["v1.1.0"])).unwrap();
        assert!(merge(&mut all, entry("zeta", "github.com/y/other", &["v1.0.0"])).is_err());
        let text = render_index(&all);
        assert!(text.find("alpha").unwrap() < text.find("zeta").unwrap());
        let back = parse_index(&text).unwrap();
        assert_eq!(back.len(), 2);
        assert_eq!(back[1].versions, ["v1.0.0", "v1.1.0"]);
    }

    #[test]
    fn searching_ranks_names_first() {
        let all = vec![
            Entry {
                description: "physics for games".into(),
                ..entry("rigid", "github.com/x/rigid", &[])
            },
            entry("physics", "github.com/x/physics", &[]),
        ];
        let found = search(&all, &["physics".into()]);
        assert_eq!(found[0].name, "physics");
        assert_eq!(found.len(), 2);
        assert!(search(&all, &["physics".into(), "nothing".into()]).is_empty());
    }
}
