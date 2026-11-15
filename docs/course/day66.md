# Day 66: Resources

Some things a program uses must be given back: an open file, a database
connection, a network socket, a lock, a temporary folder. Forget to close
a file and its last lines may never be written; forget to close
connections and a server runs out of them after a few thousand visitors.
The hard part isn't remembering to close: it's closing on *every* way out,
including errors and early `return`s. Today you learn the two tools that
make that automatic, `using` and `defer`, and how to make your own types
work with them. The project is a safe file-saving helper that never leaves
a half-written file behind.

## 66.1 The problem

Here is a careful-looking function that leaks:

```vyrn
object Connection:
    name: str
    create close(self):
        print("closed", self.name)
    end
end

create count_users(name: str) -> int:
    let conn = Connection(name)
    print("opened", name)
    if name == "backup": return 0
    let n = len(name) * 10
    conn.close()
    return n
end

main:
    print(count_users("main"))
    print(count_users("backup"))
end
```

```text
opened main
closed main
40
opened backup
0
```

The early `return` skips `close()`: the backup connection is never closed.
An error halfway through would skip it too. Every new `return` or `fail`
added later is another chance to forget.

## 66.2 `using`

`using` opens something for the length of a block and closes it when the
block ends, *however* it ends: normally, by `return`, by `stop` or `skip`
in a loop, or by an error:

```vyrn
object Connection:
    name: str
    create close(self):
        print("closed", self.name)
    end
end

create connect(name: str) -> Connection:
    print("opened", name)
    return Connection(name)
end

create count_users(name: str) -> int:
    using connect(name) as conn:
        if name == "backup": return 0
        if name == "broken": fail ValueError("no such database")
        return len(name) * 10
    end
end

main:
    print(count_users("main"))
    print(count_users("backup"))
    attempt:
        count_users("broken")
    catch ValueError as e:
        print("error:", e.message)
    end
end
```

```text
opened main
closed main
40
opened backup
closed backup
0
opened broken
closed broken
error: no such database
```

Any value with a `close()` method works with `using`: files from `open`,
databases from `db.open`, and your own objects. Several resources can be
opened in one `using`, separated by commas; they close in the opposite
order, last opened first closed:

```vyrn
object Res:
    name: str
    create close(self):
        print("close", self.name)
    end
end

main:
    using Res("source") as a, Res("copy") as b:
        print("copying", a.name, "to", b.name)
    end
end
```

```text
copying source to copy
close copy
close source
```

This is Python's `with`, C#'s `using`, and Java's try-with-resources. In
C++ and Rust the same idea is called **RAII**: a resource belongs to a
scope and is released when the scope ends.

## 66.3 `defer`

Sometimes the clean-up isn't a `close()` method: you want to delete a
temporary file, print a timing, or put a setting back. `defer` runs any
statement when the current block ends, however it ends:

```vyrn
use vyrn.fs

create convert(text: str) -> str:
    write_text("scratch.tmp", text)
    defer fs.remove("scratch.tmp")
    print("scratch exists:", fs.exists("scratch.tmp"))
    if text == "": fail ValueError("nothing to convert")
    return read_text("scratch.tmp").upper()
end

main:
    print(convert("hello"))
    print("after:", fs.exists("scratch.tmp"))
    attempt:
        convert("")
    catch ValueError:
        print("failed, and after:", fs.exists("scratch.tmp"))
    end
end
```

```text
scratch exists: true
HELLO
after: false
scratch exists: true
failed, and after: false
```

Put the `defer` right after the thing it undoes, so the opening and the
clean-up sit together. Several `defer`s run in reverse order, like
`using`: the last set up is the first undone.

### Exercise

`log_lines` sets an indent and must put it back however it ends. Use
`defer` so the indent is always restored, even when a line is too long.

```vyrn starter
object Printer:
    indent: int = 0

    create line(self, text: str):
        print(" " * self.indent + text)
    end
end

create log_lines(p: Printer, lines: list[str]):
    p.indent += 2
    each l in lines:
        if len(l) > 10: fail ValueError("too long: {l}")
        p.line(l)
    end
    p.indent -= 2
end

main:
    let p = Printer()
    log_lines(p, ["a", "b"])
    attempt:
        log_lines(p, ["c", "much too long"])
    catch ValueError as e:
        p.line(e.message)
    end
end
```

```text expect
  a
  b
  c
too long: much too long
```

> Hint: replace the last line of `log_lines` with `defer p.indent -= 2`
> right after `p.indent += 2`.

<details><summary>Solution</summary>

