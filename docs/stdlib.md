# The Vyrn Standard Library

Status: design spec, v0.1. Where an API already runs in the interpreter it
says **runs today**; everything else is design, which this document fixes so
that implementation can follow it. Syntax follows `docs/grammar.ebnf` v0.4.

## 1. Three tiers

| Tier | How you get it | What belongs there |
|---|---|---|
| **Prelude** | Always in scope, no `use` | What a beginner needs in their first hour: core types, `print`, `len`, `open`, collections, tensor math |
| **`std.*` modules** | `use std.http`, `use std.fs`, ... | Capabilities with a larger surface, a lifecycle or I/O policy: servers, memory mapping, task supervision, neural-network layers |
| **Packages** | `use app.storage (Db)` | Everything else, versioned by the package manager |

A name earns a place in the Prelude only if all four hold:

1. **Beginner-first.** A first script would otherwise need a `use` line for it.
2. **Free when unused.** It is a builtin in the VM's table or a type the
   compiler knows. Nothing is initialized or linked until a program calls it.
3. **No policy.** It takes no position on how to structure an application.
   `print` and `open` qualify. A router, a thread pool or a logging framework
   does not.
4. **Stable.** It will not need a breaking change. APIs still evolving live in
   a module, where a new version can sit beside the old one.

### Name resolution

Names resolve innermost first, so a program can always shadow the Prelude:

1. locals and parameters (innermost block first), then captured variables
2. top-level declarations of the file
3. names brought in by `use ... (a, b as c)`
4. the Prelude

`use std.http` binds the module to the name `http` and nothing else. There
are no glob imports, so reading a file tells you where every name comes
from. `use std.http as web` renames the module. **Runs today:** this order is
what `src/vm/compiler.rs` (`resolve`) implements, and `use std.web` /
`use std.http` with both import forms works.

## 2. The Prelude

### 2.1 Types

The Prelude's types are the language's own. The familiar long names map to
Vyrn's spelling as follows:

| Concept | Vyrn spelling | Notes |
|---|---|---|
| String | `str` | UTF-8, immutable, shared by reference count; slicing makes a view |
| List | `[T]` | Growable, shared by reference; `[T; N]` is a fixed array stored inline |
| Map | `{K: V}` | Insertion-ordered hash map |
| Tensor | `tensor[dtype; dims]` | `tensor[f32]` leaves the rank open; `vec2`/`vec3`/`vec4` are small tensors |
| Optional | `T?` | `none` or a `T`; `x?.f` and `x ?? default` |
| Numbers | `int` (`i8` to `u64`), `float` (`f16`, `bf16`, `f32`, `f64`), `bool` | |
| Tuples | `(A, B)` | `()` is the unit type |
| Tasks | `Task[T]`, `Channel[T]` | What an `async fn` call and `channel()` return |
| Errors | `Error` and the built-in kinds (`IoError`, `KeyError`, `TypeError`, `NotFound`, ...) | Any struct can be passed to `fail` |

### 2.2 Functions

| Group | Names | Status |
|---|---|---|
| Output | `print` | runs today |
| Conversion | `str`, `int`, `float` | runs today |
| Sequences | `len`, `range`, `enumerate`, `zip`, `sorted`, `min`, `max`, `sum` | runs today |
| Math | `abs`, `round`, `sqrt`, `exp`, `tanh` | runs today |
| Files | `open(path, mode = "r")` returning a `File` (`read`, `lines`, `write`, `close`) for use with `using`; `read_text`, `write_text` | `read_text`/`write_text` run today, `open` is design |
| Data | `parse_json`, `to_json` | runs today |
| Tasks | `sleep`, `channel`, `join`, `now` | runs today |
| Games | `Clock(hz, frames)` | runs today |
| Tensors | `tensor`, `zeros`, `ones`, `rand`, `arange`, `eye`, `vec2`/`vec3`/`vec4`, `softmax`, `relu`, `gelu`, `argmax`, `@`, `memory`, `fit`, `set_memory_limit`, `format_bytes` | runs today |
| Weights | `load_weights`, `save_weights`, `load_tokenizer` | runs today; moving to `std.nn` (section 6) |
| Devices | `cpu`, `gpu` (indexed: `gpu[0]`, `gpu[0 to 3]`) | runs today |

