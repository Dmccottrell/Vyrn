# Day 73: Channels, and work that fails

Yesterday's tasks each did one job and returned. Bigger programs have
tasks that run for a long time and pass work between them: one reads
orders, several process them, one writes the results. Today you connect
tasks with **channels**, wait on whichever thing happens first with
`task.select`, and make work survive failures with **retries**. The
project is a reliable worker that keeps going when things go wrong.

## 73.1 Channels

A **channel** is a pipe between tasks: one end `send`s values, the other
receives them with `await ch`, or with `async each x in ch` until the
channel is closed. A channel with a **capacity** holds at most that many
values; a sender waits while it's full, so a fast producer can't run far
ahead of a slow consumer:

```vyrn
async create producer(ch: any, n: int):
    each i in 1 to n:
        ch.send(i)
        print("sent", i)
    end
    ch.close()
end

async create consumer(ch: any, name: str) -> int:
    var total = 0
    async each x in ch:
        print("  {name} got {x}")
        total += x
        await sleep(0.01)
    end
    return total
end

main:
    let ch = channel(capacity = 2)
    let p = producer(ch, 5)
    let a = consumer(ch, "A")
    let b = consumer(ch, "B")
    await p
    print("total", await a + await b)
end
```

```text
sent 1
sent 2
  A got 1
  B got 2
sent 3
sent 4
  A got 3
  B got 4
sent 5
  A got 5
total 15
```

Two consumers share the work: each value goes to exactly one of them.
`ch.close()` tells receivers no more is coming, which ends their
`async each` loops. This **producer and consumers** shape (a queue of jobs
with several workers) is how web servers, print spoolers and build tools
spread work.

## 73.2 Whichever comes first

`task.select(a, b, ...)` waits for the first of several tasks or channels
and gives `(position, value)`. Combined with a channel of messages, it
lets a task react to whatever happens next: new work, or being told to
stop:

```vyrn
use vyrn.task

async create worker(jobs: any, quit: any) -> int:
    var done = 0
    loop:
        let which, value = await task.select(jobs, quit)
        if which == 1 or value == none:
            print("worker stopping after", done, "jobs")
            return done
        end
        print("doing", value)
        done += 1
    end
end

main:
    let jobs = channel()
    let quit = channel()
    let w = worker(jobs, quit)
    jobs.send("wash")
    jobs.send("dry")
    await sleep(0.01)
    quit.send(true)
    print(await w)
end
```

```text
doing wash
doing dry
worker stopping after 2 jobs
2
```

## 73.3 Retrying

Networks fail for a moment and then work again: a dropped connection, a
busy server. Trying again after a short wait fixes most of these.
**Exponential backoff** (Day 36) waits longer each time, so a struggling
server gets room to recover:

```vyrn
async create with_retries(work: any, tries: int = 4, wait: float = 0.01) -> any:
    var delay = wait
    each attempt_number in 1 to tries:
        attempt:
            return await work()
        catch NetworkError as e:
            if attempt_number == tries: fail e
            print("try {attempt_number} failed ({e.message}); waiting {delay}")
            await sleep(delay)
            delay *= 2
        end
    end
end

main:
    var calls = 0
    async create flaky_fetch() -> str:
        calls += 1
        if calls < 3: fail NetworkError("connection reset")
        return "the page"
    end
    print(await with_retries(flaky_fetch))
    async create always_down() -> str:
        fail NetworkError("server down")
    end
    attempt:
        await with_retries(always_down, tries = 2)
    catch NetworkError as e:
        print("gave up:", e.message)
    end
end
```

```text
try 1 failed (connection reset); waiting 0.01
try 2 failed (connection reset); waiting 0.02
the page
try 1 failed (server down); waiting 0.01
gave up: server down
```

Retry only what might work next time. A `NetworkError` might; a
`ValueError` from bad input never will, so `with_retries` lets those
through at once. And only retry things that are safe to do twice: fetching
a page is, but charging a card twice is not.

### Exercise

Some services say *when* to try again. Write `wait_before(try_number,
retry_after)`, giving the server's `retry_after` when it gave one (not
`none`), or else the backoff `0.5 * 2 ** (try_number - 1)`, never more
than 8.

```vyrn starter
create wait_before(try_number: int, retry_after: float?) -> float:
    return 0.0
end

main:
    print(wait_before(1, none), wait_before(3, none), wait_before(10, none), wait_before(2, 5.0))
end
```

```text expect
0.5 2.0 8.0 5.0
```

> Hint: `if retry_after != none: return retry_after`
> Hint: `min(0.5 * 2 ** (try_number - 1), 8.0)`

<details><summary>Solution</summary>

