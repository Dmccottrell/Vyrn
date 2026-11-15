# Day 57: SQL for analysis

On Day 38 you used SQL to store and find records. SQL is also one of the
most-used tools for *analysis*: companies keep their data in databases,
and analysts ask it questions in SQL directly, without loading everything
into a program first. Today you learn the SQL that answers questions:
joins, grouping with conditions, subqueries and views. The project answers
questions about a small music shop's database.

## 57.1 The shop

Three tables, linked by ids, so each fact is stored once:

```text
artists (id, name, country)
albums  (id, artist_id, title, year, price)
sales   (id, album_id, day, copies)
```

This helper builds the database for the examples; every example below
starts from it:

```vyrn
use vyrn.db

create shop() -> any:
    let conn = db.open(":memory:")
    conn.execute("create table artists (id integer primary key, name text, country text)")
    conn.execute("create table albums (id integer primary key, artist_id int, title text, year int, price real)")
    conn.execute("create table sales (id integer primary key, album_id int, day text, copies int)")
    conn.execute_many("insert into artists values (?, ?, ?)", [[1, "Nina Simone", "US"], [2, "Kraftwerk", "DE"], [3, "Björk", "IS"], [4, "Fela Kuti", "NG"]])
    conn.execute_many("insert into albums values (?, ?, ?, ?, ?)", [[1, 1, "Pastel Blues", 1965, 9.99], [2, 2, "Autobahn", 1974, 11.5], [3, 2, "Computer World", 1981, 12.0], [4, 3, "Homogenic", 1997, 10.0], [5, 4, "Zombie", 1976, 8.5], [6, 3, "Vespertine", 2001, 10.0]])
    conn.execute_many("insert into sales (album_id, day, copies) values (?, ?, ?)", [[1, "2026-11-01", 3], [2, "2026-11-01", 1], [3, "2026-11-02", 4], [4, "2026-11-02", 2], [1, "2026-11-03", 2], [5, "2026-11-03", 5], [3, "2026-11-04", 1], [4, "2026-11-04", 3]])
    return conn
end

main:
    let conn = shop()
    print(conn.tables())
    print(conn.value("select count(*) from albums"), "albums,", conn.value("select sum(copies) from sales"), "copies sold")
end
```

```text
["albums", "artists", "sales"]
6 albums, 21 copies sold
```

## 57.2 Joins in SQL

`join ... on` combines rows from two tables where the condition holds,
just like Day 55's table joins. Give tables short names (`al` for albums)
to keep the query readable:

```vyrn
use vyrn.db

create shop() -> any:
    let conn = db.open(":memory:")
    conn.execute("create table artists (id integer primary key, name text, country text)")
    conn.execute("create table albums (id integer primary key, artist_id int, title text, year int, price real)")
    conn.execute_many("insert into artists values (?, ?, ?)", [[1, "Nina Simone", "US"], [2, "Kraftwerk", "DE"], [3, "Björk", "IS"], [4, "Fela Kuti", "NG"]])
    conn.execute_many("insert into albums values (?, ?, ?, ?, ?)", [[1, 1, "Pastel Blues", 1965, 9.99], [2, 2, "Autobahn", 1974, 11.5], [3, 2, "Computer World", 1981, 12.0], [4, 3, "Homogenic", 1997, 10.0], [5, 4, "Zombie", 1976, 8.5], [6, 3, "Vespertine", 2001, 10.0]])
    return conn
end

main:
    let conn = shop()
    each row in conn.query("select ar.name, al.title, al.year from albums al join artists ar on al.artist_id = ar.id where al.year < 1980 order by al.year"):
        print("{row["year"]}  {row["title"]:<15} {row["name"]}")
    end
end
```

```text
1965  Pastel Blues    Nina Simone
1974  Autobahn        Kraftwerk
1976  Zombie          Fela Kuti
```

`left join` keeps rows from the first table with no match, filling the
other table's columns with `null` (`none` in Vyrn), exactly like
`how = "left"`.

## 57.3 Grouping with conditions

`group by` makes one row per group, and the aggregate functions (`sum`,
`count`, `avg`, `min`, `max`) summarise each. `having` filters the groups
after they're made, where `where` filters rows before:

