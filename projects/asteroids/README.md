# Asteroids

The arcade classic, written in [Vyrn](https://github.com/dmccottrell/vyrn): shoot the rocks, don't get hit.

## Run it

Install Vyrn first (see its README), then in this folder:

```powershell
vyrn run
vyrn test
```

Arrow keys turn and thrust, space fires, enter starts again.

## Files

| File | What it does |
|---|---|
| `main.vyrn` | the window, the keyboard and the sounds |
| `game.vyrn` | the rules and drawing a frame |
| `entities.vyrn` | the ship, bullets and rocks |
| `physics.vyrn` | vectors, movement and collisions |
| `tests/` | tests for the physics and the rules |

## What it shows

A game loop with frame time, a vector type with operators, inheritance with `super` and `init`, collisions, and sound. Every file is commented: read them top to bottom.
