r"""meshport.py - Battlefield 1943 models (Xbox 360) in BFBC2's PC formats: every LOD's MeshData, the mesh set, the
textures, and the plan for the shader database (which BFBC2 material each new material is cloned from).

    import bf1943import, meshport, meshview, shaderdb
    p = meshport.Porter(bf1943import.Source(bf1943import.default_source()), meshview.Index(shaderdb.find_opfor(None)))
    r = p.port('Objects/Weapons/Handheld/Beach/US_Springfield1903/Springfield_Mesh', donors=[...])
    # r: {kind, lods: [PC MeshData bytes], meshset: PC bytes, textures: {name: PC bytes}, plan: {...}, notes: [...]}

A mesh part's vertices are written in a PC vertex layout with the same elements as its BF1943 layout (positions,
bone indices and weights, normal / binormal / tangent, UVs - formats as the console converter maps them):
1. BFBC2's own layout for the same mesh part, when BFBC2 has the mesh (BF1943's weapon models it shipped);
2. otherwise the first part of a donor mesh (given, in order) with a layout of the same elements - its material is
   then the one the new material is cloned from;
3. otherwise any BFBC2 layout with those elements (the plan says that no donor material was found).
Meshes skinned to the soldier skeleton (ske01) have their bone palettes and their mesh set's bone list renumbered
from BF1943's bone order to BFBC2's (granny.bone_map). Vehicle and prop skeletons keep their indices.

Proven on the BF1943 weapon models BFBC2 shipped: ported, the Type 100, Nambu pistol, M1911A1, Mk 2 grenade and
Shin Gunto equal BFBC2's own (every position, normals within the PC format's precision, palettes, index buffer); the
Thompson differs in 1.8 % of its vertices, which DICE moved for BFBC2.
"""
import os
import re
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
for p in ('core', 'bfbc2-console-convert', 'bfbc2-anim-tools', 'bfbc2-mesh-viewer'):
    sys.path.insert(0, os.path.join(SUITE, p))
import granny  # noqa: E402
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


class Porter:
    def __init__(self, source, ix):
        self.src, self.ix = source, ix
        self._layouts = None
        self._bones = None
        self._by_sig = None

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
            for k in sorted(self.ix.parts):
                self._by_sig.setdefault(signature(self.ix.parts[k]), k)
        return self._by_sig

    def kind(self, name):
        rel, e = self.src.entries.get(f'{name}.res'.lower(), (None, None))
        if e is None:
            raise PortError(f'{name}: no mesh set in the BF1943 source')
        return e['type']

    def skinned_to_soldier(self, name):
        x = dbx.to_xml(self.src.data(f'{name}.dbx'.lower()))
        return f'{SOLDIER}/' in x.lower()

    def pc_layout(self, name, lod, part, bf_layout, donors):
        """(PC layout body, how it was chosen, donor (mesh, part) or None)."""
        own = f'{name}_lod{lod}/{part}/standard'.lower()
        if own in self.ix.parts and signature(self.ix.parts[own]) == signature(bf_layout):
            return self.ix.parts[own], 'BFBC2 has this part', None
        sig = signature(bf_layout)
        for d in donors:
            for k in sorted(self.ix.parts):
                if k.startswith(f'{d}_lod'.lower()) and k.endswith('/standard') and signature(self.ix.parts[k]) == sig:
                    return self.ix.parts[k], 'donor', (d, k.split('/')[-2])
        k = self.by_signature().get(sig)
        if k is None:
            raise PortError(f'{name} LOD {lod} {part}: no BFBC2 layout has its elements {sig}')
        return self.ix.parts[k], f'layout of {k}', None

    def port(self, name, donors=()):
        kind = self.kind(name)
        soldier = self.skinned_to_soldier(name)
        bones = self.bones() if soldier else None
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
        s = meshset.parse(self.src.data(f'{name}.res'.lower()), kind, True)
        for L, out in zip(s['lods'], lods):
            L['size'] = meshset.lod_size(out)
            if kind == 'SkinnedMeshSet' and soldier:
                L['bones'] = [bones[x] for x in L['bones']]
        plan['materials'] = sorted({sh.decode('latin-1') for L in s['lods'] for _, shs in L['sets'] for sh in shs})
        folder = name.rsplit('/', 1)[0].lower() + '/'
        textures = {}
        for k, (_, e) in sorted(self.src.entries.items()):
            if e['type'] == 'XenonTexture' and k.startswith(folder):
                try:
                    textures[e['name']] = texture.convert(self.src.data(k))
                except texture.ConvertError as er:
                    notes.append(f'{e["name"]}: not converted ({er})')
        plan['textures'] = sorted(textures)
        return {'kind': kind, 'lods': lods, 'meshset': meshset.write(s, kind), 'textures': textures, 'plan': plan,
                'notes': notes, 'pc_layouts': pc_layouts}


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
