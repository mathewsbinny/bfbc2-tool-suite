r"""texture.convert in the Rust core (rust/bc2-convert, through bc2_native) against the Python reference
(texture.convert_python): the same bytes, or both refusing.

  python test_texture_native.py           quick: built textures of every format (2D, cube, odd sizes, packed tails,
                                          every mip count), damaged headers and short data, the dispatcher and its
                                          fallback, the batch function
  python test_texture_native.py --game    + every XenonTexture of Battlefield 1943's source (what the WW2 pipeline
                                          converts) and every console texture of OPFOR's Onslaught builds
                                          (work\...\con_*), read only

Skipped (exit 0) when the module is not built (python rust\build_native.py).
"""
import os
import random
import struct
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('console-convert', 'core', 'bf1943-import')
import texture  # noqa: E402

FAILS: list[str] = []


def check(cond, msg):
    if not cond:
        FAILS.append(msg)
        print('FAIL', msg)


def outcome(fn, b):
    try:
        return 'ok', fn(b)
    except Exception as e:                       # noqa: BLE001 - the reference's exception, whatever it is
        return type(e).__name__, None


def native(b):
    return texture._native.texture_convert(bytes(b))


def same(b, name):
    """The port gives the reference's bytes, or refuses where the reference fails (and convert then gives the
    reference's own error)."""
    r = outcome(texture.convert_python, b)
    n = outcome(native, b)
    if n[0] == 'ok':
        check(r == n, f'{name}: Rust {len(n[1])} bytes, Python {r[0]} {len(r[1]) if r[1] else ""}')
    else:
        check(r[0] != 'ok' or outcome(texture.convert, b) == r, f'{name}: Rust refused, Python converted')
    check(outcome(texture.convert, b) == r, f'{name}: texture.convert differs from the reference')
    return r[0] == 'ok'


def built(fmt, w, h, mips, typ=0, depth=0, rng=random, extra=0, flags=1):
    """A console texture: the header and as much random data as the layout needs (plus extra bytes)."""
    _, size = texture.layout(w, h, mips, 9 if fmt == 9 else fmt, 6 if typ == texture.CUBE else 1)
    hdr = struct.pack('>8I', 3, typ, fmt, flags, w, h, depth, mips) + bytes(60)
    return hdr + rng.randbytes(size + extra)


