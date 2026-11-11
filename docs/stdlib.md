# The Vyrn Standard Library

Status: v0.4. Where an API already runs in the interpreter it
says **runs today**; everything else is design, which this document fixes so
that implementation can follow it. Syntax follows `docs/grammar.ebnf` v0.4.

## 1. Three tiers

| Tier | How you get it | What belongs there |
|---|---|---|
| **Prelude** | Always in scope, no `use` | What a beginner needs in their first hour: core types, `print`, `len`, `open`, collections, tensor math |
| **`vyrn.*` modules** | `use vyrn.http`, `use vyrn.fs`, ... | Capabilities with a larger surface, a lifecycle or I/O policy: servers, memory mapping, task supervision, neural-network layers |
| **Packages** | `use app.storage (Db)` | Everything else, versioned by the package manager |

A name earns a place in the Prelude only if all four hold:

1. **Beginner-first.** A first script would otherwise need a `use` line for it.
2. **Free when unused.** It is a builtin in the VM's table or a type the
   compiler knows. Nothing is initialized or linked until a program calls it.
3. **No policy.** It takes no position on how to structure an application.
   `print` and `open` qualify. A router, a thread pool or a logging framework
   does not.
4. **Stable.** It will not need a breaking change. APIs still evolving live in
   a module, where a new version can sit beside the old one.

### Name resolution

Names resolve innermost first, so a program can always shadow the Prelude:

1. locals and parameters (innermost block first), then captured variables
2. top-level declarations of the file
3. names brought in by `use ... (a, b as c)`
4. the Prelude

`use vyrn.http` binds the module to the name `http` and nothing else. There
are no glob imports, so reading a file tells you where every name comes
from. `use vyrn.http as web` renames the module. **Runs today:** this order is
what `src/vm/compiler.rs` (`resolve`) implements, and `use vyrn.web` /
`use vyrn.http` with both import forms works.

## 2. The Prelude

### 2.1 Types

The Prelude's types are the language's own. The familiar long names map to
Vyrn's spelling as follows:

| Concept | Vyrn spelling | Notes |
|---|---|---|
| String | `str` | UTF-8, immutable, shared by reference count; slicing makes a view |
| List | `[T]` | Growable, shared by reference; `[T; N]` is a fixed array stored inline |
| Map | `{K: V}` | Insertion-ordered hash map |
| Tensor | `tensor[dtype; dims]` | `tensor[f32]` leaves the rank open; `vec2`/`vec3`/`vec4` are small tensors |
| Optional | `T?` | `none` or a `T`; `x?.f` and `x ?? default` |
| Numbers | `int` (`i8` to `u64`), `float` (`f16`, `bf16`, `f32`, `f64`), `bool` | |
| Tuples | `(A, B)` | `()` is the unit type |
| Tasks | `Task[T]`, `Channel[T]` | What an `async` function call and `channel()` return |
| Errors | `Error` and the built-in kinds (`IoError`, `KeyError`, `TypeError`, `NotFound`, ...) | Any struct can be passed to `fail` |

### 2.2 Functions

| Group | Names | Status |
|---|---|---|
| Input and output | `print`, `input(prompt)` | runs today |
| Conversion | `str`, `int`, `float`, `bool`, `list`, `dict`, `set`, `repr`, `chr`, `ord`, `hex`, `oct`, `bin`, `typeof` | runs today |
| Sequences | `len`, `range`, `enumerate`, `zip`, `reversed`, `any`, `all`, `sum`, and `sorted`, `min`, `max` with `key =` (and `reverse =` for `sorted`), `map(f, xs)`, `filter(f, xs)` | runs today |
| Math | `abs`, `round`, `sqrt`, `exp`, `tanh`, `floor`, `ceil`, `pow`, `divmod` | runs today |
| Text | `format(value, spec)`; the same specs inside strings: `"{price:.2f}"`, `"{n:,}"`, `"{name:>8}"`, `"{ratio:.1%}"`, `"{n:#x}"` | runs today |
| Files | `open(path, mode = "r")` returning a `File` (`read`, `readline`, `lines`, `write`, `write_line`, `close`; `each line in f`) for use with `using`; `read_text`, `write_text` | runs today |
| Data | `parse_json`, `to_json` | runs today |
| Tasks | `sleep`, `channel`, `join`, `now` | runs today |
| Games | `Clock(hz, frames)` | runs today |
| Tensors | `tensor`, `zeros`, `ones`, `rand`, `arange`, `eye`, `vec2`/`vec3`/`vec4`, `softmax`, `relu`, `gelu`, `argmax`, `@`, `memory`, `fit`, `set_memory_limit`, `format_bytes` | runs today |
| Weights | `load_weights`, `save_weights`, `load_tokenizer` | runs today; moving to `vyrn.nn` (section 6) |
| Devices | `cpu`, `gpu` (indexed: `gpu[0]`, `gpu[0 to 3]`) | runs today |

