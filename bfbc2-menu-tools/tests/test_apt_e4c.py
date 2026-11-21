r"""Round E4c (rust/BENCHMARKS.md "E4c") against its references, both ways in one process:

- aptgraft's model in the Rust core (NativeMovie over bc2_native.AptModel; aptgraft.use_native) against the Python
  model (use_native(False)): every write of an unchanged model (keep, pack, either byte order, dead data off),
  roundtrip --pack, grafts, label aliases, mode aliases, crosshair-add - the same bytes, problems and notes, or the
  same exception (type and text);
- the native model's object rules: one object per node, shells filled on first read or write, edits of filled and
  unfilled nodes, new and shared nodes, values the Rust cannot hold (the Python then runs: the same bytes or the
  same exception), the cache view, reads that parse more;
- aptcheck's rules (check_many) and aptrefs' scan (refs_many) in Rust against Code / movie_refs.

quick: test_aptgraft.py's built movies (as test_apt_e3n.py makes them), 300 damaged copies.
--game: the 133 distinct movies of round E3n (the game copy's SwfMovies, OPFOR's pristine menus, the Xbox HUD
copies of work\xbox_dlc; read only).

  python test_apt_e4c.py [--game]
"""
import os
import random
import sys
import tempfile
import time
from typing import Any, cast

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.dirname(HERE)
SUITE = os.path.dirname(ROOT)
sys.path.insert(0, SUITE)
sys.path.insert(0, HERE)
import suite_paths  # noqa: E402

suite_paths.use('menu-tools', 'apt', 'core')
import apt  # noqa: E402
import aptcheck  # noqa: E402
import aptgraft as ag  # noqa: E402
import aptrefslib as aptrefs  # noqa: E402
import test_apt_e3n as T  # noqa: E402
import test_aptgraft as tg  # noqa: E402
from aptgraft import Ins, Ref, Str, Words  # noqa: E402

failed = 0
HAVE = ag.use_native(True) == 'rust'


def check(ok: object, what: str, detail: str = '') -> None:
    global failed
    failed += not ok
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


def both(fn: Any) -> tuple[Any, Any]:
    """fn() with the Python model, then with the Rust model."""
    ag.use_native(False)
    try:
        py = T.outcome(fn)
    finally:
        ag.use_native(True)
    return py, T.outcome(fn)


