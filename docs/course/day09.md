# Day 9: Maps, Tuples, Sets and Nesting

Lists keep things in order. But often you want to look something up by
name: a word's meaning, a player's score, a country's capital. That's what
a **map** is for. Today you also meet **tuples** (fixed groups of values)
and **sets** (collections without repeats), and build a silent auction.

## 9.1 Maps: looking things up by key

A map links **keys** to **values**, written `{key: value, ...}`:

```vyrn
main:
    capitals = {"France": "Paris", "Japan": "Tokyo", "Kenya": "Nairobi"}
    print(capitals["Japan"])
    print(len(capitals))
    print("Peru" in capitals)
    print(capitals)
end
```

```text
Tokyo
3
false
{"France": "Paris", "Japan": "Tokyo", "Kenya": "Nairobi"}
```

`capitals["Japan"]` reads as "the value for the key Japan". Keys are
usually strings or numbers, and each key appears only once. (Python calls
maps *dictionaries*; Java and C++ call them *maps* or *hash maps*.)

### Exercise

Look up the number of legs for the animal the checker types (`spider`).

```vyrn starter
main:
    legs = {"cat": 4, "bird": 2, "spider": 8, "snake": 0}
    animal = input("Animal: ")
end
```

```text input
spider
```

```text expect
Animal: A spider has 8 legs.
```

> Hint: `legs[animal]`

<details><summary>Solution</summary>

```vyrn solution
main:
    legs = {"cat": 4, "bird": 2, "spider": 8, "snake": 0}
    animal = input("Animal: ")
    print("A {animal} has {legs[animal]} legs.")
end
```

</details>

## 9.2 Adding, changing and removing

```vyrn
main:
    stock = {"apples": 10}
    stock["pears"] = 4
    stock["apples"] = 7
    stock["apples"] += 5
    print(stock)
    stock.remove("pears")
    print(stock)
    print(stock.get("kiwis", 0))
end
```

```text
{"apples": 12, "pears": 4}
{"apples": 12}
0
```

- `m[key] = value` adds a new key, or replaces the value of an existing one.
- `m.remove(key)` takes a key out.
- `m[key]` for a missing key is an error (`KeyError`), but
  `m.get(key, default)` gives the default instead.

### Exercise

Count how many times each word appears in a sentence.

```vyrn starter
main:
    text = "red blue red green blue red"
    counts = {}
end
```

```text expect
{"red": 3, "blue": 2, "green": 1}
```

> Hint: Loop over `text.split()`.
> Hint: `counts[word] = counts.get(word, 0) + 1`

<details><summary>Solution</summary>

```vyrn solution
main:
    text = "red blue red green blue red"
    counts = {}
    each word in text.split():
        counts[word] = counts.get(word, 0) + 1
    end
    print(counts)
end
```

</details>

## 9.3 Looping through a map

```vyrn
main:
    scores = {"Ada": 92, "Alan": 78, "Grace": 85}
    each name in scores:
        print(name)
    end
    each name, score in scores.items():
        print("{name}: {score}")
    end
    print(scores.keys(), scores.values())
    print(sum(scores.values()) / len(scores))
end
```

```text
Ada
Alan
Grace
Ada: 92
Alan: 78
Grace: 85
["Ada", "Alan", "Grace"] [92, 78, 85]
85.0
```

Looping over a map gives its keys, in the order they were added.
`.items()` gives each key with its value, and `each name, score in ...`
unpacks the pair into two variables.

### Exercise

Turn scores into grades: 90 and over `A`, 80 and over `B`, otherwise `C`.
Build a new map of grades and print it.

```vyrn starter
main:
    scores = {"Ada": 92, "Alan": 78, "Grace": 85, "Linus": 90}
    grades = {}
end
```

```text expect
{"Ada": "A", "Alan": "C", "Grace": "B", "Linus": "A"}
```

> Hint: `each name, score in scores.items():` and set `grades[name]`.

<details><summary>Solution</summary>

```vyrn solution
main:
    scores = {"Ada": 92, "Alan": 78, "Grace": 85, "Linus": 90}
    grades = {}
    each name, score in scores.items():
        if score >= 90:
            grades[name] = "A"
        else if score >= 80:
            grades[name] = "B"
        else:
            grades[name] = "C"
        end
    end
    print(grades)
end
```

</details>

## 9.4 Tuples: values that belong together

A **tuple** groups a fixed number of values in parentheses. Unlike a list,
a tuple can't be changed after it's made, which makes it good for things
like a point `(x, y)` or a date `(2026, 11, 11)`.

```vyrn
main:
    point = (3, 4)
    print(point[0], point[1])
    x, y = point
    print("x is {x}, y is {y}")
    a = 1
    b = 2
    a, b = b, a
    print(a, b)
end
```

```text
3 4
x is 3, y is 4
2 1
```

`x, y = point` **unpacks** the tuple into two variables, and `a, b = b, a`
swaps two variables in one line (compare that with lesson 1.6).

### Exercise

Write `min_max(numbers)` that returns the smallest and largest numbers as
a tuple, and unpack the answer.

```vyrn starter
main:
    low, high = min_max([7, 2, 9, 4])
    print("low {low}, high {high}")
end
```

