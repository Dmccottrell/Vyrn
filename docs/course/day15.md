# Day 15: Projects, Modules, Packages and Git

So far every program has been one file. Real programs are **projects**:
several files, each with one job, plus tests, settings and history. Today
you set up a project the way professionals do, split code into modules,
use other people's code, and save your work with Git. The day ends with a
smoothie bar that serves drinks and keeps track of stock and money.

## 15.1 Making a project

In a terminal, in the folder where you keep your code:

```bash
vyrn new smoothies
```

```text
created smoothies/

    cd smoothies
    vyrn run
    vyrn test
```

Inside `smoothies/`:

| File | What it's for |
|---|---|
| `vyrn.toml` | the project's name and settings |
| `main.vyrn` | where the program starts (`main:`) |
| `greeting.vyrn` | an example module |
| `tests/test_greeting.vyrn` | an example test |
| `.gitignore` | files Git should not save (built programs) |

Inside the project, `vyrn run` runs `main.vyrn`, `vyrn test` runs the
tests, `vyrn check` checks every file, and `vyrn fmt` tidies them all.

**An editor.** Any text editor works, but one that knows Vyrn helps a lot:
install VS Code and the Vyrn extension (search "Vyrn" in its extensions
panel) to get colours, errors as you type, completion and "go to
definition". Open the project folder, not single files.

### Exercise

No project needed for this one: print the three commands you would type to
make a project called `quiz`, go into it, and run it, one per line.

```vyrn starter
main:
end
```

```text expect
vyrn new quiz
cd quiz
vyrn run
```

> Hint: Three `print`s.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("vyrn new quiz")
    print("cd quiz")
    print("vyrn run")
end
```

</details>

## 15.2 Your own modules

A **module** is simply another `.vyrn` file in your project. Here is
`greeting.vyrn`:

```vyrn check
create greet(name: str) -> str:
    return "Hello, {name}!"
end
```

and `main.vyrn` uses it:

```vyrn check
create greet(name: str) -> str:
    return "Hello, {name}!"
end

main:
    print(greet("world"))
end
```

In the real project, `main.vyrn` starts with `use greeting (greet)`
instead of repeating the function: `use greeting` finds `greeting.vyrn`,
and `(greet)` brings in the names you want. Without the brackets, you write
the module's name first: `use greeting` then `greeting.greet("world")`.

Split code into modules by topic: `menu.vyrn` for the menu, `stock.vyrn`
for the stock, `main.vyrn` to tie them together. Each file stays short and
easy to find your way around. Files in folders work too: `use shop.menu`
loads `shop/menu.vyrn`.

### Exercise

Library modules can be used the same two ways. Rewrite this program to
bring in `sqrt` and `pi` by name, so `math.` is no longer needed.

```vyrn starter
use vyrn.math

main:
    print(math.sqrt(2) * math.pi)
end
```

```text expect
4.442882938158366
```

> Hint: `use vyrn.math (sqrt, pi)`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.math (sqrt, pi)

main:
    print(sqrt(2) * pi)
end
```

</details>

## 15.3 Using other people's code: packages

Code someone else has shared is a **package**. Vyrn packages are fetched
straight from a Git host, at an exact version:

```bash
vyrn add github.com/someone/colours@v1.2.0
```

This downloads the package, and records the exact version in `vyrn.lock`
so everyone working on the project gets the same code. Then:

```vyrn check
main:
    # With the package added: use "github.com/someone/colours@v1.2.0" (shade)
    print("ready")
end
```

`vyrn fetch` downloads every locked package (for example on a new
computer, or before working offline).

Python programmers use `pip` and "virtual environments" to keep each
project's packages apart. Vyrn doesn't need virtual environments: packages
always belong to one project, listed in its `vyrn.lock`.

### Exercise

Print which file records the exact versions of a project's packages, and
which command downloads them all.

```vyrn starter
main:
end
```

```text expect
Versions: vyrn.lock
Download: vyrn fetch
```

> Hint: Two `print`s.

<details><summary>Solution</summary>

```vyrn solution
main:
    print("Versions: vyrn.lock")
    print("Download: vyrn fetch")
end
```

</details>

## 15.4 Saving your history with Git

**Git** records snapshots of your project, called **commits**. You can see
what changed, go back to an earlier version, and share the project.
**GitHub** is a website that stores Git projects online.

Once per project:

```bash
git init
```

Each time you reach a good point:

```bash
git add .
```

```bash
git commit -m "Serve smoothies and track stock"
```

