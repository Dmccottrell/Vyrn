# Day 13: Finding and Handling Errors

Every programmer spends a lot of time on **bugs**: programs that stop with
an error, or worse, run and give the wrong answer. Today you learn to read
errors, to hunt down wrong answers, and to make programs that recover
gracefully when something goes wrong.

## 13.1 Reading a runtime error

Some mistakes only show up while the program runs. Vyrn then stops and
reports the **kind** of error, a message, where it happened, and the chain
of function calls that led there:

```vyrn
create average(numbers):
    return sum(numbers) / len(numbers)
end

main:
    print(average([4, 6]))
    print(average([]))
end
```

```text
5.0
error: DivisionByZero: division by zero
```

In a terminal you see more:

```text
error: DivisionByZero: division by zero
 --> avg.vyrn:2
  |
2 |     return sum(numbers) / len(numbers)
  |
  = help: check that the number you divide by is not 0
  in average (avg.vyrn:2)
  in main (avg.vyrn:7)
```

Read the bottom lines from the bottom up: `main` on line 7 called
`average`, which failed on line 2. The fix belongs in whichever of them
should have handled an empty list.

### Exercise

Make `average` return `0` for an empty list instead of failing.

```vyrn starter
create average(numbers):
    return sum(numbers) / len(numbers)
end

main:
    print(average([4, 6]))
    print(average([]))
end
```

```text expect
5.0
0
```

> Hint: `if len(numbers) == 0: return 0` before dividing.

<details><summary>Solution</summary>

```vyrn solution
create average(numbers):
    if len(numbers) == 0: return 0
    return sum(numbers) / len(numbers)
end

main:
    print(average([4, 6]))
    print(average([]))
end
```

</details>

## 13.2 The usual suspects

| Error kind | Typical cause | Typical fix |
|---|---|---|
| `IndexError` | `xs[5]` on a shorter list | check `i < len(xs)` |
| `KeyError` | `m["x"]` when `"x"` isn't a key | `m.get("x", default)` or `"x" in m` |
| `DivisionByZero` | dividing by `0` | check first |
| `ValueError` | `int("abc")`: text that isn't a number | check, or catch it (lesson 13.4) |
| `TypeError` | the wrong kind of value, like `none + 1` | find where the value came from |
| `StackOverflow` | recursion without a base case | add the base case |

```vyrn
main:
    stock = {"apples": 3}
    print(stock.get("pears", 0))
    print(stock["pears"])
end
```

```text
0
error: KeyError: key "pears" is not in the map
```

### Exercise

The program crashes on the last word. Make it print `?` for words that are
not in the dictionary.

```vyrn starter
main:
    spanish = {"cat": "gato", "dog": "perro"}
    each word in ["cat", "dog", "bird"]:
        print(word, "->", spanish[word])
    end
end
```

```text expect
cat -> gato
dog -> perro
bird -> ?
```

> Hint: `spanish.get(word, "?")`

<details><summary>Solution</summary>

```vyrn solution
main:
    spanish = {"cat": "gato", "dog": "perro"}
    each word in ["cat", "dog", "bird"]:
        print(word, "->", spanish.get(word, "?"))
    end
end
```

</details>

## 13.3 Hunting wrong answers

The hardest bugs don't crash: the program runs and prints the wrong thing.
The method:

1. **Reproduce**: find a small input that gives a wrong answer.
2. **Inspect**: `print` the values inside the program to see where they
   stop being what you expect.
3. **Fix**, then check the original case *and* a few others.

```vyrn
create count_long_words(words):
    count = 0
    each w in words:
        print("  checking", w, len(w))
        if len(w) > 5: count += 1
    end
    return count
end

main:
    print(count_long_words(["planet", "sun", "comets"]))
end
```

```text
  checking planet 6
  checking sun 3
  checking comets 6
2
```

The extra `print` shows each step. Once the bug is found, remove it.

### Exercise

`sum_to(n)` should add the numbers from 1 to `n` (so `sum_to(4)` is 10),
but gives the wrong answer. Find and fix the bug.

