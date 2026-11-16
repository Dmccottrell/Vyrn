# Day 37: Sending data to APIs

So far your programs only asked APIs questions. Today they **change**
things: create an account, add a record, update it, delete it. That means
new kinds of request (`POST`, `PUT`, `DELETE`), sending data in the
request's **body**, and proving who you are with **headers**. The project
is a habit tracker: every day you log how many minutes you practised, and a
graph website keeps the record.

## 37.1 The four verbs

Every HTTP request has a **method** that says what it wants to do:

| Method | Means | `vyrn.http` |
|---|---|---|
| `GET` | give me something | `http.get(url, headers)` |
| `POST` | here is something new; create it | `http.post(url, body, headers)` |
| `PUT` | replace this thing with what I send | `http.put(url, body, headers)` |
| `DELETE` | remove this thing | `http.delete(url, headers)` |

The body of a `POST` or `PUT` is usually JSON: give `http.post` a map and it
is sent as JSON, with the header that says so. The answer's status says
how it went: `200 OK`, or `201 Created` for something new.

```vyrn check
use vyrn.http

main:
    let r = http.post("https://httpbin.org/post", {"habit": "piano", "minutes": 25})
    print(r.status)
    print(r.json["json"])
end
```

httpbin.org is a website for testing: it answers every request by showing
what it received. Run that and it prints `200` and the data you sent.

## 37.2 Headers and tokens

**Headers** are the lines of extra information sent with a request: what
kind of data the body is, which language you'd like, and who you are. The
habit-tracking website Pixela gives each user a **token**, a password for
programs, that goes in a header with every request:

```vyrn check
use vyrn.http
use vyrn.os

main:
    let headers = {"X-USER-TOKEN": os.env("PIXELA_TOKEN")}
    let r = http.get("https://pixe.la/v1/users/ada/graphs/practice/stats", headers = headers)
    print(r.status, r.text)
end
```

Many APIs use the standard `Authorization` header instead, with the word
`Bearer` before the token: `{"Authorization": "Bearer " + token}`. Either
way, the token comes from the environment (Day 35), never from the code.

To keep a program testable, build each request as plain data first, and
send it in one place. The building is then a function you can check
without the internet:

```vyrn
create pixel_request(user: str, graph: str, day: str, minutes: int, token: str) -> map:
    return {
        "method": "POST",
        "url": "https://pixe.la/v1/users/{user}/graphs/{graph}",
        "headers": {"X-USER-TOKEN": token},
        "body": {"date": day, "quantity": str(minutes)},
    }
end

main:
    req = pixel_request("ada", "practice", "20261112", 25, "s3cret-token")
    print(req["method"], req["url"])
    print(req["body"])
end
```

```text
POST https://pixe.la/v1/users/ada/graphs/practice
{"date": "20261112", "quantity": "25"}
```

Pixela wants the date as `yyyyMMdd` and the quantity as text: every API has
its own little rules, written in its documentation. Read them carefully;
most API bugs are a format that's almost right.

### Exercise

Write `update_request(user, graph, day, minutes, token)` for changing a
day you already logged: Pixela wants a `PUT` to
`.../graphs/{graph}/{day}` with only the quantity in the body.

```vyrn starter
create update_request(user: str, graph: str, day: str, minutes: int, token: str) -> map:
    return {}
end

main:
    req = update_request("ada", "practice", "20261112", 40, "t0k")
    print(req["method"], req["url"], req["body"], req["headers"])
end
```

```text expect
PUT https://pixe.la/v1/users/ada/graphs/practice/20261112 {"quantity": "40"} {"X-USER-TOKEN": "t0k"}
```

> Hint: the same shape as `pixel_request`, with `"method": "PUT"` and the
> day at the end of the URL.

<details><summary>Solution</summary>

```vyrn solution
create update_request(user: str, graph: str, day: str, minutes: int, token: str) -> map:
    return {
        "method": "PUT",
        "url": "https://pixe.la/v1/users/{user}/graphs/{graph}/{day}",
        "headers": {"X-USER-TOKEN": token},
        "body": {"quantity": str(minutes)},
    }
end

main:
    req = update_request("ada", "practice", "20261112", 40, "t0k")
    print(req["method"], req["url"], req["body"], req["headers"])
end
```

</details>

## 37.3 Sending a request

One function sends any request built that way, choosing the `vyrn.http`
call by its method, and turns a failure status into an error:

```vyrn check
use vyrn.http

create send(req: map) -> any:
    let method = req["method"]
    var r = none
    if method == "GET":
        r = http.get(req["url"], headers = req["headers"])
    else if method == "POST":
        r = http.post(req["url"], req["body"], headers = req["headers"])
    else if method == "PUT":
        r = http.put(req["url"], req["body"], headers = req["headers"])
    else if method == "DELETE":
        r = http.delete(req["url"], headers = req["headers"])
    else:
        fail ValueError("unknown method {method}")
    end
    if not r.ok: fail NetworkError("{req["method"]} {req["url"]} said {r.status}: {r.text}")
    return r.json
end

main:
    print(send({"method": "GET", "url": "https://httpbin.org/get", "headers": {}}))
end
```

