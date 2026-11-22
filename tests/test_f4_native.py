r"""Round F4: F3's leftovers in the Rust core, each port against its Python reference (kept as *_reference, or the
module's use_native(False)) and against the reference's answers recorded in tests\f4_golden_*.json - recorded from a
frozen copy of the round's starting commit (6a7dd68), whose public functions were the Python, before anything was
changed.

  culltree  culltree.Doc's string table                            (bfbc2-culltree/culltree.py)
  disasm    xenos.disasm's header                                  (bfbc2-shader-tools/xenos.py)
  audio     audio.convert / decode, the ffmpeg glue                (bfbc2-console-convert/audio.py)
  hkpack    hkpack.Packfile as a handle, read_object / read_value  (bfbc2-auto-navkit/tools/hkpack.py)
  bodies    navsample_offline.asset_bodies (the hulls)             (bfbc2-auto-navkit/tools/navsample_offline.py)
  meshview  meshview.load_mesh's part reader                       (bfbc2-mesh-viewer/meshview.py)
  texqa     texqa.read_pc / read_xbox                              (bfbc2-console-convert/texqa.py)

Compared as tests\test_f3_native.py compares (its canon / outcome / goldens): every result or the same error, and
how many inputs the Rust core answered and how many it handed to the Python (a real input handed back is listed).

  python tests\test_f4_native.py                    quick: built, seeded and damaged inputs
  python tests\test_f4_native.py --game             + every real input (the game copy, OPFOR's work folders,
                                                     Battlefield 1943); timings
  python tests\test_f4_native.py --game --record    the goldens from the reference's answers
  --only=culltree,disasm,...                         some areas only
"""
import contextlib
import gc
import glob
import os
import random
import shutil
import struct
import sys
import tempfile
import time
from collections.abc import Callable, Iterator
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in (SUITE, HERE):
    if p not in sys.path:
        sys.path.insert(0, p)
import suite_paths  # noqa: E402

suite_paths.use('core', 'navkit-tools', 'console-convert', 'shader-tools', 'bf1943-import', 'culltree',
                'mesh-viewer')
import audio  # noqa: E402
import culltreelib as culltree  # noqa: E402
import hkpack  # noqa: E402
import navsample_offline  # noqa: E402
import test_f3_native as f3  # noqa: E402
import texqa  # noqa: E402
import xenos  # noqa: E402
from f0_parity import Checks, stdout_utf8  # noqa: E402

f3.GOLDEN = os.path.join(HERE, 'f4_golden_{}.json')
N = f3.N
GAME, RECORD = f3.GAME, f3.RECORD
Golden, Tally, canon, outcome, damaged, rel = f3.Golden, f3.Tally, f3.canon, f3.outcome, f3.damaged, f3.rel
SCRATCH = os.environ.get('BFBC2_TEST_SCRATCH') or tempfile.gettempdir()


def has(name: str) -> bool:
    return hasattr(N, name)


def wanted(area: str) -> bool:
    return f3.wanted(area)


def nat_of(name: str, fn: Callable[..., Any]) -> Callable[..., Any] | None:
    """The 'answered in Rust' probe when the module has the binding (the frozen copy's has not: recording)."""
    return fn if has(name) else None


@contextlib.contextmanager
def python_only(*mods: Any) -> Iterator[None]:
    """The modules' Rust paths off (their use_native) for the reference's answers; nothing in the frozen copy."""
    done = []
    for m in mods:
        fn = getattr(m, 'use_native', None)
        if fn is not None:
            fn(False)
            done.append(m)
    try:
        yield
    finally:
        for m in done:
            m.use_native(True)
            if m is hkpack:                                      # hkpack's use_native(True) keeps it on: as before
                hkpack._registry().use_backend('hkpack', None)
            if m is culltree:
                culltree._STRINGS_NATIVE.clear()


def ref_of(mods: tuple[Any, ...], fn: Callable[..., Any]) -> Callable[..., Any]:
    def run(*a: Any) -> Any:
        with python_only(*mods):
            return fn(*a)
    return run


def dbx_entries(pattern: str, keep: Callable[[str], bool]) -> Iterator[tuple[str, int, list[tuple[str, bytes]]]]:
    """Per bundle (path, size, [(name, bytes)]) of its .dbx entries the filter keeps."""
    from bfbc2core import fbrb
    for p in sorted(glob.glob(pattern, recursive=True)):
        try:
            es = [e for e in fbrb.entries(p) if not e.deleted and e.name.lower().endswith('.dbx') and keep(e.name)]
        except Exception:                                        # noqa: BLE001
            continue
        if not es:
            continue
        data = fbrb.read(p, [e.name for e in es])
        yield p, os.path.getsize(p), [(e.name, data[e.name]) for e in es]
        del data


# ------------------------------------------------------------------------------------------------ culltree

