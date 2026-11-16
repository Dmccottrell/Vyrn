# Day 80: Portfolio project: a command-line tool

The last five days are yours. Each is a **project brief**: what to build,
what "done" means, and a suggested route, but no step-by-step code. Build
it in a project of its own (`vyrn new`), commit as you go, and put it on
GitHub when it's finished: these are the programs to show people.

Today's brief: a **command-line tool** that does one job well, the way
`git`, `curl` and `vyrn` itself do.

## 80.1 The brief

Pick a job you actually have. Some ideas:

- `notes`: quick notes from the terminal (`notes add "buy milk"`,
  `notes list`, `notes search milk`).
- `tidy`: sort a downloads folder into subfolders by file type.
- `budget`: read a bank's CSV export and report spending by category.
- `rename`: rename many files at once by a pattern (`IMG_0042.jpg` to
  `2026-11-12 holiday 042.jpg`).

## 80.2 Done means

- [ ] Commands and options from `os.args()`, with a `help` command that
      lists them, and a clear message for anything unknown.
- [ ] Exit code 0 on success, not 0 on failure (`os.exit`), so scripts can
      use it.
- [ ] Never loses data: anything destructive asks first, or has a
      `--dry-run` that shows what would happen.
- [ ] Settings or data stored in a file in the user's home folder
      (`path.home()`), surviving a damaged file.
- [ ] Tests (`vyrn test`) for every command, with the file system or
      clock passed in where needed.
- [ ] `vyrn check` and `vyrn fmt --check` are clean.
- [ ] Built with `vyrn build`, and a README saying what it does, how to
      install it, and an example of each command.

## 80.3 A suggested route

1. Write the README first, with examples of using the tool. It's the
   design.
2. Parse the arguments into an enum of commands (Day 61), tested on its
   own: `parse(["add", "buy milk"])` gives `Command.Add("buy milk")`.
3. One function per command, taking what it needs (a storage object, the
   clock) as parameters.
4. `main` is three lines: parse, run, exit with the code.

## 80.4 Warm-up

Most tools take flags like `--dry-run` and `--limit=5`. Write
`split_flags(args)`, separating flags from the other words, as a map and
a list.

```vyrn starter
create split_flags(args: list[str]) -> (map, list[str]):
    return ({}, args)
end

main:
    print(split_flags(["tidy", "--dry-run", "Downloads", "--limit=5"]))
end
```

```text expect
({"dry-run": true, "limit": "5"}, ["tidy", "Downloads"])
```

> Hint: a word starting with `--` is a flag; split it on the first `=`
> for a value, else its value is `true`.

<details><summary>Solution</summary>

```vyrn solution
create split_flags(args: list[str]) -> (map, list[str]):
    var flags = {}
    var words = []
    each a in args:
        if a.startswith("--"):
            let parts = a[2:].split("=", 1)
            flags[parts[0]] = parts[1] if len(parts) == 2 else true
        else:
            words.append(a)
        end
    end
    return (flags, words)
end

main:
    print(split_flags(["tidy", "--dry-run", "Downloads", "--limit=5"]))
end
```

</details>

## 80.5 Going further

- Colour in the output, and a `--quiet` flag for scripts.
- Publish it as a package (`vyrn publish`) so others can `vyrn add` it.
- Shell completion: a `completions` command that prints a script for bash.
