# Day 42: Styling pages with CSS

Yesterday's pages had the browser's plain black-on-white look. **CSS**
(Cascading Style Sheets) is the language that says how a page looks:
colours, fonts, spacing, sizes. HTML says *what* things are; CSS says *how
they look*. Today you write CSS, serve it from your own server, and build
it from a theme, so the whole site can change colour by changing one map.

## 42.1 Rules

A CSS **rule** picks elements with a **selector**, then sets
**properties** inside braces:

```text
h1 {
  color: #1e88e5;
  font-family: Georgia, serif;
}

p {
  line-height: 1.6;
  max-width: 40em;
}
```

The selectors you'll use most:

| Selector | Picks |
|---|---|
| `h1` | every `<h1>` |
| `.card` | every element with `class="card"` |
| `#top` | the element with `id="top"` |
| `nav a` | every `<a>` inside a `<nav>` |
| `a:hover` | a link while the mouse is over it |

Classes are how you style *some* elements: `<p class="note">` is styled by
`.note { ... }`, and an element can have several, `class="card wide"`.

CSS goes in a `<style>` element in the `<head>`, or better, in its own file
that every page links to, so one change restyles the whole site:

```text
<link rel="stylesheet" href="/style.css">
```

`html.parse` reads classes too:

```vyrn
use vyrn.html

main:
    let page = html.parse(r'<div class="card wide" id="top"><p class="note">Hi</p><p>Bye</p></div>')
    let card = page.select_one("#top")
    print(card.classes, card.has_class("wide"))
    print([p.text each p in page.select(".card .note")])
end
```

```text
["card", "wide"] true
["Hi"]
```

The same selectors pick elements in CSS and in `select`: you'll use this
again for web scraping (Day 45).

## 42.2 Colours, sizes and fonts

```text
body {
  background: #fafafa;            /* a colour: #rrggbb, or a name like white */
  color: rgb(33, 33, 33);         /* the text colour */
  font-family: system-ui, sans-serif;
  font-size: 18px;                /* px: screen pixels */
}
h1 { font-size: 2.5rem; }         /* rem: times the page's font size */
.note { width: 50%; }             /* %: of the space it's in */
```

Text inside `/* */` is a comment. **Colours** are the same `#rrggbb` you
used in `vyrn.draw`: red, green and blue from `00` to `ff`. Vyrn can work
them out, for example to make a lighter shade of a theme colour:

```vyrn
create lighter(color: str, amount: float) -> str:
    var out = "#"
    each i in [1, 3, 5]:
        let c = int(color[i:i + 2], 16)
        let mixed = round(c + (255 - c) * amount)
        out += "{mixed:02x}"
    end
    return out
end

main:
    print(lighter("#1e88e5", 0.5))
    print(lighter("#000000", 0.25), lighter("#ff0000", 1))
end
```

```text
#8ec4f2
#404040 #ffffff
```

`int(text, 16)` reads hexadecimal, and `{x:02x}` writes it, two digits.

## 42.3 The box model

Every element is a **box**. From the inside out: the **content**, then
**padding** (space inside the border), the **border**, and the **margin**
(space outside, between boxes):

```text
.card {
  padding: 16px;              /* inside */
  border: 1px solid #ddd;     /* width, style, colour */
  border-radius: 8px;         /* rounded corners */
  margin: 12px 0;             /* outside: 12px top and bottom, 0 at the sides */
  box-shadow: 0 2px 6px rgba(0, 0, 0, 0.15);
}
```

Two numbers mean top-and-bottom then sides; four go clockwise from the top:
`margin: 1px 2px 3px 4px`. Press F12 in a browser and click an element to
see its boxes drawn over the page.

## 42.4 Serving a stylesheet

The stylesheet is just another route. Text that doesn't start with `<` is
sent as plain text, so give the right **content type** with
`http.Response`, or the browser ignores it:

```vyrn
use vyrn.http

const CSS = "body \{ font-family: system-ui; background: #fafafa; \}\nh1 \{ color: #1e88e5; \}"

route GET "/style.css" ():
    return http.Response(200, CSS, {"Content-Type": "text/css"})
end

route GET "/" ():
    return "<!doctype html><link rel=\"stylesheet\" href=\"/style.css\"><h1>Styled</h1>"
end

main:
    let r = http.test("GET", "/style.css")
    print(r.status, r.headers["content-type"])
    print(r.text)
end
```

```text
200 text/css
body { font-family: system-ui; background: #fafafa; }
h1 { color: #1e88e5; }
```

