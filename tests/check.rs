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
    ok("create mystery(): return 1\nmain:\n    let x: float = 3\n    let y: str? = none\n    let z: int = mystery()\nend\n");
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
        one("create name(id: int) -> str:\n    return id\nend\n"),
        "`name` returns str, but this is int"
    );
    assert_eq!(
        one("create sign(x: int) -> int:\n    if x > 0: return 1\n    if x < 0: return -1\nend\n"),
        "`sign` can reach its end without returning a int"
    );
    ok("create sign(x: int) -> int:\n    if x > 0:\n        return 1\n    else if x < 0:\n        return -1\n    else:\n        return 0\n    end\nend\n");
    ok("create first(xs: [int]) -> int:\n    each x in xs: return x\n    fail \"empty\"\nend\n");
}

#[test]
fn arguments_match_parameters() {
    assert_eq!(
        one("create area(w: float, h: float) -> float: return w * h\nmain:\n    print(area(2, \"3\"))\nend\n"),
        "argument `h` of `area` is float, but this is str"
    );
    assert_eq!(
        one("struct P:\n    x: int\n    y: int\nend\nmain:\n    p = P(1, 2.5)\nend\n"),
        "argument `y` of `P` is int, but this is float"
    );
}

#[test]
fn struct_fields_must_exist() {
    let src = "struct Point:\n    x: float\n    y: float\n    create norm(self) -> float: return self.x\nend\n";
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
            "{src}create f(mut p: Point):\n    p.x = \"left\"\nend\n"
        )),
        "field `x` is float, but this is str"
    );
    ok(&format!(
        "{src}main:\n    let p = Point(1.0, 2.0)\n    print(p.x + p.norm())\nend\n"
    ));
}

#[test]
fn inherited_fields_are_found() {
    ok("object Actor:\n    hp: int = 10\nend\nobject Enemy(Actor):\n    damage: int = 2\nend\nmain:\n    e = Enemy()\n    let x: Enemy = e\n    print(x.hp + x.damage)\nend\n");
    let found = errors(
        "object Actor:\n    hp: int = 10\nend\nmain:\n    let a = Actor()\n    print(a.mana)\nend\n",
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
    let src = "struct V:\n    x: float\n    create grow(mut self): self.x += 1.0\nend\n";
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
            "{src}create bump(mut v: V): v.x += 1.0\nmain:\n    let v = V(1.0)\n    bump(mut v)\nend\n"
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
        one("strict create scale(v, k: float) -> float: return k\n"),
        "parameter `v` needs a type in strict code"
    );
    assert_eq!(
        one("strict create f(x: any): return x\n"),
        "parameter `x` cannot be `any` in strict code"
    );
    assert_eq!(
        one("strict create f(n: int) -> int:\n    total = n * 2\n    return total\nend\n"),
        "`total` is not declared"
    );
    assert_eq!(
        one("kernel create f(n: int):\n    n = 2\nend\n"),
        "cannot assign to `n`: strict code takes parameters read-only"
    );
    ok("strict create f(mut n: int, k: int) -> int:\n    let twice = k * 2\n    var acc = 0\n    acc += twice\n    n = acc\n    return n\nend\n");
}

// ---- borrows --------------------------------------------------------------

#[test]
fn moved_values_are_not_used() {
    assert_eq!(
        one("create keep(own xs): return xs\nmain:\n    xs = [1, 2]\n    ys = keep(move xs)\n    print(xs)\nend\n"),
        "`xs` is used after it was moved"
    );
    assert_eq!(
        one("create keep(own xs): return xs\nmain:\n    xs = [1]\n    if len(xs) > 0: keep(move xs)\n    print(xs)\nend\n"),
        "`xs` may have been moved"
    );
    assert_eq!(
        one("create keep(own xs): return xs\nmain:\n    xs = [1]\n    each i in 0 until 3:\n        keep(move xs)\n    end\nend\n"),
        "`xs` may have been moved"
    );
    // Assigning gives the variable a new value.
    ok("create keep(own xs): return xs\nmain:\n    xs = [1]\n    keep(move xs)\n    xs = [2]\n    print(xs)\nend\n");
    // Moved on both paths, then reassigned on both.
    ok("create keep(own xs): return xs\nmain:\n    xs = [1]\n    if true:\n        keep(move xs)\n        xs = []\n    else:\n        xs = [3]\n    end\n    print(xs)\nend\n");
}

