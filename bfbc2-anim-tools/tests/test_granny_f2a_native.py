r"""Round F2a: Granny's writing side in the Rust core against its Python reference - Granny.to_bytes (the CRC),
shift_position (constant, offset and re-quantized ramp curves, float controls), set_orientations and add_tracks
(_Graft: objects, type definitions and strings copied from a donor, the sections laid out again).

Compared both ways (granny.use_native on: the Rust core reads and computes, off: the Python reader and writer): the
reports, the resource's bytes after every change and to_bytes, add_tracks' files, or the same error.

  python test_granny_f2a_native.py          quick: every animation of the WW2 rifles (M1 Garand, its rifle grenade,
                                            Type 5 and its rifle grenade): each track's position curve moved by a
                                            vector (with and without ramp, and to a rest), each quantized orientation
                                            curve rewritten, five tracks added onto it from the Garand's undeploy;
                                            damaged copies; the CRC of built resources
  python test_granny_f2a_native.py --game   + every GrannyAnimation of the game copy the same way; timings

The module is required (bfbc2core.native): without it this test fails with the error saying how to get it.
"""
import hashlib
import os
import random
import re
import struct
import sys
import time
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
for p in (SUITE, os.path.join(SUITE, 'tests')):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('anim-tools', 'core', 'mesh-viewer')
import granny  # noqa: E402
from bfbc2core import native, paths  # noqa: E402
from f0_parity import Checks, outcome, stdout_utf8  # noqa: E402
from f2a_parity import Tally, bundle_entries, damaged, watch, watched  # noqa: E402

N = native.require('granny.py writing (round F2a)', 'Granny')
ANIMS = re.compile(r'animations/weapons/handheld/(m1garand|m1garand_rgl|beach/jap_type5|beach/jap_type5_rgl)/.*\.res$')
DONOR = 'animations/weapons/handheld/m1garand/3p/standundeploy.res'
# the position formats shift_position moves (the others raise: unsupported - checked by unsupported())
MOVABLE = ('D3Constant32f', 'D3K16uC16u', 'D3K8uC8u', 'D3I1K8uC8u', 'D3I1K16uC16u', 'D3I1K32fC32f', 'DaK32fC32f')


def edits(b: bytes, seed: int) -> list[Any]:
    """Every edit of one animation, in order, with what each gives: the reports, the bytes after each (a SHA-1),
    to_bytes."""
    rng = random.Random(seed)
    out: list[Any] = []
    g = granny.Granny(b, 'anim')
    for an in granny.animations(g):
        for tg in an['TrackGroups']:
            for tr in tg['TransformTracks']:
                cd = tr['PositionCurve']['CurveData']
                if cd and cd.get('__type__', '').endswith(MOVABLE):
                    delta = [rng.uniform(-0.5, 0.5) for _ in range(3)]
                    if rng.random() < 0.2:
                        delta[rng.randrange(3)] = 0.0
                    out.append(outcome(granny.shift_position, g, cd, delta, rng.random() < 0.5))
                    out.append(hashlib.sha1(g.data).hexdigest())
                oc = tr['OrientationCurve']['CurveData']
                if oc and oc.get('__type__', '').endswith(('D4nK16uC15u', 'D4nK8uC7u')):
                    n = len(oc['KnotsControls']) // 4
                    quats = [[rng.uniform(-1, 1) for _ in range(4)] for _ in range(n)]
                    out.append(outcome(granny.set_orientations, g, oc, quats))
                    out.append(hashlib.sha1(g.data).hexdigest())
    out.append(outcome(g.to_bytes))
    names = [tr['Name'] for an in granny.animations(granny.Granny(b)) for tg in an['TrackGroups'] for tr in tg['TransformTracks']
             if (tr['PositionCurve']['CurveData'] or {}).get('__type__', '').endswith(MOVABLE)]
    if names:
        out.append(outcome(granny.shift, granny.Granny(b), rng.choice(names), '0.1,0.2,0.3', None, True))
    return out