`open` belongs in the Prelude because reading a file is part of a first
script, and `using` already makes it safe:

```vyrn
using open("scores.txt") as f:
    each line in f.lines():
        print(line)
    end
end
```

`using` calls `close()` on every way out of the block, including failures
(**runs today**). `"w"` replaces a file and `"a"` adds to its end.

### 2.3 Methods of strings, lists, maps and sets

All **run today**. Methods that take a function (`map`, `filter`, `sort`
with `key =`, ...) are written in Vyrn itself, in `src/prelude.vyrn`.

| Type | Methods |
|---|---|
| `str` | `upper`, `lower`, `title`, `capitalize`, `swapcase`, `strip`/`lstrip`/`rstrip` (optionally of given characters), `split(sep, limit)`, `rsplit`, `lines`, `chars`, `join`, `replace`, `startswith`, `endswith`, `find`, `rfind`, `index`, `count`, `contains`, `partition`, `rpartition`, `removeprefix`, `removesuffix`, `zfill`, `ljust`, `rjust`, `center`, `repeat`, `format(spec)`, `isdigit`, `isalpha`, `isalnum`, `isspace`, `isupper`, `islower`, `is_empty`, `encode` |
| `[T]` | `append`, `extend`, `insert`, `pop`, `remove`, `clear`, `sort(key, reverse)`, `reverse`, `copy`, `index`, `count`, `contains`, `first`, `last`, `is_empty`, `unique`, `join(sep)`, `chunks(n)`, `sum`; taking a function: `map`, `filter`, `reduce`, `find`, `find_index`, `any`, `all`, `count_if`, `min_by`, `max_by`, `sum_by`, `group_by`, `each`, `take_while`, `drop_while`; and `flatten` |
| `{K: V}` | `keys`, `values`, `items`, `get(key, default)`, `pop(key, default)`, `update`, `setdefault`, `has`, `copy`, `clear`, `is_empty`; taking a function: `map_values`, `filter` |
| set | `{1, 2, 3}`, `{x each x in xs}` or `set(xs)`; `add`, `remove`, `discard`, `has`, `union`, `intersection`, `difference`, `symmetric_difference`, `issubset`, `issuperset`, `copy`, `clear`; the operators `|`, `&`, `-`, `^`; `in`; `len` |

A program may declare its own `map`, `sorted` or any other Prelude name;
its declaration wins.

## 3. `vyrn.http`: a FastAPI-style async router

`vyrn.http` turns the language's `route` declarations into a running server.
The design follows FastAPI's three ideas, adapted to a compiled language:

1. **The signature is the contract.** Parameters are filled from the path,
   then the query string, then the request body, then registered providers.
   Declared types are checked before the handler runs (a `400` otherwise),
   and the declared result type is the response schema.
2. **Dependency injection by name and type.** `http.provide("db", db)` makes
   every route parameter named `db` receive that value. Tests provide fakes
   the same way.
3. **Every handler is a task.** A route body is implicitly `async`; awaiting
   a database or a model never blocks other requests.

