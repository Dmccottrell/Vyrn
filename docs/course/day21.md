# Day 21: Inheritance, Polymorphism, and Snake Part 2

Often one kind of thing is a special case of another: a dog *is an*
animal, a savings account *is an* account. **Inheritance** lets one object
type build on another, reusing its fields and methods and changing only
what's different. Today you learn it, see why it matters, and use it to
finish Snake.

## 21.1 Building on another object

```vyrn
object Animal:
    name: str
    legs: int = 4

    create describe(self) -> str:
        return "{self.name} has {self.legs} legs"
    end

    create speak(self) -> str:
        return "..."
    end
end

object Dog(Animal):
    tricks: list = []

    create speak(self) -> str:
        return "Woof!"
    end
end

main:
    rex = Dog("Rex", tricks = ["sit"])
    print(rex.describe())
    print(rex.speak())
    print(rex)
end
```

```text
Rex has 4 legs
Woof!
Dog(name="Rex", legs=4, tricks=["sit"])
```

`object Dog(Animal):` means a Dog is an Animal with extras:

- It **inherits** everything: `name`, `legs` and `describe()` come from
  `Animal` without being written again.
- It **adds** a field, `tricks`.
- It **overrides** `speak()` with its own version.

`Animal` is the **base** (or parent) type; `Dog` is **derived** from it
(the child).

### Exercise

Add `object Bird(Animal)` with 2 legs by default and a `speak` of
`Tweet!`.

```vyrn starter
object Animal:
    name: str
    legs: int = 4

    create describe(self) -> str:
        return "{self.name} has {self.legs} legs"
    end

    create speak(self) -> str:
        return "..."
    end
end

object Bird(Animal):
end

main:
    b = Bird("Tweety")
    print(b.describe(), b.speak())
end
```

```text expect
Tweety has 2 legs Tweet!
```

> Hint: A child can give an inherited field a new default: `legs: int = 2`.

<details><summary>Solution</summary>

```vyrn solution
object Animal:
    name: str
    legs: int = 4

    create describe(self) -> str:
        return "{self.name} has {self.legs} legs"
    end

    create speak(self) -> str:
        return "..."
    end
end

object Bird(Animal):
    legs: int = 2

    create speak(self) -> str:
        return "Tweet!"
    end
end

main:
    b = Bird("Tweety")
    print(b.describe(), b.speak())
end
```

</details>

## 21.2 `super`: extending instead of replacing

An overriding method can call the parent's version with `super`, and add to
it:

```vyrn
object Account:
    owner: str
    balance: float = 0.0

    create describe(self) -> str:
        return "{self.owner}: {self.balance:.2f}"
    end
end

object Savings(Account):
    rate: float = 0.02

    create describe(self) -> str:
        return super.describe() + " (saving at {self.rate:.0%})"
    end

    create add_interest(self):
        self.balance += self.balance * self.rate
    end
end

main:
    s = Savings("Ada", 1000.0)
    s.add_interest()
    print(s.describe())
end
```

```text
Ada: 1020.00 (saving at 2%)
```

### Exercise

`Manager` should describe itself as an employee plus its team size. Use
`super`.

```vyrn starter
object Employee:
    name: str
    salary: int

    create describe(self) -> str:
        return "{self.name} earns {self.salary}"
    end
end

object Manager(Employee):
    team: int = 0
end

main:
    print(Manager("Grace", 90000, 6).describe())
end
```

```text expect
Grace earns 90000 and leads 6 people
```

> Hint: `return super.describe() + " and leads {self.team} people"`

<details><summary>Solution</summary>

```vyrn solution
object Employee:
    name: str
    salary: int

    create describe(self) -> str:
        return "{self.name} earns {self.salary}"
    end
end

object Manager(Employee):
    team: int = 0

    create describe(self) -> str:
        return super.describe() + " and leads {self.team} people"
    end
end

main:
    print(Manager("Grace", 90000, 6).describe())
end
```

</details>

## 21.3 Polymorphism: one call, many behaviours

The big payoff: code that works with the base type works with every
derived type, and each object uses its **own** version of a method. This
is called **polymorphism** ("many shapes"):

