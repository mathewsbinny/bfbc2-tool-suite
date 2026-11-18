r"""bf1943import.py - bring Battlefield 1943 (Xbox 360) resources into Bad Company 2's PC formats, from the user's own
copy (nothing is shipped: the outputs stay in Project OPFOR BFBC2's work folder).

  python bf1943import.py list [<filter>...] [--source <folder>]
                                        the BF1943 resources whose names contain every filter (any case): type,
                                        size, bundle, and what import would do with it
  python bf1943import.py import <filter>... [--source <folder>] [--opfor <folder>] [--out <folder>]
                                        converts every matching resource into <out>\<resource name> and writes
                                        <out>\import.json (one record per resource: source bundle and SHA-256,
                                        type, what was done, output SHA-256, notes); default <out>:
                                        <OPFOR>\work\bf1943\import
  python bf1943import.py check [--source <folder>] [--opfor <folder>]
                                        converts every animation and skeleton of the source in memory and checks
                                        each output (reads as a PC file; the same object tree as the source; the
                                        bone table renumbered onto BFBC2's soldier); exit code 1 on a failure

<source>: the folder of BF1943's extracted Xbox 360 bundles (dist\xenon), default the Project Rising Sun workspace
next to the suite (operation-rising-sun\work\extracted\dist\xenon) or BF1943_XENON. Read only.

What is converted (PLAN):
- GrannyAnimation, DeltaAnimation: to little-endian (granny.to_pc). When every track is a soldier bone, DICE's
  per-bone table is renumbered from BF1943's soldier skeleton (ske01) to BFBC2's (same 147 bones and parents,
  another order); otherwise (vehicles, props) the indices are kept and the record says so.
- GrannyModel (skeletons): to little-endian.
- XenonTexture: to the PC's ITexture (bfbc2-console-convert texture.convert; DXT1/3/5, BC5, DXT5A, RGB565, L8).
  A third-person undeploy (3p/StandUnDeploy, 3p/CrouchUnDeploy) gets the tracks DICE adds to every one it ships:
  the same weapon's same-stance deploy animates 25 more bones than the undeploy (19 weapon parts, 6 thumbs) - on
  all 22 BF1943 undeploys BFBC2 has, exactly the 25 DICE added (undeploy_tracks).
- AimAnimation (third-person aim poses): to little-endian with the bone table renumbered onto BFBC2's soldier
  (bfbc2-anim-tools aimanim.to_pc).
Everything else is listed with the reason it is not converted yet: MeshData and mesh sets (need PC vertex layouts
from a shader database), Havok physics (BF1943's Havok differs from BFBC2's), DBX partitions (another game's
data), sounds.

The game copy and the source are never written; <out> may not lie inside either.
"""
import contextlib
import hashlib
import json
import os
import re
import struct
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'mesh-viewer', 'console-convert', 'anim-tools', 'core')
import aimanim  # noqa: E402
import granny  # noqa: E402
from bfbc2core import errors, paths  # noqa: E402

SOLDIER = 'characters/skeletons/ske01.res'
UNDEPLOY = re.compile(r'/3p/(stand|crouch)undeploy\.res$')
PARKED = 0.5                        # m: a deploy's weapon part this far from the HandsIkPose is parked (undeploy_tracks)
TRACK_FORMATS = 'animations/weapons/handheld/m1garand/3p/standundeploy.res'  # DICE's: the curve formats added
ANIMATIONS = ('GrannyAnimation', 'DeltaAnimation')
NOT_YET = {
    'MeshData': 'mesh data: needs PC vertex layouts from a shader database',
    'RigidMeshSet': 'mesh set: needs its meshes converted first',
    'SkinnedMeshSet': 'mesh set: needs its meshes converted first',
    'CompositeMeshSet': 'mesh set: needs its meshes converted first',
    'HavokPhysicsData': "physics: BF1943's Havok differs from BFBC2's",
    'AnimTreeInfo': 'animation tree: format not decoded',
    '<non-resource>': "DBX partition: another game's data",
}


class Bf1943ImportError(Exception):
    what_to_do = ('Check the names with "list", the --source folder (BF1943\'s extracted dist\\xenon) and --out; '
                  'python bf1943import.py --help shows the commands.')


def default_source():
    """BF1943's extracted Xbox 360 bundles: BF1943_XENON when set (unchecked), else Project Rising Sun's workspace
    beside the suite (operation-rising-sun\\work\\extracted\\dist\\xenon)."""
    return paths.sibling(paths.BF1943_WORKSPACE, 'work', 'extracted', 'dist', 'xenon', env='BF1943_XENON', suite=SUITE)


def fbrb1943():
    """The BF1943 tool suite's bundle reader: bf1943-tool-suite\\bf1943-fbrb-tool beside the suite FIRST, then
    BF1943_SUITE_FBRB (the opposite order to default_source's). Put on sys.path once."""
    p = paths.find_sibling(paths.BF1943_SUITE, 'bf1943-fbrb-tool', marker='fbrb1943.py', env='BF1943_SUITE_FBRB',
                           suite=SUITE)
    if p:
        paths.use_folder(p)
        import fbrb1943 as m
        return m
    raise Bf1943ImportError('the BF1943 tool suite (bf1943-tool-suite\\bf1943-fbrb-tool\\fbrb1943.py) was not found next to '
                       'this suite: its bundle reader reads the Xbox 360 bundles')


