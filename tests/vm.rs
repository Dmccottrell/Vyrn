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
        compile_source("main:\n    total = 0\n    for i in 0 until 10: total += i\nend\n").unwrap();
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
        main("fn boom(): fail \"evaluated\"\nprint(true or boom(), false and boom())"),
        "true false\n"
    );
}

#[test]
fn control_flow() {
    assert_eq!(
        main(
            "for n in [3, 8, 15]:\n\
             \x20   if n % 15 == 0:\n\
             \x20       print(\"fizzbuzz\")\n\
             \x20   elif n % 3 == 0:\n\
             \x20       print(\"fizz\")\n\
             \x20   else: print(n)\n\
             \x20   end\n\
             end\n\
             i = 0\n\
             while i < 10:\n\
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
            "total = 0\nfor i in 1 to 100: total += i\nprint(total)\n\
             print([i for i in 10 until 0 by -3], len(0 until 10 by 3), 5 in 0 to 5, 5 in 0 until 5)\n\
             for _ in 0 until 0: print(\"never\")\n\
             r = 2 to 4\nprint([x * x for x in r], r)"
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
            "print([x * 2 for x in 0 until 5 if x % 2 == 0])\n\
             print([(a, b) for a in [1, 2] for b in \"xy\"])\n\
             print({k: v * 10 for k, v in [(\"a\", 1), (\"b\", 2)]})"
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
        run("fn greet(name, greeting = \"Hello\", punct = \"!\"):\n\
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
        run("fn fib(n):\n    if n < 2: return n\n    return fib(n - 1) + fib(n - 2)\nend\nmain:\n    print(fib(20))\nend\n"),
        "6765\n"
    );
}

#[test]
fn closures_capture_by_value() {
    assert_eq!(
        run("fn adder(n):\n    return fn(x) => x + n\nend\n\
             fn twice(f, x):\n    return f(f(x))\nend\n\
             main:\n\
             \x20   add5 = adder(5)\n\
             \x20   print(add5(1), twice(add5, 0), twice(fn(s) => s + \"!\", \"hi\"))\n\
             \x20   base = 100\n\
             \x20   fn offset(x): return x + base\n\
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
            "class Actor:\n    name: str\n    hp = 10\n    fn describe(self): return \"{self.name} ({self.hp})\"\n    fn hit(self, n): self.hp -= n\nend\n\
             class Boss(Actor):\n    phase = 1\n    fn describe(self): return \"Boss \" + self.name\nend\n\
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
            "struct Counter:\n    n: int = 0\n    fn bump(mut self): self.n += 1\nend\n\
             struct Box:\n    counter: Counter\nend\n\
             fn reset(mut c): c.n = 0\n\
             main:\n\
             \x20   c = Counter()\n\
             \x20   c.bump()\n\
             \x20   c.bump()\n\
             \x20   b = Box(Counter())\n\
             \x20   b.counter.bump()\n\
             \x20   items = [Counter(), Counter()]\n\
             \x20   for mut item in items: item.bump()\n\
             \x20   print(c.n, b.counter.n, [i.n for i in items])\n\
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
             fn first_big(mut xs):\n\
             \x20   for mut x in xs:\n\
             \x20       x.n += 1\n\
             \x20       if x.n > 2: return x.n\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   xs = [C(1), C(2), C(3)]\n\
             \x20   try:\n\
             \x20       for mut x in xs:\n\
             \x20           x.n *= 10\n\
             \x20           if x.n == 20: fail \"stop here\"\n\
             \x20       end\n\
             \x20   catch: print(\"failed\")\n\
             \x20   end\n\
             \x20   print([x.n for x in xs])\n\
             \x20   for mut x in xs:\n\
             \x20       if x.n == 20: stop\n\
             \x20       x.n = 0\n\
             \x20   end\n\
             \x20   print([x.n for x in xs])\n\
             \x20   print(first_big(mut xs), [x.n for x in xs])\n\
             end\n"),
        "failed\n[10, 20, 3]\n[0, 20, 3]\n21 [1, 21, 3]\n"
    );
}

#[test]
fn enums_and_match() {
    assert_eq!(
        run(
            "enum Shape:\n    Circle(radius: float)\n    Rect(float, float)\n    Empty\n\
             \x20   fn area(self):\n\
             \x20       match self:\n\
             \x20           is Shape.Circle(r): return 3 * r * r\n\
             \x20           is Shape.Rect(w, h) if w == h: return \"square {w * h}\"\n\
             \x20           is Shape.Rect(w, h): return w * h\n\
             \x20           else: return 0\n\
             \x20       end\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   for s in [Shape.Circle(1.0), Shape.Rect(2.0, 3.0), Shape.Rect(2.0, 2.0), Shape.Empty]:\n\
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
            "for v in [0, 7, 42, \"n\", \"south\", (1, 2), [1, 2, 3, 4], none]:\n\
             \x20   match v:\n\
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
             fn lookup(m, k):\n    if k not in m: fail NotFound(k)\n    return m[k]\nend\n\
             fn risky(log):\n\
             \x20   try:\n\
             \x20       return lookup({}, \"x\")\n\
             \x20   finally:\n\
             \x20       log.append(\"cleanup\")\n\
             \x20   end\n\
             end\n\
             main:\n\
             \x20   try:\n\
             \x20       print(lookup({\"a\": 1}, \"a\"))\n\
             \x20       print(lookup({\"a\": 1}, \"b\"))\n\
             \x20       print(\"not reached\")\n\
             \x20   catch NotFound as e:\n\
             \x20       print(\"missing\", e.key)\n\
             \x20   end\n\
             \x20   try:\n\
             \x20       print(1 // 0)\n\
             \x20   catch IndexError: print(\"wrong\")\n\
             \x20   catch DivisionByZero as e: print(\"caught\", e.kind)\n\
             \x20   end\n\
             \x20   log = []\n\
             \x20   try:\n\
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
        run("fn f(log):\n\
             \x20   for i in 0 until 3:\n\
             \x20       try:\n\
             \x20           if i == 0: skip\n\
             \x20           if i == 2: return \"returned\"\n\
             \x20           log.append(\"body {i}\")\n\
             \x20       finally:\n\
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
             try:\n\
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
fn spawn_and_await() {
    assert_eq!(
        run("async fn fetch(id): return id * 10\n\
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
        run("const SIZE = 4\nconst AREA = SIZE * SIZE\nfn area(): return AREA\nmain:\n    print(SIZE, area())\nend\n"),
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
             print([l.device for l in layers])"
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
         kernel fn feed_forward(x, w: Layer):\n\
         \x20   let h = gelu(x @ w.w_up)\n\
         \x20   return x + h @ w.w_down\n\
         end\n\
         main:\n\
         \x20   layers = [Layer(rand(4, 8) * 0.1, rand(8, 4) * 0.1) for _ in 0 until 4] on gpu[0 to 1]\n\
         \x20   var h = rand(2, 3, 4)\n\
         \x20   for layer in layers:\n\
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
    let e = runtime_error("fn inner(xs):\n    return xs[10]\nend\nfn outer():\n    return inner([1, 2])\nend\nmain:\n    outer()\nend\n");
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
        runtime_error("main:\n    x = 1 + \"a\"\nend\n").message,
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
        runtime_error("fn f(a): return a\nmain:\n    g = f\n    g()\nend\n").message,
        "`f` takes 1 argument, got 0"
    );
    assert_eq!(
        runtime_error("main:\n    x = 5\n    x.go()\nend\n").message,
        "int has no method `go`"
    );
}

#[test]
fn deep_recursion_is_caught() {
    let e = runtime_error("fn down(n): return down(n + 1)\nmain:\n    down(0)\nend\n");
    assert_eq!(e.kind, "StackOverflow");
}

#[test]
fn compile_time_errors() {
    assert_eq!(
        compile_errors("main:\n    print(totl)\n    total = 1\nend\n"),
        ["`totl` is not defined"]
    );
    assert_eq!(
        compile_errors("fn f(a): return a\nmain:\n    f()\nend\n"),
        ["`f` is missing the argument `a`"]
    );
    assert_eq!(
        compile_errors("main:\n    stop\nend\n"),
        ["`stop` can only be used inside a loop"]
    );
    assert_eq!(
        compile_errors("fn f(a, b = 1): return a\nmain:\n    f(c = 1)\nend\n"),
        ["`f` has no parameter `c`"]
    );
    assert_eq!(
        compile_errors("fn f(a, b): return a\nmain:\n    f(b = 1)\nend\n"),
        ["`f` is missing the argument `a`"]
    );
    assert_eq!(
        compile_errors("const N = 1\nmain:\n    N = 2\nend\n"),
        ["cannot assign to `N`: it is a top-level constant or declaration"]
    );
    assert_eq!(
        compile_errors("main:\n    with open(p) as f: f.read()\nend\n"),
        ["`with` blocks are not supported by the interpreter yet"]
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
         fn seed():\n\
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

// ---- element types ----------------------------------------------------------

#[test]
fn tensors_store_lower_precision_weights() {
    assert_eq!(
        main(
            "w = ones(64, 64)\n\
             for dtype in [f32, f16, bf16, q8, q4]:\n\
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
             for name in sorted(w.keys()): print(name, w[name].shape, w[name].dtype, w[name])\n\
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
             try:\n\
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
