r"""texture.convert (the Rust core: rust/bc2-convert through bc2_native) against recorded results.

The Python converter (texture.convert_python) was the reference until 2026-11-17, when its Rust port had passed the
full test tier (140/140) and it was retired (rust/BENCHMARKS.md, "Retired Python"). Its results were recorded
first, from the Python code itself, in texture_golden.json beside this file: each case's output SHA-1 or its
ConvertError message. They are the oracle now.

  python test_texture_native.py           quick: 1,362 built and damaged textures from a seeded generator (every
                                          format, 2D and cube, odd sizes, packed tails, every mip count; short data,
                                          bad headers, random header bytes) against their recorded results; the
                                          batch function; ConvertError's message
  python test_texture_native.py --game    + every XenonTexture of Battlefield 1943's source and every console
                                          texture of OPFOR's Onslaught builds (work\...\con_*), read only: each
                                          input's SHA-1 and its output's against the recording
  python test_texture_native.py --record [--game]
                                          write texture_golden.json from the current converter: only after a
                                          deliberate change to the conversion, proven on the game's textures
                                          (tests\test_texture.py, test_texqa.py) - this file is the only oracle

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import hashlib
import json
import os
import random
import struct
import sys
import time
from collections.abc import Iterator
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'bf1943-import')
import texture  # noqa: E402

GOLDEN = os.path.join(HERE, 'texture_golden.json')
FAILS: list[str] = []


def check(cond: bool, msg: str) -> None:
    if not cond:
        FAILS.append(msg)
        print('FAIL', msg)


def sha1(b: bytes) -> str:
    """The first 16 hex digits of b's SHA-1 (64 bits: plenty to tell a changed output, and a small file)."""
    return hashlib.sha1(b).hexdigest()[:16]


def outcome(b: bytes) -> list[str]:
    """['ok', the output's SHA-1] or ['error', ConvertError's message]."""
    try:
        return ['ok', sha1(texture.convert(b))]
    except texture.ConvertError as e:
        return ['error', str(e)]


def built(rng: random.Random, fmt: int, w: int, h: int, mips: int, typ: int = 0, depth: int = 0, extra: int = 0,
          flags: int = 1) -> bytes:
    """A console texture: the header and as much random data as the layout needs (plus extra bytes)."""
    _, size = texture.layout(w, h, mips, 9 if fmt == 9 else fmt, 6 if typ == texture.CUBE else 1)
    hdr = struct.pack('>8I', 3, typ, fmt, flags, w, h, depth, mips) + bytes(60)
    return hdr + rng.randbytes(size + extra)


def cases() -> Iterator[tuple[str, bytes]]:
    """The quick tier's textures, always the same (seed 1943: the generator of the Python-vs-Rust test before
    2026-11-17, its random draws in the same order)."""
    rng = random.Random(1943)
    formats = [0, 18, 1, 2, 19, 20, 3, 13, 5, 10, 9]
    sizes = [(1, 1), (2, 2), (4, 4), (8, 8), (16, 16), (32, 8), (8, 32), (64, 64), (128, 32), (32, 256), (100, 60),
             (256, 256), (512, 128), (17, 33), (1024, 64)]
    for fmt in formats:
        for w, h in sizes:
            top = max(w, h).bit_length()
            for mips in sorted({1, 2, min(5, top), top}):
                yield f'format {fmt} {w}x{h} mips {mips}', built(rng, fmt, w, h, mips, extra=rng.choice([0, 1, 7]))
        if fmt not in (3, 5, 9):
            for w in (16, 64, 128):
                yield f'format {fmt} cube {w}', built(rng, fmt, w, w, w.bit_length(), typ=texture.CUBE)
    # the decoded formats as cube maps and the flags (bit 1 cleared by DXT5A only)
    for fmt in (3, 5, 9):
        yield f'format {fmt} cube refused', built(rng, fmt, 16, 16, 5, typ=texture.CUBE)
        yield f'format {fmt} flags', built(rng, fmt, 64, 64, 7, flags=0xFFFFFFFF)
    # damaged: short data, short headers, bad dimensions, unknown formats and types, every truncation of one file
    good = built(rng, 2, 64, 32, 7)
    for cut in list(range(0, 100)) + list(range(100, len(good), 97)):
        yield f'truncated to {cut}', good[:cut]
    for words in [(3, 0, 0, 1, 0, 4, 0, 1), (3, 0, 0, 1, 4, 0, 0, 1), (3, 0, 0, 1, 4, 4, 0, 0), (3, 0, 0, 1, 4, 4, 0, 16),
                  (3, 0, 0, 1, 20000, 4, 0, 1), (3, 0, 0, 1, 4, 4, 0, 4), (3, 2, 0, 1, 4, 4, 0, 1), (3, 0, 0, 1, 4, 4, 2, 1),
                  (3, 0, 7, 1, 4, 4, 0, 1), (3, 0, 99, 1, 4, 4, 0, 1), (3, 1, 3, 1, 4, 4, 0, 1)]:
        yield f'header {words}', struct.pack('>8I', *words) + bytes(60) + bytes(8192)
    for n in range(300):
        b = bytearray(built(rng, rng.choice(formats), rng.choice([4, 8, 64]), rng.choice([4, 16, 64]), rng.randint(1, 4)))
        for _ in range(rng.randint(1, 3)):
            b[rng.randrange(32)] = rng.randrange(256)
        yield f'random header bytes {n}', bytes(b)


