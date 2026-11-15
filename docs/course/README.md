# Learn Vyrn: 84 Days of Code

A complete course for people who have never written code, from your first
`print` to web apps, data analysis, games, 3D, concurrency and
professional tooling. One day is about an hour: several short lessons,
each explaining one idea from scratch and ending with an exercise.

## How to take the course

The course is built into Vyrn:

```bash
vyrn learn
```

```bash
vyrn learn next
```

`vyrn learn next` opens the next lesson and writes its exercise file into
`vyrn-course/`. Open that file in any editor (VS Code with the Vyrn
extension is recommended), write your answer, then:

```bash
vyrn learn check 1.2
```

Vyrn runs your program and compares what it prints with the goal. If it
differs, it shows the first difference. `vyrn learn hint 1.2` gives
one nudge at a time, and `vyrn learn solution 1.2` shows an answer. Your
progress is saved in `vyrn-course/progress.txt`.

You can also read the lessons here, one file per day: `day01.md`,
`day02.md`, ... Every example and every solution in them is run by the
test suite, so what you read is what Vyrn really prints.

## The syllabus

Days marked ✅ are written as lessons with exercises; the rest follow the
plan below.

### Unit 1: The basics (days 1–10)

| Day | Topic | Project |
|---|---|---|
| 1 ✅ | Programs, `print`, strings, comments, errors, `input`, variables | Name generator |
| 2 ✅ | Types (`int`, `float`, `str`, `bool`, `none`), arithmetic, converting, formatting | Tip calculator |
| 3 ✅ | `if` / `else if` / `else`, comparisons, `and` / `or` / `not`, `when` (switch) | Cave adventure |
| 4 ✅ | Modules, random numbers, lists, slices, 2D grids | Rock, paper, scissors |
| 5 ✅ | `each`, ranges, accumulators, `loop`, `stop` / `skip`, nested loops | Password generator |
| 6 ✅ | Functions, parameters, `return`, defaults, named arguments, scope | Grid robot |
| 7 ✅ | Building a program step by step | Word guess |
| 8 ✅ | Strings in depth, characters and codes, `%`, raw and multi-line strings | Caesar cipher |
| 9 ✅ | Maps, tuples, sets, nesting | Silent auction |
| 10 ✅ | Returning values, functions as values, `=>` lambdas, recursion | Calculator |

### Unit 2: Games, errors and objects (days 11–20)

| Day | Topic | Project |
|---|---|---|
| 11 ✅ | Capstone with everything so far | Twenty-One card game |
| 12 ✅ | Scope in depth: blocks, `let`, `var`, `const` | Number guessing |
| 13 ✅ | Debugging: reading errors, `vyrn check`, `attempt` / `catch` / `always`, `fail` | Bug hunt |
| 14 ✅ | Working with data records | Bigger or smaller |
| 15 ✅ | Projects and modules: `vyrn new`, `use`, your own modules, `vyrn add` packages | Smoothie bar |
| 16 ✅ | Objects: fields, methods, `self`, constructors, `to_text` | Bank account |
| 17 ✅ | Structs vs objects (values vs shared), `private` fields | Quiz app |
| 18 ✅ | Graphics: canvases, colours, turtles | Spot painting |
| 19 ✅ | Windows, keyboard, game state | Turtle race |
| 20 ✅ | Game design with data | Snake, part 1 |

### Unit 3: Intermediate (days 21–30)

| Day | Topic | Project |
|---|---|---|
| 21 ✅ | Inheritance, overriding, polymorphism, `is` type tests | Snake, part 2 |
| 22 ✅ | Collisions and movement | Pong |
| 23 ✅ | Many objects at once | Road crossing game |
| 24 ✅ | Files: read, write, append, binary files, `using` | Mail merge |
| 25 ✅ | CSV files and tables | State guessing game |
| 26 ✅ | Comprehensions; `map`, `filter`, `reduce`; pipelines `\|>` | Phonetic alphabet |
| 27 ✅ | GUI widgets; rest parameters `args...` and spread `xs...` | Unit converter app |
| 28 ✅ | Timers and events | Focus timer |
| 29 ✅ | Saving data | Password manager |
| 30 ✅ | Errors in depth, custom error types, JSON | Password manager, part 2 |

### Unit 4: Automation and APIs (days 31–40)

| Day | Topic | Project |
|---|---|---|
| 31 ✅ | Capstone | Flash cards |
| 32 ✅ | Email and dates (`vyrn.mail`, `vyrn.time`) | Birthday wisher |
| 33 ✅ | HTTP APIs and JSON | Is it daytime somewhere? |
| 34 ✅ | APIs with a GUI | Trivia quiz |
| 35 ✅ | API keys, environment variables, secrets | Weather alert |
| 36 ✅ | Combining APIs | News digest |
| 37 ✅ | POST, headers, authentication | Habit tracker |
| 38 ✅ | Databases: SQL, SQLite, records without SQL | Workout log |
| 39–40 ✅ | Capstone | Deal finder |