`open` belongs in the Prelude because reading a file is part of a first
script, and `using` already makes it safe:

```vyrn
using open("scores.txt") as f:
    each line in f.lines():
        print(line)
    end
end
```

`using` calls `close()` on every way out of the block, including failures
(**runs today** for any value with a `close` method).

## 3. `std.http`: a FastAPI-style async router

`std.http` turns the language's `route` declarations into a running server.
The design follows FastAPI's three ideas, adapted to a compiled language:

1. **The signature is the contract.** Parameters are filled from the path,
   then the query string, then the request body, then registered providers.
   Declared types are checked before the handler runs (a `400` otherwise),
   and the declared result type is the response schema.
2. **Dependency injection by name and type.** `http.provide("db", db)` makes
   every route parameter named `db` receive that value. Tests provide fakes
   the same way.
3. **Every handler is a task.** A route body is implicitly `async`; awaiting
   a database or a model never blocks other requests.

```vyrn
use std.http

struct Item:
    id: int
    name: str
end

struct NotFound:
    id: int
end

route GET "/items/{id}" (id: int, db) -> Item fails NotFound:
    if id not in db: fail NotFound(id)
    return db[id]
end

route POST "/items" (body: Item, db) -> (int, Item):
    db[body.id] = body
    return 201, body
end

main:
    http.provide("db", {1: Item(1, "lamp")})
    http.serve(port = 8080)
end
```

**Runs today:** `route` and `routes` groups, path parameters converted to
their declared types, query parameters, a JSON `body` converted to the
declared struct (a missing field is a `400`), `http.provide`,
`http.serve(port, host, requests)`, handlers running as concurrent tasks,
and responses built from return values. A string is sent as `text/plain`,
`(status, value)` sets the status, `none` is `204`, anything else is JSON.
A failure is a JSON error whose status follows its kind: `NotFound` is 404,
`BadRequest` 400, `Unauthorized` 401, `Forbidden` 403, otherwise 500.
`std.web` is the same module under its earlier name.

### 3.1 Design: the rest of the module

| API | Purpose |
|---|---|
| `http.Request` | A parameter of this type receives the whole request: `method`, `path`, `headers`, `query`, `body`, `client` |
| `http.Response(status, body, headers)` | Full control when a return value is not enough |
| `http.Router(prefix)` | A router value for composing apps from modules: `api.mount("/v1", users.router)` |
| `@http.middleware` | A function `(req: http.Request, next) -> http.Response` wrapping every route under it |
| `@require(...)`, `@rate_limit(...)` | Decorators are middleware with arguments, as in the `routes "/admin":` group of `examples/api.vyrn` |
| `route SSE` / `route WS` | Server-sent events and WebSockets. The handler takes a `stream` parameter and `await stream.send(x)` |
| `http.client` | `await http.get(url)`, `await http.post(url, json = x)`, for calling other services |
| `http.serve(app, port, workers)` | `workers > 1` runs one scheduler per OS thread, each with its own listener (SO_REUSEPORT) |

What stays out: templates, ORMs, sessions. Those are packages.

## 4. `std.fs`: files, and memory mapping without copies

The Prelude's `open` covers reading and writing. `std.fs` is for programs
where copies matter: model weights, datasets, logs larger than RAM.

```vyrn
use std.fs

fn count_errors(path: str) -> int:
    var errors = 0
    using fs.map(path) as data:
        each line in data.lines():
            if line.startswith("ERROR"): errors += 1
        end
    end
    return errors
end
```

`fs.map(path)` maps the file into the address space (`mmap`, or
`MapViewOfFile` on Windows) and returns a `Mapped` value. No byte is read
until it is touched, and nothing is copied:

- `data.lines()` yields each line as a `str` **view** into the mapping. A
  view is a borrowed slice: the borrow checker keeps it from outliving the
  `using` block. Store a line with `str(line)` to copy it out.
- `data.bytes(start, end)` is a `[u8]` view of a byte range.
- `fs.tensor_view(data, dtype, shape, offset)` is a read-only tensor whose
  storage is the mapping itself.