def doc_view(data: bytes) -> tuple[Any, ...]:
    d = culltree.Doc(data, 'x.dbx')
    return d.strings, d.head12, d.string_area, d.trailer, d.tobytes()


def doc_nat(data: bytes) -> Any:
    if len(data) < 24 or not data.startswith(culltree.MAGIC):
        return None
    return N.culltree_strings(data, struct.unpack_from('>I', data, 20)[0])


def built_doc(rng: random.Random, base: bytes) -> bytes:
    """A real document's string table replaced: random strings (UTF-8, invalid bytes), offsets in and out of range."""
    size, count = struct.unpack_from('>II', base, 16)
    body = base[24 + size:]
    strs = [bytes(rng.randrange(1, 256) for _ in range(rng.randrange(0, 12))) for _ in range(rng.randint(0, 9))]
    strs += [s.encode() for s in ('instance', 'array', 'é€', '퟿')]
    rng.shuffle(strs)
    area = b''
    offs = []
    for s in strs:
        offs.append(len(area))
        area += s + b'\0'
    if rng.random() < 0.2 and offs:
        offs[rng.randrange(len(offs))] = rng.choice([len(area), len(area) + 5, 1 << 31])
    if rng.random() < 0.2:
        area = area.rstrip(b'\0')
    table = b''.join(struct.pack('>I', o) for o in offs)
    head = base[:12] + struct.pack('>I', 24 + len(table) + len(area)) + base[16:16]
    out = head + struct.pack('>II', len(table) + len(area), len(offs)) + table + area
    return bytes(out[:12]) + base[12:16] + out[16:] + body