```vyrn
object Shape:
    name: str = "shape"
    create area(self) -> float: return 0.0
end

object Square(Shape):
    side: float = 1.0
    create area(self) -> float: return self.side * self.side
end

object Circle(Shape):
    radius: float = 1.0
    create area(self) -> float: return 3.14159 * self.radius ** 2
end

main:
    shapes = [Square("square", 2.0), Circle("circle", 1.0), Square("tile", 0.5)]
    each s in shapes:
        print("{s.name}: {s.area():.2f}")
    end
    print("total: {sum([s.area() each s in shapes]):.2f}")
end
```

```text
square: 4.00
circle: 3.14
tile: 0.25
total: 7.39
```

The loop never asks "is this a square or a circle?": each shape knows how
to compute its own area. Adding a `Triangle` later needs no change to the
loop. (C++ calls these *virtual* methods; in Vyrn every method can be
overridden.)

### Exercise

Add `object Triangle(Shape)` with `base` and `height` (area is half of
base times height), and include one in the list.

```vyrn starter
object Shape:
    name: str = "shape"
    create area(self) -> float: return 0.0
end

object Square(Shape):
    side: float = 1.0
    create area(self) -> float: return self.side * self.side
end

main:
    shapes = [Square("square", 2.0)]
    each s in shapes: print("{s.name}: {s.area():.2f}")
end
```

```text expect
square: 4.00
triangle: 6.00
```

> Hint: `object Triangle(Shape):` with `base: float = 1.0` and `height: float = 1.0`.
> Hint: Add `Triangle("triangle", 4.0, 3.0)` to the list.

<details><summary>Solution</summary>

```vyrn solution
object Shape:
    name: str = "shape"
    create area(self) -> float: return 0.0
end

object Square(Shape):
    side: float = 1.0
    create area(self) -> float: return self.side * self.side
end

object Triangle(Shape):
    base: float = 1.0
    height: float = 1.0
    create area(self) -> float: return self.base * self.height / 2
end

main:
    shapes = [Square("square", 2.0), Triangle("triangle", 4.0, 3.0)]
    each s in shapes: print("{s.name}: {s.area():.2f}")
end
```

</details>

## 21.4 `is`: asking what kind of thing it is

Sometimes you do need to know the type. `value is Type` answers, and it
counts derived types: a `Dog` *is* an `Animal`.

```vyrn
object Animal:
    name: str
end

object Dog(Animal):
end

object Cat(Animal):
end

main:
    pets = [Dog("Rex"), Cat("Tom"), Dog("Fido")]
    print(pets[0] is Dog, pets[0] is Animal, pets[1] is Dog)
    dogs = [p.name each p in pets if p is Dog]
    print(dogs)
    print(5 is int, "hi" is str, none is none, pets[1] is not Dog)
end
```

```text
true true false
["Rex", "Fido"]
true true true true
```

Prefer overriding methods (lesson 21.3) when you can; use `is` when the
question really is about the type.

### Exercise

Count the cats and the dogs.

```vyrn starter
object Animal:
    name: str
end

object Dog(Animal):
end

object Cat(Animal):
end

main:
    pets = [Dog("Rex"), Cat("Tom"), Dog("Fido"), Cat("Kit"), Cat("Mog")]
end
```

```text expect
2 dogs, 3 cats
```

> Hint: `len([p each p in pets if p is Dog])`

<details><summary>Solution</summary>

```vyrn solution
object Animal:
    name: str
end

object Dog(Animal):
end

object Cat(Animal):
end

main:
    pets = [Dog("Rex"), Cat("Tom"), Dog("Fido"), Cat("Kit"), Cat("Mog")]
    dogs = len([p each p in pets if p is Dog])
    cats = len([p each p in pets if p is Cat])
    print("{dogs} dogs, {cats} cats")
end
```

</details>

## 21.5 Setting up an object: `init`

Sometimes an object needs to check its fields, or work something out, as
soon as it's made. A method called `init` runs automatically right after
the fields are filled in:

```vyrn
object Rectangle:
    width: float
    height: float
    area: float = 0.0

    create init(self):
        if self.width <= 0 or self.height <= 0:
            fail ValueError("sides must be positive")
        end
        self.area = self.width * self.height
    end
end

main:
    print(Rectangle(3.0, 4.0).area)
    attempt:
        Rectangle(-1.0, 2.0)
    catch ValueError as e:
        print("Refused:", e.message)
    end
end
```

```text
12.0
Refused: sides must be positive
```

