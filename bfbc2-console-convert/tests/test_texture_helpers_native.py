r"""Round F0: texture.py's layout helpers (tiled, packed, layout: bc2_native.texture_tiled / texture_packed /
texture_layout, the converter's own Rust) and its PC texture reader (levels, decode: texture_levels / texture_decode)
in the Rust core against their Python, the reference that stays until the port passes a full test tier
(tiled_reference, packed_reference, layout_reference, levels_reference, decode_reference).

Compared: every value (and for decode the array's dtype, shape, every pixel and that it is writable); for damaged
inputs and odd arguments the same result or the same error.

  python test_texture_helpers_native.py          quick: 200,000 tiled offsets, every packed rule, layouts of every
                                                 format and shape, textures built here (every decoded format, odd
                                                 sizes, padded L8 rows), 2,000 damaged levels and headers; 40 PC
                                                 textures of the game copy when found
  python test_texture_helpers_native.py --game   + every PC texture (ITexture) of the game copy: levels and every
                                                 level of a decoded format decoded both ways; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.

The levels and decode references were retired in round RT2 (tiled, packed and layout stay: compared live): their
answers were recorded first (--record, and --game --record, while they were there: f0_texture_replay*.json beside
this test) and are replayed by tests/replay.py where they are gone.
"""
import contextlib
import glob
import os
import random
import struct
import sys
import time
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core')
import texture  # noqa: E402
from bfbc2core import fbrb, native, paths  # noqa: E402
from f0_parity import Checks, canon, stdout_utf8  # noqa: E402
from replay import Replay  # noqa: E402

N = native.require('texture.py helpers', 'texture_decode')
# round RT2: levels' and decode's references recorded (--record) and their answers replayed once retired
R = Replay('f0_texture', HERE, [])
LEVELS_REF = getattr(texture, 'levels_reference', None)
DECODE_REF = getattr(texture, 'decode_reference', None)


def result(fn: Any, *args: Any) -> tuple[Any, ...]:
    """('ok', value) with arrays as (dtype, shape, bytes, writable), or ('error', type, message)."""
    try:
        v = fn(*args)
    except Exception as ex:          # noqa: BLE001 - the error is what is compared
        return ('error', type(ex).__name__, str(ex))
    if isinstance(v, np.ndarray):
        return ('ok', str(v.dtype), v.shape, v.tobytes(), v.flags.writeable)
    return ('ok', canon(v))


class Tally:
    def __init__(self) -> None:
        self.same = self.ndiff = self.native = 0
        self.differ: list[str] = []

    def add(self, ok: bool, label: str, answered: bool) -> None:
        self.native += answered
        if ok:
            self.same += 1
        else:
            self.ndiff += 1
            if len(self.differ) < 10:
                self.differ.append(label)


def both(t: Tally, label: str, fn: Any, ref: Any, answered: bool, *args: Any) -> None:
    a = result(fn, *args)
    if fn in (texture.levels, texture.decode):         # recorded / replayed (round RT2)
        name = 'texture.' + fn.__name__ + '_reference'
        t.add(R.pair(name, args, a, None if ref is None else (lambda: result(ref, *args)), refused=not answered),
              f'{label}: {str(a)[:100]}', answered)
        return
    r = result(ref, *args)
    t.add(a == r, f'{label}: {str(a)[:100]} / {str(r)[:100]}', answered)


