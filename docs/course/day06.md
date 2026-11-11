# Day 6: Functions

A **function** gives a name to a piece of work, so you can use it again
without writing it again. You have already used many: `print`, `len`,
`input`, `random.choice`. Today you create your own, and build a robot that
follows instructions around a grid.

## 6.1 Creating a function

```vyrn
create greet():
    print("Hello!")
    print("Welcome to Vyrn.")
end

main:
    greet()
    greet()
end
```

```text
Hello!
Welcome to Vyrn.
Hello!
Welcome to Vyrn.
```

- `create greet():` creates a function named `greet`. The lines up to `end`
  are its **body**.
- Functions are written **outside** `main:`, usually above it.
- Creating a function doesn't run it. `greet()` **calls** it: Vyrn jumps to
  the body, runs it, and comes back.

The parentheses matter: `greet()` runs the function; plain `greet` just
names it.

### Exercise

Create a function `chorus` that prints the two lines below, and call it
twice.

```vyrn starter
main:
end
```

```text expect
Na na na
Hey hey, goodbye
Na na na
Hey hey, goodbye
```

> Hint: `create chorus():` ... `end` above `main:`.

<details><summary>Solution</summary>

```vyrn solution
create chorus():
    print("Na na na")
    print("Hey hey, goodbye")
end

main:
    chorus()
    chorus()
end
```

</details>

## 6.2 Parameters: giving a function information

Inside the parentheses, a function lists its **parameters**: names for the
values it is given when called.

```vyrn
create greet(name):
    print("Hello, {name}!")
end

create describe(animal, sound):
    print("The {animal} says {sound}.")
end

main:
    greet("Ada")
    greet("Linus")
    describe("cow", "moo")
end
```

```text
Hello, Ada!
Hello, Linus!
The cow says moo.
```

The values in a call (`"Ada"`) are called **arguments**. They are matched
to parameters in order: `describe("cow", "moo")` sets `animal` to `"cow"`
and `sound` to `"moo"`.

### Exercise

Create `box(word)` that prints the word inside a frame of `*`, as wide as
the word plus four.

```vyrn starter
main:
    box("Vyrn")
    box("functions")
end
```

```text expect
********
* Vyrn *
********
*************
* functions *
*************
```

> Hint: The frame line is `"*" * (len(word) + 4)`.
> Hint: The middle line is `"* {word} *"`.

<details><summary>Solution</summary>

```vyrn solution
create box(word):
    border = "*" * (len(word) + 4)
    print(border)
    print("* {word} *")
    print(border)
end

main:
    box("Vyrn")
    box("functions")
end
```

</details>

## 6.3 `return`: giving back an answer

`print` shows a value on the screen. `return` *hands it back* to whoever
called the function, so they can store it or use it:

```vyrn
create area(width, height):
    return width * height
end

create is_even(n):
    return n % 2 == 0
end

main:
    a = area(3, 4)
    print(a)
    print(area(2, 5) + area(1, 1))
    print(is_even(10), is_even(7))
end
```

```text
12
11
true false
```

`return` also ends the function at once. That makes "early returns" handy:

```vyrn
create describe_temp(c):
    if c >= 30: return "hot"
    if c >= 15: return "mild"
    return "cold"
end

main:
    each t in [35, 20, 3]:
        print(t, describe_temp(t))
    end
end
```

```text
35 hot
20 mild
3 cold
```

### Exercise

Write `c_to_f(c)`, which returns `c * 9 / 5 + 32`, and print a small table.

```vyrn starter
main:
    each c in [0, 37, 100]:
        print("{c} C = {c_to_f(c)} F")
    end
end
```

```text expect
0 C = 32.0 F
37 C = 98.6 F
100 C = 212.0 F
```

> Hint: `create c_to_f(c): return c * 9 / 5 + 32` (one line, so no `end`).

<details><summary>Solution</summary>

```vyrn solution
create c_to_f(c): return c * 9 / 5 + 32

main:
    each c in [0, 37, 100]:
        print("{c} C = {c_to_f(c)} F")
    end
end
```

</details>

## 6.4 Defaults and named arguments

A parameter can have a **default** value, used when the caller leaves it
out. Callers can also name arguments, which makes calls easy to read:

```vyrn
create price(amount, tax = 0.2, discount = 0.0):
    return amount * (1 + tax) * (1 - discount)
end

main:
    print(price(100))
    print(price(100, 0.05))
    print(price(100, discount = 0.5))
    print(price(amount = 50, tax = 0.0))
end
```

```text
120.0
105.0
60.0
50.0
```

Parameters with defaults come after the ones without. Named arguments can
come in any order, after the plain ones.

### Exercise

Write `repeat_word(word, times = 2, sep = " ")` that returns the word
repeated `times` times, joined by `sep`.

```vyrn starter
main:
    print(repeat_word("hip"))
    print(repeat_word("hip", 3))
    print(repeat_word("na", 4, sep = "-"))
end
```

```text expect
hip hip
hip hip hip
na-na-na-na
```

> Hint: Build a list with a loop, then `sep.join(parts)`.
> Hint: Or: `sep.join([word] * times)`.

<details><summary>Solution</summary>

```vyrn solution
create repeat_word(word, times = 2, sep = " "):
    parts = []
    each _ in 1 to times: parts.append(word)
    return sep.join(parts)
end

main:
    print(repeat_word("hip"))
    print(repeat_word("hip", 3))
    print(repeat_word("na", 4, sep = "-"))
end
```

</details>

## 6.5 Where names live: scope

A variable created inside a function belongs to that function. It exists
while the function runs, and other functions can't see it. That is called
its **scope**.

