# Day 61: Enums, patterns and `none`

Many values can only be one of a few things. A traffic light is red, amber
or green. A payment is pending, paid or refunded. A shape is a circle *with
a radius* or a rectangle *with a width and height*. An **enum** says
exactly which forms a value can take, and `when` with **patterns** handles
each form, with the checker making sure none is forgotten. Today you also
finish the story of `none` from yesterday. The project is a parser that
turns typed commands into enums.

## 61.1 Enums

An `enum` lists its **variants**. You name one with the enum's name, a dot,
and the variant:

```vyrn
enum Light:
    Red
    Amber
    Green
end

create next(light: Light) -> Light:
    when light:
        is Light.Red: return Light.Green
        is Light.Green: return Light.Amber
        is Light.Amber: return Light.Red
    end
end

main:
    var light = Light.Red
    each _ in 0 until 4:
        print(light)
        light = next(light)
    end
    print(light == Light.Red)
end
```

```text
Light.Red
Light.Green
Light.Amber
Light.Red
false
```

Why not just use text, `"red"`? Because `"rde"` is also text: a typo would
be a silent bug. `Light.Rde` is an error before the program runs.

And if a `when` forgets a variant, the checker says which:

```vyrn
enum Light:
    Red
    Amber
    Green
end

create must_stop(light: Light) -> bool:
    when light:
        is Light.Red: return true
        is Light.Green: return false
    end
end

main:
    print(must_stop(Light.Amber))
end
```

```text
error: this `when` does not handle `Light.Amber`
```

Add a variant to an enum later, and the checker finds every `when` that
must now handle it. In a big program that's a lifesaver.

## 61.2 Variants that carry data

A variant can carry values, like a small struct. Each variant has its own:

```vyrn
enum Shape:
    Circle(radius: float)
    Rect(width: float, height: float)
    Point
end

create area(s: Shape) -> float:
    when s:
        is Shape.Circle(r): return 3.14159 * r * r
        is Shape.Rect(w, h): return w * h
        is Shape.Point: return 0.0
    end
end

main:
    let shapes = [Shape.Circle(1.0), Shape.Rect(2.0, 3.5), Shape.Point]
    each s in shapes:
        print(s, area(s))
    end
end
```

```text
Shape.Circle(radius=1.0) 3.14159
Shape.Rect(width=2.0, height=3.5) 7.0
Shape.Point 0.0
```

`is Shape.Rect(w, h)` does two things: checks the variant, and takes its
values out into `w` and `h`. This is what other languages call a *tagged
union*, *sum type* or *sealed class*: one type, several shapes, and the
compiler knows them all.

### Exercise

Add a `Triangle(base: float, height: float)` variant (area: half of base
times height) and handle it in `area`.

```vyrn starter
enum Shape:
    Circle(radius: float)
    Rect(width: float, height: float)
end

create area(s: Shape) -> float:
    when s:
        is Shape.Circle(r): return 3.14159 * r * r
        is Shape.Rect(w, h): return w * h
    end
end

main:
    print(area(Shape.Rect(2.0, 3.0)))
end
```

```text expect
6.0
12.0
```

> Hint: add the variant to the enum, then `is Shape.Triangle(b, h): return b * h / 2`.
> Hint: in `main`, also print `area(Shape.Triangle(4.0, 6.0))`.

<details><summary>Solution</summary>

```vyrn solution
enum Shape:
    Circle(radius: float)
    Rect(width: float, height: float)
    Triangle(base: float, height: float)
end

create area(s: Shape) -> float:
    when s:
        is Shape.Circle(r): return 3.14159 * r * r
        is Shape.Rect(w, h): return w * h
        is Shape.Triangle(b, h): return b * h / 2
    end
end

main:
    print(area(Shape.Rect(2.0, 3.0)))
    print(area(Shape.Triangle(4.0, 6.0)))
end
```

</details>

## 61.3 Patterns

`when` can match much more than enum variants. Each `is` line is a
**pattern**, tried top to bottom; the first that fits wins:

| Pattern | Matches |
|---|---|
| `is 0`, `is "hi"` | that exact value |
| `is 1 to 9` | a number from 1 to 9 |
| `is "y" or "yes"` | any of several |
| `is (x, y)` | a tuple of two, naming its parts |
| `is [first, rest...]` | a list with at least one item |
| `is []` | an empty list |
| `is Shape.Rect(w, h)` | that variant, naming its values |
| `is n if n > 100` | anything, but only if the **guard** after `if` is true |
| `is _` or `else:` | anything else |

