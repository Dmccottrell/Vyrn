# Vyrn Syntax v0.2: Progressive Disclosure of Complexity

Status: design proposal. It supersedes the surface syntax of
[`grammar.ebnf`](grammar.ebnf) (v0.1), which has not yet been updated to match.
The samples here are not machine-checked. The compiler architecture in
[`architecture.md`](architecture.md) is unaffected; its code samples still use
v0.1 spelling.

## The principle

Every declaration answers up to five questions. Only the first is required. The
rest are optional clauses, always written in the same order, and each one you
add removes a decision from the compiler and gives it to you.

| Question | Clause | If omitted |
|---|---|---|
| What is it called? | `let name` / `var name` / `fn name` | required |
| What is it? | `: type` | inferred, or dynamic |
| Who owns it? | `own`, `mut`, `shared`, `weak` | compiler chooses; borrows are read-only |
| How long does it live? | `in region` | until its owner goes away |
| Which hardware holds it? | `on device` | wherever the surrounding code runs |

The same value at four levels of disclosure:

```
let weights = load("model.bin")
let weights: tensor[f16; V, D] = load("model.bin")
let weights: tensor[f16; V, D] on gpu[0] = load("model.bin")
let weights: shared tensor[f16; V, D] in session on gpu[0 to 3] = load("model.bin")
```

A beginner never sees lines two to four. An engineer never has to switch
languages to write them.

## Core rules

### 1. Programs start at `main`

```
main
    print("Hello, world")
```

- `main` is a keyword that opens a block. It is not a function with a signature
  to memorize.
- A program has exactly one. A file without `main` is a library.
- The top level of a file contains only declarations (`use`, `const`, `struct`,
  `fn`, `route`, `main`). Statements run only inside `main` or a function, so
  there is never a question of what executes first.
- Disclosure: `main(args)` receives command-line arguments as a list of strings.
  `main` may `await` and may fail with an error; neither needs a declaration.

### 2. Blocks are indentation, and nothing else

A line that opens a block is followed by lines indented four spaces. There are
no braces, no `end`, and no colon. The header keyword already says a block is
coming.

```
if lives == 0
    print("Game over")
else
    print("Keep going")
```

A single-statement body may sit on the header line after `then`:

```
if lives == 0 then stop
```

Brackets `( ) [ ] { }` are reserved for data: calls, indexing, lists and maps.
They never delimit code.

### 3. Variables

```
let name = "Ada"      # a name for a value; cannot be reassigned
var score = 0         # a value that changes
score += 10
const MAX = 4096      # known at compile time
```

Text interpolates with braces, with no prefix: `"Score: {score}"`.

### 4. Functions

```
fn area(width, height)
    return width * height

fn square(x) = x * x                        # one-expression form

fn area(width: f32, height: f32) returns f32
    return width * height
```

- `returns T` states the result type. `fails E` states the error type:
  `fn find(id: u64) returns User, fails NotFound`.
- Arguments are borrowed read-only unless marked. `mut x` lets the function
  change the caller's value; `own x` hands the value over. The caller writes
  the same word, so mutation is visible where it happens: `step(mut world, dt)`.
- Modifiers disclose execution models: `async fn`, `strict fn` (everything
  statically typed), `kernel fn` (compiled for SIMD and GPU).

### 5. Control flow in words

| Form | Meaning |
|---|---|
| `if` / `else if` / `else` | conditions |
| `and`, `or`, `not` | logic |
| `for item in list` | iterate |
| `for i in 1 to 10` | inclusive range |
| `for i in 0 until n` | range excluding `n` |
| `while condition` | loop while true |
| `loop` | loop forever |
| `skip` / `stop` | next iteration / leave the loop |
| `match value` with `is pattern then ...` and `else` | choose by value or shape |
| `try expr` | pass an error up to the caller |
| `await expr` | wait for an async result |

Arithmetic and comparison stay symbolic (`+ - * / == != < <=`), because those
are already the notation people learn in school. `/` always produces a
fraction; `div` and `mod` are integer division and remainder.

### 6. Structs

```
struct Player
    name = "Hero"         # type inferred from the default
    health = 100

struct Entity
    pos: vec3             # explicit, fixed-size types
    vel: vec3
    health: f32 = 100.0
```

Methods are functions declared inside the struct, taking `self`.

### 7. Web routes

The path is a literal, not a string, and its holes are typed parameters.

```
route GET /users/{id: u64} (db: Db) returns User
```

- Parameters in the path come from the URL.
- Parameters in parentheses are the request body (a struct type) or services
  provided by the application.
- A route body is implicitly async. Whatever it returns is serialized, JSON by
  default.
- `routes /admin` opens an indented group sharing a prefix.

### 8. Tensor math

Tensor expressions use the notation of linear algebra.

| Syntax | Meaning |
|---|---|
| `A * B` | matrix product (batched over leading dimensions) |
| `A'` | transpose of the last two dimensions |
| `A + B`, `A - B`, `2 * A` | elementwise, as in math |
| `A .* B`, `A ./ B`, `A .^ 2` | explicitly elementwise product, quotient, power |
| `x[all, 0]` | every row, first column |
| `x[2 to 5, all]` | rows 2 through 5 |
| `x[all, last]` | last position along the second dimension |
| `tensor[f16; B, T, D]` | element type and shape; names are shape variables |
| `x on gpu[1]` | the value, placed on that device |
| `on gpu[0]` + block | everything inside runs and allocates there |

