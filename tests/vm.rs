//! End-to-end tests of the bytecode compiler and virtual machine: each
//! program is compiled, run, and judged by what it prints.

use std::collections::HashMap;

use vyrn::ast::HttpMethod;
use vyrn::vm::{self, compile_source, run_source, Captured, Op, RunError, Value, Vm};

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

/// Runs `body` inside `main:`.
fn main(body: &str) -> String {
    let indented: String = body.lines().map(|l| format!("    {l}\n")).collect();
    run(&format!("main:\n{indented}end\n"))
}

fn runtime_error(src: &str) -> vm::RuntimeError {
    match run_source(src) {
        Err(RunError::Runtime(e)) => e,
        Err(RunError::Compile(errors)) => panic!("compile errors: {errors:?}"),
        Ok(out) => panic!("expected a runtime error, got output {out:?}"),
    }
}

fn compile_errors(src: &str) -> Vec<String> {
    match compile_source(src) {
        Err(errors) => errors.into_iter().map(|e| e.message).collect(),
        Ok(_) => panic!("expected compile errors"),
    }
}

// ---- the instruction set ----------------------------------------------------

#[test]
fn instructions_and_values_stay_small() {
    assert_eq!(std::mem::size_of::<Op>(), 4);
    assert_eq!(std::mem::size_of::<Value>(), 16);
}

#[test]
fn locals_are_operands_without_copies() {
    // `total = total + x` is one `Add` reading and writing registers.
    let program =
        compile_source("main:\n    total = 0\n    x = 5\n    total = total + x\nend\n").unwrap();
    let main = &program.protos[program.main.unwrap()];
    assert_eq!(
        main.code[..3],
        [
            Op::LoadInt { dst: 0, value: 0 },
            Op::LoadInt { dst: 1, value: 5 },
            Op::Add { dst: 0, a: 0, b: 1 },
        ]
    );
}

#[test]
fn counted_loops_cost_one_instruction_per_iteration() {
    let program =
        compile_source("main:\n    total = 0\n    each i in 0 until 10: total += i\nend\n")
            .unwrap();
    let main = &program.protos[program.main.unwrap()];
    let body: Vec<&Op> = main
        .code
        .iter()
        .skip_while(|op| !matches!(op, Op::ForPrep { .. }))
        .skip(1)
        .take_while(|op| !matches!(op, Op::ForLoop { .. }))
        .collect();
    assert_eq!(body, [&Op::Add { dst: 0, a: 0, b: 4 }]);
}

#[test]
fn tensor_operations_have_their_own_instructions() {
    let program = compile_source(
        "main:\n    w = ones(2, 2)\n    y = w @ w\n    z = y on gpu[0]\n    on gpu[1]: print(z)\nend\n",
    )
    .unwrap();
    let code = &program.protos[program.main.unwrap()].code;
    assert!(code.iter().any(|op| matches!(op, Op::MatMul { .. })));
    assert!(code.iter().any(|op| matches!(op, Op::Place { .. })));
    assert!(code.iter().any(|op| matches!(op, Op::PushDevice { .. })));
    assert!(code.iter().any(|op| matches!(op, Op::PopDevice)));
}

// ---- the examples -----------------------------------------------------------

#[test]
fn beginner_example_runs() {
    let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/examples/grade.vyrn"))
        .unwrap();
    assert_eq!(run(&src), "Average: 79.0, grade C\n61 needs another try\n");
}

// ---- language features ------------------------------------------------------

#[test]
fn arithmetic_follows_python() {
    assert_eq!(
        main("print(7 / 2, 7 // 2, -7 // 2, -7 % 3, 2 ** 10, 2 ** -1, 1 + 2.5)"),
        "3.5 3 -4 2 1024 0.5 3.5\n"
    );
    assert_eq!(
        main("print(\"ab\" + \"cd\", \"ab\" * 3, [1] + [2], (1,) + (2,))"),
        "abcd ababab [1, 2] (1, 2)\n"
    );
    assert_eq!(
        main("print(1 < 2 < 3, 3 < 2 < 4, 1 == 1.0, \"a\" < \"b\")"),
        "true false true true\n"
    );
    assert_eq!(
        main("print(5 & 3, 5 | 3, 5 ^ 3, 1 << 4, -16 >> 2, ~0)"),
        "1 7 6 16 -4 -1\n"
    );
}

#[test]
fn logic_short_circuits_and_returns_operands() {
    assert_eq!(
        main("print(0 or \"x\", 1 and 2, none ?? 5, 3 ?? 5, not [])"),
        "x 2 5 3 true\n"
    );
    assert_eq!(
        main("create boom(): fail \"evaluated\"\nprint(true or boom(), false and boom())"),
        "true false\n"
    );
}

#[test]
fn control_flow() {
    assert_eq!(
        main(
            "each n in [3, 8, 15]:\n\
             \x20   if n % 15 == 0:\n\
             \x20       print(\"fizzbuzz\")\n\
             \x20   else if n % 3 == 0:\n\
             \x20       print(\"fizz\")\n\
             \x20   else: print(n)\n\
             \x20   end\n\
             end\n\
             i = 0\n\
             loop i < 10:\n\
             \x20   i += 1\n\
             \x20   if i % 2 == 0: skip\n\
             \x20   if i > 7: stop\n\
             \x20   print(i)\n\
             end\n\
             loop:\n\
             \x20   i -= 1\n\
             \x20   if i < 5: stop\n\
             end\n\
             print(i)"
        ),
        "fizz\n8\nfizzbuzz\n1\n3\n5\n7\n4\n"
    );
}

#[test]
fn ranges_and_counted_loops() {
    assert_eq!(
        main(
            "total = 0\neach i in 1 to 100: total += i\nprint(total)\n\
             print([i each i in 10 until 0 by -3], len(0 until 10 by 3), 5 in 0 to 5, 5 in 0 until 5)\n\
             each _ in 0 until 0: print(\"never\")\n\
             r = 2 to 4\nprint([x * x each x in r], r)"
        ),
        "5050\n[10, 7, 4, 1] 4 true false\n[4, 9, 16] 2 until 5\n"
    );
}

#[test]
fn collections() {
    assert_eq!(
        main(
            "xs = [3, 1, 2]\nxs.append(4)\nxs[0] = 10\n\
             print(xs, xs[-1], xs[1:3], xs[::-1], len(xs), sorted(xs), 2 in xs)\n\
             m = {\"a\": 1}\nm[\"b\"] = 2\nprint(m, m[\"b\"], m.keys(), m.get(\"z\", 0), \"a\" in m)\n\
             t = (1, \"two\", 3.0)\nprint(t, t[1], len(t))\n\
             s = \"Hello\"\nprint(s[0], s[1:], s.upper(), \", \".join([\"a\", \"b\"]), \"ell\" in s)"
        ),
        "[10, 1, 2, 4] 4 [1, 2] [4, 2, 1, 10] 4 [1, 2, 4, 10] true\n\
         {\"a\": 1, \"b\": 2} 2 [\"a\", \"b\"] 0 true\n\
         (1, \"two\", 3.0) two 3\n\
         H ello HELLO a, b true\n"
    );
}

