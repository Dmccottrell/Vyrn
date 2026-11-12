# Day 40: Capstone: deal finder, part 2

Yesterday the deal finder learned to keep a wishlist, read prices and
remember them. Today it becomes a tool you actually use: you type commands
like `vyrn run deals.vyrn add celeste 8`, it sends one email when there are
deals (and doesn't nag you about the same deal every day), and it runs by
itself every morning.

## 40.1 Commands from the terminal

`os.args()` gives the words typed after the file name. The program looks at
the first word to decide what to do. Keeping that in a function that takes
the words as a list means it can be tried without a terminal:

```vyrn
create run(args: list) -> int:
    if not args:
        print("usage: deals add <title> <price> | remove <title> | list | check")
        return 2
    end
    let command = args[0]
    if command == "add" and len(args) >= 3:
        let title = " ".join(args[1:-1])
        attempt:
            let price = float(args[-1])
            print("added {title} at {price:.2f}")
        catch ValueError:
            print("the price must be a number, like 7.50")
            return 2
        end
    else if command == "list":
        print("(the wishlist)")
    else:
        print("unknown command: {command}")
        return 2
    end
    return 0
end

main:
    print(run(["add", "portal", "2", "1.50"]))
    print(run(["add", "celeste", "cheap"]))
    print(run(["fly"]))
end
```

```text
added portal 2 at 1.50
0
the price must be a number, like 7.50
2
unknown command: fly
2
```

`args[1:-1]` is every word between the command and the price, so titles
with spaces work without quotes. The number `run` gives back is the
program's **exit code**: `0` means it worked, anything else that it didn't.
Scripts and schedulers read it; `os.exit(code)` hands it over:

```vyrn check
use vyrn.os

create run(args: list) -> int:
    print(args)
    return 0
end

main:
    os.exit(run(os.args()))
end
```

### Exercise

Add a `remove` command: `remove <title>` prints `removed <title>`, and
`remove` on its own prints `which game?` and gives exit code 2.

```vyrn starter
create run(args: list) -> int:
    let command = args[0]
    print("unknown command: {command}")
    return 2
end

main:
    print(run(["remove", "half-life", "3"]))
    print(run(["remove"]))
end
```

```text expect
removed half-life 3
0
which game?
2
```

> Hint: `if command == "remove":` then check `len(args) < 2`.
> Hint: the title is `" ".join(args[1:])`.

<details><summary>Solution</summary>

```vyrn solution
create run(args: list) -> int:
    let command = args[0]
    if command == "remove":
        if len(args) < 2:
            print("which game?")
            return 2
        end
        print("removed", " ".join(args[1:]))
        return 0
    end
    print("unknown command: {command}")
    return 2
end

main:
    print(run(["remove", "half-life", "3"]))
    print(run(["remove"]))
end
```

</details>

## 40.2 Alerts that don't nag

If a game stays at your price for a week, one email is enough. The program
remembers what it has told you in an `alerted` table, and only alerts again
when the price drops *further*:

```vyrn
use vyrn.db

create worth_telling(conn: any, title: str, price: float) -> bool:
    let told = conn.value("select min(price) from alerted where title = ?", [title])
    return told == none or price < told
end

create told(conn: any, title: str, price: float, day: str):
    conn.execute("insert into alerted values (?, ?, ?)", [title, price, day])
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table alerted (title text, price real, day text)")
    each day, price in [("11-10", 4.99), ("11-11", 4.99), ("11-12", 3.99), ("11-13", 4.99)]:
        if worth_telling(conn, "celeste", price):
            told(conn, "celeste", price, day)
            print(day, "email: celeste is", price)
        else:
            print(day, "(already told)")
        end
    end
end
```

```text
11-10 email: celeste is 4.99
11-11 (already told)
11-12 email: celeste is 3.99
11-13 (already told)
```

## 40.3 One tidy email

All the hits go in one email, sorted by how much you save, so the best deal
is on top:

```vyrn
create email_body(hits: list) -> str:
    var lines = ["Deals on your wishlist today:", ""]
    each hit in sorted(hits, key = h => h["target"] - h["price"], reverse = true):
        let saving = hit["target"] - hit["price"]
        lines.append("- {hit["title"]}: {hit["price"]:.2f} at {hit["store"]} ({saving:.2f} under your price)")
    end
    lines.append("")
    lines.append("Happy gaming!")
    return "\n".join(lines)
end

main:
    print(email_body([
        {"title": "Portal 2", "price": 1.49, "store": "Steam", "target": 1.5},
        {"title": "Celeste", "price": 3.99, "store": "GOG", "target": 8.0},
    ]))
end
```

```text
Deals on your wishlist today:

- Celeste: 3.99 at GOG (4.01 under your price)
- Portal 2: 1.49 at Steam (0.01 under your price)

Happy gaming!
```

### Exercise

The subject line should say how many deals there are: `1 deal on your
wishlist`, or `3 deals on your wishlist`. Write `subject(hits)`.

```vyrn starter
create subject(hits: list) -> str:
    return ""
end

main:
    print(subject([1]))
    print(subject([1, 2, 3]))
end
```

```text expect
1 deal on your wishlist
3 deals on your wishlist
```

> Hint: `"deal" if len(hits) == 1 else "deals"`

<details><summary>Solution</summary>

```vyrn solution
create subject(hits: list) -> str:
    let word = "deal" if len(hits) == 1 else "deals"
    return "{len(hits)} {word} on your wishlist"
end

main:
    print(subject([1]))
    print(subject([1, 2, 3]))
end
```

</details>

## 40.4 The whole program

Here is the deal finder in one file. Every outside thing (the API, the
email, today's date) comes in through a `World` object, so the program can
be run against fakes, as below, or against the real world, as in the live
version after it:

```vyrn
use vyrn.db

object World:
    fetch: any
    send: any
    today: str
end

create best_deal(deals: list, title: str) -> map?:
    let exact = [d each d in deals if d["title"].lower() == title]
    if not exact: return none
    let best = min(exact, key = d => float(d["salePrice"]))
    return {"title": best["title"], "price": float(best["salePrice"]), "store": "store " + best["storeID"]}
end

create setup(conn: any):
    conn.execute("create table if not exists wishes (title text, target real)")
    conn.execute("create table if not exists prices (day text, title text, price real, store text)")
    conn.execute("create table if not exists alerted (title text, price real, day text)")
end

create check(conn: any, world: World) -> int:
    var hits = []
    each wish in conn.query("select title, target from wishes order by title"):
        let deal = best_deal(world.fetch(wish["title"]), wish["title"])
        if deal == none: skip
        conn.execute("insert into prices values (?, ?, ?, ?)", [world.today, wish["title"], deal["price"], deal["store"]])
        let told = conn.value("select min(price) from alerted where title = ?", [wish["title"]])
        if deal["price"] <= wish["target"] and (told == none or deal["price"] < told):
            conn.execute("insert into alerted values (?, ?, ?)", [wish["title"], deal["price"], world.today])
            hits.append("{deal["title"]}: {deal["price"]:.2f} at {deal["store"]}")
        end
    end
    let word = "deal" if len(hits) == 1 else "deals"
    if hits: world.send("{len(hits)} {word} on your wishlist", "\n".join(hits))
    print("checked; {len(hits)} new {word}")
    return 0
end

create run(args: list, conn: any, world: World) -> int:
    setup(conn)
    if not args:
        print("usage: deals add <title> <price> | remove <title> | list | check")
        return 2
    end
    let command = args[0]
    if command == "add" and len(args) >= 3:
        let title = " ".join(args[1:-1]).lower()
        conn.delete("wishes", {"title": title})
        conn.insert("wishes", {"title": title, "target": float(args[-1])})
        print("watching {title} for {float(args[-1]):.2f} or less")
    else if command == "remove" and len(args) >= 2:
        let gone = conn.delete("wishes", {"title": " ".join(args[1:]).lower()})
        print("removed" if gone else "not on the list")
    else if command == "list":
        each w in conn.query("select title, target from wishes order by title"):
            print("{w["title"]:<20} {w["target"]:>6.2f}")
        end
    else if command == "check":
        return check(conn, world)
    else:
        print("unknown command: {command}")
        return 2
    end
    return 0
end

main:
    let conn = db.open(":memory:")
    let mailbox = fake(returns = true)
    var prices = {"celeste": "4.99", "portal 2": "1.99"}
    let fetch = title => [{"title": title.title(), "salePrice": prices[title], "storeID": "1"}]
    var world = World(fetch, mailbox, "2026-11-12")
    run(["add", "Celeste", "8"], conn, world)
    run(["add", "Portal", "2", "1.50"], conn, world)
    run(["list"], conn, world)
    run(["check"], conn, world)
    world.today = "2026-11-13"
    prices["portal 2"] = "0.99"
    run(["check"], conn, world)
    each call in mailbox.calls:
        print("email:", call[0], "|", call[1].replace("\n", "; "))
    end
end
```

```text
watching celeste for 8.00 or less
watching portal 2 for 1.50 or less
celeste                8.00
portal 2               1.50
checked; 1 new deal
checked; 1 new deal
email: 1 deal on your wishlist | Celeste: 4.99 at store 1
email: 1 deal on your wishlist | Portal 2: 0.99 at store 1
```

On the second day Celeste is still 4.99, but you were already told, so only
Portal 2's new low price is in the email.

To make it real, replace `main` with the live version, which opens a file
database and fills `World` with the API (Day 39), `mail.send` (Day 32) and
today's date:

```vyrn check
use vyrn.db
use vyrn.http
use vyrn.mail
use vyrn.os
use vyrn.time

object World:
    fetch: any
    send: any
    today: str
end

create fetch(title: str) -> list:
    return http.get("https://www.cheapshark.com/api/1.0/deals?" + http.query({"title": title})).json
end

create send(subject: str, body: str):
    mail.send(to = os.env("DEALS_TO"), subject = subject, body = body, sender = os.env("MAIL_USER"),
              server = os.env("MAIL_SERVER"), user = os.env("MAIL_USER"), password = os.env("MAIL_PASSWORD"))
end

create run(args: list, conn: any, world: World) -> int:
    print(args, world.today)
    return 0
end

main:
    os.load_env()
    let world = World(fetch, send, time.format(time.now(), "%Y-%m-%d"))
    os.exit(run(os.args(), db.open("deals.db"), world))
end
```

## 40.5 Running every day

A program that should run on its own needs a **scheduler**:

- **Windows**: open Task Scheduler, *Create Basic Task*, *Daily*, and as
  the action *Start a program*: `vyrn` with the arguments
  `run C:\path\to\deals.vyrn check`, starting in the program's folder.
- **macOS and Linux**: run `crontab -e` and add a line; this one runs at
  8:15 every morning:

```text
15 8 * * * cd /home/ada/deals && vyrn run deals.vyrn check >> deals.log 2>&1
```

`>> deals.log` keeps what it printed, so you can see later what happened.
Because the program records every price and every alert in its database,
it can be stopped, moved or run twice without sending duplicate emails.

## Today you built

A complete, useful tool: commands with exit codes, a database that remembers
prices and alerts, one tidy email, every outside thing passed in so the
whole program can be tested, and a scheduler to run it. Ideas to take it
further:

- A `history <title>` command that prints the price history as `#` bars.
- Send the email only on weekdays, or as HTML (`mail.send(html = ...)`).
- Watch something else: train tickets, a book, or a share price (Day 36).
