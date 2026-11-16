# Day 77: A platformer

Unit 2 made games where things slide around a screen: Snake, Pong, a road
crossing. This unit makes games that feel physical, starting with the
most loved kind of 2D game: a **platformer**, where a character runs and
jumps between ledges. You'll write gravity, jumping that feels good, and
collision with a level made of tiles. The game logic is plain functions
on plain data, so every rule can be tested without opening a window; the
window code at the end only draws it.

## 77.1 Velocity and gravity

A thing that moves has a **position** and a **velocity** (how far it moves
each second). Gravity changes the velocity; the velocity changes the
position. Each frame lasts `dt` seconds (Day 19, and the game-dev guide):

```vyrn
struct Body:
    x: float
    y: float
    vx: float
    vy: float
end

const GRAVITY = 1500.0

create step(b: Body, dt: float) -> Body:
    let vy = b.vy + GRAVITY * dt
    return Body(b.x + b.vx * dt, b.y + vy * dt, b.vx, vy)
end

main:
    var ball = Body(0.0, 0.0, 200.0, -600.0)
    each frame in 0 until 50:
        ball = step(ball, 1.0 / 60)
        if frame % 10 == 9: print("t={(frame + 1) / 60:.2f}s  x={ball.x:.0f}  y={ball.y:.0f}")
    end
end
```

```text
t=0.17s  x=33  y=-77
t=0.33s  x=67  y=-112
t=0.50s  x=100  y=-106
t=0.67s  x=133  y=-58
t=0.83s  x=167  y=31
```

Screen coordinates grow *downwards*, so a negative `vy` is upwards. The
ball rises, slows, and falls: a curve, from two lines of arithmetic. Every
physics engine, from Mario to Unreal, starts here.

## 77.2 A level made of tiles

Platformer levels are usually grids of **tiles**: each cell is empty or
solid. Writing the level as text makes it easy to draw by hand and easy to
read:

```vyrn
const LEVEL = """
..........
..........
....###...
..........
#........#
##########
"""

const TILE = 32

create load(text: str) -> list[str]:
    return [line each line in text.strip().split("\n")]
end

create solid(level: list[str], px: float, py: float) -> bool:
    let col = int(px // TILE)
    let row = int(py // TILE)
    if row < 0 or row >= len(level) or col < 0 or col >= len(level[0]): return true
    return level[row][col] == "#"
end

main:
    let level = load(LEVEL)
    print(len(level), "rows of", len(level[0]))
    print(solid(level, 5 * 32 + 3, 2 * 32 + 10), solid(level, 40.0, 40.0), solid(level, -1.0, 0.0))
end
```

```text
6 rows of 10
true false true
```

`px // TILE` turns a pixel position into a tile column. Outside the level
counts as solid, so nothing can fall out of the world.

## 77.3 Colliding with tiles

The classic method: move along one axis at a time, and if the box ends up
inside a solid tile, push it back to the tile's edge and stop that part of
its velocity. Moving x and y separately is what lets a character slide
along a wall or land on a floor cleanly:

```vyrn
const TILE = 32

struct Player:
    x: float
    y: float
    vx: float
    vy: float
    on_ground: bool
end

const W = 20.0
const H = 28.0

create solid(level: list[str], px: float, py: float) -> bool:
    let col = int(px // TILE)
    let row = int(py // TILE)
    if row < 0 or row >= len(level) or col < 0 or col >= len(level[0]): return true
    return level[row][col] == "#"
end

create hits(level: list[str], x: float, y: float) -> bool:
    return solid(level, x, y) or solid(level, x + W - 1, y) or solid(level, x, y + H - 1) or solid(level, x + W - 1, y + H - 1)
end

create move(level: list[str], p: Player, dt: float) -> Player:
    ## Across: if the new place is inside a wall, stay put and stop.
    var x = p.x + p.vx * dt
    var vx = p.vx
    if hits(level, x, p.y):
        x = p.x
        vx = 0.0
    end
    ## Down or up: when landing, stand exactly on top of the tile hit.
    var y = p.y + p.vy * dt
    var vy = p.vy
    if hits(level, x, y):
        y = int((y + H - 1) // TILE) * TILE - H if p.vy > 0 else p.y
        vy = 0.0
    end
    ## Standing means solid ground one pixel below the feet.
    return Player(x, y, vx, vy, hits(level, x, y + 1))
end

main:
    let level = ["........", "........", "........", "########"]
    var p = Player(40.0, 0.0, 0.0, 0.0, false)
    each _ in 0 until 60:
        p = move(level, Player(p.x, p.y, p.vx, p.vy + 1500.0 / 60, p.on_ground), 1.0 / 60)
    end
    print("landed:", p.on_ground, "feet at", round(p.y + H), "floor at", 3 * TILE)
end
```

