# Day 47: Testing

Every program so far, you checked by running it and reading what it
printed. That works once. But programs change: you fix one thing and break
another without noticing. **Automated tests** are small programs that
check your program, and run in a second every time you change anything.
Professional code has as much test code as program code, often more.
Today you write tests, learn what makes them good, test code that uses
the network or the clock with fakes, and test a web app's routes. The
project puts the deal finder from Day 40 under test.

## 47.1 A first test

A test is a function whose name starts with `test_`, that uses `assert`
to check something is true. `assert` does nothing when it's true, and
fails, stopping the test, when it's false:

```vyrn
create price_with_vat(price: float) -> float:
    return round(price * 1.2, 2)
end

create test_vat_is_added():
    assert(price_with_vat(10) == 12.0)
end

create test_vat_rounds_to_pennies():
    assert(price_with_vat(0.99) == 1.19)
end

main:
    test_vat_is_added()
    test_vat_rounds_to_pennies()
    print("both passed")
end
```

```text
both passed
```

When a comparison in an `assert` is false, the error says what both sides
were, so you can see what went wrong without adding `print`s:

```vyrn
create price_with_vat(price: float) -> float:
    return round(price * 1.2, 2)
end

main:
    assert(price_with_vat(2.50) == 3.10, "VAT on 2.50")
end
```

```text
error: AssertionError: VAT on 2.50 (3.0 == 3.1 is false)
```

Here the *test* was wrong (20% of 2.50 is 0.50, not 0.60), which happens
too: a failing test means "these two disagree", and you decide which is
right.

## 47.2 `vyrn test`

In a project, tests live in the `tests` folder, in files of their own, and
import what they test. `vyrn new shop` makes a project with one already:

```text
shop/
  vyrn.toml
  main.vyrn
  prices.vyrn             the code
  tests/
    test_prices.vyrn      the tests
```

`tests/test_prices.vyrn`:

```text
use prices (price_with_vat)

create test_vat_is_added():
    assert(price_with_vat(10) == 12.0)
end

create test_vat_rounds_to_pennies():
    assert(price_with_vat(0.99) == 1.19)
end
```

