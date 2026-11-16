# Day 35: Keys and secrets

Many APIs want to know who is asking. You sign up, and they give you an
**API key**: a long password that goes with every request. Keys are
secrets: anyone who has yours can use your account, and some APIs charge
for every request. Today you learn to keep keys out of your code with
**environment variables** and `.env` files. The project checks the weather
forecast every morning and warns you when you need an umbrella.

## 35.1 What a key looks like

A weather API, OpenWeatherMap, takes its key in the query string as
`appid`:

```text
https://api.openweathermap.org/data/2.5/forecast?lat=51.5&lon=-0.12&appid=6c1b9a0f2e7d4c8a9b3e5f1d2c4a6b8e
```

Others want it in a **header**, a line of extra information sent with the
request:

```vyrn check
use vyrn.http

main:
    let r = http.get("https://api.example.com/v1/things", headers = {"Authorization": "Bearer 6c1b9a0f2e7d4c8a"})
    print(r.status)
end
```

The API's documentation says which. Either way the key is in your program
somewhere, and that is the problem: programs get shared, posted online,
and saved in Git, where old versions stay forever. Bots search GitHub for
keys all day long.

## 35.2 Environment variables

Every running program gets a set of **environment variables** from whatever
started it: named pieces of text, like `HOME` or `PATH`. Put the key in one,
and the program reads it at run time; the code itself never contains it.

```vyrn
use vyrn.os

main:
    os.set_env("WEATHER_KEY", "6c1b9a0f")
    print(os.env("WEATHER_KEY"))
    print(os.env("NO_SUCH_VARIABLE"))
    print(os.env("UNITS", "metric"))
end
```

```text
6c1b9a0f
none
metric
```

`os.env(name)` gives `none` when the variable isn't set, or the default you
give. Here `os.set_env` stands in for you setting it before running; for
real you set it in the terminal, on Windows in PowerShell:

```powershell
$env:WEATHER_KEY = "6c1b9a0f2e7d4c8a9b3e5f1d2c4a6b8e"
vyrn run weather.vyrn
```

and on macOS or Linux:

```bash
WEATHER_KEY=6c1b9a0f2e7d4c8a9b3e5f1d2c4a6b8e vyrn run weather.vyrn
```

A missing key should stop the program with a message that says how to fix
it, not fail later with a confusing `401` from the API:

```vyrn
use vyrn.os

create need(name: str) -> str:
    let value = os.env(name)
    if value == none or value == "":
        fail ValueError("set the {name} environment variable first (see the README)")
    end
    return value
end

main:
    attempt:
        key = need("STOCKS_KEY")
    catch ValueError as e:
        print(e.message)
    end
end
```

```text
set the STOCKS_KEY environment variable first (see the README)
```

### Exercise

Write `settings()`, which gives a map of the city (from `CITY`, default
`"London"`) and the units (from `UNITS`, default `"metric"`).

```vyrn starter
use vyrn.os

create settings() -> map:
    return {}
end

main:
    print(settings())
    os.set_env("CITY", "Oslo")
    print(settings())
end
```

```text expect
{"city": "London", "units": "metric"}
{"city": "Oslo", "units": "metric"}
```

> Hint: `{"city": os.env("CITY", "London"), ...}`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.os

create settings() -> map:
    return {"city": os.env("CITY", "London"), "units": os.env("UNITS", "metric")}
end

main:
    print(settings())
    os.set_env("CITY", "Oslo")
    print(settings())
end
```

</details>

## 35.3 `.env` files

Typing keys into the terminal every time gets old. The usual answer is a
file named `.env` next to the program, with one `NAME=value` per line, that
`os.load_env()` reads into the environment:

```vyrn
use vyrn.os

main:
    write_text(".env", "# my keys: never commit this file\nNEWS_KEY=e4f1a2b3c4d5\nPHONE=\"+44 7700 900123\"\n")
    print(os.load_env())
    print(os.env("NEWS_KEY"), os.env("PHONE"))
end
```

```text
["NEWS_KEY", "PHONE"]
e4f1a2b3c4d5 +44 7700 900123
```

Variables that are already set win over the file, so you can still override
one from the terminal. And the most important line of today: **tell Git to
ignore the file**, by adding it to `.gitignore`:

```text
.env
```

Then commit a `.env.example` instead, with the names but not the values, so
others know what to set:

```text
WEATHER_KEY=your-key-from-openweathermap.org
```

If a key ever does get committed, deleting the file isn't enough: the old
version is still in Git's history. **Revoke** the key on the API's website
and make a new one.

## 35.4 Project: the umbrella alert

OpenWeatherMap's free forecast gives the weather every three hours for five
days. Each entry has a time and a list of conditions; every condition has
an **id**, and ids under 700 mean something falls from the sky (2xx
thunder, 3xx drizzle, 5xx rain, 6xx snow). Here is a recorded answer,
shortened:

```vyrn
use vyrn.json

