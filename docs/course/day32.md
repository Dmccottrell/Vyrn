# Day 32: Dates and email

Today your programs learn the calendar: what day it is, how many days until
a holiday, which friends have a birthday today. Then they write
personalised letters and **send email**. The project is a birthday wisher
that checks a list of birthdays every morning and emails anyone whose day
it is.

## 32.1 Moments in time

A computer stores a moment as a number: the seconds since the start of
1970. `time.make` builds one from a date, `time.date` takes one apart, and
`time.format` writes one the way people read dates:

```vyrn
use vyrn.time

main:
    moon = time.make(1969, 7, 20, 20, 17)
    print(moon)
    parts = time.date(moon)
    print(parts["year"], parts["month"], parts["day"], parts["weekday"])
    print(time.format(moon, "%A %d %B %Y at %H:%M"))
    print(time.format(moon, "%d/%m/%y"))
end
```

```text
-14182980.0
1969 7 20 Sunday
Sunday 20 July 1969 at 20:17
20/07/69
```

The moon landing was before 1970, so its number is negative, and that's
fine. The codes in `time.format`:

| Code | Means | Example |
|---|---|---|
| `%Y`, `%y` | year | 2026, 26 |
| `%m`, `%B`, `%b` | month | 03, March, Mar |
| `%d` | day of the month | 07 |
| `%A`, `%a` | weekday | Monday, Mon |
| `%H`, `%I`, `%p` | hour (24), hour (12), AM/PM | 15, 03, PM |
| `%M`, `%S` | minute, second | 09, 26 |

`time.now()` is the moment the program runs. Lessons use fixed dates instead,
so they print the same every day.

### Exercise

Print the day of the week you were born (use 14 March 2010 here).

```vyrn starter
use vyrn.time

main:
    born = time.make(2010, 3, 14)
end
```

```text expect
You were born on a Sunday
```

> Hint: `time.date(born)["weekday"]`, or `time.format(born, "%A")`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.time

main:
    born = time.make(2010, 3, 14)
    print("You were born on a", time.format(born, "%A"))
end
```

</details>

## 32.2 Counting days

Subtracting two moments gives seconds; divide by `86400` (seconds in a day)
for days. `time.add` moves a moment by days, months or years, keeping to the
calendar (adding a month to 31 January gives the last day of February):

```vyrn
use vyrn.time

main:
    today = time.make(2026, 11, 12)
    new_year = time.make(2027, 1, 1)
    print("days until new year:", (new_year - today) / 86400)
    next_week = time.add(today, days = 7)
    print("a week from now:", time.format(next_week, "%A %d %B"))
    print("in a month:", time.format(time.add(time.make(2026, 1, 31), months = 1), "%d %B"))
end
```

```text
days until new year: 50.0
a week from now: Thursday 19 November
in a month: 28 February
```

### Exercise

How old will someone born on 2 May 2011 be on 12 November 2026? A person's
age goes up on their birthday, so count the years and take one off if this
year's birthday hasn't happened yet.

```vyrn starter
use vyrn.time

create age_on(born: float, day: float) -> int:
    return 0
end

main:
    print(age_on(time.make(2011, 5, 2), time.make(2026, 11, 12)))
    print(age_on(time.make(2011, 12, 25), time.make(2026, 11, 12)))
end
```

```text expect
15
14
```

> Hint: `b = time.date(born)` and `d = time.date(day)`; the years are
> `d["year"] - b["year"]`.
> Hint: the birthday hasn't happened if `(d["month"], d["day"]) < (b["month"], b["day"])`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.time

create age_on(born: float, day: float) -> int:
    let b = time.date(born)
    let d = time.date(day)
    var age = d["year"] - b["year"]
    if (d["month"], d["day"]) < (b["month"], b["day"]): age -= 1
    return age
end

main:
    print(age_on(time.make(2011, 5, 2), time.make(2026, 11, 12)))
    print(age_on(time.make(2011, 12, 25), time.make(2026, 11, 12)))
end
```

</details>

## 32.3 Time zones

A moment is the same everywhere, but the clock on the wall differs. Give
`time.format` a `zone` to see the local time in another place:

```vyrn
use vyrn.time

main:
    meeting = time.make(2026, 6, 1, 15, 0, zone = "Europe/London")
    each zone in ["Europe/London", "America/New_York", "Asia/Tokyo", "Australia/Sydney"]:
        print(zone.split("/")[1].replace("_", " "), time.format(meeting, "%a %H:%M", zone = zone))
    end
end
```

```text
London Mon 15:00
New York Mon 10:00
Tokyo Mon 23:00
Sydney Tue 00:00
```

`zone = "local"` means wherever your computer is. Times saved in files
should be kept as these numbers (or in UTC), and only turned into a local
clock when shown to a person.

## 32.4 Personalised letters

A **mail merge** fills a template with each person's details. Write the
template with markers, then replace each marker:

```vyrn
const TEMPLATE = """Dear [NAME],

Happy birthday! You are [AGE] today.

Best wishes,
Vyrn"""

create letter(name: str, age: int) -> str:
    return TEMPLATE.replace("[NAME]", name).replace("[AGE]", str(age))
end

main:
    print(letter("Ada", 12))
end
```

```text
Dear Ada,

Happy birthday! You are 12 today.

Best wishes,
Vyrn
```

