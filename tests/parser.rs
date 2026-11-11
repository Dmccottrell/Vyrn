//! Parser tests. Trees are compared in the compact form from `vyrn::pretty`,
//! where `a + b * c` reads `(+ a (* b c))`.

use std::path::Path;
use vyrn::{parse, parse_expr, parse_recovering, pretty, Diagnostic};

fn ex(src: &str) -> String {
    match parse_expr(src) {
        Ok(expr) => pretty::expr(&expr),
        Err(errors) => panic!("{src:?} failed to parse: {errors:?}"),
    }
}

fn tree(src: &str) -> String {
    match parse(src) {
        Ok(module) => pretty::module(&module),
        Err(errors) => {
            let report: String = errors.iter().map(|e| e.render("test.vyrn", src)).collect();
            panic!("failed to parse:\n{report}");
        }
    }
}

/// Parses `body` inside `main:` and returns the printed statements without
/// the `main` line and its indentation.
fn stmts(body: &str) -> String {
    let indented: String = body.lines().map(|l| format!("    {l}\n")).collect();
    let printed = tree(&format!("main:\n{indented}end\n"));
    printed
        .lines()
        .skip(1)
        .map(|l| format!("{}\n", &l[2..]))
        .collect()
}

fn errors(src: &str) -> Vec<Diagnostic> {
    parse_recovering(src).1
}

fn one_error(src: &str) -> Diagnostic {
    let errors = errors(src);
    assert_eq!(errors.len(), 1, "expected one error, got {errors:#?}");
    errors.into_iter().next().unwrap()
}

// ---- expressions ----------------------------------------------------------

#[test]
fn arithmetic_precedence() {
    assert_eq!(ex("a + b * c"), "(+ a (* b c))");
    assert_eq!(ex("(a + b) * c"), "(* (+ a b) c)");
    assert_eq!(ex("a - b - c"), "(- (- a b) c)");
    assert_eq!(ex("a // b % c"), "(% (// a b) c)");
    assert_eq!(ex("-x ** 2"), "(neg (** x 2))");
    assert_eq!(ex("2 ** 3 ** 2"), "(** 2 (** 3 2))");
    assert_eq!(ex("a << 1 + b"), "(<< a (+ 1 b))");
    assert_eq!(ex("a | b ^ c & d"), "(| a (^ b (& c d)))");
}

#[test]
fn matmul_shares_the_multiplication_level() {
    assert_eq!(ex("a @ b"), "(@ a b)");
    assert_eq!(ex("a @ b * c"), "(* (@ a b) c)");
    assert_eq!(ex("x + h @ w"), "(+ x (@ h w))");
    assert_eq!(ex("q @ k.mT / scale"), "(/ (@ q (. k mT)) scale)");
}

#[test]
fn logic_and_comparison() {
    assert_eq!(ex("a or b and not c"), "(or a (and b (not c)))");
    assert_eq!(ex("not a == b"), "(not (== a b))");
    assert_eq!(ex("a < b <= c"), "(compare a < b <= c)");
    assert_eq!(ex("x in xs"), "(in x xs)");
    assert_eq!(ex("x not in xs"), "(not in x xs)");
    assert_eq!(ex("a ?? b ?? c"), "(?? (?? a b) c)");
}

#[test]
fn ranges() {
    assert_eq!(ex("0 to 10"), "(to 0 10)");
    assert_eq!(ex("0 until n by 2"), "(until 0 n by 2)");
    assert_eq!(ex("i in 0 until n"), "(in i (until 0 n))");
    assert_eq!(ex("a + 1 to b - 1"), "(to (+ a 1) (- b 1))");
}

#[test]
fn placement_pipeline_and_conditional() {
    assert_eq!(ex("model on gpu[0]"), "(on model (index gpu 0))");
    assert_eq!(ex("argmax(x) on cpu"), "(on (call argmax x) cpu)");
    assert_eq!(ex("xs |> sort |> first"), "(|> (|> xs sort) first)");
    assert_eq!(
        ex("\"A\" if s >= 90 else \"F\""),
        "(if (>= s 90) \"A\" \"F\")"
    );
    assert_eq!(ex("a if x else b if y else c"), "(if x a (if y b c))");
}

