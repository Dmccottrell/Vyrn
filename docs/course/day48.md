# Day 48: Building an API

On Days 33 to 37 your programs *used* other people's APIs. Today you build
one: a web server that answers in JSON, for other programs (a phone app,
a web page's JavaScript, another server) to use. You'll learn **REST**,
the conventions almost every web API follows, so anyone who has used one
API can guess how yours works. The project is a notes API, tested from
end to end.

## 48.1 Routes that answer in JSON

A route that returns a map, list or struct sends it as JSON. Parts of the
path in braces are **path parameters**; other parameters come from the
query string:

```vyrn
use vyrn.http

const BOOKS = [
    {"id": 1, "title": "Dune", "year": 1965},
    {"id": 2, "title": "Emma", "year": 1815},
    {"id": 3, "title": "Ubik", "year": 1969},
]

route GET "/books" (after: int = 0):
    return [b each b in BOOKS if b["year"] > after]
end

route GET "/books/{id}" (id: int):
    return [b each b in BOOKS if b["id"] == id][0]
end

main:
    print(http.test("GET", "/books?after=1900").text)
    let r = http.test("GET", "/books/2")
    print(r.status, r.headers["content-type"], r.json["title"])
    print(http.test("GET", "/books/abc").status)
end
```

```text
[{"id":1,"title":"Dune","year":1965},{"id":3,"title":"Ubik","year":1969}]
200 application/json Emma
400
```

`{id}` with `id: int` only accepts numbers: `/books/abc` is answered with
`400 Bad Request` before your code runs. `after` has a default, so it can
be left out.

## 48.2 REST: nouns and verbs

**REST** organises an API around **resources** (the nouns: books, users,
orders), each with an address, and uses the HTTP methods (Day 37) as the
verbs:

| Request | Means | Success answer |
|---|---|---|
| `GET /books` | list the books | `200` and the list |
| `GET /books/7` | one book | `200` and the book, or `404` |
| `POST /books` | add a book (the body says which) | `201 Created` and the new book |
| `PUT /books/7` | replace book 7 | `200` and the book |
| `DELETE /books/7` | remove book 7 | `204 No Content` |

Addresses are plural nouns, never verbs: `POST /books`, not
`/create_book`. Status codes say what happened, so a client can tell
"not found" (`404`) from "you sent nonsense" (`400`) from "the server
broke" (`500`) without reading the message.

## 48.3 Receiving JSON

A `POST` or `PUT` sends the new data as JSON in the body. A parameter
named `body` with a struct type receives it, checked: a missing field is
a `400` with a message saying which:

```vyrn
use vyrn.http

struct NewBook:
    title: str
    year: int
end

route POST "/books" (body: NewBook):
    return 201, {"id": 4, "title": body.title, "year": body.year}
end

main:
    let r = http.test("POST", "/books", {"title": "Beloved", "year": 1987})
    print(r.status, r.json)
    let bad = http.test("POST", "/books", {"title": "No year"})
    print(bad.status, bad.json["error"])
end
```

```text
201 {"id": 4, "title": "Beloved", "year": 1987}
400 BadRequest
```

Returning `201, value` sets the status; returning `none` sends `204 No
Content`.

## 48.4 Errors as status codes

Fail with an error type named after a status, and the server turns it into
that status, with a JSON body saying what went wrong:

| Fail with | Status |
|---|---|
| `BadRequest` | 400 |
| `Unauthorized` | 401 |
| `Forbidden` | 403 |
| `NotFound` | 404 |
| `Conflict` | 409 |
| anything else | 500 |

```vyrn
use vyrn.http

struct NotFound:
    message: str
end

struct Conflict:
    message: str
end

route GET "/users/{name}" (name: str, users):
    if name not in users: fail NotFound("no user called {name}")
    return users[name]
end

route POST "/users/{name}" (name: str, users):
    if name in users: fail Conflict("{name} is taken")
    users[name] = {"name": name, "joined": "2026-11-12"}
    return 201, users[name]
end

main:
    http.provide("users", {})
    print(http.test("POST", "/users/ada").status)
    print(http.test("POST", "/users/ada").text)
    print(http.test("GET", "/users/ada").json["joined"])
    print(http.test("GET", "/users/zed").text)
end
```

```text
201
{"error": "Conflict", "message": "ada is taken"}
2026-11-12
{"error": "NotFound", "message": "no user called zed"}
```

The `users` map comes from `http.provide` (Day 43), so every route shares
it.

### Exercise

Add `DELETE /users/{name}`: it removes the user and answers `204`, or
fails with `NotFound` if there is no such user.

```vyrn starter
use vyrn.http

struct NotFound:
    message: str
end

main:
    http.provide("users", {"ada": {"name": "ada"}})
    print(http.test("DELETE", "/users/ada").status, http.test("DELETE", "/users/ada").status)
end
```

```text expect
204 404
```

> Hint: `route DELETE "/users/{name}" (name: str, users):`
> Hint: `users.pop(name)` removes it; `return none` gives `204`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.http

struct NotFound:
    message: str
end

route DELETE "/users/{name}" (name: str, users):
    if name not in users: fail NotFound("no user called {name}")
    users.pop(name)
    return none
end

main:
    http.provide("users", {"ada": {"name": "ada"}})
    print(http.test("DELETE", "/users/ada").status, http.test("DELETE", "/users/ada").status)
end
```

</details>

## 48.5 Project: the notes API

A complete REST API for notes, kept in a database so they survive a
restart. The database connection is provided to the routes; the tests use
an in-memory one, the real server a file:

```vyrn
use vyrn.db
use vyrn.http

struct NoteIn:
    title: str
    text: str
end

struct NotFound:
    message: str
end

struct BadRequest:
    message: str
end

create open_notes(path: str) -> any:
    let conn = db.open(path)
    conn.execute("create table if not exists notes (id integer primary key, title text, text text)")
    return conn
end

create find(conn: any, id: int) -> map:
    let note = conn.find_one("notes", {"id": id})
    if note == none: fail NotFound("no note {id}")
    return note
end

route GET "/notes" (conn, q: str = ""):
    return conn.query("select * from notes where title like ? order by id", ["%" + q + "%"])
end

route GET "/notes/{id}" (id: int, conn):
    return find(conn, id)
end

route POST "/notes" (body: NoteIn, conn):
    if body.title.strip() == "": fail BadRequest("a note needs a title")
    let id = conn.insert("notes", {"title": body.title, "text": body.text})
    return http.Response(201, find(conn, id), {"Location": "/notes/{id}"})
end

route PUT "/notes/{id}" (id: int, body: NoteIn, conn):
    find(conn, id)
    conn.update("notes", {"id": id}, {"title": body.title, "text": body.text})
    return find(conn, id)
end

route DELETE "/notes/{id}" (id: int, conn):
    find(conn, id)
    conn.delete("notes", {"id": id})
    return none
end

create test_the_whole_life_of_a_note():
    http.provide("conn", open_notes(":memory:"))
    let made = http.test("POST", "/notes", {"title": "Shopping", "text": "milk"})
    assert(made.status == 201)
    assert(made.headers["location"] == "/notes/1")
    assert(http.test("GET", "/notes/1").json["text"] == "milk")
    let changed = http.test("PUT", "/notes/1", {"title": "Shopping", "text": "milk, eggs"})
    assert(changed.json["text"] == "milk, eggs")
    assert(http.test("DELETE", "/notes/1").status == 204)
    assert(http.test("GET", "/notes/1").status == 404)
end

create test_search_and_mistakes():
    http.provide("conn", open_notes(":memory:"))
    http.test("POST", "/notes", {"title": "Shopping", "text": ""})
    http.test("POST", "/notes", {"title": "Ideas", "text": ""})
    assert([n["title"] each n in http.test("GET", "/notes?q=dea").json] == ["Ideas"])
    assert(http.test("POST", "/notes", {"title": " ", "text": "x"}).status == 400)
    assert(http.test("POST", "/notes", {"text": "no title"}).status == 400)
    assert(http.test("PUT", "/notes/99", {"title": "a", "text": "b"}).status == 404)
end

main:
    test_the_whole_life_of_a_note()
    test_search_and_mistakes()
    print("the notes API works")
end
```

```text
the notes API works
```

To run it for real, give `main` the file database and start the server:

```vyrn check
use vyrn.db
use vyrn.http

main:
    http.provide("conn", db.open("notes.db"))
    http.serve(port = 8000)
end
```

and try it from another program, or from the terminal with `curl`:

```bash
curl -X POST localhost:8000/notes -H "Content-Type: application/json" -d '{"title": "Hi", "text": "from curl"}'
```

### Exercise

Add paging to the list: `GET /notes?page=2&per_page=10` gives the second
ten notes. Write `page_of(items, page, per_page)`, giving that slice of a
list (pages count from 1).

```vyrn starter
create page_of(items: list, page: int, per_page: int) -> list:
    return items
end

main:
    let items = list(1 to 25)
    print(page_of(items, 1, 10), page_of(items, 3, 10), page_of(items, 4, 10))
end
```

```text expect
[1, 2, 3, 4, 5, 6, 7, 8, 9, 10] [21, 22, 23, 24, 25] []
```

> Hint: page 1 starts at 0, page 2 at `per_page`: the start is
> `(page - 1) * per_page`.

<details><summary>Solution</summary>

```vyrn solution
create page_of(items: list, page: int, per_page: int) -> list:
    let start = (page - 1) * per_page
    return items[start:start + per_page]
end

main:
    let items = list(1 to 25)
    print(page_of(items, 1, 10), page_of(items, 3, 10), page_of(items, 4, 10))
end
```

</details>

## Today you learned

- Routes return maps, lists and structs as JSON; `{id}` parts and other
  parameters are checked against their types.
- REST: plural nouns for addresses, HTTP methods for verbs, status codes
  for outcomes.
- `body: SomeStruct` receives a JSON body, checked field by field.
- `fail NotFound(...)` and friends become status codes with a JSON error.
- `http.provide` shares a database with every route; tests provide an
  in-memory one and drive the API with `http.test`.
