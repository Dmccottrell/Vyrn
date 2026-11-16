# Day 65: Generators

A list holds all its items at once. That's fine for a hundred, wasteful for
a million, and impossible for "every prime number". A **generator** gives
items one at a time, working each out only when it's asked for. Generators
let you describe endless sequences, process files bigger than memory, and
build programs as **pipelines** of small steps. The project finds things in
a huge stream of data without ever holding it all.

## 65.1 `yield`

A function with `yield` in it is a generator function. Calling it doesn't
run it: it gives a generator. Each time a value is asked for, the function
runs until its next `yield`, hands that value over, and **pauses** there,
remembering where it was:

```vyrn
create countdown(n: int):
    print("(starting)")
    var i = n
    loop i > 0:
        yield i
        i -= 1
    end
    print("(finished)")
end

main:
    let g = countdown(3)
    print("made the generator")
    print(g.next())
    print(g.next())
    each rest in g:
        print("loop got", rest)
    end
    print(g.next())
end
```

```text
made the generator
(starting)
3
2
loop got 1
(finished)
none
```

Look at the order: nothing ran until the first `next()`. `each` takes the
remaining values; when the function ends, the generator is finished and
`next()` gives `none`.

## 65.2 Endless sequences

Because nothing is worked out until it's needed, a generator can go on
forever. The program takes as many as it wants:

```vyrn
create naturals(start: int = 1):
    var n = start
    loop:
        yield n
        n += 1
    end
end

create take[T](items: any, k: int) -> list[T]:
    var out = []
    if k <= 0: return out
    each x in items:
        out.append(x)
        if len(out) == k: stop
    end
    return out
end

main:
    print(take(naturals(), 5))
    print(take(naturals(100), 3))
end
```

```text
[1, 2, 3, 4, 5]
[100, 101, 102]
```

`loop:` with no condition runs forever, which is only safe because the
generator pauses at each `yield` and the `stop` in `take` stops asking.

### Exercise

Write a generator `fibonacci()` giving 0, 1, 1, 2, 3, 5, ... forever, and
print the first ten.

```vyrn starter
create take(items: any, k: int) -> list:
    var out = []
    each x in items:
        out.append(x)
        if len(out) == k: stop
    end
    return out
end

create fibonacci():
    yield 0
end

main:
    print(take(fibonacci(), 10))
end
```

```text expect
[0, 1, 1, 2, 3, 5, 8, 13, 21, 34]
```

> Hint: keep `var a = 0` and `var b = 1`; in a `loop:`, `yield a`, then
> move along: `a, b = b, a + b`.

<details><summary>Solution</summary>

```vyrn solution
create take(items: any, k: int) -> list:
    var out = []
    each x in items:
        out.append(x)
        if len(out) == k: stop
    end
    return out
end

create fibonacci():
    var a = 0
    var b = 1
    loop:
        yield a
        a, b = b, a + b
    end
end

main:
    print(take(fibonacci(), 10))
end
```

</details>

## 65.3 Generator expressions

A comprehension in round brackets is a generator instead of a list:
`(x * x each x in xs)`. As the only argument of a function, it doesn't even
need its own brackets:

```vyrn
main:
    let squares = (x * x each x in 1 to 1000000)
    print(squares.next(), squares.next(), squares.next())
    print(sum(x * x each x in 1 to 100))
    print(any(n % 7 == 0 each n in [3, 5, 14, 9]))
    print(max(len(w) each w in "the quick brown fox".split()))
end
```

```text
1 4 9
338350
true
5
```

`squares` describes a million squares but works out only the three that
are asked for. `sum`, `any` and `max` take items one at a time, so no list
of a hundred squares is ever made. Use `[...]` when you need a list you'll
use more than once; `(...)` when you'll go through it once.

## 65.4 Pipelines

Generators that take generators chain into a **pipeline**, like the
stations of a factory line. Each stage does one small job and passes items
on, one at a time:

```vyrn
create read_lines(text: str):
    each line in text.lines():
        yield line
    end
end

create non_blank(lines: any):
    each line in lines:
        if line.strip() != "": yield line.strip()
    end
end

create numbers(lines: any):
    each line in lines:
        attempt:
            yield float(line)
        catch ValueError:
            print("(skipping {repr(line)})")
        end
    end
end

main:
    let data = "12.5\n\n  7\nbroken\n3.5\n"
    let pipeline = numbers(non_blank(read_lines(data)))
    print(sum(pipeline))
end
```

