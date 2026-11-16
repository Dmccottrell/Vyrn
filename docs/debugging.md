# Debugging

A debugger stops your program while it runs, so you can look inside it:
which line it is on, what every variable holds, and which function called
which. Use it when a program does something you didn't expect. Instead of
adding `print` everywhere, stop it and look.

## In VS Code

1. Open a `.vyrn` file.
2. Click in the margin left of a line number to set a **breakpoint**. A red
   dot appears there, and the program stops when it gets to that line.
3. Press **F5**, or run **Vyrn: Debug File** from the command palette.

When the program stops, the line is highlighted and the side bar shows:

- **Variables**: everything in scope, under *Locals*, and the program's
  top-level values, under *Globals*. Click the arrow beside a list, map
  or object to open it.
- **Call stack**: the function you are in, and the calls that led there.
  Click a line to see that function's variables.
- **Watch**: type a name such as `player.pos.x` or `scores[0]` to keep
  an eye on it.

Hovering over a variable in the editor shows its value. The buttons at the
top control what happens next:

| Button | Key | Does |
|---|---|---|
| Continue | F5 | run to the next breakpoint |
| Step Over | F10 | run this line, then stop at the next (a function it calls runs without stopping) |
| Step Into | F11 | go into the function this line calls |
| Step Out | Shift+F11 | finish this function, and stop in the one that called it |
| Stop | Shift+F5 | end the program |

What `print` shows appears in the **Debug Console**.

To pass arguments, or to stop at the first line every time, add a
`.vscode/launch.json`:

```json
{
  "version": "0.2.0",
  "configurations": [
    {
      "type": "vyrn",
      "request": "launch",
      "name": "Debug the game",
      "program": "${workspaceFolder}/main.vyrn",
      "args": ["--level", "2"],
      "stopOnEntry": true
    }
  ]
}
```

## In the terminal

```bash
vyrn debug score.vyrn
```

The program stops at its first line and waits for a command:

```
Debugging. Type `help` for the commands.
-> score.vyrn:12 in main
   12 |     var players = [Player("Ann", 3), Player("Bo", 5)]
(vyrn) break 7
breakpoint at score.vyrn:7
(vyrn) continue
-> breakpoint, score.vyrn:7 in bonus
   7 |     let doubled = points * 2
(vyrn) print points
points = 3
(vyrn) where
-> bonus at score.vyrn:7
   main at score.vyrn:15
```

| Command | Short | Does |
|---|---|---|
| `next` | `n` | run to the next line, stepping over calls |
| `step` | `s` | run to the next line, going into calls |
| `out` | `o` | run until this function returns |
| `continue` | `c` | run to the next breakpoint |
| `break 12`, `break other.vyrn:12` | `b` | stop at that line |
| `delete 12` | | remove that breakpoint |
| `print x`, `print player.pos.x`, `print items[0]` | `p` | show a value; lists, maps and objects show their parts |
| `vars` | `v` | every variable here |
| `where` | `w` | the calls that led here |
| `list` | `l` | the code around this line |
| `quit` | `q` | stop the program |
| Enter | | repeat the last command |

## How it works

`vyrn debug` runs the program on the interpreter. Before each instruction
it checks whether it has reached a new line, and if so whether it should
stop there: a breakpoint, the end of a step, or a pause. Functions that
normally run as machine code run on the interpreter while debugging, so
every line can be stopped at. Programs therefore run slower in the
debugger than under `vyrn run`.

`vyrn debug --dap` speaks the
[Debug Adapter Protocol](https://microsoft.github.io/debug-adapter-protocol/)
on standard input and output. That is how the VS Code extension uses it,
and any editor with a DAP client (Neovim, Emacs, Zed and others) can use it
the same way.
