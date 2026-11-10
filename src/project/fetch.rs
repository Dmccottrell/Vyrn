//! Getting module source: listing a repository's tags and branches,
//! downloading an archive of one commit, unpacking it safely, and hashing
//! what was unpacked.
//!
//! Tags are listed with git's own HTTPS protocol (the "smart HTTP" ref
//! advertisement), which every git host speaks, so no host needs a special
//! API and there is no central registry. Archives come from the host's
//! archive endpoint.

use std::collections::BTreeMap;
use std::fmt;
use std::io::Read;

use sha2::{Digest, Sha256};

use super::url::{GitRef, ModuleUrl};

#[derive(Debug)]
pub struct FetchError(pub String);

impl fmt::Display for FetchError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where module source comes from. The resolver uses HTTPS; tests use a
/// fake.
pub trait Fetcher {
    /// The repository's tags and branches.
    fn list_refs(&self, url: &ModuleUrl) -> Result<Vec<GitRef>, FetchError>;
    /// A `.tar.gz` archive of the repository at `commit`.
    fn archive(&self, url: &ModuleUrl, commit: &str) -> Result<Vec<u8>, FetchError>;
}

/// Parses git's ref advertisement: lines in pkt-line format, each four hex
/// digits of length and then `<commit> <ref>`. Annotated tags are followed
/// by a peeled line (`refs/tags/v1^{}`) naming the commit itself, which is
/// what is kept.
pub fn parse_ref_advertisement(body: &[u8]) -> Result<Vec<GitRef>, FetchError> {
    let mut refs: BTreeMap<String, String> = BTreeMap::new();
    let mut at = 0;
    while at + 4 <= body.len() {
        let len_hex = std::str::from_utf8(&body[at..at + 4])
            .map_err(|_| FetchError("the server's ref list is not git's format".into()))?;
        let len = usize::from_str_radix(len_hex, 16)
            .map_err(|_| FetchError("the server's ref list is not git's format".into()))?;
        if len == 0 {
            at += 4;
            continue;
        }
        if len < 4 || at + len > body.len() {
            return Err(FetchError("the server's ref list is cut short".into()));
        }
        let line = String::from_utf8_lossy(&body[at + 4..at + len]);
        at += len;
        let line = line.trim_end_matches('\n');
        if line.starts_with('#') {
            continue;
        }
        // Capabilities follow a NUL on the first ref line.
        let line = line.split('\0').next().unwrap_or("");
        let Some((commit, name)) = line.split_once(' ') else {
            continue;
        };
        if commit.len() != 40 {
            continue;
        }
        match name.strip_suffix("^{}") {
            Some(tag) => {
                refs.insert(tag.to_string(), commit.to_string());
            }
            None => {
                refs.entry(name.to_string())
                    .or_insert_with(|| commit.to_string());
            }
        }
    }
    Ok(refs
        .into_iter()
        .map(|(name, commit)| GitRef { name, commit })
        .collect())
}

/// Files of an unpacked archive, by path relative to the repository root.
pub type Tree = BTreeMap<String, Vec<u8>>;

/// The largest archive accepted, and the largest single file.
const MAX_ARCHIVE: u64 = 512 << 20;

/// Unpacks a `.tar.gz` archive into memory. The single top directory that
/// hosts add (`neural-3f9c2e1/`) is removed. Entries that are not plain
/// files (links, devices) are skipped, and any path that could escape the
/// module directory is refused.
pub fn unpack(archive: &[u8]) -> Result<Tree, FetchError> {
    let gz = flate2::read::GzDecoder::new(archive);
    let mut tar = tar::Archive::new(gz.take(MAX_ARCHIVE));
    let mut tree = Tree::new();
    let entries = tar
        .entries()
        .map_err(|e| FetchError(format!("the archive is damaged: {e}")))?;
    for entry in entries {
        let mut entry = entry.map_err(|e| FetchError(format!("the archive is damaged: {e}")))?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|e| FetchError(format!("the archive is damaged: {e}")))?
            .to_string_lossy()
            .replace('\\', "/");
        let mut parts: Vec<&str> = path
            .split('/')
            .filter(|p| !p.is_empty() && *p != ".")
            .collect();
        if path.starts_with('/') || parts.contains(&"..") {
            return Err(FetchError(format!(
                "the archive contains `{path}`, which points outside the module; it was not unpacked"
            )));
        }
        if parts.len() < 2 {
            continue;
        }
        parts.remove(0);
        let mut data = Vec::new();
        entry
            .read_to_end(&mut data)
            .map_err(|e| FetchError(format!("the archive is damaged: {e}")))?;
        tree.insert(parts.join("/"), data);
    }
    if tree.is_empty() {
        return Err(FetchError("the archive has no files".into()));
    }
    Ok(tree)
}

/// The content hash of a tree: SHA-256 over every path and its bytes, in
/// path order. It depends only on the files, not on how the host packed
/// the archive, so it is the same from any mirror.
pub fn tree_hash(tree: &Tree) -> String {
    let mut h = Sha256::new();
    for (path, data) in tree {
        h.update((path.len() as u64).to_le_bytes());
        h.update(path.as_bytes());
        h.update((data.len() as u64).to_le_bytes());
        h.update(data);
    }
    let digest = h.finalize();
    let hex: String = digest.iter().map(|b| format!("{b:02x}")).collect();
    format!("sha256-{hex}")
}