Inside an ordinary Vyrn string `{` starts interpolation, so CSS braces are
written `\{` and `\}`, or the CSS goes in a raw string `r"..."` or a file.

### Exercise

Write `rule(selector, props)`, which turns a selector and a map of
properties into a CSS rule on one line.

```vyrn starter
create rule(selector: str, props: map) -> str:
    return ""
end

main:
    print(rule("h1", {"color": "#1e88e5", "font-size": "2rem"}))
    print(rule(".note", {"padding": "8px"}))
end
```

```text expect
h1 { color: #1e88e5; font-size: 2rem; }
.note { padding: 8px; }
```

> Hint: `"; ".join(["{k}: {v}" each k, v in props.items()])` gives the
> middle.
> Hint: the braces: `"\{"` and `"\}"`.

<details><summary>Solution</summary>

```vyrn solution
create rule(selector: str, props: map) -> str:
    let body = " ".join(["{k}: {v};" each k, v in props.items()])
    return selector + " \{ " + body + " \}"
end

main:
    print(rule("h1", {"color": "#1e88e5", "font-size": "2rem"}))
    print(rule(".note", {"padding": "8px"}))
end
```

</details>

## 42.5 Project: a themed site

The theme is a map of colours and fonts. One function turns it into the
stylesheet, using the `rule` function from the exercise, and a **dark
mode**: a `@media` block whose rules only apply when the computer is set to
dark:

```vyrn
use vyrn.http

const THEME = {"accent": "#1e88e5", "text": "#212121", "page": "#fafafa", "font": "system-ui, sans-serif"}

create rule(selector: str, props: map) -> str:
    let body = " ".join(["{k}: {v};" each k, v in props.items()])
    return selector + " \{ " + body + " \}"
end

create stylesheet(t: map) -> str:
    return "\n".join([
        rule("body", {"font-family": t["font"], "color": t["text"], "background": t["page"], "max-width": "40em", "margin": "2em auto"}),
        rule("h1, h2", {"color": t["accent"]}),
        rule("a", {"color": t["accent"]}),
        rule(".card", {"padding": "16px", "border-radius": "8px", "box-shadow": "0 2px 6px rgba(0,0,0,.15)"}),
        "@media (prefers-color-scheme: dark) \{",
        "  " + rule("body", {"color": t["page"], "background": t["text"]}),
        "\}",
    ])
end

route GET "/style.css" ():
    return http.Response(200, stylesheet(THEME), {"Content-Type": "text/css"})
end

main:
    print(http.test("GET", "/style.css").text)
end
```

```text
body { font-family: system-ui, sans-serif; color: #212121; background: #fafafa; max-width: 40em; margin: 2em auto; }
h1, h2 { color: #1e88e5; }
a { color: #1e88e5; }
.card { padding: 16px; border-radius: 8px; box-shadow: 0 2px 6px rgba(0,0,0,.15); }
@media (prefers-color-scheme: dark) {
  body { color: #fafafa; background: #212121; }
}
```

Add `<link rel="stylesheet" href="/style.css">` to the head of yesterday's
home page, put the sections in `<div class="card">`, and serve both routes
with `http.serve(port = 8000)`. Change `"accent"` and every heading and
link changes with it.

`margin: 2em auto` is a classic trick: `auto` sides share the leftover
space, which centres the page.

### Exercise

Add a `"radius"` to the theme and use it for `.card`'s `border-radius`, so
a theme can choose square or round corners. Write `card_rule(theme)`.

```vyrn starter
create card_rule(t: map) -> str:
    return ".card \{ border-radius: 8px; \}"
end

main:
    print(card_rule({"radius": "0"}))
    print(card_rule({"radius": "16px"}))
end
```

```text expect
.card { border-radius: 0; }
.card { border-radius: 16px; }
```

> Hint: `".card \{ border-radius: " + t["radius"] + "; \}"`

<details><summary>Solution</summary>

```vyrn solution
create card_rule(t: map) -> str:
    return ".card \{ border-radius: " + t["radius"] + "; \}"
end

main:
    print(card_rule({"radius": "0"}))
    print(card_rule({"radius": "16px"}))
end
```

</details>

## Today you learned

- CSS rules pick elements with selectors (`h1`, `.class`, `#id`, `a b`) and
  set properties.
- Colours are `#rrggbb`; sizes are `px`, `rem` and `%`; every element is a
  box with padding, border and margin.
- Serve a stylesheet with `Content-Type: text/css`, and link it from every
  page.
- Write `\{` and `\}` for braces in Vyrn strings, and build CSS from a theme
  map so the look lives in one place.
