# Day 58: Capstone: a data dashboard, part 1

A **dashboard** is a page that answers the questions people ask every day,
with the newest data, at a glance: this month's sales, the trend, the best
products. The next two days build one for a small online shop. Today: the
data and the analysis, as functions that take a date range and return
numbers and charts. Tomorrow: the web page, with filters, served live.

## 58.1 The data

A real shop's orders would come from its database. For the course, a
function makes a year of believable orders: busier at weekends and in
December, with some products more popular than others. The `random.seed`
makes it the same every run:

```vyrn
use vyrn.db
use vyrn.random
use vyrn.time

const PRODUCTS = [("Tea", 4.5, 5), ("Coffee", 6.0, 4), ("Mug", 9.0, 2), ("Teapot", 24.0, 1)]
const REGIONS = ["North", "South", "East", "West"]

create make_shop(seed: int) -> any:
    random.seed(seed)
    let conn = db.open(":memory:")
    conn.execute("create table orders (id integer primary key, day text, region text, product text, qty int, price real)")
    var pool = []
    each name, price, weight in PRODUCTS:
        each _ in 0 until weight: pool.append((name, price))
    end
    var rows = []
    each d in 0 until 365:
        let moment = time.add(time.make(2025, 1, 1), days = d)
        let parts = time.date(moment)
        var orders = 3 + (2 if parts["weekday"] in ["Saturday", "Sunday"] else 0) + (4 if parts["month"] == 12 else 0)
        each _ in 0 until orders + random.int(0, 3):
            let name, price = random.choice(pool)
            rows.append([time.format(moment, "%Y-%m-%d"), random.choice(REGIONS), name, random.int(1, 3), price])
        end
    end
    conn.execute_many("insert into orders (day, region, product, qty, price) values (?, ?, ?, ?, ?)", rows)
    return conn
end

main:
    let conn = make_shop(42)
    print(conn.value("select count(*) from orders"), "orders")
    print(conn.query_one("select min(day) as first, max(day) as last from orders"))
    print(conn.query("select * from orders limit 2"))
end
```

```text
2001 orders
{"first": "2025-01-01", "last": "2025-12-31"}
[{"id": 1, "day": "2025-01-01", "region": "West", "product": "Tea", "qty": 2, "price": 4.5}, {"id": 2, "day": "2025-01-01", "region": "South", "product": "Tea", "qty": 2, "price": 4.5}]
```

Dates are stored as `yyyy-mm-dd` text, which sorts correctly as text, so
`between` and `order by` work on them directly.

## 58.2 The questions as functions

Each number on the dashboard is one function, taking the connection and a
date range. Keeping the range a parameter is what makes filters possible
tomorrow:

```vyrn
use vyrn.db

create totals(conn: any, start: str, end_day: str) -> map:
    return conn.query_one("select count(*) as orders, coalesce(sum(qty * price), 0) as revenue, coalesce(round(avg(qty * price), 2), 0) as average from orders where day between ? and ?", [start, end_day])
end

create by_month(conn: any, start: str, end_day: str) -> list:
    return conn.query("select substr(day, 1, 7) as month, round(sum(qty * price), 2) as revenue from orders where day between ? and ? group by month order by month", [start, end_day])
end

create top_products(conn: any, start: str, end_day: str, n: int = 3) -> list:
    return conn.query("select product, sum(qty) as units, round(sum(qty * price), 2) as revenue from orders where day between ? and ? group by product order by revenue desc, product limit ?", [start, end_day, n])
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table orders (id integer primary key, day text, region text, product text, qty int, price real)")
    conn.execute_many("insert into orders (day, region, product, qty, price) values (?, ?, ?, ?, ?)", [["2025-01-05", "North", "Tea", 2, 4.5], ["2025-01-20", "South", "Mug", 1, 9.0], ["2025-02-02", "North", "Teapot", 1, 24.0]])
    print(totals(conn, "2025-01-01", "2025-12-31"))
    print(totals(conn, "2030-01-01", "2030-12-31"))
    print(by_month(conn, "2025-01-01", "2025-12-31"))
    print(top_products(conn, "2025-01-01", "2025-01-31"))
end
```

```text
{"orders": 3, "revenue": 42.0, "average": 14.0}
{"orders": 0, "revenue": 0, "average": 0}
[{"month": "2025-01", "revenue": 18.0}, {"month": "2025-02", "revenue": 24.0}]
[{"product": "Mug", "units": 1, "revenue": 9.0}, {"product": "Tea", "units": 2, "revenue": 9.0}]
```

Two details to copy:

- `coalesce(x, 0)` gives 0 instead of `none` when there are no orders in
  the range, so an empty month shows "£0" instead of crashing the page.
- Every "top" query has a tie-breaker (`, product`): Mug and Tea both made
  £9.

### Exercise

Add `by_region(conn, start, end_day)`, giving each region's revenue,
biggest first.

```vyrn starter
use vyrn.db

create by_region(conn: any, start: str, end_day: str) -> list:
    return []
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table orders (day text, region text, qty int, price real)")
    conn.execute_many("insert into orders values (?, ?, ?, ?)", [["2025-01-05", "North", 2, 4.5], ["2025-01-20", "South", 1, 9.0], ["2025-02-02", "North", 1, 24.0], ["2026-01-01", "East", 9, 9.0]])
    print(by_region(conn, "2025-01-01", "2025-12-31"))
end
```

```text expect
[{"region": "North", "revenue": 33.0}, {"region": "South", "revenue": 9.0}]
```

> Hint: like `top_products`, grouped by `region`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