#[test]
fn comprehensions() {
    assert_eq!(
        main(
            "print([x * 2 each x in 0 until 5 if x % 2 == 0])\n\
             print([(a, b) each a in [1, 2] each b in \"xy\"])\n\
             print({k: v * 10 each k, v in [(\"a\", 1), (\"b\", 2)]})"
        ),
        "[0, 4, 8]\n[(1, \"x\"), (1, \"y\"), (2, \"x\"), (2, \"y\")]\n{\"a\": 10, \"b\": 20}\n"
    );
}

#[test]
fn string_interpolation() {
    assert_eq!(
        main("name = \"Ada\"\nscore = 91.5\nprint(\"{name} scored {score} ({score > 90})\")\nprint('{[1, 2]} \\{literal\\}')"),
        "Ada scored 91.5 (true)\n[1, 2] {literal}\n"
    );
}

#[test]
fn functions_defaults_and_keywords() {
    assert_eq!(
        run("create greet(name, greeting = \"Hello\", punct = \"!\"):\n\
             \x20   return \"{greeting}, {name}{punct}\"\n\
             end\n\
             main:\n\
             \x20   print(greet(\"Ada\"))\n\
             \x20   print(greet(\"Ada\", \"Hi\"))\n\
             \x20   print(greet(\"Ada\", punct = \"?\"))\n\
             \x20   print(greet(punct = \".\", name = \"Bo\"))\n\
             \x20   print(softmax(tensor([0.0, 0.0]), axis = -1))\n\
             end\n"),
        "Hello, Ada!\nHi, Ada!\nHello, Ada?\nHello, Bo.\ntensor([0.5, 0.5])\n"
    );
}

#[test]
fn recursion() {
    assert_eq!(
        run("create fib(n):\n    if n < 2: return n\n    return fib(n - 1) + fib(n - 2)\nend\nmain:\n    print(fib(20))\nend\n"),
        "6765\n"
    );
}

#[test]
fn closures_capture_by_value() {
    assert_eq!(
        run("create adder(n):\n    return (x) => x + n\nend\n\
             create twice(f, x):\n    return f(f(x))\nend\n\
             main:\n\
             \x20   add5 = adder(5)\n\
             \x20   print(add5(1), twice(add5, 0), twice((s) => s + \"!\", \"hi\"))\n\
             \x20   base = 100\n\
             \x20   create offset(x): return x + base\n\
             \x20   print(offset(1))\n\
             \x20   print(5 |> add5, [3, 1, 2] |> sorted)\n\
             end\n"),
        "6 10 hi!!\n101\n10 [1, 2, 3]\n"
    );
}

#[test]
fn structs_are_values() {
    assert_eq!(
        run("struct Point:\n    x: int\n    y: int = 0\nend\n\
             main:\n\
             \x20   a = Point(1, 2)\n\
             \x20   b = a\n\
             \x20   b.x = 99\n\
             \x20   print(a, b, a == Point(x = 1, y = 2), Point(5))\n\
             end\n"),
        "Point(x=1, y=2) Point(x=99, y=2) true Point(x=5, y=0)\n"
    );
}

#[test]
fn classes_are_shared_and_inherit() {
    assert_eq!(
        run(
            "object Actor:\n    name: str\n    hp = 10\n    create describe(self): return \"{self.name} ({self.hp})\"\n    create hit(self, n): self.hp -= n\nend\n\
             object Boss(Actor):\n    phase = 1\n    create describe(self): return \"Boss \" + self.name\nend\n\
             main:\n\
             \x20   a = Actor(\"imp\")\n\
             \x20   alias = a\n\
             \x20   alias.hit(3)\n\
             \x20   print(a.describe(), a.hp)\n\
             \x20   b = Boss(\"dragon\", 100)\n\
             \x20   b.hit(1)\n\
             \x20   print(b.describe(), b.hp, b.phase)\n\
             end\n"
        ),
        "imp (7) 7\nBoss dragon 99 1\n"
    );
}

#[test]
fn mut_self_and_mut_arguments_write_back() {
    assert_eq!(
        run(
            "struct Counter:\n    n: int = 0\n    create bump(mut self): self.n += 1\nend\n\
             struct Box:\n    counter: Counter\nend\n\
             create reset(mut c): c.n = 0\n\
             main:\n\
             \x20   c = Counter()\n\
             \x20   c.bump()\n\
             \x20   c.bump()\n\
             \x20   b = Box(Counter())\n\
             \x20   b.counter.bump()\n\
             \x20   items = [Counter(), Counter()]\n\
             \x20   each mut item in items: item.bump()\n\
             \x20   print(c.n, b.counter.n, [i.n each i in items])\n\
             \x20   reset(mut c)\n\
             \x20   print(c.n)\n\
             end\n"
        ),
        "2 1 [1, 1]\n0\n"
    );
}

#[test]
fn for_mut_keeps_the_list_whole_when_the_body_fails_or_leaves() {
    assert_eq!(
        run("struct C:\n    n: int\nend\n\
             create first_big(mut xs):\n\
             \x20   each mut x in xs:\n\
             \x20       x.n += 1\n\
             \x20       if x.n > 2: return x.n\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   xs = [C(1), C(2), C(3)]\n\
             \x20   attempt:\n\
             \x20       each mut x in xs:\n\
             \x20           x.n *= 10\n\
             \x20           if x.n == 20: fail \"stop here\"\n\
             \x20       end\n\
             \x20   catch: print(\"failed\")\n\
             \x20   end\n\
             \x20   print([x.n each x in xs])\n\
             \x20   each mut x in xs:\n\
             \x20       if x.n == 20: stop\n\
             \x20       x.n = 0\n\
             \x20   end\n\
             \x20   print([x.n each x in xs])\n\
             \x20   print(first_big(mut xs), [x.n each x in xs])\n\
             end\n"),
        "failed\n[10, 20, 3]\n[0, 20, 3]\n21 [1, 21, 3]\n"
    );
}

#[test]
fn enums_and_match() {
    assert_eq!(
        run(
            "enum Shape:\n    Circle(radius: float)\n    Rect(float, float)\n    Empty\n\
             \x20   create area(self):\n\
             \x20       when self:\n\
             \x20           is Shape.Circle(r): return 3 * r * r\n\
             \x20           is Shape.Rect(w, h) if w == h: return \"square {w * h}\"\n\
             \x20           is Shape.Rect(w, h): return w * h\n\
             \x20           else: return 0\n\
             \x20       end\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   each s in [Shape.Circle(1.0), Shape.Rect(2.0, 3.0), Shape.Rect(2.0, 2.0), Shape.Empty]:\n\
             \x20       print(s, s.area())\n\
             \x20   end\n\
             \x20   print(Shape.Empty == Shape.Empty, Shape.Circle(radius = 2.0))\n\
             end\n"
        ),
        "Shape.Circle(radius=1.0) 3.0\nShape.Rect(2.0, 3.0) 6.0\nShape.Rect(2.0, 2.0) square 4.0\nShape.Empty 0\ntrue Shape.Circle(radius=2.0)\n"
    );
}

