# Learn Vyrn: 100 Days of Code

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
| 18 | Graphics: canvases, colours, turtles | Spot painting |
| 19 | Windows, keyboard, game state | Turtle race |
| 20 | Game design with data | Snake, part 1 |

### Unit 3: Intermediate (days 21–30)

| Day | Topic | Project |
|---|---|---|
| 21 | Inheritance, overriding, polymorphism, `is` type tests | Snake, part 2 |
| 22 | Collisions and movement | Pong |
| 23 | Many objects at once | Road crossing game |
| 24 | Files: read, write, append, binary files, `using` | Mail merge |
| 25 | CSV files and tables | State guessing game |
| 26 | Comprehensions; `map`, `filter`, `reduce`; pipelines `\|>` | Phonetic alphabet |
| 27 | GUI widgets; rest parameters `args...` and spread `xs...` | Unit converter app |
| 28 | Timers and events | Focus timer |
| 29 | Saving data | Password manager |
| 30 | Errors in depth, custom error types, JSON | Password manager, part 2 |

### Unit 4: Automation and APIs (days 31–40)

| Day | Topic | Project |
|---|---|---|
| 31 | Capstone | Flash cards |
| 32 | Email and dates (`vyrn.mail`, `vyrn.time`) | Birthday wisher |
| 33 | HTTP APIs and JSON | Is it daytime somewhere? |
| 34 | APIs with a GUI | Trivia quiz |
| 35 | API keys, environment variables, secrets | Weather alert |
| 36 | Combining APIs | News digest |
| 37 | POST, headers, authentication | Habit tracker |
| 38 | Databases: SQL, SQLite, records without SQL | Workout log |
| 39–40 | Capstone | Deal finder |

### Unit 5: The web (days 41–60)

| Days | Topic |
|---|---|
| 41–44 | HTML and CSS: structure, forms, layout, templates (`vyrn.template`) |
| 45–47 | Web scraping (`vyrn.html`), regular expressions (`vyrn.re`) |
| 48 | Testing: `vyrn test`, `assert`, test helpers (fixtures), fakes for mocking |
| 49–50 | Automation capstone: price tracker with alerts |
| 51–53 | Web servers: `route`, path and query parameters, JSON APIs |
| 54–56 | Forms, sessions and logins, a blog with a database |
| 57 | REST design; an API other programs use |
| 58 | Security basics: input validation, password hashing, SQL injection |
| 59 | Deployment: `vyrn build`, Docker, CI/CD with GitHub Actions |
| 60 | Capstone: a todo web app |

### Unit 6: Data (days 61–70)

| Days | Topic |
|---|---|
| 61–63 | Tables (`vyrn.table`): loading, cleaning, filtering, grouping |
| 64–65 | Charts (`vyrn.chart`) |
| 66–67 | SQL for analysis: joins, aggregates |
| 68–70 | Capstone: a data dashboard web page |

### Unit 7: Advanced Vyrn (days 71–80)

| Day | Topic |
|---|---|
| 71 | Types and `vyrn check` (type hints, like mypy) |
| 72 | Enums, `when` with patterns, `none` safety (`?.`, `??`) |
| 73 | Generics (templates) and traits (interfaces, abstract classes, default methods) |
| 74 | Operator overloading, `to_text`, custom ordering |
| 75 | Decorators and closures |
| 76 | Generators (`yield`) and lazy sequences |
| 77 | Resources: `using`, `close`, RAII |
| 78 | Values and sharing: how Vyrn handles memory (copies, references, `mut`, `own`) |
| 79 | Recursion and algorithms: searching, sorting, complexity |
| 80 | Capstone: an expression evaluator |

### Unit 8: Data structures, concurrency and performance (days 81–90)

| Day | Topic |
|---|---|
| 81 | Stacks, queues and deques |
| 82 | Linked lists and trees (binary search trees) |
| 83 | Graphs: breadth-first and depth-first search, shortest paths |
| 84 | Bits: binary, `&`, `\|`, `^`, `<<`, `>>`, flags |
| 85 | Async tasks: `async`, `await`, `spawn` |
| 86 | Channels, synchronisation, timeouts, `task.select` |
| 87 | Design patterns: factory, singleton, observer, strategy, decorator |
| 88 | Profiling (`vyrn run --profile`) and speed: typed functions, the native compiler |
| 89 | Using C and Python libraries from Vyrn |
| 90 | Services that talk to each other: retries, timeouts, fault tolerance |

### Unit 9: Games and 3D (days 91–95)

| Day | Topic |
|---|---|
| 91 | Game loops, sprites, images |
| 92 | Physics: velocity, gravity, collisions; a platformer |
| 93 | 3D: cameras, shapes, lighting |
| 94 | A 3D game |
| 95 | Menus, saving high scores, packaging a game |

### Unit 10: Portfolio (days 96–100)

| Day | Project |
|---|---|
| 96 | A command-line tool (`os.args`, files, exit codes) |
| 97 | A full-stack web app with a database and logins |
| 98 | A data report generated every morning |
| 99 | A game of your own design |
| 100 | Your own project, built, tested, packaged and deployed |

## What the course covers, compared with other languages

Most courses for Python, C++ and Java cover the topics below. Here is where
each one is taught, and how Vyrn does it.

