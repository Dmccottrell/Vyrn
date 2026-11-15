# Day 62: Generics and traits

Two problems come up in every growing program. First: you write a function
for a list of ints, then need the same one for a list of names. Second:
different types should be usable in the same way (circles and squares both
have an area; files and web pages can both be read) without one being a
kind of the other. **Generics** solve the first, **traits** the second.
The project is a little drawing program where shapes of any kind can be
listed, sorted and summed.

## 62.1 Generic functions

A function that works the same for any type of item takes a **type
parameter**, written in square brackets after its name. `T` stands for
"whatever type the caller uses":

```vyrn
create first[T](xs: list[T]) -> T:
    return xs[0]
end

create last_two[T](xs: list[T]) -> list[T]:
    return xs[-2:]
end

main:
    let n = first([10, 20, 30])
    let word = first(["ada", "alan"])
    print(n + 1, word.upper(), last_two([1.5, 2.5, 3.5]))
end
```

```text
11 ADA [2.5, 3.5]
```

Why not `xs: list` and `-> any`? Because then the checker forgets what
comes out: `first([10, 20])` would be `any`, and `first([10, 20]).upper()`
would only fail when it ran. With `T`, the checker knows `first` of a list
of ints *is* an int:

```vyrn
create first[T](xs: list[T]) -> T:
    return xs[0]
end

main:
    print(first([10, 20]).upper())
end
```

```text
error: int has no method `upper`
```

Every `T` in one call must be the same type, so a function can promise
that what comes out matches what went in, or that two arguments agree:

```vyrn
create pick[T](a: T, b: T, take_first: bool) -> T:
    return a if take_first else b
end

main:
    print(pick(1, 2, true), pick("x", "y", false))
    print(pick(1, "two", true))
end
```

```text
error: `T` in `pick` stands for int from an earlier argument, but this is str
```

Several type parameters are separated by commas. Here `T` is what goes in
and `U` what comes out:

```vyrn
create map_all[T, U](xs: list[T], f: (T) -> U) -> list[U]:
    return [f(x) each x in xs]
end

main:
    let lengths = map_all(["ada", "grace"], w => len(w))
    print(lengths, sum(lengths))
end
```

```text
[3, 5] 8
```

### Exercise

Write a generic `pairs[T](xs)` that gives each item with the next one, as
tuples: `[1, 2, 3]` gives `[(1, 2), (2, 3)]`.

```vyrn starter
create pairs(xs: list) -> list:
    return []
end

main:
    print(pairs([1, 2, 3]), pairs(["a", "b"]), pairs([7]))
end
```

```text expect
[(1, 2), (2, 3)] [("a", "b")] []
```

> Hint: `create pairs[T](xs: list[T]) -> list[(T, T)]:`
> Hint: `[(xs[i], xs[i + 1]) each i in 0 until len(xs) - 1]`

<details><summary>Solution</summary>

```vyrn solution
create pairs[T](xs: list[T]) -> list[(T, T)]:
    return [(xs[i], xs[i + 1]) each i in 0 until len(xs) - 1]
end

main:
    print(pairs([1, 2, 3]), pairs(["a", "b"]), pairs([7]))
end
```

</details>

## 62.2 Generic types

Structs and objects can take type parameters too. A `Stack[T]` holds items
of one type, whichever the program chooses:

```vyrn
object Stack[T]:
    items: list[T] = []

    create push(self, item: T):
        self.items.append(item)
    end

    create pop(self) -> T?:
        if not self.items: return none
        return self.items.pop()
    end

    create size(self) -> int:
        return len(self.items)
    end
end

main:
    let s = Stack()
    s.push("first")
    s.push("second")
    print(s.pop(), s.size(), s.pop(), s.pop())
end
```

```text
second 1 first none
```

The standard library's lists and maps are generic in the same way, which
is why `list[int]` and `map[str, float]` are written with square brackets.

## 62.3 Traits

A **trait** is a set of methods a type promises to have. It says *what* a
type can do, not how. `impl Trait for Type` keeps the promise, with the
type's own version of each method:

```vyrn
trait Shape:
    create area(self) -> float
    create name(self) -> str
end

struct Circle:
    radius: float
end

struct Rect:
    width: float
    height: float
end

impl Shape for Circle:
    create area(self) -> float:
        return 3.14159 * self.radius * self.radius
    end
    create name(self) -> str:
        return "circle"
    end
end

impl Shape for Rect:
    create area(self) -> float:
        return self.width * self.height
    end
    create name(self) -> str:
        return "rectangle"
    end
end

create report(s: Shape) -> str:
    return "a {s.name()} of area {s.area():.2f}"
end

main:
    let shapes: list[Shape] = [Circle(1.0), Rect(2.0, 3.0)]
    each s in shapes:
        print(report(s))
    end
    print(Circle(2.0) is Shape, "circle" is Shape)
end
```

```text
a circle of area 3.14
a rectangle of area 6.00
true false
```

`report` takes any `Shape`: it doesn't know or care which. Adding a
`Triangle` later needs no change to `report`, only a new `impl`. This is
**polymorphism**: one piece of code working with many types.

A trait is also a contract the checker enforces. Forget a method, and the
`impl` doesn't compile:

```vyrn
trait Shape:
    create area(self) -> float
end

struct Square:
    side: float
end

impl Shape for Square:
end

main:
    print(Square(2.0))
end
```

```text
error: `Square` must have `area` to be a `Shape`
```

Pass something that isn't a `Shape` where one is wanted, and that's caught
too:

```vyrn
trait Shape:
    create area(self) -> float
end

create report(s: Shape) -> str:
    return "area {s.area()}"
end

main:
    print(report("a square"))
end
```

```text
error: argument `s` of `report` is Shape, but this is str
```

## 62.4 Default methods

A trait can give a method a body: a **default** that every type gets for
free, and can replace with its own. Defaults are often built from the
required methods:

```vyrn
trait Shape:
    create area(self) -> float
    create name(self) -> str:
        return "shape"
    end
    create describe(self) -> str:
        return "{self.name()} with area {self.area():.1f}"
    end
end

struct Square:
    side: float
end

struct Circle:
    radius: float
end

impl Shape for Square:
    create area(self) -> float:
        return self.side * self.side
    end
end

impl Shape for Circle:
    create area(self) -> float:
        return 3.14159 * self.radius * self.radius
    end
    create name(self) -> str:
        return "circle"
    end
end

main:
    print(Square(3.0).describe())
    print(Circle(1.0).describe())
end
```

```text
shape with area 9.0
circle with area 3.1
```

`Square` wrote only `area` and got `name` and `describe`. `Circle` replaced
`name`, and `describe` used its version. In Java and C# this is an
**interface with default methods**, or an **abstract class**; in Python,
an abstract base class.

### Exercise

Add a required `perimeter` method to `Shape`, and a default method
`summary(self)` giving `"area A, perimeter P"` with both to one decimal
place. Then give `Rect` its `perimeter`.

```vyrn starter
trait Shape:
    create area(self) -> float
end

struct Rect:
    w: float
    h: float
end

impl Shape for Rect:
    create area(self) -> float:
        return self.w * self.h
    end
end

main:
    print(Rect(2.0, 3.0).area())
end
```

```text expect
area 6.0, perimeter 10.0
```

> Hint: in the trait, `create perimeter(self) -> float` (no body), and
> `create summary(self) -> str:` with a body using `self.area()` and
> `self.perimeter()`.
> Hint: in `main`, `print(Rect(2.0, 3.0).summary())`.

<details><summary>Solution</summary>

```vyrn solution
trait Shape:
    create area(self) -> float
    create perimeter(self) -> float
    create summary(self) -> str:
        return "area {self.area():.1f}, perimeter {self.perimeter():.1f}"
    end
end

struct Rect:
    w: float
    h: float
end

impl Shape for Rect:
    create area(self) -> float:
        return self.w * self.h
    end
    create perimeter(self) -> float:
        return 2 * (self.w + self.h)
    end
end

main:
    print(Rect(2.0, 3.0).summary())
end
```

</details>

## 62.5 Bounds: generics that need a trait

A generic function can ask that its `T` implement a trait: `T: Shape`
reads "any type, as long as it's a shape". Inside, it may then use the
trait's methods; at each call, the checker makes sure the promise holds:

```vyrn
trait Shape:
    create area(self) -> float
end

struct Square:
    side: float
end

impl Shape for Square:
    create area(self) -> float:
        return self.side * self.side
    end
end

create largest[T: Shape](shapes: list[T]) -> T:
    return max(shapes, key = s => s.area())
end

main:
    print(largest([Square(1.0), Square(3.0), Square(2.0)]))
    print(largest([4, 9]))
end
```

