# Day 76: Borrowing other languages' libraries

No language has a library for everything. Python has decades of science
and machine-learning libraries; C has fast, battle-tested libraries for
compression, images, audio and more, and is the language operating
systems speak. Vyrn can call both directly. Today you learn when that's
worth it, how to call C functions and Python modules, and the simplest
bridge of all, running another program. The project uses a Python library
from a Vyrn program.

## 76.1 Three ways to borrow

| Way | Use when | Cost |
|---|---|---|
| run another program (`os.run`) | a tool already does the job from the command line | slowest per call; simplest; nothing to build |
| `use python.x` | a Python library does the job (NumPy, pandas, scikit-learn, ...) | needs Python installed; values are converted at the boundary |
| `extern "c"` | a C library does the job, or you need the operating system | fastest; you must describe each function exactly; mistakes can crash |

Try them in that order. Running a program is often all you need, and it
can't crash yours.

## 76.2 Running another program

`os.run(command)` runs a program and gives back its exit code and what it
printed. A list (`["git", "log", "-1"]`) passes each argument exactly as
written, with no shell to misread spaces or quotes; use that form for
anything that includes outside data:

```vyrn check
use vyrn.os

main:
    let r = os.run(["git", "log", "-1", "--format=%an: %s"])
    if r.ok:
        print("last commit by", r.out.strip())
    else:
        print("git failed:", r.err.strip())
    end
end
```

Programs that can print JSON are easiest to use this way, since the output
is data, not text to pick apart:

```vyrn
use vyrn.json

create disk_report(output: str) -> list[str]:
    let data = json.parse(output)
    return ["{d["name"]}: {d["used_pct"]}% used" each d in data["disks"] if d["used_pct"] > 80]
end

main:
    let recorded = r'{"disks": [{"name": "/", "used_pct": 91}, {"name": "/data", "used_pct": 40}]}'
    print(disk_report(recorded))
end
```

```text
["/: 91% used"]
```

## 76.3 Python libraries

`use python.numpy as np` imports a Python module, and from then on it's
used like a Vyrn module. Numbers, text, lists and maps are converted both
ways; Python objects stay Python objects, with their attributes and methods
available. Vyrn tensors and NumPy arrays share memory, so big arrays cross
without being copied:

```vyrn check
use python.statistics
use python.numpy as np

main:
    let marks = [62, 71, 75, 80, 88, 94]
    print(statistics.median(marks), statistics.stdev(marks))
    let a = np.array([[1, 2], [3, 4]])
    print(np.linalg.det(a))
    let t = a.to_tensor()
    print(t * 2)
end
```

This needs a Vyrn built with Python support, and Python installed with the
libraries you use:

```bash
cargo build --release --features python
```

The standard downloads of Vyrn leave Python support out, so that Vyrn
doesn't need Python on every computer. Without it, a program that uses
Python stops with a clear message saying how to get it.

## 76.4 C libraries

C functions are described in an `extern "c"` block: their names, the types
of their parameters and their result. Vyrn finds them in the library named
by `@link` (or in the standard C library without one) and calls them
directly, as fast as C calls C:

```vyrn check
extern "c":
    create abs(x: i32) -> i32
    create strlen(s: str) -> u64
end

@link("m")
extern "c":
    create cbrt(x: f64) -> f64
end

main:
    print(abs(-7), strlen("hello"), cbrt(27.0))
end
```

The types must match the C declaration *exactly*: `i32` is C's `int`,
`i64` is `long long`, `f64` is `double`, `str` is a `const char*`. Get one
wrong and the result is garbage, or a crash, which is why calling C needs
care. Functions that take or return raw pointers (`ptr u8`) can only be
called inside an `unsafe:` block: by writing `unsafe`, you promise the C
code only touches what it's given. This needs the `ffi` build feature:

```bash
cargo build --release --features ffi
```

## 76.5 Wrapping a library

Whatever the bridge, keep it at the edge of your program, behind a few
plain Vyrn functions. The rest of the program then doesn't know or care
that Python or C is involved, can be tested with fakes, and keeps working
if you later swap the library for another:

```vyrn
## The only place that knows how sentiment is scored. For real, `score`
## calls a Python library; tests pass a fake.
create sentiment(texts: list[str], score: any) -> list[str]:
    var labels = []
    each t in texts:
        let s = score(t)
        labels.append("positive" if s > 0.2 else ("negative" if s < -0.2 else "neutral"))
    end
    return labels
end

main:
    let fake_score = fake(returns_each = [0.9, -0.6, 0.0])
    print(sentiment(["I love it", "awful service", "it arrived"], fake_score))
end
```

```text
["positive", "negative", "neutral"]
```

## 76.6 Project: a Python library from Vyrn

The project uses Python's `difflib` to find the closest match for a
mistyped word, a feature that would take a page of code to write well.
The bridge is one small function; everything else is Vyrn:

```vyrn check
use python.difflib

create closest(word: str, choices: list[str]) -> str?:
    let found = difflib.get_close_matches(word, choices, n = 1, cutoff = 0.6)
    return found[0] if found else none
end

main:
    let commands = ["list", "add", "remove", "check", "help"]
    each typed in ["lsit", "remvoe", "chek", "xyzzy"]:
        let guess = closest(typed, commands)
        print(typed, "->", "did you mean {guess}?" if guess != none else "unknown command")
    end
end
```

With a Python-enabled Vyrn it prints `lsit -> did you mean list?` and so
on. Vyrn's own error messages do the same thing (misspell a variable and
it says "`totl` is not defined ... did you mean `total`?"), with code
written in Rust: the same idea, from another language's library.

### Exercise

Without Python, write the core of `closest` yourself: the number of
positions where two equal-length words differ, then pick the command with
the fewest differences (only among commands of the same length as the
typed word).

```vyrn starter
create differences(a: str, b: str) -> int:
    return 0
end

create closest(word: str, choices: list[str]) -> str?:
    return none
end

main:
    print(closest("lsit", ["list", "add", "help"]), closest("hepl", ["list", "add", "help"]), closest("x", ["list"]))
end
```

```text expect
list help none
```

> Hint: `differences` counts `i` where `a[i] != b[i]`.
> Hint: in `closest`, keep choices with `len(c) == len(word)`; give
> `none` if there are none, else `min(same_length, key = c => differences(word, c))`.

<details><summary>Solution</summary>

```vyrn solution
create differences(a: str, b: str) -> int:
    return len([i each i in 0 until len(a) if a[i] != b[i]])
end

create closest(word: str, choices: list[str]) -> str?:
    let same_length = [c each c in choices if len(c) == len(word)]
    if not same_length: return none
    return min(same_length, key = c => differences(word, c))
end

main:
    print(closest("lsit", ["list", "add", "help"]), closest("hepl", ["list", "add", "help"]), closest("x", ["list"]))
end
```

</details>

## Today you learned

- Borrow before you build: run a program (`os.run` with a list), use a
  Python module (`use python.x`), or call C (`extern "c"`), in that order
  of simplicity.
- Python and C support are build features (`--features python`,
  `--features ffi`); without them Vyrn says how to get them.
- C types must match exactly; raw pointers need `unsafe:`.
- Keep the bridge in a few small functions at the edge, so the rest can be
  tested with fakes.