create by_region(conn: any, start: str, end_day: str) -> list:
    return conn.query("select region, round(sum(qty * price), 2) as revenue from orders where day between ? and ? group by region order by revenue desc, region", [start, end_day])
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table orders (day text, region text, qty int, price real)")
    conn.execute_many("insert into orders values (?, ?, ?, ?)", [["2025-01-05", "North", 2, 4.5], ["2025-01-20", "South", 1, 9.0], ["2025-02-02", "North", 1, 24.0], ["2026-01-01", "East", 9, 9.0]])
    print(by_region(conn, "2025-01-01", "2025-12-31"))
end
```

</details>

## 58.3 From numbers to charts

Each chart is drawn from one of the functions, so a chart can never
disagree with the numbers beside it:

```vyrn
use vyrn.chart
use vyrn.db

create by_month(conn: any, start: str, end_day: str) -> list:
    return conn.query("select substr(day, 1, 7) as month, round(sum(qty * price), 2) as revenue from orders where day between ? and ? group by month order by month", [start, end_day])
end

create month_chart(rows: list) -> any:
    let labels = [r["month"][5:] each r in rows]
    return chart.bar(labels, [r["revenue"] each r in rows], title = "Revenue by month", x_label = "month", y_label = "£")
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table orders (id integer primary key, day text, region text, product text, qty int, price real)")
    conn.execute_many("insert into orders (day, region, product, qty, price) values (?, ?, ?, ?, ?)", [["2025-01-05", "North", "Tea", 2, 4.5], ["2025-02-02", "North", "Teapot", 1, 24.0], ["2025-03-09", "East", "Mug", 3, 9.0]])
    let rows = by_month(conn, "2025-01-01", "2025-12-31")
    let svg = month_chart(rows).svg()
    print(svg.startswith("<svg"), [r["month"][5:] each r in rows])
end
```

```text
true ["01", "02", "03"]
```

## 58.4 The whole analysis

Putting the functions together over the generated year gives the report
the dashboard will show, here printed as text:

```vyrn
use vyrn.db
use vyrn.random
use vyrn.time

const PRODUCTS = [("Tea", 4.5, 5), ("Coffee", 6.0, 4), ("Mug", 9.0, 2), ("Teapot", 24.0, 1)]
const REGIONS = ["North", "South", "East", "West"]

create make_shop(seed: int) -> any:
    random.seed(seed)
    let conn = db.open(":memory:")
    conn.execute("create table orders (id integer primary key, day text, region text, product text, qty int, price real)")
    var pool = []
    each name, price, weight in PRODUCTS:
        each _ in 0 until weight: pool.append((name, price))
    end
    var rows = []
    each d in 0 until 365:
        let moment = time.add(time.make(2025, 1, 1), days = d)
        let parts = time.date(moment)
        var orders = 3 + (2 if parts["weekday"] in ["Saturday", "Sunday"] else 0) + (4 if parts["month"] == 12 else 0)
        each _ in 0 until orders + random.int(0, 3):
            let name, price = random.choice(pool)
            rows.append([time.format(moment, "%Y-%m-%d"), random.choice(REGIONS), name, random.int(1, 3), price])
        end
    end
    conn.execute_many("insert into orders (day, region, product, qty, price) values (?, ?, ?, ?, ?)", rows)
    return conn
end

create totals(conn: any, start: str, end_day: str) -> map:
    return conn.query_one("select count(*) as orders, coalesce(sum(qty * price), 0) as revenue, coalesce(round(avg(qty * price), 2), 0) as average from orders where day between ? and ?", [start, end_day])
end

create by_month(conn: any, start: str, end_day: str) -> list:
    return conn.query("select substr(day, 1, 7) as month, round(sum(qty * price), 2) as revenue from orders where day between ? and ? group by month order by month", [start, end_day])
end

create top_products(conn: any, start: str, end_day: str, n: int = 3) -> list:
    return conn.query("select product, sum(qty) as units, round(sum(qty * price), 2) as revenue from orders where day between ? and ? group by product order by revenue desc, product limit ?", [start, end_day, n])
end

main:
    let conn = make_shop(42)
    let t = totals(conn, "2025-01-01", "2025-12-31")
    print("2025: {t["orders"]:,} orders, £{t["revenue"]:,.2f}, £{t["average"]:.2f} per order")
    let months = by_month(conn, "2025-01-01", "2025-12-31")
    let best = max(months, key = m => m["revenue"])
    let worst = min(months, key = m => m["revenue"])
    print("best month {best["month"]} (£{best["revenue"]:,.2f}), quietest {worst["month"]} (£{worst["revenue"]:,.2f})")
    each p in top_products(conn, "2025-01-01", "2025-12-31"):
        print("  {p["product"]:<7} {p["units"]:>5} units  £{p["revenue"]:>9,.2f}")
    end
    let q4 = totals(conn, "2025-10-01", "2025-12-31")
    print("last quarter: £{q4["revenue"]:,.2f} ({100 * q4["revenue"] / t["revenue"]:.0f}% of the year)")
end
```

```text
2025: 2,001 orders, £29,023.50, £14.50 per order
best month 2025-12 (£4,042.50), quietest 2025-05 (£2,023.50)
  Tea      1787 units  £ 8,041.50
  Coffee   1318 units  £ 7,908.00
  Teapot    307 units  £ 7,368.00
last quarter: £8,437.50 (29% of the year)
```

December is the best month, as the data was made to be. And the teapot,
sold in a sixth as many units as tea, brings in nearly as much money: the
sort of thing a dashboard exists to make obvious.

## Today you built

The analysis behind the dashboard: data in a database, one function per
question, each taking a date range, with safe answers for empty ranges and
tie-breakers for rankings, and charts drawn from the same functions.
Tomorrow it goes on the web.
