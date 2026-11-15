# Day 45: Web scraping

Not every website has an API. When the information is only on web pages,
a program can read the pages the way you do and pick out what it needs.
This is **web scraping**. Today you fetch pages, find things in them with
CSS selectors, follow links, and do it politely. The project collects every
book from a bookshop's catalogue into a CSV file.

## 45.1 Finding things in a page

`html.parse` turns a page into elements you can search. `select(css)`
gives every element a CSS selector matches (Day 42), `select_one` the
first. Each element has its `text`, its `tag`, its attributes with
`attr(name)`, and its own `select`, to search inside it:

```vyrn
use vyrn.html

const PAGE = r"""<html><body>
<h1>Bookshop</h1>
<article class="book"><h3><a href="catalogue/dune.html" title="Dune">Dune</a></h3><p class="price">£12.99</p><p class="stock">In stock</p></article>
<article class="book"><h3><a href="catalogue/emma.html" title="Emma">Emma</a></h3><p class="price">£7.50</p><p class="stock">Sold out</p></article>
</body></html>"""

main:
    let page = html.parse(PAGE)
    print(page.select_one("h1").text)
    each book in page.select("article.book"):
        let link = book.select_one("h3 a")
        print(link.attr("title"), "|", book.select_one(".price").text, "|", link.attr("href"))
    end
end
```

```text
Bookshop
Dune | £12.99 | catalogue/dune.html
Emma | £7.50 | catalogue/emma.html
```

The trick is finding the right selector. In a browser, right-click the
thing you want and choose **Inspect**: the developer tools show its tag,
classes and where it sits. Pick the shortest selector that finds exactly
what you want: a class like `.price` is better than a long path like
`body > div > article > p:nth-child(2)`, which breaks as soon as the page
changes a little.

## 45.2 From elements to data

Scraped text needs cleaning before it's useful: strip spaces, take off
currency signs, turn "In stock" into `true`. Do it in one function per
item, giving a clean map:

```vyrn
use vyrn.html

create book_from(el: any) -> map:
    let link = el.select_one("h3 a")
    let price = el.select_one(".price").text.strip().replace("£", "")
    return {
        "title": link.attr("title"),
        "price": float(price),
        "in_stock": "in stock" in el.select_one(".stock").text.lower(),
    }
end

main:
    let page = html.parse(r'<article class="book"><h3><a href="d.html" title="Dune">Dune</a></h3><p class="price"> £12.99 </p><p class="stock">In stock</p></article>')
    print([book_from(b) each b in page.select("article.book")])
end
```

```text
[{"title": "Dune", "price": 12.99, "in_stock": true}]
```

### Exercise

Pages often have tables. Write `table_rows(page)`, which gives each row of
the table's body as a map from the header names to the cell texts.

```vyrn starter
use vyrn.html

create table_rows(page: any) -> list:
    return []
end

main:
    let page = html.parse(r"<table><thead><tr><th>City</th><th>People</th></tr></thead><tbody><tr><td>Oslo</td><td>709000</td></tr><tr><td>Bergen</td><td>291000</td></tr></tbody></table>")
    print(table_rows(page))
end
```

```text expect
[{"City": "Oslo", "People": "709000"}, {"City": "Bergen", "People": "291000"}]
```

> Hint: `headers = [th.text each th in page.select("thead th")]`
> Hint: for each `tr` in `page.select("tbody tr")`, the cells are
> `[td.text each td in tr.select("td")]`; `dict(zip(headers, cells))` pairs them up.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.html

create table_rows(page: any) -> list:
    let headers = [th.text each th in page.select("thead th")]
    var rows = []
    each tr in page.select("tbody tr"):
        let cells = [td.text each td in tr.select("td")]
        rows.append(dict(zip(headers, cells)))
    end
    return rows
end

main:
    let page = html.parse(r"<table><thead><tr><th>City</th><th>People</th></tr></thead><tbody><tr><td>Oslo</td><td>709000</td></tr><tr><td>Bergen</td><td>291000</td></tr></tbody></table>")
    print(table_rows(page))
end
```

</details>

## 45.3 Following links

Links on a page are often **relative**: `catalogue/dune.html` means "next
to this page", `../index.html` "one folder up", `/about` "from the top of
the site". `http.join_url` turns them into full addresses, the way a
browser does:

```vyrn
use vyrn.http

main:
    let here = "https://books.toscrape.com/catalogue/page-2.html"
    print(http.join_url(here, "page-3.html"))
    print(http.join_url(here, "../index.html"))
    print(http.join_url(here, "/static/style.css"))
    print(http.join_url(here, "https://other.site/x"))
end
```

```text
https://books.toscrape.com/catalogue/page-3.html
https://books.toscrape.com/index.html
https://books.toscrape.com/static/style.css
https://other.site/x
```

A catalogue split over several pages has a "next" link at the bottom. A
scraper follows it until there isn't one:

```vyrn
use vyrn.html
use vyrn.http

const SITE = {
    "https://shop.example/page-1.html": r'<li class="book">Dune</li><li class="book">Emma</li><li class="next"><a href="page-2.html">next</a></li>',
    "https://shop.example/page-2.html": r'<li class="book">Ubik</li>',
}

