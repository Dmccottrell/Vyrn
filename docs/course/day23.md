# Day 23: Many Objects at Once: the Road Crossing Game

A turtle tries to cross a busy road. Cars appear at random on the right
and drive left; each time the turtle reaches the far side, the cars speed
up. The new idea today is managing **many objects that come and go**:
creating them, updating all of them, and removing the ones you no longer
need.

## 23.1 A list of objects

```vyrn
object Car:
    x: float
    lane: int
    speed: float = 5.0

    create update(self): self.x -= self.speed
end

main:
    cars = [Car(300.0, 0), Car(450.0, 2), Car(600.0, 1)]
    each _ in 1 to 2:
        each c in cars: c.update()
    end
    print([c.x each c in cars])
end
```

```text
[290.0, 440.0, 590.0]
```

Every frame, the game calls `update` on every car. Each car only knows
about itself.

### Exercise

Give `Car` a `speed_up(amount)` method, speed every car up by 2.5, and
update once more.

```vyrn starter
object Car:
    x: float
    lane: int
    speed: float = 5.0

    create update(self): self.x -= self.speed
end

main:
    cars = [Car(300.0, 0), Car(450.0, 2)]
    print([c.x each c in cars])
end
```

```text expect
[292.5, 442.5]
```

> Hint: `create speed_up(self, amount: float): self.speed += amount`
> Hint: Call it on each car, then `update` each car, then print.

<details><summary>Solution</summary>

```vyrn solution
object Car:
    x: float
    lane: int
    speed: float = 5.0

    create update(self): self.x -= self.speed
    create speed_up(self, amount: float): self.speed += amount
end

main:
    cars = [Car(300.0, 0), Car(450.0, 2)]
    each c in cars:
        c.speed_up(2.5)
        c.update()
    end
    print([c.x each c in cars])
end
```

</details>

## 23.2 Making new objects at random

Cars shouldn't all appear at once. Each frame, a new car appears with some
chance, in a random lane:

```vyrn
use vyrn.random

object Car:
    x: float
    lane: int
end

main:
    random.seed(23)
    cars = []
    each frame in 1 to 10:
        if random.int(1, 4) == 1:
            cars.append(Car(600.0, random.int(0, 5)))
        end
    end
    print(len(cars), "cars, in lanes", [c.lane each c in cars])
end
```

```text
2 cars, in lanes [5, 4]
```

`random.int(1, 4) == 1` is true about one time in four. Making the chance
smaller makes traffic lighter.

### Exercise

Write `maybe_spawn(cars, chance)` that adds a car in a random lane when
`random.int(1, chance) == 1`, and run it 20 times with chance 3.

```vyrn starter
use vyrn.random

object Car:
    x: float
    lane: int
end

create maybe_spawn(cars, chance):
end

main:
    random.seed(5)
    cars = []
    each _ in 1 to 20: maybe_spawn(cars, 3)
    print(len(cars))
end
```

```text expect
5
```

> Hint: The list is shared, so `cars.append(...)` inside the function changes `main`'s list.

<details><summary>Solution</summary>

```vyrn solution
use vyrn.random

object Car:
    x: float
    lane: int
end

create maybe_spawn(cars, chance):
    if random.int(1, chance) == 1:
        cars.append(Car(600.0, random.int(0, 5)))
    end
end

main:
    random.seed(5)
    cars = []
    each _ in 1 to 20: maybe_spawn(cars, 3)
    print(len(cars))
end
```

</details>

## 23.3 Removing what's gone

Cars that have driven off the left edge should be removed, or the list
grows forever and the game slows down. Keep only the cars still on screen:

```vyrn
object Car:
    x: float
end

main:
    cars = [Car(-30.0), Car(100.0), Car(-5.0), Car(400.0)]
    cars = cars.filter(c => c.x > -20)
    print([c.x each c in cars])
end
```

```text
[100.0, -5.0, 400.0]
```

`xs.filter(test)` gives a new list of the items for which `test` is true.
Don't remove items from a list while looping over it with `each`: build
the new list instead, as here.

### Exercise

Remove cars whose `x` is below 0, and print how many were removed.

```vyrn starter
object Car:
    x: float
end

main:
    cars = [Car(-1.0), Car(50.0), Car(-90.0), Car(0.0), Car(10.0)]
end
```

```text expect
removed 2, kept [50.0, 0.0, 10.0]
```

> Hint: `kept = cars.filter(c => c.x >= 0)`, then `len(cars) - len(kept)`.

<details><summary>Solution</summary>

