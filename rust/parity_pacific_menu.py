r"""Round OR14's parity check of Project OPFOR's Pacific War menu edits and texts moved off aptgraft.py's and
bc2loc.py's Python API onto the suite's Rust core (bc2_menu::pacific, bc2_menu::loc: bc2_native.pacific_main_nav,
pacific_main_root, apt_dump_text, LocFile, loc_languages, loc_load).

    python rust\parity_pacific_menu.py [--opfor <OPFOR checkout with game\ and work\>] [--old <commit>]
                                       [--new <src\tools>] [--cases N] [--seed S] [--keep] [--any-module]

1. The movie edits: menu_install.py's own Python at --old (default 0e4d0d3, read with `git show` from --opfor: its
   code helpers, edit_main_nav and edit_main_root, run on this checkout's aptgraft.py) against the Rust, on the
   movies the menu step edits (work\ww2\menu_install\orig: main_nav and main_root as build_menus.bat leaves them;
   the bundle the stage builds from, read through the install ledger), at the sizes the step passes and --cases
   random label / logo sizes, and on damaged movies (cut short; a selector, background or logo frame missing or
   placed; setGameMod changed; a character of another type; getGModStringExtensino missing) made with aptgraft and
   written: the same bytes, image ids and report lines, or the same AptError words.
2. The whole step, old against new, each in a process of its own: menu_install.build (unpack the source bundle,
   the edits, the atlas, aptcheck, the dump diff, pack) into a scratch folder with WORK there: every file the same
   (the packed bundle too) and every printed line.
3. The texts: bc2loc.LocFile against bc2_native.LocFile on every language's file in the game copy (every key's
   lookup and text, names.py's texts set and written, the bytes read back), BF1943-style files without a histogram,
   damaged files (cut short, magic changed, header sizes changed) and --cases random keys and texts (non-ASCII and
   NUL refused); bc2loc.languages / load against loc_languages / loc_load.

Skips cleanly (exit 0) without the OPFOR checkout or its game copy, part by part. Exits 1 on a difference. Nothing
is written to game\ or work\ (outputs go to a scratch folder, removed unless --keep).
"""
from __future__ import annotations

import argparse
import contextlib
import io
import os
import random
import shutil
import struct
import subprocess
import sys
import tempfile
import time
import types
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
OLD_COMMIT = '0e4d0d3'
VERBOSE = False


def any_module(core: str) -> None:
    """--any-module: core's bc2_native taken even when its fingerprint is another source state's."""
    sys.path.insert(0, core)
    import importlib

    from bfbc2core import native
    nat = importlib.import_module('bfbc2core.bc2_native')
    native._state.clear()
    native._state.update(module=nat, status='ok', where=f'{core} (--any-module)')


def modules(any_mod: bool) -> tuple[Any, Any, Any]:
    """(aptgraft, bc2loc, bc2_native) of this checkout."""
    if any_mod:
        any_module(os.path.join(SUITE, 'core'))
    sys.path[:0] = [os.path.join(SUITE, 'core'), os.path.join(SUITE, 'bfbc2-menu-tools')]
    from bfbc2core import native
    nat = native.require('the Pacific War menu parity', 'pacific_main_nav')
    import aptgraft
    import bc2loc
    return aptgraft, bc2loc, nat


def outcome(fn: Any, *a: Any, **k: Any) -> tuple[str, Any]:
    try:
        return 'ok', fn(*a, **k)
    except Exception as e:  # noqa: BLE001  (the error is the result compared)
        name = type(e).__name__
        if name == 'AptError':
            return 'AptError', str(e)
        if name in ('MenuToolError', 'ValueError') and not str(e).endswith('exception of its own)'):
            return 'refused', str(e)
        return 'error', name


class Tally:
    def __init__(self, what: str) -> None:
        self.what, self.same, self.total, self.refused = what, 0, 0, 0
        self.notes: list[str] = []

    def add(self, name: str, a: tuple[str, Any], b: tuple[str, Any]) -> None:
        self.total += 1
        ok = a == b or (a[0] == 'error' and b[0] == 'error')
        if a[0] != 'ok':
            self.refused += 1
            if VERBOSE:
                print(f'    refused: {name}: python {a[0]} {str(a[1])[:120]} | rust {b[0]} {str(b[1])[:120]}')
        if ok:
            self.same += 1
        else:
            self.notes.append(f'    {name}: python {str(a)[:300]} | rust {str(b)[:300]}')

    def line(self) -> str:
        return (f'{self.what}: {self.same} of {self.total} the same ({self.refused} refused by both)'
                + ('' if self.same == self.total else '\n' + '\n'.join(self.notes[:20])))