def area_culltree(c: Checks) -> None:
    g = Golden('culltree')
    t = Tally(g)
    rng = random.Random(0xF4A1)
    nat = nat_of('culltree_strings', doc_nat)
    ref = ref_of((culltree,), doc_view)
    root = f3.opfor_root()
    seeds: list[bytes] = []
    if root:
        for _p, _s, ents in dbx_entries(os.path.join(root, 'game', '**', 'mp_005', 'level-00.fbrb'), lambda n: True):
            seeds = [b for _, b in ents[:60]]
            break
    for i, b in enumerate(seeds):
        t.both(f'seed {i}', 'quick:seed', 0, doc_view, ref, nat, b, real=True)
    for i in range(1500 if seeds else 0):
        t.both(f'damaged {i}', 'quick:damaged', 0, doc_view, ref, nat, damaged(rng.choice(seeds), rng))
    for i in range(1500 if seeds else 0):
        t.both(f'built {i}', 'quick:built', 0, doc_view, ref, nat, built_doc(rng, rng.choice(seeds)))
    if GAME and root:
        n = 0
        cull = lambda nm: 'staticculltree' in nm.lower()                                  # noqa: E731
        for pat, keep in ((os.path.join(root, 'game', '**', '*.fbrb'), cull),
                          (os.path.join(root, 'work', 'xbox', '**', '*.fbrb'), cull),
                          (os.path.join(root, 'work', 'xbox_dlc', '**', '*.fbrb'), cull),
                          (os.path.join(root, 'game', '**', 'mp_005', '*.fbrb'), lambda nm: True),
                          (os.path.join(root, 'game', '**', 'sp_005', '*.fbrb'), lambda nm: True)):
            for p, size, ents in dbx_entries(pat, keep):
                for name, b in ents:
                    n += 1
                    t.both(f'{rel(p, root)}/{name}', rel(p, root), size, doc_view, ref, nat, b,
                           real=b.startswith(culltree.MAGIC))
        print(f'documents: {n:,}')
    print(f'culltree timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'culltree.Doc string table', GAME)
    g.save()


# ------------------------------------------------------------------------------------------------ disasm

def _edit(sh: Any, k: int) -> Any:
    """A header value changed (as a caller may change a tree): kinds the port formats and kinds it hands back."""
    h = sh.header
    if k == 0:
        sh.ctab = None
    elif k == 1:
        sh.literals = {7: [1, 2.5, -0.0, 1e300, float('nan'), float('inf'), 1e-5, 123456789.0], 2: (0.1,)}
    elif k == 2:
        sh.int_literals = {1: [1, -2, 3, 4], 0: (5, 6, 7, 8)}
    elif k == 3 and sh.ctab and sh.ctab.constants:
        sh.ctab.constants[0].name = 'néame_with_a_very_long_name_past_forty_four_characters'
    elif k == 4:
        sh.code_offset = -26
    elif k == 5:
        h.num_temp_regs = True
    elif k == 6 and h.interpolators:
        h.interpolators[0].usage = 'TEXCOORDÉ'
    elif k == 7 and h.interpolators:
        h.interpolators[0].usage = 'COLOR'
    elif k == 8:
        sh.literals = {3: [2 ** 60, 1]}
    elif k == 9 and sh.ctab and sh.ctab.constants:
        sh.ctab.constants[-1].regset = 'other'
        sh.ctab.constants[-1].count = 3
    elif k == 10 and sh.ctab and sh.ctab.constants:
        sh.ctab.constants[0].type.elements = 4
    elif k == 11:
        sh.code_size = 2 ** 70
    elif k == 12 and not sh.is_pixel and h.interpolators:
        h.interpolators[0].export_addresses = (1, 2, 3)
    elif k == 13 and sh.is_pixel:
        h.context_misc_fields.sc_output_screen_xy = 2
    elif k == 14:
        sh.is_pixel = 1 if sh.is_pixel else 0
    elif k == 15:
        sh.literals = {}
    return sh


def area_disasm(c: Checks) -> None:
    sys.path.insert(0, os.path.join(SUITE, 'bfbc2-shader-tools', 'tests'))
    import test_xenos_native as tx
    g = Golden('disasm')
    t = Tally(g)
    rng = random.Random(0xF4B2)

    def nat(b: bytes, px: bool, raw: bool, head: bool, k: int = -1) -> Any:
        try:
            sh = xenos.parse_shader(b, px)
        except Exception:                                        # noqa: BLE001
            return None
        prog = xenos.native_program(sh)
        if prog is None:
            return None
        return N.xenos_disasm(_edit(sh, k) if k >= 0 else sh, prog, raw, head)

    def fn(b: bytes, px: bool, raw: bool, head: bool, k: int = -1) -> str:
        sh = xenos.parse_shader(b, px)
        return xenos.disasm(_edit(sh, k) if k >= 0 else sh, raw, head)

    def ref(b: bytes, px: bool, raw: bool, head: bool, k: int = -1) -> str:
        sh = xenos.parse_shader_python(b, px)
        return xenos.disasm_reference(_edit(sh, k) if k >= 0 else sh, raw, head)

    probe = nat if has('xenos_disasm') else None

    def one(label: str, group: str, stamp: Any, blob: bytes, pixel: bool, real: bool) -> None:
        for raw in (False, True):
            for head in (True, False):
                t.both(f'{label} raw={raw} header={head}', group, stamp, fn, ref, probe, blob, pixel, raw, head,
                       real=real)
    for i in range(300):
        pixel = rng.random() < 0.5
        blob = tx.build(rng, pixel)
        one(f'built {i}', 'quick:built', 0, blob, pixel, False)
        one(f'built {i} damaged', 'quick:damaged', 0, tx.mutate(blob, rng), pixel, False)
        for k in range(16):
            t.both(f'built {i} edit {k}', 'quick:edited', 0, fn, ref, probe, blob, pixel, False, True, k)
    if GAME:
        items, skipped = tx.corpus()
        for s in skipped:
            print(f'  skipped: {s}')
        for label, _, blob, pixel in items:
            one(label, label.split(' ')[0].rsplit('/', 1)[0], len(items), blob, pixel, True)
        print(f'programs: {len(items):,} distinct')
    print(f'disasm timing (parse + disasm): Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'xenos.disasm (header and listing)', GAME)
    g.save()


# ------------------------------------------------------------------------------------------------ audio

def _streams_bf1943(src: Any, k: str) -> dict[int, bytes]:
    return {i: src.data(n) for i in range(3) for n in [f'{k[:-len("_wave.res")]}_wavedatasns_{i}.res']
            if n in src.entries}


def area_audio(c: Checks) -> None:
    g = Golden('audio')
    t = Tally(g)
    rng = random.Random(0xF4C3)
    ref_convert = ref_of((audio,), audio.convert)
    ref_decode = ref_of((audio,), audio.decode)
    try:
        ff = audio.ffmpeg_path()
    except audio.AudioError as e:
        print(f'no ffmpeg ({e}): EA-XMA sounds are handed to the Python')
        ff = None

    def nat_conv(b: Any, src: Any, streams: Any = None, w14: Any = 0) -> Any:
        sl = audio._stream_list(streams) if type(b) is bytes and type(src) is str else None
        return N.audio_convert(b, src, sl, w14, ff) if sl is not None else None

    def nat_dec(b: Any, src: Any, streams: Any = None) -> Any:
        sl = audio._stream_list(streams) if type(b) is bytes and type(src) is str else None
        return N.audio_decode(b, src, sl, ff) if sl is not None else None
    nc = nat_of('audio_convert', nat_conv)
    nd = nat_of('audio_decode', nat_dec)
    root = f3.opfor_root()
    src = f3.bf1943_source() if root else None
    xbox: list[tuple[str, bytes]] = []
    pc: list[tuple[str, bytes]] = []
    if root:
        for _p, _s, ents in f3.bundle_entries(os.path.join(root, 'work', 'xbox', '**', 'mp_005', 'level-00.fbrb'),
                                              {'Wave'}):
            xbox = [(n, b) for _, n, b in ents[:6]]
            break
        for _p, _s, ents in f3.bundle_entries(os.path.join(root, 'game', '**', 'mp_005', 'level-00.fbrb'), {'Wave'}):
            pc = [(n, b) for _, n, b in ents[:12]]
            break
    for n, b in xbox:
        t.both(f'xbox {n}', 'quick:xbox', 0, audio.convert, ref_convert, nc, b, 'bfbc2', None, real=True)
        t.both(f'xbox {n} decode', 'quick:xbox', 0, audio.decode, ref_decode, nd, b, 'bfbc2-xbox', None, real=True)
        for j in range(4):
            d = damaged(b, rng)
            t.both(f'xbox {n} damaged {j}', 'quick:damaged', 0, audio.convert, ref_convert, nc, d, 'bfbc2', None)
    for n, b in pc:
        t.both(f'pc {n} decode', 'quick:pc', 0, audio.decode, ref_decode, nd, b, 'bfbc2-pc', None, real=True)
        for j in range(20):
            t.both(f'pc {n} damaged {j}', 'quick:damaged', 0, audio.decode, ref_decode, nd, damaged(b, rng), 'bfbc2-pc')
    # arguments of other kinds and values (each the reference's answer or error)
    if xbox:
        b = xbox[0][1]
        for args in ((b, 'bf1943x', None), (b, 'bfbc2-pc', None), (bytearray(b), 'bfbc2', None), (b, 'bfbc2', [1]),
                     (b, 'bfbc2', {0: 'x'}), (b, 'bfbc2', {0: b'\0' * 4}), (b, b'bfbc2', None), (b, 'bfbc2', {}),
                     (b, 'bfbc2', None, -1), (b, 'bfbc2', None, 1 << 40), (b, 'bfbc2', None, 2.0)):
            t.both(f'args {args[1:]!r:.60}', 'quick:args', 0, audio.convert, ref_convert, nc, *args)
    if src is not None:
        ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'Wave')
        for k in ks[:: max(1, len(ks) // 6)][:6]:
            st = _streams_bf1943(src, k)
            t.both(f'BF1943 {k}', 'quick:bf1943', 0, audio.convert, ref_convert, nc, src.data(k), 'bf1943', st, 0,
                   real=True)
    if GAME and root:
        # OPFOR's call: wake_level.port_sounds -> audio.convert_many of every BF1943 Wave (the WW2 install)
        if src is not None:
            ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') == 'Wave')
            for k in ks:
                st = _streams_bf1943(src, k)
                t.both(f'BF1943 {k}', 'bf1943', len(ks), audio.convert, ref_convert, nc, src.data(k), 'bf1943', st, 0,
                       real=True)
                t.both(f'BF1943 {k} decode', 'bf1943', len(ks), audio.decode, ref_decode, nd, src.data(k), 'bf1943',
                       st, real=True)
        nw = 0
        for pat, source in ((os.path.join(root, 'work', 'xbox', '**', '*.fbrb'), 'bfbc2'),
                            (os.path.join(root, 'game', '**', '*.fbrb'), 'bfbc2-pc')):
            for p, size, ents in f3.bundle_entries(pat, {'Wave'}):
                for j, (_, name, b) in enumerate(ents):
                    nw += 1
                    if source == 'bfbc2':
                        if j % 3:
                            continue                             # a third of the BFBC2 Xbox sounds (ffmpeg's time)
                        t.both(f'{rel(p, root)}/{name}', rel(p, root), size, audio.convert, ref_convert, nc, b,
                               source, None, real=True)
                    else:
                        t.both(f'{rel(p, root)}/{name}', rel(p, root), size, audio.decode, ref_decode, nd, b,
                               source, None, real=True)
        print(f'game Waves: {nw:,}')
    print(f'audio timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'audio.convert / decode', False)
    g.save()


# ------------------------------------------------------------------------------------------------ hkpack

def pf_probe(data: Any) -> tuple[Any, ...]:
    """What a caller reads of a Packfile: the header values, deref of every pointer slot (and some that are not),
    objects_of each class, read_object of each object (by its class and by another name), read_value of a few
    kinds at each object, cstr of the class names' places; then every attribute (the dicts built last)."""
    pf = hkpack.Packfile(data)
    rf = hkpack.Packfile.reference(data) if type(data) is bytes else pf
    out: list[Any] = [pf.base, pf.version, pf.ptr_size, pf.contents_version]
    slots = list(rf.ptr)[:3000]
    out.append([pf.deref(s) for s in slots + [0, -4, 1 << 40, 2 ** 70, None, 'x']])
    names = sorted({c for _, c in rf.objects})
    out.append([pf.objects_of(n) for n in names + ['nothing', 5]])
    reads = []
    for off, cn in rf.objects[:3000]:
        reads.append(outcome(pf.read_object, off, cn))
        for mt, sub, carray in ((7, 0, 0), (11, 0, 0), (20, 0, 0), (29, 0, 0), (22, 7, 0), (12, 0, 2), (1, 0, 0),
                                (24, 5, 0), (25, 0, 1), (9, 0, -1)):
            reads.append(outcome(pf.read_value, off, mt, sub, cn, carray))
    reads.append(outcome(pf.read_object, -8, 'hkClass'))
    reads.append(outcome(pf.read_object, len(data) + 4, None))
    out.append(reads)
    out.append([outcome(pf.cstr, t) for t in list(rf.ptr.values())[:300]] + [outcome(pf.cstr, len(data))])
    _ = pf.ptr                                                   # the dicts (a handle builds them now)
    out.append(canon(pf))
    return tuple(out)


def area_hkpack(c: Checks) -> None:
    g = Golden('hkpack')
    t = Tally(g)
    rng = random.Random(0xF4D4)
    root = f3.opfor_root()
    nat = nat_of('hkpack_open', lambda d: N.hkpack_open(d) if type(d) is bytes else None)
    ref = ref_of((hkpack,), pf_probe)
    seeds: list[bytes] = []
    if root:
        for _p, _size, ents in f3.bundle_entries(os.path.join(root, "game", "**", "mp_005", '*.fbrb'),
                                                 {'HavokPhysicsData'}):
            seeds += [b for _, _, b in ents[:4]]
            if len(seeds) >= 8:
                break
    for i, b in enumerate(seeds):
        t.both(f'seed {i}', 'quick:seed', 0, pf_probe, ref, nat, b, real=True)
    for i in range(400 if seeds else 0):
        t.both(f'damaged {i}', 'quick:damaged', 0, pf_probe, ref, nat, damaged(rng.choice(seeds), rng))
    for i, b in enumerate(built_packfiles(rng)):
        t.both(f'built {i}', 'quick:built', 0, pf_probe, ref, nat, b)
    if GAME and root:
        n = with_classes = 0
        for pat in (os.path.join(root, 'game', '**', '*.fbrb'), os.path.join(root, 'work', 'ww2', '**', '*.fbrb')):
            for p, size, ents in f3.bundle_entries(pat, {'HavokPhysicsData'}):
                for _, name, b in ents:
                    n += 1
                    t.both(f'{rel(p, root)}/{name}', rel(p, root), size, pf_probe, ref, nat, b,
                           real=hkpack.MAGIC in b)
                    with_classes += b'hkClass\x00' in b
        print(f'packfiles: {n:,} ({with_classes:,} naming hkClass)')
    print(f'hkpack timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'hkpack.Packfile handle', bool(root))
    g.save()


def built_packfiles(rng: random.Random) -> list[bytes]:
    """Packfiles with class descriptions (the BFBC2 ones carry none): an __types__-less layout of hkClass objects,
    their members of every type the walk reads, an object of each class, pointers, arrays and C strings."""
    out = []
    for _ in range(120):
        out.append(_built_one(rng))
    return out


def _built_one(rng: random.Random) -> bytes:
    data = bytearray()
    local: list[tuple[int, int]] = []
    virt: list[tuple[int, int]] = []
    names = bytearray()
    pending: list[tuple[int, int]] = []                          # member class pointers, once the classes are placed

    def put(b: bytes) -> int:
        while len(data) % 16:
            data.append(0)
        at = len(data)
        data.extend(b)
        return at

    def name(s: str) -> int:
        at = len(names)
        names.extend(s.encode() + b'\0')
        return at

    def obj(at: int, cls: str) -> None:
        virt.append((at, name(cls)))

    def cstr(s: bytes) -> int:
        return put(s + b'\0')
    # member types: (name, type, subtype, carray, offset, class index or None)
    kinds = [(1, 0), (2, 0), (3, 0), (4, 0), (5, 0), (6, 0), (7, 0), (8, 0), (9, 0), (10, 0), (11, 0), (12, 0),
             (13, 0), (14, 0), (17, 0), (20, 0), (22, 7), (22, 11), (22, 25), (22, 0), (24, 3), (24, 6), (24, 0),
             (29, 0), (30, 0), (31, 4), (19, 0), (21, 0), (0, 0), (26, 4), (25, 0), (99, 0)]
    nclasses = rng.randint(1, 3)
    class_offs = []
    sizes = []
    member_lists = []
    for ci in range(nclasses):
        ms = []
        off = 0
        for mi in range(rng.randint(1, 8)):
            t, sub = rng.choice(kinds)
            if ci == 0 and (t == 25 or sub == 25):
                t, sub = 7, 0                                    # structs only of earlier classes: no cycle
            carray = rng.choice([0, 0, 0, 2, -1])
            ms.append((f'm{mi}', t, sub, carray, off))
            off += 16 if t in (12, 13) else 64 if t in (14, 17) else 8
        if rng.random() < 0.2 and ms:
            ms.append((ms[0][0], 7, 0, 0, 0))                    # a name twice: the dict keeps the first place
        sizes.append(off + rng.choice([0, 8]))
        member_lists.append(ms)
    for ci in range(nclasses):
        ms = member_lists[ci]
        mem_at = put(bytes(24 * len(ms)))
        for k, (mn, t, sub, carray, mo) in enumerate(ms):
            nm = cstr(mn.encode())
            local.append((mem_at + 24 * k, nm))
            if t == 25 or (t == 22 and sub == 25):
                pending.append((mem_at + 24 * k + 4, rng.randrange(ci)))
            struct.pack_into('<BBhHH', data, mem_at + 24 * k + 12, t, sub, carray, 0, mo)
        cls_at = put(bytes(32))
        cname = cstr(rng.choice([f'Class{ci}', f'Class{ci}', 'h\xc3\xa9']).encode() if rng.random() < 0.1
                     else f'Class{ci}'.encode())
        local.append((cls_at, cname))
        struct.pack_into('<ii', data, cls_at + 8, sizes[ci], 0)
        local.append((cls_at + 24, mem_at))
        struct.pack_into('<i', data, cls_at + 28, len(ms))
        obj(cls_at, 'hkClass')
        class_offs.append(cls_at)
    for at, ci in pending:
        local.append((at, class_offs[ci]))
    for ci in range(nclasses):
        if ci and rng.random() < 0.5:
            local.append((class_offs[ci] + 4, class_offs[ci - 1]))   # a parent
    for ci in range(nclasses):
        for _ in range(rng.randint(1, 3)):
            o = put(bytes(rng.randrange(256) for _ in range(sizes[ci] + 64)))
            obj(o, f'Class{ci}')
            for k in range(0, sizes[ci], 8):
                if rng.random() < 0.3:
                    tgt = rng.choice([cstr(b'text\xff'), o, put(struct.pack('<4i', 1, 2, 3, 4))])
                    local.append((o + k, tgt))
                    struct.pack_into('<i', data, o + k + 4, rng.choice([0, 1, 3, -2, 1000]))
    while len(data) % 16:
        data.append(0)
    fix_at = len(data)
    for s, d in local:
        data += struct.pack('<II', s, d)
    data += struct.pack('<II', 0xFFFFFFFF, 0)
    glob_at = len(data)
    virt_at = len(data)
    for s, nm in virt:
        data += struct.pack('<III', s, 1, nm)
    exp_at = len(data)
    # sections: __data__ (0) and __classnames__ (1)
    head = bytearray(0x40 + 0x30 * 2)
    struct.pack_into('<8I', head, 0, 0x57E0E057, 0x10C0C010, 0, 5, 0x01010004, 2, 0, 0)
    head[0x28:0x38] = b'Havok-5.5.0-r1\0\0'
    body_at = len(head)
    names_at = body_at + len(data)
    for i, (sname, st, loc, glo, vir, ex, end) in enumerate((
            (b'__data__', body_at, fix_at, glob_at, virt_at, exp_at, exp_at),
            (b'__classnames__', names_at, len(names), len(names), len(names), len(names), len(names)))):
        o = 0x40 + 0x30 * i
        head[o:o + 20] = sname.ljust(20, b'\0')
        struct.pack_into('<7I', head, o + 20, st, loc, glo, vir, ex, ex, end)
    out = bytes(head) + bytes(data) + bytes(names)
    if rng.random() < 0.2:
        out = damaged(out, rng)
    return out


# ------------------------------------------------------------------------------------------------ asset bodies

def bodies(data: bytes) -> Any:
    """navsample_offline.asset_bodies of a file's bytes (the frozen copy's through hkpack.load)."""
    fn = getattr(navsample_offline, 'asset_bodies_of', None)
    if fn is not None:
        return fn(data)
    saved = hkpack.load
    hkpack.load = lambda _p: hkpack.Packfile(data)
    try:
        return navsample_offline.asset_bodies('x')
    finally:
        hkpack.load = saved


def bodies_ref(data: bytes) -> Any:
    import navnative
    fn = getattr(navsample_offline, 'asset_bodies_of_reference', None)
    navnative.use_native(False)
    try:
        with python_only(hkpack):
            return fn(data) if fn is not None else bodies(data)
    finally:
        navnative.use_native(True)


def area_bodies(c: Checks) -> None:
    g = Golden('bodies')
    t = Tally(g)
    rng = random.Random(0xF4E5)
    nat = nat_of('navkit_asset_bodies', lambda d: N.navkit_asset_bodies(d) if type(d) is bytes else None)
    root = f3.opfor_root()
    seeds: list[bytes] = []
    if root:
        lv = sorted(glob.glob(os.path.join(root, 'work', 'levels', 'mp_005', '**', '*_havok.res'), recursive=True))
        seeds = [open(p, 'rb').read() for p in lv[:: max(1, len(lv) // 40)][:40]]
    for i, b in enumerate(seeds):
        t.both(f'seed {i}', 'quick:seed', 0, bodies, bodies_ref, nat, b, real=True)
    for i in range(600 if seeds else 0):
        t.both(f'damaged {i}', 'quick:damaged', 0, bodies, bodies_ref, nat, damaged(rng.choice(seeds), rng))
    if GAME and root:
        n = 0
        levels = sorted(glob.glob(os.path.join(root, 'work', 'levels', '*', '**', '*_havok.res'), recursive=True))
        for p in levels:
            b = open(p, 'rb').read()
            n += 1
            t.both(rel(p, root), 'work/levels', len(levels), bodies, bodies_ref, nat, b, real=hkpack.MAGIC in b)
        for pat in (os.path.join(root, 'game', '**', '*.fbrb'), os.path.join(root, 'work', 'ww2', '**', '*.fbrb')):
            for p, size, ents in f3.bundle_entries(pat, {'HavokPhysicsData'}):
                for _, name, b in ents:
                    n += 1
                    t.both(f'{rel(p, root)}/{name}', rel(p, root), size, bodies, bodies_ref, nat, b,
                           real=hkpack.MAGIC in b)
        print(f'collision assets: {n:,}')
    print(f'asset_bodies timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    # a hull with tied angles (collinear vertices on a face: Railway_Switch's) is the Python's - NumPy's argsort orders
    # ties its own way - so real files handed back are listed, not failed
    print(f'real files handed back: {getattr(t, "real_handed_n", 0)} {t.real_handed}')
    t.report(c, 'navsample_offline.asset_bodies', False)
    g.save()


# ------------------------------------------------------------------------------------------------ meshview

class _Ix:
    """A meshview.Index's parts with the meshes' data read beforehand (one pass per bundle)."""

    def __init__(self, ix: Any, data: dict[str, bytes]) -> None:
        self.parts = ix.parts
        self.data = data

    def read(self, name: str) -> bytes | None:
        return self.data.get(name.lower())


def area_meshview(c: Checks) -> None:
    import meshview
    import shaderdb
    g = Golden('meshview')
    t = Tally(g)
    rng = random.Random(0xF4F6)
    root = f3.opfor_root()
    if not root:
        print('no OPFOR folder: nothing to compare')
        return
    cache = os.path.join(SCRATCH, 'f4_meshview_index.pkl')
    src = os.path.join(root, 'work', 'tools_out', 'meshview_index.pkl')
    if not os.path.isfile(cache) and os.path.isfile(src):
        shutil.copyfile(src, cache)
    ix = meshview.Index(shaderdb.find_opfor(None), cache)
    saved = meshview.mesh_surfaces
    meshview.mesh_surfaces = lambda _ix, _name: {}

    def load(fake: Any, name: str) -> Any:
        return meshview.load_mesh(fake, name)

    parts = {'rust': 0, 'python': 0}
    real_part = getattr(N, 'meshview_part', None)

    def counted(*a: Any) -> Any:
        got = real_part(*a) if real_part is not None else None
        parts['rust' if got is not None else 'python'] += 1
        return got
    if real_part is not None:
        N.meshview_part = counted

    def nat(fake: Any, name: str) -> Any:
        return None if fake.read(f'{name}_lod0_data.res') is None else True

    ref = ref_of((meshview,), load)
    names = sorted(n for n in ix.where if n.endswith('_lod0_data.res'))
    if not GAME:
        names = names[:: max(1, len(names) // 60)][:60]
    by_bundle: dict[str, list[str]] = {}
    for n in names:
        by_bundle.setdefault(ix.where[n][0], []).append(n)
    try:
        for b, ns in sorted(by_bundle.items()):
            from bfbc2core import fbrb
            got = fbrb.read(b, [ix.where[n][1] for n in ns])
            data = {n: got[ix.where[n][1]] for n in ns if ix.where[n][1] in got}
            fake = _Ix(ix, data)
            group = 'quick' if not GAME else rel(b, root)
            for n in ns:
                mesh = n[:-len('_lod0_data.res')]
                t.both(mesh, group, 0 if not GAME else len(ns), load, ref, nat_of('meshview_part', nat), fake, mesh,
                       real=True)
                if not GAME and n in data:
                    for j in range(5):
                        dmg = _Ix(ix, {n: damaged(data[n], rng)})
                        t.both(f'{mesh} damaged {j}', 'quick:damaged', 0, load, ref, None, dmg, mesh)
    finally:
        meshview.mesh_surfaces = saved
        if real_part is not None:
            N.meshview_part = real_part
    print(f'meshes: {len(names):,}; parts read (the Rust way and the reference way): {parts}')
    print(f'meshview timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'meshview.load_mesh parts', False)
    g.save()


# ------------------------------------------------------------------------------------------------ texqa

def area_texqa(c: Checks) -> None:
    g = Golden('texqa')
    t = Tally(g)
    rng = random.Random(0xF4A7)
    root = f3.opfor_root()
    rpc = ref_of((texqa,), texqa.read_pc)
    rxb = ref_of((texqa,), texqa.read_xbox)
    npc = nat_of('texqa_read', lambda r: N.texqa_read(r, False) if type(r) is bytes else None)
    nxb = nat_of('texqa_read', lambda r: N.texqa_read(r, True) if type(r) is bytes else None)
    pc: list[bytes] = []
    xb: list[bytes] = []
    if root:
        for _p, _s, ents in f3.bundle_entries(os.path.join(root, 'game', '**', 'mp_005', 'level-00.fbrb'), {'ITexture'}):
            pc = [b for _, _, b in ents[:20]]
        for _p, _s, ents in f3.bundle_entries(os.path.join(root, 'work', 'xbox', '**', 'mp_005', 'level-00.fbrb'),
                                              {'XenonTexture'}):
            xb = [b for _, _, b in ents[:20]]
    for i, b in enumerate(pc):
        t.both(f'pc {i}', 'quick:pc', 0, texqa.read_pc, rpc, npc, b, real=True)
        for j in range(15):
            t.both(f'pc {i} damaged {j}', 'quick:damaged', 0, texqa.read_pc, rpc, npc, damaged(b, rng))
    for i, b in enumerate(xb):
        t.both(f'xbox {i}', 'quick:xbox', 0, texqa.read_xbox, rxb, nxb, b, real=True)
        for j in range(15):
            t.both(f'xbox {i} damaged {j}', 'quick:damaged', 0, texqa.read_xbox, rxb, nxb, damaged(b, rng))
    # header values of other kinds: every format and type, sizes off, the on-demand wrapper
    for i in range(400 if pc else 0):
        hb = bytearray(rng.choice(pc))
        fmt = rng.choice([0, 1, 2, 3, 5, 9, 10, 13, 18, 19, 20, 7, 99])
        struct.pack_into('<I', hb, 8, fmt)
        if rng.random() < 0.3:
            struct.pack_into('<I', hb, 4, rng.choice([0, 1, 2]))
        if rng.random() < 0.3:
            struct.pack_into('<I', hb, 28, rng.choice([1, 2, 15, 16, 0]))
        t.both(f'pc header {i}', 'quick:header', 0, texqa.read_pc, rpc, npc, bytes(hb))
        x = bytearray(rng.choice(xb)) if xb else bytearray()
        if x:
            struct.pack_into('>I', x, 8, fmt)
            t.both(f'xbox header {i}', 'quick:header', 0, texqa.read_xbox, rxb, nxb, bytes(x))
    if pc:
        w = b'RES\x02' + bytes(52) + struct.pack('<I', len(pc[0])) + bytes(4) + pc[0]
        t.both('wrapped', 'quick:header', 0, texqa.read_pc, rpc, npc, w, real=True)
        t.both('short', 'quick:header', 0, texqa.read_pc, rpc, npc, b'\x03\0\0\0')
        t.both('bytearray', 'quick:header', 0, texqa.read_pc, rpc, npc, bytearray(pc[0]))
    if GAME and root:
        n = 0
        for pat, kind in ((os.path.join(root, 'game', '**', '*.fbrb'), 'ITexture'),
                          (os.path.join(root, 'work', 'xbox', '**', '*.fbrb'), 'XenonTexture'),
                          (os.path.join(root, 'work', 'xbox_dlc', '**', '*.fbrb'), 'XenonTexture')):
            fn, rf, nt = (texqa.read_pc, rpc, npc) if kind == 'ITexture' else (texqa.read_xbox, rxb, nxb)
            for p, size, ents in f3.bundle_entries(pat, {kind}):
                for _, name, b in ents:
                    n += 1
                    t.both(f'{rel(p, root)}/{name}', rel(p, root), size, fn, rf, nt, b, real=True)
        src = f3.bf1943_source()
        if src is not None:
            ks = sorted(k for k, (_, e) in src.entries.items() if e.get('type') in ('XenonTexture', 'ITexture'))
            for k in ks:
                n += 1
                t.both(f'BF1943 {k}', 'bf1943', len(ks), texqa.read_xbox, rxb, nxb, src.data(k), real=True)
        print(f'textures: {n:,}')
    print(f'texqa timing: Rust {t.t_nat:.2f} s, Python {t.t_ref:.2f} s')
    t.report(c, 'texqa.read_pc / read_xbox', GAME)
    g.save()


def main() -> int:
    stdout_utf8()
    c = Checks()
    for name, fn in (('culltree', area_culltree), ('disasm', area_disasm), ('hkpack', area_hkpack),
                     ('bodies', area_bodies), ('texqa', area_texqa), ('meshview', area_meshview),
                     ('audio', area_audio)):
        if wanted(name):
            t0 = time.perf_counter()
            fn(c)
            print(f'-- {name}: {time.perf_counter() - t0:.1f} s', flush=True)
            gc.collect()
    return c.done()


if __name__ == '__main__':
    sys.exit(main())