```vyrn
use vyrn.http

struct Item:
    id: int
    name: str
end

struct NotFound:
    id: int
end

route GET "/items/{id}" (id: int, db) -> Item fails NotFound:
    if id not in db: fail NotFound(id)
    return db[id]
end

route POST "/items" (body: Item, db) -> (int, Item):
    db[body.id] = body
    return 201, body
end

main:
    http.provide("db", {1: Item(1, "lamp")})
    http.serve(port = 8080)
end
```

**Runs today:** `route` and `routes` groups, path parameters converted to
their declared types, query parameters, a JSON `body` converted to the
declared struct (a missing field is a `400`), `http.provide`,
`http.serve(port, host, requests)`, handlers running as concurrent tasks,
and responses built from return values. A string is sent as `text/plain`,
`(status, value)` sets the status, `none` is `204`, anything else is JSON.
A failure is a JSON error whose status follows its kind: `NotFound` is 404,
`BadRequest` 400, `Unauthorized` 401, `Forbidden` 403, otherwise 500.
`vyrn.web` is the same module under its earlier name.

### 3.1 Design: the rest of the module

| API | Purpose |
|---|---|
| `http.Request` | A parameter of this type receives the whole request: `method`, `path`, `headers`, `query`, `body`, `client` |
| `http.Response(status, body, headers)` | Full control when a return value is not enough |
| `http.Router(prefix)` | A router value for composing apps from modules: `api.mount("/v1", users.router)` |
| `@http.middleware` | A function `(req: http.Request, next) -> http.Response` wrapping every route under it |
| `@require(...)`, `@rate_limit(...)` | Decorators are middleware with arguments, as in the `routes "/admin":` group of `examples/api.vyrn` |
| `route SSE` / `route WS` | Server-sent events and WebSockets. The handler takes a `stream` parameter and `await stream.send(x)` |
| `http.client` | `await http.get(url)`, `await http.post(url, json = x)`, for calling other services |
| `http.serve(app, port, workers)` | `workers > 1` runs one scheduler per OS thread, each with its own listener (SO_REUSEPORT) |

What stays out: templates, ORMs, sessions. Those are packages.

## 4. `vyrn.fs`: files, and memory mapping without copies

The Prelude's `open` covers reading and writing. `vyrn.fs` is for programs
where copies matter: model weights, datasets, logs larger than RAM.

```vyrn
use vyrn.fs

create count_errors(path: str) -> int:
    var errors = 0
    using fs.map(path) as data:
        each line in data.lines():
            if line.startswith("ERROR"): errors += 1
        end
    end
    return errors
end
```

`fs.map(path)` maps the file into the address space (`mmap`, or
`MapViewOfFile` on Windows) and returns a `Mapped` value. No byte is read
until it is touched, and nothing is copied:

- `data.lines()` yields each line as a `str` **view** into the mapping. A
  view is a borrowed slice: the borrow checker keeps it from outliving the
  `using` block. Store a line with `str(line)` to copy it out.
- `data.bytes(start, end)` is a `[u8]` view of a byte range.
- `fs.tensor_view(data, dtype, shape, offset)` is a read-only tensor whose
  storage is the mapping itself.

**Why this matters for large models.** A 14B-parameter model at 4 bits is
about 8 GB on disk. Reading it into a buffer and then uploading it costs
8 GB of RAM on top of the GPU copy. Mapping it means the pages stream from
the page cache straight into the GPU upload (`vyrn.nn` uses this, section 6),
and on a 32 GB machine nothing is held twice.

**Runs today:** `fs.exists`, `fs.is_file`, `fs.is_dir`, `fs.list(dir)`,
`fs.walk(dir)`, `fs.mkdir` (with parents), `fs.remove` (files and
directories), `fs.rename`, `fs.copy`, `fs.size`, `fs.modified`,
`fs.read_text`, `fs.write_text`, `fs.append_text`, `fs.read_lines`. A file
that is not there fails with `NotFound`. The mapping API below is design.