#[test]
fn prefix_operators() {
    assert_eq!(ex("await db.find(id)"), "(await (call (. db find) id))");
    assert_eq!(
        ex("spawn step(move batch)"),
        "(spawn (call step (move batch)))"
    );
    assert_eq!(ex("~mask"), "(~ mask)");
}

#[test]
fn postfix_chains() {
    assert_eq!(ex("a.b.c"), "(. (. a b) c)");
    assert_eq!(ex("user?.name"), "(?. user name)");
    assert_eq!(ex("f(x)(y)"), "(call (call f x) y)");
    assert_eq!(ex("range.end"), "(. range end)");
    assert_eq!(
        ex("Clock(hz = 120, mut world, x)"),
        "(call Clock hz=120 (mut world) x)"
    );
}

#[test]
fn slicing() {
    assert_eq!(ex("h[:, -1]"), "(index h (slice _ _ _) (neg 1))");
    assert_eq!(ex("xs[1:9:2]"), "(index xs (slice 1 9 2))");
    assert_eq!(ex("xs[::2]"), "(index xs (slice _ _ 2))");
    assert_eq!(ex("xs[-3:]"), "(index xs (slice (neg 3) _ _))");
    assert_eq!(ex("xs[:n]"), "(index xs (slice _ n _))");
    assert_eq!(ex("t[..., none]"), "(index t ... none)");
    assert_eq!(ex("m[i, j]"), "(index m i j)");
}

#[test]
fn literals_and_collections() {
    assert_eq!(ex("255u8"), "255u8");
    assert_eq!(ex("1.5f32"), "1.5f32");
    assert_eq!(ex("[]"), "(list)");
    assert_eq!(ex("[1, 2,]"), "(list 1 2)");
    assert_eq!(ex("{}"), "(map)");
    assert_eq!(ex("{\"a\": 1, \"b\": 2}"), "(map (\"a\" 1) (\"b\" 2))");
    assert_eq!(ex("()"), "(tuple)");
    assert_eq!(ex("(a,)"), "(tuple a)");
    assert_eq!(ex("(a, b)"), "(tuple a b)");
    assert_eq!(ex("(a)"), "a");
    assert_eq!(ex("true and none == false"), "(and true (== none false))");
}

#[test]
fn comprehensions_and_lambdas() {
    assert_eq!(
        ex("[x * 2 each x in xs if x > 0]"),
        "(list-comp (* x 2) (each x xs) (if (> x 0)))"
    );
    assert_eq!(
        ex("[a + b each a in xs each b in ys]"),
        "(list-comp (+ a b) (each a xs) (each b ys))"
    );
    assert_eq!(
        ex("{k: v each k, v in pairs}"),
        "(map-comp k v (each (k, v) pairs))"
    );
    assert_eq!(ex("(x) => x * 2"), "(=> (x) (* x 2))");
    assert_eq!(
        ex("async (x, n = 1) => await f(x)"),
        "(async => (x, n = 1) (await (call f x)))"
    );
    assert_eq!(
        ex("map(xs, (x) => x + 1)"),
        "(call map xs (=> (x) (+ x 1)))"
    );
}

#[test]
fn string_interpolation_is_parsed() {
    assert_eq!(
        ex("\"Average: {average}, grade {letter_for(average)}\""),
        "\"Average: {average}, grade {(call letter_for average)}\""
    );
    assert_eq!(ex("'{a + b}!'"), "\"{(+ a b)}!\"");
    assert_eq!(ex("r\"{raw}\""), "\"\\{raw\\}\"");
}

// ---- statements -----------------------------------------------------------

#[test]
fn bindings_and_assignments() {
    assert_eq!(
        stmts(
            "total = 0\n\
             let a, b = 1, 2\n\
             var w: [f32; 3] on frame = zeros()\n\
             x.y[i] += 1\n\
             a, b = b, a\n\
             m @= w.T"
        ),
        "total = 0\n\
         let (a, b) = (tuple 1 2)\n\
         var w: [f32; 3] on frame = (call zeros)\n\
         (index (. x y) i) += 1\n\
         (tuple a b) = (tuple b a)\n\
         m @= (. w T)\n"
    );
}

