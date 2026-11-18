r"""Round E3n (rust/BENCHMARKS.md "E3n") against its references:

- apt.py's decoder in the Rust core (bc2_formats::apt: the constants, dump() and instructions() with dump_lines; the
  batch decoded()) against apt.py's Python, both ways in one process (apt.use_native): the same values with the same
  types in the same order, or the same exception;
- aptgraft's writer (one placement per code node, plans shared by roundtrip's writes, table-driven assembler) against
  the writer before E3n (aptgraft.REFERENCE, BFBC2_APTGRAFT_REFERENCE=1): the same bytes or the same AptError.

quick: test_aptgraft.py's built movies (a sprite with a shape, geometry, an import, clip events, a DefineFunction2,
branches, GetURL, string opcodes; hud_crosshair-like movies; little- and big-endian; grafted, crosshair-added), a
movie of 3,000 seeded EA_PushFloat values (the dump's %g: zeros, infinities, NaNs, denormals, ties) and the byte /
short / long pushes' extremes, and 600 damaged copies (truncated, bytes changed, pointers redirected).
--game: every SwfMovie of the game copy's bundles (Dist and Package), OPFOR's pristine menu bundle (original\) and
the BIGF movies of work\xbox_dlc (read only), each distinct movie once: decoded both ways; every write of the
unchanged model, roundtrip --pack and a graft of its sprites into the next movie both ways.

  python test_apt_e3n.py [--game]
"""
import hashlib
import os
import random
import struct
import sys
import tempfile
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SUITE = os.path.dirname(ROOT)
sys.path.insert(0, SUITE)
sys.path.insert(0, HERE)
import suite_paths  # noqa: E402

suite_paths.use('menu-tools', 'apt', 'core')
import apt  # noqa: E402
import aptgraft as ag  # noqa: E402
import test_aptgraft as tg  # noqa: E402
from aptgraft import Code, Ins, Ref, Words  # noqa: E402

failed = 0
HAVE_RUST = apt.use_native(True) == 'rust'


def check(ok: object, what: str, detail: str = '') -> None:
    global failed
    failed += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


def strict(a: Any, b: Any) -> bool:
    """Equal values with equal types, recursively (namedtuples by their fields)."""
    if type(a) is not type(b):
        return False
    if isinstance(a, (tuple, list)):
        return len(a) == len(b) and all(strict(x, y) for x, y in zip(a, b))
    return bool(a == b)


def outcome(fn: Any) -> Any:
    try:
        return ('ok', fn())
    except Exception as e:                    # the reference's exception is part of its answer
        return ('error', type(e).__name__, str(e))


def apt_data(members: list[tuple[str, bytes]]) -> tuple[str, bytes, bytes]:
    d = dict(members)
    n = next(k for k in d if k.lower().endswith('.apt'))[:-4]
    return n, d[n + '.apt'], d[n + '.const']


def decode(data: tuple[str, bytes, bytes], native: bool, batch: bool = False) -> Any:
    """(constants, instructions(), dump_lines, dump()) of a movie, or the exception, with one backend."""
    apt.use_native(native)

    def run() -> Any:
        m = apt.Movie(data=data)
        if batch:
            m = next(apt.decoded([m]))
        return m.const, m.instructions(), m.dump_lines, m.dump()
    return outcome(run)


def same_decode(data: tuple[str, bytes, bytes]) -> tuple[bool, int]:
    """Python, Rust and Rust's batch agree; returns (agree, instructions)."""
    py = decode(data, False)
    if not HAVE_RUST:
        return True, len(py[1][1]) if py[0] == 'ok' else 0
    rs, bt = decode(data, True), decode(data, True, batch=True)
    return strict(py, rs) and strict(py, bt), len(py[1][1]) if py[0] == 'ok' else 0


def writes(members: list[tuple[str, bytes]], label: str) -> list[Any]:
    """Every write of the unchanged model, as bytes or the AptError."""
    def mk() -> ag.Movie:
        return ag.Movie(members, label)
    out = []
    for kw in ({}, {'layout': 'pack', 'gap': 16}, {'order': '<'}, {'order': '>'}, {'layout': ag.GRAFT_LAYOUT},
               {'dead': False}, {'layout': 'pack', 'order': '>'}):
        out.append(outcome(lambda kw=kw: mk().write(**kw)))
    m = outcome(mk)
    if m[0] == 'ok':                          # several writes of one model, in turn
        mv = m[1]
        out.append(outcome(lambda: [mv.write(), mv.write(layout='pack'), mv.write(order='>'), mv.write(dead=False)]))
    return out