```text expect
low 2, high 9
```

> Hint: `return (min(numbers), max(numbers))`

<details><summary>Solution</summary>

```vyrn solution
create min_max(numbers):
    return (min(numbers), max(numbers))
end

main:
    low, high = min_max([7, 2, 9, 4])
    print("low {low}, high {high}")
end
```

</details>

## 9.5 Sets: no repeats

A **set** holds each value at most once, written `{a, b, c}`. It's the
right tool for "which different things are there?" and "what do these
have in common?":

```vyrn
main:
    visited = {"Paris", "Rome", "Paris", "Oslo"}
    print(visited, len(visited))
    visited.add("Lima")
    print("Rome" in visited)
    mine = {"pizza", "sushi", "tacos"}
    yours = {"sushi", "curry", "tacos"}
    print(mine & yours)
    print(mine | yours)
    print(mine - yours)
    print(set([3, 1, 3, 2, 1]))
end
```

```text
{"Paris", "Rome", "Oslo"} 3
true
{"sushi", "tacos"}
{"pizza", "sushi", "tacos", "curry"}
{"pizza"}
{3, 1, 2}
```

| Operation | Gives |
|---|---|
| `a & b` | what's in both |
| `a \| b` | what's in either |
| `a - b` | what's in `a` but not `b` |
| `set(list)` | the list's values without repeats |

An empty set is written `set()`, because `{}` is an empty map.

### Exercise

Print how many *different* words this sentence has, ignoring capitals.

```vyrn starter
main:
    text = "The quick fox and the lazy dog and THE cat"
end
```

```text expect
7 different words
```

> Hint: `set(text.lower().split())`

<details><summary>Solution</summary>

```vyrn solution
main:
    text = "The quick fox and the lazy dog and THE cat"
    words = set(text.lower().split())
    print("{len(words)} different words")
end
```

</details>

## 9.6 Nesting: collections inside collections

Lists and maps can hold each other, as deep as you need. That's how real
data is shaped:

```vyrn
main:
    trips = {
        "France": {"cities": ["Paris", "Lyon"], "visits": 3},
        "Japan": {"cities": ["Tokyo"], "visits": 1},
    }
    print(trips["France"]["cities"][1])
    trips["Japan"]["cities"].append("Kyoto")
    trips["Peru"] = {"cities": ["Lima"], "visits": 1}
    each country, info in trips.items():
        print("{country}: {", ".join(info["cities"])} ({info["visits"]} visits)")
    end
end
```

```text
Lyon
France: Paris, Lyon (3 visits)
Japan: Tokyo, Kyoto (1 visits)
Peru: Lima (1 visits)
```

Read `trips["France"]["cities"][1]` left to right: the France entry, then
its cities, then the second city.

### Exercise

A list of maps is the classic shape for records. Print each student whose
average is at least 80.

```vyrn starter
main:
    students = [
        {"name": "Ada", "marks": [90, 85, 88]},
        {"name": "Ben", "marks": [60, 75, 70]},
        {"name": "Cai", "marks": [80, 82, 79]},
    ]
end
```

```text expect
Ada 87.7
Cai 80.3
```

> Hint: `average = sum(s["marks"]) / len(s["marks"])`
> Hint: Print with `{average:.1f}`.

<details><summary>Solution</summary>

```vyrn solution
main:
    students = [
        {"name": "Ada", "marks": [90, 85, 88]},
        {"name": "Ben", "marks": [60, 75, 70]},
        {"name": "Cai", "marks": [80, 82, 79]},
    ]
    each s in students:
        average = sum(s["marks"]) / len(s["marks"])
        if average >= 80: print("{s["name"]} {average:.1f}")
    end
end
```

</details>

## 9.7 Project: the Silent Auction

Bidders type their name and bid one after another; nobody sees the others'
bids. At the end, the highest bidder wins.

Plan: keep a map from name to bid; keep asking until there are no more
bidders; then find the key with the largest value.

### Exercise

The checker enters three bidders.

```vyrn starter
main:
    bids = {}
    print("Welcome to the silent auction.")
end
```

```text input
Ana
120
yes
Bo
95
yes
Cy
140
no
```

```text expect
Welcome to the silent auction.
Name: Bid: Any more bidders? Name: Bid: Any more bidders? Name: Bid: Any more bidders? Cy wins with a bid of 140.
```

> Hint: `loop:` ... ask name and bid, store `bids[name] = bid`, and `stop` when the answer is not `yes`.
> Hint: Find the winner by looping over `bids.items()`, keeping the best so far.

<details><summary>Solution</summary>

```vyrn solution
main:
    bids = {}
    print("Welcome to the silent auction.")
    loop:
        name = input("Name: ")
        bid = int(input("Bid: "))
        bids[name] = bid
        if input("Any more bidders? ") != "yes": stop
    end
    winner = ""
    best = 0
    each name, bid in bids.items():
        if bid > best:
            best = bid
            winner = name
        end
    end
    print("{winner} wins with a bid of {best}.")
end
```

</details>

**Day 9 done.** You have all of Vyrn's basic collections: lists, maps,
tuples and sets, and you can nest them. Tomorrow: functions that give back
answers, and functions as values.
