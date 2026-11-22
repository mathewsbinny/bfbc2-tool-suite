r"""granny.curve_keys and granny.sample on the Rust core (round PR1b-2-rest: bc2_convert::granny_edit) against the
retired Python, kept here as the reference (curve_keys_reference, sample_reference: granny.py's own before the round,
unchanged):

- built curves of every format the reader takes (constant, float knots and controls, the quantized positions with
  8- and 16-bit knots, the one-control D3I1 forms, the quantized orientations with every selector), random values,
  empty ones, identity and none; a format it does not read and fields missing raise GrannyError on both;
- each curve sampled at its keys, between them, before and after (quaternions: the nearer of q and -q, unit length);
- with --game: every curve of the WW2 tools' animations (the rifles', the soldiers') and of every 8th other animation
  of the game copy's stock bundles, the same keys and the same samples, value for value (floats compared by their bits).

  python test_granny_curves.py [--game]
"""
import math
import os
import random
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('anim-tools')
import granny  # noqa: E402

# ------------------------------------------------------------------------------------------------ the reference

SCALE_TABLE = granny.SCALE_TABLE
OFFSET_TABLE = granny.OFFSET_TABLE


def _trunc_scale(v: int) -> float:
    return float(struct.unpack('<f', struct.pack('<I', (v & 0xFFFF) << 16))[0])


def _quat(a: int, b: int, c: int, scales: list[float], offsets: list[float], top: int) -> tuple[float, ...]:
    i1 = ((b & top) >> (top.bit_length() - 2)) | ((c & top) >> (top.bit_length() - 1))
    i2, i3, i4 = (i1 + 1) & 3, (i1 + 2) & 3, (i1 + 3) & 3
    mask = top - 1
    q = [0.0] * 4
    q[i2] = (a & mask) * scales[i2] + offsets[i2]
    q[i3] = (b & mask) * scales[i3] + offsets[i3]
    q[i4] = (c & mask) * scales[i4] + offsets[i4]
    q[i1] = math.sqrt(max(0.0, 1 - q[i2] ** 2 - q[i3] ** 2 - q[i4] ** 2)) * (-1 if a & top else 1)
    return tuple(q)


def curve_keys_reference(cd: Any) -> list[tuple[Any, tuple[Any, ...]]]:
    """granny.curve_keys before round PR1b-2-rest."""
    if not cd:
        return []
    t = cd.get('__type__', '')
    if t.endswith('DaIdentity'):
        return []
    if t.endswith('Constant32f'):
        return [(0.0, tuple(cd['Controls']))]
    if t.endswith('DaK32fC32f'):
        k, c = cd['Knots'], cd['Controls']
        if not k:
            return []
        dim = len(c) // len(k)
        return [(k[i], tuple(c[i * dim:(i + 1) * dim])) for i in range(len(k))]
    if t.endswith('D3K16uC16u') or t.endswith('D3K8uC8u'):
        kc = cd['KnotsControls']
        n = len(kc) // 4
        s = _trunc_scale(cd['OneOverKnotScaleTrunc'])
        sc, of = cd['ControlScales'], cd['ControlOffsets']
        return [(kc[i] / s, tuple(kc[n + 3 * i + j] * sc[j] + of[j] for j in range(3))) for i in range(n)]
    if t.endswith(('D3I1K8uC8u', 'D3I1K16uC16u', 'D3I1K32fC32f')):
        kc = cd['KnotsControls']
        n = len(kc) // 2
        s = _trunc_scale(cd['OneOverKnotScaleTrunc']) if 'OneOverKnotScaleTrunc' in cd else 1.0
        sc, of = cd['ControlScales'], cd['ControlOffsets']
        return [(kc[i] / s, tuple(kc[n + i] * sc[j] + of[j] for j in range(3))) for i in range(n)]
    if t.endswith('D4nK16uC15u') or t.endswith('D4nK8uC7u'):
        kc = cd['KnotsControls']
        n = len(kc) // 4
        wide = t.endswith('D4nK16uC15u')
        top, unit = (0x8000, 1 / 32767) if wide else (0x80, 1 / 127)
        sel = cd['ScaleOffsetTableEntries']
        scales = [SCALE_TABLE[(sel >> (4 * k)) & 15] * unit for k in range(4)]
        offsets = [OFFSET_TABLE[(sel >> (4 * k)) & 15] for k in range(4)]
        s = cd['OneOverKnotScale']
        return [(kc[i] / s, _quat(kc[n + 3 * i], kc[n + 3 * i + 1], kc[n + 3 * i + 2], scales, offsets, top))
                for i in range(n)]
    raise granny.GrannyError(f'curve format {t} is not read')


