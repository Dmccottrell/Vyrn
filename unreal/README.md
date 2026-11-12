# Vyrn in Unreal Engine

**VyrnScripting** is an Unreal Engine 5 plugin that lets you write gameplay
in Vyrn. Put a **Vyrn Script** component on an actor, point it at a
`.vyrn` file, and the script runs with the actor. It can move it, spawn
things, trace lines, read keys and talk to Blueprints. Save the file while
the game is running and the change shows straight away.

```vyrn
# Content/Scripts/spinner.vyrn
use unreal (rotate)

create tick(dt: float):
    rotate(yaw = 90 * dt)      # a quarter turn every second
end
```

## How it fits together

```
 your .vyrn scripts              Content/Scripts/door.vyrn, unreal.vyrn
        │  host.call("move", ...)          ▲ begin_play / tick(dt) / end_play
        ▼                                  │
 vyrn.dll   (the Vyrn runtime, built from runtime/embed, C API in vyrn.h)
        │                                  ▲
        ▼                                  │
 VyrnScripting plugin (C++)   UVyrnScriptComponent: loads the script, calls it
        │                     each frame, answers its host calls
        ▼
 Unreal Engine: actors, the world, Blueprints, input
```

- **runtime/embed** builds `vyrn.dll` (`libvyrn.so` on Linux,
  `libvyrn.dylib` on Mac). This is all of Vyrn as a library with a small C
  API (`include/vyrn.h`). Any C or C++ program can use it, not only Unreal.
- **unreal/VyrnScripting** is the Unreal plugin. It is a C++ module that
  loads the library, and a component that runs scripts.
- **unreal/VyrnScripting/Scripts/unreal.vyrn** is Vyrn's side: `Vec3`,
  `Rotator`, and friendly functions such as `location()`, `move()`,
  `spawn_actor()` and `trace()`.

