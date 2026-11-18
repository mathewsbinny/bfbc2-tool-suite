r"""meshport.py - Battlefield 1943 models (Xbox 360) in BFBC2's PC formats: every LOD's MeshData, the mesh set, the
textures, and the plan for the shader database (which BFBC2 material each new material is cloned from).

    import bf1943import, meshport, meshview, shaderdb
    p = meshport.Porter(bf1943import.Source(bf1943import.default_source()), meshview.Index(shaderdb.find_opfor(None)))
    r = p.port('Objects/Weapons/Handheld/Beach/US_Springfield1903/Springfield_Mesh', donors=[...])
    # r: {kind, lods: [PC MeshData bytes], meshset: PC bytes, textures: {name: PC bytes}, poses: {name: PC bytes},
    #     names: {lods: [names], meshset: name}, plan: {...}, notes: [...]}
    r = p.port('Characters/Beach/3P/Us_Scout/US_Scout_Mesh', rename={'Characters/Beach/': 'Characters/WW2/'})

rename ({old prefix: new prefix}, any case; or a function of a name) ports the mesh under a new name / namespace -
BF1943's soldiers next to BFBC2's same-named ones: every name the port writes or the mesh set holds is renamed: the
LOD MeshData and mesh set names, the mesh set's shader set names (materials), shader names and LOD settings, the
textures and skeleton poses of the mesh's folder. plan['renamed'] maps each old name to its new one, for the
partitions the caller clones (shaders, LOD settings, the mesh's own).

A mesh part's vertices are written in a PC vertex layout with the same elements as its BF1943 layout (positions,
bone indices and weights, normal / binormal / tangent, UVs - formats as the console converter maps them):
1. BFBC2's own layout for the same mesh part, when BFBC2 has the mesh (BF1943's weapon models it shipped);
2. otherwise the first part of a donor mesh (given, in order) with a layout of the same elements - its material is
   then the one the new material is cloned from;
3. otherwise any BFBC2 layout with those elements (the plan says that no donor material was found).
Meshes skinned to the soldier skeleton (ske01) have their bone palettes and their mesh set's bone list renumbered
from BF1943's bone order to BFBC2's (granny.bone_map). Vehicle and prop skeletons keep their indices.

The layout search runs in the suite's Rust core when it is built (round F2b, rust/bc2-convert/src/meshport.rs,
through importcore: BFBC2's part signatures once, the donors' parts once, the last resort per signature and donors) -
the same choices as pc_layout_cached, the Python search kept as its reference (and pc_layout_reference, the first
scan); BFBC2_IMPORTER_PYTHON=1 runs the Python. The MeshData and mesh set readers and writers are mesh.py's and
meshset.py's (in the Rust core since round F0).

Proven on the BF1943 weapon models BFBC2 shipped: ported, the Type 100, Nambu pistol, M1911A1, Mk 2 grenade and
Shin Gunto equal BFBC2's own (every position, normals within the PC format's precision, palettes, index buffer); the
Thompson differs in 1.8 % of its vertices, which DICE moved for BFBC2.
"""
import bisect
import os
import re
import sys
from typing import Any

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('mesh-viewer', 'anim-tools', 'console-convert', 'core', 'bf1943-import')
import granny  # noqa: E402
import importcore  # noqa: E402
import mesh  # noqa: E402
import meshset  # noqa: E402
import texture  # noqa: E402
from bfbc2core import dbx  # noqa: E402

SOLDIER = 'characters/skeletons/ske01'


class PortError(Exception):
    what_to_do = 'Name a BFBC2 donor mesh whose parts have the same vertex elements (meshport.Porter.port donors=).'


def signature(layout):
    """The elements of a layout as the PC holds them: (format, usage, stream) in order."""
    return tuple((mesh.PC_FORMAT.get(f, f), u, s) for f, u, _, s in mesh.elements(layout)[0])


def _subset(small, big):
    """Whether every element of small is in big (as many times)."""
    pool = list(big)
    for e in small:
        if e not in pool:
            return False
        pool.remove(e)
    return True


