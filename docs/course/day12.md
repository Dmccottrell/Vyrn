# Day 12: Scope, `let`, `var` and `const`

Programs get bigger, and with more names comes more chance of mixing them
up. Today you learn exactly where each name lives (its **scope**), and how
to say "this never changes" so Vyrn can hold you to it. The project is a
number-guessing game with difficulty levels.

## 12.1 Block scope

A variable created inside a block (an `if`, a loop, a function) belongs to
that block and disappears at its `end`:

```vyrn check
main:
    temperature = 25
    if temperature > 20:
        advice = "wear a hat"
        print(advice)
    end
    # `advice` doesn't exist here.
end
```

Trying to use `advice` after the `end` is an error that `vyrn check`
catches before running:

```text
error: `advice` is not defined
```

When you need a value after a block, create the variable **before** it:

```vyrn
main:
    temperature = 25
    advice = "no advice"
    if temperature > 20:
        advice = "wear a hat"
    end
    print(advice)
end
```

```text
wear a hat
```

Why does Vyrn work this way? Because it keeps mistakes local: a name made
for a moment inside a loop can't accidentally clash with something
elsewhere. (Python lets block variables leak out; C++, Java and Vyrn
don't.)

### Exercise

This program should print the largest even number in the list, but it
doesn't compile. Fix it by creating `largest` in the right place.

```vyrn starter broken
main:
    numbers = [3, 8, 5, 12, 7]
    each n in numbers:
        if n % 2 == 0:
            largest = n
        end
    end
    print("Largest even:", largest)
end
```

```text expect
Largest even: 12
```

> Hint: Create `largest = 0` before the loop.
> Hint: Inside, only replace it when `n` is even **and** bigger: `if n % 2 == 0 and n > largest:`.

<details><summary>Solution</summary>

```vyrn solution
main:
    numbers = [3, 8, 5, 12, 7]
    largest = 0
    each n in numbers:
        if n % 2 == 0 and n > largest:
            largest = n
        end
    end
    print("Largest even:", largest)
end
```

</details>

## 12.2 Function scope, and sharing without globals

Each call of a function gets its own fresh variables. A function sees its
parameters, its own variables, and the file's top-level declarations
(functions, `const`s, types), but **not** the variables of `main` or of
other functions.

```vyrn
const BONUS = 5

create add_bonus(score):
    total = score + BONUS
    return total
end

main:
    score = 10
    print(add_bonus(score))
    print(add_bonus(20))
end
```

```text
15
25
```

Many languages have **global variables**, changeable from anywhere. They
make programs hard to follow: any function might change anything. Vyrn
has no global variables. Values travel in through parameters and out
through `return`, so you can always see where a value came from. Day 16
shows the other way: an **object** that keeps its own changing state.

### Exercise

`roll_total` should add up the dice, and `main` should print the total of
both rounds. Make it work by passing values in and returning them, without
any variable shared between functions.

```vyrn starter
create roll_total(rolls):
    total = 0
    each r in rolls: total += r
end

main:
    first = roll_total([3, 5, 6])
    second = roll_total([1, 2])
    print("Total of both rounds:", first + second)
end
```

```text expect
Total of both rounds: 17
```

> Hint: `roll_total` computes `total` but never gives it back.

<details><summary>Solution</summary>

```vyrn solution
create roll_total(rolls):
    total = 0
    each r in rolls: total += r
    return total
end

main:
    first = roll_total([3, 5, 6])
    second = roll_total([1, 2])
    print("Total of both rounds:", first + second)
end
```

</details>

## 12.3 `let` and `var`: saying what changes

Plain `x = 1` is fine for short programs. In longer ones it helps to say
what you mean:

- `let name = value` creates a name that will **never** be given a new
  value.
- `var name = value` creates a variable you **plan** to change.

```vyrn
main:
    let tax_rate = 0.2
    var total = 0.0
    each price in [10.0, 20.0, 5.0]:
        total += price * (1 + tax_rate)
    end
    print("{total:.2f}")
end
```

```text
42.00
```

If you later write `tax_rate = 0.3` by mistake, Vyrn refuses, and says how
to fix it if you really meant it:

```text
error: cannot assign to `tax_rate`: it is declared with `let`
  = help: declare it with `var tax_rate` to change it
```

A `let` list can still have items added (the name keeps pointing at the
same list). What `let` forbids is pointing the name at a different value.

### Exercise

Mark each variable `let` or `var` correctly: the program should still run
and print the same.

```vyrn starter
main:
    name = "Ada"
    lives = 3
    lives -= 1
    greeting = "Hi, {name}"
    print(greeting, lives)
end
```

```text expect
Hi, Ada 2
```

> Hint: Only `lives` changes after it is created.

<details><summary>Solution</summary>

```vyrn solution
main:
    let name = "Ada"
    var lives = 3
    lives -= 1
    let greeting = "Hi, {name}"
    print(greeting, lives)
end
```

</details>

## 12.4 `const`: values fixed for the whole program

A `const` at the top of a file is fixed before the program even starts,
and every function can read it. Use it for the settings of your program:

```vyrn
const MAX_TRIES = 5
const LEVELS = {"easy": 10, "hard": 5}
const WELCOME = "Guess my number!"

main:
    print(WELCOME)
    print("Easy gives {LEVELS["easy"]} tries, the default is {MAX_TRIES}.")
end
```

```text
Guess my number!
Easy gives 10 tries, the default is 5.
```

Writing constants in CAPITALS makes them stand out. Changing one later
(say, `MAX_TRIES = 7` in `main`) is an error.

### Exercise

Replace the repeated "magic number" `0.15` with a constant `DISCOUNT`, so
it's defined in one place.

```vyrn starter
create sale_price(price):
    return price * (1 - 0.15)
end

main:
    print("Discount: {0.15:.0%}")
    print("{sale_price(40):.2f}")
end
```

```text expect
Discount: 15%
34.00
```

> Hint: `const DISCOUNT = 0.15` at the top, then use `DISCOUNT` in both places.

<details><summary>Solution</summary>

```vyrn solution
const DISCOUNT = 0.15

create sale_price(price):
    return price * (1 - DISCOUNT)
end

main:
    print("Discount: {DISCOUNT:.0%}")
    print("{sale_price(40):.2f}")
end
```

</details>

## 12.5 Project: Guess the Number

The computer picks a number from 1 to 100; the player chooses easy (10
tries) or hard (5 tries) and guesses, with `Too high` and `Too low` as
clues.

### Exercise

The checker picks `easy`, then plays the smart way: always guess the middle
of what's left, so each clue halves the possibilities. With the seed kept,
it guesses `50`, `75`, `62`, `68`, `71`, `69`, `70`. (Halving finds any
number from 1 to 100 in at most 7 guesses: that idea is called **binary
search**, and it comes back on Day 79.)

```vyrn starter
use vyrn.random

const TRIES = {"easy": 10, "hard": 5}

main:
    random.seed(100)
    let secret = random.int(1, 100)
    print("I'm thinking of a number between 1 and 100.")
end
```

```text input
easy
50
75
62
68
71
69
70
```

```text expect
I'm thinking of a number between 1 and 100.
easy or hard? You have 10 tries. Guess: Too low.
You have 9 tries. Guess: Too high.
You have 8 tries. Guess: Too low.
You have 7 tries. Guess: Too low.
You have 6 tries. Guess: Too high.
You have 5 tries. Guess: Too low.
You have 4 tries. Guess: Yes! It was 70.
```

> Hint: `var left = TRIES[input("easy or hard? ")]`
> Hint: `loop left > 0:` read a guess; if it's right, print and `return`; otherwise give a clue and take a try away.
> Hint: After the loop, the player has run out of tries.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

const TRIES = {"easy": 10, "hard": 5}

main:
    random.seed(100)
    let secret = random.int(1, 100)
    print("I'm thinking of a number between 1 and 100.")
    var left = TRIES[input("easy or hard? ")]
    loop left > 0:
        let guess = int(input("You have {left} tries. Guess: "))
        if guess == secret:
            print("Yes! It was {secret}.")
            return
        else if guess > secret:
            print("Too high.")
        else:
            print("Too low.")
        end
        left -= 1
    end
    print("Out of tries. It was {secret}.")
end
```

</details>

`return` inside `main` ends the program early.

**Day 12 done.** You know where every name lives and how to make Vyrn guard
the ones that shouldn't change. Tomorrow: finding and handling errors.