def write(golden: dict[str, Any]) -> None:
    """texture_golden.json, one case per line (a change shows as the lines of the cases it changed)."""
    lines = ['{']
    keys = sorted(golden)
    for i, k in enumerate(keys):
        v, end = golden[k], (',' if i < len(keys) - 1 else '')
        if isinstance(v, list):
            rows = [json.dumps(r) for r in v]
            lines += [f'{json.dumps(k)}: [', *[r + ',' for r in rows[:-1]], *rows[-1:], ']' + end]
        elif isinstance(v, dict):
            rows = [f'{json.dumps(n)}: {json.dumps(v[n])}' for n in sorted(v)]
            lines += [f'{json.dumps(k)}: {{', *[r + ',' for r in rows[:-1]], *rows[-1:], '}' + end]
        else:
            lines.append(f'{json.dumps(k)}: {json.dumps(v)}{end}')
    lines.append('}')
    with open(GOLDEN, 'w', encoding='utf-8', newline='\n') as f:
        f.write('\n'.join(lines) + '\n')


def load() -> dict[str, Any]:
    try:
        with open(GOLDEN, encoding='utf-8') as f:
            return dict(json.load(f))
    except FileNotFoundError:
        return {}


def quick(golden: dict[str, Any], record: bool) -> None:
    texture.require()
    got = [[name, *outcome(b)] for name, b in cases()]
    if record:
        golden['quick'] = got
        print(f'{len(got)} built and damaged textures recorded')
    else:
        want = golden.get('quick', [])
        check(len(want) == len(got), f'{len(got)} cases, {len(want)} recorded')
        bad = 0
        for g, w in zip(got, want):
            if g != w:
                bad += 1
                if bad <= 10:
                    check(False, f'{g[0]}: {g[1:]} != recorded {w[1:]}')
        check(bad == 0, f'{bad} of {len(got)} cases differ from the recording')
        print(f'{len(got)} built and damaged textures: {bad} differ from the recording '
              f'({sum(g[1] == "ok" for g in got)} converted, {sum(g[1] != "ok" for g in got)} refused)')
    # the batch function: the same results in order, errors returned (not raised) with the same messages
    items = [b for _, b in cases()] + [b'short']
    many = texture.convert_many(items)
    check(len(many) == len(items), 'batch: one result per texture')
    for b, r in zip(items, many):
        one = outcome(b)
        if isinstance(r, texture.ConvertError):
            check(one == ['error', str(r)], f'batch: refused ({r}), one by one {one}')
        else:
            check(one == ['ok', sha1(r)], 'batch: differs from convert')
    try:
        texture.convert(b'short')
        check(False, 'a short texture must raise ConvertError')
    except texture.ConvertError as e:
        check(str(e) == 'shorter than a texture header', f'message {e}')


def game(golden: dict[str, Any], record: bool) -> None:
    import bf1943import
    sys.path.insert(0, HERE)
    import test_texture
    src = bf1943import.Source(bf1943import.default_source())
    names = sorted(k for k, (_, e) in src.entries.items() if e['type'] == 'XenonTexture')
    con = test_texture.index('XenonTexture', console=True)
    groups = {'bf1943': [(k, src.data(k)) for k in names],
              'onslaught': [(n, open(p, 'rb').read()) for n, p in sorted(con.items())]}
    for group, items in groups.items():
        t = time.perf_counter()
        got = {n: [sha1(b), *outcome(b)] for n, b in items}
        took = time.perf_counter() - t
        if record:
            golden[group] = got
            print(f'{group}: {len(got)} textures recorded')
            continue
        want = golden.get(group, {})
        same = changed_input = differ = 0
        for n, g in got.items():
            w = want.get(n)
            if w is None:
                continue
            if w[0] != g[0]:
                changed_input += 1
                check(False, f'{group} {n}: the input is not the recorded one (check it, then re-record)')
            elif w != g:
                differ += 1
                check(False, f'{group} {n}: {g[1:]} != recorded {w[1:]}')
            else:
                same += 1
        new, gone = sorted(set(got) - set(want)), sorted(set(want) - set(got))
        check(not new and not gone, f'{group}: {len(new)} textures not recorded {new[:3]}, {len(gone)} recorded ones '
                                    f'missing {gone[:3]}')
        print(f'{group}: {len(got)} textures ({sum(len(b) for _, b in items) / 1e6:.0f} MB) in {took:.2f} s: '
              f'{same} as recorded, {differ} differ, {changed_input} inputs changed')
    converted = sum(v[1] == 'ok' for v in golden.get('bf1943', {}).values())
    check(converted >= 690, f'only {converted} BF1943 textures convert')


if __name__ == '__main__':
    record = '--record' in sys.argv
    golden = load()
    quick(golden, record)
    if '--game' in sys.argv:
        game(golden, record)
    if record and not FAILS:
        golden['recorded'] = time.strftime('%Y-%m-%d')
        write(golden)
        print(f'wrote {GOLDEN}')
    print('all passed' if not FAILS else f'{len(FAILS)} failures')
    sys.exit(1 if FAILS else 0)
