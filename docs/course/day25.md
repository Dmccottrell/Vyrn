# Day 25: CSV files and tables

A lot of the world's data lives in **CSV** files: spreadsheets saved as
plain text, one row per line, with commas between the columns. Today you
read and write CSV files, then meet **tables**, which let you sort, filter
and total columns of data in one line each. The project is a game that
asks you to name the states of a country, checking your answers against a
CSV file.

## 25.1 What a CSV file is

Open any spreadsheet and save it as CSV, and you get text like this:

```text
name,age,city
Ada,36,London
Grace,85,New York
```

The first line is the **header**: the names of the columns. Every other
line is one **row**, its values in the same order as the header, separated
by commas. That is the whole format.

`csv.read` reads such a file into a list of **maps**, one per row, with the
header names as keys:

```vyrn
use vyrn.csv

main:
    write_text("people.csv", "name,age,city\nAda,36,London\nGrace,85,New York\n")
    people = csv.read("people.csv")
    print(len(people), "rows")
    print(people[0])
    each person in people:
        print(person["name"], "lives in", person["city"])
    end
end
```

```text
2 rows
{"name": "Ada", "age": "36", "city": "London"}
Ada lives in London
Grace lives in New York
```

Notice that the age is `"36"`, in quotes: everything in a CSV file is
text. Convert it when you need a number: `int(person["age"])`.

### Exercise

Read the scores file and print each player's score doubled.

```vyrn starter
use vyrn.csv

main:
    write_text("scores.csv", "player,score\nmo,12\nkai,30\n")
end
```

```text expect
mo 24
kai 60
```

> Hint: `each row in csv.read("scores.csv"):`
> Hint: `int(row["score"]) * 2` turns the text into a number first.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.csv

main:
    write_text("scores.csv", "player,score\nmo,12\nkai,30\n")
    each row in csv.read("scores.csv"):
        print(row["player"], int(row["score"]) * 2)
    end
end
```

</details>

## 25.2 Writing a CSV file

`csv.write(path, rows)` does the reverse: give it a list of maps (all with
the same keys) and it writes the header and the rows. Values with commas
or quotes in them are quoted for you, so spreadsheets read them back
correctly.

```vyrn
use vyrn.csv

main:
    fruit = [
        {"name": "apple", "price": 0.5, "note": "red, crunchy"},
        {"name": "pear", "price": 0.75, "note": "green"},
    ]
    csv.write("fruit.csv", fruit)
    print(read_text("fruit.csv"))
    back = csv.read("fruit.csv")
    print(back[0]["note"])
end
```

```text
name,price,note
apple,0.5,"red, crunchy"
pear,0.75,green

red, crunchy
```

The note `red, crunchy` has a comma in it, so it was written in quotes;
reading it back gives the note without them.

### Exercise

Write a CSV of three squares, with columns `n` and `square`, then print the
file.

```vyrn starter
use vyrn.csv

main:
    rows = []
end
```

```text expect
n,square
1,1
2,4
3,9
```

> Hint: `rows.append({"n": n, "square": n * n})` inside `each n in 1 to 3:`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.csv

main:
    rows = []
    each n in 1 to 3:
        rows.append({"n": n, "square": n * n})
    end
    csv.write("squares.csv", rows)
    print(read_text("squares.csv").rstrip())
end
```

</details>

## 25.3 Tables: data you can ask questions

A list of maps is fine for a few rows. For real data, a **table** is
easier: it knows its columns, turns numbers into numbers, prints neatly,
and answers questions about whole columns at once.

```vyrn
use vyrn.table

main:
    write_text("weather.csv", "day,temp,rain\nMon,12,3\nTue,15,0\nWed,9,8\nThu,14,1\n")
    t = table.read_csv("weather.csv")
    print(t)
    print("columns:", t.columns, "rows:", len(t))
    print("average temperature:", t.mean("temp"))
    print("total rain:", t.sum("rain"))
    print("warmest:", t.max("temp"))
end
```

```text
day  temp  rain
---  ----  ----
Mon    12     3
Tue    15     0
Wed     9     8
Thu    14     1
columns: ["day", "temp", "rain"] rows: 4
average temperature: 12.5
total rain: 12
warmest: 15
```

Tables never change in place: `sort`, `where` and `select` give you a new
table, so you can try things freely.

```vyrn
use vyrn.table

main:
    t = table.from_csv("day,temp,rain\nMon,12,3\nTue,15,0\nWed,9,8\nThu,14,1\n")
    print(t.where("rain", ">", 2))
    print(t.sort("temp", descending = true).select(["day", "temp"]).head(2))
    each row in t.where("rain", "==", 0):
        print("dry day:", row["day"])
    end
end
```