```text
(skipping "broken")
23.0
```

Each stage is a few lines and easy to test on its own; together they clean
and add up the data in a single pass. Swap `read_lines` for a stage that
reads a real file, a database or a web API, and the rest doesn't change.

### Exercise

Add a stage `running_total(xs)` that yields the total so far after each
number, and use it to print the running totals of `[5, 10, 3, 2]`.

```vyrn starter
create running_total(xs: any):
    each x in xs:
        yield x
    end
end

main:
    print([t each t in running_total([5, 10, 3, 2])])
end
```

```text expect
[5, 15, 18, 20]
```

> Hint: `var total = 0` before the loop; add each `x`, then `yield total`.

<details><summary>Solution</summary>

```vyrn solution
create running_total(xs: any):
    var total = 0
    each x in xs:
        total += x
        yield total
    end
end

main:
    print([t each t in running_total([5, 10, 3, 2])])
end
```

</details>

## 65.5 Generators in objects

On Day 63 an object's `iter` method made `each x in obj` work. Write it
with `yield` and the object can be looped over without building a list:

```vyrn
object Range2D:
    width: int
    height: int

    create iter(self):
        each y in 0 until self.height:
            each x in 0 until self.width:
                yield (x, y)
            end
        end
    end
end

main:
    let grid = Range2D(3, 2)
    print([cell each cell in grid])
    print(len([c each c in Range2D(1000, 1000) if c[0] == c[1]]))
end
```

```text
[(0, 0), (1, 0), (2, 0), (0, 1), (1, 1), (2, 1)]
1000
```

## 65.6 Project: watching a stream

A sensor sends a reading every second, forever. The program must spot when
the temperature stays above a limit for three readings in a row, and
report the first time it happens. With generators, "forever" is no
problem: the stream is endless, the pipeline stops as soon as it has the
answer.

```vyrn
use vyrn.random

create sensor(seed: int):
    random.seed(seed)
    var temp = 20.0
    var second = 0
    loop:
        temp += random.float(-1.0, 1.5)
        yield (second, round(temp, 1))
        second += 1
    end
end

create windows(items: any, size: int):
    var recent = []
    each item in items:
        recent.append(item)
        if len(recent) > size: recent.pop(0)
        if len(recent) == size: yield recent.copy()
    end
end

create first_alarm(readings: any, limit: float) -> (int, list)?:
    each w in windows(readings, 3):
        if all(t > limit each _, t in w):
            return (w[0][0], [t each _, t in w])
        end
    end
    return none
end

main:
    let alarm = first_alarm(sensor(7), 25.0)
    if alarm != none:
        let start, temps = alarm
        print("too hot from second {start}: {temps}")
    end
end
```

```text
too hot from second 17: [25.1, 26.0, 27.5]
```

The sensor never ends, but nothing waits for it to: `first_alarm` returns
as soon as it finds three hot readings in a row, and the generators simply
stop being asked. `windows` is a reusable stage that slides along any
stream; it never holds more than three readings.

### Exercise

Write `changes(items)`, a stage that yields an item only when it differs
from the one before (so a stream of repeated readings becomes just the
changes).

```vyrn starter
create changes(items: any):
    each x in items:
        yield x
    end
end

main:
    print([x each x in changes([1, 1, 2, 2, 2, 3, 1, 1])])
end
```

```text expect
[1, 2, 3, 1]
```

> Hint: keep `var last = none` and a `var first = true`; yield when it's
> the first item or `x != last`, then remember `x`.

<details><summary>Solution</summary>

```vyrn solution
create changes(items: any):
    var first = true
    var last = none
    each x in items:
        if first or x != last: yield x
        first = false
        last = x
    end
end

main:
    print([x each x in changes([1, 1, 2, 2, 2, 3, 1, 1])])
end
```

</details>

## Today you learned

- A function with `yield` is a generator: it pauses at each `yield` and
  resumes when the next value is asked for (`next()`, `each`).
- Generators can be endless; the program takes only what it needs.
- `(x each x in xs)` is a generator expression; `sum(x each x in xs)` needs
  no extra brackets.
- Chain generators into pipelines of small, testable stages.
- An `iter` method with `yield` makes an object loopable without a list.
