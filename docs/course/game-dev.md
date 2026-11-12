# Making Games in Vyrn: from Pong to a First-Person Shooter

This guide teaches game programming the way it's done in C++ engines, in
Vyrn: the game loop, frame time, input, entities, collision, 3D cameras,
first-person controls and hitscan shooting. It ends with a complete small
FPS, `examples/fps.vyrn`, and a plan for growing it toward the shooters
you play.

Every example that doesn't need a window runs as shown; the ones that open
a window are marked and compile-checked.

## 1. The game loop and frame time

Every game, from Pong to Call of Duty, runs the same loop many times a
second:

1. **Input**: read the keyboard, mouse and controllers.
2. **Update**: move everything, by how much time has passed.
3. **Draw**: render the new state.

```vyrn check
use vyrn.draw

main:
    let win = draw.window(640, 360, "Loop")
    var x = 0.0
    var dt = 0.016
    loop win.open():
        x += 120 * dt
        win.clear("black")
        win.circle(x % 640, 180, 12, "orange")
        dt = win.show()
    end
end
```

`win.show()` gives back **`dt`**: the seconds since the last frame (about
0.016 at 60 frames a second). Multiplying speeds by `dt` makes motion the
same on fast and slow computers: `x += 120 * dt` moves 120 pixels per
second, however many frames that is. C++ engines call this *delta time*.

```vyrn
create advance(position, speed, dt):
    return position + speed * dt
end

main:
    var fast = 0.0
    each _ in 1 to 120: fast = advance(fast, 120.0, 1.0 / 120)
    var slow = 0.0
    each _ in 1 to 30: slow = advance(slow, 120.0, 1.0 / 30)
    print("{fast:.1f} {slow:.1f}")
end
```

```text
120.0 120.0
```

One second of game time at 120 fps and at 30 fps: the same distance.

## 2. Entities: one object per thing in the world

Each player, enemy, bullet or pickup is an object with its own state and
an `update(dt)` method. The world is lists of them:

```vyrn
object Projectile:
    x: float
    z: float
    vx: float
    vz: float
    life: float = 2.0

    create update(self, dt: float):
        self.x += self.vx * dt
        self.z += self.vz * dt
        self.life -= dt
    end

    create alive(self) -> bool: return self.life > 0
end

main:
    var shots = [Projectile(0.0, 0.0, 10.0, 0.0), Projectile(0.0, 0.0, 0.0, 30.0, life = 0.5)]
    each _ in 1 to 60:
        each s in shots: s.update(1.0 / 60)
        shots = shots.filter(s => s.alive())
    end
    print(len(shots), "left", [round(s.x, 1) each s in shots])
end
```

```text
1 left [10.0]
```

Update everything, then remove what's finished. Bigger engines organise
this as an *entity-component system*; the idea is the same.

## 3. 3D: the camera

A 3D scene is drawn through a **camera**: where you stand and which way
you look. `yaw` turns left and right, `pitch` looks up and down.

```vyrn
use vyrn.draw

main:
    let cam = draw.camera(0, 1.6, 0)
    print([round(v, 2) each v in cam.direction])
    cam.turn(90, 0)
    print([round(v, 2) each v in cam.direction])
    cam.turn(0, 30)
    print([round(v, 2) each v in cam.direction])
end
```

```text
[0.0, 0.0, 1.0]
[1.0, 0.0, 0.0]
[0.87, 0.5, 0.0]
```

- `cam.direction` is where it looks, as a direction of length 1.
- `cam.forward(d)` walks along the ground the way it faces; `cam.strafe(d)`
  steps sideways; `cam.fly(d)` moves exactly where it looks.
- `cam.position`, `cam.yaw`, `cam.pitch`, `cam.move_to(x, y, z)`,
  `cam.look_at(x, y, z)`, `cam.set_fov(degrees)`.

Shapes are drawn through the camera: `win.box(cam, [x, y, z, width,
height, depth], colour)`, `win.cube`, `win.sphere`, `win.plane`, and your
own models with `win.mesh(cam, model, x, y, z, size, colour, rotation)`.

## 4. First-person controls

Mouse look turns the camera by how far the mouse moved this frame;
WASD moves relative to where you face:

