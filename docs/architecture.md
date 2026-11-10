# Vyrn Master Architecture: Tiered Compiler and Data-Oriented Runtime

Status: draft v0.1, code samples updated to grammar v0.4. Nothing described here is implemented yet. Numbers marked
"target" are design budgets, not measurements.

Companion documents: [`grammar.ebnf`](grammar.ebnf) defines the syntax used in
every code sample below.

## 0. Goals and how they are reconciled

| Goal | Mechanism | Where the cost goes |
|---|---|---|
| Compile speed of Go | A custom dev backend that emits machine code directly from the IR, with function-level incremental rebuilds | Dev builds run roughly 2-4x slower than release builds |
| Execution speed of C++/Rust | A release backend with whole-program monomorphization and LLVM optimization | Release builds take minutes on large programs |
| Memory safety without a GC | Ownership, function-local borrow checking, ARC for shared objects | References cannot be stored freely; some patterns need `shared` or handles |
| Readability of Python | Indentation syntax, optional types, no lifetime annotations | Untyped code pays for boxing and dynamic dispatch until it is annotated |

"No compromise" is achieved per build, not per binary: the same source gets fast
compiles in the dev tier and fast execution in the release tier. The two tiers
share one front end and one IR, so they cannot disagree about what a program
means.

## 1. Compiler pipeline

```
 source (.vyrn)
     |
     v
  Parse ---------> AST            per file, parallel, no cross-file dependency
     |
     v
  Resolve + Type -> HIR           names bound, types inferred inside function bodies
     |
     v
  Lower ---------> VIR            SSA; ownership, borrows and ARC made explicit
     |                            borrow check, layout selection, async lowering
     |
     +------------------+--------------------------+
     |                  |                          |
     v                  v                          v
  DEV BACKEND      RELEASE BACKEND            TENSOR BACKEND
  single pass      whole-program VIR opts     tensor regions of VIR
  x86-64, arm64,   then LLVM                  lowered to MLIR
  wasm             x86-64, arm64, wasm        PTX, SPIR-V, CPU SIMD
```

### 1.1 Rules that keep the front end fast

These are language rules, chosen because they bound compiler work.

1. **Signatures are the inference boundary.** Types are inferred inside a function
   body, never across functions. A function's public signature is exactly what is
   written (omitted types mean `any`). Changing a body can never force another
   module to be re-typechecked.
2. **Modules form a DAG.** Import cycles are an error. Each module compiles to an
   interface summary (signatures, layouts, inlinable bodies) that dependents read
   instead of the source.
3. **Borrow checking is function-local.** References are second class: they may be
   parameters, locals and return values tied to a parameter, but they are not
   stored in heap fields. No lifetime variables exist, so there is no
   inter-procedural lifetime solving.
4. **No textual inclusion, no macros that generate declarations at parse time.**
   Decorators run on typed HIR and may not add top-level names.
5. **The compiler is a query engine.** Every stage is a memoized function of its
   inputs keyed by content hash. An edit invalidates only the queries that read
   the changed data.

### 1.2 VIR

VIR is the single mid-level IR and the contract between front end and backends.

- SSA with block arguments.
- Ownership is explicit: `move`, `borrow`, `borrow_mut`, `retain`, `release`, `drop`
  are instructions. The borrow checker and ARC optimizer run here.
- Types are fully resolved. `any` is a concrete boxed, tagged representation, so
  dynamic code needs no special handling in the backends.
- Data layout is explicit. A loop over a struct-of-arrays collection appears as
  loads and stores on individual columns (see section 3).
- Tensor operations are first-class instructions (`tensor.matmul`, `tensor.slice`,
  `tensor.place`), carrying dtype, shape and device in their types.
- Async functions are already lowered to state machines.

## 2. Pillar 1: Tiered backends

### 2.1 Dev backend

Purpose: the edit-run loop. Target: under 50 ms from saving a file to running the
changed code in a 500k-line project, and a cold build throughput above 1M
lines/s on 16 cores.

- **Direct emission.** One pass over VIR emits machine code using pre-assembled
  instruction templates with holes for operands (the copy-and-patch technique).
  No LLVM, no instruction selection DAG, no register allocation beyond a linear
  scan within a block. Targets: x86-64, AArch64, Wasm.
- **No linker in the loop.** Code is written into a persistent image with an
  indirection table for calls. Rebuilding a function writes a new body and
  updates one table slot. A running process can accept the patch, which gives
  hot reload for game and web development.
