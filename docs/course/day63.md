# Day 63: Types that feel built in

`1 + 2`, `"a" < "b"`, `len(xs)`, `x in xs`, `each x in xs`, `print(x)`:
the built-in types work with all of these. Your own types can too. Today
you give a type **operators**, a way to **print** itself, an **order** so
it can be sorted, and the methods that make it behave like a **collection**.
The project is a `Money` type that adds up correctly and a `Playlist` that
works like a list.

## 63.1 Operators

On Day 17 you met `operator +`. Any of the arithmetic and comparison
operators can be defined the same way, as a method named `operator` and
the symbol:

```vyrn
struct Vec2:
    x: float
    y: float

    create operator +(self, o: Vec2) -> Vec2:
        return Vec2(self.x + o.x, self.y + o.y)
    end
    create operator -(self, o: Vec2) -> Vec2:
        return Vec2(self.x - o.x, self.y - o.y)
    end
    create operator *(self, k: float) -> Vec2:
        return Vec2(self.x * k, self.y * k)
    end
    create operator -(self) -> Vec2:
        return Vec2(-self.x, -self.y)
    end
end

main:
    let a = Vec2(1.0, 2.0)
    let b = Vec2(3.0, 5.0)
    print(a + b, b - a, a * 3, 3 * a, -a)
end
```

```text
Vec2(x=4.0, y=7.0) Vec2(x=2.0, y=3.0) Vec2(x=3.0, y=6.0) Vec2(x=3.0, y=6.0) Vec2(x=-1.0, y=-2.0)
```

- `operator -` with one other parameter is subtraction; with only `self`
  it is negation, `-a`.
- `3 * a` works although `3` is a number: for `+` and `*`, if the left
  side has no operator, Vyrn tries the right side's.

| Operator | Method |
|---|---|
| `a + b`, `a - b`, `a * b`, `a / b`, `a // b`, `a % b`, `a ** b` | `operator +(self, other)`, ... |
| `-a` | `operator -(self)` |
| `a == b`, `a != b` | `operator ==(self, other)` |
| `a < b`, `a > b`, `a <= b`, `a >= b` | `operator <(self, other)` gives all four |
| `a[i]` | `operator [](self, i)` |

Only define an operator when its meaning is obvious. `money + money` is
obvious; `user + user` is not, and a method with a name (`merge`) is
clearer.

## 63.2 Printing: `to_text`

`print`, `str()` and `{...}` in strings show a struct with all its fields.
Give it a `to_text` method and they use that instead:

```vyrn
struct Money:
    pence: int

    create to_text(self) -> str:
        let sign = "-" if self.pence < 0 else ""
        let p = abs(self.pence)
        return "{sign}£{p // 100}.{p % 100:02}"
    end
end

main:
    let price = Money(1999)
    print(price)
    print("Total: {price}, refund: {Money(-250)}")
    print([Money(5), Money(100)], str(price) == "£19.99")
end
```

```text
£19.99
Total: £19.99, refund: -£2.50
[£0.05, £1.00] true
```

Why store pence instead of pounds as a `float`? Because floats can't hold
most decimals exactly: `0.1 + 0.2` is `0.30000000000000004`. Money in
whole pence (or cents) is always exact. Every bank works this way.

### Exercise

Give `Duration` (a number of seconds) a `to_text` showing hours, minutes
and seconds like `1h 02m 05s`, leaving out the hours when there are none.

```vyrn starter
struct Duration:
    seconds: int
end

main:
    print(Duration(3725), Duration(65), Duration(0))
end
```

```text expect
1h 02m 05s 1m 05s 0m 00s
```

> Hint: hours `s // 3600`, minutes `s % 3600 // 60`, seconds `s % 60`.
> Hint: `{m:02}` writes two digits; `1m` has no leading zero when there
> are no hours, so build the text in two cases.

<details><summary>Solution</summary>

```vyrn solution
struct Duration:
    seconds: int

    create to_text(self) -> str:
        let h = self.seconds // 3600
        let m = self.seconds % 3600 // 60
        let s = self.seconds % 60
        if h > 0: return "{h}h {m:02}m {s:02}s"
        return "{m}m {s:02}s"
    end
end

main:
    print(Duration(3725), Duration(65), Duration(0))
end
```

