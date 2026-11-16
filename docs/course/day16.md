# Day 16: Objects

As programs grow, related data and the functions that work on it belong
together. A bank account has a balance *and* ways to deposit and withdraw;
a game character has health *and* ways to take damage and heal. An
**object** bundles both. (In Python, C++ and Java this is a *class*; Vyrn
calls it `object` because that's what it makes.)

## 16.1 Declaring an object

```vyrn
object Dog:
    name: str
    age: int
end

main:
    rex = Dog("Rex", 3)
    print(rex.name, rex.age)
    rex.age += 1
    print(rex.age)
    print(rex)
end
```

```text
Rex 3
4
Dog(name="Rex", age=4)
```

- `object Dog:` declares a new **type**, named in CapitalCase by
  convention. Each line inside lists a **field**: a name and its type
  (`str`, `int`, `float`, `bool`, `list`, `map`, ...).
- `Dog("Rex", 3)` makes one dog, an **instance**, filling the fields in
  order. Calling the type like this is called its **constructor**.
- `rex.name` reads a field; `rex.age += 1` changes one.

### Exercise

Declare `object Book` with a `title`, an `author` and a number of `pages`,
make one, and print it as shown.

```vyrn starter
main:
end
```

```text expect
Dune by Frank Herbert, 412 pages
```

> Hint: `object Book:` with three fields, above `main:`.
> Hint: `b = Book("Dune", "Frank Herbert", 412)`

<details><summary>Solution</summary>

```vyrn solution
object Book:
    title: str
    author: str
    pages: int
end

main:
    b = Book("Dune", "Frank Herbert", 412)
    print("{b.title} by {b.author}, {b.pages} pages")
end
```

</details>

## 16.2 Default values and named fields

Fields can have defaults, so they can be left out when making an instance.
Fields can also be given by name, in any order:

```vyrn
object Player:
    name: str
    health: int = 100
    level: int = 1
    items: list = []
end

main:
    a = Player("Ada")
    b = Player("Ben", level = 5)
    print(a)
    print(b)
    a.items.append("map")
    print(a.items, b.items)
end
```

```text
Player(name="Ada", health=100, level=1, items=[])
Player(name="Ben", health=100, level=5, items=[])
["map"] []
```

Each instance gets its **own** fresh list for `items`: adding to Ada's
doesn't touch Ben's.

### Exercise

Declare `object Order` with a `dish`, a `quantity` defaulting to 1, and
`spicy` defaulting to `false`. Make the two orders shown.

```vyrn starter broken
main:
    first = Order("curry", spicy = true)
    second = Order("rice", 2)
    print(first)
    print(second)
end
```

```text expect
Order(dish="curry", quantity=1, spicy=true)
Order(dish="rice", quantity=2, spicy=false)
```

> Hint: `quantity: int = 1` and `spicy: bool = false`.

<details><summary>Solution</summary>

```vyrn solution
object Order:
    dish: str
    quantity: int = 1
    spicy: bool = false
end

main:
    first = Order("curry", spicy = true)
    second = Order("rice", 2)
    print(first)
    print(second)
end
```

</details>

## 16.3 Methods: functions that belong to an object

A function written inside the object is a **method**. Its first parameter,
`self`, is the instance it was called on:

```vyrn
object Rectangle:
    width: float
    height: float

    create area(self) -> float:
        return self.width * self.height
    end

    create is_square(self) -> bool:
        return self.width == self.height
    end
end

main:
    r = Rectangle(3.0, 4.0)
    print(r.area(), r.is_square())
    print(Rectangle(2.0, 2.0).is_square())
end
```

```text
12.0 false
true
```

`r.area()` calls `area` with `self` set to `r`. Inside, `self.width` is
that rectangle's width.

`-> float` after the parentheses says what the method returns. It's
optional, but it documents the method and lets `vyrn check` catch
mistakes.

### Exercise

Give `Circle` a method `area()` (pi times radius squared, use `3.14159`)
and `diameter()`.

```vyrn starter
object Circle:
    radius: float
end

main:
    c = Circle(2.0)
    print("{c.area():.2f} {c.diameter()}")
end
```

```text expect
12.57 4.0
```

> Hint: `create area(self) -> float: return 3.14159 * self.radius ** 2`

<details><summary>Solution</summary>

```vyrn solution
object Circle:
    radius: float

    create area(self) -> float:
        return 3.14159 * self.radius ** 2
    end

    create diameter(self) -> float:
        return self.radius * 2
    end
end

main:
    c = Circle(2.0)
    print("{c.area():.2f} {c.diameter()}")
end
```

</details>

## 16.4 Methods that change the object

The real power of objects: methods keep the object's **state** correct.
Instead of changing `balance` from anywhere, all changes go through
methods that check the rules:

```vyrn
object Account:
    owner: str
    balance: float = 0.0

    create deposit(self, amount: float):
        if amount <= 0: fail ValueError("deposits must be positive")
        self.balance += amount
    end

    create withdraw(self, amount: float) -> bool:
        if amount > self.balance: return false
        self.balance -= amount
        return true
    end
end

main:
    acct = Account("Ada")
    acct.deposit(100.0)
    print(acct.withdraw(30.0), acct.balance)
    print(acct.withdraw(500.0), acct.balance)
end
```

```text
true 70.0
false 70.0
```

### Exercise

Write a `Counter` object with a `count` starting at 0 and methods
`click()` (add one), `reset()` and `value()`.

```vyrn starter
object Counter:
    count: int = 0
end

main:
    c = Counter()
    c.click()
    c.click()
    c.click()
    print(c.value())
    c.reset()
    c.click()
    print(c.value())
end
```

```text expect
3
1
```

> Hint: `count: int = 0`, then three methods using `self.count`.

<details><summary>Solution</summary>

```vyrn solution
object Counter:
    count: int = 0

    create click(self): self.count += 1
    create reset(self): self.count = 0
    create value(self) -> int: return self.count
end

main:
    c = Counter()
    c.click()
    c.click()
    c.click()
    print(c.value())
    c.reset()
    c.click()
    print(c.value())
end
```

</details>

## 16.5 Choosing how an object prints

By default an object prints all its fields. A method named `to_text`
decides instead, everywhere: in `print`, in `{...}`, in `str(...)`, and
inside printed lists:

```vyrn
object Money:
    cents: int

    create to_text(self) -> str:
        return "${self.cents // 100}.{self.cents % 100:02d}"
    end
end

main:
    price = Money(1999)
    print(price)
    print("Total: {price}")
    print([Money(5), Money(250)])
end
```

```text
$19.99
Total: $19.99
[$0.05, $2.50]
```

### Exercise

Give `Temperature` a `to_text` that shows one decimal and a degree sign.

```vyrn starter
object Temperature:
    celsius: float
end

main:
    print(Temperature(21.456))
    print([Temperature(-3.0), Temperature(30.25)])
end
```

```text expect
21.5°C
[-3.0°C, 30.2°C]
```

> Hint: `return "{self.celsius:.1f}°C"`

<details><summary>Solution</summary>

```vyrn solution
object Temperature:
    celsius: float

    create to_text(self) -> str:
        return "{self.celsius:.1f}°C"
    end
end

main:
    print(Temperature(21.456))
    print([Temperature(-3.0), Temperature(30.25)])
end
```

</details>

## 16.6 Objects are shared

An important fact: assigning an object to another name does **not** copy
it. Both names refer to the same object:

```vyrn
object Box:
    contents: list = []
end

create fill(b: Box):
    b.contents.append("toy")
end

main:
    a = Box()
    b = a
    b.contents.append("book")
    fill(a)
    print(a.contents)
    print(b.contents)
end
```

```text
["book", "toy"]
["book", "toy"]
```

`b = a` gives the same box a second name, and `fill(a)` changes the box
the caller holds. That's usually what you want for things with an
identity: there is one bank account, one player, one game.

(C++ and Java programmers: an object variable behaves like a reference,
or a smart pointer, and the object is freed automatically once nothing
refers to it. There's no `new` or `delete`.)

Tomorrow you meet `struct`, which is the opposite: a value that is copied.

### Exercise

`add_points` should change the player it's given. Write it, and see both
names show the new score.

```vyrn starter
object Player:
    name: str
    score: int = 0
end

create add_points(player: Player, n: int):
end

main:
    p = Player("Ada")
    same = p
    add_points(p, 10)
    print(p.score, same.score)
end
```

```text expect
10 10
```

> Hint: `create add_points(player: Player, n: int): player.score += n`

<details><summary>Solution</summary>

```vyrn solution
object Player:
    name: str
    score: int = 0
end

create add_points(player: Player, n: int):
    player.score += n
end

main:
    p = Player("Ada")
    same = p
    add_points(p, 10)
    print(p.score, same.score)
end
```

</details>

## 16.7 Project: the Pet Shelter

A shelter keeps a list of pets. Each pet has a name, a species and a
hunger level from 0 (full) to 10 (starving). Feeding lowers hunger by 3
(never below 0); each day raises it by 2 (never above 10).

### Exercise

Write the `Pet` object with methods `feed()`, `next_day()` and `to_text()`
(`Rex the dog (hunger 4)`), then run the shelter for two days, feeding
only the hungriest pet each day.

```vyrn starter
object Pet:
    name: str
    species: str
    hunger: int = 5
end

main:
    pets = [Pet("Rex", "dog", 6), Pet("Tom", "cat", 2), Pet("Nemo", "fish")]
    each day in 1 to 2:
        print("Day {day}: {pets}")
    end
end
```

```text expect
Day 1: [Rex the dog (hunger 6), Tom the cat (hunger 2), Nemo the fish (hunger 5)]
Feeding Rex
Day 2: [Rex the dog (hunger 5), Tom the cat (hunger 4), Nemo the fish (hunger 7)]
Feeding Nemo
```

> Hint: `feed`: `self.hunger = max(0, self.hunger - 3)`. `next_day`: `self.hunger = min(10, self.hunger + 2)`.
> Hint: Each day: print, find `hungriest = max(pets, key = p => p.hunger)`, feed it, then call `next_day()` on every pet.

<details><summary>Solution</summary>

```vyrn solution
object Pet:
    name: str
    species: str
    hunger: int = 5

    create feed(self): self.hunger = max(0, self.hunger - 3)
    create next_day(self): self.hunger = min(10, self.hunger + 2)

    create to_text(self) -> str:
        return "{self.name} the {self.species} (hunger {self.hunger})"
    end
end

main:
    pets = [Pet("Rex", "dog", 6), Pet("Tom", "cat", 2), Pet("Nemo", "fish")]
    each day in 1 to 2:
        print("Day {day}: {pets}")
        hungriest = max(pets, key = p => p.hunger)
        print("Feeding {hungriest.name}")
        hungriest.feed()
        each p in pets: p.next_day()
    end
end
```

</details>

**Day 16 done.** You can design your own types with data and behaviour.
Tomorrow: values versus shared objects, keeping fields private, and a quiz
app built from objects.
