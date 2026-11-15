# Day 59: Capstone: a data dashboard, part 2

Yesterday's functions answer the questions; today they go on a web page.
The dashboard shows the key numbers as large tiles, a chart of revenue by
month and a table of top products, with a form to choose the date range
and region. Everything is drawn from the database on each request, so the
page is always up to date. You'll use routes and query parameters
(Day 48), templates (Day 44), charts as SVG (Day 56) and tests (Day 47).

## 59.1 Filters as query parameters

A dashboard's filters belong in the address, `/?start=2025-10-01&region=North`,
so a filtered view can be bookmarked and shared. Each filter is a route
parameter with a default, and is checked before it reaches SQL. The
region filter becomes part of the `where` clause, always through `?`:

```vyrn
use vyrn.time

struct BadRequest:
    message: str
end

const REGIONS = ["North", "South", "East", "West"]

create check_filters(start: str, end_day: str, region: str) -> (str, str, str):
    each d in [start, end_day]:
        attempt:
            time.parse(d, "%Y-%m-%d")
        catch ValueError:
            fail BadRequest("dates are written yyyy-mm-dd, not {d}")
        end
    end
    if start > end_day: fail BadRequest("the start is after the end")
    if region != "all" and region not in REGIONS: fail BadRequest("no region called {region}")
    return (start, end_day, region)
end

create region_sql(region: str) -> (str, list):
    if region == "all": return ("", [])
    return (" and region = ?", [region])
end

main:
    print(check_filters("2025-01-01", "2025-03-31", "North"))
    print(region_sql("all"), region_sql("East"))
    each start, end_day, region in [("2025-13-01", "2025-12-31", "all"), ("2025-06-01", "2025-01-01", "all"), ("2025-01-01", "2025-12-31", "Mars")]:
        attempt:
            check_filters(start, end_day, region)
        catch BadRequest as e:
            print(e.message)
        end
    end
end
```

```text
("2025-01-01", "2025-03-31", "North")
("", []) (" and region = ?", ["East"])
dates are written yyyy-mm-dd, not 2025-13-01
the start is after the end
no region called Mars
```

`region_sql` returns a piece of SQL *and* its values: the SQL text only
ever contains `?`, never the region itself, so the filter can't be used
for SQL injection (Day 51).

## 59.2 The page

The template has three parts: tiles of key numbers, the chart, and the
table, with the filter form at the top. Values are formatted before they
reach the template:

```vyrn
use vyrn.template

const PAGE = r'<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>Dashboard</title><style>.tiles{display:flex;gap:16px}.tile{padding:12px;border:1px solid #ddd;border-radius:8px}.big{font-size:2rem}</style></head><body><h1>Shop dashboard</h1><form><input type="date" name="start" value="{{start}}"><input type="date" name="end_day" value="{{end_day}}"><select name="region">{{#each regions}}<option {{#if selected}}selected{{/if}}>{{name}}</option>{{/each}}</select><button>Show</button></form><div class="tiles">{{#each tiles}}<div class="tile"><div>{{label}}</div><div class="big">{{value}}</div></div>{{/each}}</div>{{{chart}}}<table><tr><th>Product</th><th>Units</th><th>Revenue</th></tr>{{#each products}}<tr><td>{{product}}</td><td>{{units}}</td><td>{{revenue}}</td></tr>{{else}}<tr><td colspan="3">No sales in this range</td></tr>{{/each}}</table></body></html>'

main:
    let html_text = template.render(PAGE, {
        "start": "2025-01-01", "end_day": "2025-12-31",
        "regions": [{"name": r, "selected": r == "North"} each r in ["all", "North", "South"]],
        "tiles": [{"label": "Revenue", "value": "£1,234.00"}, {"label": "Orders", "value": "56"}],
        "chart": "<svg></svg>",
        "products": [],
    })
    print(len(html_text) > 500, "No sales in this range" in html_text, "<option selected>North</option>" in html_text)
end
```

```text
true true true
```

## 59.3 Project: the dashboard

The whole program: the shop data from yesterday, the questions with the
region filter added, the route, and tests that drive it through
`http.test` and read the page with `html.parse`:

```vyrn
use vyrn.chart
use vyrn.db
use vyrn.html
use vyrn.http
use vyrn.random
use vyrn.template
use vyrn.time

const PRODUCTS = [("Tea", 4.5, 5), ("Coffee", 6.0, 4), ("Mug", 9.0, 2), ("Teapot", 24.0, 1)]
const REGIONS = ["North", "South", "East", "West"]
const PAGE = r'<!doctype html><html><head><title>Dashboard</title></head><body><h1>Shop dashboard</h1><form><input type="date" name="start" value="{{start}}"><input type="date" name="end_day" value="{{end_day}}"><select name="region">{{#each regions}}<option {{#if selected}}selected{{/if}}>{{name}}</option>{{/each}}</select><button>Show</button></form><div class="tiles">{{#each tiles}}<div class="tile"><div class="label">{{label}}</div><div class="big">{{value}}</div></div>{{/each}}</div>{{{chart}}}<table>{{#each products}}<tr><td>{{product}}</td><td>{{units}}</td><td>{{revenue}}</td></tr>{{else}}<tr><td>No sales in this range</td></tr>{{/each}}</table></body></html>'

struct BadRequest:
    message: str
end

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

create check_filters(start: str, end_day: str, region: str):
    each d in [start, end_day]:
        attempt:
            time.parse(d, "%Y-%m-%d")
        catch ValueError:
            fail BadRequest("dates are written yyyy-mm-dd, not {d}")
        end
    end
    if start > end_day: fail BadRequest("the start is after the end")
    if region != "all" and region not in REGIONS: fail BadRequest("no region called {region}")
end

## Runs `sql` (with `where {range}` in it) for the date range and region.
create ask(conn: any, sql: str, start: str, end_day: str, region: str) -> list:
    var where = "day between ? and ?"
    var values = [start, end_day]
    if region != "all":
        where += " and region = ?"
        values.append(region)
    end
    return conn.query(sql.replace("\{range}", where), values)
end

route GET "/" (conn, start: str = "2025-01-01", end_day: str = "2025-12-31", region: str = "all"):
    check_filters(start, end_day, region)
    let t = ask(conn, "select count(*) as orders, coalesce(sum(qty * price), 0) as revenue from orders where \{range}", start, end_day, region)[0]
    let months = ask(conn, "select substr(day, 1, 7) as month, sum(qty * price) as revenue from orders where \{range} group by month order by month", start, end_day, region)
    let products = ask(conn, "select product, sum(qty) as units, sum(qty * price) as revenue from orders where \{range} group by product order by revenue desc, product limit 4", start, end_day, region)
    let average = t["revenue"] / t["orders"] if t["orders"] > 0 else 0
    let chart_svg = chart.bar([m["month"][5:] each m in months], [m["revenue"] each m in months], title = "Revenue by month", y_label = "£").svg() if months else ""
    return template.render(PAGE, {
        "start": start, "end_day": end_day,
        "regions": [{"name": r, "selected": r == region} each r in ["all"] + REGIONS],
        "tiles": [{"label": "Revenue", "value": "£{t["revenue"]:,.2f}"}, {"label": "Orders", "value": "{t["orders"]:,}"}, {"label": "Average order", "value": "£{average:.2f}"}],
        "chart": chart_svg,
        "products": [p | {"revenue": "£{p["revenue"]:,.2f}"} each p in products],
    })
end

create tiles(page: any) -> map:
    let labels = [d.text each d in page.select(".tile .label")]
    let values = [d.text each d in page.select(".tile .big")]
    return dict(zip(labels, values))
end

create test_the_year_at_a_glance():
    let page = html.parse(http.test("GET", "/").text)
    let t = tiles(page)
    assert(t["Orders"] == "2,001")
    assert(len(page.select("svg")) == 1)
    assert(page.select_one("td").text == "Tea")
end

create test_filters_change_the_numbers():
    let all = tiles(html.parse(http.test("GET", "/").text))
    let north = tiles(html.parse(http.test("GET", "/?region=North").text))
    assert(all["Revenue"] != north["Revenue"])
    let page = html.parse(http.test("GET", "/?region=North").text)
    assert(page.select_one("option[selected]").text == "North")
end

create test_an_empty_range_still_works():
    let page = html.parse(http.test("GET", "/?start=2030-01-01&end_day=2030-12-31").text)
    assert(tiles(page)["Orders"] == "0")
    assert(page.select_one("td").text == "No sales in this range")
    assert(len(page.select("svg")) == 0)
end

create test_bad_filters_are_refused():
    assert(http.test("GET", "/?region=Mars").status == 400)
    assert(http.test("GET", "/?start=2025-12-01&end_day=2025-01-01").status == 400)
    assert(http.test("GET", "/?region=North%27%20or%201=1--").status == 400)
end

main:
    http.provide("conn", make_shop(42))
    each test in [test_the_year_at_a_glance, test_filters_change_the_numbers, test_an_empty_range_still_works, test_bad_filters_are_refused]:
        test()
        print("ok", test.name)
    end
    let page = html.parse(http.test("GET", "/?start=2025-12-01&end_day=2025-12-31&region=East").text)
    print(tiles(page))
end
```

```text
ok test_the_year_at_a_glance
ok test_filters_change_the_numbers
ok test_an_empty_range_still_works
ok test_bad_filters_are_refused
{"Revenue": "£1,119.00", "Orders": "77", "Average order": "£14.53"}
```

The last test tries an SQL injection through the region filter; it never
reaches SQL, because only the four real regions are allowed, and even if
it did, `?` would make it harmless. `ask` builds the `where` part of each
query in one place, so the date range and region filter can't be applied
to one number on the page and forgotten for another.

To use it, replace `make_shop(42)` with the real database and serve it
(Day 52):

```vyrn check
use vyrn.db
use vyrn.http

main:
    http.provide("conn", db.open("shop.db"))
    http.serve(port = 8000)
end
```

### Exercise

Add a tile for the best day (the day with the most revenue) in the range.
Write `best_day(rows)`, which takes the result of a "revenue per day"
query and gives `"2025-12-20 (£96.00)"`, or `"none"` when there are no
rows.

```vyrn starter
create best_day(rows: list) -> str:
    return "none"
end

main:
    print(best_day([{"day": "2025-12-19", "revenue": 81.0}, {"day": "2025-12-20", "revenue": 96.0}]))
    print(best_day([]))
end
```

```text expect
2025-12-20 (£96.00)
none
```

> Hint: `max(rows, key = r => r["revenue"])` when `rows` isn't empty.

<details><summary>Solution</summary>

```vyrn solution
create best_day(rows: list) -> str:
    if not rows: return "none"
    let best = max(rows, key = r => r["revenue"])
    return "{best["day"]} (£{best["revenue"]:.2f})"
end

main:
    print(best_day([{"day": "2025-12-19", "revenue": 81.0}, {"day": "2025-12-20", "revenue": 96.0}]))
    print(best_day([]))
end
```

</details>

## Today you built

A live, filterable dashboard: numbers, a chart and a table from one
database, filters in the address checked before use, one place that
applies them, and tests that read the real page. That's the shape of
most internal tools in most companies. Ideas to take it further:

- Compare with the previous period: "£1,018 (+12% on November)".
- A download link giving the filtered rows as CSV (`t.to_csv()`).
- Refresh the numbers live with server-sent events (`stream` routes).
