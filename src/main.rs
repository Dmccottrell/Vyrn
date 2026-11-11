//! The `vyrn` command-line tool: one command for everything.
//!
//! | Command | Does |
//! |---|---|
//! | `vyrn new <name>` | Creates a project: `vyrn.toml`, `main.vyrn`, a module and a test |
//! | `vyrn run [file] [args]` | Runs a program (the project's main file by default) |
//! | `vyrn check [file]` | Reports type, `let`, `strict` and borrow errors without running |
//! | `vyrn test [path]` | Runs every `test_*` function in `tests/` |
//! | `vyrn fmt [--check] [files]` | Formats files in the one Vyrn style |
//! | `vyrn add <url>` | Fetches a URL module and locks it in `vyrn.lock` |
//! | `vyrn fetch` | Downloads every locked module, for working offline |
//! | `vyrn build [file] [-o out]` | Makes one executable file that runs the program |
//! | `vyrn bytecode`, `parse`, `lex <file>` | Shows the compiler's view of a file |
//!
//! An executable made by `vyrn build` is this program with the bundled
//! source attached; it runs that program instead of reading commands.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use vyrn::project::bundle::{Bundle, Recorder};
use vyrn::project::deploy::{self, Target};
use vyrn::project::loader::{self, Disk, SourceFile};
use vyrn::project::manifest::{self, Manifest};
use vyrn::project::{self, lock, resolve};
use vyrn::vm::{self, Vm};
use vyrn::{parse_recovering, pretty, Diagnostic, Lexer};

const USAGE: &str = "\
usage: vyrn <command> [arguments]

  (no command)           start the interactive prompt (also: vyrn repl)
  file.vyrn [args...]    run a file (same as vyrn run file.vyrn)
  new <name>             create a project
  run [file] [args...]   run a program (the project's main file by default)
  check [file]           find errors without running
  test [path]            run the test_* functions in tests/
  fmt [--check] [files]  format code in the one Vyrn style
  add <url>              fetch and lock a module, e.g. github.com/ann/neural@v1
  fetch                  download every locked module, to work offline
  build [file] [-o out] [--target=T]
                         make one executable file; T is native (default),
                         linux-gnu, linux-musl, linux-arm64, macos, windows,
                         or wasm32-edge for Vercel, Cloudflare and Deno
  lsp                    run the language server (for editors)
  bytecode <file>        show the compiled bytecode
  parse <file>           show the syntax tree
  lex <file>             show the tokens";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let Some(bundle) = std::env::current_exe()
        .ok()
        .and_then(|exe| Bundle::from_executable(&exe))
    {
        return run_bundle(bundle, &args);
    }
    let Some((command, rest)) = args.split_first() else {
        return repl();
    };
    match command.as_str() {
        "repl" => repl(),
        "lsp" => {
            vyrn::lsp::run_stdio();
            ExitCode::SUCCESS
        }
        "--version" | "-V" | "version" => {
            println!("vyrn {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        file if file.ends_with(".vyrn") => run(&args),
        "new" => new(rest),
        "run" => run(rest),
        "check" => check(rest),
        "test" => test(rest),
        "fmt" => fmt(rest),
        "add" => add(rest),
        "fetch" => fetch(),
        "build" => build(rest),
        "bytecode" | "parse" | "lex" => {
            let Some(file) = rest.first() else {
                eprintln!("usage: vyrn {command} <file.vyrn>");
                return ExitCode::from(2);
            };
            let src = match std::fs::read_to_string(file) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("error: cannot read {file}: {e}");
                    return ExitCode::FAILURE;
                }
            };
            match command.as_str() {
                "lex" => lex(file, &src),
                "parse" => parse(file, &src),
                _ => bytecode(file),
            }
        }
        "help" | "--help" | "-h" => {
            println!("{USAGE}");
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!("error: unknown command `{command}`\n\n{USAGE}");
            ExitCode::from(2)
        }
    }
}

// ==== loading programs ======================================================

