# Day 79: Finishing a game

A game that runs isn't yet a game people can play: it needs a title
screen, a pause button, sound, a "game over" with the score, a table of
high scores that's still there tomorrow, and a way to give it to a friend.
These are the last 20% of a game that take 50% of the time, and today you
do them properly. The project turns a small dodging game into a finished
one.

## 79.1 Screens as states

A finished game has several **screens**: the title menu, playing, paused,
game over. Exactly one is active at a time. That's an enum (Day 61), and
the game is a **state machine**: each screen decides, from the input,
which screen comes next:

```vyrn
enum Screen:
    Title
    Playing
    Paused
    GameOver(score: int)
end

create next_screen(s: Screen, key: str, crashed: bool, score: int) -> Screen:
    when s:
        is Screen.Title:
            if key == "enter": return Screen.Playing
        is Screen.Playing:
            if crashed: return Screen.GameOver(score)
            if key == "p": return Screen.Paused
        is Screen.Paused:
            if key == "p": return Screen.Playing
            if key == "escape": return Screen.Title
        is Screen.GameOver(_):
            if key == "enter": return Screen.Title
    end
    return s
end

main:
    var s = Screen.Title
    each key, crashed in [("", false), ("enter", false), ("p", false), ("x", false), ("p", false), ("", true), ("enter", false)]:
        s = next_screen(s, key, crashed, 42)
        print("{key:<6} -> {s}")
    end
end
```

```text
       -> Screen.Title
enter  -> Screen.Playing
p      -> Screen.Paused
x      -> Screen.Paused
p      -> Screen.Playing
       -> Screen.GameOver(score=42)
enter  -> Screen.Title
```

Every possible move between screens is in one function, readable at a
glance and tested by a list of key presses. Because `Screen` is an enum,
the checker complains if a new screen (say `Settings`) is added and
`next_screen` forgets it.

## 79.2 High scores that last

High scores live in a file, so they survive closing the game. Keep the top
five, newest first among equal scores, and never crash because the file is
missing or damaged (Day 30):

```vyrn
use vyrn.fs
use vyrn.json

const FILE = "scores.json"

create load_scores() -> list:
    if not fs.exists(FILE): return []
    attempt:
        let data = json.load(FILE)
        if data is list: return data
    catch ValueError:
        print("(scores file damaged; starting fresh)")
    end
    return []
end

create add_score(name: str, score: int) -> int?:
    var scores = load_scores()
    scores.insert(0, {"name": name, "score": score})
    scores = sorted(scores, key = s => s["score"], reverse = true)[:5]
    json.save(FILE, scores)
    each i, s in enumerate(scores):
        if s["name"] == name and s["score"] == score: return i + 1
    end
    return none
end

main:
    if fs.exists(FILE): fs.remove(FILE)
    each name, score in [("ADA", 120), ("BOB", 80), ("CY", 200), ("DEE", 95), ("ED", 60), ("FAY", 10), ("GUS", 150)]:
        let place = add_score(name, score)
        print(name, score, "-> place", place ?? "not in the top 5")
    end
    print([s["name"] each s in load_scores()])
    write_text(FILE, r"[{broken")
    print(load_scores())
end
```

```text
ADA 120 -> place 1
BOB 80 -> place 2
CY 200 -> place 1
DEE 95 -> place 3
ED 60 -> place 5
FAY 10 -> place not in the top 5
GUS 150 -> place 2
["CY", "GUS", "ADA", "DEE", "BOB"]
(scores file damaged; starting fresh)
[]
```

### Exercise

Arcade games ask for exactly three letters. Write `clean_name(typed)`:
keep only letters, in capitals, at most three; `"???"` if none are left.

```vyrn starter
create clean_name(typed: str) -> str:
    return typed
end

main:
    print(clean_name("ada"), clean_name("  Ab1c2d "), clean_name("42!"))
end
```

```text expect
ADA ABC ???
```

> Hint: `"".join([c.upper() each c in typed if c.isalpha()])[:3]`

<details><summary>Solution</summary>

```vyrn solution
create clean_name(typed: str) -> str:
    let letters = "".join([c.upper() each c in typed if c.isalpha()])[:3]
    return letters if letters else "???"
end

main:
    print(clean_name("ada"), clean_name("  Ab1c2d "), clean_name("42!"))
end
```

</details>

## 79.3 Sound

Sound makes a game feel alive far more than its size suggests. `vyrn.sound`
plays files (WAV, Ogg, MP3), and can also **make** simple sounds from
waves, which is handy before you have real sound files:

```vyrn check
use vyrn.sound

main:
    let jump = sound.tone(660, 0.12, "square", 0.3)
    let crash = sound.tone(110, 0.4, "noise", 0.4)
    let music = sound.load("assets/theme.ogg")
    let playing = music.play(volume = 0.5, loop = true)
    jump.play()
    crash.play(speed = 0.8)
    playing.set_volume(0.2)
end
```

Without a sound device (on a server, in a test), playing is silent rather
than an error, so the game still runs. Two habits: keep effects short and
quiet compared with the music, and give the player a way to turn sound
off, saved with their settings.

## 79.4 Project: a finished dodging game

Blocks fall; the player moves left and right to dodge them; the score is
how long you survive. The game rules are a `Game` object with an
`update(dt, left, right)` method, so they can be tested without a window,
as on Day 77. The screen logic and high scores are 79.1 and 79.2:

