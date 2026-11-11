# Day 24: Files

Everything a program keeps in variables disappears when it ends. **Files**
keep data between runs: notes, high scores, settings, documents. Today you
write, read and add to files, work with folders and paths, handle binary
files like images, and build a mail-merge tool that writes personalised
letters.

## 24.1 Writing and reading a whole file

```vyrn
main:
    write_text("hello.txt", "Hello from Vyrn!\nSecond line.\n")
    text = read_text("hello.txt")
    print(text)
    print(len(text.lines()), "lines")
end
```

```text
Hello from Vyrn!
Second line.

2 lines
```

- `write_text(path, text)` creates the file (or replaces it) with the text.
- `read_text(path)` gives the whole file as one string.
- `text.lines()` splits it into a list of lines.

The file appears in the folder you ran the program from. A **path** like
`"hello.txt"` is relative to that folder; `"notes/today.txt"` points into a
sub-folder.

### Exercise

Save three high scores, one per line, then read them back and print the
best.

```vyrn starter
main:
    scores = [120, 340, 95]
end
```

```text expect
Best: 340
```

> Hint: `"\n".join([str(s) each s in scores])` makes the file's text.
> Hint: Read it back, `.lines()`, convert each with `int`, then `max`.

<details><summary>Solution</summary>

```vyrn solution
main:
    scores = [120, 340, 95]
    write_text("scores.txt", "\n".join([str(s) each s in scores]))
    saved = [int(line) each line in read_text("scores.txt").lines()]
    print("Best:", max(saved))
end
```

</details>

## 24.2 Adding to a file

`append_text` adds to the end instead of replacing:

```vyrn
main:
    write_text("log.txt", "")
    append_text("log.txt", "started\n")
    append_text("log.txt", "loaded 3 files\n")
    append_text("log.txt", "finished\n")
    print(read_text("log.txt"))
end
```

```text
started
loaded 3 files
finished

```

### Exercise

Keep a diary: append each entry as `<day>: <text>`, then print the file.

```vyrn starter
main:
    write_text("diary.txt", "")
    entries = [("Mon", "Started the course"), ("Tue", "Wrote my first loop")]
end
```

```text expect
Mon: Started the course
Tue: Wrote my first loop
```

> Hint: `each day, text in entries: append_text("diary.txt", "{day}: {text}\n")`

<details><summary>Solution</summary>

```vyrn solution
main:
    write_text("diary.txt", "")
    entries = [("Mon", "Started the course"), ("Tue", "Wrote my first loop")]
    each day, text in entries:
        append_text("diary.txt", "{day}: {text}\n")
    end
    print(read_text("diary.txt").rstrip())
end
```

</details>

## 24.3 Opening files with `using`

For big files, or to write a little at a time, **open** the file. `using`
opens it, and closes it again at `end`, even if something fails in
between:

```vyrn
main:
    using open("shopping.txt", "w") as f:
        f.write("milk\n")
        f.write("bread\n")
        f.write("tea\n")
    end
    using open("shopping.txt") as f:
        each line in f:
            print("-", line)
        end
    end
end
```

```text
- milk
- bread
- tea
```

The mode says what you'll do: `"r"` read (the default), `"w"` write
(replacing), `"a"` append. Looping over an open file gives one line at a
time, without its line break, without loading the whole file into memory.

`using` works for anything that must be closed: files, database
connections, network sockets. Day 77 shows how to make your own objects
work with it.

### Exercise

Copy only the lines that contain `TODO` from one file to another.

```vyrn starter
main:
    write_text("code.txt", "x = 1\n# TODO: name this better\ny = 2\n# TODO: add tests\n")
end
```

```text expect
# TODO: name this better
# TODO: add tests
```

> Hint: Open `code.txt` for reading and `todos.txt` for writing, both with `using`.
> Hint: `if "TODO" in line: out.write(line + "\n")`, then print `read_text("todos.txt")`.

<details><summary>Solution</summary>

```vyrn solution
main:
    write_text("code.txt", "x = 1\n# TODO: name this better\ny = 2\n# TODO: add tests\n")
    using open("code.txt") as source:
        using open("todos.txt", "w") as out:
            each line in source:
                if "TODO" in line: out.write(line + "\n")
            end
        end
    end
    print(read_text("todos.txt").rstrip())
end
```

</details>

## 24.4 When the file isn't there

Reading a missing file fails. Check first with `fs.exists`, or catch the
error:

```vyrn
use vyrn.fs

main:
    print(fs.exists("missing.txt"))
    attempt:
        print(read_text("missing.txt"))
    catch Error as e:
        print("Could not read it:", e.kind)
    end
end
```

```text
false
Could not read it: IoError
```

### Exercise

Write `load_count(path)` that returns the number saved in the file, or 0
if there is no file yet: the classic "first run" case. Then count three
runs.

```vyrn starter
use vyrn.fs

create load_count(path):
    return 0
end

main:
    each _ in 1 to 3:
        count = load_count("runs.txt") + 1
        write_text("runs.txt", str(count))
        print("This is run", count)
    end
end
```

```text expect
This is run 1
This is run 2
This is run 3
```

> Hint: `if not fs.exists(path): return 0`, then `return int(read_text(path))`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.fs

create load_count(path):
    if not fs.exists(path): return 0
    return int(read_text(path))
end