```vyrn
use vyrn.db

create shop() -> any:
    let conn = db.open(":memory:")
    conn.execute("create table artists (id integer primary key, name text, country text)")
    conn.execute("create table albums (id integer primary key, artist_id int, title text, year int, price real)")
    conn.execute("create table sales (id integer primary key, album_id int, day text, copies int)")
    conn.execute_many("insert into artists values (?, ?, ?)", [[1, "Nina Simone", "US"], [2, "Kraftwerk", "DE"], [3, "Björk", "IS"], [4, "Fela Kuti", "NG"]])
    conn.execute_many("insert into albums values (?, ?, ?, ?, ?)", [[1, 1, "Pastel Blues", 1965, 9.99], [2, 2, "Autobahn", 1974, 11.5], [3, 2, "Computer World", 1981, 12.0], [4, 3, "Homogenic", 1997, 10.0], [5, 4, "Zombie", 1976, 8.5], [6, 3, "Vespertine", 2001, 10.0]])
    conn.execute_many("insert into sales (album_id, day, copies) values (?, ?, ?)", [[1, "2026-11-01", 3], [2, "2026-11-01", 1], [3, "2026-11-02", 4], [4, "2026-11-02", 2], [1, "2026-11-03", 2], [5, "2026-11-03", 5], [3, "2026-11-04", 1], [4, "2026-11-04", 3]])
    return conn
end

main:
    let conn = shop()
    let sql = "select ar.name, sum(s.copies) as copies, round(sum(s.copies * al.price), 2) as revenue from sales s join albums al on s.album_id = al.id join artists ar on al.artist_id = ar.id group by ar.name having sum(s.copies) >= 5 order by revenue desc"
    each row in conn.query(sql):
        print("{row["name"]:<12} {row["copies"]:>3} copies  £{row["revenue"]}")
    end
end
```

```text
Kraftwerk      6 copies  £71.5
Björk          5 copies  £50.0
Nina Simone    5 copies  £49.95
Fela Kuti      5 copies  £42.5
```

The query joins three tables, works out revenue per sale line, adds it up
per artist, and keeps artists who sold at least 5 copies. Read it in the
order the database does: `from` and `join` (which rows), `where` (which
rows to keep), `group by` (which groups), `having` (which groups to
keep), `select` (which columns), `order by`.

### Exercise

Which day sold the most copies? Write one query giving the day and its
total, biggest first, and print the top row.

```vyrn starter
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table sales (album_id int, day text, copies int)")
    conn.execute_many("insert into sales values (?, ?, ?)", [[1, "2026-11-01", 3], [2, "2026-11-01", 1], [3, "2026-11-02", 4], [4, "2026-11-02", 2], [1, "2026-11-03", 2], [5, "2026-11-03", 5]])
end
```

```text expect
2026-11-03 7
```

> Hint: `select day, sum(copies) as total from sales group by day order by total desc limit 1`
> Hint: `conn.query_one(...)` gives the first row as a map.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table sales (album_id int, day text, copies int)")
    conn.execute_many("insert into sales values (?, ?, ?)", [[1, "2026-11-01", 3], [2, "2026-11-01", 1], [3, "2026-11-02", 4], [4, "2026-11-02", 2], [1, "2026-11-03", 2], [5, "2026-11-03", 5]])
    let best = conn.query_one("select day, sum(copies) as total from sales group by day order by total desc limit 1")
    print(best["day"], best["total"])
end
```

</details>

## 57.4 Subqueries and views

A **subquery** is a query inside a query, used as a value or a list.
"Albums that never sold" asks for albums whose id is *not in* the list of
ids in `sales`:

```vyrn
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table albums (id integer primary key, title text, price real)")
    conn.execute("create table sales (album_id int, copies int)")
    conn.execute_many("insert into albums values (?, ?, ?)", [[1, "Pastel Blues", 9.99], [2, "Autobahn", 11.5], [6, "Vespertine", 10.0]])
    conn.execute_many("insert into sales values (?, ?)", [[1, 3], [2, 1]])
    print(conn.query("select title from albums where id not in (select album_id from sales)"))
    print(conn.query("select title, price from albums where price > (select avg(price) from albums)"))
end
```

```text
[{"title": "Vespertine"}]
[{"title": "Autobahn", "price": 11.5}]
```

A **view** saves a query under a name, so it can be used like a table.
Put a complicated join in a view once, and every later question gets
simpler:

```vyrn
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table albums (id integer primary key, title text, price real)")
    conn.execute("create table sales (album_id int, copies int)")
    conn.execute_many("insert into albums values (?, ?, ?)", [[1, "Pastel Blues", 9.99], [2, "Autobahn", 11.5]])
    conn.execute_many("insert into sales values (?, ?)", [[1, 3], [2, 1], [1, 2]])
    conn.execute("create view sale_lines as select al.title, s.copies, s.copies * al.price as revenue from sales s join albums al on s.album_id = al.id")
    print(conn.query("select title, sum(revenue) as total from sale_lines group by title order by total desc"))
end
```

```text
[{"title": "Pastel Blues", "total": 49.95}, {"title": "Autobahn", "total": 11.5}]
```

## 57.5 Project: questions for the shop

The owner's questions, each answered by one query against a view of every
sale line with its album and artist:

```vyrn
use vyrn.db

