r"""DbxEdit's commands in process (bfbc2core.dbxedit, rust\bc2-formats\src\dbxedit) against what DbxEdit.exe
answered before it was retired (round R4, 2026-11-18: csharp_golden.py): the same files written, byte for byte, the
same messages and exit codes.

  python core\tests\test_dbxedit.py           quick: the cases of DbxEdit's unit tests (DbxTests.cs) on the same
                                              built files, each command as DbxEdit.exe answered it
                                              (dbxedit_golden.json) and as DbxTests.cs expects; the usage, option and
                                              damaged-file errors; the drop-in runner, the batch, the log
  python core\tests\test_dbxedit.py --game    + the corpus (Project OPFOR BFBC2's work\levels, work\navkit\common and
                                              work\ww2): every edit fingerprint of fingerprints_795745.txt, and the
                                              recorded sample (dbxedit_golden_game_0.json / _1.json: 2,000 files through xml,
                                              roundtrip, import --like / no reference / edited, check and the edit
                                              battery on every 4th) whose files are still there unchanged

Skipped (exit 0) when the Rust core is not built (python rust\build_native.py).
"""
import hashlib
import os
import random
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import time
from collections.abc import Callable
from concurrent.futures import ThreadPoolExecutor
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.abspath(os.path.join(HERE, '..', '..'))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core')
from bfbc2core import dbxedit, native, paths  # noqa: E402

if HERE not in sys.path:
    sys.path.insert(0, HERE)
import csharp_golden as cg  # noqa: E402

NATIVE: Any = native.load()
NATIVE = NATIVE if NATIVE is not None and hasattr(NATIVE, 'dbxedit_run') else None
FINGERPRINTS = os.path.join(SUITE, 'bfbc2-dbx-tools', 'tests', 'fingerprints_795745.txt')
GOLDEN = 'dbxedit_golden.json'
GOLDEN_GAME = 'dbxedit_golden_game_*.json'      # two shards (the repository keeps every file under 1 MB)
EXE = os.path.join('X:\\', 'retired', 'DbxEdit.exe')    # a command line naming DbxEdit: no file needs to be there

G1, G2, G3, G4 = ('11111111-1111-1111-1111-111111111111', '22222222-2222-2222-2222-222222222222',
                  '33333333-3333-3333-3333-333333333333', '44444444-4444-4444-4444-444444444444')
GP = '99999999-9999-9999-9999-999999999999'


# ------------------------------------------------------------------------------------------- built files
# DbxTests.cs' Builder: elements interned in its order into a table that starts with the empty string, written as
# DbxEdit's Serialize writes a new file.

def varint(v: int) -> bytes:
    v &= 0xFFFFFFFF
    out = bytearray()
    while v >= 0x80:
        out.append(v & 0x7F | 0x80)
        v >>= 7
    out.append(v)
    return bytes(out)


class Builder:
    def __init__(self, guid: str) -> None:
        self.table = ['']
        self.partition = self.container('partition', 'guid', guid, 'primaryInstance', guid)
        self.top = [self.partition]

    def intern(self, s: str) -> int:
        if s in self.table:
            return self.table.index(s)
        self.table.append(s)
        return len(self.table) - 1

    def e(self, tag: str, kind: int, *attrs: str) -> dict:
        el: dict[str, Any] = {'tag': self.intern(tag), 'kind': kind, 'attrs': [], 'children': None, 'payload': b'',
                              'cb': 0}
        for i in range(0, len(attrs) - 1, 2):
            el['attrs'].append((self.intern(attrs[i]), self.intern(attrs[i + 1])))
        return el

    def container(self, tag: str, *attrs: str) -> dict:
        el = self.e(tag, 0xA, *attrs)
        el['children'] = []
        return el

    def instance(self, ty: str, guid: str, id_: str | None = None) -> dict:
        i = (self.container('instance', 'guid', guid, 'type', ty) if id_ is None
             else self.container('instance', 'guid', guid, 'type', ty, 'id', id_))
        self.partition['children'].append(i)
        return i

    def text(self, parent: dict, name: str, value: str) -> dict:
        el = self.e('field', 2, 'name', name)
        el['payload'] = varint(self.intern(value))
        parent['children'].append(el)
        return el

    def numbers(self, parent: dict, tag: str, name: str, words: list[int]) -> dict:
        el = self.e(tag, 7, 'name', name)
        el['payload'] = varint(len(words)) + varint(4) + b''.join(struct.pack('>I', w & 0xFFFFFFFF) for w in words)
        parent['children'].append(el)
        return el

    def ints(self, parent: dict, tag: str, name: str, *v: int) -> dict:
        return self.numbers(parent, tag, name, list(v))

    def floats(self, parent: dict, tag: str, name: str, *v: float) -> dict:
        return self.numbers(parent, tag, name, [struct.unpack('>I', struct.pack('>f', x))[0] for x in v])

    def bool(self, parent: dict, name: str, v: bool) -> dict:
        el = self.e('field', 6, 'name', name)
        el['payload'] = bytes([1, 1 if v else 0])
        parent['children'].append(el)
        return el

    def ref(self, parent: dict, name: str, target: str) -> dict:
        el = self.e('field', 2, 'ref', target, 'name', name)
        el['payload'] = varint(self.intern(''))
        parent['children'].append(el)
        return el

    def ref_list(self, parent: dict, name: str, *targets: str) -> dict:
        a = self.container('array', 'name', name)
        for t in targets:
            item = self.e('item', 2, 'ref', t)
            item['payload'] = varint(self.intern(''))
            a['children'].append(item)
        parent['children'].append(a)
        return a

    def placed(self, guid: str, blueprint: str, x: float, y: float, z: float, slot: int) -> dict:
        i = self.instance('Entity.ReferenceObjectData', guid)
        self.ref(i, 'ReferencedObject', f'objects/props/{blueprint}/AAAAAAAA-0000-0000-0000-000000000001')
        self.floats(i, 'complex', 'Transform', 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, x, y, z, 1)
        self.ints(i, 'field', 'Enumeration', slot)
        return i

    def _write(self, out: bytearray, el: dict) -> None:
        out += varint(el['tag'])
        out.append(el['kind'] << 4 | len(el['attrs']))
        for k, v in el['attrs']:
            out += varint(k) + varint(v)
        if el['kind'] != 0xA:
            out += el['payload']
            return
        out.append(el['cb'])
        for c in el['children']:
            self._write(out, c)
        out.append(0)

    def bytes(self) -> bytes:
        area = bytearray()
        offsets = []
        for s in self.table:
            offsets.append(len(area))
            area += s.encode('utf-8') + b'\0'
        tree = bytearray()
        for el in self.top:
            self._write(tree, el)
        size = 4 * len(self.table) + len(area)
        head = b'{binary}' + struct.pack('>IIII', 24 + size, 0, size, len(self.table))
        return head + b''.join(struct.pack('>I', o) for o in offsets) + bytes(area) + bytes(tree)