# ------------------------------------------------------------------------------------------------ 1. the edits

def old_edits(opfor: str, commit: str, A: Any) -> types.SimpleNamespace:
    """menu_install.py's movie edits at commit (its constants, code helpers, edit_main_nav, edit_main_root) on A."""
    src = subprocess.run(['git', '-C', opfor, 'show', f'{commit}:src/tools/ww2/menu_install.py'], capture_output=True,
                         check=True).stdout.decode('utf-8').replace('\r\n', '\n')
    consts = src[src.index("LABEL = 'BFBC2143'"):src.index('def log(')]
    funcs = src[src.index('def clone(i):'):src.index('def texts_present():')]
    ns: dict[str, Any] = {'A': A, 'Ins': A.Ins, 'Ref': A.Ref, 'Str': A.Str, 'Words': A.Words, 'struct': struct}
    exec(compile(consts + '\n' + funcs, f'menu_install.py@{commit}', 'exec'), ns)  # noqa: S102  (the reference)
    return types.SimpleNamespace(**ns)


def py_nav(A: Any, old: Any, path: str, label: tuple[int, int], logo: tuple[int, int]) -> Any:
    m = A.Movie.load(path)
    report: list[str] = []
    ids = old.edit_main_nav(m, report, label, logo)
    return m.to_res(layout=A.GRAFT_LAYOUT), tuple(ids), report


def rs_nav(nat: Any, path: str, label: tuple[int, int], logo: tuple[int, int]) -> Any:
    res, ids, report = nat.pacific_main_nav(path, label, logo)
    return res, tuple(ids), report


def py_root(A: Any, old: Any, path: str) -> Any:
    m = A.Movie.load(path)
    report: list[str] = []
    old.edit_main_root(m, report)
    return m.to_res(layout=A.GRAFT_LAYOUT), report