### Unit 5: The web (days 41–53)

| Day | Topic | Project |
|---|---|---|
| 41 ✅ | HTML: tags, elements, links, escaping; serving pages | Your home page |
| 42 ✅ | CSS: selectors, colours, the box model, stylesheets | A themed site |
| 43 ✅ | Forms, checking input, redirects; flexbox and grid | Guestbook |
| 44 ✅ | Templates (`vyrn.template`), layouts, static sites | Portfolio generator |
| 45 ✅ | Web scraping (`vyrn.html`), following links, politeness | Catalogue scraper |
| 46 ✅ | Regular expressions (`vyrn.re`) | Log reader |
| 47 ✅ | Testing: `vyrn test`, `assert`, fixtures, fakes, testing routes | Tested deal finder |
| 48 ✅ | JSON APIs: routes, status codes, REST design | An API other programs use |
| 49 ✅ | Cookies, sessions, password hashing, logins | Members area |
| 50 ✅ | A web app with a database | Blog |
| 51 ✅ | Security: validation, XSS, CSRF, SQL injection, secrets | Securing the blog |
| 52 ✅ | Deployment: `vyrn build`, Docker, GitHub Actions | Blog on the internet |
| 53 ✅ | Capstone | Todo web app |

### Unit 6: Data (days 54–59)

| Day | Topic | Project |
|---|---|---|
| 54 ✅ | Tables (`vyrn.table`): loading, cleaning, summarising | Cleaning survey data |
| 55 ✅ | Filtering, grouping, joining tables | Sales by region |
| 56 ✅ | Charts (`vyrn.chart`) | Weather charts |
| 57 ✅ | SQL for analysis: joins, aggregates, views | Music shop questions |
| 58–59 ✅ | Capstone | A data dashboard web page |

### Unit 7: Advanced Vyrn (days 60–69)

| Day | Topic | Project |
|---|---|---|
| 60 ✅ | Types and `vyrn check` | Typing an old program |
| 61 ✅ | Enums, `when` with patterns, `none` safety (`?.`, `??`) | Traffic lights and parsers |
| 62 ✅ | Generics and traits (interfaces, default methods) | Shapes and containers |
| 63 ✅ | Operator overloading, `to_text`, custom ordering | Money and vectors |
| 64 ✅ | Closures and decorators | Timing and caching |
| 65 ✅ | Generators (`yield`) and lazy sequences | Endless sequences |
| 66 ✅ | Resources: `using`, `close` | Safe files and connections |
| 67 ✅ | Values and sharing: copies, references, `mut`, `own` | Bugs to find |
| 68 ✅ | Recursion and algorithms: searching, sorting, complexity | Speed races |
| 69 ✅ | Capstone | An expression evaluator |

### Unit 8: Data structures, concurrency and speed (days 70–76)

| Day | Topic | Project |
|---|---|---|
| 70 ✅ | Stacks, queues, linked lists, trees | Undo and a word index |
| 71 ✅ | Graphs: breadth-first and depth-first search, shortest paths; bits and flags | Route planner |
| 72 ✅ | Async tasks: `async`, `await`, `spawn` | Many downloads at once |
| 73 ✅ | Channels, timeouts, `task.select`; retries and fault tolerance | A reliable worker |
| 74 ✅ | Design patterns: factory, observer, strategy, decorator | A plugin system |
| 75 ✅ | Profiling and speed: typed functions, the native compiler, `task.parallel` | Faster image filter |
| 76 ✅ | Using C and Python libraries | Borrowing a library |

### Unit 9: Games and 3D (days 77–79)

| Day | Topic | Project |
|---|---|---|
| 77 ✅ | Game loops, sprites, physics | A platformer |
| 78 ✅ | 3D: cameras, lighting, models | A 3D maze |
| 79 | Menus, sound, high scores, packaging a game | Finishing a game |

### Unit 10: Portfolio (days 80–84)

Each day is a project brief: what to build, a checklist, and hints.

| Day | Project |
|---|---|
| 80 | A command-line tool (`os.args`, files, exit codes) |
| 81 | A full-stack web app with a database and logins |
| 82 | A data report generated every morning |
| 83 | A game of your own design |
| 84 | Your own project, built, tested, packaged and deployed |

## What the course covers, compared with other languages

Most courses for Python, C++ and Java cover the topics below. Here is where
each one is taught, and how Vyrn does it.

