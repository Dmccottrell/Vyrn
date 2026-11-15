# Day 67: Values and sharing

Some of the hardest bugs in any language come from one question: *when I
change this, what else changes?* Change a list inside a function, and the
caller's list changes too. Copy a struct and change the copy, and the
original doesn't. Today you learn exactly which values are **copied** and
which are **shared** in Vyrn, how `mut` and `move` make sharing visible in
the code, and how Vyrn frees memory without a garbage collector. The
project is a hunt for four sharing bugs.

## 67.1 Two kinds of value

Every value in Vyrn is one of two kinds:

| Kind | Examples | `b = a` makes |
|---|---|---|
| **values** | numbers, `bool`, `str`, tuples, `struct`s, enums | a separate copy |
| **shared** | lists, maps, sets, `object`s | a second name for the *same* thing |

```vyrn
struct Point:
    x: int
end

object Counter:
    n: int
end

main:
    var a = Point(1)
    var b = a
    b.x = 99
    print("struct:", a.x, b.x)

    let c = Counter(1)
    let d = c
    d.n = 99
    print("object:", c.n, d.n)

    let xs = [1, 2]
    let ys = xs
    ys.append(3)
    print("list:", xs, ys)
end
```

```text
struct: 1 99
object: 99 99
list: [1, 2, 3] [1, 2, 3]
```

`b` is a copy of the point, so changing it leaves `a` alone. But `d` and
`c` are two names for one counter, and `ys` and `xs` for one list: change
it through either name and both see it.

Why have both? Copying is safe but costs time for big things; sharing is
cheap but you must know it's happening. Small, simple data (a point, a
colour, a date) is best as a `struct`. Things with an identity that
several parts of a program refer to (a player, a bank account, a
connection) are best as `object`s.

Strings are values even though they can be long: you can't change a
string in place (`s.upper()` gives a new one), so sharing one is never
visible.

## 67.2 Copying on purpose

When you want an independent list, ask for one with `.copy()`, or build a
new one:

```vyrn
main:
    let original = [3, 1, 2]
    let sorted_copy = sorted(original)
    let copy = original.copy()
    copy.append(4)
    print(original, sorted_copy, copy)

    let grid = [[0, 0], [0, 0]]
    let shallow = grid.copy()
    shallow[0][0] = 7
    print(grid)
    let deep = [row.copy() each row in grid]
    deep[1][1] = 9
    print(grid, deep)
end
```

```text
[3, 1, 2] [1, 2, 3] [3, 1, 2, 4]
[[7, 0], [0, 0]]
[[7, 0], [0, 0]] [[7, 0], [0, 9]]
```

`.copy()` is **shallow**: it makes a new outer list, but the rows inside
are still shared, so `shallow[0][0] = 7` changed `grid` too. To copy all
the way down, copy each level, as `deep` does.

### Exercise

`add_bonus` should give a *new* map of scores with 5 added to each, leaving
the original alone. It changes the original. Fix it.

```vyrn starter
create add_bonus(scores: map) -> map:
    let result = scores
    each name in result.keys():
        result[name] += 5
    end
    return result
end

main:
    let scores = {"ada": 10, "alan": 20}
    let boosted = add_bonus(scores)
    print(scores, boosted)
end
```

```text expect
{"ada": 10, "alan": 20} {"ada": 15, "alan": 25}
```

> Hint: `let result = scores.copy()`, or build a new map:
> `{name: s + 5 each name, s in scores.items()}`.

<details><summary>Solution</summary>

```vyrn solution
create add_bonus(scores: map) -> map:
    return {name: s + 5 each name, s in scores.items()}
end

main:
    let scores = {"ada": 10, "alan": 20}
    let boosted = add_bonus(scores)
    print(scores, boosted)
end
```

</details>

## 67.3 `mut`: changing what you're given

Passing a value to a function works like `b = a`: structs are copied in,
lists and objects are shared. A function that changes its argument must
say so with `mut`, and so must the caller, so a reader can see at the call
that something will change:

```vyrn
struct Point:
    x: int
    y: int
end

create shift(mut p: Point, dx: int):
    p.x += dx
end

create add_item(mut basket: list[str], item: str):
    basket.append(item)
end

main:
    var p = Point(1, 1)
    shift(mut p, 10)
    var basket = ["tea"]
    add_item(mut basket, "cake")
    print(p, basket)
end
```

```text
Point(x=11, y=1) ["tea", "cake"]
```

Forget the `mut` at the call, and the checker points it out:

```vyrn
create add_item(mut basket: list[str], item: str):
    basket.append(item)
end

main:
    var basket = ["tea"]
    add_item(basket, "cake")
end
```

```text
error: `add_item` changes its argument `basket`; pass it with `mut`
```

Two more rules keep `mut` safe. A `let` can't be passed as `mut` (it's
promised not to change), and a value passed as `mut` can't also be passed
another way in the same call, because then the function would see it
change under its feet:

```vyrn
create append_len(mut a: list, b: list):
    a.append(len(b))
end

main:
    var xs = [1, 2]
    append_len(mut xs, xs)
end
```

```text
error: `xs` is passed as `mut` and also used elsewhere in this call
```

## 67.4 `own` and `move`: giving things away

Sometimes a function should *keep* what it's given: a queue that stores
jobs, a cache that stores results. Its parameter is `own`, and the caller
says `move` to hand the value over. After that, the caller can't use it:
it belongs to someone else now.