Derived objects inherit `init` too. (This is what Python's `__init__` and
the constructors of Java and C++ are for.)

### Exercise

Make `Username` store its `name` in lowercase, and refuse names shorter
than 3 characters.

```vyrn starter
object Username:
    name: str
end

main:
    print(Username("AdaL").name)
    attempt:
        Username("Al")
    catch ValueError as e:
        print(e.message)
    end
end
```

```text expect
adal
too short
```

> Hint: In `init`: `if len(self.name) < 3: fail ValueError("too short")`, then `self.name = self.name.lower()`.

<details><summary>Solution</summary>

```vyrn solution
object Username:
    name: str

    create init(self):
        if len(self.name) < 3: fail ValueError("too short")
        self.name = self.name.lower()
    end
end

main:
    print(Username("AdaL").name)
    attempt:
        Username("Al")
    catch ValueError as e:
        print(e.message)
    end
end
```

</details>

## 21.6 Project: Snake, Part 2

The game becomes objects: a `Snake` that moves, grows and checks crashes,
and a `Scoreboard`. A `Game` ties them together. Slicing helps: the body
is `self.cells[1:]`.

### Exercise

Finish `Snake` and play a scripted game: the snake moves right, eats the
food at `(4, 2)`, then turns down.

```vyrn starter
const COLS = 10
const ROWS = 6

struct Cell:
    x: int
    y: int
end

object Snake:
    cells: list = [Cell(2, 2), Cell(1, 2), Cell(0, 2)]
    dx: int = 1
    dy: int = 0

    create head(self) -> Cell: return self.cells[0]
end

object Scoreboard:
    score: int = 0
    create add(self): self.score += 1
    create to_text(self) -> str: return "Score: {self.score}"
end

main:
    snake = Snake()
    board = Scoreboard()
    food = Cell(4, 2)
    moves = ["right", "right", "down", "down"]
end
```

```text expect
Cell(x=3, y=2) Score: 0
Cell(x=4, y=2) Score: 1
Cell(x=4, y=3) Score: 1
Cell(x=4, y=4) Score: 1
length 4, crashed: false
```

> Hint: Give `Snake` methods `turn(key)`, `step(food) -> bool` (true when it ate) and `crashed() -> bool`.
> Hint: Each move: `snake.turn(key)`, then `if snake.step(food): board.add()`, then print the head and the board.

<details><summary>Solution</summary>

```vyrn solution
const COLS = 10
const ROWS = 6

struct Cell:
    x: int
    y: int
end

object Snake:
    cells: list = [Cell(2, 2), Cell(1, 2), Cell(0, 2)]
    dx: int = 1
    dy: int = 0

    create head(self) -> Cell: return self.cells[0]

    create turn(self, key: str):
        when key:
            is "left":
                if self.dx == 0:
                    self.dx = -1
                    self.dy = 0
                end
            is "right":
                if self.dx == 0:
                    self.dx = 1
                    self.dy = 0
                end
            is "up":
                if self.dy == 0:
                    self.dx = 0
                    self.dy = -1
                end
            is "down":
                if self.dy == 0:
                    self.dx = 0
                    self.dy = 1
                end
        end
    end

    create step(self, food: Cell) -> bool:
        let h = self.head()
        let new_head = Cell((h.x + self.dx) % COLS, (h.y + self.dy) % ROWS)
        self.cells.insert(0, new_head)
        if new_head == food: return true
        self.cells.pop()
        return false
    end

    create crashed(self) -> bool:
        return self.head() in self.cells[1:]
    end
end

object Scoreboard:
    score: int = 0
    create add(self): self.score += 1
    create to_text(self) -> str: return "Score: {self.score}"
end

main:
    snake = Snake()
    board = Scoreboard()
    food = Cell(4, 2)
    moves = ["right", "right", "down", "down"]
    each key in moves:
        snake.turn(key)
        if snake.step(food): board.add()
        print(snake.head(), board)
    end
    print("length {len(snake.cells)}, crashed: {snake.crashed()}")
end
```

</details>

**Try it on screen:** replace the scripted moves with the window loop from
Day 20, calling `snake.turn` for the arrow keys, moving the food after each
meal, and ending the game when `snake.crashed()`.

**Day 21 done.** Inheritance, `super`, polymorphism and `is`: the core of
object-oriented programming. Tomorrow: Pong, with collisions.