def renamer(rename):
    """A function of a name for Porter.port's rename: None keeps names; a function is used as it is; a dict {old
    prefix: new prefix} renames a name starting with a prefix (any case, the longest first), keeping the rest as it
    is."""
    if rename is None:
        return lambda n: n
    if callable(rename):
        return rename
    pairs = sorted(rename.items(), key=lambda kv: -len(kv[0]))

    def f(n):
        for old, new in pairs:
            if n.lower().startswith(old.lower()):
                return new + n[len(old):]
        return n
    return f


def rename_meshset(s, rename):
    """A parsed mesh set (meshset.parse) with its names renamed in place - the LOD settings, each LOD's shader set
    names (the mesh's name and its shader set's guid) and shader names; returns {old: new} of the names changed."""
    done = {}

    def r(b):
        old = b.decode('latin-1')
        new = rename(old)
        if new != old:
            done[old] = new
        return new.encode('latin-1')
    s['lod_settings'] = r(s['lod_settings'])
    for L in s['lods']:
        L['sets'] = [(r(name), [r(sh) for sh in shaders]) for name, shaders in L['sets']]
    return done


# pc_layout's reference: every call scans BFBC2's mesh parts (sorted, each part's signature computed again) - kept as
# the reference the cached search (Porter's part signatures, donor lists and fallback choices, round E3g) is proven
# against; BFBC2_MESHPORT_REFERENCE=1 (or REFERENCE = True) ports with it
REFERENCE = os.environ.get('BFBC2_MESHPORT_REFERENCE', '') not in ('', '0')


