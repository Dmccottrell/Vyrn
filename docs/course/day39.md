# Day 39: Capstone: deal finder, part 1

The next two days are a **capstone**: one bigger program that uses
everything from this unit. The deal finder keeps a wishlist of games and
the price you'd be happy to pay for each. Every day it asks a deals API for
current prices, records them in a database, and emails you when something
on your list drops to your price. Today: the plan, the data, and the
prices. Tomorrow: alerts, commands, and running every day.

## 39.1 The plan

Before writing code, write down what the program does and which pieces do
it. Small pieces that each do one thing are easier to write, test and fix:

| Piece | Job | From |
|---|---|---|
| `Wishlist` | games and target prices, in a database | Day 38 |
| `fetch_deals(title)` | current prices from the API | Days 33, 36 |
| `record(prices)` | keep a price history | Day 38 |
| `matches(...)` | which deals are at or under the target | Day 26 |
| `alert(...)` | one email listing the matches | Days 32, 35 |
| `main` | commands: `add`, `list`, `check` | Day 40 |

The API is CheapShark, which compares prices across game shops. It's free
and needs no key:

```text
https://www.cheapshark.com/api/1.0/deals?title=portal&upperPrice=15
```

## 39.2 The wishlist

The wishlist is a table with a title and a target price. An object wraps
it, so nothing else in the program needs to know there is a database:

```vyrn
use vyrn.db

object Wishlist:
    conn: any

    create add(self, title: str, target: float):
        let existing = self.conn.find_one("wishes", {"title": title.lower()})
        if existing:
            self.conn.update("wishes", {"title": title.lower()}, {"target": target})
        else:
            self.conn.insert("wishes", {"title": title.lower(), "target": target})
        end
    end

    create remove(self, title: str) -> bool:
        return self.conn.delete("wishes", {"title": title.lower()}) > 0
    end

    create items(self) -> list:
        return self.conn.query("select title, target from wishes order by title")
    end
end

create open_wishlist(conn: any) -> Wishlist:
    if "wishes" not in conn.tables():
        conn.create_table("wishes", {"title": "text", "target": "real"})
    end
    return Wishlist(conn)
end

main:
    let wishes = open_wishlist(db.open(":memory:"))
    wishes.add("Portal 2", 5)
    wishes.add("Celeste", 8)
    wishes.add("portal 2", 4)
    print(wishes.remove("Doom"), wishes.items())
end
```

```text
false [{"title": "celeste", "target": 8.0}, {"title": "portal 2", "target": 4.0}]
```

Adding a game that's already there changes its target instead of adding it
twice. Titles are kept in lower case, so `Portal 2` and `portal 2` are the
same game.

### Exercise

Add a method `target(self, title)` that gives the target price for a
title, or `none` if it isn't on the list.

```vyrn starter
use vyrn.db

object Wishlist:
    conn: any

    create target(self, title: str) -> float?:
        return none
    end
end

main:
    let conn = db.open(":memory:")
    conn.create_table("wishes", {"title": "text", "target": "real"})
    conn.insert("wishes", {"title": "celeste", "target": 8.0})
    let w = Wishlist(conn)
    print(w.target("Celeste"), w.target("Doom"))
end
```

```text expect
8.0 none
```

> Hint: `row = self.conn.find_one("wishes", {"title": title.lower()})`, then
> `row["target"]` if there is a row.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

object Wishlist:
    conn: any

    create target(self, title: str) -> float?:
        let row = self.conn.find_one("wishes", {"title": title.lower()})
        if row == none: return none
        return row["target"]
    end
end

main:
    let conn = db.open(":memory:")
    conn.create_table("wishes", {"title": "text", "target": "real"})
    conn.insert("wishes", {"title": "celeste", "target": 8.0})
    let w = Wishlist(conn)
    print(w.target("Celeste"), w.target("Doom"))
end
```

</details>

## 39.3 Reading the deals

CheapShark answers with a list of deals, one per shop that sells the game.
Prices are text, and searching for "portal" also finds "Portal 2" and
"Portal Stories", so the program keeps only exact title matches and the
cheapest shop:

```vyrn
use vyrn.json

const DEALS = r'[{"title": "Portal 2", "storeID": "1", "salePrice": "1.99", "normalPrice": "9.99", "savings": "80.08"}, {"title": "Portal 2", "storeID": "7", "salePrice": "2.49", "normalPrice": "9.99", "savings": "75.08"}, {"title": "Portal Stories: Mel", "storeID": "1", "salePrice": "0.00", "normalPrice": "0.00", "savings": "0"}]'

const STORES = {"1": "Steam", "7": "GOG", "11": "Humble"}

create best_deal(deals: list, title: str) -> map?:
    let exact = [d each d in deals if d["title"].lower() == title.lower()]
    if not exact: return none
    let best = min(exact, key = d => float(d["salePrice"]))
    return {
        "title": best["title"],
        "price": float(best["salePrice"]),
        "normal": float(best["normalPrice"]),
        "store": STORES.get(best["storeID"], "store " + best["storeID"]),
    }
end

main:
    let deals = json.parse(DEALS)
    print(best_deal(deals, "portal 2"))
    print(best_deal(deals, "half-life 3"))
end
```

```text
{"title": "Portal 2", "price": 1.99, "normal": 9.99, "store": "Steam"}
none
```

Turning the API's answer into a small map of exactly what the program needs
is worth doing right at the edge: everything after it works with clean
numbers and names, and if the API changes, only `best_deal` changes.

The live fetch, cached for an hour (Day 36) so testing doesn't hammer the
API:

```vyrn check
use vyrn.fs
use vyrn.http
use vyrn.json
use vyrn.time

