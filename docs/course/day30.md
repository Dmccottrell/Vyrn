# Day 30: Errors in depth

On Day 13 you caught errors with `attempt` and `catch`. Today you go
further: reading what an error says, making **your own kinds of error**
that carry useful details, passing an error on after noting it, and making
a program survive damaged files. The project finishes the password
manager: it fails clearly when a site is missing and recovers from a
broken vault file.

## 30.1 What an error knows

A caught error is a value. Name it with `as e`, then read its **kind** (what
sort of problem) and its **message** (the details):

```vyrn
main:
    attempt:
        n = int("twelve")
    catch Error as e:
        print("kind:", e.kind)
        print("message:", e.message)
        print("whole:", e)
    end
end
```

```text
kind: ValueError
message: "twelve" is not an integer
whole: ValueError: "twelve" is not an integer
```

The kinds you will meet most:

| Kind | Happens when |
|---|---|
| `ValueError` | text isn't a number, a value is out of range, JSON is broken |
| `TypeError` | a value is the wrong type: `1 + "a"` |
| `KeyError` | a map has no such key: `ages["nobody"]` |
| `IndexError` | a list has no such position: `xs[99]` |
| `DivisionByZero` | dividing by zero |
| `NotFound` | a file that isn't there |

`catch Error` catches every kind. Catch the kinds you expect instead, when
you can: a `TypeError` usually means a mistake in the program, and hiding
it makes it harder to find.

```vyrn
create safe_divide(a: float, b: float) -> float?:
    attempt:
        return a / b
    catch DivisionByZero:
        return none
    end
end

main:
    print(safe_divide(10, 4), safe_divide(1, 0))
end
```

```text
2.5 none
```

### Exercise

Look up each name in the ages map, printing the age, or `unknown` when the
name isn't there, by catching `KeyError`.

```vyrn starter
main:
    ages = {"ada": 36, "alan": 41}
    each name in ["ada", "grace", "alan"]:
        print(name, ages[name])
    end
end
```

```text expect
ada 36
grace unknown
alan 41
```

> Hint: wrap the `print` in `attempt:`, and print `name, "unknown"` in
> `catch KeyError:`.

<details><summary>Solution</summary>

```vyrn solution
main:
    ages = {"ada": 36, "alan": 41}
    each name in ["ada", "grace", "alan"]:
        attempt:
            print(name, ages[name])
        catch KeyError:
            print(name, "unknown")
        end
    end
end
```

</details>

## 30.2 Your own kinds of error

A `struct` can be an error: `fail` it, and `catch` it by its name. Its
fields carry the details, so whoever catches it knows exactly what went
wrong:

```vyrn
struct NotEnoughMoney:
    needed: float
    have: float
end

create pay(balance: float, price: float) -> float:
    if price > balance: fail NotEnoughMoney(price, balance)
    return balance - price
end

main:
    print(pay(20, 5))
    attempt:
        pay(3, 7.5)
    catch NotEnoughMoney as e:
        print("you need", e.needed - e.have, "more")
    end
end
```

```text
15
you need 4.5 more
```

Why not just `fail ValueError("not enough money")`? Because the catcher
would have only a sentence. With your own type it gets numbers it can use,
and it can catch exactly this problem and nothing else.

Saying what a function can fail with is good manners, and lets `vyrn check`
help: add `fails` after the result type:

```vyrn
struct NotEnoughMoney:
    needed: float
    have: float
end

create pay(balance: float, price: float) -> float fails NotEnoughMoney:
    if price > balance: fail NotEnoughMoney(price, balance)
    return balance - price
end

main:
    print(pay(10, 3))
end
```

```text
7
```

### Exercise

Make an error type `TooYoung` with fields `name` and `age`, and a function
`check_age(name, age)` that fails with it under 13. Catch it and print the
message shown.

```vyrn starter
main:
    each name, age in [("mo", 15), ("kai", 9)]:
        print(name, "may join")
    end
end
```