#[test]
fn patterns() {
    assert_eq!(
        main(
            "each v in [0, 7, 42, \"n\", \"south\", (1, 2), [1, 2, 3, 4], none]:\n\
             \x20   when v:\n\
             \x20       is 0: print(\"zero\")\n\
             \x20       is 1 to 9 as d: print(\"digit\", d)\n\
             \x20       is \"n\" or \"north\": print(\"north\")\n\
             \x20       is (a, b): print(\"pair\", a + b)\n\
             \x20       is [first, ...middle, last]: print(\"list\", first, middle, last)\n\
             \x20       is none: print(\"nothing\")\n\
             \x20       is other: print(\"other\", other)\n\
             \x20   end\n\
             end\n\
             if let (x, y) = (3, 4): print(x * y)"
        ),
        "zero\ndigit 7\nother 42\nnorth\nother south\npair 3\nlist 1 [2, 3] 4\nnothing\n12\n"
    );
}

#[test]
fn failures_and_handlers() {
    assert_eq!(
        run("struct NotFound:\n    key: str\nend\n\
             create lookup(m, k):\n    if k not in m: fail NotFound(k)\n    return m[k]\nend\n\
             create risky(log):\n\
             \x20   attempt:\n\
             \x20       return lookup({}, \"x\")\n\
             \x20   always:\n\
             \x20       log.append(\"cleanup\")\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   attempt:\n\
             \x20       print(lookup({\"a\": 1}, \"a\"))\n\
             \x20       print(lookup({\"a\": 1}, \"b\"))\n\
             \x20       print(\"not reached\")\n\
             \x20   catch NotFound as e:\n\
             \x20       print(\"missing\", e.key)\n\
             \x20   end\n\
             \x20   attempt:\n\
             \x20       print(1 // 0)\n\
             \x20   catch IndexError: print(\"wrong\")\n\
             \x20   catch DivisionByZero as e: print(\"caught\", e.kind)\n\
             \x20   end\n\
             \x20   log = []\n\
             \x20   attempt:\n\
             \x20       risky(log)\n\
             \x20   catch: print(\"risky failed\")\n\
             \x20   end\n\
             \x20   print(log)\n\
             end\n"),
        "1\nmissing b\ncaught DivisionByZero\nrisky failed\n[\"cleanup\"]\n"
    );
}

#[test]
fn finally_runs_on_every_exit() {
    assert_eq!(
        run("create f(log):\n\
             \x20   each i in 0 until 3:\n\
             \x20       attempt:\n\
             \x20           if i == 0: skip\n\
             \x20           if i == 2: return \"returned\"\n\
             \x20           log.append(\"body {i}\")\n\
             \x20       always:\n\
             \x20           log.append(\"finally {i}\")\n\
             \x20       end\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   log = []\n\
             \x20   print(f(log))\n\
             \x20   print(log)\n\
             end\n"),
        "returned\n[\"finally 0\", \"body 1\", \"finally 1\", \"finally 2\"]\n"
    );
}

#[test]
fn defer_runs_when_the_block_ends() {
    assert_eq!(
        main(
            "log = []\n\
             attempt:\n\
             \x20   defer log.append(\"closed\")\n\
             \x20   log.append(\"opened\")\n\
             \x20   x = [][0]\n\
             catch: log.append(\"failed\")\n\
             end\n\
             print(log)"
        ),
        "[\"opened\", \"closed\", \"failed\"]\n"
    );
}

#[test]
fn tasks_interleave_while_sleeping() {
    assert_eq!(
        run("async create worker(name, delay):\n\
             \x20   print(name, \"start\")\n\
             \x20   await sleep(delay)\n\
             \x20   print(name, \"done\")\n\
             \x20   return name\n\
             end\n\
             main:\n\
             \x20   slow = worker(\"slow\", 0.03)\n\
             \x20   fast = worker(\"fast\", 0.01)\n\
             \x20   print(\"both started\")\n\
             \x20   print(await join(slow, fast))\n\
             end\n"),
        "slow start\nfast start\nboth started\nfast done\nslow done\n(\"slow\", \"fast\")\n"
    );
}

#[test]
fn channels_feed_async_for() {
    assert_eq!(
        run("async create produce(ch, n):\n\
             \x20   each i in 0 until n:\n\
             \x20       ch.send(i * i)\n\
             \x20       await sleep(0)\n\
             \x20   end\n\
             \x20   ch.close()\n\
             end\n\
             main:\n\
             \x20   ch = channel()\n\
             \x20   producer = produce(ch, 4)\n\
             \x20   total = 0\n\
             \x20   async each x in ch:\n\
             \x20       print(\"got\", x)\n\
             \x20       total += x\n\
             \x20   end\n\
             \x20   await producer\n\
             \x20   print(total)\n\
             end\n"),
        "got 0\ngot 1\ngot 4\ngot 9\n14\n"
    );
}

#[test]
fn async_for_over_a_list() {
    assert_eq!(main("async each x in [1, 2, 3]: print(x)"), "1\n2\n3\n");
}

#[test]
fn awaiting_a_failed_task_raises_its_error() {
    assert_eq!(
        run("struct Bad:\n    why: str\nend\n\
             async create boom():\n\
             \x20   await sleep(0)\n\
             \x20   fail Bad(\"bad input\")\n\
             end\n\
             main:\n\
             \x20   t = boom()\n\
             \x20   attempt:\n\
             \x20       await t\n\
             \x20   catch Bad as e:\n\
             \x20       print(\"caught\", e.why)\n\
             \x20   end\n\
             end\n"),
        "caught bad input\n"
    );
}

#[test]
fn waiting_forever_is_a_deadlock() {
    let e = runtime_error("main:\n    ch = channel()\n    x = await ch\nend\n");
    assert_eq!(e.kind, "Deadlock");
}

#[test]
fn unawaited_failures_are_reported() {
    let _ = vyrn::vm::task::take_unobserved();
    run("async create boom(): fail \"lost\"\nmain:\n    t = boom()\nend\n");
    let warnings = vyrn::vm::task::take_unobserved();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(warnings[0].contains("boom"), "{warnings:?}");
    assert!(warnings[0].contains("lost"), "{warnings:?}");
}

#[test]
fn small_integer_operands_behave_like_any_other() {
    assert_eq!(
        main("x = 2.5\nn = 7\nprint(x + 1, x - 1, x < 3, x >= 3, x == 2, n != 7, n - -3)\nn += 2\nn -= 1\nprint(n, [1] == 1, none != 0)"),
        "3.5 1.5 true false false false 10\n8 false true\n"
    );
    let big = "big = 9223372036854775807\n";
    let with_literal = runtime_error(&format!("main:\n    {big}    print(big + 1)\nend\n"));
    let with_variable = runtime_error(&format!(
        "main:\n    {big}    one = 1\n    print(big + one)\nend\n"
    ));
    assert_eq!(with_literal.kind, with_variable.kind);
    assert_eq!(with_literal.message, with_variable.message);
    let e = runtime_error("main:\n    print(\"a\" < 1)\nend\n");
    assert_eq!(e.kind, "TypeError");
}

#[test]
fn spawn_and_await() {
    assert_eq!(
        run("async create fetch(id): return id * 10\n\
             main:\n\
             \x20   t = spawn fetch(4)\n\
             \x20   a, b = await join(fetch(1), t)\n\
             \x20   print(a, b, await fetch(2))\n\
             end\n"),
        "10 40 20\n"
    );
}

#[test]
fn constants_and_globals() {
    assert_eq!(
        run("const SIZE = 4\nconst AREA = SIZE * SIZE\ncreate area(): return AREA\nmain:\n    print(SIZE, area())\nend\n"),
        "4 16\n"
    );
}