#[test]
fn one_line_and_block_bodies() {
    assert_eq!(
        stmts(
            "if lives == 0: stop\n\
             each e in es: e.update(dt)\n\
             loop running:\n    tick()\nend"
        ),
        "if (== lives 0)\n  stop\n\
         each e in es\n  (call (. e update) dt)\n\
         loop running\n  (call tick)\n"
    );
}

#[test]
fn if_chains_share_one_end() {
    assert_eq!(
        stmts(
            "if a:\n    x = 1\n\
             else if b: x = 2\n\
             else if c:\n    x = 3\n    y = 4\n\
             else: x = 0\n\
             end"
        ),
        "if a\n  x = 1\n\
         else if b\n  x = 2\n\
         else if c\n  x = 3\n  y = 4\n\
         else\n  x = 0\n"
    );
}

#[test]
fn nested_blocks_close_in_order() {
    assert_eq!(
        stmts(
            "each row in grid:\n\
             \x20   each cell in row:\n\
             \x20       if cell.alive:\n\
             \x20           count += 1\n\
             \x20       end\n\
             \x20   end\n\
             end\n\
             print(count)"
        ),
        "each row in grid\n\
         \x20 each cell in row\n\
         \x20   if (. cell alive)\n\
         \x20     count += 1\n\
         (call print count)\n"
    );
}

#[test]
fn loops_of_every_kind() {
    assert_eq!(
        stmts(
            "each mut e in world: e.x += 1\n\
             par each dev, batch in zip(devices, data): run(batch)\n\
             async each event in stream: handle(event)\n\
             loop: skip"
        ),
        "each mut e in world\n  (. e x) += 1\n\
         par each (dev, batch) in (call zip devices data)\n  (call run batch)\n\
         async each event in stream\n  (call handle event)\n\
         loop\n  skip\n"
    );
}

#[test]
fn match_arms() {
    assert_eq!(
        stmts(
            "when shape:\n\
             \x20   is Shape.Circle(r): return r\n\
             \x20   is Shape.Rect(w, h) if w > 0:\n\
             \x20       return w * h\n\
             \x20   is 1 to 9 or 20 as n: return n\n\
             \x20   is [first, ...rest]: return first\n\
             \x20   is Point(x = 0, y = _): return 0\n\
             \x20   else: return -1\n\
             end"
        ),
        "when shape\n\
         \x20 is Shape.Circle(r)\n    return r\n\
         \x20 is Shape.Rect(w, h) if (> w 0)\n    return (* w h)\n\
         \x20 is 1 to 9 or 20 as n\n    return n\n\
         \x20 is [first, ...rest]\n    return first\n\
         \x20 is Point(x = 0, y = _)\n    return 0\n\
         \x20 else\n    return (neg 1)\n"
    );
}

#[test]
fn try_with_on_region() {
    assert_eq!(
        stmts(
            "attempt:\n    risky()\n\
             catch IoError as e: log(e)\n\
             catch: fail Unknown()\n\
             always: close()\n\
             end\n\
             using open(p) as f, lock: f.read()\n\
             on gpu[1]: y = x @ w\n\
             region frame:\n    let tmp: [u8] on frame = scratch()\nend"
        ),
        "attempt\n  (call risky)\n\
         catch IoError as e\n  (call log e)\n\
         catch\n  fail (call Unknown)\n\
         always\n  (call close)\n\
         using (call open p) as f, lock\n  (call (. f read))\n\
         on (index gpu 1)\n  y = (@ x w)\n\
         region frame\n  let tmp: [u8] on frame = (call scratch)\n"
    );
}

#[test]
fn nested_functions_and_doc_comments() {
    assert_eq!(
        stmts("## Squares a number.\ncreate sq(x): return x * x\nprint(sq(3))"),
        "## Squares a number.\ncreate sq(x)\n  return (* x x)\n(call print (call sq 3))\n"
    );
}

#[test]
fn lines_continue_after_operators_and_inside_brackets() {
    assert_eq!(
        stmts("total = a +\n    b +\n    c\nresult = f(\n    1,\n    2,\n)"),
        "total = (+ (+ a b) c)\nresult = (call f 1 2)\n"
    );
}

// ---- declarations ---------------------------------------------------------

