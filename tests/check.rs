//! The checks that run before code generation: gradual types, `let`,
//! `strict` and borrow rules. Each rule has programs it rejects and
//! programs it must accept.

use vyrn::check::check;
use vyrn::parse;

fn errors(src: &str) -> Vec<String> {
    let module = parse(src).unwrap_or_else(|e| panic!("parse errors: {e:?}"));
    check(&module).into_iter().map(|d| d.message).collect()
}

fn ok(src: &str) {
    let found = errors(src);
    assert!(found.is_empty(), "expected no errors, got {found:?}");
}

fn one(src: &str) -> String {
    let found = errors(src);
    assert_eq!(found.len(), 1, "expected one error, got {found:?}");
    found.into_iter().next().unwrap_or_default()
}

// ---- types ----------------------------------------------------------------

#[test]
fn annotated_bindings_must_match() {
    assert_eq!(
        one("main:\n    let n: int = \"seven\"\nend\n"),
        "`n` is int, but this is str"
    );
    assert_eq!(
        one("main:\n    var names: [str] = [1, 2]\nend\n"),
        "`names` is [str], but this is [int]"
    );
    // An int is a fine float, `none` a fine optional, and unknowns pass.
    ok("fn mystery(): return 1\nmain:\n    let x: float = 3\n    let y: str? = none\n    let z: int = mystery()\nend\n");
}

#[test]
fn assignments_keep_the_declared_type() {
    assert_eq!(
        one("main:\n    var total: float = 0\n    total = \"done\"\nend\n"),
        "`total` is float, but this is str"
    );
    // Unannotated variables are dynamic, as in Python.
    ok("main:\n    x = 1\n    x = \"one\"\n    var y = 2\n    y = [y]\nend\n");
}

#[test]
fn returns_match_the_signature() {
    assert_eq!(
        one("fn name(id: int) -> str:\n    return id\nend\n"),
        "`name` returns str, but this is int"
    );
    assert_eq!(
        one("fn sign(x: int) -> int:\n    if x > 0: return 1\n    if x < 0: return -1\nend\n"),
        "`sign` can reach its end without returning a int"
    );
    ok("fn sign(x: int) -> int:\n    if x > 0:\n        return 1\n    elif x < 0:\n        return -1\n    else:\n        return 0\n    end\nend\n");
    ok("fn first(xs: [int]) -> int:\n    for x in xs: return x\n    fail \"empty\"\nend\n");
}

#[test]
fn arguments_match_parameters() {
    assert_eq!(
        one("fn area(w: float, h: float) -> float: return w * h\nmain:\n    print(area(2, \"3\"))\nend\n"),
        "argument `h` of `area` is float, but this is str"
    );
    assert_eq!(
        one("struct P:\n    x: int\n    y: int\nend\nmain:\n    p = P(1, 2.5)\nend\n"),
        "argument `y` of `P` is int, but this is float"
    );
}

#[test]
fn struct_fields_must_exist() {
    let src = "struct Point:\n    x: float\n    y: float\n    fn norm(self) -> float: return self.x\nend\n";
    assert_eq!(
        one(&format!(
            "{src}main:\n    let p = Point(1.0, 2.0)\n    print(p.z)\nend\n"
        )),
        "`Point` has no field `z`"
    );
    assert_eq!(
        one(&format!(
            "{src}main:\n    let p = Point(1.0, 2.0)\n    print(p.size())\nend\n"
        )),
        "`Point` has no method `size`"
    );
    assert_eq!(
        one(&format!(
            "{src}fn f(mut p: Point):\n    p.x = \"left\"\nend\n"
        )),
        "field `x` is float, but this is str"
    );
    ok(&format!(
        "{src}main:\n    let p = Point(1.0, 2.0)\n    print(p.x + p.norm())\nend\n"
    ));
}

#[test]
fn inherited_fields_are_found() {
    ok("class Actor:\n    hp: int = 10\nend\nclass Enemy(Actor):\n    damage: int = 2\nend\nmain:\n    e = Enemy()\n    let x: Enemy = e\n    print(x.hp + x.damage)\nend\n");
    let found = errors(
        "class Actor:\n    hp: int = 10\nend\nmain:\n    let a = Actor()\n    print(a.mana)\nend\n",
    );
    assert_eq!(found, ["`Actor` has no field `mana`"]);
}

#[test]
fn obvious_operator_mistakes() {
    assert_eq!(
        one("main:\n    print(1 + \"a\")\nend\n"),
        "cannot apply `+` to int and str"
    );
    // These are fine: concatenation, repetition, mixed numbers.
    ok("main:\n    print(\"a\" + \"b\", \"ab\" * 3, 1 + 2.5, 7 // 2)\nend\n");
}

// ---- let --------------------------------------------------------------------

#[test]
fn let_bindings_are_not_reassigned() {
    assert_eq!(
        one("main:\n    let limit = 10\n    limit = 20\nend\n"),
        "cannot assign to `limit`: it is declared with `let`"
    );
    assert_eq!(
        one("main:\n    let n = 1\n    n += 1\nend\n"),
        "cannot assign to `n`: it is declared with `let`"
    );
    ok("main:\n    var n = 1\n    n += 1\n    m = 1\n    m = 2\nend\n");
}