`y = W * x + b` in a paper is `let y = W * x + b` in Vyrn.

## Examples

### A. Beginner: grade calculator

```
fn letter_for(score)
    if score >= 90 then return "A"
    if score >= 80 then return "B"
    if score >= 70 then return "C"
    return "F"

main
    let scores = [92, 78, 85, 61]

    var total = 0
    for score in scores
        total += score

    let average = total / scores.count
    print("Average: {average}, grade {letter_for(average)}")

    for score in scores
        if score < 70
            print("{score} needs another try")
```

No types, no imports, no memory vocabulary.

### B. Game developer: data-oriented entities with strict memory

```
const MAX_ENTITIES = 4096
const GRAVITY = vec3(0.0, -9.81, 0.0)

struct Entity
    pos: vec3
    vel: vec3
    health: f32 = 100.0
    alive: bool = true

strict fn step(mut world: [Entity; MAX_ENTITIES], dt: f32)
    for mut e in world
        if not e.alive then skip
        e.vel += GRAVITY * dt
        e.pos += e.vel * dt
        if e.pos.y < 0.0
            e.pos.y = 0.0
            e.health -= e.vel.length() * 0.5
            e.alive = e.health > 0.0

main
    var world: [Entity; MAX_ENTITIES] = load_level("arena")
    var clock = Clock(hz = 120)

    while clock.running
        region frame
            let contacts: [Contact] in frame = find_contacts(world)
            resolve(mut world, contacts)
            step(mut world, clock.tick())
```

Disclosed here: fixed-size array type, 32-bit floats, an exclusive `mut` borrow,
and a `frame` region whose allocations are all released when its block ends.
The array is stored one column per field, with no annotation.

### C. Web developer: async route returning JSON

```
use std.web
use app.storage (Db)

struct User
    id: u64
    name: str
    email: str?

route GET /users/{id: u64} (db: Db) returns User, fails NotFound
    return try await db.users.find(id)

route GET /users/{id: u64}/summary (db: Db)
    let user, orders = await join(
        db.users.find(id),
        db.orders.count(user_id = id),
    )
    return {
        "id": user.id,
        "name": user.name,
        "orders": orders,
        "vip": orders > 100,
    }

main
    web.serve(port = 8080)
```

The two database calls run concurrently. Routes are compiled into one lookup
table, and the JSON encoder for `User` is generated at compile time.

### D. AI engineer: tensor loop across GPUs

```
struct Layer[D]
    w_up: tensor[f16; D, 4 * D]
    w_down: tensor[f16; 4 * D, D]
    gain: tensor[f16; D]

kernel fn feed_forward(x: tensor[f16; B, T, D], w: Layer[D]) returns tensor[f16; B, T, D]
    let h = gelu(x * w.w_up)
    return (x + h * w.w_down) .* w.gain

main
    let model = load_model("sml-14b.weights") on gpu[0 to 3]
    let tokens = encode("The capital of France is")

    var h = model.embed[tokens]
    for layer in model.layers
        on layer.device
            h = feed_forward(h, layer)

    let logits = h[all, last] * model.unembed'
    print(decode(argmax(logits) on cpu))
```

`x * w.w_up` is a batched matrix product compiled to a GPU kernel. `on gpu[0 to 3]`
spreads the weights over four devices; `on layer.device` runs each layer where
its weights are, moving `h` only at device boundaries.

## What changes from v0.1

| v0.1 | v0.2 |
|---|---|
| Top-level statements allowed | `main` block required; top level is declarations only |
| `header:` then indented block | No colon; `then` for one-line bodies |
| `elif`, `break`, `continue` | `else if`, `stop`, `skip` |
| `a..b`, `a..=b` | `a until b`, `a to b` |
| `-> T ! E` | `returns T, fails E` |
| `match` / `case pattern:` | `match` / `is pattern then` |
| `route GET "/users/{id}" (id: u64)` | `route GET /users/{id: u64}` |
| `A @ B`, `A.T`, `x[:, -1]` | `A * B`, `A'`, `x[all, last]` |
| `import`, `from ... import` | `use` |
| `f"..."` | interpolation in every string |
| `//`, `%` | `div`, `mod` |
| (none) | `region name` blocks and the `in region` clause |

## Open questions

1. **`*` on tensors is the matrix product.** This matches mathematics and Julia
   and differs from NumPy, where `*` is elementwise. People arriving from Python
   will get this wrong at first. Shape checking catches most mistakes, but not
   square matrices.
2. **Indentation without a closing keyword** makes pasted or reformatted code
   fragile. A formatter that ships with the compiler is required, not optional.
3. **Path literals** need a dedicated lexer mode after an HTTP method keyword.
4. **`in`** now means both iteration (`for x in xs`) and region (`in frame`). The
   positions never overlap, but it is one word with two jobs.