#[test]
fn functions_with_modifiers_and_results() {
    assert_eq!(
        tree(
            "pub strict create step(mut world: [Entity], dt: f32 = 0.016):\n    pass_through()\nend\n\
             async create fetch(id: u64) -> User fails NotFound: return await db.find(id)\n\
             export \"wasm\" create render(own scene: Scene) -> int:\n    return 0\nend\n\
             kernel create mm[T](a: tensor[T; M, K], b: tensor[T; K, N]) -> tensor[T; M, N]: return a @ b\n"
        ),
        "pub strict create step(mut world: [Entity], dt: f32 = 0.016)\n  (call pass_through)\n\
         async create fetch(id: u64) -> User fails NotFound\n  return (await (call (. db find) id))\n\
         export \"wasm\" create render(own scene: Scene) -> int\n  return 0\n\
         kernel create mm[T](a: tensor[T; M, K], b: tensor[T; K, N]) -> tensor[T; M, N]\n  return (@ a b)\n"
    );
}

#[test]
fn types() {
    assert_eq!(
        tree(
            "type A = {str: [u8]}\n\
             type B = (int, str?)\n\
             type C = (int, int) -> int\n\
             type D = int | str | float\n\
             type E = shared Node[T]?\n\
             type F = ref mut Canvas\n\
             type G = tensor[f16; ..., _, 4 * D] on gpu[0 to 3]\n\
             type H = vec[f32, 3]\n\
             type I = tensor[f32]\n"
        ),
        "type A = {str: [u8]}\n\
         type B = (int, str?)\n\
         type C = (int, int) -> int\n\
         type D = int | str | float\n\
         type E = shared Node[T]?\n\
         type F = ref mut Canvas\n\
         type G = tensor[f16; ..., _, (* 4 D)] on (index gpu (to 0 3))\n\
         type H = vec[f32, 3]\n\
         type I = tensor[f32]\n"
    );
}

#[test]
fn structs_classes_and_enums() {
    assert_eq!(
        tree(
            "struct Entity:\n\
             \x20   pos: vec3\n\
             \x20   health: f32 = 100.0\n\
             \x20   pub name = \"hero\"\n\
             \x20   create damage(mut self, n: f32):\n\
             \x20       self.health -= n\n\
             \x20   end\n\
             end\n\
             object Enemy(Actor):\n    speed = 2\nend\n\
             enum Shape:\n    Circle(radius: f32)\n    Rect(f32, f32)\n    Empty\n    Big = 10\nend\n"
        ),
        "struct Entity\n\
         \x20 pos: vec3\n\
         \x20 health: f32 = 100.0\n\
         \x20 pub name = \"hero\"\n\
         \x20 create damage(mut self, n: f32)\n\
         \x20   (. self health) -= n\n\
         object Enemy(Actor)\n  speed = 2\n\
         enum Shape\n  Circle(radius: f32)\n  Rect(f32, f32)\n  Empty\n  Big = 10\n"
    );
}

#[test]
fn traits_impls_and_externs() {
    assert_eq!(
        tree(
            "trait Drawable(Sized):\n    create draw(self, mut frame: Frame)\n    create size(self) -> int\nend\n\
             impl Drawable for Shape:\n    create draw(self, mut frame: Frame): frame.add(self)\nend\n\
             impl [T] Stack[T]:\n    create push(mut self, own x: T): self.items.append(x)\nend\n\
             impl [Entity]:\n    create alive(self): return self\nend\n\
             extern \"js\":\n    create log(msg: str)\nend\n\
             extern \"c\" create puts(s: ptr u8) -> i32\n"
        ),
        "trait Drawable(Sized)\n  create draw(self, mut frame: Frame)\n  create size(self) -> int\n\
         impl Drawable for Shape\n  create draw(self, mut frame: Frame)\n    (call (. frame add) self)\n\
         impl[T] Stack[T]\n  create push(mut self, own x: T)\n    (call (. (. self items) append) x)\n\
         impl [Entity]\n  create alive(self)\n    return self\n\
         extern \"js\"\n  create log(msg: str)\n\
         extern \"c\"\n  create puts(s: ptr u8) -> i32\n"
    );
}