const FORECAST = r'{"cod": "200", "list": [{"dt_txt": "2026-11-12 06:00:00", "weather": [{"id": 803, "main": "Clouds"}]}, {"dt_txt": "2026-11-12 09:00:00", "weather": [{"id": 500, "main": "Rain"}]}, {"dt_txt": "2026-11-12 12:00:00", "weather": [{"id": 501, "main": "Rain"}]}, {"dt_txt": "2026-11-12 15:00:00", "weather": [{"id": 800, "main": "Clear"}]}, {"dt_txt": "2026-11-12 18:00:00", "weather": [{"id": 800, "main": "Clear"}]}]}'

## The times today when something falls from the sky.
create wet_hours(forecast: map, hours: int) -> list:
    var wet = []
    each entry in forecast["list"][:hours // 3]:
        if any([c["id"] < 700 each c in entry["weather"]]):
            wet.append(entry["dt_txt"][11:16])
        end
    end
    return wet
end

main:
    let wet = wet_hours(json.parse(FORECAST), 12)
    if wet:
        print("Bring an umbrella: rain at", ", ".join(wet))
    else:
        print("No rain in the next 12 hours")
    end
end
```

```text
Bring an umbrella: rain at 09:00, 12:00
```

The live program puts it together: the key from `.env`, the forecast from
the API, and the alert by email (Day 32) or just printed:

```vyrn check
use vyrn.http
use vyrn.os

create wet_hours(forecast: map, hours: int) -> list:
    var wet = []
    each entry in forecast["list"][:hours // 3]:
        if any([c["id"] < 700 each c in entry["weather"]]):
            wet.append(entry["dt_txt"][11:16])
        end
    end
    return wet
end

main:
    os.load_env()
    let key = os.env("WEATHER_KEY")
    if key == none:
        print("Put WEATHER_KEY=... in a .env file first")
        return
    end
    let query = http.query({"lat": 51.5, "lon": -0.12, "appid": key})
    let r = http.get("https://api.openweathermap.org/data/2.5/forecast?" + query)
    if r.status == 401:
        print("The API doesn't accept this key (new keys take an hour to start working)")
        return
    end
    let wet = wet_hours(r.json, 12)
    print("Umbrella! Rain at " + ", ".join(wet) if wet else "No rain today")
end
```

Run it every morning with Task Scheduler (Windows) or `cron` (macOS and
Linux), and it becomes a small service of your own.

### Exercise

`dt_txt` is in UTC. Make `wet_hours` give the hours in a time zone instead,
using `time.parse` and `time.format(..., zone = ...)`. The forecast here has
rain at 09:00 and 12:00 UTC; in New York that is 04:00 and 07:00.

```vyrn starter
use vyrn.time

create wet_hours(entries: list, zone: str) -> list:
    return [e["dt_txt"][11:16] each e in entries if e["weather"][0]["id"] < 700]
end

main:
    entries = [{"dt_txt": "2026-11-12 09:00:00", "weather": [{"id": 500}]}, {"dt_txt": "2026-11-12 12:00:00", "weather": [{"id": 501}]}, {"dt_txt": "2026-11-12 15:00:00", "weather": [{"id": 800}]}]
    print(wet_hours(entries, "America/New_York"))
end
```

```text expect
["04:00", "07:00"]
```

> Hint: `moment = time.parse(e["dt_txt"], "%Y-%m-%d %H:%M:%S")`
> Hint: `time.format(moment, "%H:%M", zone = zone)`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.time

create local_hour(text: str, zone: str) -> str:
    return time.format(time.parse(text, "%Y-%m-%d %H:%M:%S"), "%H:%M", zone = zone)
end

create wet_hours(entries: list, zone: str) -> list:
    return [local_hour(e["dt_txt"], zone) each e in entries if e["weather"][0]["id"] < 700]
end

main:
    entries = [{"dt_txt": "2026-11-12 09:00:00", "weather": [{"id": 500}]}, {"dt_txt": "2026-11-12 12:00:00", "weather": [{"id": 501}]}, {"dt_txt": "2026-11-12 15:00:00", "weather": [{"id": 800}]}]
    print(wet_hours(entries, "America/New_York"))
end
```

</details>

## Today you learned

- API keys go in the query string or a header; they are passwords and must
  never be in your code.
- `os.env(name, default)` reads an environment variable; check for missing
  keys early with a helpful message.
- `os.load_env()` reads a `.env` file; put `.env` in `.gitignore` and commit
  a `.env.example`.
- A key that leaks must be revoked, not just deleted.
