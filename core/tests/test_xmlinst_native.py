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
from bfbc2core import xmlinst  # noqa: E402

FAILS: list[str] = []
SUFFIXES = (None, 'Entity.AreaGeometryEntityData', '.AreaOrderEntityData', 'Entity.ReferenceObjectData',
            '.SoldierSpawnEntityData', '.VehicleSpawnEntityData', 'Terrain.TerrainSplinePointData', '')
NAMES = ('ReferencedObject', 'Vehicle', 'Mesh', 'Name', 'x')


def check(ok, what, detail=''):
    print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)
    if not ok:
        FAILS.append(what)


def compare(text, counts):
    for s in SUFFIXES:
        got = list(xmlinst.instances(text, s))
        want = list(xmlinst.instances_reference(text, s))
        counts['instances'] += len(want)
        if got != want:
            return f'instances {s}: {got[:2]} / {want[:2]}'
        for _, _, body in want if s is None else ():
            for n in NAMES:
                counts['refs'] += 1
                a, b = xmlinst.field_ref(body, n), xmlinst.field_ref_reference(body, n)
                if a != b:
                    return f'field_ref {n}: {a} / {b}'
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
        for s in SUFFIXES[1:7]:
            for _, _, body in xmlinst.instances_reference(x, s):
                xmlinst.field_ref_reference(body, 'ReferencedObject')
        t3 = time.perf_counter()
        t_r += t2 - t
        t_p += t3 - t2
    check(not bad, f'{len(files)} converted XML files of work\\levels: {counts["instances"]} instances and '
          f'{counts["refs"]} references the same', '; '.join(bad[:2]))
    print(f'     NavKit\'s six scans over them: {t_r:.1f} s (the patterns {t_p:.1f} s)')


def main(argv):
    from bfbc2core import native
    try:
        native.require('xmlinst (round F2b)', 'xmlinst_instances')
    except native.NativeMissing as e:
        check(False, 'the Rust core is built from these sources', str(e))
        return 1
    quick()
    if '--game' in argv:
        game()
    if FAILS:
        print(f'{len(FAILS)} FAILED')
        return 1
    print('all passed')
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