Values cross between Vyrn and Unreal as JSON. That makes it simple and
safe, and fast enough for gameplay logic: AI, doors, pickups, turrets,
game rules and UI flow. Keep heavy per-frame maths over thousands of
objects in C++ or Niagara, and call it from Vyrn through a host function
of your own (see [Your own host functions](#your-own-host-functions-c)).

## What you need

- **Unreal Engine 5** with a C++ toolchain. On Windows that is Visual
  Studio 2022 (or Build Tools) with the **Game development with C++**
  workload. This is the same setup as any Unreal C++ project.
- **Rust**, to build `vyrn.dll`. This is the same setup you used to build
  `vyrn`.
- A **C++ project**. A Blueprint-only project gets C++ by adding one class
  in the editor: **Tools > New C++ Class > None > Create Class**.

## Install (Windows, PowerShell)

Close the Unreal Editor, then run this from the Vyrn folder:

```powershell
cd $HOME\Vyrn
```

```powershell
.\unreal\install-plugin.ps1 -Project "C:\Users\you\Documents\Unreal Projects\MyShooter"
```

If PowerShell says running scripts is disabled, allow it for this window
only, then run the command above again:

```powershell
Set-ExecutionPolicy -Scope Process Bypass
```

The script:

1. builds `vyrn.dll` (`cargo build --release --manifest-path runtime\embed\Cargo.toml`),
2. copies the plugin, with the DLL inside, to `MyShooter\Plugins\VyrnScripting`,
3. copies `unreal.vyrn` and the examples to `MyShooter\Content\Scripts`
   (it never overwrites a script you already have).

Open the project. When Unreal says the **VyrnScripting** module is
missing or out of date and asks to rebuild it, choose **Yes**. Check
**Edit > Plugins**: *Vyrn Scripting* should be ticked. The Output Log
shows `LogVyrn: Vyrn 0.4.0 is ready`.

Release builds of Vyrn also come with ready-made plugin zips
(`VyrnScripting-<version>-Win64.zip`, Linux, Mac). Unzip one into your
project's `Plugins` folder to skip building the DLL. Then copy
`unreal.vyrn` into `Content\Scripts` yourself.

### Doing it by hand (or on Linux and Mac)

```bash
cargo build --release --manifest-path runtime/embed/Cargo.toml
```

Then:

- Copy `unreal/VyrnScripting` to `YourProject/Plugins/VyrnScripting`.
- Copy the library from `runtime/embed/target/release/` into
  `Plugins/VyrnScripting/Source/ThirdParty/Vyrn/<platform>/`:
  - **Win64:** `vyrn.dll` and `vyrn.dll.lib`. Also copy `vyrn.dll` to
    `Plugins/VyrnScripting/Binaries/Win64/`.
  - **Linux:** `libvyrn.so`.
  - **Mac:** `libvyrn.dylib`.
- Copy the `.vyrn` files from `unreal/VyrnScripting/Scripts` to
  `YourProject/Content/Scripts`.

## Your first script

1. Drag a **Cube** into the level.
2. With it selected, click **Add** in the Details panel and choose **Vyrn Script**.
3. Set **Script** to `spinner.vyrn`.
4. Press **Play**: the cube turns and bobs.
5. Open `Content/Scripts/spinner.vyrn` in VS Code (the Vyrn extension
   gives colours, errors and completion). Change `DEFAULT_SPEED` to `360`
   and save. The cube speeds up without stopping the game.

If a script has a mistake, the error goes to the Output Log and the screen
with its file, line and column. The script's `tick` pauses until you fix
the file and save.

## Writing scripts

The component calls these functions, if the script has them:

| Function | When |
|---|---|
| `begin_play()` | once, when the game starts (or the actor spawns) |
| `tick(dt: float)` | every frame; `dt` is the seconds since the last one |
| `end_play()` | once, when the actor is removed or the game stops |
| `reloaded()` | after a hot reload; without it, `begin_play()` runs again |

A script lives as long as its actor, so `const` objects keep values from
frame to frame. Each actor with a Vyrn Script component runs its own copy
of the script:

```vyrn
use unreal (Vec3, location, set_location)

object Bob:
    start: Vec3 = Vec3()
    age: float = 0.0
end

const BOB = Bob()

create begin_play():
    BOB.start = location()
end

create tick(dt: float):
    BOB.age += dt
    set_location(BOB.start + Vec3(0.0, 0.0, 30.0 * BOB.age.sin()))
end
```

`print(...)` writes to the Output Log under `LogVyrn`, with the actor's
name.

### What `unreal.vyrn` gives you

Import what you use, `use unreal (Vec3, location, move)`, or all of it
with `use unreal` and then write `unreal.location()`.

Unreal measures in centimetres, degrees and seconds. X is forward, Y is
right and Z is up.

**Types**

| Name | What |
|---|---|
| `Vec3(x, y, z)` | a position or direction; `+ - *`, unary `-`, `.length()`, `.normalized()`, `.dot(v)` |
| `Rotator(pitch, yaw, roll)` | which way something faces |
| `Hit` | what `trace` hit: `.actor`, `.point`, `.normal`, `.distance` |

**This actor**

| Function | What |
|---|---|
| `me()` | the actor's name |
| `location()` / `set_location(p, sweep = false)` | where it is / put it somewhere (gives false if `sweep` hit something) |
| `move(by, sweep = true)` | move by an offset, stopping at walls; false if blocked |
| `rotation()` / `set_rotation(r)` | which way it faces |
| `rotate(pitch = 0, yaw = 0, roll = 0)` | turn by some degrees |
| `forward()` / `right()` | the directions it faces |
| `get_var(name, default = none)` / `set_var(name, value)` | the component's **Variables** |

**The world**

| Function | What |
|---|---|
| `time()` | game seconds since the level started |
| `player()` | the player pawn's name, or none |
| `find(tag)` | names of actors with a tag (Details > Actor > Tags) |
| `location_of(actor)` / `distance_to(actor)` | where another actor is / how far |
| `spawn_actor(class_path, at, facing = Rotator())` | place an actor; gives its name, or none |
| `destroy(actor)` | remove an actor |
| `trace(start, finish)` | a line through the world: the first `Hit`, or none |
| `key_down(key)` | whether a key is held: `"W"`, `"SpaceBar"`, `"LeftMouseButton"`, `"Gamepad_FaceButton_Bottom"` |

**Messages**

| Function | What |
|---|---|
| `log(x)` / `warn(x)` / `error(x)` | to the Output Log |
| `screen(text, seconds = 2, color = "white")` | on the game screen (`"red"`, `"#ff8800"`, ...) |
| `event(name, data = none)` | fires the component's **On Script Event** in Blueprints |

For `spawn_actor`, get a Blueprint's class path by right-clicking it in
the Content Browser and choosing **Copy Reference**. That gives
`/Script/Engine.Blueprint'/Game/FPS/BP_Rocket.BP_Rocket'`. Use the part
in quotes and add `_C`: `"/Game/FPS/BP_Rocket.BP_Rocket_C"`. Engine
classes are written like `"/Script/Engine.PointLight"`.

Actors are referred to by name, the text that `find`, `player` and
`spawn_actor` give. Names stay valid until the actor is destroyed.

### The examples

| Script | What it shows |
|---|---|
| `spinner.vyrn` | the three event functions, Variables, state that lasts between frames |
| `door.vyrn` | reacting to the player's distance, easing, telling Blueprints with `event` |
| `turret.vyrn` | an FPS turret: turning towards the player, line-of-sight traces, spawning projectiles on a cooldown |

## Vyrn and Blueprints

**Variables (editor to script):** the component's *Variables* map holds
per-actor settings. Add `speed` = `180` on one spinner and `30` on
another, and the same script reads them with `get_var("speed", 90)`.
Numbers and `true`/`false` arrive as numbers and booleans, and anything
else arrives as text. Scripts can write them with `set_var`, and
Blueprints can read the map.

**Events (script to Blueprint):** `event("door_opened")` or
`event("scored", {"points": 10})` fires **On Script Event (Name, Data
Json)** on the component. In the actor's Blueprint, select the component
and click **+** next to *On Script Event*, then switch on *Name* to play
a sound, update the HUD, and so on. To read the fields of *Data Json* in
Blueprints, turn on Unreal's **Json Blueprint Utilities** plugin and use
its **Load Json from String** and **Get Field** nodes.

**Calls (Blueprint to script):** the **Call Function** node runs any
script function, with the arguments as a JSON list:

```
Call Function   Name: "take_damage"   Args Json: [25, "explosion"]
  → Return Value: the result as JSON    Success: true / false
```

Next to it: **Has Function**, **Reload Script**, **Get Last Error** and
**Get Script Path**.

## Your own host functions (C++)

Any C++ function can be given to scripts. Register it on the component.
The arguments arrive as JSON values, and you return JSON text:

```cpp
#include "VyrnScriptComponent.h"

void AMyCharacter::BeginPlay()
{
    // Before Super::BeginPlay(), so the script sees it from the start.
    if (UVyrnScriptComponent* Script = FindComponentByClass<UVyrnScriptComponent>())
    {
        Script->RegisterHostFunction(TEXT("ammo"), [this](const TArray<TSharedPtr<FJsonValue>>&)
        {
            return FString::FromInt(Ammo);
        });
        Script->RegisterHostFunction(TEXT("give_ammo"), [this, Script](const TArray<TSharedPtr<FJsonValue>>& Args)
        {
            if (Args.Num() < 1 || Args[0]->Type != EJson::Number)
            {
                Script->Fail(TEXT("give_ammo needs a number"));
                return FString();
            }
            Ammo += static_cast<int32>(Args[0]->AsNumber());
            return FString();
        });
    }
    Super::BeginPlay();
}
```

In the script:

```vyrn
use vyrn.host

create tick(dt: float):
    if host.call("ammo") < 5: host.call("give_ammo", 30)
end
```

A host function with the same name as a built-in one replaces it. For
example, register your own `"key_down"` to read Enhanced Input actions
instead of raw keys.

`host.available()` is true inside Unreal, so a script can tell whether it
is running in the game or under `vyrn run` and `vyrn test`.

## Testing scripts without Unreal

The game rules in a script are ordinary Vyrn, so put them in functions
that take plain values and test them with `vyrn test`. This is the same
as the Asteroids project, whose `Game.update` takes the controls as a map.
Keep the `unreal.*` calls in thin `tick` functions.

The repository tests the plugin's scripts this way too.
`runtime/embed/tests/unreal_test.c` runs `spinner.vyrn`, `door.vyrn` and
`turret.vyrn` against a fake Unreal written in C, through the same
`vyrn.dll` API.

## Packaging the game

Scripts are plain files, not Unreal assets, so tell Unreal to ship them:
**Project Settings > Packaging > Additional Non-Asset Directories To
Copy** → add `Scripts`. The plugin's build rules already copy `vyrn.dll`
next to the game.

Turn off **Hot Reload** on the components for release builds if you don't
want players changing scripts. It costs only one file check every half
second either way.

## Troubleshooting

| Symptom | Fix |
|---|---|
| "The following modules are missing or built with a different engine version: VyrnScripting" | Choose **Yes** to rebuild. If it fails, right-click the `.uproject` > **Generate Visual Studio project files**, open the `.sln`, and build *Development Editor*. The first error in the Output window says what's wrong. |
| `Could not load the Vyrn runtime library` in the Output Log | `vyrn.dll` isn't in `Plugins\VyrnScripting\Binaries\Win64`. Run `install-plugin.ps1` again with the editor closed. |
| `LNK1181: cannot open input file 'vyrn.dll.lib'` | The library wasn't copied into `Source\ThirdParty\Vyrn\Win64`. Run `install-plugin.ps1` again. |
| `` `host` is not defined `` | Scripts that call `host.call` directly need `use vyrn.host`. `unreal.vyrn` already has it. |
| `the host has no function ...` | A typo in a `host.call` name, or a C++ function registered after the script called it. |
| `cannot read .../Content/Scripts/x.vyrn` | Check **Script** on the component. It is relative to `Content/Scripts`. |
| Copying fails because `vyrn.dll` is in use | Close the Unreal Editor first. |

The plugin is written against Unreal Engine 5's standard APIs (actor
components, `UGameplayStatics`, the Json module). Those have stayed stable
across 5.x. If your engine version reports a compile error in the plugin,
the first error line names the file and the API to adjust.

## Limits, for now

- Scripts run on the game thread, one engine per component. Each
  component's script has its own globals.
- Values cross as JSON, so pass names and numbers, not object handles.
  Actors are referred to by name.
- Unreal properties and functions aren't reflected automatically yet.
  Reach them through a host function (above) or a Blueprint event.
- Enhanced Input actions aren't wrapped yet. Use `key_down`, or register
  a host function that reads your Input Actions.