/// A path as short as possible: relative to the current directory when it
/// is inside it.
fn shown(path: &Path) -> String {
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|c| c.canonicalize().ok());
    let full = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    match cwd.as_deref().and_then(|c| full.strip_prefix(c).ok()) {
        Some(rel) => rel.display().to_string(),
        None => path.display().to_string(),
    }
}

/// Prints diagnostics, each against the file it points into.
fn report(diagnostics: &[Diagnostic], files: &[SourceFile]) {
    for d in diagnostics {
        let path = d.file.clone().unwrap_or_else(|| "<program>".into());
        let text = files
            .iter()
            .find(|f| f.path.display().to_string() == path)
            .map(|f| f.text.clone())
            .or_else(|| std::fs::read_to_string(&path).ok())
            .unwrap_or_default();
        eprintln!("{}", d.render(&shown(Path::new(&path)), &text));
    }
}

/// The file a command works on: the one given, or the project's main file.
fn entry_file(arg: Option<&String>) -> Result<PathBuf, String> {
    if let Some(a) = arg {
        let path = PathBuf::from(a);
        if path.is_dir() {
            return project_main(&path);
        }
        return Ok(path);
    }
    project_main(Path::new("."))
}

fn project_main(dir: &Path) -> Result<PathBuf, String> {
    let root = manifest::find_root(dir).ok_or_else(|| {
        "no file given, and this is not a project (no vyrn.toml here or above); run `vyrn new <name>` to start one".to_string()
    })?;
    let m = read_manifest(&root)?;
    Ok(root.join(m.main))
}

fn read_manifest(root: &Path) -> Result<Manifest, String> {
    let text = std::fs::read_to_string(root.join(manifest::MANIFEST))
        .map_err(|e| format!("cannot read vyrn.toml: {e}"))?;
    Manifest::parse(&text)
}

/// Loads and compiles a program with its modules, reporting any errors.
fn compile_program(entry: &Path) -> Option<(vm::Program, Vec<SourceFile>)> {
    let fetcher = project::default_fetcher();
    let loaded = match project::load_program(entry, fetcher.as_ref(), false) {
        Ok(l) => l,
        Err((diagnostics, files)) => {
            report(&diagnostics, &files);
            return None;
        }
    };
    match vm::compile_checked(&loaded.module) {
        Ok(program) => Some((program, loaded.files)),
        Err(mut diagnostics) => {
            loader::attach_files(&mut diagnostics, &loaded.files);
            report(&diagnostics, &loaded.files);
            None
        }
    }
}

thread_local! {
    static PROFILE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

fn execute(program: vm::Program, args: &[String], files: &[SourceFile]) -> ExitCode {
    let uses_gpu = mentions_gpu(files);
    vyrn::vm::stdlib::set_args(args.to_vec());
    let mut machine = Vm::new(program);
    let profile = PROFILE.with(|p| p.get());
    if profile {
        machine.enable_profile();
    }
    #[cfg(feature = "gpu")]
    use_gpu(&mut machine, uses_gpu);
    #[cfg(not(feature = "gpu"))]
    let _ = uses_gpu;
    let result = machine.run_main(args);
    if profile {
        eprintln!("\n{}", machine.profile_report());
    }
    for warning in vyrn::vm::task::take_unobserved() {
        eprintln!("warning: {warning}");
    }
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{}", render_runtime_error(&error, files));
            ExitCode::FAILURE
        }
    }
}

