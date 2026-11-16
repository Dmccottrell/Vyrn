# Day 36: Combining APIs

One API answers one kind of question. Interesting programs ask several and
put the answers together: the answer to the first decides what to ask the
second. Today's project watches a share price, and when it moves a lot,
fetches the news about that company to explain why, and sends you a short
**digest**. On the way you learn to **cache** answers so you don't ask the
same question twice, and to be polite to APIs that limit how often you ask.

## 36.1 One answer leads to the next

A stock API gives the closing price for each day. The change from one day
to the next, as a percentage, tells you how much it moved:

```vyrn
use vyrn.json

const PRICES = r'{"Meta Data": {"2. Symbol": "TSLA"}, "Time Series (Daily)": {"2026-11-11": {"4. close": "312.40"}, "2026-11-10": {"4. close": "289.10"}, "2026-11-09": {"4. close": "291.50"}}}'

create change(prices: map) -> float:
    let days = prices["Time Series (Daily)"]
    let dates = sorted(days.keys(), reverse = true)
    let today = float(days[dates[0]]["4. close"])
    let before = float(days[dates[1]]["4. close"])
    return (today - before) / before * 100
end

main:
    let moved = change(json.parse(PRICES))
    print("TSLA moved {moved:+.1f}%")
    if abs(moved) >= 5: print("big move: fetch the news")
end
```

```text
TSLA moved +8.1%
big move: fetch the news
```

The prices come as text (`"312.40"`), so `float` turns them into numbers.
The dates are map keys; sorting them newest first finds today and the day
before. `{moved:+.1f}` shows the sign even when it's positive.

Only when the move is big does the program ask the second API, a news
search:

```vyrn
use vyrn.json

const NEWS = r'{"status": "ok", "totalResults": 3, "articles": [{"source": {"name": "Reuters"}, "title": "Tesla shares jump after record deliveries", "url": "https://example.com/a"}, {"source": {"name": "BBC"}, "title": "Electric car sales rise across Europe", "url": "https://example.com/b"}, {"source": {"name": "Bloomberg"}, "title": "Tesla opens new factory", "url": "https://example.com/c"}]}'

main:
    each a in json.parse(NEWS)["articles"][:2]:
        print("{a["source"]["name"]}: {a["title"]}")
    end
end
```

```text
Reuters: Tesla shares jump after record deliveries
BBC: Electric car sales rise across Europe
```

### Exercise

Write `headlines(news, words)`, which keeps only the articles whose title
contains one of the words (in any case), giving their titles.

```vyrn starter
create headlines(news: map, words: list) -> list:
    return []
end

main:
    news = {"articles": [{"title": "Tesla shares jump"}, {"title": "Rain expected all week"}, {"title": "Musk says TESLA will grow"}]}
    print(headlines(news, ["tesla", "musk"]))
end
```

```text expect
["Tesla shares jump", "Musk says TESLA will grow"]
```

> Hint: `any([w in title.lower() each w in words])`

<details><summary>Solution</summary>

```vyrn solution
create mentions(title: str, words: list) -> bool:
    return any([w in title.lower() each w in words])
end

create headlines(news: map, words: list) -> list:
    return [a["title"] each a in news["articles"] if mentions(a["title"], words)]
end

main:
    news = {"articles": [{"title": "Tesla shares jump"}, {"title": "Rain expected all week"}, {"title": "Musk says TESLA will grow"}]}
    print(headlines(news, ["tesla", "musk"]))
end
```

</details>

## 36.2 Caching answers

Free API plans allow only so many requests: Alpha Vantage, the stock API,
allows 25 a day. Run your program a few times while writing it and you've
used them up. A **cache** keeps each answer in a file, and gives the saved
copy while it's still fresh:

```vyrn
use vyrn.fs
use vyrn.json
use vyrn.time

## Gives the saved answer if it is younger than `max_age` seconds, or asks
## `fetch` for a new one and saves it.
create cached(path: str, max_age: float, fetch: any) -> any:
    if fs.exists(path) and time.now() - fs.modified(path) < max_age:
        return json.load(path)
    end
    let answer = fetch()
    json.save(path, answer)
    return answer
end

main:
    if fs.exists("prices.json"): fs.remove("prices.json")
    api = fake(returns = {"close": 312.4})
    each _ in 0 until 3:
        print(cached("prices.json", 3600, api))
    end
    print("the API was asked", len(api.calls), "time")
end
```

```text
{"close": 312.4}
{"close": 312.4}
{"close": 312.4}
the API was asked 1 time
```

Three answers, one request. Choose the age by how fast the data changes:
an hour for news, a day for yesterday's closing prices, never for history.

## 36.3 Being polite: rate limits

An API that gets too many requests in a short time answers `429 Too Many
Requests`, often with a `Retry-After` header saying how many seconds to
wait. A program asking for many things should pause between requests, and
wait and try again when told to:

```vyrn
use vyrn.time

create polite_get(fetch: any, tries: int = 3) -> any:
    each attempt_number in 1 to tries:
        let r = fetch()
        if r["status"] != 429: return r
        let wait = float(r["headers"].get("retry-after", "1"))
        print("rate limited; waiting {wait} s (try {attempt_number})")
        time.sleep(wait / 100)
    end
    fail NetworkError("still rate limited after {tries} tries")
end

main:
    api = fake(returns_each = [{"status": 429, "headers": {"retry-after": "2"}}, {"status": 200, "headers": {}, "json": [1, 2]}])
    print(polite_get(api)["json"])
end
```

