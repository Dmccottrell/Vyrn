# Learn Vyrn

This tutorial takes about an hour. It assumes you have programmed a little
in some language; if you know Python, most of it will look familiar. Every
example is a complete program: save it as a `.vyrn` file and run it with
`vyrn run file.vyrn`. The output shown is what it really prints (a test
runs every example in this page).

Install Vyrn from its repository:

```bash
cargo install --path .
```

## 1. Hello

```vyrn
main:
    name = "Ada"
    year = 1843
    print("Hello, {name}!")
    print("{name} wrote the first program in {year}, {2026 - year} years ago.")
end
```

It prints:

```text
Hello, Ada!
Ada wrote the first program in 1843, 183 years ago.
```

A program starts at `main:`. A block (the body of `main`, a function, an
`if`) is a line ending in `:` and the lines under it, closed by `end`.
Text in `"..."` can include any expression in `{braces}`.

## 2. Lists, maps and loops

```vyrn
main:
    scores = [72, 95, 88]
    scores.append(100)
    print(scores, len(scores), sum(scores) / len(scores))

    ages = {"Ada": 36, "Alan": 41}
    ages["Grace"] = 85
    each name, age in ages.items():
        print("{name} is {age}")
    end

    squares = [n * n each n in 1 to 5]
    print(squares)
end
```

It prints:

```text
[72, 95, 88, 100] 4 88.75
Ada is 36
Alan is 41
Grace is 85
[1, 4, 9, 16, 25]
```

`[...]` is a list and `{...}` a map. `each` walks through anything you can
loop over; `1 to 5` counts from 1 to 5 inclusive (`0 until 5` stops before
5). `[n * n each n in ...]` builds a list in one line.

## 3. Functions, `let` and `var`

```vyrn
fn greet(name, excited = false):
    return "Hello, {name}" + ("!" if excited else ".")
end

fn area(width: float, height: float) -> float:
    return width * height
end

main:
    print(greet("Ada"), greet("Alan", excited = true))
    print(area(3.0, 4.5))

    let limit = 10      # `let` never changes
    var count = 0       # `var` can
    loop count < limit:
        count += 3
    end
    print(count)
end
```

It prints:

```text
Hello, Ada. Hello, Alan!
13.5
12
```

Parameters can have defaults and be passed by name (`excited = true`).
Types are optional: `area` declares them, so `vyrn check` tells you if a
call passes the wrong kind of value, before the program runs. `let` names a
value that never changes; `var` one that can. Plain `x = 1` works too, as in
Python.

## 4. Choices: `if` and `when`

```vyrn
enum Shape:
    Circle(radius: float)
    Rect(width: float, height: float)
    Empty
end

fn area(s: Shape) -> float:
    when s:
        is Shape.Circle(r): return 3.14159 * r * r
        is Shape.Rect(w, h): return w * h
        is Shape.Empty: return 0.0
    end
end

fn size(n: int) -> str:
    if n < 10:
        return "small"
    else if n < 100:
        return "medium"
    else:
        return "large"
    end
end

main:
    print(area(Shape.Circle(1.0)), area(Shape.Rect(2.0, 3.0)), area(Shape.Empty))
    print(size(5), size(50), size(500))
end
```

It prints:

```text
3.14159 6.0 0.0
small medium large
```

An `enum` lists the forms a value can take, and `when` handles each one.
Leave a case out and `vyrn check` tells you which:

```text
error: this `when` does not handle `Shape.Empty`
  = help: add `is Shape.Empty: ...` for each, or an `else:` arm
```

## 5. Structs and classes

```vyrn
struct Point:
    x: float
    y: float

    fn length(self) -> float:
        return sqrt(self.x * self.x + self.y * self.y)
    end
end

object Counter:
    count: int = 0

    fn add(self, n: int):
        self.count += n
    end
end

main:
    var a = Point(3.0, 4.0)
    var b = a            # a struct is a value: b is a copy
    b.x = 0.0
    print(a.x, b.x, a.length())

    let c = Counter()
    let same = c         # a class is shared: same is c
    same.add(5)
    print(c.count)
end
```

It prints:

```text
3.0 0.0 5.0
5
```

A `struct` is a value, like a number: assigning it copies it. An `object` is
shared: two names can refer to the same object. Methods take `self` first.