// ---- tensors and devices ----------------------------------------------------

#[test]
fn tensor_math() {
    assert_eq!(
        main(
            "a = tensor([[1, 2], [3, 4]])\n\
             b = eye(2)\n\
             print(a @ b)\n\
             print(a * a, a + 1, a - tensor([10, 20]))\n\
             print(a.T, a[:, -1], a[1], a[0, 1], a.shape)\n\
             print(a.sum(), a.sum(0), a.mean(), argmax(tensor([0.1, 0.7, 0.2])))\n\
             print(gelu(tensor([0.0])), relu(tensor([-1.0, 2.0])), a > 2)\n\
             x = zeros(2, 3, 4)\n\
             print(x[..., 0].shape, x[none].shape, (x @ ones(4, 5)).shape)"
        ),
        "tensor([[1.0, 2.0], [3.0, 4.0]])\n\
         tensor([[1.0, 4.0], [9.0, 16.0]]) tensor([[2.0, 3.0], [4.0, 5.0]]) tensor([[-9.0, -18.0], [-7.0, -16.0]])\n\
         tensor([[1.0, 3.0], [2.0, 4.0]]) tensor([2.0, 4.0]) tensor([3.0, 4.0]) 2.0 (2, 2)\n\
         10.0 tensor([4.0, 6.0]) 2.5 1\n\
         tensor([0.0]) tensor([0.0, 2.0]) tensor([[0.0, 0.0], [1.0, 1.0]])\n\
         (2, 3) (1, 2, 3, 4) (2, 3, 5)\n"
    );
}

#[test]
fn vectors_for_game_math() {
    assert_eq!(
        main(
            "v = vec3(3.0, 4.0, 0.0)\n\
             print(v.x, v.y, v.length())\n\
             v.y = 0.5\n\
             v += vec3(1.0, 1.0, 1.0) * 2\n\
             print(v)"
        ),
        "3.0 4.0 5.0\ntensor([5.0, 2.5, 2.0])\n"
    );
}

#[test]
fn device_placement() {
    assert_eq!(
        main(
            "w = ones(2, 2)\n\
             print(w.device, (w on gpu[1]).device)\n\
             on gpu[0]:\n\
             \x20   h = ones(2, 2)\n\
             \x20   print(h.device, (h @ (w on gpu[0])).device)\n\
             end\n\
             print(zeros(1).device)\n\
             layers = [ones(1), ones(1), ones(1), ones(1)] on gpu[0 to 1]\n\
             print([l.device each l in layers])"
        ),
        "cpu gpu:1\ngpu:0 gpu:0\ncpu\n[gpu:0, gpu:0, gpu:1, gpu:1]\n"
    );
}

#[test]
fn mixing_devices_is_an_error() {
    let e = runtime_error(
        "main:\n    a = ones(2, 2)\n    b = ones(2, 2) on gpu[0]\n    c = a @ b\nend\n",
    );
    assert_eq!(e.kind, "DeviceError");
    assert!(e.message.contains("cpu") && e.message.contains("gpu:0"));
    assert_eq!(e.trace[0].line, 4);
}

#[test]
fn shape_errors_are_reported() {
    let e = runtime_error("main:\n    x = ones(2, 3) @ ones(2, 3)\nend\n");
    assert_eq!(e.kind, "ShapeError");
    assert!(e.message.contains("inner dimensions 3 and 2"));
}