def both(fn: Any) -> tuple[Any, Any]:
    """fn() with the reference writer, then with E3n's."""
    ag.REFERENCE = True
    try:
        ref = outcome(fn)
    finally:
        ag.REFERENCE = False
    return ref, outcome(fn)


def graft_all(src_members: list[tuple[str, bytes]], dst_members: list[tuple[str, bytes]], most: int = 6) -> Any:
    """src's sprites (up to most) grafted into dst and placed, a label alias on dst's first label, written three ways."""
    src, dst = ag.Movie(src_members, 'src'), ag.Movie(dst_members, 'dst')
    g = ag.Grafter(src, dst)
    got = []
    for c in [c for c in range(len(src.chartable.vals)) if src.char_type(c) == 5][:most]:
        r = outcome(lambda c=c: g.char(c))
        got.append(r)
        if r[0] == 'ok':
            dst.add_item(0, ag.place_item(dst, ag.free_depths(dst, 1)[0], r[1], 'grafted', 1.5, 2.5))
    labels = [ag.text(it.vals[1]) for f in range(len(dst.frames().vals) // 2) for it in dst.frame_items(f)
              if it.tag == 'label']
    if labels:
        got.append(outcome(lambda: ag.add_label_alias(dst, labels[0], labels[0] + '_e3n')))
    return got, dst.write(layout=ag.GRAFT_LAYOUT), dst.write(layout='pack'), dst.write(order='<' if dst.big else '>')


def float_movie() -> list[tuple[str, bytes]]:
    """A movie whose action pushes 3,000 seeded float bit patterns and the byte / short / long extremes."""
    rng = random.Random(3)
    special = [0, 0x80000000, 0x7F800000, 0xFF800000, 0x7FC00000, 0xFFC00001, 1, 0x807FFFFF, 0x00800000,
               struct.unpack('<I', struct.pack('<f', 1234565.0))[0], struct.unpack('<I', struct.pack('<f', 0.0001))[0],
               struct.unpack('<I', struct.pack('<f', 999999.5))[0], struct.unpack('<I', struct.pack('<f', 1e-5))[0],
               struct.unpack('<I', struct.pack('<f', 100000.0))[0], struct.unpack('<I', struct.pack('<f', 0.5))[0]]
    bits = special + [rng.getrandbits(32) for _ in range(1500)] + \
        [struct.unpack('<I', struct.pack('<f', rng.choice([1, -1]) * rng.randint(0, 10 ** rng.randint(1, 9))
                                          / 10 ** rng.randint(0, 6)))[0] for _ in range(1485)]
    ins = [Ins(0xB4, [b]) for b in bits]
    ins += [Ins(0xB5, [v]) for v in (0, 0x7F, 0x80, 0xFF)] + [Ins(0xB6, [v]) for v in (0, 0x7FFF, 0x8000, 0xFFFF)]
    ins += [Ins(0xB7, [v]) for v in (0, 0x7FFFFFFF, 0x80000000, 0xFFFFFFFF)] + [Ins(0xB9, [7]), Ins(0)]
    m = ag.Movie(tg.blank('floats'))
    m.add_item(0, Words('apt', None, [1, Ref(Code('apt', None, ins))], 'action'))
    return m.write()


_SPANS: dict[bytes, list[tuple[int, int]]] = {}


def damaged(members: list[tuple[str, bytes]], rng: random.Random) -> tuple[str, bytes, bytes]:
    """A copy of the movie's .apt truncated, or with its code damaged: bytes of instructions changed, an operand set
    to an offset inside or past the file, an opcode made a branch / function / list / unknown opcode. (Not the
    structure's counts: with a frame's item count changed the reference writes one line per item, billions.)"""
    name, a, c = apt_data(members)
    if a not in _SPANS:
        apt.use_native(False)
        _SPANS[a] = [(i.at, max(i.end, i.at + 1)) for i, *_ in apt.Movie(data=(name, a, c)).instructions()]
    spans = _SPANS[a]
    b = bytearray(a)
    kind = rng.randrange(4)
    at, end = spans[rng.randrange(len(spans))]
    if kind == 0:
        b = b[:rng.randrange(len(b))]
    elif kind == 1:
        for _ in range(rng.randint(1, 3)):
            b[rng.randrange(at, end)] = rng.randrange(256)
    elif kind == 2 and end - at >= 5:         # an aligned operand word
        o = (at + 4) & ~3
        if o + 4 <= len(b):
            b[o:o + 4] = struct.pack('<I', rng.choice([rng.randrange(len(b) + 64), 0xFFFFFFF0, 0x7FFFFFFF]))
    else:
        b[at] = rng.choice([0x99, 0x9D, 0xB8, 0x8E, 0x9B, 0x96, 0x88, 0x80, 0xC3])
    return name, bytes(b), c


def quick() -> None:
    work = tempfile.mkdtemp(prefix='apt_e3n_')
    src, _imp, _sid, spid = tg.build_source()
    le = src.write()
    be = ag.Movie(le, 'src').write(order='>')
    dst = tg.target()
    xh = tg.crosshair_movie('hud_crosshair', ['a', 'b'], 0x129, image=True)
    xh_src = tg.crosshair_movie('src_crosshair', ['c'], 0x29)
    xh_be = ag.Movie(xh_src.write(), 'x').write(order='>')
    grafted = ag.Movie(dst.write(), 'dst')
    gg = ag.Grafter(ag.Movie(be, 'be'), grafted)
    grafted.add_item(0, ag.place_item(grafted, 9, gg.char(spid), 'w', 1, 2))
    added = ag.Movie(xh.write(), 'xh')
    ag.add_crosshair(added, ag.Movie(xh_be, 'xbe'), 'c')
    movies = {'source (LE)': le, 'source (BE)': be, 'target': dst.write(), 'crosshair': xh.write(),
              'crosshair source (BE)': xh_be, 'grafted': grafted.write(layout=ag.GRAFT_LAYOUT),
              'crosshair added': added.write(layout=ag.GRAFT_LAYOUT), 'floats': float_movie()}

    # the decoder
    if not HAVE_RUST:
        print('skip the Rust decoder checks: the Rust core is not built')
    bad, n_ins = [], 0
    for label, mem in movies.items():
        ok, n = same_decode(apt_data(mem))
        n_ins += n
        if not ok:
            bad.append(label)
    check(not bad, f'decoder: {len(movies)} built movies ({n_ins} instructions), Python and Rust (single, batch) the same',
          ', '.join(bad))
    rng = random.Random(5)
    bad, refused, errors = [], 0, 0
    for k in range(600):
        label = list(movies)[k % len(movies)]
        data = damaged(movies[label], rng)
        ok, _ = same_decode(data)
        if not ok:
            bad.append(f'{label} #{k}')
        py = decode(data, False)
        errors += py[0] == 'error'
        if HAVE_RUST and py[0] == 'ok':
            apt.use_native(True)
            m = apt.Movie(data=data)
            refused += apt._native.apt_decode(m.name, m.a, m.big, m.movie, m.const, False, apt.Func) is None
    check(not bad, f'decoder: 600 damaged movies the same both ways ({errors} raise in Python, {refused} more '
                   'decoded by Python because Rust refused them)', ', '.join(bad[:5]))
    apt.use_native(True)
    ms = [apt.Movie(data=apt_data(m)) for m in movies.values()]
    singles = [(m.instructions(), m.dump_lines) for m in [apt.Movie(data=apt_data(m)) for m in movies.values()]]
    batched = [(m.instructions(), m.dump_lines) for m in apt.decoded(ms, chunk=3)]
    check(strict(singles, batched), 'decoded(): chunks of 3 give each movie its own records and lines')
    m = apt.Movie(data=apt_data(le))
    m.sink = lambda *a: None
    check(not m._native_ok() and m.dump() == apt.Movie(data=apt_data(le)).dump(), 'a Movie with a sink runs in Python')

    # the writer
    bad = []
    for label, mem in movies.items():
        r, n = both(lambda mem=mem, label=label: writes(mem, label))
        if not strict(r, n):
            bad.append(label)
    check(not bad, f'writer: every write of {len(movies)} built movies byte-identical to the reference', ', '.join(bad))
    bad = []
    for label, mem in movies.items():
        p = os.path.join(work, hashlib.md5(label.encode()).hexdigest() + '.res')
        open(p, 'wb').write(ag.write_bigf(mem))
        r, n = both(lambda p=p: ag.roundtrip(p, pack=True))
        if not strict(r, n) or r[0] != 'ok' or r[1][0]:
            bad.append(f'{label}: {r} / {n}'[:200])
    check(not bad, 'writer: roundtrip --pack of each the same problems (none) and notes as the reference', '; '.join(bad))
    pairs = [('source (LE)', 'target'), ('source (BE)', 'crosshair source (BE)'), ('crosshair', 'source (LE)'),
             ('grafted', 'crosshair added')]
    bad = []
    for a, b in pairs:
        r, n = both(lambda a=a, b=b: graft_all(movies[a], movies[b]))
        if not strict(r, n):
            bad.append(f'{a} -> {b}')
    check(not bad, f'writer: {len(pairs)} grafts (sprites, placements, a label alias) written three ways as the '
                   'reference', ', '.join(bad))
    r, n = both(lambda: (lambda m: (ag.add_crosshair(m, ag.Movie(xh_be, 'x'), 'c'), m.to_res(layout=ag.GRAFT_LAYOUT)))(
        ag.Movie(xh.write(), 'xh')))
    check(strict(r, n) and r[0] == 'ok', 'writer: crosshair-add written as the reference')
    rng = random.Random(9)
    bad, errs = [], 0
    for k in range(200):
        label = list(movies)[k % len(movies)]
        dname, da, dc = damaged(movies[label], rng)
        mem = [(dname + '.apt', da), (dname + '.const', dc)]
        r, n = both(lambda mem=mem: writes(mem, 'damaged'))
        errs += r[0] == 'error' or any(x[0] == 'error' for x in r[1])
        if not strict(r, n):
            bad.append(f'{label} #{k}')
    check(not bad, f'writer: 200 damaged movies written (or refused) as the reference ({errs} refused)', ', '.join(bad[:5]))
    import shutil
    shutil.rmtree(work, ignore_errors=True)


def game_movies() -> list[tuple[str, bytes]]:
    """(where, .res bytes) of each distinct movie the tools touch (module text)."""
    import menucommon as mc
    from bfbc2core import fbrb
    root = mc.find_opfor()
    seen: set[str] = set()
    out = []
    tops = [os.path.join(root, 'game', 'Dist', 'win32'), os.path.join(root, 'game', 'Package'),
            os.path.join(root, 'original', 'Package')]
    for top in tops:
        for dp, _, fs in os.walk(top):
            for f in sorted(fs):
                if not f.endswith('.fbrb'):
                    continue
                p = os.path.join(dp, f)
                ents = [e.name for e in fbrb.entries(p) if e.type == 'SwfMovie']
                if ents:
                    for name, data in fbrb.read(p, ents).items():
                        h = hashlib.md5(data).hexdigest()
                        if h not in seen:
                            seen.add(h)
                            out.append((f'{os.path.relpath(p, root)}:{name}', data))
    for dp, _, fs in os.walk(os.path.join(root, 'work', 'xbox_dlc')):
        for f in sorted(fs):
            if f.lower().endswith('.res'):
                p = os.path.join(dp, f)
                data = open(p, 'rb').read()
                h = hashlib.md5(data).hexdigest()
                if data[:4] == b'BIGF' and h not in seen:
                    seen.add(h)
                    out.append((os.path.relpath(p, root), data))
    return out


def game() -> None:
    movies = []
    for where, data in game_movies():
        try:
            mem = ag.read_bigf_bytes(data, where)
            ag.Movie(mem, where)
            movies.append((where, mem))
        except ag.AptError:
            pass                              # a .res that is not a movie
    work = tempfile.mkdtemp(prefix='apt_e3n_game_')
    try:
        bad, n_ins, n_lines = [], 0, 0
        t = {'py': 0.0, 'rs': 0.0}
        for where, mem in movies:
            parts = apt_data(mem)
            t0 = time.perf_counter()
            py = decode(parts, False)
            t1 = time.perf_counter()
            rs = decode(parts, True) if HAVE_RUST else py
            t['py'] += t1 - t0
            t['rs'] += time.perf_counter() - t1
            if py[0] == 'ok':
                n_ins += len(py[1][1])
                n_lines += len(py[1][2])
            if not strict(py, rs) or (HAVE_RUST and not strict(rs, decode(parts, True, batch=True))):
                bad.append(where)
        check(not bad, f'decoder: {len(movies)} movies ({n_ins} instructions, {n_lines} dump lines) the same in Python '
                       f'and Rust (Python {t["py"]:.1f} s, Rust {t["rs"]:.1f} s)', ', '.join(bad[:4]))
        bad, n_writes = [], 0
        t = {'ref': 0.0, 'new': 0.0}
        for k, (where, mem) in enumerate(movies):
            p = os.path.join(work, 'm.res')
            open(p, 'wb').write(ag.write_bigf(mem))

            def job(mem: Any = mem, p: str = p, k: int = k) -> Any:
                big = ag.Movie(mem).big
                nxt = next((m for _, m in movies[k + 1:] + movies[:k] if ag.Movie(m).big == big), None)
                return writes(mem, 'm'), ag.roundtrip(p, pack=True), outcome(lambda: graft_all(mem, nxt, 2)) if nxt else None
            ag.REFERENCE = True
            t0 = time.perf_counter()
            try:
                r = outcome(job)
            finally:
                ag.REFERENCE = False
            t1 = time.perf_counter()
            n = outcome(job)
            t['ref'] += t1 - t0
            t['new'] += time.perf_counter() - t1
            n_writes += len(r[1][0]) + 3 if r[0] == 'ok' else 0
            if not strict(r, n) or r[0] != 'ok':
                bad.append(where)
        check(not bad, f'writer: {len(movies)} movies, {n_writes} writes (each unchanged model 8 ways, two sprites grafted '
                       f'and written three ways) and roundtrip --pack the same as the reference (reference {t["ref"]:.1f} s, E3n '
                       f'{t["new"]:.1f} s)', ', '.join(bad[:4]))
    finally:
        import shutil
        shutil.rmtree(work, ignore_errors=True)


def game_tools() -> None:
    """The tools' outputs with the Rust decoder and with apt.py's Python: aptrefs xref (text and JSON), aptcheck's
    findings on every movie set's movies, bc2loc dump (English, Russian)."""
    import contextlib
    import io

    import aptcheck
    import aptrefs
    import bc2loc
    import menucommon as mc
    from bfbc2core import fbrb
    if not HAVE_RUST:
        print('skip the tools both ways: the Rust core is not built')
        return
    work = tempfile.mkdtemp(prefix='apt_e3n_tools_')
    try:
        outs: dict[bool, Any] = {}
        times: dict[bool, list[float]] = {}
        root = mc.find_opfor()
        for native in (False, True):
            apt.use_native(native)
            d = os.path.join(work, str(native))
            os.makedirs(d)
            t0 = time.perf_counter()
            with contextlib.redirect_stdout(io.StringIO()):
                code = aptrefs.main(['aptrefs.py', 'xref', '--out', os.path.join(d, 'x.txt'), '--json',
                                     os.path.join(d, 'x.json')])
            t1 = time.perf_counter()
            found = []
            for _key, rel in mc.MOVIE_SETS:
                by_bundle: dict[str, list[str]] = {}
                for _n, (bundle, entry) in mc.bundle_movies(mc.bundle_layers(root, rel)).items():
                    by_bundle.setdefault(bundle, []).append(entry)
                for bundle, entries in by_bundle.items():
                    ms = []
                    for entry, data in sorted(fbrb.read(bundle, entries).items()):
                        with contextlib.suppress(mc.MenuToolError):
                            ms.append(mc.movie_from_members(mc.parse_bigf(data, entry), entry))
                    for m in apt.decoded(ms):
                        c = aptcheck.Code(m)
                        found.append((m.name, len(c.recs), len(c.funcs), sorted(c.findings())))
            t2 = time.perf_counter()
            with contextlib.redirect_stdout(io.TextIOWrapper(io.BytesIO(), encoding='utf-8')):
                for lang in ('english', 'russian'):
                    bc2loc.main(['bc2loc.py', 'dump', '--lang', lang, '--out', os.path.join(d, f'loc_{lang}.txt')])
            t3 = time.perf_counter()
            outs[native] = (code, [open(os.path.join(d, f), 'rb').read() for f in sorted(os.listdir(d))], found)
            times[native] = [t1 - t0, t2 - t1, t3 - t2]
        same = outs[False] == outs[True]
        n_movies = len(outs[False][2])
        check(same and outs[False][0] == 0 and n_movies > 100,
              f'tools: aptrefs xref (text, JSON), aptcheck on {n_movies} movies ({sum(f[1] for f in outs[False][2])} '
              f'instructions), bc2loc dump (English, Russian) the same with Python and Rust decoding '
              f'(Python {"/".join(f"{x:.1f}" for x in times[False])} s, Rust {"/".join(f"{x:.1f}" for x in times[True])} s)')
    finally:
        import shutil
        shutil.rmtree(work, ignore_errors=True)


def main(argv: list[str]) -> int:
    quick()
    if '--game' in argv:
        game_tools()
        game()
    apt.use_native(True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