| API | Behaviour |
|---|---|
| `fs.map(path) -> Mapped` | Read-only mapping; `fails IoError` |
| `fs.map_mut(path, size) -> MappedMut` | Writable mapping; changes reach the file on `flush()` or at the end of `using` |
| `fs.read(path) -> [u8]`, `fs.write(path, data)` | Whole-file helpers |
| `fs.walk(dir)`, `fs.glob(pattern)`, `fs.stat(path)`, `fs.remove`, `fs.rename` | Directory work |
| `fs.watch(path)` | A `Channel` of change events, for `async for` |

On WebAssembly there is no `mmap`, so `fs.map` reads the file into linear
memory once. Programs keep the same code and lose only the laziness.

## 5. `vyrn.task`: green threads and supervision

**Runs today, in the language itself:** every `async` function call starts a
*green thread*. Tasks are stackful, so any function can `await`, and each
suspended task costs only its own registers and frames. A single-threaded
scheduler runs them with timers and channels, reports deadlocks, and warns
about failed tasks nobody awaited. The Prelude keeps the everyday part:
`spawn`, `await`, `async for`, `sleep`, `channel`, `join`.

`vyrn.task` adds the structure that larger programs need.

```vyrn
use vyrn.task

async create fetch(url: str) -> str:
    await sleep(0.1)
    return "page at {url}"
end

async create crawl(urls: [str]) -> [str]:
    # Structured concurrency: the group outlives none of its tasks. Tasks
    # still running at `end` are cancelled.
    using task.group() as g:
        each url in urls: g.spawn(fetch(url))
        return await g.wait()
    end
end
```

**Runs today:** `task.group`, `task.timeout`, `task.select`, `task.cancel`,
`task.done`, `task.sleep`. Awaiting a cancelled task fails with
`Cancelled`.

| API | Purpose |
|---|---|
| `task.group()` | Structured concurrency, as above: `g.spawn(work)`, `await g.wait()` (or `await g`) for every result, `g.cancel()` to stop the unfinished |
| `task.timeout(seconds, work)` | `await task.timeout(2.0, fetch(url))` gives the result, or fails with `Timeout` and stops the work |
| `task.select(a, b, ...)` | Waits for the first of several tasks or channels; `await` gives `(index, value)` |
| `task.cancel(t)`, `task.done(t)` | Stop a task; whether it has finished |
| `task.yield_now()` | Lets other tasks run inside a long computation |
| `task.current()` | The running task's handle, for names and cancellation checks |
| `channel(capacity = n)` | A bounded channel: `send` waits when it is full, which gives back-pressure |
| `task.Pool(workers = n)` | OS threads for CPU-bound work: `await pool.run(f, own data)`. Arguments must be passed with `own`, so no value is shared between threads unsynchronized |

The split follows the cost model. Green threads are cheap and share one OS
thread, so the Prelude has them. A `Pool` adds threads, and with them the
rule that values cross threads only by `own`. That is an explicit choice,
so it sits behind `use vyrn.task`.

## 5a. Everyday modules

All **run today** (`src/vm/stdlib.rs`).