```vyrn
object Queue:
    jobs: list = []

    create add(self, own job: list[str]):
        self.jobs.append(job)
    end
end

main:
    let q = Queue()
    var job = ["print", "report.pdf"]
    q.add(move job)
    print(q.jobs)
    print(job)
end
```

```text
error: `job` is used after it was moved
```

Without the rule, the caller could keep changing `job` and silently change
the job inside the queue. If you still need it, give away a copy:
`q.add(job.copy())`.

### Exercise

Make the program compile: the caller should keep using its list after
saving it, so it must give away a copy, not the list itself.

```vyrn starter
object History:
    saved: list = []

    create save(self, own items: list[int]):
        self.saved.append(items)
    end
end

main:
    let h = History()
    var items = [1, 2]
    h.save(items.copy())
    items.append(3)
    print(items, h.saved)
end
```

```text expect
[1, 2, 3] [[1, 2]]
```

> Hint: it already compiles! `items.copy()` is a new list, so there's
> nothing to `move`. Check the output matches, and compare with what
> `move items` would do.

<details><summary>Solution</summary>

```vyrn solution
object History:
    saved: list = []

    create save(self, own items: list[int]):
        self.saved.append(items)
    end
end

main:
    let h = History()
    var items = [1, 2]
    h.save(items.copy())
    items.append(3)
    print(items, h.saved)
end
```

</details>

## 67.5 Where memory goes

You never free memory yourself in Vyrn. Values (numbers, structs) live
where they're stored and vanish with it. Shared things (lists, maps,
objects) are **reference counted**: each knows how many names refer to it,
and is freed the moment that number reaches zero.

| Language | How memory is freed |
|---|---|
| C | by hand, with `free`; forgetting leaks, freeing twice crashes |
| C++, Rust | when the owner goes out of scope (RAII) |
| Python, Swift, Vyrn | reference counting: when the last reference goes |
| Java, JavaScript, Go | a garbage collector pauses now and then to find unused memory |

Reference counting frees memory at a predictable moment, with no pauses,
which matters for games and servers. Its one weakness: objects that refer
to each other in a ring keep each other alive. A tree where each child
points back to its parent is such a ring. Keep references pointing one
way (parents to children), and find a parent by walking down from the top
when you need it.

## 67.6 Project: four sharing bugs

Each program below has a sharing bug. Read it, predict the output, then
check. The fix is in the text after each.

**Bug 1: one row, three times.**

```vyrn
main:
    let row = [0, 0, 0]
    var grid = []
    each _ in 0 until 3:
        grid.append(row)
    end
    grid[0][0] = 1
    print(grid)
end
```

```text
[[1, 0, 0], [1, 0, 0], [1, 0, 0]]
```

All three rows are the same list. Append `row.copy()`, or build each row
fresh: `[[0, 0, 0] each _ in 0 until 3]`.

**Bug 2: the default that remembers.**

```vyrn
object Team:
    name: str
    members: list = []
end

main:
    let a = Team("red")
    let b = Team("blue")
    a.members.append("ada")
    print(a.members, b.members)
end
```

```text
["ada"] []
```

No bug! Vyrn makes a fresh `[]` for every object. (In Python, a list
default in a function is created once and shared by every call, a classic
trap that Vyrn doesn't have.)

**Bug 3: the struct that didn't change.**

```vyrn
struct Player:
    name: str
    score: int
end

main:
    var players = [Player("ada", 0), Player("alan", 0)]
    each p in players:
        p.score += 10
    end
    print([p.score each p in players])
end
```

```text
[0, 0]
```

`p` is a copy of each struct, so the loop changes copies. Change the list
itself: `each i in 0 until len(players): players[i].score += 10`, or make
`Player` an `object` if players are things with an identity.

**Bug 4: the shared result.**

```vyrn
create clean(names: list[str]) -> list[str]:
    each i in 0 until len(names):
        names[i] = names[i].strip().title()
    end
    return names
end

main:
    let raw = ["  ada ", "ALAN"]
    let tidy = clean(raw)
    print(tidy, raw)
end
```

```text
["Ada", "Alan"] ["Ada", "Alan"]
```

`clean` changed the caller's list: `tidy` and `raw` are the same list.
An ordinary function may change a shared argument without saying so; a
`strict create` (Day 60) may not, and the checker would have said "cannot
change `names`: strict code takes parameters read-only". Build a new list
instead, which is clearer anyway: `return [n.strip().title() each n in names]`.

### Exercise

Fix bug 3 by making `Player` an object, so that the loop changes the real
players.

```vyrn starter
struct Player:
    name: str
    score: int
end

main:
    var players = [Player("ada", 0), Player("alan", 0)]
    each p in players:
        p.score += 10
    end
    print([p.score each p in players])
end
```

```text expect
[10, 10]
```

> Hint: change `struct` to `object`. Nothing else.

<details><summary>Solution</summary>

```vyrn solution
object Player:
    name: str
    score: int
end

main:
    var players = [Player("ada", 0), Player("alan", 0)]
    each p in players:
        p.score += 10
    end
    print([p.score each p in players])
end
```

</details>

## Today you learned

- Numbers, strings, tuples, structs and enums are copied; lists, maps,
  sets and objects are shared.
- `.copy()` makes a shallow copy; copy each level for a deep one.
- `mut` at the parameter *and* the call shows a function changes its
  argument; `let`s can't be passed as `mut`.
- `own` parameters keep what they're given; the caller writes `move`, and
  can't use it afterwards.
- Memory is freed by reference counting, when the last reference goes;
  avoid rings of objects referring to each other.
