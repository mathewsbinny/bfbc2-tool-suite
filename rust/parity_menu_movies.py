r"""Round OR14's parity check of Project OPFOR's movie tools moved off aptgraft.py's Python onto bc2_native.AptMovie
(bc2_menu::graft: add_icon_frame, split_image_part / add_crosshair's image tiles, the constant swap, the queries).

    python rust\parity_menu_movies.py [--opfor <OPFOR checkout with game\>] [--old <commit>] [--new <src\tools>]
                                      [--old-suite <suite>] [--tools t1,t2] [--cases N] [--seed S] [--keep]

1. Tools, old against new, each in a process of its own writing into a scratch folder (nothing goes into game\ or
   work\; the game copy and work\ww2\pictures are read): the old Python is --opfor's src\tools at --old (default
   0e4d0d3, taken with `git archive` into the scratch folder) on --old-suite's aptgraft and Rust core (default the
   suite checkout beside --opfor); the new is --new (default the OPFOR checkout beside this suite) on this suite's
   core. Tools: type95_icon (both common bundle pairs: movie, atlas, atlas_meta, sheets), weapon_icons (both kinds:
   movie, atlas, sheet; BF1943's HUD pictures through bf1943_hud_icons), duplicates (the weaponicon label aliases),
   wake_ui (every team-name movie, the combat-area texture, ww2_crosshairs' movie, atlas, sheet and reported
   numbers), bf1943_hud_icons (the kill-feed and vehicle-HUD pictures), qa_crosshairs (qa_checks' crosshairs check
   on the built and the stock movie), console_icons (ons_mp_002's Mark V on both bundles; only when the new side has
   moved it). Every written file must be byte for byte the same and every printed line the same (the scratch
   folders and checkouts named alike).
2. Damaged inputs (game copy and BF1943 needed): a movie cut short, a missing label, a shape that is not one quad, a
   label already there, a crosshair id already present or unknown, tiles that miss every part, an image without
   tiles - this checkout's aptgraft.py against bc2_native.AptMovie: the same AptError words or the same bytes.
3. Random cases (no game data): --cases synthetic movies (sprites, labels, quad and non-quad shapes, clip actions,
   label hiding) through add_icon_frame / add_label_alias and --cases random parts through split_image_part, this
   checkout's aptgraft.py (pure Python) against the Rust.

Skips cleanly (exit 0) without the OPFOR checkout, its game copy or BF1943's files, part by part. Exits 1 on a
difference. Needs this checkout's Rust core built (`python rust\build_native.py`).
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
TOOLS = ('type95_icon', 'weapon_icons', 'duplicates', 'wake_ui', 'bf1943_hud_icons', 'qa_crosshairs',
         'console_icons')


# ------------------------------------------------------------------------------------------------ 1. the side runner

def any_module(core: str) -> None:
    """--any-module: core's bfbc2core.bc2_native taken even when its fingerprint is another checkout state's (a round
    whose other work keeps changing the sources the module was built from)."""
    sys.path.insert(0, core)
    import importlib

    from bfbc2core import native
    nat = importlib.import_module('bfbc2core.bc2_native')
    native._state.clear()
    native._state.update(module=nat, status='ok', where=f'{core} (--any-module)')


def side_main(args: argparse.Namespace) -> int:
    """One tool on one side (a process of its own): the tool's outputs under args.out, its printed lines on stdout
    between markers, its time on a @@TIME line."""
    tools = args.tools_dir
    if args.any_module:
        any_module(os.path.join(SUITE, 'core'))
    sys.path[:0] = [os.path.join(tools, 'ww2'), os.path.join(tools, 'onslaught'), tools]
    game_opfor = args.game_opfor
    out = os.path.abspath(args.out)
    os.makedirs(out, exist_ok=True)
    import attachments as at
    at.OPFOR = game_opfor                      # the game copy and work\ww2\pictures are read there; outputs patched
    at.PREVIEW = os.path.join(out, 'preview')
    buf = io.StringIO()
    t0 = time.perf_counter()
    with contextlib.redirect_stdout(buf):
        run_tool(args.tool, out, game_opfor)
    dt = time.perf_counter() - t0
    print('@@BEGIN')
    print(buf.getvalue(), end='')
    print('@@END')
    print(f'@@TIME {dt:.2f}')
    return 0


def run_tool(tool: str, out: str, game_opfor: str) -> None:
    game = os.path.join(game_opfor, 'game')
    if tool == 'type95_icon':
        import type95_icon as t95
        t95.OUT = os.path.join(out, 'type95_icon')
        t95.PREVIEW = os.path.join(out, 'preview', 'type95_icon')
        t95.main()
    elif tool == 'weapon_icons':
        import weapon_icons as wi
        wi.OUT = os.path.join(out, 'weapon_icons')
        wi.PREVIEW = os.path.join(out, 'preview', 'weapon_icons')
        wi.PICTURES = os.path.join(game_opfor, 'work', 'ww2', 'pictures')
        nam = os.path.join(game, 'Package', 'levels', 'nam_common')
        wi.SRC = {'ingame': os.path.join(nam, 'ingame-00.fbrb'), 'loader': os.path.join(nam, 'loader-00.fbrb')}
        wi.main()
    elif tool == 'duplicates':
        import duplicates as dup
        dup.OUT = os.path.join(out, 'duplicates')
        dup.hud_icons()
    elif tool == 'wake_ui':
        import wake_ui as wu
        import ww2_crosshairs as wx
        wu.SRC = {k: os.path.join(game, 'Package', 'levels', 'nam_common', f'{k}-00.fbrb') for k in ('ingame', 'loader')}
        wx.NAM_INGAME = os.path.join(game, 'Package', 'levels', 'nam_common', 'ingame-00.fbrb')
        wx.MP_INGAME = os.path.join(game, 'Dist', 'win32', 'levels', 'mp_common', 'ingame-00.fbrb')
        wx.PREVIEW = os.path.join(out, 'preview', 'crosshairs')
        wu.main(['--out', os.path.join(out, 'wake_ui')])
    elif tool == 'bf1943_hud_icons':
        import bf1943_hud_icons as hud
        import numpy as np
        for name, tl in (('kill_feed', hud.KILL_FEED), ('vehicle', hud.VEHICLE)):
            labels = ['sherman', 'chi-ha', 'Zero', 'Corsair', 'LCVP', 'willys', 'kurogane', 'Bofors', 'hmg',
                      'bombraid', 'bombraid2'] if tl == hud.KILL_FEED else \
                ['Sherman', 'Chi-Ha', 'Zero', 'Corsair', 'LCVP', 'Willys', 'Kurogane', 'Bofors', 'HMG', 'bombraid',
                 'bombraid2']
            got = hud.icons(tl, labels)
            d = os.path.join(out, 'bf1943_hud_icons', name)
            os.makedirs(d, exist_ok=True)
            for lab, px in got.items():
                np.save(os.path.join(d, lab + '.npy'), px)
            print(f'{name}: {sorted(got)} {[got[k].shape for k in sorted(got)]}')
    elif tool == 'qa_crosshairs':
        qa_crosshairs(out, game)
    elif tool == 'console_icons':
        console_icons(out, game_opfor)
    else:
        raise SystemExit(f'no tool {tool}')


def qa_crosshairs(out: str, game: str) -> None:
    """qa_checks.check_crosshairs (as test_ww2_crosshairs drives it) on the stock nam_common movie and on wake_ui's
    built one (built here first, into out)."""
    import qa_checks as Q
    import ww2_crosshairs as wx
    from bfbc2core import dbx, fbrb
    wx.NAM_INGAME = os.path.join(game, 'Package', 'levels', 'nam_common', 'ingame-00.fbrb')
    wx.MP_INGAME = os.path.join(game, 'Dist', 'win32', 'levels', 'mp_common', 'ingame-00.fbrb')
    built = os.path.join(out, 'qa_crosshairs', 'built')
    wx.build(built, None, log=print)
    stock = fbrb.read(wx.NAM_INGAME, [wx.MOVIE])[wx.MOVIE]
    vals = ['rifle', 'Knife', 'Sherman_cannon', 'P40_warhawk_us', 'ka52_pilot', 'NoSuchCrosshair', 'debugHud']
    for name, movie in (('stock', stock), ('built', open(os.path.join(built, *wx.MOVIE.split('/')), 'rb').read())):
        d = tempfile.mkdtemp(prefix='parity-qa-')
        try:
            menu = os.path.join(d, 'wake_ui', 'ingame', 'Menu')
            os.makedirs(menu)
            with open(os.path.join(menu, 'hud_crosshair.res'), 'wb') as f:
                f.write(movie)
            lvl = os.path.join(d, 'install', 'level')
            os.makedirs(os.path.join(lvl, 'objects'))
            lines = []
            for k, v in enumerate(vals):
                n = f'objects/thing{k}.dbx'
                with open(os.path.join(lvl, *n.split('/')), 'w', encoding='utf-8') as f:
                    f.write(f'<field name="CrosshairTypeId">{v}</field>')
                lines.append(f'{n}|Dbx|00010000|1|1')
            with open(os.path.join(lvl, 'fbrb.manifest'), 'w', encoding='utf-8') as f:
                f.write('\n'.join(lines) + '\n')
            ctx = types.SimpleNamespace(work=d, install=os.path.join(d, 'install'), suite=wx.at.SUITE,
                                        game=os.path.join(d, 'game'))
            keep = dbx.to_xml
            setattr(dbx, 'to_xml', lambda data: bytes(data).decode('utf-8'))  # noqa: B010  (as the test patches it)
            try:
                r = Q.check_crosshairs(ctx, None)
            finally:
                dbx.to_xml = keep
            print(f'{name}: {r["summary"]}')
            for line in r['findings']:
                print(f'  {line.replace(d, "<TMP>")}')
        finally:
            shutil.rmtree(d, ignore_errors=True)


def console_icons(out: str, game_opfor: str) -> None:
    """console_icons.build for ons_mp_002's two bundles into out (its WORK patched; the game copy read)."""
    import console_common as cc
    import console_icons as ci
    cc.GAME = os.path.join(game_opfor, 'game')
    cc.OPFOR = game_opfor
    ci.MFT = os.path.join(cc.GAME, 'Dist', 'win32', 'package.mft')
    ci.chains.__defaults__ = (ci.MFT,)
    ci.WORK = os.path.join(out, 'console_icons')
    names = ci.names('ons_mp_002')
    print(f'vehicle names: {sorted(names)}')
    for name in ci.BUNDLES:
        _src, lines = ci.build('ons_mp_002', name, names)
        print('\n'.join(line.replace(out, '<OUT>') for line in lines))


