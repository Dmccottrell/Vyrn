# Day 18: Drawing Pictures

Time for graphics! Today you draw shapes, use colours and coordinates, and
steer a turtle that draws as it moves. Every picture is saved as a PNG
file you can open in any image viewer. The day ends with a spot painting
in the style of modern art.

## 18.1 A canvas and coordinates

`use vyrn.draw` brings in graphics. A **canvas** is a picture in memory,
made of **pixels** (tiny squares of colour). Positions are `(x, y)`
counted in pixels from the **top-left corner**: `x` grows to the right,
`y` grows **downwards**.

```vyrn
use vyrn.draw

main:
    pic = draw.canvas(200, 100)
    pic.clear("white")
    pic.rect(10, 10, 50, 30, "red")
    pic.save("first.png")
    print(pic.width, pic.height)
    print(pic.get_pixel(20, 20), pic.get_pixel(100, 50))
end
```

```text
200 100
#e53935 #ffffff
```

- `draw.canvas(200, 100)` makes a picture 200 pixels wide and 100 high.
- `pic.clear("white")` paints it all white.
- `pic.rect(x, y, width, height, colour)` draws a filled rectangle whose
  top-left corner is at `(x, y)`.
- `pic.save("first.png")` writes the picture to a file.
- `pic.get_pixel(x, y)` tells you the colour at a point, as a hex code.
  (That's how these lessons check your pictures.)

### Exercise

Draw a 40 by 40 blue square in the bottom-right corner of a 100 by 100
white canvas, and check two pixels.

```vyrn starter
use vyrn.draw

main:
    pic = draw.canvas(100, 100)
    pic.clear("white")
    print(pic.get_pixel(80, 80), pic.get_pixel(10, 10))
end
```

```text expect
#1e88e5 #ffffff
```

> Hint: The square's top-left corner is at `(60, 60)`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    pic = draw.canvas(100, 100)
    pic.clear("white")
    pic.rect(60, 60, 40, 40, "blue")
    print(pic.get_pixel(80, 80), pic.get_pixel(10, 10))
end
```

</details>

## 18.2 Colours

A colour can be written three ways:

- a **name**: `"red"`, `"skyblue"`, `"gold"`, `"darkgreen"`, ...
- a **hex code**: `"#ff8800"`: two hex digits each for red, green and blue,
  from `00` (none) to `ff` (full)
- a **list** `[red, green, blue]` from 0 to 255, or `[r, g, b, alpha]`
  where alpha 0 is see-through and 255 solid

```vyrn
use vyrn.draw

main:
    pic = draw.canvas(30, 10)
    pic.rect(0, 0, 10, 10, "tomato")
    pic.rect(10, 0, 10, 10, "#00ff00")
    pic.rect(20, 0, 10, 10, [0, 0, 255])
    print(pic.get_pixel(5, 5), pic.get_pixel(15, 5), pic.get_pixel(25, 5))
    print(draw.rgb(255, 136, 0))
end
```

```text
#ff6347 #00ff00 #0000ff
#ff8800
```

Mixing light: red + green = yellow, all three = white, none = black.

### Exercise

Paint three stripes of a flag, each 30 pixels wide, in orange `[255, 153,
51]`, white, and green `[19, 136, 8]`, then print the colour of each.

```vyrn starter
use vyrn.draw

main:
    flag = draw.canvas(90, 60)
end
```

```text expect
#ff9933 #ffffff #138808
```

> Hint: The stripes start at x = 0, 30 and 60.
> Hint: Check the middle of each: `flag.get_pixel(15, 30)`, ...

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    flag = draw.canvas(90, 60)
    flag.rect(0, 0, 30, 60, [255, 153, 51])
    flag.rect(30, 0, 30, 60, "white")
    flag.rect(60, 0, 30, 60, [19, 136, 8])
    print(flag.get_pixel(15, 30), flag.get_pixel(45, 30), flag.get_pixel(75, 30))
end
```

</details>

## 18.3 More shapes

```vyrn check
use vyrn.draw

main:
    pic = draw.canvas(300, 200)
    pic.clear("white")
    pic.circle(60, 60, 40, "gold")
    pic.circle(60, 60, 40, "black", fill = false, width = 3)
    pic.line(120, 20, 280, 100, "navy", width = 4)
    pic.triangle(150, 180, 200, 110, 250, 180, "green")
    pic.polygon([[20, 190], [60, 130], [100, 190]], "purple")
    pic.text("Shapes!", 130, 150, "black")
    pic.save("shapes.png")
end
```

| Method | Draws |
|---|---|
| `rect(x, y, w, h, colour)` | a rectangle from its top-left corner |
| `circle(cx, cy, radius, colour)` | a circle around its centre |
| `line(x1, y1, x2, y2, colour)` | a line |
| `triangle(x1, y1, x2, y2, x3, y3, colour)` | a triangle |
| `polygon(points, colour)` | a shape through a list of points |
| `text(words, x, y, colour)` | text |
| `pixel(x, y, colour)` | one pixel |

Add `fill = false` for an outline only, and `width = n` for thicker lines.

### Exercise

Draw a target: three circles at the centre of a 100 by 100 canvas, radius
40 red, 25 white, 10 red. Check the colours at distance 5, 15 and 30 from
the centre.

```vyrn starter
use vyrn.draw

main:
    pic = draw.canvas(100, 100)
    pic.clear("white")
end
```

```text expect
#e53935 #ffffff #e53935
```

> Hint: Draw the biggest circle first: later shapes go on top.
> Hint: Check `pic.get_pixel(55, 50)`, `(65, 50)` and `(80, 50)`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    pic = draw.canvas(100, 100)
    pic.clear("white")
    pic.circle(50, 50, 40, "red")
    pic.circle(50, 50, 25, "white")
    pic.circle(50, 50, 10, "red")
    print(pic.get_pixel(55, 50), pic.get_pixel(65, 50), pic.get_pixel(80, 50))
end
```

</details>

## 18.4 Drawing with loops

Loops and shapes together make patterns:

```vyrn
use vyrn.draw

main:
    pic = draw.canvas(200, 40)
    pic.clear("white")
    colours = ["red", "orange", "gold", "green", "blue"]
    each i in 0 until 5:
        pic.rect(i * 40, 0, 40, 40, colours[i])
    end
    pic.save("rainbow.png")
    print([pic.get_pixel(i * 40 + 20, 20) each i in 0 until 5])
end
```

```text
["#e53935", "#fb8c00", "#ffc107", "#43a047", "#1e88e5"]
```

### Exercise

Draw a chessboard: 8 by 8 squares of 10 pixels, black where `row + col` is
odd. Print the colours of the first two squares of the first two rows.

```vyrn starter
use vyrn.draw

main:
    board = draw.canvas(80, 80)
    board.clear("white")
end
```

```text expect
#ffffff #000000
#000000 #ffffff
```

> Hint: Two nested loops over `row` and `col` from 0 until 8.
> Hint: `if (row + col) % 2 == 1: board.rect(col * 10, row * 10, 10, 10, "black")`

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    board = draw.canvas(80, 80)
    board.clear("white")
    each row in 0 until 8:
        each col in 0 until 8:
            if (row + col) % 2 == 1:
                board.rect(col * 10, row * 10, 10, 10, "black")
            end
        end
    end
    print(board.get_pixel(5, 5), board.get_pixel(15, 5))
    print(board.get_pixel(5, 15), board.get_pixel(15, 15))
end
```

</details>

## 18.5 The turtle

A **turtle** is a pen you steer: it starts in the middle of the canvas,
facing right, and draws a line wherever it moves. Turtle positions are
measured from the centre, with `y` going **up**, like a maths graph.

```vyrn
use vyrn.draw

main:
    pic = draw.canvas(200, 200)
    pic.clear("white")
    t = draw.turtle(pic)
    t.color("blue")
    t.forward(50)
    t.left(90)
    t.forward(50)
    print(round(t.x), round(t.y), t.heading)
    pic.save("corner.png")
end
```

```text
50 50 90.0
```

| Command | Does |
|---|---|
| `forward(n)`, `back(n)` | move `n` pixels |
| `left(deg)`, `right(deg)` | turn |
| `goto(x, y)`, `home()` | jump to a point, or back to the middle |
| `pen_up()`, `pen_down()` | stop and start drawing |
| `color(c)`, `width(n)` | the pen's colour and thickness |
| `circle(r)`, `dot(size, colour)` | a circle, a dot |
| `begin_fill()`, `end_fill()` | fill the shape drawn in between |
| `x`, `y`, `heading` | where it is and which way it faces |

### Exercise

Draw a square with sides of 60 using a loop, and print where the turtle
ends and which way it faces.

```vyrn starter
use vyrn.draw

main:
    pic = draw.canvas(200, 200)
    t = draw.turtle(pic)
end
```

```text expect
0 0 0.0
```

> Hint: `each _ in 1 to 4:` forward 60, then left 90.
> Hint: Print `round(t.x), round(t.y), t.heading`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    pic = draw.canvas(200, 200)
    t = draw.turtle(pic)
    each _ in 1 to 4:
        t.forward(60)
        t.left(90)
    end
    print(round(t.x), round(t.y), t.heading)
end
```

</details>

## 18.6 Turtle shapes and functions

Any regular shape with `n` sides turns `360 / n` degrees at each corner.
A function makes it reusable:

```vyrn check
use vyrn.draw

create shape(t, sides, size, colour):
    t.color(colour)
    each _ in 1 to sides:
        t.forward(size)
        t.right(360 / sides)
    end
end

main:
    pic = draw.canvas(300, 300)
    pic.clear("white")
    t = draw.turtle(pic)
    colours = ["red", "orange", "gold", "green", "blue", "purple"]
    each sides in 3 to 8:
        shape(t, sides, 60, colours[sides - 3])
    end
    pic.save("shapes.png")
end
```

### Exercise

Draw a star: 5 lines of 100, turning right 144 degrees after each. Print
the heading at the end (it should face where it started).

```vyrn starter
use vyrn.draw

main:
    pic = draw.canvas(200, 200)
    t = draw.turtle(pic)
end
```

```text expect
0.0
```

> Hint: Five times: `t.forward(100)` then `t.right(144)`. 5 × 144 = 720: two full turns.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    pic = draw.canvas(200, 200)
    t = draw.turtle(pic)
    each _ in 1 to 5:
        t.forward(100)
        t.right(144)
    end
    print(t.heading)
end
```

</details>

## 18.7 Project: a Spot Painting

Paint a 10 by 10 grid of dots in random colours from a palette, evenly
spaced, and save it.

### Exercise

Keep the seed. Print the colours of the first three dots.

```vyrn starter
use vyrn.draw
use vyrn.random

const PALETTE = ["#e63946", "#f1c453", "#2a9d8f", "#264653", "#e76f51"]

main:
    random.seed(18)
    pic = draw.canvas(500, 500)
    pic.clear("#f4f1de")
end
```

```text expect
#e63946 #e63946 #264653
```

> Hint: Dot centres are at `25 + col * 50` and `25 + row * 50`.
> Hint: `pic.circle(x, y, 15, random.choice(PALETTE))` inside two loops.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw
use vyrn.random

const PALETTE = ["#e63946", "#f1c453", "#2a9d8f", "#264653", "#e76f51"]

main:
    random.seed(18)
    pic = draw.canvas(500, 500)
    pic.clear("#f4f1de")
    each row in 0 until 10:
        each col in 0 until 10:
            pic.circle(25 + col * 50, 25 + row * 50, 15, random.choice(PALETTE))
        end
    end
    pic.save("spots.png")
    print(pic.get_pixel(25, 25), pic.get_pixel(75, 25), pic.get_pixel(125, 25))
end
```

</details>

Open `spots.png` to admire your art.

**Day 18 done.** You can draw with shapes, colours, loops and a turtle.
Tomorrow: a live window that reacts to the keyboard.