def unsupported(b: bytes) -> list[Any]:
    """shift_position on every position curve of a format it does not move, set_orientations on every other
    orientation curve: the same refusals."""
    out: list[Any] = []
    g = granny.Granny(b, 'anim')
    for an in granny.animations(g):
        for tg in an['TrackGroups']:
            for tr in tg['TransformTracks']:
                cd = tr['PositionCurve']['CurveData']
                if cd and not cd.get('__type__', '').endswith(MOVABLE):
                    out.append(outcome(granny.shift_position, g, cd, [0.1, 0.2, 0.3], False))
                oc = tr['OrientationCurve']['CurveData']
                if oc and not oc.get('__type__', '').endswith(('D4nK16uC15u', 'D4nK8uC7u')):
                    out.append(outcome(granny.set_orientations, g, oc, []))
    return out


def adds(b: bytes, donor: bytes, seed: int, clash: bool = False) -> Any:
    """add_tracks onto b: copies of three donor tracks, a constant and an identity (and a clash)."""
    rng = random.Random(seed)
    gd = granny.Granny(donor)
    dt = granny.animations(gd)[0]['TrackGroups'][0]['TransformTracks']
    pick = rng.sample(dt, min(3, len(dt)))
    spec: list[dict[str, Any]] = [{'name': f'zz_copy_{t["Name"]}', 'position': ('copy', t['PositionCurve']['CurveData']),
                                   'orientation': ('copy', t['OrientationCurve']['CurveData'])} for t in pick]
    extra: list[dict[str, Any]] = [
        {'name': 'aa_const', 'position': ('constant', (1.0, 2.0, 3.0)), 'orientation': ('constant', (0.0, 0.0, 0.6, 0.8))},
        {'name': 'mm_identity', 'position': None, 'orientation': None},
        {'name': 'mm_other', 'position': ('other',), 'orientation': ('constant', (1.0, 2.0))}]
    spec += extra[:2]
    if clash:
        spec = (extra[2:] if seed % 2 else spec[-1:]) * 2 + [dict(spec[0], name='x')]
    return granny.add_tracks(b, spec, donor)


def agree(rs: Any, py: Any) -> bool:
    """The same outcome both ways; or a resource the Rust core's Granny refuses to read (a GrannyError saying why,
    GRANNY-EDIT-2) where the Python reader fails too (its own exception): a refusal both ways."""
    if rs == py:
        return True
    if refused(rs) and rs[1] == 'GrannyError' and refused(py) and py[1] != 'GrannyError':
        REFUSALS[0] += 1
        return True
    return False


REFUSALS = [0]


def refused(o: Any) -> bool:
    """An outcome that is an error."""
    return isinstance(o, tuple) and bool(o) and o[0] == 'error'


def both(t: Tally, label: str, fn: Any, *args: Any) -> list[float]:
    times, res = [], []
    native = True
    for on in (True, False):
        granny.use_native(on)
        t0 = time.perf_counter()
        try:
            if on:
                got, native = watched(lambda: outcome(fn, *args))
                res.append(got)
            else:
                res.append(outcome(fn, *args))
        finally:
            granny.use_native(True)
        times.append(time.perf_counter() - t0)
    ok = agree(res[0], res[1])
    t.add(ok, label if ok else f'{label}: {repr(res[0])[:100]} / {repr(res[1])[:100]}', res[1][0] == 'error', native)
    return times


