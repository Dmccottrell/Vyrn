//! Projects with several files and URL modules, against an in-memory git
//! host: resolving versions, locking, the content-addressed cache, working
//! offline, and refusing code that changed after it was locked.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use vyrn::project::fetch::{FetchError, Fetcher};
use vyrn::project::lock::Lockfile;
use vyrn::project::resolve::Resolver;
use vyrn::project::url::{GitRef, ModuleUrl};
use vyrn::project::{load_program, loader};
use vyrn::vm::{compile_checked, Captured, Vm};

/// A git host in memory: tags, and the files at each commit.
#[derive(Default)]
struct FakeHost {
    tags: RefCell<Vec<(String, String)>>,
    commits: RefCell<HashMap<String, Vec<(String, String)>>>,
    downloads: Cell<usize>,
}

impl FakeHost {
    fn publish(&self, tag: &str, commit: &str, files: &[(&str, &str)]) {
        self.tags.borrow_mut().push((tag.into(), commit.into()));
        self.commits.borrow_mut().insert(
            commit.into(),
            files
                .iter()
                .map(|(p, t)| (p.to_string(), t.to_string()))
                .collect(),
        );
    }
}

impl Fetcher for FakeHost {
    fn list_refs(&self, _: &ModuleUrl) -> Result<Vec<GitRef>, FetchError> {
        Ok(self
            .tags
            .borrow()
            .iter()
            .map(|(t, c)| GitRef {
                name: format!("refs/tags/{t}"),
                commit: c.clone(),
            })
            .collect())
    }

