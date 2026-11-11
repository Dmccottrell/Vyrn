# Unit 1: The Basics (Days 1–10)

You will learn to print, store values, do arithmetic, make decisions,
repeat things, and write your own functions. By Day 10 you will have built
a word-guessing game and a calculator.

Every program on these pages runs exactly as shown: type it into a file
such as `day1.vyrn` and run `vyrn day1.vyrn`, or try lines one at a time
in the prompt by typing `vyrn` on its own.

---

## Day 1: Printing and text

**Goal:** make Vyrn say things.

Every Vyrn program starts at `main:` and ends with `end`. `print` shows
values, separated by spaces. Text goes in double quotes, and anything in
`{braces}` inside text is filled in.

```vyrn
main:
    print("Hello, world!")
    print("Vyrn", "is", "ready")
    name = "Ada"
    print("Hello, {name}!")
    print("Your name has {len(name)} letters.")
    print("A" + "da", "ha" * 3)
end
```

```text
Hello, world!
Vyrn is ready
Hello, Ada!
Your name has 3 letters.
Ada hahaha
```

`+` joins text and `*` repeats it. `len` counts characters. Lines starting
with `#` are comments: notes for people, ignored by Vyrn.

**Try it**

1. Print your name and your favourite food on separate lines.
2. Put your name in a variable and print it three times on one line.

**Project: a poster.** Print a small poster for an event, with a border of
`=` signs as wide as the title.

<details><summary>A solution</summary>

```vyrn
main:
    title = "Robot Fair 2026"
    border = "=" * len(title)
    print(border)
    print(title)
    print(border)
    print("Saturday, 10am, Town Hall")
end
```

```text
===============
Robot Fair 2026
===============
Saturday, 10am, Town Hall
```

</details>

---

## Day 2: Numbers and arithmetic

**Goal:** calculate.

Whole numbers are `int`s and numbers with a point are `float`s. `/` always
gives a float; `//` divides and rounds down; `%` is the remainder; `**` is
a power. `{value:.2f}` shows a number with two decimals.

```vyrn
main:
    print(7 + 3, 7 - 3, 7 * 3, 7 / 2, 7 // 2, 7 % 2, 2 ** 10)
    price = 19.99
    quantity = 3
    total = price * quantity
    print("Total: {total:.2f}")
    print(10 / 3, "or rounded: {10 / 3:.2f}")
    print(round(2.567, 1), abs(-4), int("42") + 1, float("2.5") * 2)
end
```

```text
10 4 21 3.5 3 1 1024
Total: 59.97
3.3333333333333335 or rounded: 3.33
2.6 4 43 5.0
```

`int(text)` and `float(text)` turn text into numbers; `str(number)` goes
the other way.

**Try it**

1. Work out how many seconds there are in a week.
2. Convert 100 degrees Fahrenheit to Celsius: `(f - 32) * 5 / 9`.

**Project: split the bill.** Given a bill, a tip percentage and a number of
people, print what each person pays, to two decimals.

<details><summary>A solution</summary>

```vyrn
main:
    bill = 124.50
    tip_percent = 15
    people = 4
    total = bill * (1 + tip_percent / 100)
    each_pays = total / people
    print("Bill with tip: {total:.2f}")
    print("Each person pays: {each_pays:.2f}")
end
```

```text
Bill with tip: 143.17
Each person pays: 35.79
```

</details>

---

## Day 3: Decisions with `if`

**Goal:** make a program choose.

`if` runs a block only when a condition is true. `else if` and `else` give
other choices. A block ends with `end`. Comparisons are `==`, `!=`, `<`,
`<=`, `>`, `>=`; combine them with `and`, `or` and `not`.

```vyrn
main:
    age = 15
    height = 140
    if age >= 18:
        print("Adult ticket: 12")
    else if age >= 12:
        print("Teen ticket: 8")
    else:
        print("Child ticket: 5")
    end
    if height >= 120 and age >= 10:
        print("You can ride the rollercoaster.")
    end
    if not (age > 65): print("No senior discount.")
end
```

