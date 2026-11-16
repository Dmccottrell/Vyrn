# Day 60: Types and `vyrn check`

Since Day 6 you've been writing `name: str` and `-> int` on functions. This
unit is about the parts of Vyrn that make big programs safe and fast, and
types come first. Today you learn what Vyrn checks before your program
runs, how to describe lists, maps, optional values and functions as types,
and how to use `vyrn check` to find mistakes in a whole project at once.
The project is adding types to an untyped program and fixing what the
checker finds.

## 60.1 Checked before it runs

Every `vyrn run` first **checks** the program: if a function wants text and
gets a number, the program doesn't start at all:

```vyrn
create shout(text: str) -> str:
    return text.upper() + "!"
end

main:
    print("this line never runs")
    print(shout(42))
end
```

```text
error: argument `text` of `shout` is str, but this is int
```

Python would print the first line and then crash on the second; C++ and
Java would refuse to compile. Vyrn refuses to run, which is the same thing
without a separate compile step. A mistake found before the program runs
can't reach the person using it.

`vyrn check file.vyrn` checks without running, and `vyrn check` on its own
checks a whole project: every problem in every file, with the line and a
hint. Run it before you commit.

## 60.2 Where types come from

You don't have to write types everywhere. Vyrn works them out from values:

| You write | Vyrn knows |
|---|---|
| `let n = 5` | `n` is `int` |
| `let names = ["Ada", "Alan"]` | `names` is a list of `str` |
| `let p = Point(1, 2)` | `p` is a `Point` |
| `let total = price * 1.2` | `total` is `float` |

So mistakes are found even without annotations:

```vyrn
main:
    let names = ["Ada", "Alan"]
    let first = names[0]
    print(first + 1)
end
```

```text
error: cannot apply `+` to str and int
```

Where you *do* write types, they are promises the checker holds you to.
Function parameters and results are the important ones: they are the
contract between a function and everyone who calls it.

```vyrn
create half(n: int) -> int:
    return n / 2
end

main:
    print(half(7))
end
```

```text
error: `half` returns int, but this is float
```

`/` always gives a `float` (`7 / 2` is `3.5`). The function promised an
`int`, so the checker caught the broken promise. Fix it by changing the
promise (`-> float`) or the code (`n // 2`).

A `var` without a type can hold anything, like a Python variable; give it a
type and it can only hold that type:

```vyrn
main:
    var anything = 5
    anything = "five"
    var count: int = 5
    count = "five"
end
```

```text
error: `count` is int, but this is str
```

### Exercise

The function promises a `str` but can end without returning one (when the
list is empty). Make it always return text: `"nobody"` for an empty list.

```vyrn starter
create first_name(names: list[str]) -> str?:
    if names: return names[0]
    return none
end

main:
    print(first_name(["Ada", "Alan"]), first_name([]))
end
```

```text expect
Ada nobody
```

> Hint: change the result type to `-> str` and return `"nobody"` instead of
> `none`.

<details><summary>Solution</summary>

```vyrn solution
create first_name(names: list[str]) -> str:
    if names: return names[0]
    return "nobody"
end

main:
    print(first_name(["Ada", "Alan"]), first_name([]))
end
```

</details>

## 60.3 Describing types

| Type | Means | Example |
|---|---|---|
| `int`, `float`, `str`, `bool` | the basic values | `age: int` |
| `list[int]` (or `[int]`) | a list of ints | `scores: list[int]` |
| `map[str, float]` (or `{str: float}`) | a map from text to floats | `prices: map[str, float]` |
| `(int, str)` | a tuple: an int then a text | `-> (int, str)` |
| `int?` | an int, or `none` | `-> int?` |
| `Point` | a struct or object you declared | `p: Point` |
| `(int) -> str` | a function taking an int and giving text | `f: (int) -> str` |
| `any` | anything: switches checking off | `data: any` |

The item types are checked too:

```vyrn
main:
    let scores: list[int] = [90, 85]
    scores.append(77)
    scores.append("ninety")
end
```

```text
error: an item of this list[int] is int, but this is str
```

## 60.4 Optional values

`int?` says "an int, or `none`". A function that might not find anything
returns one. The checker won't let you use it as an int until you've
checked:

```vyrn
create find(names: list[str], who: str) -> int?:
    each i, n in enumerate(names):
        if n == who: return i
    end
    return none
end

main:
    let i = find(["Ada", "Alan"], "Alan")
    print(i + 1)
end
```

```text
error: `i` may be none, so it cannot be used in arithmetic
```

After `if i != none:`, the checker knows it's an int inside the `if`. Or
give a default with `??`:

```vyrn
create find(names: list[str], who: str) -> int?:
    each i, n in enumerate(names):
        if n == who: return i
    end
    return none
end

main:
    let i = find(["Ada", "Alan"], "Alan")
    if i != none: print("position", i + 1)
    let j = find(["Ada"], "Grace") ?? -1
    print(j)
end
```

```text
position 2
-1
```

This removes the most common crash in Python and Java, using a missing
value (`NoneType has no attribute`, `NullPointerException`), by making you
handle it where it can happen. Day 61 has more tools for it.

## 60.5 Function types

A function that takes another function says what kind with a function
type. A lambda passed to it gets its parameter types from it, so its body
is checked too:

```vyrn
create apply_all(xs: list[int], f: (int) -> int) -> list[int]:
    return [f(x) each x in xs]
end

create square(n: int) -> int:
    return n * n
end

main:
    print(apply_all([1, 2, 3], square))
    print(apply_all([1, 2, 3], n => n * 10))
    print(apply_all([1, 2, 3], n => n + "!"))
end
```

```text
error: cannot apply `+` to int and str
```

Here `n` in the last lambda is an `int` because `apply_all` said so, and
`int + str` is caught.

### Exercise

Write `count_if(xs, test)`, typed: it takes a list of ints and a function
from int to bool, and gives how many items pass the test.

```vyrn starter
create count_if(xs: list[int], test: any) -> any:
    return 0
end

main:
    print(count_if([3, 8, 12, 5], n => n > 4), count_if([], n => true))
end
```

```text expect
3 0
```

> Hint: `test: (int) -> bool` and `-> int`.
> Hint: `len([x each x in xs if test(x)])`

<details><summary>Solution</summary>

```vyrn solution
create count_if(xs: list[int], test: (int) -> bool) -> int:
    return len([x each x in xs if test(x)])
end

main:
    print(count_if([3, 8, 12, 5], n => n > 4), count_if([], n => true))
end
```

</details>

## 60.6 Strict functions

Ordinary functions may leave out types, which is handy while you're
exploring. A `strict create` must type every parameter and its result and
can't use `any`, so nothing in it goes unchecked. Strict, fully typed
functions are also the ones Vyrn can compile to machine code (Day 75):

```vyrn
strict create average(xs: list[float]) -> float:
    if not xs: return 0.0
    return sum(xs) / len(xs)
end

main:
    print(average([2.0, 4.0, 9.0]))
end
```

```text
5.0
```

```vyrn
strict create average(xs) -> float:
    return sum(xs) / len(xs)
end

main:
    print(average([1.0]))
end
```

```text
error: parameter `xs` needs a type in strict code
```

A good habit for growing programs: explore without types, then add types
to every function that others call, and make the busiest ones `strict`.

## 60.7 Project: typing an old program

Here is a small program written without types, the way you'd write a quick
script. It runs, mostly:

```vyrn
create total(items):
    var sum = 0
    each item in items:
        sum += item["price"] * item["qty"]
    end
    return sum
end

create cheapest(items):
    var best = none
    each item in items:
        if best == none or item["price"] < best["price"]: best = item
    end
    return best
end

main:
    let basket = [{"name": "tea", "price": 3.5, "qty": 2}, {"name": "cake", "price": 2.25, "qty": 1}]
    print(total(basket), cheapest(basket)["name"])
    print(cheapest([])["name"])
end
```

```text
9.25 cake
error: TypeError: none cannot be indexed
```

The crash happens only when the basket is empty, which a quick test would
miss. Adding types, with a `struct` instead of maps, makes the checker
point at the problem before it can happen:

```vyrn
struct Item:
    name: str
    price: float
    qty: int
end

create total(items: list[Item]) -> float:
    return sum([i.price * i.qty each i in items])
end

create cheapest(items: list[Item]) -> Item?:
    if not items: return none
    return min(items, key = i => i.price)
end

main:
    let basket = [Item("tea", 3.5, 2), Item("cake", 2.25, 1)]
    print(total(basket), cheapest(basket)?.name)
    print(cheapest([])?.name ?? "the basket is empty")
end
```

```text
9.25 cake
the basket is empty
```

What changed, and why it's better:

- `Item` is a struct, so `item["prise"]` (a typo) would be an error found
  by the checker, not a `KeyError` found by a user.
- `cheapest` says it returns `Item?`, so every caller *must* deal with the
  empty case: `cheapest([]).name` would be an error ("may be none"). The
  crash from the untyped version can't be written any more.
- `total` and `cheapest` say what they take, so calling them with the wrong
  thing is caught where it is called.

`?.name` gives `none` instead of failing when the value is `none`, and `??`
gives a default (Day 61).

### Exercise

Type this function fully: it takes a map from names to scores and a pass
mark, and gives the names that passed, sorted.

```vyrn starter
create passed(scores, mark):
    return sorted([name each name, score in scores.items() if score >= mark])
end

main:
    print(passed({"Ada": 81, "Alan": 64, "Grace": 90}, 70))
end
```

```text expect
["Ada", "Grace"]
```

> Hint: `scores: map[str, int]`, `mark: int`, and the result is `list[str]`.

<details><summary>Solution</summary>

```vyrn solution
create passed(scores: map[str, int], mark: int) -> list[str]:
    return sorted([name each name, score in scores.items() if score >= mark])
end

main:
    print(passed({"Ada": 81, "Alan": 64, "Grace": 90}, 70))
end
```

</details>

## Today you learned

- Every run is checked first; `vyrn check` checks a whole project without
  running it.
- Types come from values; written types on parameters, results and `var`s
  are promises the checker enforces.
- `list[int]`, `map[str, float]`, `(int, str)`, `int?`, `(int) -> str` and
  `any` describe values; item types are checked too.
- An optional value must be checked (`if x != none`, `??`) before use.
- `strict create` functions are fully typed; type what others call, and
  make the busy parts strict.
