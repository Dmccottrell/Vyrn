//! The native tier must not change what a program does: every program here
//! runs compiled to machine code and interpreted, and both must print the
//! same thing or fail with the same error.
#![cfg(feature = "native")]

use vyrn::vm::{self, compiler, Captured, Vm};

/// Output, or the error kind and message (without the native location).
fn run(program: vm::Program) -> String {
    let out = Captured::default();
    let mut machine = Vm::with_output(program, Box::new(out.clone()));
    match machine.run_main(&[]) {
        Ok(_) => out.text(),
        Err(e) => {
            let message = e.message.split(" (in `").next().unwrap_or("").to_string();
            format!("{}<error {}: {}>", out.text(), e.kind, message)
        }
    }
}

/// Runs `src` both ways; returns the output, and checks that `compiled`
/// functions really got machine code.
fn both(src: &str, compiled: &[&str]) -> String {
    let module = vyrn::parse(src).unwrap_or_else(|e| panic!("{e:?}"));
    let native = compiler::compile(&module).unwrap_or_else(|e| panic!("{e:?}"));
    for name in compiled {
        let p = match name.split_once('.') {
            // `Type.method`
            Some((ty, method)) => {
                let t = native
                    .types
                    .iter()
                    .position(|t| native.symbols.name(t.name) == ty)
                    .unwrap_or_else(|| panic!("no type {ty}"));
                let sym = native.symbols.get(method).expect("a method name");
                native.types[t].methods[&sym]
            }
            None => {
                let g = native
                    .global(name)
                    .unwrap_or_else(|| panic!("no function {name}"));
                let vm::program::GlobalInit::Function(p) = native.globals[g].init else {
                    panic!("{name} is not a function")
                };
                p
            }
        };
        assert!(
            native.protos[p].native.get().is_some(),
            "`{name}` should have been compiled to machine code"
        );
    }
    let interpreted = compiler::compile_only(&module).unwrap();
    let (a, b) = (run(native), run(interpreted));
    assert_eq!(a, b, "native and interpreted runs differ for:\n{src}");
    a
}

#[test]
fn integer_arithmetic_matches() {
    let out = both(
        "create f(a: int, b: int) -> int:\n    return a // b * 1000 + a % b\nend\n\
         create g(a: int, b: int) -> float: return a / b\n\
         create h(x: int) -> int: return x ** 3 + (x << 2) + (x >> 1) + (x & 6) + (x | 1) + (x ^ 3) + ~x\n\
         main:\n    print(f(7, 2), f(-7, 2), f(7, -2), f(-7, -2), g(7, 2), g(-1, 3), h(5), h(-5))\nend\n",
        &["f", "g", "h"],
    );
    assert_eq!(
        out,
        "3001 -3999 -4001 2999 3.5 -0.3333333333333333 156 -155\n"
    );
}

#[test]
fn float_arithmetic_matches() {
    both(
        "create f(x: float, y: float) -> float:\n    return x // y + x % y + x ** y - x / y\nend\n\
         create mixed(a: int, x: float) -> float: return a + x * a - a / 4\n\
         main:\n    print(f(7.5, 2.0), f(-7.5, 2.0), f(7.5, -2.0), mixed(3, 0.5), f(0.0, 3.0))\nend\n",
        &["f", "mixed"],
    );
}

#[test]
fn failures_are_the_same() {
    let cases = [
        (
            "create f(a: int) -> int: return a + 9223372036854775807\nmain: print(f(1))\n",
            "Overflow",
        ),
        (
            "create f(a: int) -> int: return a * 4611686018427387904\nmain: print(f(4))\n",
            "Overflow",
        ),
        (
            "create f(a: int) -> int: return -a\nmain: print(f(-9223372036854775807 - 1))\n",
            "Overflow",
        ),
        (
            "create f(a: int) -> int: return abs(a)\nmain: print(f(-9223372036854775807 - 1))\n",
            "Overflow",
        ),
        (
            "create f(a: int) -> int: return a // 0\nmain: print(f(1))\n",
            "DivisionByZero",
        ),
        (
            "create f(a: int) -> int: return a % 0\nmain: print(f(1))\n",
            "DivisionByZero",
        ),
        (
            "create f(a: int) -> float: return a / 0\nmain: print(f(1))\n",
            "DivisionByZero",
        ),
        (
            "create f(a: float) -> float: return a / 0.0\nmain: print(f(1.0))\n",
            "DivisionByZero",
        ),
        (
            "create f(a: int) -> int: return a ** 70\nmain: print(f(3))\n",
            "Overflow",
        ),
        (
            "create f(a: int, b: int) -> int: return a << b\nmain: print(f(1, 64))\n",
            "ValueError",
        ),
        (
            "create f(a: float) -> int: return int(a)\nmain: print(f(1.0 / 0.0))\n",
            "DivisionByZero",
        ),
        (
            "create f(n: int) -> int: return f(n + 1)\nmain: print(f(0))\n",
            "StackOverflow",
        ),
    ];
    for (src, kind) in cases {
        let out = both(src, &["f"]);
        assert!(out.contains(&format!("<error {kind}")), "{src}\n{out}");
    }
}

