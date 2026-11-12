# Day 41: Web pages

Every web page is a text file written in **HTML**, the language browsers
read. Today you learn enough HTML to write pages by hand, then have Vyrn
write them for you: from data, safely, and served by a web server of your
own. The project is a personal home page made from a map of facts about
you.

## 41.1 Tags and elements

HTML marks up text with **tags** in angle brackets. Most come in pairs, an
opening `<p>` and a closing `</p>`, and everything between them is one
**element**:

```text
<!doctype html>
<html>
  <head>
    <title>My page</title>
  </head>
  <body>
    <h1>Hello!</h1>
    <p>This is a <strong>web page</strong>.</p>
  </body>
</html>
```

- `<!doctype html>` says "this is modern HTML".
- `<head>` holds facts *about* the page: its title (shown on the tab).
- `<body>` holds what you see.
- Elements **nest**: the `<strong>` is inside the `<p>`, which is inside
  the `<body>`. Indenting shows the nesting, like blocks in Vyrn.

Save that text in a file called `page.html` and open it in a browser, and
you have made a web page. Vyrn can save it for you, and also read it back
the way a browser does with `vyrn.html`:

```vyrn
use vyrn.html

const PAGE = """<!doctype html>
<html>
  <head><title>My page</title></head>
  <body>
    <h1>Hello!</h1>
    <p>This is a <strong>web page</strong>.</p>
  </body>
</html>"""

main:
    write_text("page.html", PAGE)
    let page = html.parse(read_text("page.html"))
    print(page.title)
    print(page.select_one("h1").text)
    print(page.select_one("p").text)
    print(page.select_one("strong").parent.tag)
end
```

```text
My page
Hello!
This is a web page.
p
```

## 41.2 The tags you'll use most

| Tag | Is |
|---|---|
| `<h1>` ... `<h6>` | headings, from biggest to smallest |
| `<p>` | a paragraph |
| `<a href="https://...">text</a>` | a link |
| `<img src="cat.png" alt="A cat">` | a picture (no closing tag) |
| `<ul>` with `<li>` items | a bulleted list; `<ol>` numbers them |
| `<strong>`, `<em>` | important, emphasised |
| `<div>`, `<span>` | a box, a piece of text: no look of their own, used for layout and styling |
| `<br>` | a line break (no closing tag) |

`href`, `src` and `alt` are **attributes**: extra information inside the
opening tag, as `name="value"`. `alt` describes a picture for people who
can't see it, and for when it doesn't load: always include it.

Writing HTML from data is a loop that builds text:

```vyrn
main:
    let links = [("Vyrn", "https://vyrn.dev"), ("Wikipedia", "https://wikipedia.org")]
    var out = ["<ul>"]
    each name, url in links:
        out.append("  <li><a href=\"{url}\">{name}</a></li>")
    end
    out.append("</ul>")
    print("\n".join(out))
end
```

```text
<ul>
  <li><a href="https://vyrn.dev">Vyrn</a></li>
  <li><a href="https://wikipedia.org">Wikipedia</a></li>
</ul>
```

### Exercise

Write `table(rows)`, which gives an HTML table: `<table>`, then one `<tr>`
(table row) per row with a `<td>` (table cell) per value.

```vyrn starter
create table(rows: list) -> str:
    return ""
end

main:
    print(table([["Ada", 36], ["Alan", 41]]))
end
```

```text expect
<table>
<tr><td>Ada</td><td>36</td></tr>
<tr><td>Alan</td><td>41</td></tr>
</table>
```

> Hint: for each row, `"".join(["<td>{v}</td>" each v in row])`.
> Hint: put `<tr>` around that, and `<table>` lines around everything.

<details><summary>Solution</summary>

```vyrn solution
create table(rows: list) -> str:
    var lines = ["<table>"]
    each row in rows:
        let cells = "".join(["<td>{v}</td>" each v in row])
        lines.append("<tr>{cells}</tr>")
    end
    lines.append("</table>")
    return "\n".join(lines)
end

main:
    print(table([["Ada", 36], ["Alan", 41]]))
end
```

</details>

## 41.3 Text that could be HTML

What if the data has a `<` in it? A browser reads it as the start of a tag.
Someone could type `<script>` into your guestbook and run their code on
every visitor's computer; this attack is called **cross-site scripting**.
`html.escape` makes text safe by turning the special characters into
entities (Day 34 turned them back):

```vyrn
use vyrn.html

main:
    let comment = "I <3 this! <script>steal()</script>"
    print("<p>" + html.escape(comment) + "</p>")
    print(html.escape("Tom & \"Jerry\""))
end
```

```text
<p>I &lt;3 this! &lt;script&gt;steal()&lt;/script&gt;</p>
Tom &amp; &quot;Jerry&quot;
```

