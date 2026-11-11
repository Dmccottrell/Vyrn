# Day 20: Snake, Part 1: The Game's Data

Snake is a classic: a snake slides around a grid, eats food, grows longer,
and must never bite itself. Today you design the game's **data** and
**rules** and test them without a window, then put them on screen. Tomorrow
adds scoring, levels and polish.

Designing data first is how real games are made: if the rules are right
as plain data, drawing them is easy.

## 20.1 Cells and the snake

The grid is made of cells, each a `struct` with a column and a row. The
snake is a list of cells, **head first**.

```vyrn
struct Cell:
    x: int
    y: int
end

main:
    snake = [Cell(5, 3), Cell(4, 3), Cell(3, 3)]
    print("head:", snake[0])
    print("length:", len(snake))
    print(Cell(4, 3) in snake, Cell(9, 9) in snake)
end
```

```text
head: Cell(x=5, y=3)
length: 3
true false
```

Because `Cell` is a struct (a value), two cells with the same `x` and `y`
are equal, so `in` can ask "is this cell part of the snake?".

### Exercise

Write `occupied(snake, x, y)` returning whether the snake covers that cell.

```vyrn starter
struct Cell:
    x: int
    y: int
end

create occupied(snake, x, y):
    return false
end

main:
    snake = [Cell(5, 3), Cell(4, 3), Cell(3, 3)]
    print(occupied(snake, 4, 3), occupied(snake, 4, 4))
end
```

```text expect
true false
```

> Hint: `return Cell(x, y) in snake`

<details><summary>Solution</summary>

```vyrn solution
struct Cell:
    x: int
    y: int
end

create occupied(snake, x, y):
    return Cell(x, y) in snake
end

main:
    snake = [Cell(5, 3), Cell(4, 3), Cell(3, 3)]
    print(occupied(snake, 4, 3), occupied(snake, 4, 4))
end
```

</details>

## 20.2 Moving

To move, add a new head one cell in the direction of travel, and remove the
last cell. The body follows the head without moving each part separately.
The direction is a pair `(dx, dy)`: right is `(1, 0)`, up is `(0, -1)`.

### Exercise

Write `step(snake, dx, dy)` that moves the snake one cell (changing the
list in place). Running off an edge wraps to the other side of a 10 by 8
grid.

```vyrn starter
const COLS = 10
const ROWS = 8

struct Cell:
    x: int
    y: int
end

create step(snake, dx, dy):
end

main:
    snake = [Cell(9, 3), Cell(8, 3), Cell(7, 3)]
    step(snake, 1, 0)
    print(snake)
    step(snake, 0, -1)
    print(snake)
end
```

```text expect
[Cell(x=0, y=3), Cell(x=9, y=3), Cell(x=8, y=3)]
[Cell(x=0, y=2), Cell(x=0, y=3), Cell(x=9, y=3)]
```

> Hint: `head = Cell((snake[0].x + dx) % COLS, (snake[0].y + dy) % ROWS)`
> Hint: `snake.insert(0, head)` then `snake.pop()`.

<details><summary>Solution</summary>

```vyrn solution
const COLS = 10
const ROWS = 8

struct Cell:
    x: int
    y: int
end

create step(snake, dx, dy):
    head = Cell((snake[0].x + dx) % COLS, (snake[0].y + dy) % ROWS)
    snake.insert(0, head)
    snake.pop()
end

main:
    snake = [Cell(9, 3), Cell(8, 3), Cell(7, 3)]
    step(snake, 1, 0)
    print(snake)
    step(snake, 0, -1)
    print(snake)
end
```

</details>

## 20.3 Eating and growing

When the new head lands on the food, the snake **doesn't** lose its tail
that step, so it grows by one.

### Exercise

Change `step` to take the food's cell and return `true` when the snake ate.

```vyrn starter
const COLS = 10
const ROWS = 8

struct Cell:
    x: int
    y: int
end

create step(snake, dx, dy, food):
    head = Cell((snake[0].x + dx) % COLS, (snake[0].y + dy) % ROWS)
    snake.insert(0, head)
    snake.pop()
    return false
end

main:
    snake = [Cell(3, 3), Cell(2, 3)]
    food = Cell(5, 3)
    each _ in 1 to 3:
        ate = step(snake, 1, 0, food)
        print(ate, len(snake))
    end
end
```

```text expect
false 2
true 3
false 3
```

> Hint: After inserting the head: `if head == food: return true`, before popping.

<details><summary>Solution</summary>

```vyrn solution
const COLS = 10
const ROWS = 8

struct Cell:
    x: int
    y: int
end

create step(snake, dx, dy, food):
    head = Cell((snake[0].x + dx) % COLS, (snake[0].y + dy) % ROWS)
    snake.insert(0, head)
    if head == food: return true
    snake.pop()
    return false
end

main:
    snake = [Cell(3, 3), Cell(2, 3)]
    food = Cell(5, 3)
    each _ in 1 to 3:
        ate = step(snake, 1, 0, food)
        print(ate, len(snake))
    end
end
```

</details>

## 20.4 Crashing

The game ends when the head runs into the body.

### Exercise

Write `crashed(snake)`: whether the head is anywhere in the rest of the
snake.

