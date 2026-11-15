# Day 53: Capstone: a todo web app

This unit's capstone is a complete web application: people sign up, log
in, and keep their own todo list, which nobody else can see or change. It
uses everything from Days 41 to 52: templates, forms, a database, logins,
security, tests and deployment settings. Read it as a whole, run the
tests, then make it yours.

## 53.1 The plan

| Piece | Days |
|---|---|
| users with hashed passwords, sessions in cookies | 49 |
| todos in a database, each belonging to a user | 38, 50 |
| pages from templates, with forms that POST and redirect | 43, 44 |
| CSRF tokens on every form; every query checks the owner | 51 |
| tests for each promise, through `http.test` | 47 |
| settings from the environment | 52 |

One rule matters more than the rest: **every query about todos includes
the user**. Finding todo 7 is `where id = ? and owner = ?`, never just
`where id = ?`. Otherwise anyone logged in could change todo 7 by typing
its number into a form: this is called **broken access control**, and it's
the most common serious bug in real web apps.

## 53.2 The app

```vyrn
use vyrn.crypto
use vyrn.db
use vyrn.html
use vyrn.http
use vyrn.template

const LAYOUT = r'<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>Todos</title></head><body>{{{content}}}</body></html>'

const LIST = r'<h1>{{name}}&#39;s todos</h1><form method="post" action="/todos"><input type="hidden" name="csrf" value="{{csrf}}"><input name="text" required><button>Add</button></form><ul>{{#each todos}}<li class="{{#if done}}done{{/if}}"><form method="post" action="/todos/{{id}}/toggle"><input type="hidden" name="csrf" value="{{csrf}}"><button>{{#if done}}Undo{{else}}Done{{/if}}</button></form> {{text}}</li>{{else}}<li class="empty">Nothing to do!</li>{{/each}}</ul><form method="post" action="/logout"><button>Log out</button></form>'

struct Forbidden:
    message: str
end

struct NotFound:
    message: str
end

create setup(conn: any):
    conn.execute("create table users (name text primary key, password text)")
    conn.execute("create table sessions (id text primary key, name text, csrf text)")
    conn.execute("create table todos (id integer primary key, owner text, text text, done int)")
end

## The logged-in user's session, or none.
create session_of(request, conn) -> map?:
    return conn.find_one("sessions", {"id": request.cookies.get("session", "")})
end

## The session, or a failure: for routes that need a user and a valid form.
create require_form(request, conn, csrf: str) -> map:
    let s = session_of(request, conn)
    if s == none: fail Forbidden("log in first")
    if not crypto.same(csrf, s["csrf"]): fail Forbidden("bad form token")
    return s
end

create login_response(conn: any, name: str) -> any:
    let id = crypto.token()
    conn.insert("sessions", {"id": id, "name": name, "csrf": crypto.token(16)})
    return http.Response(303, "", {"Location": "/", "Set-Cookie": http.cookie("session", id, max_age = 7 * 86400)})
end

route POST "/signup" (name: str, password: str, conn):
    if not (2 <= len(name) <= 30) or not all(c.isalnum() each c in name):
        return http.Response(400, "Names are 2 to 30 letters and digits.")
    end
    if len(password) < 10: return http.Response(400, "Passwords need at least 10 characters.")
    if conn.find_one("users", {"name": name}) != none: return http.Response(409, "That name is taken.")
    conn.insert("users", {"name": name, "password": crypto.hash_password(password)})
    return login_response(conn, name)
end

route POST "/login" (name: str, password: str, conn):
    let user = conn.find_one("users", {"name": name})
    if user == none or not crypto.check_password(password, user["password"]):
        return http.Response(401, "Wrong name or password.")
    end
    return login_response(conn, name)
end

route POST "/logout" (request, conn):
    conn.delete("sessions", {"id": request.cookies.get("session", "")})
    return http.Response(303, "", {"Location": "/login", "Set-Cookie": http.cookie("session", "", max_age = 0)})
end

route GET "/" (request, conn):
    let s = session_of(request, conn)
    if s == none: return http.Response(303, "", {"Location": "/login"})
    let todos = conn.query("select id, text, done from todos where owner = ? order by done, id", [s["name"]])
    let content = template.render(LIST, {"name": s["name"], "csrf": s["csrf"], "todos": [t | {"csrf": s["csrf"]} each t in todos]})
    return template.render(LAYOUT, {"content": content})
end

route POST "/todos" (text: str, request, conn, csrf: str = ""):
    let s = require_form(request, conn, csrf)
    if text.strip() == "": return http.Response(400, "A todo needs some text.")
    conn.insert("todos", {"owner": s["name"], "text": text.strip()[:200], "done": 0})
    return http.Response(303, "", {"Location": "/"})
end

route POST "/todos/{id}/toggle" (id: int, request, conn, csrf: str = ""):
    let s = require_form(request, conn, csrf)
    let changed = conn.execute("update todos set done = 1 - done where id = ? and owner = ?", [id, s["name"]])
    if changed == 0: fail NotFound("no todo {id}")
    return http.Response(303, "", {"Location": "/"})
end

## Test helpers: sign up, and keep the session cookie and form token.
create sign_up(name: str) -> map:
    let r = http.test("POST", "/signup", form = {"name": name, "password": "a-long-password"})
    let cookie = {"Cookie": r.headers["set-cookie"].split(";")[0]}
    let page = html.parse(http.test("GET", "/", headers = cookie).text)
    return {"cookie": cookie, "csrf": page.select_one("input[name=csrf]").attr("value")}
end

create add(user: map, text: str) -> int:
    return http.test("POST", "/todos", form = {"text": text, "csrf": user["csrf"]}, headers = user["cookie"]).status
end

create items(user: map) -> list[str]:
    let page = html.parse(http.test("GET", "/", headers = user["cookie"]).text)
    return [li.text.replace("Done", "").replace("Undo", "").strip() each li in page.select("li")]
end

create test_people_see_only_their_own_todos():
    let ada = sign_up("ada")
    let alan = sign_up("alan")
    add(ada, "Write the engine notes")
    add(alan, "Break the code")
    assert(items(ada) == ["Write the engine notes"])
    assert(items(alan) == ["Break the code"])
end

create test_nobody_can_change_someone_elses_todo():
    let ada = sign_up("ada")
    let mallory = sign_up("mallory")
    add(ada, "Secret plan")
    let attack = http.test("POST", "/todos/1/toggle", form = {"csrf": mallory["csrf"]}, headers = mallory["cookie"])
    assert(attack.status == 404)
end

create test_forms_need_the_token():
    let ada = sign_up("ada")
    let forged = http.test("POST", "/todos", form = {"text": "from another site"}, headers = ada["cookie"])
    assert(forged.status == 403)
    assert(items(ada) == ["Nothing to do!"])
end

create test_done_todos_sink_to_the_bottom():
    let ada = sign_up("ada")
    add(ada, "first")
    add(ada, "second")
    http.test("POST", "/todos/1/toggle", form = {"csrf": ada["csrf"]}, headers = ada["cookie"])
    assert(items(ada) == ["second", "first"])
end

create test_logged_out_visitors_are_sent_to_login():
    assert(http.test("GET", "/").headers["location"] == "/login")
    assert(http.test("POST", "/todos", form = {"text": "x"}).status == 403)
end

main:
    each test in [test_people_see_only_their_own_todos, test_nobody_can_change_someone_elses_todo, test_forms_need_the_token, test_done_todos_sink_to_the_bottom, test_logged_out_visitors_are_sent_to_login]:
        let conn = db.open(":memory:")
        setup(conn)
        http.provide("conn", conn)
        test()
        print("ok", test.name)
    end
end
```

