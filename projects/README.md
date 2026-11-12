# Five projects in Vyrn

Each folder is a complete Vyrn project with comments throughout, tests,
and its own README. Open one, read it top to bottom, and run it.

| Project | What it is | What it teaches |
|---|---|---|
| [expenses](expenses/) | A command-line expense tracker | modules, SQLite, arguments, money in cents, tests |
| [adventure](adventure/) | A text adventure game | objects, data-driven design, a command parser, JSON saves |
| [notes](notes/) | A web app with a JSON API | routes, status codes, an HTML page, a database |
| [weather](weather/) | Data analysis with charts and a report | CSV, tables, charts, templates |
| [asteroids](asteroids/) | The arcade game | game loops, vectors with operators, inheritance, collisions, sound |

Run any of them from its folder:

```powershell
cd asteroids
vyrn run
vyrn test
```

`publish.ps1` makes each project its own GitHub repository.
