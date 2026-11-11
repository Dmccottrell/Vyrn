//! The everyday standard library: Prelude functions, methods of strings,
//! lists and maps, format specs, sets, files, and the `std.*` modules.

use vyrn::vm::{run_source, RunError};

fn run(src: &str) -> String {
    match run_source(src) {
        Ok(out) => out,
        Err(RunError::Compile(errors)) => {
            let report: String = errors.iter().map(|e| e.render("test.vyrn", src)).collect();
            panic!("compile errors:\n{report}");
        }
        Err(RunError::Runtime(e)) => panic!("runtime error: {e}"),
    }
}

/// Runs `body` inside `main:`, after `uses`.
fn main(uses: &str, body: &str) -> String {
    let indented: String = body.lines().map(|l| format!("    {l}\n")).collect();
    run(&format!("{uses}\nmain:\n{indented}end\n"))
}

fn error(src: &str) -> String {
    match run_source(src) {
        Err(RunError::Runtime(e)) => format!("{}: {}", e.kind, e.message),
        other => panic!("expected a runtime error, got {:?}", other.map(|_| ())),
    }
}

#[test]
fn prelude_functions() {
    assert_eq!(
        main("", "print(bool(0), typeof(1.5), repr(\"hi\"), chr(65), ord(\"A\"), hex(255), oct(8), bin(5))\nprint(divmod(7, 2), pow(2, 10), pow(2, 10, 7), floor(2.7), ceil(2.1), floor(-2.5))\nprint(any([false, true]), all([true, false]), reversed([1, 2, 3]), reversed(\"abc\"), list(\"ab\"), dict([(\"a\", 1)]))"),
        "false float \"hi\" A 65 0xff 0o10 0b101\n(3, 1) 1024 2 2 3 -3\ntrue false [3, 2, 1] cba [\"a\", \"b\"] {\"a\": 1}\n"
    );
}

#[test]
fn callbacks_from_the_vyrn_prelude() {
    let out = main(
        "struct P:\n    name: str\n    age: int\nend",
        "people = [P(\"Ada\", 36), P(\"Alan\", 41), P(\"Grace\", 85), P(\"Bo\", 36)]\n\
         print(sorted(people, key = (p) => p.age, reverse = true).map(p => p.name))\n\
         print(max(people, key = p => p.age).name, min(people, key = p => p.age).name, max(4, 9, 2))\n\
         xs = [5, 3, 8, 1]\n\
         xs.sort(reverse = true)\n\
         words = [\"pear\", \"fig\", \"banana\"]\n\
         words.sort(key = len)\n\
         print(xs, words, map((w) => w.upper(), words), filter(w => len(w) > 3, words))\n\
         print(xs.filter(x => x > 2).map(x => x * 10).reduce((a, b) => a + b), xs.find(x => x < 4), xs.find_index(x => x == 8))\n\
         print(xs.any(x => x > 7), xs.all(x => x > 0), xs.count_if(x => x % 2 == 1), xs.sum_by(x => x * 2))\n\
         print(people.group_by(p => p.age).keys(), [[1, 2], [3]].flatten(), xs.min_by(x => -x))\n\
         m = {\"a\": 1, \"b\": 2}\n\
         print(m.map_values(v => v * 10), m.filter((k, v) => v > 1))",
    );
    assert_eq!(
        out,
        "[\"Grace\", \"Alan\", \"Ada\", \"Bo\"]\nGrace Ada 9\n[8, 5, 3, 1] [\"fig\", \"pear\", \"banana\"] [\"FIG\", \"PEAR\", \"BANANA\"] [\"pear\", \"banana\"]\n160 3 0\ntrue true 3 34\n[36, 41, 85] [1, 2, 3] 8\n{\"a\": 10, \"b\": 20} {\"b\": 2}\n"
    );
}

#[test]
fn a_program_can_define_its_own_map_and_sorted() {
    assert_eq!(
        run("create map(x):\n    return \"mine\"\nend\ncreate sorted(xs):\n    return \"also mine\"\nend\nmain: print(map(1), sorted([2, 1]), [2, 1].map(x => x + 1))\n"),
        "mine also mine [3, 2]\n"
    );
}

