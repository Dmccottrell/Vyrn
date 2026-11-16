# Day 26: Comprehensions, map, filter, reduce and pipelines

Much of programming is turning one list into another: prices into prices
with tax, words into their lengths, a list of people into just the adults.
Today you learn the short ways to write that: **comprehensions**, the
functions `map`, `filter` and `reduce`, and **pipelines** that read like a
recipe. The project spells names in the NATO phonetic alphabet
("Alfa Bravo Charlie").

## 26.1 List comprehensions

Here is the long way to double every number:

```vyrn
main:
    numbers = [3, 1, 4, 1, 5]
    doubled = []
    each n in numbers:
        doubled.append(n * 2)
    end
    print(doubled)
end
```

```text
[6, 2, 8, 2, 10]
```

A **comprehension** says the same in one line: "a list of `n * 2`, for each
`n` in `numbers`":

```vyrn
main:
    numbers = [3, 1, 4, 1, 5]
    print([n * 2 each n in numbers])
    print([len(w) each w in ["sun", "moon", "stars"]])
    print([n * n each n in 1 to 5])
end
```

```text
[6, 2, 8, 2, 10]
[3, 4, 5]
[1, 4, 9, 16, 25]
```

Read it from the middle out: `each n in numbers` is the loop, and what is
in front of `each` is what goes in the new list.

Add `if` at the end to keep only some items:

```vyrn
main:
    numbers = [3, 1, 4, 1, 5, 9, 2, 6]
    print([n each n in numbers if n > 3])
    print([n * 10 each n in numbers if n % 2 == 0])
    words = ["apple", "kiwi", "banana", "fig"]
    print([w.upper() each w in words if len(w) <= 4])
end
```

```text
[4, 5, 9, 6]
[40, 20, 60]
["KIWI", "FIG"]
```

Two `each` parts make every pair, like a loop inside a loop:

```vyrn
main:
    print([[x, y] each x in 1 to 2 each y in ["a", "b"]])
end
```

```text
[[1, "a"], [1, "b"], [2, "a"], [2, "b"]]
```

### Exercise

From the list of temperatures in Celsius, make a list of the ones above
20, converted to Fahrenheit (`c * 9 / 5 + 32`).

```vyrn starter
main:
    celsius = [12, 25, 30, 18, 21]
end
```

```text expect
[77.0, 86.0, 69.8]
```

> Hint: `[... each c in celsius if c > 20]`
> Hint: what goes in front is `c * 9 / 5 + 32`.

<details><summary>Solution</summary>

```vyrn solution
main:
    celsius = [12, 25, 30, 18, 21]
    print([c * 9 / 5 + 32 each c in celsius if c > 20])
end
```

</details>

## 26.2 Map and set comprehensions

Curly brackets with `key: value` make a **map**; curly brackets with one
value make a **set** (no repeats):

```vyrn
main:
    words = ["sun", "moon", "stars"]
    lengths = {w: len(w) each w in words}
    print(lengths)
    print({n % 3 each n in 1 to 10})
    prices = {"tea": 2.0, "cake": 3.5, "soup": 4.0}
    print({item: p * 1.2 each item, p in prices.items() if p > 2})
end
```

```text
{"sun": 3, "moon": 4, "stars": 5}
{1, 2, 0}
{"cake": 4.2, "soup": 4.8}
```

`prices.items()` gives `(key, value)` pairs, and `each item, p in` takes
each pair apart.

### Exercise

Make a map from each word to its first letter, in capitals.

```vyrn starter
main:
    words = ["vyrn", "code", "loop"]
end
```

```text expect
{"vyrn": "V", "code": "C", "loop": "L"}
```

> Hint: `{w: ... each w in words}`; the first letter is `w[0]`.

<details><summary>Solution</summary>

```vyrn solution
main:
    words = ["vyrn", "code", "loop"]
    print({w: w[0].upper() each w in words})
end
```

</details>

## 26.3 `map`, `filter` and `reduce`

Three functions do the same jobs, each taking a **function** to apply. A
short function written with `=>` (a **lambda**, from Day 10) is usual:

| Function | Gives | Like |
|---|---|---|
| `map(f, xs)` | `f(x)` for every `x` | `[f(x) each x in xs]` |
| `filter(f, xs)` | the `x` where `f(x)` is true | `[x each x in xs if f(x)]` |
| `reduce(f, xs)` | one value, combining the items two at a time | a loop with a running total |

```vyrn
main:
    numbers = [3, 1, 4, 1, 5]
    print(map(n => n * 2, numbers))
    print(filter(n => n > 2, numbers))
    print(reduce((a, b) => a + b, numbers))
    print(reduce((a, b) => a if a > b else b, numbers))
    print(reduce((total, n) => total + n * n, numbers, 0))
end
```

```text
[6, 2, 8, 2, 10]
[3, 4, 5]
14
5
52
```

`reduce` walks along the list carrying a value: `((((3 + 1) + 4) + 1) + 5)`.
With a third argument it starts from that value instead of the first item;
that is how the sum of squares works.

Lists also have these as **methods**, which chain nicely:

