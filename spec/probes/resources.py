"""The resource kinds and loose files that had no format of their own before round SPG4, measured on DICE's files
(read-only; spec/formats/occluder.md OCC-*, soot.md SOOT-*, animtree.md ATREE-*, bink.md BINK-*, audio.md WAVE-IR-*,
havok.md HAVOK-RAG-1, game_folder.md GAME-FILE-*).

    python spec/probes/resources.py [--opfor <folder>]

Reads, from Project OPFOR BFBC2's game copy (DICE's files only, dated before 2013): every distinct `OccluderMesh`,
`SootMesh`, `AnimTreeInfo`, `ImpulseResponse`, `RagdollResource` and `BinkMemory` entry of DICE's bundles (each name
once, the first bundle in path order), the movies and the aerial height map under `Output/win32`, and the loose files
of the game folder (`database.dbmanifest`, `dfe`, `InstallationData.xml`, `UpdateHistory.log`,
`dialoglogo128x128.jpg`, `BFBC2Game.exe`'s names for them). Prints structure and counts only (sizes, counts, field
words that are counts or versions; no payload). `checks(game)` recomputes the stable answers the rules cite for
core/tests/test_spec_probes.py.
"""
from __future__ import annotations

import argparse
import collections
import math
import os
import re
import struct
import sys
import time
from pathlib import Path
from typing import Any

SUITE = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(SUITE / 'core'))
from bfbc2core import fbrb, paths  # noqa: E402

TYPES = ('OccluderMesh', 'SootMesh', 'AnimTreeInfo', 'ImpulseResponse', 'RagdollResource', 'BinkMemory')
Counter = collections.Counter

EXPECTED: dict[str, Any] = {
    'n': {'AnimTreeInfo': 1926, 'BinkMemory': 56, 'ImpulseResponse': 5, 'OccluderMesh': 361, 'RagdollResource': 1,
          'SootMesh': 40},
    # OCC-HDR-1, OCC-TRI-1, OCC-VTX-1
    'occ_layout': 361, 'occ_rest': 361, 'occ_in_box': 361, 'occ_index_ok': 360, 'occ_empty': 1,
    'occ_tri_block': {'absent': 39, 'present': 321, 'present, empty': 1},
    'occ_tri_records': {'one a triangle': 253, 'more': 68},
    'occ_w': {'1.0, block': 321, 'k x 12.75, no block': 39},
    # SOOT-HDR-1, SOOT-VTX-1, SOOT-TAIL-1
    'soot_layout': 40, 'soot_version': [18], 'soot_stride': {12: 34, 16: 6}, 'soot_boxes': [2, 4, 8, 10, 27],
    'soot_index_ok': 40, 'soot_tail': 40,
    # ATREE-HDR-1, ATREE-PTR-1
    'atree_version': [6], 'atree_layout': 1926, 'atree_aligned': 1926, 'atree_pointers_inside': 1926,
    # WAVE-IR-1
    'ir_header': [('irf1', 1, 2, 48000, 1024, 1024, 2048, 1)], 'ir_layout': 5,
    # HAVOK-RAG-1
    'ragdoll': [(16944, True, 'Havok-5.5.0-r1')],
    # BINK-HDR-1, BINK-RES-1
    'bink_res': {('BIKi', 1280, 720, 25.0, 0): 40, ('BIKi', 1280, 720, 29.97, 0): 16}, 'bink_res_size': 56,
    # BINK-OUT-1
    'movies': {('BinkMemory', None, 'BIKi', 0): 22, ('BinkTexture', (0, 0), 'BIKi', 0): 3,
               ('BinkTexture', (0, 0), 'BIKi', 1): 7, ('BinkTexture', (1, 1), 'BIKi', 0): 1},
    'movies_size': 33, 'movies_lengths_zero': 33,
    # GAME-FILE-3
    'aerial': [(8.0, 512, 512, -2048.0, -2048.0, True)],
    # GAME-FILE-4 - 7
    'dbmanifest': ('Database', 2, 2), 'dfe': (1000, True),
    'installation': ['CurrentVersion', 'EulaVersion', 'InstalledHashes', 'PatcherVersion', 'PbEulaVersion', 'Sku'],
    'installation_version': '795745', 'update_history': [('patch', '515195', '795745')],
    'jpeg': (128, 128),
    'exe_names': {'AerialHeightmap.res': True, 'InstallationData': False, 'UpdateHistory': False, 'binkw32.dll': True,
                  'database.dbmanifest': True, 'dialoglogo': False},
}


