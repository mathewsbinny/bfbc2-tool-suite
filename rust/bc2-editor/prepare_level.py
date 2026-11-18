"""Prepares a level for the map editor (bfbc2-editor runs this; EDITOR_ARCHITECTURE.md 3.10: an unprepared level is
prepared from the editor, not by sending the user to another tool).

  python prepare_level.py <level> [--opfor <Project OPFOR BFBC2 folder>] [--terrain-only]

The steps are the suite's own, nothing new: NavKit's prepare (navkit.prepare - the shared bundles sp_common and
mp_common unpacked once, the level's bundles unpacked, its objects, collision and spawn points extracted into
work\\levels\\<level> and work\\navkit) and the terrain unpack the Map Inspector does on a level's first opening
(bfbc2core.level.heightfield_of: the level's terrain-00.fbrb into work\\levels\\<level>\\terrain, stamped). Nothing
is written into the game copy. --terrain-only: only the terrain step (a prepared level whose terrain unpack is missing
or older than its bundle).

Output, one line each, for the editor's progress view:
  @step <n> <of> <what>     a step starts
  @log <text>               what the step says (NavKit's log)
  @error <text>             why it stopped, in plain words (exit code 1)
  @done                     finished (exit code 0)
"""
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(os.path.dirname(HERE))           # rust\bc2-editor -> the suite
for sub in ('', os.path.join('bfbc2-auto-navkit'), os.path.join('bfbc2-auto-navkit', 'tools')):
    p = os.path.join(SUITE, sub) if sub else SUITE
    if p not in sys.path:
        sys.path.insert(0, p)


def emit(kind: str, text: str = '') -> None:
    print(f'@{kind} {text}'.rstrip(), flush=True)


def main(argv: list[str]) -> int:
    args = list(argv[1:])
    opfor = None
    if '--opfor' in args:
        i = args.index('--opfor')
        opfor = args[i + 1] if i + 1 < len(args) else None
        del args[i:i + 2]
    terrain_only = '--terrain-only' in args
    args = [a for a in args if a != '--terrain-only']
    if len(args) != 1:
        emit('error', 'Which level? (prepare_level.py <level> [--opfor <folder>] [--terrain-only])')
        return 1
    try:
        import navkit
        from bfbc2core import level as levelmod
    except Exception as e:                       # the suite's Rust core missing, an old checkout ...
        emit('error', f'The suite\'s Python tools could not start: {e}')
        return 1
    try:
        pr = navkit.Project(opfor)
        lv = pr.level(args[0])
        steps = 1 if terrain_only else 4
        step = [0]

        def start(n: int, what: str) -> None:
            if n > step[0]:
                step[0] = n
                emit('step', f'{n} {steps} {what}')

        def log(text: str) -> None:
            t = str(text).strip()
            low = t.lower()
            if not terrain_only:
                if (low.startswith('unpacking') or low.endswith('already unpacked')) and '_common' not in low:
                    start(2, 'Unpacking the level')
                elif low.startswith('extracting objects'):
                    start(3, 'Extracting objects, collision and spawn points')
            if t:
                emit('log', t)

        if not terrain_only:
            start(1, 'Unpacking the shared bundles (sp_common, mp_common)')
            navkit.prepare(pr, lv, log=log)
        start(steps, 'Unpacking the terrain')
        levelmod.heightfield_of(pr.root, lv, log)
    except navkit.NavKitError as e:
        emit('error', str(e))
        return 1
    except Exception as e:                       # what the steps raise (a damaged bundle, a full disk ...)
        emit('error', f'{type(e).__name__}: {e}')
        return 1
    emit('done')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
