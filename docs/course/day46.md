# Day 46: Regular expressions

`split`, `find` and `replace` work when you know exactly what the text
says. When you only know its *shape*, "a number, then a dash, then four
digits", you need a **regular expression**: a tiny language for describing
patterns in text. Every programming language has them, written almost the
same way. Today you learn the pieces, then use them to read a web server's
log and find out which pages are popular and what went wrong.

## 46.1 Patterns

`re.test(pattern, text)` says whether the pattern appears anywhere in the
text; `re.find_all` gives every piece that matches:

```vyrn
use vyrn.re

main:
    print(re.test(r"cat", "concatenate"))
    print(re.find_all(r"\d+", "3 cats, 12 dogs and 140 fish"))
    print(re.find_all(r"[A-Z]\w*", "Ada met Alan in London"))
end
```

```text
true
["3", "12", "140"]
["Ada", "Alan", "London"]
```

Write patterns in **raw strings** (`r"..."`), so a backslash stays a
backslash. The pieces:

| Pattern | Matches |
|---|---|
| `cat` | the letters c, a, t |
| `.` | any one character |
| `\d`, `\w`, `\s` | a digit; a letter, digit or `_`; a space, tab or newline |
| `\D`, `\W`, `\S` | anything *but* those |
| `[aeiou]`, `[a-z]`, `[^0-9]` | one of these; a range; anything not in it |
| `x*`, `x+`, `x?` | `x` zero or more times; one or more; maybe once |
| `x{3}`, `x{2,4}` | exactly 3; 2 to 4 |
| `^`, `$` | the start, the end of the text |
| `\b` | the edge of a word |
| `cat\|dog` | either |
| `\.`, `\(`, `\$` | a real `.`, `(` or `$` (a backslash takes away the special meaning) |

`+` and `*` are **greedy**: they take as much as they can. Add `?` to take
as little as possible: `<.+?>` matches one tag, `<.+>` from the first `<`
to the last `>`.

### Exercise

Write `is_postcode(text)` for a made-up postcode format: two capital
letters, one or two digits, a space, a digit, then two capital letters,
with nothing before or after.

```vyrn starter
use vyrn.re

create is_postcode(text: str) -> bool:
    return false
end

main:
    each t in ["SW1 2AB", "M4 9XY", "SW12 2AB", "sw1 2ab", "SW1 2AB please"]:
        print(t, is_postcode(t))
    end
end
```

```text expect
SW1 2AB true
M4 9XY false
SW12 2AB true
sw1 2ab false
SW1 2AB please false
```

> Hint: `[A-Z]{2}` is two capital letters; `\d{1,2}` one or two digits.
> Hint: `^` and `$` make it the whole text.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.re

create is_postcode(text: str) -> bool:
    return re.test(r"^[A-Z]{2}\d{1,2} \d[A-Z]{2}$", text)
end

main:
    each t in ["SW1 2AB", "M4 9XY", "SW12 2AB", "sw1 2ab", "SW1 2AB please"]:
        print(t, is_postcode(t))
    end
end
```

</details>

## 46.2 Groups: taking a match apart

Parentheses make a **group**, so you can take out just part of a match.
`re.find` gives the first match with its `text`, where it `start`s and
`end`s, and its `groups`; name a group with `(?P<name>...)` and it's in
`named` too:

```vyrn
use vyrn.re

main:
    let m = re.find(r"(\d{4})-(\d{2})-(\d{2})", "Due on 2026-11-12, paid late")
    print(m["text"], m["groups"], m["start"])
    let who = re.find(r"(?P<user>[\w.]+)@(?P<site>[\w.]+)", "write to ada.l@mail.com")
    print(who["named"]["user"], who["named"]["site"])
    print(re.find(r"\d+", "no numbers here"))
    print(re.find_all(r"(\w+)=(\d+)", "w=3 h=40 d=7"))
end
```

```text
2026-11-12 ["2026", "11", "12"] 7
ada.l mail.com
none
[("w", "3"), ("h", "40"), ("d", "7")]
```

With groups, `find_all` gives each match's groups as a tuple. `re.matches`
gives every match in full, like `re.find` does for the first.

## 46.3 Replacing and splitting

`re.replace` replaces every match. In the replacement, `$1` is the first
group; or give a function, which is called with each match and gives its
replacement:

```vyrn
use vyrn.re

main:
    print(re.replace(r"(\d{2})/(\d{2})/(\d{4})", "Born 14/03/2010", "$3-$2-$1"))
    print(re.replace(r"\s+", "too    many   spaces", " "))
    print(re.replace(r"\d+", "3 apples and 12 pears", m => str(int(m["text"]) * 2)))
    print(re.split(r"[,;]\s*", "red, green;blue,  white"))
    print(re.replace(r"(?i)vyrn", "VYRN, Vyrn, vyrn!", "Vyrn"))
end
```

```text
Born 2010-03-14
too many spaces
6 apples and 24 pears
["red", "green", "blue", "white"]
Vyrn, Vyrn, Vyrn!
```

`(?i)` at the start makes a pattern ignore case.

### Exercise

Hide all but the last four digits of every card number in a text: a card
number here is four groups of four digits separated by spaces or dashes.

```vyrn starter
use vyrn.re

create mask(text: str) -> str:
    return text
end

main:
    print(mask("Paid with 4111 1111 1111 1234 and 5500-0000-0000-0004."))