/// A failure nothing caught, shown like a compile error: where it happened,
/// the line of code, what usually fixes it, and the calls that led there.
fn render_runtime_error(error: &vm::RuntimeError, files: &[SourceFile]) -> String {
    let mut out = format!("error: {}: {}", error.kind, error.message);
    let user_frames: Vec<&vyrn::vm::machine::TraceLine> = error
        .trace
        .iter()
        .filter(|t| !vyrn::prelude::is_prelude_offset(t.file << 24) && t.line > 0)
        .collect();
    let location = |t: &vyrn::vm::machine::TraceLine| -> (String, Option<String>) {
        match files.get(t.file as usize) {
            Some(f) => (
                format!("{}:{}", shown(&f.path), t.line),
                f.text.lines().nth(t.line as usize - 1).map(str::to_string),
            ),
            None => (format!("line {}", t.line), None),
        }
    };
    if let Some(first) = user_frames.first() {
        let (place, code) = location(first);
        let gutter = first.line.to_string().len();
        out.push_str(&format!("\n{}--> {place}", " ".repeat(gutter)));
        if let Some(code) = code {
            out.push_str(&format!(
                "\n{} |\n{} | {}\n{} |",
                " ".repeat(gutter),
                first.line,
                code,
                " ".repeat(gutter)
            ));
        }
        if let Some(hint) = error.hint() {
            out.push_str(&format!("\n{} = help: {hint}", " ".repeat(gutter)));
        }
        for t in &user_frames {
            let (place, _) = location(t);
            out.push_str(&format!("\n  in {} ({place})", t.function));
        }
    } else if let Some(hint) = error.hint() {
        out.push_str(&format!("\n  = help: {hint}"));
    }
    out
}

/// Runs `gpu[...]` blocks on a real GPU when there is one. Set
/// `VYRN_GPU=off` to keep everything on the CPU.
#[cfg(feature = "gpu")]
fn use_gpu(machine: &mut Vm, uses_gpu: bool) {
    if std::env::var("VYRN_GPU").is_ok_and(|v| v == "off") {
        return;
    }
    match vyrn::vm::gpu::WgpuBackend::new() {
        Ok(backend) => {
            if std::env::var_os("VYRN_VERBOSE").is_some() {
                eprintln!("using {}", vyrn::vm::TensorBackend::name(&backend));
            }
            machine.set_gpu_backend(Box::new(backend));
        }
        // Only worth mentioning to programs that ask for a GPU.
        Err(e) if uses_gpu => eprintln!("note: {e}; `gpu` blocks run on the CPU"),
        Err(_) => {}
    }
}

fn mentions_gpu(files: &[SourceFile]) -> bool {
    files.iter().any(|f| f.text.contains("gpu"))
}

// ==== commands =============================================================

fn run(args: &[String]) -> ExitCode {
    // `vyrn run --profile file.vyrn`: afterwards, which functions took the
    // time.
    let profile = args.first().is_some_and(|a| a == "--profile");
    let args = if profile { &args[1..] } else { args };
    PROFILE.with(|p| p.set(profile));
    // `vyrn run` runs the project; `vyrn run file.vyrn a b` passes `a b`.
    let (file, program_args) = match args.first() {
        Some(a) if a.ends_with(".vyrn") || Path::new(a).is_dir() => (Some(a), &args[1..]),
        _ => (None, args),
    };
    let entry = match entry_file(file) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let Some((program, files)) = compile_program(&entry) else {
        return ExitCode::FAILURE;
    };
    execute(program, program_args, &files)
}

fn run_bundle(mut bundle: Bundle, args: &[String]) -> ExitCode {
    let entry = PathBuf::from(&bundle.entry);
    let root = PathBuf::from(&bundle.root);
    let loaded = match loader::load(&entry, &root, &mut bundle) {
        Ok(l) => l,
        Err((diagnostics, files)) => {
            report(&diagnostics, &files);
            return ExitCode::FAILURE;
        }
    };
    match vm::compile_checked(&loaded.module) {
        Ok(program) => execute(program, args, &loaded.files),
        Err(mut diagnostics) => {
            loader::attach_files(&mut diagnostics, &loaded.files);
            report(&diagnostics, &loaded.files);
            ExitCode::FAILURE
        }
    }
}