```text expect
mo may join
kai is 9: too young
```

> Hint: `struct TooYoung:` with `name: str` and `age: int`.
> Hint: in the loop, `attempt:` `check_age(name, age)` `print(...)`
> `catch TooYoung as e:` `print("{e.name} is {e.age}: too young")`.

<details><summary>Solution</summary>

```vyrn solution
struct TooYoung:
    name: str
    age: int
end

create check_age(name: str, age: int):
    if age < 13: fail TooYoung(name, age)
end

main:
    each name, age in [("mo", 15), ("kai", 9)]:
        attempt:
            check_age(name, age)
            print(name, "may join")
        catch TooYoung as e:
            print("{e.name} is {e.age}: too young")
        end
    end
end
```

</details>

## 30.3 Passing an error on, and cleaning up

Sometimes a function should note an error but let its caller decide what
to do. `fail e` inside a `catch` passes the same error on:

```vyrn
create read_number(text: str) -> int:
    attempt:
        return int(text)
    catch ValueError as e:
        print("(log) could not read", text)
        fail e
    end
end

main:
    attempt:
        read_number("42x")
    catch ValueError as e:
        print("the caller decides:", e.message)
    end
end
```

```text
(log) could not read 42x
the caller decides: "42x" is not an integer
```

And `always:` runs whether there was an error or not: the place to close
files, stop timers, or print "done":

```vyrn
main:
    each text in ["7", "seven"]:
        attempt:
            print("got", int(text))
        catch ValueError:
            print("not a number:", text)
        always:
            print("finished with", text)
        end
    end
end
```

```text
got 7
finished with 7
not a number: seven
finished with seven
```

## 30.4 Surviving broken files

Files get damaged: a program crashes halfway through saving, someone edits
the JSON by hand and leaves out a bracket. A careful program checks what it
loads and falls back to something safe instead of crashing:

```vyrn
use vyrn.json
use vyrn.fs

create load_settings(path: str) -> map:
    let defaults = {"volume": 70, "music": true}
    if not fs.exists(path): return defaults
    attempt:
        let loaded = json.load(path)
        if loaded is not map: fail ValueError("settings must be a JSON object")
        return defaults | loaded
    catch ValueError as e:
        print("warning: {path} is damaged ({e.message}); using the defaults")
        return defaults
    end
end

main:
    write_text("good.json", r'{"volume": 30}')
    write_text("bad.json", r'{"volume": 30')
    print(load_settings("good.json"))
    print(load_settings("bad.json"))
    print(load_settings("missing.json"))
end
```

```text
{"volume": 30, "music": true}
warning: bad.json is damaged (bad.json: invalid JSON at byte 13: expected `,` or `}`); using the defaults
{"volume": 70, "music": true}
{"volume": 70, "music": true}
```

`defaults | loaded` merges two maps: everything from `defaults`, with
anything in `loaded` taking its place. So a settings file that only
mentions the volume still gets the default music setting.

### Exercise

Load a list of numbers from JSON. Give the list, or an empty list with a
warning when the file is damaged or doesn't hold a list.

```vyrn starter
use vyrn.json

create load_numbers(path: str) -> list:
    return json.load(path)
end

main:
    write_text("a.json", "[1, 2, 3]")
    write_text("b.json", "[1, 2,")
    write_text("c.json", r'{"not": "a list"}')
    print(load_numbers("a.json"), load_numbers("b.json"), load_numbers("c.json"))
end
```

```text expect
warning: b.json is damaged
warning: c.json is damaged
[1, 2, 3] [] []
```

> Hint: `attempt:` around `json.load`; `catch ValueError:` gives the warning.
> Hint: `if numbers is not list:` treat it as damaged too.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.json

create load_numbers(path: str) -> list:
    attempt:
        let numbers = json.load(path)
        if numbers is not list: fail ValueError("not a list")
        return numbers
    catch ValueError:
        print("warning: {path} is damaged")
        return []
    end