| Command | Does |
|---|---|
| `git status` | what changed since the last commit |
| `git diff` | the changes, line by line |
| `git add .` | choose every change for the next commit |
| `git commit -m "message"` | save a snapshot with a description |
| `git log --oneline` | the list of snapshots |
| `git push` | send commits to GitHub (after linking the project with `git remote add origin <url>`) |

Commit small and often, with messages that say *what* changed and *why*.

### Exercise

Put the Git commands in the right order to save a first snapshot of a new
project, and print them.

```vyrn starter
main:
    steps = ["git commit -m \"First version\"", "git init", "git add ."]
end
```

```text expect
git init
git add .
git commit -m "First version"
```

> Hint: Print `steps[1]`, `steps[2]`, `steps[0]`.

<details><summary>Solution</summary>

```vyrn solution
main:
    steps = ["git commit -m \"First version\"", "git init", "git add ."]
    each i in [1, 2, 0]:
        print(steps[i])
    end
end
```

</details>

`\"` puts a double quote inside a double-quoted string.

## 15.5 Project: the Smoothie Bar

A smoothie bar has a menu, stock and a till. Each order needs the right
ingredients and enough money. The commands are a smoothie name, `report`
(show stock and takings) and `off` (close).

In a project you'd split this into `menu.vyrn`, `stock.vyrn` and
`main.vyrn`; here it's one file so it can be checked.

### Exercise

The checker orders a `berry` paying 5, a `mango` paying 3 (not enough), a
`green` paying 5, then `report` and `off`.

```vyrn starter
const MENU = {
    "berry": {"cost": 3.5, "needs": {"berries": 150, "yogurt": 100}},
    "mango": {"cost": 4.0, "needs": {"mango": 200, "yogurt": 50}},
    "green": {"cost": 4.5, "needs": {"spinach": 80, "mango": 100}},
}

create can_make(stock, needs):
    return true
end

create make(stock, needs):
end

main:
    stock = {"berries": 300, "yogurt": 200, "mango": 250, "spinach": 100}
    till = 0.0
end
```

```text input
berry
5
mango
3
green
5
report
off
```

```text expect
What would you like? (berry/mango/green) Insert money: Here is your berry smoothie! Change: 1.50
What would you like? (berry/mango/green) Insert money: That's not enough: 4.00 needed. Refunded.
What would you like? (berry/mango/green) Insert money: Here is your green smoothie! Change: 0.50
What would you like? (berry/mango/green) Stock: {"berries": 150, "yogurt": 100, "mango": 150, "spinach": 20}
Takings: 8.00
What would you like? (berry/mango/green) Closing up. Goodbye!
```

> Hint: `can_make` loops over `needs.items()` and returns `false` (after saying what's missing) if `stock[item] < amount`.
> Hint: `make` subtracts each amount: maps are shared, so `main` sees the change.
> Hint: In `main`, a `loop:` reads `What would you like? (berry/mango/green) `; `off` stops; `report` prints and `skip`s.

<details><summary>Solution</summary>

```vyrn solution
const MENU = {
    "berry": {"cost": 3.5, "needs": {"berries": 150, "yogurt": 100}},
    "mango": {"cost": 4.0, "needs": {"mango": 200, "yogurt": 50}},
    "green": {"cost": 4.5, "needs": {"spinach": 80, "mango": 100}},
}

create can_make(stock, needs):
    each item, amount in needs.items():
        if stock[item] < amount:
            print("Sorry, not enough {item}.")
            return false
        end
    end
    return true
end

create make(stock, needs):
    each item, amount in needs.items(): stock[item] -= amount
end

main:
    stock = {"berries": 300, "yogurt": 200, "mango": 250, "spinach": 100}
    till = 0.0
    loop:
        order = input("What would you like? (berry/mango/green) ")
        if order == "off": stop
        if order == "report":
            print("Stock: {stock}")
            print("Takings: {till:.2f}")
            skip
        end
        if order not in MENU:
            print("We don't make {order}.")
            skip
        end
        drink = MENU[order]
        paid = float(input("Insert money: "))
        if paid < drink["cost"]:
            print("That's not enough: {drink["cost"]:.2f} needed. Refunded.")
            skip
        end
        if not can_make(stock, drink["needs"]): skip
        make(stock, drink["needs"])
        till += drink["cost"]
        print("Here is your {order} smoothie! Change: {paid - drink["cost"]:.2f}")
    end
    print("Closing up. Goodbye!")
end
```

</details>

**Day 15 done.** You can organise code into projects and modules, use
packages, and keep your history in Git. Tomorrow: objects, which bundle
data and the code that works on it.