## 6. Missing values and errors

```vyrn
struct NotFound:
    name: str
end

fn find(name: str) -> int?:
    let people = {"Ada": 36, "Alan": 41}
    if name in people: return people[name]
    return none
end

fn age_of(name: str) -> int:
    let age = find(name)
    if age == none: fail NotFound(name)
    return age
end

main:
    print(find("Ada"), find("Bob"))
    print(find("Bob") ?? 0)
    attempt:
        print(age_of("Alan"))
        print(age_of("Bob"))
    catch NotFound as e:
        print("no one called", e.name)
    end
end
```

It prints:

```text
36 none
0
41
no one called Bob
```

`int?` means "an int, or `none`". Vyrn will not let you use it as an `int`
until you have checked: after `if age == none: fail ...`, it knows `age` is
an int. `??` gives a default. `fail` stops with an error, and `attempt` /
`catch` handles it; any struct can be an error.

## 7. Doing several things at once

```vyrn
async fn fetch(name: str, seconds: float) -> str:
    await sleep(seconds)
    return "{name} done"
end

main:
    # Both start now and wait at the same time.
    let slow = fetch("slow", 0.02)
    let fast = fetch("fast", 0.01)
    print(await join(slow, fast))

    let jobs = channel()
    jobs.send(1)
    jobs.send(2)
    jobs.close()
    async each job in jobs:
        print("job", job)
    end
end
```

It prints:

```text
("slow done", "fast done")
job 1
job 2
```

Calling an `async fn` starts it right away; `await` waits for its result.
While one task waits (here, `sleep`), the others run, so the two fetches
above take as long as the slower one, not both together. A `channel` passes
values between tasks, and `async for` reads them as they arrive.

## 8. Tensors and kernels

```vyrn
kernel fn leaky(x: f32) -> f32:
    return x if x > 0.0 else 0.1 * x
end

main:
    let w = tensor([[1.0, 2.0], [3.0, 4.0]])
    let x = tensor([[1.0, -1.0]])
    let y = x @ w           # matrix product
    print(y, y.shape)
    print(leaky(y))         # a kernel runs on every element
    print(leaky(-5.0))      # and on plain numbers
end
```

It prints:

```text
tensor([[-2.0, -2.0]]) (1, 2)
tensor([[-0.2, -0.2]])
-0.5
```

`@` is the matrix product. A `kernel fn` is written for one number and
runs on every element of a tensor; inside `on gpu[0]:`, Vyrn turns it into
a GPU program (see `docs/nn.md`).

## 9. Projects

```bash
vyrn new hello
```

creates a project: `vyrn.toml`, `main.vyrn`, a module and a test. Files use
each other with `use`:

```vyrn check
use greeting (greet)

main:
    print(greet("world"))
end
```

`use greeting` loads `greeting.vyrn` from the project, and
`use "github.com/someone/module@v1"` fetches a module from any git host,
locked to an exact version in `vyrn.lock`. Then:

| Command | Does |
|---|---|
| `vyrn run` | runs the project |
| `vyrn test` | runs every `fn test_...()` in `tests/` |
| `vyrn check` | finds mistakes without running |
| `vyrn fmt` | formats your code in the one Vyrn style |
| `vyrn build` | makes one file you can copy to any computer and run |

## 10. A web API

```vyrn check
use std.http

struct Todo:
    id: int
    title: str
end

route GET "/todos/{id}" (id: int, todos):
    return todos[id]
end

route POST "/todos" (body: Todo, todos):
    todos[body.id] = body
    return 201, body
end

main:
    http.provide("todos", {1: Todo(1, "learn Vyrn")})
    http.serve(port = 8080)
end
```

Run it and open `http://127.0.0.1:8080/todos/1`. Each `route` turns a URL
into a function call: `{id}` in the path becomes the `id` parameter, the
JSON body of a `POST` becomes a `Todo`, and what the function returns is
sent back as JSON. `vyrn build --target=wasm32-edge` packages the same
program for Vercel or Cloudflare (see `docs/deploy.md`).

## Where next

- `docs/syntax.md`: the whole syntax on one page, compared with Python.
- `docs/stdlib.md`: what is built in, and the standard library.
- `docs/principles.md`: the rules Vyrn is designed by.
- `examples/`: a web API, a game loop, and model inference.