#[test]
fn routes_and_groups() {
    let module = parse(
        "route GET \"/users/{id}\" (id: u64, db: Db) -> User fails NotFound:\n    return await db.find(id)\nend\n\
         routes \"/admin\":\n    @require(role = \"admin\")\n    route DELETE \"/users/{id}\" (id: u64): return 204\n    routes \"/v2\":\n        route GET \"/health\": return \"ok\"\n    end\nend\n",
    )
    .unwrap();
    let routes = module.routes();
    assert_eq!(routes.len(), 3);
    assert_eq!(routes[0].path.template(), "/users/{id}");
    assert_eq!(
        routes[0]
            .path
            .params()
            .map(|p| p.name.as_str())
            .collect::<Vec<_>>(),
        ["id"]
    );
    assert_eq!(routes[1].method.as_str(), "DELETE");
    assert_eq!(routes[2].path.template(), "/health");
    assert_eq!(
        pretty::module(&module),
        "route GET \"/users/{id}\" (id: u64, db: Db) -> User fails NotFound\n  return (await (call (. db find) id))\n\
         routes \"/admin\"\n  @require(role=\"admin\")\n  route DELETE \"/users/{id}\" (id: u64)\n    return 204\n  routes \"/v2\"\n    route GET \"/health\" ()\n      return \"ok\"\n"
    );
}

#[test]
fn use_const_and_main_with_args() {
    assert_eq!(
        tree(
            "use vyrn.web\nuse vyrn.io as io\nuse app.db (Db, Pool as P)\n\
             pub const MAX: u32 = 10\n\
             main(args):\n    print(args)\nend\n"
        ),
        "use vyrn.web\nuse vyrn.io as io\nuse app.db (Db, Pool as P)\n\
         pub const MAX: u32 = 10\n\
         main(args)\n  (call print args)\n"
    );
}

#[test]
fn every_example_and_fixture_parses() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut count = 0;
    for dir in ["examples", "tests/fixtures"] {
        for entry in std::fs::read_dir(root.join(dir)).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "vyrn") {
                let src = std::fs::read_to_string(&path).unwrap();
                let module = parse(&src).unwrap_or_else(|errors| {
                    let name = path.display().to_string();
                    panic!(
                        "{}",
                        errors
                            .iter()
                            .map(|e| e.render(&name, &src))
                            .collect::<String>()
                    )
                });
                assert!(!module.items.is_empty());
                count += 1;
            }
        }
    }
    assert!(count >= 5);
}

#[test]
fn node_ids_are_unique() {
    let src = std::fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/all_constructs.vyrn"),
    )
    .unwrap();
    let module = parse(&src).unwrap();
    // Every node id appears once in the debug dump.
    let dump = format!("{module:?}");
    let mut ids: Vec<u32> = dump
        .match_indices("NodeId(")
        .map(|(i, _)| dump[i + 7..].split(')').next().unwrap().parse().unwrap())
        .collect();
    let total = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), total);
    assert!(total > 300, "{total}");
}

#[test]
fn spans_point_at_the_source() {
    let src = "main:\n    total = price * 2\nend\n";
    let module = parse(src).unwrap();
    let vyrn::ast::ItemKind::Main(main) = &module.items[0].kind else {
        panic!()
    };
    let stmt = &main.body.stmts[0];
    assert_eq!(stmt.span.text(src), "total = price * 2");
    let vyrn::ast::StmtKind::Assign { value, .. } = &stmt.kind else {
        panic!()
    };
    assert_eq!(value.span.text(src), "price * 2");
    assert_eq!(
        module.items[0].span.text(src),
        "main:\n    total = price * 2\nend"
    );
}

// ---- errors and recovery --------------------------------------------------

#[test]
fn missing_colon_gets_a_hint() {
    let e = one_error("create area(w, h)\n    return w * h\nend\n");
    assert_eq!(
        e.message,
        "expected `:` to end the `create` line, found the end of the line"
    );
    assert!(e.hint.unwrap().contains("ends with `:`"));
    assert_eq!((e.span.start.line, e.span.start.column), (1, 18));
}

#[test]
fn assignment_in_a_condition_gets_a_hint() {
    let e = one_error("main:\n    if x = 1: stop\nend\n");
    assert_eq!(e.hint.as_deref(), Some("use `==` to compare; `=` assigns"));
}

#[test]
fn unclosed_block_points_at_its_header() {
    let e = one_error("main:\n    each x in xs:\n        print(x)\n");
    // The `for` is reported, not `main`, because it is the innermost block.
    assert_eq!(e.message, "this `each` block is never closed");
    assert_eq!((e.span.start.line, e.span.start.column), (2, 5));
}

