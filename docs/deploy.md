# Deploying Vyrn: Cloud Binaries and Edge WebAssembly

One program, two kinds of place to run it:

| | Cloud (servers, GPU instances) | Edge (Vercel, Cloudflare, Deno, browsers) |
|---|---|---|
| Command | `vyrn build --target=linux-musl` | `vyrn build --target=wasm32-edge` |
| Output | one static Linux executable | a directory: `vyrn.wasm`, the program, a few lines of JavaScript |
| Runs today | the bytecode VM with the program bundled; typed functions as machine code (section 3) | the bytecode VM compiled to WebAssembly |
| Next | lists, structs and strings in native code; an LLVM release tier (section 4) | the same, through the WebAssembly backend |

Both builds run the same source. `examples/api.vyrn` serves the same
responses from `vyrn run`, from a static Linux binary, and from the edge
build driven with web `Request` objects.

## 1. Compiler flags

```text
vyrn build [file] [-o out] [--target=T]
```

| `--target` | Builds | Uses |
|---|---|---|
| `native` (default) | an executable for this machine | the running `vyrn` |
| `linux-gnu` | x86-64 Linux, glibc | runtime for `x86_64-unknown-linux-gnu` |
| `linux-musl` | x86-64 Linux, fully static: runs on any distribution, in `FROM scratch` containers | `x86_64-unknown-linux-musl` |
| `linux-arm64` | ARM64 Linux (Graviton, Ampere, Jetson) | `aarch64-unknown-linux-gnu` |
| `macos`, `macos-intel`, `windows` | desktop systems | `aarch64-apple-darwin`, `x86_64-apple-darwin`, `x86_64-pc-windows-msvc` |
| any Rust target triple | that system | the runtime built for it |
| `wasm32-edge` | an edge deployment directory | `vyrn.wasm` |

Runtimes for other targets are looked up in `$VYRN_RUNTIMES` (default
`~/.vyrn/runtimes/<triple>/vyrn`, and `runtimes/wasm32/vyrn.wasm`). A
missing one is an error that gives the command to build it:

```bash
cargo build --release --target x86_64-unknown-linux-musl --no-default-features
```

```bash
cargo build --release --target wasm32-unknown-unknown --manifest-path runtime/wasm/Cargo.toml
```

Runtimes leave out the module fetcher (`--no-default-features`): a built
program carries all its modules, so it never fetches anything. This also
keeps the musl build free of C code (the TLS library is the only C in the
default build).

## 2. What exists today

### 2.1 Cloud: static executables

`vyrn build` appends the program (every source file it loads, URL modules
included) to a copy of the `vyrn` runtime for the target, followed by the
bundle's length and a marker (`src/project/bundle.rs`). At start-up the
runtime finds the marker at the end of its own file and runs the program.
The musl runtime is a 3.2 MB `static-pie` executable with no dynamic
dependencies at all.

What this gives cloud GPU instances today: the tensor work (`@`, kernels,
`on gpu[0]`) runs in the GPU backend (`--features gpu`) and in native Rust,
so a large model's inference loop spends its time in native code. Only the
glue between tensor operations is interpreted. Section 3 removes that last
layer.

### 2.2 Edge: WebAssembly

`runtime/wasm` compiles the whole toolchain (parser, checker, compiler,
VM) to `wasm32-unknown-unknown`: an 867 KB module with **no imports**, so
any WebAssembly host can load it. Its interface is plain memory and
integers (`vyrn_run`, `vyrn_load`, `vyrn_handle`), and
`runtime/wasm/vyrn-edge.mjs` wraps it in the web platform's own types:

```js
const vyrn = await loadVyrn(wasmModule);
const handle = vyrn.serve(programBytes);   // (Request) => Promise<Response>
```

`vyrn build --target=wasm32-edge` writes:

| File | For |
|---|---|
| `vyrn.wasm` | the runtime |
| `program.mjs` | the program bundle, as bytes |
| `vyrn-edge.mjs` | the JavaScript above |
| `api/index.js` | a Vercel Edge Function that sends every request to the program |
| `vercel.json` | rewrites every path to that function |
| `worker.mjs` | a standard `fetch` handler for Cloudflare Workers, Deno and Bun |

