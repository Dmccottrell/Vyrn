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

#[test]
fn list_and_map_types_written_either_way_check_their_items() {
    assert_eq!(
        one("main:\n    let xs: list[int] = [1, 2]\n    xs.append(\"three\")\nend\n"),
        "an item of this list[int] is int, but this is str"
    );
    assert_eq!(
        one("create f(xs: [str]):\n    xs.insert(0, 5)\nend\nmain:\n    f([])\nend\n"),
        "an item of this list[str] is str, but this is int"
    );
    assert_eq!(
        one("main:\n    let m: map[str, int] = {\"a\": 1}\n    let s: str = m.values()[0]\nend\n"),
        "`s` is str, but this is int"
    );
    ok("main:\n    let xs: list[float] = [1.5]\n    xs.append(2)\n    xs.extend([3.5])\n    let last: float = xs.pop()\nend\n");
}

#[test]
fn function_types_check_lambdas_and_calls() {
    ok("create apply(f: (int) -> int, n: int) -> int:\n    return f(n)\nend\n\
        create map_all[T, U](xs: list[T], f: (T) -> U) -> list[U]:\n    return [f(x) each x in xs]\nend\n\
        create square(n: int) -> int: return n * n\n\
        main:\n    print(apply(x => x * 2, 5), apply(square, 3), map_all([\"a\"], w => w.upper()))\n\
        \x20   let g: (int) -> str = n => \"#{n}\"\n    print(g(4))\nend\n");
    assert_eq!(
        one("create apply(f: (int) -> int, n: int) -> int:\n    return f(n)\nend\nmain:\n    print(apply(x => x + \"!\", 1))\nend\n"),
        "cannot apply `+` to int and str"
    );
    assert_eq!(
        one("create apply(f: (int) -> int, n: int) -> int:\n    return f(n)\nend\nmain:\n    print(apply(x => \"a\", 1))\nend\n"),
        "argument `f` of `apply` is (int) -> int, but this is (int) -> str"
    );
    assert_eq!(
        one("main:\n    let g: (int) -> str = n => \"{n}\"\n    print(g(\"x\"))\nend\n"),
        "argument `1` of `g` is int, but this is str"
    );
    assert_eq!(
        one("create run(f: (int) -> int) -> int:\n    return f(1, 2)\nend\nmain:\n    print(run(x => x))\nend\n"),
        "this function takes 1 argument, but 2 are given"
    );
    assert_eq!(
        one("main:\n    let f = 3\n    f()\nend\n"),
        "this is int, not a function, so it cannot be called"
    );
}

#[test]
fn traits_are_types_and_bounds_are_enforced() {
    let prelude = "trait Shape:\n    create area(self) -> float\nend\n\
                   struct Sq:\n    s: float\nend\nstruct Dog:\n    n: str\nend\n\
                   impl Shape for Sq:\n    create area(self) -> float:\n        return self.s\n    end\nend\n\
                   create total[T: Shape](xs: list[T]) -> float:\n    return 0.0\nend\n";
    ok(&format!("{prelude}main:\n    let t: Shape = Sq(2.0)\n    print(t.area() + 1.0, total([Sq(1.0)]))\n    let all: list[Shape] = [Sq(1.0)]\nend\n"));
    assert_eq!(
        one(&format!(
            "{prelude}main:\n    print(total([Dog(\"x\")]))\nend\n"
        )),
        "`T` in `total` must be a `Shape`, but here it is Dog"
    );
    assert_eq!(
        one(&format!("{prelude}main:\n    let s: Shape = 5\nend\n")),
        "`s` is Shape, but this is int"
    );
    assert_eq!(
        one(&format!(
            "{prelude}main:\n    let t: Shape = Sq(1.0)\n    t.bark()\nend\n"
        )),
        "`Shape` has no method `bark`"
    );
}

#[test]
fn numbers_have_only_number_methods() {
    assert_eq!(
        one("create first[T](xs: list[T]) -> T:\n    return xs[0]\nend\nmain:\n    print(first([10, 20]).upper())\nend\n"),
        "int has no method `upper`"
    );
    ok("main:\n    let x = 2.5\n    print(x.round(), x.floor(), (3).is_odd(), x.clamp(0, 1))\nend\n");
}

#[test]
fn own_parameters_need_move_and_moved_values_are_gone() {
    let take = "create take(own xs: list) -> int:\n    return len(xs)\nend\n";
    assert_eq!(
        one(&format!(
            "{take}main:\n    var xs = [1]\n    print(take(xs))\nend\n"
        )),
        "`take` keeps its argument `xs`; pass it with `move`"
    );
    assert_eq!(
        one(&format!(
            "{take}main:\n    var xs = [1]\n    print(take(move xs))\n    print(xs)\nend\n"
        )),
        "`xs` is used after it was moved"
    );
    ok(&format!("{take}main:\n    var xs = [1]\n    print(take(xs.copy()), take([2]), take(move xs))\nend\n"));
}

#[test]
fn strict_code_cannot_change_read_only_parameters_through_methods() {
    assert_eq!(
        one("strict create add(xs: list[int]):\n    xs.append(1)\nend\nmain:\n    add([1])\nend\n"),
        "cannot call `.append()` on `xs`: strict code takes parameters read-only"
    );
    ok("strict create add(mut xs: list[int]):\n    xs.append(1)\nend\ncreate loose(xs: list[int]):\n    xs.append(1)\nend\nmain:\n    var a = [1]\n    add(mut a)\n    loose(a)\nend\n");
}

#[test]
fn checking_a_field_for_none_narrows_it() {
    let n = "object N:\n    v: int\n    next: N? = none\nend\ncreate take(n: N) -> int:\n    return n.v\nend\n";
    ok(&format!("{n}create f(head: N):\n    if head.next != none:\n        print(take(head.next), head.next.v)\n    end\n    if head.next == none: return\n    print(take(head.next))\nend\nmain:\n    f(N(1))\nend\n"));
    assert_eq!(
        one(&format!("{n}create f(head: N):\n    if head.next == none: return\n    head.next = none\n    print(take(head.next))\nend\nmain:\n    f(N(1))\nend\n")),
        "argument `n` of `take` is N, but this may be none"
    );
}