# ------------------------------------------------------------------------------------------------ 1. the orchestrator

def files_of(root: str) -> dict[str, str]:
    out = {}
    for d, _, fs in os.walk(root):
        for f in fs:
            p = os.path.join(d, f)
            out[os.path.relpath(p, root).replace(os.sep, '/')] = p
    return out


def run_side(side: str, tool: str, tools_dir: str, toolkit: str, game_opfor: str, out: str,
             any_mod: bool = False) -> tuple[list[str], float, str]:
    env = dict(os.environ, OPFOR_TOOLKIT=toolkit, OPFOR_ROOT=game_opfor, PYTHONDONTWRITEBYTECODE='1',
               PYTHONIOENCODING='utf-8')
    cmd = [sys.executable, os.path.abspath(__file__), '--side', side, '--tool', tool, '--tools-dir', tools_dir,
           '--game-opfor', game_opfor, '--out', out] + (['--any-module'] if any_mod else [])
    r = subprocess.run(cmd, capture_output=True, text=True, encoding='utf-8', errors='replace', env=env)
    text = r.stdout
    if '@@BEGIN' not in text or '@@END' not in text:
        return [], 0.0, f'exit {r.returncode}: {(r.stdout + r.stderr)[-1500:]}'
    body = text.split('@@BEGIN\n', 1)[1].split('@@END', 1)[0]
    dt = float(text.split('@@TIME ', 1)[1].split()[0])
    roots = sorted({out, tools_dir, os.path.dirname(os.path.dirname(tools_dir)), game_opfor}, key=len, reverse=True)
    lines = []
    for line in body.splitlines():
        for r_, name in ((x, '<OUT>' if x == out else '<OPFOR>') for x in roots):
            line = line.replace(r_, name)
        lines.append(line)
    return lines, dt, ''


