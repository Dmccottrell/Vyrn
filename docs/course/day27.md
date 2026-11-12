# Day 27: Apps with buttons, and functions that take any number of things

Today has two parts. First, two small features that make functions
friendlier: a function that takes **any number** of arguments, and
**spreading** a list into a call. Then you build a desktop **app**: a
window with text fields, buttons and a slider, that converts between
units.

## 27.1 Any number of arguments: `args...`

`print` takes as many things as you give it. Your functions can too: put
`...` after the last parameter's name, and it collects every remaining
argument into a list.

```vyrn
create total(numbers...) -> int:
    return sum(numbers)
end

create greet(greeting: str, names...):
    each name in names:
        print(greeting, name)
    end
end

main:
    print(total(1, 2, 3))
    print(total(10))
    print(total())
    greet("Hi", "Ada", "Grace", "Alan")
end
```

```text
6
10
0
Hi Ada
Hi Grace
Hi Alan
```

`numbers` is an ordinary list inside the function. A rest parameter comes
last, after the ordinary ones, which are filled first: `greeting` gets
`"Hi"`, and `names` gets the rest.

### Exercise

Write `longest(words...)`, which gives the longest word it is given.

```vyrn starter
create longest(words...) -> str:
    return ""
end

main:
    print(longest("fig", "banana", "kiwi"))
end
```

```text expect
banana
```

> Hint: `max(words, key = w => len(w))`

<details><summary>Solution</summary>

```vyrn solution
create longest(words...) -> str:
    return max(words, key = w => len(w))
end

main:
    print(longest("fig", "banana", "kiwi"))
end
```

</details>

## 27.2 Spreading a list into a call: `xs...`

The other way round: when the values are already in a list, put `...`
after it in the call, and its items become separate arguments.

```vyrn
create area(width: float, height: float) -> float:
    return width * height
end

create total(numbers...) -> int:
    return sum(numbers)
end

main:
    size = [3, 4]
    print(area(size...))
    scores = [10, 20, 30]
    print(total(scores...))
    print(total(1, scores..., 100))
end
```

```text
12
60
161
```

`area(size...)` is the same as `area(3, 4)`. Spreading and rest parameters
go well together: a function can pass on everything it was given with
`other(args...)`.

### Exercise

Finish `log`, which prints its level in brackets and then all its other
arguments, by passing them on to `print`.

```vyrn starter
create log(level: str, parts...):
    print(level)
end

main:
    log("INFO", "loaded", 3, "files")
end
```

```text expect
[INFO] loaded 3 files
```

> Hint: `print("[" + level + "]", parts...)`

<details><summary>Solution</summary>

```vyrn solution
create log(level: str, parts...):
    print("[" + level + "]", parts...)
end

main:
    log("INFO", "loaded", 3, "files")
end
```

</details>

## 27.3 A window with widgets

Day 19 opened windows and drew in them. Apps add **widgets**: things to
click and type into. Each frame, you ask for each widget, and it draws
itself and tells you what the user did:

| Widget | Gives |
|---|---|
| `win.label(text)` | nothing; shows text |
| `win.input(name, hint = "...")` | the text typed into it so far |
| `win.button(label)` | `true` on the frame it is clicked |
| `win.checkbox(label, checked = x)` | the new state, flipped when clicked |
| `win.slider(name, low, high, value)` | the new value, while it is dragged |

Leave out x and y, and widgets **lay themselves out**, one under another.
Between `win.row()` and `win.end_row()` they go side by side.

```vyrn check
use vyrn.draw

main:
    let win = draw.window(360, 220, "Greeter")
    var message = ""
    loop win.open():
        win.clear("#263238")
        win.layout(20, 20, gap = 12)
        win.label("What's your name?")
        let name = win.input("name", hint = "Type here")
        win.row()
        if win.button("Greet"): message = "Hello, {name}!"
        if win.button("Clear"):
            message = ""
            win.set_input("name", "")
        end
        win.end_row()
        win.label(message, color = "#a5d6a7")
        win.show()
    end
end
```

Run it with `vyrn run`, click the field, type, and press **Greet**.

This style is called **immediate mode**: there are no widget objects to
create and keep. Every frame you simply say what should be on screen, and
the code reads top to bottom like the window looks.

## 27.4 Project: a unit converter