def pc_texture(rng: random.Random, fmt: int, w: int, h: int, mips: int, cube: bool) -> bytes:
    """A PC ITexture: the header and random levels (DICE's L8 rows padded to 4 bytes now and then)."""
    bpb, edge, _ = texture.FORMATS[fmt]
    sizes, data = [], b''
    for s in range(6 if cube else 1):
        for m in range(mips):
            lw, lh = max(w >> m, 1), max(h >> m, 1)
            if fmt == 10:
                pitch = (lw + 3) // 4 * 4 if rng.random() < 0.5 else lw
                n = pitch * lh
            else:
                n = max((lw + 3) // 4, 1) * max((lh + 3) // 4, 1) * bpb
            if s == 0:
                sizes.append(n)
            data += bytes(rng.randrange(256) for _ in range(n))
    hdr = struct.pack('<8I', 0, 1 if cube else 0, fmt, 0, w, h, 1, mips) + struct.pack('<15I', *(sizes + [0] * (15 - len(sizes))))
    return hdr + data


def damaged(b: bytes, rng: random.Random) -> bytes:
    x = bytearray(b)
    k = rng.randrange(3)
    if k in (1, 2) and x:
        for _ in range(rng.randint(1, 3)):
            x[rng.randrange(min(len(x), 92)) if rng.random() < 0.6 else rng.randrange(len(x))] = rng.randrange(256)
    if k in (0, 2) and x:
        del x[rng.randrange(len(x)):]
    return bytes(x)


def game_textures(limit: int | None) -> list[tuple[str, bytes]]:
    try:
        opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
    except Exception as e:                                       # noqa: BLE001
        print(f'skip the game copy ({e})')
        return []
    out: list[tuple[str, bytes]] = []
    seen: set[str] = set()
    for p in sorted(glob.glob(os.path.join(opfor, 'game', '**', '*.fbrb'), recursive=True)):
        try:
            names = [e.name for e in fbrb.entries(p) if e.type == 'ITexture' and not e.deleted and e.name.lower() not in seen]
        except Exception:                                        # noqa: BLE001
            continue
        if not names:
            continue
        seen.update(n.lower() for n in names)
        for n, b in sorted(fbrb.read(p, names).items()):
            out.append((n, b))
        if limit and len(out) >= limit:
            return out[:limit]
    return out


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    if hasattr(texture, 'use_native'):
        c.check(texture.use_native(True), "the Rust core runs texture.py's helpers")
    rng = random.Random(0xF05)

    t = Tally()
    for _ in range(200_000):
        args = (rng.randrange(1 << rng.choice([5, 9, 13])), rng.randrange(1 << rng.choice([5, 9, 13])),
                rng.randrange(1, 1 << rng.choice([5, 9, 13])), rng.randrange(5))
        a, r = texture.tiled(*args), texture.tiled_reference(*args)
        t.add(a == r, f'tiled {args}', N.texture_tiled(*args) is not None)
    xs = np.arange(64, dtype=np.int64)
    a_arr, r_arr = texture.tiled(xs, xs, 64, 2), texture.tiled_reference(xs, xs, 64, 2)
    t.add(bool((a_arr == r_arr).all()), 'tiled on NumPy arrays (the Python)', False)
    c.check(not t.ndiff, f'tiled: {t.same:,} identical ({t.native:,} in Rust; arrays in Python)', '; '.join(t.differ))

    tp = Tally()
    for _ in range(20_000):
        pargs = (rng.choice([0, 1, 2, 3, 4, 7, 16, 17, 64, 1000, 2048, 4096]), rng.choice([0, 1, 2, 4, 16, 33, 512, 2048]),
                 rng.randrange(16), rng.choice([1, 4, 4, 0]))
        both(tp, f'packed {pargs}', texture.packed, texture.packed_reference, N.texture_packed(*pargs)[0], *pargs)
    c.check(not tp.ndiff, f'packed: {tp.same:,} identical ({tp.native:,} in Rust)', '; '.join(tp.differ))

    tl = Tally()
    for _ in range(5_000):
        fmt = rng.choice(list(texture.FORMATS) + [7, -1])
        largs = (rng.choice([1, 2, 4, 8, 16, 32, 100, 256, 1024, 4096, 0]), rng.choice([1, 2, 4, 8, 16, 64, 333, 2048, 0]),
                 rng.randrange(0, 14), fmt, rng.choice([1, 1, 6, 0]))
        both(tl, f'layout {largs}', texture.layout, texture.layout_reference, N.texture_layout(*largs) is not None, *largs)
    c.check(not tl.ndiff, f'layout: {tl.same:,} identical ({tl.native:,} in Rust)', '; '.join(tl.differ))

    built = []
    for fmt in sorted(texture.DECODED):
        for _ in range(25):
            w, h = rng.choice([1, 2, 3, 4, 5, 8, 13, 16, 64, 100]), rng.choice([1, 2, 4, 7, 16, 32, 50])
            mips = rng.randint(1, 4)
            built.append(pc_texture(rng, fmt, w, h, mips, rng.random() < 0.1))
    tt = Tally()
    for i, b in enumerate(built):
        both(tt, f'built {i} levels', texture.levels, LEVELS_REF, N.texture_levels(b) is not None, b)
        fmt = struct.unpack_from('<I', b, 8)[0]
        for s, m, w, h, d in texture.levels(b):
            both(tt, f'built {i} decode {s}/{m}', texture.decode, DECODE_REF,
                 N.texture_decode(d, w, h, fmt) is not None, d, w, h, fmt)
    c.check(not tt.ndiff, f'built textures (every decoded format): levels and every level decoded: {tt.same:,} '
            f'identical ({tt.native:,} in Rust)', '; '.join(tt.differ))

    td = Tally()
    for i in range(1000):
        b = damaged(rng.choice(built), rng)
        both(td, f'damaged {i} levels', texture.levels, LEVELS_REF, N.texture_levels(b) is not None, b)
        b = rng.choice(built)
        fmt = struct.unpack_from('<I', b, 8)[0]
        _, _, w, h, d = rng.choice(texture.levels(b))
        dargs = (damaged(d, rng), rng.choice([w, w, w + 4, 0, -3]), rng.choice([h, h, h + 1, 0]),
                 rng.choice([fmt, fmt, fmt, 3, 13, 5, 9, 99]))
        both(td, f'damaged {i} decode', texture.decode, DECODE_REF, N.texture_decode(*dargs) is not None,
             *dargs)
    c.check(not td.ndiff, f'damaged levels and headers, odd sizes and formats: {td.same:,} the same result or error '
            f'({td.native:,} in Rust)', '; '.join(td.differ))

    items = game_textures(None if game else 40)
    if items:
        tg = Tally()
        tn = tr = 0.0
        nlev = 0
        fmts: dict[int, int] = {}
        for name, b in items:
            both(tg, f'{name} levels', texture.levels, LEVELS_REF, N.texture_levels(b) is not None, b)
            try:
                lv = texture.levels(b)
            except Exception:                                    # noqa: BLE001
                continue
            fmt = struct.unpack_from('<I', b, 8)[0]
            if fmt not in texture.DECODED:
                continue
            fmts[fmt] = fmts.get(fmt, 0) + 1
            for s, m, w, h, d in lv:
                nlev += 1
                t0 = time.perf_counter()
                with contextlib.suppress(Exception):
                    texture.decode(d, w, h, fmt)
                t1 = time.perf_counter()
                if DECODE_REF is not None:
                    with contextlib.suppress(Exception):
                        DECODE_REF(d, w, h, fmt)
                tr += time.perf_counter() - t1
                tn += t1 - t0
                both(tg, f'{name} decode {s}/{m}', texture.decode, DECODE_REF,
                     N.texture_decode(d, w, h, fmt) is not None, d, w, h, fmt)
        what = ', '.join(f'format {k}: {v:,}' for k, v in sorted(fmts.items()))
        c.check(not tg.ndiff, f'{len(items):,} game textures ({what}; {nlev:,} levels decoded both ways): {tg.same:,} '
                f'identical ({tg.native:,} in Rust)', '; '.join(tg.differ))
        print(f'timing (decode): NumPy {tr:.2f} s, Rust {tn:.2f} s')
    c.check(R.finish(), 'levels / decode references: recorded, or replayed as recorded')
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
