# Day 22: Pong

Pong was one of the first video games: two paddles, one ball, and a lot of
bouncing. Building it teaches the two ideas at the heart of most games:
**movement** (position changes by velocity every frame) and **collision**
(noticing when two things touch).

## 22.1 Position and velocity

Everything that moves has a position `(x, y)` and a **velocity**
`(vx, vy)`: how far it moves each frame. Each frame, add the velocity to
the position.

```vyrn
object Ball:
    x: float = 100.0
    y: float = 50.0
    vx: float = 3.0
    vy: float = -2.0

    create move(self):
        self.x += self.vx
        self.y += self.vy
    end
end

main:
    b = Ball()
    each frame in 1 to 3:
        b.move()
        print(frame, b.x, b.y)
    end
end
```

```text
1 103.0 48.0
2 106.0 46.0
3 109.0 44.0
```

### Exercise

Give `Ball` a `speed_up(factor)` method that multiplies both velocity
parts, then move it twice after speeding up by 1.5.

```vyrn starter
object Ball:
    x: float = 0.0
    y: float = 0.0
    vx: float = 2.0
    vy: float = 1.0

    create move(self):
        self.x += self.vx
        self.y += self.vy
    end
end

main:
    b = Ball()
    b.speed_up(1.5)
    b.move()
    b.move()
    print(b.x, b.y)
end
```

```text expect
6.0 3.0
```

> Hint: `self.vx *= factor` and the same for `vy`.

<details><summary>Solution</summary>

```vyrn solution
object Ball:
    x: float = 0.0
    y: float = 0.0
    vx: float = 2.0
    vy: float = 1.0

    create move(self):
        self.x += self.vx
        self.y += self.vy
    end

    create speed_up(self, factor: float):
        self.vx *= factor
        self.vy *= factor
    end
end

main:
    b = Ball()
    b.speed_up(1.5)
    b.move()
    b.move()
    print(b.x, b.y)
end
```

</details>

## 22.2 Bouncing off walls

When the ball reaches the top or bottom edge, flip the vertical velocity:
`vy = -vy`. The ball then heads back the other way.

### Exercise

Add `bounce_walls(height)`: if the ball is above 0 or below `height`, flip
`vy`. Watch the ball bounce off the top.

```vyrn starter
object Ball:
    y: float = 4.0
    vy: float = -3.0

    create move(self): self.y += self.vy
end

main:
    b = Ball()
    each _ in 1 to 4:
        b.move()
        b.bounce_walls(100.0)
        print(b.y, b.vy)
    end
end
```

```text expect
1.0 -3.0
-2.0 3.0
1.0 3.0
4.0 3.0
```

> Hint: `if self.y < 0 or self.y > height: self.vy = -self.vy`

<details><summary>Solution</summary>

```vyrn solution
object Ball:
    y: float = 4.0
    vy: float = -3.0

    create move(self): self.y += self.vy

    create bounce_walls(self, height: float):
        if self.y < 0 or self.y > height: self.vy = -self.vy
    end
end

main:
    b = Ball()
    each _ in 1 to 4:
        b.move()
        b.bounce_walls(100.0)
        print(b.y, b.vy)
    end
end
```

</details>

## 22.3 Collisions between rectangles

Two rectangles touch when they overlap on **both** axes. For rectangles
given as `(x, y, width, height)`:

- they overlap horizontally when `a.x < b.x + b.width` and
  `b.x < a.x + a.width`;
- and vertically in the same way with `y` and `height`.

```vyrn
use vyrn.draw

create overlaps(ax, ay, aw, ah, bx, by, bw, bh):
    return ax < bx + bw and bx < ax + aw and ay < by + bh and by < ay + ah
end

main:
    print(overlaps(0, 0, 10, 10, 5, 5, 10, 10))
    print(overlaps(0, 0, 10, 10, 20, 0, 5, 5))
    print(draw.overlaps(0, 0, 10, 10, 5, 5, 10, 10))
end
```

```text
true
false
true
```

`draw.overlaps` is the same test, built in.

### Exercise

A paddle is at `x = 380`, `y = 120`, 10 wide and 60 tall. Print which of
the balls (each 8 by 8) hit it.

```vyrn starter
use vyrn.draw

main:
    balls = [(375.0, 150.0), (375.0, 40.0), (390.0, 178.0), (360.0, 150.0)]
end
```

```text expect
[true, false, false, false]
```

> Hint: `[draw.overlaps(x, y, 8, 8, 380, 120, 10, 60) each x, y in balls]`, or a loop.
> Hint: The third ball starts at x = 390, the paddle's right edge: touching edges don't overlap.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