| Topic | Vyrn | Day |
|---|---|---|
| Variables and types (int, float, string, bool, none/null, char) | `x = 1`; `int`, `float`, `str`, `bool`, `none`; a character is a one-letter `str` | 1, 2, 8 |
| Operators: arithmetic, comparison, logical, bitwise | `+ - * / // % **`, `== != < >`, `and or not`, `& \| ^ ~ << >>` | 2, 3, 84 |
| if / else / switch | `if` / `else if` / `else`, `when` / `is` | 3 |
| Loops: for, while, range-based for | `each x in xs`, `each i in 1 to 10`, `loop condition` | 5 |
| Strings and formatting (f-strings) | every string fills in `{x}`, with formats `{x:.2f}` | 1, 2, 8 |
| Lists / arrays / vectors, 2D arrays | lists `[...]`, nested lists | 4 |
| Tuples, sets, dictionaries / maps | `(a, b)`, `{a, b}`, `{k: v}` | 9 |
| Functions, parameters, return, scope | `create f(x):`, defaults, named arguments | 6, 10, 12 |
| Overloading | one function with defaults, named arguments or `args...` | 6, 27 |
| Pass by reference | lists, maps and objects are shared; `mut` parameters | 16, 78 |
| Recursion | functions calling themselves | 10, 79 |
| Input (input, cin, Scanner) | `input("prompt")` | 1 |
| File I/O, binary files, CSV, JSON | `read_text`, `write_text`, `append_text`, `read_bytes`, `vyrn.csv`, `vyrn.json` | 24, 25, 30 |
| Errors (try/except/finally, throw, custom) | `attempt` / `catch` / `always`, `fail`, your own error structs | 13, 30 |
| Modules, imports, packages (pip, Maven) | `use vyrn.x`, your own files, `vyrn add` (no virtual environments needed: packages are per project) | 15 |
| Git and GitHub | lessons alongside projects | 15, 59 |
| Classes and objects, constructors | `object` with fields and methods; calling the type constructs it | 16 |
| Encapsulation, access modifiers | `private` fields and methods | 17 |
| Structs, records, dataclasses | `struct`: a value type with fields | 17 |
| Inheritance, polymorphism, overriding, virtual | `object Dog(Animal):`, methods are overridable | 21 |
| Interfaces, abstract classes | `trait` with required and default methods, `impl` | 73 |
| Comprehensions | `[x * 2 each x in xs if x > 0]`, map and set comprehensions | 26 |
| Lambdas, higher-order functions, map/filter/reduce, streams | `x => x * 2`, `xs.map(f)`, `xs.filter(f)`, `xs.reduce(f)`, `xs \|> f` | 10, 26 |
| Regular expressions | `vyrn.re` | 47 |
| Type hints and static checking (mypy) | `x: int`, `-> str`, `vyrn check` | 71 |
| Generics, templates | `create first[T](xs: list[T]) -> T`, `struct Box[T]` | 73 |
| Enums, sealed classes, pattern matching | `enum` with data, `when` with patterns | 72 |
| Testing (pytest, JUnit), fixtures, mocking | `vyrn test`, `assert`, helper functions, passing fakes | 48 |
| HTTP APIs (requests) | `vyrn.http`: `http.get`, `http.post` | 33–37 |
| Databases, SQL, ORM | `vyrn.db`: SQL, plus `insert` / `find` / `update` / `delete` for records | 38, 66 |
| Packaging and build tools (pyproject, CMake, Maven) | `vyrn.toml`, `vyrn build` makes one executable | 59, 100 |
| Operator overloading | `create operator +(self, other)` | 74 |
| Decorators and closures | `@decorator`, nested functions that remember | 75 |
| Generators, iterators, lazy evaluation | `yield`, generators, `g.next()` | 76 |
| Context managers, RAII, destructors | `using x as y:` ... `end` calls `close` | 77 |
| Pointers, references, smart pointers, move semantics | values (structs, numbers) are copied; objects and lists are shared and freed automatically; `own` moves, `mut` lends | 78 |
| Data structures: stacks, queues, linked lists, trees, graphs | built with lists, maps and objects | 81–83 |
| Bit manipulation | `& \| ^ ~ << >>`, `bin`, `hex` | 84 |
| Concurrency: threads, async, coroutines | `async` / `await`, `spawn`, tasks that switch only at `await` | 85 |
| Synchronisation: mutexes, channels, condition variables | channels, `task.select`, timeouts; a channel as a lock | 86 |
| Design patterns | factory, singleton, observer, strategy, decorator | 87 |
| Profiling and optimisation | `vyrn run --profile`; typed functions compiled to machine code | 88 |
| C extensions, calling native code | `extern "c"`, the Python bridge | 89 |
| Web frameworks (Flask, Spring Boot) | `route GET "/path":` with JSON, templates and forms | 51–60 |
| Microservices, distributed systems, fault tolerance | services with `vyrn.http`, retries, timeouts | 90 |
| Docker and CI/CD | `vyrn build`, a Dockerfile, GitHub Actions | 59 |
| Memory model, garbage collection | reference counting, regions, no garbage-collector pauses | 78 |

What Vyrn does **not** have, on purpose: manual `new` / `delete`, raw
pointer arithmetic, undefined behaviour, metaclasses and a global
interpreter lock. Day 78 explains what it does instead and why.