def dice(p: Path) -> bool:
    return time.gmtime(p.stat().st_mtime).tm_year < 2013


def entries(game: Path) -> dict[str, list[tuple[str, bytes]]]:
    """{type: [(name, bytes)]} of the TYPES, each name once."""
    out: dict[str, list[tuple[str, bytes]]] = collections.defaultdict(list)
    seen: set[str] = set()
    for p in sorted((p for p in game.glob('**/*.fbrb') if dice(p)), key=lambda p: str(p).lower()):
        es = [e for e in fbrb.entries(p) if not e.deleted and e.type in TYPES and e.name.lower() not in seen]
        if not es:
            continue
        seen.update(e.name.lower() for e in es)
        got = fbrb.read(p, [e.name for e in es])
        for e in es:
            b = got[e.name]
            # BinkMemory: only the header is measured (the videos are large)
            out[e.type].append((e.name, b[:64] + struct.pack('<I', len(b)) if e.type == 'BinkMemory' else b))
    return out


def occluders(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """OCC-*: box (6 f32), u32 rest length, a 16-byte part header {u16 vertices, u16 indices, u32 vertex offset,
    u32 index offset, u32 triangle-block offset} (offsets from byte 28), the triangle block, vertices, indices."""
    layout = rest = in_box = index_ok = empty = 0
    block: Counter[str] = Counter()
    recs: Counter[str] = Counter()
    w: Counter[str] = Counter()
    for _, d in items:
        mn, mx = struct.unpack_from('<3f', d, 0), struct.unpack_from('<3f', d, 12)
        rest += struct.unpack_from('<I', d, 24)[0] == len(d) - 28
        nv, ni, vo, io, to = struct.unpack_from('<HHIII', d, 28)
        if 28 + vo + 16 * nv == 28 + io and 28 + io + 2 * ni == len(d) and ni % 3 == 0:
            layout += 1
        vs = [struct.unpack_from('<4f', d, 28 + vo + 16 * k) for k in range(nv)]
        in_box += all(mn[a] - 1e-3 <= v[a] <= mx[a] + 1e-3 for v in vs for a in range(3))
        idx = struct.unpack_from(f'<{ni}H', d, 28 + io)
        if not idx:
            empty += 1
        else:
            index_ok += max(idx) < nv
        size = vo - to if to else 0
        block['absent' if not to else 'present, empty' if size == 0 else 'present'] += 1
        if size:
            recs['one a triangle' if size == 24 * (ni // 3) else
                 'more' if size % 24 == 0 and size > 24 * (ni // 3) else 'other'] += 1
        ws = {v[3] for v in vs}
        if ws:
            w['1.0, block' if ws == {1.0} and size else
              'k x 12.75, no block' if not to and len(ws) == 1 and (next(iter(ws)) / 12.75).is_integer() else 'other'] += 1
    out.update(occ_layout=layout, occ_rest=rest, occ_in_box=in_box, occ_index_ok=index_ok, occ_empty=empty,
               occ_tri_block=dict(sorted(block.items())), occ_tri_records=dict(sorted(recs.items())),
               occ_w=dict(sorted(w.items())))


def soots(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """SOOT-*: 5 u32 (version, stride, two flags, n boxes), n x 6 f32, 4 u32 {1, vertex bytes, index bytes, 0},
    vertices, u16 indices, 64 u32 tail {0, n, triangles, 60, 60 words}."""
    layout = index_ok = tail = 0
    versions, boxes = set(), set()
    stride: Counter[int] = Counter()
    for _, d in items:
        ver, st, _f1, _f2, n = struct.unpack_from('<5I', d, 0)
        versions.add(ver)
        boxes.add(n)
        stride[st] += 1
        h = 0x14 + 24 * n
        one, vb, ib, zero = struct.unpack_from('<4I', d, h)
        if (one, zero) == (1, 0) and h + 16 + vb + ib + 256 == len(d) and vb % st == 0:
            layout += 1
        idx = struct.unpack_from(f'<{ib // 2}H', d, h + 16 + vb)
        index_ok += bool(idx) and max(idx) < vb // st
        t = struct.unpack_from('<4I', d, len(d) - 256)
        tail += t == (0, n, ib // 6, 60)
    out.update(soot_layout=layout, soot_version=sorted(versions), soot_stride=dict(sorted(stride.items())),
               soot_boxes=sorted(boxes), soot_index_ok=index_ok, soot_tail=tail)


def anim_trees(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """ATREE-*: {u32 version, u32 base address, u32 sizes of blocks A, B, C}, the blocks; a word in
    [base, base + blocks] is a pointer into them."""
    versions = set()
    layout = aligned = inside = 0
    for _, d in items:
        ver, base, a, b, c = struct.unpack_from('<5I', d, 0)
        versions.add(ver)
        layout += 20 + a + b + c == len(d)
        aligned += a % 16 == 0 and b % 16 == 0 and c % 16 == 0
        words = struct.unpack_from(f'<{len(d) // 4}I', d, 0)[5:]
        rel = [x - base for x in words if base <= x < base + len(d)]
        inside += all(r % 4 == 0 and r <= a + b + c for r in rel)
    out.update(atree_version=sorted(versions), atree_layout=layout, atree_aligned=aligned,
               atree_pointers_inside=inside)


def impulse_responses(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    """WAVE-IR-1: big-endian 48-byte header, then ceil(frames / block) blocks of 32 + 4 x FFT-size bytes."""
    heads = set()
    layout = 0
    for _, d in items:
        magic, ver, ch, frames, rate, blk, blk2, fft, one = struct.unpack_from('>4s8I', d, 0)
        heads.add((magic.decode('latin-1'), ver, ch, rate, blk, blk2, fft, one))
        layout += 48 + math.ceil(frames / blk) * (32 + 2 * ch * fft) == len(d)
    out.update(ir_header=sorted(heads), ir_layout=layout)


def ragdolls(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    out['ragdoll'] = sorted((len(d), d[:8] == bytes.fromhex('57e0e05710c0c010'), d[40:54].decode('latin-1'))
                            for _, d in items)


def bink(d: bytes, at: int) -> tuple[str, int, int, float, int, bool]:
    """BINK-HDR-1: signature, width, height, frame rate, audio tracks, whether the size word fits."""
    sig, size, frames, _largest, frames2, wd, ht, num, den, _flags, tracks = struct.unpack_from('<4s10I', d, at)
    return sig.decode('latin-1'), wd, ht, round(num / den, 2), tracks, frames == frames2 and size


def bink_resources(items: list[tuple[str, bytes]], out: dict[str, Any]) -> None:
    got: Counter[tuple[str, int, int, float, int]] = Counter()
    fits = 0
    for _, d in items:
        sig, wd, ht, fps, tracks, size = bink(d, 0)
        got[(sig, wd, ht, fps, tracks)] += 1
        fits += size + 8 == struct.unpack_from('<I', d, 64)[0]
    out.update(bink_res=dict(sorted(got.items())), bink_res_size=fits)


def movies(game: Path, out: dict[str, Any]) -> None:
    """BINK-OUT-1: the RES wrapper with both length words 0; BinkTexture carries 8 bytes more."""
    got: Counter[tuple[str, Any, str, int]] = Counter()
    size = zero = 0
    for p in sorted((game / 'Output' / 'win32' / 'movies').glob('**/*.res')):
        if not dice(p):
            continue
        with open(p, 'rb') as f:
            d = f.read(128)
        n = p.stat().st_size
        kind = d[4:56].rstrip(b'\0').decode('latin-1')
        at = 72 if kind == 'BinkTexture' else 64
        pre = struct.unpack_from('<2I', d, 64) if kind == 'BinkTexture' else None
        sig, _wd, _ht, _fps, tracks, sz = bink(d, at)
        got[(kind, pre, sig, tracks)] += 1
        size += sz + 8 + at == n
        zero += struct.unpack_from('<2I', d, 56) == (0, 0) and d[:4] == b'RES\x02'
    out.update(movies=dict(sorted(got.items(), key=str)), movies_size=size, movies_lengths_zero=zero)


def loose(game: Path, out: dict[str, Any]) -> None:
    """GAME-FILE-*: the aerial height map, database.dbmanifest, dfe, the patch record, the dialog logo."""
    aerial = []
    for p in sorted((game / 'Output' / 'win32' / 'levels').glob('*/aerialheightmap.res')):
        d = p.read_bytes()
        cell, w, h, x0, z0, _scale, _offset = struct.unpack_from('<fIIffff', d, 0)
        aerial.append((cell, w, h, x0, z0, 28 + 2 * w * h == len(d)))
    out['aerial'] = aerial
    root = (game / 'database.dbmanifest').read_bytes()
    m = re.search(rb'<([A-Za-z]+)[\s>]', root)
    same = 0
    copies = 0
    for b in (game / 'Dist' / 'win32' / 'build_overlay-00.fbrb', game / 'Package' / 'build_overlay-00.fbrb'):
        got = fbrb.read(b, ['database.dbmanifest'])
        copies += len(got)
        same += got.get('database.dbmanifest') == root
    out['dbmanifest'] = (m.group(1).decode() if m else '', copies, same)
    d = (game / 'dfe').read_bytes()
    c = Counter(d)
    out['dfe'] = (len(d), -sum(n / len(d) * math.log2(n / len(d)) for n in c.values()) > 7.5)
    x = (game / 'InstallationData.xml').read_text(encoding='latin-1')
    out['installation'] = sorted(set(re.findall(r'<([A-Za-z]+)>', x)) - {'InstallationData'})
    v = re.search(r'<CurrentVersion>(\d+)</CurrentVersion>', x)
    out['installation_version'] = v.group(1) if v else ''
    log = (game / 'UpdateHistory.log').read_text(encoding='latin-1').splitlines()
    out['update_history'] = [m2.groups() for line in log
                             for m2 in [re.search(r'/(patch)_(\d+)_(\d+)\s*$', line)] if m2]
    j = (game / 'dialoglogo128x128.jpg').read_bytes()
    sof = re.search(rb'\xff[\xc0\xc2]..\x08(..)(..)', j, re.S)
    out['jpeg'] = struct.unpack('>HH', sof.group(2) + sof.group(1)) if sof and j[:2] == b'\xff\xd8' else None
    exe = (game / 'BFBC2Game.exe').read_bytes()
    out['exe_names'] = {n: n.encode().lower() in exe.lower() for n in EXPECTED['exe_names']}


def analyse(game: Path) -> dict[str, Any]:
    items = entries(game)
    out: dict[str, Any] = {'n': {t: len(v) for t, v in sorted(items.items())}}
    occluders(items['OccluderMesh'], out)
    soots(items['SootMesh'], out)
    anim_trees(items['AnimTreeInfo'], out)
    impulse_responses(items['ImpulseResponse'], out)
    ragdolls(items['RagdollResource'], out)
    bink_resources(items['BinkMemory'], out)
    movies(game, out)
    loose(game, out)
    return out


def checks(game: Path) -> list[tuple[bool, str]]:
    got = analyse(game)
    return [(got.get(k) == want, f'resources probe: {k} = {want!r} (got {got.get(k)!r})')
            for k, want in EXPECTED.items()]


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (default: found as the suite finds it)')
    a = ap.parse_args()
    game = Path(paths.game_dir(paths.find_opfor(a.opfor)))
    t = time.time()
    for k, v in sorted(analyse(game).items()):
        print(f'{k}: {v}')
    print(f'({time.time() - t:.1f} s)')


if __name__ == '__main__':
    os.environ.setdefault('PYTHONIOENCODING', 'utf-8')
    main()