```text
Teen ticket: 8
You can ride the rollercoaster.
No senior discount.
```

A one-line `if` puts its single statement after the `:` and needs no `end`.

**Try it**

1. Print whether a number is even or odd (`n % 2 == 0`).
2. Print whether a year is a leap year: divisible by 4, but not by 100
   unless also by 400.

**Project: a theme-park gate.** Work out the ticket price from age, add 3
for a photo if wanted, and print the total.

<details><summary>A solution</summary>

```vyrn
main:
    age = 9
    wants_photo = true
    price = 0
    if age < 4:
        price = 0
    else if age < 12:
        price = 5
    else if age < 18:
        price = 8
    else:
        price = 12
    end
    if wants_photo: price += 3
    print("Please pay {price}.")
end
```

```text
Please pay 8.
```

`price` is created *before* the `if`. A variable first made inside a block
belongs to that block and is gone after its `end`, so a value you want
afterwards must already exist outside it.

</details>

---

## Day 4: Randomness and lists

**Goal:** pick things by chance and keep many values together.

`use vyrn.random` brings in the random module. A list keeps values in
order: `[a, b, c]`. Positions count from 0, and `-1` is the last item.

```vyrn
use vyrn.random

main:
    random.seed(4)
    fruits = ["apple", "banana", "cherry"]
    print(fruits[0], fruits[-1], len(fruits))
    fruits.append("date")
    print(fruits)
    print(random.int(1, 6), random.choice(fruits))
    hands = ["rock", "paper", "scissors"]
    print("Computer plays", random.choice(hands))
end
```

```text
apple cherry 3
["apple", "banana", "cherry", "date"]
3 cherry
Computer plays rock
```

`random.seed(n)` makes the "random" choices the same every run, which is
handy for checking work. Leave it out to get new choices each time.

**Try it**

1. Make a list of three friends and print one at random to pay for lunch.
2. Print a random number between 1 and 100.

**Project: rock, paper, scissors.** The player's choice is in a variable;
the computer chooses at random; print who wins.

<details><summary>A solution</summary>

```vyrn
use vyrn.random

main:
    random.seed(2)
    hands = ["rock", "paper", "scissors"]
    player = "rock"
    computer = random.choice(hands)
    print("You: {player}, computer: {computer}")
    if player == computer:
        print("Draw!")
    else if (player == "rock" and computer == "scissors") or (player == "paper" and computer == "rock") or (player == "scissors" and computer == "paper"):
        print("You win!")
    else:
        print("You lose!")
    end
end
```

```text
You: rock, computer: paper
You lose!
```

</details>

---

## Day 5: Loops

**Goal:** repeat work.

`each item in list:` runs a block once per item. `each i in 1 to 5`
counts 1, 2, 3, 4, 5 (`to` includes the end; `until` stops before it).
`loop condition:` repeats while the condition holds. `stop` leaves a loop
early and `skip` goes on to the next round.

```vyrn
main:
    each name in ["Ada", "Alan", "Grace"]:
        print("Hi", name)
    end
    total = 0
    each n in 1 to 100: total += n
    print("1 + 2 + ... + 100 =", total)
    each i in 0 until 10 by 3: print(i)
    count = 3
    loop count > 0:
        print(count)
        count -= 1
    end
    print("Lift off!")
end
```

```text
Hi Ada
Hi Alan
Hi Grace
1 + 2 + ... + 100 = 5050
0
3
6
9
3
2
1
Lift off!
```

**Try it**

1. Print the 7 times table from 7 × 1 to 7 × 12.
2. Add up only the even numbers from 1 to 50 (use `skip` for odd ones).

**Project: a password generator.** Build a password from a number of
letters, digits and symbols, then shuffle it.

<details><summary>A solution</summary>

