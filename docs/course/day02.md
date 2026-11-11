# Day 2: Kinds of Data and Numbers

Programs work with different kinds of data: text, whole numbers, numbers
with a decimal point, yes-or-no answers. Today you meet each kind, learn to
calculate, and build a tip calculator.

## 2.1 The kinds of data

Every value in Vyrn has a **type**: the kind of thing it is.

| Type | What it holds | Examples |
|---|---|---|
| `str` | text (a *string* of characters) | `"hello"`, `"42"`, `""` |
| `int` | a whole number (an *integer*) | `7`, `-3`, `1_000_000` |
| `float` | a number with a decimal point | `3.14`, `-0.5`, `2.0` |
| `bool` | yes or no: `true` or `false` | `true` |
| `none` | "nothing here" | `none` |

`typeof` tells you a value's type:

```vyrn
main:
    print(typeof("hello"), typeof(42), typeof(3.5), typeof(true), typeof(none))
    print(typeof("42"))
end
```

```text
str int float bool none
str
```

`"42"` in quotes is text that happens to contain digits; `42` without
quotes is a number. That difference matters: you can do maths with `42`,
not with `"42"`.

Underscores make big numbers readable: `1_000_000` is one million.

### Exercise

Print the type of each of these four values, one per line: `"3"`, `3`,
`3.0`, `false`.

```vyrn starter
main:
    print(typeof("3"))
end
```

```text expect
str
int
float
bool
```

> Hint: One `print(typeof(...))` per value.

<details><summary>Solution</summary>

```vyrn solution
main:
    print(typeof("3"))
    print(typeof(3))
    print(typeof(3.0))
    print(typeof(false))
end
```

</details>

## 2.2 Arithmetic

| Operator | Meaning | Example | Result |
|---|---|---|---|
| `+` | add | `7 + 2` | `9` |
| `-` | subtract | `7 - 2` | `5` |
| `*` | multiply | `7 * 2` | `14` |
| `/` | divide | `7 / 2` | `3.5` |
| `//` | divide, rounding down to a whole number | `7 // 2` | `3` |
| `%` | remainder after dividing | `7 % 2` | `1` |
| `**` | power | `7 ** 2` | `49` |

```vyrn
main:
    print(7 + 2, 7 - 2, 7 * 2, 7 / 2, 7 // 2, 7 % 2, 7 ** 2)
    print(10 / 2)
end
```

```text
9 5 14 3.5 3 1 49
5.0
```

`/` always gives a `float`, even when it divides exactly: `10 / 2` is
`5.0`.

**Order of operations.** As in maths, `**` comes first, then `*`, `/`, `//`
and `%`, then `+` and `-`. Parentheses go first of all:

```vyrn
main:
    print(2 + 3 * 4)
    print((2 + 3) * 4)
    print(2 ** 3 ** 2)
end
```

```text
14
20
512
```

`%` is more useful than it looks: `n % 2` is `0` for even numbers and `1`
for odd ones, and `minutes % 60` is what's left over after whole hours.

### Exercise

There are 200 minutes of video. Print how many whole hours that is and how
many minutes are left over, as `3 hours and 20 minutes`.

```vyrn starter
main:
    minutes = 200
end
```

```text expect
3 hours and 20 minutes
```

> Hint: `minutes // 60` gives the whole hours.
> Hint: `minutes % 60` gives the minutes left over.

<details><summary>Solution</summary>

```vyrn solution
main:
    minutes = 200
    hours = minutes // 60
    left = minutes % 60
    print("{hours} hours and {left} minutes")
end
```

</details>

## 2.3 Converting between types

`input` always gives text, even if someone types a number. To calculate
with it, convert it:

| Function | Converts to | Example |
|---|---|---|
| `int(x)` | whole number | `int("42")` is `42`; `int(3.9)` is `3` |
| `float(x)` | decimal number | `float("2.5")` is `2.5` |
| `str(x)` | text | `str(42)` is `"42"` |
| `round(x, n)` | `x` rounded to `n` decimal places | `round(2.567, 1)` is `2.6` |

```vyrn
main:
    age_text = "41"
    age = int(age_text)
    print(age + 1)
    print("41" + "1")
    print(str(41) + "!")
    print(round(3.14159, 2), round(7.5))
end
```

```text
42
411
41!
3.14 8
```

`"41" + "1"` joins two strings into `"411"`; `41 + 1` adds numbers. Mixing
them is a mistake Vyrn catches:

```vyrn check
main:
    print("Age: " + str(41))
end
```

Written as `print("Age: " + 41)`, Vyrn refuses to run it at all:

```text
error: cannot apply `+` to str and int
```

Fix it with `"Age: " + str(41)`, or more simply with `"Age: {41}"`.

### Exercise

The checker types `1990` as a birth year. Print the age that person turns
in 2026, as `You turn 36 in 2026`.

```vyrn starter
main:
    year = input("Birth year: ")
end
```

```text input
1990
```

```text expect
Birth year: You turn 36 in 2026
```

> Hint: `input` gives text. Convert it with `int(year)` before subtracting.

<details><summary>Solution</summary>