/// The archive address for a commit on well-known hosts. Any other host is
/// assumed to follow the Gitea/Forgejo layout, which most self-hosted
/// forges use.
pub fn archive_url(url: &ModuleUrl, commit: &str) -> String {
    let repo = &url.repo;
    let name = repo.rsplit('/').next().unwrap_or(repo);
    match url.host.as_str() {
        "github.com" => format!("https://codeload.github.com/{repo}/tar.gz/{commit}"),
        "gitlab.com" => {
            format!("https://gitlab.com/{repo}/-/archive/{commit}/{name}-{commit}.tar.gz")
        }
        "bitbucket.org" => format!("https://bitbucket.org/{repo}/get/{commit}.tar.gz"),
        host => format!("https://{host}/{repo}/archive/{commit}.tar.gz"),
    }
}

/// Fetches over HTTPS. Honours `HTTPS_PROXY` and, for networks that
/// inspect TLS, the certificates in `SSL_CERT_FILE`.
#[cfg(feature = "net")]
pub struct HttpFetcher {
    agent: ureq::Agent,
}

#[cfg(feature = "net")]
impl HttpFetcher {
    pub fn new() -> HttpFetcher {
        use ureq::tls::{parse_pem, PemItem, RootCerts, TlsConfig};
        let mut tls = TlsConfig::builder();
        if let Some(pem) = std::env::var_os("SSL_CERT_FILE").and_then(|p| std::fs::read(p).ok()) {
            let certs: Vec<_> = parse_pem(&pem)
                .filter_map(|item| match item {
                    Ok(PemItem::Certificate(c)) => Some(c),
                    _ => None,
                })
                .collect();
            if !certs.is_empty() {
                tls = tls.root_certs(RootCerts::Specific(std::sync::Arc::new(certs)));
            }
        }
        let agent = ureq::Agent::config_builder()
            .tls_config(tls.build())
            .timeout_global(Some(std::time::Duration::from_secs(120)))
            .user_agent("vyrn")
            .build()
            .new_agent();
        HttpFetcher { agent }
    }

    fn get(&self, address: &str) -> Result<Vec<u8>, FetchError> {
        let mut response = self
            .agent
            .get(address)
            .call()
            .map_err(|e| FetchError(format!("could not fetch {address}: {e}")))?;
        response
            .body_mut()
            .with_config()
            .limit(MAX_ARCHIVE)
            .read_to_vec()
            .map_err(|e| FetchError(format!("could not read {address}: {e}")))
    }
}

#[cfg(feature = "net")]
impl Default for HttpFetcher {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "net")]
impl Fetcher for HttpFetcher {
    fn list_refs(&self, url: &ModuleUrl) -> Result<Vec<GitRef>, FetchError> {
        let address = format!("{}.git/info/refs?service=git-upload-pack", url.repo_url());
        let body = self.get(&address).map_err(|e| {
            FetchError(format!(
                "{e}\n  is `{}` a public git repository?",
                url.repo_url()
            ))
        })?;
        parse_ref_advertisement(&body)
    }

    fn archive(&self, url: &ModuleUrl, commit: &str) -> Result<Vec<u8>, FetchError> {
        self.get(&archive_url(url, commit))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pkt(line: &str) -> String {
        format!("{:04x}{line}", line.len() + 4)
    }

    #[test]
    fn ref_advertisements_are_parsed() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let c = "c".repeat(40);
        let body = format!(
            "{}0000{}{}{}{}0000",
            pkt("# service=git-upload-pack\n"),
            pkt(&format!("{a} HEAD\0multi_ack side-band\n")),
            pkt(&format!("{a} refs/heads/main\n")),
            pkt(&format!("{b} refs/tags/v1.0.0\n")),
            pkt(&format!("{c} refs/tags/v1.0.0^{{}}\n")),
        );
        let refs = parse_ref_advertisement(body.as_bytes()).unwrap();
        let find = |n: &str| refs.iter().find(|r| r.name == n).map(|r| r.commit.clone());
        assert_eq!(find("refs/heads/main"), Some(a));
        // The annotated tag resolves to the commit, not the tag object.
        assert_eq!(find("refs/tags/v1.0.0"), Some(c));
    }

    fn tar_gz(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut builder = tar::Builder::new(Vec::new());
        for (path, data) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append_data(&mut header, path, *data).unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        gz.finish().unwrap()
    }

    #[test]
    fn archives_unpack_without_the_top_directory() {
        let archive = tar_gz(&[
            ("neural-abc/lib.vyrn", b"fn f(): return 1\n"),
            ("neural-abc/src/a.vyrn", b"x"),
        ]);
        let tree = unpack(&archive).unwrap();
        assert_eq!(tree.keys().collect::<Vec<_>>(), ["lib.vyrn", "src/a.vyrn"]);
    }

    #[test]
    fn hashes_depend_only_on_content() {
        let one = unpack(&tar_gz(&[("x-1/a", b"1"), ("x-1/b", b"2")])).unwrap();
        let two = unpack(&tar_gz(&[("other/b", b"2"), ("other/a", b"1")])).unwrap();
        let changed = unpack(&tar_gz(&[("x-1/a", b"1"), ("x-1/b", b"3")])).unwrap();
        assert_eq!(tree_hash(&one), tree_hash(&two));
        assert_ne!(tree_hash(&one), tree_hash(&changed));
        assert!(tree_hash(&one).starts_with("sha256-"));
    }

    #[test]
    fn archive_addresses() {
        let url = ModuleUrl::parse("github.com/ann/neural@v1").unwrap();
        assert_eq!(
            archive_url(&url, "abc"),
            "https://codeload.github.com/ann/neural/tar.gz/abc"
        );
        let url = ModuleUrl::parse("codeberg.org/ann/neural@v1").unwrap();
        assert_eq!(
            archive_url(&url, "abc"),
            "https://codeberg.org/ann/neural/archive/abc.tar.gz"
        );
    }
}
