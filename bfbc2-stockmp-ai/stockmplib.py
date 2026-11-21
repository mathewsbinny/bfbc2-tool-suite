r"""Stock multiplayer maps with an AI layer: the library half of `bfbc2 stockmp` (summary, the path data a level
uses, installing a build into the game on its install ledger, round DF2). Round RT3 retired the command line
stockmp.py: bfbc2.exe answers it alone (bfbc2core.native.RETIRED_CLI; its usage text is
rust\bc2-cli\src\stockmp\stockmp_usage.txt). The tests import pathdata_for, summary, installed_files, apply_files, install, BACKUP, APPLIED; the pipeline is
pipeline.py (STOCK_MP_AI_PLAN.md, option c). Nothing is written outside <OPFOR>\work\stockmp\<lv>\ except by
install(paths, ['--apply']), which is for the user to run (never run by the build).
"""
import os
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'stockmp')

import levelmodel  # noqa: E402
import pipeline  # noqa: E402
from bfbc2core import gameledger  # noqa: E402

# worlds whose path data the user's install has under another level's name (DICE's console Onslaught files, which
# Napisal's ports install): stock level -> path-data folder
SAME_WORLD = {'mp_005': 'ons_mp_005gr', 'mp_005gr': 'ons_mp_005gr', 'mp_005sr': 'ons_mp_005gr',
              'mp_002': 'ons_mp_002', 'mp_002sr': 'ons_mp_002', 'mp_004': 'ons_mp_004', 'mp_004sdm': 'ons_mp_004',
              'mp_008': 'ons_mp_008', 'mp_008cq': 'ons_mp_008', 'mp_008sdm': 'ons_mp_008'}
BACKUP = pipeline.BACKUP           # '.opfor-stockmp-orig'
APPLIED = pipeline.APPLIED         # '.opfor-stockmp-applied'


def opt(argv: list[str], name: str, default: Any = None) -> Any:
    return argv[argv.index(name) + 1] if name in argv and argv.index(name) + 1 < len(argv) else default


def pathdata_for(paths: pipeline.Paths, folder: str | None = None) -> dict[str, str]:
    base = os.path.join(paths.game, 'Output', 'win32', 'ai', 'levels')
    for f in [folder] if folder else [paths.lv, SAME_WORLD.get(paths.lv)]:
        # as before this tool's install: its kept original, never the copy it installed where the level had none
        inf = pipeline.stock_file(os.path.join(base, f, 'infantry', 'pathdata.hpd')) if f else None
        if f and inf:
            veh = os.path.join(base, f, 'vehicle', 'pathdata.hpd')
            src = 'the level\'s own path data' if f == paths.lv else f'path data of {f} (the same world)'
            return {'infantry': inf, 'vehicle': pipeline.stock_file(veh) or veh, 'source': src}
    raise SystemExit(f'{paths.lv}: no path data found (looked in {base} for {folder or paths.lv} / '
                     f'{SAME_WORLD.get(paths.lv)}); build one with NavKit and pass --pathdata')


def summary(paths: pipeline.Paths) -> None:
    v, b = paths.bundles()[-1]
    d = pipeline.unpacked(b, paths.src(v))
    entry = pipeline.level_entry(d, pipeline._level_name(d, paths.lv))
    if entry is None:
        raise SystemExit(f'{paths.lv}: no level partition in {d}')
    lvl = pipeline.find_entry(d, entry)
    if lvl is None:
        raise SystemExit(f'{paths.lv}: the level partition {entry} is not in {d}')
    with open(lvl, 'rb') as f:
        m = levelmodel.model_of(f.read())
    print(f'{m.name}: mode {m.mode}, factions {m.factions}, attacker {m.attacker}, HasAI {m.has_ai}')
    for cp in m.capture_points:
        print(f'  capture point {cp.label or cp.name:14s} owner {cp.owner:11s} at {tuple(round(x) for x in cp.pos)}, '
              f'{len(cp.spawners)} spawners')
    kinds: dict[tuple[str, str], int] = {}
    for s in m.spawners:
        kinds[(s.team, s.player_type)] = kinds.get((s.team, s.player_type), 0) + 1
    print(f'  soldier spawners: {kinds}')
    for st in m.rush_stages():
        print(f'  stage {st.index}: {st.trigger_name}, M-COMs {[tuple(round(x) for x in p) for _g, p in st.mcoms]}')


def owner(lv: str) -> str:
    """The owner stock-MP installs a level's build as, in the game copy's install ledger."""
    return f'stockmp:{lv.lower()}'


LEVEL_FOLDERS = (('Dist', 'win32', 'levels'), ('Package', 'levels'), ('Output', 'win32', 'ai', 'levels'))
LEGACY = (BACKUP, APPLIED, '.opfor-stockmp-new', '.opfor-stockmp-prev')