def tools_part(args: argparse.Namespace, scratch: str) -> int:
    game_opfor = args.opfor
    if not os.path.isdir(os.path.join(game_opfor, 'game')):
        print(f'tools: skipped (no game copy in {game_opfor})')
        return 0
    new_tools = args.new or os.path.join(os.path.dirname(SUITE), 'project-opfor-bfbc2', 'src', 'tools')
    if not os.path.isdir(new_tools):
        print(f'tools: skipped (no OPFOR src\\tools at {new_tools})')
        return 0
    old_root = os.path.join(scratch, 'old_opfor')
    if not os.path.isdir(os.path.join(old_root, 'src', 'tools')):
        os.makedirs(old_root, exist_ok=True)
        arch = subprocess.run(['git', '-C', game_opfor, 'archive', '--format=tar', args.old, 'src/tools'],
                              capture_output=True)
        if arch.returncode:
            print(f'tools: skipped (git archive {args.old}: {arch.stderr.decode(errors="replace")[-300:]})')
            return 0
        subprocess.run(['tar', '-x', '-C', old_root], input=arch.stdout, check=True)
    old_tools = os.path.join(old_root, 'src', 'tools')
    old_toolkit = os.path.dirname(args.old_suite)
    new_toolkit = os.path.dirname(SUITE)
    bad = 0
    wanted = args.tools.split(',') if args.tools else list(TOOLS)
    for tool in wanted:
        if tool == 'console_icons' and 'aptgraft.' in open(os.path.join(new_tools, 'onslaught', 'console_icons.py'),
                                                            encoding='utf-8').read():
            print('console_icons: skipped (the new side still runs aptgraft)')
            continue
        res = {}
        for side, tools_dir, toolkit in (('old', old_tools, old_toolkit), ('new', new_tools, new_toolkit)):
            out = os.path.join(scratch, side, tool)
            if os.path.isdir(out):
                shutil.rmtree(out)
            res[side] = run_side(side, tool, tools_dir, toolkit, game_opfor, out,
                                 side == 'new' and args.any_module) + (out,)
        (ol, ot, oe, oo), (nl, nt, ne, no) = res['old'], res['new']
        if oe or ne:
            print(f'{tool}: FAILED to run - old: {oe or "ok"} | new: {ne or "ok"}')
            bad += 1
            continue
        fo, fn = files_of(oo), files_of(no)
        same = [k for k in fo if k in fn and open(fo[k], 'rb').read() == open(fn[k], 'rb').read()]
        diff = sorted(set(fo) ^ set(fn)) + [k for k in fo if k in fn and k not in same]
        size = sum(os.path.getsize(fo[k]) for k in same)
        lines_same = ol == nl
        print(f'{tool}: {len(same)} of {len(set(fo) | set(fn))} files identical ({size:,} bytes), printed lines '
              f'{"identical" if lines_same else "DIFFER"} ({len(ol)}); old {ot:.1f} s, new {nt:.1f} s')
        if args.verbose or not lines_same:
            for a, b in zip(ol, nl):
                if a != b or args.verbose:
                    print(f'    old: {a}\n    new: {b}' if a != b else f'    {a}')
            if len(ol) != len(nl):
                print(f'    old has {len(ol)} lines, new {len(nl)}')
        if diff:
            print(f'    differ: {diff[:20]}')
        bad += bool(diff) or not lines_same
    return bad