```vyrn
use vyrn.random

const WIDTH = 400.0
const HEIGHT = 500.0

struct Block:
    x: float
    y: float
    speed: float
end

object Game:
    x: float = 200.0
    blocks: list = []
    time: float = 0.0
    next_block: float = 0.0
    crashed: bool = false

    create update(self, dt: float, left: bool, right: bool):
        if self.crashed: return
        self.time += dt
        if left: self.x = max(20.0, self.x - 300 * dt)
        if right: self.x = min(WIDTH - 20, self.x + 300 * dt)
        self.next_block -= dt
        if self.next_block <= 0:
            self.blocks.append(Block(random.float(0, WIDTH - 40), -40.0, 150 + self.time * 10))
            self.next_block = max(0.25, 0.9 - self.time * 0.02)
        end
        var kept = []
        each b in self.blocks:
            let moved = Block(b.x, b.y + b.speed * dt, b.speed)
            if moved.y < HEIGHT: kept.append(moved)
            let px = self.x - 15
            let py = HEIGHT - 50
            if moved.x < px + 30 and px < moved.x + 40 and moved.y < py + 30 and py < moved.y + 40:
                self.crashed = true
            end
        end
        self.blocks = kept
    end

    create score(self) -> int:
        return int(self.time * 10)
    end
end

create survive(seed: int, policy: any) -> int:
    random.seed(seed)
    let g = Game()
    var frames = 0
    loop not g.crashed and frames < 60 * 120:
        let left, right = policy(g)
        g.update(1.0 / 60, left, right)
        frames += 1
    end
    return g.score()
end

## Moves away from the nearest block that's coming down near the player.
create dodge(g: Game) -> (bool, bool):
    let near = [b each b in g.blocks if b.y > HEIGHT - 250 and abs(b.x + 20 - g.x) < 60]
    if not near: return (false, false)
    let b = near[0]
    let go_left = b.x + 20 > g.x and g.x > 60 or g.x > WIDTH - 60
    return (go_left, not go_left)
end

main:
    print("standing still scores", survive(1, g => (false, false)))
    print("dodging scores", survive(1, dodge))
end
```

```text
standing still scores 51
dodging scores 462
```

The two lines test the game by playing it automatically. The "dodging"
player isn't clever, but it should do better than standing still; if a
change to the rules ever made dodging useless, this would show it at
once. Game studios run thousands of such automated players to test
balance.

The window version adds the screens, drawing and sound around the same
`Game`:

```vyrn check
use vyrn.draw
use vyrn.sound

enum Screen:
    Title
    Playing
    GameOver(score: int)
end

main:
    let win = draw.window(400, 500, "Dodge")
    let crash = sound.tone(110, 0.4, "noise", 0.4)
    var screen = Screen.Title
    var time = 0.0
    var dt = 0.016
    loop win.open():
        win.clear("#101820")
        when screen:
            is Screen.Title:
                win.text("DODGE", 130, 180, "white", 5)
                win.text("press Enter", 135, 260, "#9e9e9e", 2)
                if win.key_pressed("enter"):
                    time = 0.0
                    screen = Screen.Playing
                end
            is Screen.Playing:
                time += dt
                win.text("score {int(time * 10)}", 10, 10, "white", 2)
                if win.key_pressed("escape"):
                    crash.play()
                    screen = Screen.GameOver(int(time * 10))
                end
            is Screen.GameOver(score):
                win.text("GAME OVER", 95, 200, "tomato", 4)
                win.text("score {score}", 140, 260, "white", 2)
                if win.key_pressed("enter"): screen = Screen.Title
        end
        dt = win.show()
    end
end
```

(In this sketch Escape stands in for crashing; put the `Game` object's
`update` in the `Playing` screen, draw its blocks, and switch to
`GameOver` when `crashed` is true.)

## 79.5 Giving it to people

`vyrn build` makes a single program file (Day 52). A game also has
**assets**: pictures, sounds, levels. Keep them in an `assets` folder next
to the game and load them with paths relative to it:

```text
dodge/
  vyrn.toml
  main.vyrn
  game.vyrn
  assets/
    theme.ogg
    font.png
  tests/
    test_game.vyrn
```

```bash
vyrn build -o dodge
```

Zip `build/dodge` together with the `assets` folder, and anyone on the same
kind of computer can unzip it and play. Build once on each system you want
to support (Windows, macOS, Linux); a GitHub Actions workflow (Day 52) can
build all three on every release. Before sharing, play it all the way
through once more: menus, pause, game over, high scores, sound off.

### Exercise

Write `stars(score, thresholds)`, giving a rating of 1 to 3 stars: one
star for any score, two for reaching `thresholds[0]`, three for
`thresholds[1]`, shown as `"★★☆"`.

```vyrn starter
create stars(score: int, thresholds: list[int]) -> str:
    return ""
end

main:
    print(stars(10, [100, 250]), stars(120, [100, 250]), stars(400, [100, 250]))
end
```

```text expect
★☆☆ ★★☆ ★★★
```

> Hint: count `n = 1 + len([t each t in thresholds if score >= t])`,
> then `"★" * n + "☆" * (3 - n)`.

<details><summary>Solution</summary>

```vyrn solution
create stars(score: int, thresholds: list[int]) -> str:
    let n = 1 + len([t each t in thresholds if score >= t])
    return "★" * n + "☆" * (3 - n)
end

main:
    print(stars(10, [100, 250]), stars(120, [100, 250]), stars(400, [100, 250]))
end
```

</details>

## Today you learned

- Screens are an enum and the game a state machine, all moves in one
  tested function.
- Save high scores to a file, keep the top few, survive a damaged file.
- `sound.tone` and `sound.load` for effects and music; silent without a
  device; let players turn it off.
- Test a game by letting simple automatic players play it.
- Ship with `vyrn build` plus an `assets` folder.