def quick():
    if texture._native is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return
    rng = random.Random(1943)
    n = 0
    formats = [0, 18, 1, 2, 19, 20, 3, 13, 5, 10, 9]
    sizes = [(1, 1), (2, 2), (4, 4), (8, 8), (16, 16), (32, 8), (8, 32), (64, 64), (128, 32), (32, 256), (100, 60),
             (256, 256), (512, 128), (17, 33), (1024, 64)]
    for fmt in formats:
        for w, h in sizes:
            top = max(w, h).bit_length()
            for mips in sorted({1, 2, min(5, top), top}):
                b = built(fmt, w, h, mips, rng=rng, extra=rng.choice([0, 1, 7]))
                same(b, f'format {fmt} {w}x{h} mips {mips}')
                n += 1
        if fmt not in (3, 5, 9):
            for w in (16, 64, 128):
                same(built(fmt, w, w, w.bit_length(), typ=texture.CUBE, rng=rng), f'format {fmt} cube {w}')
                n += 1
    # the decoded formats as cube maps and the flags (bit 1 cleared by DXT5A only)
    for fmt in (3, 5, 9):
        same(built(fmt, 16, 16, 5, typ=texture.CUBE, rng=rng), f'format {fmt} cube refused')
        same(built(fmt, 64, 64, 7, flags=0xFFFFFFFF, rng=rng), f'format {fmt} flags')
        n += 2
    # damaged: short data, short headers, bad dimensions, unknown formats and types, every truncation of one file
    good = built(2, 64, 32, 7, rng=rng)
    for cut in list(range(0, 100)) + list(range(100, len(good), 97)):
        same(good[:cut], f'truncated to {cut}')
        n += 1
    for words in [(3, 0, 0, 1, 0, 4, 0, 1), (3, 0, 0, 1, 4, 0, 0, 1), (3, 0, 0, 1, 4, 4, 0, 0), (3, 0, 0, 1, 4, 4, 0, 16),
                  (3, 0, 0, 1, 20000, 4, 0, 1), (3, 0, 0, 1, 4, 4, 0, 4), (3, 2, 0, 1, 4, 4, 0, 1), (3, 0, 0, 1, 4, 4, 2, 1),
                  (3, 0, 7, 1, 4, 4, 0, 1), (3, 0, 99, 1, 4, 4, 0, 1), (3, 1, 3, 1, 4, 4, 0, 1)]:
        same(struct.pack('>8I', *words) + bytes(60) + bytes(8192), f'header {words}')
        n += 1
    for _ in range(300):
        b = bytearray(built(rng.choice(formats), rng.choice([4, 8, 64]), rng.choice([4, 16, 64]), rng.randint(1, 4),
                            rng=rng))
        for _ in range(rng.randint(1, 3)):
            b[rng.randrange(32)] = rng.randrange(256)
        same(bytes(b), 'random header bytes')
        n += 1
    # the batch function: the same results in order, errors returned
    items = [built(f, 32, 32, 3, rng=rng) for f in formats] + [b'short']
    many = texture._native.texture_convert_many(items)
    for b, r in zip(items, many):
        if isinstance(r, ValueError):
            check(outcome(texture.convert_python, b)[0] != 'ok', 'batch: refused one Python converts')
        else:
            check(r == texture.convert_python(b), 'batch: differs')
    # the dispatcher
    check(texture.backend() == 'rust', 'backend should be rust')
    check(texture.use_native(False) == 'python' and texture.convert(items[0]) == texture.convert_python(items[0]),
          'use_native(False)')
    check(texture.use_native(True) == 'rust', 'use_native(True)')
    try:
        texture.convert(b'short')
        check(False, 'a short texture must raise ConvertError')
    except texture.ConvertError as e:
        check(str(e) == 'shorter than a texture header', f'message {e}')
    print(f'{n} built and damaged textures: {len(FAILS)} differences')


def game():
    if texture._native is None:
        return
    import bf1943import
    src = bf1943import.Source(bf1943import.default_source())
    names = sorted(k for k, (_, e) in src.entries.items() if e['type'] == 'XenonTexture')
    datas = [src.data(k) for k in names]
    t = time.perf_counter()
    ref = [outcome(texture.convert_python, b) for b in datas]
    tp = time.perf_counter() - t
    t = time.perf_counter()
    nat = [outcome(native, b) for b in datas]
    tn = time.perf_counter() - t
    t = time.perf_counter()
    many = texture._native.texture_convert_many(datas)
    tm = time.perf_counter() - t
    ok = refused = 0
    for k, r, nn, m in zip(names, ref, nat, many):
        if r[0] == 'ok':
            ok += 1
            check(nn == r and m == r[1], f'{k}: differs')
        else:
            refused += 1
            check(nn[0] != 'ok' and isinstance(m, ValueError), f'{k}: Python {r[0]}, Rust converted')
    print(f'BF1943: {len(names)} textures ({sum(map(len, datas)) / 1e6:.0f} MB): {ok} identical, {refused} refused by '
          f'both; Python {tp:.1f} s, Rust {tn:.2f} s one by one, {tm:.2f} s batch')
    check(ok >= 690, f'only {ok} BF1943 textures converted')
    # the Onslaught builds' console textures (BFBC2's own Xbox data)
    import test_texture
    con = test_texture.index('XenonTexture', console=True)
    n = 0
    for name, p in sorted(con.items()):
        same(open(p, 'rb').read(), name)
        n += 1
    print(f'Onslaught console textures: {n} compared')


if __name__ == '__main__':
    sys.path.insert(0, HERE)
    quick()
    if '--game' in sys.argv:
        game()
    print('all passed' if not FAILS else f'{len(FAILS)} failures')
    sys.exit(1 if FAILS else 0)
