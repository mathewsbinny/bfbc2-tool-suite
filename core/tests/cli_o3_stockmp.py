r"""Round O3's stockmp group: the command lines test_cli_o3.py replays for bfbc2-stockmp-ai\stockmp.py.

cases(): the quick tier's (synthetic inputs made in the case folder: a fake Project OPFOR folder, built outputs of
made-up bytes with their build_report.json, a game folder with installed files, markers and backups; no game data).
Also summary and generate --minimal on synthetic levels (test_stockmp's DBX builders, bundles made with
bfbc2core.fbrb.build, path data from cli_o2_nav), and the hand-offs (the full generate, build, validate, all).
opfor_calls(opfor): the --game tier's (Project OPFOR's real builds in work\stockmp, copied to the case folder; the
game copy read in place by `install --plan`, `summary` and `generate --minimal`, every install / revert into a game
folder in the case folder, every unpack and configuration into the case folder's work folder).
"""
from __future__ import annotations

import hashlib
import json
import os
from typing import Any, cast

Case = tuple[str, str, dict[str, Any], list[str], dict[str, str], bool]
Call = tuple[str, str, list[str], dict[str, Any], dict[str, str], bool]

VARIANTS = {'PYTHONUTF8=1': {'PYTHONUTF8': '1'}, 'PYTHONUNBUFFERED=1': {'PYTHONUNBUFFERED': '1'},
            'PYTHONIOENCODING=utf-8': {'PYTHONIOENCODING': 'utf-8'}}

APPLIED = '.opfor-stockmp-applied'
BACKUP = '.opfor-stockmp-orig'
DIST = 'Dist\\win32\\levels\\{lv}\\level-00.fbrb'
PKG = 'Package\\levels\\{lv}\\level-00.fbrb'
PD = 'Output\\win32\\ai\\levels\\{lv}\\{kind}\\pathdata.hpd'


def with_variants(rows: list[Case], picked: list[str]) -> list[Case]:
    out = list(rows)
    out += [(k + ' (merged)', t, f, a, e, True) for k, t, f, a, e, _ in rows]
    for label, env in VARIANTS.items():
        out += [(f'{k} [{label}]', t, f, a, dict(e, **env), m) for k, t, f, a, e, m in list(out)
                if k.split(' (merged)')[0] in picked and '[' not in k]
    return out


def sha(b: bytes) -> str:
    return hashlib.sha256(b).hexdigest()


def built(lv: str = 'mp_005', root: str | None = None, extra: dict[str, bytes] | None = None,
          report: Any = 'ok', names: list[str] | None = None) -> dict[str, Any]:
    """work\\<root>\\out\\game\\... (made-up bytes) and its build_report.json: report 'ok' lists every output with its
    hash, None leaves the report out, a dict / str is written as given (str: raw text)."""
    root = root or lv
    rels = names if names is not None else [DIST.format(lv=lv), PKG.format(lv=lv), PD.format(lv=lv, kind='infantry'),
                                             PD.format(lv=lv, kind='vehicle')]
    files: dict[str, Any] = {'opfor\\game\\x.txt': b'game'}
    outputs = {}
    for i, rel in enumerate(rels):
        data = f'{rel} built {i}\n'.encode() * (3 + i)
        files[f'work\\{root}\\out\\game\\{rel}'] = data
        outputs['out\\game\\' + rel] = sha(data)
    for rel, data in (extra or {}).items():
        files[f'work\\{root}\\out\\game\\{rel}'] = data
    if report == 'ok':
        files[f'work\\{root}\\build_report.json'] = json.dumps({'lv': lv, 'outputs': outputs}, indent=1)
    elif isinstance(report, dict):
        files[f'work\\{root}\\build_report.json'] = json.dumps(report(outputs) if callable(report) else report)
    elif isinstance(report, str):
        files[f'work\\{root}\\build_report.json'] = report
    elif callable(report):
        files[f'work\\{root}\\build_report.json'] = json.dumps(report(outputs))
    return files


