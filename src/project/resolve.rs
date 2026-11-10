//! Turning a module URL into a directory of verified source.
//!
//! ```text
//! use "github.com/ann/neural@v1"
//!        │
//!        ├─ locked?  ── yes ─► commit + hash from vyrn.lock
//!        │                     cached?  ── yes ─► ~/.vyrn/modules/<hash>/
//!        │                              └─ no ──► download that commit,
//!        │                                        check the hash matches
//!        └─ no ────► list refs (git smart HTTP), pick the newest match,
//!                    download, hash, cache, add to vyrn.lock
//! ```
//!
//! The cache is content-addressed: a directory is named by the hash of its
//! files and never changes, so any number of projects share it and a
//! mirror cannot substitute different code.

use std::fs;
use std::path::{Path, PathBuf};

use super::fetch::{tree_hash, unpack, Fetcher, Tree};
use super::lock::{Locked, Lockfile};
use super::url::{select, ModuleUrl};

pub struct Resolver<'f> {
    fetcher: &'f dyn Fetcher,
    /// `~/.vyrn/modules`
    cache: PathBuf,
    pub lock: Lockfile,
    /// Never touch the network; everything must be locked and cached.
    pub offline: bool,
    /// Whether `lock` gained entries that should be saved.
    pub lock_changed: bool,
}

/// Where Vyrn keeps fetched modules: `$VYRN_HOME`, or `~/.vyrn`.
pub fn vyrn_home() -> PathBuf {
    if let Some(home) = std::env::var_os("VYRN_HOME") {
        return PathBuf::from(home);
    }
    let base = std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."));
    base.join(".vyrn")
}

impl<'f> Resolver<'f> {
    pub fn new(fetcher: &'f dyn Fetcher, cache: PathBuf, lock: Lockfile) -> Resolver<'f> {
        Resolver {
            fetcher,
            cache,
            lock,
            offline: false,
            lock_changed: false,
        }
    }

    /// The directory holding the module's source, fetching it if needed.
    pub fn resolve(&mut self, text: &str) -> Result<PathBuf, String> {
        let url = ModuleUrl::parse(text).map_err(|e| e.0)?;
        let locked = self.lock.modules.get(text).cloned();
        let root = match locked {
            Some(locked) => {
                let dir = self.cache.join(&locked.hash);
                if !dir.is_dir() {
                    if self.offline {
                        return Err(format!(
                            "`{text}` is not in the module cache and the network is off; run `vyrn fetch` while online"
                        ));
                    }
                    let tree = self.download(&url, &locked.commit)?;
                    let hash = tree_hash(&tree);
                    if hash != locked.hash {
                        return Err(format!(
                            "`{text}` changed since it was locked: commit {} now hashes to {hash}, but vyrn.lock says {}. \
                             Someone may have rewritten the repository; check it, then run `vyrn add {text}` to accept the new code",
                            locked.commit, locked.hash
                        ));
                    }
                    self.store(&tree, &hash)?;
                }
                dir
            }
            None => {
                if self.offline {
                    return Err(format!(
                        "`{text}` is not in vyrn.lock and the network is off; run `vyrn add {text}` while online"
                    ));
                }
                let refs = self.fetcher.list_refs(&url).map_err(|e| e.0)?;
                let (version, commit) = select(&url.version, &refs).ok_or_else(|| {
                    let tags: Vec<&str> = refs
                        .iter()
                        .filter_map(|r| r.name.strip_prefix("refs/tags/"))
                        .collect();
                    format!(
                        "`{}` has no version matching `{text}`; its tags are: {}",
                        url.repo_url(),
                        if tags.is_empty() {
                            "none".to_string()
                        } else {
                            tags.join(", ")
                        }
                    )
                })?;
                let tree = self.download(&url, &commit)?;
                let hash = tree_hash(&tree);
                let dir = self.store(&tree, &hash)?;
                self.lock.modules.insert(
                    text.to_string(),
                    Locked {
                        version,
                        commit,
                        hash,
                    },
                );
                self.lock_changed = true;
                dir
            }
        };
        Ok(match &url.subdir {
            Some(sub) => root.join(sub),
            None => root,
        })
    }

    fn download(&self, url: &ModuleUrl, commit: &str) -> Result<Tree, String> {
        let archive = self.fetcher.archive(url, commit).map_err(|e| e.0)?;
        unpack(&archive).map_err(|e| format!("`{}`: {e}", url.text))
    }

    /// Writes a tree into the cache under its hash. It is unpacked next to
    /// its final place and renamed, so a crash never leaves half a module.
    fn store(&self, tree: &Tree, hash: &str) -> Result<PathBuf, String> {
        let dir = self.cache.join(hash);
        if dir.is_dir() {
            return Ok(dir);
        }
        let io = |e: std::io::Error| {
            format!(
                "cannot write the module cache at {}: {e}",
                self.cache.display()
            )
        };
        fs::create_dir_all(&self.cache).map_err(io)?;
        let tmp = self.cache.join(format!(
            ".tmp-{}-{}",
            std::process::id(),
            &hash[7..19.min(hash.len())]
        ));
        let _ = fs::remove_dir_all(&tmp);
        for (path, data) in tree {
            let target = tmp.join(Path::new(path));
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(io)?;
            }
            fs::write(&target, data).map_err(io)?;
        }
        match fs::rename(&tmp, &dir) {
            Ok(()) => Ok(dir),
            // Another process stored the same module first.
            Err(_) if dir.is_dir() => {
                let _ = fs::remove_dir_all(&tmp);
                Ok(dir)
            }
            Err(e) => Err(io(e)),
        }
    }
}