## 37.4 Dates for APIs

APIs each want dates their own way. `time.format` writes them however
they're wanted, and `time.add` gives yesterday:

```vyrn
use vyrn.time

main:
    today = time.make(2026, 11, 12)
    print(time.format(today, "%Y%m%d"))
    print(time.format(time.add(today, days = -1), "%Y%m%d"))
    print(time.format(today, "%Y-%m-%dT%H:%M:%SZ"))
end
```

```text
20261112
20261111
2026-11-12T00:00:00Z
```

## 37.5 Project: the habit tracker

Setting up on Pixela takes three requests, done once: create a user, create
a graph, and after that, one pixel a day. The tracker asks how many minutes
you practised and logs them. Here `send` is a fake, so you can see exactly
what would go over the network:

```vyrn
use vyrn.time

const PIXELA = "https://pixe.la/v1/users"

object Tracker:
    user: str
    token: str
    graph: str
    send: any

    create setup(self):
        self.send({"method": "POST", "url": PIXELA, "headers": {}, "body": {"token": self.token, "username": self.user, "agreeTermsOfService": "yes", "notMinor": "yes"}})
        let graph = {"id": self.graph, "name": "Practice", "unit": "minutes", "type": "int", "color": "sora"}
        self.send({"method": "POST", "url": "{PIXELA}/{self.user}/graphs", "headers": self.auth(), "body": graph})
    end

    create log(self, day: float, minutes: int):
        let date = time.format(day, "%Y%m%d")
        self.send({"method": "POST", "url": "{PIXELA}/{self.user}/graphs/{self.graph}", "headers": self.auth(), "body": {"date": date, "quantity": str(minutes)}})
    end

    create auth(self) -> map:
        return {"X-USER-TOKEN": self.token}
    end
end

main:
    network = fake(returns = {"isSuccess": true})
    tracker = Tracker("ada", "s3cret-token", "practice", network)
    tracker.setup()
    tracker.log(time.make(2026, 11, 12), 25)
    each call in network.calls:
        let req = call[0]
        print(req["method"], req["url"].replace(PIXELA, "..."), req["headers"])
    end
end
```

```text
POST ... {}
POST .../ada/graphs {"X-USER-TOKEN": "s3cret-token"}
POST .../ada/graphs/practice {"X-USER-TOKEN": "s3cret-token"}
```

The first request has no token header because it is the one that *creates*
the user with that token. To run it for real, replace the fake with 37.3's
`send`, take the token from `.env`, and read the minutes with `input()`.
Your graph is then at `https://pixe.la/v1/users/<you>/graphs/practice.html`.

### Exercise

Add a `remove(day)` method that deletes a logged day: a `DELETE` to
`.../graphs/{graph}/{yyyyMMdd}` with the token header and no body.

```vyrn starter
use vyrn.time

object Tracker:
    user: str
    token: str
    graph: str
    send: any
end

main:
    network = fake(returns = {"isSuccess": true})
    tracker = Tracker("ada", "t0k", "practice", network)
    tracker.remove(time.make(2026, 11, 12))
    print(network.last_call[0])
end
```

```text expect
{"method": "DELETE", "url": "https://pixe.la/v1/users/ada/graphs/practice/20261112", "headers": {"X-USER-TOKEN": "t0k"}}
```

> Hint: `create remove(self, day: float):` inside the object.
> Hint: `self.send({"method": "DELETE", "url": ..., "headers": {"X-USER-TOKEN": self.token}})`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.time

object Tracker:
    user: str
    token: str
    graph: str
    send: any

    create remove(self, day: float):
        let date = time.format(day, "%Y%m%d")
        let url = "https://pixe.la/v1/users/{self.user}/graphs/{self.graph}/{date}"
        self.send({"method": "DELETE", "url": url, "headers": {"X-USER-TOKEN": self.token}})
    end
end

main:
    network = fake(returns = {"isSuccess": true})
    tracker = Tracker("ada", "t0k", "practice", network)
    tracker.remove(time.make(2026, 11, 12))
    print(network.last_call[0])
end
```

</details>

## Today you learned

- `POST` creates, `PUT` replaces, `DELETE` removes; `http.post` and
  `http.put` send a map as a JSON body.
- Headers carry tokens: `X-USER-TOKEN`, or `Authorization: Bearer ...`.
- Build requests as plain data, and send them in one place, so the building
  can be tested without the internet.
- Format dates exactly the way each API wants them.
