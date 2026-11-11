# Day 14: Project: Bigger or Smaller

Today's game is driven by **data**: a list of records, each one a map with
the same keys. The player sees two mountains and guesses which is taller;
a correct guess scores a point and brings in a new challenger. Keeping the
facts in data, and the rules in code, means you can swap in rivers, cities
or planets without changing the program.

## 14.1 Records: a list of maps

```vyrn
const MOUNTAINS = [
    {"name": "Everest", "height": 8849, "country": "Nepal"},
    {"name": "Kilimanjaro", "height": 5895, "country": "Tanzania"},
    {"name": "Denali", "height": 6190, "country": "USA"},
]

main:
    each m in MOUNTAINS:
        print("{m["name"]:<12} {m["height"]:>5} m  ({m["country"]})")
    end
end
```

```text
Everest       8849 m  (Nepal)
Kilimanjaro   5895 m  (Tanzania)
Denali        6190 m  (USA)
```

Each map is one **record**, and each key is a **field**. Day 17 shows
`struct`, Vyrn's way to declare a record's fields once, with their types.

### Exercise

Print the name of the tallest mountain in the list.

```vyrn starter
const MOUNTAINS = [
    {"name": "Everest", "height": 8849, "country": "Nepal"},
    {"name": "Kilimanjaro", "height": 5895, "country": "Tanzania"},
    {"name": "Denali", "height": 6190, "country": "USA"},
]

main:
end
```

```text expect
Everest
```

> Hint: Keep the best record so far in a loop.
> Hint: Or: `max(MOUNTAINS, key = m => m["height"])["name"]`.

<details><summary>Solution</summary>

```vyrn solution
const MOUNTAINS = [
    {"name": "Everest", "height": 8849, "country": "Nepal"},
    {"name": "Kilimanjaro", "height": 5895, "country": "Tanzania"},
    {"name": "Denali", "height": 6190, "country": "USA"},
]

main:
    tallest = max(MOUNTAINS, key = m => m["height"])
    print(tallest["name"])
end
```

</details>

## 14.2 Describing a record

A small function that turns a record into text keeps the game code tidy:

### Exercise

Write `describe(m)` returning `<name>, in <country>`.

```vyrn starter
create describe(m):
    return ""
end

main:
    print(describe({"name": "Denali", "height": 6190, "country": "USA"}))
end
```

```text expect
Denali, in USA
```

> Hint: `return "{m["name"]}, in {m["country"]}"`

<details><summary>Solution</summary>

```vyrn solution
create describe(m):
    return "{m["name"]}, in {m["country"]}"
end

main:
    print(describe({"name": "Denali", "height": 6190, "country": "USA"}))
end
```

</details>

## 14.3 One round

### Exercise

Write `check(a, b, answer)`: `answer` is `"a"` or `"b"`; return `true` if
the player picked the taller one.

```vyrn starter
create check(a, b, answer):
    return false
end

main:
    everest = {"name": "Everest", "height": 8849, "country": "Nepal"}
    denali = {"name": "Denali", "height": 6190, "country": "USA"}
    print(check(everest, denali, "a"))
    print(check(everest, denali, "b"))
    print(check(denali, everest, "b"))
end
```

```text expect
true
false
true
```

> Hint: Work out the right answer: `taller = "a" if a["height"] > b["height"] else "b"`.

<details><summary>Solution</summary>

```vyrn solution
create check(a, b, answer):
    taller = "a" if a["height"] > b["height"] else "b"
    return answer == taller
end

main:
    everest = {"name": "Everest", "height": 8849, "country": "Nepal"}
    denali = {"name": "Denali", "height": 6190, "country": "USA"}
    print(check(everest, denali, "a"))
    print(check(everest, denali, "b"))
    print(check(denali, everest, "b"))
end
```

</details>

## 14.4 The game

The winner of each round stays as `a`; a new random mountain becomes `b`.
The game ends at the first wrong answer, or when the mountains run out.

### Exercise

The checker answers `a`, `a`, `b`, `a`, then makes a mistake with `b`.

```vyrn starter
use vyrn.random

const MOUNTAINS = [
    {"name": "Everest", "height": 8849, "country": "Nepal"},
    {"name": "K2", "height": 8611, "country": "Pakistan"},
    {"name": "Kilimanjaro", "height": 5895, "country": "Tanzania"},
    {"name": "Mont Blanc", "height": 4806, "country": "France"},
    {"name": "Denali", "height": 6190, "country": "USA"},
    {"name": "Aconcagua", "height": 6961, "country": "Argentina"},
    {"name": "Fuji", "height": 3776, "country": "Japan"},
]

create describe(m): return "{m["name"]}, in {m["country"]}"

create check(a, b, answer):
    taller = "a" if a["height"] > b["height"] else "b"
    return answer == taller
end

main:
    random.seed(14)
    pool = list(MOUNTAINS)
    random.shuffle(pool)
    a = pool.pop()
    score = 0
end
```

```text input
a
a
b
a
b
```

```text expect
A: Denali, in USA
B: Fuji, in Japan
Which is taller, a or b? Right! Score: 1
A: Denali, in USA
B: Mont Blanc, in France
Which is taller, a or b? Right! Score: 2
A: Denali, in USA
B: Everest, in Nepal
Which is taller, a or b? Right! Score: 3
A: Everest, in Nepal
B: K2, in Pakistan
Which is taller, a or b? Right! Score: 4
A: Everest, in Nepal
B: Kilimanjaro, in Tanzania
Which is taller, a or b? Wrong! Everest is 8849 m, Kilimanjaro is 5895 m.
Final score: 4
```

> Hint: `loop len(pool) > 0:` take `b = pool.pop()`, show both, ask.
> Hint: On a right answer, `score += 1` and, if `b` was taller, `a = b`. On a wrong one, show the heights and `stop`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

const MOUNTAINS = [
    {"name": "Everest", "height": 8849, "country": "Nepal"},
    {"name": "K2", "height": 8611, "country": "Pakistan"},
    {"name": "Kilimanjaro", "height": 5895, "country": "Tanzania"},
    {"name": "Mont Blanc", "height": 4806, "country": "France"},
    {"name": "Denali", "height": 6190, "country": "USA"},
    {"name": "Aconcagua", "height": 6961, "country": "Argentina"},
    {"name": "Fuji", "height": 3776, "country": "Japan"},
]

create describe(m): return "{m["name"]}, in {m["country"]}"

create check(a, b, answer):
    taller = "a" if a["height"] > b["height"] else "b"
    return answer == taller
end

main:
    random.seed(14)
    pool = list(MOUNTAINS)
    random.shuffle(pool)
    a = pool.pop()
    score = 0
    loop len(pool) > 0:
        b = pool.pop()
        print("A: {describe(a)}")
        print("B: {describe(b)}")
        answer = input("Which is taller, a or b? ")
        if not check(a, b, answer):
            print("Wrong! {a["name"]} is {a["height"]} m, {b["name"]} is {b["height"]} m.")
            stop
        end
        score += 1
        print("Right! Score: {score}")
        if b["height"] > a["height"]: a = b
    end
    print("Final score: {score}")
end
```

</details>

**Try it:** make a second data list (rivers and their lengths, planets and
their moons) and let the player choose which one to play.

**Day 14 done.** Data in, rules in code: a pattern you'll use in every
kind of program.