</details>

## 63.3 Equality and order

Two structs are equal when all their fields are equal, without you doing
anything. Define `operator ==` when equality means something else, such as
names that should match whatever their case. Define `operator <` and your
type can be compared, sorted, and used with `min` and `max`:

```vyrn
struct Version:
    major: int
    minor: int
    patch: int

    create operator <(self, o: Version) -> bool:
        return (self.major, self.minor, self.patch) < (o.major, o.minor, o.patch)
    end
    create to_text(self) -> str:
        return "{self.major}.{self.minor}.{self.patch}"
    end
end

create parse(text: str) -> Version:
    let parts = [int(p) each p in text.split(".")]
    return Version(parts[0], parts[1], parts[2])
end

main:
    let versions = [parse(t) each t in ["1.10.0", "1.2.3", "1.2.10", "0.9.9"]]
    print(sorted(versions))
    print(max(versions), parse("1.2.3") < parse("1.10.0"), parse("2.0.0") >= parse("1.99.99"))
    print(sorted(["1.10.0", "1.2.3", "1.2.10"]))
end
```

```text
[0.9.9, 1.2.3, 1.2.10, 1.10.0]
1.10.0 true true
["1.10.0", "1.2.10", "1.2.3"]
```

The last line shows why the type is needed: sorted as text, `"1.10.0"`
comes before `"1.2.3"` because the character `1` comes before `2`. As
`Version`s they sort by number. Tuples compare item by item, which makes
`<` a one-liner.

Equal values must give the same key in a map: two structs with equal
fields are the same key, so structs work in sets and as map keys:

```vyrn
struct Cell:
    x: int
    y: int
end

main:
    let visited = {Cell(0, 0), Cell(1, 0), Cell(0, 0)}
    print(len(visited), Cell(1, 0) in visited)
    let names = {Cell(0, 0): "home"}
    print(names[Cell(0, 0)])
end
```

```text
2 true
home
```

### Exercise

Give `Card` an order: by rank (2 to 14, where 14 is an ace), and for equal
ranks by suit in the order clubs, diamonds, hearts, spades.

```vyrn starter
const SUITS = ["clubs", "diamonds", "hearts", "spades"]

struct Card:
    rank: int
    suit: str
end

main:
    let hand = [Card(14, "clubs"), Card(2, "spades"), Card(14, "hearts"), Card(9, "diamonds")]
    print([c.rank each c in hand])
end
```

```text expect
[(2, "spades"), (9, "diamonds"), (14, "clubs"), (14, "hearts")]
true
```

> Hint: `operator <` compares `(self.rank, SUITS.index(self.suit))` with
> the other card's tuple.
> Hint: print `[(c.rank, c.suit) each c in sorted(hand)]`, then
> `Card(14, "spades") > Card(14, "hearts")`.

<details><summary>Solution</summary>

```vyrn solution
const SUITS = ["clubs", "diamonds", "hearts", "spades"]

struct Card:
    rank: int
    suit: str

    create operator <(self, o: Card) -> bool:
        return (self.rank, SUITS.index(self.suit)) < (o.rank, SUITS.index(o.suit))
    end
end

main:
    let hand = [Card(14, "clubs"), Card(2, "spades"), Card(14, "hearts"), Card(9, "diamonds")]
    print([(c.rank, c.suit) each c in sorted(hand)])
    print(Card(14, "spades") > Card(14, "hearts"))
end
```

</details>

## 63.4 Types that act like collections

Three methods make an object work with `len`, `in` and `each`:

| Write | Calls |
|---|---|
| `len(x)` | `x.len()` |
| `item in x` | `x.contains(item)` |
| `each item in x` | `x.iter()`, which gives a list, or `yield`s each item |

```vyrn
object Playlist:
    name: str
    songs: list[str] = []

    create add(self, song: str):
        if song not in self.songs: self.songs.append(song)
    end
    create len(self) -> int:
        return len(self.songs)
    end
    create contains(self, song: str) -> bool:
        return song.lower() in [s.lower() each s in self.songs]
    end
    create iter(self):
        each i, song in enumerate(self.songs):
            yield "{i + 1}. {song}"
        end
    end
    create operator [](self, i: int) -> str:
        return self.songs[i]
    end
end

main:
    let p = Playlist("Road trip")
    p.add("Bohemian Rhapsody")
    p.add("Africa")
    p.add("Africa")
    print(len(p), "AFRICA" in p, "Hey Jude" in p, p[0])
    each line in p:
        print(line)
    end
    print([s each s in p])
end
```