```vyrn solution
object Printer:
    indent: int = 0

    create line(self, text: str):
        print(" " * self.indent + text)
    end
end

create log_lines(p: Printer, lines: list[str]):
    p.indent += 2
    defer p.indent -= 2
    each l in lines:
        if len(l) > 10: fail ValueError("too long: {l}")
        p.line(l)
    end
end

main:
    let p = Printer()
    log_lines(p, ["a", "b"])
    attempt:
        log_lines(p, ["c", "much too long"])
    catch ValueError as e:
        p.line(e.message)
    end
end
```

</details>

## 66.4 Making your own resources

Give a type a `close` method and it works with `using`. A good resource
type also refuses to be used after it's closed, so mistakes show up
straight away instead of as strange behaviour later:

```vyrn
object Logger:
    path: str
    lines: list[str] = []
    closed: bool = false

    create write(self, text: str):
        if self.closed: fail ValueError("the log {self.path} is closed")
        self.lines.append(text)
    end

    create close(self):
        if self.closed: return
        write_text(self.path, "\n".join(self.lines) + "\n")
        self.closed = true
    end
end

main:
    var kept = none
    using Logger("run.log") as log:
        log.write("started")
        log.write("finished")
        kept = log
    end
    print(read_text("run.log").strip().split("\n"))
    attempt:
        kept.write("too late")
    catch ValueError as e:
        print(e.message)
    end
end
```

```text
["started", "finished"]
the log run.log is closed
```

Two habits worth copying: `close` does nothing the second time (closing
twice is harmless), and every other method checks that it's still open.

## 66.5 Project: saving files safely

If a program crashes while saving a file, the file is left half-written:
the old version is gone and the new one is broken. The safe way is to
write to a temporary file, and only when it's complete, rename it over the
real one in a single step. A resource object makes that easy to use
correctly:

```vyrn
use vyrn.fs

object SafeWriter:
    path: str
    parts: list[str] = []
    done: bool = false

    create write(self, text: str):
        self.parts.append(text)
    end

    ## Call when everything is written; until then, close throws it away.
    create commit(self):
        write_text(self.path + ".tmp", "".join(self.parts))
        fs.rename(self.path + ".tmp", self.path)
        self.done = true
    end

    create close(self):
        if not self.done: print("(not committed: {self.path} left as it was)")
    end
end

create save_scores(path: str, scores: map) -> bool:
    attempt:
        using SafeWriter(path) as out:
            each name, score in scores.items():
                if score < 0: fail ValueError("bad score for {name}")
                out.write("{name},{score}\n")
            end
            out.commit()
        end
        return true
    catch ValueError as e:
        print("not saved:", e.message)
        return false
    end
end

main:
    save_scores("scores.csv", {"ada": 30, "alan": 25})
    print(read_text("scores.csv").strip())
    save_scores("scores.csv", {"ada": 40, "grace": -1})
    print(read_text("scores.csv").strip())
end
```

```text
ada,30
alan,25
(not committed: scores.csv left as it was)
not saved: bad score for grace
ada,30
alan,25
```

The second save fails halfway, and the file still holds the first, good
version. Databases use the same idea, a **transaction**: `conn.begin()`,
then `commit()` if everything worked, or `rollback()` if not.

### Exercise

Write a `Timer` resource that prints how long its block took, using the
clock values it's given (so the lesson's output is the same every run).
`Timer(label, clock)` calls `clock()` when made and again on `close`.

```vyrn starter
object Timer:
    label: str
    clock: any
end

main:
    let clock = fake(returns_each = [10.0, 12.5])
    print("nothing timed yet")
end
```

```text expect
working...
build took 2.5 s
```

> Hint: give `Timer` a `start: float = 0.0` field, and a `begin(self)`
> method that sets it from `self.clock()` and returns `self`.
> Hint: `using Timer("build", clock).begin() as t:` then `print("working...")`;
> `close` prints `self.clock() - self.start`.

<details><summary>Solution</summary>

```vyrn solution
object Timer:
    label: str
    clock: any
    start: float = 0.0

    create begin(self) -> Timer:
        self.start = self.clock()
        return self
    end

    create close(self):
        print("{self.label} took {self.clock() - self.start} s")
    end
end

main:
    let clock = fake(returns_each = [10.0, 12.5])
    using Timer("build", clock).begin() as t:
        print("working...")
    end
end
```

</details>

## Today you learned

- Resources (files, connections, locks) must be given back on every way
  out, including errors and early returns.
- `using x as name:` closes `x` when the block ends, however it ends;
  several close last first.
- `defer statement` runs any clean-up when the block ends.
- A type with `close()` works with `using`; make `close` safe to call twice
  and refuse use after closing.
- Write to a temporary file and rename it to save safely.