def damaged_nav(A: Any, old: Any, base: str, out: str) -> list[tuple[str, str]]:
    """[(name, movie path)] of damaged main_nav movies made from base with aptgraft."""
    data = open(base, 'rb').read()
    made = []

    def save(name: str, m: Any) -> None:
        p = os.path.join(out, name + '.res')
        open(p, 'wb').write(m.to_res(layout=A.GRAFT_LAYOUT))
        made.append((name, p))
    for cut in (len(data) // 2, len(data) - 7, 100):
        p = os.path.join(out, f'cut{cut}.res')
        open(p, 'wb').write(data[:cut])
        made.append((f'cut{cut}', p))
    # already edited (the Pacific War frame there)
    m = A.Movie.load(base)
    old.edit_main_nav(m, [], (120, 22), (302, 113))
    save('edited_twice', m)
    # a character of another type
    m = A.Movie.load(base)
    m.char(old.BGBAR).vals[0] = 1
    save('bgbar_not_sprite', m)
    # gameModBackground's frame BFBC2143 places something
    m = A.Movie.load(base)
    bg = m.char(old.BACKGROUND)
    m.add_item(2, old.place_copy(old.places(m, bg, 0)[0]), bg)
    save('bg_frame_placed', m)
    # the selector's label moved to frame 1
    m = A.Movie.load(base)
    sel = m.char(old.SELECTOR)
    for it in m.frame_items(0, sel):
        if it.tag == 'label':
            it.vals[1] = A.Ref(A.Str('apt', None, b'Other'))
    save('selector_label_renamed', m)
    # loggo_mc's Nam frame places another character
    m = A.Movie.load(base)
    logo = m.char(old.LOGO)
    f = old.labels_of(m, logo)[old.NAM]
    old.places(m, logo, f)[0].vals[3] = 440
    save('logo_other_char', m)
    # bgBar's BFBC2143 frame label removed
    m = A.Movie.load(base)
    bar = m.char(old.BGBAR)
    for fr in range(len(m.frames(bar).vals) // 2):
        items = m.items(fr, bar)
        if items is not None:
            items.vals = [v for v in items.vals
                          if not (v.node.tag == 'label' and A.text(v.node.vals[1]) == old.LABEL)]
            m.frames(bar).vals[2 * fr] = len(items.vals)
    save('bgbar_no_label', m)
    # setGameMod renamed / its head changed / its hiding moved
    for name, change in (('setgamemod_renamed', 'rename'), ('setgamemod_head', 'head'), ('setgamemod_hide', 'hide')):
        m = A.Movie.load(base)
        for i, fname, _pool in old.functions(m):
            if fname == 'setGameMod':
                if change == 'rename':
                    i.args[0] = A.Ref(A.Str('apt', None, b'setGameModX'))
                elif change == 'head':
                    i.body.insert(0, A.Ins(0x17))
                else:
                    del i.body[22]
        save(name, m)
    return made


def damaged_root(A: Any, old: Any, base: str, out: str) -> list[tuple[str, str]]:
    made = []
    data = open(base, 'rb').read()
    p = os.path.join(out, 'root_cut.res')
    open(p, 'wb').write(data[:len(data) // 3])
    made.append(('root_cut', p))
    m = A.Movie.load(base)
    for i, _n, pool in old.functions(m):
        if i.op == 0x9B and pool is not None:
            for x in i.body:
                if x.op in (0xA2, 0xA3) and x.args[0] < len(pool.names) and pool.names[x.args[0]] == '_NAM':
                    x.op = 0x17
                    x.args = []
    p = os.path.join(out, 'root_no_nam.res')
    open(p, 'wb').write(m.to_res(layout=A.GRAFT_LAYOUT))
    made.append(('root_no_nam', p))
    m = A.Movie.load(base)
    old.edit_main_root(m, [])
    p = os.path.join(out, 'root_twice.res')
    open(p, 'wb').write(m.to_res(layout=A.GRAFT_LAYOUT))
    made.append(('root_twice', p))
    return made


def source_movies(opfor: str, scratch: str) -> list[tuple[str, str, str]]:
    """[(name, main_nav path, main_root path)]: work\\ww2\\menu_install\\orig, and the source bundle's movies."""
    out = []
    orig = os.path.join(opfor, 'work', 'ww2', 'menu_install', 'orig')
    if os.path.isfile(os.path.join(orig, 'main_nav.res')):
        out.append(('orig', os.path.join(orig, 'main_nav.res'), os.path.join(orig, 'main_root.res')))
    bundle = os.path.join(opfor, 'game', 'Package', 'menu', 'main', 'mainmenu-00.fbrb')
    tools = os.path.join(opfor, 'src', 'tools')
    if os.path.isfile(bundle):
        sys.path[:0] = [tools]
        try:
            from opforcore.stages import before_path
            src = before_path(bundle, 'pacificmenu')
        except Exception:  # noqa: BLE001  (no ledger: the bundle as it is)
            src = bundle
        from bfbc2core import fbrb
        got = fbrb.read(src, ['Menu/main_nav.res', 'Menu/main_root.res'])
        d = os.path.join(scratch, 'source')
        os.makedirs(d, exist_ok=True)
        for k, v in got.items():
            open(os.path.join(d, os.path.basename(k)), 'wb').write(v)
        out.append(('source bundle', os.path.join(d, 'main_nav.res'), os.path.join(d, 'main_root.res')))
    return out


def edits_part(args: argparse.Namespace, scratch: str) -> int:
    A, _loc, nat = modules(args.any_module)
    movies = source_movies(args.opfor, scratch)
    if not movies:
        print('edits: skipped (no menu_install orig movies or main menu bundle)')
        return 0
    old = old_edits(args.opfor, args.old, A)
    t = Tally('edits (main_nav, main_root)')
    tp = tr = 0.0
    rnd = random.Random(args.seed)
    sizes = [((120, 22), (302, 113)), ((107, 22), (302, 113))] + [
        ((rnd.randint(1, 400), rnd.randint(1, 60)), (rnd.randint(1, 600), rnd.randint(1, 300))) for _ in range(args.cases)]
    for name, nav, root in movies:
        for label, logo in sizes:
            t0 = time.perf_counter()
            a = outcome(py_nav, A, old, nav, label, logo)
            t1 = time.perf_counter()
            b = outcome(rs_nav, nat, nav, label, logo)
            tp, tr = tp + t1 - t0, tr + time.perf_counter() - t1
            t.add(f'{name} main_nav {label} {logo}', a, b)
        t.add(f'{name} main_root', outcome(py_root, A, old, root), outcome(lambda p: tuple(nat.pacific_main_root(p)), root))
        d = os.path.join(scratch, 'damaged', name.replace(' ', '_'))
        os.makedirs(d, exist_ok=True)
        for dn, p in damaged_nav(A, old, nav, d):
            t.add(f'{name} {dn}', outcome(py_nav, A, old, p, (120, 22), (302, 113)),
                  outcome(rs_nav, nat, p, (120, 22), (302, 113)))
        for dn, p in damaged_root(A, old, root, d):
            t.add(f'{name} {dn}', outcome(py_root, A, old, p),
                  outcome(lambda q: tuple(nat.pacific_main_root(q)), p))
        # the dumps the semantic diff compares
        for p in (nav, root):
            t.add(f'{name} dump {os.path.basename(p)}', outcome(lambda q: A.dump_text(A.load_members(q)[1]), p),
                  outcome(nat.apt_dump_text, p))
    print(t.line())
    print(f'    main_nav edits: Python {tp:.2f} s, Rust {tr:.2f} s')
    return int(t.same != t.total)


# ------------------------------------------------------------------------------------------------ 2. the step

def side_main(args: argparse.Namespace) -> int:
    """menu_install.build on one side (a process of its own) into args.out."""
    if args.any_module:
        any_module(os.path.join(SUITE, 'core'))
    tools = args.tools_dir
    sys.path[:0] = [os.path.join(tools, 'ww2'), os.path.join(tools, 'onslaught'), os.path.join(tools, 'menus'), tools]
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    import console_common as cc
    import menu_art as ma
    import menu_install as mi
    root = os.environ['OPFOR_ROOT']            # the game copy and work read there; everything written into out
    here = os.path.abspath(os.path.join(tools, '..', '..'))
    for mod in (cc, ma, mi):                   # every module path under this side's checkout: the game copy's
        for k, v in list(vars(mod).items()):
            if isinstance(v, str) and k.isupper() and os.path.normcase(v).startswith(os.path.normcase(here)):
                setattr(mod, k, root + v[len(here):])
    mi.WORK = os.path.join(out, 'work')
    os.makedirs(mi.WORK, exist_ok=True)
    from opforcore.stages import before_path  # the bundle from before the stage (the installed one holds its edits)
    setattr(cc, 'source_bundle', lambda path, own, game=None: before_path(path, own))  # noqa: B010
    ma.OUT = os.path.join(out, 'preview')
    ma.DICE = os.path.join(ma.OUT, 'dice')
    src_dice = os.path.join(root, 'work', 'ww2', 'preview', 'menu', 'dice')
    if os.path.isdir(src_dice):
        shutil.copytree(src_dice, ma.DICE, dirs_exist_ok=True)
    buf = io.StringIO()
    t0 = time.perf_counter()
    with contextlib.redirect_stdout(buf):
        packed, report = mi.build(os.path.join(out, 'plan', 'mainmenu'))
        for r in report:
            print(r)
    dt = time.perf_counter() - t0
    print('@@BEGIN')
    print(buf.getvalue(), end='')
    print('@@END')
    print(f'@@TIME {dt:.2f}')
    return 0


def files_of(root: str) -> dict[str, str]:
    out = {}
    for d, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(d, f)
            out[os.path.relpath(p, root).replace(os.sep, '/')] = p
    return out


def step_part(args: argparse.Namespace, scratch: str) -> int:
    opfor = args.opfor
    if not os.path.isfile(os.path.join(opfor, 'game', 'Package', 'menu', 'main', 'mainmenu-00.fbrb')):
        print('step: skipped (no main menu bundle)')
        return 0
    new_tools = args.new or os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2', 'src', 'tools')
    old_root = os.path.join(scratch, 'old_opfor')
    os.makedirs(old_root, exist_ok=True)
    arch = subprocess.run(['git', '-C', opfor, 'archive', '--format=tar', args.old, 'src/tools'], capture_output=True)
    if arch.returncode:
        print('step: skipped (git archive failed)')
        return 0
    subprocess.run(['tar', '-x', '-C', old_root], input=arch.stdout, check=True)
    res = {}
    for side, tools, toolkit in (('old', os.path.join(old_root, 'src', 'tools'), os.path.dirname(args.old_suite)),
                                 ('new', new_tools, os.path.dirname(SUITE))):
        out = os.path.join(scratch, 'step', side)
        env = dict(os.environ, OPFOR_TOOLKIT=toolkit, OPFOR_ROOT=opfor, PYTHONDONTWRITEBYTECODE='1',
                   PYTHONIOENCODING='utf-8')
        cmd = [sys.executable, os.path.abspath(__file__), '--side', '--tools-dir', tools, '--out', out] + (
            ['--any-module'] if args.any_module and side == 'new' else [])
        r = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8', errors='replace', env=env)
        if '@@BEGIN' not in r.stdout:
            print(f'step: {side} FAILED: {(r.stdout + r.stderr)[-1500:]}')
            return 1
        body = r.stdout.split('@@BEGIN\n', 1)[1].split('@@END', 1)[0]
        mark = os.sep + os.path.join('step', side) + os.sep
        lines = [line.replace(out, '<OUT>').replace(tools, '<TOOLS>').replace(mark, '<SIDE>')
                 for line in body.splitlines()]
        res[side] = (lines, float(r.stdout.split('@@TIME ', 1)[1].split()[0]), out)
    (ol, ot, oo), (nl, nt, no) = res['old'], res['new']
    fo, fn = files_of(oo), files_of(no)
    def content(path: str, side: str) -> bytes:
        b = open(path, 'rb').read()
        return b.replace(os.path.join(scratch, 'step', side).encode('utf-8'), b'<OUT>') if path.endswith('.txt') else b
    same = [k for k in fo if k in fn and content(fo[k], "old") == content(fn[k], "new")]
    diff = sorted(set(fo) ^ set(fn)) + [k for k in fo if k in fn and k not in same]
    print(f'step (menu_install.build): {len(same)} of {len(set(fo) | set(fn))} files identical, printed lines '
          f'{"identical" if ol == nl else "DIFFER"} ({len(ol)}); old {ot:.1f} s, new {nt:.1f} s')
    if ol != nl:
        for a, b in zip(ol, nl):
            if a != b:
                print(f'    old: {a}\n    new: {b}')
    if diff:
        print(f'    differ: {diff[:20]}')
    return int(bool(diff) or ol != nl)


# ------------------------------------------------------------------------------------------------ 3. the texts

def loc_case(L: Any, nat: Any, data: bytes, hist: bytes | None, keys: list[str],
             sets: list[tuple[str, str]]) -> tuple[tuple[str, Any], tuple[str, Any]]:
    def py() -> Any:
        f = L.LocFile(data, hist, 'f')
        got = [f.lookup(k) for k in keys]
        texts = [f.text(h) for h in list(f.order)[:200]]
        for k, t in sets:
            f.set(k, t)
        b = f.to_bytes()
        back = L.LocFile(b, None)
        return got, texts, b, [back.raw.get(L.khash(k)) for k, _ in sets]

    def rs() -> Any:
        f = nat.LocFile(data, hist, 'f')
        got = [f.lookup(k) for k in keys]
        texts = [f.text(h) for h in f.order()[:200]]
        for k, t in sets:
            f.set(k, t)
        b = f.to_bytes()
        back = nat.LocFile(b, None)
        return got, texts, b, [back.raw_get(nat.loc_khash(k)) for k, _ in sets]
    return outcome(py), outcome(rs)


def texts_part(args: argparse.Namespace, scratch: str) -> int:
    _A, L, nat = modules(args.any_module)
    opfor = args.opfor
    if not os.path.isdir(os.path.join(opfor, 'game')):
        print('texts: skipped (no game copy)')
        return 0
    t = Tally('texts (LocFile, languages, load)')
    langs_py = L.languages(opfor)
    langs_rs = dict(nat.loc_languages(opfor))
    t.add('languages', ('ok', dict(langs_py)), ('ok', langs_rs))
    rnd = random.Random(args.seed)
    tools = os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2', 'src', 'tools', 'ww2')
    pairs: list[tuple[str, str]] = []
    if os.path.isdir(tools):
        sys.path[:0] = [tools]
        with contextlib.suppress(Exception):
            import names
            pairs = list(names.pairs())
    for lang in sorted(langs_py):
        a = outcome(lambda lg: L.load(lg, opfor), lang)
        b = outcome(lambda lg: nat.loc_load(lg, opfor), lang)
        t.add(f'load {lang}', (a[0], None), (b[0], None))
        from bfbc2core import fbrb
        names_ = [f'Localization/bin/language_{lang}_normal.bin', f'Localization/bin/language_{lang}_histogram.bin']
        have = {e.name.lower(): e.name for e in fbrb.entries(langs_py[lang])}
        got = {k.lower(): v for k, v in fbrb.read(langs_py[lang], [have[n.lower()] for n in names_ if n.lower() in have]).items()}
        data, hist = got[names_[0].lower()], got.get(names_[1].lower())
        keys = [k for k, _ in pairs] + ['MP_LOADING_TEAM_PW_CQ_1', 'NAME_M24', 'ID_M_NOTHING', '0x1234', '0xDEADBEEF', '']
        keys += [''.join(rnd.choice('ABCDEFGHIJKLMNOPQRSTUVWXYZ_0123456789') for _ in range(rnd.randint(1, 20)))
                 for _ in range(args.cases)]
        a, b = loc_case(L, nat, data, hist, keys, pairs)
        t.add(f'{lang} names.py texts', a, b)
        a, b = loc_case(L, nat, data, None, keys[:50], [])
        t.add(f'{lang} without histogram', a, b)
        for cut in (10, 35, len(data) // 2, len(data) - 1):
            a, b = loc_case(L, nat, data[:cut], hist, keys[:10], [])
            t.add(f'{lang} cut {cut}', a, b)
        bad = bytearray(data)
        bad[0] ^= 1
        t.add(f'{lang} magic', *loc_case(L, nat, bytes(bad), hist, [], []))
        bad = bytearray(data)
        struct.pack_into('<I', bad, 8, struct.unpack_from('<I', bad, 8)[0] + 1000)
        t.add(f'{lang} count', *loc_case(L, nat, bytes(bad), hist, [], []))
        if hist:
            hb = bytearray(hist)
            hb[0] ^= 1
            t.add(f'{lang} histogram magic', *loc_case(L, nat, data, bytes(hb), keys[:5], []))
        for _ in range(args.cases // 10 + 1):
            k = rnd.choice(keys[:-1] or ['X'])
            text = ''.join(chr(rnd.choice([rnd.randint(32, 126)] * 8 + [0, 0xE9, 0x20AC])) for _ in range(rnd.randint(0, 30)))
            t.add(f'{lang} set {k!r} {text!r}', *loc_case(L, nat, data, hist, [k], [(k, text)]))
    print(t.line())
    return int(t.same != t.total)


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', default=os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2'))
    ap.add_argument('--old', default=OLD_COMMIT)
    ap.add_argument('--old-suite', default=None)
    ap.add_argument('--new', default=None)
    ap.add_argument('--cases', type=int, default=40)
    ap.add_argument('--seed', type=int, default=14)
    ap.add_argument('--keep', action='store_true')
    ap.add_argument('--verbose', action='store_true')
    ap.add_argument('--any-module', action='store_true')
    ap.add_argument('--parts', default='edits,step,texts')
    ap.add_argument('--side', action='store_true', help=argparse.SUPPRESS)
    ap.add_argument('--tools-dir', help=argparse.SUPPRESS)
    ap.add_argument('--out', help=argparse.SUPPRESS)
    args = ap.parse_args(argv)
    if args.side:
        return side_main(args)
    global VERBOSE
    VERBOSE = args.verbose
    if not os.path.isdir(os.path.join(args.opfor, 'src', 'tools')):
        print(f'skipped: no OPFOR checkout at {args.opfor}')
        return 0
    if args.old_suite is None:
        args.old_suite = os.path.join(os.path.dirname(os.path.abspath(args.opfor)), 'bfbc2-tool-suite')
    scratch = tempfile.mkdtemp(prefix='parity-pacific-')
    bad = 0
    try:
        parts = args.parts.split(',')
        if 'edits' in parts:
            bad += edits_part(args, scratch)
        if 'texts' in parts:
            bad += texts_part(args, scratch)
        if 'step' in parts:
            bad += step_part(args, scratch)
    finally:
        if args.keep:
            print(f'kept: {scratch}')
        else:
            shutil.rmtree(scratch, ignore_errors=True)
    print('parity: ' + ('ok' if not bad else f'{bad} part(s) DIFFER'))
    return int(bool(bad))


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
