# Day 52: Putting it on the internet

Your blog runs on `localhost`, which only your own computer can reach.
Today you put an app on a server where anyone can visit it, and set things
up so that every change you push is checked by the tests first. You'll
learn how to configure an app for the place it runs, build it into a
single file, package it with **Docker**, and run checks automatically with
**GitHub Actions**.

## 52.1 Configuration from the environment

On your computer the database is `blog.db` and the port is 8000. On a
server they're something else, and the server also has secrets your
computer shouldn't. So the app reads its settings from **environment
variables** (Day 35), with sensible defaults for your computer:

```vyrn
use vyrn.os

struct Config:
    port: int
    host: str
    database: str
    secure_cookies: bool
end

create load_config() -> Config:
    return Config(
        int(os.env("PORT", "8000")),
        os.env("HOST", "127.0.0.1"),
        os.env("DATABASE", "blog.db"),
        os.env("ENVIRONMENT", "development") == "production",
    )
end

main:
    print(load_config())
    os.set_env("PORT", "8080")
    os.set_env("HOST", "0.0.0.0")
    os.set_env("ENVIRONMENT", "production")
    print(load_config())
end
```

```text
Config(port=8000, host="127.0.0.1", database="blog.db", secure_cookies=false)
Config(port=8080, host="0.0.0.0", database="blog.db", secure_cookies=true)
```

Two settings matter on a server:

- `host = "0.0.0.0"` means "accept connections from other computers".
  The default, `127.0.0.1`, only accepts your own: right for developing,
  useless on a server.
- In production, cookies get `secure = true` (sent only over HTTPS).

The main program uses the config and nothing else:

```vyrn check
use vyrn.db
use vyrn.http
use vyrn.os

main:
    os.load_env()
    let port = int(os.env("PORT", "8000"))
    let host = os.env("HOST", "127.0.0.1")
    http.provide("conn", db.open(os.env("DATABASE", "blog.db")))
    print("listening on {host}:{port}")
    http.serve(port = port, host = host)
end
```

## 52.2 A health check

Servers and hosting services check that an app is alive by asking a
**health** address now and then, and restart it if it doesn't answer.
Make it check what the app needs, such as the database:

```vyrn
use vyrn.db
use vyrn.http

route GET "/health" (conn):
    attempt:
        conn.value("select 1")
        return {"status": "ok"}
    catch Error:
        return http.Response(503, {"status": "database unavailable"})
    end
end

main:
    http.provide("conn", db.open(":memory:"))
    let r = http.test("GET", "/health")
    print(r.status, r.json)
end
```

```text
200 {"status": "ok"}
```

## 52.3 One file to copy

`vyrn build` turns a project into a single program file with Vyrn inside
it. The server doesn't need Vyrn installed:

```bash
vyrn build -o blog
```

```text
built build/blog (5 files, 35.7 MB): copy it anywhere and run it, nothing to install
```

Copy `build/blog` to a Linux server and run it with its settings:

```bash
PORT=80 HOST=0.0.0.0 ENVIRONMENT=production DATABASE=/var/blog/blog.db ./blog
```

Build on the same kind of system as the server (Linux for a Linux server):
a program built on Windows runs on Windows.

## 52.4 Docker

**Docker** packs an app with everything it needs into an **image** that
runs the same on any computer with Docker: your laptop, a server, or a
hosting service. A `Dockerfile` is the recipe. This one builds the app
in a first stage with Vyrn, then copies only the finished file into a
small second stage:

```text
FROM debian:bookworm-slim AS build
RUN apt-get update && apt-get install -y curl ca-certificates
RUN curl -fsSL https://raw.githubusercontent.com/dmccottrell/vyrn/main/install.sh | sh
WORKDIR /app
COPY . .
RUN ~/.vyrn/bin/vyrn test && ~/.vyrn/bin/vyrn build -o blog

FROM debian:bookworm-slim
COPY --from=build /app/build/blog /usr/local/bin/blog
ENV HOST=0.0.0.0 PORT=8080 ENVIRONMENT=production DATABASE=/data/blog.db
VOLUME /data
EXPOSE 8080
CMD ["blog"]
```

