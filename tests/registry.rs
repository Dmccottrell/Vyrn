//! The package registry, through the `vyrn` command: publishing into a
//! registry, then finding the package by name.

use std::path::Path;
use std::process::Command;

fn vyrn(dir: &Path, registry: &Path, args: &[&str]) -> (bool, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_vyrn"))
        .args(args)
        .current_dir(dir)
        .env("VYRN_REGISTRY", registry)
        .env("VYRN_HOME", dir.join(".vyrn-home"))
        .output()
        .expect("vyrn runs");
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    (out.status.success(), text)
}

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn publishing_then_finding_by_name() {
    let root = std::env::temp_dir().join(format!("vyrn-registry-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("registry")).unwrap();
    let index = root.join("registry/index.json");
    std::fs::write(
        &index,
        r#"{"packages": [{"name": "physics-extra", "description": "More physics for games", "url": "github.com/ann/physics-extra", "versions": ["v0.9.0", "v1.10.0", "v1.9.0"], "keywords": ["games"], "license": "MIT"}]}"#,
    )
    .unwrap();

    let (ok, _) = vyrn(&root, &index, &["new", "greeter"]);
    assert!(ok);
    let pkg = root.join("greeter");
    // Without the registry fields, publishing says what to add.
    let (ok, out) = vyrn(&pkg, &index, &["publish", "--to", "../registry"]);
    assert!(
        !ok && out.contains("repository = \"github.com/you/your-package\""),
        "{out}"
    );
    let manifest = pkg.join("vyrn.toml");
    let mut text = std::fs::read_to_string(&manifest).unwrap();
    text.push_str("version = \"1.0.0\"\ndescription = \"Friendly greetings\"\nrepository = \"github.com/ann/greeter\"\nkeywords = \"greeting, example\"\n");
    std::fs::write(&manifest, text).unwrap();

    if git(&pkg, &["init", "-q"])
        && git(&pkg, &["add", "-A"])
        && git(&pkg, &["commit", "-qm", "first"])
    {
        // Untagged versions can't be fetched, so they can't be published.
        let (ok, out) = vyrn(&pkg, &index, &["publish", "--to", "../registry"]);
        assert!(!ok && out.contains("git tag v1.0.0"), "{out}");
        assert!(git(&pkg, &["tag", "v1.0.0"]));
        let (ok, out) = vyrn(&pkg, &index, &["publish", "--to", "../registry"]);
        assert!(ok, "{out}");
        assert!(out.contains("added greeter v1.0.0"), "{out}");
    } else {
        // No git here: write the entry the way publish would.
        let mut entries =
            vyrn::project::registry::parse_index(&std::fs::read_to_string(&index).unwrap())
                .unwrap();
        vyrn::project::registry::merge(
            &mut entries,
            vyrn::project::registry::Entry {
                name: "greeter".into(),
                description: "Friendly greetings".into(),
                url: "github.com/ann/greeter".into(),
                versions: vec!["v1.0.0".into()],
                keywords: vec!["greeting".into(), "example".into()],
                license: String::new(),
            },
        )
        .unwrap();
        std::fs::write(&index, vyrn::project::registry::render_index(&entries)).unwrap();
    }

    let (ok, out) = vyrn(&root, &index, &["search", "greet"]);
    assert!(
        ok && out.starts_with("greeter v1.0.0  Friendly greetings"),
        "{out}"
    );
    let (_, out) = vyrn(&root, &index, &["search", "games"]);
    assert!(out.starts_with("physics-extra v1.10.0"), "{out}");
    let (ok, out) = vyrn(&root, &index, &["info", "physics-extra"]);
    assert!(
        ok && out.contains("use \"github.com/ann/physics-extra@v1\""),
        "{out}"
    );
    let (ok, out) = vyrn(&root, &index, &["add", "no-such-package"]);
    assert!(!ok && out.contains("vyrn search no-such-package"), "{out}");
    let _ = std::fs::remove_dir_all(&root);
}
