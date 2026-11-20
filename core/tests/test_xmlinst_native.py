r"""Round F2b (rust/BENCHMARKS.md, "F2b"): bfbc2core.xmlinst in the Rust core against its patterns
(instances_reference, field_ref_reference) - the same instances in the same order, the same references.

  python test_xmlinst_native.py          quick: 3,000 seeded XML texts (either attribute order, fields closed by />
                                         or >, missing closing tags, <instances> and <fieldx> look-alikes, duplicate
                                         attributes, non-ASCII text where the port refuses or not)
  python test_xmlinst_native.py --game   + every converted level partition and blueprint XML of Project OPFOR's
                                         work\levels (read only): every instance, with and without each type
                                         suffix NavKit asks, and the references its tools read

The module is required (the test fails without it, saying how to build it).
"""
import glob
import os
import random
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(HERE)), 'tests'))
from bfbc2core import native, xmlinst  # noqa: E402
from replay import Replay  # noqa: E402

# round RT2: the patterns' answers per text recorded (--record) and compared with once they were retired
REPLAY = Replay('f2b_xmlinst', HERE, [], canon=lambda v: v)

FAILS: list[str] = []
SUFFIXES = (None, 'Entity.AreaGeometryEntityData', '.AreaOrderEntityData', 'Entity.ReferenceObjectData',
            '.SoldierSpawnEntityData', '.VehicleSpawnEntityData', 'Terrain.TerrainSplinePointData', '')
NAMES = ('ReferencedObject', 'Vehicle', 'Mesh', 'Name', 'x')


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def _ref(owner, name):
    """A reference by name (once retired, the replay stub stands in for it)."""
    return getattr(owner, name)


def outcome(fn, *a):
    try:
        v = fn(*a)
        return ('ok', list(v) if fn in (xmlinst.instances, getattr(xmlinst, 'instances_reference', None)) else v)
    except Exception as e:                                       # noqa: BLE001 - compared: both must agree
        return ('error', type(e).__name__, str(e))


def answers(text, inst, ref, n=None):
    """Every answer for one text in order: instances with each suffix, then field_ref of each instance's body for
    each name; one Refused error for the whole text when any piece is refused."""
    out = []
    for s in SUFFIXES:
        o = outcome(inst, text, s)
        out.append(o)
        if o[0] != 'ok':
            continue
        for _, _, body in o[1] if s is None else ():
            for nm in NAMES:
                out.append(outcome(ref, body, nm))
                if n is not None:
                    n['refs'] += 1
                    n['refused'] |= native.require('xmlinst', 'xmlinst_field_ref').xmlinst_field_ref(body, nm) is None
        if n is not None:
            n['instances'] += len(o[1])
            n['refused'] |= native.require('xmlinst', 'xmlinst_instances').xmlinst_instances(text, s) is None
    if any(o[0] == 'error' and o[1] == 'Refused' for o in out):
        return ('error', 'Refused', 'a piece of the text the Rust core refuses')
    return out


def compare(text, counts):
    n = {'instances': 0, 'refs': 0, 'refused': False}
    got = answers(text, xmlinst.instances, xmlinst.field_ref, n)
    counts['instances'] += n['instances']
    counts['refs'] += n['refs']
    live = REPLAY.live(xmlinst, 'instances_reference')
    ref = (lambda: answers(text, _ref(xmlinst, 'instances_reference'), _ref(xmlinst, 'field_ref_reference'))) if live else None
    if not REPLAY.pair('text', (text,), got, ref, refused=n['refused']):
        return f'answers differ: {str(got)[:200]}'
    return None


def random_text(rnd):
    parts = []
    for _ in range(rnd.randint(0, 6)):
        attrs = [f'guid="G{rnd.randrange(9)}"', f'type="{rnd.choice(["Entity.ReferenceObjectData", "X.SoldierSpawnEntityData", "A.B", ""])}"']
        rnd.shuffle(attrs)
        if rnd.random() < 0.1:
            attrs.append('type="Dup.VehicleSpawnEntityData"')
        if rnd.random() < 0.05:
            attrs.append('w\u00e9ird="1"')
        fields = []
        for _ in range(rnd.randint(0, 4)):
            fa = [f'name="{rnd.choice(NAMES)}"', f'ref="r{rnd.randrange(9)}"'] if rnd.random() < 0.8 else \
                [f'name="{rnd.choice(NAMES)}"']
            rnd.shuffle(fa)
            close = rnd.choice(['/>', '>', ' />', '>x</field>'])
            tag = rnd.choice(['field', 'field', 'fieldx', 'field\u00e9'])
            fields.append(f'<{tag} {" ".join(fa)}{close}')
        body = ''.join(fields) + rnd.choice(['', 'text \u00fc', '\n'])
        open_tag = rnd.choice(['instance', 'instance', 'instance', 'instances'])
        end = rnd.choice(['</instance>', '</instance>', '</instance>', ''])
        parts.append(f'<{open_tag} {" ".join(attrs)}>{body}{end}')
    return rnd.choice(['', '<partition>']) + '\n'.join(parts)


def quick():
    rnd = random.Random(41)
    counts = {'instances': 0, 'refs': 0}
    bad = []
    for _ in range(3000):
        r = compare(random_text(rnd), counts)
        if r:
            bad.append(r)
    check(not bad, f'3,000 seeded texts: {counts["instances"]} instances and {counts["refs"]} references the same',
          '; '.join(bad[:2]))


def game():
    root = os.environ.get('OPFOR_ROOT') or r'C:\Programming\Projects\project-opfor-bfbc2'
    files = sorted(glob.glob(os.path.join(root, 'work', 'levels', '*', 'dist', 'levels', '*.xml')))
    files += sorted(glob.glob(os.path.join(root, 'work', 'levels', '*', 'dist', 'levels', '*', '*.xml')))
    if not files:
        print('SKIP game: no converted levels in work\\levels')
        return
    counts = {'instances': 0, 'refs': 0}
    bad = []
    t_r = t_p = 0.0
    for f in files:
        x = open(f, encoding='utf-8', errors='replace').read()
        r = compare(x, counts)
        if r:
            bad.append(f'{f}: {r}')
        t = time.perf_counter()
        for s in SUFFIXES[1:7]:
            for _, _, body in xmlinst.instances(x, s):
                xmlinst.field_ref(body, 'ReferencedObject')
        t2 = time.perf_counter()
        if REPLAY.live(xmlinst, 'instances_reference'):
            for s in SUFFIXES[1:7]:
                for _, _, body in _ref(xmlinst, 'instances_reference')(x, s):
                    _ref(xmlinst, 'field_ref_reference')(body, 'ReferencedObject')
        t3 = time.perf_counter()
        t_r += t2 - t
        t_p += t3 - t2
    check(not bad, f'{len(files)} converted XML files of work\\levels: {counts["instances"]} instances and '
          f'{counts["refs"]} references the same', '; '.join(bad[:2]))
    print(f'     NavKit\'s six scans over them: {t_r:.1f} s' + (f' (the patterns {t_p:.1f} s)' if REPLAY.live(xmlinst, 'instances_reference') else ''))


def main(argv):
    try:
        native.require('xmlinst (round F2b)', 'xmlinst_instances')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    quick()
    if '--game' in argv:
        game()
    check(REPLAY.finish(), 'the patterns: recorded, or compared with the recording')
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