def built_crc(t: Tally) -> None:
    """to_bytes (the Granny CRC recomputed) of resources cut short and of headers whose CRC range runs past the
    data, both ways: the same bytes or the same refusal."""
    sys.path.insert(0, HERE)
    import test_granny_native as tn
    base = tn.built('<')
    rng = random.Random(0xC2C)
    for i in range(300):
        b = bytearray(base)
        g0 = len(b) - len(base) + base.find(granny.MAGIC)
        if i % 3 == 0:
            struct.pack_into('<I', b, g0 + 36, rng.randrange(0, 2 * len(b)))        # total
        elif i % 3 == 1:
            struct.pack_into('<I', b, g0 + 40, rng.randrange(1 << 32))             # an old CRC
        else:
            del b[rng.randrange(g0 + 44, len(b)):]
        both(t, f'crc {i}', lambda d=bytes(b): granny.Granny(d).to_bytes())


def main(argv: list[str]) -> int:
    stdout_utf8()
    game = '--game' in argv
    c = Checks()
    watch(granny, 'shift_position_reference', 'set_orientations_reference', 'add_tracks_reference')
    c.check(granny.use_native(True) == 'rust', 'the Rust core runs granny.py\'s reader and writer')
    tc = Tally()
    built_crc(tc)
    c.check(not tc.ndiff, f'the Granny CRC of built resources: {tc.text()}', '; '.join(tc.differ))
    import meshview
    import shaderdb
    try:
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                       # noqa: BLE001
        print(f'skip the game copy ({e})')
        return c.done()
    donor = ix.read(DONOR)
    if game:
        opfor = paths.sibling(paths.OPFOR_DIR, suite=SUITE)
        items: Any = ((nm, b) for nm, _, b in bundle_entries(os.path.join(opfor, 'game', '**', '*.fbrb'), {'GrannyAnimation'}))
    else:
        names = sorted(n for n in ix.where if ANIMS.search(n))
        items = [(n, ix.read(n)) for n in names]
    te, ta, td, tu = Tally(), Tally(), Tally(), Tally()
    t_edit = [0.0, 0.0]
    t_add = [0.0, 0.0]
    count = 0
    for k, (nm, b) in enumerate(items):
        if b is None:
            continue
        count += 1
        if game and count % 250 == 0:
            print(f'  {count:,} animations', flush=True)
        te_ = both(te, nm, edits, b, k)
        t_edit[0] += te_[0]
        t_edit[1] += te_[1]
        a = both(ta, nm, adds, b, donor, k)
        t_add[0] += a[0]
        t_add[1] += a[1]
        both(tu, nm + ' clash', adds, b, donor, k, True)
        both(tu, nm + ' unsupported', unsupported, b)
        rng = random.Random(k)
        # the game tier cuts copies short only: a changed count makes the Python reader build lists of millions of
        # numbers (the Rust reader refuses past 2^24 items and hands over), gigabytes over the corpus
        cuts = [b[:rng.randrange(len(b) + 1)] if game else damaged(b, rng) for _ in range(2)]
        both(td, nm + ' damaged', edits, cuts[0], k)
        both(td, nm + ' damaged base', adds, cuts[1], donor, k)
    c.check(not te.ndiff, f'{count:,} animations: every position curve moved, every quantized orientation curve '
            f'rewritten, to_bytes, both ways: {te.text()}', '; '.join(te.differ))
    c.check(not ta.ndiff, f'{count:,} animations: tracks added from the Garand\'s undeploy both ways: {ta.text()}',
            '; '.join(ta.differ))
    c.check(not td.ndiff, f'damaged animations: {td.text()}', '; '.join(td.differ))
    c.check(not tu.ndiff, f'curves shift_position does not move, track specs add_tracks refuses: {tu.text()}',
            '; '.join(tu.differ))
    print(f'{REFUSALS[0]} damaged resources the Rust core refuses to read (a GrannyError saying why) where the Python '
          'reader failed with its own exception')
    print(f'timing: the edits Python {t_edit[1]:.2f} s, Rust {t_edit[0]:.2f} s; add_tracks Python {t_add[1]:.2f} s, '
          f'Rust {t_add[0]:.2f} s (each with its reads)')
    return c.done()


if __name__ == '__main__':
    sys.exit(main(sys.argv))
