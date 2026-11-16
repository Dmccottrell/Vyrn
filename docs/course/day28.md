# Day 28: Time, timers and events

Programs often need to wait: a game moves things 60 times a second, a
reminder goes off in 25 minutes, a quiz gives you 10 seconds to answer.
Today you measure time, wait for it, show it as `mm:ss`, run steady
**ticks**, and react to **events** like key presses. The project is a
**focus timer**: 25 minutes of work, 5 of rest, repeated.

## 28.1 Waiting and measuring

`time.sleep(seconds)` pauses the program. `time.monotonic()` is a clock
that only goes forwards, for measuring how long something took:

```vyrn
use vyrn.time

main:
    start = time.monotonic()
    print("waiting...")
    time.sleep(0.2)
    took = time.monotonic() - start
    print("waited at least 0.2 seconds:", took >= 0.2)
    print("and not much more:", took < 1)
end
```

```text
waiting...
waited at least 0.2 seconds: true
and not much more: true
```

Why not use the time of day for measuring? Because the computer's clock
can be changed (daylight saving, an internet time update), and then a
measurement would be wrong. `monotonic` never jumps.

For the date and time of day there is `time.now()` and `time.format`
(Day 32 goes further):

```vyrn
use vyrn.time

main:
    moment = time.make(2026, 3, 14, 15, 9, 26)
    print(time.format(moment, "%A %d %B %Y, %H:%M"))
end
```

```text
Saturday 14 March 2026, 15:09
```

### Exercise

Time how long it takes to add up the numbers 1 to 100000, and print
whether it took less than five seconds.

```vyrn starter
use vyrn.time

main:
    total = 0
end
```

```text expect
5000050000
fast: true
```

> Hint: take `time.monotonic()` before and after the loop.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.time

main:
    total = 0
    start = time.monotonic()
    each n in 1 to 100000:
        total += n
    end
    print(total)
    print("fast:", time.monotonic() - start < 5)
end
```

</details>

## 28.2 Showing time as `mm:ss`

Timers count in seconds, but people read minutes and seconds. `//` divides
and drops the remainder; `%` gives the remainder. A format spec like
`{m:02}` pads a number to two digits with zeros:

```vyrn
create clock_text(seconds: int) -> str:
    m = seconds // 60
    s = seconds % 60
    return "{m:02}:{s:02}"
end

main:
    print(clock_text(1500))
    print(clock_text(65))
    print(clock_text(9))
end
```

```text
25:00
01:05
00:09
```

### Exercise

Extend it to hours: `h:mm:ss` when there is at least an hour, `mm:ss`
otherwise.

```vyrn starter
create clock_text(seconds: int) -> str:
    return ""
end

main:
    print(clock_text(3725), clock_text(125))
end
```

```text expect
1:02:05 02:05
```

> Hint: hours are `seconds // 3600`; minutes are then `seconds % 3600 // 60`.

<details><summary>Solution</summary>

```vyrn solution
create clock_text(seconds: int) -> str:
    h = seconds // 3600
    m = seconds % 3600 // 60
    s = seconds % 60
    if h > 0: return "{h}:{m:02}:{s:02}"
    return "{m:02}:{s:02}"
end

main:
    print(clock_text(3725), clock_text(125))
end
```

</details>

## 28.3 Steady ticks with a `Clock`

To do something many times a second, at an even pace, use a `Clock`.
`clock.tick()` waits until the next tick and gives the time a tick lasts;
`frames` stops it after that many ticks:

```vyrn
main:
    var clock = Clock(hz = 20, frames = 5)
    var elapsed = 0.0
    loop clock.running:
        elapsed += clock.tick()
        print("tick", clock.frame, "at", elapsed.round(2), "seconds")
    end
end
```

```text
tick 1 at 0.05 seconds
tick 2 at 0.1 seconds
tick 3 at 0.15 seconds
tick 4 at 0.2 seconds
tick 5 at 0.25 seconds
```

A `Clock` is how games keep moving at the same speed on fast and slow
computers. Without `frames`, it runs until you call `clock.stop()`.

## 28.4 Events: things that happen

An **event** is something the program reacts to when it happens: a key
pressed, a button clicked, a timer running out. In a window loop, you check
for them every frame:

```vyrn check
use vyrn.draw

main:
    let win = draw.window(360, 200, "Events")
    var presses = 0
    var last = "nothing yet"
    loop win.open():
        if win.key_pressed("space"):
            presses += 1
            last = "space"
        end
        if win.clicked(): last = "a click at {win.mouse_x}, {win.mouse_y}"
        win.clear("#202830")
        win.text("Space pressed {presses} times", 20, 40, "white")
        win.text("Last: {last}", 20, 80, "#90caf9")
        win.show()
    end
end
```