def sample_reference(keys: Any, t: float, default: Any) -> Any:
    """granny.sample before round PR1b-2-rest."""
    if not keys:
        return default
    if t <= keys[0][0] or len(keys) == 1:
        return keys[0][1]
    for (t0, v0), (t1, v1) in zip(keys, keys[1:]):
        if t <= t1:
            f = (t - t0) / (t1 - t0) if t1 > t0 else 0
            if len(v0) == 4:
                if sum(a * b for a, b in zip(v0, v1)) < 0:
                    v1 = tuple(-x for x in v1)
                q = [a + (b - a) * f for a, b in zip(v0, v1)]
                n = math.sqrt(sum(x * x for x in q)) or 1.0
                return tuple(x / n for x in q)
            return tuple(a + (b - a) * f for a, b in zip(v0, v1))
    return keys[-1][1]


WW2 = ('m1garand', 'jap_type5', 'jap_type38', 'type100', 'type99', 'winchester', 'characters/')

# ------------------------------------------------------------------------------------------------ comparing

def bits(v: Any) -> Any:
    """Floats by their bits (-0.0 and NaNs told apart), ints as floats (the port's values are floats)."""
    if isinstance(v, (list, tuple)):
        return [bits(x) for x in v]
    if isinstance(v, (int, float)) and not isinstance(v, bool):
        return struct.pack('<d', float(v))
    return v


def outcome(fn: Any, *a: Any) -> Any:
    try:
        return ('ok', bits(fn(*a)))
    except (granny.GrannyError, KeyError, IndexError, ZeroDivisionError, TypeError):
        return ('raised',)