    fn archive(&self, _: &ModuleUrl, commit: &str) -> Result<Vec<u8>, FetchError> {
        self.downloads.set(self.downloads.get() + 1);
        let commits = self.commits.borrow();
        let files = commits
            .get(commit)
            .ok_or_else(|| FetchError(format!("no commit {commit}")))?;
        let mut builder = tar::Builder::new(Vec::new());
        for (path, text) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(text.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder
                .append_data(
                    &mut header,
                    format!("repo-{commit}/{path}"),
                    text.as_bytes(),
                )
                .unwrap();
        }
        let tar = builder.into_inner().unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        std::io::Write::write_all(&mut gz, &tar).unwrap();
        Ok(gz.finish().unwrap())
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!("vyrn-test-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn write(&self, path: &str, text: &str) -> PathBuf {
        let p = self.0.join(path);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, text).unwrap();
        p
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(loaded: &loader::Loaded) -> String {
    let program = compile_checked(&loaded.module).unwrap_or_else(|e| panic!("{e:?}"));
    let out = Captured::default();
    let mut vm = Vm::with_output(program, Box::new(out.clone()));
    vm.run_main(&[]).unwrap();
    out.text()
}

fn load(entry: &Path, root: &Path, resolver: &mut Resolver) -> Result<loader::Loaded, Vec<String>> {
    loader::load(
        entry,
        root,
        &mut loader::Disk {
            resolver: Some(resolver),
        },
    )
    .map_err(|(d, _)| d.into_iter().map(|d| d.message).collect())
}

const NEURAL_V1: &[(&str, &str)] = &[
    ("lib.vyrn", "use layers (relu1)\n\nfn version() -> str: return \"1.0\"\nfn activate(x: float) -> float: return relu1(x)\n"),
    ("layers.vyrn", "fn relu1(x: float) -> float: return x if x > 0.0 else 0.0\n"),
];

#[test]
fn url_modules_are_fetched_locked_and_cached() {
    let dir = TempDir::new("url");
    let main = dir.write(
        "app/main.vyrn",
        "use \"example.com/ann/neural@v1\"\n\nmain:\n    print(neural.version(), neural.activate(-2.0), neural.activate(3.0))\nend\n",
    );
    let host = FakeHost::default();
    host.publish("v1.0.0", &"a".repeat(40), NEURAL_V1);
    host.publish(
        "v2.0.0",
        &"b".repeat(40),
        &[("lib.vyrn", "fn version() -> str: return \"2.0\"\n")],
    );
    let cache = dir.0.join("cache");

    // First run: resolves v1 to v1.0.0, downloads it, locks it.
    let mut resolver = Resolver::new(&host, cache.clone(), Lockfile::default());
    let loaded = load(&main, &dir.0.join("app"), &mut resolver).unwrap();
    assert_eq!(run(&loaded), "1.0 0.0 3.0\n");
    let locked = resolver.lock.modules["example.com/ann/neural@v1"].clone();
    assert_eq!(locked.version, "v1.0.0");
    assert_eq!(locked.commit, "a".repeat(40));
    assert!(cache.join(&locked.hash).join("lib.vyrn").is_file());
    assert_eq!(host.downloads.get(), 1);

    // A newer v1 is published; the lock keeps the program on what it tested.
    host.publish("v1.1.0", &"c".repeat(40), NEURAL_V1);
    let lock = resolver.lock.clone();
    let mut resolver = Resolver::new(&host, cache.clone(), lock.clone());
    resolver.offline = true;
    let loaded = load(&main, &dir.0.join("app"), &mut resolver).unwrap();
    assert_eq!(run(&loaded), "1.0 0.0 3.0\n");
    assert_eq!(host.downloads.get(), 1, "the cache answered, offline");

    // The repository is rewritten under the same commit: refused.
    host.commits.borrow_mut().insert(
        "a".repeat(40),
        vec![(
            "lib.vyrn".into(),
            "fn version() -> str: return \"evil\"\n".into(),
        )],
    );
    std::fs::remove_dir_all(&cache).unwrap();
    let mut resolver = Resolver::new(&host, cache.clone(), lock);
    let errors = load(&main, &dir.0.join("app"), &mut resolver)
        .err()
        .unwrap();
    assert!(
        errors[0].contains("changed since it was locked"),
        "{errors:?}"
    );
}

#[test]
fn missing_versions_list_what_exists() {
    let dir = TempDir::new("versions");
    let main = dir.write(
        "main.vyrn",
        "use \"example.com/ann/neural@v3\"\nmain: print(1)\n",
    );
    let host = FakeHost::default();
    host.publish("v1.0.0", &"a".repeat(40), NEURAL_V1);
    let mut resolver = Resolver::new(&host, dir.0.join("cache"), Lockfile::default());
    let errors = load(&main, &dir.0, &mut resolver).err().unwrap();
    assert!(errors[0].contains("has no version matching"), "{errors:?}");
    assert!(errors[0].contains("v1.0.0"), "{errors:?}");
}

#[test]
fn projects_lock_into_vyrn_lock() {
    let dir = TempDir::new("lockfile");
    dir.write("vyrn.toml", "[package]\nname = \"app\"\n");
    let main = dir.write(
        "main.vyrn",
        "use \"example.com/ann/neural@v1\" (activate)\nmain: print(activate(1.5))\n",
    );
    let host = FakeHost::default();
    host.publish("v1.0.0", &"a".repeat(40), NEURAL_V1);
    std::env::set_var("VYRN_HOME", dir.0.join("home"));
    let loaded = load_program(&main, &host, false).unwrap_or_else(|(d, _)| panic!("{d:?}"));
    assert_eq!(run(&loaded), "1.5\n");
    let lock = std::fs::read_to_string(dir.0.join("vyrn.lock")).unwrap();
    assert!(
        lock.contains("url = \"example.com/ann/neural@v1\""),
        "{lock}"
    );
    assert!(lock.contains("version = \"v1.0.0\""), "{lock}");
    assert!(dir.0.join("home/modules").is_dir());
}

#[test]
fn local_modules_have_their_own_namespaces() {
    let dir = TempDir::new("local");
    dir.write("geo/shapes.vyrn", "struct Point:\n    x: float\n    y: float\nend\nfn load() -> str: return \"geo\"\nfn norm(p: Point) -> float: return sqrt(p.x * p.x + p.y * p.y)\n");
    dir.write(
        "store.vyrn",
        "fn load() -> str: return \"store\"\nmain: print(\"modules do not run their main\")\n",
    );
    let main = dir.write(
        "main.vyrn",
        "use geo.shapes\nuse geo.shapes (Point as P)\nuse store\n\nmain:\n    let load = 7\n    print(shapes.load(), store.load(), load)\n    print(shapes.norm(P(3.0, 4.0)))\n    match P(0.0, 1.0):\n        is shapes.Point(x, y): print(x, y)\n    end\nend\n",
    );
    let host = FakeHost::default();
    let mut resolver = Resolver::new(&host, dir.0.join("cache"), Lockfile::default());
    let loaded = load(&main, &dir.0, &mut resolver).unwrap();
    assert_eq!(run(&loaded), "geo store 7\n5.0\n0.0 1.0\n");

    let bad = dir.write(
        "bad.vyrn",
        "use store\nuse geo.lines\nmain: print(store.save())\n",
    );
    let errors = load(&bad, &dir.0, &mut resolver).err().unwrap();
    assert_eq!(errors, ["cannot find module `geo.lines`"]);
    let bad = dir.write("bad2.vyrn", "use store (save)\nmain: print(1)\n");
    let errors = load(&bad, &dir.0, &mut resolver).err().unwrap();
    assert_eq!(errors, ["module `store` has no `save`"]);
    let bad = dir.write("bad3.vyrn", "use store\nmain: print(store.save())\n");
    let errors = load(&bad, &dir.0, &mut resolver).err().unwrap();
    assert_eq!(errors, ["module `store` has no `save`"]);
}

#[test]
fn errors_name_the_file_they_are_in() {
    let dir = TempDir::new("files");
    dir.write(
        "util.vyrn",
        "fn half(x: int) -> int:\n    return \"half\"\nend\n",
    );
    let main = dir.write("main.vyrn", "use util\nmain: print(util.half(4))\n");
    let host = FakeHost::default();
    let mut resolver = Resolver::new(&host, dir.0.join("cache"), Lockfile::default());
    let loaded = load(&main, &dir.0, &mut resolver).unwrap();
    let mut errors = compile_checked(&loaded.module).err().unwrap();
    loader::attach_files(&mut errors, &loaded.files);
    assert_eq!(errors.len(), 1);
    assert!(errors[0].file.as_deref().unwrap().ends_with("util.vyrn"));
    assert_eq!(errors[0].span.start.line, 2);
}

#[test]
fn url_modules_can_be_renamed_with_as() {
    let dir = TempDir::new("alias");
    let main = dir.write(
        "main.vyrn",
        "use \"github.com/Dmccottrell/A.I-SML@v3\" as yuvra\nmain: print(yuvra.version(), yuvra.activate(-1.0))\n",
    );
    let host = FakeHost::default();
    host.publish("v3.1.0", &"d".repeat(40), NEURAL_V1);
    host.publish("v3.0.2", &"e".repeat(40), NEURAL_V1);
    let mut resolver = Resolver::new(&host, dir.0.join("cache"), Lockfile::default());
    let loaded = load(&main, &dir.0, &mut resolver).unwrap();
    assert_eq!(run(&loaded), "1.0 0.0\n");
    assert_eq!(
        resolver.lock.modules["github.com/Dmccottrell/A.I-SML@v3"].version,
        "v3.1.0"
    );
    // Without `as`, the name comes from the repository: `A_I_SML`.
    assert_eq!(
        vyrn::parser::url_module_name("github.com/Dmccottrell/A.I-SML@v3").as_deref(),
        Some("A_I_SML")
    );
}