```vyrn check
use vyrn.draw

main:
    let win = draw.window(800, 500, "Walk around")
    win.hide_cursor()
    let cam = draw.camera(0, 1.6, -8)
    var dt = 0.016
    loop win.open():
        cam.turn(win.mouse_dx * 0.15, -win.mouse_dy * 0.15)
        if win.key_down("w"): cam.forward(5 * dt)
        if win.key_down("s"): cam.forward(-5 * dt)
        if win.key_down("d"): cam.strafe(5 * dt)
        if win.key_down("a"): cam.strafe(-5 * dt)
        win.clear("#87b5e0")
        win.box(cam, [0, -0.05, 0, 30, 0.1, 30], "#5d7a4a")
        win.box(cam, [0, 1, 5, 2, 2, 2], "tomato")
        dt = win.show()
    end
end
```

`win.mouse_dx` and `win.mouse_dy` are the mouse movement since the last
frame. `-mouse_dy` makes moving the mouse up look up.

## 5. Collision: boxes and sliding

`vyrn.game` has the maths of 3D games. A **box** is a list `[x, y, z,
width, height, depth]` (its centre and size), the same list `win.box`
draws, so one list per wall does both jobs.

```vyrn
use vyrn.game

main:
    let wall = [0, 1, 5, 10, 2, 1]
    print(game.overlaps3d([0, 1, 4.6, 0.6, 1.8, 0.6], wall))
    print(game.overlaps3d([0, 1, 3.0, 0.6, 1.8, 0.6], wall))
    # Walking diagonally into the wall: the blocked part stops, the rest slides.
    let pos = game.slide([0, 1, 4.0], [0.5, 0, 0.5], [wall], [0.6, 1.8, 0.6])
    print(pos)
end
```

```text
true
false
[0.5, 1.0, 4.0]
```

`game.slide(position, movement, boxes, size)` tries each axis on its own,
so moving into a wall at an angle slides along it, the way every FPS
feels.

## 6. Hitscan shooting: ray casts

Most FPS guns are **hitscan**: the shot is a ray from the eye along the
view, and whatever it hits first is hit. `game.raycast` does exactly that:

```vyrn
use vyrn.game

main:
    let crate = [0, 1, 4, 1, 1, 1]
    let enemy = [0, 1, 10, 0.8, 2, 0.8]
    let world = [crate, enemy]
    print(game.raycast([0, 1, 0], [0, 0, 1], world))
    print(game.raycast([0, 1.8, 0], [0, 0, 1], world))
    print(game.raycast([0, 1, 0], [0, 0, -1], world))
end
```

```text
{"index": 0, "distance": 3.5, "point": [0.0, 1.0, 3.5]}
{"index": 1, "distance": 9.6, "point": [0.0, 1.8, 9.6]}
none
```

Aimed low, the crate blocks the shot; aimed over it, the enemy is hit;
aimed behind, nothing. `index` says which box was hit, so a game checks
whether that box belongs to an enemy. In a game, the ray starts at
`cam.position` and goes along `cam.direction`.

### Exercise: damage falloff

Many shooters reduce damage with distance. Write `damage(distance)`: 30
up to 10 metres, then 1 less per metre, but never below 15.

```vyrn
create damage(distance):
    if distance <= 10: return 30
    return max(15, 30 - (distance - 10).floor())
end

main:
    print([damage(d) each d in [3, 10, 14.5, 40]])
end
```

```text
[30, 30, 26, 15]
```

## 7. Models from 3D tools

Make a model in Blender (or download one), export it as **Wavefront OBJ**,
and load it:

```vyrn
use vyrn.draw

main:
    write_text("ramp.obj", "v 0 0 0\nv 2 0 0\nv 2 1 0\nv 0 0 2\nv 2 0 2\nv 2 1 2\nf 1 2 3\nf 4 6 5\nf 1 4 5 2\nf 2 5 6 3\nf 1 3 6 4\n")
    let ramp = draw.load_obj("ramp.obj")
    print(ramp)
end
```

```text
Mesh(6 points, 8 triangles)
```

Draw it with `win.mesh(cam, ramp, x, y, z, size, colour, rotation =
[0, 90, 0])`. Faces with four or more corners are split into triangles.

## 8. Speed

Game code runs every frame, so speed matters:

- **Measure first**: `vyrn run --profile game.vyrn` shows which functions
  take the time when the game ends.
