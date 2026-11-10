//! `vyrn build`: a program as one executable file.
//!
//! The executable is a copy of `vyrn` itself with the program appended:
//! every source file it loads (URL modules included), then the bundle's
//! length and a marker. When `vyrn` starts, it looks for the marker at the
//! end of its own file; if it is there, it runs the bundled program instead
//! of reading its command line as a `vyrn` command. Nothing is installed and
//! nothing is fetched on the machine that runs it.

use std::collections::BTreeMap;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use super::loader::{Disk, Source};

/// Marks the end of an executable that carries a program.
pub const MAGIC: &[u8; 8] = b"VYRNPKG1";

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Bundle {
    pub entry: String,
    pub root: String,
    /// Source text by path, as the loader read it.
    pub files: BTreeMap<String, String>,
    /// URL module -> its directory, as resolved when building.
    pub urls: BTreeMap<String, String>,
}

impl Bundle {
    pub fn encode(&self) -> Vec<u8> {
        let mut out = Vec::new();
        let mut put = |s: &str| {
            out.extend_from_slice(&(s.len() as u64).to_le_bytes());
            out.extend_from_slice(s.as_bytes());
        };
        put(&self.entry);
        put(&self.root);
        put(&self.files.len().to_string());
        for (k, v) in &self.files {
            put(k);
            put(v);
        }
        put(&self.urls.len().to_string());
        for (k, v) in &self.urls {
            put(k);
            put(v);
        }
        out
    }

    pub fn decode(data: &[u8]) -> Option<Bundle> {
        let mut at = 0usize;
        let mut get = || -> Option<String> {
            let len = u64::from_le_bytes(data.get(at..at + 8)?.try_into().ok()?) as usize;
            at += 8;
            let s = std::str::from_utf8(data.get(at..at.checked_add(len)?)?)
                .ok()?
                .to_string();
            at += len;
            Some(s)
        };
        let entry = get()?;
        let root = get()?;
        let mut files = BTreeMap::new();
        for _ in 0..get()?.parse::<usize>().ok()? {
            let k = get()?;
            files.insert(k, get()?);
        }
        let mut urls = BTreeMap::new();
        for _ in 0..get()?.parse::<usize>().ok()? {
            let k = get()?;
            urls.insert(k, get()?);
        }
        Some(Bundle {
            entry,
            root,
            files,
            urls,
        })
    }

    /// The bundle carried by the executable at `exe`, if there is one.
    pub fn from_executable(exe: &Path) -> Option<Bundle> {
        let mut f = std::fs::File::open(exe).ok()?;
        let size = f.metadata().ok()?.len();
        if size < 16 {
            return None;
        }
        f.seek(SeekFrom::End(-16)).ok()?;
        let mut tail = [0u8; 16];
        f.read_exact(&mut tail).ok()?;
        if &tail[8..] != MAGIC {
            return None;
        }
        let len = u64::from_le_bytes(tail[..8].try_into().ok()?);
        f.seek(SeekFrom::End(-16 - len as i64)).ok()?;
        let mut data = vec![0u8; len as usize];
        f.read_exact(&mut data).ok()?;
        Bundle::decode(&data)
    }

    /// Writes `base` (the `vyrn` executable) with this bundle appended.
    pub fn write_executable(&self, base: &Path, out: &Path) -> std::io::Result<()> {
        let mut bytes = std::fs::read(base)?;
        // A `vyrn` that already carries a program is first stripped of it.
        if bytes.len() >= 16 && &bytes[bytes.len() - 8..] == MAGIC {
            let len = u64::from_le_bytes(
                bytes[bytes.len() - 16..bytes.len() - 8]
                    .try_into()
                    .unwrap_or_default(),
            );
            bytes.truncate(bytes.len() - 16 - len as usize);
        }
        let data = self.encode();
        bytes.extend_from_slice(&data);
        bytes.extend_from_slice(&(data.len() as u64).to_le_bytes());
        bytes.extend_from_slice(MAGIC);
        if let Some(parent) = out.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(out, bytes)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(out, std::fs::Permissions::from_mode(0o755))?;
        }
        Ok(())
    }
}

impl Source for Bundle {
    fn read(&mut self, path: &Path) -> std::io::Result<String> {
        self.files
            .get(&path.to_string_lossy().to_string())
            .cloned()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "not in the bundle"))
    }

    fn resolve_url(&mut self, url: &str) -> Result<PathBuf, String> {
        self.urls
            .get(url)
            .map(PathBuf::from)
            .ok_or_else(|| format!("`{url}` is not in this program's bundle"))
    }

    fn exists(&mut self, path: &Path) -> bool {
        self.files.contains_key(&path.to_string_lossy().to_string())
    }
}

/// Reads from disk and records everything read, to build a bundle.
pub struct Recorder<'d, 'r, 'f> {
    pub disk: &'d mut Disk<'r, 'f>,
    pub bundle: Bundle,
}

impl Source for Recorder<'_, '_, '_> {
    fn read(&mut self, path: &Path) -> std::io::Result<String> {
        let text = self.disk.read(path)?;
        self.bundle
            .files
            .insert(path.to_string_lossy().to_string(), text.clone());
        Ok(text)
    }

    fn resolve_url(&mut self, url: &str) -> Result<PathBuf, String> {
        let dir = self.disk.resolve_url(url)?;
        self.bundle
            .urls
            .insert(url.to_string(), dir.to_string_lossy().to_string());
        Ok(dir)
    }

    fn exists(&mut self, path: &Path) -> bool {
        self.disk.exists(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundles_round_trip_through_an_executable() {
        let mut b = Bundle {
            entry: "/p/main.vyrn".into(),
            root: "/p".into(),
            ..Bundle::default()
        };
        b.files
            .insert("/p/main.vyrn".into(), "main: print(1)\n".into());
        b.urls
            .insert("github.com/a/b@v1".into(), "/cache/sha256-1".into());
        assert_eq!(Bundle::decode(&b.encode()), Some(b.clone()));

        let dir = std::env::temp_dir().join(format!("vyrn-bundle-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let base = dir.join("base");
        std::fs::write(&base, b"\x7fELF pretend executable").unwrap();
        let out = dir.join("app");
        b.write_executable(&base, &out).unwrap();
        assert_eq!(Bundle::from_executable(&out), Some(b.clone()));
        assert_eq!(Bundle::from_executable(&base), None);
        // Building from a bundled executable replaces its program.
        let out2 = dir.join("app2");
        b.write_executable(&out, &out2).unwrap();
        assert_eq!(
            std::fs::metadata(&out2).unwrap().len(),
            std::fs::metadata(&out).unwrap().len()
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
