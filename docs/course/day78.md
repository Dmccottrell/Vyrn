# Day 78: A 3D maze

Yesterday's platformer was flat. Today the game gets a third dimension:
you walk through a maze in first person, with lights, fog and a minimap.
3D sounds hard, but a 3D game is the same loop as a 2D one (input,
update, draw), with a **camera** that decides what the screen shows. The
rules of the maze (where the walls are, where you can walk, how to reach
the exit) are still a grid and still plain, testable functions. The
`game-dev.md` guide goes much further, to a full shooter; this lesson is
the first step.

## 78.1 Coordinates in 3D

A 3D position has three numbers. In Vyrn's 3D, **x** is right, **y** is up
and **z** is forward. A flat maze lives on the ground, so walls sit at
grid cells `(column, row)` with x = column and z = row, and y is height:

```vyrn
const MAZE = """
#########
#S..#...#
#.#.#.#.#
#.#...#.#
#.#####.#
#...#..E#
#########
"""

create load(text: str) -> list[str]:
    return text.strip().split("\n")
end

create find(maze: list[str], c: str) -> (int, int):
    each row, line in enumerate(maze):
        let col = line.find(c)
        if col >= 0: return (col, row)
    end
    fail ValueError("no {c} in the maze")
end

main:
    let maze = load(MAZE)
    print(len(maze[0]), "by", len(maze), "cells")
    print("start", find(maze, "S"), "exit", find(maze, "E"))
    let walls = [(col, row) each row, line in enumerate(maze) each col, c in enumerate(line) if c == "#"]
    print(len(walls), "wall blocks")
end
```

```text
9 by 7 cells
start (1, 1) exit (7, 5)
40 wall blocks
```

Each `#` becomes a cube one unit wide in the world, standing on the
ground at x = column + 0.5, z = row + 0.5.

## 78.2 Turning and walking

The player has a position and a **yaw**: the direction they face, in
degrees. Walking forward moves along that direction, worked out with sine
and cosine; turning just changes the yaw:

```vyrn
use vyrn.math

create ahead(x: float, z: float, yaw: float, distance: float) -> (float, float):
    let r = math.radians(yaw)
    return (x + distance * math.sin(r), z + distance * math.cos(r))
end

main:
    each yaw in [0, 90, 180, 45]:
        let nx, nz = ahead(0.0, 0.0, yaw, 1.0)
        print("yaw {yaw:>3}: ({nx:.2f}, {nz:.2f})")
    end
end
```

```text
yaw   0: (0.00, 1.00)
yaw  90: (1.00, 0.00)
yaw 180: (0.00, -1.00)
yaw  45: (0.71, 0.71)
```

Yaw 0 looks along +z, yaw 90 along +x. `draw.camera` does exactly this
for you (`cam.forward(d)`, `cam.turn(degrees)`); knowing the maths lets
you move other things, such as enemies, the same way.

## 78.3 Not walking through walls

The player is a small circle on the floor plan. Before moving, check the
cells the circle would touch; as on Day 77, try each axis separately, so
walking into a wall at an angle slides along it instead of stopping dead:

```vyrn
const RADIUS = 0.25

create is_wall(maze: list[str], x: float, z: float) -> bool:
    let col = int(x // 1)
    let row = int(z // 1)
    if row < 0 or row >= len(maze) or col < 0 or col >= len(maze[0]): return true
    return maze[row][col] == "#"
end

create blocked(maze: list[str], x: float, z: float) -> bool:
    each dx, dz in [(-RADIUS, -RADIUS), (RADIUS, -RADIUS), (-RADIUS, RADIUS), (RADIUS, RADIUS)]:
        if is_wall(maze, x + dx, z + dz): return true
    end
    return false
end

create slide(maze: list[str], x: float, z: float, dx: float, dz: float) -> (float, float):
    let nx = x if blocked(maze, x + dx, z) else x + dx
    let nz = z if blocked(maze, nx, z + dz) else z + dz
    return (nx, nz)
end

main:
    let maze = ["#####", "#...#", "#...#", "#####"]
    print(slide(maze, 1.5, 1.5, 0.3, 0.0))
    print(slide(maze, 3.5, 1.5, 0.3, 0.0))
    print(slide(maze, 3.5, 1.5, 0.3, 0.3))
end
```

```text
(1.8, 1.5)
(3.5, 1.5)
(3.5, 1.8)
```

Against the right-hand wall, the move to the right is refused, but the
part of a diagonal move that goes along the wall still happens.

### Exercise

Write `at_exit(maze, x, z)`: true when the player stands in the `E` cell.

```vyrn starter
create at_exit(maze: list[str], x: float, z: float) -> bool:
    return false
end

main:
    let maze = ["####", "#.E#", "####"]
    print(at_exit(maze, 2.5, 1.5), at_exit(maze, 1.5, 1.5))
end
```

```text expect
true false
```

> Hint: the cell is `maze[int(z // 1)][int(x // 1)]`.

<details><summary>Solution</summary>

```vyrn solution
create at_exit(maze: list[str], x: float, z: float) -> bool:
    return maze[int(z // 1)][int(x // 1)] == "E"
end

main:
    let maze = ["####", "#.E#", "####"]
    print(at_exit(maze, 2.5, 1.5), at_exit(maze, 1.5, 1.5))
end
```

</details>

## 78.4 A hint: the way out

Lost in a maze? Breadth-first search (Day 71) finds the shortest way out
on the grid, step by step. The game can draw it on a minimap, or point an
arrow at the next cell:

```vyrn
create way_out(maze: list[str], start: (int, int), goal: (int, int)) -> list?:
    var came_from = {start: none}
    var queue = [start]
    loop queue:
        let cell = queue.pop(0)
        if cell == goal:
            var path = []
            var at = goal
            loop at != none:
                path.insert(0, at)
                at = came_from[at]
            end
            return path
        end
        let col, row = cell
        each next in [(col + 1, row), (col - 1, row), (col, row + 1), (col, row - 1)]:
            let c, r = next
            if maze[r][c] != "#" and next not in came_from:
                came_from[next] = cell
                queue.append(next)
            end
        end
    end
    return none
end

main:
    let maze = ["#########", "#S..#...#", "#.#.#.#.#", "#.#...#.#", "#.#####.#", "#...#..E#", "#########"]
    let path = way_out(maze, (1, 1), (7, 5))
    print(len(path) - 1, "steps")
    each row, line in enumerate(maze):
        print("".join(["*" if (col, row) in path and c == "." else c each col, c in enumerate(line)]))
    end
end
```

```text
14 steps
#########
#S**#***#
#.#*#*#*#
#.#***#*#
#.#####*#
#...#..E#
#########
```

## 78.5 Project: the maze in 3D

Putting it on screen: a camera at the player's eyes, a cube per wall, a
floor, a sun and a little ambient light so the shadows aren't black, and
fog to make the far end of the corridor fade out (which looks good and
hides how small the maze really is). The arrow keys turn and walk; the
movement goes through `slide`, so walls stop you:

```vyrn check
use vyrn.draw
use vyrn.math

const RADIUS = 0.25

create is_wall(maze: list[str], x: float, z: float) -> bool:
    let col = int(x // 1)
    let row = int(z // 1)
    if row < 0 or row >= len(maze) or col < 0 or col >= len(maze[0]): return true
    return maze[row][col] == "#"
end

create blocked(maze: list[str], x: float, z: float) -> bool:
    each dx, dz in [(-RADIUS, -RADIUS), (RADIUS, -RADIUS), (-RADIUS, RADIUS), (RADIUS, RADIUS)]:
        if is_wall(maze, x + dx, z + dz): return true
    end
    return false
end

main:
    let maze = ["#########", "#S..#...#", "#.#.#.#.#", "#.#...#.#", "#.#####.#", "#...#..E#", "#########"]
    let win = draw.window(800, 500, "Maze")
    let cam = draw.camera(1.5, 0.6, 1.5)
    var dt = 0.016
    var spin = 0.0
    loop win.open():
        spin += 90 * dt
        if win.key_down("left"): cam.turn(-120 * dt, 0)
        if win.key_down("right"): cam.turn(120 * dt, 0)
        var step = 0.0
        if win.key_down("up"): step = 2.5 * dt
        if win.key_down("down"): step = -2.5 * dt
        let r = math.radians(cam.yaw)
        let dx = step * math.sin(r)
        let dz = step * math.cos(r)
        let nx = cam.x if blocked(maze, cam.x + dx, cam.z) else cam.x + dx
        let nz = cam.z if blocked(maze, nx, cam.z + dz) else cam.z + dz
        cam.move_to(nx, 0.6, nz)

        win.clear("#202830")
        win.ambient(0.35, "white")
        win.light([0.4, -1, 0.3], "white", 0.8)
        win.fog("#202830", 3, 9)
        win.box(cam, [4.5, -0.05, 3.5, 9, 0.1, 7], "#4a4a4a")
        each row, line in enumerate(maze):
            each col, c in enumerate(line):
                if c == "#": win.cube(cam, col + 0.5, 0.5, row + 0.5, 1, "#8d6e63")
                if c == "E": win.cube(cam, col + 0.5, 0.2, row + 0.5, 0.3, "gold", rotation = [0, spin, 0])
            end
        end
        if maze[int(cam.z // 1)][int(cam.x // 1)] == "E": win.text("You escaped!", 320, 230, "gold", 3)
        dt = win.show()
    end
end
```

The rules are the functions you tested above; the window loop only reads
keys, moves the camera through `slide`'s logic, and draws. The spinning
gold cube at the exit uses `rotation`, with `spin` growing by 90 degrees
a second.

### Exercise

A minimap: write `minimap(maze, col, row)` giving the maze as text with an
`@` where the player is.

```vyrn starter
create minimap(maze: list[str], col: int, row: int) -> str:
    return "\n".join(maze)
end

main:
    print(minimap(["#####", "#...#", "#####"], 2, 1))
end
```

```text expect
#####
#.@.#
#####
```

> Hint: build each line with `"@" if (c, r) == (col, row) else ch`.

<details><summary>Solution</summary>

```vyrn solution
create minimap(maze: list[str], col: int, row: int) -> str:
    var lines = []
    each r, line in enumerate(maze):
        lines.append("".join(["@" if (c, r) == (col, row) else ch each c, ch in enumerate(line)]))
    end
    return "\n".join(lines)
end

main:
    print(minimap(["#####", "#...#", "#####"], 2, 1))
end
```

</details>

## Today you learned

- 3D positions are x (right), y (up), z (forward); a flat maze is a grid
  on the x–z plane.
- Facing is a yaw angle; forward is `(sin, cos)` of it.
- Collide on the floor plan, one axis at a time, so walls slide.
- Breadth-first search gives hints; the camera, cubes, lights and fog draw
  the world.
