# Day 29: Saving data with JSON

On Day 24 you saved text. Real programs save **structured** data: a list of
contacts, each with a name, an email and a phone number; settings with
on/off switches; a game's level, inventory and score. Today you use
**JSON**, the most common way to save structured data, and build a
**password manager** that remembers passwords between runs.

## 29.1 What JSON is

JSON is text that looks almost exactly like Vyrn's lists and maps:

```text
{"name": "Ada", "languages": ["Vyrn", "Python"], "admin": true, "boss": null}
```

`{...}` is a map (JSON calls it an *object*), `[...]` a list, and values are
text in double quotes, numbers, `true`, `false` or `null` (Vyrn's `none`).
Because it is plain text, every language can read it: a web page in
JavaScript, a server in Python, your Vyrn program.

`json.dumps` turns Vyrn values into JSON text; `json.parse` turns JSON text
back into Vyrn values:

```vyrn
use vyrn.json

main:
    person = {"name": "Ada", "languages": ["Vyrn", "Python"], "admin": true, "boss": none}
    text = json.dumps(person)
    print(text)
    back = json.parse(text)
    print(back["languages"][0], back["boss"] == none)
    print(json.dumps({"level": 3, "items": ["key"]}, 2))
end
```

```text
{"name":"Ada","languages":["Vyrn","Python"],"admin":true,"boss":null}
Vyrn true
{
  "level": 3,
  "items": [
    "key"
  ]
}
```

The `2` asks for **pretty** JSON, indented two spaces, which is easier for
people to read.

One thing to watch: in Vyrn text, `{` starts an insertion like `{name}`.
To write JSON by hand inside a program, use a **raw** string, `r'...'`,
where braces are just braces:

```vyrn
use vyrn.json

main:
    settings = json.parse(r'{"volume": 70, "music": true}')
    print(settings["volume"])
end
```

```text
70
```

### Exercise

Parse the JSON for a shopping basket and print the total cost.

```vyrn starter
use vyrn.json

main:
    text = r'[{"item": "tea", "price": 2.5, "qty": 2}, {"item": "cake", "price": 3.0, "qty": 1}]'
end
```

```text expect
8.0
```

> Hint: `basket = json.parse(text)` is a list of maps.
> Hint: `sum([b["price"] * b["qty"] each b in basket])`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.json

main:
    text = r'[{"item": "tea", "price": 2.5, "qty": 2}, {"item": "cake", "price": 3.0, "qty": 1}]'
    basket = json.parse(text)
    print(sum([b["price"] * b["qty"] each b in basket]))
end
```

</details>

## 29.2 Saving and loading files

`json.save(path, value)` writes JSON to a file, and `json.load(path)` reads
it back. Together they let a program remember things:

```vyrn
use vyrn.json

main:
    game = {"player": "Kai", "level": 2, "inventory": ["sword", "map"]}
    json.save("save.json", game)
    loaded = json.load("save.json")
    loaded["level"] += 1
    loaded["inventory"].append("lamp")
    json.save("save.json", loaded)
    print(json.load("save.json"))
end
```

```text
{"player": "Kai", "level": 3, "inventory": ["sword", "map", "lamp"]}
```

The pattern is always: **load, change, save**. The first time a program
runs there is no file yet, so check with `fs.exists` and start fresh:

```vyrn
use vyrn.json
use vyrn.fs

create load_or(path: str, fresh: any) -> any:
    if fs.exists(path): return json.load(path)
    return fresh
end

main:
    if fs.exists("visits.json"): fs.remove("visits.json")
    each _ in 1 to 3:
        visits = load_or("visits.json", {"count": 0})
        visits["count"] += 1
        json.save("visits.json", visits)
        print("visit number", visits["count"])
    end
end
```

```text
visit number 1
visit number 2
visit number 3
```

### Exercise

Keep a high-score table: load it (or start with an empty list), add the
new score, keep the top three, save it, and print it.

```vyrn starter
use vyrn.json
use vyrn.fs

create add_score(path: str, name: str, score: int) -> list:
    return []
end

main:
    if fs.exists("best.json"): fs.remove("best.json")
    add_score("best.json", "mo", 40)
    add_score("best.json", "kai", 90)
    add_score("best.json", "ada", 70)
    print(add_score("best.json", "sam", 55))
end
```

```text expect
[{"name": "kai", "score": 90}, {"name": "ada", "score": 70}, {"name": "sam", "score": 55}]
```

> Hint: `scores = json.load(path) if fs.exists(path) else []`
> Hint: append `{"name": name, "score": score}`, then
> `sorted(scores, key = s => s["score"], reverse = true)[:3]`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.json
use vyrn.fs

create add_score(path: str, name: str, score: int) -> list:
    scores = json.load(path) if fs.exists(path) else []
    scores.append({"name": name, "score": score})
    best = sorted(scores, key = s => s["score"], reverse = true)[:3]
    json.save(path, best)
    return best
end

main:
    if fs.exists("best.json"): fs.remove("best.json")
    add_score("best.json", "mo", 40)
    add_score("best.json", "kai", 90)
    add_score("best.json", "ada", 70)
    print(add_score("best.json", "sam", 55))
end
```

</details>

## 29.3 Making passwords

A strong password is long and random. `random.choice` picks one character
at a time from the characters allowed:

```vyrn
use vyrn.random

const LETTERS = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
const DIGITS = "0123456789"
const SYMBOLS = "!#$%&*+-?@"

create make_password(length: int) -> str:
    let pool = LETTERS + DIGITS + SYMBOLS
    return "".join([random.choice(pool) each _ in 0 until length])
end

main:
    random.seed(42)
    print(make_password(12))
    print(len(make_password(20)))
end
```

```text
dmNh$568DTxI
20
```

`random.seed(42)` makes the "random" choices the same every run, which is
what you want in a lesson or a test. In a real password manager, leave the
seed out, so every password is different.

### Exercise

Many websites demand at least one letter, one digit and one symbol. Write
`is_strong(password)` to check a password is at least 12 characters and has
all three.

```vyrn starter
const DIGITS = "0123456789"
const SYMBOLS = "!#$%&*+-?@"

create is_strong(password: str) -> bool:
    return false
end

main:
    print(is_strong("abc"), is_strong("abcdefghijk1!"), is_strong("abcdefghijklm1"))
end
```

```text expect
false true false
```

> Hint: `any([c in DIGITS each c in password])` says whether there is a digit.
> Hint: `c.isalpha()` is true for a letter.

<details><summary>Solution</summary>

```vyrn solution
const DIGITS = "0123456789"
const SYMBOLS = "!#$%&*+-?@"

create is_strong(password: str) -> bool:
    let letter = any([c.isalpha() each c in password])
    let digit = any([c in DIGITS each c in password])
    let symbol = any([c in SYMBOLS each c in password])
    return len(password) >= 12 and letter and digit and symbol
end

main:
    print(is_strong("abc"), is_strong("abcdefghijk1!"), is_strong("abcdefghijklm1"))
end
```

</details>

## 29.4 Project: a password manager

The vault is a JSON file mapping each website to its login. Three
functions do the work: add an entry, find one, and list what is saved. The
program around them would ask questions with `input()`; here the steps are
written out so you can see every one.

```vyrn
use vyrn.json
use vyrn.fs
use vyrn.random

const VAULT = "vault.json"
const POOL = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789!#$%&*+-?@"

create load_vault() -> map:
    return json.load(VAULT) if fs.exists(VAULT) else {}
end

create add(site: str, email: str, password: str = ""):
    var vault = load_vault()
    if password == "": password = "".join([random.choice(POOL) each _ in 0 until 16])
    vault[site.lower()] = {"email": email, "password": password}
    json.save(VAULT, vault)
end

create find(site: str) -> map?:
    return load_vault().get(site.lower())
end

main:
    if fs.exists(VAULT): fs.remove(VAULT)
    random.seed(7)
    add("Mail.com", "ada@mail.com")
    add("games.net", "ada99@mail.com", "hunter2-is-bad")
    let login = find("MAIL.COM")
    print(login["email"], len(login["password"]))
    print(find("nowhere.org"))
    print(sorted(load_vault().keys()))
end
```

```text
ada@mail.com 16
none
["games.net", "mail.com"]
```

Site names are stored in lower case so `"Mail.com"` and `"MAIL.COM"` are
the same site. `find` gives `none` for a site that isn't there; tomorrow you
make it fail with a clear error instead, and handle a vault file that has
been damaged.

A real vault would also be **encrypted**, so the file is useless to anyone
who copies it; Day 58 covers that. Never keep real passwords in a plain
JSON file.

### Exercise

Add `remove(site)`, which deletes a site from the vault and gives `true`,
or gives `false` if it wasn't there.

```vyrn starter
use vyrn.json
use vyrn.fs

const VAULT = "vault2.json"

create remove(site: str) -> bool:
    return false
end

main:
    json.save(VAULT, {"mail.com": {"email": "a@b.c", "password": "x"}})
    print(remove("Mail.com"), remove("mail.com"), json.load(VAULT))
end
```

```text expect
true false {}
```

> Hint: load it; `if site.lower() not in vault: return false`.
> Hint: `vault.remove(site.lower())` (or `vault.pop(...)`), then save.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.json
use vyrn.fs

const VAULT = "vault2.json"

create remove(site: str) -> bool:
    var vault = json.load(VAULT) if fs.exists(VAULT) else {}
    if site.lower() not in vault: return false
    vault.pop(site.lower())
    json.save(VAULT, vault)
    return true
end

main:
    json.save(VAULT, {"mail.com": {"email": "a@b.c", "password": "x"}})
    print(remove("Mail.com"), remove("mail.com"), json.load(VAULT))
end
```

</details>

## Today you learned

- JSON is text for lists, maps, text, numbers, `true`/`false` and `null`.
- `json.dumps` and `json.parse` convert; `json.save` and `json.load` use files.
- Write JSON by hand in a raw string, `r'...'`, so braces stay braces.
- Load, change, save; start fresh when `fs.exists` says there is no file yet.
- `random.choice` builds random passwords; `random.seed` repeats a run.