create shop() -> any:
    let conn = db.open(":memory:")
    conn.execute("create table artists (id integer primary key, name text, country text)")
    conn.execute("create table albums (id integer primary key, artist_id int, title text, year int, price real)")
    conn.execute("create table sales (id integer primary key, album_id int, day text, copies int)")
    conn.execute_many("insert into artists values (?, ?, ?)", [[1, "Nina Simone", "US"], [2, "Kraftwerk", "DE"], [3, "Björk", "IS"], [4, "Fela Kuti", "NG"]])
    conn.execute_many("insert into albums values (?, ?, ?, ?, ?)", [[1, 1, "Pastel Blues", 1965, 9.99], [2, 2, "Autobahn", 1974, 11.5], [3, 2, "Computer World", 1981, 12.0], [4, 3, "Homogenic", 1997, 10.0], [5, 4, "Zombie", 1976, 8.5], [6, 3, "Vespertine", 2001, 10.0]])
    conn.execute_many("insert into sales (album_id, day, copies) values (?, ?, ?)", [[1, "2026-11-01", 3], [2, "2026-11-01", 1], [3, "2026-11-02", 4], [4, "2026-11-02", 2], [1, "2026-11-03", 2], [5, "2026-11-03", 5], [3, "2026-11-04", 1], [4, "2026-11-04", 3]])
    conn.execute("create view lines as select s.day, s.copies, al.title, al.year, al.price, ar.name as artist, ar.country, s.copies * al.price as revenue from sales s join albums al on s.album_id = al.id join artists ar on al.artist_id = ar.id")
    return conn
end

const QUESTIONS = [
    ("Total revenue", "select round(sum(revenue), 2) as answer from lines"),
    ("Best-selling album", "select title as answer from lines group by title order by sum(copies) desc, title limit 1"),
    ("Revenue by decade", "select (year / 10) * 10 as decade, round(sum(revenue), 2) as revenue from lines group by decade order by decade"),
    ("Countries never sold", "select country as answer from artists where country not in (select country from lines)"),
    ("Average sale (copies)", "select round(avg(copies), 2) as answer from lines"),
]

main:
    let conn = shop()
    each question, sql in QUESTIONS:
        let rows = conn.query(sql)
        if len(rows) == 1 and "answer" in rows[0]:
            print("{question}: {rows[0]["answer"]}")
        else:
            print("{question}:")
            each r in rows:
                print("   ", r)
            end
        end
    end
end
```

```text
Total revenue: 213.95
Best-selling album: Computer World
Revenue by decade:
    {"decade": 1960, "revenue": 49.95}
    {"decade": 1970, "revenue": 54.0}
    {"decade": 1980, "revenue": 60.0}
    {"decade": 1990, "revenue": 50.0}
Countries never sold:
Average sale (copies): 2.63
```

"Countries never sold" has no rows: every country has sold something. Four
albums tie for best-selling with 5 copies each, so that query breaks the tie
by title (`order by sum(copies) desc, title`); without a tie-breaker, which
one comes first is up to the database. Watch for ties in any "top" question.
Keeping questions as data (a list of name and SQL) makes adding the next
question one line, and the same list could feed a web dashboard (the
capstone).

### Exercise

Add a question: the artist with the most different albums *in the shop*
(not sold): name and count, from the `albums` and `artists` tables.

```vyrn starter
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table artists (id integer primary key, name text)")
    conn.execute("create table albums (id integer primary key, artist_id int, title text)")
    conn.execute_many("insert into artists values (?, ?)", [[1, "Nina Simone"], [2, "Kraftwerk"], [3, "Björk"]])
    conn.execute_many("insert into albums values (?, ?, ?)", [[1, 1, "Pastel Blues"], [2, 2, "Autobahn"], [3, 2, "Computer World"], [4, 3, "Homogenic"], [5, 2, "Radio-Activity"]])
end
```

```text expect
Kraftwerk 3
```

> Hint: join albums to artists, `group by ar.name`, `count(*) as albums`,
> order by it, `limit 1`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

main:
    let conn = db.open(":memory:")
    conn.execute("create table artists (id integer primary key, name text)")
    conn.execute("create table albums (id integer primary key, artist_id int, title text)")
    conn.execute_many("insert into artists values (?, ?)", [[1, "Nina Simone"], [2, "Kraftwerk"], [3, "Björk"]])
    conn.execute_many("insert into albums values (?, ?, ?)", [[1, 1, "Pastel Blues"], [2, 2, "Autobahn"], [3, 2, "Computer World"], [4, 3, "Homogenic"], [5, 2, "Radio-Activity"]])
    let top = conn.query_one("select ar.name, count(*) as albums from albums al join artists ar on al.artist_id = ar.id group by ar.name order by albums desc limit 1")
    print(top["name"], top["albums"])
end
```

</details>

## Today you learned

- `join ... on` combines tables in SQL; `left join` keeps unmatched rows.
- `group by` with `sum`, `count`, `avg`, `min`, `max`; `having` filters
  groups, `where` filters rows.
- Subqueries use one query's result inside another (`in (select ...)`,
  `> (select avg(...))`).
- Views name a query so it can be used like a table.