# ------------------------------------------------------------------------------------------------ 2. / 3. in process

def outcome(fn: Any, *a: Any, **k: Any) -> tuple[str, Any]:
    """('ok', value) or ('AptError', words) or ('error', 'Type: words')."""
    try:
        return 'ok', fn(*a, **k)
    except Exception as e:  # noqa: BLE001  (the error is the result compared)
        name = type(e).__name__
        return ('AptError', str(e)) if name == 'AptError' else ('error', f'{name}: {e}')


class Tally:
    def __init__(self, what: str) -> None:
        self.what, self.same, self.total, self.refused = what, 0, 0, 0
        self.notes: list[str] = []

    def add(self, name: str, a: tuple[str, Any], b: tuple[str, Any]) -> None:
        self.total += 1
        ok = a == b or (a[0] == 'error' and b[0] == 'error')
        if a[0] != 'ok':
            self.refused += 1
        if ok:
            self.same += 1
        else:
            self.notes.append(f'    {name}: aptgraft {str(a)[:300]} | rust {str(b)[:300]}')

    def line(self) -> str:
        return (f'{self.what}: {self.same} of {self.total} the same ({self.refused} refused by both)'
                + ('' if self.same == self.total else '\n' + '\n'.join(self.notes[:20])))


def ref_modules(any_mod: bool = False) -> tuple[Any, Any]:
    if any_mod:
        any_module(os.path.join(SUITE, 'core'))
    sys.path[:0] = [os.path.join(SUITE, 'core'), os.path.join(SUITE, 'bfbc2-menu-tools'),
                    os.path.join(SUITE, 'bfbc2-bf1943-import')]
    from bfbc2core import native
    nat = native.require('the movie API parity', 'AptMovie')
    import aptgraft
    return aptgraft, nat


def blank(ag: Any, name: str = 't') -> list[tuple[str, bytes]]:
    """test_aptgraft.blank: an empty little-endian movie."""
    sig = ag.SIG
    a = bytearray(b'Apt Data:74\x1a\0\0\0\0')
    mv = len(a)
    a += struct.pack('<15I', 9, sig, 1, mv + 60, 0, 1, mv + 68, 1280, 720, 33, 0, 0, 0, 0, 0)
    a += struct.pack('<2I', 0, 0)
    a += struct.pack('<I', mv)
    c = b'Apt constant file\x1a\0\0' + struct.pack('<3I', mv, 0, 32)
    g = b'Apt PCGL geometry file\x1a\0' + struct.pack('<2I', 0, 32)
    return [(name + '.apt', bytes(a)), (name + '.const', c), (name + '.geo', g)]