#[test]
fn mut_arguments_are_exclusive() {
    let src = "create swap(mut a, mut b): return a\ncreate add(mut a, b): return a\n";
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
        one("create reset(mut xs): xs.clear()\nmain:\n    xs = [1]\n    reset(xs)\nend\n"),
        "`reset` changes its argument `xs`; pass it with `mut`"
    );
    // A temporary has no one to see the change.
    ok("create reset(mut xs): xs.clear()\nmain:\n    reset([1, 2])\nend\n");
}

#[test]
fn for_mut_has_the_list_to_itself() {
    assert_eq!(
        one("main:\n    var xs = [1, 2]\n    each mut x in xs:\n        x += len(xs)\n    end\nend\n"),
        "`xs` cannot be used here: an `each mut` loop is changing its elements"
    );
    assert_eq!(
        one("main:\n    let xs = [1, 2]\n    each mut x in xs: x += 1\nend\n"),
        "cannot change the elements of `xs`: it is declared with `let`"
    );
    ok("main:\n    var xs = [1, 2]\n    each mut x in xs: x += 1\n    print(xs)\nend\n");
}

#[test]
fn changing_a_loop_copy_is_reported() {
    let src = "struct E:\n    hp: int\nend\n";
    assert_eq!(
        one(&format!(
            "{src}create heal(es: [E]):\n    each e in es: e.hp = 10\nend\n"
        )),
        "this changes a copy: `e` is a copy of the element"
    );
    ok(&format!(
        "{src}create heal(mut es: [E]):\n    each mut e in es: e.hp = 10\nend\n"
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

// ---- C functions --------------------------------------------------------------

#[test]
fn pointer_passing_c_calls_need_unsafe() {
    let decl = "extern \"c\":\n    create cos(x: f64) -> f64\n    create fill(x: tensor[f32], n: i32)\n    create open_dev() -> ptr u8\nend\n";
    assert_eq!(
        one(&format!("{decl}main:\n    fill(zeros(4), 4)\nend\n")),
        "`fill` passes raw pointers to C, so it can only be called in an `unsafe:` block"
    );
    assert_eq!(
        one(&format!("{decl}main:\n    h = open_dev()\nend\n")),
        "`open_dev` passes raw pointers to C, so it can only be called in an `unsafe:` block"
    );
    // Numbers only: safe to call anywhere. Pointers: fine inside `unsafe:`.
    ok(&format!("{decl}main:\n    print(cos(0.0))\n    unsafe:\n        fill(zeros(4), 4)\n        let h = open_dev()\n    end\nend\n"));
    assert_eq!(
        one(&format!("{decl}main:\n    print(cos(\"zero\"))\nend\n")),
        "argument `x` of `cos` is float, but this is str"
    );
}

// ---- null safety ----------------------------------------------------------------

const USERS: &str = "struct User:\n    name: str\n    email: str?\nend\ncreate find(id: int) -> User?:\n    if id == 1: return User(\"Ada\", none)\n    return none\nend\n";

#[test]
fn optional_values_must_be_checked_before_use() {
    assert_eq!(
        one(&format!(
            "{USERS}main:\n    let u = find(1)\n    print(u.name)\nend\n"
        )),
        "`u` may be none, so it cannot be used in `.name`"
    );
    assert_eq!(
        one(&format!(
            "{USERS}create domain(u: User) -> str:\n    return u.email.split(\"@\")[1]\nend\n"
        )),
        "`u.email` may be none, so it cannot be used in `.split()`"
    );
    assert_eq!(
        one("main:\n    let n: int? = none\n    print(n + 1)\nend\n"),
        "`n` may be none, so it cannot be used in arithmetic"
    );
    assert_eq!(
        one(&format!("{USERS}create greet(name: str): print(name)\nmain:\n    let u = find(1)\n    greet(u?.name)\nend\n")),
        "argument `name` of `greet` is str, but this may be none"
    );
    assert_eq!(
        one(&format!(
            "{USERS}create first() -> User:\n    return find(1)\nend\n"
        )),
        "`first` returns User, but this may be none"
    );
}

#[test]
fn checks_narrow_optional_values() {
    ok(&format!(
        "{USERS}main:\n\
         \x20   let u = find(1)\n\
         \x20   if u != none: print(u.name)\n\
         \x20   if u != none and u.name == \"Ada\": print(\"hi\")\n\
         \x20   print(u?.name ?? \"nobody\")\n\
         \x20   var v: User? = find(2)\n\
         \x20   if v == none:\n\
         \x20       return\n\
         \x20   end\n\
         \x20   print(v.name)\n\
         end\n"
    ));
    ok(&format!(
        "{USERS}create name_of(u: User?) -> str:\n\
         \x20   if u == none: return \"nobody\"\n\
         \x20   return u.name\n\
         end\n\
         create shout(u: User?) -> str:\n\
         \x20   if u != none:\n\
         \x20       return u.name\n\
         \x20   else:\n\
         \x20       return \"?\"\n\
         \x20   end\n\
         end\n"
    ));
    // Assigning `none` again ends the narrowing.
    assert_eq!(
        one(&format!("{USERS}main:\n    var v: User? = find(1)\n    if v != none:\n        v = none\n        print(v.name)\n    end\nend\n")),
        "`v` may be none, so it cannot be used in `.name`"
    );
}

// ---- exhaustive match -----------------------------------------------------------

const SHAPE: &str =
    "enum Shape:\n    Circle(r: float)\n    Rect(w: float, h: float)\n    Empty\nend\n";

#[test]
fn matches_handle_every_case() {
    assert_eq!(
        one(&format!("{SHAPE}create area(s: Shape) -> float:\n    when s:\n        is Shape.Circle(r): return 3.14 * r * r\n        is Shape.Rect(w, h): return w * h\n    end\n    return 0.0\nend\n")),
        "this `when` does not handle `Shape.Empty`"
    );
    // A partial pattern or a guard does not count.
    assert_eq!(
        one(&format!("{SHAPE}create f(s: Shape):\n    when s:\n        is Shape.Circle(r) if r > 0.0: print(r)\n        is Shape.Rect(0.0, h): print(h)\n        is Shape.Empty: print(0)\n    end\nend\n")),
        "this `when` does not handle `Shape.Circle`, `Shape.Rect`"
    );
    assert_eq!(
        one("create f(b: bool):\n    when b:\n        is true: print(1)\n    end\nend\n"),
        "this `when` does not handle `false`"
    );
    ok(&format!("{SHAPE}create f(s: Shape):\n    when s:\n        is Shape.Circle(r): print(r)\n        is Shape.Rect(_, _) or Shape.Empty: print(0)\n    end\nend\n"));
    ok(&format!("{SHAPE}create f(s: Shape):\n    when s:\n        is Shape.Circle(r): print(r)\n        else: print(0)\n    end\nend\n"));
    ok(&format!("{SHAPE}create f(s: Shape):\n    when s:\n        is Shape.Empty: print(0)\n        is other: print(other)\n    end\nend\n"));
    // Unknown types are not checked.
    ok("create f(x):\n    when x:\n        is 1: print(1)\n    end\nend\n");
}

// ---- generics ---------------------------------------------------------------------

#[test]
fn generic_functions_infer_their_type_parameters() {
    let src = "create first[T](xs: [T]) -> T:\n    return xs[0]\nend\ncreate pair[T](a: T, b: T) -> [T]:\n    return [a, b]\nend\n";
    assert_eq!(
        one(&format!(
            "{src}main:\n    let n: str = first([1, 2])\nend\n"
        )),
        "`n` is str, but this is int"
    );
    assert_eq!(
        one(&format!("{src}main:\n    print(pair(1, \"a\"))\nend\n")),
        "`T` in `pair` stands for int from an earlier argument, but this is str"
    );
    assert_eq!(
        one(&format!(
            "{src}main:\n    let xs: [str] = pair(1, 2)\nend\n"
        )),
        "`xs` is [str], but this is [int]"
    );
    ok(&format!("{src}main:\n    let a: int = first([1, 2])\n    let b: [float] = pair(1, 2.5)\n    let c: str = first([\"x\"])\nend\n"));
}