```vyrn
use vyrn.random

main:
    random.seed(11)
    letters = list("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")
    digits = list("0123456789")
    symbols = list("!#$%&*+")
    chars = []
    each _ in 1 to 8: chars.append(random.choice(letters))
    each _ in 1 to 3: chars.append(random.choice(digits))
    each _ in 1 to 2: chars.append(random.choice(symbols))
    random.shuffle(chars)
    print("Your password:", "".join(chars))
end
```

```text
Your password: ywvIo53Wsi&6&
```

</details>

---

## Day 6: Functions

**Goal:** name a piece of work and reuse it.

`create` declares a function. It takes parameters, and `return` gives a
result back. Write functions above `main:`.

```vyrn
create greet(name):
    print("Hello, {name}!")
end

create area(width, height):
    return width * height
end

create describe(temp_c):
    if temp_c > 25: return "hot"
    if temp_c > 15: return "mild"
    return "cold"
end

main:
    greet("Ada")
    print(area(3, 4))
    each t in [30, 20, 5]:
        print(t, "is", describe(t))
    end
end
```

```text
Hello, Ada!
12
30 is hot
20 is mild
5 is cold
```

**Try it**

1. Write `square(n)` that returns `n * n`.
2. Write `is_even(n)` that returns `true` or `false`.

**Project: a unit converter.** Functions for kilometres to miles, kilograms
to pounds and Celsius to Fahrenheit, then a table of conversions.

<details><summary>A solution</summary>

```vyrn
create km_to_miles(km): return km * 0.621371
create kg_to_lb(kg): return kg * 2.20462
create c_to_f(c): return c * 9 / 5 + 32

main:
    each km in [1, 5, 10, 42.195]:
        print("{km} km = {km_to_miles(km):.2f} miles")
    end
    print("70 kg = {kg_to_lb(70):.1f} lb")
    print("37 C = {c_to_f(37):.1f} F")
end
```

```text
1 km = 0.62 miles
5 km = 3.11 miles
10 km = 6.21 miles
42.195 km = 26.22 miles
70 kg = 154.3 lb
37 C = 98.6 F
```

</details>

---

## Day 7: Project day: Guess the Word

**Goal:** combine everything so far into a game.

The computer picks a secret word. The player guesses letters; correct ones
appear, wrong ones cost a life. Here the guesses come from a list so the
game can be checked; Day 12 shows how to read them from the keyboard.

```vyrn
use vyrn.random

create show(word, found):
    shown = ""
    each c in word.chars():
        shown += c if c in found else "_"
    end
    return shown
end

main:
    random.seed(1)
    word = random.choice(["planet", "rocket", "galaxy"])
    found = []
    lives = 5
    each guess in ["a", "e", "z", "t", "p", "n", "l"]:
        if guess in word:
            found.append(guess)
            print("{guess}: yes  {show(word, found)}")
        else:
            lives -= 1
            print("{guess}: no   lives left: {lives}")
        end
        if "_" not in show(word, found):
            print("You found {word}!")
            stop
        end
        if lives == 0:
            print("Out of lives. It was {word}.")
            stop
        end
    end
end
```

```text
a: yes  __a___
e: yes  __a_e_
z: no   lives left: 4
t: yes  __a_et
p: yes  p_a_et
n: yes  p_anet
l: yes  planet
You found planet!
```

`word.chars()` gives the letters of a word as a list, and `in` checks
whether something is in a list or a string.

**Project:** extend the game: show the letters already guessed, and don't
take a life for guessing the same wrong letter twice.

---

## Day 8: Parameters, defaults and the Caesar cipher

**Goal:** functions with more inputs.

Parameters can have defaults, and callers can name them:
`price(9.99, tax = 0.2)`. Functions can call other functions.

```vyrn
create price_with_tax(amount, tax = 0.2):
    return amount * (1 + tax)
end

create is_prime(n):
    if n < 2: return false
    each d in 2 until n:
        if d * d > n: stop
        if n % d == 0: return false
    end
    return true
end

main:
    print("{price_with_tax(10):.2f}", "{price_with_tax(10, tax = 0.05):.2f}")
    print([n each n in 1 to 30 if is_prime(n)])
end
```