#[test]
fn a_gpu_backend_receives_the_work() {
    use std::cell::Cell;
    use std::rc::Rc;
    use vyrn::vm::tensor::{Device, ElemOp, MapOp, Reduce, ReferenceBackend, Tensor, TensorError};

    /// Counts the matrix products it is asked to run.
    struct Counting(Rc<Cell<usize>>);
    impl vm::TensorBackend for Counting {
        fn name(&self) -> &str {
            "counting"
        }
        fn matmul(&self, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError> {
            self.0.set(self.0.get() + 1);
            ReferenceBackend.matmul(a, b)
        }
        fn elementwise(&self, op: ElemOp, a: &Tensor, b: &Tensor) -> Result<Tensor, TensorError> {
            ReferenceBackend.elementwise(op, a, b)
        }
        fn map(&self, op: MapOp, a: &Tensor) -> Tensor {
            ReferenceBackend.map(op, a)
        }
        fn reduce(&self, op: Reduce, a: &Tensor, axis: Option<i64>) -> Result<Tensor, TensorError> {
            ReferenceBackend.reduce(op, a, axis)
        }
        fn softmax(&self, a: &Tensor, axis: i64) -> Result<Tensor, TensorError> {
            ReferenceBackend.softmax(a, axis)
        }
        fn transfer(&self, a: &Tensor, to: Device) -> Result<Tensor, TensorError> {
            ReferenceBackend.transfer(a, to)
        }
    }

    let program = compile_source(
        "main:\n    a = ones(2, 2)\n    b = a @ a\n    on gpu[0]:\n        c = ones(2, 2)\n        d = c @ c @ c\n    end\nend\n",
    )
    .unwrap();
    let calls = Rc::new(Cell::new(0));
    let mut machine = Vm::with_output(program, Box::new(Captured::default()));
    machine.set_gpu_backend(Box::new(Counting(calls.clone())));
    machine.run_main(&[]).unwrap();
    // Two products ran on the GPU; the CPU product did not reach it.
    assert_eq!(calls.get(), 2);
}

#[test]
fn small_transformer_block_runs_on_gpus() {
    // A miniature version of examples/inference.vyrn with real weights.
    let out = run(
        "struct Layer:\n\
         \x20   w_up: tensor[f16; D, H]\n\
         \x20   w_down: tensor[f16; H, D]\n\
         end\n\
         kernel create feed_forward(x: tensor[f32], w: Layer):\n\
         \x20   let h = gelu(x @ w.w_up)\n\
         \x20   return x + h @ w.w_down\n\
         end\n\
         main:\n\
         \x20   layers = [Layer(rand(4, 8) * 0.1, rand(8, 4) * 0.1) each _ in 0 until 4] on gpu[0 to 1]\n\
         \x20   var h = rand(2, 3, 4)\n\
         \x20   each layer in layers:\n\
         \x20       on layer.w_up.device:\n\
         \x20           h = feed_forward(h on layer.w_up.device, layer)\n\
         \x20       end\n\
         \x20   end\n\
         \x20   logits = (h[:, -1] on cpu) @ rand(4, 10)\n\
         \x20   print(h.shape, h.device, logits.shape)\n\
         \x20   probs = softmax(logits)\n\
         \x20   print(probs.shape, round(probs.sum(), 4), argmax(probs, axis = -1).shape)\n\
         end\n",
    );
    assert_eq!(out, "(2, 3, 4) gpu:1 (2, 10)\n(2, 10) 2.0 (2,)\n");
}

// ---- errors -----------------------------------------------------------------

#[test]
fn runtime_errors_carry_a_trace() {
    let e = runtime_error("create inner(xs):\n    return xs[10]\nend\ncreate outer():\n    return inner([1, 2])\nend\nmain:\n    outer()\nend\n");
    assert_eq!(e.kind, "IndexError");
    assert_eq!(e.message, "index 10 is out of range for a list of length 2");
    let trace: Vec<(String, u32)> = e
        .trace
        .iter()
        .map(|t| (t.function.clone(), t.line))
        .collect();
    assert_eq!(
        trace,
        [
            ("inner".to_string(), 2),
            ("outer".to_string(), 5),
            ("main".to_string(), 8)
        ]
    );
    assert_eq!(e.to_string(), "IndexError: index 10 is out of range for a list of length 2\n  in inner at line 2\n  in outer at line 5\n  in main at line 8");
}

#[test]
fn a_failure_passed_on_by_catch_keeps_its_line() {
    let e = runtime_error(
        "create pick(xs):\n    return xs[5]\nend\nmain:\n    attempt:\n        pick([1])\n    catch DivisionByZero:\n        print(\"no\")\n    end\nend\n",
    );
    assert_eq!(e.kind, "IndexError");
    let lines: Vec<u32> = e.trace.iter().map(|t| t.line).collect();
    assert_eq!(lines, [2, 6]);
}

#[test]
fn catch_error_catches_every_failure() {
    assert_eq!(
        run(
            "struct Oops:\n    why: str\nend\nmain:\n    attempt:\n        fail Oops(\"x\")\n    catch Error as e:\n        print(\"caught\", e)\n    end\n    attempt:\n        print(1 / 0)\n    catch Error as e:\n        print(\"caught\", e)\n    end\nend\n"
        ),
        "caught Oops(why=\"x\")\ncaught DivisionByZero: division by zero\n"
    );
}

#[test]
fn uncaught_user_failures_report_their_type() {
    let e = runtime_error(
        "struct Oops:\n    why: str\nend\nmain:\n    fail Oops(\"bad input\")\nend\n",
    );
    assert_eq!(e.kind, "Oops");
    assert_eq!(e.message, "Oops(why=\"bad input\")");
}

#[test]
fn runtime_type_errors() {
    assert_eq!(
        runtime_error("create add(a, b): return a + b\nmain:\n    x = add(1, \"a\")\nend\n")
            .message,
        "cannot apply `+` to int and str"
    );
    assert_eq!(
        runtime_error("main:\n    x = 9223372036854775807 + 1\nend\n").kind,
        "Overflow"
    );
    assert_eq!(
        runtime_error("main:\n    x = {\"a\": 1}[\"b\"]\nend\n").kind,
        "KeyError"
    );
    // Known functions are checked at compile time; a call through a variable
    // is checked when it runs.
    assert_eq!(
        runtime_error("create f(a): return a\nmain:\n    g = f\n    g()\nend\n").message,
        "`f` takes 1 argument, got 0"
    );
    assert_eq!(
        runtime_error("main:\n    x = 5\n    x.go()\nend\n").message,
        "int has no method `go`; numbers have abs, sqrt, sin, cos, tan, floor, ceil, round, clamp, min, max, pow, sign, radians, degrees, is_even, is_odd"
    );
}

#[test]
fn deep_recursion_is_caught() {
    let e = runtime_error("create down(n): return down(n + 1)\nmain:\n    down(0)\nend\n");
    assert_eq!(e.kind, "StackOverflow");
}

#[test]
fn compile_time_errors() {
    assert_eq!(
        compile_errors("main:\n    print(totl)\n    total = 1\nend\n"),
        ["`totl` is not defined"]
    );
    assert_eq!(
        compile_errors("create f(a): return a\nmain:\n    f()\nend\n"),
        ["`f` is missing the argument `a`"]
    );
    assert_eq!(
        compile_errors("main:\n    stop\nend\n"),
        ["`stop` can only be used inside a loop"]
    );
    assert_eq!(
        compile_errors("create f(a, b = 1): return a\nmain:\n    f(c = 1)\nend\n"),
        ["`f` has no parameter `c`"]
    );
    assert_eq!(
        compile_errors("create f(a, b): return a\nmain:\n    f(b = 1)\nend\n"),
        ["`f` is missing the argument `a`"]
    );
    assert_eq!(
        compile_errors("const N = 1\nmain:\n    N = 2\nend\n"),
        ["cannot assign to `N`: it is a top-level constant or declaration"]
    );
}

#[test]
fn unknown_names_suggest_close_matches() {
    let errors = compile_source("main:\n    total = 1\n    print(totl)\nend\n").unwrap_err();
    assert_eq!(errors[0].hint.as_deref(), Some("did you mean `total`?"));
}

// ---- routes -----------------------------------------------------------------

#[test]
fn routes_dispatch_and_return_json() {
    let program = compile_source(
        "struct User:\n\
         \x20   id: u64\n\
         \x20   name: str\n\
         \x20   email: str?\n\
         end\n\
         struct NotFound:\n    id: u64\nend\n\
         route GET \"/users/{id}\" (id: u64, db) -> User fails NotFound:\n\
         \x20   if id not in db: fail NotFound(id)\n\
         \x20   return db[id]\n\
         end\n\
         routes \"/admin\":\n\
         \x20   route GET \"/stats\" (db):\n\
         \x20       return {\"users\": len(db), \"ok\": true, \"ids\": sorted(db.keys())}\n\
         \x20   end\n\
         end\n\
         create seed():\n\
         \x20   return {1: User(1, \"Ada\", \"ada@example.com\"), 2: User(2, \"Bo\", none)}\n\
         end\n",
    )
    .unwrap();
    let mut machine = Vm::with_output(program, Box::new(Captured::default()));
    let db = machine.call_global("seed", vec![]).unwrap();
    let providers = HashMap::from([("db".to_string(), db)]);

    let user = machine
        .call_route(HttpMethod::Get, "/users/2", &providers)
        .unwrap()
        .unwrap();
    assert_eq!(
        machine.to_json(&user),
        r#"{"id":2,"name":"Bo","email":null}"#
    );

    let stats = machine
        .call_route(HttpMethod::Get, "/admin/stats", &providers)
        .unwrap()
        .unwrap();
    assert_eq!(
        machine.to_json(&stats),
        r#"{"users":2,"ok":true,"ids":[1,2]}"#
    );

    let missing = machine
        .call_route(HttpMethod::Get, "/users/9", &providers)
        .unwrap_err();
    assert_eq!(missing.kind, "NotFound");
    assert_eq!(machine.to_json(&missing.value), r#"{"id":9}"#);

    assert!(machine
        .call_route(HttpMethod::Post, "/users/2", &providers)
        .unwrap()
        .is_none());
    assert!(machine
        .call_route(HttpMethod::Get, "/nowhere", &providers)
        .unwrap()
        .is_none());
    assert_eq!(
        machine
            .call_route(HttpMethod::Get, "/users/abc", &providers)
            .unwrap_err()
            .kind,
        "BadRequest"
    );
}

#[test]
fn main_receives_command_line_arguments() {
    let program = compile_source("main(args):\n    print(len(args), args[0])\nend\n").unwrap();
    let out = Captured::default();
    let mut machine = Vm::with_output(program, Box::new(out.clone()));
    machine
        .run_main(&["hello".to_string(), "x".to_string()])
        .unwrap();
    assert_eq!(out.text(), "2 hello\n");
}

#[test]
fn hosts_can_finish_background_tasks() {
    let program = compile_source(
        "async create later(msg):\n    await sleep(0.01)\n    print(msg)\nend\n\
         create start(): later(\"background done\")\n",
    )
    .unwrap();
    let out = Captured::default();
    let mut machine = Vm::with_output(program, Box::new(out.clone()));
    machine.call_global("start", Vec::new()).unwrap();
    assert_eq!(out.text(), "");
    machine.run_pending().unwrap();
    assert_eq!(out.text(), "background done\n");
}

// ---- element types ----------------------------------------------------------

#[test]
fn tensors_store_lower_precision_weights() {
    assert_eq!(
        main(
            "w = ones(64, 64)\n\
             each dtype in [f32, f16, bf16, q8, q4]:\n\
             \x20   packed = w.to(dtype)\n\
             \x20   print(packed.dtype, packed.nbytes, (ones(1, 64) @ packed).sum())\n\
             end\n\
             print(tensor([0.5, 1.5], f16), tensor([1, 2]).dtype)\n\
             print(memory([w, w.to(q4), \"not a tensor\"]))"
        ),
        "f32 16384 4096.0\n\
         f16 8192 4096.0\n\
         bf16 8192 4096.0\n\
         q8 4352 4095.75\n\
         q4 2304 4096.0\n\
         tensor([0.5, 1.5], dtype=f16) f32\n\
         18688\n"
    );
}

#[test]
fn quantized_tensors_are_read_only() {
    let e = runtime_error("main:\n    v = vec3(1.0, 2.0, 3.0).to(q8)\n    v.x = 5.0\nend\n");
    assert_eq!(e.kind, "ShapeError");
    assert!(e.message.contains(".to(f32)"));
    assert_eq!(
        runtime_error("main:\n    x = ones(2).to(\"f8\")\nend\n").message,
        "unknown element type `f8`; use f32, f16, bf16, q8 or q4"
    );
}

// ---- model files, tokenizers, files and JSON --------------------------------

fn fixture(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

#[test]
fn loads_safetensors_written_by_the_reference_library() {
    assert_eq!(
        main(&format!(
            "w = load_weights(\"{}\")\n\
             each name in sorted(w.keys()): print(name, w[name].shape, w[name].dtype, w[name])\n\
             print(load_weights(\"{}\", q4)[\"layer.0.weight\"].dtype)",
            fixture("weights.safetensors"),
            fixture("weights.safetensors")
        )),
        "layer.0.bias (4,) f16 tensor([0.5, -1.25, 3.0, 65504.0], dtype=f16)\n\
         layer.0.weight (2, 3) f32 tensor([[0.0, 0.25, 0.5], [0.75, 1.0, 1.25]])\n\
         positions (2,) f32 tensor([7.0, -9.0])\n\
         scale () f32 tensor(2.5)\n\
         q4\n"
    );
}

#[test]
fn weights_round_trip_through_a_file() {
    let path = std::env::temp_dir().join(format!("vyrn-test-{}.safetensors", std::process::id()));
    let path = path.to_string_lossy();
    assert_eq!(
        main(&format!(
            "save_weights(\"{path}\", {{\"w\": tensor([[1.5, -2.0]]), \"h\": tensor([0.1, 7.0], f16), \"b\": tensor([3.0], bf16)}})\n\
             back = load_weights(\"{path}\")\n\
             print(back[\"w\"], back[\"h\"], back[\"b\"])\n\
             attempt:\n\
             \x20   save_weights(\"{path}\", {{\"q\": ones(32).to(q4)}})\n\
             catch IoError as e: print(\"refused\")\n\
             end"
        )),
        "tensor([[1.5, -2.0]]) tensor([0.099975586, 7.0], dtype=f16) tensor([3.0], dtype=bf16)\nrefused\n"
    );
    let _ = std::fs::remove_file(&*path);
}

#[test]
fn tokenizers_encode_and_decode() {
    assert_eq!(
        main(&format!(
            "tok = load_tokenizer(\"{}\")\n\
             ids = tok.encode(\"Hello world<|endoftext|>\")\n\
             print(ids, tok.decode(ids), tok.token_id(\"<|endoftext|>\"), tok.vocab_size)",
            fixture("tokenizers/gpt2.json")
        )),
        "[40, 69, 290, 79, 521, 0] Hello world<|endoftext|> 0 539\n"
    );
}

#[test]
fn files_and_json() {
    let path = std::env::temp_dir().join(format!("vyrn-test-{}.json", std::process::id()));
    let path = path.to_string_lossy();
    assert_eq!(
        main(&format!(
            "data = {{\"name\": \"Ada\", \"scores\": [90, 85.5], \"ok\": true, \"none\": none}}\n\
             write_text(\"{path}\", to_json(data))\n\
             back = parse_json(read_text(\"{path}\"))\n\
             print(back, back == data)"
        )),
        "{\"name\": \"Ada\", \"scores\": [90, 85.5], \"ok\": true, \"none\": none} true\n"
    );
    let _ = std::fs::remove_file(&*path);
    assert_eq!(
        runtime_error("main:\n    read_text(\"/no/such/file\")\nend\n").kind,
        "IoError"
    );
}

// ---- device memory ----------------------------------------------------------

#[test]
fn device_memory_is_accounted_and_limited() {
    assert_eq!(
        run("struct Layer:\n    w: tensor[f32; D, D]\nend\n\
             main:\n\
             \x20   layers = [Layer(rand(256, 256)) each _ in 0 until 8]\n\
             \x20   small = [Layer(l.w.to(q4)) each l in layers]\n\
             \x20   print(format_bytes(memory(layers)), format_bytes(memory(small)))\n\
             \x20   set_memory_limit(gpu[0], 1 * MB)\n\
             \x20   attempt:\n\
             \x20       big = layers on gpu[0]\n\
             \x20   catch OutOfMemory as e: print(\"refused\")\n\
             \x20   end\n\
             \x20   print(memory_used(gpu[0]))\n\
             \x20   placed = fit(layers, gpu[0])\n\
             \x20   print([l.w.device each l in placed], format_bytes(memory_used(gpu[0])))\n\
             \x20   placed = none\n\
             \x20   print(memory_used(gpu[0]))\n\
             \x20   resident = small on gpu[0]\n\
             \x20   print(format_bytes(memory_used(gpu[0])))\n\
             \x20   halves = fit(layers, gpu[1], 3 * 256 * KB)\n\
             \x20   print(len([l each l in halves if l.w.device == gpu[1]]))\n\
             end\n"),
        "2.0 MB 288.0 KB\n\
         refused\n\
         0\n\
         [gpu:0, gpu:0, gpu:0, gpu:0, cpu, cpu, cpu, cpu] 1.0 MB\n\
         0\n\
         288.0 KB\n\
         3\n"
    );
}

#[test]
fn fit_needs_a_budget_or_a_limit() {
    assert!(
        runtime_error("main:\n    x = fit([ones(2)], gpu[3])\nend\n")
            .message
            .contains("set_memory_limit")
    );
}

// ---- keyword arguments, closures and with -------------------------------------

#[test]
fn keyword_arguments_reach_closures_methods_and_builtins() {
    assert_eq!(
        run(
            "struct Box:\n    w: float\n    create scaled(self, factor = 2.0, offset = 0.0): return self.w * factor + offset\nend\n\
             create make(): return (x, scale = 1, bias = 0) => x * scale + bias\n\
             main:\n\
             \x20   f = make()\n\
             \x20   print(f(3), f(3, bias = 1), f(x = 2, scale = 10), f(1, 2, bias = 5))\n\
             \x20   b = Box(1.5)\n\
             \x20   print(b.scaled(), b.scaled(offset = 1.0), b.scaled(factor = 4.0, offset = 0.5))\n\
             \x20   g = softmax\n\
             \x20   print(g(tensor([0.0, 0.0]), axis = -1))\n\
             end\n"
        ),
        "3 4 20 7\n3.0 4.0 6.5\ntensor([0.5, 0.5])\n"
    );
    assert_eq!(
        runtime_error("main:\n    f = (x) => x\n    f(1, size = 2)\nend\n").message,
        "`<lambda>` has no parameter `size`; its parameters are: x"
    );
    assert_eq!(
        runtime_error("main:\n    f = (x, y) => x\n    f(y = 2)\nend\n").message,
        "`<lambda>` is missing the argument `x`"
    );
}

#[test]
fn closures_share_the_variables_they_capture() {
    assert_eq!(
        run("create counter():\n\
             \x20   count = 0\n\
             \x20   create next():\n\
             \x20       count += 1\n\
             \x20       return count\n\
             \x20   end\n\
             \x20   return next\n\
             end\n\
             create outer(mut xs):\n\
             \x20   add = (x) => xs.append(x)\n\
             \x20   add(1)\n\
             \x20   xs = xs + [99]\n\
             \x20   return len(xs)\n\
             end\n\
             main:\n\
             \x20   c = counter()\n\
             \x20   c()\n\
             \x20   c()\n\
             \x20   print(c(), counter()())\n\
             \x20   adders = []\n\
             \x20   each i in 0 until 3: adders.append((x) => x + i)\n\
             \x20   print([f(10) each f in adders])\n\
             \x20   create fact(n):\n\
             \x20       if n <= 1: return 1\n\
             \x20       return n * fact(n - 1)\n\
             \x20   end\n\
             \x20   print(fact(10))\n\
             \x20   seen = []\n\
             \x20   log = (m) => seen.append(m)\n\
             \x20   log(\"a\")\n\
             \x20   seen = seen + [\"b\"]\n\
             \x20   log(\"c\")\n\
             \x20   print(seen)\n\
             \x20   ys = [0]\n\
             \x20   print(outer(mut ys), ys)\n\
             end\n"),
        "3 1\n[10, 11, 12]\n3628800\n[\"a\", \"b\", \"c\"]\n3 [0, 1, 99]\n"
    );
}

#[test]
fn with_closes_resources_in_reverse_order() {
    assert_eq!(
        run(
            "object Resource:\n    name: str\n    log: [str]\n    create close(self): self.log.append(\"closed \" + self.name)\nend\n\
             create use_two(log):\n\
             \x20   using Resource(\"a\", log) as a, Resource(\"b\", log) as b:\n\
             \x20       log.append(\"using {a.name} and {b.name}\")\n\
             \x20       return \"done\"\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   log = []\n\
             \x20   print(use_two(log))\n\
             \x20   attempt:\n\
             \x20       using Resource(\"c\", log): fail \"boom\"\n\
             \x20   catch: log.append(\"caught\")\n\
             \x20   end\n\
             \x20   print(log)\n\
             end\n"
        ),
        "done\n[\"using a and b\", \"closed b\", \"closed a\", \"closed c\", \"caught\"]\n"
    );
}

// ---- kernels ----------------------------------------------------------------

#[test]
fn kernels_run_on_numbers_and_on_tensors() {
    assert_eq!(
        run("const SCALE = 2\n\
             kernel create leaky(x: f32) -> f32:\n    return x if x > 0.0 else 0.01 * x\nend\n\
             kernel create mix(a: f32, b: f32, t: f32) -> f32:\n    let d = b - a\n    return a + d * t * SCALE\nend\n\
             main:\n\
             \x20   print(leaky(-2.0), leaky(3.0))\n\
             \x20   let x = arange(4) - 2.0\n\
             \x20   print(leaky(x))\n\
             \x20   print(mix(x, ones(4), 0.25))\n\
             end\n"),
        "-0.02 3.0\ntensor([-0.02, -0.01, 0.0, 1.0])\ntensor([-0.5, 0.0, 0.5, 1.0])\n"
    );
}

#[test]
fn kernels_that_take_tensors_work_on_whole_tensors() {
    assert_eq!(
        run("kernel create scale_rows(x: tensor[f32], k: f32) -> tensor[f32]: return x * k\nmain: print(scale_rows(ones(2), 3.0))\n"),
        "tensor([3.0, 3.0])\n"
    );
}

#[test]
fn kernels_explain_what_cannot_run_on_tensors() {
    let e = runtime_error(
        "kernel create noisy(x: f32) -> f32:\n    print(x)\n    return x\nend\nmain:\n    print(noisy(2.0))\n    noisy(ones(3))\nend\n",
    );
    assert_eq!(e.kind, "KernelError");
    assert!(
        e.message
            .contains("is not `let` bindings followed by one `return`"),
        "{}",
        e.message
    );
    let e = runtime_error(
        "kernel create add(a: f32, b: f32) -> f32: return a + b\nmain: add(ones(2), ones(3))\n",
    );
    assert_eq!(e.kind, "ShapeError");
    // Known values are checked before running; others when the call runs.
    assert!(compile_source(
        "kernel create add(a: f32, b: f32) -> f32: return a + b\nmain: add(ones(2), \"x\")\n"
    )
    .is_err());
    let e = runtime_error(
        "kernel create add(a: f32, b: f32) -> f32: return a + b\ncreate text(): return \"x\"\nmain: add(ones(2), text())\n",
    );
    assert_eq!(e.kind, "TypeError");
}

// ---- decorators, variadics, spread, bound methods ---------------------------

#[test]
fn decorators_wrap_functions() {
    assert_eq!(
        run("create twice(f):\n    return x => f(f(x))\nend\n\
             create tagged(label):\n    create wrap(f):\n        create inner(args...):\n            return \"{label}:{f(args...)}\"\n        end\n        return inner\n    end\n    return wrap\nend\n\
             @twice\ncreate inc(x): return x + 1\n\
             @tagged(\"sum\")\ncreate add(a, b): return a + b\n\
             main:\n    print(inc(1), add(2, 3))\nend\n"),
        "3 sum:5\n"
    );
}

#[test]
fn rest_parameters_and_spread_arguments() {
    assert_eq!(
        run("create total(label, nums...):\n    return \"{label} {nums} {sum(nums)}\"\nend\n\
             main:\n    xs = [4, 5]\n    print(total(\"a\"))\n    print(total(\"b\", 1, 2))\n    print(total(\"c\", xs..., 6))\n    print(max(xs...))\nend\n"),
        "a [] 0\nb [1, 2] 3\nc [4, 5, 6] 15\n5\n"
    );
}

#[test]
fn methods_taken_as_values_remember_their_object() {
    assert_eq!(
        run("object Counter:\n    n: int = 0\n    create add(self, k): self.n += k\nend\n\
             main:\n    c = Counter()\n    f = c.add\n    f(2)\n    [3, 4].each(c.add)\n    print(c.n)\nend\n"),
        "9\n"
    );
}

#[test]
fn types_define_their_own_operators_and_text() {
    assert_eq!(
        run("struct V:\n    x: int\n    y: int\n\
             \x20   create operator +(self, o: V) -> V: return V(self.x + o.x, self.y + o.y)\n\
             \x20   create operator *(self, k: int) -> V: return V(self.x * k, self.y * k)\n\
             \x20   create operator -(self) -> V: return V(-self.x, -self.y)\n\
             \x20   create operator <(self, o: V) -> bool: return self.x < o.x\n\
             \x20   create operator [](self, i: int) -> int: return self.x if i == 0 else self.y\n\
             \x20   create to_text(self) -> str: return \"<{self.x},{self.y}>\"\n\
             end\n\
             main:\n    a = V(1, 2)\n    print(a + V(3, 4), a * 3, 2 * a, -a, a[1])\n    print(\"{a}\", [V(5, 0), a], sorted([V(5, 0), a]), max([V(5, 0), a]), V(0, 9) < a)\nend\n"),
        "<4,6> <3,6> <2,4> <-1,-2> 2\n<1,2> [<5,0>, <1,2>] [<1,2>, <5,0>] <5,0> true\n"
    );
}

#[test]
fn generators_make_values_as_they_are_asked_for() {
    assert_eq!(
        run("create naturals():\n    var n = 0\n    loop:\n        n += 1\n        yield n\n    end\nend\n\
             create take(source, k):\n    var got = 0\n    each x in source:\n        if got == k: return\n        yield x\n        got += 1\n    end\nend\n\
             create noisy():\n    print(\"start\")\n    yield 1\n    print(\"after 1\")\n    yield 2\nend\n\
             main:\n    print(list(take(naturals(), 3)), sum(take(naturals(), 10)))\n    g = noisy()\n    print(\"made\")\n    print(g.next())\n    print(g.next(), g.next())\n    print(\"-\".join(take(naturals(), 3).map(str)))\nend\n"),
        "[1, 2, 3] 55\nmade\nstart\n1\nafter 1\n2 none\n1-2-3\n"
    );
}

#[test]
fn builtin_error_kinds_can_be_raised_and_caught() {
    assert_eq!(
        run("create age(n):\n    if n < 0: fail ValueError(\"age cannot be negative\")\n    return n\nend\n\
             main:\n    attempt:\n        age(-1)\n    catch ValueError as e:\n        print(e.message)\n    end\nend\n"),
        "age cannot be negative\n"
    );
}

#[test]
fn is_tests_a_values_type() {
    assert_eq!(
        run("object Animal:\n    name: str\nend\nobject Dog(Animal):\nend\n\
             main:\n    d = Dog(\"Rex\")\n    print(d is Dog, d is Animal, Animal(\"x\") is Dog, d is not Dog)\n    print(5 is int, \"s\" is str, none is none, d is none, d is not none)\nend\n"),
        "true true false false\ntrue true true false true\n"
    );
}

#[test]
fn traits_give_default_methods() {
    assert_eq!(
        run("trait Shape:\n    create area(self) -> float\n    create describe(self) -> str:\n        return \"area {self.area()}\"\n    end\nend\n\
             struct Square:\n    side: float\nend\n\
             impl Shape for Square:\n    create area(self) -> float: return self.side * self.side\nend\n\
             main:\n    print(Square(3.0).describe())\nend\n"),
        "area 9.0\n"
    );
}

#[test]
fn private_members_belong_to_their_type() {
    let src = "object Account:\n    private balance: float = 0.0\n\
               \x20   create deposit(self, x: float): self.balance += x\n\
               \x20   create current(self) -> float: return self.balance\nend\n\
               object Savings(Account):\n    create bonus(self): self.deposit(self.balance)\nend\n\
               main:\n    a = Savings()\n    a.deposit(5.0)\n    a.bonus()\n    print(a.current())\n    print(a.balance)\nend\n";
    let e = runtime_error(src);
    assert_eq!(e.kind, "PrivateError");
    assert_eq!(
        e.message,
        "`balance` is private to `Account`: only its own methods can use it"
    );
}

#[test]
fn super_calls_the_base_class_method() {
    assert_eq!(
        run("object Animal:\n    name: str\n    create intro(self) -> str: return \"I am {self.name}\"\nend\n\
             object Dog(Animal):\n    create intro(self) -> str: return super.intro() + \", a dog\"\nend\n\
             object Puppy(Dog):\n    create intro(self) -> str: return super.intro() + \" (small)\"\nend\n\
             main:\n    print(Puppy(\"Rex\").intro())\nend\n"),
        "I am Rex, a dog (small)\n"
    );
}

#[test]
fn objects_run_init_after_their_fields_are_set() {
    assert_eq!(
        run("object Temp:\n    c: float\n    f: float = 0.0\n\
             \x20   create init(self):\n        if self.c < -273.15: fail ValueError(\"too cold\")\n        self.f = self.c * 9 / 5 + 32\n    end\nend\n\
             object Warm(Temp):\nend\n\
             main:\n    print(Temp(100.0).f, Warm(0.0).f)\n    attempt:\n        Temp(-300.0)\n    catch ValueError as e:\n        print(e.message)\n    end\nend\n"),
        "212.0 32.0\ntoo cold\n"
    );
}

#[test]
fn a_derived_object_can_change_an_inherited_default() {
    assert_eq!(
        run("object Animal:\n    name: str\n    legs: int = 4\nend\nobject Bird(Animal):\n    legs: int = 2\n    wings: int = 2\nend\n\
             main:\n    print(Bird(\"Tweety\"), Animal(\"Rex\"))\nend\n"),
        "Bird(name=\"Tweety\", legs=2, wings=2) Animal(name=\"Rex\", legs=4)\n"
    );
}

#[test]
fn numbers_have_methods() {
    assert_eq!(
        run("main:\n    print((-3).abs(), 15.clamp(0, 10), 3.7.floor(), 2.0.sqrt() > 1.41, 4.is_even(), 2.pow(10), 0.0.sin())\nend\n"),
        "3 10 3 true true 1024 0.0\n"
    );
}

#[test]
fn an_else_after_a_one_line_if_belongs_to_the_block_if() {
    assert_eq!(
        run("main:\n    each n in [1, 2]:\n        if n == 1:\n            print(\"one\")\n            if true: print(\"inner\")\n        else:\n            print(\"other\")\n        end\n    end\nend\n"),
        "one\ninner\nother\n"
    );
}

#[test]
fn fakes_stand_in_for_dependencies_and_record_calls() {
    assert_eq!(
        run("create save(db, name):\n    db.insert(\"people\", name)\n    return db.count()\nend\n\
             main:\n    let db = fake(methods = {\"count\": fake(returns = 1)})\n    print(save(db, \"Ada\"), db.insert.calls, db.insert.called_with(\"people\", \"Ada\"))\n\
             \x20   let clock = fake(returns_each = [8, 15])\n    print(clock(), clock(), clock.call_count)\n\
             \x20   attempt:\n        fake(fails = ValueError(\"down\"))()\n    catch ValueError as e:\n        print(e.message)\n    end\nend\n"),
        "1 [[\"people\", \"Ada\"]] true\n8 15 2\ndown\n"
    );
}

#[test]
fn a_builtin_task_on_its_own_line_is_waited_for() {
    assert_eq!(
        run("use vyrn.time\nmain:\n    let start = time.monotonic()\n    time.sleep(0.05)\n    print(time.monotonic() - start >= 0.05)\nend\n"),
        "true\n"
    );
}
