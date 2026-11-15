# Day 50: A web app with a database

Today the pieces of this unit come together into a real web application: a
blog. Pages come from templates (Day 44), posts live in a database
(Day 38), new posts arrive through forms (Day 43), and only the author can
write (Day 49). You'll also learn how to organise a bigger app so it stays
easy to change, and how to test it from the outside.

## 50.1 How a web app is organised

A web app has three layers. Keep them apart and each stays simple:

| Layer | Job | Here |
|---|---|---|
| **data** | reading and writing the database | `all_posts`, `get_post`, `add_post` |
| **routes** | turning requests into calls, and results into responses | `route GET "/"`, ... |
| **templates** | turning data into HTML | `LIST`, `POST`, `LAYOUT` |

Routes stay short: they read the request, call the data layer, and hand
the result to a template. SQL appears only in the data layer, HTML only in
templates. Changing the page's look never touches the database code, and
moving to another database never touches the HTML.

## 50.2 The data layer

```vyrn
use vyrn.db
use vyrn.time

create setup(conn: any):
    conn.execute("create table if not exists posts (id integer primary key, title text, body text, slug text unique, created real)")
    conn.execute("create table if not exists comments (id integer primary key, post_id int, name text, text text)")
end

create slugify(title: str) -> str:
    let kept = "".join([c if c.isalnum() else " " each c in title.lower()])
    return "-".join(kept.split())
end

create add_post(conn: any, title: str, body: str, created: float) -> str:
    let slug = slugify(title)
    conn.insert("posts", {"title": title, "body": body, "slug": slug, "created": created})
    return slug
end

create all_posts(conn: any) -> list:
    return conn.query("select title, slug, created from posts order by created desc")
end

create get_post(conn: any, slug: str) -> map?:
    return conn.find_one("posts", {"slug": slug})
end

main:
    let conn = db.open(":memory:")
    setup(conn)
    print(add_post(conn, "Hello, World!", "My first post.", time.make(2026, 11, 1)))
    add_post(conn, "Learning SQL", "Joins are fun.", time.make(2026, 11, 8))
    print([p["slug"] each p in all_posts(conn)])
    print(get_post(conn, "learning-sql")["body"], get_post(conn, "nope"))
end
```

```text
hello-world
["learning-sql", "hello-world"]
Joins are fun. none
```

A **slug** is the title made safe for an address: `/posts/learning-sql`
reads much better than `/posts/2`, and people can guess what's behind it.

## 50.3 The pages

The templates share a layout. Dates are formatted before they reach the
template, which only fills gaps. `{{else}}` inside an `{{#each}}` is shown
when the list is empty:

```vyrn
use vyrn.template

const LAYOUT = r'<!doctype html><html><head><title>{{title}}</title></head><body><header><a href="/">My blog</a></header><main>{{{content}}}</main></body></html>'

const LIST = r'<h1>Posts</h1>{{#each posts}}<article><h2><a href="/posts/{{slug}}">{{title}}</a></h2><p class="date">{{date}}</p></article>{{else}}<p>Nothing yet.</p>{{/each}}'

create page(title: str, body: str, data: map) -> str:
    let content = template.render(body, data)
    return template.render(LAYOUT, {"title": title, "content": content})
end

main:
    let posts = [{"title": "Learning SQL", "slug": "learning-sql", "date": "8 Nov 2026"}]
    print(page("Posts", LIST, {"posts": posts}).split("<main>")[1])
    print(page("Posts", LIST, {"posts": []}).split("<main>")[1])
end
```

```text
<h1>Posts</h1><article><h2><a href="/posts/learning-sql">Learning SQL</a></h2><p class="date">8 Nov 2026</p></article></main></body></html>
<h1>Posts</h1><p>Nothing yet.</p></main></body></html>
```

A mistake in a template, such as an `{{/if}}` with no `{{#if}}`, is a
`TemplateError` naming the problem, so a broken page is found the first
time it's shown, which is one more reason to test the pages an app makes.

## 50.4 Project: the blog