Why `[NAME]` and not `{name}`? Because the template is data, often read
from a file, and the markers must stay as they are until you fill them.
For bigger templates, `vyrn.template` (Day 44) does the same with loops and
conditions.

## 32.5 Sending email

`mail.send` sends a message through a mail server. Your email provider
tells you its server name; you log in with your address and an **app
password** (a password made just for programs, in your account's security
settings). Never write the password in the program: keep it in an
**environment variable** (Day 35 explains these):

```vyrn check
use vyrn.mail
use vyrn.os

main:
    mail.send(
        to = "friend@example.com",
        subject = "Happy birthday!",
        body = "Have a great day!",
        sender = "me@example.com",
        server = "smtp.example.com",
        user = "me@example.com",
        password = os.env("MAIL_PASSWORD"),
    )
    print("sent")
end
```

On Windows, set the variable in PowerShell before running:

```powershell
$env:MAIL_PASSWORD = "your-app-password"
```

and on macOS or Linux:

```bash
export MAIL_PASSWORD="your-app-password"
```

## 32.6 Project: the birthday wisher

The birthdays live in a CSV file. Every morning the program finds today's
birthdays and sends each person a letter. Choosing *who* is a function of
the date, so it can be tested on any date; the sending is passed in, so a
test can pass a **fake** that only records what would have been sent
(Day 48 covers fakes fully):

```vyrn
use vyrn.csv
use vyrn.time

const TEMPLATE = "Dear [NAME],\nHappy birthday! You are [AGE] today."

create birthdays_on(rows: list, day: float) -> list:
    let d = time.date(day)
    return [row each row in rows if int(row["month"]) == d["month"] and int(row["day"]) == d["day"]]
end

create wish(rows: list, day: float, send: any) -> int:
    let year = time.date(day)["year"]
    let people = birthdays_on(rows, day)
    each person in people:
        let age = year - int(person["year"])
        let body = TEMPLATE.replace("[NAME]", person["name"]).replace("[AGE]", str(age))
        send(person["email"], "Happy birthday, {person["name"]}!", body)
    end
    return len(people)
end

main:
    write_text("birthdays.csv", "name,email,year,month,day\nAda,ada@example.com,2012,11,12\nKai,kai@example.com,2010,3,4\nMo,mo@example.com,2011,11,12\n")
    rows = csv.read("birthdays.csv")
    sent = fake(returns = true)
    count = wish(rows, time.make(2026, 11, 12), sent)
    print(count, "letters")
    each call in sent.calls:
        print("to", call[0], "-", call[2].lines()[1])
    end
end
```

```text
2 letters
to ada@example.com - Happy birthday! You are 14 today.
to mo@example.com - Happy birthday! You are 15 today.
```

For real, pass a function that calls `mail.send`, and run the program every
morning with your computer's scheduler (Task Scheduler on Windows, `cron` on
macOS and Linux).

### Exercise

People born on 29 February only get a real birthday every four years.
Change `birthdays_on` so that in a year without 29 February, their birthday
counts on 28 February.

```vyrn starter
use vyrn.time

create birthdays_on(rows: list, day: float) -> list:
    let d = time.date(day)
    return [row["name"] each row in rows if row["month"] == d["month"] and row["day"] == d["day"]]
end

main:
    rows = [{"name": "Leap", "month": 2, "day": 29}, {"name": "Ann", "month": 2, "day": 28}]
    print(birthdays_on(rows, time.make(2027, 2, 28)), birthdays_on(rows, time.make(2028, 2, 28)), birthdays_on(rows, time.make(2028, 2, 29)))
end
```

```text expect
["Leap", "Ann"] ["Ann"] ["Leap"]
```

> Hint: a year has 29 February if the day after 28 February is still in
> February: `time.date(time.add(time.make(year, 2, 28), days = 1))["month"] == 2`.
> Hint: in a year without it, a row with month 2 and day 29 should match
> 28 February too.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.time

create has_leap_day(year: int) -> bool:
    return time.date(time.add(time.make(year, 2, 28), days = 1))["month"] == 2
end

create birthdays_on(rows: list, day: float) -> list:
    let d = time.date(day)
    let moved = not has_leap_day(d["year"]) and d["month"] == 2 and d["day"] == 28
    var found = []
    each row in rows:
        let today = row["month"] == d["month"] and row["day"] == d["day"]
        let leap = moved and row["month"] == 2 and row["day"] == 29
        if today or leap: found.append(row["name"])
    end
    return found
end

main:
    rows = [{"name": "Leap", "month": 2, "day": 29}, {"name": "Ann", "month": 2, "day": 28}]
    print(birthdays_on(rows, time.make(2027, 2, 28)), birthdays_on(rows, time.make(2028, 2, 28)), birthdays_on(rows, time.make(2028, 2, 29)))
end
```

</details>

## Today you learned

- A moment is a number of seconds; `time.make`, `time.date` and
  `time.format` build it, take it apart and write it.
- Subtract moments for durations; `time.add` moves along the calendar.
- `zone = "Asia/Tokyo"` shows the clock somewhere else.
- A mail merge fills `[MARKERS]` in a template with `replace`.
- `mail.send` sends email; keep passwords in environment variables.
- Pass the sending function in, so a fake can stand in for it in tests.