```text
day  temp  rain
---  ----  ----
Mon    12     3
Wed     9     8
day  temp
---  ----
Tue    15
Thu    14
dry day: Tue
```

| Asking | Writing |
|---|---|
| rows where rain is over 2 | `t.where("rain", ">", 2)` |
| warmest first | `t.sort("temp", descending = true)` |
| only some columns | `t.select(["day", "temp"])` |
| the first few rows | `t.head(2)` |
| one whole column, as a list | `t.column("temp")` |

### Exercise

From the table of pupils, print the names of those who scored 70 or more,
best first, one per line.

```vyrn starter
use vyrn.table

main:
    t = table.from_csv("name,score\nAmi,64\nBen,91\nCal,70\nDee,85\n")
end
```

```text expect
Ben
Dee
Cal
```

> Hint: chain them: `t.where(...).sort(...)`.
> Hint: `each row in ...: print(row["name"])`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.table

main:
    t = table.from_csv("name,score\nAmi,64\nBen,91\nCal,70\nDee,85\n")
    each row in t.where("score", ">=", 70).sort("score", descending = true):
        print(row["name"])
    end
end
```

</details>

## 25.4 Project: name the states

The game: a CSV file lists states with their capitals. The player types
state names; each correct one is ticked off, and at the end the game says
which ones were missed and saves them to a new CSV file, to study later.

```vyrn
use vyrn.csv

## The states not yet guessed, from the CSV rows.
create missing(rows: list, guessed: set) -> list:
    return [row["state"] each row in rows if row["state"] not in guessed]
end

main:
    write_text("states.csv", "state,capital\nOhio,Columbus\nTexas,Austin\nMaine,Augusta\nIdaho,Boise\n")
    rows = csv.read("states.csv")
    names = {row["state"].lower(): row["state"] each row in rows}
    guessed = set()
    each answer in ["texas", "OHIO", "Florida", "idaho", "texas"]:
        key = answer.strip().lower()
        if key not in names:
            print(answer, "is not one of them")
        else if names[key] in guessed:
            print("you already had", names[key])
        else:
            guessed.add(names[key])
            print("yes!", names[key], "-", len(guessed), "of", len(rows))
        end
    end
    left = missing(rows, guessed)
    print("missed:", ", ".join(left))
    csv.write("to_learn.csv", [row each row in rows if row["state"] in left])
    print(read_text("to_learn.csv").rstrip())
end
```

```text
yes! Texas - 1 of 4
yes! Ohio - 2 of 4
Florida is not one of them
yes! Idaho - 3 of 4
you already had Texas
missed: Maine
state,capital
Maine,Augusta
```

The answers are a list here so the example always prints the same thing.
In the real game, read them with `input()` in a `loop`, and stop when the
player types `exit` or has named them all.

The trick that makes it forgiving is the map from lower-case names to the
real names: `"OHIO"` and `"ohio"` both find `"Ohio"`.

### Exercise

Finish `score`: given the rows and a list of guesses, it gives how many
different states were named correctly, ignoring case and spaces at the
ends.

```vyrn starter
use vyrn.csv

create score(rows: list, guesses: list) -> int:
    return 0
end

main:
    rows = csv.parse("state,capital\nOhio,Columbus\nTexas,Austin\nMaine,Augusta\n")
    print(score(rows, [" ohio", "Texas", "texas", "Utah"]))
end
```

```text expect
2
```

> Hint: make a set of the real names in lower case, and a set of the
> cleaned guesses.
> Hint: the answer is how many cleaned guesses are in the set of names:
> `len(names & guessed)` works on two sets.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.csv

create score(rows: list, guesses: list) -> int:
    names = {row["state"].lower() each row in rows}
    guessed = {g.strip().lower() each g in guesses}
    return len(names & guessed)
end

main:
    rows = csv.parse("state,capital\nOhio,Columbus\nTexas,Austin\nMaine,Augusta\n")
    print(score(rows, [" ohio", "Texas", "texas", "Utah"]))
end
```

</details>

## Today you learned

- A CSV file is text: a header line, then one line per row, commas between.
- `csv.read` gives a list of maps; every value is text until you convert it.
- `csv.write` writes maps back, quoting values that need it.
- A **table** (`vyrn.table`) prints neatly and answers questions about
  columns: `mean`, `sum`, `max`, `where`, `sort`, `select`, `head`.
- Tables are never changed in place; every question gives a new table.
