# Day 38: Databases

JSON and CSV files are fine for a few hundred records. When you have
thousands, want to search them quickly, or have several programs using the
same data at once, you want a **database**. Today you use SQLite, the
database inside every phone and web browser, which keeps everything in one
file. You will store records the easy way first, then learn **SQL**, the
language almost every database speaks. The project is a workout log that
answers questions like "how far did I run this month?".

## 38.1 Tables, rows and columns

A database holds **tables**. A table is like a spreadsheet: each **row** is
one record, each **column** one kind of information, and every value in a
column has the same type.

| id | date | exercise | minutes |
|---|---|---|---|
| 1 | 2026-11-10 | running | 30 |
| 2 | 2026-11-11 | swimming | 45 |

`db.open` opens a database file (making it if it isn't there);
`":memory:"` gives one that lives only while the program runs, which is
handy for trying things out. `create_table` makes a table from a map of
column names and types, adding an `id` column that numbers the rows:

```vyrn
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.create_table("workouts", {"date": "text", "exercise": "text", "minutes": "int"})
    print(conn.insert("workouts", {"date": "2026-11-10", "exercise": "running", "minutes": 30}))
    print(conn.insert("workouts", {"date": "2026-11-11", "exercise": "swimming", "minutes": 45}))
    print(conn.insert("workouts", {"date": "2026-11-12", "exercise": "running", "minutes": 25}))
    each row in conn.all("workouts"):
        print(row)
    end
end
```

```text
1
2
3
{"id": 1, "date": "2026-11-10", "exercise": "running", "minutes": 30}
{"id": 2, "date": "2026-11-11", "exercise": "swimming", "minutes": 45}
{"id": 3, "date": "2026-11-12", "exercise": "running", "minutes": 25}
```

`insert` gives the new row's `id`, and every row comes back as a map.

## 38.2 Records without SQL

Finding, changing and deleting rows takes a `where` map: the rows whose
columns have all those values.

```vyrn
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.create_table("workouts", {"date": "text", "exercise": "text", "minutes": "int"})
    conn.insert("workouts", {"date": "2026-11-10", "exercise": "running", "minutes": 30})
    conn.insert("workouts", {"date": "2026-11-11", "exercise": "swimming", "minutes": 45})
    conn.insert("workouts", {"date": "2026-11-12", "exercise": "running", "minutes": 25})

    print(len(conn.find("workouts", {"exercise": "running"})), "runs")
    print(conn.find_one("workouts", {"date": "2026-11-11"})["exercise"])
    print(conn.update("workouts", {"id": 3}, {"minutes": 35}), "changed")
    print(conn.delete("workouts", {"exercise": "swimming"}), "deleted")
    print([r["minutes"] each r in conn.all("workouts")])
end
```

```text
2 runs
swimming
1 changed
1 deleted
[30, 35]
```

`update` and `delete` give how many rows they changed. `delete` insists on
a `where`, so you can't empty a table by accident.

### Exercise

Make a `books` table with a title, an author and a `read` flag (`int`, 0 or
1). Add the three books, mark *Dune* as read, and print the titles of the
books not read yet.

```vyrn starter
use vyrn.db

main:
    let conn = db.open(":memory:")
end
```

```text expect
["Emma", "Ubik"]
```

> Hint: `conn.create_table("books", {"title": "text", "author": "text", "read": "int"})`
> Hint: `conn.update("books", {"title": "Dune"}, {"read": 1})`, then
> `conn.find("books", {"read": 0})`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.create_table("books", {"title": "text", "author": "text", "read": "int"})
    conn.insert("books", {"title": "Dune", "author": "Frank Herbert", "read": 0})
    conn.insert("books", {"title": "Emma", "author": "Jane Austen", "read": 0})
    conn.insert("books", {"title": "Ubik", "author": "Philip K. Dick", "read": 0})
    conn.update("books", {"title": "Dune"}, {"read": 1})
    print([b["title"] each b in conn.find("books", {"read": 0})])
end
```

</details>

## 38.3 Asking in SQL

`find` only matches exact values. To ask "workouts longer than 30
minutes", "the total per exercise" or "the five most recent", you write
**SQL** (Structured Query Language). `conn.query(sql)` runs a question and
gives the rows:

```vyrn
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.create_table("workouts", {"date": "text", "exercise": "text", "minutes": "int"})
    each date, exercise, minutes in [("2026-11-09", "cycling", 60), ("2026-11-10", "running", 30), ("2026-11-11", "swimming", 45), ("2026-11-12", "running", 25)]:
        conn.insert("workouts", {"date": date, "exercise": exercise, "minutes": minutes})
    end

    print(conn.query("select date, minutes from workouts where minutes > 30"))
    print(conn.query("select exercise from workouts order by minutes desc limit 2"))
    each row in conn.query("select exercise, sum(minutes) as total, count(*) as times from workouts group by exercise order by total desc"):
        print("{row["exercise"]:<10} {row["total"]:>4} min in {row["times"]}")
    end
    print(conn.value("select avg(minutes) from workouts"))
end
```

```text
[{"date": "2026-11-09", "minutes": 60}, {"date": "2026-11-11", "minutes": 45}]
[{"exercise": "cycling"}, {"exercise": "swimming"}]
cycling      60 min in 1
running      55 min in 2
swimming     45 min in 1
40.0
```

Read a `select` like a sentence:

| Part | Means |
|---|---|
| `select date, minutes` | which columns to give (`*` for all) |
| `from workouts` | which table |
| `where minutes > 30` | only rows where this is true (`and`, `or`, `=`, `!=`, `like`) |
| `group by exercise` | one result row per exercise, combining the rows with `sum`, `count`, `avg`, `min`, `max` |
| `order by total desc` | sorted, biggest first |
| `limit 2` | at most two rows |

`conn.value` gives just the first column of the first row: handy for a
single number.

### Exercise

How many minutes of running are there in total? Give the answer with one
`conn.value` call.

```vyrn starter
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table workouts (exercise text, minutes int)")
    conn.execute("insert into workouts values ('running', 30), ('swimming', 45), ('running', 25)")
end
```

```text expect
55
```

> Hint: `select sum(minutes) from workouts where exercise = 'running'`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table workouts (exercise text, minutes int)")
    conn.execute("insert into workouts values ('running', 30), ('swimming', 45), ('running', 25)")
    print(conn.value("select sum(minutes) from workouts where exercise = 'running'"))
end
```

</details>

## 38.4 Never build SQL from text

The exercise wrote `'running'` straight into the SQL. With a value that
came from a user, **never** do that by joining text: a name like
`x' or '1'='1` would change the meaning of the statement. This is called
**SQL injection**, and it is one of the most common ways websites get
broken into. Instead, write `?` where a value goes and pass the values
separately; the database keeps them apart from the statement:

```vyrn
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table users (name text, secret text)")
    conn.execute("insert into users values (?, ?)", ["ada", "piano"])
    conn.execute("insert into users values (?, ?)", ["alan", "chess"])

    let typed = "nobody' or '1'='1"
    let risky = conn.query("select name from users where name = '" + typed + "'")
    print("joined text found", len(risky), "users")
    let safe = conn.query("select name from users where name = ?", [typed])
    print("with ? found", len(safe), "users")
end
```

```text
joined text found 2 users
with ? found 0 users
```

The joined text matched *every* user. With `?` the typed text is just a
name nobody has. `insert`, `find` and the other record functions always
work this way, so they're safe too.

## 38.5 Project: the workout log

The log keeps workouts in a file, `workouts.db`, so they're still there
next time. A small object wraps the database with the questions the app
asks, so the rest of the program never writes SQL:

```vyrn
use vyrn.db
use vyrn.fs

object Log:
    conn: any

    create add(self, date: str, exercise: str, minutes: int, km: float = 0.0) -> int:
        return self.conn.insert("workouts", {"date": date, "exercise": exercise, "minutes": minutes, "km": km})
    end

    create month(self, month: str) -> list:
        return self.conn.query("select exercise, sum(minutes) as minutes, sum(km) as km from workouts where date like ? group by exercise order by minutes desc", [month + "-%"])
    end

    create best(self, exercise: str) -> map?:
        return self.conn.query_one("select date, km from workouts where exercise = ? order by km desc limit 1", [exercise])
    end
end

create open_log(path: str) -> Log:
    let conn = db.open(path)
    if "workouts" not in conn.tables():
        conn.create_table("workouts", {"date": "text", "exercise": "text", "minutes": "int", "km": "real"})
    end
    return Log(conn)
end

main:
    if fs.exists("workouts.db"): fs.remove("workouts.db")
    var log = open_log("workouts.db")
    log.add("2026-10-30", "running", 40, 7.5)
    log.add("2026-11-02", "running", 30, 5.0)
    log.add("2026-11-05", "swimming", 45)
    log.add("2026-11-09", "running", 52, 10.2)
    log.conn.close()

    log = open_log("workouts.db")
    each row in log.month("2026-11"):
        print("{row["exercise"]}: {row["minutes"]} min, {row["km"]} km")
    end
    let best = log.best("running")
    print("longest run: {best["km"]} km on {best["date"]}")
end
```

```text
running: 82 min, 15.2 km
swimming: 45 min, 0.0 km
longest run: 10.2 km on 2026-11-09
```

The program closes the database and opens it again halfway, as if it were
run a second time: the workouts are still there. `like ?` with `"2026-11-%"`
matches every date that starts with `2026-11-`; `%` stands for any text.

### Exercise

Add a method `streak(self, dates)` that is simpler than it sounds: given a
list of the dates to check, give how many of them have at least one workout.

```vyrn starter
use vyrn.db

object Log:
    conn: any

    create streak(self, dates: list) -> int:
        return 0
    end
end

main:
    let conn = db.open(":memory:")
    conn.create_table("workouts", {"date": "text", "minutes": "int"})
    each d in ["2026-11-10", "2026-11-11", "2026-11-11", "2026-11-13"]: conn.insert("workouts", {"date": d, "minutes": 20})
    print(Log(conn).streak(["2026-11-10", "2026-11-11", "2026-11-12", "2026-11-13"]))
end
```

```text expect
3
```

> Hint: for each date, `self.conn.find_one("workouts", {"date": d})` is
> `none` when there was no workout that day.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

object Log:
    conn: any

    create streak(self, dates: list) -> int:
        return len([d each d in dates if self.conn.find_one("workouts", {"date": d}) != none])
    end
end

main:
    let conn = db.open(":memory:")
    conn.create_table("workouts", {"date": "text", "minutes": "int"})
    each d in ["2026-11-10", "2026-11-11", "2026-11-11", "2026-11-13"]: conn.insert("workouts", {"date": d, "minutes": 20})
    print(Log(conn).streak(["2026-11-10", "2026-11-11", "2026-11-12", "2026-11-13"]))
end
```

</details>

## Today you learned

- A database holds tables of rows and columns; SQLite keeps one in a file.
- `create_table`, `insert`, `all`, `find`, `update` and `delete` handle
  records without SQL.
- SQL's `select ... from ... where ... group by ... order by ... limit`
  asks bigger questions; `query`, `query_one` and `value` run them.
- Always pass values with `?`, never by joining text: that's how SQL
  injection is stopped.
