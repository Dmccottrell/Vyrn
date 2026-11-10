//! Module URLs: `use "github.com/ann/neural@v1"`.
//!
//! A module URL names a git repository on any host, an optional directory
//! inside it, and a version:
//!
//! | Version | Means |
//! |---|---|
//! | `@v1` | the newest tag `v1.x.y` |
//! | `@v1.4` | the newest tag `v1.4.y` |
//! | `@v1.4.2` | exactly that tag |
//! | `@main` | the newest commit on branch `main` |
//! | `@3f9c2e1...` (40 hex digits) | exactly that commit |

use std::fmt;

/// A parsed module URL.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ModuleUrl {
    /// `github.com`
    pub host: String,
    /// `ann/neural`: the repository's path on its host.
    pub repo: String,
    /// A directory inside the repository, for repositories that hold several
    /// modules: `github.com/ann/models/llama@v2` has `llama`.
    pub subdir: Option<String>,
    pub version: VersionReq,
    /// The text as written, which is also the key in `vyrn.lock`.
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum VersionReq {
    /// `v1`, `v1.4`, `v1.4.2`: tags compared as numbers.
    Semver(Vec<u64>),
    /// A branch or a tag that is not a version.
    Ref(String),
    /// A full commit hash.
    Commit(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UrlError(pub String);

impl fmt::Display for UrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl ModuleUrl {
    pub fn parse(text: &str) -> Result<ModuleUrl, UrlError> {
        let bad = |why: &str| {
            UrlError(format!(
                "`{text}` is not a module URL: {why}; write it as \"host/owner/repo@version\", for example \"github.com/ann/neural@v1\""
            ))
        };
        if text.contains("://") {
            return Err(bad(
                "leave out the scheme; modules are always fetched over HTTPS",
            ));
        }
        let (path, version) = text
            .split_once('@')
            .ok_or_else(|| bad("it has no `@version`"))?;
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if segments.len() < 3 {
            return Err(bad("it needs a host, an owner and a repository"));
        }
        let host = segments[0];
        if !host.contains('.') {
            return Err(bad("the first part must be a host name such as github.com"));
        }
        for s in &segments {
            if *s == ".." || *s == "." || s.contains('\\') {
                return Err(bad("it contains `.`, `..` or `\\`"));
            }
        }
        if version.is_empty() {
            return Err(bad("the version after `@` is empty"));
        }
        let subdir = (segments.len() > 3).then(|| segments[3..].join("/"));
        Ok(ModuleUrl {
            host: host.to_ascii_lowercase(),
            repo: format!("{}/{}", segments[1], segments[2].trim_end_matches(".git")),
            subdir,
            version: VersionReq::parse(version),
            text: text.to_string(),
        })
    }

    /// The repository's HTTPS address.
    pub fn repo_url(&self) -> String {
        format!("https://{}/{}", self.host, self.repo)
    }
}

impl VersionReq {
    pub fn parse(text: &str) -> VersionReq {
        if text.len() == 40 && text.chars().all(|c| c.is_ascii_hexdigit()) {
            return VersionReq::Commit(text.to_ascii_lowercase());
        }
        match parse_semver(text) {
            Some(parts) => VersionReq::Semver(parts),
            None => VersionReq::Ref(text.to_string()),
        }
    }
}

/// `v1.4.2` or `1.4.2` (one to three parts) as numbers.
pub fn parse_semver(tag: &str) -> Option<Vec<u64>> {
    let digits = tag.strip_prefix('v').unwrap_or(tag);
    let parts: Option<Vec<u64>> = digits.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| (1..=3).contains(&p.len()))
}

/// A tag or branch and the commit it points to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GitRef {
    /// `refs/tags/v1.2.0`, `refs/heads/main`
    pub name: String,
    pub commit: String,
}

/// The ref a version request selects: the matching tag with the highest
/// version, or the named branch or tag. Returns the ref's short name and
/// commit.
pub fn select(req: &VersionReq, refs: &[GitRef]) -> Option<(String, String)> {
    match req {
        VersionReq::Commit(c) => Some((c.clone(), c.clone())),
        VersionReq::Semver(want) => refs
            .iter()
            .filter_map(|r| {
                let tag = r.name.strip_prefix("refs/tags/")?;
                let version = parse_semver(tag)?;
                // Complete versions only: `v1` asks for a release, not for a
                // tag that is itself called `v1`, unless written exactly.
                let matches = version.len() >= want.len() && version[..want.len()] == want[..];
                matches.then(|| (version, tag.to_string(), r.commit.clone()))
            })
            .max_by(|a, b| a.0.cmp(&b.0))
            .map(|(_, tag, commit)| (tag, commit)),
        VersionReq::Ref(name) => ["refs/heads/", "refs/tags/"].iter().find_map(|prefix| {
            refs.iter()
                .find(|r| r.name == format!("{prefix}{name}"))
                .map(|r| (name.clone(), r.commit.clone()))
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn urls_are_parsed() {
        let url = ModuleUrl::parse("github.com/Dmccottrell/neural@v1").unwrap();
        assert_eq!(url.host, "github.com");
        assert_eq!(url.repo, "Dmccottrell/neural");
        assert_eq!(url.subdir, None);
        assert_eq!(url.version, VersionReq::Semver(vec![1]));
        assert_eq!(url.repo_url(), "https://github.com/Dmccottrell/neural");

        let url = ModuleUrl::parse("codeberg.org/ann/models/llama/v2@main").unwrap();
        assert_eq!(url.subdir.as_deref(), Some("llama/v2"));
        assert_eq!(url.version, VersionReq::Ref("main".into()));

        for bad in [
            "https://github.com/a/b@v1",
            "github.com/a@v1",
            "github.com/a/b",
            "localhost/a/b@v1",
            "github.com/a/../b@v1",
            "github.com/a/b@",
        ] {
            assert!(ModuleUrl::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn versions_pick_the_newest_matching_tag() {
        let refs: Vec<GitRef> = [
            ("refs/heads/main", "m"),
            ("refs/tags/v1.2.0", "a"),
            ("refs/tags/v1.10.0", "b"),
            ("refs/tags/v1.9.3", "c"),
            ("refs/tags/v2.0.0", "d"),
            ("refs/tags/nightly", "e"),
        ]
        .iter()
        .map(|(n, c)| GitRef {
            name: n.to_string(),
            commit: c.to_string(),
        })
        .collect();
        let pick = |v: &str| select(&VersionReq::parse(v), &refs);
        assert_eq!(pick("v1"), Some(("v1.10.0".into(), "b".into())));
        assert_eq!(pick("v1.9"), Some(("v1.9.3".into(), "c".into())));
        assert_eq!(pick("v1.2.0"), Some(("v1.2.0".into(), "a".into())));
        assert_eq!(pick("v3"), None);
        assert_eq!(pick("main"), Some(("main".into(), "m".into())));
        assert_eq!(pick("nightly"), Some(("nightly".into(), "e".into())));
    }
}