```bash
vyrn build --target=wasm32-edge
```

```bash
cd build/edge && vercel deploy
```

A custom domain (for example `emvryx.app`) is attached in Vercel's project
settings, like for any other project; the routes are the program's own.

**How routing is kept.** On the edge the platform owns the socket. The VM
runs in *hosted* mode (`Vm::set_hosted`): `main:` runs as usual, so
`web.provide(...)` registers its providers, and `web.serve(...)` returns at
once instead of listening. Each request then arrives through
`Vm::handle_request(method, path, query, body, content_type)`, which uses
the same route matching, parameter conversion (path, query, JSON body into
the declared struct), providers and response rules as the TCP server.
`tests/web.rs` checks it without a socket, and the edge build of
`examples/api.vyrn` answered `GET`, `POST`, 404 and 201 identically to the
native server when driven by Node with web `Request` objects.

**What is stubbed.** A WebAssembly module with no imports has no operating
system, so the capabilities that need one fail with a teaching error that a
program can `catch`, instead of crashing:

| Capability | On the edge |
|---|---|
| Sockets (`web.serve` listening) | hosted mode: the platform delivers requests |
| Files (`read_text`, `load_weights`, `open`) | unavailable: `IoError` ("operation not supported") |
| Clock (`sleep`, `now`, `Clock`) | unavailable: `Unavailable` error |
| Threads, GPU | none; tensors run on the CPU path |
| Python and C bridges | not built for WebAssembly |

The next step is optional imports the host may provide (`env.now_ms`,
`env.fetch`, and WebGPU in browsers), each switching a stub back to the
real capability. Modules without them keep working.

## 3. The native tier (runs today)

Typed functions are compiled to machine code when a program loads, with
Cranelift (`src/native/`), on every platform `vyrn` runs on, with nothing
else installed. It works for `vyrn run`, the prompt, the language server's
checks and the executables `vyrn build` makes.

```vyrn
create fib(n: int) -> int:
    if n < 2: return n
    return fib(n - 1) + fib(n - 2)
end
```

**What compiles.** A top-level function or a struct method whose
parameters and result are `int`, `float`, `bool`, `str`, `[int]`,
`[float]`, or a struct whose fields are all `int`, `float` or `bool`, when
its body stays within those types:

- arithmetic, comparisons (chained too), `and`/`or`/`not`, `a if c else b`;
- `if`, `else if`, `else`, `loop`, `loop cond`, `each` over ranges,
  `range(...)` and lists, `stop`, `skip`, `return`;
- calls to other compiled functions and methods, recursion included, and
  `abs`, `min`, `max`, `sqrt`, `floor`, `ceil`, `int`, `float`, `exp`,
  `tanh` and the `std.math` functions;
- **lists**: literals, `[]` typed by its context, `[x] * n`, `+`,
  comprehensions, `len`, indexing (negative too), item assignment,
  `append`, `copy`, `list(xs)`, `sum` of `[int]`, and lists as results;
- **structs**: constructors (by position or name), fields, field
  assignment, methods (not `mut self`, which writes back to the caller);
- **strings**: literals, interpolation with format specs (`"{x:.2f}"`),
  `+`, `*`, comparisons, `in`, `len`, indexing, `str()`, `int(text)`,
  `float(text)`, the common methods (`upper`, `strip`, `replace`,
  `startswith`, `find`, `count`, ...), and `print`.

