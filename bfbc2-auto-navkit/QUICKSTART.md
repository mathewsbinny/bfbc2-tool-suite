# NavKit Quickstart

NavKit makes the navigation data (`pathdata.hpd`) that Bad Company 2's AI soldiers need to find their
way around a level. You choose a level, NavKit measures the level in the running game, and it builds the
file. This page shows how to do that from start to finish.

To learn how it works inside, read [README.md](README.md) and the
[specification](research/KYNAPSE_SPEC.md).

## What works today

| Level | Can NavKit build it? |
|---|---|
| The 13 campaign levels (`sp_*`) | Yes. NavKit's files are checked against DICE's. |
| Onslaught maps (`ons_mp_*`) | Yes. They have AI, so they can be measured. |
| Stock multiplayer maps (`mp_*`) | **Not yet.** NavKit measures a level while its AI is running, and stock multiplayer maps start without AI. Support is planned. |

The GUI and `navkit.py levels` mark the levels you can use today as *samplable*.

## Before you start

You need:

1. **Project OPFOR, set up and built.** Run `setup_game.bat`, then `build.bat` (the game copy and OPFOR's
   core), then `build_menus.bat` (it also builds `tools\FbrbTool.exe` and `tools\DbxTool.exe`).
   OPFOR must be able to start a level: `OPFOR.exe` starts the game.
2. **Python 3.10 or newer** from [python.org](https://www.python.org/). When you install it, tick
   "Add python.exe to PATH".
3. **numpy.** Open a command prompt and run:

   ```
   pip install numpy matplotlib
   ```

   `matplotlib` is only needed for NavKit's picture tools, not for building.

Close the game before you use NavKit. Two steps start and close the game on their own.

## The easy way: the NavKit window

Double-click **`navkit\NavKit.pyw`**. The window has a level list at the top, the sampling area below
it, then the steps, and a log of what NavKit is doing at the bottom.

1. **Pick a level** in the *Level* list, for example `sp_01`. Leave *Only levels with AI* ticked to list
   only the levels NavKit can measure.
2. Click **1. Prepare.** NavKit unpacks the level, reads its objects (buildings, fences, stairs, spawn
   points) and chooses the area to measure. The first time, it also unpacks data shared by all levels,
   which takes about a minute. After that it takes seconds. The *Sampling area* boxes then show the
   chosen area.
3. Click **2. Sample in game** and confirm. NavKit starts the game, loads the level and measures it. Then
   it closes the game and does it again with a second soldier size. Each run takes about 1 to 5
   minutes. The game window may stay black the whole time. This is normal.
   **Do not click in the game or close it.** NavKit closes it when it has finished.
4. Click **3. Build.** NavKit creates `pathdata.hpd` from the measurements. This takes a few minutes
   for a large level.
5. Click **4. Check.** NavKit scores the result (see [Reading the check](#reading-the-check)).
6. To try it in game, click **Install in game**. NavKit keeps the level's original file. When you
   are done, click **Restore original** to put it back.

**Run all (1-4)** does steps 1 to 4 in a row. A tick next to a step means it has been done for that
level. **Open output folder** shows that level's files.

## The same steps from a command prompt

Run these from the Project OPFOR folder:

```
python navkit\navkit.py levels            list levels; "yes (samplable)" = NavKit can build it
python navkit\navkit.py prepare sp_01
python navkit\navkit.py sample  sp_01     asks before it starts the game; add --yes to skip the question
python navkit\navkit.py build   sp_01
python navkit\navkit.py check   sp_01
python navkit\navkit.py install sp_01     restore sp_01 puts the original back
python navkit\navkit.py status  sp_01     what has been done for a level
python navkit\navkit.py all     sp_01     prepare, sample, build and check in one go
```

## Reading the check

For a level that has an original file (campaign and Onslaught), **check** compares NavKit's file with it,
using the positions where the level's AI is told to go (patrol routes, positions to hold). For sp_01 it
prints:

```
1078 order / waypoint positions: ...
sp_01: pathdata.hpd  on an area: 633/1078  ...      <- the original file
navkit: pathdata.hpd  on an area: 651/1078  ...     <- NavKit's file
pairwise route check: 71277/79663 ... (89.5%)
```

- **"on an area"**: how many of those positions lie on walkable ground in each file. NavKit's number
  should be close to the original's or higher. Some positions are off the ground in both, such as
  windows and vehicle seats.
- **"pairwise route check"**: of all pairs of positions the original connects with routes, the share
  NavKit also connects. 85 to 95 % is typical. The original has gaps of its own, so this is a guide,
  not a pass mark.

For a level without an original file, check does a **self-check**: how many pairs of positions on
walkable ground NavKit's routes connect.

The full report is in `work\navkit\<level>\check.txt`.

## Where the files go

| File | What it is |
|---|---|
| `work\navkit\<level>\navkit.json` | the sampling area and sector numbering chosen by Prepare |
| `work\navsample\<level>_navkit.bin`, `..._navkit_hi.bin` | the two in-game measurements |
| `work\navkit\<level>\pathdata.hpd` | **the result** |
| `work\navkit\<level>\check.txt` | the full check report |
| `work\levels\<level>\` | the unpacked level and the objects NavKit read from it |
| `game\Output\win32\ai\levels\<level>\infantry\pathdata.hpd` | the file the game uses; Install puts NavKit's here and keeps the original as `pathdata.hpd.opfor-orig` |

NavKit never changes your original Bad Company 2 install. It works only on OPFOR's copy in `game\`.

## Changing the sampling area

Prepare sizes the area automatically:

- **Levels with path data:** the area that data covers, plus 20 m.
- **Other levels:** the area around the level's objects and spawn points, plus 50 m.

If parts of the level are missing at the edges, make the area larger. In the window, type new values in
*Sampling area* before you click Prepare or Sample in game. From a command prompt, add the area like this:

```
python navkit\navkit.py prepare sp_01 --box -900,-620,440,520 --y -30,600
```

`--box` is the west, south, east and north edge in metres (min X, min Z, max X, max Z). `--y` is the
bottom and top height. Make the top higher than the tallest roof. A larger area takes longer to sample.

## Troubleshooting

| Problem | What to do |
|---|---|
| "the game is running - close it first" | Quit Bad Company 2 (OPFOR) and try again. |
| "sampling did not finish: the game exited" | The level crashed or was closed during sampling. Look at the end of `game\logs\opfor.log`, then try again. |
| "sampling did not finish: timed out" | The run took over an hour. Use a smaller area or check that the level loads at all. |
| "...has no AI path data, so its AI (and the sampler) may not start" | The level is a stock multiplayer map, which is not supported yet. See [What works today](#what-works-today). |
| A run was cancelled or the PC crashed during sampling | NavKit edits `game\opfor.ini` while sampling and puts it back afterwards. If that was interrupted, the next Sample puts it back, and **Cancel** in the window does too. The saved copy is `work\navkit\opfor.ini.backup`. |
| The AI acts oddly with NavKit's file installed | Click **Restore original** (or `navkit.py restore <level>`). The level is back to its original data. |
| "FbrbTool.exe missing" / "no game copy" | Run `build_menus.bat` or `setup_game.bat` first (see [Before you start](#before-you-start)). |

## How NavKit stays clean

NavKit is written from a published specification of the file format. It contains no code from the
original licensed generator and never runs it. Levels are measured through the game's own physics
queries, and all game data comes from your own installation. See [README.md](README.md#rules-clean-room-approach).