def _bundle_files(folder):
    """[(rel, path)] of the .fbrb files under folder, in the order Source has always taken them (os.walk, each
    folder's files sorted)."""
    out = []
    for dp, _, fs in os.walk(folder):
        for f in sorted(fs):
            if f.endswith('.fbrb'):
                path = os.path.join(dp, f)
                out.append((os.path.relpath(path, folder).replace('\\', '/'), path))
    return out


class WholeSource:
    """The BF1943 resources by name (lower case; the first bundle holding a name wins - its copies are identical),
    every bundle read whole and kept (raw bytes and decompressed payload: ~1.2 GB for the full source). The reference
    for Source (round E3r), which reads the same bytes one bundle at a time; BF1943_SOURCE_WHOLE=1 makes Source this."""

    def __init__(self, folder):
        if not os.path.isdir(folder):
            raise Bf1943ImportError(f'no BF1943 source folder at {folder} (pass --source: the extracted dist\\xenon folder)')
        self.folder = os.path.abspath(folder)
        fb = fbrb1943()
        self.bundles: dict[str, Any]
        self.entries: dict[str, tuple[str, dict[str, Any]]]
        self.bundles, self.entries = {}, {}
        for rel, path in _bundle_files(self.folder):
            try:
                bd = fb.Bundle(open(path, 'rb').read())
            except ValueError:                          # the three bundles of another layout (fbrb1943's docstring)
                continue
            self.bundles[rel] = bd
            for e in bd.entries:
                self.entries.setdefault(e['name'].lower(), (rel, e))
        if not self.entries:
            raise Bf1943ImportError(f'{folder}: no BF1943 bundles in it')

    def data(self, key):
        rel, e = self.entries[key]
        return self.bundles[rel].data(e['name'])

    def find(self, filters):
        fl = [f.lower().replace('\\', '/') for f in filters]
        return sorted(k for k in self.entries if all(f in k for f in fl))


# ------------------------------------------------------------------------------------------------ the lazy source

INDEX_VERSION = 1
MEMORY_ENV = 'BF1943_SOURCE_MEMORY_MB'      # decoded spans of compressed payloads kept in the process (default MEMORY_MB)
MEMORY_MB = 128
SPAN = 1 << 20                              # output bytes between a compressed payload's restart points
CHUNK = 1 << 16                             # compressed bytes fed to the inflater at a time while the points are made
INDEX_ENV = 'BF1943_SOURCE_INDEX'           # the bundle index's folder; 0: none (every bundle checked on each open)


def index_dir():
    """Where Source keeps its bundle index: BF1943_SOURCE_INDEX (a folder; 0: none), else
    %LOCALAPPDATA%\\bfbc2-tool-suite\\cache\\bf1943source (bfbc2core.caches 'bf1943source'). None: no index."""
    v = os.environ.get(INDEX_ENV, '')
    if v == '0' or v.lower() == 'off':
        return None
    return v or os.path.join(os.environ.get('LOCALAPPDATA') or os.path.expanduser('~'), 'bfbc2-tool-suite', 'cache',
                             'bf1943source')


def _reader_signature(fb):
    """The bundle reader's own source (fbrb1943.py): its checks decide which bundles a Source takes."""
    try:
        return hashlib.sha1(open(fb.__file__, 'rb').read()).hexdigest()
    except (OSError, AttributeError, TypeError):
        return 'unknown'


class _BundleInfo:
    """What Source keeps of a bundle: its file's identity, its entries, where its payload starts in the file."""
    __slots__ = ('path', 'size', 'mtime_ns', 'entries', 'compressed', 'payload_at', 'payload_len')

    def __init__(self, path, size, mtime_ns, entries, compressed, payload_at, payload_len):
        self.path, self.size, self.mtime_ns = path, size, mtime_ns
        self.entries, self.compressed, self.payload_at, self.payload_len = entries, compressed, payload_at, payload_len


class _Bundles(dict):
    """Source.bundles: {rel: the bundle read whole (fbrb1943.Bundle)} as WholeSource had it, each read when asked
    for (not kept). Iterating, len and `in` need no reads."""

    def __init__(self, source, rels):
        super().__init__((r, None) for r in rels)
        self._source = source

    def __getitem__(self, rel):
        if not dict.__contains__(self, rel):
            raise KeyError(rel)
        return self._source._whole(rel)

    def get(self, rel, default=None):
        return self[rel] if dict.__contains__(self, rel) else default

    def values(self):
        return [self[r] for r in self]

    def items(self):
        return [(r, self[r]) for r in self]


