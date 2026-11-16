# The Crown of Vyrn

A text adventure: explore a castle, unlock the keep and take the crown. Written in [Vyrn](https://github.com/dmccottrell/vyrn).

## Run it

Install Vyrn first (see its README), then in this folder:

```powershell
vyrn run
vyrn test
```

Commands in the game: `look`, `n`/`s`/`e`/`w`, `take <item>`, `drop <item>`, `i`, `save`, `load`, `help`, `quit`.

## Files

| File | What it does |
|---|---|
| `main.vyrn` | starts the game and reads the keyboard |
| `game.vyrn` | the rules: moving, items, locked doors, saving |
| `world.vyrn` | the castle's rooms, as data |
| `tests/` | plays the game from tests |

## What it shows

Structs and objects, a world kept as data, a command parser with `when`, JSON saves, and testing game logic. Every file is commented: read them top to bottom.
