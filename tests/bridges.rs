//! The Python and C bridges. Each test needs its feature:
//! `cargo test --features python,ffi --test bridges`.

#[allow(unused_imports)]
use vyrn::vm::{run_source, RunError};

#[allow(dead_code)]
fn run(src: &str) -> String {
    match run_source(src) {
        Ok(out) => out,
        Err(RunError::Compile(e)) => panic!("compile errors: {e:?}"),
        Err(RunError::Runtime(e)) => panic!("runtime error: {e}"),
    }
}

#[cfg(feature = "python")]
#[test]
fn python_modules_values_and_errors() {
    let out = run("use python.math\n\
         use python.json (dumps, loads)\n\
         main:\n\
         \x20   print(math.sqrt(16.0), math.factorial(20))\n\
         \x20   print(dumps({\"b\": 1, \"a\": [1, 2.5, none, true]}, sort_keys = true))\n\
         \x20   let data = loads(\"[3, 1, 2]\")\n\
         \x20   print(len(data), data[0], [x * 10 for x in data])\n\
         \x20   try:\n\
         \x20       math.sqrt(-1.0)\n\
         \x20   catch PythonError as e:\n\
         \x20       print(e.message)\n\
         \x20   end\n\
         end\n");
    assert_eq!(
        out,
        "4.0 2432902008176640000\n{\"a\": [1, 2.5, null, true], \"b\": 1}\n3 3 [30, 10, 20]\nValueError: math domain error\n"
    );
}

#[cfg(feature = "python")]
#[test]
fn missing_python_modules_say_how_to_install_them() {
    match run_source("use python.surely_not_a_module_xyz\nmain: print(1)\n") {
        Err(RunError::Runtime(e)) => {
            assert_eq!(e.kind, "PythonError");
            assert!(
                e.message.contains("pip install surely_not_a_module_xyz"),
                "{}",
                e.message
            );
        }
        other => panic!("{other:?}"),
    }
}

#[cfg(not(feature = "python"))]
#[test]
fn python_needs_its_feature() {
    match run_source("use python.math\nmain: print(math.pi)\n") {
        Err(RunError::Runtime(e)) => {
            assert!(e.message.contains("--features python"), "{}", e.message)
        }
        other => panic!("{other:?}"),
    }
}

#[cfg(feature = "ffi")]
#[test]
fn c_libraries_take_numbers_strings_tensors_and_handles() {
    let dir = std::env::temp_dir().join(format!("vyrn-ffi-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let lib = dir.join("libaccel.so");
    let src = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/ffi/accel.c");
    let status = std::process::Command::new("cc")
        .args(["-shared", "-fPIC", "-O2", "-o"])
        .arg(&lib)
        .arg(src)
        .status();
    if !status.is_ok_and(|s| s.success()) {
        eprintln!("skipping: no C compiler");
        return;
    }
    let out = run(&format!(
        "@link(\"m\")\nextern \"c\":\n    fn pow(x: f64, y: f64) -> f64\nend\n\
         extern \"c\":\n    fn strlen(s: str) -> u64\n    fn abs(n: i32) -> i32\nend\n\
         @link(\"{}\")\nextern \"c\":\n    fn accel_sum(x: tensor[f32], n: i32) -> f32\n    fn accel_new() -> ptr u8\n    fn accel_add(h: ptr u8, x: tensor[f32], n: i32)\n    fn accel_total(h: ptr u8) -> f64\n    fn accel_free(h: ptr u8)\nend\n\
         main:\n    print(pow(2.0, 10.0), strlen(\"héllo\"), abs(-7))\n    let x = arange(5)\n    unsafe:\n        print(accel_sum(x, 5))\n        let h = accel_new()\n        accel_add(h, x, 5)\n        accel_add(h, x, 3)\n        print(accel_total(h))\n        accel_free(h)\n    end\n    try:\n        abs(9999999999)\n    catch OverflowError as e:\n        print(\"refused\")\n    end\nend\n",
        lib.display()
    ));
    assert_eq!(out, "1024.0 6 7\n10.0\n13.0\nrefused\n");
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(not(feature = "ffi"))]
#[test]
fn c_needs_its_feature() {
    match run_source(
        "@link(\"m\")\nextern \"c\":\n    fn cos(x: f64) -> f64\nend\nmain: print(cos(0.0))\n",
    ) {
        Err(RunError::Runtime(e)) => assert!(e.message.contains("--features ffi"), "{}", e.message),
        other => panic!("{other:?}"),
    }
}
