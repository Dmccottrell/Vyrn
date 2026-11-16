# Day 49: Logins

Most web apps have accounts: you sign up, log in, and the site remembers
who you are from page to page. Today you build that properly. You'll learn
how a browser remembers things with **cookies**, how a server remembers a
logged-in visitor with **sessions**, and the one right way to store
passwords. The project is a members-only area with sign-up, log-in and
log-out.

## 49.1 Cookies

HTTP forgets: each request arrives on its own, with no memory of the last.
A **cookie** is a small piece of text the server asks the browser to keep
and send back with every later request to that site. The server sets it
with a `Set-Cookie` header; the browser returns it in a `Cookie` header:

```vyrn
use vyrn.http

route GET "/theme/{name}" (name: str):
    return http.Response(200, "theme set", {"Set-Cookie": http.cookie("theme", name, max_age = 30 * 86400)})
end

route GET "/" (request):
    let theme = request.cookies.get("theme", "light")
    return "<body class=\"{theme}\">Hello</body>"
end

main:
    let r = http.test("GET", "/theme/dark")
    print(r.headers["set-cookie"])
    print(http.test("GET", "/").text)
    print(http.test("GET", "/", headers = {"Cookie": "theme=dark"}).text)
end
```

```text
theme=dark; Path=/; Max-Age=2592000; HttpOnly; SameSite=Lax
<body class="light">Hello</body>
<body class="dark">Hello</body>
```

A browser follows this by itself; `http.test` doesn't keep cookies, so the
example sends the `Cookie` header by hand, as the browser would.

`http.cookie` adds safe settings for you:

| Setting | Means |
|---|---|
| `Max-Age=...` | keep it this many seconds (`max_age = 0` deletes it) |
| `HttpOnly` | the page's JavaScript can't read it, so injected scripts can't steal it |
| `SameSite=Lax` | other sites can't make the browser send it with their forms |
| `Secure` (with `secure = true`) | only send it over HTTPS: use this on a real site |

Cookies are stored on the visitor's computer, and they can edit them. Never
trust a cookie to say *who* someone is: `user=admin` is something anyone
can type.

## 49.2 Storing passwords

Never store passwords as they are. Databases leak, and people use the same
password everywhere: a leaked table of real passwords opens their email
and bank too. Instead, store a **hash**: a scrambled fingerprint that can
check a password but can't be turned back into it:

```vyrn
use vyrn.crypto

main:
    let stored = crypto.hash_password("correct horse battery")
    print(stored.split("$")[0], stored.split("$")[1])
    print(crypto.check_password("correct horse battery", stored))
    print(crypto.check_password("Correct horse battery", stored))
    print(stored == crypto.hash_password("correct horse battery"))
end
```

```text
pbkdf2-sha256 100000
true
false
false
```

Three details make this the right way, and they're why you use
`hash_password` instead of something like `crypto.sha256(password)`:

- A random **salt** is mixed in, so the same password gives a different
  hash each time (the last line), and attackers can't use tables of
  pre-computed hashes.
- The hashing is repeated 100,000 times (**rounds**), so trying millions of
  guesses is slow.
- `check_password` compares in a way that takes the same time whether the
  first character or the last is wrong, so timing reveals nothing.

## 49.3 Sessions