def synthetic(ag: Any, rng: random.Random) -> bytes:
    """A random weaponicon-like movie (aptgraft's Python model): images, shapes (one textured quad mostly; two parts,
    other part types, a zero span now and then), sprite timelines with labels from a small pool and places of
    shapes, images and nothing, a clip action now and then; written in GRAFT_LAYOUT."""
    W, R, S = ag.Words, ag.Ref, ag.Str
    m = ag.Movie(blank(ag))
    f = ag.f32
    images = []
    for _ in range(rng.randint(1, 3)):
        n = W('apt', None, [7, ag.SIG, 0], 'image')
        i = m.add_char(n)
        n.vals[2] = i
        images.append(i)
    shapes = []
    for _ in range(rng.randint(1, 3)):
        w0, h0 = rng.choice([0.0, rng.uniform(1, 200)]) if rng.random() < 0.05 else rng.uniform(1, 200), \
            rng.uniform(1, 100)
        x0, y0 = rng.uniform(-100, 0), rng.uniform(-50, 0)
        sh = W('apt', None, [1, ag.SIG, f(x0), f(y0), f(x0 + w0), f(y0 + h0), 0], 'shape')
        sid = m.add_char(sh)
        sh.vals[6] = sid
        parts = []
        for _p in range(2 if rng.random() < 0.15 else 1):
            nv = rng.choice([12, 12, 12, 6, 18])
            verts = W('geo', None, [f(rng.uniform(-100, 100)) for _ in range(nv)], 'geo.vertices')
            kind = 5 if rng.random() < 0.9 else rng.choice([1, 4])
            if kind == 1:
                p = W('geo', None, [1, nv // 6, R(verts), rng.getrandbits(32)], 'geo.part1')
            else:
                mat = [f(rng.uniform(-2, 2)) if rng.random() < 0.7 else 0 for _ in range(16)]
                p = W('geo', None, [kind, nv // 6, R(verts), rng.getrandbits(32)] + mat + [rng.choice(images), 0],
                      f'geo.part{kind}')
            parts.append(R(p))
        m.add_geometry(sid, W('geo', None, parts, 'geo.parts'))
        shapes.append(sid)
    pool = ['PBLB', 'Boat', 'M1A2', 'X y']
    for _ in range(rng.randint(1, 4)):
        sp = W('apt', None, [5, ag.SIG, 0, 0, 0], 'sprite')
        m.add_char(sp)
        for fr in range(rng.randint(1, 3)):
            for lab in rng.sample(pool, rng.randint(0, 2)):
                flags = rng.choice([0, 0x70000, 0x10000])
                m.add_item(fr, W('apt', None, [2, R(S('apt', None, lab.encode())), flags, fr], 'label'), sp)
            for _k in range(rng.randint(0, 2)):
                cid = rng.choice(shapes + images + [len(m.chartable.vals) + 5])
                p = ag.place_item(m, rng.randint(1, 4), cid, rng.choice([None, 'pic', 'icon']),
                                  rng.uniform(-50, 50), rng.uniform(-50, 50), flags=rng.choice([0x26, 0x26, 0x24, 0x06]))
                if rng.random() < 0.05:
                    p.vals[15] = R(W('apt', None, [0, 0], 'clipactions'))
                m.add_item(fr, p, sp)
    if rng.random() < 0.3:
        m.add_item(0, W('apt', None, [2, R(S('apt', None, b'PBLB')), 0, 0], 'label'))
    return m.to_res(layout=ag.GRAFT_LAYOUT)


def hide_py(ag: Any, m: Any, name: str, keep: int) -> list[tuple[Any, Any]]:
    """console_icons.add_icon's hiding, as it was."""
    hidden = []
    for c in range(len(m.chartable.vals)):
        if c == keep or m.char_type(c) != 5:
            continue
        o = m.char(c)
        for fr in range(len(m.frames(o).vals) // 2):
            for it in m.frame_items(fr, o):
                if it.tag == 'label' and ag.text(it.vals[1]) == name:
                    hidden.append((it, it.vals[1]))
                    it.vals[1] = ag.Ref(ag.Str('apt', None, b'__opfor_hidden_' + name.encode('latin1')))
    return hidden


def icon_case(ag: Any, nat: Any, res: bytes, like: str, label: str, size: tuple[float, float],
              hide: int | None) -> tuple[tuple[str, Any], tuple[str, Any]]:
    def ref() -> Any:
        m = ag.Movie(ag.read_bigf_bytes(res), 'synthetic')
        hidden = hide_py(ag, m, label, hide) if hide is not None else []
        try:
            r = ag.add_icon_frame(m, like, label, size)
        finally:
            for it, v in hidden:
                it.vals[1] = v
        n = outcome(ag.add_label_alias, m, like, label)
        return r, n, m.to_res(layout=ag.GRAFT_LAYOUT)

    def new() -> Any:
        m = nat.AptMovie.from_bytes(res, 'synthetic')
        r = m.add_icon_frame(like, label, size, hide_except=hide)
        n = outcome(m.add_label_alias, like, label)
        return r, n, m.to_res()
    return outcome(ref), outcome(new)


def random_part(rng: random.Random, f32: Any) -> tuple[list[int], list[int], list[tuple]]:
    """A textured part's fields, its vertices and tiles: an axis-aligned rectangle mostly (any corner order, flipped
    and scaled matrices), skewed matrices, bent corners, other counts now and then; tiles on a grid, some missing."""
    a, d = rng.choice([1.0, -1.0, 0.5, 2.0, rng.uniform(-3, 3)]), rng.choice([1.0, -1.0, 0.5, rng.uniform(-3, 3)])
    b = c = 0.0
    if rng.random() < 0.1:
        b, c = rng.uniform(-1, 1), rng.uniform(-1, 1)
    if rng.random() < 0.05:
        a = d = 0.0
    tx, ty = rng.uniform(-20, 120), rng.uniform(-20, 120)
    x0, y0 = rng.uniform(-50, 50), rng.uniform(-50, 50)
    w, h = rng.choice([rng.uniform(0.5, 120), 30.0, 92.0]), rng.choice([rng.uniform(0.5, 120), 30.0, 92.0])
    corners = [(x0, y0), (x0 + w, y0), (x0 + w, y0 + h), (x0, y0 + h)]
    tris = rng.choice([[0, 1, 2, 0, 2, 3], [0, 1, 3, 1, 2, 3], [3, 2, 1, 3, 1, 0]])
    verts = []
    for k in tris:
        verts += list(corners[k])
    if rng.random() < 0.05:
        verts[rng.randrange(12)] += rng.uniform(1, 5)
    if rng.random() < 0.03:
        verts = verts[:6]
    mat = [0.0] * 16
    mat[0], mat[1], mat[4], mat[5], mat[12], mat[13] = a, b, c, d, tx, ty
    for k in (2, 3, 6, 7, 8, 9, 10, 11, 14, 15):
        mat[k] = rng.choice([0.0, rng.uniform(-1, 1)])
    count = 2 if rng.random() > 0.03 else 3
    fields = [rng.choice([4, 5]), count, 0, rng.getrandbits(32)] + [f32(v) for v in mat] + [rng.randint(1, 900), 0]
    if rng.random() < 0.05:
        fields += [rng.getrandbits(32)]
    step = rng.choice([30, 16, 64, 200])
    tiles = []
    for ty_ in range(-1, 140, step):
        for tx_ in range(-60, 180, step):
            if rng.random() < 0.8:
                tiles.append((tx_, ty_, rng.choice([step, step - 2, step + 3]), step, rng.randint(1, 500),
                              rng.randint(0, 400), rng.randint(0, 400)))
    if rng.random() < 0.1:
        tiles = [(500, 500, 10, 10, 7, 0, 0)]                # misses the part
    return fields, [f32(v) for v in verts], tiles


def split_case(ag: Any, nat: Any, fields: list[int], verts: list[int], tiles: list[tuple]) -> tuple[Any, Any]:
    def ref() -> Any:
        part = ag.Words('geo', None, list(fields), f'geo.part{fields[0]}')
        part.vals[2] = ag.Ref(ag.Words('geo', None, list(verts), 'geo.vertices'))
        out = []
        for q in ag.split_image_part(part, tiles):
            v = list(q.vals)
            vs = list(v[2].node.vals)
            v[2] = 0
            out.append((v, vs))
        return out

    def new() -> Any:
        return [(list(f), list(v)) for f, v in nat.apt_split_image_part(fields, verts, tiles)]
    return outcome(ref), outcome(new)


def random_part_cases(ag: Any, nat: Any, rng: random.Random, n: int) -> int:
    t = Tally('add_icon_frame / add_label_alias on synthetic movies')
    for k in range(n):
        res = synthetic(ag, rng)
        like = rng.choice(['PBLB', 'PBLB', 'PBLB', 'M1A2', 'Nope'])
        label = rng.choice(['Type95 Boat', 'Boat', 'X y', 'MRKV'])
        size = rng.choice([(78, 36), (78.0, 36.0), (rng.uniform(1, 200), rng.uniform(1, 80)), (0, 0)])
        hide = rng.choice([None, None, rng.randint(0, 8)])
        a, b = icon_case(ag, nat, res, like, label, size, hide)
        t.add(f'synthetic {k} like={like} label={label} size={size} hide={hide}', a, b)
    print(t.line())
    s = Tally('split_image_part on random parts')
    for k in range(n):
        fields, verts, tiles = random_part(rng, ag.f32)
        a, b = split_case(ag, nat, fields, verts, tiles)
        s.add(f'part {k}', a, b)
    print(s.line())
    return (t.total - t.same) + (s.total - s.same)


def damaged_cases(ag: Any, nat: Any, game_opfor: str) -> int:
    game = os.path.join(game_opfor, 'game')
    nam = os.path.join(game, 'Package', 'levels', 'nam_common', 'ingame-00.fbrb')
    if not os.path.isfile(nam):
        print(f'damaged inputs: skipped (no game copy in {game_opfor})')
        return 0
    from bfbc2core import fbrb
    t = Tally('damaged inputs')
    wi = fbrb.read(nam, ['Menu/weaponicon.res'])['Menu/weaponicon.res']
    # a movie cut short: both read it the same or refuse alike
    n = len(wi)
    for cut in sorted({n - 1, n - 4, n - 100, n // 2, n // 3, 4096, 200, 40, 17, 8, 3}):
        b = wi[:cut]

        def ref(b: bytes = b) -> Any:
            m = ag.Movie(ag.read_bigf_bytes(b, 'cut'), 'cut')
            return ag.add_label_alias(m, 'PBLB', 'Type95 Boat'), m.to_res(layout=ag.GRAFT_LAYOUT)

        def new(b: bytes = b) -> Any:
            m = nat.AptMovie.from_bytes(b, 'cut', 'cut')
            return m.add_label_alias('PBLB', 'Type95 Boat'), m.to_res()
        t.add(f'cut at {cut} of {n}', outcome(ref), outcome(new))
    # not a BIGF at all
    t.add('not a movie', outcome(lambda: ag.read_bigf_bytes(b'RIFF' + wi[4:100], 'x.res')),
          outcome(lambda: nat.AptMovie.from_bytes(b'RIFF' + wi[4:100], 'm', 'x.res')))

    def pair(name: str, fr: Any, fn: Any) -> None:
        def ref() -> Any:
            m = ag.Movie(ag.read_bigf_bytes(wi), 'weaponicon')
            r = fr(m)
            return r, m.to_res(layout=ag.GRAFT_LAYOUT)

        def new() -> Any:
            m = nat.AptMovie.from_bytes(wi, 'weaponicon')
            r = fn(m)
            return r, m.to_res()
        t.add(name, outcome(ref), outcome(new))
    pair('label alias of a missing label', lambda m: ag.add_label_alias(m, 'NoSuchLabel', 'X'),
         lambda m: m.add_label_alias('NoSuchLabel', 'X'))
    pair('icon frame like a missing label', lambda m: ag.add_icon_frame(m, 'NoSuchLabel', 'X', (10, 10)),
         lambda m: m.add_icon_frame('NoSuchLabel', 'X', (10, 10)))
    pair('icon frame with a label there already', lambda m: ag.add_icon_frame(m, 'PBLB', 'Garand', (78, 36)),
         lambda m: m.add_icon_frame('PBLB', 'Garand', (78, 36)))
    pair('label_frame of a missing label', lambda m: m.label_frame('NoSuchLabel', m.char(483)),
         lambda m: m.label_frame('NoSuchLabel', 483))
    # the labels whose frame's first shape is not one textured quad (an94_sp_k: two pictures)
    m = ag.Movie(ag.read_bigf_bytes(wi), 'weaponicon')
    odd = []
    for c in range(len(m.chartable.vals)):
        if m.char_type(c) != 5:
            continue
        o = m.char(c)
        for fr in range(len(m.frames(o).vals) // 2):
            items = m.frame_items(fr, o)
            pl = [it for it in items if it.tag == 'place' and it.vals[1] & 2 and not isinstance(it.vals[3], ag.Ref)
                  and m.char_type(it.vals[3]) == 1]
            labs = [ag.text(it.vals[1]) for it in items if it.tag == 'label']
            if pl and labs:
                e = m.geo_entry(pl[0].vals[3])
                parts = e[0].vals[e[1] + 2].node.vals if e else []
                if len(parts) != 1 or parts[0].node.vals[0] != 5:
                    odd.append(labs[0])
    for lab in sorted(set(odd))[:6]:
        pair(f'icon frame like {lab} (not one quad)', lambda m, lab=lab: ag.add_icon_frame(m, lab, 'NewIcon', (40, 20)),
             lambda m, lab=lab: m.add_icon_frame(lab, 'NewIcon', (40, 20)))
    for lab in ('Garand', 'M1A2', 'PBLB'):
        pair(f'icon frame like {lab}', lambda m, lab=lab: ag.add_icon_frame(m, lab, 'NewIcon', (55.5, 21)),
             lambda m, lab=lab: m.add_icon_frame(lab, 'NewIcon', (55.5, 21)))
    print(f'  (labels whose shape is not one quad: {len(set(odd))})')
    # crosshairs: BF1943's into nam_common's
    try:
        import bf1943import
        src_bf = bf1943import.Source(bf1943import.default_source()).data('menu/hud_crosshair.res')
    except Exception as e:  # noqa: BLE001  (no BF1943 source folder)
        print(t.line())
        print(f'crosshairs: skipped (BF1943: {type(e).__name__}: {e})')
        return t.total - t.same
    hc = fbrb.read(nam, ['Menu/hud_crosshair.res'])['Menu/hud_crosshair.res']

    def ch(name: str, steps: list[tuple[str, Any]]) -> None:
        def ref() -> Any:
            dst = ag.Movie(ag.read_bigf_bytes(hc), 'dst')
            src = ag.Movie(ag.read_bigf_bytes(src_bf), 'src')
            out = [outcome(lambda c=c, k=k: report(ag.add_crosshair(dst, src, c, **k))) for c, k in steps]
            return out, dst.to_res(layout=ag.GRAFT_LAYOUT)

        def new() -> Any:
            dst = nat.AptMovie.from_bytes(hc, 'dst')
            src = nat.AptMovie.from_bytes(src_bf, 'src')
            out = [outcome(lambda c=c, k=k: report(dst.add_crosshair(src, c, **k))) for c, k in steps]
            return out, dst.to_res()
        t.add(name, outcome(ref), outcome(new))

    def report(r: dict) -> dict:
        return {k: (sorted(v.items()) if isinstance(v, dict) else v) for k, v in r.items()}
    ch('crosshair id already present', [('Sherman_cannon', {}), ('Sherman_cannon', {})])
    ch('crosshair id of the target already', [('launcher_v', {})])
    ch('crosshair id unknown', [('NoSuchCrosshair', {})])
    ch('crosshair on a frame that is not free', [('P40_warhawk_us', {'frame': 0})])
    # tiles: every BF1943 image of the three onto far-away tiles (they miss every part), an image without tiles
    sm = ag.Movie(ag.read_bigf_bytes(src_bf), 'src')
    imgs: list[int] = []
    for cid in ('Sherman_cannon', 'P40_warhawk_us', 'P40_warhawk_jp'):
        lab = ag.crosshair_table(sm)[cid]['label']
        for _, it in ag.crosshair_objects(sm, lab):
            imgs += [i for i in ag.deps(sm, it.vals[3])['images'] if i not in imgs]
    far = {i: [(5000, 5000, 30, 30, 300 + k, 0, 0)] for k, i in enumerate(imgs)}
    ch('tiles that miss every part', [('Sherman_cannon', {'images': far}), ('P40_warhawk_jp', {'images': far})])
    near = {i: [(x, y, 30, 30, 300 + k, 4 * k, 8) for x in range(-1, 120, 30) for y in range(-1, 120, 30)]
            for k, i in enumerate(imgs)}
    ch('tiles on a grid', [(c, {'images': near}) for c in ('Sherman_cannon', 'P40_warhawk_us', 'P40_warhawk_jp')])
    if imgs:
        ch('an image without tiles', [('P40_warhawk_us', {'images': {imgs[0]: []}})])
    print(t.line())
    return t.total - t.same


def main(argv: list[str]) -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument('--opfor', default=os.environ.get('OPFOR_ROOT') or r'C:\Programming\Projects\project-opfor-bfbc2')
    ap.add_argument('--old', default=OLD_COMMIT)
    ap.add_argument('--new')
    ap.add_argument('--old-suite')
    ap.add_argument('--tools', help=f'a comma list of {", ".join(TOOLS)} (default all); "none": skip part 1')
    ap.add_argument('--cases', type=int, default=500)
    ap.add_argument('--seed', type=int, default=20261127)
    ap.add_argument('--scratch')
    ap.add_argument('--keep', action='store_true')
    ap.add_argument('-v', '--verbose', action='store_true')
    ap.add_argument('--any-module', action='store_true',
                    help="this checkout's module even when its fingerprint is another checkout state's")
    # the side runner (internal)
    ap.add_argument('--side')
    ap.add_argument('--tool')
    ap.add_argument('--tools-dir')
    ap.add_argument('--game-opfor')
    ap.add_argument('--out')
    args = ap.parse_args(argv[1:])
    if args.side:
        return side_main(args)
    args.old_suite = args.old_suite or os.path.join(os.path.dirname(args.opfor), 'bfbc2-tool-suite')
    scratch = args.scratch or tempfile.mkdtemp(prefix='parity-menu-movies-')
    bad = 0
    try:
        ag, nat = ref_modules(args.any_module)
        t = time.perf_counter()
        ag.use_native(False)                              # the reference: aptgraft's own Python model
        bad += random_part_cases(ag, nat, random.Random(args.seed), args.cases)
        print(f'  ({time.perf_counter() - t:.1f} s)')
        ag.use_native(True)
        t = time.perf_counter()
        bad += damaged_cases(ag, nat, args.opfor)
        print(f'  ({time.perf_counter() - t:.1f} s)')
        if args.tools != 'none':
            bad += tools_part(args, scratch)
    finally:
        if not args.keep and not args.scratch:
            shutil.rmtree(scratch, ignore_errors=True)
        elif args.keep:
            print(f'scratch kept: {scratch}')
    print('parity: ' + ('DIFFERENCES' if bad else 'identical'))
    return 1 if bad else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