```vyrn
main:
    words = ["pear", "fig", "banana", "kiwi"]
    print(words.filter(w => len(w) == 4).map(w => w.upper()))
    print(sorted(words, key = w => len(w)))
end
```

```text
["PEAR", "KIWI"]
["fig", "pear", "kiwi", "banana"]
```

Which to use? Comprehensions are usually clearest. `map` and `filter` are
handy when you already have a function: `map(str, numbers)` turns every
number into text.

### Exercise

Use `reduce` to multiply all the numbers together.

```vyrn starter
main:
    numbers = [2, 3, 4]
end
```

```text expect
24
```

> Hint: `reduce((a, b) => a * b, numbers)`

<details><summary>Solution</summary>

```vyrn solution
main:
    numbers = [2, 3, 4]
    print(reduce((a, b) => a * b, numbers))
end
```

</details>

## 26.4 Pipelines with `|>`

When a value goes through several steps, nested calls read inside out:
`shout(trim(read_name()))` happens in the order read, trim, shout, but you
read it backwards. A **pipeline** writes the steps in the order they
happen. `x |> f` means `f(x)`:

```vyrn
create trim(s: str) -> str: return s.strip()
create shout(s: str) -> str: return s.upper() + "!"
create words(s: str) -> list: return s.split()

main:
    print("  hello there  " |> trim |> shout)
    print("  one two three " |> trim |> words |> len)
    print([3, 1, 2] |> sorted |> sum)
end
```

```text
HELLO THERE!
3
6
```

Read `|>` as "and then". With more arguments, the value goes in first:
`x |> f(y)` means `f(x, y)`:

```vyrn
create add_tax(price: float, rate: float) -> float: return price * (1 + rate)
create rounded(x: float, places: int) -> float: return round(x, places)

main:
    print(19.99 |> add_tax(0.2) |> rounded(2))
end
```

```text
23.99
```

### Exercise

Write `clean` (strip spaces and make lower case) and `count_vowels`, then
use a pipeline to count the vowels in `"  Vyrn Is Fun  "`.

```vyrn starter
create clean(s: str) -> str:
    return s
end

create count_vowels(s: str) -> int:
    return 0
end

main:
    print("  Vyrn Is Fun  " |> clean |> count_vowels)
end
```

```text expect
2
```

> Hint: `clean` is `s.strip().lower()`.
> Hint: `count_vowels` is `len([c each c in s if c in "aeiou"])`.

<details><summary>Solution</summary>

```vyrn solution
create clean(s: str) -> str:
    return s.strip().lower()
end

create count_vowels(s: str) -> int:
    return len([c each c in s if c in "aeiou"])
end

main:
    print("  Vyrn Is Fun  " |> clean |> count_vowels)
end
```

</details>

## 26.5 Project: the phonetic alphabet

Pilots, police and radio operators spell words with code words so letters
can't be misheard: "B" is "Bravo", "D" is "Delta". The alphabet comes as a
CSV file; a map comprehension turns it into a lookup table, and a list
comprehension spells any word.

```vyrn
use vyrn.csv

create load_alphabet(path: str) -> map:
    return {row["letter"]: row["code"] each row in csv.read(path)}
end

## The code words for a word's letters, skipping anything else.
create spell(word: str, codes: map) -> list:
    return [codes[c] each c in word.upper() if c in codes]
end

main:
    letters = "ABCDEFGHIJKLMNOPQRSTUVWXYZ"
    words = "Alfa Bravo Charlie Delta Echo Foxtrot Golf Hotel India Juliett Kilo Lima Mike November Oscar Papa Quebec Romeo Sierra Tango Uniform Victor Whiskey X-ray Yankee Zulu".split()
    csv.write("nato.csv", [{"letter": letters[i], "code": words[i]} each i in 0 until 26])
    codes = load_alphabet("nato.csv")
    print(" ".join(spell("Vyrn", codes)))
    print(spell("ok, go!", codes))
end
```

```text
Victor Yankee Romeo November
["Oscar", "Kilo", "Golf", "Oscar"]
```

The whole program is two comprehensions: one builds the map, one uses it.

### Exercise

Spell the name, then turn it back: take the first letter of each code word
and join them into a word again.

```vyrn starter
create unspell(codes: list) -> str:
    return ""
end

main:
    print(unspell(["Kilo", "Alfa", "India"]))
end
```

```text expect
KAI
```

> Hint: `[w[0] each w in codes]` gives the first letters.
> Hint: `"".join(...)` puts them together.

<details><summary>Solution</summary>

```vyrn solution
create unspell(codes: list) -> str:
    return "".join([w[0] each w in codes])
end

main:
    print(unspell(["Kilo", "Alfa", "India"]))
end
```

</details>

## Today you learned

- `[f(x) each x in xs if test]` builds a list in one line; `{k: v each ...}`
  builds a map and `{x each ...}` a set.
- `map`, `filter` and `reduce` take a function; lists have `.map` and
  `.filter` too, which chain.
- `reduce` combines items two at a time, optionally from a starting value.
- `x |> f` means `f(x)`, and `x |> f(y)` means `f(x, y)`: steps in the order
  they happen.