| Topic | Vyrn | Day |
|---|---|---|
| Variables and types (int, float, string, bool, none/null, char) | `x = 1`; `int`, `float`, `str`, `bool`, `none`; a character is a one-letter `str` | 1, 2, 8 |
| Operators: arithmetic, comparison, logical, bitwise | `+ - * / // % **`, `== != < >`, `and or not`, `& \| ^ ~ << >>` | 2, 3, 71 |
| if / else / switch | `if` / `else if` / `else`, `when` / `is` | 3 |
| Loops: for, while, range-based for | `each x in xs`, `each i in 1 to 10`, `loop condition` | 5 |
| Strings and formatting (f-strings) | every string fills in `{x}`, with formats `{x:.2f}` | 1, 2, 8 |
| Lists / arrays / vectors, 2D arrays | lists `[...]`, nested lists | 4 |
| Tuples, sets, dictionaries / maps | `(a, b)`, `{a, b}`, `{k: v}` | 9 |
| Functions, parameters, return, scope | `create f(x):`, defaults, named arguments | 6, 10, 12 |
| Overloading | one function with defaults, named arguments or `args...` | 6, 27 |
| Pass by reference | lists, maps and objects are shared; `mut` parameters | 16, 67 |
| Recursion | functions calling themselves | 10, 68 |
| Input (input, cin, Scanner) | `input("prompt")` | 1 |
| File I/O, binary files, CSV, JSON | `read_text`, `write_text`, `append_text`, `read_bytes`, `vyrn.csv`, `vyrn.json` | 24, 25, 30 |
| Errors (try/except/finally, throw, custom) | `attempt` / `catch` / `always`, `fail`, your own error structs | 13, 30 |
| Modules, imports, packages (pip, Maven) | `use vyrn.x`, your own files, `vyrn add` (no virtual environments needed: packages are per project) | 15 |
| Git and GitHub | lessons alongside projects | 15, 52 |
| Classes and objects, constructors | `object` with fields and methods; calling the type constructs it | 16 |
| Encapsulation, access modifiers | `private` fields and methods | 17 |
| Structs, records, dataclasses | `struct`: a value type with fields | 17 |
| Inheritance, polymorphism, overriding, virtual | `object Dog(Animal):`, methods are overridable | 21 |
| Interfaces, abstract classes | `trait` with required and default methods, `impl` | 62 |
| Comprehensions | `[x * 2 each x in xs if x > 0]`, map and set comprehensions | 26 |
| Lambdas, higher-order functions, map/filter/reduce, streams | `x => x * 2`, `xs.map(f)`, `xs.filter(f)`, `xs.reduce(f)`, `xs \|> f` | 10, 26 |
| Regular expressions | `vyrn.re` | 46 |
| Type hints and static checking (mypy) | `x: int`, `-> str`, `vyrn check` | 60 |
| Generics, templates | `create first[T](xs: list[T]) -> T`, `struct Box[T]` | 62 |
| Enums, sealed classes, pattern matching | `enum` with data, `when` with patterns | 61 |
| Testing (pytest, JUnit), fixtures, mocking | `vyrn test`, `assert`, helper functions, passing fakes | 47 |
| HTTP APIs (requests) | `vyrn.http`: `http.get`, `http.post` | 33–37 |
| Databases, SQL, ORM | `vyrn.db`: SQL, plus `insert` / `find` / `update` / `delete` for records | 38, 57 |
| Packaging and build tools (pyproject, CMake, Maven) | `vyrn.toml`, `vyrn build` makes one executable | 52, 84 |
| Operator overloading | `create operator +(self, other)` | 63 |
| Decorators and closures | `@decorator`, nested functions that remember | 64 |
| Generators, iterators, lazy evaluation | `yield`, generators, `g.next()` | 65 |
| Context managers, RAII, destructors | `using x as y:` ... `end` calls `close` | 66 |
| Pointers, references, smart pointers, move semantics | values (structs, numbers) are copied; objects and lists are shared and freed automatically; `own` moves, `mut` lends | 67 |
| Data structures: stacks, queues, linked lists, trees, graphs | built with lists, maps and objects | 70–71 |
| Bit manipulation | `& \| ^ ~ << >>`, `bin`, `hex` | 71 |
| Concurrency: threads, async, coroutines | `async` / `await`, `spawn`, tasks that switch only at `await` | 72 |
| Synchronisation: mutexes, channels, condition variables | channels, `task.select`, timeouts; a channel as a lock | 73 |
| Design patterns | factory, singleton, observer, strategy, decorator | 74 |
| Profiling and optimisation | `vyrn run --profile`; typed functions compiled to machine code | 75 |
| C extensions, calling native code | `extern "c"`, the Python bridge | 76 |
| Web frameworks (Flask, Spring Boot) | `route GET "/path":` with JSON, templates and forms | 48–53 |
| Microservices, distributed systems, fault tolerance | services with `vyrn.http`, retries, timeouts | 73 |
| Docker and CI/CD | `vyrn build`, a Dockerfile, GitHub Actions | 52 |
| Memory model, garbage collection | reference counting, regions, no garbage-collector pauses | 67 |

What Vyrn does **not** have, on purpose: manual `new` / `delete`, raw
pointer arithmetic, undefined behaviour, metaclasses and a global
interpreter lock. Day 67 explains what it does instead and why.