```vyrn
create describe(x: any) -> str:
    when x:
        is 0: return "zero"
        is 1 to 9: return "between 1 and 9"
        is n if n is int and n < 0: return "negative"
        is "y" or "yes": return "agreement"
        is []: return "an empty list"
        is [only]: return "a list of just {only}"
        is [first, rest...]: return "a list starting {first}, {len(rest)} more"
        is (a, b): return "a pair: {a} and {b}"
        else: return "something else"
    end
end

main:
    each value in [0, 7, -3, "yes", [], [4], [1, 2, 3], ("x", 2), 12.5]:
        print(describe(value))
    end
end
```

```text
zero
between 1 and 9
negative
agreement
an empty list
a list of just 4
a list starting 1, 2 more
a pair: x and 2
something else
```

Order matters: `is 0` must come before a pattern that would also match 0.

### Exercise

Write `fizz(n)` with a single `when` on the tuple `(n % 3, n % 5)`:
`(0, 0)` is `"FizzBuzz"`, `(0, _)` is `"Fizz"`, `(_, 0)` is `"Buzz"`, and
anything else is the number as text.

```vyrn starter
create fizz(n: int) -> str:
    return str(n)
end

main:
    print([fizz(n) each n in 1 to 15])
end
```

```text expect
["1", "2", "Fizz", "4", "Buzz", "Fizz", "7", "8", "Fizz", "Buzz", "11", "Fizz", "13", "14", "FizzBuzz"]
```

> Hint: `when (n % 3, n % 5):` then `is (0, 0): return "FizzBuzz"`.
> Hint: `_` in a pattern matches anything without naming it.

<details><summary>Solution</summary>

```vyrn solution
create fizz(n: int) -> str:
    when (n % 3, n % 5):
        is (0, 0): return "FizzBuzz"
        is (0, _): return "Fizz"
        is (_, 0): return "Buzz"
        else: return str(n)
    end
end

main:
    print([fizz(n) each n in 1 to 15])
end
```

</details>

## 61.4 `if let`: one pattern

When you only care about one variant, `if let` tries a single pattern and
names its parts if it fits:

```vyrn
enum Reply:
    Found(value: int)
    Missing(reason: str)
end

main:
    each reply in [Reply.Found(42), Reply.Missing("no such user")]:
        if let Reply.Found(v) = reply:
            print("got", v)
        else:
            print("nothing:", reply)
        end
    end
end
```

```text
got 42
nothing: Reply.Missing(reason="no such user")
```

An enum like `Reply` is a good way to return "a value, or why there isn't
one": the caller can't use the value without first checking which it got.

## 61.5 Living with `none`

Yesterday you met `int?`: a value that may be `none`, which must be checked
before it's used. Three tools make the checking short:

| Write | Means |
|---|---|
| `if x != none:` | inside, `x` is known not to be none |
| `x ?? fallback` | `x`, or `fallback` when `x` is none |
| `x?.name`, `x?.method()` | `none` if `x` is none, otherwise `x.name` |

```vyrn
struct User:
    name: str
    email: str?
end

create find(users: list[User], name: str) -> User?:
    each u in users:
        if u.name == name: return u
    end
    return none
end

main:
    let users = [User("ada", "ada@mail.com"), User("alan", none)]
    print(find(users, "ada")?.email)
    print(find(users, "alan")?.email ?? "no email")
    print(find(users, "grace")?.email ?? "no such user")
    print(find(users, "grace")?.email?.upper())
end
```

```text
ada@mail.com
no email
no such user
none
```

`?.` stops at the first `none` and gives `none` for the whole chain; `??`
then turns it into something useful. Without them, each step would need its
own `if`.

### Exercise

Write `city_of(people, name)`, giving the person's city in capitals, or
`"UNKNOWN"` when there's no such person or they have no address. Use `?.`
and `??`, no `if`.

```vyrn starter
struct Address:
    city: str
end

struct Person:
    name: str
    address: Address?
end

create find(people: list[Person], name: str) -> Person?:
    each p in people:
        if p.name == name: return p
    end
    return none
end

create city_of(people: list[Person], name: str) -> str:
    return "UNKNOWN"
end

main:
    let people = [Person("ada", Address("London")), Person("alan", none)]
    print(city_of(people, "ada"), city_of(people, "alan"), city_of(people, "grace"))
end
```

```text expect
LONDON UNKNOWN UNKNOWN
```

> Hint: `find(people, name)?.address?.city?.upper() ?? "UNKNOWN"`

<details><summary>Solution</summary>

```vyrn solution
struct Address:
    city: str
end

struct Person:
    name: str
    address: Address?
end

create find(people: list[Person], name: str) -> Person?:
    each p in people:
        if p.name == name: return p
    end
    return none
end

create city_of(people: list[Person], name: str) -> str:
    return find(people, name)?.address?.city?.upper() ?? "UNKNOWN"
end

main:
    let people = [Person("ada", Address("London")), Person("alan", none)]
    print(city_of(people, "ada"), city_of(people, "alan"), city_of(people, "grace"))
end
```

