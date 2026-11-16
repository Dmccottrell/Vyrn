# Day 44: Templates

Building pages by joining strings works, but it gets messy fast: HTML mixed
into code, quotes inside quotes, and one forgotten `html.escape` is a
security hole. A **template** is the page written as HTML, with gaps for
the data. `vyrn.template` fills the gaps, and escapes everything by
default. The project generates a whole multi-page portfolio site from one
data file.

## 44.1 Filling gaps

`{{name}}` is a gap. `template.render(text, data)` fills each one from a
map (or a struct's fields):

```vyrn
use vyrn.template

main:
    let text = r"<h1>{{title}}</h1><p>By {{author.name}}, {{year}}</p>"
    print(template.render(text, {"title": "Dune", "year": 1965, "author": {"name": "Frank Herbert"}}))
    print(template.render(r"<p>{{comment}}</p>", {"comment": "<script>steal()</script>"}))
    print(template.render(r"<div>{{{trusted}}}</div>", {"trusted": "<em>mine</em>"}))
end
```

```text
<h1>Dune</h1><p>By Frank Herbert, 1965</p>
<p>&lt;script&gt;steal()&lt;/script&gt;</p>
<div><em>mine</em></div>
```

- `{{author.name}}` goes into maps and structs.
- `{{x}}` is **escaped**: safe for anything a visitor typed.
- `{{{x}}}` (three braces) is inserted as it is: only for HTML your own
  program made.

Templates use `{{ }}`, and Vyrn strings use `{ }` for interpolation, so
write templates in raw strings (`r"..."`, or `r"""..."""` for several
lines) or, better, in their own files.

## 44.2 Lists and choices

`{{#each list}}...{{/each}}` repeats its inside for every item; inside it,
names are looked up in the item first, `{{this}}` is the item itself and
`{{@index}}` its position. `{{#if x}}...{{else}}...{{/if}}` chooses;
`{{#unless x}}` is the opposite. Anything "empty" (`false`, `none`, `0`,
`""`, `[]`) counts as false:

```vyrn
use vyrn.template

const LIST = r"<h2>{{title}}</h2>{{#if items}}<ol>{{#each items}}<li>{{name}}{{#if done}} ✓{{/if}}</li>{{/each}}</ol>{{else}}<p>Nothing to do!</p>{{/if}}"

main:
    let todos = [{"name": "Learn templates", "done": true}, {"name": "Build a site", "done": false}]
    print(template.render(LIST, {"title": "Today", "items": todos}))
    print(template.render(LIST, {"title": "Tomorrow", "items": []}))
    print(template.render(r"{{#each tags}}#{{this}} {{/each}}", {"tags": ["vyrn", "web"]}))
end
```

```text
<h2>Today</h2><ol><li>Learn templates ✓</li><li>Build a site</li></ol>
<h2>Tomorrow</h2><p>Nothing to do!</p>
#vyrn #web 
```

### Exercise

Write a template for a scoreboard: a `<table>` with one row per player
showing the position (counting from 1), the name and the score, and a
`<p>No players yet</p>` when the list is empty. The positions come from
the data, so number the players before rendering.

```vyrn starter
use vyrn.template

const BOARD = r"<table></table>"

main:
    let players = [{"name": "Ada", "score": 30}, {"name": "Alan", "score": 25}]
    let numbered = players
    print(template.render(BOARD, {"players": numbered}))
    print(template.render(BOARD, {"players": []}))
end
```

```text expect
<table><tr><td>1</td><td>Ada</td><td>30</td></tr><tr><td>2</td><td>Alan</td><td>25</td></tr></table>
<p>No players yet</p>
```

> Hint: `numbered = [p | {"place": i + 1} each i, p in enumerate(players)]`
> adds a `place` to each map.
> Hint: `{{#if players}}<table>{{#each players}}<tr>...</tr>{{/each}}</table>{{else}}...{{/if}}`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.template

const BOARD = r"{{#if players}}<table>{{#each players}}<tr><td>{{place}}</td><td>{{name}}</td><td>{{score}}</td></tr>{{/each}}</table>{{else}}<p>No players yet</p>{{/if}}"

main:
    let players = [{"name": "Ada", "score": 30}, {"name": "Alan", "score": 25}]
    let numbered = [p | {"place": i + 1} each i, p in enumerate(players)]
    print(template.render(BOARD, {"players": numbered}))
    print(template.render(BOARD, {"players": []}))
end
```

</details>

## 44.3 Template files and layouts

Real templates live in files, usually in a `templates` folder, where your
editor colours them as HTML. `template.render_file(path, data)` reads one.

Every page of a site shares the same top and bottom: the `<head>`, the
menu, the footer. Put that in a **layout** template with a gap for the
page's own content, render the page first, then put it in the layout with
`{{{content}}}` (three braces: it's HTML your program made):

```vyrn
use vyrn.fs
use vyrn.template

const LAYOUT = r"""<!doctype html>
<html><head><title>{{title}} | My site</title></head>
<body><nav>{{#each menu}}<a href="{{url}}">{{label}}</a> {{/each}}</nav>
<main>{{{content}}}</main>
<footer>Made with Vyrn</footer></body></html>"""

const ABOUT = r"<h1>About</h1><p>{{bio}}</p>"

const MENU = [{"url": "/", "label": "Home"}, {"url": "/about", "label": "About"}]

create page(name: str, title: str, data: map) -> str:
    let content = template.render_file("templates/{name}.html", data)
    return template.render_file("templates/layout.html", {"title": title, "menu": MENU, "content": content})
end

main:
    fs.mkdir("templates")
    write_text("templates/layout.html", LAYOUT)
    write_text("templates/about.html", ABOUT)
    print(page("about", "About", {"bio": "I like engines & music."}))
end
```

```text
<!doctype html>
<html><head><title>About | My site</title></head>
<body><nav><a href="/">Home</a> <a href="/about">About</a> </nav>
<main><h1>About</h1><p>I like engines &amp; music.</p></main>
<footer>Made with Vyrn</footer></body></html>
```

Change the layout once, and every page changes.

## 44.4 Templates in routes

A route returning text that starts with `<` is a page, so a route is just
`render_file` with the right data:

```vyrn check
use vyrn.http
use vyrn.template

route GET "/" ():
    return template.render_file("templates/home.html", {"name": "Ada"})
end

route GET "/hello/{name}" (name: str):
    return template.render(r"<h1>Hello, {{name}}!</h1>", {"name": name})
end

main:
    http.serve(port = 8000)
end
```

`/hello/<script>` shows the text `<script>` safely, because `{{name}}`
escapes it.

## 44.5 Project: a portfolio site generator

Websites don't always need a server. A **static site** is a folder of
finished HTML files, which free hosts like GitHub Pages serve as they are.
This generator reads the projects from a JSON file and writes a home page
plus one page per project:

```vyrn
use vyrn.fs
use vyrn.json
use vyrn.template

const LAYOUT = r'<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>{{title}}</title></head><body><nav><a href="index.html">Home</a></nav>{{{content}}}</body></html>'
const HOME = r'<h1>{{name}}</h1><p>{{intro}}</p><ul>{{#each projects}}<li><a href="{{slug}}.html">{{title}}</a> ({{year}})</li>{{/each}}</ul>'
const PROJECT = r'<h1>{{title}}</h1><p>{{summary}}</p>{{#if tags}}<p>{{#each tags}}<span class="tag">{{this}}</span> {{/each}}</p>{{/if}}'

create slug(title: str) -> str:
    return "-".join(title.lower().split())
end

create build(data: map, out: str) -> list:
    fs.mkdir(out)
    let projects = [p | {"slug": slug(p["title"])} each p in sorted(data["projects"], key = p => p["year"], reverse = true)]
    var written = []
    let home = template.render(HOME, data | {"projects": projects})
    write_text("{out}/index.html", template.render(LAYOUT, {"title": data["name"], "content": home}))
    written.append("index.html")
    each p in projects:
        let body = template.render(PROJECT, p)
        write_text("{out}/{p["slug"]}.html", template.render(LAYOUT, {"title": p["title"], "content": body}))
        written.append(p["slug"] + ".html")
    end
    return written
end

main:
    json.save("portfolio.json", {
        "name": "Ada's projects",
        "intro": "Things I built while learning Vyrn.",
        "projects": [
            {"title": "Snake Game", "year": 2025, "summary": "The classic, with a high-score table.", "tags": ["games"]},
            {"title": "Deal Finder", "year": 2026, "summary": "Emails me when games go on sale.", "tags": ["apis", "databases"]},
        ],
    })
    print(build(json.load("portfolio.json"), "site"))
    print(read_text("site/index.html"))
    print(read_text("site/deal-finder.html").split("<body>")[1])
end
```

```text
["index.html", "deal-finder.html", "snake-game.html"]
<!doctype html><html><head><meta name="viewport" content="width=device-width, initial-scale=1"><title>Ada&#39;s projects</title></head><body><nav><a href="index.html">Home</a></nav><h1>Ada&#39;s projects</h1><p>Things I built while learning Vyrn.</p><ul><li><a href="deal-finder.html">Deal Finder</a> (2026)</li><li><a href="snake-game.html">Snake Game</a> (2025)</li></ul></body></html>
<nav><a href="index.html">Home</a></nav><h1>Deal Finder</h1><p>Emails me when games go on sale.</p><p><span class="tag">apis</span> <span class="tag">databases</span> </p></body></html>
```

Open `site/index.html` in a browser and click through. Adding a project is
now one entry in the JSON file and running the generator again. (`&#39;`
is the escaped `'`, which the browser shows normally.)