```text
12.00 10.50
[2, 3, 5, 7, 11, 13, 17, 19, 23, 29]
```

`[n each n in 1 to 30 if is_prime(n)]` is a *comprehension*: a list built
in one line from a loop and a condition.

**Project: the Caesar cipher.** Shift every letter by a number of places
to encrypt; shift back to decrypt.

<details><summary>A solution</summary>

```vyrn
const ALPHABET = "abcdefghijklmnopqrstuvwxyz"

create caesar(text, shift):
    out = ""
    each c in text.chars():
        i = ALPHABET.find(c)
        if i == -1:
            out += c
        else:
            out += ALPHABET[(i + shift) % 26]
        end
    end
    return out
end

main:
    secret = caesar("meet me at noon", 3)
    print(secret)
    print(caesar(secret, -3))
end
```

```text
phhw ph dw qrrq
meet me at noon
```

</details>

---

## Day 9: Maps and nesting

**Goal:** look things up by name.

A map (Python calls it a dictionary) links keys to values:
`{"Ada": 92}`. Lists and maps can hold each other, as deep as you like.

```vyrn
main:
    scores = {"Ada": 92, "Alan": 78}
    scores["Grace"] = 85
    print(scores["Ada"], len(scores), "Alan" in scores)
    each name, score in scores.items():
        print("{name}: {score}")
    end
    print(scores.get("Bob", 0))
    travel = {
        "France": {"cities": ["Paris", "Lyon"], "visits": 2},
        "Japan": {"cities": ["Tokyo"], "visits": 1},
    }
    print(travel["France"]["cities"][1], travel["Japan"]["visits"])
end
```

```text
92 3 true
Ada: 92
Alan: 78
Grace: 85
0
Lyon 1
```

`m.get(key, default)` gives the default when the key is missing, instead
of failing.

**Project: a gradebook.** Turn scores into grades in a new map, then print
the class average and the best student.

<details><summary>A solution</summary>

```vyrn
create grade(score):
    if score >= 90: return "A"
    if score >= 80: return "B"
    if score >= 70: return "C"
    return "F"
end

main:
    scores = {"Ada": 92, "Alan": 78, "Grace": 85, "Linus": 64}
    grades = {}
    each name, score in scores.items():
        grades[name] = grade(score)
    end
    print(grades)
    average = sum(scores.values()) / len(scores)
    best = max(scores.keys(), key = n => scores[n])
    print("Average: {average:.1f}, best: {best}")
end
```

```text
{"Ada": "A", "Alan": "C", "Grace": "B", "Linus": "F"}
Average: 79.8, best: Ada
```

`key = n => scores[n]` tells `max` to compare names by their scores. `n =>
scores[n]` is a small function written inline.

</details>

---

## Day 10: Return values: a calculator

**Goal:** functions that give back results you can keep using.

Functions are values too: they can be stored in a map and called later.

```vyrn
create add(a, b): return a + b
create subtract(a, b): return a - b
create multiply(a, b): return a * b
create divide(a, b): return a / b

main:
    operations = {"+": add, "-": subtract, "*": multiply, "/": divide}
    result = 10
    each step in [["+", 5], ["*", 3], ["-", 6], ["/", 4]]:
        op = step[0]
        n = step[1]
        before = result
        result = operations[op](result, n)
        print("{before} {op} {n} = {result}")
    end
end
```

```text
10 + 5 = 15
15 * 3 = 45
45 - 6 = 39
39 / 4 = 9.75
```

**Project:** add `**` (power) and `%` (remainder) to the calculator, and
make dividing by zero print a friendly message instead of failing (Day 13
shows another way).

---

**Unit 1 done.** You can print, calculate, decide, repeat, keep data in
lists and maps, and write functions. Next: debugging, objects, and your
first graphics.