def sample() -> Builder:
    b = Builder(GP)
    a = b.instance('Game.Thing', G1, 'First')
    b.text(a, 'Name', 'Levels/Test & <Demo>')
    b.ints(a, 'field', 'Count', 7)
    b.floats(a, 'field', 'Scale', 1.5)
    b.floats(a, 'field', 'One', 1.0)
    b.bool(a, 'Enabled', True)
    b.text(a, 'Empty', '')
    b.ref_list(a, 'Objects', G2, G3)
    c = b.instance('Game.Other', G2)
    b.ref(c, 'Partner', G1)
    b.ints(c, 'field', 'Id', 5)
    d = b.instance('Game.Other', G3)
    b.ref(d, 'Partner', G2)
    return b


def level_sample() -> Builder:
    b = Builder(GP)
    level = b.instance('GameSharedResources.LevelData', G1, 'Level')
    b.ref_list(level, 'Objects', G2, G3)
    group = b.instance('GameSharedResources.StaticModelGroupEntityData', G2)
    b.ref(group, 'PhysicsData', G3)
    terrain = b.instance('GameSharedResources.TerrainEntityData', G3)
    b.ref(terrain, 'PhysicsData', 'null')
    return b


# ------------------------------------------------------------------------------------------- running a case

Runner = Callable[[list[str]], tuple[int, str | bytes, str | bytes]]


def run_native(args: list[str]) -> tuple[int, str, str]:
    code, out, err, _ = NATIVE.dbxedit_run(args, dbxedit.log_path())
    return code, out, err


def snapshot(folder: str) -> dict[str, bytes]:
    out = {}
    for d, _, fs in os.walk(folder):
        for f in fs:
            p = os.path.join(d, f)
            with open(p, 'rb') as h:
                out[os.path.relpath(p, folder)] = h.read()
    return out


def same(want: list[Any], got: list[Any]) -> tuple[bool, str]:
    """want and got as [exit code, stdout, stderr, files]: what differs."""
    if want[0] != got[0]:
        return False, f'exit code {want[0]} / {got[0]}'
    if want[1] != got[1]:
        return False, f'stdout {want[1]!r} / {got[1]!r}'
    if want[2] != got[2]:
        return False, f'stderr {want[2]!r} / {got[2]!r}'
    if want[3] != got[3]:
        diff = sorted(k for k in set(want[3]) | set(got[3]) if want[3].get(k) != got[3].get(k))
        return False, f'files differ: {diff[:4]}'
    return True, ''


def write(path: str, data: bytes | str) -> None:
    with open(path, 'wb') as f:
        f.write(data.encode('utf-8') if isinstance(data, str) else data)


# The cases of DbxTests.cs, as command lines: (name, files to write {name: bytes|str}, args with {d} the folder,
# what DbxTests expects of the in-process result (or None)).