Notice `vyrn test` in the build: if a test fails, the image isn't made.
Build and run it:

```bash
docker build -t blog .
```

```bash
docker run -p 8080:8080 -v blog-data:/data blog
```

`-v blog-data:/data` keeps the database in a **volume** outside the
container, so it survives when the container is replaced by a new version.
Services such as Fly.io, Render and Railway take a Dockerfile and run it
for you, with HTTPS included.

## 52.5 Checking every change: GitHub Actions

**Continuous integration** (CI) runs your checks on every push, on a fresh
computer, so nobody can merge code that breaks the tests. On GitHub, a file
`.github/workflows/check.yml` in the repository says what to run:

```text
name: check
on: [push, pull_request]

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Install Vyrn
        run: curl -fsSL https://raw.githubusercontent.com/dmccottrell/vyrn/main/install.sh | sh
      - name: Check types
        run: ~/.vyrn/bin/vyrn check
      - name: Check formatting
        run: ~/.vyrn/bin/vyrn fmt --check
      - name: Run the tests
        run: ~/.vyrn/bin/vyrn test
```

Each push now gets a green tick or a red cross on GitHub. Make `main`
**protected** (in the repository's settings) so changes must arrive as a
pull request with a green tick. Adding a **deploy** job that runs only on
`main` after the tests pass gives **continuous deployment**: merge, and
the site updates itself.

## 52.6 HTTPS

Browsers warn on plain `http://` sites, and cookies marked `Secure` aren't
sent over them. A hosting service usually gives you HTTPS automatically.
On your own server, put a **reverse proxy** in front of the app: Caddy
gets free certificates from Let's Encrypt by itself. Its whole
configuration, for the domain `myblog.example`:

```text
myblog.example {
    reverse_proxy localhost:8080
}
```

Caddy answers on HTTPS and passes requests to your app, which keeps
listening on `localhost`, unreachable from outside except through Caddy.

### Exercise

A deployed app should refuse to start with an unsafe configuration. Write
`problems(env)`, which checks a map of settings and gives the problems:
in production there must be a `SECRET_KEY` of at least 32 characters, and
`DEBUG` must not be `"true"`.

```vyrn starter
create problems(env: map) -> list[str]:
    return []
end

main:
    print(problems({"ENVIRONMENT": "development"}))
    print(problems({"ENVIRONMENT": "production", "DEBUG": "true", "SECRET_KEY": "short"}))
    print(problems({"ENVIRONMENT": "production", "SECRET_KEY": "x" * 40}))
end
```

```text expect
[]
["SECRET_KEY must be at least 32 characters", "DEBUG must be off in production"]
[]
```

> Hint: return `[]` straight away unless `env.get("ENVIRONMENT") == "production"`.
> Hint: `len(env.get("SECRET_KEY", "")) < 32` is the first problem.

<details><summary>Solution</summary>

```vyrn solution
create problems(env: map) -> list[str]:
    if env.get("ENVIRONMENT") != "production": return []
    var found = []
    if len(env.get("SECRET_KEY", "")) < 32: found.append("SECRET_KEY must be at least 32 characters")
    if env.get("DEBUG") == "true": found.append("DEBUG must be off in production")
    return found
end

main:
    print(problems({"ENVIRONMENT": "development"}))
    print(problems({"ENVIRONMENT": "production", "DEBUG": "true", "SECRET_KEY": "short"}))
    print(problems({"ENVIRONMENT": "production", "SECRET_KEY": "x" * 40}))
end
```

</details>

## Today you learned

- Read settings from environment variables with defaults; listen on
  `0.0.0.0` on a server; use secure cookies in production.
- A `/health` route lets hosts check the app is alive.
- `vyrn build` makes one file to copy to a server; Docker packs it into an
  image that runs anywhere, with data in a volume.
- GitHub Actions runs `vyrn check`, `vyrn fmt --check` and `vyrn test` on
  every push; protect `main` so only passing changes get in.
- Put HTTPS in front with the hosting service or a reverse proxy.