def installed(files: dict[str, Any], lv: str = 'mp_005', root: str | None = None, backups: tuple[str, ...] = (),
              changed: tuple[str, ...] = (), game: str = 'game') -> dict[str, Any]:
    """files plus a game folder (case folder\\<game>) where every built output is installed: the file, its marker,
    a backup of an original for the rels in backups, and rels in changed altered after the install."""
    out = dict(files)
    root = root or lv
    prefix = f'work\\{root}\\out\\game\\'
    for name, data in files.items():
        if not name.startswith(prefix):
            continue
        rel = name[len(prefix):]
        dst = f'{game}\\{rel}'
        out[dst] = data + b'changed' if rel in changed else data
        out[dst + APPLIED] = sha(data)
        if rel in backups:
            out[dst + BACKUP] = b'the original ' + rel.encode()
    return out


def args(lv: str, *more: str, game: str | None = '{d}\\game') -> list[str]:
    a = ['install', lv, *more, '--opfor', '{d}\\opfor', '--work', '{d}\\work']
    return a + (['--game', game] if game else [])


def install_cases() -> list[Case]:
    lv = 'mp_005'
    rows: list[Case] = []

    def add(key: str, files: dict[str, Any], argv: list[str], env: dict[str, str] | None = None) -> None:
        rows.append((key, 'stockmp', files, argv, env or {}, False))

    b = built()
    # --plan
    add('plan new', b, args(lv))
    add('plan default game (opfor\\game)', b, args(lv, game=None))
    add('plan explicit --plan', b, args(lv, '--plan'))
    add('plan replace', dict(b, **{f'game\\{DIST.format(lv=lv)}': b'stock'}), args(lv))
    add('plan upper-case level', b, args('MP_005'))
    add('plan variant', built(lv, 'mp_005-exp0'), args(lv, '--variant', 'exp0'))
    add('plan variant missing build', b, args(lv, '--variant', 'exp1'))
    add('plan empty variant', b, ['install', lv, '--variant', '', '--opfor', '{d}\\opfor', '--work', '{d}\\work',
                                  '--game', '{d}\\game'])
    add('plan nothing built', {'opfor\\game\\x.txt': b''}, args(lv))
    add('plan no report', built(report=None), args(lv))
    add('plan report missing a file', built(report=lambda o: {'outputs': dict(list(o.items())[:2])}), args(lv))
    add('plan report lists more', built(report=lambda o: {'outputs': dict(o, **{'out\\game\\gone.bin': '00'})}),
        args(lv))
    add('plan report hash differs', built(report=lambda o: {'outputs': {k: '0' * 64 for k in o}}), args(lv))
    add('plan report forward slashes', built(report=lambda o: {'outputs': {k.replace('\\', '/'): v
                                                                           for k, v in o.items()}}), args(lv))
    add('plan report upper case', built(report=lambda o: {'outputs': {k.upper(): v for k, v in o.items()}}), args(lv))
    add('plan report outputs null', built(report=lambda o: {'outputs': None}), args(lv))
    add('plan report no outputs', built(report=lambda o: {}), args(lv))
    add('plan report many problems', built(extra={f'x{i}.bin': b'x' for i in range(9)}), args(lv))
    add('plan report not json', built(report='{not json'), args(lv))
    add('plan report a list', built(report='[1, 2]'), args(lv))
    add('plan many files', built(names=[f'Dist\\win32\\levels\\{lv}\\f{i:04d}.bin' for i in range(400)]), args(lv))
    add('plan non-ascii names', built(names=[f'Dist\\win32\\levels\\{lv}\\café.bin',
                                             f'Dist\\win32\\levels\\{lv}\\ж.bin']), args(lv))
    add('plan cp1252 name', built(names=[f'Dist\\win32\\levels\\{lv}\\café.bin']), args(lv))
    add('plan installed', installed(b), args(lv))
    # --apply
    add('apply new', b, args(lv, '--apply'))
    add('apply over originals', dict(b, **{f'game\\{DIST.format(lv=lv)}': b'stock dist',
                                           f'game\\{PKG.format(lv=lv)}': b'stock pkg'}), args(lv, '--apply'))
    add('apply again (already)', installed(b), args(lv, '--apply'))
    add('apply incomplete build', built(report=None), args(lv, '--apply'))
    add('apply changed after install', installed(b, changed=(PKG.format(lv=lv),)), args(lv, '--apply'))
    add('apply nothing built', {'opfor\\game\\x.txt': b''}, args(lv, '--apply'))
    add('apply variant', built(lv, 'mp_005-exp0'), args(lv, '--variant', 'exp0', '--apply'))
    # an earlier build installed: the new build's files differ from the installed ones
    nb = built()
    for k in list(nb):
        if k.endswith('.fbrb'):
            nb[k] = nb[k] + b'v2'
    nb['work\\mp_005\\build_report.json'] = json.dumps({'outputs': {
        k[len('work\\mp_005\\'):]: sha(v) for k, v in nb.items() if k.startswith('work\\mp_005\\out\\')}})
    old = installed(b, backups=(DIST.format(lv=lv),))
    add('apply a new build over the installed one',
        dict(nb, **{k: v for k, v in old.items() if k.startswith('game\\')}), args(lv, '--apply'))
    # --revert
    add('revert installed', installed(b), args(lv, '--revert'))
    add('revert with backups', installed(b, backups=(DIST.format(lv=lv), PKG.format(lv=lv))), args(lv, '--revert'))
    add('revert nothing', {'opfor\\game\\x.txt': b''}, args(lv, '--revert'))
    add('revert built, not installed', b, args(lv, '--revert'))
    add('revert changed', installed(b, changed=(DIST.format(lv=lv),)), args(lv, '--revert'))
    # only the game's markers (the build output deleted): the installation record
    inst = installed(b, backups=(PKG.format(lv=lv),))
    add('revert from markers only', {k: v for k, v in inst.items() if not k.startswith('work\\')}, args(lv, '--revert'))
    other = installed(built('mp_002'), 'mp_002')
    add('revert other level installed', dict(other, **{'opfor\\game\\x.txt': b''}), args(lv, '--revert'))
    add('revert marker without file', dict({k: v for k, v in inst.items() if not k.startswith('work\\')
                                            and not k.endswith('pathdata.hpd')}), args(lv, '--revert'))
    add('both --apply and --revert', b, args(lv, '--apply', '--revert'))
    return rows


