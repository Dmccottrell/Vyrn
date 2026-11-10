//! Projects: programs made of several files, and modules fetched by URL.
//!
//! - [`url`]: parsing `use "host/owner/repo@version"` and choosing versions
//! - [`fetch`]: listing refs over git's HTTPS protocol, archives, hashes
//! - [`lock`]: `vyrn.lock`, the exact commit and hash of every URL module
//! - [`resolve`]: the content-addressed module cache
//! - [`loader`]: loading every file a program uses into one namespaced tree

pub mod bundle;
pub mod fetch;
pub mod loader;
pub mod lock;
pub mod manifest;
pub mod resolve;
pub mod url;

use std::path::{Path, PathBuf};

use crate::diagnostic::Diagnostic;

pub use loader::{Loaded, SourceFile};

/// A fetcher for builds without network support.
pub struct NoNetwork;

impl fetch::Fetcher for NoNetwork {
    fn list_refs(&self, url: &url::ModuleUrl) -> Result<Vec<url::GitRef>, fetch::FetchError> {
        Err(fetch::FetchError(format!(
            "`{}` is not cached, and this vyrn was built without network support",
            url.text
        )))
    }

    fn archive(&self, url: &url::ModuleUrl, _: &str) -> Result<Vec<u8>, fetch::FetchError> {
        self::NoNetwork.list_refs(url).map(|_| Vec::new())
    }
}

/// The fetcher `vyrn` uses: HTTPS when built with the `net` feature.
pub fn default_fetcher() -> Box<dyn fetch::Fetcher> {
    #[cfg(feature = "net")]
    {
        Box::new(fetch::HttpFetcher::new())
    }
    #[cfg(not(feature = "net"))]
    {
        Box::new(NoNetwork)
    }
}

/// Loads a program and every module it uses. URL modules are fetched and
/// locked in the project's `vyrn.lock` (next to `vyrn.toml`, or next to the
/// file when there is no project).
pub fn load_program(
    entry: &Path,
    fetcher: &dyn fetch::Fetcher,
    offline: bool,
) -> Result<Loaded, (Vec<Diagnostic>, Vec<SourceFile>)> {
    let root = manifest::find_root(entry).unwrap_or_else(|| {
        entry
            .parent()
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| PathBuf::from("."))
    });
    let lock_path = root.join(manifest::LOCKFILE);
    let lock = match std::fs::read_to_string(&lock_path) {
        Ok(text) => match lock::Lockfile::parse(&text) {
            Ok(l) => l,
            Err(e) => return Err((vec![Diagnostic::new(e, Default::default())], Vec::new())),
        },
        Err(_) => lock::Lockfile::default(),
    };
    let mut resolver = resolve::Resolver::new(fetcher, resolve::vyrn_home().join("modules"), lock);
    resolver.offline = offline;
    let result = loader::load(
        entry,
        &root,
        &mut loader::Disk {
            resolver: Some(&mut resolver),
        },
    );
    if resolver.lock_changed {
        if let Err(e) = std::fs::write(&lock_path, resolver.lock.render()) {
            let d = Diagnostic::new(
                format!("cannot write {}: {e}", lock_path.display()),
                Default::default(),
            );
            return Err((vec![d], Vec::new()));
        }
    }
    result
}