class Porter:
    def __init__(self, source, ix):
        self.src, self.ix = source, ix
        self._layouts = None
        self._bones = None
        self._by_sig = None
        self._keys = None                     # BFBC2's part names, sorted
        self._entry_keys = None               # the BF1943 source's entry names, sorted
        self._sigs = None                     # {part name: signature}
        self._donor_parts = {}                # donor -> [(part name, signature)] of its LODs' standard parts, sorted
        self._fallback = {}                   # (signature, donors) -> pc_layout's last choice (or None)
        self._native_ix: Any = None           # the Rust core's MeshLayouts (False: not available)

    def layouts(self):
        """BF1943's mesh part layouts, from every level's shader database."""
        if self._layouts is None:
            self._layouts = {}
            for k, (_, e) in sorted(self.src.entries.items()):
                if e['type'] == 'Dx9ShaderDatabase' and k.startswith('levels/'):
                    for n, rec in mesh.console_layouts(self.src.data(k)).items():
                        self._layouts.setdefault(n, rec)
        return self._layouts

    def bones(self):
        if self._bones is None:
            self._bones = granny.bone_map(self.src.data(SOLDIER + '.res'), self.ix.read(SOLDIER + '.res'))
        return self._bones

    def by_signature(self):
        if self._by_sig is None:
            self._by_sig = {}
            sigs = self.signatures()
            for k in self.part_names():
                self._by_sig.setdefault(sigs[k], k)
        return self._by_sig

    def entries_under(self, folder):
        """The BF1943 source's entry names starting with folder, in name order (the sorted names kept, the folder's
        run found by bisection)."""
        if self._entry_keys is None:
            self._entry_keys = sorted(self.src.entries)
        keys = self._entry_keys
        i = bisect.bisect_left(keys, folder)
        out = []
        while i < len(keys) and keys[i].startswith(folder):
            out.append(keys[i])
            i += 1
        return out

    def part_names(self):
        """BFBC2's mesh part names (the index's layouts), sorted - once."""
        if self._keys is None:
            self._keys = sorted(self.ix.parts)
        return self._keys

    def signatures(self):
        """{BFBC2 part name: signature of its layout} - once."""
        if self._sigs is None:
            self._sigs = {k: signature(v) for k, v in self.ix.parts.items()}
        return self._sigs

    def donor_parts(self, d):
        """[(part name, signature)] of a donor mesh's standard parts (every LOD), in name order - once a donor."""
        if d not in self._donor_parts:
            pre, sigs = f'{d}_lod'.lower(), self.signatures()
            self._donor_parts[d] = [(k, sigs[k]) for k in self.part_names() if k.startswith(pre) and
                                    k.endswith('/standard')]
        return self._donor_parts[d]

    def kind(self, name):
        rel, e = self.src.entries.get(f'{name}.res'.lower(), (None, None))
        if e is None:
            raise PortError(f'{name}: no mesh set in the BF1943 source')
        return e['type']

    def skinned_to_soldier(self, name):
        x = dbx.to_xml(self.src.data(f'{name}.dbx'.lower()))
        return f'{SOLDIER}/' in x.lower()

    def pc_layout(self, name, lod, part, bf_layout, donors):
        """(PC layout body, how it was chosen, donor (mesh, part) or None): BFBC2's own part with the same elements;
        else a donor's part of the same material kind (the part name's last word), else the donor's first with the
        same elements; else the first BFBC2 part with them; else the layout keeping the most of them (pc_layout_reference
        says it in full - this is the same search on the parts' signatures, donor lists and choices kept)."""
        if REFERENCE:
            return self.pc_layout_reference(name, lod, part, bf_layout, donors)
        r = self._pc_layout_rust(name, lod, part, bf_layout, donors)
        return r if r is not None else self.pc_layout_cached(name, lod, part, bf_layout, donors)

    def _native_layouts(self):
        """The Rust core's index of BFBC2's part layouts (made once), or None."""
        if self._native_ix is None:
            m = importcore.native()
            try:
                self._native_ix = m.MeshLayouts([(k, bytes(self.ix.parts[k])) for k in self.part_names()]) \
                    if m is not None else False
            except (ValueError, TypeError):
                self._native_ix = False
        return self._native_ix or None

    def _pc_layout_rust(self, name, lod, part, bf_layout, donors):
        """pc_layout's search in the Rust core, or None (no core, or refused)."""
        m = self._native_layouts()
        if m is None:
            return None
        donors = list(donors)
        try:
            r = m.search(name, lod, part, bytes(bf_layout), donors)
        except (TypeError, OverflowError):
            return None
        if r is None:
            return None
        parts = self.ix.parts
        if r[0] == 'own':
            return parts[r[1]], 'BFBC2 has this part', None
        if r[0] == 'donor':
            return parts[r[1]], 'donor', (donors[r[2]], r[3])
        if r[0] == 'layout':
            return parts[r[1]], f'layout of {r[1]}', None
        if r[0] == 'none':
            raise PortError(f'{name} LOD {lod} {part}: no BFBC2 layout holds a part of its elements '
                            f'{signature(bf_layout)}')
        _, k, left, d, folder = r
        return parts[k], f'layout of {k} ({left} element(s) left out)', \
            ((donors[d], folder) if d is not None else None)

    def pc_layout_cached(self, name, lod, part, bf_layout, donors):
        """pc_layout's search in Python (round E3g's cached search; the reference of the Rust core's)."""
        parts = self.ix.parts
        own = f'{name}_lod{lod}/{part}/standard'.lower()
        sig = signature(bf_layout)
        if own in parts and self.signatures()[own] == sig:
            return parts[own], 'BFBC2 has this part', None
        kind = part.rsplit('_', 1)[-1].lower() if '_' in part else ''
        for d in donors:
            hits = [k for k, s in self.donor_parts(d) if s == sig]
            same = [k for k in hits if k.split('/')[-2].rsplit('_', 1)[-1] == kind]
            if hits:
                k = (same or hits)[0]
                return parts[k], 'donor', (d, k.split('/')[-2])
        k = self.by_signature().get(sig)
        if k is not None:
            return parts[k], f'layout of {k}', None
        key = (sig, tuple(donors))
        if key not in self._fallback:
            sigs, best = self.signatures(), None
            for d in [None] + list(donors):
                pre = None if d is None else f'{d}_lod'.lower()
                for k2 in self.part_names():
                    if pre is not None and not k2.startswith(pre):
                        continue
                    s2 = sigs[k2]
                    if _subset(s2, sig) and any(u == 1 for _, u, _ in s2) and (best is None or len(s2) > best[0] or
                                                                             (len(s2) == best[0] and d and not best[2])):
                        best = (len(s2), k2, d)
            self._fallback[key] = best
        best = self._fallback[key]
        if best is None:
            raise PortError(f'{name} LOD {lod} {part}: no BFBC2 layout holds a part of its elements {sig}')
        dropped = len(sig) - best[0]
        return parts[best[1]], f'layout of {best[1]} ({dropped} element(s) left out)', \
            ((best[2], best[1].split('/')[-2]) if best[2] else None)

    def pc_layout_reference(self, name, lod, part, bf_layout, donors):
        """pc_layout as it was written first: every call scans BFBC2's parts, each signature computed again (the
        reference; REFERENCE)."""
        own = f'{name}_lod{lod}/{part}/standard'.lower()
        if own in self.ix.parts and signature(self.ix.parts[own]) == signature(bf_layout):
            return self.ix.parts[own], 'BFBC2 has this part', None
        sig = signature(bf_layout)
        # a donor part of the same material kind first (the part name's last word: _Glass takes a glass part, not
        # the first part with the same vertex - the US tank cockpit's glass took the JP cockpit's body, opaque)
        kind = part.rsplit('_', 1)[-1].lower() if '_' in part else ''
        for d in donors:
            hits = [k for k in sorted(self.ix.parts) if k.startswith(f'{d}_lod'.lower()) and k.endswith('/standard')
                    and signature(self.ix.parts[k]) == sig]
            same = [k for k in hits if k.split('/')[-2].rsplit('_', 1)[-1] == kind]
            if hits:
                k = (same or hits)[0]
                return self.ix.parts[k], 'donor', (d, k.split('/')[-2])
        k = self.by_signature().get(sig)
        if k is not None:
            return self.ix.parts[k], f'layout of {k}', None
        # no layout with exactly its elements: the one keeping the most of them (a donor's first) - the elements it
        # lacks are left out (mesh.convert reorder='drop'); it must keep positions
        best = None
        for d in [None] + list(donors):
            for k2 in sorted(self.ix.parts):
                if d is not None and not k2.startswith(f'{d}_lod'.lower()):
                    continue
                s2 = signature(self.ix.parts[k2])
                if _subset(s2, sig) and any(u == 1 for _, u, _ in s2) and (best is None or len(s2) > best[0] or
                                                                         (len(s2) == best[0] and d and not best[2])):
                    best = (len(s2), k2, d)
        if best is None:
            raise PortError(f'{name} LOD {lod} {part}: no BFBC2 layout holds a part of its elements {sig}')
        dropped = len(sig) - best[0]
        return self.ix.parts[best[1]], f'layout of {best[1]} ({dropped} element(s) left out)', \
            ((best[2], best[1].split('/')[-2]) if best[2] else None)

    def port(self, name, donors=(), rename=None):
        """The mesh's PC files (module docstring); rename: under another name / namespace (renamer)."""
        R = renamer(rename)
        texture.require()                   # its textures convert in the Rust core: say so before porting anything
        kind = self.kind(name)
        soldier = self.skinned_to_soldier(name)
        bones: Any = self.bones() if soldier else None          # a soldier's bone map, used only for soldiers
        lods, plan, notes, pc_layouts = [], {'mesh': name, 'kind': kind, 'skinned_to_soldier': soldier, 'parts': {}}, [], []
        for lod in range(16):
            key = f'{name}_lod{lod}_data.res'.lower()
            if key not in self.src.entries:
                break
            b = self.src.data(key)
            m = mesh.parse(b, True)
            pairs = []
            for p in m['parts']:
                pn = p['name'].decode('latin-1')
                bf = self.layouts().get(f'{name}_lod{lod}/{pn}/standard'.lower())
                if bf is None:
                    raise PortError(f'{name} LOD {lod} {pn}: no layout in BF1943\'s shader databases')
                pc, how, donor = self.pc_layout(name, lod, pn, bf, donors)
                pairs.append((bf, pc))
                plan['parts'].setdefault(pn, {'layout': how, 'donor': donor})
            lods.append(mesh.convert(b, pairs, reorder='drop', bones=bones))
            pc_layouts.append([pc for _, pc in pairs])
        if not lods:
            raise PortError(f'{name}: no LODs in the BF1943 source')
        # BF1943's rigid sets have no flag byte per LOD (BFBC2's do); composite and skinned sets read as BFBC2's
        s = meshset.parse(self.src.data(f'{name}.res'.lower()), kind, True,
                          flag=False if kind == 'RigidMeshSet' else None)
        for L, out in zip(s['lods'], lods):
            L['size'] = meshset.lod_size(out)
            if kind == 'SkinnedMeshSet' and soldier:
                L['bones'] = [bones[x] for x in L['bones']]
        renamed = rename_meshset(s, R)
        plan['materials'] = sorted({sh.decode('latin-1') for L in s['lods'] for _, shs in L['sets'] for sh in shs})
        folder = name.rsplit('/', 1)[0].lower() + '/'
        textures, poses = {}, {}
        for k in self.entries_under(folder):
            e = self.src.entries[k][1]
            if e['type'] == 'XenonTexture':
                try:
                    textures[R(e['name'])] = texture.convert(self.src.data(k))
                except texture.ConvertError as er:
                    notes.append(f'{e["name"]}: not converted ({er})')
                    continue
            elif e['type'] == 'GrannyAnimation' and k.endswith('skeletonpose.res') and '/' not in k[len(folder):]:
                # the mesh's skeleton pose (a one-frame animation of its skeleton), its bone table onto BFBC2's
                try:
                    poses[R(e['name'])] = granny.to_pc(self.src.data(k), bones, e['name'])
                except granny.GrannyError as er:
                    notes.append(f'{e["name"]}: not converted ({er})')
                    continue
            else:
                continue
            if R(e['name']) != e['name']:
                renamed[e['name']] = R(e['name'])
        new = R(name)
        names = {'lods': [f'{new}_lod{i}_data' for i in range(len(lods))], 'meshset': new}
        if new != name:
            renamed[name] = new
            renamed.update({f'{name}_lod{i}_data': f'{new}_lod{i}_data' for i in range(len(lods))})
            plan['mesh'], plan['source_mesh'] = new, name
        plan['textures'] = sorted(textures)
        plan['renamed'] = renamed
        return {'kind': kind, 'lods': lods, 'meshset': meshset.write(s, kind), 'textures': textures, 'poses': poses,
                'names': names, 'plan': plan, 'notes': notes, 'pc_layouts': pc_layouts}