end
```

```text expect
Paid with **** **** **** 1234 and **** **** **** 0004.
```

> Hint: `\d{4}[ -]\d{4}[ -]\d{4}[ -](\d{4})` with the last group kept.
> Hint: the replacement is `"**** **** **** $1"`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.re

create mask(text: str) -> str:
    return re.replace(r"\d{4}[ -]\d{4}[ -]\d{4}[ -](\d{4})", text, "**** **** **** $1")
end

main:
    print(mask("Paid with 4111 1111 1111 1234 and 5500-0000-0000-0004."))
end
```

</details>

## 46.4 When not to use them

Regular expressions are powerful and hard to read. Some rules of thumb:

- If `split`, `startswith` or `in` does the job, use them.
- Don't parse HTML with regular expressions: use `html.parse`, which
  copes with everything real pages do.
- Don't try to fully check email addresses with one: the real rules are
  enormous. Check roughly, then send a confirmation email.
- Give a complicated pattern a name, as a constant, with a comment showing
  an example of what it matches.

## 46.5 Project: reading a server log

Web servers write a line for every request. Each line has the visitor's
address, the time, the request, the status and the size:

```text
203.0.113.9 - - [12/Nov/2026:10:15:32 +0000] "GET /books/dune HTTP/1.1" 200 5120
```

One pattern with named groups takes a line apart; lines that don't match
are counted, not crashed on:

```vyrn
use vyrn.re

## 203.0.113.9 - - [12/Nov/2026:10:15:32 +0000] "GET /books HTTP/1.1" 200 5120
const LINE = r'^(?P<ip>\S+) \S+ \S+ \[(?P<time>[^\]]+)\] "(?P<method>[A-Z]+) (?P<path>\S+) [^"]*" (?P<status>\d{3}) (?P<size>\d+|-)$'

const LOG = """203.0.113.9 - - [12/Nov/2026:10:15:32 +0000] "GET /books/dune HTTP/1.1" 200 5120
198.51.100.4 - - [12/Nov/2026:10:15:40 +0000] "GET /books/emma HTTP/1.1" 200 4870
203.0.113.9 - - [12/Nov/2026:10:16:01 +0000] "POST /basket HTTP/1.1" 303 -
192.0.2.77 - - [12/Nov/2026:10:16:05 +0000] "GET /books/dune HTTP/1.1" 200 5120
192.0.2.77 - - [12/Nov/2026:10:16:09 +0000] "GET /wp-admin.php HTTP/1.1" 404 312
this line got cut off halfway
198.51.100.4 - - [12/Nov/2026:10:17:44 +0000] "GET /checkout HTTP/1.1" 500 230"""

create parse(log: str) -> (list, int):
    var hits = []
    var broken = 0
    each line in log.lines():
        let m = re.find(LINE, line)
        if m == none:
            broken += 1
        else:
            hits.append(m["named"])
        end
    end
    return (hits, broken)
end

main:
    let hits, broken = parse(LOG)
    print(len(hits), "requests,", broken, "unreadable")
    var pages = {}
    each h in hits:
        if h["method"] == "GET" and h["status"] == "200":
            pages[h["path"]] = pages.get(h["path"], 0) + 1
        end
    end
    print("popular:", sorted(pages.items(), key = kv => kv[1], reverse = true)[0])
    print("errors:", ["{h["status"]} {h["path"]}" each h in hits if int(h["status"]) >= 400])
    print("visitors:", len(set([h["ip"] each h in hits])))
end
```

```text
6 requests, 1 unreadable
popular: ("/books/dune", 2)
errors: ["404 /wp-admin.php", "500 /checkout"]
visitors: 3
```

The 404 for `/wp-admin.php` is a bot looking for a weak WordPress site;
every public server sees thousands of these. The 500 is a real bug in the
shop's checkout.

### Exercise

Add up the bytes sent per visitor (`size`, where `-` means 0), and give the
visitor who downloaded the most.

```vyrn starter
create biggest(hits: list) -> (str, int):
    return ("", 0)
end

main:
    let hits = [{"ip": "a", "size": "5120"}, {"ip": "b", "size": "4870"}, {"ip": "a", "size": "-"}, {"ip": "b", "size": "1000"}]
    print(biggest(hits))
end
```

```text expect
("b", 5870)
```

> Hint: keep a map from ip to total: `0 if h["size"] == "-" else int(h["size"])`.
> Hint: `max(totals.items(), key = kv => kv[1])`

<details><summary>Solution</summary>

```vyrn solution
create biggest(hits: list) -> (str, int):
    var totals = {}
    each h in hits:
        let size = 0 if h["size"] == "-" else int(h["size"])
        totals[h["ip"]] = totals.get(h["ip"], 0) + size
    end
    return max(totals.items(), key = kv => kv[1])
end

main:
    let hits = [{"ip": "a", "size": "5120"}, {"ip": "b", "size": "4870"}, {"ip": "a", "size": "-"}, {"ip": "b", "size": "1000"}]
    print(biggest(hits))
end
```

</details>

## Today you learned

- A regular expression describes the shape of text: characters, classes
  (`\d`, `[a-z]`), repeats (`+`, `*`, `{2,4}`), anchors (`^`, `$`).
- `re.test`, `re.find`, `re.find_all`, `re.matches`; groups `( )` and named
  groups `(?P<name> )` take matches apart.
- `re.replace` with `$1` or a function; `re.split` on a pattern; `(?i)`
  ignores case.
- Use simpler tools when they work, never parse HTML with patterns, and
  name complicated patterns.
