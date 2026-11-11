# Day 5: Loops

Computers never get bored. A **loop** runs the same lines again and again:
once for each item in a list, a set number of times, or for as long as
something stays true. Today ends with a password generator.

## 5.1 `each`: once for every item

```vyrn
main:
    fruits = ["apple", "banana", "cherry"]
    each fruit in fruits:
        print("I like", fruit)
    end
    print("Done!")
end
```

```text
I like apple
I like banana
I like cherry
Done!
```

Read it as "for **each** fruit **in** fruits, do this". The block runs
three times; each time, `fruit` holds the next item. You choose the name:
`each f in fruits` works just as well, but a clear name helps.

`each` also walks through the letters of a string:

```vyrn
main:
    each letter in "Vyrn":
        print(letter)
    end
end
```

```text
V
y
r
n
```

### Exercise

Print each name in the list with a greeting: `Welcome, <name>!`

```vyrn starter
main:
    guests = ["Ada", "Grace", "Linus"]
end
```

```text expect
Welcome, Ada!
Welcome, Grace!
Welcome, Linus!
```

> Hint: `each guest in guests:` ... `end`

<details><summary>Solution</summary>

```vyrn solution
main:
    guests = ["Ada", "Grace", "Linus"]
    each guest in guests:
        print("Welcome, {guest}!")
    end
end
```

</details>

## 5.2 Counting with `to`, `until` and `by`

To count, give `each` a **range**:

```vyrn
main:
    each i in 1 to 5: print(i)
    print("---")
    each i in 0 until 3: print(i)
    print("---")
    each i in 0 to 20 by 5: print(i)
    print("---")
    each i in 3 to 1 by -1: print(i)
end
```

```text
1
2
3
4
5
---
0
1
2
---
0
5
10
15
20
---
3
2
1
```

- `1 to 5` counts 1, 2, 3, 4, 5: `to` **includes** the end.
- `0 until 3` counts 0, 1, 2: `until` **stops before** the end. It fits
  list positions, which go from 0 until the length.
- `by` sets the step, and a negative step counts down.

A loop with one line can go on the header line, with no `end`.

### Exercise

Print the 7 times table from `7 x 1 = 7` to `7 x 10 = 70`.

```vyrn starter
main:
end
```

```text expect
7 x 1 = 7
7 x 2 = 14
7 x 3 = 21
7 x 4 = 28
7 x 5 = 35
7 x 6 = 42
7 x 7 = 49
7 x 8 = 56
7 x 9 = 63
7 x 10 = 70
```

> Hint: `each n in 1 to 10:`
> Hint: `print("7 x {n} = {7 * n}")`

<details><summary>Solution</summary>

```vyrn solution
main:
    each n in 1 to 10:
        print("7 x {n} = {7 * n}")
    end
end
```

</details>

## 5.3 Adding things up

A very common pattern: start a variable before the loop, and update it
each time round. This is called an **accumulator**:

```vyrn
main:
    prices = [4.5, 12.0, 3.25]
    total = 0.0
    each p in prices:
        total += p
    end
    print("Total: {total:.2f}")

    scores = [72, 95, 88, 61]
    best = scores[0]
    each s in scores:
        if s > best: best = s
    end
    print("Best:", best)
end
```

```text
Total: 19.75
Best: 95
```

Vyrn has these built in too: `sum(prices)`, `max(scores)`, `min(scores)`.
Writing them yourself once shows how they work, and the same pattern
solves problems no built-in covers.

### Exercise

Count how many scores are 80 or more, and work out their average.

```vyrn starter
main:
    scores = [72, 95, 88, 61, 80, 99]
end
```

```text expect
4 scores of 80 or more, averaging 90.5
```

> Hint: Keep two accumulators: `count = 0` and `total = 0`.
> Hint: Inside the loop, `if s >= 80:` add to both.

<details><summary>Solution</summary>

```vyrn solution
main:
    scores = [72, 95, 88, 61, 80, 99]
    count = 0
    total = 0
    each s in scores:
        if s >= 80:
            count += 1
            total += s
        end
    end
    print("{count} scores of 80 or more, averaging {total / count}")
end
```

</details>

## 5.4 `loop`: repeat while something is true

When you don't know in advance how many times to repeat, use `loop` with a
condition. It runs while the condition is true:

```vyrn
main:
    money = 100
    years = 0
    loop money < 200:
        money *= 1.1
        years += 1
    end
    print("Doubled after {years} years: {money:.2f}")
end
```

```text
Doubled after 8 years: 214.36
```