class Tally:
    def __init__(self) -> None:
        self.curves = self.samples = 0
        self.fails: list[str] = []

    def curve(self, cd: Any, what: str) -> None:
        self.curves += 1
        a, b = outcome(curve_keys_reference, cd), outcome(granny.curve_keys, cd)
        if a != b:
            self.fails.append(f'{what}: curve_keys {a[0]} / {b[0]}')
            return
        if a[0] != 'ok':
            return
        keys = curve_keys_reference(cd)
        times = [k[0] for k in keys]
        ts = [-1.0, 0.0] + times + [(x + y) / 2 for x, y in zip(times, times[1:])] + [(times[-1] if times else 0) + 1]
        for t in ts[:: max(1, len(ts) // 24)]:            # at most ~24 samples a curve (each crosses into Rust)
            self.samples += 1
            ra, rb = bits(sample_reference(keys, t, (0.0, 0.0, 0.0))), bits(granny.sample(keys, t, (0.0, 0.0, 0.0)))
            if ra != rb:
                self.fails.append(f'{what}: sample at {t}')
                return


def built(rng: random.Random, tally: Tally) -> None:
    f = lambda: rng.uniform(-3, 3)                         # noqa: E731
    for k in range(400):
        n = rng.choice([0, 1, 2, 3, 7, 30])
        sel = rng.randrange(1 << 16)
        tr = rng.randrange(1, 1 << 16)
        forms: list[dict[str, Any]] = [
            {'__type__': 'CurveDataHeader_D3Constant32f', 'Controls': [f() for _ in range(3)]},
            {'__type__': 'CurveDataHeader_D4Constant32f', 'Controls': [f() for _ in range(4)]},
            {'__type__': 'CurveDataHeader_DaK32fC32f', 'Knots': sorted(rng.random() for _ in range(n)),
             'Controls': [f() for _ in range(n * rng.choice([3, 4]))]},
            {'__type__': 'CurveDataHeader_D3K16uC16u', 'OneOverKnotScaleTrunc': tr,
             'ControlScales': [f() for _ in range(3)], 'ControlOffsets': [f() for _ in range(3)],
             'KnotsControls': sorted(rng.randrange(65536) for _ in range(n)) + [rng.randrange(65536) for _ in range(3 * n)]},
            {'__type__': 'CurveDataHeader_D3K8uC8u', 'OneOverKnotScaleTrunc': tr,
             'ControlScales': [f() for _ in range(3)], 'ControlOffsets': [f() for _ in range(3)],
             'KnotsControls': sorted(rng.randrange(256) for _ in range(n)) + [rng.randrange(256) for _ in range(3 * n)]},
            {'__type__': 'CurveDataHeader_D3I1K16uC16u', 'OneOverKnotScaleTrunc': tr,
             'ControlScales': [f() for _ in range(3)], 'ControlOffsets': [f() for _ in range(3)],
             'KnotsControls': sorted(rng.randrange(65536) for _ in range(n)) + [rng.randrange(65536) for _ in range(n)]},
            {'__type__': 'CurveDataHeader_D3I1K32fC32f', 'ControlScales': [f() for _ in range(3)],
             'ControlOffsets': [f() for _ in range(3)],
             'KnotsControls': sorted(rng.random() for _ in range(n)) + [f() for _ in range(n)]},
            {'__type__': 'CurveDataHeader_D4nK16uC15u', 'ScaleOffsetTableEntries': sel, 'OneOverKnotScale': rng.uniform(1, 60),
             'KnotsControls': sorted(rng.randrange(65536) for _ in range(n)) + [rng.randrange(65536) for _ in range(3 * n)]},
            {'__type__': 'CurveDataHeader_D4nK8uC7u', 'ScaleOffsetTableEntries': sel, 'OneOverKnotScale': rng.uniform(1, 60),
             'KnotsControls': sorted(rng.randrange(256) for _ in range(n)) + [rng.randrange(256) for _ in range(3 * n)]},
        ]
        for cd in forms:
            tally.curve(cd, f'built {k} {cd["__type__"]} n={n}')
    edges: tuple[Any, ...] = ({}, None, {'__type__': 'CurveDataHeader_DaIdentity'}, {'__type__': 'CurveDataHeader_D3I1K16uC16u'},
               {'__type__': 'CurveDataHeader_Unknown', 'Controls': [1.0]}, {'__type__': 'CurveDataHeader_D3Constant32f'},
               {'__type__': 'CurveDataHeader_D3K8uC8u', 'OneOverKnotScaleTrunc': 0, 'ControlScales': [1, 1, 1],
                'ControlOffsets': [0, 0, 0], 'KnotsControls': [1, 2, 3, 4]})
    for edge in edges:
        tally.curve(edge, f'edge {edge}')


def real(tally: Tally) -> int:
    """Every animation curve of the game copy's stock bundles; the number of animations read."""
    suite_paths.use('mesh-viewer')
    import meshview
    import shaderdb
    try:
        ix = meshview.Index(shaderdb.find_opfor(None))
    except shaderdb.ShaderDbError:
        print('SKIP --game: no Project OPFOR BFBC2 game copy')
        return 0
    names = sorted(n for n in ix.where if n.startswith('animations/') and n.endswith('.res'))
    # the WW2 tools' own (the rifles', the soldiers'), and every 8th of the rest (all of them: hours in the reference)
    names = [n for i, n in enumerate(names) if i % 8 == 0 or any(k in n for k in WW2)]
    done = 0
    for i in range(0, len(names), 400):
        got = ix.read_many(names[i:i + 400])
        for n, b in got.items():
            if not b:
                continue
            try:
                g = granny.Granny(b, n)
                anims = granny.animations(g)
            except Exception:                       # noqa: BLE001 - not a Granny animation (an aim table ...)
                continue
            done += 1
            for an in anims:
                for tg in an.get('TrackGroups') or []:
                    for tr in tg.get('TransformTracks') or []:
                        for c in ('OrientationCurve', 'PositionCurve', 'ScaleShearCurve'):
                            cd = (tr.get(c) or {}).get('CurveData')
                            tally.curve(cd, f'{n} {tr.get("Name")} {c}')
    return done


def main(argv: list[str]) -> int:
    tally = Tally()
    built(random.Random(0x6E4), tally)
    print(f'built: {tally.curves} curves, {tally.samples} samples')
    if '--game' in argv:
        n = real(tally)
        print(f'with the game copy: {n} animations, {tally.curves} curves, {tally.samples} samples in all')
    for f in tally.fails[:20]:
        print('FAIL', f)
    print('all equal' if not tally.fails else f'{len(tally.fails)} differ')
    return 1 if tally.fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