```vyrn solution
object Car:
    x: float
end

main:
    cars = [Car(-1.0), Car(50.0), Car(-90.0), Car(0.0), Car(10.0)]
    kept = cars.filter(c => c.x >= 0)
    print("removed {len(cars) - len(kept)}, kept {[c.x each c in kept]}")
end
```

</details>

## 23.4 The player, levels and game over

The turtle moves up one lane per key press. Reaching the top starts the
next level, with faster cars. Touching a car ends the game.

### Exercise

Write `Player` with `up()`, `at_top()` and `restart()`, and a `Level`
object that counts levels and gives the car speed (`3 + 2 * level`). Play
a scripted crossing of 3 lanes, twice.

```vyrn starter
const LANES = 3

object Player:
    lane: int = 0
end

object Level:
    number: int = 1
end

main:
    p = Player()
    level = Level()
    each _ in 1 to 6:
        p.up()
        if p.at_top():
            level.next()
            p.restart()
        end
        print("lane {p.lane}, level {level.number}, car speed {level.speed()}")
    end
end
```

```text expect
lane 1, level 1, car speed 5
lane 2, level 1, car speed 5
lane 0, level 2, car speed 7
lane 1, level 2, car speed 7
lane 2, level 2, car speed 7
lane 0, level 3, car speed 9
```

> Hint: `at_top` is `self.lane == LANES`; `restart` sets `lane` back to 0.
> Hint: `Level` needs `next()` (add one) and `speed()`.

<details><summary>Solution</summary>

```vyrn solution
const LANES = 3

object Player:
    lane: int = 0

    create up(self): self.lane += 1
    create at_top(self) -> bool: return self.lane == LANES
    create restart(self): self.lane = 0
end

object Level:
    number: int = 1

    create next(self): self.number += 1
    create speed(self) -> int: return 3 + 2 * self.number
end

main:
    p = Player()
    level = Level()
    each _ in 1 to 6:
        p.up()
        if p.at_top():
            level.next()
            p.restart()
        end
        print("lane {p.lane}, level {level.number}, car speed {level.speed()}")
    end
end
```

</details>

## 23.5 The whole game

```vyrn check
use vyrn.draw
use vyrn.random

const WIDTH = 600
const LANE = 40
const LANES = 8

object Car:
    x: float
    lane: int
    colour: str
end

main:
    let win = draw.window(WIDTH, LANE * (LANES + 2), "Road Crossing")
    let colours = ["red", "orange", "gold", "lime", "skyblue", "violet"]
    var cars = []
    var lane = 0
    var level = 1
    var alive = true
    loop win.open():
        if alive:
            if win.key_pressed("up"): lane += 1
            if lane > LANES:
                lane = 0
                level += 1
            end
            if random.int(1, 6) == 1:
                cars.append(Car(WIDTH * 1.0, random.int(1, LANES), random.choice(colours)))
            end
            each c in cars: c.x -= 2 + level
            cars = cars.filter(c => c.x > -60)
            let player_y = (LANES + 1 - lane) * LANE
            each c in cars:
                let car_y = (LANES + 1 - c.lane) * LANE
                if draw.overlaps(c.x, car_y + 5, 50, 30, 290, player_y + 10, 20, 20):
                    alive = false
                end
            end
        else if win.key_pressed("space"):
            cars = []
            lane = 0
            level = 1
            alive = true
        end
        win.clear("#333333")
        each c in cars:
            win.rect(c.x, (LANES + 1 - c.lane) * LANE + 5, 50, 30, c.colour)
        end
        win.circle(300, (LANES + 1 - lane) * LANE + 20, 10, "green")
        win.text("Level {level}", 10, 10, "white")
        if not alive: win.text("GAME OVER - press space", 200, 10, "white")
        win.show()
    end
end
```

### Exercise

The trickiest line is turning a lane into a screen position. Lane 0 is the
bottom (the start) and lane `LANES + 1` the top. Write `lane_to_y(lane)`
and print the y for lanes 0, 1 and 9.

```vyrn starter
const LANE = 40
const LANES = 8

create lane_to_y(lane):
    return 0
end

main:
    print(lane_to_y(0), lane_to_y(1), lane_to_y(9))
end
```

```text expect
360 320 0
```

> Hint: Screen `y` grows downwards, so higher lanes have smaller `y`: `(LANES + 1 - lane) * LANE`.

<details><summary>Solution</summary>

```vyrn solution
const LANE = 40
const LANES = 8

create lane_to_y(lane):
    return (LANES + 1 - lane) * LANE
end

main:
    print(lane_to_y(0), lane_to_y(1), lane_to_y(9))
end
```

</details>

**Day 23 done.** Spawning, updating, filtering and colliding many objects:
the same pattern runs bullets, enemies, particles and raindrops in every
game. Tomorrow: files.