#[test]
fn string_methods() {
    assert_eq!(
        main("", "print(\"hello world\".title(), \"hELLO\".capitalize(), \"a,b,c\".split(\",\", 1), \"a b\".split(), \"abcb\".count(\"b\"))\nprint(\"12\".isdigit(), \"ab\".isalpha(), \" \".isspace(), \"x--\".rstrip(\"-\"), \"7\".zfill(3), \"ab\".center(6, \"*\"), \"ab\".ljust(4) + \"|\")\nprint(\"key=val=x\".partition(\"=\"), \"a.b.c\".rsplit(\".\", 1), \"pre_x\".removeprefix(\"pre_\"), \"a\\nb\".lines(), \"abc\".rfind(\"c\"), \"ab\".repeat(2))"),
        "Hello World Hello [\"a\", \"b,c\"] [\"a\", \"b\"] 2\ntrue true true x 007 **ab** ab  |\n(\"key\", \"=\", \"val=x\") [\"a.b\", \"c\"] x [\"a\", \"b\"] 2 abab\n"
    );
}

#[test]
fn list_and_map_methods() {
    assert_eq!(
        main("", "xs = [3, 1, 3, 2]\nxs.remove(3)\nprint(xs, xs.count(3), xs.first(), xs.last(), [1, 1, 2].unique(), [1, 2].join(\", \"), [1, 2, 3, 4, 5].chunks(2))\nm = {\"a\": 1}\nm.update({\"b\": 2})\nprint(m.pop(\"a\"), m.pop(\"zz\", 0), m.setdefault(\"c\", 3), m, m.has(\"b\"), m.copy() == m)"),
        "[1, 3, 2] 1 1 2 [1, 2] 1, 2 [[1, 2], [3, 4], [5]]\n1 0 3 {\"b\": 2, \"c\": 3} true true\n"
    );
}

#[test]
fn format_specs_follow_python() {
    assert_eq!(
        main("", "price = 1234.5678\nname = \"Ada\"\nm = {\"a:b\": 3}\nprint(\"{price:.2f}|{price:,.1f}|{name:>6}|{name:<6}|{name:^7}|{42:05}|{0.5:.0%}|{m[\"a:b\"]}|{255:#x}|{7:+}|{1e6:.3e}|{123456789:_}|{-3.14159:08.2f}\")\nprint(format(2.5), format(0.1 + 0.2, \".3g\"))"),
        "1234.57|1,234.6|   Ada|Ada   |  Ada  |00042|50%|3|0xff|+7|1.000e+06|123_456_789|-0003.14\n2.5 0.3\n"
    );
    assert!(error("main: print(\"{1:q}\")\n").contains("not a format spec"));
}

#[test]
fn sets() {
    assert_eq!(
        main("", "a = {1, 2, 3}\nb = {x % 3 each x in 0 until 10}\nc = set([1, 1, 2])\nc.add(5)\nc.discard(1)\nprint(a, b, a | b, a & {2, 9}, a - {1}, a ^ {3, 4}, 2 in a, len(c), c, set(), a == {3, 2, 1})\nprint(sorted({\"b\", \"a\"}), {1, 2}.issubset([1, 2, 3]), to_json({1}))"),
        "{1, 2, 3} {0, 1, 2} {1, 2, 3, 0} {2} {2, 3} {1, 2, 4} true 2 {2, 5} set() true\n[\"a\", \"b\"] true [1]\n"
    );
}

