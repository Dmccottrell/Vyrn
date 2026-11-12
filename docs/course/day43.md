# Day 43: Forms and layout

So far your pages only *showed* things. **Forms** let visitors send
something back: a name, a message, a vote. Today you write forms, receive
what they send in your routes, check it, and answer. Then you arrange pages
into columns and grids with CSS **layout**. The project is a guestbook
where visitors leave messages.

## 43.1 A form

A form is a `<form>` element holding **inputs** and a button. When the
button is pressed, the browser sends every input's `name` and value to the
form's `action` address, with the form's `method`:

```text
<form action="/sign" method="post">
  <label>Your name <input name="name" required maxlength="40"></label>
  <label>Message <textarea name="message" required></textarea></label>
  <label>Rating
    <select name="stars">
      <option>5</option><option>4</option><option>3</option>
    </select>
  </label>
  <button>Sign the guestbook</button>
</form>
```

| Input | Is |
|---|---|
| `<input name="x">` | one line of text; `type="email"`, `"number"`, `"password"`, `"date"`, `"checkbox"` change what it takes |
| `<textarea name="x">` | several lines |
| `<select name="x">` with `<option>`s | a choice from a list |
| `<label>` | the words for an input; clicking them focuses it |

`required` and `maxlength` make the browser check before sending. They
help honest visitors, but anyone can send anything to your server without
using your form, so **the server must check too**.

## 43.2 Receiving a form

The browser sends the fields as `name=Ada&message=Hello+there&stars=5`.
A route receives each field as a parameter of the same name, converted to
the parameter's type:

```vyrn
use vyrn.http

route POST "/sign" (name: str, message: str, stars: int):
    return "Thanks {name}! {stars} stars for: {message}"
end

main:
    let r = http.test("POST", "/sign", form = {"name": "Ada", "message": "Hello there", "stars": "5"})
    print(r.status, r.text)
    print(http.test("POST", "/sign", form = {"name": "Ada"}).status)
    print(http.test("POST", "/sign", form = {"name": "Ada", "message": "x", "stars": "lots"}).status)
end
```

```text
200 Thanks Ada! 5 stars for: Hello there
400
400
```

A missing field, or `lots` where a number belongs, is answered with `400
Bad Request` before your code runs. `http.test(..., form = {...})` sends the
fields the way a browser does. The route can also take `request`, and read
every field from `request.form`, a map.

GET forms put the fields in the address instead, as a query string: right
for searches, which people like to bookmark.

```vyrn
use vyrn.http

route GET "/search" (q: str, page: int = 1):
    return "results for {q}, page {page}"
end

main:
    print(http.test("GET", "/search?q=green+tea").text)
    print(http.test("GET", "/search?q=tea&page=3").text)
end
```

```text
results for green tea, page 1
results for tea, page 3
```

## 43.3 Checking what arrives

Types are checked for you; everything else is your job. Collect every
problem, so the visitor can fix them all at once:

```vyrn
create problems(name: str, message: str, stars: int) -> list:
    var found = []
    if name.strip() == "": found.append("Please give your name.")
    if len(name) > 40: found.append("Names are at most 40 letters.")
    if len(message.strip()) < 3: found.append("The message is too short.")
    if stars < 1 or stars > 5: found.append("Stars go from 1 to 5.")
    return found
end

main:
    print(problems("Ada", "Lovely site!", 5))
    print(problems("  ", "hi", 9))
end
```

```text
[]
["Please give your name.", "The message is too short.", "Stars go from 1 to 5."]
```

### Exercise

Add a check to `email_problems(email)`: an address needs exactly one `@`,
with something before it and a `.` somewhere after it.

```vyrn starter
create email_problems(email: str) -> list:
    return []
end

main:
    each e in ["ada@mail.com", "ada.mail.com", "@mail.com", "ada@mail", "a@b@c.com"]:
        print(e, email_problems(e))
    end
end
```

```text expect
ada@mail.com []
ada.mail.com ["That isn't an email address."]
@mail.com ["That isn't an email address."]
ada@mail ["That isn't an email address."]
a@b@c.com ["That isn't an email address."]
```

> Hint: `parts = email.split("@")` must have length 2.
> Hint: then `parts[0] != ""` and `"." in parts[1]`.

<details><summary>Solution</summary>

```vyrn solution
create email_problems(email: str) -> list:
    let parts = email.split("@")
    if len(parts) == 2 and parts[0] != "" and "." in parts[1]: return []
    return ["That isn't an email address."]
end

main:
    each e in ["ada@mail.com", "ada.mail.com", "@mail.com", "ada@mail", "a@b@c.com"]:
        print(e, email_problems(e))
    end
end
```

</details>

## 43.4 Answering a form: redirect after POST

After a successful POST, don't answer with a page: answer with a
**redirect** to a normal page. Otherwise pressing the browser's refresh
button sends the form again, and the message appears twice. A redirect is
status `303 See Other` with a `Location` header.

Both routes need the same list of messages. Vyrn has no global variables:
instead, `main` **provides** a value under a name with `http.provide`, and
any route with a parameter of that name receives it. A list is shared, not
copied, so what one route appends the other sees:

```vyrn
use vyrn.http

route POST "/sign" (name: str, message: str, messages):
    messages.append((name, message))
    return http.Response(303, "", {"Location": "/"})
end

route GET "/" (messages):
    return "<ul>" + "".join(["<li>{n}: {m}</li>" each n, m in messages]) + "</ul>"
end

main:
    http.provide("messages", [])
    let r = http.test("POST", "/sign", form = {"name": "Ada", "message": "Hi!"})
    print(r.status, r.headers["location"])
    print(http.test("GET", "/").text)
end
```

```text
303 /
<ul><li>Ada: Hi!</li></ul>
```

A browser follows the redirect by itself, so the visitor lands on the
guestbook page and sees their message.

## 43.5 Layout: flexbox and grid

Boxes normally stack one under another. Two CSS tools arrange them:

**Flexbox** lines up children in a row (or column), sharing the space:

```text
nav { display: flex; gap: 16px; justify-content: space-between; align-items: center; }
```

**Grid** places children in columns. This one makes as many 220-pixel-wide
columns as fit, so the page works on phones and wide screens alike:

```text
.cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; }
```

| Property | Does |
|---|---|
| `display: flex` | children in a row; `flex-direction: column` for a column |
| `gap` | space between children |
| `justify-content` | spacing along the row: `start`, `center`, `space-between` |
| `align-items` | alignment across it: `start`, `center`, `stretch` |
| `display: grid` | children in a grid; `grid-template-columns` says the columns |
| `1fr` | one share of the leftover space |

Pages must work on phones. Add this to every page's `<head>`, or phones
pretend to be a wide screen and shrink everything:

```text
<meta name="viewport" content="width=device-width, initial-scale=1">
```

## 43.6 Project: the guestbook

Everything together: a page with the form and the messages in a grid of
cards, a POST route that checks the fields and either shows the problems or
saves and redirects. Names and messages are escaped (Day 41), because
visitors wrote them:

```vyrn
use vyrn.html
use vyrn.http

const STYLE = r".cards { display: grid; grid-template-columns: repeat(auto-fill, minmax(220px, 1fr)); gap: 16px; } .card { padding: 12px; border: 1px solid #ddd; border-radius: 8px; } .error { color: #c62828; }"

const FORM = r'<form action="/sign" method="post"><input name="name" placeholder="Name" required><textarea name="message" required></textarea><button>Sign</button></form>'

create page(entries: list, errors: list = []) -> str:
    let cards = "".join(["<div class=\"card\"><strong>{html.escape(n)}</strong><p>{html.escape(m)}</p></div>" each n, m in entries])
    let shown = "".join(["<p class=\"error\">{e}</p>" each e in errors])
    return "<!doctype html><title>Guestbook</title><style>{STYLE}</style><h1>Guestbook</h1>{shown}{FORM}<div class=\"cards\">{cards}</div>"
end

route GET "/" (entries):
    return page(entries)
end

route POST "/sign" (name: str, message: str, entries):
    var errors = []
    if name.strip() == "": errors.append("Please give your name.")
    if len(message.strip()) < 3: errors.append("The message is too short.")
    if errors: return http.Response(400, page(entries, errors), {"Content-Type": "text/html"})
    entries.append((name.strip(), message.strip()))
    return http.Response(303, "", {"Location": "/"})
end

main:
    http.provide("entries", [])
    print(http.test("POST", "/sign", form = {"name": "Ada", "message": "Lovely <b>site</b>!"}).status)
    let bad = http.test("POST", "/sign", form = {"name": " ", "message": "hi"})
    print(bad.status, [p.text each p in html.parse(bad.text).select(".error")])
    let home = html.parse(http.test("GET", "/").text)
    print([c.select_one("p").text each c in home.select(".card")])
    print("<b>" in http.test("GET", "/").text)
end
```

```text
303
400 ["Please give your name.", "The message is too short."]
["Lovely <b>site</b>!"]
false
```

The last line checks that the visitor's `<b>` was escaped rather than sent
as HTML. Serve it with `http.serve(port = 8000)` and sign it yourself.

The messages live in a list, so they vanish when the server stops; Day 56
keeps them in a database.

### Exercise

Show the newest message first, and number the cards: `#2`, `#1`. Write
`card_titles(entries)` giving the strong text of each card in order.

```vyrn starter
create card_titles(entries: list) -> list:
    return [n each n, m in entries]
end

main:
    print(card_titles([("Ada", "Hi"), ("Alan", "Hello"), ("Grace", "Hey")]))
end
```

```text expect
["#3 Grace", "#2 Alan", "#1 Ada"]
```

> Hint: `each i, entry in enumerate(entries)` gives the position; the
> number is `i + 1`.
> Hint: build the list, then `.reverse()` it, or use `reversed`.

<details><summary>Solution</summary>

```vyrn solution
create card_titles(entries: list) -> list:
    var titles = []
    each i, entry in enumerate(entries):
        titles.append("#{i + 1} {entry[0]}")
    end
    titles.reverse()
    return titles
end

main:
    print(card_titles([("Ada", "Hi"), ("Alan", "Hello"), ("Grace", "Hey")]))
end
```

</details>

## Today you learned

- A `<form>` sends its inputs' names and values to its `action` with its
  `method`; route parameters receive them by name.
- Check every field on the server, collect all the problems, and show them.
- After a successful POST, redirect with `303` and a `Location`.
- Routes share values that `main` gives them with `http.provide(name, value)`.
- Flexbox lines things up, grid places them in columns; add the viewport
  `<meta>` for phones.