COLOUR = re.compile(r'_(c|d|cm|dm|diffuse|basecolor)\.res$', re.I)


def preview_parts(result, lod=0):
    """A ported mesh's LOD as the mesh viewer's parts (meshview._layout / render / page), read back from the PC
    bytes with the PC layouts and drawn with the ported colour texture."""
    import numpy as np
    b = result['lods'][lod]
    m = mesh.parse(b, False)
    image, tex = None, next((t for t in result['textures'] if COLOUR.search(t)), None)
    if tex:
        try:
            _, _, w, h, data = texture.levels(result['textures'][tex])[0]
            fmt = struct_fmt(result['textures'][tex])
            image = texture.decode(data, w, h, fmt).clip(0, 255).astype(np.uint8)[:, :, :3]
        except Exception:                                       # noqa: BLE001 - drawn grey then
            image = None
    out = []
    for i, p in enumerate(m['parts']):
        if p['name'].lower().endswith(b'_zonly') or not p['triangles']:
            continue
        d = mesh.decode(b, result['pc_layouts'][lod][i], i, False)
        out.append({'name': p['name'].decode('latin-1'), 'surface': None, 'positions': d['positions'][:, :3],
                    'normals': d['normals'][:, :3] if 'normals' in d else None,
                    'uvs': d['uvs'][:, :2] if 'uvs' in d else None, 'indices': d['indices'], 'texture': tex,
                    'image': image if 'uvs' in d else None})
    return out


def struct_fmt(pc_texture):
    import struct
    return struct.unpack_from('<3I', pc_texture, 0)[2]