- **Typed functions run as machine code.** A function whose parameters and
  result have types (`create step(x: float, v: float, dt: float) -> float`)
  is compiled by Vyrn's native compiler, like C++.
- **Do less per frame**: remove finished entities, don't rebuild lists
  that don't change, and keep the number of shapes drawn reasonable.

3D shapes are filled on the **GPU** when the computer has one (through
Vulkan, Metal or DirectX 12), and by Vyrn's own software renderer
otherwise; both give the same picture. `draw.renderer()` says which is in
use, and `draw.use_gpu(false)` switches to software. A room of a few dozen
boxes takes about 2 ms per frame in software at 800 by 500; the GPU keeps
big scenes fast.

**Sound** comes from `vyrn.sound`: load effects and music once, play them
whenever something happens.

```vyrn
use vyrn.sound

main:
    let shot = sound.tone(180, 0.08, wave = "noise", volume = 0.4)
    let hit = sound.tone(880, 0.05, wave = "square", volume = 0.3)
    print(shot, hit)
    shot.play()
end
```

```text
Sound(0.08 seconds) Sound(0.05 seconds)
```

Use `sound.load("shot.wav")` for recorded effects and
`music.play(volume = 0.4, loop = true)` for background music.

## 9. The complete game: Target Range

`examples/fps.vyrn` puts it all together: an arena of walls and crates,
mouse look, WASD and jumping with gravity, sliding collision, a magazine
with reloading, moving targets that take three hits, a crosshair, score,
timer and accuracy. Run it:

```bash
vyrn run examples/fps.vyrn
```

Read it top to bottom: every part is one of the sections above.

## 10. Multiplayer

Online games send small messages many times a second. `vyrn.net` has the
two kinds games use:

- **UDP** for things sent every frame (inputs, positions): fast, and a lost
  message doesn't matter because the next one is 16 ms behind.
- **TCP** for things that must arrive (chat, joining a match, scores).

```vyrn
use vyrn.net
use vyrn.time

main:
    let server = net.udp()
    let player = net.udp()
    player.send("127.0.0.1:{server.port}", {"name": "Ada", "keys": ["left", "up"]})
    time.sleep(0.05)
    let msg = server.receive()
    print(msg["data"]["name"], msg["data"]["keys"])
    server.send(msg["from"], [{"name": "Ada", "x": 120, "y": 80}])
    time.sleep(0.05)
    print(player.receive()["data"])
end
```

```text
Ada ["left", "up"]
[{"name": "Ada", "x": 120, "y": 80}]
```

The pattern every competitive shooter uses: **players send their inputs,
the server decides what happens** (moves everyone, checks hits) and sends
the world back. Players can't cheat by claiming they hit someone, because
only the server decides. `examples/multiplayer.vyrn` is a complete server
and client:

```bash
vyrn run examples/multiplayer.vyrn server
```

```bash
vyrn run examples/multiplayer.vyrn play 127.0.0.1 Ada
```

## 11. From here to a modern shooter

Games like Call of Duty or XDefiant are made by hundreds of people over
years, with engines that took decades. The way there is the way they
started: one small, working game, improved one system at a time. A
roadmap, roughly in order:

| Step | What to build | In Vyrn today |
|---|---|---|
| 1 | Movement, mouse look, collision, hitscan | ✅ `vyrn.draw`, `vyrn.game` |
| 2 | Levels from files (JSON or your own format) | ✅ `vyrn.json`, files |
| 3 | Enemies that chase and shoot back (state machines, line of sight with `game.raycast`) | ✅ objects, `when` |
| 4 | Weapons as data: damage, fire rate, recoil, spread | ✅ structs and maps |
| 5 | Menus, settings, saved progress | ✅ widgets, files, `vyrn.db` |
| 6 | Models from Blender | ✅ `draw.load_obj` |
| 7 | A GPU renderer for big scenes | ✅ automatic with a GPU; textures and per-pixel lighting planned |
| 8 | Sound effects and music | ✅ `vyrn.sound` |
| 9 | Online multiplayer (a server, client prediction, lag compensation) | ✅ `vyrn.net` (UDP and TCP); prediction and lag compensation are yours to build on it |

Start with step 1 (done!), then make the targets fight back.