When someone logs in, the server makes a long random **session id** with
`crypto.token()`, remembers which user it belongs to, and gives the browser
the id in a cookie. On each request, the id from the cookie finds the user.
Guessing a valid id is hopeless (it's 64 random hex digits), and logging
out just forgets it:

```vyrn
use vyrn.crypto

main:
    var sessions = {}
    let id = crypto.token()
    sessions[id] = "ada"
    print(len(id), sessions.get(id), sessions.get("made-up-id"))
    sessions.pop(id)
    print(sessions.get(id))
end
```

```text
64 ada none
none
```

## 49.4 Middleware that knows who's there

Every page needs to know the logged-in user. Rather than repeating that in
each route, a **middleware** (Day 41's `http.use`) does it for every
request: here, it turns pages under `/members` away unless the session is
valid:

```vyrn
use vyrn.http

create members_only(request, next):
    if request.path.startswith("/members"):
        let sessions = {"s3cret": "ada"}
        let user = sessions.get(request.cookies.get("session", ""))
        if user == none: return http.Response(303, "", {"Location": "/login"})
    end
    return next(request)
end

route GET "/members" ():
    return "<h1>Members only</h1>"
end

main:
    http.use(members_only)
    let away = http.test("GET", "/members")
    print(away.status, away.headers["location"])
    print(http.test("GET", "/members", headers = {"Cookie": "session=s3cret"}).text)
end
```

```text
303 /login
<h1>Members only</h1>
```

## 49.5 Project: a members area

Everything together: sign-up, log-in, a members page that greets you by
name, and log-out. Users and sessions are kept in a database, so a
restart doesn't log everyone out.

```vyrn
use vyrn.crypto
use vyrn.db
use vyrn.html
use vyrn.http

create setup(conn: any):
    conn.execute("create table if not exists users (name text primary key, password text)")
    conn.execute("create table if not exists sessions (id text primary key, name text)")
end

create current_user(request, conn) -> str?:
    let id = request.cookies.get("session", "")
    return conn.value("select name from sessions where id = ?", [id])
end

create start_session(conn: any, name: str) -> any:
    let id = crypto.token()
    conn.insert("sessions", {"id": id, "name": name})
    return http.Response(303, "", {"Location": "/members", "Set-Cookie": http.cookie("session", id, max_age = 7 * 86400)})
end

route POST "/signup" (name: str, password: str, conn):
    if len(password) < 10: return http.Response(400, "Passwords need at least 10 characters.")
    if conn.find_one("users", {"name": name}) != none: return http.Response(409, "That name is taken.")
    conn.insert("users", {"name": name, "password": crypto.hash_password(password)})
    return start_session(conn, name)
end

route POST "/login" (name: str, password: str, conn):
    let user = conn.find_one("users", {"name": name})
    if user == none or not crypto.check_password(password, user["password"]):
        return http.Response(401, "Wrong name or password.")
    end
    return start_session(conn, name)
end

route POST "/logout" (request, conn):
    conn.delete("sessions", {"id": request.cookies.get("session", "")})
    return http.Response(303, "", {"Location": "/", "Set-Cookie": http.cookie("session", "", max_age = 0)})
end

route GET "/members" (request, conn):
    let name = current_user(request, conn)
    if name == none: return http.Response(303, "", {"Location": "/login"})
    return "<h1>Welcome back, {html.escape(name)}!</h1>"
end

create session_cookie(r: any) -> map:
    let id = r.headers["set-cookie"].split(";")[0]
    return {"Cookie": id}
end

main:
    let conn = db.open(":memory:")
    setup(conn)
    http.provide("conn", conn)

    let signed = http.test("POST", "/signup", form = {"name": "ada", "password": "analytical-engine"})
    print("sign up:", signed.status, signed.headers["location"])
    print(http.test("GET", "/members", headers = session_cookie(signed)).text)

    print("short password:", http.test("POST", "/signup", form = {"name": "bob", "password": "123"}).status)
    print("taken:", http.test("POST", "/signup", form = {"name": "ada", "password": "something-else"}).status)
    print("wrong password:", http.test("POST", "/login", form = {"name": "ada", "password": "babbage"}).status)

    let again = http.test("POST", "/login", form = {"name": "ada", "password": "analytical-engine"})
    let cookie = session_cookie(again)
    print("log in:", again.status)
    print("log out:", http.test("POST", "/logout", headers = cookie).status)
    print("after log out:", http.test("GET", "/members", headers = cookie).status)
    print("stored:", conn.value("select password from users").startswith("pbkdf2-sha256$"))
end
```

```text
sign up: 303 /members
<h1>Welcome back, ada!</h1>
short password: 400
taken: 409
wrong password: 401
log in: 303
log out: 303
after log out: 303
stored: true
```

Look at what a wrong password says: "Wrong name or password", not "wrong
password". Saying which part was wrong would tell an attacker which names
exist. And after logging out, the old cookie is useless, because its
session is gone from the database, even if someone copied it.

The pages with the forms are plain HTML (Day 43):
`<form action="/login" method="post">` with `name` and `password` inputs,
`type="password"` on the second so it shows dots.

### Exercise

Sessions should expire. Store when each session was made, and make
`valid(created, now, max_age)` say whether a session is still young
enough.

```vyrn starter
create valid(created: float, now: float, max_age: float) -> bool:
    return true
end

main:
    let week = 7 * 86400
    print(valid(1000.0, 1000.0 + 3600, week), valid(1000.0, 1000.0 + week + 1, week))
end
```

```text expect
true false
```

> Hint: valid while `now - created <= max_age`.
> Hint: in the app, store `time.now()` in a `created` column, and check it
> in `current_user`.

<details><summary>Solution</summary>

```vyrn solution
create valid(created: float, now: float, max_age: float) -> bool:
    return now - created <= max_age
end

main:
    let week = 7 * 86400
    print(valid(1000.0, 1000.0 + 3600, week), valid(1000.0, 1000.0 + week + 1, week))
end
```

</details>

## Today you learned

- Cookies are small texts the browser keeps and sends back; set them with
  `http.cookie`, read them with `request.cookies`. Never trust them to say
  who someone is.
- Store `crypto.hash_password(pw)`, never the password; check with
  `crypto.check_password`.
- A session id from `crypto.token()` in a cookie, looked up on the server,
  says who is logged in; logging out deletes the session.
- Middleware can check every request in one place.
- Don't reveal whether the name or the password was wrong.