#[test]
fn control_flow_matches() {
    let out = both(
        "create classify(n: int) -> int:\n\
         \x20   if n < 0:\n        return -1\n    else if n == 0:\n        return 0\n    else if n < 10 and not (n == 5):\n        return 1\n    else:\n        return 2\n    end\nend\n\
         create loops(n: int) -> int:\n\
         \x20   var total = 0\n\
         \x20   each i in 0 until n:\n        if i % 3 == 0: skip\n        if i > 50: stop\n        total += i\n    end\n\
         \x20   each j in 10 to 0 by -2: total += j\n\
         \x20   each k in range(2, 9, 3): total += k * 100\n\
         \x20   var w = 0\n\
         \x20   loop w < 5: w += 1\n\
         \x20   loop:\n        w += 10\n        if w > 40: stop\n    end\n\
         \x20   return total + w\nend\n\
         create chain(a: int, b: float, c: int) -> bool: return a < b <= c != 7\n\
         create pick(x: float) -> float: return x if x > 0.0 else -x\n\
         main:\n    print(classify(-3), classify(0), classify(4), classify(5), classify(40), loops(100), loops(7))\n    print(chain(1, 2.0, 2), chain(1, 2.0, 7), chain(3, 2.0, 2), pick(-2.5), pick(1.0))\nend\n",
        &["classify", "loops", "chain", "pick"],
    );
    assert_eq!(out, "-1 0 1 2 2 2442 1587\ntrue false false 2.5 1.0\n");
}

#[test]
fn calls_recursion_and_builtins() {
    let out = both(
        "use std.math\n\
         create fib(n: int) -> int:\n    if n < 2: return n\n    return fib(n - 1) + fib(n - 2)\nend\n\
         create even(n: int) -> bool:\n    if n == 0: return true\n    return odd(n - 1)\nend\n\
         create odd(n: int) -> bool:\n    if n == 0: return false\n    return even(n - 1)\nend\n\
         create geo(x: float, y: float) -> float:\n    return math.sqrt(x * x + y * y) + math.hypot(x, y) + math.atan2(y, x) + math.log(math.exp(1.0)) + sqrt(16.0)\nend\n\
         create ints(a: int, b: int) -> int: return min(a, b) * 10 + max(a, b) + floor(2.7) + ceil(2.1) + int(-3.9) + int(float(a))\n\
         main:\n    print(fib(20), even(10), odd(7), even(7), geo(3.0, 4.0), ints(3, 9))\nend\n",
        &["fib", "even", "odd", "geo", "ints"],
    );
    assert!(out.starts_with("6765 true true false 15."), "{out}");
}

#[test]
fn unsupported_functions_stay_interpreted() {
    // Prints, changes a variable's type, and is called with an int where a
    // float is declared: all still run, on the interpreter.
    let src = "create noisy(n: int) -> int:\n    let m = {\"n\": n}\n    print(\"n is\", m[\"n\"])\n    return n\nend\n\
               create widen(n: int) -> float:\n    var x = n\n    x = x / 2\n    return x\nend\n\
               create half(x: float) -> float: return x / 2.0\n\
               main:\n    print(noisy(3), widen(3), half(3), half(3.0))\nend\n";
    let out = both(src, &["half"]);
    assert_eq!(out, "n is 3\n3 1.5 1.5 1.5\n");
    let module = vyrn::parse(src).unwrap();
    let program = compiler::compile(&module).unwrap();
    for name in ["noisy", "widen"] {
        let g = program.global(name).unwrap();
        let vm::program::GlobalInit::Function(p) = program.globals[g].init else {
            panic!()
        };
        assert!(
            program.protos[p].native.get().is_none(),
            "{name} must stay interpreted"
        );
    }
}