def installed_files(game: str, lv: str) -> list[str]:
    """The game files under the level's own folders with a stock-MP marker from before the install ledger beside them
    (game-relative, '/'): read only there, never the whole game folder - a junction or an odd name elsewhere in the
    game copy is no concern of the revert (AU4-DS-9). Folders that cannot be read are skipped."""
    out = set()
    for parts in LEVEL_FOLDERS:
        top = os.path.join(game, *parts, lv)
        for dp, _d, fs in os.walk(top):
            for f in fs:
                low = f.lower()
                suf = next((s for s in LEGACY if low.endswith(s)), None)
                if suf:
                    out.add(os.path.relpath(os.path.join(dp, f[:-len(suf)]), game).replace('\\', '/'))
    return sorted(out)


def _shown(rel: str) -> str:
    return rel.replace('/', '\\')


def apply_files(game: str, out: str, rels: list[str], lv: str) -> int:
    """--apply as one journalled commit of stockmp:<lv> in the game copy's install ledger (round DF2): what each file
    replaced kept once by sha256; a failure (a locked file, a full disk) or a kill part way puts every file back, so
    Dist, Package and the path data are never from different builds (deep scan C1, AU4-DS-5). 0, or 1 with the
    reason on stderr."""
    led = gameledger.Ledger(game)
    files = {}
    for rel in sorted(rels, key=lambda r: r.replace('\\', '/')):
        with open(os.path.join(out, rel), 'rb') as f:
            files[rel.replace('\\', '/')] = f.read()
    try:
        plan = led.install(owner(lv), files, derived=False, note='stock-MP AI layer')
    except gameledger.LedgerError as e:
        print(f'nothing installed: {e}', file=sys.stderr)
        return 1
    for n in plan.notes:
        if n.endswith(': already installed'):
            print(f'already   {_shown(n[:-len(": already installed")])}')
        else:
            print(f'  {n}')
    for w in plan.writes:
        if w.startswith('write '):
            print(f'installed {_shown(w[6:])}')
        elif w.startswith('remove '):
            print(f'removed   {_shown(w[7:])} (no longer in the build)')
    return 0


def install(paths: pipeline.Paths, argv: list[str]) -> int:
    """--plan / --apply / --revert (round DF2: on the game copy's install ledger; stock-MP's markers from before it
    adopted first). The exit code (1 with the reason on stderr when nothing was done)."""
    game = opt(argv, '--game', paths.game)
    out = os.path.join(paths.root, 'out', 'game')
    mode = '--apply' if '--apply' in argv else '--revert' if '--revert' in argv else '--plan'
    me = owner(paths.lv)
    led = gameledger.Ledger(game)
    built = [os.path.relpath(os.path.join(dp, f), out) for dp, _d, fs in os.walk(out) for f in fs]
    if mode != '--revert':
        if not built:
            print(f'nothing built in {out} (run build first)', file=sys.stderr)
            return 1
        problems = pipeline.check_report(paths)
        if problems:                    # out\game must be one complete build, as its report lists it
            msg = f'{out} is not one complete build: ' + '; '.join(problems[:6])
            if mode == '--apply':
                print(msg + ' - nothing installed', file=sys.stderr)
                return 1
            print('warning: ' + msg)
    if mode == '--plan':
        for rel in sorted(built):
            print(f'{"replace" if os.path.isfile(os.path.join(game, rel)) else "new":8s} {rel}')
        var = f' --variant {paths.variant}' if paths.variant else ''
        print(f"(plan only: `install {paths.lv}{var} --apply` copies these into {game}, keeping what they replace in "
              "the game copy's install ledger)")
        return 0
    legacy = sorted(set(installed_files(game, paths.lv)) | {r.replace('\\', '/') for r in built})
    try:
        p = gameledger.adopt_legacy(led, me, legacy, 'stockmp')
    except gameledger.LedgerError as e:
        print(f'nothing {"installed" if mode == "--apply" else "reverted"}: {e}', file=sys.stderr)
        return 1
    if p is not None:
        print(f"stock-MP's markers from before the install ledger moved into it "
              f"({sum(1 for w in p.writes if w.startswith('adopt '))} file(s))")
    if mode == '--apply':
        return apply_files(game, out, built, paths.lv)
    mine = led.owner_files(me)
    if not mine:
        print(f'nothing installed by stock-MP in {game} (no install of {me} in its install ledger)')
        return 0
    lines = []
    for rel in mine:
        holders = led.holders(rel)
        i = holders.index(me) if me in holders else -1
        above = holders[i + 1] if 0 <= i < len(holders) - 1 else None
        _held, before = led.before(me, rel)
        if above:
            lines.append(f"kept      {_shown(rel)} (stock-MP's layer taken out from under {above}'s install: its revert "
                         'now puts back what was there before stock-MP)')
        elif before is not None:
            lines.append(f'restored  {_shown(rel)}')
        else:
            lines.append(f'removed   {_shown(rel)}')
    try:
        plan = led.uninstall(me)
    except gameledger.LedgerError as e:
        print(f'nothing reverted: {e}', file=sys.stderr)
        return 1
    for n in plan.notes:
        if 'taken out from under' not in n:
            print(f'  {n}')
    for line in lines:
        print(line)
    return 0
