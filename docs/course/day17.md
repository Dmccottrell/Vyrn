# Day 17: Structs, Privacy, and a Quiz App

Yesterday's objects are **shared**: two names can point at one object.
Today you meet their partner, the **struct**, a value that is **copied**,
like a number. You'll learn when to use each, how to keep an object's
insides `private`, and you'll build a quiz app from both.

## 17.1 Structs are values

A `struct` is declared like an object, but behaves like a number: assigning
or passing it makes a **copy**.

```vyrn
struct Point:
    x: int
    y: int
end

main:
    a = Point(1, 2)
    b = a
    b.x = 99
    print(a)
    print(b)
    print(a == Point(1, 2))
end
```

```text
Point(x=1, y=2)
Point(x=99, y=2)
true
```

Changing `b` left `a` alone: `b` was a copy. And two structs with the same
fields are equal (`==`), just as two `5`s are.

Compare objects: `b = a` shares, so changing `b` changes `a`, and `==`
asks "is this the very same object?".

### Exercise

Make a copy of the starting position, move the copy, and print both to show
the start was not changed.

```vyrn starter
struct Position:
    row: int
    col: int
end

main:
    start = Position(0, 0)
end
```

```text expect
start: Position(row=0, col=0)
moved: Position(row=2, col=3)
```

> Hint: `moved = start`, then change `moved.row` and `moved.col`.

<details><summary>Solution</summary>

```vyrn solution
struct Position:
    row: int
    col: int
end

main:
    start = Position(0, 0)
    moved = start
    moved.row = 2
    moved.col = 3
    print("start:", start)
    print("moved:", moved)
end
```

</details>

## 17.2 Struct or object?

| Use a `struct` when... | Use an `object` when... |
|---|---|
| the thing *is* its values: a point, a colour, a date, a quiz question | the thing has an identity that changes over time: a player, an account, a game |
| two with the same values are interchangeable | two with the same values are still two different things |
| it's small | it may be large, or many places need to see its changes |

Python's dataclasses, Java's records and C++'s plain structs are all close
to Vyrn's `struct`. Python and Java classes, and C++ classes used through
pointers, are like Vyrn's `object`.

### Exercise

Decide: print `struct` or `object` for each, as `<thing>: <choice>`.

```vyrn starter
main:
    things = ["a colour (red, green, blue)", "a chat room with members", "a playing card"]
end
```

```text expect
a colour (red, green, blue): struct
a chat room with members: object
a playing card: struct
```

> Hint: Does it change over time, with others needing to see the change?

<details><summary>Solution</summary>

```vyrn solution
main:
    things = ["a colour (red, green, blue)", "a chat room with members", "a playing card"]
    choices = ["struct", "object", "struct"]
    each i in 0 until len(things):
        print("{things[i]}: {choices[i]}")
    end
end
```

</details>

## 17.3 Methods on structs

Structs have methods too. A method that gives back a **new** value is the
natural style for values:

```vyrn
struct Vec2:
    x: float
    y: float

    create plus(self, other: Vec2) -> Vec2:
        return Vec2(self.x + other.x, self.y + other.y)
    end

    create length(self) -> float:
        return (self.x ** 2 + self.y ** 2) ** 0.5
    end

    create scale(mut self, k: float):
        self.x *= k
        self.y *= k
    end
end

main:
    a = Vec2(3.0, 4.0)
    print(a.length(), a.plus(Vec2(1.0, 1.0)))
    var b = a
    b.scale(2.0)
    print(a, b)
end
```

```text
5.0 Vec2(x=4.0, y=5.0)
Vec2(x=3.0, y=4.0) Vec2(x=6.0, y=8.0)
```

A method that changes a struct in place says so with `mut self`, and works
on a `var`. Only that copy changes.

### Exercise

Give `Fraction` a method `times(other)` returning the product, and
`to_text` showing `3/8`.

```vyrn starter
struct Fraction:
    top: int
    bottom: int
end

main:
    half = Fraction(1, 2)
    three_quarters = Fraction(3, 4)
    print(half.times(three_quarters))
end
```

```text expect
3/8
```

> Hint: `return Fraction(self.top * other.top, self.bottom * other.bottom)`

<details><summary>Solution</summary>

```vyrn solution
struct Fraction:
    top: int
    bottom: int

    create times(self, other: Fraction) -> Fraction:
        return Fraction(self.top * other.top, self.bottom * other.bottom)
    end

    create to_text(self) -> str:
        return "{self.top}/{self.bottom}"
    end
end

main:
    half = Fraction(1, 2)
    three_quarters = Fraction(3, 4)
    print(half.times(three_quarters))
end
```

</details>

## 17.4 `private`: keeping the insides inside

An object's methods keep its rules: an account's balance only changes
through `deposit` and `withdraw`. But nothing stops other code writing
`acct.balance = 1000000`... unless the field is `private`:

```vyrn
object Account:
    owner: str
    private balance: float = 0.0

    create deposit(self, amount: float):
        if amount <= 0: fail ValueError("deposits must be positive")
        self.balance += amount
    end

    create current(self) -> float:
        return self.balance
    end
end

main:
    acct = Account("Ada")
    acct.deposit(50.0)
    print(acct.current())
    print(acct.balance)
end
```

```text
50.0
error: PrivateError: `balance` is private to `Account`: only its own methods can use it
```

`private` fields and methods can only be used inside the object's own
methods (and those of objects built on it, Day 21). Everything else goes
through the public methods. This is called **encapsulation**: you can
change how an object works inside without breaking the code that uses it.