| Module | Functions |
|---|---|
| (built in) | `read_text`, `write_text`, `append_text`, `read_bytes` / `write_bytes` (lists of 0 to 255), `open(path, mode)` |
| `vyrn.os` | `os.args()` (the arguments after the file name), `os.env(name, default)`, `os.set_env`, `os.environ()`, `os.cwd()`, `os.chdir`, `os.exit(code)`, `os.platform()`, `os.run(command, input)`: a list (`["git", "status"]`) or a shell line; gives `code`, `ok`, `out`, `err` |
| `vyrn.path` | `path.join(a, b, ...)`, `path.name`, `path.stem`, `path.ext`, `path.parent`, `path.absolute`, `path.home()` |
| `vyrn.time` | `time.now()` (seconds since 1970), `time.format(t, "%Y-%m-%d %H:%M:%S")`, `time.parse(text, pattern)`, `time.date(t)` (year, month, day, hour, minute, second, weekday), `time.monotonic()`, `time.sleep`. Times are UTC |
| `vyrn.random` | `random.int(low, high)` (both included), `random.float(low, high)`, `random.choice`, `random.shuffle` (in place), `random.sample(xs, k)`, `random.seed(n)` |
| `vyrn.math` | `sqrt`, `exp`, `log(x, base)`, `log2`, `log10`, `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `hypot`, `degrees`, `radians`, `floor`, `ceil`, `gcd`, `factorial`, `clamp`, `isnan`, `isinf`; constants `pi`, `e`, `tau`, `inf`, `nan` |
| `vyrn.json` | `json.parse(text)`, `json.dumps(value, indent)`, `json.load(path)`, `json.save(path, value)` |
| `vyrn.re` | `re.test(pattern, text)`, `re.find` (a match with `text`, `start`, `end`, `groups`, `named`, or `none`), `re.find_all`, `re.replace(pattern, text, with)` (`$1` inserts a group), `re.split` |
| `vyrn.csv` | `csv.read(path)` and `csv.parse(text)` (a list of maps from the header, or of lists with `header = false`), `csv.write(path, rows)`, `csv.format(rows)` |
| `vyrn.http` client | `http.get(url, headers)`, `http.post(url, body, headers)`, `http.put`, `http.delete`: a response with `status`, `ok`, `text`, `json`, `headers`. Maps and lists are sent as JSON |

## 5b. Application modules

All **run today**. Each module's file in `src/vm/` starts with an example.

### `vyrn.db`: SQLite databases

```vyrn
use vyrn.db

main:
    using db.open("shop.db") as conn:
        conn.execute("create table if not exists items (name text, price real)")
        conn.execute("insert into items values (?, ?)", ["pen", 1.5])
        each row in conn.query("select * from items where price < ?", [10]):
            print(row["name"], row["price"])
        end
    end