def usage_cases() -> list[Case]:
    rows: list[Case] = []
    for key, argv in (('no arguments', []), ('only a command', ['install']), ('unknown command', ['frobnicate', 'mp_005']),
                      ('help', ['--help', 'x']), ('empty argument', ['', 'mp_005'])):
        rows.append((f'usage {key}', 'stockmp', {}, argv, {}, False))
    # the commands the Python answers (here: a fake folder without path data or bundles): handed off, same answers
    for key, argv in (('generate no path data', ['generate', 'mp_005']), ('summary no bundle', ['summary', 'mp_005']),
                      ('validate no config', ['validate', 'mp_005']), ('build no config', ['build', 'mp_005'])):
        rows.append((f'handed {key}', 'stockmp', {'opfor\\game\\x.txt': b''},
                     argv + ['--opfor', '{d}\\opfor', '--work', '{d}\\work'], {}, False))
    return rows


# ------------------------------------------------------------------------------------------- summary, generate

def _synthetic() -> Any:
    """bfbc2-stockmp-ai\\tests\\test_stockmp.py's synthetic DBX builders (no game data)."""
    import importlib
    import sys
    here = os.path.dirname(os.path.abspath(__file__))
    tests = os.path.join(os.path.dirname(os.path.dirname(here)), 'bfbc2-stockmp-ai', 'tests')
    if tests not in sys.path:
        sys.path.insert(0, tests)
    return importlib.import_module('test_stockmp')


def _bundle(entries: list[tuple[str, str, bytes]]) -> bytes:
    import sys
    suite = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
    if suite not in sys.path:
        sys.path.insert(0, suite)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import fbrb
    return bytes(fbrb.build(entries))


def _hpd(seed: int, sectors: tuple[int, int] = (1, 3), nodes: int = 10) -> bytes:
    import cli_o2_nav
    return cast(bytes, cli_o2_nav._hpd_file(seed, sectors, nodes))


