# Day 72: Doing several things at once

Most of the time a program that talks to the network is **waiting**: for
a web page, a database, a file. Fetching ten pages one after another means
waiting ten times; fetching them all at once means waiting about as long
as the slowest one. Today you learn Vyrn's **tasks**: functions that can
pause while they wait and let others run. The project downloads many
pages at once, politely.

## 72.1 `async` and `await`

A function marked `async` runs as a **task**. Calling it starts it and
gives back the task straight away; `await` waits for its result. While a
task waits (for `sleep`, a download, a message), other tasks run:

```vyrn
async create make_tea(name: str, minutes: float) -> str:
    print("{name}: kettle on")
    await sleep(minutes / 100)
    print("{name}: ready")
    return "{name}'s tea"
end

main:
    let ada = make_tea("Ada", 3)
    let alan = make_tea("Alan", 1)
    print("both started")
    print(await ada, "and", await alan)
end
```

```text
Ada: kettle on
Alan: kettle on
both started
Alan: ready
Ada: ready
Ada's tea and Alan's tea
```

Both kettles boil at the same time: Alan's tea (1 "minute") is ready
before Ada's (3). Without `async`, Alan would wait for Ada's tea to finish
first. (The example divides by 100 so it runs in hundredths of a second.)

Tasks take turns on one thread. A task only lets others run where it
waits (`await`), so code between two `await`s runs without interruption:
no two tasks ever change the same variable at the same moment. That makes
tasks much easier to get right than threads in most languages.

## 72.2 Waiting for many

`join(a, b, ...)` waits for all of several tasks and gives their results
in order. A **task group** does the same for any number of tasks, and
makes sure none outlives the block:

```vyrn
use vyrn.task
use vyrn.time

async create fetch(page: int) -> str:
    await sleep(0.05)
    return "page {page}"
end

main:
    let start = time.now()
    var one_by_one = []
    each p in 1 to 5:
        one_by_one.append(await fetch(p))
    end
    let slow = time.now() - start

    let start2 = time.now()
    using task.group() as g:
        each p in 1 to 5: g.spawn(fetch(p))
        print(await g.wait())
    end
    let fast = time.now() - start2
    print("one by one took about 5 waits; together about 1:", fast < slow / 2)
end
```

```text
["page 1", "page 2", "page 3", "page 4", "page 5"]
one by one took about 5 waits; together about 1: true
```

The results come back in the order the tasks were added, whatever order
they finished in.

### Exercise

Three friends each take a different time to arrive. Start all three
`arrive` tasks at once, and print the names in the order they arrived
(the order of the "here" lines shows it), then all three results with
`join`.

```vyrn starter
async create arrive(name: str, minutes: int) -> str:
    await sleep(minutes / 1000)
    print(name, "is here")
    return name
end

main:
    print(await arrive("Ada", 30))
end
```

```text expect
Grace is here
Alan is here
Ada is here
("Ada", "Alan", "Grace")
```

> Hint: `print(await join(arrive("Ada", 30), arrive("Alan", 20), arrive("Grace", 10)))`

<details><summary>Solution</summary>

```vyrn solution
async create arrive(name: str, minutes: int) -> str:
    await sleep(minutes / 1000)
    print(name, "is here")
    return name
end

main:
    print(await join(arrive("Ada", 30), arrive("Alan", 20), arrive("Grace", 10)))
end
```

</details>

## 72.3 When tasks fail, or take too long

A task that fails passes its error to whoever awaits it, like a function
call. `task.timeout` gives up on work that takes too long, and stops it:

```vyrn
use vyrn.task

async create flaky(n: int) -> int:
    await sleep(0.01)
    if n == 2: fail ValueError("item {n} is broken")
    return n * 10
end

async create stuck() -> str:
    await sleep(10)
    return "never"
end

main:
    each n in 1 to 3:
        attempt:
            print(await flaky(n))
        catch ValueError as e:
            print("failed:", e.message)
        end
    end
    attempt:
        await task.timeout(0.05, stuck())
    catch Timeout as e:
        print(e.message)
    end
end
```

```text
10
failed: item 2 is broken
30
the work took longer than 0.05 seconds
```