</details>

## 61.6 Project: a command parser

Programs that take typed commands (a game, a chat bot, a tool) are much
easier to write in two steps: first **parse** the text into an enum, then
**act** on the enum. The parser deals with all the messy text; the rest of
the program only sees clean, checked values:

```vyrn
enum Command:
    Go(direction: str)
    Take(item: str)
    Drop(item: str)
    Look
    Quit
    Unknown(text: str)
end

create parse(line: str) -> Command:
    when line.lower().split():
        is ["go", d] if d in ["north", "south", "east", "west"]: return Command.Go(d)
        is ["n" or "s" or "e" or "w" as short]:
            return Command.Go({"n": "north", "s": "south", "e": "east", "w": "west"}[short])
        is ["take" or "get", item, more...]: return Command.Take(" ".join([item] + more))
        is ["drop", item]: return Command.Drop(item)
        is ["look"] or ["l"]: return Command.Look
        is ["quit"] or ["exit"]: return Command.Quit
        else: return Command.Unknown(line)
    end
end

create act(cmd: Command, bag: list[str]) -> str:
    when cmd:
        is Command.Go(d): return "You walk {d}."
        is Command.Take(item):
            bag.append(item)
            return "Taken: {item}."
        is Command.Drop(item):
            if item not in bag: return "You don't have {item}."
            bag.remove(item)
            return "Dropped: {item}."
        is Command.Look: return "You carry: {", ".join(bag) or "nothing"}."
        is Command.Quit: return "Bye!"
        is Command.Unknown(text): return "I don't understand '{text}'."
    end
end

main:
    var bag = []
    each line in ["go north", "w", "take old brass key", "look", "drop sword", "Dance", "quit"]:
        let cmd = parse(line)
        print("> {line:<20} {cmd}")
        print("  " + act(cmd, bag))
    end
end
```

```text
> go north             Command.Go(direction="north")
  You walk north.
> w                    Command.Go(direction="west")
  You walk west.
> take old brass key   Command.Take(item="old brass key")
  Taken: old brass key.
> look                 Command.Look
  You carry: old brass key.
> drop sword           Command.Drop(item="sword")
  You don't have sword.
> Dance                Command.Unknown(text="Dance")
  I don't understand 'Dance'.
> quit                 Command.Quit
  Bye!
```

`is ["n" or "s" or "e" or "w" as short]` matches a one-word list whose word
is any of four, and names it `short`. `is ["take" or "get", item, more...]`
takes the first word of the item and the rest. Adding a command is: one
variant, one parse pattern, one `act` case; and if you forget the `act`
case, the checker tells you.

### Exercise

Add an `Inventory` command, typed as `i` or `inventory`, that prints how
many items are in the bag. Write just the new `parse` pattern and `act` case
in these cut-down versions.

```vyrn starter
enum Command:
    Look
    Unknown(text: str)
end

create parse(line: str) -> Command:
    when line.lower().split():
        is ["look"]: return Command.Look
        else: return Command.Unknown(line)
    end
end

create act(cmd: Command, bag: list[str]) -> str:
    when cmd:
        is Command.Look: return "You look around."
        is Command.Unknown(text): return "I don't understand '{text}'."
    end
end

main:
    each line in ["i", "inventory", "look"]:
        print(act(parse(line), ["key", "lamp"]))
    end
end
```

```text expect
You carry 2 items.
You carry 2 items.
You look around.
```

> Hint: a variant `Inventory`; the pattern `is ["i"] or ["inventory"]`.
> Hint: `is Command.Inventory: return "You carry {len(bag)} items."`

<details><summary>Solution</summary>

```vyrn solution
enum Command:
    Look
    Inventory
    Unknown(text: str)
end

create parse(line: str) -> Command:
    when line.lower().split():
        is ["look"]: return Command.Look
        is ["i"] or ["inventory"]: return Command.Inventory
        else: return Command.Unknown(line)
    end
end

create act(cmd: Command, bag: list[str]) -> str:
    when cmd:
        is Command.Look: return "You look around."
        is Command.Inventory: return "You carry {len(bag)} items."
        is Command.Unknown(text): return "I don't understand '{text}'."
    end
end

main:
    each line in ["i", "inventory", "look"]:
        print(act(parse(line), ["key", "lamp"]))
    end
end
```

</details>

## Today you learned

- An `enum` lists the forms a value can take; variants can carry data.
- `when` with `is` patterns: values, ranges, `or`, tuples, lists with
  `rest...`, variants with their data, `as` names and `if` guards. The
  checker finds variants a `when` forgets.
- `if let` tries one pattern.
- `?.` and `??` handle values that may be `none` without a pile of `if`s.
- Parse text into an enum first, then act on the enum.
