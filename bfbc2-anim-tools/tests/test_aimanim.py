r"""aimanim.py: generated aim poses (AimAnimation) read, written and renumbered.

Without game data (quick tier):
- a made-up aim animation with all three bone kinds (fixed, rotation, moving) writes to the documented layout
  (header, size word, bone table, 16-byte alignment from 0x0c, block sizes) and reads back the same, in both byte
  orders (a big-endian copy built by swapping every word);
- renumbering maps the bone table and sorts it by the new index, each bone's data travelling with it; a bone
  missing from the map is an error; a file of the wrong size is refused.

With --game (game tier; each part skipped when its files are not on the PC):
- every AimAnimation of the game copy reads to its exact size, its rotations are unit quaternions and it writes
  back byte for byte;
- every AimAnimation of Battlefield 1943 (the Rising Sun workspace next to the suite) reads to its exact size and
  converts onto BFBC2's soldier (bone names kept through the renumbering).

  python test_aimanim.py [--game]
"""
import math
import os
import struct
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
sys.path.insert(0, os.path.join(HERE, '..'))
sys.path.insert(0, os.path.join(SUITE, 'core'))
import aimanim  # noqa: E402

N = 25


def sample():
    q = lambda k: (0.0, math.sin(k / 10), 0.0, math.cos(k / 10))                       # noqa: E731
    return {'addr': 0x08A255B0, 'poses': N, 'bones': [
        (0, 0, ((0.0, 0.0, 0.0, 1.0), (0.0, 0.0, 0.0, 1.0))),
        (2, 1, ((0.1, 0.2, 0.3, 0.0), [q(k) for k in range(N)])),
        (7, 2, [((k * 0.01, 1.4, 0.2, 1.0), q(k)) for k in range(N)]),
    ]}


def big(b):
    """The big-endian file of a PC one: every 32-bit word swapped, the byte table and pose count kept."""
    nb = struct.unpack_from('<I', b, 0x10)[0]
    out = bytearray(b)
    for o in range(0, len(b), 4):
        if 0x14 <= o < 0x1c + 2 * nb:
            continue
        out[o:o + 4] = b[o:o + 4][::-1]
    return bytes(out)


def units(check):
    a = sample()
    b = aimanim.write(a)
    data_at = 0x1c + 2 * 3
    data_at += (-(data_at - 0x0c)) % 16
    check(len(b) == data_at + 32 + (16 + 16 * N) + 32 * N, 'write: block sizes 32 / 16 + 16n / 32n after the aligned table')
    check(struct.unpack_from('<5I', b, 0) == (1, a['addr'], len(b) - 12, a['addr'] + 0x10, 3) and b[0x14] == N,
          'write: header (version, address, size, address + 0x10, bone count, pose count)')
    check(aimanim.parse(b) == aimanim.parse(big(b), True), 'parse: the same in both byte orders')
    r = aimanim.parse(b)
    check([(x[0], x[1]) for x in r['bones']] == [(0, 0), (2, 1), (7, 2)], 'parse: bone table read back')
    check(abs(r['bones'][2][2][3][0][0] - 0.03) < 1e-6, 'parse: a moving bone\'s pose 3 position read back')
    m = aimanim.renumbered(a, {0: 0, 2: 9, 7: 4})
    check([(x[0], x[1]) for x in m['bones']] == [(0, 0), (4, 2), (9, 1)], 'renumbered: mapped and sorted by new index')
    check(m['bones'][1][2] == a['bones'][2][2], 'renumbered: a bone\'s data travels with it')
    try:
        aimanim.renumbered(a, {0: 0, 2: 9})
        check(False, 'renumbered: a bone missing from the map is refused')
    except aimanim.AimError:
        check(True, 'renumbered: a bone missing from the map is refused')
    try:
        aimanim.parse(b + b'\0' * 16)
        check(False, 'parse: a file of the wrong size is refused')
    except aimanim.AimError:
        check(True, 'parse: a file of the wrong size is refused')
    check(aimanim.to_pc(big(b)) == b, 'to_pc: a big-endian file without a bone map converts to the PC file')


def unit_quats(a):
    qs = []
    for _, kind, block in a['bones']:
        qs += [block[1]] if kind == 0 else list(block[1]) if kind == 1 else [q for _, q in block]
    return max(abs(math.sqrt(sum(x * x for x in q)) - 1) for q in qs)


def game(check):
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-mesh-viewer'))
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools'))
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-bf1943-import'))
    try:
        import meshview
        import shaderdb
        ix = meshview.Index(shaderdb.find_opfor(None))
    except Exception as e:                                                  # noqa: BLE001 - no game copy
        print(f'SKIP game copy: {e}')
        ix = None
    if ix is not None:
        keys = sorted(k for k in ix.where if k.endswith('generatedaimanimation.res'))
        bad = [k for k in keys if not _pc_ok(ix.read(k))]
        check(keys and not bad, f'game copy: {len(keys)} aim animations read, unit rotations, written back byte for byte',
              ', '.join(bad[:3]))
    try:
        import bf1943import
        src = bf1943import.Source(bf1943import.default_source())
    except Exception as e:                                                  # noqa: BLE001 - no BF1943 workspace
        print(f'SKIP BF1943: {e}')
        return
    imp = bf1943import.Importer(src)
    keys = sorted(k for k, (_, e) in src.entries.items() if e['type'] == 'AimAnimation')
    bad = []
    for k in keys:
        try:
            out, rec = imp.convert(k)
            a, pc = aimanim.parse(src.data(k), True), aimanim.parse(out)
            if len(a['bones']) != len(pc['bones']) or unit_quats(pc) > 1e-4:
                bad.append(k)
        except Exception:                                                   # noqa: BLE001 - listed as a failure
            bad.append(k)
    check(keys and not bad, f'BF1943: {len(keys)} aim animations converted onto BFBC2\'s soldier', ', '.join(bad[:3]))


def _pc_ok(b):
    try:
        a = aimanim.parse(b)
        return unit_quats(a) < 1e-4 and aimanim.write(a) == b
    except aimanim.AimError:
        return False


def main(argv):
    fails = []

    def check(ok, what, detail=''):
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
        if not ok:
            fails.append(what)

    units(check)
    if '--game' in argv:
        game(check)
    return 1 if fails else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