def _kits(ts: Any, missing: str | None = None, two: str | None = None) -> bytes:
    """mp_common's level-00 bundle with the eight kit partitions (one SoldierAsset each)."""
    entries = [('Levels/MP_Common.dbx', '<non-resource>', b'{binary}')]
    n = 0
    for fac in ('US', 'RU'):
        for kit in ('Assault', 'Demolition', 'Support', 'Recon'):
            n += 1
            if f'{fac}/{kit}' == missing:
                continue
            b = ts.B()
            insts = [b.inst(ts.G(700 + n), 'GameSharedResources.SoldierAsset', [b.text('Name', f'{fac} {kit}')])]
            if f'{fac}/{kit}' == two:
                insts.append(b.inst(ts.G(750 + n), 'GameSharedResources.SoldierAsset', [b.text('Name', 'again')]))
            insts.append(b.inst(ts.G(780 + n), 'GameSharedResources.SoldierKit', [b.text('Name', 'kit')]))
            entries.append((f'Objects/Soldiers/{fac}/{kit}.dbx', '<non-resource>', b.done(ts.G(800 + n), insts)))
    return _bundle(entries)


def level_cases() -> list[Case]:
    ts = _synthetic()
    rows: list[Case] = []
    conquest, rush = ts.synthetic_level('conquest'), ts.synthetic_level('rush')

    def level(data: bytes, name: str = 'MP_005') -> bytes:
        return _bundle([(f'Levels/{name}.dbx', '<non-resource>', data),
                        (f'Levels/{name}/Precomputed/x.res', 'mesh', b'mesh bytes')])

    def game(lv: str = 'mp_005', data: bytes = conquest, name: str = 'MP_005', pkg: bool = False,
             pathdata: str | None = 'own', kits: bytes | None = None) -> dict[str, Any]:
        files: dict[str, Any] = {f'opfor\\game\\Dist\\win32\\levels\\{lv}\\level-00.fbrb': level(data, name)}
        if pkg:
            files[f'opfor\\game\\Package\\levels\\{lv}\\level-00.fbrb'] = level(data, name)
        folder = lv if pathdata == 'own' else pathdata
        if folder:
            files[f'opfor\\game\\Output\\win32\\ai\\levels\\{folder}\\infantry\\pathdata.hpd'] = _hpd(1)
            files[f'opfor\\game\\Output\\win32\\ai\\levels\\{folder}\\vehicle\\pathdata.hpd'] = _hpd(2)
        files['opfor\\game\\Dist\\win32\\levels\\mp_common\\level-00.fbrb'] = kits if kits is not None else _kits(ts)
        return files

    def add(key: str, files: dict[str, Any], argv: list[str]) -> None:
        rows.append((key, 'stockmp', files, argv + ['--opfor', '{d}\\opfor', '--work', '{d}\\work'], {}, False))

    g = game()
    dist = 'opfor\\game\\Dist\\win32\\levels\\mp_005\\level-00.fbrb'
    hpd_inf = 'opfor\\game\\Output\\win32\\ai\\levels\\mp_005\\infantry\\pathdata.hpd'
    # summary
    add('summary conquest', g, ['summary', 'mp_005'])
    add('summary upper-case level', g, ['summary', 'MP_005'])
    add('summary rush', game('mp_002', rush, 'MP_002'), ['summary', 'mp_002'])
    add('summary dist and package', game(pkg=True), ['summary', 'mp_005'])
    add('summary variant', g, ['summary', 'mp_005', '--variant', 'exp0'])
    bundle = g[dist]
    src = 'work\\mp_005\\src\\dist\\'
    cached = dict(g, **{src + 'fbrb.manifest': 'Levels/MP_005.dbx|<non-resource>|00010000|1|1\n',
                        src + 'Levels\\MP_005.dbx': rush, src + '.source.sha256': sha(bundle)})
    add('summary cached unpack (stamp matches)', cached, ['summary', 'mp_005'])
    add('summary stale unpack (stamp differs)', dict(cached, **{src + '.source.sha256': '0' * 64,
                                                                src + 'junk.txt': b'old'}), ['summary', 'mp_005'])
    add('summary stamp with spaces', dict(cached, **{src + '.source.sha256': ' ' + sha(bundle) + '\n'}),
        ['summary', 'mp_005'])
    add('summary installed (backup read)', dict(g, **{dist: b'authored bundle', dist + BACKUP: bundle}),
        ['summary', 'mp_005'])
    add('summary added by the install (no bundle left)', dict(g, **{dist + APPLIED: sha(bundle)}), ['summary', 'mp_005'])
    add('summary no bundle', {'opfor\\game\\x.txt': b''}, ['summary', 'mp_005'])
    add('summary other level name in the bundle', game(name='MP_006'), ['summary', 'mp_005'])
    add('summary damaged bundle', dict(g, **{dist: b'FbRB junk'}), ['summary', 'mp_005'])
    add('summary level partition not DBX', game(data=b'not a partition at all'), ['summary', 'mp_005'])
    add('summary deleted entry', dict(cached, **{src + 'fbrb.manifest': 'Levels/MP_005.dbx|*deleted*|00010000|1|1\n'}),
        ['summary', 'mp_005'])
    no_file = {k: v for k, v in cached.items() if k != src + 'Levels\\MP_005.dbx'}
    add('summary entry missing from the unpack', no_file, ['summary', 'mp_005'])
    # generate --minimal (the minimal layer: no snapping), the path data's lookup, the hand-offs
    add('generate minimal', g, ['generate', 'mp_005', '--minimal'])
    add('generate minimal variant', g, ['generate', 'mp_005', '--minimal', '--variant', 'exp0'])
    add('generate minimal rush', game('mp_002', rush, 'MP_002', pathdata='ons_mp_002'),
        ['generate', 'mp_002', '--minimal'])
    add('generate minimal same world', game(pathdata='ons_mp_005gr'), ['generate', 'mp_005', '--minimal'])
    add('generate minimal --pathdata', game(pathdata='mine'), ['generate', 'mp_005', '--minimal', '--pathdata', 'mine'])
    add('generate minimal --pathdata missing', g, ['generate', 'mp_005', '--minimal', '--pathdata', 'nowhere'])
    add('generate minimal --bots 4', g, ['generate', 'mp_005', '--minimal', '--bots', '4'])
    add('generate minimal --bots x', g, ['generate', 'mp_005', '--minimal', '--bots', 'x'])
    add('generate minimal vehicle missing', {k: v for k, v in g.items() if 'vehicle' not in k},
        ['generate', 'mp_005', '--minimal'])
    add('generate minimal path data installed (backup)', dict(g, **{hpd_inf + BACKUP: _hpd(4)}),
        ['generate', 'mp_005', '--minimal'])
    add('generate minimal path data with an infinite position', dict(g, **{hpd_inf: _hpd(3)}),
        ['generate', 'mp_005', '--minimal'])
    add('generate minimal no mp_common', {k: v for k, v in g.items() if 'mp_common' not in k},
        ['generate', 'mp_005', '--minimal'])
    add('generate minimal a kit missing', game(kits=_kits(ts, missing='RU/Support')), ['generate', 'mp_005', '--minimal'])
    add('generate minimal two soldier assets', game(kits=_kits(ts, two='US/Recon')), ['generate', 'mp_005', '--minimal'])
    common = 'opfor\\work\\navkit\\common\\mp_common\\'
    add('generate minimal NavKit common unpack older', dict(g, **{common + 'fbrb.manifest': ('x\n', 1e9)}),
        ['generate', 'mp_005', '--minimal'])
    add('generate minimal NavKit common unpack newer (damaged kit)',
        dict(g, **{common + 'fbrb.manifest': 'x\n', common + 'Objects\\Soldiers\\US\\Assault.dbx': b'junk'}),
        ['generate', 'mp_005', '--minimal'])
    add('generate minimal damaged path data', dict(g, **{hpd_inf: b'junk' * 40}), ['generate', 'mp_005', '--minimal'])
    add('generate minimal empty path data', dict(g, **{hpd_inf: _hpd(4, (0, 0))}), ['generate', 'mp_005', '--minimal'])
    add('generate minimal no level', {k: v for k, v in g.items() if k != dist}, ['generate', 'mp_005', '--minimal'])
    add('generate full (snapped: the Python)', g, ['generate', 'mp_005'])
    add('generate full no path data', {k: v for k, v in g.items() if 'pathdata' not in k}, ['generate', 'mp_005'])
    add('all no config no path data', {k: v for k, v in g.items() if 'pathdata' not in k}, ['all', 'mp_005'])
    return rows