```text
error: `T` in `largest` must be a `Shape`, but here it is int
```

The difference from `shapes: list[Shape]`: with `T`, the result keeps its
exact type. `largest` of a list of squares *is* a `Square`, so you can use
`.side` on it; with `list[Shape]` you'd only know it was some shape.

## 62.6 Traits or inheritance?

On Day 21 you made `object Dog(Animal)`: a dog *is a kind of* animal and
gets everything an animal has. Traits are different: a type *can do*
something. Use inheritance when types really are kinds of one another and
share data; use traits when unrelated types should share behaviour. A type
can implement many traits but has only one parent, so traits mix better:
a `Song` and a `Podcast` can both be `Playable` and `Downloadable` without
being related.

## 62.7 Project: a drawing of shapes

Every shape can report its area and draw itself as text. The `Drawing`
holds any shapes, and works with them only through the trait:

```vyrn
trait Shape:
    create area(self) -> float
    create draw(self) -> list[str]
    create label(self) -> str:
        return "shape"
    end
end

struct Square:
    side: int
end

struct Triangle:
    height: int
end

impl Shape for Square:
    create area(self) -> float:
        return float(self.side * self.side)
    end
    create draw(self) -> list[str]:
        return ["#" * (self.side * 2) each _ in 0 until self.side]
    end
    create label(self) -> str:
        return "square {self.side}"
    end
end

impl Shape for Triangle:
    create area(self) -> float:
        return self.height * self.height / 2
    end
    create draw(self) -> list[str]:
        return ["*" * (2 * i + 1) each i in 0 until self.height]
    end
end

object Drawing:
    shapes: list[Shape] = []

    create add(self, s: Shape):
        self.shapes.append(s)
    end

    create total_area(self) -> float:
        return sum([s.area() each s in self.shapes])
    end

    create render(self) -> str:
        var lines = []
        each s in sorted(self.shapes, key = s => s.area()):
            lines.append("{s.label()} ({s.area()}):")
            lines.extend(s.draw())
        end
        return "\n".join(lines)
    end
end

main:
    let d = Drawing()
    d.add(Square(3))
    d.add(Triangle(3))
    d.add(Square(2))
    print(d.render())
    print("total area", d.total_area())
end
```

```text
square 2 (4.0):
####
####
shape (4.5):
*
***
*****
square 3 (9.0):
######
######
######
total area 17.5
```

`Drawing` never mentions `Square` or `Triangle`: it sorts, sums and draws
through the trait alone. `Triangle` didn't write `label`, so it got the
default, `"shape"`. A new kind of shape is one `struct` and one `impl`;
`Drawing` doesn't change.

### Exercise

Add a `Line` shape, `Line(length)`, drawn as one row of `-`, with area 0
and the label `"line"`. Print its drawing and its `describe`-style text.

```vyrn starter
trait Shape:
    create area(self) -> float
    create draw(self) -> list[str]
    create label(self) -> str:
        return "shape"
    end
end

main:
    print("nothing yet")
end
```

```text expect
line, area 0.0
-----
```

> Hint: `struct Line:` with `length: int`, and `impl Shape for Line:` with
> all three methods.
> Hint: `draw` gives `["-" * self.length]`.

<details><summary>Solution</summary>

```vyrn solution
trait Shape:
    create area(self) -> float
    create draw(self) -> list[str]
    create label(self) -> str:
        return "shape"
    end
end

struct Line:
    length: int
end

impl Shape for Line:
    create area(self) -> float:
        return 0.0
    end
    create draw(self) -> list[str]:
        return ["-" * self.length]
    end
    create label(self) -> str:
        return "line"
    end
end

main:
    let l = Line(5)
    print("{l.label()}, area {l.area()}")
    print("\n".join(l.draw()))
end
```

</details>

## Today you learned

- `[T]` after a function's name makes it generic: `T` is whatever type the
  caller uses, and the checker keeps track of it.
- Structs and objects can be generic: `Stack[T]`.
- A `trait` lists methods; `impl Trait for Type` provides them; any value
  of an implementing type can be used as the trait.
- Trait methods with a body are defaults; types may replace them.
- `T: Shape` asks that `T` implement a trait.
- Inheritance is "is a kind of"; traits are "can do".
