# Day 33: Web APIs

Websites are for people; **APIs** are websites for programs. Instead of a
page, they answer with data, usually JSON: the weather, the position of the
space station, today's exchange rates, the time the sun rises. Today you ask
APIs questions, read their answers, and handle the times they say no. The
project tells you whether it is daytime anywhere in the world.

## 33.1 What an API is

You ask an API a question by **requesting a URL**, just as a browser does:

```text
https://api.sunrise-sunset.org/json?lat=51.5&lng=-0.12&formatted=0
```

| Part | Here | Means |
|---|---|---|
| scheme | `https://` | how to talk to it (securely) |
| host | `api.sunrise-sunset.org` | which computer to ask |
| path | `/json` | which question |
| query | `?lat=51.5&lng=-0.12&formatted=0` | the details: `name=value`, joined by `&` |

The answer comes back with a **status code** saying how it went:

| Code | Means |
|---|---|
| 200 | OK: here is your data |
| 400 | Bad request: your question doesn't make sense |
| 401, 403 | Not allowed: you need a key, or the key is wrong |
| 404 | Not found: no such thing |
| 429 | Too many requests: slow down |
| 500 | The server broke: try later |

Build query strings with `http.query`, which makes any value safe to put in
a URL (spaces, `&` and accents need **encoding**):

```vyrn
use vyrn.http

main:
    params = http.query({"lat": 51.5, "lng": -0.12, "formatted": 0})
    print("https://api.sunrise-sunset.org/json?" + params)
    print(http.query({"q": "fish & chips", "city": "Zürich"}))
end
```

```text
https://api.sunrise-sunset.org/json?lat=51.5&lng=-0.12&formatted=0
q=fish%20%26%20chips&city=Z%C3%BCrich
```

## 33.2 Asking an API

`http.get(url)` sends the request and waits for the answer. The response
has the status (`r.status`, and `r.ok`, true for 200 to 299), the body as
text (`r.text`) and, for JSON, the data (`r.json`):

```vyrn check
use vyrn.http

main:
    let r = http.get("http://api.open-notify.org/iss-now.json")
    if not r.ok:
        print("the API said", r.status)
        return
    end
    let where = r.json["iss_position"]
    print("The space station is over latitude {where["latitude"]}, longitude {where["longitude"]}")
end
```

Run it with `vyrn run`: it needs the internet, and prints where the space
station is right now (it moves 7.7 km every second).

Every example below that asks a real API uses a **recorded answer** instead,
so the lessons print the same each time and work offline. Programmers do this
all the time: you save one real answer and develop against it.

## 33.3 Reading the answer

API answers nest maps and lists. Find your way in one step at a time:

```vyrn
use vyrn.json

const ANSWER = r'{"results": {"sunrise": "2026-11-12T07:12:40+00:00", "sunset": "2026-11-12T16:13:05+00:00", "day_length": 32425}, "status": "OK"}'

main:
    data = json.parse(ANSWER)
    print(data["status"])
    print(data["results"]["sunrise"])
    hours = data["results"]["day_length"] / 3600
    print("the day lasts {hours:.1f} hours")
end
```

```text
OK
2026-11-12T07:12:40+00:00
the day lasts 9.0 hours
```

When a field might be missing, `.get(key, default)` gives the default
instead of failing:

```vyrn
use vyrn.json

main:
    data = json.parse(r'{"name": "London", "population": 8982000}')
    print(data.get("population", "unknown"), data.get("mayor", "unknown"))
end
```

```text
8982000 unknown
```

### Exercise

The recorded answer is a weather forecast with a list of hours. Print the
highest temperature and the hour it happens.

```vyrn starter
use vyrn.json

const ANSWER = r'{"hourly": [{"time": "09:00", "temp": 11.5}, {"time": "12:00", "temp": 14.2}, {"time": "15:00", "temp": 13.8}]}'

main:
    data = json.parse(ANSWER)
end
```

```text expect
warmest: 14.2 at 12:00
```

> Hint: `hours = data["hourly"]` is a list of maps.
> Hint: `best = max(hours, key = h => h["temp"])`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.json

const ANSWER = r'{"hourly": [{"time": "09:00", "temp": 11.5}, {"time": "12:00", "temp": 14.2}, {"time": "15:00", "temp": 13.8}]}'

main:
    data = json.parse(ANSWER)
    best = max(data["hourly"], key = h => h["temp"])
    print("warmest: {best["temp"]} at {best["time"]}")
end
```

</details>

## 33.4 When the API says no

Networks fail. Servers break, keys run out, Wi-Fi drops. A program that
talks to an API should check the status, and catch the failure when there
is no answer at all:

```vyrn check
use vyrn.http