`vyrn test` finds every `test_` function in `tests/`, runs each one in a
fresh copy of the program (so tests can't affect each other), and reports.
If `price_with_vat` had a bug and rounded down, it would say:

```text
ok      tests/test_prices.vyrn::test_vat_is_added
FAILED  tests/test_prices.vyrn::test_vat_rounds_to_pennies
        AssertionError: 1.18 == 1.19 is false
  in test_vat_rounds_to_pennies at line 8

1 passed, 1 failed
```

It exits with code 1 when anything fails, so scripts and CI services
(Day 52) can refuse to publish broken code.

## 47.3 What to test

Good tests check **behaviour**: what goes in and what comes out, not how
the function does it. Then you can rewrite the inside freely, and the
tests tell you whether it still works. For each function, test:

- a **typical** case;
- the **edges**: empty lists, zero, the first and last item, the largest
  allowed value;
- the **mistakes**: bad input should fail in the way you promised.

`assert_fails` checks that something fails, optionally with a particular
kind of error, and gives you the error to look at:

```vyrn
struct TooYoung:
    age: int
end

create ticket_price(age: int) -> float:
    if age < 0: fail ValueError("age can't be negative")
    if age < 3: fail TooYoung(age)
    if age < 16: return 5.0
    if age >= 65: return 7.0
    return 10.0
end

create test_prices_by_age():
    assert(ticket_price(10) == 5.0)
    assert(ticket_price(30) == 10.0)
    assert(ticket_price(70) == 7.0)
end

create test_edges():
    assert(ticket_price(3) == 5.0)
    assert(ticket_price(15) == 5.0)
    assert(ticket_price(16) == 10.0)
    assert(ticket_price(65) == 7.0)
end

create test_mistakes():
    assert_fails(() => ticket_price(-1), "ValueError")
    let e = assert_fails(() => ticket_price(2), "TooYoung")
    assert(e.age == 2)
end

main:
    test_prices_by_age()
    test_edges()
    test_mistakes()
    print("all passed")
end
```

```text
all passed
```

Edges are where bugs live: is 16 a child or an adult? The test decides,
and writes the decision down where everyone can see it.

### Exercise

Write tests for `initials(name)`, which gives the first letter of each
word in capitals. Test a typical name, a single word, extra spaces, and
that an empty name fails with a `ValueError`. (The function has a bug with
extra spaces; your tests should find it. Then fix it.)

```vyrn starter
create initials(name: str) -> str:
    if name.strip() == "": fail ValueError("no name")
    return "".join([w[0].upper() each w in name.split(" ")])
end

create test_initials():
    assert(initials("ada lovelace") == "AL")
end

main:
    test_initials()
    print("passed")
end
```

```text expect
passed
```

> Hint: add `assert(initials("  grace   hopper ") == "GH")`: it fails,
> because `split(" ")` gives empty words.
> Hint: `split()` with no argument splits on any run of spaces.

<details><summary>Solution</summary>

```vyrn solution
create initials(name: str) -> str:
    if name.strip() == "": fail ValueError("no name")
    return "".join([w[0].upper() each w in name.split()])
end

create test_initials():
    assert(initials("ada lovelace") == "AL")
    assert(initials("cher") == "C")
    assert(initials("  grace   hopper ") == "GH")
    assert_fails(() => initials(""), "ValueError")
end

main:
    test_initials()
    print("passed")
end
```

</details>

## 47.4 Fixtures: shared setup

Several tests often need the same starting point: a list of sample users,
a database with some rows. Put it in a helper function, a **fixture**, and
call it at the start of each test. Each test gets fresh data, so one test
changing it can't break another:

```vyrn
use vyrn.db

create sample_db() -> any:
    let conn = db.open(":memory:")
    conn.create_table("books", {"title": "text", "year": "int"})
    conn.insert("books", {"title": "Dune", "year": 1965})
    conn.insert("books", {"title": "Emma", "year": 1815})
    return conn
end

create older_than(conn: any, year: int) -> list[str]:
    return [r["title"] each r in conn.query("select title from books where year < ? order by title", [year])]
end

create test_finds_old_books():
    let conn = sample_db()
    assert(older_than(conn, 1900) == ["Emma"])
end

create test_after_deleting():
    let conn = sample_db()
    conn.delete("books", {"title": "Emma"})
    assert(older_than(conn, 1900) == [])
end

create test_finds_all():
    let conn = sample_db()
    assert(len(older_than(conn, 3000)) == 2)
end

main:
    test_finds_old_books()
    test_after_deleting()
    test_finds_all()
    print("3 passed")
end
```

```text
3 passed
```

`test_after_deleting` removed a book, but `test_finds_all` still sees two,
because each test made its own database with `sample_db()`.

## 47.5 Fakes: testing without the outside world

Code that calls an API, sends email or reads the clock is hard to test:
the network is slow and changes, emails really get sent, the time is
different every run. Since Day 32 you've passed such things in as
functions. In a test, pass a **fake** instead: `fake(returns = ...)`
answers with a value you choose, `fake(returns_each = [...])` with a
different value each call, `fake(fails = ...)` fails, and every fake
remembers how it was called:

```vyrn
create remind(users: list[map], today: str, send: any) -> int:
    var sent = 0
    each u in users:
        if u["due"] == today:
            send(u["email"], "Your book is due today")
            sent += 1
        end
    end
    return sent
end

create test_only_due_users_are_emailed():
    let mail = fake(returns = true)
    let users = [{"email": "a@x.com", "due": "2026-11-12"}, {"email": "b@x.com", "due": "2026-11-20"}]
    assert(remind(users, "2026-11-12", mail) == 1)
    assert(mail.call_count == 1)
    assert(mail.last_call[0] == "a@x.com")
end

create test_a_failing_mail_server_is_reported():
    let broken = fake(fails = NetworkError("server down"))
    let users = [{"email": "a@x.com", "due": "2026-11-12"}]
    let e = assert_fails(() => remind(users, "2026-11-12", broken), "NetworkError")
    assert("down" in e.message)
end

main:
    test_only_due_users_are_emailed()
    test_a_failing_mail_server_is_reported()
    print("passed")
end
```

```text
passed
```

No email was sent and no network was used, yet the test proves that the
right person would be emailed, and what happens when the mail server is
down.

## 47.6 Testing a web app

`http.test` (Day 41) sends a request to your own routes without a network,
so routes are as easy to test as functions:

```vyrn
use vyrn.http

route GET "/hello/{name}" (name: str):
    return {"greeting": "Hello, {name.title()}!"}
end

route POST "/signup" (email: str):
    if "@" not in email: return http.Response(400, {"error": "that isn't an email"})
    return 201, {"email": email}
end

create test_greets_by_name():
    let r = http.test("GET", "/hello/ada")
    assert(r.status == 200)
    assert(r.json["greeting"] == "Hello, Ada!")
end

create test_signup_checks_the_email():
    assert(http.test("POST", "/signup", form = {"email": "ada@mail.com"}).status == 201)
    let bad = http.test("POST", "/signup", form = {"email": "ada"})
    assert(bad.status == 400)
    assert(bad.json["error"] == "that isn't an email")
end

create test_unknown_pages_are_404():
    assert(http.test("GET", "/nowhere").status == 404)
end

main:
    test_greets_by_name()
    test_signup_checks_the_email()
    test_unknown_pages_are_404()
    print("routes passed")
end
```

```text
routes passed
```

### Exercise

Write a test for this route: an item that exists comes back with status
200 and its name; one that doesn't gives 404.

```vyrn starter
use vyrn.http

struct NotFound:
    message: str
end

route GET "/items/{id}" (id: int):
    let items = {1: "lamp", 2: "desk"}
    if id not in items: fail NotFound("no item {id}")
    return {"id": id, "name": items[id]}
end

create test_items():
    assert(true)
end

main:
    test_items()
    print("passed")
end
```

```text expect
passed
```

> Hint: `assert(http.test("GET", "/items/2").json["name"] == "desk")`
> Hint: `assert(http.test("GET", "/items/9").status == 404)`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.http

struct NotFound:
    message: str
end

route GET "/items/{id}" (id: int):
    let items = {1: "lamp", 2: "desk"}
    if id not in items: fail NotFound("no item {id}")
    return {"id": id, "name": items[id]}
end

create test_items():
    let r = http.test("GET", "/items/2")
    assert(r.status == 200)
    assert(r.json["name"] == "desk")
    assert(http.test("GET", "/items/9").status == 404)
end

main:
    test_items()
    print("passed")
end
```

</details>

## 47.7 Project: testing the deal finder

The deal finder (Day 40) was written so every outside thing came in
through a `World`. That pays off now: the whole program, commands and all,
can be tested with fakes. Here are tests for its two most important
promises: it emails about a new deal, and it doesn't email about the same
deal twice.

```vyrn
use vyrn.db

object World:
    fetch: any
    send: any
    today: str
end

create check(conn: any, world: World) -> int:
    var hits = []
    each wish in conn.query("select title, target from wishes"):
        let deals = world.fetch(wish["title"])
        if not deals: skip
        let price = min([float(d["salePrice"]) each d in deals])
        let told = conn.value("select min(price) from alerted where title = ?", [wish["title"]])
        if price <= wish["target"] and (told == none or price < told):
            conn.execute("insert into alerted values (?, ?, ?)", [wish["title"], price, world.today])
            hits.append("{wish["title"]}: {price:.2f}")
        end
    end
    if hits: world.send("{len(hits)} deals", "\n".join(hits))
    return len(hits)
end

create setup() -> any:
    let conn = db.open(":memory:")
    conn.execute("create table wishes (title text, target real)")
    conn.execute("create table alerted (title text, price real, day text)")
    conn.execute("insert into wishes values ('celeste', 8)")
    return conn
end

create shop(price: str) -> any:
    return fake(returns = [{"salePrice": price}])
end

create test_a_new_deal_is_emailed():
    let mail = fake(returns = true)
    assert(check(setup(), World(shop("4.99"), mail, "11-12")) == 1)
    assert(mail.last_call[1] == "celeste: 4.99")
end

create test_the_same_deal_is_not_emailed_twice():
    let conn = setup()
    let mail = fake(returns = true)
    check(conn, World(shop("4.99"), mail, "11-12"))
    check(conn, World(shop("4.99"), mail, "11-13"))
    assert(mail.call_count == 1)
end

create test_a_lower_price_is_emailed_again():
    let conn = setup()
    let mail = fake(returns = true)
    check(conn, World(shop("4.99"), mail, "11-12"))
    check(conn, World(shop("3.99"), mail, "11-13"))
    assert(mail.call_count == 2)
end

create test_prices_over_target_are_ignored():
    let mail = fake(returns = true)
    assert(check(setup(), World(shop("9.99"), mail, "11-12")) == 0)
    assert(not mail.called)
end

main:
    each t in [test_a_new_deal_is_emailed, test_the_same_deal_is_not_emailed_twice, test_a_lower_price_is_emailed_again, test_prices_over_target_are_ignored]:
        t()
        print("ok", t.name)
    end
end
```

```text
ok test_a_new_deal_is_emailed
ok test_the_same_deal_is_not_emailed_twice
ok test_a_lower_price_is_emailed_again
ok test_prices_over_target_are_ignored
```

In a project, the tests go in `tests/test_deals.vyrn` and `vyrn test` runs
them. Notice the test names: each is a sentence saying what the program
promises. When one fails, its name tells you which promise broke.

### Exercise

Add a test that a wish nobody sells (the shop gives an empty list) sends
nothing and doesn't fail.

```vyrn starter
create check_one(deals: list, send: any) -> int:
    if not deals: return 0
    send("deal", str(deals[0]))
    return 1
end

create test_unsold_games_send_nothing():
    assert(true)
end

main:
    test_unsold_games_send_nothing()
    print("ok")
end
```

```text expect
ok
```

> Hint: `let mail = fake(returns = true)`, then
> `assert(check_one([], mail) == 0)` and `assert(not mail.called)`.

<details><summary>Solution</summary>

```vyrn solution
create check_one(deals: list, send: any) -> int:
    if not deals: return 0
    send("deal", str(deals[0]))
    return 1
end

create test_unsold_games_send_nothing():
    let mail = fake(returns = true)
    assert(check_one([], mail) == 0)
    assert(not mail.called)
end

main:
    test_unsold_games_send_nothing()
    print("ok")
end
```

</details>

## Today you learned

- A test is a `test_...` function with `assert`s; a failed comparison
  shows both sides. `vyrn test` runs every test in `tests/`, each in a
  fresh program.
- Test typical cases, edges and mistakes; `assert_fails` checks that
  something fails, and how.
- Fixtures are helper functions that build fresh test data for each test.
- Fakes stand in for the network, email and the clock, and record how they
  were called.
- `http.test` makes routes as easy to test as functions.
- Name tests after the promise they check.