class Source:
    """The BF1943 resources by name (lower case; the first bundle holding a name wins - its copies are identical).

    Round E3r: the bundles are no longer held whole (WholeSource kept every raw bundle and decompressed payload: ~1.2
    GB for BF1943's 82 bundles). Opening checks each bundle with fbrb1943.Bundle exactly as WholeSource reads it (the
    same bundles taken, the same entries, in the same order) and keeps only the entries; data() reads an entry from a
    stored payload where it lies, and from a compressed one through restart points (_points: a compressed payload is
    decompressed once per process, then any entry costs at most SPAN bytes of inflating) and the decoded spans of the
    last reads (BF1943_SOURCE_MEMORY_MB, default 128). The check's result per bundle -
    keyed by the file's path, size and modification time and the reader's own source - is kept on disk (index_dir(),
    a few MB), so a later open reads no payload at all. A bundle changed on disk after the open is refused when a
    read needs its file (Bf1943ImportError), never read as other bytes; what was already decoded is answered as it
    was read (WholeSource kept every bundle as it was at the open). data() gives WholeSource's bytes for every entry
    (tests/test_bf1943import.py --game: every entry of every bundle)."""

    def __new__(cls, folder, *a, **kw):
        if cls is Source and os.environ.get('BF1943_SOURCE_WHOLE', '') not in ('', '0'):
            return WholeSource(folder)
        return super().__new__(cls)

    def __init__(self, folder, memory_mb=None):
        if not os.path.isdir(folder):
            raise Bf1943ImportError(f'no BF1943 source folder at {folder} (pass --source: the extracted dist\\xenon folder)')
        self.folder = os.path.abspath(folder)
        if memory_mb is None:
            try:
                memory_mb = float(os.environ.get(MEMORY_ENV, '') or MEMORY_MB)
            except ValueError:
                memory_mb = MEMORY_MB
        self.budget = int(max(memory_mb, 0) * 2 ** 20)
        self._spans: dict[tuple[str, int], bytes] = {}  # (rel, span) -> decoded bytes, least recently used first
        self._held = 0
        self._restarts: dict[str, Any] = {}            # rel -> restart points (_points)
        import threading
        self._lock = threading.Lock()
        self.stats = {'checked': 0, 'from_index': 0, 'indexed': 0, 'spans': 0, 'hits': 0, 'whole': 0,
                      'stored_reads': 0, 'decompressed_bytes': 0}
        self.info: dict[str, _BundleInfo] = {}
        self.entries: dict[str, tuple[str, dict[str, Any]]] = {}
        fb = fbrb1943()
        files = _bundle_files(self.folder)
        index, index_path = self._load_index(fb)
        fresh = {}
        for rel, path in files:
            try:
                st = os.stat(path)
            except OSError:
                st = None
            row = index.get(rel) if st is not None else None
            if row is not None and st is not None and row[0] == st.st_size and row[1] == st.st_mtime_ns:
                self.stats['from_index'] += 1
            else:
                row = self._check(fb, path, st)
                self.stats['checked'] += 1
            if st is not None:
                fresh[rel] = row
            if row[2] is None:                          # refused: one of the three bundles of another layout
                continue
            entries, compressed, payload_at, payload_len = row[2], row[3], row[4], row[5]
            self.info[rel] = _BundleInfo(path, row[0], row[1], entries, compressed, payload_at, payload_len)
            for e in entries:
                self.entries.setdefault(e['name'].lower(), (rel, e))
        if not self.entries:
            raise Bf1943ImportError(f'{folder}: no BF1943 bundles in it')
        self.bundles = _Bundles(self, list(self.info))
        if index_path and fresh != index:
            self._save_index(index_path, fresh, fb)

    # ---- the index of checked bundles
    def _index_path(self, fb):
        d = index_dir()
        if not d:
            return None
        key = hashlib.sha1(os.path.normcase(self.folder).encode('utf-8')).hexdigest()[:20]
        return os.path.join(d, f'{key}.pkl')

    def _load_index(self, fb):
        p = self._index_path(fb)
        if p is None:
            return {}, None
        try:
            import pickle
            with open(p, 'rb') as fh:
                got = pickle.load(fh)
            if (got.get('version') == INDEX_VERSION and got.get('reader') == _reader_signature(fb) and
                    got.get('folder') == os.path.normcase(self.folder)):
                from bfbc2core import caches
                caches.touch(p)
                return got['bundles'], p
        except Exception:                                   # noqa: BLE001 - missing, old or damaged: checked again
            pass
        return {}, p

    def _save_index(self, p, rows, fb):
        import pickle
        try:
            from bfbc2core import atomicio, caches
            os.makedirs(os.path.dirname(p), exist_ok=True)
            atomicio.write_bytes(p, pickle.dumps({'version': INDEX_VERSION, 'reader': _reader_signature(fb),
                                                  'folder': os.path.normcase(self.folder), 'bundles': rows},
                                                 protocol=pickle.HIGHEST_PROTOCOL))
            caches.maybe_prune('bf1943source', os.path.dirname(p))
        except OSError:
            pass                                            # only a cache: the next open checks the bundles again

    def _check(self, fb, path, st):
        """(size, mtime_ns, entries or None when fbrb1943 refuses the bundle, compressed, payload start, payload
        length) - fbrb1943.Bundle's verdict on the whole file, as WholeSource reads it."""
        size, mtime = (st.st_size, st.st_mtime_ns) if st is not None else (-1, -1)
        try:
            raw = open(path, 'rb').read()
            bd = fb.Bundle(raw)
        except ValueError:
            return (size, mtime, None, False, 0, 0)
        # (the payload is not kept: one bundle's raw bytes and payload at a time is the open's whole footprint)
        return (size, mtime, bd.entries, bool(bd.compressed), 8 + struct.unpack_from('>I', raw, 4)[0], len(bd.payload))

    # ---- reading
    # A compressed payload is one gzip stream. Its first read decompresses it once, start to end, and keeps restart
    # points: every SPAN bytes of output, the inflater's whole state (zlib's Decompress.copy(): its window and bit
    # position) and how far into the stream it had read. The output between two points is then a span: the earlier
    # point's state copied and fed the stream's bytes up to the later point gives exactly those bytes (the same input
    # into the same state). An entry is read from the spans it covers; decoded spans are kept, least recently used
    # dropped first, within the budget. (E3e's bundle cache could not do this in Rust: the safe inflaters have no
    # inflatePrime; Python's zlib copies the state whole.)
    def _keep(self, key, data):
        """A decoded span kept as the most recently used (the least recently used dropped beyond the budget)."""
        if len(data) > self.budget:
            return
        old = self._spans.pop(key, None)
        if old is not None:
            self._held -= len(old)
        self._spans[key] = data
        self._held += len(data)
        while self._held > self.budget and len(self._spans) > 1:
            self._held -= len(self._spans.pop(next(iter(self._spans))))

    def _changed(self, rel, b):
        try:
            st = os.stat(b.path)
        except OSError as e:
            raise Bf1943ImportError(f'{rel}: the BF1943 bundle cannot be read any more ({e})') from None
        if (st.st_size, st.st_mtime_ns) != (b.size, b.mtime_ns):
            raise Bf1943ImportError(f'{rel}: the BF1943 bundle changed on disk after the source was opened; open it '
                                    f'again')

    def _body(self, rel, b, start=0, end=None):
        """The compressed payload's bytes [start, end) (the file checked unchanged first)."""
        self._changed(rel, b)
        with open(b.path, 'rb') as fh:
            fh.seek(b.payload_at + start)
            return fh.read() if end is None else fh.read(end - start)

    def _points(self, rel, b):
        """The restart points of a compressed payload: ([input offsets], [output offsets], [inflater states]), the
        first at 0 and a last (input end, payload length, None); or False when the stream is not one plain gzip
        member (then the payload is decompressed whole on each read, as WholeSource did). The spans decoded on the
        way are kept as far as the budget allows."""
        got = self._restarts.get(rel)
        if got is not None:
            return got
        import zlib
        body = self._body(rel, b)
        d = zlib.decompressobj(31)
        ins, outs = [0], [0]
        states: list[Any] = [d.copy()]
        pieces: list[bytes] = []
        out = 0
        ok = True
        n = len(body)
        for at in range(0, max(n, 1), CHUNK):
            try:
                o = d.decompress(body[at:at + CHUNK])
            except zlib.error:
                ok = False
                break
            out += len(o)
            pieces.append(o)
            if d.eof:
                ok = not d.unused_data and at + CHUNK >= n
                break
            if out - outs[-1] >= SPAN and at + CHUNK < n:
                self._keep((rel, len(ins) - 1), b''.join(pieces))
                pieces = []
                ins.append(at + CHUNK)
                outs.append(out)
                states.append(d.copy())
        self.stats['indexed'] += 1
        self.stats['decompressed_bytes'] += out
        if not ok or not d.eof or out != b.payload_len:
            self._restarts[rel] = False
            return False
        self._keep((rel, len(ins) - 1), b''.join(pieces))
        ins.append(n)
        outs.append(out)
        states.append(None)
        got = (ins, outs, states)
        self._restarts[rel] = got
        return got

    def _span(self, rel, b, points, j):
        key = (rel, j)
        got = self._spans.get(key)
        if got is not None:
            self._spans[key] = self._spans.pop(key)           # most recently used
            self.stats['hits'] += 1
            return got
        ins, outs, states = points
        d = states[j].copy()
        data = d.decompress(self._body(rel, b, ins[j], ins[j + 1]))
        if len(data) != outs[j + 1] - outs[j]:
            raise Bf1943ImportError(f'{rel}: the BF1943 bundle\'s payload changed after the source was opened')
        self.stats['spans'] += 1
        self.stats['decompressed_bytes'] += len(data)
        self._keep(key, data)
        return data

    def _compressed(self, rel, b, offset, size):
        points = self._points(rel, b)
        if points is False:                                     # not one plain member: the whole stream, each time
            import gzip
            import zlib
            try:
                payload = gzip.decompress(self._body(rel, b))
            except (OSError, EOFError, zlib.error) as e:
                raise Bf1943ImportError(f'{rel}: the BF1943 bundle\'s payload no longer decompresses ({e})') from None
            if len(payload) != b.payload_len:
                raise Bf1943ImportError(f'{rel}: the BF1943 bundle\'s payload changed after the source was opened')
            self.stats['whole'] += 1
            return payload[offset:offset + size]
        if size == 0:
            return b''
        import bisect
        outs = points[1]
        j = bisect.bisect_right(outs, offset) - 1
        parts = []
        end = offset + size
        while offset < end:
            span = self._span(rel, b, points, j)
            lo = offset - outs[j]
            take = span[lo:lo + (end - offset)]
            parts.append(take)
            offset += len(take)
            j += 1
        return parts[0] if len(parts) == 1 else b''.join(parts)

    def data(self, key):
        rel, e = self.entries[key]
        b = self.info[rel]
        with self._lock:
            if b.compressed:
                return self._compressed(rel, b, e['offset'], e['size'])
            self._changed(rel, b)
            self.stats['stored_reads'] += 1
            with open(b.path, 'rb') as fh:
                fh.seek(b.payload_at + e['offset'])
                out = fh.read(e['size'])
            if len(out) != e['size']:
                raise Bf1943ImportError(f'{rel}: the BF1943 bundle is shorter than when the source was opened')
            return out

    def _whole(self, rel):
        """The bundle read whole (fbrb1943.Bundle), as WholeSource kept it."""
        b = self.info[rel]
        self._changed(rel, b)
        return fbrb1943().Bundle(open(b.path, 'rb').read())

    def drop(self):
        """Forget the decoded spans and restart points kept (the entries stay; the next reads decompress again)."""
        with self._lock:
            self._spans.clear()
            self._restarts.clear()
            self._held = 0

    def find(self, filters):
        fl = [f.lower().replace('\\', '/') for f in filters]
        return sorted(k for k in self.entries if all(f in k for f in fl))