create station_position() -> map?:
    attempt:
        let r = http.get("http://api.open-notify.org/iss-now.json")
        if r.status == 429:
            print("too many requests; wait a minute")
            return none
        end
        if not r.ok:
            print("the API said {r.status}")
            return none
        end
        return r.json["iss_position"]
    catch NetworkError as e:
        print("no connection:", e.message)
        return none
    end
end

main:
    print(station_position())
end
```

To test code like this without the internet, pass the fetching in as a
function. A test gives a **fake** that returns a recorded answer, or fails:

```vyrn
create describe(fetch: any) -> str:
    attempt:
        let data = fetch()
        return "over {data["latitude"]}, {data["longitude"]}"
    catch Error as e:
        return "unavailable ({e.kind})"
    end
end

main:
    print(describe(fake(returns = {"latitude": "47.6", "longitude": "-122.3"})))
    print(describe(fake(fails = NetworkError("timed out"))))
end
```

```text
over 47.6, -122.3
unavailable (NetworkError)
```

## 33.5 Project: is it daytime?

The sunrise-sunset API gives sunrise and sunset in UTC, for any latitude
and longitude. Is it daytime there now? Compare the moment now with the
two times:

```vyrn
use vyrn.json
use vyrn.time

## A time from the API: "2026-11-12T07:12:40+00:00".
create api_time(text: str) -> float:
    return time.parse(text[:19], "%Y-%m-%d %H:%M:%S")
end

create is_daytime(answer: map, now: float) -> bool:
    let results = answer["results"]
    return api_time(results["sunrise"]) <= now and now < api_time(results["sunset"])
end

const LONDON = r'{"results": {"sunrise": "2026-11-12T07:12:40+00:00", "sunset": "2026-11-12T16:13:05+00:00"}, "status": "OK"}'
const SYDNEY = r'{"results": {"sunrise": "2026-11-11T18:46:05+00:00", "sunset": "2026-11-12T08:44:52+00:00"}, "status": "OK"}'

main:
    each hour in [6, 12, 20]:
        now = time.make(2026, 11, 12, hour, 0)
        london = "day" if is_daytime(json.parse(LONDON), now) else "night"
        sydney = "day" if is_daytime(json.parse(SYDNEY), now) else "night"
        print("{hour:02}:00 UTC: London {london}, Sydney {sydney}")
    end
end
```

```text
06:00 UTC: London night, Sydney day
12:00 UTC: London day, Sydney night
20:00 UTC: London night, Sydney night
```

At 20:00 UTC, it's 7 in the morning in Sydney the next day, so a new
sunrise has happened. That's why the program, for real, asks the API for
today's times each time, with `date=today`.

The live version:

```vyrn check
use vyrn.http
use vyrn.time

create api_time(text: str) -> float:
    return time.parse(text[:19], "%Y-%m-%d %H:%M:%S")
end

main:
    let query = http.query({"lat": -33.87, "lng": 151.21, "formatted": 0})
    let r = http.get("https://api.sunrise-sunset.org/json?" + query)
    if not r.ok:
        print("no answer:", r.status)
        return
    end
    let results = r.json["results"]
    let now = time.now()
    let day = api_time(results["sunrise"]) <= now and now < api_time(results["sunset"])
    print("It is", "day" if day else "night", "in Sydney")
end
```

### Exercise

Write `sun_hours(answer)`, which gives how many hours of daylight there are,
rounded to one decimal place, worked out from sunrise and sunset.

```vyrn starter
use vyrn.json
use vyrn.time

create api_time(text: str) -> float:
    return time.parse(text[:19], "%Y-%m-%d %H:%M:%S")
end

create sun_hours(answer: map) -> float:
    return 0.0
end

main:
    print(sun_hours(json.parse(r'{"results": {"sunrise": "2026-06-21T03:43:00+00:00", "sunset": "2026-06-21T20:21:00+00:00"}}')))
end
```

```text expect
16.6
```

> Hint: subtract the two moments for seconds; divide by 3600.
> Hint: `round(x, 1)` rounds to one place.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.json
use vyrn.time

create api_time(text: str) -> float:
    return time.parse(text[:19], "%Y-%m-%d %H:%M:%S")
end

create sun_hours(answer: map) -> float:
    let r = answer["results"]
    return round((api_time(r["sunset"]) - api_time(r["sunrise"])) / 3600, 1)
end

main:
    print(sun_hours(json.parse(r'{"results": {"sunrise": "2026-06-21T03:43:00+00:00", "sunset": "2026-06-21T20:21:00+00:00"}}')))
end
```

</details>

## Today you learned

- An API answers a URL with data, usually JSON; the query string carries
  the details, built safely with `http.query`.
- `http.get` gives a response with `status`, `ok`, `text` and `json`.
- Walk into nested answers one step at a time; `.get` copes with missing
  fields.
- Check the status, catch `NetworkError`, and test with recorded answers
  and fakes.