Anything else (maps, sets, lists of strings or structs, objects, a
variable that changes type, `int ** n` with a non-literal `n`, whose type
depends on `n`'s sign) leaves the function to the interpreter, and a call
to an interpreted function keeps its caller interpreted too. A list or
struct argument is checked item by item when the call starts: a list of
`int` given where `[float]` is declared runs on the interpreter.

**How values are kept.** List items are read and written in place (the
layout of the VM's lists and values is measured at startup and checked;
if the check fails, items go through runtime calls). A struct is a stack
slot of one word per field, and every variable owns its own, which is
the interpreter's value semantics. Strings point at the VM's own
reference-counted text. Strings and lists made during a call live in an
arena the VM owns; at the end of each loop round the ones no variable
still holds are freed, so a loop that builds a million temporary strings
does not keep them. String methods, conversions, `print` and string
indexing run the VM's own code, so they cannot behave differently.

**Nothing changes but speed.** The interpreter calls the machine code
only when the arguments have exactly the declared types, and runs the
bytecode otherwise. Integer overflow, division by zero, `//` and `%`
rounding toward negative infinity, `/` giving a float, NaN comparisons,
`min`/`max` keeping the first of equal values, and the limit of 10,000
nested calls all behave as in the interpreter, and failures come back
as the same errors, naming the function and line. `tests/native.rs` runs
every program both ways and requires the same output and the same errors,
including 120 randomly generated functions on edge-case inputs such as
the largest and smallest integers.

| Benchmark (one core) | Interpreter | Native tier | CPython 3.13 | C (`clang -O2`) |
|---|---|---|---|---|
| `fib(30)`, recursion | 0.141 s | 0.007 s | 0.069 s | 0.003 s |
| 5M-step integer loop | 0.206 s | 0.008 s | 0.203 s | 0.005 s |
| Mandelbrot 300×200, floats | 0.374 s | 0.010 s | 0.224 s | 0.008 s |
| Collatz up to 300,000 | 2.380 s | 0.134 s | 2.144 s | 0.039 s |
| Bubble sort, 3,000 `[int]` | 0.453 s | 0.014 s | 0.239 s | |
| Sieve of primes to 2M, `[int]` | 0.413 s | 0.042 s | 0.103 s | |
| Orbit, 1M steps of `Vec2` struct methods | 0.955 s | 0.048 s | 0.675 s | |
| FizzBuzz to 1M, strings | 0.181 s | 0.055 s | | |

The gap to C is mostly the overflow checks, which C does not make.
String-heavy code gains least, because each string operation still
allocates as the interpreter's does.

**Seeing it.** `VYRN_NATIVE=verbose vyrn run file.vyrn` lists each
function and why it was or was not compiled; `vyrn bytecode` marks
compiled functions; `VYRN_NATIVE=off` turns the tier off. The cargo feature
`native` (on by default, never on WebAssembly) builds it.

**How it works.** For each candidate the lowering walks the syntax tree
with the types of every local known, produces Cranelift IR, and drops
the function if it meets anything outside the subset; this repeats until
the remaining functions only call each other. Each compiled function also
gets an entry point taking its arguments as an array of 64-bit words.
Failures are written to a small state block (`src/vm/native_abi.rs`)
that the VM turns into the usual error values.

## 4. Design: the rest of the native backend

Section 3 covers numbers. The design below extends it to every type, with
LLVM as a release tier.

Goal: a statically linked Linux binary whose Vyrn code is machine code, so
the inference loop around the GPU work has no interpreter between
operations.

### 4.1 Pipeline

```text
source ─▶ syntax tree ─▶ checker (src/check.rs) ─▶ VIR ─▶ LLVM IR ─▶ object ─▶ static link
                         types, borrows, null      SSA, typed,   inkwell     + vyrn_rt.a (musl)
                         safety, exhaustiveness    explicit RC                + tensor runtime
```

1. **VIR** (Vyrn IR, `docs/architecture.md` section 2) is built from the
   checked tree: SSA form, a type on every value, reference-count
   increments and decrements made explicit (and then removed where the
   borrow checker proves them unnecessary), and `when` lowered to
   decision trees, which the exhaustiveness check makes total.
2. **Types decide representation.** What the checker knows statically
   becomes LLVM's own types; what it does not (`any`, unannotated code)
   becomes a 16-byte tagged value, the same layout the VM uses, handled by
   calls into the runtime library. Typed code pays nothing for dynamic
   code elsewhere.

   | Vyrn | LLVM |
   |---|---|
   | `int`, `i32`, `f32`, `float`, `bool` | `i64`, `i32`, `float`, `double`, `i1` |
   | `struct` (value type) | an LLVM struct, passed by value or pointer |
   | `object`, `[T]`, `{K: V}`, `str` | pointer to a reference-counted runtime object |
   | `T?` | `{ i1, T }`, or a nullable pointer for reference types |
   | `tensor` | pointer to the runtime's `DeviceTensor` |
   | `any` | `{ i64 tag, i64 payload }` with runtime helpers |

3. **Tensor operations stay library calls.** `@`, element-wise math,
   placement and `kernel` functions lower to calls into the same tensor runtime
   the VM uses (`vyrn_rt_matmul(a, b)`), which dispatches to the CPU
   reference, wgpu, or a CUDA backend. The GPU kernels are already native,
   and what the backend removes is the interpretation between them.
4. **`async`** keeps the stackful tasks of the VM: each task gets its own
   stack (mmap'd, guard page), and `await` is a stack switch in the
   runtime. This keeps "any function can await", which stackless state
   machines would not.
5. **Linking.** The object file is linked against `vyrn_rt.a`, the Rust
   runtime (tensors, tasks, HTTP, the scheduler), built for
   `x86_64-unknown-linux-musl`, with `-static`. The result needs nothing
   on the machine: no interpreter, no libc, no Python.

### 4.2 What a typed function becomes

```vyrn
strict create dot(a: [f32], b: [f32]) -> f32:
    var acc: f32 = 0.0
    each i in 0 until len(a):
        acc += a[i] * b[i]
    end
    return acc
end
```

```llvm
define float @dot(ptr %a, ptr %b) {
entry:
  %n = call i64 @vyrn_rt_list_len(ptr %a)
  %nb = call i64 @vyrn_rt_list_len(ptr %b)
  %short = icmp ult i64 %nb, %n
  br i1 %short, label %oob, label %check          ; one bounds check, hoisted
check:
  %empty = icmp eq i64 %n, 0
  br i1 %empty, label %exit, label %setup
setup:
  %pa = call ptr @vyrn_rt_list_f32_data(ptr %a)
  %pb = call ptr @vyrn_rt_list_f32_data(ptr %b)
  br label %loop
loop:
  %i = phi i64 [ 0, %setup ], [ %i1, %loop ]
  %acc = phi float [ 0.0, %setup ], [ %acc1, %loop ]
  %xa = getelementptr float, ptr %pa, i64 %i
  %xb = getelementptr float, ptr %pb, i64 %i
  %va = load float, ptr %xa
  %vb = load float, ptr %xb
  %acc1 = call float @llvm.fmuladd.f32(float %va, float %vb, float %acc)
  %i1 = add i64 %i, 1
  %done = icmp eq i64 %i1, %n
  br i1 %done, label %exit, label %loop
exit:
  %result = phi float [ 0.0, %check ], [ %acc1, %loop ]
  ret float %result
oob:
  call void @vyrn_rt_fail_index(i64 %n, i64 %nb)
  unreachable
}
```

`a` and `b` are shared borrows (the default parameter mode), so the
compiled function takes no reference-count operations, and the loop is a
plain counted loop with one hoisted bounds check. (Checked: LLVM 18's `llc`
compiles this IR as written.)

**Floating-point order.** Summing floats in a different order can change
the last bits of the result, so ordinary compiled code keeps the source's
order exactly, as the VM does, and LLVM leaves this loop scalar. In
`kernel` function code, whose results may already differ in the last bits between
CPU and GPU, the backend marks arithmetic `reassoc contract`; with that,
`opt -O2` for AVX2 turns the same loop into 8-wide vector code (also
checked). The trade-off is visible in the source, as rule 4 asks.

### 4.3 Order of work

1. Done: numbers, lists of numbers, structs of numbers and strings
   (section 3), validated against the bytecode VM. Next: maps, lists of
   strings and structs, and `object`s.
2. VIR, so the Cranelift and LLVM tiers share one lowering.
3. LLVM codegen from the same VIR (through `inkwell`), for release builds:
   `vyrn build --target=linux-musl --release` links statically against
   `vyrn_rt.a`.
4. The WebAssembly backend from the same VIR, replacing the VM in edge
   builds, with the same stubs and imports as section 2.2.
