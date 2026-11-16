# Vyrn Notes

A small web app for notes, with a JSON API, written in [Vyrn](https://github.com/dmccottrell/vyrn).

## Run it

Install Vyrn first (see its README), then in this folder:

```powershell
vyrn run
```

Then open http://localhost:8080. The API: `GET /api/notes`, `POST /api/notes` with `{"title": ..., "body": ...}`, `GET /api/search?q=...`, `DELETE /api/notes/{id}`.

## Files

| File | What it does |
|---|---|
| `main.vyrn` | the routes and the server |
| `store.vyrn` | the notes in a SQLite database |
| `page.vyrn` | the HTML, CSS and JavaScript page |
| `tests/` | tests for the note store |

## What it shows

Web routes, a JSON API with status codes, an HTML page talking to it, dependency injection, and a database. Every file is commented: read them top to bottom.