### Exercise

Add a "next project" link to the bottom of each project page. Write
`with_next(projects)`, which gives each project map a `next` key holding
the following project's slug, and the first project's slug for the last
one, so the links go round in a circle.

```vyrn starter
create with_next(projects: list) -> list:
    return projects
end

main:
    let ps = [{"slug": "a"}, {"slug": "b"}, {"slug": "c"}]
    print([p["next"] each p in with_next(ps)])
end
```

```text expect
["b", "c", "a"]
```

> Hint: the project after position `i` is at `(i + 1) % len(projects)`.
> Hint: `p | {"next": ...}` makes a copy with the extra key.

<details><summary>Solution</summary>

```vyrn solution
create with_next(projects: list) -> list:
    let n = len(projects)
    return [p | {"next": projects[(i + 1) % n]["slug"]} each i, p in enumerate(projects)]
end

main:
    let ps = [{"slug": "a"}, {"slug": "b"}, {"slug": "c"}]
    print([p["next"] each p in with_next(ps)])
end
```

</details>

## Today you learned

- `template.render(text, data)` fills `{{gaps}}`, escaping them; `{{{x}}}`
  inserts your own HTML as it is.
- `{{#each}}`, `{{#if}}`/`{{else}}` and `{{#unless}}` repeat and choose.
- Keep templates in files and share a layout with a `{{{content}}}` gap.
- A static site is a folder of HTML files: generate it from data.
