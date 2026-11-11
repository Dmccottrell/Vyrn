# Vyrn Syntax Guide (v0.4)

Vyrn keeps what makes Python easy to read (header lines ending in `:`,
indentation, slicing, comprehensions, type hints, `a if c else b`) but has its
own vocabulary: `fn`, `each`, `loop`, `when`, `using`, `attempt`, `object`.
Each word says what the line does, so Vyrn code is recognisably Vyrn. Every block closes with `end`,
so pasted or reformatted code cannot silently change meaning.

The formal definition is [`grammar.ebnf`](grammar.ebnf).

## Five rules

1. **A header ends with `:`.** After it comes either one statement on the
   same line, or a block closed by `end`.

   ```vyrn
   if lives == 0: stop

   each enemy in enemies:
       enemy.update(dt)
   end
   ```

2. **A chain shares one `end`.** `else if`, `else`, `is`, `catch` and `always`
   continue the statement above them.

   ```vyrn
   if score >= 90:
       grade = "A"
   else if score >= 80: grade = "B"
   else: grade = "F"
   end
   ```

   A one-line `if` cannot take an `else if` or `else`. Use the block form, or
   the expression `grade = "A" if score >= 90 else "F"`.

3. **Programs start at `main:`.** The top level of a file holds declarations
   only; statements run inside `main` or a function.

4. **Assignment declares, as in Python.** `total = 0` creates a variable.
   `let` makes a name fixed, `var` declares one explicitly, and `const` is
   computed at compile time. Inside `strict` and `kernel` functions every new
   name needs `let`, `var` or `const`, so a typo cannot create a variable.

5. **Everything past the name is optional.** Types, ownership, lifetime and
   hardware are added only when you need them:

   ```vyrn
   weights = load("model.bin")
   let weights: tensor[f16; V, D] = load("model.bin")
   let weights: tensor[f16; V, D] on gpu[0] = load("model.bin")
   let weights: shared tensor[f16; V, D] on gpu[0 to 3] = load("model.bin")
   ```

## Python to Vyrn

### Same as Python

`if` `else` `and` `or` `not` `in` `not in` `return` `async` `await` `self`
`@decorator` `x: int` `-> int`
`a if c else b` `x[1:9:2]` `x[:, -1]` `x[..., none]` `+ - * / // % **`
`== != < <= > >=` `"""multi-line"""` `'single'` `"double"` `r"raw"` `#` comments

### Vyrn's own words

| Python | Vyrn | Why |
|---|---|---|
| `def f(x):` | `fn f(x):` ... `end` | Shorter; one keyword for functions and lambdas |
| `elif c:` | `else if c:` | Two plain words, as in most languages |
| `for x in xs:` | `each x in xs:` | Says what happens: once for each item |
| `while c:` | `loop c:` | One word for every loop; `loop:` alone runs forever |
| `match x:` / `case P:` | `when x:` / `is P:` | Reads as a sentence: `when x:` / `is 0:` |
| `with open(p) as f:` | `using open(p) as f:` | Says the resource is in use until `end` |
| `try:` / `finally:` | `attempt:` / `always:` | `always` runs whether or not something failed |
| `class Point:` | `object Point:` | Says what it makes |
| `[x * 2 for x in xs]` | `[x * 2 each x in xs]` | Comprehensions read like loops |
| `lambda x: x * 2` | `fn(x) => x * 2` | Same keyword as named functions |
| `f"Hi {name}"` | `"Hi {name}"` | Every string interpolates; no prefix to forget |
| `True` `False` `None` | `true` `false` `none` | Lowercase like every other keyword |
| `break` / `continue` | `stop` / `skip` | Plain English |
| `pass` | (nothing) | `end` closes an empty block |
| `import x` / `from x import a` | `use x` / `use x (a)` | One form for both |
| `raise E` | `fail E` | Pairs with `fails E` in signatures |
| `except E as e:` | `catch E as e:` | The common word across languages |
| `range(0, 10)` | `0 until 10` | `0 to 10` includes 10; `by 2` sets the step |
| `if __name__ == "__main__":` | `main:` | One obvious entry point |
| `x.T` | `x.T` | Unchanged: transpose is a property, as in PyTorch |

Writing a Python word out of habit is not a mystery error: the parser names
the Vyrn word, as in "`while` is not a Vyrn keyword" with the hint "write
`loop condition:`". The retired words (`while`, `match`, `class`, ...) are
ordinary names in Vyrn, so `match = find(xs)` is fine.

### Added for 2026-era code

| Syntax | Meaning |
|---|---|
| `let` / `var` / `const` | Fixed, mutable, compile-time names |
| `struct` | Value type with a fixed layout (stored as columns in arrays) |
| `enum`, `trait`, `impl` | Tagged unions, interfaces, implementations |
| `x?.name`, `x ?? default` | Optional chaining and default for `none` |
| `xs \|> sum` | Pipeline |
| `-> T fails E` | Typed errors; a failure passes up to the caller until a `catch` |
| `mut x`, `own x` | Mutable borrow, ownership transfer (no lifetimes) |
| `a @ b`, `a * b` | Matrix product, element-wise product |
| `tensor[f16; B, T, D]` | Tensor type with checked shape |
| `on gpu[0]:`, `x on cpu` | Run or place on a device |
| `region frame:`, `on frame` | Memory arena freed in one step |
| `route GET "/users/{id}" (id: u64):` | HTTP route, implicitly async |
| `spawn task`, `par each` | Concurrent task, parallel loop |
| `strict fn`, `kernel fn` | Fully typed function, GPU/SIMD function |

## The four examples

- [`examples/grade.vyrn`](../examples/grade.vyrn): beginner, no types at all
- [`examples/game.vyrn`](../examples/game.vyrn): game loop with fixed-size data and a frame region
- [`examples/api.vyrn`](../examples/api.vyrn): async JSON routes
- [`examples/inference.vyrn`](../examples/inference.vyrn): tensor layers spread across GPUs