### Exercise

Make `attempts` private, and add a method `attempts_left()` so `main`
doesn't need to touch the field.

```vyrn starter
object Lock:
    code: int
    attempts: int = 3

    create try_code(self, guess: int) -> bool:
        if self.attempts == 0: return false
        if guess == self.code: return true
        self.attempts -= 1
        return false
    end
end

main:
    lock = Lock(4321)
    print(lock.try_code(1111), lock.try_code(4321))
    print("Attempts left:", lock.attempts)
end
```

```text expect
false true
Attempts left: 2
```

> Hint: `private attempts: int = 3`
> Hint: `create attempts_left(self) -> int: return self.attempts`, and use it in `main`.

<details><summary>Solution</summary>

```vyrn solution
object Lock:
    code: int
    private attempts: int = 3

    create try_code(self, guess: int) -> bool:
        if self.attempts == 0: return false
        if guess == self.code: return true
        self.attempts -= 1
        return false
    end

    create attempts_left(self) -> int:
        return self.attempts
    end
end

main:
    lock = Lock(4321)
    print(lock.try_code(1111), lock.try_code(4321))
    print("Attempts left:", lock.attempts_left())
end
```

</details>

## 17.5 Private methods

Helpers that only the object itself should call can be private too:

```vyrn
object Thermostat:
    private target: float = 20.0

    create set_target(self, value: float):
        self.target = self.clamp(value)
    end

    create target_now(self) -> float: return self.target

    private create clamp(self, value: float) -> float:
        return max(10.0, min(30.0, value))
    end
end

main:
    t = Thermostat()
    t.set_target(42.0)
    print(t.target_now())
    t.set_target(18.5)
    print(t.target_now())
end
```

```text
30.0
18.5
```

### Exercise

Add a private method `format_line(label, value)` to `Receipt`, used by
`show()`, so every line is formatted the same way.

```vyrn starter
object Receipt:
    items: map = {}

    create add(self, name: str, price: float):
        self.items[name] = price
    end

    create show(self):
        each name, price in self.items.items():
            print("{name:<10}{price:>7.2f}")
        end
        print("{"TOTAL":<10}{sum(self.items.values()):>7.2f}")
    end
end

main:
    r = Receipt()
    r.add("bread", 2.25)
    r.add("cheese", 6.5)
    r.show()
end
```

```text expect
bread        2.25
cheese       6.50
TOTAL        8.75
```

> Hint: `private create format_line(self, label: str, value: float) -> str: return "{label:<10}{value:>7.2f}"`

<details><summary>Solution</summary>

```vyrn solution
object Receipt:
    items: map = {}

    create add(self, name: str, price: float):
        self.items[name] = price
    end

    create show(self):
        each name, price in self.items.items():
            print(self.format_line(name, price))
        end
        print(self.format_line("TOTAL", sum(self.items.values())))
    end

    private create format_line(self, label: str, value: float) -> str:
        return "{label:<10}{value:>7.2f}"
    end
end

main:
    r = Receipt()
    r.add("bread", 2.25)
    r.add("cheese", 6.5)
    r.show()
end
```

</details>

## 17.6 Project: the Quiz App

The design, using the right tool for each part:

- `struct Question`: a question's text and its answer. A plain value.
- `object Quiz`: the list of questions, which one is next, and the score.
  It changes as the quiz goes on, so it's an object, with private state.

### Exercise

The checker answers `true`, `true`, `false`.

```vyrn starter
struct Question:
    text: str
    answer: bool
end

object Quiz:
    questions: list
    private next_index: int = 0
    private score: int = 0
end

main:
    let quiz = Quiz([
        Question("Octopuses have three hearts.", true),
        Question("Venus is the closest planet to the Sun.", false),
        Question("Honey can last for thousands of years.", true),
    ])
end
```

```text input
true
true
false
```

```text expect
Q1. Octopuses have three hearts. (true/false) Right!
Q2. Venus is the closest planet to the Sun. (true/false) Wrong: it's false.
Q3. Honey can last for thousands of years. (true/false) Wrong: it's true.
You scored 1/3.
```

> Hint: Give `Quiz` methods `has_more()`, `ask()` and `score_text()`.
> Hint: In `ask`, read the answer with `input(...) == "true"`, compare with the question's answer, then move `next_index` on.

<details><summary>Solution</summary>

```vyrn solution
struct Question:
    text: str
    answer: bool
end

object Quiz:
    questions: list
    private next_index: int = 0
    private score: int = 0

    create has_more(self) -> bool:
        return self.next_index < len(self.questions)
    end

    create ask(self):
        let q = self.questions[self.next_index]
        self.next_index += 1
        let reply = input("Q{self.next_index}. {q.text} (true/false) ") == "true"
        if reply == q.answer:
            self.score += 1
            print("Right!")
        else:
            print("Wrong: it's {q.answer}.")
        end
    end

    create score_text(self) -> str:
        return "{self.score}/{len(self.questions)}"
    end
end

main:
    let quiz = Quiz([
        Question("Octopuses have three hearts.", true),
        Question("Venus is the closest planet to the Sun.", false),
        Question("Honey can last for thousands of years.", true),
    ])
    loop quiz.has_more(): quiz.ask()
    print("You scored {quiz.score_text()}.")
end
```

</details>

**Try it:** load the questions from a list of maps with a comprehension:
`[Question(q["text"], q["answer"]) each q in data]`.

**Day 17 done.** You know values from shared objects, and how to protect
an object's insides. Tomorrow: pictures!