The whole app: the list, a page per post with its comments, a comment
form anyone can use, and a new-post form only the author can use. The
author's session check is a single function, `is_author`, so a real login
(Day 49) can replace the simple one here.

```vyrn
use vyrn.db
use vyrn.html
use vyrn.http
use vyrn.template
use vyrn.time

const LAYOUT = r'<!doctype html><html><head><title>{{title}}</title></head><body><header><a href="/">My blog</a></header><main>{{{content}}}</main></body></html>'
const LIST = r'<h1>Posts</h1>{{#each posts}}<article><h2><a href="/posts/{{slug}}">{{title}}</a></h2><p class="date">{{date}}</p></article>{{else}}<p>Nothing yet.</p>{{/each}}'
const POST = r'<article><h1>{{title}}</h1><p class="date">{{date}}</p><div class="body">{{body}}</div></article><h2>{{count}} comments</h2>{{#each comments}}<div class="comment"><strong>{{name}}</strong>: {{text}}</div>{{/each}}<form method="post" action="/posts/{{slug}}/comments"><input name="name"><textarea name="text"></textarea><button>Comment</button></form>'

struct NotFound:
    message: str
end

create setup(conn: any):
    conn.execute("create table posts (id integer primary key, title text, body text, slug text unique, created real)")
    conn.execute("create table comments (id integer primary key, post_id int, name text, text text)")
end

create slugify(title: str) -> str:
    let kept = "".join([c if c.isalnum() else " " each c in title.lower()])
    return "-".join(kept.split())
end

create page(title: str, body: str, data: map) -> str:
    return template.render(LAYOUT, {"title": title, "content": template.render(body, data)})
end

create nice_date(t: float) -> str:
    return time.format(t, "%d %b %Y").lstrip("0")
end

create find_post(conn: any, slug: str) -> map:
    let post = conn.find_one("posts", {"slug": slug})
    if post == none: fail NotFound("no post called {slug}")
    return post
end

create is_author(request) -> bool:
    return request.cookies.get("session", "") == "author-session"
end

route GET "/" (conn):
    let posts = [p | {"date": nice_date(p["created"])} each p in conn.query("select * from posts order by created desc")]
    return page("My blog", LIST, {"posts": posts})
end

route GET "/posts/{slug}" (slug: str, conn):
    let post = find_post(conn, slug)
    let comments = conn.find("comments", {"post_id": post["id"]})
    return page(post["title"], POST, post | {"date": nice_date(post["created"]), "comments": comments, "count": len(comments)})
end

route POST "/posts/{slug}/comments" (slug: str, name: str, text: str, conn):
    let post = find_post(conn, slug)
    if name.strip() == "" or text.strip() == "": return http.Response(400, "Both a name and a comment, please.")
    conn.insert("comments", {"post_id": post["id"], "name": name.strip(), "text": text.strip()})
    return http.Response(303, "", {"Location": "/posts/{slug}"})
end

route POST "/posts" (title: str, body: str, request, conn, clock):
    if not is_author(request): return http.Response(403, "Only the author can post.")
    let slug = slugify(title)
    conn.insert("posts", {"title": title, "body": body, "slug": slug, "created": clock()})
    return http.Response(303, "", {"Location": "/posts/{slug}"})
end

main:
    let conn = db.open(":memory:")
    setup(conn)
    http.provide("conn", conn)
    http.provide("clock", fake(returns_each = [time.make(2026, 11, 1), time.make(2026, 11, 8)]))
    let author = {"Cookie": "session=author-session"}

    print(html.parse(http.test("GET", "/").text).select_one("main p").text)
    print(http.test("POST", "/posts", form = {"title": "Sneaky", "body": "x"}).status)
    print(http.test("POST", "/posts", form = {"title": "Hello, World!", "body": "First <b>post</b>."}, headers = author).headers["location"])
    http.test("POST", "/posts", form = {"title": "Learning SQL", "body": "Joins!"}, headers = author)

    let home = html.parse(http.test("GET", "/").text)
    print([a.text each a in home.select("article h2 a")], [d.text each d in home.select(".date")])

    print(http.test("POST", "/posts/hello-world/comments", form = {"name": "Alan", "text": "Nice!"}).status)
    let post = html.parse(http.test("GET", "/posts/hello-world").text)
    print(post.title, "|", post.select_one(".body").text, "|", post.select_one("h2").text, "|", post.select_one(".comment").text)
    print(http.test("GET", "/posts/missing").status)
end
```