```text
rate limited; waiting 2.0 s (try 1)
[1, 2]
```

(The example divides the wait by 100 so the lesson doesn't take seconds to
run; the real one waits the full time.)

### Exercise

Many APIs say "wait longer each time": 1 second, then 2, then 4. Write
`backoff(try_number)` giving the wait for each try, never more than 30
seconds.

```vyrn starter
create backoff(try_number: int) -> int:
    return 1
end

main:
    print([backoff(n) each n in 1 to 7])
end
```

```text expect
[1, 2, 4, 8, 16, 30, 30]
```

> Hint: `2 ** (try_number - 1)` doubles each time; `min` caps it.

<details><summary>Solution</summary>

```vyrn solution
create backoff(try_number: int) -> int:
    return min(2 ** (try_number - 1), 30)
end

main:
    print([backoff(n) each n in 1 to 7])
end
```

</details>

## 36.4 Project: the stock news digest

The whole program: for each share you follow, work out the move; for the
big movers, find the news; put it all in one message. The two APIs are
passed in as functions, so here they are fakes with recorded answers, and
for real they call `http.get`:

```vyrn
create change(closes: list) -> float:
    return (closes[0] - closes[1]) / closes[1] * 100
end

create digest(symbols: map, prices: any, news: any, threshold: float = 5) -> str:
    var lines = []
    each symbol, company in symbols.items():
        let moved = change(prices(symbol))
        let arrow = "▲" if moved > 0 else "▼"
        lines.append("{symbol} {arrow} {abs(moved):.1f}%")
        if abs(moved) >= threshold:
            each title in news(company)[:2]:
                lines.append("  - {title}")
            end
        end
    end
    return "\n".join(lines)
end

main:
    closes = {"TSLA": [312.4, 289.1], "AAPL": [228.0, 227.1], "NVDA": [131.2, 140.6]}
    stories = {"Tesla": ["Tesla shares jump after record deliveries", "Tesla opens new factory", "Old news"], "Nvidia": ["Chip stocks fall on export rules"]}
    prices = symbol => closes[symbol]
    news = company => stories.get(company, [])
    print(digest({"TSLA": "Tesla", "AAPL": "Apple", "NVDA": "Nvidia"}, prices, news))
end
```

```text
TSLA ▲ 8.1%
  - Tesla shares jump after record deliveries
  - Tesla opens new factory
AAPL ▲ 0.4%
NVDA ▼ 6.7%
  - Chip stocks fall on export rules
```

The live version fills in the two functions with real requests, keys from
`.env` (Day 35) and the cache from 36.2:

```vyrn check
use vyrn.fs
use vyrn.http
use vyrn.json
use vyrn.os
use vyrn.time

create cached(path: str, max_age: float, fetch: any) -> any:
    if fs.exists(path) and time.now() - fs.modified(path) < max_age:
        return json.load(path)
    end
    let answer = fetch()
    json.save(path, answer)
    return answer
end

create prices(symbol: str) -> list:
    let query = http.query({"function": "TIME_SERIES_DAILY", "symbol": symbol, "apikey": os.env("STOCK_KEY")})
    let data = cached("prices-{symbol}.json", 86400, () => http.get("https://www.alphavantage.co/query?" + query).json)
    let days = data["Time Series (Daily)"]
    return [float(days[d]["4. close"]) each d in sorted(days.keys(), reverse = true)[:2]]
end

create news(company: str) -> list:
    let query = http.query({"q": company, "sortBy": "publishedAt", "apiKey": os.env("NEWS_KEY")})
    let data = cached("news-{company}.json", 3600, () => http.get("https://newsapi.org/v2/everything?" + query).json)
    return [a["title"] each a in data["articles"]]
end

main:
    os.load_env()
    each symbol, company in {"TSLA": "Tesla", "NVDA": "Nvidia"}.items():
        let closes = prices(symbol)
        let moved = (closes[0] - closes[1]) / closes[1] * 100
        print("{symbol} {moved:+.1f}%")
        if abs(moved) >= 5:
            each title in news(company)[:3]: print("  - " + title)
        end
    end
end
```

### Exercise

Sort the digest so the biggest moves (up or down) come first. Write
`by_size(moves)`, which takes a map of symbol to move and gives the symbols
in that order.

```vyrn starter
create by_size(moves: map) -> list:
    return list(moves.keys())
end

main:
    print(by_size({"TSLA": 8.1, "AAPL": 0.4, "NVDA": -6.7}))
end
```

```text expect
["TSLA", "NVDA", "AAPL"]
```

> Hint: `sorted(moves.keys(), key = s => abs(moves[s]), reverse = true)`

<details><summary>Solution</summary>

```vyrn solution
create by_size(moves: map) -> list:
    return sorted(moves.keys(), key = s => abs(moves[s]), reverse = true)
end

main:
    print(by_size({"TSLA": 8.1, "AAPL": 0.4, "NVDA": -6.7}))
end
```

</details>

## Today you learned

- Let one API's answer decide what to ask the next.
- Cache answers in files, and reuse them while they are fresh.
- Respect rate limits: pause, honour `Retry-After`, and back off.
- Pass the API calls in as functions so the whole program can run on
  recorded answers.