fn check(args: &[String]) -> ExitCode {
    let entry = match entry_file(args.first()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let fetcher = project::default_fetcher();
    let loaded = match project::load_program(&entry, fetcher.as_ref(), false) {
        Ok(l) => l,
        Err((diagnostics, files)) => {
            report(&diagnostics, &files);
            return ExitCode::FAILURE;
        }
    };
    let mut diagnostics = vyrn::check::check(&loaded.module);
    if diagnostics.is_empty() {
        println!("{}: ok", entry.display());
        return ExitCode::SUCCESS;
    }
    loader::attach_files(&mut diagnostics, &loaded.files);
    report(&diagnostics, &loaded.files);
    ExitCode::FAILURE
}

fn new(args: &[String]) -> ExitCode {
    let Some(name) = args.first() else {
        eprintln!("usage: vyrn new <name>");
        return ExitCode::from(2);
    };
    let dir = PathBuf::from(name);
    let package = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| name.clone());
    if dir.exists() {
        eprintln!("error: `{name}` already exists; choose another name or remove it");
        return ExitCode::FAILURE;
    }
    let files = [
        (manifest::MANIFEST, Manifest::new(&package).render()),
        (
            "main.vyrn",
            format!(
                "# {package}: run it with `vyrn run`.\n\nuse greeting (greet)\n\nmain:\n    print(greet(\"world\"))\nend\n"
            ),
        ),
        (
            "greeting.vyrn",
            "create greet(name: str) -> str:\n    return \"Hello, {name}!\"\nend\n".to_string(),
        ),
        (
            "tests/test_greeting.vyrn",
            "# Run with `vyrn test`.\n\nuse greeting (greet)\n\ncreate test_greet():\n    assert(greet(\"Ada\") == \"Hello, Ada!\")\nend\n".to_string(),
        ),
        (".gitignore", "build/\n".to_string()),
    ];
    for (path, text) in files {
        let path = dir.join(path);
        let written = path
            .parent()
            .map_or(Ok(()), std::fs::create_dir_all)
            .and_then(|_| std::fs::write(&path, text));
        if let Err(e) = written {
            eprintln!("error: cannot write {}: {e}", path.display());
            return ExitCode::FAILURE;
        }
    }
    println!("created {name}/\n\n    cd {name}\n    vyrn run\n    vyrn test");
    ExitCode::SUCCESS
}

/// Runs the `test_*` functions of every test file.
fn test(args: &[String]) -> ExitCode {
    let start = args.first().map(PathBuf::from).unwrap_or_else(|| {
        manifest::find_root(Path::new("."))
            .map(|r| r.join("tests"))
            .unwrap_or_else(|| PathBuf::from("tests"))
    });
    let mut files = Vec::new();
    collect_vyrn_files(&start, &mut files);
    if files.is_empty() {
        eprintln!(
            "no tests found in {}; a test is a `create test_...()` in a .vyrn file there",
            start.display()
        );
        return ExitCode::FAILURE;
    }
    let (mut passed, mut failed) = (0, 0);
    for file in &files {
        let Some((program, _)) = compile_program(file) else {
            failed += 1;
            continue;
        };
        let names: Vec<String> = program
            .globals
            .iter()
            .map(|g| g.name.clone())
            .filter(|n| n.starts_with("test_"))
            .collect();
        drop(program);
        for name in names {
            // A fresh program for each test, so tests cannot affect each other.
            let Some((program, _)) = compile_program(file) else {
                failed += 1;
                break;
            };
            let out = vm::Captured::default();
            let mut machine = Vm::with_output(program, Box::new(out.clone()));
            let label = format!("{}::{name}", shown(file));
            match machine.call_global(&name, Vec::new()) {
                Ok(_) => {
                    passed += 1;
                    println!("ok      {label}");
                }
                Err(e) => {
                    failed += 1;
                    println!("FAILED  {label}\n        {e}");
                    for line in out.text().lines() {
                        println!("        | {line}");
                    }
                }
            }
        }
    }
    println!("\n{passed} passed, {failed} failed");
    if failed == 0 {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

fn collect_vyrn_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        if path.extension().is_some_and(|e| e == "vyrn") {
            out.push(path.to_path_buf());
        }
        return;
    }
    let Ok(entries) = std::fs::read_dir(path) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        let name = p
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        if name.starts_with('.') || name == "build" || name == "target" {
            continue;
        }
        collect_vyrn_files(&p, out);
    }
}