#[test]
fn recovery_reports_independent_errors_once_each() {
    let src = "\
create a()
    return 1
end

create b():
    let = 2
end

create c():
    return 3
end
";
    let (module, errors) = parse_recovering(src);
    assert_eq!(errors.len(), 2, "{errors:#?}");
    assert_eq!(errors[0].span.start.line, 1);
    assert_eq!(errors[1].span.start.line, 6);
    // `b` and `c` survive: the broken `a` header did not swallow them.
    let printed = pretty::module(&module);
    assert!(printed.contains("create b()"), "{printed}");
    assert!(printed.contains("create c()\n  return 3"), "{printed}");
}

#[test]
fn a_broken_statement_does_not_end_its_block_early() {
    let src = "main:\n    if ready\n        go()\n    end\n    print(\"after\")\nend\n";
    let (module, errors) = parse_recovering(src);
    assert_eq!(errors.len(), 1, "{errors:#?}");
    assert!(pretty::module(&module).contains("(call print \"after\")"));
}

#[test]
fn top_level_statements_point_to_main() {
    let e = one_error("print(\"hi\")\n");
    assert!(e.hint.unwrap().contains("inside `main:`"));
}

#[test]
fn stray_end() {
    let e = one_error("main:\n    x = 1\nend\nend\n");
    assert_eq!(e.message, "`end` has no open block to close");
}

#[test]
fn one_line_if_cannot_take_else() {
    let e = one_error("main:\n    if a: x = 1\n    else: x = 2\nend\n");
    assert_eq!(e.message, "a one-line `if` cannot continue with `else`");
}

#[test]
fn compound_statement_after_a_colon() {
    let e = one_error("main:\n    if a: each x in xs: go()\nend\n");
    assert_eq!(e.message, "`each` cannot follow `:` on the same line");
}

#[test]
fn python_spellings_get_vyrn_hints() {
    let cases = [
        (
            "def area(w):\n    return w\nend\n",
            "`def` is not a Vyrn keyword",
            "create",
        ),
        (
            "fn area(w):\n    return w\nend\n",
            "`fn` is not a Vyrn keyword",
            "create",
        ),
        ("import os\n", "`import` is not a Vyrn keyword", "use"),
        (
            "main:\n    each x in xs:\n        break\n    end\nend\n",
            "`break` is not a Vyrn keyword",
            "stop",
        ),
        (
            "main:\n    each x in xs:\n        continue\n    end\nend\n",
            "`continue` is not a Vyrn keyword",
            "skip",
        ),
        (
            "main:\n    raise ValueError(\"x\")\nend\n",
            "`raise` is not a Vyrn keyword",
            "fail",
        ),
        (
            "main:\n    x = True\nend\n",
            "`True` is written `true` in Vyrn",
            "",
        ),
        (
            "main:\n    when x:\n        case 1: stop\n    end\nend\n",
            "`case` is not a Vyrn keyword",
            "is",
        ),
        (
            "main:\n    x: int = 1\nend\n",
            "a typed variable is declared with `let` or `var`",
            "var",
        ),
    ];
    for (src, message, hint_word) in cases {
        let e = one_error(src);
        assert_eq!(e.message, message, "{src}");
        if !hint_word.is_empty() {
            assert!(
                e.hint.as_deref().unwrap_or("").contains(hint_word),
                "{src}: {:?}",
                e.hint
            );
        }
    }
}