end

main:
    write_text("a.json", "[1, 2, 3]")
    write_text("b.json", "[1, 2,")
    write_text("c.json", r'{"not": "a list"}')
    print(load_numbers("a.json"), load_numbers("b.json"), load_numbers("c.json"))
end
```

</details>

## 30.5 Project: the password manager, part 2

Yesterday's `find` gave `none` for a missing site, and the vault crashed on
a damaged file. Now `find` fails with a `NoSuchSite` error that lists the
sites the vault does have, and a damaged vault is kept aside as a backup
instead of being overwritten.

```vyrn
use vyrn.json
use vyrn.fs

const VAULT = "vault3.json"

struct NoSuchSite:
    site: str
    known: list
end

create load_vault() -> map:
    if not fs.exists(VAULT): return {}
    attempt:
        let vault = json.load(VAULT)
        if vault is not map: fail ValueError("not a map")
        return vault
    catch ValueError:
        fs.rename(VAULT, VAULT + ".damaged")
        print("the vault was damaged; it is saved as {VAULT}.damaged")
        return {}
    end
end

create find(site: str) -> map fails NoSuchSite:
    let vault = load_vault()
    let key = site.lower()
    if key not in vault: fail NoSuchSite(site, sorted(vault.keys()))
    return vault[key]
end

main:
    json.save(VAULT, {"mail.com": {"email": "ada@mail.com", "password": "x9!"}})
    print(find("Mail.com")["email"])
    attempt:
        find("shop.com")
    catch NoSuchSite as e:
        let known = ", ".join(e.known)
        print("no login for {e.site}; saved sites: {known}")
    end
    write_text(VAULT, r"{not json")
    attempt:
        find("mail.com")
    catch NoSuchSite as e:
        print("now saved:", e.known)
    end
    print(fs.exists(VAULT + ".damaged"))
end
```

```text
ada@mail.com
no login for shop.com; saved sites: mail.com
the vault was damaged; it is saved as vault3.json.damaged
now saved: []
true
```

The error carries the list of known sites, so the message can help:
"no login for shop.com; saved sites: mail.com". And no data is ever thrown
away: the damaged file is renamed, not deleted, so it can be repaired by
hand.

### Exercise

Add a `WeakPassword` error with a `reason` field, and make `add_login` fail
with it when the password is shorter than 12 characters or has no digit.

```vyrn starter
create add_login(site: str, password: str):
    print("saved", site)
end

main:
    each site, pw in [("a.com", "short1"), ("b.com", "longbutnodigits"), ("c.com", "long-enough-1")]:
        add_login(site, pw)
    end
end
```

```text expect
a.com: too short
b.com: needs a digit
saved c.com
```

> Hint: `struct WeakPassword:` with `reason: str`.
> Hint: `fail WeakPassword("too short")`; in `main`, catch it and print
> `"{site}: {e.reason}"`.

<details><summary>Solution</summary>

```vyrn solution
struct WeakPassword:
    reason: str
end

create add_login(site: str, password: str) fails WeakPassword:
    if len(password) < 12: fail WeakPassword("too short")
    if not any([c.isdigit() each c in password]): fail WeakPassword("needs a digit")
    print("saved", site)
end

main:
    each site, pw in [("a.com", "short1"), ("b.com", "longbutnodigits"), ("c.com", "long-enough-1")]:
        attempt:
            add_login(site, pw)
        catch WeakPassword as e:
            print("{site}: {e.reason}")
        end
    end
end
```

</details>

## Today you learned

- A caught error has a `kind` and a `message`; catch the kinds you expect.
- A `struct` can be an error type: `fail` it with details, `catch` it by
  name, and declare it with `fails`.
- `fail e` inside a `catch` passes the error on; `always:` runs either way.
- Check what you load, fall back to safe defaults, and keep damaged files
  rather than overwrite them.