#[test]
fn let_structs_are_not_changed() {
    let src = "struct V:\n    x: float\n    fn grow(mut self): self.x += 1.0\nend\n";
    assert_eq!(
        one(&format!(
            "{src}main:\n    let v = V(1.0)\n    v.x = 2.0\nend\n"
        )),
        "cannot change a field of `v`: it is declared with `let`"
    );
    assert_eq!(
        one(&format!(
            "{src}main:\n    let v = V(1.0)\n    v.grow()\nend\n"
        )),
        "cannot call `mut self` method `grow` on `v`: it is declared with `let`"
    );
    assert_eq!(
        one(&format!(
            "{src}fn bump(mut v: V): v.x += 1.0\nmain:\n    let v = V(1.0)\n    bump(mut v)\nend\n"
        )),
        "cannot pass as `mut` `v`: it is declared with `let`"
    );
    ok(&format!(
        "{src}main:\n    var v = V(1.0)\n    v.x = 2.0\n    v.grow()\nend\n"
    ));
}

// ---- strict -----------------------------------------------------------------

#[test]
fn strict_functions_are_fully_typed() {
    assert_eq!(
        one("strict fn scale(v, k: float) -> float: return k\n"),
        "parameter `v` needs a type in strict code"
    );
    assert_eq!(
        one("strict fn f(x: any): return x\n"),
        "parameter `x` cannot be `any` in strict code"
    );
    assert_eq!(
        one("strict fn f(n: int) -> int:\n    total = n * 2\n    return total\nend\n"),
        "`total` is not declared"
    );
    assert_eq!(
        one("kernel fn f(n: int):\n    n = 2\nend\n"),
        "cannot assign to `n`: strict code takes parameters read-only"
    );
    ok("strict fn f(mut n: int, k: int) -> int:\n    let twice = k * 2\n    var acc = 0\n    acc += twice\n    n = acc\n    return n\nend\n");
}

// ---- borrows --------------------------------------------------------------

#[test]
fn moved_values_are_not_used() {
    assert_eq!(
        one("fn keep(own xs): return xs\nmain:\n    xs = [1, 2]\n    ys = keep(move xs)\n    print(xs)\nend\n"),
        "`xs` is used after it was moved"
    );
    assert_eq!(
        one("fn keep(own xs): return xs\nmain:\n    xs = [1]\n    if len(xs) > 0: keep(move xs)\n    print(xs)\nend\n"),
        "`xs` may have been moved"
    );
    assert_eq!(
        one("fn keep(own xs): return xs\nmain:\n    xs = [1]\n    for i in 0 until 3:\n        keep(move xs)\n    end\nend\n"),
        "`xs` may have been moved"
    );
    // Assigning gives the variable a new value.
    ok("fn keep(own xs): return xs\nmain:\n    xs = [1]\n    keep(move xs)\n    xs = [2]\n    print(xs)\nend\n");
    // Moved on both paths, then reassigned on both.
    ok("fn keep(own xs): return xs\nmain:\n    xs = [1]\n    if true:\n        keep(move xs)\n        xs = []\n    else:\n        xs = [3]\n    end\n    print(xs)\nend\n");
}

#[test]
fn mut_arguments_are_exclusive() {
    let src = "fn swap(mut a, mut b): return a\nfn add(mut a, b): return a\n";
    assert_eq!(
        one(&format!(
            "{src}main:\n    xs = [1]\n    swap(mut xs, mut xs)\nend\n"
        )),
        "`xs` is passed as `mut` and also used elsewhere in this call"
    );
    assert_eq!(
        one(&format!(
            "{src}main:\n    xs = [1]\n    add(mut xs, xs)\nend\n"
        )),
        "`xs` is passed as `mut` and also used elsewhere in this call"
    );
    ok(&format!(
        "{src}main:\n    xs = [1]\n    ys = [2]\n    swap(mut xs, mut ys)\nend\n"
    ));
}

#[test]
fn mut_parameters_need_mut_arguments() {
    assert_eq!(
        one("fn reset(mut xs): xs.clear()\nmain:\n    xs = [1]\n    reset(xs)\nend\n"),
        "`reset` changes its argument `xs`; pass it with `mut`"
    );
    // A temporary has no one to see the change.
    ok("fn reset(mut xs): xs.clear()\nmain:\n    reset([1, 2])\nend\n");
}

#[test]
fn for_mut_has_the_list_to_itself() {
    assert_eq!(
        one("main:\n    var xs = [1, 2]\n    for mut x in xs:\n        x += len(xs)\n    end\nend\n"),
        "`xs` cannot be used here: a `for mut` loop is changing its elements"
    );
    assert_eq!(
        one("main:\n    let xs = [1, 2]\n    for mut x in xs: x += 1\nend\n"),
        "cannot change the elements of `xs`: it is declared with `let`"
    );
    ok("main:\n    var xs = [1, 2]\n    for mut x in xs: x += 1\n    print(xs)\nend\n");
}

#[test]
fn changing_a_loop_copy_is_reported() {
    let src = "struct E:\n    hp: int\nend\n";
    assert_eq!(
        one(&format!(
            "{src}fn heal(es: [E]):\n    for e in es: e.hp = 10\nend\n"
        )),
        "this changes a copy: `e` is a copy of the element"
    );
    ok(&format!(
        "{src}fn heal(mut es: [E]):\n    for mut e in es: e.hp = 10\nend\n"
    ));
}

#[test]
fn errors_stop_compilation_with_positions() {
    let errors = vyrn::vm::compile_source("main:\n    let n = 1\n    n = 2\nend\n").unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].span.start.line, 3);
    assert_eq!(
        errors[0].hint.as_deref(),
        Some("declare it with `var n` to change it")
    );
}
