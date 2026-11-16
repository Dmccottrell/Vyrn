# Day 75: Making programs fast

Most programs are fast enough. When one isn't, guessing where the time
goes is almost always wrong: measure first. Today you measure with the
**profiler**, then speed up the slow part three ways, in order of how much
they usually help: a better algorithm, types that let Vyrn compile the
code to machine code, and using every core of the processor. The project
speeds up an image filter.

## 75.1 Measure first

`vyrn run --profile file.vyrn` runs the program and then lists the time
spent in each function:

```bash
vyrn run --profile report.vyrn
```

```text
function                              calls       own ms  ms per call
match_names                               1      812.441     812.4410
load_rows                                 1       41.020      41.0200
main                                      1        2.301       2.3010
```

"Own ms" is time spent in that function itself, not in functions it
calls. Here `match_names` is 95% of the time: speeding up anything else
would be wasted effort. To time one part of a program yourself, take
`time.now()` before and after:

```vyrn
use vyrn.time

main:
    let start = time.now()
    let total = sum(i * i each i in 0 until 200000)
    let took = time.now() - start
    print(total, took < 5.0)
end
```

```text
2666646666700000 true
```

(The lesson prints only whether it took under five seconds, so it shows
the same on every computer.)

## 75.2 A better algorithm beats everything

Day 68's lesson in practice: the biggest speed-ups come from doing less
work, not doing the same work faster. Finding which names in one list
appear in another, with a list (O(n²)) and with a set (O(n)):

```vyrn
use vyrn.time

create common_slow(a: list[str], b: list[str]) -> int:
    return len([x each x in a if x in b])
end

create common_fast(a: list[str], b: list[str]) -> int:
    let seen = set(b)
    return len([x each x in a if x in seen])
end

main:
    let a = ["user{i}" each i in 0 until 3000]
    let b = ["user{i * 2}" each i in 0 until 3000]
    var start = time.now()
    let slow = common_slow(a, b)
    let slow_time = time.now() - start
    start = time.now()
    let fast = common_fast(a, b)
    let fast_time = time.now() - start
    print(slow, fast, "set version more than 10 times faster:", fast_time * 10 < slow_time)
end
```

```text
1500 1500 set version more than 10 times faster: true
```

Same answer; with 3,000 names the set version is hundreds of times faster,
and the gap grows with the data. Before anything else, ask: am I searching
a list inside a loop? Recomputing something that doesn't change? Reading a
file over and over?

## 75.3 Types make code fast

A function whose parameters and result are typed with numbers, and that
uses only numbers and lists of numbers inside, is **compiled to machine
code** when the program loads, the way C is. Untyped code runs in Vyrn's
interpreter, which has to check what each value is as it goes:

```vyrn
use vyrn.time

create typed_sum(n: int) -> int:
    var t = 0
    each i in 0 until n:
        t += i % 7
    end
    return t
end

create untyped_sum(n):
    var t = 0
    each i in 0 until n:
        t += i % 7
    end
    return t
end

main:
    var start = time.now()
    let a = untyped_sum(2000000)
    let untyped_time = time.now() - start
    start = time.now()
    let b = typed_sum(2000000)
    let typed_time = time.now() - start
    print(a == b, "typed more than 5 times faster:", typed_time * 5 < untyped_time)
end
```

```text
true typed more than 5 times faster: true
```

The two functions differ only in `n: int` and `-> int`. On a typical
computer the typed one is around twenty times faster. That's why Day 60
suggested typing the functions others call, and making the busiest ones
`strict`: strict functions are guaranteed fully typed, so they always
compile.

What compiles: numbers (`int`, `float`, `bool`), lists of numbers,
structs of numbers, arithmetic, comparisons, loops, `if`, and calls to
other compiled functions. Anything else (text, maps, objects, printing)
runs in the interpreter, which is still plenty fast for most work. A good
design keeps the number-crunching in small typed functions, and the rest
of the program untyped and comfortable.

### Exercise

Make this function compile to machine code by giving it types, and check
the answer is unchanged.

```vyrn starter
create count_primes(limit):
    var count = 0
    each n in 2 until limit:
        var prime = true
        var d = 2
        loop d * d <= n:
            if n % d == 0:
                prime = false
                stop
            end
            d += 1
        end
        if prime: count += 1
    end
    return count
end

main:
    print(count_primes(20000))
end
```

```text expect
2262
```

> Hint: `create count_primes(limit: int) -> int:`, and `var prime: bool = true`.

<details><summary>Solution</summary>

```vyrn solution
create count_primes(limit: int) -> int:
    var count = 0
    each n in 2 until limit:
        var prime: bool = true
        var d = 2
        loop d * d <= n:
            if n % d == 0:
                prime = false
                stop
            end
            d += 1
        end
        if prime: count += 1
    end
    return count
end

main:
    print(count_primes(20000))
end
```

</details>

## 75.4 Using every core

