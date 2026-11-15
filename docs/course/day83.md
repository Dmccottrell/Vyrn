# Day 83: Portfolio project: a game of your own

Today's brief is a complete game, designed by you. Not a copy of a famous
game: take an idea you like and make it your own, with your rules, your
look and your twist. Days 77 to 79 are the toolkit.

## 83.1 The brief

Pick a size you can finish: a small finished game beats a large
unfinished one. Some starting points:

- A **one-screen arcade** game with a single clever mechanic: gravity
  that flips, a weapon that rebounds, a timer that rewinds.
- A **puzzle** game on a grid with ten hand-made levels of rising
  difficulty.
- A **3D explorer**: a small world to wander, with something to find
  (Day 78 and the game-dev guide).
- A **two-player** game on one keyboard, or over the network (`vyrn.net`).

## 83.2 Done means

- [ ] Title screen, playing, pause and game over, as a state machine
      (Day 79).
- [ ] Rules in `update` functions tested without a window, including an
      automatic player that checks the game can be won (Day 79).
- [ ] Movement scaled by `dt`, so it plays the same on any computer.
- [ ] Sound effects, and music that can be turned off.
- [ ] High scores (or level progress) saved, surviving a damaged file.
- [ ] Someone else has played it and you changed something because of what
      you saw.
- [ ] Built with `vyrn build`, zipped with its assets, with a README and a
      screenshot or short video.

## 83.3 A suggested route

1. Make the core mechanic fun with squares and circles before drawing any
   art. If it isn't fun as squares, art won't save it.
2. Then levels or difficulty, then screens and scores, then sound and art,
   in that order.
3. Watch someone play without helping them. Note every moment they're
   confused.

## 83.4 Warm-up

Games feel smoother when things **ease** instead of moving at constant
speed. Write `ease_out(t)`: for `t` from 0 to 1, give `1 - (1 - t)²`,
which starts fast and slows into place.

```vyrn starter
create ease_out(t: float) -> float:
    return t
end

main:
    print([round(ease_out(t / 4), 3) each t in 0 to 4])
end
```

```text expect
[0.0, 0.438, 0.75, 0.938, 1.0]
```

> Hint: `1 - (1 - t) ** 2`

<details><summary>Solution</summary>

```vyrn solution
create ease_out(t: float) -> float:
    return 1 - (1 - t) ** 2
end

main:
    print([round(ease_out(t / 4), 3) each t in 0 to 4])
end
```

</details>

## 83.5 Going further

- A level editor: levels as text files the game loads, made in the game.
- Gamepad support (`draw.gamepad()`).
- Put it in a browser: Vyrn runs on WebAssembly (the playground in
  `playground/` shows how).