fn fmt(args: &[String]) -> ExitCode {
    let check_only = args.iter().any(|a| a == "--check");
    let mut files = Vec::new();
    for a in args.iter().filter(|a| *a != "--check") {
        collect_vyrn_files(Path::new(a), &mut files);
    }
    if args.iter().all(|a| a == "--check") {
        let root = manifest::find_root(Path::new(".")).unwrap_or_else(|| PathBuf::from("."));
        collect_vyrn_files(&root, &mut files);
    }
    let mut status = ExitCode::SUCCESS;
    for file in files {
        let Ok(src) = std::fs::read_to_string(&file) else {
            continue;
        };
        match vyrn::fmt::format(&src) {
            Ok(formatted) if formatted == src => {}
            Ok(formatted) => {
                if check_only {
                    println!("would reformat {}", file.display());
                    status = ExitCode::FAILURE;
                } else if let Err(e) = std::fs::write(&file, formatted) {
                    eprintln!("error: cannot write {}: {e}", file.display());
                    status = ExitCode::FAILURE;
                } else {
                    println!("formatted {}", file.display());
                }
            }
            Err(e) => {
                eprintln!("{}: {e}", file.display());
                status = ExitCode::FAILURE;
            }
        }
    }
    status
}

/// Runs `work` with the project's resolver, then saves its lock file.
fn with_resolver(work: impl FnOnce(&mut resolve::Resolver) -> Result<(), String>) -> ExitCode {
    let root = manifest::find_root(Path::new(".")).unwrap_or_else(|| PathBuf::from("."));
    let lock_path = root.join(manifest::LOCKFILE);
    let lock = match std::fs::read_to_string(&lock_path) {
        Ok(t) => match lock::Lockfile::parse(&t) {
            Ok(l) => l,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        Err(_) => lock::Lockfile::default(),
    };
    let fetcher = project::default_fetcher();
    let mut resolver =
        resolve::Resolver::new(fetcher.as_ref(), resolve::vyrn_home().join("modules"), lock);
    resolver.offline = project::offline_requested();
    let result = work(&mut resolver);
    if resolver.lock_changed {
        if let Err(e) = std::fs::write(&lock_path, resolver.lock.render()) {
            eprintln!("error: cannot write {}: {e}", lock_path.display());
            return ExitCode::FAILURE;
        }
    }
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

fn add(args: &[String]) -> ExitCode {
    let Some(url) = args.first() else {
        eprintln!(
            "usage: vyrn add <host/owner/repo@version>, e.g. vyrn add github.com/ann/neural@v1"
        );
        return ExitCode::from(2);
    };
    let url = url.trim_matches('"').to_string();
    with_resolver(|resolver| {
        // Resolve afresh, so `vyrn add` also moves a module to its newest match.
        resolver.lock.modules.remove(&url);
        resolver.resolve(&url)?;
        resolver.lock_changed = true;
        let locked = &resolver.lock.modules[&url];
        println!(
            "added {url}: {} (commit {}, {})\n\nuse it with:\n\n    use \"{url}\"",
            locked.version,
            &locked.commit[..12.min(locked.commit.len())],
            locked.hash
        );
        Ok(())
    })
}

fn fetch() -> ExitCode {
    with_resolver(|resolver| {
        let urls: Vec<String> = resolver.lock.modules.keys().cloned().collect();
        for url in &urls {
            resolver.resolve(url)?;
        }
        println!("{} module(s) ready", urls.len());
        Ok(())
    })
}

fn build(args: &[String]) -> ExitCode {
    let mut out: Option<PathBuf> = None;
    let mut file = None;
    let mut target = String::from("native");
    let mut it = args.iter();
    while let Some(a) = it.next() {
        if a == "-o" {
            out = it.next().map(PathBuf::from);
        } else if a == "--target" {
            target = it.next().cloned().unwrap_or_default();
        } else if let Some(t) = a.strip_prefix("--target=") {
            target = t.to_string();
        } else {
            file = Some(a);
        }
    }
    let target = match Target::parse(&target) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(2);
        }
    };
    let entry = match entry_file(file) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };
    let entry = entry.canonicalize().unwrap_or(entry);
    let root = manifest::find_root(&entry)
        .unwrap_or_else(|| entry.parent().map(PathBuf::from).unwrap_or_default());
    let name = read_manifest(&root).map(|m| m.name).unwrap_or_else(|_| {
        entry
            .file_stem()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "program".into())
    });

    // Load through a recorder, so the bundle holds exactly what was read.
    let lock = std::fs::read_to_string(root.join(manifest::LOCKFILE))
        .ok()
        .and_then(|t| lock::Lockfile::parse(&t).ok())
        .unwrap_or_default();
    let fetcher = project::default_fetcher();
    let mut resolver =
        resolve::Resolver::new(fetcher.as_ref(), resolve::vyrn_home().join("modules"), lock);
    let mut disk = Disk {
        resolver: Some(&mut resolver),
    };
    let mut recorder = Recorder {
        disk: &mut disk,
        bundle: Bundle {
            entry: entry.to_string_lossy().to_string(),
            root: root.to_string_lossy().to_string(),
            ..Bundle::default()
        },
    };
    let loaded = match loader::load(&entry, &root, &mut recorder) {
        Ok(l) => l,
        Err((diagnostics, files)) => {
            report(&diagnostics, &files);
            return ExitCode::FAILURE;
        }
    };
    if let Err(mut diagnostics) = vm::compile_checked(&loaded.module) {
        loader::attach_files(&mut diagnostics, &loaded.files);
        report(&diagnostics, &loaded.files);
        return ExitCode::FAILURE;
    }
    let bundle = recorder.bundle;
    let base = match &target {
        Target::WasmEdge => {
            let wasm = match deploy::wasm_runtime() {
                Ok(w) => w,
                Err(e) => {
                    eprintln!("error: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let out = out.unwrap_or_else(|| root.join("build").join("edge"));
            return match deploy::write_edge(&bundle, &wasm, &out) {
                Ok(files) => {
                    println!("built {} for edge platforms:", shown(&out));
                    for f in files {
                        println!("    {}", shown(&f));
                    }
                    println!(
                        "\ndeploy to Vercel:  cd {} && vercel deploy\ndeploy elsewhere: worker.mjs is a standard fetch handler (Cloudflare Workers, Deno, Bun)",
                        shown(&out)
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("error: cannot write {}: {e}", out.display());
                    ExitCode::FAILURE
                }
            };
        }
        Target::Triple(triple) => match deploy::native_runtime(triple) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("error: {e}");
                return ExitCode::FAILURE;
            }
        },
        Target::Native => match std::env::current_exe() {
            Ok(b) => b,
            Err(e) => {
                eprintln!("error: cannot find the vyrn executable: {e}");
                return ExitCode::FAILURE;
            }
        },
    };
    let windows = match &target {
        Target::Triple(t) => t.contains("windows"),
        _ => cfg!(windows),
    };
    let exe_name = if windows { format!("{name}.exe") } else { name };
    let out = out.unwrap_or_else(|| root.join("build").join(exe_name));
    if let Err(e) = bundle.write_executable(&base, &out) {
        eprintln!("error: cannot write {}: {e}", out.display());
        return ExitCode::FAILURE;
    }
    let size = std::fs::metadata(&out).map(|m| m.len()).unwrap_or(0);
    println!(
        "built {} ({} files, {:.1} MB): copy it anywhere and run it, nothing to install",
        out.display(),
        bundle.files.len(),
        size as f64 / 1e6
    );
    ExitCode::SUCCESS
}

// ==== single-file views ======================================================

fn lex(file: &str, src: &str) -> ExitCode {
    let (tokens, errors) = Lexer::new(src).run();
    if !errors.is_empty() {
        for error in &errors {
            eprintln!("{}", error.render(file, src));
        }
        return ExitCode::FAILURE;
    }
    for token in &tokens {
        println!(
            "{:>4}:{:<3} {:?}",
            token.span.start.line, token.span.start.column, token.kind
        );
    }
    ExitCode::SUCCESS
}

fn parse(file: &str, src: &str) -> ExitCode {
    let (module, diagnostics) = parse_recovering(src);
    if !diagnostics.is_empty() {
        for diagnostic in &diagnostics {
            eprintln!("{}", diagnostic.render(file, src));
        }
        return ExitCode::FAILURE;
    }
    print!("{}", pretty::module(&module));
    ExitCode::SUCCESS
}

fn bytecode(file: &str) -> ExitCode {
    match compile_program(Path::new(file)) {
        Some((program, _)) => {
            print!("{}", program.disassemble());
            ExitCode::SUCCESS
        }
        None => ExitCode::FAILURE,
    }
}

// ==== the interactive prompt ===============================================

const REPL_HELP: &str = "\
Type Vyrn code and press Enter. A line ending in `:` opens a block; finish
it with `end`. Declarations (create, struct, object, enum, use, const) and
variables last until you leave.

  :vars    show the variables
  :reset   forget everything
  :help    show this
  :quit    leave (or Ctrl-D)";

/// The indentation for the next line of an open block.
fn continuation_indent(buffer: &str) -> String {
    let mut depth: i32 = 0;
    for line in buffer.lines() {
        let t = line.trim();
        let first = t.split(|c: char| !c.is_alphanumeric()).next().unwrap_or("");
        let continues = matches!(first, "else" | "catch" | "always" | "is");
        if t == "end" {
            depth -= 1;
        } else if t.ends_with(':') && !continues {
            depth += 1;
        } else if continues && !t.ends_with(':') {
            // A one-line `else: x` closes nothing and opens nothing.
        }
    }
    "    ".repeat(depth.max(0) as usize)
}

fn repl() -> ExitCode {
    use rustyline::error::ReadlineError;
    use vyrn::repl::{Outcome, Session};

    println!(
        "Vyrn {} - type code to run it, :help for help, :quit to leave",
        env!("CARGO_PKG_VERSION")
    );
    let mut editor = match rustyline::DefaultEditor::new() {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: cannot start the prompt: {e}");
            return ExitCode::FAILURE;
        }
    };
    let history = project::resolve::vyrn_home().join("history");
    let _ = editor.load_history(&history);
    let mut session = Session::default();
    let mut buffer = String::new();
    loop {
        let (prompt, indent) = if buffer.is_empty() {
            (">>> ", String::new())
        } else {
            ("... ", continuation_indent(&buffer))
        };
        let line = match editor.readline_with_initial(prompt, (&indent, "")) {
            Ok(line) => line,
            Err(ReadlineError::Interrupted) => {
                if !buffer.is_empty() {
                    buffer.clear();
                    println!("(entry cleared)");
                } else {
                    println!("(type :quit or press Ctrl-D to leave)");
                }
                continue;
            }
            Err(ReadlineError::Eof) => break,
            Err(e) => {
                eprintln!("error: {e}");
                break;
            }
        };
        if buffer.is_empty() {
            match line.trim() {
                "" => continue,
                ":quit" | ":q" | "quit" | "exit" | ":exit" => break,
                ":help" | ":h" | "help" => {
                    println!("{REPL_HELP}");
                    continue;
                }
                ":reset" => {
                    session = Session::default();
                    println!("(everything forgotten)");
                    continue;
                }
                ":vars" => {
                    let vars = session.variables();
                    if vars.is_empty() {
                        println!("(no variables yet)");
                    }
                    for (name, value) in vars {
                        println!("{name} = {value}");
                    }
                    continue;
                }
                _ => {}
            }
        }
        // An empty line inside a block is part of it; two in a row give up
        // on finishing the block and run what is there.
        let give_up = !buffer.is_empty() && line.trim().is_empty() && buffer.ends_with("\n\n");
        if !buffer.is_empty() {
            buffer.push('\n');
        }
        buffer.push_str(&line);
        let outcome = session.eval(&buffer);
        match outcome {
            Outcome::NeedsMore if !give_up => continue,
            Outcome::NeedsMore => {
                eprintln!("error: the block is not finished; close it with `end`")
            }
            Outcome::Done(Some(value)) => println!("{value}"),
            Outcome::Done(None) => {}
            Outcome::Failed(e) => eprintln!("{e}"),
        }
        let _ = editor.add_history_entry(buffer.trim_end());
        buffer.clear();
    }
    let _ = std::fs::create_dir_all(project::resolve::vyrn_home());
    let _ = editor.save_history(&history);
    ExitCode::SUCCESS
}