**Why this matters for large models.** A 14B-parameter model at 4 bits is
about 8 GB on disk. Reading it into a buffer and then uploading it costs
8 GB of RAM on top of the GPU copy. Mapping it means the pages stream from
the page cache straight into the GPU upload (`std.nn` uses this, section 6),
and on a 32 GB machine nothing is held twice.

| API | Behaviour |
|---|---|
| `fs.map(path) -> Mapped` | Read-only mapping; `fails IoError` |
| `fs.map_mut(path, size) -> MappedMut` | Writable mapping; changes reach the file on `flush()` or at the end of `using` |
| `fs.read(path) -> [u8]`, `fs.write(path, data)` | Whole-file helpers |
| `fs.walk(dir)`, `fs.glob(pattern)`, `fs.stat(path)`, `fs.remove`, `fs.rename` | Directory work |
| `fs.watch(path)` | A `Channel` of change events, for `async for` |

On WebAssembly there is no `mmap`, so `fs.map` reads the file into linear
memory once. Programs keep the same code and lose only the laziness.

## 5. `std.task`: green threads and supervision

**Runs today, in the language itself:** every `async fn` call starts a
*green thread*. Tasks are stackful, so any function can `await`, and each
suspended task costs only its own registers and frames. A single-threaded
scheduler runs them with timers and channels, reports deadlocks, and warns
about failed tasks nobody awaited. The Prelude keeps the everyday part:
`spawn`, `await`, `async for`, `sleep`, `channel`, `join`.

`std.task` adds the structure that larger programs need.

```vyrn
use std.task

async fn fetch(url: str) -> str:
    await sleep(0.1)
    return "page at {url}"
end

async fn crawl(urls: [str]) -> [str]:
    var pages = []
    # Structured concurrency: the group outlives none of its tasks. Leaving
    # the block waits for all of them, and the first failure cancels the rest.
    using task.group() as g:
        each url in urls: g.spawn(fetch(url))
    end
    each t in g.tasks(): pages.append(await t)
    return pages
end
```

| API | Purpose |
|---|---|
| `task.group()` | Structured concurrency, as above; `g.cancel()` stops every task in it |
| `task.timeout(seconds, work)` | `await task.timeout(2.0, fetch(url))` fails with `Timeout` |
| `task.select(a, b, ...)` | Waits for the first of several tasks or channels; returns its index and value |
| `task.yield_now()` | Lets other tasks run inside a long computation |
| `task.current()` | The running task's handle, for names and cancellation checks |
| `channel(capacity = n)` | A bounded channel: `send` waits when it is full, which gives back-pressure |
| `task.Pool(workers = n)` | OS threads for CPU-bound work: `await pool.run(f, own data)`. Arguments must be passed with `own`, so no value is shared between threads unsynchronized |

The split follows the cost model. Green threads are cheap and share one OS
thread, so the Prelude has them. A `Pool` adds threads, and with them the
rule that values cross threads only by `own`. That is an explicit choice,
so it sits behind `use std.task`.

## 6. Tensors: the Prelude and `std.nn`

The line is **math versus models**.

**Prelude: tensor math.** The tensor type, constructors, element-wise
arithmetic, `@`, reductions and the standard element-wise functions
(`exp`, `tanh`, `relu`, `gelu`, `softmax`) are numeric primitives in the
same sense as `+` on integers. They compile to the VM's tensor instructions
(`MatMul`, `Place`, ...), which a `TensorBackend` executes per device. That
is the CPU reference or the wgpu GPU backend (**runs today**,
`cargo build --features gpu`). They have no state and no configuration, so
`x @ w` means the same thing in every program. Placement is part of the
language (`on gpu[0]:`, `x on gpu[1]`), not of a library.

**`std.nn`: model building blocks.** Layers carry parameters and state,
model formats change, and sampling strategies are a matter of taste. These
need versioning, so they live in a module:

| API | Purpose |
|---|---|
| `nn.load(path, dtype = "q4", devices = gpu[0])` | Loads safetensors through `fs.map`, quantizes on the way if asked, and spreads layers over devices with `fit` |
| `nn.Linear`, `nn.Embedding`, `nn.RMSNorm`, `nn.Attention`, `nn.MLP` | Layers as structs: weights are fields, `forward` is a `kernel fn` |
| `nn.KVCache(layers, max_tokens)` | Preallocated attention cache, placed with the layers |
| `nn.Tokenizer.load(path)` | Hugging Face `tokenizer.json` (BPE, byte-level and SentencePiece) |
| `nn.sample(logits, temperature, top_k, top_p)` | Next-token sampling |
| `nn.generate(model, tokens, max_new)` | An `async` stream of tokens, for `async for` |

`load_weights`, `save_weights` and `load_tokenizer` run today as Prelude
functions. They move to `std.nn` as `nn.load_weights`, `nn.save_weights` and
`nn.Tokenizer.load`, with the Prelude names kept as aliases for one release.

## 7. Two complete programs

### 7.1 A first script: no `use` at all

Everything here is Prelude: types, `open`, `using`, string methods, lists,
maps, tensors.

```vyrn
# grades.vyrn: read "name,score" lines and summarize them.

main:
    scores = {}
    using open("grades.csv") as f:
        each line in f.lines():
            let parts = line.split(",")
            scores[parts[0]] = float(parts[1])
        end
    end

    let values = tensor(scores.values())
    print("students: {len(scores)}")
    print("average:  {round(values.sum() / len(scores), 1)}")

    each name, score in scores.items():
        let grade = "A" if score >= 90 else "B" if score >= 80 else "C"
        print("{name}: {score} ({grade})")
    end
end
```

### 7.2 An inference server: explicit dependencies

The `use` lines name exactly what the program depends on beyond the
Prelude. The model is memory-mapped and spread across GPUs, each request is
a task, and generation streams over server-sent events.

```vyrn
use std.http
use std.fs
use std.task
use std.nn
use app.auth (require_key)

struct Prompt:
    text: str
    max_tokens: int = 128
end

struct Health:
    model: str
    gpu_memory: str
end

const MODEL = "models/sml-14b-q4.safetensors"

route GET "/health" (model) -> Health:
    return Health(MODEL, format_bytes(memory_used(gpu[0])))
end

routes "/v1":
    @require_key
    route POST "/complete" (body: Prompt, model, tokenizer) -> str:
        let tokens = tokenizer.encode(body.text)
        let output = await task.timeout(30.0, model.complete(tokens, body.max_tokens))
        return tokenizer.decode(output)
    end

    @require_key
    route SSE "/stream" (body: Prompt, model, tokenizer, stream):
        let tokens = tokenizer.encode(body.text)
        async each token in nn.generate(model, tokens, body.max_tokens):
            await stream.send(tokenizer.decode([token]))
        end
    end
end

main:
    # Pages stream from disk to the GPUs; nothing is held twice in RAM.
    let model = nn.load(MODEL, dtype = "q4", devices = gpu[0 to 1])
    http.provide("model", model)
    http.provide("tokenizer", nn.Tokenizer.load("models/tokenizer.json"))
    http.serve(port = 8080)
end
```

## 8. Implementation status

| Piece | Where | Status |
|---|---|---|
| Prelude functions and types | `src/vm/builtins.rs`, `src/vm/value.rs` | runs, except `open` |
| `use std.<module>` with module and name imports | `src/vm/compiler.rs` (`use_decl`, `module_member`) | runs |
| `std.http` routing, providers, server | `src/vm/machine.rs` (`resolve_route`, `service_servers`), `src/vm/web.rs` | runs; `Request`, `Response`, `Router`, middleware, SSE/WS, client are design |
| Green threads, channels, timers | `src/vm/machine.rs`, `src/vm/task.rs` | runs |
| `std.task` groups, timeouts, select, pools | | design |
| `std.fs` mapping and views | | design; views depend on borrow checking of slices (`src/check.rs`) |
| Tensor math on CPU and GPU | `src/vm/tensor.rs`, `src/vm/gpu.rs` | runs |
| `std.nn` | `src/vm/weights.rs`, `src/vm/tokenizer.rs` hold its first pieces | design |

Build order: `open` and `File` first (the beginner path), then `std.fs.map`
with views (the large-model path), then `std.nn.load` on top of it, then
`task.group`, `timeout` and `select`, then the rest of `std.http`.