#[test]
fn files() {
    let dir = std::env::temp_dir().join(format!("vyrn-files-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("t.txt");
    let p = path.display().to_string().replace('\\', "\\\\");
    let out = main(
        "use std.fs",
        &format!(
            "using open(\"{p}\", \"w\") as f:\n    f.write_line(\"alpha\")\n    f.write(\"beta\\n\")\nend\nusing open(\"{p}\", \"a\") as f: f.write_line(\"gamma\")\nusing open(\"{p}\") as f:\n    each line in f: print(\"line:\", line)\nend\nprint(fs.read_lines(\"{p}\"), fs.exists(\"{p}\"), fs.size(\"{p}\"))\nf = open(\"{p}\")\nprint(f.readline().strip(), f.read().split())\nf.close()\nfs.remove(\"{p}\")\nprint(fs.exists(\"{p}\"))"
        ),
    );
    assert_eq!(
        out,
        "line: alpha\nline: beta\nline: gamma\n[\"alpha\", \"beta\", \"gamma\"] true 17\nalpha [\"beta\", \"gamma\"]\nfalse\n"
    );
    let _ = std::fs::remove_dir_all(&dir);
    assert!(error("main: open(\"/surely/not/here.txt\")\n").starts_with("NotFound: cannot open"));
}

#[test]
fn modules() {
    let out = main(
        "use std.path\nuse std.time\nuse std.random\nuse std.math\nuse std.json\nuse std.re\nuse std.csv\nuse std.os",
        "print(path.join(\"a\", \"b\", \"c.txt\").replace(\"\\\\\", \"/\"), path.name(\"x/y.txt\"), path.stem(\"x/y.txt\"), path.ext(\"x/y.txt\"))\n\
         print(time.format(0.0), time.format(1794355200.0, \"%a %d %b %Y %j\"), time.parse(\"2026-11-11\"), time.date(86400.5).get(\"weekday\"))\n\
         random.seed(7)\n\
         a = [random.int(1, 6) each _ in 0 until 20]\n\
         random.seed(7)\n\
         print(a == [random.int(1, 6) each _ in 0 until 20], a.all(x => x >= 1 and x <= 6), len(random.sample([1, 2, 3, 4], 2)))\n\
         print(math.pi, math.floor(2.5), math.log(math.e), math.gcd(12, 18), math.factorial(5), math.inf > 10, math.clamp(15, 0, 10))\n\
         print(json.dumps({\"a\": [1, 2]}), json.dumps({\"a\": 1}, 2), json.parse(\"[1, 2.5]\"))\n\
         print(re.test(\"\\\\d+\", \"ab12\"), re.find(\"(\\\\w+)@(\\\\w+)\", \"mail me@site now\").get(\"groups\"), re.find_all(\"\\\\d\", \"a1b2\"), re.replace(\"o\", \"foo\", \"0\"), re.split(\",\\\\s*\", \"a, b,c\"))\n\
         rows = csv.parse(\"name,age\\nAda,36\\n\\\"Lovelace, A\\\",37\\n\")\n\
         print(rows, csv.format(rows).lines())\n\
         print(os.env(\"VYRN_SURELY_UNSET\", \"default\"), typeof(os.platform()), os.run([\"echo\", \"hi\"]).get(\"out\").strip())",
    );
    assert_eq!(
        out,
        "a/b/c.txt y.txt y .txt\n\
         1970-01-01 00:00:00 Wed 11 Nov 2026 315 1794355200.0 Friday\n\
         true true 2\n\
         3.141592653589793 2 1.0 6 120 true 10\n\
         {\"a\":[1,2]} {\n  \"a\": 1\n} [1, 2.5]\n\
         true [\"me\", \"site\"] [\"1\", \"2\"] f00 [\"a\", \"b\", \"c\"]\n\
         [{\"name\": \"Ada\", \"age\": \"36\"}, {\"name\": \"Lovelace, A\", \"age\": \"37\"}] [\"name,age\", \"Ada,36\", \"\\\"Lovelace, A\\\",37\"]\n\
         default str hi\n"
    );
}

#[test]
fn errors_name_what_went_wrong() {
    assert_eq!(
        error("main: print([1][5])\n"),
        "IndexError: index 5 is out of range for a list of length 1"
    );
    assert!(error("use std.random\nmain: random.int(5, 1)\n").contains("below the low end"));
    assert!(error("main: print([].reduce((a, b) => a + b))\n").contains("empty list"));
}
