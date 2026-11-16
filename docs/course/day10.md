# Day 10: Functions with Outputs

Functions that give back answers are the building blocks of every larger
program. Today you see how to design them well, discover that functions are
values you can store and pass around, meet **recursion** (a function that
calls itself), and build a calculator.

## 10.1 Many small functions, one clear program

Compare a program that does everything in `main` with one split into
functions that each `return` one answer:

```vyrn
create average(numbers):
    return sum(numbers) / len(numbers)
end

create grade(score):
    if score >= 90: return "A"
    if score >= 80: return "B"
    if score >= 70: return "C"
    return "F"
end

create report(name, scores):
    avg = average(scores)
    return "{name}: {avg:.1f} ({grade(avg)})"
end

main:
    print(report("Ada", [95, 88, 92]))
    print(report("Ben", [72, 65, 80]))
end
```

```text
Ada: 91.7 (A)
Ben: 72.3 (C)
```

Each function can be understood, tested and fixed on its own. A good rule:
if you can't describe what a function does in one sentence, split it.

### Exercise

Write `is_palindrome(word)`: `true` if the word reads the same backwards,
ignoring capitals. Then print the palindromes in the list.

```vyrn starter
main:
    words = ["Level", "vyrn", "Racecar", "noon", "moon"]
end
```

```text expect
Level
Racecar
noon
```

> Hint: `w = word.lower()` then `return w == w[::-1]`.

<details><summary>Solution</summary>

```vyrn solution
create is_palindrome(word):
    w = word.lower()
    return w == w[::-1]
end

main:
    words = ["Level", "vyrn", "Racecar", "noon", "moon"]
    each word in words:
        if is_palindrome(word): print(word)
    end
end
```

</details>

## 10.2 Returning several values

A function can return a tuple and the caller unpacks it:

```vyrn
create divide(a, b):
    return (a // b, a % b)
end

create stats(numbers):
    return (min(numbers), max(numbers), sum(numbers) / len(numbers))
end

main:
    q, r = divide(17, 5)
    print("17 / 5 = {q} remainder {r}")
    low, high, mean = stats([4, 8, 15, 16, 23, 42])
    print(low, high, mean)
end
```

```text
17 / 5 = 3 remainder 2
4 42 18.0
```

(`divmod(17, 5)` is built in and does the same as `divide`.)

### Exercise

Write `split_name(full)` returning the first and last name, and use it.

```vyrn starter
create split_name(full):
    return ("", "")
end

main:
    first, last = split_name("Katherine Johnson")
    print("Last: {last}, first: {first}")
end
```

```text expect
Last: Johnson, first: Katherine
```

> Hint: `parts = full.split()` then `return (parts[0], parts[-1])`.

<details><summary>Solution</summary>

```vyrn solution
create split_name(full):
    parts = full.split()
    return (parts[0], parts[-1])
end

main:
    first, last = split_name("Katherine Johnson")
    print("Last: {last}, first: {first}")
end
```

</details>

## 10.3 Functions are values

A function's name, without `()`, is the function itself. You can store it
in a variable, put it in a list or map, and pass it to another function:

```vyrn
create shout(text): return text.upper() + "!"
create whisper(text): return text.lower() + "..."

create apply_to_all(f, words):
    result = []
    each w in words: result.append(f(w))
    return result
end

main:
    speak = shout
    print(speak("hello"))
    print(apply_to_all(whisper, ["HI", "THERE"]))
    styles = {"loud": shout, "quiet": whisper}
    print(styles["quiet"]("Good night"))
end
```

```text
HELLO!
["hi...", "there..."]
good night...
```

A function that takes or returns another function is called a
**higher-order function**. You'll use them all the time: `sorted(xs, key =
f)` takes one, for example.

### Exercise

Write `apply_twice(f, x)` that returns `f(f(x))`, and test it with
`double` and with `exclaim`.

```vyrn starter
create double(n): return n * 2
create exclaim(s): return s + "!"

create apply_twice(f, x):
    return x
end

main:
    print(apply_twice(double, 5))
    print(apply_twice(exclaim, "wow"))
end
```

```text expect
20
wow!!
```

> Hint: `create apply_twice(f, x): return f(f(x))`

<details><summary>Solution</summary>

```vyrn solution
create double(n): return n * 2
create exclaim(s): return s + "!"
create apply_twice(f, x): return f(f(x))

main:
    print(apply_twice(double, 5))
    print(apply_twice(exclaim, "wow"))
end
```

</details>

## 10.4 Small functions without a name: `=>`

For a tiny function used once, write it on the spot with an arrow:
`x => x * 2` means "a function that takes `x` and gives back `x * 2`". With
more than one parameter, use parentheses: `(a, b) => a + b`.

```vyrn
main:
    double = x => x * 2
    add = (a, b) => a + b
    print(double(21), add(2, 3))
    words = ["kiwi", "fig", "banana", "apple"]
    print(sorted(words, key = w => len(w)))
    print(words.map(w => w.upper()))
    print(words.filter(w => "a" in w))