#[test]
fn else_if_chains() {
    let (module, errors) = parse_recovering(
        "main:\n    if a:\n        x = 1\n    else if b:\n        x = 2\n    else:\n        x = 3\n    end\nend\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
    assert!(pretty::module(&module).contains("else if b"));
}

#[test]
fn python_habits_are_explained() {
    let cases = [
        (
            "main:\n    while x < 3:\n        x += 1\n    end\nend\n",
            "while",
            "`loop condition:`",
        ),
        (
            "main:\n    for x in xs:\n        print(x)\n    end\nend\n",
            "for",
            "`each item in items:`",
        ),
        (
            "main:\n    if a:\n        x = 1\n    elif b:\n        x = 2\n    end\nend\n",
            "elif",
            "`else if condition:`",
        ),
        (
            "main:\n    match x:\n        is 1: print(1)\n    end\nend\n",
            "match",
            "`when value:`",
        ),
        (
            "main:\n    try:\n        risky()\n    catch:\n        print(1)\n    end\nend\n",
            "try",
            "`attempt:`",
        ),
        ("class Point:\n    x: int\nend\n", "class", "`object Name:`"),
        (
            "main:\n    with open(p) as f:\n        print(f)\n    end\nend\n",
            "with",
            "`using resource as name:`",
        ),
    ];
    let (_, errors) = parse_recovering("main:\n    print([x * 2 for x in xs])\nend\n");
    assert!(
        errors[0]
            .hint
            .as_deref()
            .is_some_and(|h| h.contains("each x in xs")),
        "{errors:?}"
    );
    for (src, word, hint) in cases {
        let (_, errors) = parse_recovering(src);
        let message = format!("`{word}` is not a Vyrn keyword");
        let found = errors.iter().find(|e| e.message == message);
        assert!(
            found.is_some_and(|e| e.hint.as_deref().is_some_and(|h| h.contains(hint))),
            "{src}\nexpected {message:?} with a hint mentioning {hint}, got {:?}",
            errors
                .iter()
                .map(|e| (&e.message, &e.hint))
                .collect::<Vec<_>>()
        );
    }
    // The same words are fine as names.
    let (_, errors) = parse_recovering(
        "main:\n    match = 1\n    with = [match]\n    class = with[0]\n    print(class)\nend\n",
    );
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn invalid_assignment_targets() {
    assert_eq!(
        one_error("main:\n    f() = 1\nend\n").message,
        "cannot assign to this expression"
    );
    assert!(one_error("main:\n    a, b += 1\nend\n")
        .hint
        .unwrap()
        .contains("single target"));
    assert_eq!(
        one_error("main:\n    a?.b = 1\nend\n").message,
        "cannot assign to this expression"
    );
}

#[test]
fn route_errors() {
    let e = one_error("route GET \"/users/{id}\" (db: Db): return 1\n");
    assert_eq!(e.message, "path parameter `{id}` has no matching parameter");
    assert_eq!(e.span.start.column, 20);
    assert_eq!(
        one_error("route GET \"users\": return 1\n").message,
        "a route path must start with `/`"
    );
    assert_eq!(
        one_error("route get \"/x\": return 1\n").message,
        "HTTP methods are written in capitals: `GET`"
    );
    assert!(one_error("route GET /users: return 1\n")
        .hint
        .unwrap()
        .contains("strings"));
}

#[test]
fn declaration_errors() {
    assert_eq!(
        one_error("struct A(B):\n    x: int\nend\n").message,
        "a struct cannot extend another type"
    );
    assert_eq!(
        one_error("extern \"c\":\n    create f(x: int): return 1\nend\n").message,
        "this function cannot have a body"
    );
    assert_eq!(
        one_error("create main():\n    go()\nend\n").message,
        "`main` is not a function name"
    );
    assert!(one_error("main:\n    a = 1\nend\nmain:\n    b = 2\nend\n")
        .message
        .contains("one `main`"));
    assert_eq!(
        one_error("struct P:\n    x\nend\n").hint.as_deref(),
        Some("a field is written `name: Type` or `name = default`")
    );
}

#[test]
fn errors_inside_interpolation_point_into_the_string() {
    let e = one_error("main:\n    print(\"total: {a +}\")\nend\n");
    assert_eq!(e.span.start.line, 2);
    assert_eq!(e.span.start.column, 23);
}

#[test]
fn lexical_errors_are_not_repeated_as_syntax_errors() {
    let errors = errors("main:\n    x = a && b\nend\n");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].message, "`&&` is not a Vyrn operator");
}

#[test]
fn rendered_parse_error() {
    let src = "main:\n    each x in xs\n        print(x)\n    end\nend\n";
    let rendered = errors(src)[0].render("loop.vyrn", src);
    assert_eq!(
        rendered,
        "\
error: expected `:` to end the `each` line, found the end of the line
 --> loop.vyrn:2:17
  |
2 |     each x in xs
  |                 ^
  = help: a header line ends with `:`, followed by one statement or by a block closed with `end`
"
    );
}