Be careful: if the condition never becomes false, the loop never ends.
(Press Ctrl+C to stop a program that's stuck.) `loop:` with no condition
runs forever, until a `stop` inside it, which is the next lesson.

### Exercise

Starting from 1, keep doubling a number until it is bigger than 1000.
Print each value.

```vyrn starter
main:
    n = 1
end
```

```text expect
1
2
4
8
16
32
64
128
256
512
```

> Hint: `loop n <= 1000:` then print, then double.
> Hint: Print *before* doubling, so 1024 is never printed.

<details><summary>Solution</summary>

```vyrn solution
main:
    n = 1
    loop n <= 1000:
        print(n)
        n *= 2
    end
end
```

</details>

## 5.5 `stop` and `skip`

- `stop` leaves the loop at once.
- `skip` jumps straight to the next round.

```vyrn
main:
    each n in 1 to 10:
        if n % 2 == 0: skip
        if n > 7: stop
        print(n)
    end

    answers = ["maybe", "no", "yes", "later"]
    each a in answers:
        if a == "yes":
            print("Found a yes!")
            stop
        end
        print("Not yet:", a)
    end
end
```

```text
1
3
5
7
Not yet: maybe
Not yet: no
Found a yes!
```

### Exercise

Keep asking for words until the person types `done`, then print how many
words they typed. The checker types `red`, `green`, `blue`, `done`.

```vyrn starter
main:
    count = 0
    loop:
        word = input("Word: ")
    end
end
```

```text input
red
green
blue
done
```

```text expect
Word: Word: Word: Word: You typed 3 words.
```

> Hint: `if word == "done": stop` comes *before* counting the word.

<details><summary>Solution</summary>

```vyrn solution
main:
    count = 0
    loop:
        word = input("Word: ")
        if word == "done": stop
        count += 1
    end
    print("You typed {count} words.")
end
```

</details>

## 5.6 Loops inside loops

A loop can run inside another. For every round of the outer loop, the
inner loop runs completely:

```vyrn
main:
    each row in 1 to 3:
        line = ""
        each col in 1 to 4:
            line += "{row * col:>4}"
        end
        print(line)
    end
end
```

```text
   1   2   3   4
   2   4   6   8
   3   6   9  12
```

`:>4` right-aligns each number in 4 spaces, so the columns line up.

### Exercise

Draw a triangle of `*`s, 5 rows high: one star on the first row, five on
the last.

```vyrn starter
main:
end
```

```text expect
*
**
***
****
*****
```

> Hint: `each row in 1 to 5:` and an inner loop adding `row` stars to a line.
> Hint: Or, without an inner loop: `"*" * row`.

<details><summary>Solution</summary>

```vyrn solution
main:
    each row in 1 to 5:
        line = ""
        each _ in 1 to row:
            line += "*"
        end
        print(line)
    end
end
```

</details>

## 5.7 Project: the Password Generator

Ask how many letters, digits and symbols the password should have, pick
them at random, shuffle them, and print the result.

`list("abc")` turns a string into a list of its characters:
`["a", "b", "c"]`. `"".join(xs)` does the opposite, gluing a list of
strings into one.

### Exercise

The checker asks for 6 letters, 2 digits and 2 symbols. Keep the seed.

```vyrn starter
use vyrn.random

main:
    random.seed(9)
    letters = list("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")
    digits = list("0123456789")
    symbols = list("!#$%&*+?")
    print("Welcome to the password generator!")
end
```

```text input
6
2
2
```

```text expect
Welcome to the password generator!
How many letters? How many digits? How many symbols? Your password is: d%d8+wls1Q
```

> Hint: Three `int(input(...))` questions, then an empty list `chars = []`.
> Hint: One loop per kind: `each _ in 1 to n: chars.append(random.choice(letters))`.
> Hint: `random.shuffle(chars)`, then `"".join(chars)`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

main:
    random.seed(9)
    letters = list("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")
    digits = list("0123456789")
    symbols = list("!#$%&*+?")
    print("Welcome to the password generator!")
    n_letters = int(input("How many letters? "))
    n_digits = int(input("How many digits? "))
    n_symbols = int(input("How many symbols? "))
    chars = []
    each _ in 1 to n_letters: chars.append(random.choice(letters))
    each _ in 1 to n_digits: chars.append(random.choice(digits))
    each _ in 1 to n_symbols: chars.append(random.choice(symbols))
    random.shuffle(chars)
    print("Your password is:", "".join(chars))
end
```

</details>

`_` is the name to use for a loop variable you don't need.

**Day 5 done.** Loops let your programs do a lot of work with a little
code. Tomorrow: functions, to give a name to a piece of work.
