//! Files and environment variables, for the library.
//!
//! On a computer these are the operating system's. On WebAssembly (the
//! browser, the course website) there is no file system, so files and
//! environment variables live in memory for the length of one run: a
//! program can write a file and read it back, as the course's lessons do,
//! and nothing touches the visitor's computer. [`reset`] empties them.
//!
//! Time and secure random numbers work the same way. A computer has a
//! clock; WebAssembly without imports has none, so there the host says what
//! time it is ([`set_unix_time`]) and time inside a run is *virtual*:
//! `sleep` moves the clock forward at once instead of waiting, so a program
//! that sleeps prints the same as on a computer, only sooner. Secure random
//! bytes come from a seed the host gives ([`seed_random`]), stretched with
//! SHA-256.

use std::io::{self, BufRead, Write};

pub type Reader = Box<dyn BufRead>;
pub type Writer = Box<dyn Write>;

#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::*;

    pub fn read(path: &str) -> io::Result<Vec<u8>> {
        std::fs::read(path)
    }
    pub fn write(path: &str, data: &[u8]) -> io::Result<()> {
        std::fs::write(path, data)
    }
    pub fn append(path: &str, data: &[u8]) -> io::Result<()> {
        std::fs::OpenOptions::new()
            .append(true)
            .create(true)
            .open(path)?
            .write_all(data)
    }
    pub fn open_read(path: &str) -> io::Result<Reader> {
        Ok(Box::new(io::BufReader::new(std::fs::File::open(path)?)))
    }
    pub fn open_write(path: &str, append: bool) -> io::Result<Writer> {
        let f = if append {
            std::fs::OpenOptions::new()
                .append(true)
                .create(true)
                .open(path)?
        } else {
            std::fs::File::create(path)?
        };
        Ok(Box::new(io::BufWriter::new(f)))
    }
    pub fn exists(path: &str) -> bool {
        std::path::Path::new(path).exists()
    }
    pub fn is_file(path: &str) -> bool {
        std::path::Path::new(path).is_file()
    }
    pub fn is_dir(path: &str) -> bool {
        std::path::Path::new(path).is_dir()
    }
    pub fn create_dir_all(path: &str) -> io::Result<()> {
        std::fs::create_dir_all(path)
    }
    pub fn remove(path: &str) -> io::Result<()> {
        if is_dir(path) {
            std::fs::remove_dir_all(path)
        } else {
            std::fs::remove_file(path)
        }
    }
    pub fn rename(from: &str, to: &str) -> io::Result<()> {
        std::fs::rename(from, to)
    }
    pub fn copy(from: &str, to: &str) -> io::Result<()> {
        std::fs::copy(from, to).map(|_| ())
    }
    pub fn size(path: &str) -> io::Result<u64> {
        std::fs::metadata(path).map(|m| m.len())
    }
    pub fn modified(path: &str) -> io::Result<f64> {
        let m = std::fs::metadata(path)?.modified()?;
        Ok(m.duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0))
    }
    /// The names in a directory (not sorted), and whether each is a directory.
    pub fn list(dir: &str) -> io::Result<Vec<(String, bool)>> {
        Ok(std::fs::read_dir(dir)?
            .filter_map(Result::ok)
            .map(|e| {
                (
                    e.file_name().to_string_lossy().into_owned(),
                    e.path().is_dir(),
                )
            })
            .collect())
    }
    pub fn env(name: &str) -> Option<String> {
        std::env::var(name).ok()
    }
    pub fn set_env(name: &str, value: &str) {
        std::env::set_var(name, value);
    }
    pub fn env_vars() -> Vec<(String, String)> {
        std::env::vars().collect()
    }
    pub fn reset() {}

    pub use std::time::Instant;
    pub fn sleep(d: std::time::Duration) {
        std::thread::sleep(d);
    }
    pub fn unix_now() -> f64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
    pub fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
        getrandom::getrandom(buf).map_err(|e| io::Error::other(e.to_string()))
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};

    #[derive(Default)]
    struct Disk {
        files: BTreeMap<String, (Vec<u8>, f64)>,
        dirs: BTreeSet<String>,
        env: BTreeMap<String, String>,
        tick: f64,
    }

    thread_local! {
        static DISK: RefCell<Disk> = RefCell::new(Disk::default());
    }

    /// `./a/../b//c` is `b/c`.
    fn norm(path: &str) -> String {
        let mut parts: Vec<&str> = Vec::new();
        for p in path.split('/') {
            match p {
                "" | "." => {}
                ".." => {
                    parts.pop();
                }
                p => parts.push(p),
            }
        }
        parts.join("/")
    }

    fn not_found(path: &str) -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, format!("{path} does not exist"))
    }

    fn parent_ok(d: &Disk, path: &str) -> io::Result<()> {
        match path.rsplit_once('/') {
            Some((dir, _)) if !d.dirs.contains(dir) => Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("the folder {dir} does not exist"),
            )),
            _ => Ok(()),
        }
    }

    pub fn read(path: &str) -> io::Result<Vec<u8>> {
        let p = norm(path);
        DISK.with(|d| {
            d.borrow()
                .files
                .get(&p)
                .map(|(b, _)| b.clone())
                .ok_or_else(|| not_found(path))
        })
    }
    pub fn write(path: &str, data: &[u8]) -> io::Result<()> {
        let p = norm(path);
        DISK.with(|d| {
            let mut d = d.borrow_mut();
            parent_ok(&d, &p)?;
            d.tick += 1.0;
            let t = d.tick;
            d.files.insert(p, (data.to_vec(), t));
            Ok(())
        })
    }
    pub fn append(path: &str, data: &[u8]) -> io::Result<()> {
        let mut old = read(path).unwrap_or_default();
        old.extend_from_slice(data);
        write(path, &old)
    }
    pub fn open_read(path: &str) -> io::Result<Reader> {
        Ok(Box::new(io::Cursor::new(read(path)?)))
    }

    /// Writes into memory, and into the file when flushed or dropped.
    struct MemWriter {
        path: String,
        buf: Vec<u8>,
    }
    impl Write for MemWriter {
        fn write(&mut self, data: &[u8]) -> io::Result<usize> {
            self.buf.extend_from_slice(data);
            Ok(data.len())
        }
        fn flush(&mut self) -> io::Result<()> {
            write(&self.path, &self.buf)
        }
    }
    impl Drop for MemWriter {
        fn drop(&mut self) {
            let _ = write(&self.path, &self.buf);
        }
    }

    pub fn open_write(path: &str, append: bool) -> io::Result<Writer> {
        let buf = if append {
            read(path).unwrap_or_default()
        } else {
            Vec::new()
        };
        let w = MemWriter {
            path: path.to_string(),
            buf,
        };
        write(path, &w.buf)?;
        Ok(Box::new(w))
    }
    pub fn exists(path: &str) -> bool {
        is_file(path) || is_dir(path)
    }
    pub fn is_file(path: &str) -> bool {
        let p = norm(path);
        DISK.with(|d| d.borrow().files.contains_key(&p))
    }
    pub fn is_dir(path: &str) -> bool {
        let p = norm(path);
        p.is_empty() || DISK.with(|d| d.borrow().dirs.contains(&p))
    }
    pub fn create_dir_all(path: &str) -> io::Result<()> {
        let p = norm(path);
        DISK.with(|d| {
            let mut d = d.borrow_mut();
            let mut so_far = String::new();
            for part in p.split('/').filter(|s| !s.is_empty()) {
                if !so_far.is_empty() {
                    so_far.push('/');
                }
                so_far.push_str(part);
                d.dirs.insert(so_far.clone());
            }
            Ok(())
        })
    }
    pub fn remove(path: &str) -> io::Result<()> {
        let p = norm(path);
        DISK.with(|d| {
            let mut d = d.borrow_mut();
            if d.files.remove(&p).is_some() {
                return Ok(());
            }
            if d.dirs.remove(&p) {
                let inside = format!("{p}/");
                d.files.retain(|k, _| !k.starts_with(&inside));
                d.dirs.retain(|k| !k.starts_with(&inside));
                return Ok(());
            }
            Err(not_found(path))
        })
    }
    pub fn rename(from: &str, to: &str) -> io::Result<()> {
        let data = read(from)?;
        write(to, &data)?;
        remove(from)
    }
    pub fn copy(from: &str, to: &str) -> io::Result<()> {
        let data = read(from)?;
        write(to, &data)
    }
    pub fn size(path: &str) -> io::Result<u64> {
        read(path).map(|b| b.len() as u64)
    }
    pub fn modified(path: &str) -> io::Result<f64> {
        let p = norm(path);
        DISK.with(|d| {
            d.borrow()
                .files
                .get(&p)
                .map(|(_, t)| *t)
                .ok_or_else(|| not_found(path))
        })
    }
    pub fn list(dir: &str) -> io::Result<Vec<(String, bool)>> {
        let p = norm(dir);
        if !is_dir(&p) {
            return Err(not_found(dir));
        }
        let prefix = if p.is_empty() {
            String::new()
        } else {
            format!("{p}/")
        };
        let direct = |k: &str| -> Option<String> {
            let rest = k.strip_prefix(&prefix)?;
            (!rest.is_empty() && !rest.contains('/')).then(|| rest.to_string())
        };
        DISK.with(|d| {
            let d = d.borrow();
            let mut out: Vec<(String, bool)> = d
                .files
                .keys()
                .filter_map(|k| direct(k).map(|n| (n, false)))
                .collect();
            out.extend(d.dirs.iter().filter_map(|k| direct(k).map(|n| (n, true))));
            Ok(out)
        })
    }
    pub fn env(name: &str) -> Option<String> {
        DISK.with(|d| d.borrow().env.get(name).cloned())
    }
    pub fn set_env(name: &str, value: &str) {
        DISK.with(|d| {
            d.borrow_mut()
                .env
                .insert(name.to_string(), value.to_string());
        });
    }
    pub fn env_vars() -> Vec<(String, String)> {
        DISK.with(|d| {
            d.borrow()
                .env
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        })
    }
    pub fn reset() {
        DISK.with(|d| *d.borrow_mut() = Disk::default());
    }

    /// Every file, in the order of their paths: what a run wrote, for the
    /// host to show (pictures, charts, CSV files).
    pub fn files() -> Vec<(String, Vec<u8>)> {
        DISK.with(|d| {
            d.borrow()
                .files
                .iter()
                .map(|(k, (b, _))| (k.clone(), b.clone()))
                .collect()
        })
    }

    use std::cell::Cell;
    use std::time::Duration;

    thread_local! {
        /// Virtual time since the module started.
        static ELAPSED: Cell<Duration> = const { Cell::new(Duration::ZERO) };
        /// The Unix time the host last gave, and `ELAPSED` at that moment.
        static UNIX: Cell<(f64, Duration)> = const { Cell::new((0.0, Duration::ZERO)) };
        static SEED: RefCell<([u8; 32], u64)> = const { RefCell::new(([0; 32], 0)) };
    }

    /// A moment on the virtual clock.
    #[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Instant(Duration);

    impl Instant {
        pub fn now() -> Instant {
            Instant(ELAPSED.with(Cell::get))
        }
        pub fn duration_since(&self, earlier: Instant) -> Duration {
            self.0.saturating_sub(earlier.0)
        }
        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            self.duration_since(earlier)
        }
        pub fn elapsed(&self) -> Duration {
            Instant::now().duration_since(*self)
        }
    }
    impl std::ops::Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, d: Duration) -> Instant {
            Instant(self.0 + d)
        }
    }
    impl std::ops::AddAssign<Duration> for Instant {
        fn add_assign(&mut self, d: Duration) {
            self.0 += d;
        }
    }
    impl std::ops::Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, d: Duration) -> Instant {
            Instant(self.0.saturating_sub(d))
        }
    }
    impl std::ops::Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, other: Instant) -> Duration {
            self.duration_since(other)
        }
    }

    /// Moves the virtual clock forward; nothing waits.
    pub fn sleep(d: Duration) {
        ELAPSED.with(|e| e.set(e.get() + d));
    }
    pub fn unix_now() -> f64 {
        let (base, at) = UNIX.with(Cell::get);
        base + (ELAPSED.with(Cell::get) - at).as_secs_f64()
    }
    /// The host tells the module what time it is (seconds since 1970).
    pub fn set_unix_time(seconds: f64) {
        UNIX.with(|u| u.set((seconds, ELAPSED.with(Cell::get))));
    }
    /// The host gives fresh random bytes; until it does, there are none.
    pub fn seed_random(seed: &[u8]) {
        use sha2::{Digest, Sha256};
        let key: [u8; 32] = Sha256::digest(seed).into();
        SEED.with(|s| *s.borrow_mut() = (key, 1));
    }
    pub fn random_bytes(buf: &mut [u8]) -> io::Result<()> {
        use sha2::{Digest, Sha256};
        SEED.with(|s| {
            let mut s = s.borrow_mut();
            if s.1 == 0 {
                return Err(io::Error::other("the host gave no random seed"));
            }
            for chunk in buf.chunks_mut(32) {
                let mut h = Sha256::new();
                h.update(s.0);
                h.update(s.1.to_le_bytes());
                s.1 += 1;
                chunk.copy_from_slice(&h.finalize()[..chunk.len()]);
            }
            Ok(())
        })
    }
}

pub use imp::*;

pub fn read_to_string(path: &str) -> io::Result<String> {
    String::from_utf8(read(path)?).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("{path} is not text (UTF-8)"),
        )
    })
}