```text
Nothing yet.
403
/posts/hello-world
["Learning SQL", "Hello, World!"] ["8 Nov 2026", "1 Nov 2026"]
303
Hello, World! | First <b>post</b>. | 1 comments | Alan: Nice!
404
```

Notice the post body: `First <b>post</b>.` shows the tags as text, because
`{{body}}` escapes it. That's the safe default. A blog that wants
formatting would let the author write in Markdown and convert it to HTML
on save, then use `{{{body}}}` only for that trusted HTML.

The clock is provided too, as a fake in the example, so the dates in the
output are the same every run; the real app provides `time.now`.

### Exercise

"1 comments" is clumsy. Add a `comment_word(n)` function giving `"1
comment"`, `"2 comments"` and `"No comments yet"`, for the template to use
instead of the count.

```vyrn starter
create comment_word(n: int) -> str:
    return "{n} comments"
end

main:
    print(comment_word(0), "|", comment_word(1), "|", comment_word(5))
end
```

```text expect
No comments yet | 1 comment | 5 comments
```

> Hint: three cases: `n == 0`, `n == 1`, and the rest.

<details><summary>Solution</summary>

```vyrn solution
create comment_word(n: int) -> str:
    if n == 0: return "No comments yet"
    if n == 1: return "1 comment"
    return "{n} comments"
end

main:
    print(comment_word(0), "|", comment_word(1), "|", comment_word(5))
end
```

</details>

## 50.5 Running it for real

The real `main` provides a file database and the real clock, and serves:

```vyrn check
use vyrn.db
use vyrn.http
use vyrn.time

main:
    let conn = db.open("blog.db")
    conn.execute("create table if not exists posts (id integer primary key, title text, body text, slug text unique, created real)")
    conn.execute("create table if not exists comments (id integer primary key, post_id int, name text, text text)")
    http.provide("conn", conn)
    http.provide("clock", time.now)
    http.serve(port = 8000)
end
```

### Exercise

Two posts with the same title get the same slug, and the second insert
fails (slugs are `unique`). Write `unique_slug(slug, taken)` that adds
`-2`, `-3`, ... until the slug isn't in the `taken` list.

```vyrn starter
create unique_slug(slug: str, taken: list[str]) -> str:
    return slug
end

main:
    print(unique_slug("hello", []), unique_slug("hello", ["hello"]), unique_slug("hello", ["hello", "hello-2"]))
end
```

```text expect
hello hello-2 hello-3
```

> Hint: start with `var candidate = slug` and `var n = 2`; loop while
> `candidate in taken`, setting `candidate = "{slug}-{n}"` and adding one
> to `n`.

<details><summary>Solution</summary>

```vyrn solution
create unique_slug(slug: str, taken: list[str]) -> str:
    var candidate = slug
    var n = 2
    loop candidate in taken:
        candidate = "{slug}-{n}"
        n += 1
    end
    return candidate
end

main:
    print(unique_slug("hello", []), unique_slug("hello", ["hello"]), unique_slug("hello", ["hello", "hello-2"]))
end
```

</details>

## Today you learned

- Keep a web app in three layers: data (SQL), routes (requests and
  responses), templates (HTML).
- Slugs make readable addresses; `unique` columns stop duplicates.
- `{{#each}}` ... `{{else}}` handles empty lists; test the pages your app
  produces with `html.parse`.
- `{{x}}` escapes what visitors and authors write; use `{{{x}}}` only for
  HTML you made and trust.
- Provide the clock and the database to the routes, so tests can use fakes.