create cached(path: str, max_age: float, fetch: any) -> any:
    if fs.exists(path) and time.now() - fs.modified(path) < max_age:
        return json.load(path)
    end
    let answer = fetch()
    json.save(path, answer)
    return answer
end

create fetch_deals(title: str) -> list:
    let url = "https://www.cheapshark.com/api/1.0/deals?" + http.query({"title": title, "pageSize": 20})
    let slug = title.lower().replace(" ", "-")
    return cached("deals-{slug}.json", 3600, () => http.get(url).json)
end

main:
    each d in fetch_deals("portal 2")[:3]:
        print(d["title"], d["salePrice"])
    end
end
```

## 39.4 Price history

Each check stores today's best price, so the program can tell a real drop
from the usual price, and show how a price has changed:

```vyrn
use vyrn.db

create record(conn: any, day: str, deal: map):
    conn.execute("insert into prices (day, title, price, store) values (?, ?, ?, ?)", [day, deal["title"].lower(), deal["price"], deal["store"]])
end

create lowest(conn: any, title: str) -> float?:
    return conn.value("select min(price) from prices where title = ?", [title.lower()])
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table prices (day text, title text, price real, store text)")
    record(conn, "2026-11-10", {"title": "Celeste", "price": 9.99, "store": "Steam"})
    record(conn, "2026-11-11", {"title": "Celeste", "price": 4.99, "store": "GOG"})
    record(conn, "2026-11-12", {"title": "Celeste", "price": 7.49, "store": "Steam"})
    print("lowest seen:", lowest(conn, "celeste"))
    print("never seen:", lowest(conn, "doom"))
    each row in conn.query("select day, price from prices where title = 'celeste' order by day"):
        print(row["day"], "#" * round(row["price"]))
    end
end
```

```text
lowest seen: 4.99
never seen: none
2026-11-10 ##########
2026-11-11 #####
2026-11-12 #######
```

A quick bar of `#`s is a chart you can print anywhere (Unit 6 draws real
ones).

### Exercise

Write `is_new_low(conn, title, price)`: true when the price is lower than
every price recorded before, or when there is no history yet.

```vyrn starter
use vyrn.db

create is_new_low(conn: any, title: str, price: float) -> bool:
    return false
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table prices (day text, title text, price real, store text)")
    print(is_new_low(conn, "celeste", 7.0))
    conn.execute("insert into prices values ('2026-11-11', 'celeste', 4.99, 'GOG')")
    print(is_new_low(conn, "celeste", 7.0), is_new_low(conn, "celeste", 3.99))
end
```

```text expect
true
false true
```

> Hint: `low = conn.value("select min(price) from prices where title = ?", [title])`
> is `none` with no history.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

create is_new_low(conn: any, title: str, price: float) -> bool:
    let low = conn.value("select min(price) from prices where title = ?", [title.lower()])
    return low == none or price < low
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table prices (day text, title text, price real, store text)")
    print(is_new_low(conn, "celeste", 7.0))
    conn.execute("insert into prices values ('2026-11-11', 'celeste', 4.99, 'GOG')")
    print(is_new_low(conn, "celeste", 7.0), is_new_low(conn, "celeste", 3.99))
end
```

</details>

## 39.5 Putting today's pieces together

One function checks the whole wishlist: fetch, pick the best deal, record
it, and collect the ones at or under target. The fetching is passed in, so
here a map of recorded answers stands in for the API:

```vyrn
use vyrn.db

create best_deal(deals: list, title: str) -> map?:
    let exact = [d each d in deals if d["title"].lower() == title.lower()]
    if not exact: return none
    let best = min(exact, key = d => float(d["salePrice"]))
    return {"title": best["title"], "price": float(best["salePrice"]), "store": "store " + best["storeID"]}
end

create check(conn: any, day: str, fetch: any) -> list:
    var hits = []
    each wish in conn.query("select title, target from wishes"):
        let deal = best_deal(fetch(wish["title"]), wish["title"])
        if deal == none: skip
        conn.execute("insert into prices values (?, ?, ?, ?)", [day, wish["title"], deal["price"], deal["store"]])
        if deal["price"] <= wish["target"]: hits.append(deal | {"target": wish["target"]})
    end
    return hits
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table wishes (title text, target real)")
    conn.execute("create table prices (day text, title text, price real, store text)")
    conn.execute_many("insert into wishes values (?, ?)", [["celeste", 8], ["portal 2", 1.5], ["hades", 10]])
    let answers = {
        "celeste": [{"title": "Celeste", "salePrice": "4.99", "storeID": "7"}],
        "portal 2": [{"title": "Portal 2", "salePrice": "1.99", "storeID": "1"}],
        "hades": [],
    }
    each hit in check(conn, "2026-11-12", title => answers[title]):
        print("{hit["title"]}: {hit["price"]} at {hit["store"]} (wanted {hit["target"]})")
    end
    print(conn.value("select count(*) from prices"), "prices recorded")
end
```

```text
Celeste: 4.99 at store 7 (wanted 8.0)
2 prices recorded
```

Portal 2 is recorded but not a hit (1.99 is over the 1.50 target), and
nobody sells Hades today, so nothing is recorded for it.

## Today you built

The deal finder's data side: a wishlist in a database, deals read from an
API and cleaned at the edge, and a price history. Tomorrow it learns to
send alerts, take commands, and run by itself every day.