- **Generics without monomorphization.** A generic function compiles once and
  receives a hidden witness table (size, alignment, method pointers). This is
  the largest single saving in compile time and code size.
- **Checks on.** Bounds, overflow, shape and device checks all trap with source
  locations. A cycle-leak detector and a task-deadlock detector run in this tier.
- **Tensor code is not compiled.** Tensor instructions call a prebuilt kernel
  library (section 4.4), so editing model code never invokes MLIR.

Optimizations kept in the dev tier are the cheap ones that change asymptotic
behaviour: ARC pair elimination, column-wise loop generation for SoA data, and
inlining of trivial accessors.

### 2.2 Release backend

Purpose: maximum steady-state speed. Compile time is unconstrained.

1. **Whole-program VIR optimization**
   - Monomorphize all generics; specialize `any`-typed call sites whose argument
     types are known.
   - Devirtualize trait and class method calls using the closed-world class
     hierarchy.
   - Ownership optimization: turn copies into moves, remove retain/release pairs,
     promote `shared` values that never escape a thread to non-atomic counts, and
     promote non-escaping heap objects to the stack.
   - Layout optimization: hot/cold field splitting driven by profile data.
   - Async: inline and fuse state machines, remove allocations for tasks whose
     lifetime is bounded by their parent scope.
2. **LLVM** for scalar optimization, vectorization and code generation, with
   `noalias` on every borrow (exclusive borrows make this sound, and it is the
   main reason well-typed Vyrn should match C++ with `restrict` everywhere).
3. **Profile-guided optimization** using counters collected from dev-tier runs.
4. **Tensor regions** go to the tensor backend (section 4).

### 2.3 Keeping the tiers honest

- One semantics document, one conformance suite, run against both backends on
  every change.
- Differential fuzzing: random VIR is executed by both backends and results are
  compared.
- A release build never changes observable behaviour except by removing traps
  that the optimizer proves cannot fire. Overflow and bounds checks stay on in
  release unless disabled per function with `@unchecked`.

### 2.4 The interpreter (implemented)

The REPL and `vyrn run` use a bytecode interpreter that compiles straight from
the syntax tree (`src/vm`). It needs no type information, so it runs the
dynamic subset of the language today, ahead of type checking and VIR.

**Register machine, not stack machine.** Each frame owns up to 256 registers;
instructions name their operands directly. `total = total + x` is one `Add`
reading and writing registers in place, where a stack machine needs four
instructions. Fewer instructions means fewer trips through dispatch, which
dominates interpreter time. Lua 5 made the same move for the same reason.

| Decision | Reason |
|---|---|
| 4-byte instructions (`Op` is a `Copy` enum, size checked by a test) | Dense code, one load per dispatch |
| 16-byte values, reference counted, no tracing collector | Matches the language's ownership model: memory is released when the last reference goes away |
| Structs copied on write (`Rc::make_mut`) | Value semantics without copying on every assignment |
| Sliding register windows | A callee's parameters are the caller's argument registers; arguments are never copied, and `mut` parameters write back for free |
| Non-recursive dispatch loop | Deep Vyrn recursion cannot overflow the Rust stack; the limit is a clean `StackOverflow` error at 10,000 frames |
| Counted loops (`ForPrep`/`ForLoop`) | `for i in 0 until n` costs one instruction per iteration |
| `TakeIndex` for `for mut` | The element is moved out of the list and back, so struct elements change in place with no copy |
| Dedicated `MatMul`, `Place`, `PushDevice`/`PopDevice` | Tensor work is dispatched to a `TensorBackend` per device; the VM never does tensor math itself |
| Failures unwind through handler records | `fail`/`catch`/`finally` and `defer` cost nothing until something fails |

**Tensor backends.** `ReferenceBackend` runs every tensor instruction on the
CPU and is installed for all devices by default, so GPU-targeted code runs
anywhere with placement rules still enforced. A CUDA, Metal or WebGPU backend
implements the same trait and is installed with `Vm::set_gpu_backend`.

**Closures and calls.** Variables that closures use live in shared cells,
so assignments are seen on both sides and nested functions can recurse;
loop variables are new in each iteration, so each closure keeps its own.
Keyword arguments are matched at compile time when the callee is known and
by a `CallKw` instruction at run time otherwise (closures, methods, values
passed around). `with` closes its resources in reverse order on every exit.