end
```

```text
42 5
["fig", "kiwi", "apple", "banana"]
["KIWI", "FIG", "BANANA", "APPLE"]
["banana", "apple"]
```

`xs.map(f)` gives a new list of `f` applied to each item; `xs.filter(f)`
keeps the items for which `f` gives `true`. Day 26 goes further with these.

### Exercise

Sort the players by score, highest first, and print their names.

```vyrn starter
main:
    players = [("Ana", 72), ("Bo", 95), ("Cy", 88)]
end
```

```text expect
["Bo", "Cy", "Ana"]
```

> Hint: `sorted(players, key = p => p[1], reverse = true)`
> Hint: Then `.map(p => p[0])` for the names.

<details><summary>Solution</summary>

```vyrn solution
main:
    players = [("Ana", 72), ("Bo", 95), ("Cy", 88)]
    ranked = sorted(players, key = p => p[1], reverse = true)
    print(ranked.map(p => p[0]))
end
```

</details>

## 10.5 Recursion: a function that calls itself

Some problems contain smaller copies of themselves. The factorial of 5 is
`5 × 4 × 3 × 2 × 1`, which is `5 × (factorial of 4)`. A function can
solve it by calling itself on the smaller problem:

```vyrn
create factorial(n):
    if n <= 1: return 1
    return n * factorial(n - 1)
end

create countdown(n):
    if n == 0:
        print("Lift off!")
        return
    end
    print(n)
    countdown(n - 1)
end

main:
    print(factorial(5), factorial(20))
    countdown(3)
end
```

```text
120 2432902008176640000
3
2
1
Lift off!
```

Every recursive function needs a **base case** that stops it (`n <= 1`,
`n == 0`), and each call must move closer to it. Without one, the function
calls itself forever, until Vyrn stops it with a `StackOverflow` error.

### Exercise

Write `sum_digits(n)` recursively: the sum of the digits of 4096 is
4 + 0 + 9 + 6 = 19.

```vyrn starter
create sum_digits(n):
    return n
end

main:
    print(sum_digits(4096))
    print(sum_digits(7))
end
```

```text expect
19
7
```

> Hint: The last digit is `n % 10`; the rest is `n // 10`.
> Hint: Base case: a single digit (`n < 10`) is its own sum.

<details><summary>Solution</summary>

```vyrn solution
create sum_digits(n):
    if n < 10: return n
    return n % 10 + sum_digits(n // 10)
end

main:
    print(sum_digits(4096))
    print(sum_digits(7))
end
```

</details>

## 10.6 Project: the Calculator

Put the operations in a map from symbol to function. The calculator keeps
a running result: the person types an operator and a number, and the
result updates, until they type `=`.

### Exercise

The checker starts from `10`, then types `+ 5`, `* 3`, `/ 4` and `=`.

```vyrn starter
create add(a, b): return a + b
create subtract(a, b): return a - b
create multiply(a, b): return a * b
create divide(a, b): return a / b

main:
    operations = {"+": add, "-": subtract, "*": multiply, "/": divide}
    result = float(input("Start with: "))
end
```

```text input
10
+ 5
* 3
/ 4
=
```

```text expect
Start with: Next (like + 5, or =): 10.0 + 5.0 = 15.0
Next (like + 5, or =): 15.0 * 3.0 = 45.0
Next (like + 5, or =): 45.0 / 4.0 = 11.25
Next (like + 5, or =): Result: 11.25
```

> Hint: Inside a `loop:`, read a line and `stop` if it is `=`.
> Hint: `symbol, number = line.split()` then `operations[symbol](result, float(number))`.

<details><summary>Solution</summary>

```vyrn solution
create add(a, b): return a + b
create subtract(a, b): return a - b
create multiply(a, b): return a * b
create divide(a, b): return a / b

main:
    operations = {"+": add, "-": subtract, "*": multiply, "/": divide}
    result = float(input("Start with: "))
    loop:
        line = input("Next (like + 5, or =): ")
        if line == "=": stop
        symbol, number = line.split()
        n = float(number)
        new = operations[symbol](result, n)
        print("{result} {symbol} {n} = {new}")
        result = new
    end
    print("Result:", result)
end
```

</details>

**Try it:** add `**` for powers, and print `Unknown operator` instead of
failing when the symbol isn't in the map (`symbol in operations`).

**Day 10 done, and Unit 1 with it!** You can write real programs: they
calculate, decide, repeat, keep data in collections, and are organised into
functions. Unit 2 builds bigger games, and the tools that keep bigger
programs tidy: scope, error handling and objects.