```vyrn
create add_points(score):
    bonus = 10
    return score + bonus
end

main:
    score = 5
    score = add_points(score)
    print(score)
end
```

```text
15
```

`bonus` lives only inside `add_points`. The `score` in `main` and the
`score` parameter are different variables that happen to share a name:
the function gets a copy of the value and hands a new value back with
`return`. This is how functions share information: **arguments in,
`return` out.**

Values that never change and that every function needs can be declared at
the top of the file with `const`:

```vyrn
const MAX_LIVES = 3

create lives_left(lost):
    return MAX_LIVES - lost
end

main:
    print(lives_left(1))
end
```

```text
2
```

By convention, constants are written in CAPITALS.

### Exercise

This program should print `Level 3`, but `level` never changes. Fix it so
`level_up` gives back the new level.

```vyrn starter
create level_up(level):
    level += 1
end

main:
    level = 1
    level_up(level)
    level_up(level)
    print("Level {level}")
end
```

```text expect
Level 3
```

> Hint: The function needs to `return level`.
> Hint: And `main` needs to keep the answer: `level = level_up(level)`.

<details><summary>Solution</summary>

```vyrn solution
create level_up(level):
    return level + 1
end

main:
    level = 1
    level = level_up(level)
    level = level_up(level)
    print("Level {level}")
end
```

</details>

## 6.6 Functions that use functions

Big problems are solved by splitting them into small functions, each doing
one job, then combining them:

```vyrn
create is_vowel(letter):
    return letter.lower() in "aeiou"
end

create count_vowels(text):
    count = 0
    each c in text:
        if is_vowel(c): count += 1
    end
    return count
end

main:
    print(count_vowels("Functions are fun"))
end
```

```text
6
```

A comment starting with `##` just above a function is its **doc comment**:
editors show it when you hover over the function's name.

```vyrn
## The larger of two numbers.
create bigger(a, b):
    return a if a > b else b
end

main:
    print(bigger(3, 8))
end
```

```text
8
```

`a if a > b else b` is a one-line choice: the value is `a` if the condition
holds, otherwise `b`.

### Exercise

Write `is_prime(n)` (a prime has no divisors except 1 and itself), then
`primes_up_to(limit)` that returns a list of the primes up to `limit`.

```vyrn starter
main:
    print(primes_up_to(30))
end
```

```text expect
[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
```

> Hint: In `is_prime`, numbers below 2 are not prime; then try every divisor from 2 until n.
> Hint: `if n % d == 0: return false`; after the loop, `return true`.

<details><summary>Solution</summary>

```vyrn solution
create is_prime(n):
    if n < 2: return false
    each d in 2 until n:
        if n % d == 0: return false
    end
    return true
end

create primes_up_to(limit):
    found = []
    each n in 2 to limit:
        if is_prime(n): found.append(n)
    end
    return found
end

main:
    print(primes_up_to(30))
end
```

</details>

## 6.7 Project: the Grid Robot

A robot starts at the middle of a 5 by 5 grid, facing north. It follows a
string of commands: `F` moves forward one square, `L` and `R` turn left and
right. It can't leave the grid: a move that would take it off is ignored.

Split the work into functions:

- `turn_left(facing)` and `turn_right(facing)` return the new direction.
- `step(x, y, facing)` returns where one step forward would land.
- `run(commands)` follows the commands and returns the final position.

Directions in order clockwise: `["N", "E", "S", "W"]`. Turning right moves
one along the list; `%` wraps around the end.

### Exercise

Finish the functions. `main` first tests `turn_right` on its own (testing
each small piece is a good habit), then prints where the robot ends after
`FFRFFLFRRF`.

```vyrn starter
const DIRECTIONS = ["N", "E", "S", "W"]

create turn_right(facing):
    i = DIRECTIONS.index(facing)
    return DIRECTIONS[(i + 1) % 4]
end

main:
    print(turn_right("W"))
end
```

```text expect
N
(4, 1) facing S
```

> Hint: `turn_left` is the same with `(i + 3) % 4`.
> Hint: In `step`, north is `y - 1`, south `y + 1`, east `x + 1`, west `x - 1`.
> Hint: In `run`, only move if `0 <= nx <= 4 and 0 <= ny <= 4`.

<details><summary>Solution</summary>

```vyrn solution
const DIRECTIONS = ["N", "E", "S", "W"]

create turn_right(facing):
    i = DIRECTIONS.index(facing)
    return DIRECTIONS[(i + 1) % 4]
end

create turn_left(facing):
    i = DIRECTIONS.index(facing)
    return DIRECTIONS[(i + 3) % 4]
end

create step(x, y, facing):
    when facing:
        is "N": return (x, y - 1)
        is "S": return (x, y + 1)
        is "E": return (x + 1, y)
        else: return (x - 1, y)
    end
end

create run(commands):
    x = 2
    y = 2
    facing = "N"
    each c in commands:
        when c:
            is "L": facing = turn_left(facing)
            is "R": facing = turn_right(facing)
            is "F":
                nx, ny = step(x, y, facing)
                if 0 <= nx <= 4 and 0 <= ny <= 4:
                    x = nx
                    y = ny
                end
        end
    end
    return "({x}, {y}) facing {facing}"
end

main:
    print(turn_right("W"))
    print(run("FFRFFLFRRF"))
end
```

</details>

`return (x, y - 1)` gives back two values at once, as a **tuple**, and
`nx, ny = step(...)` unpacks them into two variables. Day 9 covers tuples.

**Day 6 done.** You can create functions, pass them values, and get answers
back. Tomorrow you use everything so far to build a complete word game.
