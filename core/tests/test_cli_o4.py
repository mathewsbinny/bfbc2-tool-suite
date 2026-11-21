r"""Round O4's command lines in Rust (rust\bc2-cli, bfbc2.exe: NavKit's driver navkit.py, the editor's prepare_level.py,
levelobjects, ladders, ordercheck, navcompare; NavKit's other tools; the test harness; texqa) against the Python command
lines they port, as processes: the same exit codes, the same bytes on stdout and stderr (each alone, and both on one
pipe), the same files and the same log records (rust/BENCHMARKS.md, "O4"). The method and the machinery are round
O2's (test_cli_o2.py: Place, run, run_all, diff).

  python core\tests\test_cli_o4.py            quick: every recorded command line through bfbc2.exe, against what the
                                              Python command lines answered (cli_o4_<group>_golden*.json)
  python core\tests\test_cli_o4.py --live     + the Python command lines run again beside it, both ways compared
  python core\tests\test_cli_o4.py --game     + Project OPFOR's real calls replayed read-only (inputs copied to a
                                              temporary folder, the game copy read in place), both ways, timed
  python core\tests\test_cli_o4.py --record   record the goldens from the Python command lines
     --group navkit|navtools|harness|texqa   one group only (default: all four)
     --who                    list the lines bfbc2.exe handed to the Python
     --exe <bfbc2.exe>        the binary (default: native.find_binary)
     --ref <suite folder>     the suite copy whose Python command lines answer (default: BFBC2_O2_REF, else this
                              checkout): while the Rust sources change, a frozen copy keeps a matching core module

The cases come from cli_o4_<group>.py beside this file: cases() (the quick tier's command lines, on synthetic inputs
made in the case folder: no game data) and opfor_calls(opfor) (the --game tier's: (label, tool, args, files, env,
must_succeed)). A group module may also give game_check(check, exe, ref) for a game-tier proof of its own (the level
prepare's byte-identity: test_prepare_o4.py).

Retired tools (round RT1, bfbc2core.native.RETIRED_CLI: graphstats, prepare_level - their .py deleted): their lines
run through bfbc2.exe only. A line the Rust answers is compared with its recording as before; a line it handed to
the Python (BFBC2_CLI_TRACE: 'retired') must end with the retirement error (test_cli_o2.retired_check), its
recording kept as the record of what the Python said there. --live and --game run no Python for them (a working
OPFOR call must still work in Rust); --record keeps their recorded lines.

Skipped (exit 0) without the binary or without the Rust core module (the Python command lines need it).
"""
from __future__ import annotations

import datetime
import importlib
import json
import os
import re
import shutil
import sys
import time
from collections.abc import Callable
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)
import csharp_golden as cg  # noqa: E402
import test_cli_o2 as o2  # noqa: E402

GROUPS = ('navkit', 'navtools', 'harness', 'texqa')
PREFIX = 'cli_o4_'
SHARD = o2.SHARD


def group_module(group: str) -> Any:
    return importlib.import_module(f'{PREFIX}{group}')


def golden_names(group: str) -> list[str]:
    base = f'{PREFIX}{group}_golden'
    return sorted(n for n in os.listdir(HERE) if re.fullmatch(base + r'(_\d+)?\.json', n))


def load_golden(group: str) -> dict[str, Any]:
    out: dict[str, Any] = {}
    for n in golden_names(group):
        out.update(cg.load(n).get('cases', {}))
    return out


def save_golden(group: str, cases: dict[str, Any], meta: dict[str, Any]) -> list[str]:
    for old in golden_names(group):
        os.remove(cg.path(old))
    shards: list[dict[str, Any]] = [{}]
    size = 0
    for k, v in cases.items():
        n = len(json.dumps(k)) + len(json.dumps(v, ensure_ascii=False).encode('utf-8')) + 4
        if size + n > SHARD and shards[-1]:
            shards.append({})
            size = 0
        shards[-1][k] = v
        size += n
    names = []
    for i, part in enumerate(shards):
        name = f'{PREFIX}{group}_golden.json' if len(shards) == 1 else f'{PREFIX}{group}_golden_{i}.json'
        cg.save(name, {'meta': dict(meta, shard=i, shards=len(shards)), 'cases': part})
        names.append(name)
    return names