def cases() -> list[tuple[str, dict[str, Any], list[str], Any]]:
    s = sample().bytes()
    lvl = level_sample().bytes()
    out: list[tuple[str, dict[str, Any], list[str], Any]] = []
    # format and round trip
    out.append(('roundtrip of a built file', {'a.dbx': s}, ['roundtrip', '{d}\\a.dbx'],
                lambda r: r[0] == 0 and '1 identical' in r[1]))
    out.append(('roundtrip of a folder (plain XML and an empty file)', {'a.dbx': s, 'b.dbx': '<?xml version="1.0"?>',
                                                                      'sub\\c.dbx': lvl, 'e.dbx': b''},
                ['roundtrip', '{d}'], lambda r: r[0] == 0 and '2 identical, 0 different, 1 not binary' in r[1]))
    # XML export
    out.append(('xml export', {'a.dbx': s}, ['xml', '{d}\\a.dbx'],
                lambda r: r[0] == 0 and 'converted 1 file(s)' in r[1] and b'Levels/Test &amp; &lt;Demo&gt;' in r[3]['a.xml']
                and b'<field name="One">1</field>' in r[3]['a.xml'] and b'<field name="Empty" />' in r[3]['a.xml']))
    hb = Builder(GP)
    hb.ints(hb.instance('T', G1), 'field', 'Hash', 0x3F800000)
    out.append(('xml export: hash fields are whole numbers', {'h.dbx': hb.bytes()}, ['xml', '{d}'],
                lambda r: b'<field name="Hash">1065353216</field>' in r[3]['h.xml']))
    # XML import
    x = None
    from bfbc2core import dbx as _dbx
    x = _dbx.to_xml(s)
    out.append(('import: unedited XML comes back byte for byte', {'a.dbx': s, 'a.xml': x},
                ['import', '{d}\\a.xml', '{d}\\b.dbx'], lambda r: r[0] == 0 and r[3]['b.dbx'] == s
                and '0 changed, 0 new, 0 removed' in r[1]))
    edited = (x.replace('>7<', '>8<').replace('"Scale">1.5<', '"Scale">2<').replace('>true<', '>false<')
              .replace('Levels/Test &amp; &lt;Demo&gt;', 'Renamed'))
    out.append(('import: edited values keep their types', {'a.dbx': s, 'e.xml': edited},
                ['import', '{d}\\e.xml', '{d}\\b.dbx', '--like', '{d}\\a.dbx'],
                lambda r: r[0] == 0 and '4 changed' in r[1]))
    out.append(('import: too many digits rounded', {'a.dbx': s, 'r.xml': x.replace('"Scale">1.5<', '"Scale">0.123456789123<')},
                ['import', '{d}\\r.xml', '{d}\\b.dbx', '--like', '{d}\\a.dbx'],
                lambda r: r[0] == 0 and '1 decimal(s) rounded' in r[1]))
    start = x.index('\t<instance guid="' + G3)
    end = x.index('\t</instance>', start) + len('\t</instance>\r\n')
    block = x[start:end]
    moved = x[:start] + x[end:]
    moved = moved.replace('</partition>', block.replace(G3, G4) + '</partition>')
    out.append(('import: an instance removed and a copy added', {'a.dbx': s, 'm.xml': moved},
                ['import', '{d}\\m.xml', '{d}\\b.dbx', '--like', '{d}\\a.dbx'],
                lambda r: r[0] == 0 and '1 removed' in r[1] and r[2] == ''))
    out.append(('import without a reference: guesses reported, exit code 3', {'blind.xml': x},
                ['import', '{d}\\blind.xml', '{d}\\b.dbx'],
                lambda r: r[0] == 3 and 'typed by guess' in r[2] and _dbx.to_xml(r[3]['b.dbx']) == x))
    out.append(('import with --types', {'a.dbx': s, 'blind.xml': x, 't\\x.dbx': s},
                ['import', '{d}\\blind.xml', '{d}\\b.dbx', '--types', '{d}\\t'], None))
    # XML reader
    doc = ('<?xml version="1.0"?>\r\n<!-- a comment -->\r\n<partition guid=\'' + GP + '\'>\r\n'
           '\t<instance guid="' + G1 + '" type="T">\r\n'
           '\t\t<field name="A" name="B">x</field>\r\n'
           '\t\t<field name="Esc">a&amp;b&#65;&#x42;<![CDATA[<c>]]></field>\r\n'
           '\t\t<field name="Lines">one\r\ntwo</field>\r\n'
           '\t\t<array name="L">\r\n\t\t</array>\r\n'
           '\t\t<array name="N"></array>\r\n'
           '\t</instance>\r\n</partition>\r\n')
    out.append(('XML reader: duplicates, entities, CDATA, line breaks, empty containers', {'reader.xml': doc},
                ['import', '{d}\\reader.xml', '{d}\\b.dbx'], lambda r: r[0] == 3 and b'a&bAB<c>' in r[3]['b.dbx']))
    for name, bad in (('mismatched tags', '<partition><instance></partition>'), ('no root', 'text'),
                      ('unknown entity', '<p>&nope;</p>'), ('unclosed value', '<p a="x></p>'),
                      ('text and children', '<p>x<q/></p>'), ('a bad character reference', '<p>&#xD800;</p>'),
                      ('a hex reference that is no number', '<p>&#xZZ;</p>'), ('UTF-16 with a BOM', '﻿<p>x</p>'.encode('utf-16')),
                      ('a value for a container', '<p>\r\n<q>x</q>\r\n</p>'), ('a bool that is no bool', None)):
        if bad is None:
            continue
        out.append((f'XML reader refuses {name}' if name != 'UTF-16 with a BOM' else 'XML reader: UTF-16 text',
                    {'bad.xml': bad}, ['import', '{d}\\bad.xml', '{d}\\b.dbx'], None))
    out.append(('import: a value that does not fit its type', {'a.dbx': s, 'v.xml': x.replace('>true<', '>maybe<')},
                ['import', '{d}\\v.xml', '{d}\\b.dbx', '--like', '{d}\\a.dbx'], lambda r: r[0] == 1 and 'import failed' in r[2]))
    out.append(('import: a missing reference file', {'a.xml': x}, ['import', '{d}\\a.xml', '{d}\\b.dbx', '--like', '{d}\\nope.dbx'],
                lambda r: r[0] == 1))
    out.append(('import: a plain-XML reference', {'a.xml': x, 'p.dbx': '<?xml version="1.0"?>'},
                ['import', '{d}\\a.xml', '{d}\\b.dbx', '--like', '{d}\\p.dbx'], lambda r: r[0] == 1))
    out.append(('import: unknown option', {'a.xml': x}, ['import', '{d}\\a.xml', '{d}\\b.dbx', '--nope'],
                lambda r: r[0] == 2))
    many = x.replace('<field name="Count">', '<field name="Count"' + ''.join(f' a{i}="{i}"' for i in range(16)) + '>')
    out.append(('import: 16 attributes refused', {'a.dbx': s, 'w.xml': many},
                ['import', '{d}\\w.xml', '{d}\\b.dbx', '--like', '{d}\\a.dbx'], lambda r: r[0] == 1 and '15 at most' in r[2]))
    # edit commands
    out.append(('replace', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'replace=Levels/Test & <Demo>|Other'],
                lambda r: r[0] == 0 and '1 string(s)' in r[1]))
    out.append(('replace: no match', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'replace=nothing like this|x'],
                lambda r: r[0] == 1 and 'not found' in r[2] and 'b.dbx' not in r[3]))
    out.append(('replaceprefix with exceptions', {'a.dbx': s},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'replaceprefix=levels/|maps/|levels/keep'], lambda r: r[0] == 0))
    out.append(('replaceprefix: case kept', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'replaceprefix=LEVELS/|Maps/'], None))
    out.append(('drop', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'drop=' + G2],
                lambda r: r[0] == 0 and '1 instance(s), 2 reference(s)' in r[1]))
    out.append(('drop: no such instance (exit 1)', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'drop=' + '0' * 8],
                lambda r: r[0] == 1 and 'no instance' in r[2]))
    out.append(('dropfield, set, setfield, partguid', {'a.dbx': s},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'dropfield=Scale', 'set=Enabled|0', f'setfield={G1}|Count|42',
                 f'setfield={G1}|One|2.5', f'setfield={G1}|Name|New/Name', 'partguid=' + G4],
                lambda r: r[0] == 0 and 'setfield: 3 field(s)' in r[1]))
    out.append(('setfield: no such field', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', f'setfield={G1}|Missing|1'],
                lambda r: r[0] == 1))
    out.append(('setfield: decimal into a decimal field, value with spaces and |', {'a.dbx': s},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', f'setfield={G1}|Scale|3', f'setfield={G1}|Name|a | b '], None))
    out.append(('setfield: not a number (exit 1, unexpected error)', {'a.dbx': s},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', f'setfield={G1}|Count|x'], lambda r: r[0] == 1))
    out.append(('set: not a byte', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'set=Enabled|300'], lambda r: r[0] == 1))
    out.append(('set: no such field', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'set=Nope|true'], lambda r: r[0] == 1))
    out.append(('additem at the end and before an entry', {'a.dbx': s},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', f'additem={G1}|Objects|{G4}', f'additem={G1}|Objects|{G1}|{G3}'],
                lambda r: r[0] == 0))
    out.append(('additem: no such array', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', f'additem={G1}|NoSuchArray|{G4}'],
                lambda r: r[0] == 1 and 'NOT FOUND' in r[1]))
    tb = Builder(GP)
    tb.instance('Game.Target', G4)
    target = tb.bytes()
    out.append(('graft: the instance and what it references', {'src.dbx': s, 't.dbx': target},
                ['edit', '{d}\\t.dbx', '{d}\\b.dbx', f'graft={{d}}\\src.dbx|{G3}|{G4}'],
                lambda r: r[0] == 0 and '3 instance(s)' in r[1]))
    out.append(('graft twice: skipped when present', {'src.dbx': s, 't.dbx': target},
                ['edit', '{d}\\t.dbx', '{d}\\b.dbx', f'graft={{d}}\\src.dbx|{G3}', f'graft={{d}}\\src.dbx|{G3}'],
                lambda r: r[0] == 0))
    out.append(('graft: nothing to graft', {'src.dbx': s, 't.dbx': target},
                ['edit', '{d}\\t.dbx', '{d}\\b.dbx', f'graft={{d}}\\src.dbx|{"5" * 8}'], lambda r: r[0] == 1))
    out.append(('copyfield', {'src.dbx': s, 't.dbx': target},
                ['edit', '{d}\\t.dbx', '{d}\\b.dbx', f'copyfield={{d}}\\src.dbx|{G1}|Objects|{G4}'],
                lambda r: r[0] == 0 and 'copied' in r[1]))
    out.append(('copyfield: no such field', {'src.dbx': s, 't.dbx': target},
                ['edit', '{d}\\t.dbx', '{d}\\b.dbx', f'copyfield={{d}}\\src.dbx|{G1}|NoSuchField|{G4}'], lambda r: r[0] == 1))
    out.append(('copyfield: a missing source file', {'t.dbx': target},
                ['edit', '{d}\\t.dbx', '{d}\\b.dbx', f'copyfield={{d}}\\nope.dbx|{G1}|Objects|{G4}'], lambda r: r[0] == 1))
    out.append(('edit: commands from a file (comments, blank lines, setfield spaces)',
                {'a.dbx': s, 'cmd.txt': f'# a comment\r\n\r\n  replace=Levels/Test & <Demo>|X  \r\n  setfield={G1}|Name|  y  \r\n'
                 f'drop={G3}\n'},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', '@{d}\\cmd.txt'], lambda r: r[0] == 0))
    out.append(('edit: a missing @file', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', '@{d}\\nope.txt'],
                lambda r: r[0] == 1))
    out.append(('edit: unknown command (exit 2)', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'nonsense'],
                lambda r: r[0] == 2 and 'unknown command' in r[2]))
    out.append(('edit: a command without its parts', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'replace=x'],
                lambda r: r[0] == 1))
    out.append(('edit: not a binary file (exit 2)', {'a.dbx': '<?xml version="1.0"?><p/>'},
                ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'drop=x'], lambda r: r[0] == 2))
    out.append(('edit: a missing input', {}, ['edit', '{d}\\a.dbx', '{d}\\b.dbx', 'drop=x'], lambda r: r[0] == 1))
    out.append(('edit: the output over the input', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\a.dbx', 'drop=' + G3],
                lambda r: r[0] == 0))
    out.append(('edit: output into a missing folder', {'a.dbx': s}, ['edit', '{d}\\a.dbx', '{d}\\no\\b.dbx', 'drop=' + G3],
                lambda r: r[0] == 1))
    # level check
    out.append(('check: a sound level', {'l.dbx': lvl}, ['check', '{d}\\l.dbx'], lambda r: r[0] == 0 and '0 error(s)' in r[1]))
    b = level_sample()
    b.partition['children'][0]['children'][0]['children'][0]['attrs'][0] = (b.intern('ref'), b.intern('null'))
    out.append(('check: an empty entry in the object list', {'l.dbx': b.bytes()}, ['check', '{d}\\l.dbx'],
                lambda r: r[0] == 1 and 'empty entry' in r[1] and 'not in the level\'s object list' in r[1]))
    b = level_sample()
    b.partition['children'][0]['children'][0]['children'][1]['attrs'][0] = (b.intern('ref'), b.intern(G4))
    out.append(('check: an unresolved entry', {'l.dbx': b.bytes()}, ['check', '{d}\\l.dbx'],
                lambda r: r[0] == 1 and 'not in this file' in r[1]))
    b = level_sample()
    b.partition['children'][1]['children'][0]['attrs'][0] = (b.intern('ref'), b.intern('null'))
    out.append(('check: a model group without PhysicsData', {'l.dbx': b.bytes()}, ['check', '{d}\\l.dbx'],
                lambda r: r[0] == 1 and 'without PhysicsData' in r[1]))
    b = level_sample()
    b.placed(G4, 'crate', 1, 2, 3, 77)
    b.placed('55555555-5555-5555-5555-555555555555', 'barrel', 4, 5, 6, 77)
    other = Builder(GP)
    other.placed(G4, 'crate', 1, 2, 3, 12)
    out.append(('check: a shared cull slot, --slots-from', {'l.dbx': b.bytes(), 'o.dbx': other.bytes()},
                ['check', '{d}\\l.dbx', '--slots-from', '{d}\\o.dbx'],
                lambda r: r[0] == 0 and 'shared by 2 objects' in r[1] and 'another slot' in r[1]))
    ext = level_sample()
    lvl_inst = ext.partition['children'][0]
    ext.ref(lvl_inst, 'Ext1', 'some/where/' + G1)
    ext.ref(lvl_inst, 'Ext2', 'missing/file/' + G2)
    ext.ref(lvl_inst, 'Ext3', 'some/where/' + G4)
    there = Builder(GP)
    there.instance('X', G1)
    out.append(('check: references into bundle folders', {'l.dbx': ext.bytes(), 'bundle\\some\\where.dbx': there.bytes()},
                ['check', '{d}\\l.dbx', '--bundle', '{d}\\bundle'],
                lambda r: r[0] == 1 and 'not in the given folders' in r[1] and 'that file does not have' in r[1]))
    out.append(('check: unknown option', {'l.dbx': lvl}, ['check', '{d}\\l.dbx', '--nope'], lambda r: r[0] == 2))
    out.append(('check: not binary', {'l.dbx': 'x' * 40}, ['check', '{d}\\l.dbx'], lambda r: r[0] == 2))
    # cull slots
    mine = Builder(GP)
    mine.placed(G1, 'crate', 0, 0, 0, 900)
    mine.placed(G2, 'crate', 10, 0, 10, 901)
    mine.placed(G3, 'barrel', 100, 0, 100, 902)
    mine.placed(G4, 'statue', 0, 0, 0, 903)
    theirs = Builder(GP)
    theirs.placed(G1, 'crate', 0, 0, 0, 1)
    theirs.placed('66666666-6666-6666-6666-666666666666', 'crate', 10, 0, 10, 2)
    theirs.placed('77777777-7777-7777-7777-777777777777', 'barrel', 120, 0, 100, 3)
    theirs.placed('88888888-8888-8888-8888-888888888888', 'barrel', 300, 0, 300, 4)
    out.append(('fixslots: by guid, position, nearest, none', {'m.dbx': mine.bytes(), 't.dbx': theirs.bytes()},
                ['fixslots', '{d}\\m.dbx', '{d}\\o.dbx', '--from', '{d}\\t.dbx'],
                lambda r: r[0] == 0 and 'matched by guid 1, by blueprint and position 1, nearest within 50 m 1, no counterpart (slot 0) 1' in r[1]))
    out.append(('fixslots: usage', {'m.dbx': mine.bytes()}, ['fixslots', '{d}\\m.dbx', '{d}\\o.dbx', '--fro', 'x'],
                lambda r: r[0] == 2))
    # command line
    out.append(('no arguments: usage (exit 2)', {}, [], lambda r: r[0] == 2 and 'usage: DbxEdit' in r[2]))
    out.append(('unknown command: usage (exit 2)', {}, ['frobnicate', 'x'], lambda r: r[0] == 2))
    out.append(('xml: a missing file and a short one', {'s.dbx': b'{bin'}, ['xml', '{d}\\nope.dbx', '{d}\\s.dbx'],
                lambda r: r[0] == 0 and 'converted 0 file(s)' in r[1]))
    # damaged files: each command fails as DbxEdit fails
    for cut in (30, 60, len(s) // 2, len(s) - 3):
        out.append((f'damaged file cut at {cut}: roundtrip, xml, edit, check',
                    {'d.dbx': s[:cut]}, ['roundtrip', '{d}\\d.dbx'], None))
        out.append((f'damaged file cut at {cut}: xml', {'d.dbx': s[:cut]}, ['xml', '{d}\\d.dbx'], None))
        out.append((f'damaged file cut at {cut}: edit', {'d.dbx': s[:cut]}, ['edit', '{d}\\d.dbx', '{d}\\o.dbx', 'drop=' + G3], None))
        out.append((f'damaged file cut at {cut}: check', {'d.dbx': s[:cut]}, ['check', '{d}\\d.dbx'], None))
    return out


def run_case(d: str, files: dict[str, Any], args: list[str], run: Runner) -> tuple[list[Any], tuple[Any, ...]]:
    """One command line in a fresh folder d holding files: ([exit code, stdout, stderr, {file: SHA-1}] as recorded,
    (exit code, stdout, stderr, {file: bytes}) for DbxTests.cs' expectations)."""
    cg.fresh(d)
    for name, data in files.items():
        p = os.path.join(d, name)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        write(p, data)
    code, out, err = run([a.replace('{d}', d) for a in args])
    out_t, err_t = cg.text(out, (d, '<dir>')), cg.text(err, (d, '<dir>'))
    return [code, out_t, err_t, cg.snapshot(d)], (code, out_t, err_t, snapshot(d))


def quick(check: Any, root: str) -> None:
    rows = cases()
    golden = cg.load(GOLDEN).get('cases', {})
    bad_golden, bad_expect, missing = [], [], []
    for name, files, args, expect in rows:
        got, b = run_case(os.path.join(root, 'dbx'), files, args, run_native)
        if name not in golden:
            missing.append(name)
        else:
            ok, why = same(golden[name], got)
            if not ok:
                bad_golden.append(f'{name}: {why}')
        if expect is not None and not expect(b):
            bad_expect.append(f'{name}: {b[:3]}')
    check(not bad_golden and not missing and len(golden) == len(rows),
          f'{len(rows)} cases of DbxEdit\'s unit tests as command lines: the files, messages and exit codes '
          f'DbxEdit.exe gave ({GOLDEN})', '; '.join(bad_golden[:3] + [f'not recorded: {m}' for m in missing[:3]]))
    check(not bad_expect, f'{len(rows)} cases: what DbxTests.cs expects of each', '; '.join(bad_expect[:3]))


def api(check: Any, root: str) -> None:
    d = os.path.join(root, 'api')
    shutil.rmtree(d, ignore_errors=True)
    os.makedirs(d)
    a = os.path.join(d, 'a.dbx')
    write(a, sample().bytes())
    r = dbxedit.edit(a, os.path.join(d, 'b.dbx'), [f'setfield={G1}|Count|9', 'drop=' + G3])
    check(r.returncode == 0 and 'drop ' + G3 + ': 1 instance(s)' in r.stdout and 'setfield: 1 field(s)' in r.stdout,
          'dbxedit.edit: messages and exit code (text, \\n line ends)', repr(r))
    r = dbxedit.run_cmd([EXE, 'xml', a], capture_output=True, text=True)
    check(r.returncode == 0 and r.stdout == 'converted 1 file(s)\n' and os.path.isfile(a[:-4] + '.xml'),
          'dbxedit.run_cmd: a command line naming DbxEdit (DbxEdit.exe by any path) runs in process', repr(r))
    r = dbxedit.run_cmd(['DbxEdit', 'xml', a], capture_output=True)
    check(r.stdout == b'converted 1 file(s)\r\n', 'dbxedit.run_cmd without text: bytes with \\r\\n, as the exe prints', repr(r))
    try:
        dbxedit.run(['edit', a, os.path.join(d, 'c.dbx'), 'nonsense'], check=True)
        check(False, 'check=True raises on a non-zero exit code')
    except subprocess.CalledProcessError as e:
        check(e.returncode == 2, 'check=True raises CalledProcessError with the exit code')
    r = dbxedit.import_xml(a[:-4] + '.xml', os.path.join(d, 'back.dbx'))
    check(r.returncode == 0 and open(os.path.join(d, 'back.dbx'), 'rb').read() == open(a, 'rb').read(),
          'dbxedit.import_xml: the original beside the XML is the reference; the original comes back')
    level = os.path.join(d, 'l.dbx')
    write(level, level_sample().bytes())
    check(dbxedit.check(level).returncode == 0 and dbxedit.roundtrip(d).returncode == 0, 'dbxedit.check, dbxedit.roundtrip')
    runs = [['edit', a, os.path.join(d, f'm{i}.dbx'), f'setfield={G1}|Count|{i}'] for i in range(40)]
    rs = dbxedit.run_many(runs)
    one = [dbxedit.run(r) for r in runs[:3]]
    check(all(r.returncode == 0 for r in rs) and [r.stdout for r in rs[:3]] == [r.stdout for r in one]
          and len({open(os.path.join(d, f'm{i}.dbx'), 'rb').read() for i in range(40)}) == 40,
          'dbxedit.run_many: 40 edits on all cores, the results of one by one')
    # the log, as DbxEdit keeps it
    old = os.environ.get('LOCALAPPDATA')
    os.environ['LOCALAPPDATA'] = os.path.join(d, 'local')
    try:
        dbxedit.xml(os.path.join(d, 'nope.dbx'))
        logs = os.listdir(os.path.join(d, 'local', 'bfbc2-tool-suite', 'logs'))
        text = open(os.path.join(d, 'local', 'bfbc2-tool-suite', 'logs', logs[0]), encoding='utf-8').read()
        check(logs[0].startswith('dbxedit-') and 'started (in process): xml' in text and 'WARNING xml:' in text,
              'each run logged in dbxedit-<date>.log, its warnings too', text[-200:])
    finally:
        if old is None:
            os.environ.pop('LOCALAPPDATA', None)
        else:
            os.environ['LOCALAPPDATA'] = old
    old_env = os.environ.get('BFBC2_DBXEDIT_EXE')
    os.environ['BFBC2_DBXEDIT_EXE'] = '1'        # the retired switch: nothing reads it any more
    try:
        r = dbxedit.run(['roundtrip', a])
        check(r.returncode == 0 and '1 identical' in r.stdout and not hasattr(dbxedit, 'use_exe'),
              'BFBC2_DBXEDIT_EXE (retired) changes nothing: the commands run in process')
    finally:
        if old_env is None:
            os.environ.pop('BFBC2_DBXEDIT_EXE', None)
        else:
            os.environ['BFBC2_DBXEDIT_EXE'] = old_env
    r = dbxedit.run_cmd([sys.executable, '-c', 'print(5)'], capture_output=True, text=True)
    check(r.stdout.strip() == '5', 'dbxedit.run_cmd runs other programs as subprocess.run')


# ------------------------------------------------------------------------------------------- corpus (--game)

def corpus_files(root: str) -> list[str]:
    out = []
    for top in (os.path.join(root, 'work', 'levels'), os.path.join(root, 'work', 'navkit', 'common'), os.path.join(root, 'work', 'ww2')):
        for d, _, names in os.walk(top):
            out += [os.path.join(d, n) for n in names if n.lower().endswith('.dbx')]
    return sorted(out)


def sha1(b: bytes) -> str:
    return hashlib.sha1(b).hexdigest()


def battery(data: bytes) -> list[str]:
    """DbxTests.cs' Battery: edit commands made from a file's own content (deterministic)."""
    from bfbc2core import dbx
    f = dbx.parse(data)
    cmds: list[str] = []
    inst = f.instances
    guids = {(i.attr('guid') or '').lower() for i in inst}
    referenced: list[str] = []

    def walk(e: Any) -> None:
        r = e.attr('ref')
        if r is not None and r.lower() in guids and r not in referenced:
            referenced.append(r)
        for c in e.children or []:
            walk(c)
    for e in f.roots:
        walk(e)
    if referenced:
        cmds.append('drop=' + referenced[len(referenced) // 2])
    names = [c.text for i in inst for c in (i.children or []) if c.attr('name') == 'Name' and c.kind == 2]
    name = next((v for v in names if v and '|' not in v), None)
    if name is not None:
        cmds.append(f'replace={name}|{name}_X')
        top = name.split('/')[0]
        cmds.append(f'replaceprefix={top}|OPFOR_TEST|{top}/zz')
    cmds.append('dropfield=Enumeration')
    cmds.append('partguid=11111111-2222-3333-4444-555555555555')
    b = next((c for i in inst for c in (i.children or []) if c.kind == 6 and c.attr('name') is not None), None)
    if b is not None:
        cmds.append(f'set={b.attr("name")}|true')
    for i in inst:
        n4 = next((c for c in (i.children or []) if c.kind == 7 and len(c.raw) == 6 and c.attr('name') is not None), None)
        if n4 is not None:
            cmds.append(f'setfield={i.attr("guid")}|{n4.attr("name")}|42')
            break
    for i in inst:
        arr = next((c for c in (i.children or []) if c.tag == 'array' and c.children and c.children[0].tag == 'item'
                    and c.attr('name') is not None), None)
        if arr is not None:
            cmds.append(f'additem={i.attr("guid")}|{arr.attr("name")}|aaaaaaaa-0000-0000-0000-000000000000')
            break
    return cmds


def mutate(xml: str, rnd: random.Random) -> str:
    """DbxTests.cs' Mutate, in spirit: decimals halved, text suffixed, bools flipped, an instance removed and one
    copied under a new guid."""
    lines = xml.replace('\r\n', '\n').split('\n')
    for i, ln in enumerate(lines):
        if rnd.random() > 0.15:
            continue
        gt, lt = ln.find('>'), ln.rfind('</')
        if gt < 0 or lt <= gt + 1 or ln.lstrip().startswith('</'):
            continue
        value = ln[gt + 1:lt]
        if value == 'true':
            nv = 'false'
        elif value == 'false':
            nv = 'true'
        elif all(t and (t[0].isdigit() or t[0] == '-') and 'E' not in t for t in value.split('/')):
            nv = '/'.join(repr(struct.unpack('>f', struct.pack('>f', float(t) * 0.5))[0]) if '.' in t else t
                          for t in value.split('/'))
        elif '/' not in value:
            nv = value + '_x'
        else:
            continue
        lines[i] = ln[:gt + 1] + nv + ln[lt:]
    starts = [i for i, ln in enumerate(lines) if ln.startswith('\t<instance ')]
    if len(starts) >= 3:
        s = rnd.choice(starts)
        e = next((j for j in range(s, len(lines)) if lines[j] == '\t</instance>'), -1)
        if e > s:
            del lines[s:e + 1]
        starts = [i for i, ln in enumerate(lines) if ln.startswith('\t<instance ')]
        c = rnd.choice(starts)
        ce = next((j for j in range(c, len(lines)) if lines[j] == '\t</instance>'), -1)
        if ce > c:
            copy = lines[c:ce + 1]
            copy[0] = re.sub('guid="[^"]+"', f'guid="0BADC0DE-0000-4000-8000-{rnd.randrange(10**8, 10**9):012d}"', copy[0])
            lines[ce + 1:ce + 1] = copy
    return '\r\n'.join(lines)


def corpus_runs(data: bytes, h: str, d: str, run: Runner) -> list[list[Any]]:
    """Every command on one corpus file (its bytes data, SHA-1 h) in the folder d: [label, exit code, stdout digest,
    stderr digest, the folder's digest] per run - as dbxedit_golden_game_0.json / _1.json recorded DbxEdit.exe's. The edited
    import's edits and whether the edit battery runs (every 4th file) come from the file's SHA-1."""
    rnd = random.Random(int(h[:8], 16))
    name = 'f.dbx'
    out: list[list[Any]] = []

    def one(label: str, files: dict[str, Any], args: list[str]) -> dict[str, bytes]:
        got, b = run_case(d, files, args, run)
        out.append([label, got[0], cg.digest(got[1]), cg.digest(got[2]), cg.tree_digest(d)])
        return dict(b[3])
    xml = one('xml', {name: data}, ['xml', '{d}\\' + name]).get('f.xml')
    one('roundtrip', {name: data}, ['roundtrip', '{d}\\' + name])
    if xml is not None:
        one('import --like', {name: data, 'x.xml': xml}, ['import', '{d}\\x.xml', '{d}\\o.dbx', '--like', '{d}\\' + name])
        one('import (no reference)', {'x.xml': xml}, ['import', '{d}\\x.xml', '{d}\\o.dbx'])
        edited = mutate(xml.decode('utf-8'), rnd)
        one('import (edited)', {name: data, 'x.xml': edited}, ['import', '{d}\\x.xml', '{d}\\o.dbx', '--like', '{d}\\' + name])
        one('check', {name: data}, ['check', '{d}\\' + name])
        try:
            commands = battery(data) if int(h[:2], 16) % 4 == 0 else []
        except ValueError:
            commands = []           # a file the suite's reader refuses: no battery (DbxEdit's own runs above)
        for c in commands:
            one('edit ' + c, {name: data}, ['edit', '{d}\\' + name, '{d}\\o.dbx', c])
    return out


def by_sha1(files: list[str]) -> dict[str, str]:
    """{SHA-1: the first file with those bytes}."""
    out: dict[str, str] = {}
    for f in files:
        try:
            with open(f, 'rb') as fh:
                out.setdefault(sha1(fh.read()), f)
        except OSError:
            continue
    return out


def golden_edits(check: Any, by_hash: dict[str, str], root: str) -> None:
    """Every edit fingerprint of fingerprints_795745.txt (DbxEdit's output file, exit code and printed text), in
    process."""
    lines = [ln.split(' ', 5) for ln in open(FINGERPRINTS, encoding='utf-8') if ln.startswith('edit ')]
    checked = bad = 0
    out = os.path.join(root, 'g.dbx')
    for p in lines:
        found = by_hash.get(p[1])
        if found is None:
            continue
        f = found
        cmd = p[5].rstrip('\r\n')
        if os.path.exists(out):
            os.remove(out)
        code, so, se, _ = NATIVE.dbxedit_run(['edit', f, out, cmd], None)
        result = sha1(open(out, 'rb').read()) if os.path.exists(out) else 'none'
        text = (so + se).replace('\n', '\r\n')
        checked += 1
        if not (result == p[2] and str(code) == p[3] and sha1(text.encode('utf-8')) == p[4]):
            bad += 1
            if bad <= 5:
                print(f'       fingerprint differs: {cmd} on {f} ({result == p[2]}, {code}/{p[3]})')
    check(checked >= 150 and bad == 0, f'{checked} of DbxEdit\'s {len(lines)} edit fingerprints in process: the same '
          'output file, exit code and printed text', f'{bad} differ')


def game(check: Any, root: str) -> None:
    try:
        opfor: str | None = paths.find_opfor()
    except paths.PathsError:
        opfor = None
    if not opfor:
        print('     (no Project OPFOR BFBC2 copy: the corpus half skipped)')
        return
    by_hash = by_sha1(corpus_files(opfor))
    golden_edits(check, by_hash, root)
    recorded = {k: v for shard in (0, 1) for k, v in cg.load(GOLDEN_GAME.replace('*', str(shard))).get('files', {}).items()}
    here = [(h, by_hash[h]) for h in sorted(recorded) if h in by_hash]
    t = time.time()
    workers = 4
    chunks = [here[i::workers] for i in range(workers)]

    def worker(k: int) -> tuple[int, list[str]]:
        runs, bad = 0, []
        for h, f in chunks[k]:
            try:
                with open(f, 'rb') as fh:
                    data = fh.read()          # read once: a build beside the test may replace the file
            except OSError:
                continue
            if sha1(data) != h:
                continue
            want = recorded[h][1]
            got = corpus_runs(data, h, os.path.join(root, f'w{k}'), run_native)
            runs += len(got)
            if got != want:
                diff = next((f'{w[0]}: {w[1:]} / {g[1:]}' for w, g in zip(want, got) if w != g),
                            f'{len(want)} runs recorded, {len(got)} now')
                bad.append(f'{f}: {diff}')
        return runs, bad
    total, diffs = 0, []
    with ThreadPoolExecutor(workers) as pool:
        for runs, bad in pool.map(worker, range(workers)):
            total += runs
            diffs += bad
    for d in diffs[:10]:
        print('       ' + d)
    # the corpus is OPFOR's work folders: files rebuilt with other bytes since the recording are not there by SHA-1
    check(not diffs and len(here) >= len(recorded) // 4,
          f'{len(here)} of the {len(recorded)} recorded corpus files still there unchanged, {total} command runs (xml, '
          f'roundtrip, import --like / none / edited, check, the edit battery on every 4th): the files, messages '
          f'and exit codes DbxEdit.exe gave ({GOLDEN_GAME})  ({time.time() - t:.0f} s)',
          f'{len(diffs)} differ' if diffs else 'fewer than a quarter of the recorded files are still there')


def main(argv: list[str]) -> int:
    if NATIVE is None:
        print('skipped: the Rust core is not built (python rust\\build_native.py)')
        return 0
    failed = 0

    def check(ok: bool, what: str, detail: str = '') -> None:
        nonlocal failed
        failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)

    root = tempfile.mkdtemp(prefix='dbxedit_test_')
    try:
        quick(check, root)
        api(check, root)
        if '--game' in argv:
            game(check, root)
    finally:
        shutil.rmtree(root, ignore_errors=True)
    print(f'\n{"ALL PASSED" if not failed else f"{failed} FAILED"}')
    return 1 if failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