```vyrn starter
create sum_to(n):
    total = 0
    each i in 1 until n:
        total += i
    end
    return total
end

main:
    print(sum_to(4))
    print(sum_to(100))
end
```

```text expect
10
5050
```

> Hint: Print `i` inside the loop: which number is missing?
> Hint: `until` stops *before* the end; `to` includes it.

<details><summary>Solution</summary>

```vyrn solution
create sum_to(n):
    total = 0
    each i in 1 to n:
        total += i
    end
    return total
end

main:
    print(sum_to(4))
    print(sum_to(100))
end
```

</details>

An answer that's wrong by exactly one step is so common it has a name: an
**off-by-one error**.

## 13.4 `attempt` and `catch`: recovering from failures

Some errors can't be prevented by checking first: the user types `forty`
when asked for a number, or a file is missing. Wrap the risky code in
`attempt:`, and say what to do instead in `catch`:

```vyrn
create to_number(text):
    attempt:
        return int(text)
    catch ValueError:
        return -1
    end
end

main:
    each t in ["42", "forty", "7"]:
        print(t, "->", to_number(t))
    end
    attempt:
        print(10 / 0)
    catch DivisionByZero as e:
        print("Caught:", e.message)
    end
end
```

```text
42 -> 42
forty -> -1
7 -> 7
Caught: division by zero
```

- `catch ValueError:` handles only that kind of error; others still stop
  the program.
- `catch ValueError as e:` also names the error, so you can read
  `e.message`.
- `catch Error:` handles every kind.
- Several `catch` lines can follow one `attempt`; the first that matches
  runs.

### Exercise

Keep asking for an age until the answer is a whole number. The checker
types `twelve`, `12.5`, then `12`.

```vyrn starter
main:
    loop:
        text = input("Age: ")
    end
end
```

```text input
twelve
12.5
12
```

```text expect
Age: That's not a whole number, try again.
Age: That's not a whole number, try again.
Age: Next year you'll be 13.
```

> Hint: Inside the loop: `attempt:` `age = int(text)` ... `stop` ... `catch ValueError:` print the message.
> Hint: Create `age = 0` before the loop so it exists afterwards.

<details><summary>Solution</summary>

```vyrn solution
main:
    age = 0
    loop:
        text = input("Age: ")
        attempt:
            age = int(text)
            stop
        catch ValueError:
            print("That's not a whole number, try again.")
        end
    end
    print("Next year you'll be {age + 1}.")
end
```

</details>

## 13.5 `always`: clean up no matter what

`always:` runs whether the `attempt` succeeded, failed and was caught, or
failed and wasn't. It's for clean-up that must happen: closing a file,
saving progress, printing a final report.

```vyrn
create risky(n):
    attempt:
        print("  working on", n)
        return 100 // n
    catch DivisionByZero:
        print("  can't divide by zero")
        return 0
    always:
        print("  finished", n)
    end
end

main:
    print(risky(4))
    print(risky(0))
end
```

```text
  working on 4
  finished 4
25
  working on 0
  can't divide by zero
  finished 0
0
```

### Exercise

Add up the items in each order. One order is broken: instead of stopping
the whole program, print `skipped` for it and carry on with the rest.

```vyrn starter
main:
    orders = ["3", "5", "oops", "2"]
    total = 0
    each o in orders:
        total += int(o)
    end
    print("Total items:", total)
end
```

```text expect
skipped oops
Total items: 10
```

> Hint: Wrap `total += int(o)` in `attempt:` ... `catch ValueError:` ... `end`.

<details><summary>Solution</summary>

```vyrn solution
main:
    orders = ["3", "5", "oops", "2"]
    total = 0
    each o in orders:
        attempt:
            total += int(o)
        catch ValueError:
            print("skipped", o)
        end
    end
    print("Total items:", total)
end
```

</details>

## 13.6 `fail`: raising your own errors

Your functions can report problems too. `fail` stops the function and
sends an error up to whoever called it, until some `catch` handles it:

```vyrn
create withdraw(balance, amount):
    if amount <= 0: fail ValueError("the amount must be positive")
    if amount > balance: fail ValueError("not enough money: balance is {balance}")
    return balance - amount
end

main:
    var balance = 100
    each amount in [30, -5, 500, 20]:
        attempt:
            balance = withdraw(balance, amount)
            print("Withdrew {amount}, balance {balance}")
        catch ValueError as e:
            print("Refused {amount}: {e.message}")
        end
    end
end
```

```text
Withdrew 30, balance 70
Refused -5: the amount must be positive
Refused 500: not enough money: balance is 70
Withdrew 20, balance 50
```

Built-in kinds to fail with: `Error`, `ValueError`, `TypeError`,
`KeyError` and `IndexError`. On Day 30 you'll make your own kinds, which
carry extra information.

Why fail instead of printing a message? Because the function doesn't know
what the caller wants to do about it: show a message, try again, use a
default. Failing lets the caller decide.

### Exercise

Write `parse_percent(text)` that turns `"45%"` into `45`. Fail with a
`ValueError` if the text doesn't end with `%` or the number is not between
0 and 100.

```vyrn starter
create parse_percent(text):
    return 0
end

main:
    each t in ["45%", "120%", "abc", "7%"]:
        attempt:
            print(t, "->", parse_percent(t))
        catch ValueError as e:
            print(t, "->", "error:", e.message)
        end
    end
end
```

```text expect
45% -> 45
120% -> error: must be between 0 and 100
abc -> error: must end with %
7% -> 7
```

> Hint: `if not text.endswith("%"): fail ValueError("must end with %")`
> Hint: `n = int(text[:-1])` drops the last character.

<details><summary>Solution</summary>

```vyrn solution
create parse_percent(text):
    if not text.endswith("%"): fail ValueError("must end with %")
    n = int(text[:-1])
    if n < 0 or n > 100: fail ValueError("must be between 0 and 100")
    return n
end

main:
    each t in ["45%", "120%", "abc", "7%"]:
        attempt:
            print(t, "->", parse_percent(t))
        catch ValueError as e:
            print(t, "->", "error:", e.message)
        end
    end
end
```

</details>

## 13.7 Project: the Bug Hunt

This shopping-cart program has three bugs. It runs, but the totals are
wrong. Use the method from lesson 13.3: reproduce, inspect, fix.

### Exercise

```vyrn starter
const PRICES = {"apple": 0.5, "bread": 2.25, "milk": 1.2}

create line_total(item, quantity):
    return PRICES[item] + quantity
end

create cart_total(cart):
    total = 0.0
    each item, quantity in cart.items():
        total = line_total(item, quantity)
    end
    return total
end

create with_discount(total):
    if total > 10: return total * 0.1
    return total
end

main:
    cart = {"apple": 6, "bread": 2, "milk": 3}
    total = cart_total(cart)
    print("Total: {total:.2f}")
    print("To pay: {with_discount(total):.2f}")
end
```

```text expect
Total: 11.10
To pay: 9.99
```

> Hint: A line's total is price **times** quantity.
> Hint: `total = ...` replaces the running total each time; it should add.
> Hint: A 10% discount means paying 90%: `total * 0.9`.

<details><summary>Solution</summary>

```vyrn solution
const PRICES = {"apple": 0.5, "bread": 2.25, "milk": 1.2}

create line_total(item, quantity):
    return PRICES[item] * quantity
end

create cart_total(cart):
    total = 0.0
    each item, quantity in cart.items():
        total += line_total(item, quantity)
    end
    return total
end

create with_discount(total):
    if total > 10: return total * 0.9
    return total
end

main:
    cart = {"apple": 6, "bread": 2, "milk": 3}
    total = cart_total(cart)
    print("Total: {total:.2f}")
    print("To pay: {with_discount(total):.2f}")
end
```

</details>

**Day 13 done.** Errors are no longer scary: they're messages telling you
where to look. Tomorrow: a game built around data.