```vyrn solution
main:
    year = int(input("Birth year: "))
    age = 2026 - year
    print("You turn {age} in 2026")
end
```

</details>

## 2.4 Showing numbers nicely

Inside `{braces}` you can add a colon and a **format**:

| Format | Shows | Example | Result |
|---|---|---|---|
| `.2f` | two decimal places | `"{3.14159:.2f}"` | `3.14` |
| `,` | thousands separators | `"{1234567:,}"` | `1,234,567` |
| `.1%` | as a percentage | `"{0.256:.1%}"` | `25.6%` |
| `>6` | right-aligned in 6 spaces | `"{42:>6}"` | `    42` |
| `<6`, `^6` | left-aligned, centred | `"{42:<6}"` | `42    ` |
| `03d` | padded with zeros | `"{7:03d}"` | `007` |

```vyrn
main:
    price = 4.5
    print("Price: {price:.2f}")
    print("Population: {8100000000:,}")
    print("Battery: {0.873:.0%}")
    print("[{7:03d}] [{"hi":>5}] [{"hi":<5}]")
end
```

```text
Price: 4.50
Population: 8,100,000,000
Battery: 87%
[007] [   hi] [hi   ]
```

### Exercise

Print a receipt line for 3 notebooks at 2.5 each: the item name
left-aligned in 10 spaces, then the total with 2 decimals right-aligned in
8 spaces.

```vyrn starter
main:
    item = "notebook"
    count = 3
    price = 2.5
end
```

```text expect
notebook      7.50
```

> Hint: The total is `count * price`.
> Hint: `"{item:<10}{total:>8.2f}"`

<details><summary>Solution</summary>

```vyrn solution
main:
    item = "notebook"
    count = 3
    price = 2.5
    total = count * price
    print("{item:<10}{total:>8.2f}")
end
```

</details>

## 2.5 Changing a variable with `+=`

To add to a variable, you could write `score = score + 10`. The short form
is `score += 10`. The same works for `-=`, `*=`, `/=`, `//=` and `%=`.

```vyrn
main:
    score = 0
    score += 10
    score += 5
    score -= 3
    score *= 2
    print(score)
end
```

```text
24
```

### Exercise

A savings account starts at 1000 and grows by 5% each year. Using `*=`,
print the balance after each of three years, with two decimals.

```vyrn starter
main:
    balance = 1000.0
end
```

```text expect
1050.00
1102.50
1157.62
```

> Hint: Growing by 5% means multiplying by `1.05`.
> Hint: `balance *= 1.05` then `print("{balance:.2f}")`, three times.

<details><summary>Solution</summary>

```vyrn solution
main:
    balance = 1000.0
    balance *= 1.05
    print("{balance:.2f}")
    balance *= 1.05
    print("{balance:.2f}")
    balance *= 1.05
    print("{balance:.2f}")
end
```

</details>

## 2.6 True and false

A `bool` is `true` or `false`. Comparing two values gives one:

```vyrn
main:
    print(5 > 3, 5 < 3, 5 == 5, 5 != 5)
    is_adult = 20 >= 18
    print("adult:", is_adult)
    print(bool(0), bool(7), bool(""), bool("hi"))
end
```

```text
true false true false
adult: true
false true false true
```

`==` asks "are these equal?" (one `=` would *store* a value instead). `!=`
means "not equal". Tomorrow you use these to make decisions.

`bool(x)` is `false` for "empty" values (`0`, `0.0`, `""`, `none`, an
empty list) and `true` for everything else.

### Exercise

Print whether 2026 is divisible by 4, then whether 2028 is, each as
`2026 divisible by 4: false`.

```vyrn starter
main:
end
```

```text expect
2026 divisible by 4: false
2028 divisible by 4: true
```

> Hint: A number is divisible by 4 when `number % 4 == 0`.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("2026 divisible by 4: {2026 % 4 == 0}")
    print("2028 divisible by 4: {2028 % 4 == 0}")
end
```

</details>

## 2.7 Project: the Tip Calculator

Ask for the bill, the tip percentage and the number of people, then say
what each person pays.

Plan: **ask** three things (converting them to numbers), **work out** the
total with tip and each share, **show** the share with two decimals.

### Exercise

The checker types a bill of `150`, a tip of `12` percent, and `5` people.

```vyrn starter
main:
    print("Welcome to the tip calculator!")
    bill = float(input("Total bill: "))
end
```

```text input
150
12
5
```

```text expect
Welcome to the tip calculator!
Total bill: Tip percentage: People: Each person pays 33.60
```

> Hint: The tip percentage and the people are whole numbers: use `int(...)`.
> Hint: The total is `bill * (1 + tip / 100)`.
> Hint: Each share is `total / people`, shown with `:.2f`.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Welcome to the tip calculator!")
    bill = float(input("Total bill: "))
    tip = int(input("Tip percentage: "))
    people = int(input("People: "))
    total = bill * (1 + tip / 100)
    share = total / people
    print("Each person pays {share:.2f}")
end
```

</details>

**Day 2 done.** You know Vyrn's kinds of data, can calculate, convert and
format numbers. Tomorrow: making decisions.