def game(check: Callable[..., None], exe: str, group: str, mod: Any) -> dict[str, Any]:
    """Project OPFOR's real calls of the group, read-only: each a (label, tool, args, files, env, must_succeed) of
    the group's opfor_calls; each alone and on one pipe, both ways (test_cli_o2.game's method)."""
    try:
        opfor: str | None = o2.paths.find_opfor()
    except o2.paths.PathsError:
        opfor = None
    if not opfor:
        print('     (no Project OPFOR BFBC2 copy: the game tier skipped)')
        return {}
    pl = o2.Place(20 + GROUPS.index(group))
    cmd = o2.commands(exe)
    bad: list[str] = []
    times = {'rust': 0.0, 'python': 0.0}
    runs = 0
    answered = {'rust': 0, 'python': 0}
    calls: list[Any] = []
    try:
        calls = mod.opfor_calls(opfor)
        for label, tool, args, sources, env, must in calls:
            files = {}
            for name, src in sources.items():
                if isinstance(src, tuple) or src is None:     # (contents, modification time) made by the group
                    files[name] = src
                    continue
                with open(src, 'rb') as f:
                    files[name] = (f.read(), os.path.getmtime(src))
            got: dict[str, Any] = {}
            if tool in o2.RETIRED:              # round RT1: Rust alone; a working call must still work in Rust
                for merged in (False, True):
                    r, took, w = o2.run([exe, tool], pl, files, args, env, merged)
                    times['rust'] += took
                    runs += 1
                    if not merged:
                        answered['rust' if w == 'rust' else 'python'] += 1
                        if must and (w != 'rust' or r['code'] != 0):
                            bad.append(f'{label}: the retired tool\'s working call failed in Rust (who {w!r}, exit '
                                       f'{r["code"]}: {(r["err"] or r["out"])[-160:]!r})')
                continue
            for side in ('python', 'rust'):
                for merged in (False, True):
                    r, took, w = o2.run(cmd[side](tool), pl, files, args, env, merged)
                    times[side] += took
                    runs += 1
                    got[f'{side} {merged}'] = r
                    if side == 'rust' and not merged:
                        answered[w or 'python'] += 1
                        if (w or 'python') == 'python' and '--who' in sys.argv:
                            print(f'     handed to Python: {label}')
            if must and got['python False']['code'] != 0:
                bad.append(f'{label}: the Python command line failed here (exit code {got["python False"]["code"]}: '
                           f'{(got["python False"]["err"] or got["python False"]["out"])[:160]!r}), not a replay of '
                           'a working call')
            for merged in (False, True):
                why = o2.diff(got[f'python {merged}'], got[f'rust {merged}'])
                if why:
                    bad.append(f'{label}{" (merged)" if merged else ""}: {why}')
    finally:
        shutil.rmtree(pl.root, ignore_errors=True)
    print(f'     {group}: OPFOR\'s calls: {runs} runs, Python {times["python"]:.1f} s, Rust {times["rust"]:.1f} s; '
          f'answered by Rust {answered["rust"]}, handed to Python {answered["python"]}')
    check(not bad, f'{group}: Project OPFOR\'s real calls, read-only ({len(calls)} command lines), each alone and on '
          f'one pipe: Rust answered every one as Python did', ' | '.join(bad[:3]))
    return {'calls': len(calls), 'times': times, 'answered': answered}


def main(argv: list[str]) -> int:
    exe = o2.find_exe(argv)
    if exe is None or not os.path.isfile(exe):
        print('skipped: bfbc2.exe is not built (python rust\\build_native.py)')
        return 0
    if o2.REF == o2.SUITE and o2.native.load() is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    groups = [argv[argv.index('--group') + 1]] if '--group' in argv else list(GROUPS)
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    t = time.time()
    for group in groups:
        mod = group_module(group)
        o2.LOOSE[:] = list(getattr(mod, 'LOOSE', ()))   # round DF2: files compared by size (the ledger's records)
        rows: list[o2.Case] = mod.cases()
        keys = [r[0] for r in rows]
        if len(set(keys)) != len(keys):
            check(False, f'{group}: case keys are unique', str([k for k in keys if keys.count(k) > 1][:3]))
            continue
        if '--record' in argv:
            results, times, _who = o2.run_all(rows, ['python'], exe)
            kept = load_golden(group)                    # a retired tool's lines keep their recording
            names = save_golden(group, {k: results['python|' + k] if 'python|' + k in results else kept[k]
                                        for k in keys if 'python|' + k in results or k in kept},
                                {'recorded': datetime.date.today().isoformat(), 'by': 'the Python command lines',
                                 'python': sys.version.split()[0], 'cases': len(rows)})
            print(f'{group}: recorded {len(rows)} command lines from the Python ({times["python"]:.0f} s): '
                  f'{", ".join(names)}')
            continue
        sides = ['rust', 'python'] if '--live' in argv else ['rust']
        results, times, who = o2.run_all(rows, sides, exe)
        golden = load_golden(group)
        bad, live_bad, missing = [], [], []
        tool_of = {r[0]: r[1] for r in rows}
        for key in keys:
            got = results['rust|' + key]
            if tool_of[key] in o2.RETIRED:
                why = o2.retired_check(tool_of[key], golden.get(key), got, who.get(key, ''))
                if why == 'not recorded':
                    missing.append(key)
                elif why:
                    bad.append(f'{key}: {why}')
                continue
            if key not in golden:
                missing.append(key)
            elif why := o2.diff(golden[key], got):
                bad.append(f'{key}: {why}')
            if 'python' in sides and (why := o2.diff(results['python|' + key], got)):
                live_bad.append(f'{key}: {why}')
        per_tool = []
        for tool in sorted({r[1] for r in rows}):
            ks = [r[0] for r in rows if r[1] == tool]
            r_n = sum(who.get(k) == 'rust' for k in ks)
            if tool in o2.RETIRED:
                per_tool.append(f'{tool} {len(ks)} (Rust {r_n}, the retirement error {len(ks) - r_n})')
                continue
            per_tool.append(f'{tool} {len(ks)} (Rust {r_n}, Python {len(ks) - r_n})')
            if '--who' in argv:
                for k in ks:
                    if who.get(k) != 'rust':
                        print(f'     handed to Python: {k}')
        check(not bad and not missing, f'{group}: {len(rows)} command lines through bfbc2.exe ({"; ".join(per_tool)}): '
              'exit codes, both streams\' bytes, files and log records as the Python command lines answered '
              f'({len(rows) - len(bad) - len(missing)} of {len(rows)} as recorded)',
              ' | '.join(bad[:3] + [f'not recorded: {m}' for m in missing[:3]]))
        if 'python' in sides:
            check(not live_bad, f'{group}: the same {len(rows)} through the Python command lines now, both ways: the '
                  f'same answers (Python {times["python"]:.0f} s, Rust {times["rust"]:.0f} s of process time)',
                  ' | '.join(live_bad[:3]))
        if '--game' in argv:
            game(check, exe, group, mod)
    print(f'\n{"ALL PASSED" if not failed else f"{failed} FAILED"}  ({time.time() - t:.0f} s)')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
