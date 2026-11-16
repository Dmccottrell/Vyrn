# Day 54: Tables of data

This unit is about **data analysis**: taking a pile of numbers and finding
out what they say. Most data comes as a table, rows and columns, like a
spreadsheet. `vyrn.table` holds such tables and answers questions about
them in a line each. Today you load data, look at it, and (the part that
takes most of a real analyst's time) clean it. The project cleans a messy
survey.

## 54.1 Loading and looking

`table.read_csv(path)` loads a CSV file; `table.from_csv(text)` reads it
from text. Numbers become numbers and empty cells become `none`.
Printing a table lines it up:

```vyrn
use vyrn.table

const DATA = "name,age,city,score\nAda,36,London,91\nAlan,41,Leeds,78\nGrace,,York,88\nMo,29,Leeds,\nKai,52,London,64\n"

main:
    let t = table.from_csv(DATA)
    print(t)
    print(t.shape, t.columns)
    print(t.head(2))
    print(t.column("name"), t.row(0))
end
```

```text
name   age  city    score
-----  ---  ------  -----
Ada     36  London     91
Alan    41  Leeds      78
Grace       York       88
Mo      29  Leeds
Kai     52  London     64
(5, 4) ["name", "age", "city", "score"]
name  age  city    score
----  ---  ------  -----
Ada    36  London     91
Alan   41  Leeds      78
["Ada", "Alan", "Grace", "Mo", "Kai"] {"name": "Ada", "age": 36, "city": "London", "score": 91}
```

Always look at the data first. Already you can see two empty cells: Grace
has no age, Mo has no score.

## 54.2 Summaries

One method answers each everyday question about a column. Empty cells are
skipped:

```vyrn
use vyrn.table

const DATA = "name,age,city,score\nAda,36,London,91\nAlan,41,Leeds,78\nGrace,,York,88\nMo,29,Leeds,\nKai,52,London,64\n"

main:
    let t = table.from_csv(DATA)
    print("mean score:", t.mean("score"), "median:", t.median("score"))
    print("ages:", t.min("age"), "to", t.max("age"), "- known for", t.count("age"))
    print("cities:", t.unique("city"))
    print(t.describe())
end
```

```text
mean score: 80.25 median: 83.0
ages: 29 to 52 - known for 4
cities: ["London", "Leeds", "York"]
column  count   mean   min   max
------  -----  -----  ----  ----
age         4   39.5  29.0  52.0
score       4  80.25  64.0  91.0
```

`describe()` summarises every number column at once: a quick first look
at any new data.

## 54.3 Choosing rows and columns

`where` keeps the rows where one column compares to a value; `keep` takes
a list of true/false, one per row, for anything more complicated. `sort`
and `select` give new tables too. None of them change the original:

```vyrn
use vyrn.table

const DATA = "name,age,city,score\nAda,36,London,91\nAlan,41,Leeds,78\nGrace,,York,88\nMo,29,Leeds,\nKai,52,London,64\n"

main:
    let t = table.from_csv(DATA)
    print(t.where("city", "==", "Leeds").column("name"))
    print(t.where("score", ">=", 80).select(["name", "score"]))
    let older_londoners = t.keep([r["city"] == "London" and (r["age"] ?? 0) > 40 each r in t])
    print(older_londoners.column("name"))
    print(t.sort("score", descending = true).column("name"))
end
```

```text
["Alan", "Mo"]
name   score
-----  -----
Ada       91
Grace     88
["Kai"]
["Ada", "Grace", "Alan", "Kai", "Mo"]
```

`each r in t` gives each row as a map, so `keep`'s list is a
comprehension. Rows with an empty score sort last.

### Exercise

Find the names of people under 40 with a score of at least 80, best score
first.

```vyrn starter
use vyrn.table

const DATA = "name,age,score\nAda,36,91\nAlan,41,78\nZoe,22,85\nMo,29,79\nKai,31,88\n"

main:
    let t = table.from_csv(DATA)
end
```

```text expect
["Ada", "Kai", "Zoe"]
```

> Hint: `t.keep([r["age"] < 40 and r["score"] >= 80 each r in t])`
> Hint: then `.sort("score", descending = true).column("name")`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.table

const DATA = "name,age,score\nAda,36,91\nAlan,41,78\nZoe,22,85\nMo,29,79\nKai,31,88\n"

main:
    let t = table.from_csv(DATA)
    let young_and_good = t.keep([r["age"] < 40 and r["score"] >= 80 each r in t])
    print(young_and_good.sort("score", descending = true).column("name"))
end
```

</details>

## 54.4 Cleaning

Real data is messy. People type `leeds`, ` Leeds ` and `LEEDS`; forms get
submitted twice; someone's age is 410. Analysis on dirty data gives wrong
answers with complete confidence, so clean first. The usual steps:

| Problem | Fix |
|---|---|
| missing values | drop the rows (`drop_missing`) or fill them (`fill_missing`) |
| inconsistent text | strip spaces, fix case: a new column from a comprehension |
| impossible values | keep only rows in the allowed range |
| duplicates | keep the first row for each key |
| bad column names | `rename` |

```vyrn
use vyrn.table

const DATA = "Name,Age,City\nAda,36, london\nAlan,410,LEEDS\nGrace,,York\nMo,29,leeds \n"

main:
    var t = table.from_csv(DATA)
    t = t.rename("Name", "name").rename("Age", "age").rename("City", "city")
    let cities = [r["city"].strip().title() each r in t]
    t = t.drop("city").add_column("city", cities)
    print(t)
    print(t.drop_missing("age").column("name"))
    print(t.fill_missing("age", 0).column("age"))
    let sensible = t.keep([r["age"] == none or 0 < r["age"] < 120 each r in t])
    print(sensible.column("name"), sensible.unique("city"))
end
```

```text
name   age  city
-----  ---  ------
Ada     36  London
Alan   410  Leeds
Grace       York
Mo      29  Leeds
["Ada", "Alan", "Mo"]
[36, 410, 0, 29]
["Ada", "Grace", "Mo"] ["London", "York", "Leeds"]
```

Should Grace's missing age be dropped or filled? It depends on the
question. For "average age", drop it: filling with 0 would drag the
average down. For "how many people answered", keep her. Write down each
cleaning decision; someone will ask.

## 54.5 Project: cleaning a survey

A survey asked people their age, country and how many hours a week they
spend reading. The raw answers have every problem above. The cleaning is
a pipeline of small functions, each with one job, so each can be checked
on its own:

```vyrn
use vyrn.table

const RAW = "id,age,country,hours\n1,34,UK,5\n2,27, uk ,3\n3,,France,4\n3,,France,4\n4,19,USA,\n5,250,UK,2\n6,45,united kingdom,7\n7,31,France,-1\n8,22,USA,12\n"

const COUNTRY = {"uk": "UK", "united kingdom": "UK", "france": "France", "usa": "USA"}

create fix_countries(t: any) -> any:
    let fixed = [COUNTRY.get(r["country"].strip().lower(), r["country"].strip()) each r in t]
    return t.drop("country").add_column("country", fixed)
end

create drop_duplicates(t: any, key: str) -> any:
    var seen = set()
    var mask = []
    each r in t:
        mask.append(r[key] not in seen)
        seen.add(r[key])
    end
    return t.keep(mask)
end

create plausible(t: any) -> any:
    return t.keep([(r["age"] == none or 10 <= r["age"] <= 100) and (r["hours"] == none or 0 <= r["hours"] <= 100) each r in t])
end

main:
    let raw = table.from_csv(RAW)
    let clean = plausible(drop_duplicates(fix_countries(raw), "id"))
    print("rows: {len(raw)} raw, {len(clean)} clean")
    print(clean)
    let answered = clean.drop_missing("hours")
    print(answered.group_by("country", "hours", "mean"))
end
```

```text
rows: 9 raw, 6 clean
id  age  hours  country
--  ---  -----  -------
 1   34      5  UK
 2   27      3  UK
 3           4  France
 4   19         USA
 6   45      7  UK
 8   22     12  USA
country  hours_mean
-------  ----------
UK              5.0
France          4.0
USA            12.0
```

From 9 answers, 6 survive: one duplicate, an age of 250 and a negative
reading time are gone, and "uk", " uk " and "united kingdom" are all
"UK". `group_by` gives one row per country (Day 55 goes further). The
answer is only as good as the cleaning, so the cleaning steps are code you
can read, test and run again, not edits made by hand in a spreadsheet.

### Exercise

Write `report(raw, clean)`, giving a one-line summary of what cleaning
removed: `"kept 6 of 9 rows (3 removed)"`.

```vyrn starter
use vyrn.table

create report(raw: any, clean: any) -> str:
    return ""
end

main:
    let raw = table.from_csv("x\n1\n2\n3\n")
    print(report(raw, raw.where("x", "<", 3)))
end
```

```text expect
kept 2 of 3 rows (1 removed)
```

> Hint: `len(t)` is a table's number of rows.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.table

create report(raw: any, clean: any) -> str:
    return "kept {len(clean)} of {len(raw)} rows ({len(raw) - len(clean)} removed)"
end

main:
    let raw = table.from_csv("x\n1\n2\n3\n")
    print(report(raw, raw.where("x", "<", 3)))
end
```

</details>

## Today you learned

- `table.read_csv` loads data; `print`, `head`, `shape` and `describe` let
  you look before you analyse.
- `mean`, `median`, `min`, `max`, `count` and `unique` summarise a column.
- `where`, `keep`, `sort` and `select` make new tables from old.
- Clean before analysing: missing values, inconsistent text, impossible
  values, duplicates. Keep the cleaning in code, and write down why.