```vyrn solution
create wait_before(try_number: int, retry_after: float?) -> float:
    if retry_after != none: return retry_after
    return min(0.5 * 2 ** (try_number - 1), 8.0)
end

main:
    print(wait_before(1, none), wait_before(3, none), wait_before(10, none), wait_before(2, 5.0))
end
```

</details>

## 73.4 Failing well: circuit breakers

When a service is really down, retrying every request makes things worse:
every request now waits through all its retries, and the struggling
service is hammered. A **circuit breaker** notices repeated failures and
stops calling for a while, failing at once instead, then lets one request
through to see if the service has recovered:

```vyrn
object Breaker:
    failures: int = 0
    limit: int = 3
    open_until: float = 0.0
    cool_down: float = 30.0

    create allow(self, now: float) -> bool:
        return now >= self.open_until
    end
    create record(self, ok: bool, now: float):
        if ok:
            self.failures = 0
        else:
            self.failures += 1
            if self.failures >= self.limit: self.open_until = now + self.cool_down
        end
    end
end

main:
    let b = Breaker()
    each now, ok in [(0, false), (1, false), (2, false), (3, true), (20, true), (40, true), (41, false)]:
        if not b.allow(now):
            print("t={now}: not calling, the breaker is open")
            skip
        end
        b.record(ok, now)
        print("t={now}: called, {"ok" if ok else "failed"} (failures {b.failures})")
    end
end
```

```text
t=0: called, failed (failures 1)
t=1: called, failed (failures 2)
t=2: called, failed (failures 3)
t=3: not calling, the breaker is open
t=20: not calling, the breaker is open
t=40: called, ok (failures 0)
t=41: called, failed (failures 1)
```

After three failures, calls stop for 30 seconds, then resume. Large
systems of services that call each other ("microservices") use timeouts,
retries and breakers together, so one failing part doesn't drag down the
rest.

## 73.5 Project: a reliable worker

Jobs arrive on a channel. Several workers take them, each with a timeout
and retries; jobs that fail for good go to a **dead-letter** list for a
person to look at, instead of being lost:

```vyrn
use vyrn.task

async create process(job: int) -> str:
    await sleep(0.005)
    if job % 5 == 0: fail ValueError("job {job} has bad data")
    if job == 3: await sleep(1)
    return "job {job} done"
end

async create worker(name: str, jobs: any, results: list, dead: list):
    async each job in jobs:
        attempt:
            let r = await task.timeout(0.1, process(job))
            results.append(r)
        catch Timeout:
            dead.append((job, "too slow"))
        catch ValueError as e:
            dead.append((job, e.message))
        end
    end
end

main:
    let jobs = channel(capacity = 3)
    var results = []
    var dead = []
    let workers = [worker("w{i}", jobs, results, dead) each i in 1 to 3]
    each job in 1 to 10: jobs.send(job)
    jobs.close()
    each w in workers: await w
    print(len(results), "done:", sorted(results))
    print("dead letters:", sorted(dead))
end
```

```text
7 done: ["job 1 done", "job 2 done", "job 4 done", "job 6 done", "job 7 done", "job 8 done", "job 9 done"]
dead letters: [(3, "too slow"), (5, "job 5 has bad data"), (10, "job 10 has bad data")]
```

Every one of the ten jobs is accounted for: done, or in the dead letters
with the reason. The slow job didn't hold up the others, because the other
workers kept taking jobs while one waited for its timeout.

### Exercise

Count each worker's jobs. Write `tally(log)`, which takes a list of
`(worker, job)` pairs and gives a map from worker to how many jobs it did.

```vyrn starter
create tally(log: list) -> map:
    return {}
end

main:
    print(tally([("w1", 1), ("w2", 2), ("w1", 3), ("w3", 4), ("w1", 5)]))
end
```

```text expect
{"w1": 3, "w2": 1, "w3": 1}
```

> Hint: `counts[w] = counts.get(w, 0) + 1` for each pair.

<details><summary>Solution</summary>

```vyrn solution
create tally(log: list) -> map:
    var counts = {}
    each w, _ in log:
        counts[w] = counts.get(w, 0) + 1
    end
    return counts
end

main:
    print(tally([("w1", 1), ("w2", 2), ("w1", 3), ("w3", 4), ("w1", 5)]))
end
```

</details>

## Today you learned

- Channels pass values between tasks; a capacity makes fast senders wait;
  `close` ends receivers' `async each` loops.
- `task.select` waits for whichever task or channel is ready first.
- Retry with growing waits, only what might succeed later and is safe to
  repeat.
- Circuit breakers stop calling a service that keeps failing; dead letters
  keep failed work for a person.