create crawl(start: str, fetch: any) -> list:
    var titles = []
    var url = start
    loop url != none:
        let page = html.parse(fetch(url))
        titles.extend([b.text each b in page.select("li.book")])
        let next = page.select_one("li.next a")
        url = http.join_url(url, next.attr("href")) if next != none else none
    end
    return titles
end

main:
    print(crawl("https://shop.example/page-1.html", url => SITE[url]))
end
```

```text
["Dune", "Emma", "Ubik"]
```

As in the API days, fetching is passed in, so the crawler runs here on a
recorded site; for real, `fetch` is `url => http.get(url).text`.

## 45.4 Scraping politely

A scraper can ask for pages much faster than a person clicks. Be a good
guest:

1. **Check the rules.** Most sites have a `/robots.txt` file listing which
   paths programs should stay out of, and the site's terms may forbid
   scraping altogether. If there's an API, use it instead.
2. **Slow down.** Wait a second or so between pages (`time.sleep(1)`).
3. **Say who you are**, with a `User-Agent` header that names your program.
4. **Don't fetch the same page twice**: cache it (Day 36).
5. **Only take what you need**, and never collect people's personal
   information.

`robots.txt` is simple text. Here is a small reader for its most common
form, `Disallow` lines under `User-agent: *`:

```vyrn
create disallowed(robots: str) -> list:
    var rules = []
    var for_everyone = false
    each line in robots.lines():
        let parts = line.split("#")[0].split(":", 1)
        if len(parts) < 2: skip
        let field = parts[0].strip().lower()
        let value = parts[1].strip()
        if field == "user-agent": for_everyone = value == "*"
        if field == "disallow" and for_everyone and value != "": rules.append(value)
    end
    return rules
end

create allowed(robots: str, path: str) -> bool:
    return not any([path.startswith(rule) each rule in disallowed(robots)])
end

main:
    let robots = "User-agent: BadBot\nDisallow: /\n\nUser-agent: *\nDisallow: /checkout/\nDisallow: /account  # private\n"
    print(disallowed(robots))
    print(allowed(robots, "/catalogue/page-2.html"), allowed(robots, "/checkout/pay"))
end
```

```text
["/checkout/", "/account"]
true false
```

## 45.5 Project: the catalogue scraper

books.toscrape.com is a pretend bookshop made for practising scraping.
This program walks its whole catalogue, politely, and saves every book:

```vyrn check
use vyrn.csv
use vyrn.html
use vyrn.http
use vyrn.time

const START = "https://books.toscrape.com/catalogue/page-1.html"
const HEADERS = {"User-Agent": "vyrn-course-scraper (learning)"}

create book_from(el: any, page_url: str) -> map:
    let link = el.select_one("h3 a")
    return {
        "title": link.attr("title"),
        "price": float(el.select_one(".price_color").text.replace("£", "")),
        "rating": el.select_one("p.star-rating").classes[1],
        "url": http.join_url(page_url, link.attr("href")),
    }
end

main:
    var books = []
    var url = START
    loop url != none:
        let r = http.get(url, headers = HEADERS)
        if not r.ok:
            print("stopped: {url} said {r.status}")
            stop
        end
        let page = html.parse(r.text)
        books.extend([book_from(el, url) each el in page.select("article.product_pod")])
        print("{len(books)} books so far")
        let next = page.select_one("li.next a")
        url = http.join_url(url, next.attr("href")) if next != none else none
        time.sleep(1)
    end
    csv.write("books.csv", books)
    print("saved", len(books), "books")
end
```

It takes about a minute for the 50 pages. The rating is a class name on
the stars (`<p class="star-rating Three">`), so it's the second class.

### Exercise

The ratings are words. Write `stars(word)` turning `"One"` to `"Five"`
into 1 to 5, and use it to find the average rating of the books given.

```vyrn starter
create stars(word: str) -> int:
    return 0
end

main:
    let books = [{"rating": "Three"}, {"rating": "Five"}, {"rating": "One"}, {"rating": "Five"}]
    print(stars("Four"), sum([stars(b["rating"]) each b in books]) / len(books))
end
```

```text expect
4 3.5
```

> Hint: `["One", "Two", "Three", "Four", "Five"].index(word) + 1`

<details><summary>Solution</summary>

```vyrn solution
create stars(word: str) -> int:
    return ["One", "Two", "Three", "Four", "Five"].index(word) + 1
end

main:
    let books = [{"rating": "Three"}, {"rating": "Five"}, {"rating": "One"}, {"rating": "Five"}]
    print(stars("Four"), sum([stars(b["rating"]) each b in books]) / len(books))
end
```

</details>

## Today you learned

- `html.parse`, `select` and `select_one` find elements by CSS selector;
  `text`, `attr` and `classes` read them. Use the browser's Inspect to find
  selectors.
- Clean each item into a map at once; `http.join_url` resolves relative
  links; follow "next" links until there are none.
- Scrape politely: obey `robots.txt` and the site's terms, slow down, say
  who you are, cache, and take only what you need.