```text
ok test_people_see_only_their_own_todos
ok test_nobody_can_change_someone_elses_todo
ok test_forms_need_the_token
ok test_done_todos_sink_to_the_bottom
ok test_logged_out_visitors_are_sent_to_login
```

Each test gets a fresh database (`main` provides a new one before each),
and each one's name is a promise the app keeps. Look at
`test_nobody_can_change_someone_elses_todo`: Mallory is logged in and has
a valid form token, but todo 1 is Ada's, and the `and owner = ?` in the
query makes it simply not exist for her.

## 53.3 Making it real

To run it, split it into files the way a project is laid out:

```text
todos/
  vyrn.toml
  main.vyrn          settings from the environment, http.serve
  app.vyrn           the routes and helpers
  templates/         layout.html, list.html, login.html
  tests/
    test_app.vyrn    the tests above
  Dockerfile         from Day 52
  .github/workflows/check.yml
```

`main.vyrn` is short:

```vyrn check
use vyrn.db
use vyrn.http
use vyrn.os

main:
    os.load_env()
    let conn = db.open(os.env("DATABASE", "todos.db"))
    conn.execute("create table if not exists users (name text primary key, password text)")
    conn.execute("create table if not exists sessions (id text primary key, name text, csrf text)")
    conn.execute("create table if not exists todos (id integer primary key, owner text, text text, done int)")
    http.provide("conn", conn)
    http.serve(port = int(os.env("PORT", "8000")), host = os.env("HOST", "127.0.0.1"))
end
```

The login page is a plain form (Day 43) posting `name` and `password` to
`/login`, with a link to a sign-up form posting to `/signup`.

### Exercise

Add a delete button. Write the query function `delete_todo(conn, id,
owner)` that deletes the todo only if it belongs to `owner`, and gives
whether anything was deleted.

```vyrn starter
use vyrn.db

create delete_todo(conn: any, id: int, owner: str) -> bool:
    return false
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table todos (id integer primary key, owner text, text text, done int)")
    conn.insert("todos", {"owner": "ada", "text": "a", "done": 0})
    print(delete_todo(conn, 1, "mallory"), delete_todo(conn, 1, "ada"), delete_todo(conn, 1, "ada"))
end
```

```text expect
false true false
```

> Hint: `conn.delete("todos", {"id": id, "owner": owner}) > 0`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.db

create delete_todo(conn: any, id: int, owner: str) -> bool:
    return conn.delete("todos", {"id": id, "owner": owner}) > 0
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table todos (id integer primary key, owner text, text text, done int)")
    conn.insert("todos", {"owner": "ada", "text": "a", "done": 0})
    print(delete_todo(conn, 1, "mallory"), delete_todo(conn, 1, "ada"), delete_todo(conn, 1, "ada"))
end
```

</details>

## Today you built

A multi-user web app with accounts, private data, protection against the
common attacks, and tests for each of its promises. Ideas to take it
further:

- Due dates, sorted by soonest, with overdue ones in red.
- Sharing a list with another user, who can see but not change it.
- A JSON API (Day 48) beside the pages, for a phone app.
- Deploy it (Day 52) and use it yourself.
