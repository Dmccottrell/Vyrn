# Day 64: Closures and decorators

Functions in Vyrn are values: you've stored them in variables, passed them
to `sorted` and `map_all`, and returned lambdas. Today you see the two
powerful ideas this leads to. A **closure** is a function that remembers
the variables around it when it was made. A **decorator** is a function
that wraps another function to add something, such as timing, logging or
remembering answers. The project speeds up a slow calculation from
seconds to an instant with one line.

## 64.1 Functions that make functions

A function can create a function and return it. The inner function can use
the outer function's variables, *even after the outer function has
finished*:

```vyrn
create multiplier(k: int) -> (int) -> int:
    return x => x * k
end

main:
    let double = multiplier(2)
    let triple = multiplier(3)
    print(double(10), triple(10), multiplier(100)(7))
end
```

```text
20 30 700
```

`double` and `triple` are both `x => x * k`, but each remembers its own
`k`. A function together with the variables it remembers is a
**closure**: it "closes over" them.

## 64.2 Closures with memory

A closure can also *change* the variables it remembers, which gives it
private state that nothing else can touch:

```vyrn
create make_counter() -> () -> int:
    var count = 0
    create next() -> int:
        count += 1
        return count
    end
    return next
end

main:
    let tickets = make_counter()
    let visitors = make_counter()
    print(tickets(), tickets(), tickets())
    print(visitors(), tickets())
end
```

```text
1 2 3
1 4
```

Each call of `make_counter` makes a new `count`, so the two counters are
independent. This is what an object with one method does, in a few lines:
use whichever reads more clearly.

A closure made in a loop remembers that time round the loop's value:

```vyrn
main:
    var buttons = []
    each name in ["save", "open", "quit"]:
        buttons.append(() => "you clicked {name}")
    end
    each b in buttons:
        print(b())
    end
end
```

```text
you clicked save
you clicked open
you clicked quit
```

(In Python and JavaScript's older `var`, all three would say `quit`, a
classic bug. Vyrn gives each time round the loop its own `name`.)

### Exercise

Write `make_average()`, which gives a function: each call with a number adds
it to the numbers seen so far, and gives the average of all of them.

```vyrn starter
create make_average() -> (float) -> float:
    return x => x
end

main:
    let avg = make_average()
    print(avg(10), avg(20), avg(60))
end
```

```text expect
10.0 15.0 30.0
```

> Hint: keep `var total = 0.0` and `var count = 0` in `make_average`.
> Hint: the inner function adds to both and returns `total / count`.

<details><summary>Solution</summary>

```vyrn solution
create make_average() -> (float) -> float:
    var total = 0.0
    var count = 0
    create add(x: float) -> float:
        total += x
        count += 1
        return total / count
    end
    return add
end

main:
    let avg = make_average()
    print(avg(10), avg(20), avg(60))
end
```

</details>

## 64.3 Wrapping a function

A function that takes a function and gives back a new one that does
something *around* it is a **wrapper**:

```vyrn
use vyrn.time

create timed(f: any) -> any:
    create wrapper(args...):
        let start = time.now()
        let result = f(args...)
        print("{f.name} took {time.now() - start < 1}")
        return result
    end
    return wrapper
end

create slow_sum(n: int) -> int:
    return sum([i * i each i in 0 until n])
end

main:
    let measured = timed(slow_sum)
    print(measured(1000))
end
```

```text
slow_sum took true
332833500
```

`args...` collects whatever arguments the wrapper gets, and `f(args...)`
passes them all on, so the wrapper works for any function. `f.name` is the
wrapped function's name. (The example prints only whether it took under a
second, so it prints the same every time; for real, print the seconds.)

## 64.4 Decorators

Writing `slow_sum = timed(slow_sum)` after every function gets tedious.
`@timed` above a function does it for you: the function is created, passed
to `timed`, and the result takes its name. That's a **decorator**:

```vyrn
create shout(f: any) -> any:
    return (args...) => f(args...).upper() + "!"
end

@shout
create greet(name: str) -> str:
    return "hello, {name}"
end

main:
    print(greet("ada"))
end
```