end
```

| API | Does |
|---|---|
| `db.open(path)` | A database file (made if missing); `":memory:"` lives only while the program runs |
| `conn.execute(sql, params)` | Runs a statement, gives the rows changed; several statements without params run as a script |
| `conn.execute_many(sql, rows)` | One statement per row of params |
| `conn.query(sql, params)` | Every row, as maps from column to value |
| `conn.query_one(...)`, `conn.value(...)` | The first row, or its first column; `none` when there is none |
| `conn.last_id()`, `conn.tables()` | The last inserted row id; the table names |
| `conn.begin()`, `conn.commit()`, `conn.rollback()`, `conn.close()` | Transactions; `using` closes |
| `conn.create_table(name, columns)` | A table from `{"name": "text", "age": "int"}`, plus an `id` column |
| `conn.insert(table, record)` | Adds a map or struct as a row; gives its `id` |
| `conn.all(table)`, `conn.find(table, where)`, `conn.find_one(table, where)` | Rows matching every `column: value` in `where` |
| `conn.update(table, where, changes)`, `conn.delete(table, where)` | How many rows changed; `delete` needs a `where` |

Params are a list for `?` or a map for `:name`, always bound separately
from the SQL, so input cannot change a statement. Errors are
`DatabaseError` with SQLite's message.

### `vyrn.draw`: pictures, games, 3D and windows

| API | Does |
|---|---|
| `draw.window(w, h, title)` | A window: `loop win.open():` ... `win.show()` (60 fps; gives seconds since the last frame) |
| `draw.canvas(w, h)` | The same surface without a window; `save("pic.png")`, `get_pixel(x, y)` |
| `clear`, `pixel`, `line`, `rect`, `circle`, `triangle`, `polygon`, `text`, `image` | Drawing on either; `fill = false` and `width` for outlines; colours by name, `"#rrggbb"`, `[r, g, b]` or `[r, g, b, a]` |
| `win.key_down(k)`, `win.key_pressed(k)` | Keys by name: `"left"`, `"space"`, `"a"`, `"1"`, ... |
| `win.mouse_x`, `win.mouse_y`, `win.mouse_down(button)`, `win.clicked()`, `win.typed()` | The mouse, and text typed this frame |
| `win.button(label, x, y)`, `win.input(name, x, y, w, hint)`, `win.checkbox(label, x, y, checked)`, `win.set_input(name, text)` | Widgets for apps: a button is true on the frame it is clicked; an input gives its text |
| `draw.turtle(surface)` | `forward`, `back`, `left`, `right`, `goto`, `home`, `pen_up`, `pen_down`, `color`, `width`, `circle`, `dot`, `write`, `begin_fill`, `end_fill` |
| `draw.image(path)` | A PNG, drawn with `surface.image(img, x, y, scale)` |
| `draw.camera(x, y, z)` | 3D: `move`, `move_to`, `forward`, `turn`, `look_at`, `set_fov` |
| `cube`, `sphere`, `plane`, `mesh`, `line3d` | 3D shapes, shaded, hidden behind nearer ones: `win.cube(cam, x, y, z, size, color, rotation = [rx, ry, rz])` |
| `draw.mesh(points, triangles)`, `draw.cube_mesh()`, `draw.sphere_mesh(detail)` | Shapes of your own |
| `draw.overlaps(x1, y1, w1, h1, x2, y2, w2, h2)`, `draw.rgb`, `draw.text_width` | Collisions, colours, text size |
| `win.box(cam, [x, y, z, w, h, d], colour)` | A box from the same lists `vyrn.game` collides with |
| `cam.direction`, `cam.position`, `cam.strafe(d)`, `cam.fly(d)` | First-person aiming and movement |
| `win.mouse_dx`, `win.mouse_dy`, `win.hide_cursor()`, `win.show_cursor()` | Mouse look |
| `draw.load_obj(path)` | A model exported from Blender and other 3D tools |

### `vyrn.game`: the maths of 3D games

Boxes are `[x, y, z, width, height, depth]` (centre and size); points
and directions are `[x, y, z]`.

| API | Does |
|---|---|
| `game.raycast(origin, direction, boxes, max_distance)` | The first box a ray hits: `{"index", "distance", "point"}`, or `none` (hitscan shots, line of sight) |
| `game.overlaps3d(a, b)` | Whether two boxes touch |
| `game.slide(position, movement, boxes, size)` | Moves without passing through boxes, sliding along walls |
| `game.distance(a, b)`, `game.normalize(v)` | Between points; a direction of length 1 |

`docs/course/game-dev.md` teaches game programming with them, and
`examples/fps.vyrn` is a complete small shooter.

Everything is drawn in software, so it looks the same on every machine.
`examples/snake.vyrn`, `examples/cube3d.vyrn` and `examples/tips.vyrn`
are a game, a 3D scene and a small app.

### `vyrn.html`: reading web pages

`html.parse(text)` gives a page; `page.select(css)` and
`page.select_one(css)` find elements by CSS selector (`"a[href]"`,
`"div.price"`, `"#main > p"`). An element has `text`, `raw_text`, `tag`,
`html`, `outer_html`, `id`, `classes`, `attrs`, `children`, `parent`,
`attr(name)`, `has_class(name)` and its own `select`. `page.title`,
`page.text`. `html.escape(text)`. Pages are read as browsers read them.

### `vyrn.template`: pages made from data

```vyrn
use vyrn.template

main:
    let page = template.render(r"<h1>{{title}}</h1>{{#each items}}<li>{{name}}</li>{{/each}}",
                               {"title": "Shop", "items": [{"name": "pen"}]})
    print(page)