main:
    each _ in 1 to 3:
        count = load_count("runs.txt") + 1
        write_text("runs.txt", str(count))
        print("This is run", count)
    end
end
```

</details>

## 24.5 Folders and paths

```vyrn
use vyrn.fs
use vyrn.path

main:
    fs.mkdir("reports")
    each name in ["jan", "feb"]:
        write_text(path.join("reports", "{name}.txt"), "report for {name}")
    end
    print(sorted(fs.list("reports")))
    p = path.join("reports", "jan.txt")
    print(p, path.name(p), path.stem(p), path.ext(p))
    print(fs.size(p), "bytes")
end
```

```text
["feb.txt", "jan.txt"]
reports/jan.txt jan.txt jan .txt
14 bytes
```

| Tool | Does |
|---|---|
| `fs.mkdir(dir)`, `fs.list(dir)` | make a folder; the names inside one |
| `fs.exists(p)`, `fs.is_file(p)`, `fs.is_dir(p)` | what's there |
| `fs.remove(p)`, `fs.rename(a, b)`, `fs.copy(a, b)` | delete, move, copy |
| `path.join(a, b, ...)` | a path from parts, with the right separators for the computer |
| `path.name(p)`, `path.stem(p)`, `path.ext(p)`, `path.parent(p)` | parts of a path |

Always build paths with `path.join`: Windows writes `reports\jan.txt`
while macOS and Linux write `reports/jan.txt`, and `path.join` picks the
right one.

### Exercise

Make a folder `photos`, put three empty files in it with different
extensions, and print the names of the `.png` files only.

```vyrn starter
use vyrn.fs
use vyrn.path

main:
    fs.mkdir("photos")
    each name in ["beach.png", "notes.txt", "cat.png"]:
        write_text(path.join("photos", name), "")
    end
end
```

```text expect
["beach.png", "cat.png"]
```

> Hint: `sorted([n each n in fs.list("photos") if path.ext(n) == ".png"])`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.fs
use vyrn.path

main:
    fs.mkdir("photos")
    each name in ["beach.png", "notes.txt", "cat.png"]:
        write_text(path.join("photos", name), "")
    end
    print(sorted([n each n in fs.list("photos") if path.ext(n) == ".png"]))
end
```

</details>

## 24.6 Binary files

Images, sounds and other non-text files are **binary**: sequences of
**bytes**, numbers from 0 to 255. `read_bytes` gives them as a list of
numbers and `write_bytes` writes such a list:

```vyrn
main:
    write_bytes("tiny.bin", [137, 80, 78, 71, 13, 10])
    data = read_bytes("tiny.bin")
    print(data, len(data))
    print([hex(b) each b in data])
end
```

```text
[137, 80, 78, 71, 13, 10] 6
["0x89", "0x50", "0x4e", "0x47", "0xd", "0xa"]
```

Every PNG image starts with those bytes (`0x89`, then the letters `P`,
`N`, `G`), which is how programs recognise the format.

### Exercise

Check whether a file is a PNG by looking at its first four bytes.

```vyrn starter
create is_png(path):
    return false
end

main:
    write_bytes("a.png", [137, 80, 78, 71, 13, 10, 26, 10])
    write_text("b.png", "not really")
    print(is_png("a.png"), is_png("b.png"))
end
```

```text expect
true false
```

> Hint: `read_bytes(path)[:4] == [137, 80, 78, 71]`

<details><summary>Solution</summary>

```vyrn solution
create is_png(path):
    return read_bytes(path)[:4] == [137, 80, 78, 71]
end

main:
    write_bytes("a.png", [137, 80, 78, 71, 13, 10, 26, 10])
    write_text("b.png", "not really")
    print(is_png("a.png"), is_png("b.png"))
end
```

</details>

## 24.7 Project: Mail Merge

A **mail merge** fills one template letter with many names. The template
says `[name]` where each name goes; the program writes one letter per
person into a folder.

### Exercise

```vyrn starter
use vyrn.fs
use vyrn.path

main:
    write_text("template.txt", "Dear [name],\nYou are invited to the party on Saturday!\nBest wishes\n")
    write_text("names.txt", "Ada\nGrace\nLinus\n")
end
```

```text expect
["letter_Ada.txt", "letter_Grace.txt", "letter_Linus.txt"]
Dear Grace,
You are invited to the party on Saturday!
Best wishes
```

> Hint: Read the template once, then for each name, `template.replace("[name]", name)`.
> Hint: Write each letter to `path.join("letters", "letter_{name}.txt")` after `fs.mkdir("letters")`.
> Hint: Print `sorted(fs.list("letters"))`, then Grace's letter.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.fs
use vyrn.path

main:
    write_text("template.txt", "Dear [name],\nYou are invited to the party on Saturday!\nBest wishes\n")
    write_text("names.txt", "Ada\nGrace\nLinus\n")
    template = read_text("template.txt")
    fs.mkdir("letters")
    each name in read_text("names.txt").lines():
        letter = template.replace("[name]", name.strip())
        write_text(path.join("letters", "letter_{name}.txt"), letter)
    end
    print(sorted(fs.list("letters")))
    print(read_text(path.join("letters", "letter_Grace.txt")).rstrip())
end
```

</details>

**Day 24 done.** Your programs can now remember things between runs.
Tomorrow: CSV files and tables, the everyday format for data.