```text
HELLO, ADA!
```

A decorator can take settings of its own. Then it's a function that gives
the decorator, one level deeper:

```vyrn
create repeat(times: int):
    create decorator(f: any) -> any:
        create wrapper(args...) -> list:
            return [f(args...) each _ in 0 until times]
        end
        return wrapper
    end
    return decorator
end

@repeat(3)
create roll() -> str:
    return "*"
end

main:
    print(roll())
end
```

```text
["*", "*", "*"]
```

Read `@repeat(3)` as: call `repeat(3)`, which gives a decorator, which
wraps `roll`.

### Exercise

Write a decorator `@logged` that prints `calling NAME with ARGS` before the
call and `NAME gave RESULT` after.

```vyrn starter
create add(a: int, b: int) -> int:
    return a + b
end

main:
    print(add(2, 3))
end
```

```text expect
calling add with [2, 3]
add gave 5
5
```

> Hint: `create logged(f: any) -> any:` with a `wrapper(args...)` inside.
> Hint: `f.name` is the name; `args` prints as a list.

<details><summary>Solution</summary>

```vyrn solution
create logged(f: any) -> any:
    create wrapper(args...):
        print("calling {f.name} with {args}")
        let result = f(args...)
        print("{f.name} gave {result}")
        return result
    end
    return wrapper
end

@logged
create add(a: int, b: int) -> int:
    return a + b
end

main:
    print(add(2, 3))
end
```

</details>

## 64.5 Project: remembering answers

Fibonacci numbers: each is the sum of the two before (0, 1, 1, 2, 3, 5,
8, ...). The obvious recursive function is beautifully short and terribly
slow, because it works out the same numbers over and over: `fib(30)` calls
`fib` more than a million times.

A **memo** decorator keeps a map of answers already worked out,
and gives the saved answer instead of calculating again:

```vyrn
create memo(f: any) -> any:
    let saved = {}
    create wrapper(n: int):
        if n not in saved: saved[n] = f(n)
        return saved[n]
    end
    return wrapper
end

@memo
create fib(n: int) -> int:
    if n < 2: return n
    return fib(n - 1) + fib(n - 2)
end

main:
    print(fib(30), fib(90))
end
```

```text
832040 2880067194370816120
```

Without `@memo`, `fib(90)` would take longer than you've been alive. With
it, each `fib(n)` is worked out once: 90 calculations. The recursive call
inside `fib` calls the *decorated* `fib`, so it uses the saved answers too.
This trick, **memoisation**, is the heart of dynamic programming, which
Day 68 returns to.

### Exercise

Count how many times the real calculation runs. Write `counted(f)`, a
function (not a decorator) that gives a pair: the wrapped function, and a
function that tells how many times it has been called.

```vyrn starter
create counted(f: any) -> (any, any):
    return (f, () => 0)
end

main:
    let wrapped, count = counted(x => x * 2)
    wrapped(1)
    wrapped(2)
    print(count(), wrapped(5), count())
end
```

```text expect
2 10 3
```

> Hint: `var n = 0` in `counted`; the wrapper adds one each call.
> Hint: return `(wrapper, () => n)`.

<details><summary>Solution</summary>

```vyrn solution
create counted(f: any) -> (any, any):
    var n = 0
    create wrapper(args...):
        n += 1
        return f(args...)
    end
    return (wrapper, () => n)
end

main:
    let wrapped, count = counted(x => x * 2)
    wrapped(1)
    wrapped(2)
    print(count(), wrapped(5), count())
end
```

</details>

## Today you learned

- A function can make and return functions; they remember the variables
  around them (closures), and can change them.
- Closures made in a loop each remember their own time round.
- A wrapper takes a function and gives one that does something around it;
  `args...` passes any arguments on, and `f.name` is the function's name.
- `@decorator` above a function wraps it; `@decorator(settings)` takes
  settings.
- `@memo` saves answers, turning slow recursion fast.