main:
    balls = [(375.0, 150.0), (375.0, 40.0), (390.0, 178.0), (360.0, 150.0)]
    hits = []
    each b in balls:
        x, y = b
        hits.append(draw.overlaps(x, y, 8, 8, 380, 120, 10, 60))
    end
    print(hits)
end
```

</details>

## 22.4 Paddles and the ball together

### Exercise

Put it together without a window: the ball travels right towards the
paddle; when they overlap, flip `vx` and count a hit. Run 30 frames.

```vyrn starter
use vyrn.draw

object Paddle:
    x: float
    y: float
    width: float = 10.0
    height: float = 60.0
end

object Ball:
    x: float = 300.0
    y: float = 140.0
    vx: float = 5.0
    size: float = 8.0
end

main:
    paddle = Paddle(380.0, 120.0)
    ball = Ball()
    hits = 0
end
```

```text expect
hits: 1, ball at x 300.0 moving -5.0
```

> Hint: Each frame: `ball.x += ball.vx`, then if `draw.overlaps(...)` the ball and paddle, `ball.vx = -ball.vx` and `hits += 1`.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.draw

object Paddle:
    x: float
    y: float
    width: float = 10.0
    height: float = 60.0
end

object Ball:
    x: float = 300.0
    y: float = 140.0
    vx: float = 5.0
    size: float = 8.0
end

main:
    paddle = Paddle(380.0, 120.0)
    ball = Ball()
    hits = 0
    each _ in 1 to 30:
        ball.x += ball.vx
        if draw.overlaps(ball.x, ball.y, ball.size, ball.size, paddle.x, paddle.y, paddle.width, paddle.height):
            ball.vx = -ball.vx
            hits += 1
        end
    end
    print("hits: {hits}, ball at x {ball.x} moving {ball.vx}")
end
```

</details>

## 22.5 The full game

Two players: `w`/`s` move the left paddle, the arrow keys the right one.
A ball past a paddle scores for the other side and restarts in the middle,
a little faster each time.

```vyrn check
use vyrn.draw

const WIDTH = 600
const HEIGHT = 400

object Paddle:
    x: float
    y: float = 170.0
    score: int = 0

    create move(self, dy: float):
        self.y = max(0.0, min(HEIGHT - 60.0, self.y + dy))
    end
end

object Ball:
    x: float = 300.0
    y: float = 200.0
    vx: float = 4.0
    vy: float = 3.0

    create reset(self, direction: float):
        self.x = 300.0
        self.y = 200.0
        self.vx = direction * (abs(self.vx) + 0.5)
    end
end

main:
    let win = draw.window(WIDTH, HEIGHT, "Pong")
    let left = Paddle(20.0)
    let right = Paddle(570.0)
    let ball = Ball()
    loop win.open():
        if win.key_down("w"): left.move(-6.0)
        if win.key_down("s"): left.move(6.0)
        if win.key_down("up"): right.move(-6.0)
        if win.key_down("down"): right.move(6.0)

        ball.x += ball.vx
        ball.y += ball.vy
        if ball.y < 0 or ball.y > HEIGHT - 10: ball.vy = -ball.vy
        each p in [left, right]:
            if draw.overlaps(ball.x, ball.y, 10, 10, p.x, p.y, 10, 60):
                ball.vx = -ball.vx
            end
        end
        if ball.x < 0:
            right.score += 1
            ball.reset(1.0)
        else if ball.x > WIDTH:
            left.score += 1
            ball.reset(-1.0)
        end

        win.clear("black")
        win.rect(left.x, left.y, 10, 60, "white")
        win.rect(right.x, right.y, 10, 60, "white")
        win.rect(ball.x, ball.y, 10, 10, "white")
        win.text("{left.score}   {right.score}", 280, 20, "white")
        win.show()
    end
end
```

### Exercise

The scoring rule is easy to get wrong, so test it on its own: write
`point_for(ball_x, width)` returning `"right"` when the ball leaves on the
left, `"left"` when it leaves on the right, and `""` otherwise.

```vyrn starter
create point_for(ball_x, width):
    return ""
end

main:
    print([point_for(x, 600) each x in [-3, 300, 605]])
end
```

```text expect
["right", "", "left"]
```

> Hint: Two `if`s and a final `return ""`.

<details><summary>Solution</summary>

```vyrn solution
create point_for(ball_x, width):
    if ball_x < 0: return "right"
    if ball_x > width: return "left"
    return ""
end

main:
    print([point_for(x, 600) each x in [-3, 300, 605]])
end
```

</details>

**Day 22 done.** Movement, bouncing, collisions and scoring: the physics
of nearly every 2D game. Tomorrow: a game with many moving objects at
once.