def edits(members: list[tuple[str, bytes]]) -> Any:
    """A label alias on the first frame label, a mode alias, a mode case: each result, then three writes."""
    m = ag.Movie(members, 'e4c')
    got = []
    labels = [ag.text(it.vals[1]) for f in range(len(m.frames().vals) // 2) for it in m.frame_items(f)
              if it.tag == 'label']
    if labels:
        got.append(T.outcome(lambda: ag.add_label_alias(m, labels[0], labels[0] + '_e4c')))
    got.append(T.outcome(lambda: ag.add_mode_alias(m, 'onslaught', 'conquest')))
    got.append(T.outcome(lambda: ag.add_mode_case(m, 'getMapDesc', 'onslaught', 'KEY')))
    return got, m.write(layout=ag.GRAFT_LAYOUT), m.write(layout='pack'), m.write(order='<' if m.big else '>')


def crosshair(dst: list[tuple[str, bytes]], src: list[tuple[str, bytes]]) -> Any:
    d, s = ag.Movie(dst, 'dst'), ag.Movie(src, 'src')
    ids = [k for k, v in ag.crosshair_table(s).items() if v['start'] is not None]
    out = []
    for cid in ids[:2]:
        out.append(T.outcome(lambda cid=cid: sorted(ag.add_crosshair(d, s, cid).items(), key=str)))
    return [repr(x) for x in out], d.to_res(layout=ag.GRAFT_LAYOUT)


def scans(members: list[tuple[str, bytes]]) -> tuple[bool, bool]:
    """aptcheck's and aptrefs' Rust scans against their Python, on one movie (decoded by apt.py's own reader where
    the Rust decoder refuses it)."""
    m = T.apt_data(members)
    try:
        am = apt.Movie(data=m)
    except Exception:
        return True, True
    py_c = T.outcome(lambda: aptcheck.check_many_python([apt.Movie(data=m)]))
    rs_c = T.outcome(lambda: aptcheck.check_many([apt.Movie(data=m)]))
    py_r = T.outcome(lambda: aptrefs.refs_many_python([apt.Movie(data=m)]))
    rs_r = T.outcome(lambda: aptrefs.refs_many([am]))
    return T.strict(py_c, rs_c), py_r == rs_r and T.strict(py_r, rs_r)


def rt_both(members: list[tuple[str, bytes]], work: str, k: int) -> tuple[Any, Any]:
    p = os.path.join(work, f'{k}.res')
    with open(p, 'wb') as f:
        f.write(ag.write_bigf(members))
    return T.outcome(lambda: ag.roundtrip_python(p, pack=True)), T.outcome(lambda: next(ag.roundtrip_many([p], True)))


def objects() -> None:
    """The native model's object rules."""
    src, _imp, sid, spid = tg.build_source()
    le = src.write()
    m = ag.Movie(le, 'src')
    check(type(m).__name__ == 'NativeMovie' and m.char(sid) is m.char(sid) and m.chartable is m.chartable,
          'objects: a NativeMovie; one object per node')
    sh = m.char(spid)
    shell = type(sh) is not Words and isinstance(sh, Words)
    sh.vals                                  # noqa: B018  (read: filled)
    check(shell and type(sh) is Words and sh in cast(ag.NativeMovie, m)._live.values(), 'objects: a shell until read, then its plain class')

    def mutate(mk: Any) -> Any:
        mv = mk()
        part = mv.geo_entry(sid)[0].vals[mv.geo_entry(sid)[1] + 2].node.vals[0].node
        verts = part.vals[2].node            # an unfilled shell: written before it is read
        verts.vals = [ag.f32(1.0)] * 12
        it = next(i for i in mv.frame_items(0) if i.tag == 'place' and isinstance(i.vals[13], Ref))
        it.vals[13].node.text = b'a much longer instance name'     # an unfilled string grown: it moves
        lbl = Words('apt', None, [2, Ref(Str('apt', None, b'new')), 0, 0], 'label')
        mv.add_item(0, lbl)
        mv.add_item(0, lbl)                  # one new node in two places
        return mv.write(), mv.write(layout='pack'), mv.write(order='>'), mv.leftovers()
    py, rs = both(lambda: mutate(lambda: ag.Movie(le, 'src')))
    check(T.strict(py, rs) and py[0] == 'ok', 'objects: shells written before read, a grown node, a node in two places')

    def unplaced(mk: Any) -> Any:
        mv = mk()
        mv.char(spid).at = None              # a node read from the file made new: the dead-data pass cannot clear it
        return mv.write()
    py, rs = both(lambda: unplaced(lambda: ag.Movie(le, 'src')))
    check(T.strict(py, rs) and py[0] == 'error', 'objects: a read node offset taken away: the same exception', str(rs))

    def shared(mk: Any) -> Any:
        a, b = mk(), mk()
        b.add_item(0, a.frame_items(0)[0])  # another movie's node (Python shares it)
        return b.write(), b.write(layout='pack')
    py, rs = both(lambda: shared(lambda: ag.Movie(le, 'src')))
    check(T.strict(py, rs) and py[0] == 'ok', 'objects: another movie\'s node put into this one')
    for what, bad in (('a str in a words node', 'x'), ('a negative instruction argument', -1),
                      ('an argument past 32 bits', 1 << 40), ('a float argument', 1.5), ('a bool', True)):
        def broken(bad: Any = bad, what: str = what) -> Any:
            mv = ag.Movie(le, 'src')
            if what.startswith('a str'):
                mv.char(sid).vals[2] = bad
            else:
                code = next(n for n in ag.code_nodes(mv) if any(i.op == 0xB7 for i in n.ins))
                next(i for i in code.ins if i.op == 0xB7).args[0] = bad
            return mv.write()
        py, rs = both(broken)
        check(T.strict(py, rs), f'objects: {what}: the same answer as the Python model', f'{py} / {rs}')

    def branch(mk: Any) -> Any:
        mv = mk()
        code = next(n for n in ag.code_nodes(mv) if any(i.op in ag.BRANCH for i in n.ins))
        next(i for i in code.ins if i.op in ag.BRANCH).target = Ins(0)    # a target outside the stream
        return mv.write()
    py, rs = both(lambda: branch(lambda: ag.Movie(le, 'src')))
    check(T.strict(py, rs) and py[0] == 'error', 'objects: a branch to nothing raises as the Python model', str(rs))
    mv = ag.Movie(le, 'src')
    code_at = next(k for k in mv.cache if k[2] == 'c')
    check(code_at in mv.cache and ('apt', -1, 'c') not in mv.cache and len(mv.cache) == len(list(mv.cache))
          and mv.code(code_at[1]) is mv.cache[code_at], 'objects: the cache view and code() of a read stream')
    py, rs = both(lambda: (lambda mv: (type(mv.string('apt', 1)).__name__, mv.write()))(ag.Movie(le, 'src')))
    check(T.strict(py, rs), 'objects: a read that parses more runs the Python model, the same answer')
    mv = ag.Movie(le, 'src')
    mv.string('apt', 1)
    check(type(mv) is ag.Movie, 'objects: ... and the movie is the Python model from then on')


def quick() -> None:
    if not HAVE:
        print('skip: the Rust core is not built (aptgraft runs its Python model)')
        return
    objects()
    src, _imp, _sid, spid = tg.build_source()
    le = src.write()
    be = ag.Movie(le, 'src').write(order='>')
    xh = tg.crosshair_movie('hud_crosshair', ['a', 'b'], 0x129, image=True)
    xh_src = tg.crosshair_movie('src_crosshair', ['c'], 0x29)
    xh_be = ag.Movie(xh_src.write(), 'x').write(order='>')
    movies = {'source (LE)': le, 'source (BE)': be, 'target': tg.target().write(), 'crosshair': xh.write(),
              'crosshair source (BE)': xh_be, 'floats': T.float_movie()}
    work = tempfile.mkdtemp(prefix='apt_e4c_')
    try:
        bad = [k for k, mem in movies.items() if not T.strict(*both(lambda mem=mem, k=k: T.writes(mem, k)))]
        check(not bad, f'model: every write of {len(movies)} built movies the same as the Python model', ', '.join(bad))
        bad = [k for n, (k, mem) in enumerate(movies.items()) if not T.strict(*rt_both(mem, work, n))]
        check(not bad, 'model: roundtrip --pack of each the same', ', '.join(bad))
        pairs = [('source (LE)', 'target'), ('source (BE)', 'crosshair source (BE)'), ('crosshair', 'source (LE)')]
        bad = [f'{a}->{b}' for a, b in pairs if not T.strict(*both(lambda a=a, b=b: T.graft_all(movies[a], movies[b])))]
        check(not bad, f'model: {len(pairs)} grafts written three ways the same', ', '.join(bad))
        bad = [k for k, mem in movies.items() if not T.strict(*both(lambda mem=mem: edits(mem)))]
        check(not bad, 'model: label alias, mode alias, mode case and their writes the same', ', '.join(bad))
        py, rs = both(lambda: crosshair(xh.write(), xh_be))
        check(T.strict(py, rs) and py[0] == 'ok', 'model: crosshair-add the same')
        rng = random.Random(11)
        bad, errs = [], 0
        for k in range(300):
            label = list(movies)[k % len(movies)]
            name, a, c = T.damaged(movies[label], rng)
            mem = [(name + '.apt', a), (name + '.const', c)]
            py, rs = both(lambda mem=mem: T.writes(mem, 'damaged'))
            r1, r2 = rt_both(mem, work, 1000 + k)
            errs += py[0] == 'error' or any(x[0] == 'error' for x in py[1])
            if not (T.strict(py, rs) and T.strict(r1, r2)):
                bad.append(f'{label} #{k}')
        check(not bad, f'model: 300 damaged movies written and round-tripped as the Python model ({errs} refused)',
              ', '.join(bad[:5]))
        bad = []
        for label, mem in movies.items():
            sc, sr = scans(mem)
            if not (sc and sr):
                bad.append(label)
        rng = random.Random(12)
        for k in range(200):
            label = list(movies)[k % len(movies)]
            name, a, cst = T.damaged(movies[label], rng)
            sc, sr = scans([(name + '.apt', a), (name + '.const', cst)])
            if not (sc and sr):
                bad.append(f'{label} #{k}')
        check(not bad, 'scans: aptcheck check_many and aptrefs refs_many the same as Code / movie_refs (built, 200 '
                       'damaged)', ', '.join(bad[:5]))
    finally:
        import shutil
        shutil.rmtree(work, ignore_errors=True)


def game() -> None:
    movies = []
    for where, data in T.game_movies():
        try:
            mem = ag.read_bigf_bytes(data, where)
            ag.Movie(mem, where)
            movies.append((where, mem))
        except ag.AptError:
            pass
    work = tempfile.mkdtemp(prefix='apt_e4c_game_')
    t = {'py': 0.0, 'rs': 0.0}

    def timed(fn: Any) -> tuple[Any, Any]:
        ag.use_native(False)
        t0 = time.perf_counter()
        try:
            py = T.outcome(fn)
        finally:
            ag.use_native(True)
        t1 = time.perf_counter()
        rs = T.outcome(fn)
        t['py'] += t1 - t0
        t['rs'] += time.perf_counter() - t1
        return py, rs
    try:
        bigs = [ag.Movie(m).big for _, m in movies]
        crosshairs = [k for k, (_, m) in enumerate(movies) if T.outcome(lambda m=m: ag.crosshair_table(ag.Movie(m)))[0] == 'ok']
        bad, n_writes = [], 0
        for k, (where, mem) in enumerate(movies):
            nxt = next((m for j, (_, m) in enumerate(movies[k + 1:] + movies[:k]) if bigs[(k + 1 + j) % len(movies)] == bigs[k]),
                       None)
            ok = True
            r = timed(lambda mem=mem: T.writes(mem, 'm'))
            ok &= T.strict(*r) and r[0][0] == 'ok'
            n_writes += len(r[0][1]) + 3 if r[0][0] == 'ok' else 0
            p = os.path.join(work, 'm.res')
            with open(p, 'wb') as f:
                f.write(ag.write_bigf(mem))
            t0 = time.perf_counter()
            r1 = T.outcome(lambda p=p: ag.roundtrip_python(p, pack=True))
            t1 = time.perf_counter()
            r2 = T.outcome(lambda p=p: next(ag.roundtrip_many([p], True)))
            t['py'] += t1 - t0
            t['rs'] += time.perf_counter() - t1
            ok &= T.strict(r1, r2) and r1[0] == 'ok' and not r1[1][0]
            if nxt is not None:
                r = timed(lambda mem=mem, nxt=nxt: T.graft_all(mem, nxt, 2))
                ok &= T.strict(*r)
                n_writes += 3
            r = timed(lambda mem=mem: edits(mem))
            ok &= T.strict(*r)
            n_writes += 3
            if k in crosshairs:
                for j in crosshairs:
                    if j != k:
                        r = timed(lambda mem=mem, j=j: crosshair(mem, movies[j][1]))
                        ok &= T.strict(*r)
                        n_writes += 1
            c, rr = scans(mem)
            ok &= c and rr
            if not ok:
                bad.append(where)
        check(not bad, f'game: {len(movies)} movies - {n_writes} writes (each unchanged model 8 ways, grafts, label / '
                       f'mode aliases and mode cases, crosshair-add between the {len(crosshairs)} crosshair movies), '
                       f'roundtrip --pack, aptcheck and aptrefs the same as the Python (Python {t["py"]:.1f} s, Rust '
                       f'{t["rs"]:.1f} s)', ', '.join(bad[:4]))
    finally:
        import shutil
        shutil.rmtree(work, ignore_errors=True)


def main(argv: list[str]) -> int:
    quick()
    if '--game' in argv and HAVE:
        game()
    ag.use_native(True)
    print('all passed' if not failed else f'{failed} check(s) failed')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