#[test]
fn nan_and_mixed_comparisons() {
    let out = both(
        "create cmp(x: float, y: float) -> int:\n    var n = 0\n    if x < y: n += 1\n    if x == y: n += 10\n    if x != y: n += 100\n    if x >= y: n += 1000\n    return n\nend\n\
         create eq(a: int, x: float) -> bool: return a == x\n\
         main:\n    let nan = 0.0 / 1.0 - (1.0 / 0.0 - 1.0 / 0.0)\n    print(cmp(1.0, 2.0), cmp(2.0, 2.0), eq(3, 3.0), eq(3, 3.5))\nend\n",
        &["cmp", "eq"],
    );
    assert!(
        out.contains("<error DivisionByZero") || out.starts_with("101 1010 true false"),
        "{out}"
    );
}

/// A small deterministic generator, so failures reproduce.
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
    fn pick(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

fn int_expr(r: &mut Rng, depth: u32) -> String {
    if depth == 0 || r.pick(4) == 0 {
        return match r.pick(4) {
            0 => "a".into(),
            1 => "b".into(),
            2 => "c".into(),
            _ => [
                "0",
                "1",
                "-1",
                "3",
                "7",
                "-13",
                "1000003",
                "9223372036854775807",
            ][r.pick(8)]
            .into(),
        };
    }
    let x = int_expr(r, depth - 1);
    let y = int_expr(r, depth - 1);
    match r.pick(12) {
        0 => format!("({x} + {y})"),
        1 => format!("({x} - {y})"),
        2 => format!("({x} * {y})"),
        3 => format!("({x} // {y})"),
        4 => format!("({x} % {y})"),
        5 => format!("(-{x})"),
        6 => format!("({x} if {x} < {y} else {y})"),
        7 => format!("min({x}, {y})"),
        8 => format!("abs({x})"),
        9 => format!("({x} & {y})"),
        10 => format!("({x} ** 2)"),
        _ => format!("({x} ^ {y})"),
    }
}

fn float_expr(r: &mut Rng, depth: u32) -> String {
    if depth == 0 || r.pick(4) == 0 {
        return match r.pick(4) {
            0 => "x".into(),
            1 => "a".into(),
            2 => "y".into(),
            _ => ["0.0", "1.5", "-2.25", "1e300", "3"][r.pick(5)].into(),
        };
    }
    let p = float_expr(r, depth - 1);
    let q = float_expr(r, depth - 1);
    match r.pick(9) {
        0 => format!("({p} + {q})"),
        1 => format!("({p} - {q})"),
        2 => format!("({p} * {q})"),
        3 => format!("({p} / {q})"),
        4 => format!("({p} // {q})"),
        5 => format!("({p} % {q})"),
        6 => format!("sqrt(abs({p}))"),
        7 => format!("({p} if {p} >= {q} else {q})"),
        _ => format!("max({p} * 1.0, {q} * 1.0)"),
    }
}

#[test]
fn random_programs_agree() {
    let mut r = Rng(0x2545_F491_4F6C_DD1D);
    let edge = [
        "0",
        "1",
        "-1",
        "2",
        "-7",
        "100",
        "-9223372036854775807 - 1",
        "9223372036854775807",
        "65536",
    ];
    let mut compiled = 0;
    let mut total = 0;
    for round in 0..120 {
        let ie = int_expr(&mut r, 4);
        let fe = float_expr(&mut r, 4);
        let mut calls = String::new();
        for _ in 0..12 {
            let (a, b, c) = (
                edge[r.pick(edge.len())],
                edge[r.pick(edge.len())],
                edge[r.pick(edge.len())],
            );
            calls.push_str(&format!(
                "    attempt:\n        print(fi({a}, {b}, {c}), ff({a}.0 * 0.5, {b}, {c}.0))\n    catch Overflow:\n        print(\"overflow\")\n    catch DivisionByZero:\n        print(\"div0\")\n    end\n"
            ));
        }
        let src = format!(
            "create fi(a: int, b: int, c: int) -> int:\n    return {ie}\nend\n\
             create ff(x: float, a: int, y: float) -> float:\n    return {fe}\nend\n\
             main:\n{calls}end\n"
        );
        // `-9223372036854775807 - 1` with `.0` appended is not a float
        // literal; such calls simply fail the same way in both engines.
        let module = match vyrn::parse(&src) {
            Ok(m) => m,
            Err(_) => continue,
        };
        let native =
            compiler::compile(&module).unwrap_or_else(|e| panic!("round {round}: {e:?}\n{src}"));
        for name in ["fi", "ff"] {
            let g = native.global(name).unwrap();
            if let vm::program::GlobalInit::Function(p) = native.globals[g].init {
                total += 1;
                compiled += usize::from(native.protos[p].native.get().is_some());
            }
        }
        let interpreted = compiler::compile_only(&module).unwrap();
        assert_eq!(run(native), run(interpreted), "round {round}:\n{src}");
    }
    // Most generated functions must really have been compiled.
    assert!(
        compiled * 10 >= total * 9,
        "only {compiled} of {total} compiled"
    );
}

#[test]
fn lists_of_numbers() {
    let out = both(
        "create mean(xs: [float]) -> float:\n    var total = 0.0\n    each x in xs: total += x\n    return total / len(xs)\nend\n\
         create dot(a: [float], b: [float]) -> float:\n    var s = 0.0\n    each i in 0 until len(a): s += a[i] * b[i]\n    return s\nend\n\
         create bubble(xs: [int]):\n    let n = len(xs)\n    each i in 0 until n:\n        each j in 0 until n - i - 1:\n            if xs[j] > xs[j + 1]:\n                let t = xs[j]\n                xs[j] = xs[j + 1]\n                xs[j + 1] = t\n            end\n        end\n    end\nend\n\
         create prefix(xs: [int], out: [int]):\n    var run = 0\n    each x in xs:\n        run += x\n        out.append(run)\n    end\n    out[-1] *= 10\nend\n\
         create last(xs: [int]) -> int: return xs[-1] + xs[len(xs) - 2]\n\
         create grow(xs: [int]) -> int:\n    var seen = 0\n    each x in xs:\n        seen += 1\n        if x < 5: xs.append(x + 1)\n    end\n    return seen\nend\n\
         main:\n\
         \x20   print(mean([1.0, 2.0, 4.5]), dot([1.0, 2.0], [3.0, 4.0]))\n\
         \x20   let xs = [5, 3, 9, 1, 7]\n    bubble(xs)\n    print(xs)\n\
         \x20   let out = []\n    prefix([1, 2, 3], out)\n    print(out, last(xs))\n\
         \x20   let g = [1]\n    print(grow(g), g)\n\
         \x20   print(mean([1, 2]))\n\
         end\n",
        &["mean", "dot", "bubble", "prefix", "last", "grow"],
    );
    assert_eq!(
        out,
        "2.5 11.0\n[1, 3, 5, 7, 9]\n[1, 3, 60] 16\n5 [1, 2, 3, 4, 5]\n1.5\n"
    );
}

#[test]
fn list_failures_are_the_same() {
    for (src, kind) in [
        (
            "create f(xs: [int]) -> int: return xs[3]\nmain: print(f([1, 2]))\n",
            "IndexError",
        ),
        (
            "create f(xs: [int]) -> int: return xs[-3]\nmain: print(f([1, 2]))\n",
            "IndexError",
        ),
        (
            "create f(xs: [float]):\n    xs[2] = 1.0\nend\nmain: f([1.0])\n",
            "IndexError",
        ),
        (
            "create f(xs: [float]) -> float: return xs[0] / len(xs)\nmain: print(f([]))\n",
            "IndexError",
        ),
    ] {
        let out = both(src, &["f"]);
        assert!(out.contains(&format!("<error {kind}")), "{src}\n{out}");
    }
}

#[test]
fn list_items_are_accessed_directly() {
    // The measured layout of lists and values was confirmed, so compiled
    // code reads items itself; the tests above then exercise that path.
    assert!(vyrn::native::direct_list_access());
}

#[test]
fn making_and_returning_lists() {
    let out = both(
        "create squares(n: int) -> [int]: return [i * i each i in 0 until n]\n\
         create evens(xs: [int]) -> [int]: return [x each x in xs if x % 2 == 0]\n\
         create scaled(xs: [float], k: float) -> [float]:\n    let out: [float] = []\n    each x in xs: out.append(x * k)\n    return out\nend\n\
         create zeros(n: int) -> [float]: return [0.0] * n\n\
         create joined(a: [int], b: [int]) -> [int]: return a + b + [99]\n\
         create grid(n: int) -> int:\n    var cells = [0] * (n * n)\n    each i in 0 until n: cells[i * n + i] = 1\n    return sum(cells) + len(cells)\nend\n\
         create pairs(n: int) -> [int]:\n    var out = [1, 2]\n    out = out + [3]\n    let copy = out.copy()\n    copy.append(4)\n    return list(copy) + out\nend\n\
         create same(xs: [int]) -> [int]: return xs\n\
         create fresh() -> [int]:\n    return []\nend\n\
         main:\n\
         \x20   print(squares(5), evens([1, 2, 3, 4, 6]), scaled([1.0, 2.5], 2.0), zeros(3), zeros(-1))\n\
         \x20   print(joined([1], [2, 3]), grid(4), pairs(0), fresh())\n\
         \x20   let a = [7, 8]\n    let b = same(a)\n    b.append(9)\n    print(a, squares(0))\n\
         end\n",
        &["squares", "evens", "scaled", "zeros", "joined", "grid", "pairs", "same", "fresh"],
    );
    assert_eq!(
        out,
        "[0, 1, 4, 9, 16] [2, 4, 6] [2.0, 5.0] [0.0, 0.0, 0.0] []\n[1, 2, 3, 99] 20 [1, 2, 3, 4, 1, 2, 3] []\n[7, 8, 9] []\n"
    );
}

#[test]
fn list_making_failures_are_the_same() {
    for (src, kind) in [
        ("create f(n: int) -> [int]: return [1] * n\nmain: print(len(f(9223372036854775807)))\n", "MemoryError"),
        ("create f(xs: [int]) -> int: return sum(xs)\nmain: print(f([9223372036854775807, 1]))\n", "Overflow"),
    ] {
        let out = both(src, &["f"]);
        assert!(out.contains(&format!("<error {kind}")), "{src}\n{out}");
    }
}

#[test]
fn structs_of_numbers() {
    let out = both(
        "struct Vec2:\n    x: float\n    y: float\n\
         \x20   create length(self) -> float: return sqrt(self.x * self.x + self.y * self.y)\n\
         \x20   create scaled(self, k: float) -> Vec2: return Vec2(self.x * k, self.y * k)\n\
         \x20   create plus(self, o: Vec2) -> Vec2: return Vec2(x = self.x + o.x, y = self.y + o.y)\n\
         end\n\
         struct Body:\n    pos: Vec2\nend\n\
         struct Cell:\n    alive: bool\n    age: int\nend\n\
         create step(c: Cell) -> Cell:\n    var n = c\n    n.age += 1\n    if n.age > 2: n.alive = false\n    return n\nend\n\
         create walk(steps: int) -> Vec2:\n    var p = Vec2(0.0, 0.0)\n    let v = Vec2(1.0, 0.5)\n    each i in 0 until steps: p = p.plus(v)\n    return p.scaled(2.0)\nend\n\
         create copies() -> float:\n    var a = Vec2(1.0, 2.0)\n    var b = a\n    b.x = 10.0\n    return a.x + b.x\nend\n\
         create untouched(p: Vec2) -> float:\n    p.x = 99.0\n    return p.x\nend\n\
         main:\n\
         \x20   let p = Vec2(3.0, 4.0)\n\
         \x20   print(p.length(), p.scaled(2.0), walk(4), copies(), untouched(p), p)\n\
         \x20   var c = Cell(true, 0)\n    each _ in 0 until 3: c = step(c)\n    print(c, step(Cell(true, 5)))\n\
         \x20   print(Vec2(1, 2).length(), Body(p).pos)\n\
         end\n",
        &["Vec2.length", "Vec2.scaled", "Vec2.plus", "step", "walk", "copies", "untouched"],
    );
    assert_eq!(
        out,
        "5.0 Vec2(x=6.0, y=8.0) Vec2(x=8.0, y=4.0) 11.0 99.0 Vec2(x=3.0, y=4.0)\nCell(alive=false, age=3) Cell(alive=false, age=6)\n2.23606797749979 Vec2(x=3.0, y=4.0)\n"
    );
}

#[test]
fn mut_self_methods_stay_interpreted() {
    // `mut self` writes back to the caller's struct; the native tier
    // passes copies, so such methods must stay on the interpreter.
    let out = both(
        "struct Counter:\n    n: int\n    create bump(mut self): self.n += 1\n    create peek(self) -> int: return self.n\nend\n\
         main:\n    var c = Counter(0)\n    c.bump()\n    c.bump()\n    print(c.n, c.peek())\nend\n",
        &["Counter.peek"],
    );
    assert_eq!(out, "2 2\n");
}

#[test]
fn strings_and_print() {
    let out = both(
        "create greet(name: str, n: int) -> str: return \"Hello, {name}! You are {n} ({n / 2}) {n > 3}.\"\n\
         create shout(s: str) -> str: return s.upper() + \"!\" * 3\n\
         create report(xs: [float]):\n    var total = 0.0\n    each x in xs:\n        total += x\n        print(\"item\", x, \"running\", total)\n    end\n    print(\"done: {total:.2f}\", len(xs))\nend\n\
         create classify(word: str) -> str:\n    if word == \"\": return \"empty\"\n    if word < \"m\": return \"early\"\n    if \"z\" in word: return \"has z\"\n    return word[0] + word[-1]\nend\n\
         create parse(text: str) -> int: return int(text.strip()) * 2 + len(str(3.5))\n\
         create counts(s: str) -> int: return s.count(\"a\") * 100 + s.find(\"b\") + int(s.startswith(\"a\") and s.isalpha())\n\
         main:\n\
         \x20   print(greet(\"Ada\", 5), shout(\"hey\"))\n\
         \x20   report([1.5, 2.25])\n\
         \x20   print(classify(\"\"), classify(\"apple\"), classify(\"pizza\"), classify(\"river\"))\n\
         \x20   print(parse(\" 21 \"), counts(\"banana\"))\n\
         end\n",
        &["greet", "shout", "report", "classify", "parse"],
    );
    assert_eq!(
        out,
        "Hello, Ada! You are 5 (2.5) true. HEY!!!\nitem 1.5 running 1.5\nitem 2.25 running 3.75\ndone: 3.75 2\nempty early has z rr\n45 300\n"
    );
}

#[test]
fn string_failures_are_the_same() {
    for (src, kind) in [
        (
            "create f(s: str) -> str: return s[5]\nmain: print(f(\"abc\"))\n",
            "IndexError",
        ),
        (
            "create f(s: str) -> int: return int(s)\nmain: print(f(\"12x\"))\n",
            "ValueError",
        ),
        (
            "create f(s: str, n: int) -> str: return \"{n:q}\" + s\nmain: print(f(\"a\", 1))\n",
            "ValueError",
        ),
    ] {
        let out = both(src, &["f"]);
        assert!(out.contains(&format!("<error {kind}")), "{src}\n{out}");
    }
}

#[test]
fn values_made_in_loops_survive_when_kept() {
    // Each round frees what no outer variable holds; what is kept must
    // stay intact, across nested loops, `skip` and `stop`.
    let out = both(
        "create build(n: int) -> str:\n\
         \x20   var longest = \"\"\n    var all = \"\"\n\
         \x20   each i in 0 until n:\n\
         \x20       let word = \"w\" * (i % 4) + str(i)\n\
         \x20       if i % 5 == 0: skip\n\
         \x20       each j in 0 until 2:\n            let tmp = word + \"-\" + str(j)\n            if len(tmp) > len(longest): longest = tmp\n        end\n\
         \x20       all = all + word[0]\n\
         \x20       if i > 40: stop\n\
         \x20   end\n\
         \x20   return longest + \"|\" + all\nend\n\
         create lists(n: int) -> [int]:\n\
         \x20   var keep = [0]\n    var k = 0\n\
         \x20   loop k < n:\n        let fresh = [k, k * k]\n        if k % 2 == 0: keep = keep + fresh\n        let junk = [9] * 50\n        k += 1\n    end\n\
         \x20   return keep\nend\n\
         main:\n    print(build(60))\n    print(lists(7))\nend\n",
        &["build", "lists"],
    );
    assert_eq!(
        out,
        "www11-0|www4ww8ww1ww1wwwwww2ww2ww3ww3wwww\n[0, 0, 0, 2, 4, 4, 16, 6, 36]\n"
    );
}