```text
landed: true feet at 96 floor at 96
```

The player falls, hits the floor, and stands on it, with their feet
exactly on the floor's top edge.

## 77.4 Jumping that feels good

Jumping is only allowed on the ground, or the player could fly. Two small
tricks make it feel far better, and nearly every good platformer uses
them:

- **Coyote time:** you can still jump for a moment (a tenth of a second)
  after running off a ledge, because people press jump a fraction late.
- **Jump buffering:** a jump pressed a moment *before* landing still
  happens when you land.

```vyrn
object JumpControl:
    since_ground: float = 1.0
    since_press: float = 1.0
    window: float = 0.1

    create update(self, dt: float, on_ground: bool, pressed: bool) -> bool:
        self.since_ground = 0.0 if on_ground else self.since_ground + dt
        self.since_press = 0.0 if pressed else self.since_press + dt
        if self.since_ground <= self.window and self.since_press <= self.window:
            self.since_ground = 1.0
            self.since_press = 1.0
            return true
        end
        return false
    end
end

main:
    let dt = 1.0 / 60
    let late = JumpControl()
    late.update(dt, true, false)
    each _ in 0 until 4: late.update(dt, false, false)
    print("pressed 4 frames after leaving the ledge:", late.update(dt, false, true))
    let early = JumpControl()
    early.update(dt, false, true)
    each _ in 0 until 3: early.update(dt, false, false)
    print("pressed 4 frames before landing:", early.update(dt, true, false))
    let flying = JumpControl()
    each _ in 0 until 30: flying.update(dt, false, false)
    print("pressed in mid-air long after:", flying.update(dt, false, true))
end
```

```text
pressed 4 frames after leaving the ledge: true
pressed 4 frames before landing: true
pressed in mid-air long after: false
```

### Exercise

**Variable jump height:** if the player lets go of jump while still
rising, cut their upward speed in half, so a tap gives a small hop and a
hold a full jump. Write `release(vy, holding)`.

```vyrn starter
create release(vy: float, holding: bool) -> float:
    return vy
end

main:
    print(release(-600.0, true), release(-600.0, false), release(200.0, false))
end
```

```text expect
-600.0 -300.0 200.0
```

> Hint: only change it when not holding *and* still going up (`vy < 0`).

<details><summary>Solution</summary>

```vyrn solution
create release(vy: float, holding: bool) -> float:
    if not holding and vy < 0: return vy / 2
    return vy
end

main:
    print(release(-600.0, true), release(-600.0, false), release(200.0, false))
end
```

</details>

## 77.5 Project: the platformer

The game: a level with ledges and coins, a player who runs and jumps, a
camera that follows them. The rules live in one `update` function that
takes the input as data, so a test can play the game by "pressing keys"
with a list:

```vyrn
const TILE = 32
const W = 20.0
const H = 28.0
const GRAVITY = 1500.0
const RUN = 180.0
const JUMP = -560.0

const LEVEL = [
    "....................",
    "............o.......",
    "..........#####.....",
    "....o...............",
    "...####........o....",
    "..............###...",
    "#..................#",
    "####################",
]

object Game:
    level: list[str]
    x: float = 40.0
    y: float = 150.0
    vx: float = 0.0
    vy: float = 0.0
    on_ground: bool = false
    coins: set = set()

    create solid(self, px: float, py: float) -> bool:
        let col = int(px // TILE)
        let row = int(py // TILE)
        if row < 0 or row >= len(self.level) or col < 0 or col >= len(self.level[0]): return true
        return self.level[row][col] == "#"
    end

    create hits(self, x: float, y: float) -> bool:
        return self.solid(x, y) or self.solid(x + W - 1, y) or self.solid(x, y + H - 1) or self.solid(x + W - 1, y + H - 1)
    end

    create update(self, dt: float, left: bool, right: bool, jump: bool):
        self.vx = (RUN if right else 0.0) - (RUN if left else 0.0)
        if jump and self.on_ground: self.vy = JUMP
        self.vy += GRAVITY * dt
        let nx = self.x + self.vx * dt
        if not self.hits(nx, self.y): self.x = nx
        let ny = self.y + self.vy * dt
        if self.hits(self.x, ny):
            if self.vy > 0: self.y = int((ny + H - 1) // TILE) * TILE - H
            self.vy = 0.0
        else:
            self.y = ny
        end
        self.on_ground = self.hits(self.x, self.y + 1)
        let col = int((self.x + W / 2) // TILE)
        let row = int((self.y + H / 2) // TILE)
        if 0 <= row < len(self.level) and self.level[row][col] == "o": self.coins.add((col, row))
    end
end

## Plays a script of (keys held, number of frames): "L", "R", "J" for left,
## right and jump.
create play(game: Game, script: list):
    each keys, frames in script:
        each _ in 0 until frames:
            game.update(1.0 / 60, "L" in keys, "R" in keys, "J" in keys)
        end
    end
end

main:
    let g = Game(LEVEL)
    play(g, [("", 20), ("RJ", 16), ("R", 20), ("", 40)])
    print("x={g.x:.0f} y={g.y:.0f} on ground: {g.on_ground}, coins: {sorted(g.coins)}")
    let still = Game(LEVEL)
    play(still, [("", 90)])
    print("standing still: y={still.y:.0f}, on ground: {still.on_ground}, coins: {len(still.coins)}")
end
```

```text
x=130 y=100 on ground: true, coins: [(4, 3)]
standing still: y=196, on ground: true, coins: 0
```

Waiting, then running right while jumping, lands the player on the first
ledge with its coin; standing still, they settle on the floor. Scripts like
these are the game's tests: change the jump strength and you'll see at
once whether that ledge can still be reached.

The window version draws the same `Game`: tiles as squares, coins as
circles, and a camera that keeps the player in the middle of the screen
by drawing everything shifted by the camera's position:

```vyrn check
use vyrn.draw

const TILE = 32

main:
    let level = ["....................", "....o...............", "...####.............", "####################"]
    let win = draw.window(480, 256, "Platformer")
    var x = 40.0
    var dt = 0.016
    loop win.open():
        if win.key_down("right"): x += 180 * dt
        if win.key_down("left"): x -= 180 * dt
        let camera = max(0.0, x - 240)
        win.clear("#87ceeb")
        each row, line in enumerate(level):
            each col, c in enumerate(line):
                let sx = col * TILE - camera
                if c == "#": win.rect(sx, row * TILE, TILE, TILE, "#5d4037")
                if c == "o": win.circle(sx + 16, row * TILE + 16, 8, "gold")
            end
        end
        win.rect(x - camera, 2 * TILE + 4, 20, 28, "crimson")
        dt = win.show()
    end
end
```

Put the `Game` object and this drawing together, read the arrow keys and
space into `update`, and you have a playable game. Because `update` is
tested on its own, you can change the jump height, add enemies or design
levels and check the rules still hold without playing through every time.

### Exercise

Add a **goal**: the level is finished when the player touches a `G` tile.
Write `at_goal(level, x, y)` that checks the tile under the player's
centre.

```vyrn starter
const TILE = 32

create at_goal(level: list[str], x: float, y: float) -> bool:
    return false
end

main:
    let level = ["....G", "#####"]
    print(at_goal(level, 4 * 32 + 5, 2.0), at_goal(level, 10.0, 2.0))
end
```

```text expect
true false
```

> Hint: centre column `int((x + 10) // TILE)`, row `int((y + 14) // TILE)`.

<details><summary>Solution</summary>

```vyrn solution
const TILE = 32

create at_goal(level: list[str], x: float, y: float) -> bool:
    let col = int((x + 10) // TILE)
    let row = int((y + 14) // TILE)
    if row < 0 or row >= len(level) or col < 0 or col >= len(level[0]): return false
    return level[row][col] == "G"
end

main:
    let level = ["....G", "#####"]
    print(at_goal(level, 4 * 32 + 5, 2.0), at_goal(level, 10.0, 2.0))
end
```

</details>

## Today you learned

- Gravity changes velocity, velocity changes position, scaled by `dt`.
- Tile levels written as text; pixel to tile with `//`.
- Collide one axis at a time, push back to the tile edge, stop that
  velocity.
- Coyote time, jump buffering and variable jump height make jumping feel
  good.
- Keep game rules in an `update` function that tests can drive with
  recorded input; the window only draws.