`key_pressed` is true only on the frame the key goes down; `key_down` is
true for as long as it is held.

## 28.5 Project: a focus timer

The **Pomodoro** method: work for 25 minutes, rest for 5; after four work
sessions, rest for 15. The rules are a small function that says what comes
next, which you can test in an instant:

```vyrn
const WORK = 25 * 60
const SHORT = 5 * 60
const LONG = 15 * 60

## What follows `phase`, with how many work sessions are done.
create next_phase(phase: str, done: int) -> (str, int):
    if phase == "work":
        let finished = done + 1
        return ("long break" if finished % 4 == 0 else "break", finished)
    end
    return ("work", done)
end

create length(phase: str) -> int:
    when phase:
        is "work": return WORK
        is "break": return SHORT
        else: return LONG
    end
end

create clock_text(seconds: int) -> str:
    return "{seconds // 60:02}:{seconds % 60:02}"
end

main:
    var phase = "work"
    var done = 0
    each _ in 0 until 9:
        print(phase, clock_text(length(phase)))
        phase, done = next_phase(phase, done)
    end
end
```

```text
work 25:00
break 05:00
work 25:00
break 05:00
work 25:00
break 05:00
work 25:00
long break 15:00
work 25:00
```

The app counts down with a `Clock` and moves to the next phase when the time
runs out. Space pauses and resumes:

```vyrn check
use vyrn.draw

const WORK = 25 * 60
const SHORT = 5 * 60
const LONG = 15 * 60

create next_phase(phase: str, done: int) -> (str, int):
    if phase == "work":
        let finished = done + 1
        return ("long break" if finished % 4 == 0 else "break", finished)
    end
    return ("work", done)
end

create length(phase: str) -> int:
    when phase:
        is "work": return WORK
        is "break": return SHORT
        else: return LONG
    end
end

main:
    let win = draw.window(360, 220, "Focus")
    var phase = "work"
    var done = 0
    var left = float(length(phase))
    var paused = false
    var dt = 1.0 / 60
    loop win.open():
        if win.key_pressed("space"): paused = not paused
        if not paused: left -= dt
        if left <= 0:
            phase, done = next_phase(phase, done)
            left = float(length(phase))
        end
        let secs = int(left.ceil())
        win.clear("#b71c1c" if phase == "work" else "#1b5e20")
        win.text(phase.upper(), 20, 30, "white", size = 3)
        win.text("{secs // 60:02}:{secs % 60:02}", 20, 90, "white", size = 6)
        win.text("{done} done   space: {"resume" if paused else "pause"}", 20, 180, "white")
        dt = win.show()
    end
end
```

Here the window's own frame time (`dt = win.show()`) does the counting, so
the timer stays right even if a frame is slow.

### Exercise

Add up how long a whole cycle takes, four work sessions and their breaks,
ending with the long break, by stepping `next_phase` until the long break
is over. Print it with `clock_text`-style hours.

```vyrn starter
create next_phase(phase: str, done: int) -> (str, int):
    if phase == "work":
        let finished = done + 1
        return ("long break" if finished % 4 == 0 else "break", finished)
    end
    return ("work", done)
end

create length(phase: str) -> int:
    when phase:
        is "work": return 25 * 60
        is "break": return 5 * 60
        else: return 15 * 60
    end
end

main:
    var total = 0
end
```

```text expect
2:10:00
```

> Hint: start with `phase = "work"` and `done = 0`; loop, adding
> `length(phase)`, until you have added the long break.
> Hint: `loop:` ... `if phase == "long break": stop` after adding it.

<details><summary>Solution</summary>

```vyrn solution
create next_phase(phase: str, done: int) -> (str, int):
    if phase == "work":
        let finished = done + 1
        return ("long break" if finished % 4 == 0 else "break", finished)
    end
    return ("work", done)
end

create length(phase: str) -> int:
    when phase:
        is "work": return 25 * 60
        is "break": return 5 * 60
        else: return 15 * 60
    end
end

main:
    var total = 0
    var phase = "work"
    var done = 0
    loop:
        total += length(phase)
        if phase == "long break": stop
        phase, done = next_phase(phase, done)
    end
    print("{total // 3600}:{total % 3600 // 60:02}:{total % 60:02}")
end
```

</details>

## Today you learned

- `time.sleep` waits; `time.monotonic()` measures, and never jumps.
- `//` and `%` split seconds into minutes and seconds; `{m:02}` pads with
  zeros.
- A `Clock` ticks at a steady rate: `loop clock.running:` ... `clock.tick()`.
- Events (keys, clicks) are checked every frame; `key_pressed` is the
  moment, `key_down` the whole time.
- Keep the rules (what comes next) in small functions you can test.