Always put a timeout on anything that talks to the network: a server that
never answers would otherwise freeze that part of your program forever.

## 72.4 Not too many at once

Starting a thousand downloads at once overloads the other server and your
own connection. Limit how many run together. A **channel** (tomorrow's
topic) is one way; the simplest is to work through the list in batches.
`0 until n by 3` counts 0, 3, 6, ...: the start of each batch.

```vyrn
use vyrn.task

async create fetch(page: int) -> str:
    await sleep(0.01)
    return "p{page}"
end

async create fetch_all(pages: list[int], at_once: int) -> list[str]:
    var results = []
    each start in 0 until len(pages) by at_once:
        let batch = pages[start:start + at_once]
        using task.group() as g:
            each p in batch: g.spawn(fetch(p))
            results.extend(await g.wait())
        end
        print("batch done:", batch)
    end
    return results
end

main:
    print(await fetch_all(list(1 to 7), 3))
end
```

```text
batch done: [1, 2, 3]
batch done: [4, 5, 6]
batch done: [7]
["p1", "p2", "p3", "p4", "p5", "p6", "p7"]
```

## 72.5 Project: downloading many pages

A real downloader with everything above: at most a few downloads at once,
a timeout on each, and failures reported without stopping the others. The
fetching function is passed in, so here it's a stand-in that sleeps and
sometimes fails; for real it would call `http.get`.

```vyrn
use vyrn.task

async create fake_fetch(url: str) -> str:
    let n = int(url.split("/")[-1])
    await sleep(0.01 * (n % 3 + 1))
    if n == 4: fail NetworkError("connection reset")
    if n == 6: await sleep(1)
    return "<html>page {n}</html>"
end

async create safe_fetch(fetch: any, url: str, seconds: float) -> (str, str):
    attempt:
        let page = await task.timeout(seconds, fetch(url))
        return (url, "ok, {len(page)} characters")
    catch Timeout:
        return (url, "too slow")
    catch Error as e:
        return (url, "failed: {e.message}")
    end
end

async create download_all(urls: list[str], fetch: any, at_once: int = 3) -> list:
    var report = []
    each start in 0 until len(urls) by at_once:
        using task.group() as g:
            each url in urls[start:start + at_once]: g.spawn(safe_fetch(fetch, url, 0.2))
            report.extend(await g.wait())
        end
    end
    return report
end

main:
    let urls = ["https://example.com/page/{n}" each n in 1 to 7]
    each url, outcome in await download_all(urls, fake_fetch):
        print("{url.split("/")[-1]}: {outcome}")
    end
end
```

```text
1: ok, 19 characters
2: ok, 19 characters
3: ok, 19 characters
4: failed: connection reset
5: ok, 19 characters
6: too slow
7: ok, 19 characters
```

`safe_fetch` turns every outcome into a value, so one broken page can't
stop the group, and the report lists every page whatever happened to it.
That's a good pattern for any batch of independent work.

### Exercise

Add a summary line: write `summary(report)` counting how many pages were
ok and how many weren't.

```vyrn starter
create summary(report: list) -> str:
    return ""
end

main:
    print(summary([("a", "ok, 23 characters"), ("b", "too slow"), ("c", "ok, 9 characters")]))
end
```

```text expect
2 ok, 1 problems
```

> Hint: count the outcomes that start with `"ok"`.

<details><summary>Solution</summary>

```vyrn solution
create summary(report: list) -> str:
    let ok = len([o each _, o in report if o.startswith("ok")])
    return "{ok} ok, {len(report) - ok} problems"
end

main:
    print(summary([("a", "ok, 23 characters"), ("b", "too slow"), ("c", "ok, 9 characters")]))
end
```

</details>

## Today you learned

- `async` functions run as tasks; calling one starts it, `await` waits for
  its result; other tasks run while one waits.
- Tasks switch only at `await`, so code between awaits never runs at the
  same moment as other tasks' code.
- `join` and `task.group()` wait for many tasks, giving results in order.
- Failures travel to whoever awaits; `task.timeout` stops work that takes
  too long. Limit how many run at once.
