# Vyrn's Principles

Vyrn aims to be as easy as Python while keeping the best idea from each
major language and leaving that language's worst problem behind. Combining
everything is how languages become hard (C++, Scala, Perl), so every feature
is checked against the five rules below before it is added. A proposal that
breaks a rule needs a written reason, in its pull request, for why the rule
should give way.

## The five rules

### 1. One obvious way

Each task has one way to do it. A new feature must *replace* a pattern,
not add a second option for it.

- Module paths use dots everywhere: `use std.http`, `use python.numpy`,
  `use app.storage`. There is no `::`, and no second spelling.
- Errors use `fail`, `attempt` and `catch`. There are no exceptions plus result types
  plus error codes.
- Each keyword names what the line does: `each` item, `loop` while a
  condition holds, `when` a value is this, `using` a resource, `attempt`
  something and `always` clean up, make an `object`. Python's shape is
  kept, but Vyrn has its own words, and the parser explains the Vyrn word
  when a Python one is typed out of habit.
- Blocks end with `end`. Indentation is for people, `end` is for the parser,
  so a lost tab cannot change what a program does.
- One formatter (`vyrn fmt`) with no options, so style never needs arguing.

### 2. The five-minute test

A beginner can learn a new feature in five minutes, or the feature stays
behind an opt-in keyword that beginners never meet.

| Everyone sees | Only those who opt in see |
|---|---|
| `x = 1`, `fn`, `if`, `each`, `print`, lists, maps, f-strings | `strict`, `kernel`, `mut`/`own`/`move`, `on gpu[0]`, `region`, `unsafe`, `extern` |

Type annotations are optional. Code without them runs as dynamically as
Python; adding them buys checking where they are written.

### 3. Errors teach

Every error says what went wrong, where, and how to fix it, as Rust and Elm
do. An error that only says what went wrong is a bug in the compiler.

```text
error: cannot assign to `limit`: it is declared with `let`
 --> prices.vyrn:3:5
  |
3 |     limit = 20
  |     ^^^^^
  = hint: declare it with `var limit` to change it
```

### 4. No surprises

- **No implicit conversions.** `"3" + 1` is an error, not `"31"` or `4`.
  The only widening is `int` to `float`, which loses nothing that matters.
- **No hidden costs.** Copies of large values, allocations in `kernel`
  code, device transfers and blocking calls are visible in the source:
  `x on gpu[0]`, `str(view)`, `await`.
- **No hidden state.** No global interpreter lock, no mutable default
  arguments shared between calls, no loop variable shared by every closure.

### 5. Editions

Mistakes in the language get fixed without breaking anyone's code. Each
project names its edition in `vyrn.toml` (`edition = "2026"`). A new edition
may change a default or remove a feature, and `vyrn fix` rewrites code from
one edition to the next. Modules of different editions work together in
one program, so the ecosystem never splits the way Python 2 and 3 did.

## Python's problems, and Vyrn's answers

| Python problem | Vyrn | Status |
|---|---|---|
| Slow | Bytecode interpreter today; native compilation of typed code; `kernel` code to SIMD and GPU | Interpreter done, native code planned |
| No real multi-core (the GIL) | Green threads for waiting work; `task.Pool` OS threads for heavy work, with values crossing threads only by `own` | Green threads done |
| Type errors at run time | Gradual types checked before running (`vyrn check`) | Done |
| `None` errors | `T?` must be checked (`if x != none`, `??`, `?.`) before it is used as a `T` | Done |
| pip, venv, conda, poetry | One tool: `vyrn new`, `vyrn add`, `vyrn fmt`, `vyrn test`, `vyrn build`; dependencies locked by hash | Done |
| Hard to ship | `vyrn build` makes one executable file | Done |
| Indentation changes meaning | Blocks end with `end` | Done |
| Late-binding closures in loops | Each loop iteration has its own variable | Done |
| Mutable default arguments | Defaults are evaluated at each call | Done |
| Python 2 to 3 | Editions | Designed |

## The best idea from each language

| From | Vyrn takes | And leaves behind |
|---|---|---|
| Python | Readable syntax, comprehensions, `with` (spelled `using`), f-strings, the library ecosystem (through `use python.x`) | Speed, packaging, the GIL |
| Rust | Memory safety without a garbage collector, `match` (spelled `when`), enums with data, cargo-style tooling, editions, teaching errors | Lifetime annotations: borrows stay inside one function |
| Go | One formatter, single-file deploys, easy concurrency, fast builds, URL imports | `if err != nil` on every line |
| TypeScript | Types you add gradually | A type system too clever to read |
| Kotlin and Swift | Null safety with `T?`, `?.` and `??` | Needing an IDE to write code |
| JavaScript | `async`/`await`, running in the browser (WebAssembly) | `==` coercion and implicit conversions |
| Elixir and Erlang | Pipelines `\|>`, supervised task groups | Unfamiliar syntax |
| Zig | Compile-time code, no hidden allocations in fast code | Manual memory management |
| Julia and NumPy | Math that reads like math, built-in tensors, `@` | Slow startup |
| C and C++ | Direct access to hardware libraries through `extern "c"` | Undefined behaviour outside `unsafe` |

## Checking a proposal

Before a feature is added, its proposal answers:

1. Which existing pattern does it replace? (Rule 1)
2. Can a beginner learn it in five minutes, and if not, which keyword hides
   it? (Rule 2)
3. What do its error messages say, and what fix do they suggest? (Rule 3)
4. What does it cost, and where is that cost visible in the source? (Rule 4)
5. If it changes behaviour, which edition does it belong to? (Rule 5)
