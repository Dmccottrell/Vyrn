# Day 82: Portfolio project: a report every morning

Today's brief is an **automated report**: a program that runs by itself
every day, gathers data, analyses it, and delivers the result as an email
or a web page. It combines the API days (Unit 4) with the data days
(Unit 6).

## 82.1 The brief

Some ideas:

- **Your city today**: weather, air quality and public transport delays,
  with a chart of the week.
- **Market watch**: your chosen shares or currencies, with the biggest
  moves and the news behind them (Day 36).
- **Project pulse**: commits, open issues and pull requests across your
  GitHub repositories.
- **Habit report**: from your habit tracker's data (Day 37), streaks and
  trends.

## 82.2 Done means

- [ ] Fetches from at least two sources, with keys from the environment,
      timeouts and retries (Days 35, 73).
- [ ] Keeps a history in a database, so it can say "compared with last
      week".
- [ ] At least two charts and one table, with honest axes (Day 56).
- [ ] Delivered as an HTML email or a page on a static site (Day 44).
- [ ] Runs on a schedule (Task Scheduler, cron, or a GitHub Actions
      `schedule`), and logs each run.
- [ ] If a source is down, the report still goes out, saying what's
      missing.
- [ ] Tested with recorded API answers and a fake clock; no test touches
      the network.

## 82.3 A suggested route

1. Record one real answer from each API into files, and build everything
   against those.
2. One function per section of the report, each returning data, then one
   that renders it all.
3. Only at the end, swap the recorded answers for live calls, behind the
   same function signatures.

## 82.4 Warm-up

Reports often compare with the past. Write `change(now, before)` giving a
short phrase: `"up 12%"`, `"down 5%"`, `"no change"`, or `"new"` when there
was nothing before.

```vyrn starter
create change(now: float, before: float?) -> str:
    return ""
end

main:
    print(change(112.0, 100.0), "|", change(95.0, 100.0), "|", change(100.0, 100.0), "|", change(7.0, none))
end
```

```text expect
up 12% | down 5% | no change | new
```

> Hint: deal with `none` (and zero) first; then the percentage is
> `round(100 * (now - before) / before)`.

<details><summary>Solution</summary>

```vyrn solution
create change(now: float, before: float?) -> str:
    if before == none or before == 0: return "new"
    let pct = round(100 * (now - before) / before)
    if pct > 0: return "up {pct}%"
    if pct < 0: return "down {-pct}%"
    return "no change"
end

main:
    print(change(112.0, 100.0), "|", change(95.0, 100.0), "|", change(100.0, 100.0), "|", change(7.0, none))
end
```

</details>

## 82.5 Going further

- A small dashboard (Days 58 and 59) beside the email, for exploring the
  history.
- Alerts only when something unusual happens, instead of every day.
- Run each source in parallel with tasks (Day 72), so one slow API doesn't
  delay the rest.