```vyrn starter
struct Cell:
    x: int
    y: int
end

create crashed(snake):
    return false
end

main:
    print(crashed([Cell(1, 1), Cell(2, 1), Cell(3, 1)]))
    print(crashed([Cell(2, 1), Cell(2, 2), Cell(1, 2), Cell(1, 1), Cell(2, 1)]))
end
```

```text expect
false
true
```

> Hint: The rest of the snake is `snake[1:]`.

<details><summary>Solution</summary>

```vyrn solution
struct Cell:
    x: int
    y: int
end

create crashed(snake):
    return snake[0] in snake[1:]
end

main:
    print(crashed([Cell(1, 1), Cell(2, 1), Cell(3, 1)]))
    print(crashed([Cell(2, 1), Cell(2, 2), Cell(1, 2), Cell(1, 1), Cell(2, 1)]))
end
```

</details>

## 20.5 Seeing it: a text picture

Before any window, a text picture is a quick way to see the state:

### Exercise

Write `picture(snake, food)` returning the grid as lines: `@` for the
head, `o` for the body, `*` for food, `.` for empty.

```vyrn starter
const COLS = 8
const ROWS = 4

struct Cell:
    x: int
    y: int
end

create picture(snake, food):
    return ""
end

main:
    print(picture([Cell(3, 1), Cell(2, 1), Cell(1, 1)], Cell(6, 2)))
end
```

```text expect
........
.oo@....
......*.
........
```

> Hint: Two loops: rows (`y`), then columns (`x`), building one line per row.
> Hint: Check the head first: `if c == snake[0]:`, then `else if c in snake:`, then food.

<details><summary>Solution</summary>

```vyrn solution
const COLS = 8
const ROWS = 4

struct Cell:
    x: int
    y: int
end

create picture(snake, food):
    lines = []
    each y in 0 until ROWS:
        line = ""
        each x in 0 until COLS:
            c = Cell(x, y)
            if c == snake[0]:
                line += "@"
            else if c in snake:
                line += "o"
            else if c == food:
                line += "*"
            else:
                line += "."
            end
        end
        lines.append(line)
    end
    return "\n".join(lines)
end

main:
    print(picture([Cell(3, 1), Cell(2, 1), Cell(1, 1)], Cell(6, 2)))
end
```

</details>

## 20.6 On screen

The same rules, drawn in a window. Each cell is a 20 by 20 square; the
arrow keys change direction (but never straight back into the body).

```vyrn check
use vyrn.draw
use vyrn.random

const CELL = 20
const COLS = 30
const ROWS = 20

struct Cell:
    x: int
    y: int
end

create random_cell() -> Cell:
    return Cell(random.int(0, COLS - 1), random.int(0, ROWS - 1))
end

create step(snake, dx, dy, food):
    head = Cell((snake[0].x + dx) % COLS, (snake[0].y + dy) % ROWS)
    snake.insert(0, head)
    if head == food: return true
    snake.pop()
    return false
end

main:
    let win = draw.window(COLS * CELL, ROWS * CELL, "Snake")
    win.set_fps(10)
    var snake = [Cell(5, 10), Cell(4, 10), Cell(3, 10)]
    var dx = 1
    var dy = 0
    var food = random_cell()
    loop win.open():
        if win.key_down("left") and dx == 0:
            dx = -1
            dy = 0
        else if win.key_down("right") and dx == 0:
            dx = 1
            dy = 0
        else if win.key_down("up") and dy == 0:
            dx = 0
            dy = -1
        else if win.key_down("down") and dy == 0:
            dx = 0
            dy = 1
        end
        if step(snake, dx, dy, food): food = random_cell()
        win.clear("#102018")
        win.rect(food.x * CELL, food.y * CELL, CELL, CELL, "red")
        each c in snake:
            win.rect(c.x * CELL + 1, c.y * CELL + 1, CELL - 2, CELL - 2, "lime")
        end
        win.show()
    end
end
```

`win.set_fps(10)` slows the loop to 10 frames a second, so the snake moves
at a playable speed.

### Exercise

The checks `and dx == 0` stop the snake reversing into itself. Write
`turn(dx, dy, key)` that returns the new direction for a key, ignoring a
turn straight back.

```vyrn starter
create turn(dx, dy, key):
    return (dx, dy)
end

main:
    print(turn(1, 0, "up"))
    print(turn(1, 0, "left"))
    print(turn(0, -1, "left"))
    print(turn(0, -1, "down"))
end
```

```text expect
(0, -1)
(1, 0)
(-1, 0)
(0, -1)
```

> Hint: `when key:` with one case per arrow, each checking it's not reversing.
> Hint: `is "left": if dx == 0: return (-1, 0)`

<details><summary>Solution</summary>

```vyrn solution
create turn(dx, dy, key):
    when key:
        is "left":
            if dx == 0: return (-1, 0)
        is "right":
            if dx == 0: return (1, 0)
        is "up":
            if dy == 0: return (0, -1)
        is "down":
            if dy == 0: return (0, 1)
    end
    return (dx, dy)
end

main:
    print(turn(1, 0, "up"))
    print(turn(1, 0, "left"))
    print(turn(0, -1, "left"))
    print(turn(0, -1, "down"))
end
```

</details>

**Day 20 done, and Unit 2 with it!** You can design data, write and test
rules, use objects and structs, handle errors, organise projects, and draw
in windows. Unit 3 goes further: inheritance, files, comprehensions,
apps with buttons, and more games.