end
```

`{{name}}` (HTML-escaped), `{{{raw}}}`, `{{a.b}}` (map keys and struct
fields), `{{#each list}}` with `{{this}}` and `{{@index}}`, `{{#if x}}` /
`{{else}}` / `{{/if}}`, `{{#unless x}}`, `{{! comment }}`.
`template.render_file(path, data)` reads the template from a file. Write
templates in raw strings `r"..."` or files: in ordinary strings `{...}`
interpolates. A route returning text that starts with `<` is sent as a web
page.

### `vyrn.table`: data in rows and columns

| API | Does |
|---|---|
| `table.read_csv(path)`, `table.from_csv(text)`, `table.from_rows(rows, columns)` | A table; numbers in CSV become numbers, empty cells `none` |
| `t.head(n)`, `t.tail(n)`, `t.row(i)`, `t.column(name)`, `t.columns`, `t.rows`, `t.shape`, `len(t)` | Looking at it; `print(t)` shows an aligned table |
| `t.where(column, op, value)` | Rows that match: `==`, `!=`, `<`, `<=`, `>`, `>=`, `contains`, `startswith`, `in` |
| `t.sort(column, descending)`, `t.select(columns)`, `t.add_column(name, values)` | New tables |
| `t.sum`, `t.mean`, `t.median`, `t.min`, `t.max`, `t.count`, `t.unique` | Of one column |
| `t.group_by(column, value, how)` | One row per group: `how` is sum, mean, median, min, max or count |
| `t.to_csv(path)`, `each row in t` | Writing, and rows as maps |

### `vyrn.chart`: charts

`chart.line(xs, ys)`, `chart.scatter(xs, ys)`, `chart.bar(labels,
values)`, `chart.hist(values, bins)`, `chart.pie(labels, values)`, each with
`title`, `x_label`, `y_label`, `width`, `height`. A map of name to values
draws several series with a legend. `c.save("chart.png")` or
`c.save("chart.svg")`; `c.svg()` gives the SVG text.

### `vyrn.mail`: sending email

`mail.send(to, subject, body, sender, server, port, user, password,
security, html)`: `to` is an address or a list; `security` is
`"starttls"` (port 587, the default), `"tls"` (465) or `"none"`. Keep
passwords out of code: `password = os.env("MAIL_PASSWORD")`.

## 6. Tensors: the Prelude and `vyrn.nn`

The line is **math versus models**.

**Prelude: tensor math.** The tensor type, constructors, element-wise
arithmetic, `@`, reductions and the standard element-wise functions
(`exp`, `tanh`, `relu`, `gelu`, `softmax`) are numeric primitives in the
same sense as `+` on integers. They compile to the VM's tensor instructions
(`MatMul`, `Place`, ...), which a `TensorBackend` executes per device. That
is the CPU reference or the wgpu GPU backend (**runs today**,
`cargo build --features gpu`). They have no state and no configuration, so
`x @ w` means the same thing in every program. Placement is part of the
language (`on gpu[0]:`, `x on gpu[1]`), not of a library.

**`vyrn.nn`: model building blocks.** Layers carry parameters and state,
model formats change, and sampling strategies are a matter of taste. These
need versioning, so they live in a module:

| API | Purpose |
|---|---|
| `nn.load(path, dtype = "q4", devices = gpu[0])` | Loads safetensors through `fs.map`, quantizes on the way if asked, and spreads layers over devices with `fit` |
| `nn.Linear`, `nn.Embedding`, `nn.RMSNorm`, `nn.Attention`, `nn.MLP` | Layers as structs: weights are fields, `forward` is a `kernel` function |
| `nn.KVCache(layers, max_tokens)` | Preallocated attention cache, placed with the layers |
| `nn.Tokenizer.load(path)` | Hugging Face `tokenizer.json` (BPE, byte-level and SentencePiece) |
| `nn.sample(logits, temperature, top_k, top_p)` | Next-token sampling |
| `nn.generate(model, tokens, max_new)` | An `async` stream of tokens, for `async for` |

`load_weights`, `save_weights` and `load_tokenizer` run today as Prelude
functions. They move to `vyrn.nn` as `nn.load_weights`, `nn.save_weights` and
`nn.Tokenizer.load`, with the Prelude names kept as aliases for one release.

## 7. Two complete programs

### 7.1 A first script: no `use` at all

Everything here is Prelude: types, `open`, `using`, string methods, lists,
maps, tensors.

```vyrn
# grades.vyrn: read "name,score" lines and summarize them.

main:
    scores = {}
    using open("grades.csv") as f:
        each line in f.lines():
            let parts = line.split(",")
            scores[parts[0]] = float(parts[1])
        end
    end

    let values = tensor(scores.values())
    print("students: {len(scores)}")
    print("average:  {round(values.sum() / len(scores), 1)}")

    each name, score in scores.items():
        let grade = "A" if score >= 90 else "B" if score >= 80 else "C"
        print("{name}: {score} ({grade})")
    end
end
```

### 7.2 An inference server: explicit dependencies

The `use` lines name exactly what the program depends on beyond the
Prelude. The model is memory-mapped and spread across GPUs, each request is
a task, and generation streams over server-sent events.

```vyrn
use vyrn.http
use vyrn.fs
use vyrn.task
use vyrn.nn
use app.auth (require_key)

struct Prompt:
    text: str
    max_tokens: int = 128
end

struct Health:
    model: str
    gpu_memory: str
end

const MODEL = "models/sml-14b-q4.safetensors"

route GET "/health" (model) -> Health:
    return Health(MODEL, format_bytes(memory_used(gpu[0])))
end

routes "/v1":
    @require_key
    route POST "/complete" (body: Prompt, model, tokenizer) -> str:
        let tokens = tokenizer.encode(body.text)
        let output = await task.timeout(30.0, model.complete(tokens, body.max_tokens))
        return tokenizer.decode(output)
    end

    @require_key
    route SSE "/stream" (body: Prompt, model, tokenizer, stream):
        let tokens = tokenizer.encode(body.text)
        async each token in nn.generate(model, tokens, body.max_tokens):
            await stream.send(tokenizer.decode([token]))
        end
    end
end

main:
    # Pages stream from disk to the GPUs; nothing is held twice in RAM.
    let model = nn.load(MODEL, dtype = "q4", devices = gpu[0 to 1])
    http.provide("model", model)
    http.provide("tokenizer", nn.Tokenizer.load("models/tokenizer.json"))
    http.serve(port = 8080)
end
```

## 8. Implementation status

| Piece | Where | Status |
|---|---|---|
| Prelude functions and types | `src/vm/builtins.rs`, `src/vm/stdlib.rs`, `src/prelude.vyrn` | runs |
| Files, sets, format specs, methods (section 2) | `src/vm/stdlib.rs`, `src/vm/builtins.rs` | runs |
| `vyrn.os`, `vyrn.path`, `vyrn.time`, `vyrn.random`, `vyrn.math`, `vyrn.json`, `vyrn.re`, `vyrn.csv`, HTTP client | `src/vm/stdlib.rs` | runs |
| `vyrn.db`, `vyrn.draw`, `vyrn.html`, `vyrn.template`, `vyrn.table`, `vyrn.chart`, `vyrn.mail` | `src/vm/lib_*.rs` | runs |
| `use vyrn.<module>` with module and name imports | `src/vm/compiler.rs` (`use_decl`, `module_member`) | runs |
| `vyrn.http` routing, providers, server | `src/vm/machine.rs` (`resolve_route`, `service_servers`), `src/vm/web.rs` | runs; `Request`, `Response`, `Router`, middleware, SSE/WS are design |
| Green threads, channels, timers | `src/vm/machine.rs`, `src/vm/task.rs` | runs |
| `vyrn.task` groups, timeouts, select, cancel | `src/vm/stdlib.rs`, `src/vm/machine.rs` (`await_race`, `apply_cancels`) | runs; pools, bounded channels, `yield_now` are design |
| `vyrn.fs` files and directories | `src/vm/stdlib.rs` | runs |
| `vyrn.fs` mapping and views | | design; views depend on borrow checking of slices (`src/check.rs`) |
| Tensor math on CPU and GPU | `src/vm/tensor.rs`, `src/vm/gpu.rs` | runs |
| `vyrn.nn` | `src/vm/weights.rs`, `src/vm/tokenizer.rs` hold its first pieces | design |

Build order from here: `vyrn.fs.map` with views (the large-model path), then
`vyrn.nn.load` and `generate` on top of it, then the rest of `vyrn.http`, then
`task.Pool`.