```text
2 true false Bohemian Rhapsody
1. Bohemian Rhapsody
2. Africa
["1. Bohemian Rhapsody", "2. Africa"]
```

The playlist decides what "in" means (any case) and what looping gives
(numbered lines), and the code using it reads like it's using a list.

## 63.5 Project: money that adds up

A shop's basket in `Money`: adding, multiplying by a quantity, comparing,
summing a list, and printing, all reading like ordinary arithmetic:

```vyrn
struct Money:
    pence: int

    create operator +(self, o: Money) -> Money:
        return Money(self.pence + o.pence)
    end
    create operator -(self, o: Money) -> Money:
        return Money(self.pence - o.pence)
    end
    create operator *(self, k: int) -> Money:
        return Money(self.pence * k)
    end
    create operator <(self, o: Money) -> bool:
        return self.pence < o.pence
    end
    create percent(self, p: int) -> Money:
        return Money(round(self.pence * p / 100))
    end
    create to_text(self) -> str:
        let sign = "-" if self.pence < 0 else ""
        let p = abs(self.pence)
        return "{sign}£{p // 100}.{p % 100:02}"
    end
end

create pounds(text: str) -> Money:
    let parts = text.split(".")
    let pence = int(parts[1].ljust(2, "0")[:2]) if len(parts) > 1 else 0
    return Money(int(parts[0]) * 100 + pence)
end

main:
    let basket = [(pounds("3.50"), 2), (pounds("12.99"), 1), (pounds("0.45"), 6)]
    let lines = [price * qty each price, qty in basket]
    let subtotal = sum(lines)
    let discount = subtotal.percent(10) if subtotal > pounds("20") else Money(0)
    print("lines:", lines)
    print("subtotal {subtotal}, discount {discount}, total {subtotal - discount}")
    print("most expensive line:", max(lines))
end
```

```text
lines: [£7.00, £12.99, £2.70]
subtotal £22.69, discount £2.27, total £20.42
most expensive line: £12.99
```

`sum(lines)` works because `Money` has `operator +`; `max` and the `>` in
the discount rule work because it has `operator <`. Each price is exact to
the penny, whatever the sums.

### Exercise

Split a bill fairly: `split(total, people)` gives a list of `Money`, one per
person, that adds up to exactly the total. When it doesn't divide evenly,
the first people pay a penny more.

```vyrn starter
struct Money:
    pence: int

    create to_text(self) -> str:
        return "£{self.pence // 100}.{self.pence % 100:02}"
    end
end

create split(total: Money, people: int) -> list[Money]:
    return []
end

main:
    print(split(Money(1000), 3))
    print(split(Money(1000), 4))
end
```

```text expect
[£3.34, £3.33, £3.33]
[£2.50, £2.50, £2.50, £2.50]
```

> Hint: each pays `total.pence // people`, and the first
> `total.pence % people` people pay one more.

<details><summary>Solution</summary>

```vyrn solution
struct Money:
    pence: int

    create to_text(self) -> str:
        return "£{self.pence // 100}.{self.pence % 100:02}"
    end
end

create split(total: Money, people: int) -> list[Money]:
    let share = total.pence // people
    let extra = total.pence % people
    return [Money(share + (1 if i < extra else 0)) each i in 0 until people]
end

main:
    print(split(Money(1000), 3))
    print(split(Money(1000), 4))
end
```

</details>

## Today you learned

- `operator +`, `-`, `*`, `==`, `<`, `[]` and the others make your types
  work with symbols; `operator -(self)` is negation.
- `to_text` decides how a value prints.
- Structs are equal when their fields are; `operator <` gives sorting,
  `min`, `max` and every comparison. Structs can be map keys.
- `len`, `contains` and `iter` methods make an object work with `len`, `in`
  and `each`.
- Keep money in whole pence, never in floats.