A modern processor has several **cores**, each able to run a program at
the same time. `task.parallel(work, items)` runs `work(x)` for every item,
spread over all the cores, and gives the results in order. Each core runs
its own copy of the program, so nothing is shared and nothing can clash;
`work` must be a top-level function, and items and results plain data:

```vyrn
use vyrn.task

create collatz_steps(start: int) -> int:
    var n = start
    var steps = 0
    loop n != 1:
        n = n // 2 if n % 2 == 0 else 3 * n + 1
        steps += 1
    end
    return steps
end

create longest(chunk: list[int]) -> (int, int):
    var best = (0, 0)
    each n in chunk:
        let s = collatz_steps(n)
        if s > best[1]: best = (n, s)
    end
    return best
end

main:
    let chunks = [list(start until start + 25000) each start in 1 until 200001 by 25000]
    let results = task.parallel(longest, chunks)
    print(max(results, key = r => r[1]))
end
```

```text
(156159, 382)
```

The search is split into 8 chunks, each searched on its own core; the
best of the chunks' bests is the answer. Splitting work into independent
pieces like this is called **data parallelism**. On a machine with 8
cores it's up to 8 times faster. It only helps with work that needs the
processor: waiting for the network is Day 72's job, not this one's.

## 75.5 Project: a faster image filter

A blur replaces each pixel with the average of itself and its neighbours.
An image is a grid of brightness values; a real photo has millions. Here
is the plain version, then a typed version working on one row at a time,
then the rows spread over every core:

```vyrn
use vyrn.task
use vyrn.time

const W = 300
const H = 200

create make_image() -> list:
    return [[(x * 7 + y * 13) % 256 each x in 0 until W] each y in 0 until H]
end

create blur_plain(img: list) -> list:
    var out = []
    each y in 0 until H:
        var row = []
        each x in 0 until W:
            var total = 0
            var count = 0
            each dy in -1 to 1:
                each dx in -1 to 1:
                    let yy = y + dy
                    let xx = x + dx
                    if 0 <= yy < H and 0 <= xx < W:
                        total += img[yy][xx]
                        count += 1
                    end
                end
            end
            row.append(total // count)
        end
        out.append(row)
    end
    return out
end

## One output row from the rows above, at and below it.
create blur_row(above: list[int], here: list[int], below: list[int], has_above: bool, has_below: bool) -> list[int]:
    var row: list[int] = []
    let w = len(here)
    each x in 0 until w:
        var total = 0
        var count = 0
        each dx in -1 to 1:
            let xx = x + dx
            if 0 <= xx < w:
                total += here[xx]
                count += 1
                if has_above:
                    total += above[xx]
                    count += 1
                end
                if has_below:
                    total += below[xx]
                    count += 1
                end
            end
        end
        row.append(total // count)
    end
    return row
end

create blur_job(job: list) -> list[int]:
    return blur_row(job[0], job[1], job[2], job[3], job[4])
end

create jobs_for(img: list) -> list:
    return [[img[max(y - 1, 0)], img[y], img[min(y + 1, H - 1)], y > 0, y < H - 1] each y in 0 until H]
end

main:
    let img = make_image()
    var start = time.now()
    let a = blur_plain(img)
    let plain = time.now() - start
    start = time.now()
    let b = [blur_job(j) each j in jobs_for(img)]
    let typed = time.now() - start
    start = time.now()
    let c = task.parallel(blur_job, jobs_for(img))
    let parallel = time.now() - start
    print("same result:", a == b and b == c)
    print("typed rows faster than plain:", typed < plain)
    print("a pixel before and after:", img[50][50], a[50][50])
end
```

```text
same result: true
typed rows faster than plain: true
a pixel before and after: 232 232
```

All three give the same image. The typed row function compiles to machine
code; spreading rows over cores helps most on big photos, where the time
saved outweighs the cost of sending rows to each core. Measure with your
own images before deciding: that's the whole lesson of today.

### Exercise

`blur_plain` checks `0 <= yy < H and 0 <= xx < W` nine times per pixel.
Write `clamp(v, low, high)` (a typed function) giving `v` pushed into the
range, the building block for an "edge pixels repeat" blur that needs no
checks.

```vyrn starter
create clamp(v: int, low: int, high: int) -> int:
    return v
end

main:
    print(clamp(-1, 0, 9), clamp(5, 0, 9), clamp(12, 0, 9))
end
```

```text expect
0 5 9
```

> Hint: `max(low, min(v, high))`

<details><summary>Solution</summary>

```vyrn solution
create clamp(v: int, low: int, high: int) -> int:
    return max(low, min(v, high))
end

main:
    print(clamp(-1, 0, 9), clamp(5, 0, 9), clamp(12, 0, 9))
end
```

</details>

## Today you learned

- Measure before optimising: `vyrn run --profile`, and `time.now()`.
- A better algorithm (a set instead of a list) beats any other speed-up.
- Typed numeric functions are compiled to machine code: often 10 to 50
  times faster.
- `task.parallel` spreads independent work over every core.
- Keep the number-crunching small and typed; keep the rest simple.