def plan(kind):
    if kind in ANIMATIONS:
        return 'animation to PC (bones renumbered onto BFBC2\'s soldier when it animates the soldier)'
    if kind == 'GrannyModel':
        return 'skeleton to PC'
    if kind == 'AimAnimation':
        return "aim animation to PC (bones renumbered onto BFBC2's soldier)"
    if kind == 'XenonTexture':
        return 'texture to PC'
    return 'not converted: ' + NOT_YET.get(kind, f'{kind}: no converter')


class Importer:
    def __init__(self, source, opfor=None, thumb_donors=None):
        """thumb_donors: {BF1943 weapon animation folder (lower case, e.g. animations/weapons/handheld/beach/k98):
        a BFBC2 weapon animation folder of the same base set (e.g. animations/weapons/handheld/us_sni_m24)} - the 3p
        undeploys under the first take their thumb tracks from the second's same-stance undeploy (DICE's animated
        curves, copied) instead of the deploy's last frame (undeploy_tracks)."""
        self.src = source
        self._bones = None
        self._opfor = opfor
        self.thumb_donors = {k.lower().rstrip('/'): v.lower().rstrip('/') for k, v in (thumb_donors or {}).items()}
        src_ske = source.entries.get(SOLDIER)
        self.src_soldier = source.data(SOLDIER) if src_ske else None

    def soldier_map(self):
        """{BF1943 bone index: BFBC2 bone index} and the BF1943 soldier's bone names."""
        if self._bones is None:
            if self.src_soldier is None:
                raise Bf1943ImportError(f'the source has no {SOLDIER} (the BF1943 soldier skeleton)')
            import meshview
            import shaderdb
            ix = meshview.Index(shaderdb.find_opfor(self._opfor))
            pc = ix.read(SOLDIER)
            if pc is None:
                raise Bf1943ImportError(f'the game copy has no {SOLDIER}')
            names = {b['Name'] for b in granny.Granny(self.src_soldier).root()['Skeletons'][0]['Bones']}
            self._bones = (granny.bone_map(self.src_soldier, pc), names)
        return self._bones

    def index(self):
        """The game copy's stock bundles (meshview.Index), opened once."""
        if getattr(self, '_ix', None) is None:
            import meshview
            import shaderdb
            self._ix = meshview.Index(shaderdb.find_opfor(self._opfor))
        return self._ix

    def game_read(self, name):
        """index().read(name), each name read once per Importer (round E3r: every 3p undeploy read DICE's Garand
        undeploy - the curve formats - again, 0.05 s from its bundle each time; bytes are not changed by callers)."""
        memo = self.__dict__.setdefault('_game_reads', {})
        if name not in memo:
            memo[name] = self.index().read(name)
        return memo[name]

    def undeploy_tracks(self, key, out):
        """(PC bytes, note) of a converted third-person undeploy with the tracks DICE adds: the bones its weapon's
        same-stance deploy animates and it does not (BF1943's 3p undeploys: 63 tracks, the deploys 88 - 19 weapon
        parts and 6 thumbs, the 25 DICE added to all 22 it shipped). Each holds the deploy's last frame (DICE's rule
        for the weapon parts; DICE's thumbs are animated, which no BF1943 file has - the deploy's last frame is 13 deg
        from them on average, the bind pose 20), but a weapon part the deploy has parked 0.5 m or more from the
        weapon's HandsIkPose takes that pose (the crouching Garand's clip parts). Curve formats from DICE's own
        Garand undeploy (DaIdentity, D3 / D4Constant32f). Measured on the 22: tests\\test_bf1943import.py.
        With a thumb donor (thumb_donors) the 6 thumbs are that BFBC2 undeploy's curves, copied. Measured leave-one-out
        on the 22 (2026-11-09): exact where the donor is the same animation (Vietnam's Garand, Thompson, M1911, TNT -
        DICE shipped them twice), but another weapon of the same base set does not beat the deploy's last frame
        (thumbs vs DICE's, median / mean degrees, all 22: deploy's last frame 8.0 / 13.4, nearest-kind donors 6.9 /
        13.7 and 8.7 / 12.7; the Garand from the M24 9.1 / 15.1 standing, 21.0 / 22.9 crouching, against 0.1 / 7.3
        and 4.8 / 9.0)."""
        m = UNDEPLOY.search(key)
        if not m:
            return out, None
        dep = key[:m.start()] + f'/3p/{m.group(1)}deploy.res'
        if dep not in self.src.entries:
            return out, None
        have = {t['Name'] for t in granny.animations(granny.Granny(out, key))[0]['TrackGroups'][0]['TransformTracks']}

        def tracks(k):
            # a deploy's and a HandsIkPose's tracks, read only here: converted once per Importer (round E3r; check()
            # converts each undeploy twice, and both stances share a weapon's HandsIkPose)
            memo = self.__dict__.setdefault('_tracks', {})
            if k not in memo:
                g = granny.Granny(self.convert(k, extra_tracks=False)[0], k)
                memo[k] = {t['Name']: t for t in granny.animations(g)[0]['TrackGroups'][0]['TransformTracks']}
            return memo[k]
        dtr = tracks(dep)
        ik_key = key[:m.start()] + '/3p/handsikpose.res'
        ik = tracks(ik_key) if ik_key in self.src.entries else {}

        def last(t, curve):
            k = granny.curve_keys(t[curve]['CurveData'])
            return k[-1][1] if k else None
        add = []
        for name in sorted(set(dtr) - have):
            pos, ori = last(dtr[name], 'PositionCurve'), last(dtr[name], 'OrientationCurve')
            if name in ik and name.startswith('jntWpn'):
                ip, io = last(ik[name], 'PositionCurve'), last(ik[name], 'OrientationCurve')
                d = sum((a - b) ** 2 for a, b in zip(pos or (0, 0, 0), ip or (0, 0, 0))) ** 0.5
                if d >= PARKED:
                    pos, ori = ip, io
            add.append({'name': name, 'position': ('constant', pos) if pos else None,
                        'orientation': ('constant', ori) if ori else None})
        if not add:
            return out, None
        donor = self.game_read(TRACK_FORMATS)
        if donor is None:
            raise Bf1943ImportError(f'the game copy has no {TRACK_FORMATS} (the curve formats for the added tracks)')
        # the thumbs from a same-base-set BFBC2 undeploy (thumb_donors): its curves copied as they are
        thumb_key = None
        folder = self.thumb_donors.get(key[:m.start()])
        if folder:
            thumb_key = f'{folder}/3p/{m.group(1)}undeploy.res'
            tb = self.game_read(thumb_key)
            if tb is None:
                raise Bf1943ImportError(f'{key}: thumb donor {thumb_key} is not in the game copy')
            tt = {t['Name']: t for t in granny.animations(granny.Granny(tb, thumb_key))[0]['TrackGroups'][0]
                  ['TransformTracks']}
            thumbs = [a['name'] for a in add if 'Thumb' in a['name']]
            missing = [n for n in thumbs if n not in tt]
            if missing:
                raise Bf1943ImportError(f'{key}: thumb donor {thumb_key} has no {missing}')
            add = [a for a in add if a['name'] not in thumbs]
            copied = [{'name': n, 'position': ('copy', tt[n]['PositionCurve']['CurveData']),
                       'orientation': ('copy', tt[n]['OrientationCurve']['CurveData'])} for n in thumbs]
        out = granny.add_tracks(out, add, donor, key) if add else out
        note = f'{len(add)} tracks added from {dep.rsplit("/", 1)[-1]} (the bones its deploy animates, held at its ' \
            f'last frame)'
        if thumb_key:
            out = granny.add_tracks(out, copied, tb, key)
            note += f'; {len(copied)} thumb tracks copied from {thumb_key}'
        return out, note

    def convert(self, key, extra_tracks=True):
        """(output bytes or None, record) for one resource; extra_tracks: a 3p undeploy gets DICE's added tracks
        (undeploy_tracks)."""
        rel, e = self.src.entries[key]
        b = self.src.data(key)
        rec = {'name': e['name'], 'type': e['type'], 'bundle': rel, 'sha256': hashlib.sha256(b).hexdigest()}
        kind = e['type']
        out = None
        if kind in ANIMATIONS:
            g = granny.Granny(b, e['name'])
            tracks = {t['Name'] for a in granny.animations(g) for tg in a['TrackGroups'] for t in tg['TransformTracks']}
            words = granny.struct.unpack_from('>%dI' % (g.strings_from // 4), b, 0)
            table, at = granny.bone_table(list(words))
            bones, names = self.soldier_map()
            if tracks and tracks <= names:
                out = granny.to_pc(b, bones, e['name'])
                rec['done'] = f'animation to PC; {table} table of {len(at)} bone(s) renumbered onto BFBC2\'s soldier'
                if extra_tracks and kind == 'GrannyAnimation':
                    out, note = self.undeploy_tracks(key, out)
                    if note:
                        rec['done'] += '; ' + note
            else:
                out = granny.to_pc(b, None, e['name'])
                rec['done'] = 'animation to PC; bone indices kept (not a soldier animation)'
                if at:
                    rec['note'] = 'its bone table indexes another skeleton: check that skeleton\'s bone order'
        elif kind == 'AimAnimation':
            bones, _ = self.soldier_map()
            out = aimanim.to_pc(b, bones)
            rec['done'] = "aim animation to PC; bone table renumbered onto BFBC2's soldier"
        elif kind == 'GrannyModel':
            out = granny.to_pc(b, None, e['name'])
            rec['done'] = 'skeleton to PC'
        elif kind == 'XenonTexture':
            import texture
            try:
                out = texture.convert(b)
                rec['done'] = 'texture to PC'
            except texture.ConvertError as er:
                rec['done'] = f'not converted: {er}'
        else:
            rec['done'] = 'not converted: ' + NOT_YET.get(kind, f'{kind}: no converter')
        if out is not None:
            rec['out_sha256'] = hashlib.sha256(out).hexdigest()
        return out, rec


def _inside(path, folder):
    path, folder = os.path.normcase(os.path.abspath(path)), os.path.normcase(os.path.abspath(folder))
    return path == folder or path.startswith(folder.rstrip('\\/') + os.sep)


def target(out_dir, name):
    """<out_dir>/<name>, refused when the name would leave out_dir."""
    p = os.path.abspath(os.path.join(out_dir, *name.replace('\\', '/').split('/')))
    if not _inside(p, out_dir):
        raise Bf1943ImportError(f'{name}: the name leads outside the output folder')
    return p


def run_import(source, filters, out_dir, opfor=None, log=print):
    import shaderdb
    if not filters:
        raise Bf1943ImportError('import needs at least one filter (part of a resource name)')
    keys = source.find(filters)
    if not keys:
        raise Bf1943ImportError(f'no BF1943 resource name contains {" and ".join(filters)} (list shows the names)')
    game = None
    with contextlib.suppress(Exception):                    # no game copy: only the source is guarded
        game = os.path.join(shaderdb.find_opfor(opfor), 'game')
    for guarded in (source.folder, game):
        if guarded and _inside(out_dir, guarded):
            raise Bf1943ImportError(f'{out_dir}: the output folder may not lie inside {guarded}')
    if any(source.entries[k][1]['type'] == 'XenonTexture' for k in keys):
        import texture
        texture.require()                   # textures convert in the Rust core: say so before converting anything
    imp = Importer(source, opfor)
    records = []
    for k in keys:
        out, rec = imp.convert(k)
        if out is not None:
            p = target(out_dir, rec['name'])
            os.makedirs(os.path.dirname(p), exist_ok=True)
            tmp = p + '.part'
            open(tmp, 'wb').write(out)
            os.replace(tmp, p)
            rec['out'] = os.path.relpath(p, out_dir).replace('\\', '/')
        records.append(rec)
        log(f'{rec["name"]}: {rec["done"]}')
    os.makedirs(out_dir, exist_ok=True)
    man = os.path.join(out_dir, 'import.json')
    old = {}
    if os.path.isfile(man):
        old = {r['name'].lower(): r for r in json.load(open(man, encoding='utf-8')).get('resources', [])}
    old.update({r['name'].lower(): r for r in records})
    tmp = man + '.part'
    json.dump({'source': source.folder, 'resources': sorted(old.values(), key=lambda r: r['name'].lower())},
              open(tmp, 'w', encoding='utf-8'), indent=1)
    os.replace(tmp, man)
    done = sum(1 for r in records if 'out' in r)
    log(f'{done} of {len(records)} resources converted into {out_dir} (records: import.json)')
    return records


def clean(v):
    if isinstance(v, dict):
        return {k: clean(x) for k, x in v.items() if not k.startswith('__') and k != 'Padding'}
    if isinstance(v, list):
        return [clean(x) for x in v]
    return v


def same_clean(a, b):
    """clean(a) == clean(b), without building the cleaned copies (round E3r: check() spent 12 s of the import test in
    clean - 14 million calls). Equal trees are equal cleaned (the same keys are left out of both), so the plain
    comparison - in C - answers first; only where it fails is the tree walked, leaving out the keys clean() drops
    ('__...', 'Padding'). Leaves compare as dict and list items do (identity, then ==); anything clean() does not
    walk on one side only is compared cleaned, as before."""
    if not isinstance(a, (dict, list)) and not isinstance(b, (dict, list)):
        return a == b                                   # two leaves: clean() leaves them as they are
    return _same_inside(a, b)


def _same_inside(a, b):
    """same_clean of two items inside a dict or list (compared as such: identity first)."""
    if a is b or a == b:
        return True
    da, db = isinstance(a, dict), isinstance(b, dict)
    if da and db:
        ka = [k for k in a if not k.startswith('__') and k != 'Padding']
        kb = [k for k in b if not k.startswith('__') and k != 'Padding']
        return len(ka) == len(kb) and all(k in b and _same_inside(a[k], b[k]) for k in ka)
    la, lb = isinstance(a, list), isinstance(b, list)
    if la and lb:
        return len(a) == len(b) and all(_same_inside(x, y) for x, y in zip(a, b))
    if da or db or la or lb:
        return clean(a) == clean(b)
    return False


def check(source, opfor=None, log=print, cleaned=None):
    """Converts every animation and skeleton and checks each output; returns the failures. cleaned: the tree
    comparison (default same_clean; tests pass the reference, lambda a, b: clean(a) == clean(b))."""
    same = cleaned or same_clean
    imp = Importer(source, opfor)
    bones, names = imp.soldier_map()
    pc_names = None
    src_names = None
    fails: list[str]
    counts: dict[str, int]
    fails, counts = [], {}
    for k, (_rel, e) in sorted(source.entries.items()):
        if e['type'] not in ANIMATIONS + ('GrannyModel',):
            continue
        try:
            out, rec = imp.convert(k)
            full = out
            if 'tracks added' in rec['done']:                   # a 3p undeploy: the tree is checked without them
                out = imp.convert(k, extra_tracks=False)[0]
                old = {t['Name']: t for t in granny.animations(granny.Granny(out))[0]['TrackGroups'][0]['TransformTracks']}
                new = {t['Name']: t for t in granny.animations(granny.Granny(full))[0]['TrackGroups'][0]['TransformTracks']}
                if any(n not in new or not same(new[n], t) for n, t in old.items()):
                    raise granny.GrannyError('adding tracks changed a track it had')
            src, cv = granny.Granny(source.data(k), e['name']), granny.Granny(out, e['name'])
            if cv.e != '<':
                raise granny.GrannyError('output is not little-endian')
            if not same(src.root(), cv.root()):
                raise granny.GrannyError('the object tree changed')
            if 'renumbered' in rec['done']:
                if pc_names is None:
                    import meshview
                    import shaderdb
                    pc = meshview.Index(shaderdb.find_opfor(opfor)).read(SOLDIER)
                    pc_names = [b['Name'] for b in granny.Granny(pc).root()['Skeletons'][0]['Bones']]
                if src_names is None:                           # (the same soldier for every animation)
                    src_names = [b['Name'] for b in granny.Granny(imp.src_soldier).root()['Skeletons'][0]['Bones']]
                w_src = granny.struct.unpack_from('>%dI' % (src.strings_from // 4), source.data(k), 0)
                w_out = granny.struct.unpack_from('<%dI' % (cv.strings_from // 4), out, 0)
                _, a_src = granny.bone_table(list(w_src))
                _, a_out = granny.bone_table(list(w_out))
                if sorted(src_names[w_src[i]] for i in a_src) != sorted(pc_names[w_out[i]] for i in a_out):
                    raise granny.GrannyError('the renumbered bone table names other bones')
            counts[e['type']] = counts.get(e['type'], 0) + 1
        except Exception as er:                             # noqa: BLE001 - every failure is reported
            fails.append(f'{e["name"]}: {er}')
    log(', '.join(f'{v} {k}' for k, v in sorted(counts.items())) + f' converted and checked; {len(fails)} failed')
    for f in fails[:30]:
        log('FAIL ' + f)
    return fails


def main(argv):
    if any(a in ('-h', '--help', '/?') for a in argv[1:]):
        print(__doc__)
        return 0
    args, opts = [], {}
    it = iter(argv[1:])
    for a in it:
        if a.startswith('--'):
            opts[a] = next(it, None)
            if opts[a] is None:
                raise Bf1943ImportError(f'{a} needs a value')
        else:
            args.append(a)
    if not args or args[0] not in ('list', 'import', 'check'):
        print(__doc__)
        return 1 if args else 0
    source = Source(opts.get('--source') or default_source())
    if args[0] == 'list':
        for k in source.find(args[1:]):
            rel, e = source.entries[k]
            print(f'{e["size"]:10d}  {e["type"]:20s}  {e["name"]}  [{rel}]  -> {plan(e["type"])}')
        return 0
    if args[0] == 'check':
        return 1 if check(source, opts.get('--opfor')) else 0
    out = opts.get('--out')
    if not out:
        import shaderdb
        out = os.path.join(shaderdb.find_opfor(opts.get('--opfor')), 'work', 'bf1943', 'import')
    run_import(source, args[1:], out, opts.get('--opfor'))
    return 0


if __name__ == '__main__':
    errors.setup('bf1943import', gui=False)    # a log; every error explained in words (exit code 1)
    with errors.guard():
        sys.exit(main(sys.argv))