def cases() -> list[Case]:
    picked = ['usage no arguments', 'plan new', 'plan non-ascii names', 'plan cp1252 name', 'plan many files',
              'apply new', 'revert installed', 'apply changed after install', 'summary conquest', 'generate minimal']
    return with_variants(usage_cases() + install_cases() + level_cases(), picked)


# ------------------------------------------------------------------------------------------- the game (--game)

def _tree(top: str, prefix: str) -> dict[str, str]:
    out = {}
    for d, _ds, fs in os.walk(top):
        for f in fs:
            p = os.path.join(d, f)
            out[prefix + os.path.relpath(p, top)] = p
    return out


def opfor_calls(opfor: str) -> list[Call]:
    """OPFOR's documented stock-MP calls (README, STOCK_MP_AI_PLAN.md: `stockmp.py install mp_005 --variant exp0`,
    --plan by default, then --apply / --revert), on its real builds: out\\game and build_report.json copied to the case
    folder; --plan reads the real game copy (read-only); --apply / --revert write a game folder in the case folder."""
    work = os.path.join(opfor, 'work', 'stockmp')
    out: list[Call] = []
    # summary and generate --minimal (the exp0 variant's layer) on the real game copy, read in place; the unpacks and
    # the configuration go to the case folder's work folder (--work)
    real = ['--opfor', opfor, '--work', '{d}\\work']
    for lv in ('mp_005', 'mp_005sr', 'mp_002', 'mp_004sdm', 'mp_008', 'mp_009gr'):
        if os.path.isdir(os.path.join(opfor, 'game', 'Dist', 'win32', 'levels', lv)):
            out.append((f'summary {lv} (the real game)', 'stockmp', ['summary', lv, *real], {}, {}, True))
    for lv, more, must in (('mp_005', [], True), ('mp_005', ['--variant', 'exp0'], True), ('mp_002', [], True),
                           ('mp_004', [], True), ('mp_008', ['--bots', '6'], True), ('mp_009gr', [], False)):
        if os.path.isdir(os.path.join(opfor, 'game', 'Dist', 'win32', 'levels', lv)):
            out.append((f'generate {lv} --minimal {" ".join(more)} (the real game)', 'stockmp',
                        ['generate', lv, '--minimal', *more, *real], {}, {}, must))
    if not os.path.isdir(work):
        return out
    for root in sorted(os.listdir(work)):
        rep = os.path.join(work, root, 'build_report.json')
        if not os.path.isfile(rep):
            continue
        lv, _, variant = root.partition('-')
        files: dict[str, Any] = _tree(os.path.join(work, root, 'out'), f'work\\{root}\\out\\')
        files[f'work\\{root}\\build_report.json'] = rep
        var = ['--variant', variant] if variant else []
        base = ['install', lv, *var, '--opfor', opfor, '--work', '{d}\\work']
        out.append((f'install {root} --plan (the real game)', 'stockmp', base, files, {}, True))
        out.append((f'install {root} --plan --game (empty)', 'stockmp', base + ['--game', '{d}\\game'], files, {}, True))
        out.append((f'install {root} --apply (a game folder in the case folder)', 'stockmp',
                    base + ['--game', '{d}\\game', '--apply'], files, {}, True))
        # the state --apply leaves (every built file installed with its marker), reverted
        inst = dict(files)
        for name, src in files.items():
            pre = f'work\\{root}\\out\\game\\'
            if name.startswith(pre):
                with open(src, 'rb') as f:
                    data = f.read()
                inst[f'game\\{name[len(pre):]}'] = (data, os.path.getmtime(src))
                inst[f'game\\{name[len(pre):]}{APPLIED}'] = (sha(data).encode(), os.path.getmtime(src))
        out.append((f'install {root} --revert (that game folder)', 'stockmp',
                    base + ['--game', '{d}\\game', '--revert'], inst, {}, True))
        out.append((f'install {root} --apply again (already)', 'stockmp',
                    base + ['--game', '{d}\\game', '--apply'], inst, {}, True))
    return out