The rule: **escape everything that came from outside the program** before
putting it in a page. Day 44's templates do it for you.

## 41.4 Serving a page

A browser gets pages from a **web server**. In Vyrn a server is a set of
`route`s: each says which address it answers and gives back the page. Text
that starts with `<` is sent as HTML:

```vyrn
use vyrn.http

route GET "/" ():
    return "<!doctype html><title>Home</title><h1>Welcome</h1><p><a href=\"/about\">About me</a></p>"
end

route GET "/about" ():
    return "<!doctype html><title>About</title><h1>About me</h1><p>I'm learning Vyrn.</p>"
end

main:
    let r = http.test("GET", "/about")
    print(r.status, r.headers["content-type"])
    print(r.text)
    print(http.test("GET", "/nowhere").status)
end
```

```text
200 text/html; charset=utf-8
<!doctype html><title>About</title><h1>About me</h1><p>I'm learning Vyrn.</p>
404
```

`http.test` asks your own routes, without a network: perfect for trying
them out (and for tests, Day 48). To open the pages in a real browser, end
`main` with `http.serve(port = 8000)` instead and visit
`http://localhost:8000`:

```vyrn check
use vyrn.http

route GET "/" ():
    return "<!doctype html><title>Home</title><h1>Welcome</h1>"
end

main:
    print("open http://localhost:8000 (Ctrl+C stops it)")
    http.serve(port = 8000)
end
```

`localhost` means "this computer". Nobody else can see it yet; Day 59 puts
a site on the internet.

## 41.5 Project: your home page

The page is made from a map of facts, so changing a fact never means
editing HTML. Each part of the page is its own small function:

```vyrn
use vyrn.html
use vyrn.http

const ME = {
    "name": "Ada",
    "tagline": "Learning to code, one day at a time",
    "likes": ["maths", "music", "engines"],
    "links": [("GitHub", "https://github.com/ada"), ("Blog", "https://ada.dev")],
}

create bullet_list(items: list) -> str:
    return "<ul>" + "".join(["<li>{html.escape(str(i))}</li>" each i in items]) + "</ul>"
end

create link_list(links: list) -> str:
    return "<ul>" + "".join(["<li><a href=\"{html.escape(u)}\">{html.escape(n)}</a></li>" each n, u in links]) + "</ul>"
end

create home(me: map) -> str:
    let name = html.escape(me["name"])
    return "\n".join([
        "<!doctype html>",
        "<html><head><title>{name}</title></head><body>",
        "<h1>{name}</h1>",
        "<p><em>{html.escape(me["tagline"])}</em></p>",
        "<h2>Things I like</h2>",
        bullet_list(me["likes"]),
        "<h2>Find me</h2>",
        link_list(me["links"]),
        "</body></html>",
    ])
end

route GET "/" ():
    return home(ME)
end

main:
    let page = html.parse(http.test("GET", "/").text)
    print(page.title, "|", [li.text each li in page.select("li")])
    print([a.attr("href") each a in page.select("a")])
    write_text("index.html", home(ME))
end
```

```text
Ada | ["maths", "music", "engines", "GitHub", "Blog"]
["https://github.com/ada", "https://ada.dev"]
```

The program checks its own page by parsing it, then saves it as
`index.html`: open that file in a browser to see it. Put in your own facts.

### Exercise

Add a `"projects"` list to the facts, where each project is a map with a
`name` and a `year`, and write `project_list(projects)` that shows them
newest first as `<li>name (year)</li>`.

```vyrn starter
create project_list(projects: list) -> str:
    return "<ul></ul>"
end

main:
    print(project_list([{"name": "Snake", "year": 2025}, {"name": "Deal finder", "year": 2026}]))
end
```

```text expect
<ul><li>Deal finder (2026)</li><li>Snake (2025)</li></ul>
```

> Hint: `sorted(projects, key = p => p["year"], reverse = true)`

<details><summary>Solution</summary>

```vyrn solution
create project_list(projects: list) -> str:
    let newest = sorted(projects, key = p => p["year"], reverse = true)
    return "<ul>" + "".join(["<li>{p["name"]} ({p["year"]})</li>" each p in newest]) + "</ul>"
end

main:
    print(project_list([{"name": "Snake", "year": 2025}, {"name": "Deal finder", "year": 2026}]))
end
```

</details>

## Today you learned

- HTML is text with nested elements: tags, attributes, `<head>` and
  `<body>`.
- Build HTML from data with loops and small functions, and read it back with
  `html.parse`.
- `html.escape` everything that came from outside the program.
- A `route` that returns HTML is a web page; `http.test` tries it,
  `http.serve` opens it to a browser.