Good apps keep the **logic** (converting) apart from the **window** (typing
and clicking). The logic is plain functions you can test without a window:

```vyrn
## How many metres each unit is.
const METRES = {"mm": 0.001, "cm": 0.01, "m": 1.0, "km": 1000.0, "in": 0.0254, "ft": 0.3048, "mi": 1609.344}

create convert(value: float, from: str, to: str) -> float:
    return value * METRES[from] / METRES[to]
end

main:
    print(convert(5, "km", "mi").round(3))
    print(convert(6, "ft", "cm").round(1))
    print(convert(1, "mi", "ft").round(0))
end
```

```text
3.107
182.9
5280.0
```

Every unit goes through metres: first to metres, then from metres to the
target. That is why one table of seven numbers covers all 49 pairs.

The app around it:

```vyrn check
use vyrn.draw

const METRES = {"mm": 0.001, "cm": 0.01, "m": 1.0, "km": 1000.0, "in": 0.0254, "ft": 0.3048, "mi": 1609.344}
const UNITS = ["mm", "cm", "m", "km", "in", "ft", "mi"]

create convert(value: float, from: str, to: str) -> float:
    return value * METRES[from] / METRES[to]
end

main:
    let win = draw.window(520, 260, "Unit converter")
    var from = "km"
    var to = "mi"
    loop win.open():
        win.clear("#1e2a38")
        win.layout(20, 20, gap = 10)
        let text = win.input("value", hint = "A number")
        win.row()
        win.label("from")
        each unit in UNITS:
            if win.button(unit, w = 44): from = unit
        end
        win.end_row()
        win.row()
        win.label("to  ")
        each unit in UNITS:
            if win.button(unit, w = 44): to = unit
        end
        win.end_row()
        attempt:
            let result = convert(float(text), from, to)
            win.label("{text} {from} = {result:.4f} {to}", color = "#ffe082")
        catch ValueError:
            win.label("Type a number to convert", color = "#90a4ae")
        end
        win.show()
    end
end
```

`attempt`/`catch` (Day 13) handles the moments when the field is empty or
holds something that isn't a number: instead of crashing, the app shows a
hint.

### Exercise

Temperatures can't go through one table, because 0 °C is not 0 °F. Write
`to_celsius(value, unit)` for `"C"`, `"F"` and `"K"`, then
`convert_temp(value, from, to)` that goes through Celsius.

```vyrn starter
create to_celsius(value: float, unit: str) -> float:
    return value
end

create from_celsius(c: float, unit: str) -> float:
    when unit:
        is "F": return c * 9 / 5 + 32
        is "K": return c + 273.15
        else: return c
    end
end

create convert_temp(value: float, from: str, to: str) -> float:
    return value
end

main:
    print(convert_temp(212, "F", "C"), convert_temp(0, "C", "K"), convert_temp(300, "K", "F").round(2))
end
```

```text expect
100.0 273.15 80.33
```

> Hint: `to_celsius` undoes `from_celsius`: for `"F"` it is
> `(value - 32) * 5 / 9`, for `"K"` it is `value - 273.15`.
> Hint: `convert_temp` is `from_celsius(to_celsius(value, from), to)`.

<details><summary>Solution</summary>

```vyrn solution
create to_celsius(value: float, unit: str) -> float:
    when unit:
        is "F": return (value - 32) * 5 / 9
        is "K": return value - 273.15
        else: return value
    end
end

create from_celsius(c: float, unit: str) -> float:
    when unit:
        is "F": return c * 9 / 5 + 32
        is "K": return c + 273.15
        else: return c
    end
end

create convert_temp(value: float, from: str, to: str) -> float:
    return from_celsius(to_celsius(value, from), to)
end

main:
    print(convert_temp(212, "F", "C"), convert_temp(0, "C", "K"), convert_temp(300, "K", "F").round(2))
end
```

</details>

## Today you learned

- `create f(things...)` collects any number of arguments into a list.
- `f(xs...)` spreads a list's items into separate arguments.
- Widgets (`label`, `input`, `button`, `checkbox`, `slider`) are asked for
  each frame and say what the user did; without x and y they lay themselves
  out, side by side between `row()` and `end_row()`.
- Keep the logic in plain functions, apart from the window, so you can test
  it.
