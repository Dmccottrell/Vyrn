# Day 19: Windows, Keys and Game State

Yesterday's pictures were saved to files. Today they come alive in a
**window** that redraws many times a second and reacts to the keyboard.
You'll learn the game loop, connect keys to functions, keep each
character's state in its own object, and finish with a turtle race.

## 19.1 A window and the game loop

```vyrn check
use vyrn.draw

main:
    let win = draw.window(400, 300, "My first window")
    var x = 0
    loop win.open():
        x = (x + 2) % 400
        win.clear("white")
        win.circle(x, 150, 20, "tomato")
        win.show()
    end
end
```

Every game and animation has this shape, the **game loop**:

1. `loop win.open():` repeats until the window is closed.
2. **Update**: change the state (here, move `x`).
3. **Draw**: clear, then draw everything where it is now.
4. `win.show()` puts the frame on screen, and waits so the loop runs
   about 60 times a second.

A window has all the drawing methods of a canvas. Run it on your computer
with `vyrn run`: a ball glides across the window. (A window needs a
screen, so these lessons check the parts of a game that don't need one.)

### Exercise

The game loop's *update* step can be tested without a window. Write
`next_x(x, speed, width)`, which moves `x` by `speed` and wraps around the
width, then print the first positions.

```vyrn starter
create next_x(x, speed, width):
    return x
end

main:
    var x = 390
    each _ in 1 to 4:
        x = next_x(x, 4, 400)
        print(x)
    end
end
```

```text expect
394
398
2
6
```

> Hint: `return (x + speed) % width`

<details><summary>Solution</summary>

```vyrn solution
create next_x(x, speed, width):
    return (x + speed) % width
end

main:
    var x = 390
    each _ in 1 to 4:
        x = next_x(x, 4, 400)
        print(x)
    end
end
```

</details>

## 19.2 Reading the keyboard

```vyrn check
use vyrn.draw

main:
    let win = draw.window(400, 300, "Move the square")
    var x = 200
    var y = 150
    loop win.open():
        if win.key_down("left"): x -= 3
        if win.key_down("right"): x += 3
        if win.key_down("up"): y -= 3
        if win.key_down("down"): y += 3
        if win.key_pressed("space"):
            x = 200
            y = 150
        end
        win.clear("black")
        win.rect(x - 10, y - 10, 20, 20, "lime")
        win.show()
    end
end
```

- `win.key_down("left")` is true for every frame the key is **held**:
  good for movement.
- `win.key_pressed("space")` is true only on the frame the key **goes
  down**: good for actions like jumping or shooting.
- Keys are named `"left"`, `"right"`, `"up"`, `"down"`, `"space"`,
  `"enter"`, `"escape"`, letters `"a"` to `"z"` and digits `"0"` to `"9"`.
- The mouse: `win.mouse_x`, `win.mouse_y`, `win.mouse_down("left")`,
  `win.clicked()`.

### Exercise

Keep the square inside the window: write `clamp(value, low, high)` and use
it so moving never goes past the edges.

```vyrn starter
create clamp(value, low, high):
    return value
end

main:
    print(clamp(-5, 10, 390), clamp(200, 10, 390), clamp(450, 10, 390))
end
```

```text expect
10 200 390
```

> Hint: `return max(low, min(high, value))`

<details><summary>Solution</summary>

```vyrn solution
create clamp(value, low, high):
    return max(low, min(high, value))
end

main:
    print(clamp(-5, 10, 390), clamp(200, 10, 390), clamp(450, 10, 390))
end
```

</details>

## 19.3 Keys as data: event handlers

Instead of a long list of `if`s, connect each key to a function in a map.
Each function is an **event handler**: code that runs when something
happens.

```vyrn check
use vyrn.draw

create forwards(t): t.forward(5)
create backwards(t): t.back(5)
create turn_left(t): t.left(10)
create turn_right(t): t.right(10)

main:
    let win = draw.window(500, 400, "Sketch")
    let pen = draw.turtle(win)
    let handlers = {"w": forwards, "s": backwards, "a": turn_left, "d": turn_right}
    win.clear("white")
    loop win.open():
        each key, handler in handlers.items():
            if win.key_down(key): handler(pen)
        end
        if win.key_pressed("c"): win.clear("white")
        win.show()
    end
end
```

Adding a new control now means adding one line to the map: the loop
doesn't change. Functions passed around like this are the
**higher-order functions** from Day 10 at work.

### Exercise

Simulate the sketch without a window: run a list of key presses through
the handlers, and print where the pen ends.

```vyrn starter
use vyrn.draw

create forwards(t): t.forward(10)
create turn_left(t): t.left(90)

main:
    pic = draw.canvas(200, 200)
    pen = draw.turtle(pic)
    handlers = {"w": forwards, "a": turn_left}
    presses = ["w", "w", "a", "w", "x"]
end
```

```text expect
20 10 90.0
```

> Hint: For each press, `if key in handlers: handlers[key](pen)`.
> Hint: Print `round(pen.x), round(pen.y), pen.heading`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

create forwards(t): t.forward(10)
create turn_left(t): t.left(90)

main:
    pic = draw.canvas(200, 200)
    pen = draw.turtle(pic)
    handlers = {"w": forwards, "a": turn_left}
    presses = ["w", "w", "a", "w", "x"]
    each key in presses:
        if key in handlers: handlers[key](pen)
    end
    print(round(pen.x), round(pen.y), pen.heading)
end
```

</details>

## 19.4 Each character keeps its own state

With several moving things, give each its own object. Every instance
remembers its own position and speed:

```vyrn
object Ball:
    x: float
    speed: float

    create update(self, width: float):
        self.x += self.speed
        if self.x < 0 or self.x > width: self.speed = -self.speed
    end
end

main:
    balls = [Ball(10.0, 30.0), Ball(50.0, -20.0)]
    each frame in 1 to 3:
        each b in balls: b.update(100.0)
        print(frame, [b.x each b in balls])
    end
end
```

```text
1 [40.0, 30.0]
2 [70.0, 10.0]
3 [100.0, -10.0]
```

In a window, each frame calls `update` on every ball, then draws every
ball. That's the whole trick behind games with many moving parts.

### Exercise

Give `Ball` a vertical position too, with gravity: each update adds
`gravity` to the speed, then the speed to `y`. A ball that reaches the
floor (`y >= floor`) stops there with speed 0.

```vyrn starter
object Ball:
    y: float = 0.0
    speed: float = 0.0

    create update(self, gravity: float, floor: float):
    end
end

main:
    b = Ball()
    each _ in 1 to 5:
        b.update(4.0, 30.0)
        print(b.y, b.speed)
    end
end
```

```text expect
4.0 4.0
12.0 8.0
24.0 12.0
30.0 0.0
30.0 0.0
```

> Hint: `self.speed += gravity` then `self.y += self.speed`.
> Hint: Then `if self.y >= floor:` set `self.y = floor` and `self.speed = 0.0`.

<details><summary>Solution</summary>

```vyrn solution
object Ball:
    y: float = 0.0
    speed: float = 0.0

    create update(self, gravity: float, floor: float):
        self.speed += gravity
        self.y += self.speed
        if self.y >= floor:
            self.y = floor
            self.speed = 0.0
        end
    end
end

main:
    b = Ball()
    each _ in 1 to 5:
        b.update(4.0, 30.0)
        print(b.y, b.speed)
    end
end
```

</details>

## 19.5 Project: the Turtle Race

Five turtles line up on the left. Each round, each moves forward a random
distance; the first past the finish line wins. Each turtle is its own
instance with its own colour and position.

### Exercise

Run the race on a canvas (keep the seed) and print the winner.

```vyrn starter
use vyrn.draw
use vyrn.random

const COLOURS = ["red", "orange", "green", "blue", "purple"]
const FINISH = 200

main:
    random.seed(19)
    pic = draw.canvas(500, 300)
    pic.clear("white")
    racers = []
    each i in 0 until 5:
        t = draw.turtle(pic)
        t.color(COLOURS[i])
        t.pen_up()
        t.goto(-230, -100 + i * 50)
        t.pen_down()
        racers.append(t)
    end
end
```

```text expect
The purple turtle wins!
```

> Hint: `winner = ""` then `loop winner == "":` move every racer `random.int(1, 10)`.
> Hint: The first racer with `t.x >= FINISH` gives the winner: `COLOURS[i]`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw
use vyrn.random

const COLOURS = ["red", "orange", "green", "blue", "purple"]
const FINISH = 200

main:
    random.seed(19)
    pic = draw.canvas(500, 300)
    pic.clear("white")
    racers = []
    each i in 0 until 5:
        t = draw.turtle(pic)
        t.color(COLOURS[i])
        t.pen_up()
        t.goto(-230, -100 + i * 50)
        t.pen_down()
        racers.append(t)
    end
    winner = ""
    loop winner == "":
        each i, t in enumerate(racers):
            t.forward(random.int(1, 10))
            if t.x >= FINISH and winner == "": winner = COLOURS[i]
        end
    end
    pic.save("race.png")
    print("The {winner} turtle wins!")
end
```

</details>

`enumerate(xs)` gives each item with its position: `each i, t in
enumerate(racers)`.

**Try it on screen:** draw the race in a window instead, calling
`win.show()` after each round, so you can watch it.

**Day 19 done.** Windows, keys, handlers and per-object state: you have
everything a game needs. Tomorrow: Snake.