**Tensor storage.** Tensors store `f32`, `f16`, `bf16`, or block-quantized
`q8`/`q4` (llama.cpp's Q8_0/Q4_0 layout). Compute is `f32`; matrix products
convert quantized weights a row at a time, never expanding them. Every
tensor counts against its device's memory, and `set_memory_limit` makes a
device enforce a capacity, so running out of GPU memory is caught in
development.

**Tasks in the interpreter.** Tasks are stackful: a suspended task keeps
its registers and call frames, so any function can `await`. Calling an
`async fn` starts a task at once and runs it until it first waits (as in
JavaScript); the caller gets a task handle. `await` takes a task, a
channel, or a tuple or list of tasks (`join(a, b)` builds one).
`sleep(seconds)` and `channel()` (with `send`, `close`, `len`) are
built in, and `async for x in ch` reads a channel until it is closed.
The scheduler is a single-threaded ready queue plus timers; when every
task waits on something that can never happen, the program stops with a
`Deadlock` error. A task that fails and is never awaited is reported as a
warning. When `main` returns, tasks still running are cancelled; hosts
that keep a program alive call `Vm::run_pending`.

**Standard library in the interpreter.** `use std.web` (or `std.http`)
brings in `web.serve(port, host, requests)` and `web.provide(name,
value)`. The server is part of the scheduler: `web.serve` suspends the
calling task, the scheduler accepts connections while tasks wait, and each
request runs its route as a task, so a handler that awaits does not hold
up other requests. Route parameters come from the path, then the query
string, then `body` (the request body, parsed as JSON when it is JSON),
then the providers. A string result is sent as text, `(status, body)`
sets the status, `none` is 204, anything else is JSON; failures become
JSON errors whose status follows the kind (`NotFound` 404, `BadRequest`
400, otherwise 500). `Clock(hz, frames)` is in the prelude: `tick()` waits
for the next frame and returns the fixed step, `running`, `frame`,
`elapsed` and `fps` report on it, and `stop()` ends the loop.

**Interpreter-only limitations.** `spawn` of a plain (non-`async`)
function call runs it immediately. `par for` runs sequentially. Tasks
share one OS thread, and the HTTP server speaks plain HTTP/1.1 with one
request per connection (no TLS or keep-alive).

**Measured performance** (release build, best of 7, 4-core cloud VM,
CPython 3.13 on the same programs, outputs cross-checked):

| Workload | Vyrn VM | CPython 3.13 | Relative |
|---|---|---|---|
| 30M-iteration integer loop | 1070 ms | 1661 ms | 1.55x faster |
| `fib(30)`: 1.6M calls | 232 ms | 97 ms | 2.4x slower |
| 1,000 entities x 1,000 frames | 208 ms | 144 ms | 1.4x slower |

Calls and field access are where CPython 3.13's specializing interpreter
leads. Profiling shows the cost spread across bounds-checked register access,
out-of-line clone and drop of values, and linear field lookup. The planned
fixes, in order of expected gain:

1. NaN-boxed 8-byte values, so scalars copy and drop without branches.
2. Inline caches for field and method lookup.
3. Superinstructions for compare-and-branch and for `x.f op= y`.
4. A bytecode verifier at load time, which makes unchecked register access
   sound.
5. Frames that do not touch reference counts on call and return.

## 3. Pillar 2: Data-oriented memory model

### 3.1 Ownership

| Form | Meaning | Cost |
|---|---|---|
| `struct` value | Owned, stored inline, moved or copied | None |
| `x: T` parameter | Shared read-only borrow | None |
| `mut x: T` parameter | Exclusive borrow | None |
| `own x: T` parameter | Ownership moves to the callee | None |
| `class` / `shared T` | Reference counted | Count updates, mostly elided statically |
| `weak T` | Non-owning reference to a shared value | Checked upgrade |
| `ptr T` | Raw pointer, only in `unsafe` | None |

There is no tracing collector. Cycles among `class` instances must be broken with
`weak`; the dev tier reports cycles that leak.

**Regions.** A region is a bump allocator with a scope. Everything allocated
inside is freed in one step when the scope ends, and the borrow checker prevents
references from escaping it. The runtime opens a region automatically per HTTP
request and per game frame, so most short-lived allocations cost a pointer
increment and are never individually freed.

### 3.2 Layout is a property of the container

Developers declare *what* an entity is. The compiler decides *how* it is stored,
based on where it is stored.

| Declaration | Default layout |
|---|---|
| A single `struct` value | Fields reordered by alignment to remove padding |
| `[T]` or `[T; N]` where `T` is a struct | **Struct of arrays**: one contiguous, 64-byte-aligned column per field |
| `bool` column | Bitset |
| Nested struct field (e.g. `pos: vec3`) | Kept together as one column of `vec3`; splitting is opt-in |
| `class` instance | Heap object with header; never SoA |

Opt-outs are explicit: `@layout(aos)` for collections accessed mostly by random
whole-element lookup, `@layout(c)` to fix field order for FFI.

Consequences:

- **Element references are row handles.** `for mut e in world` binds `e` to
  (collection, index). `e.pos` compiles to an indexed load from the `pos` column.
  The loop reads and writes only the columns it mentions, so untouched fields
  never enter the cache.
- **Stable identity uses handles, not pointers.** `pool[T]` is a SoA collection
  with generational indices. A `Handle[T]` is 8 bytes, survives reallocation and
  detects use after removal.
- **Columns are tensors.** `world.pos` is a zero-copy `tensor[f32; N, 3]` view.
  The same bytes can be iterated by game code, passed to a `kernel fn`, or
  uploaded to a GPU buffer without conversion.
- **Parallelism falls out of the layout.** `par for mut e in world` splits by
  index range. The borrow checker proves iterations touch disjoint rows.

### 3.3 Game entity example

```vyrn
const GRAVITY = vec3(0.0, -9.81, 0.0)

struct Entity:
    pos: vec3
    vel: vec3
    health: f32 = 100.0
    name: str
    alive: bool = true
end

strict fn step(mut world: [Entity], dt: f32):
    for mut e in world:
        if not e.alive: skip
        e.vel += GRAVITY * dt
        e.pos += e.vel * dt
    end
end

fn lowest_point(world: [Entity]) -> f32: return min(world.pos[:, 1])
```

What the compiler does with it:

- `[Entity]` is stored as five columns. `step` touches three of them (`pos`, `vel`,
  `alive`); `health` and `name` are never loaded.
- `alive` is a bitset, so the skip test processes 64 entities per word.
- The loop body has no aliasing (exclusive borrow) and no calls, so the release
  backend vectorizes it.
- `world.pos[:, 1]` is a strided view over the `pos` column. No copy is made.

No layout annotation appears in the source.

### 3.4 Object-oriented programming

OOP is supported, with defaults chosen so that it does not undo the layout model.

- `class` gives identity, encapsulation (`pub`), methods and reference semantics.
- Polymorphism is through `trait`. Calls on a concrete type are static. Calls
  through a trait-typed value use a vtable, and the release tier devirtualizes
  them when the set of implementers is known.
- **Single implementation inheritance for `class` only, no inheritance for
  `struct`.** Struct layouts stay flat and predictable; classes keep the
  modelling tool people expect. Grammar v0.4 spells it as in Python:
  `class Enemy(Actor):`.
- Guidance enforced by lints, not by the type system: classes for long-lived
  objects with identity (scenes, services, connections), structs in collections
  for anything that exists in the thousands.

## 4. Pillar 3: The ML / GPU bridge

### 4.1 Hybrid lowering

General code and tensor code start in the same IR and separate late.

```
 VIR function
   |
   |-- scalar / control-flow instructions ----> dev or release backend --> x86-64 / arm64 / wasm
   |
   `-- maximal runs of tensor.* instructions
           |
           v
        Tensor region (dataflow graph, shapes symbolic or concrete)
           |
           v
        MLIR: high-level tensor ops -> linalg -> loops / vector
           |
           +--> GPU dialect -> NVVM -> LLVM NVPTX -> PTX      (NVIDIA)
           +--> GPU dialect -> SPIR-V                         (Vulkan, WebGPU)
           `--> LLVM -> AVX-512 / NEON / SVE                  (CPU fallback)
```

- **Region formation** happens in VIR. Any `kernel fn` is one region. In other
  functions, adjacent tensor instructions are grouped automatically. Scalar code
  between tensor operations stays on the CPU path and calls into the region.
- **What crosses the boundary** is a typed handle: (device, buffer, dtype, shape,
  strides). No serialization, no interpreter, no Python-style dispatch.
- **Graph-level optimization** runs before MLIR: operator fusion, constant
  folding of shapes, common subexpression elimination, in-place update when the
  borrow checker proves the input is dead, and memory planning (buffer reuse
  from liveness).
- **Custom tensor dialect.** Vyrn defines its own MLIR dialect for the high-level
  operations so that fusion and placement decisions are made with full type and
  ownership information, then lowers to upstream dialects.

The exact upstream MLIR pass pipeline changes between LLVM releases and must be
pinned and verified against the LLVM version adopted at implementation time.
Per-kernel performance tuning (tiling, scheduling, vendor-library substitution)
is an empirical workstream, not something this document can settle.

### 4.2 Devices in the type system

- A tensor type carries its device: `tensor[f16; V, D] on gpu[0]`.
- `on gpu[i]:` ... `end` sets the device for allocations and kernels in a
  block. The same keyword names a memory region: `on frame:` ... `end`.
- `x on gpu[1]` is an explicit transfer. Mixing devices in one operation without
  a transfer is a compile error in strict code and a trap in dynamic code, so
  hidden host-device copies cannot occur.
- Ownership applies to device memory exactly as to host memory. Dropping the last
  owner frees the device buffer deterministically; there is no finalizer queue.

### 4.3 Multi-GPU execution for a 14B-parameter model

Sizing: 14B parameters is about 28 GB at 16-bit, 14 GB at 8-bit and 7 GB at
4-bit, before activations and KV cache. The runtime therefore assumes weights
may not fit on one device.

- **Weights are memory-mapped.** A model file is a header plus aligned raw
  columns. Loading maps the file and transfers each shard to its device from
  pinned memory. Startup cost is I/O, not parsing.
- **Sharding is declared, not hand-coded.** `on gpu[0 to 3]` on a weight collection
  asks the planner to place layers across devices; the planner balances memory
  and minimizes the number of activation transfers.
- **Pipeline parallelism for inference.** Each device runs its contiguous slice
  of layers. Only the activation tensor crosses devices, once per boundary.
- **Data parallelism for training.** `par for` over devices runs replicas;
  gradients are reduced with a built-in all-reduce.
- **Automatic differentiation** is a transformation on tensor regions in VIR,
  performed before lowering, so the backward pass gets the same fusion and
  memory planning as the forward pass.
- **Async integration.** A kernel launch returns a future. Web handlers `await`
  inference without blocking a scheduler thread, and the runtime batches
  concurrent requests into one launch when shapes allow.

### 4.4 Tensor code in the dev tier

Compiling through MLIR is too slow for the edit-run loop, so the dev tier does
not do it.

- Each tensor instruction dispatches to a prebuilt kernel library shipped with
  the toolchain (one generic implementation per operation, per device).
- Fused, specialized kernels from previous release builds are cached on disk by
  hash of (region graph, dtypes, shapes, device) and reused by dev builds when
  present.
- Shape and device errors are still reported at compile time in strict code,
  because that checking happens in the front end.

### 4.5 Tensor example

```vyrn
kernel fn attention(
    q: tensor[f16; B, H, T, D],
    k: tensor[f16; B, H, T, D],
    v: tensor[f16; B, H, T, D],
    mask: tensor[bool; T, T],
) -> tensor[f16; B, H, T, D]:
    let scores = (q @ k.mT) / sqrt(D)
    return softmax(where(mask, scores, -inf), axis = -1) @ v
end

strict fn generate(model: Model on gpu[0 to 3], prompt: tensor[i32; B, T]) -> tensor[i32; B]:
    var h = model.embed[prompt]
    for layer in model.layers:
        on layer.device: h = layer.forward(h)
    end
    return argmax(h[:, -1] @ model.unembed, axis = -1) on cpu
end
```

What the compiler does with it:

- `attention` becomes one tensor region. The two matrix products, the mask and
  the softmax are fused where profitable and emitted as PTX. A mismatch between
  the shapes of `q`, `k` and `v` is a compile error.
- `model.embed[prompt]` is a gather, a native indexing form.
- `on layer.device:` moves `h` only when consecutive layers are on different
  devices.
- The final `on cpu` is the only host transfer, and it moves `B` integers.

## 5. Pillar 4: Concurrency and web

### 5.1 Tasks

- `async fn` compiles to a stackless state machine. A suspended task is a small
  heap or region object, typically a few hundred bytes, so millions can exist.
- `spawn f()` starts a task and returns a handle. `await` suspends.
- Scheduler: M:N work stealing by default. Servers can select thread-per-core,
  where each core owns its tasks, sockets and region, and nothing is shared
  across cores without an explicit channel.
- I/O: io_uring on Linux, kqueue on macOS and BSD, IOCP on Windows, and the host
  event loop on Wasm.
- **Structured concurrency.** A task cannot outlive the scope that spawned it
  unless it is explicitly detached. Leaving a scope awaits or cancels its
  children. This is also what allows child tasks to borrow from the parent.
- Blocking calls made from async code are moved to a blocking pool
  automatically, so a slow FFI call cannot stall a scheduler thread.

### 5.2 Data-race freedom

Guaranteed at compile time, by the same rules as single-threaded code:

- A value moved into a task is no longer accessible to the spawner.
- Any number of tasks may hold read-only borrows, or one may hold a `mut` borrow.
- Whether a type may cross a task boundary is inferred from its fields. `shared T`
  uses atomic counts only when the compiler sees it cross threads.
- Shared mutable state requires a synchronization type (`Mutex[T]`, `Atomic[T]`,
  `Channel[T]`). The data lives inside the lock, so it cannot be reached without
  holding it.

### 5.3 Deadlocks

Deadlock freedom cannot be proven for arbitrary programs. The design removes the
common causes statically and detects the rest.

| Cause | Handling |
|---|---|
| Holding a lock across `await` | Compile error |
| Acquiring two locks in inconsistent order | `with a.lock(), b.lock():` acquires in a canonical order. Nested acquisition of two `Mutex` values requires declared lock levels, checked at compile time |
| Re-locking a held mutex | Compile error when visible in one function; dev-tier trap otherwise |
| Tasks waiting on each other through channels | Not prevented statically. The dev tier maintains a wait-for graph and reports the cycle with task backtraces |
| Forgotten task that never completes | Prevented by structured concurrency |

The claim made to users is therefore: no data races ever, no mutex deadlocks in
code that compiles, and channel deadlocks diagnosed in development.

### 5.4 Web routes

`route` declarations are compiled, not registered at run time.

- All routes in the program are merged into one static radix tree at compile
  time. Conflicting or unreachable routes are compile errors.
- Path and query parameters are parsed straight into their declared types.
- JSON encoders and decoders are generated per struct type at compile time. No
  reflection, no intermediate document tree.
- Each request runs in its own region. The response is written, then the region
  is released in one step.
- Provider parameters (database pools, configuration) are resolved at compile
  time; a missing provider is a compile error.

```vyrn
route GET "/users/{id}" (id: u64, db: Db) -> User fails NotFound:
    return try await db.users.find(id)
end

route POST "/generate" (req: Prompt, model: Model) -> Completion:
    let tokens = await model.generate(req.tokens)
    return Completion(text = decode(tokens))
end
```

### 5.5 WebAssembly

- Both backends emit Wasm. The dev backend targets it directly for fast browser
  iteration.
- `export "wasm" fn` and `extern "js"` generate the JavaScript glue and
  TypeScript declarations.
- `await` on a JavaScript promise suspends the task and resumes from the host
  event loop.
- Tensor regions lower to SPIR-V for WebGPU, with Wasm SIMD as the fallback.

## 6. Risks and open decisions

1. **Two backends doubles the code generation surface.** Mitigated by the shared
   VIR and differential testing, but it is the largest ongoing cost of this
   design.
2. **SoA by default is not always faster.** Random whole-element access is slower
   than with array-of-structs. The default needs validation on real workloads,
   and `@layout(aos)` must stay cheap to apply.
3. **Second-class references restrict some designs** (self-referential structures,
   iterators that store borrows). The escape hatches are handles, `shared`, and
   `unsafe`. This needs testing against real programs before it is frozen.
4. **Dev and release generics differ in mechanism** (witness tables versus
   monomorphization). Semantics are identical; performance characteristics are
   not, and performance bugs may only appear in one tier.
5. **Gradual typing boundary.** Typed code calling untyped code inserts run-time
   checks. Their cost and their error messages need a dedicated design.
6. **GPU portability.** PTX is NVIDIA-only. SPIR-V coverage for the full tensor
   operation set, and AMD and Apple targets, are unscheduled.
7. **Class inheritance** is limited to one base class (section 3.4); whether
   that is enough for real game and UI code needs validating.

## 7. Build order

1. Front end to VIR, with the borrow checker, validated by an interpreter.
   *Done so far: lexer, parser, AST and the bytecode interpreter (section 2.4).
   Type checking, the borrow checker and VIR are next.*
2. Dev backend for x86-64. This is the first usable compiler.
3. SoA collections and regions.
4. Async runtime and `route`.
5. Tensor instructions with the prebuilt kernel library, single GPU.
6. Release backend through LLVM.
7. Tensor backend through MLIR, then multi-GPU planning.
8. Wasm targets, AArch64, hot reload.
