r"""renderqa.py - draw meshes offline with the game's own PC shader programs, textures and vertex data (Direct3D 11,
off screen) and check the pictures: a shader or texture conversion that is wrong shows here without starting the game.

  python renderqa.py render <mesh>... --db <db> [--stage <folder>]... [-o <folder>]
        each mesh's LOD 0 drawn with the database's programs (front, three-quarter and top views), the pictures
        checked (black, magenta, flat, blown out, NaN, alpha test discarding a whole part, texture slots with
        nothing bound, missing textures, parts whose layout has no program); a contact sheet, worst first
  python renderqa.py compare <mesh>... --db <db> [--truth <db> | --truth auto] [--stage <folder>]... [-o <folder>]
        the same mesh drawn twice - DICE's own entry (the --truth database, or with auto the first BFBC2 level whose
        database draws the mesh, with BFBC2's own files) and the --db database's (with the staged files first) -
        and the two pictures compared (coverage overlap, mean difference, PSNR, colour shift); worst first
  python renderqa.py validate <mesh>... [-o <folder>]
        stock meshes with their own level's database beside the mesh viewer's plain texture render (meshview.py):
        the harness itself is checked - the pictures must look like the game
  python renderqa.py strips --db <db> [--terrain <VisualTerrain>] [--stage <folder>]... [--only <text>]... [-o <folder>]
        a VisualTerrain's baked road and decal strips (terrain.road_strips), each drawn alone with its entry's program
        for the strip layout from above and at three-quarters, the terrain's named engine constants estimated
        (strip_values: a flat height map, an empty dynamic mask); each covered pixel's depth taken back to the world
        and held against the strip's own vertices (out-of-bounds: the sheets across the sky a shader of another
        vertex layout draws), no program for the layout, nothing drawn, NaN, another level's texture flagged; exit
        code 1 when a strip fails. --terrain defaults to terrains/<name>/<name>.visual.res for a level:<name> db -
        DICE's own strips with their own level's database are the harness's check (they pass)
  python renderqa.py patches --db <db> [--stage <folder>]... [--only <text>]... [-o <folder>]
        every terrain patch entry (PatchMaskShaders) drawn on a 16 x 16-cell patch the way the engine draws one (the
        grid stream, the mask atlas stream; posScale / posOffset / heightmapTexCoordScaleAndOffset and the height map
        through the vertex shader's own block records, a flat height map, every mask tile half covered), in each patch
        layout the entry has a program for (PATCH_LAYOUT always, PATCH_LAYOUT_NO_TILES where DICE compiles it); out of
        the patch's box, nothing drawn, black, NaN, no program, another level's texture, an engine constant or texture
        the shader reads that no record of its block names: exit code 1. DICE's own patches with their own level's
        database are the harness's check (they pass)
  python renderqa.py wake [-o <folder>] [--limit N] [--only <text>]
        Project OPFOR's Wake Island: every mesh wake_audit.drawn_meshes() lists (OPFOR's src\tools\ww2, imported
        read-only), drawn with the installed Wake database (work\ww2\install); meshes a BFBC2 level also draws are
        compared with DICE's entry, the others rendered and flagged

A <db> is what shaderdb.py takes: a Dx11_Single.res / Dx10_Single.res file, level:<name>, or <bundle.fbrb>[:<entry>].
--stage: a folder of FbrbTool-unpacked bundles, each with its fbrb.manifest (e.g. OPFOR's work\ww2\install) - read
before the game copy. Meshes are named as the game names them (Objects/Vehicles/Land/Humvee/Humvee_mesh; any case).

Output: <folder>\results.json (every number), summary.txt, sheet_<n>.png (worst first) and one PNG per mesh.

What is the game's and what is guessed (README.md, "renderqa"):
  - the game's: vertex and index data, the vertex layout and the program the engine picks for it (main pass, not
    instanced, no point light), the compiled vertex and pixel shaders, the input layout, the textures in their own
    block formats with their mips and sRGB flag, the shaders' sampler states, the material constants of the
    program's parameter block, the light colours and sun direction of a level's weather state (default BFBC2's
    mp_005, its active state), its exposure and tone mapping (filmic curve, saturation, contrast, brightness);
  - guessed / simplified: no shadows (the shadow map reads fully lit), no point lights, no fog, no bloom or flare;
    the reflection cube the engine binds itself (outdoorLightReflectionEnvmap) is Sky_Beach_Generic; bone matrices
    are the rest pose (composite vehicles: their parts' transforms; destructible composites: every part shown);
    world matrix identity; no back-face culling; opaque draws (no blend states); time 0.
Needs numpy and Pillow, and shaderview.exe (native\build.bat; built into the suite's build\ folder on first use
when the Visual Studio C++ tools are installed). Writes only into the output folder and the suite's build\ folder.
"""
from __future__ import annotations

import argparse
import contextlib
import ctypes
import json
import math
import os
import pickle
import re
import shutil
import struct
import subprocess
import sys
import tempfile
from collections.abc import Callable, Iterable, Sequence
from dataclasses import dataclass, field
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(HERE)
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('core', 'shader-tools', 'console-convert', 'mesh-viewer')
import mesh as meshdata  # noqa: E402
import meshset  # noqa: E402
import pcbuild as P  # noqa: E402
import shaderdb as S  # noqa: E402

SUITE = _ROOT
BUILD = os.path.join(SUITE, 'build')
NATIVE = os.path.join(HERE, 'native')
VIEWS = (('threequarter', 45.0, 28.0), ('front', 0.0, 8.0), ('top', 0.0, 89.0))      # (name, azimuth, elevation)
ENVMAP = 'Textures/Sky/Sky_Beach_Generic_RGBA16f'
WEATHER = 'weather/r_mp005'          # the default light: BFBC2's Arica Harbor, its active weather state
NOON_DIR = (0.35, 0.8, 0.5)          # towards the key light: about 53 degrees up, over the three-quarter camera's side
BACKGROUND = (0.32, 0.36, 0.42)      # behind the mesh in the pictures (not drawn by the shaders, not measured)
TEX_FORMATS = {0: 8, 18: 8, 1: 16, 2: 16, 19: 16, 20: 16, 13: 0, 9: 0, 10: 0}        # format -> block bytes (0: texels)
BELOW_NORMAL = 0x4000                # Windows process priority: the game may be running a playtest

# mp_005's active weather state (Sunny), used when no weather can be read (tests without the game copy)
MP005_SUNNY = {
    'outdoorlight': {'KeyColor': [1.0, 0.971, 0.914], 'KeyColorIntensity': [1.576], 'KeySpecularScale': [1.0],
                     'TopColor': [0.268, 0.324, 0.427], 'TopColorIntensity': [1.0],
                     'BottomColor': [0.055, 0.059, 0.051], 'BottomColorIntensity': [1.0]},
    'sun': {'Theta': [63.666], 'Phi': [53.248]},
    'postprocess': {'MiddleGray': [1.0], 'MinExposure': [1.0], 'MaxExposure': [1.0], 'BloomExponent': [1.0],
                    'Brightness': [1.0, 1.0, 1.0], 'Contrast': [1.0, 1.0, 1.0], 'Saturation': [1.0, 1.0, 1.0],
                    'BloomScale': [0.05, 0.05, 0.05]},
}

# engine values the weather does not set (as Project OPFOR's wake_preview.py sets them)
FIXED: dict[str, Any] = {
    'outdoorLightShadowmapSizeAndInvSize': (1.0, 1.0, 1.0, 1.0),
    'pointLightPos0': (0.0, -1000.0, 0.0, 0.0), 'pointLightColor0': (0.0, 0.0, 0.0, 0.0),
    'pointLightRadius0': (0.001, 0.0, 0.0, 0.0), 'time': (0.0, 0.0, 0.0, 0.0),
    'transparentStartAndEndAndClamp': (0.0, 2000.0, 1.0, 0.0), 'viewportZMinMax': (0.0, 1.0, 0.0, 0.0),
    'outdoorLightHemisphereDir': (0.0, 1.0, 0.0, 0.0),
    'outdoorLightShadowTransform0': np.zeros((4, 4)), 'outdoorLightShadowTransform1': np.zeros((4, 4)),
    'outdoorLightShadowTransform2': np.zeros((4, 4)),
}

# flags and how much they weigh when sorting worst first
WEIGHT = {'no-mesh': 10, 'nothing-drawn': 10, 'nan': 8, 'black': 8, 'magenta': 8, 'no-entry': 6, 'no-program': 6,
          'alpha-discards-all': 6, 'bound-empty': 5, 'missing-texture': 5, 'dimension': 5, 'blown-out': 5,
          'layout-fallback': 4, 'flat': 4, 'bad-texture': 3, 'no-layout': 3, 'render-failed': 10}


class RenderQaError(Exception):
    """A problem the user can fix (a missing tool, an unreadable input)."""

    def __init__(self, message: str, what_to_do: str = 'Check the names and paths given (python renderqa.py -h).'):
        super().__init__(message)
        self.what_to_do = what_to_do


# ------------------------------------------------------------------------------------------------ files

class Files:
    """Game files by name: staged folders first (FbrbTool-unpacked bundles with an fbrb.manifest), then the game
    copy (meshview's index of the stock bundles; its cache kept in the suite's build\\ folder, so nothing is
    written into the project folder)."""

    def __init__(self, root: str | None, stages: Sequence[str] = (), cache: str | None = None) -> None:
        self.root = root
        self.paths: dict[str, str] = {}
        self.types: dict[str, str] = {}
        for st in stages:
            self.add_stage(st)
        self._cache = cache
        self._ix: Any = None
        self._bundle_types: dict[str, dict[str, str]] = {}

    def add_stage(self, folder: str) -> None:
        if not os.path.isdir(folder):
            raise RenderQaError(f'--stage {folder}: no such folder')
        tops = [folder] if os.path.isfile(os.path.join(folder, 'fbrb.manifest')) else \
            [os.path.join(folder, d) for d in sorted(os.listdir(folder))]
        for top in tops:
            man = os.path.join(top, 'fbrb.manifest')
            if not os.path.isfile(man):
                continue
            for line in open(man, encoding='utf-8').read().splitlines()[1:]:
                p = line.split('|')
                if len(p) > 1:
                    self.paths.setdefault(p[0].lower(), os.path.join(top, *p[0].split('/')))
                    self.types.setdefault(p[0].lower(), p[1])

    def ix(self) -> Any:
        if self._ix is None:
            if not self.root:
                raise RenderQaError('no game copy (Project OPFOR BFBC2 folder) to read stock files from')
            import meshview
            cache = self._cache or os.path.join(BUILD, 'meshview_index.pkl')
            seed = os.path.join(self.root, 'work', 'tools_out', 'meshview_index.pkl')
            if not os.path.isfile(cache) and os.path.isfile(seed):
                os.makedirs(os.path.dirname(cache), exist_ok=True)
                shutil.copyfile(seed, cache)            # the mesh viewer's index, if current, saves the indexing
            self._ix = meshview.Index(self.root, cache)
        return self._ix

    def staged(self, name: str) -> bool:
        p = self.paths.get(name.lower())
        return bool(p and os.path.isfile(p))

    def read(self, name: str, stock_only: bool = False) -> bytes | None:
        if not stock_only:
            p = self.paths.get(name.lower())
            if p and os.path.isfile(p):
                return open(p, 'rb').read()
        if not self.root:
            return None
        try:
            return self.ix().read(name.lower())
        except Exception:                                   # noqa: BLE001 - unreadable: treated as missing
            return None

    def kind(self, name: str, stock_only: bool = False) -> str | None:
        """FbrbTool type of an entry (RigidMeshSet, ...)."""
        if not stock_only and name.lower() in self.types and self.staged(name):
            return self.types[name.lower()]
        if not self.root:
            return None
        hit = self.ix().where.get(name.lower())
        if not hit:
            return None
        if hit[0] not in self._bundle_types:
            from bfbc2core import fbrb
            self._bundle_types[hit[0]] = {e.name.lower(): e.type for e in fbrb.entries(hit[0])}
        return self._bundle_types[hit[0]].get(name.lower())


class StockFiles(Files):
    """The game copy only (DICE's ground truth), sharing another Files' index."""

    def __init__(self, other: Files) -> None:
        super().__init__(other.root)
        self._ix = other.ix() if other.root else None
        self._bundle_types = other._bundle_types

    def read(self, name: str, stock_only: bool = True) -> bytes | None:
        return super().read(name, True)

    def kind(self, name: str, stock_only: bool = True) -> str | None:
        return super().kind(name, True)


# ------------------------------------------------------------------------------------------------ light

def _xml_fields(xml: str) -> dict[str, list[float]]:
    out: dict[str, list[float]] = {}
    for m in re.finditer(r'<(?:field|complex) name="(\w+)">([^<]*)<', xml):
        with contextlib.suppress(ValueError):
            out[m.group(1)] = [float(x) for x in m.group(2).split('/')]
    return out


def read_weather(files: Files, folder: str = WEATHER) -> tuple[dict[str, dict[str, list[float]]], str]:
    """({'outdoorlight', 'sun', 'postprocess': fields}, label) of a weather folder's active state: its
    WeatherSettings' ActiveState indexes its WeatherSystem's States; the state names the partitions."""
    from bfbc2core import dbx
    folder = folder.strip('/').lower()
    names = set(files.paths)
    if files.root:
        names |= set(files.ix().where)
    settings = sorted(n for n in names if n.startswith(folder + '/weathersettings') and n.endswith('.dbx'))
    system = sorted(n for n in names if n.startswith(folder + '/weathersystem') and n.endswith('.dbx'))
    if not settings or not system:
        raise RenderQaError(f'no WeatherSettings / WeatherSystem partition under {folder}/')
    sb, yb = files.read(settings[0]), files.read(system[0])
    if sb is None or yb is None:
        raise RenderQaError(f'{folder}: weather partitions unreadable')
    active = int(_xml_fields(dbx.to_xml(sb)).get('ActiveState', [0])[0])
    states = re.findall(r'<item ref="([^"]+)/[0-9a-fA-F-]{36}"', dbx.to_xml(yb))
    if active >= len(states):
        raise RenderQaError(f'{folder}: ActiveState {active} of {len(states)} states')
    sx = files.read(states[active] + '.dbx')
    if sx is None:
        raise RenderQaError(f'{states[active]}: not found')
    out: dict[str, dict[str, list[float]]] = {}
    for kind in ('Sun', 'OutdoorLight', 'PostProcess'):
        m = re.search(r'ref="([^"]+)/[0-9a-fA-F-]{36}" name="%s"' % kind, dbx.to_xml(sx))
        b = files.read(m.group(1) + '.dbx') if m else None
        if b is None:
            raise RenderQaError(f'{states[active]}: no {kind} partition')
        out[kind.lower()] = _xml_fields(dbx.to_xml(b))
    return out, states[active]


def light(weather: dict[str, dict[str, list[float]]], sun_from: str = 'noon') -> tuple[dict[str, Any], dict[str, Any]]:
    """(engine light constants, post-process settings) as BFBC2 derives them from a weather state: colours times
    their intensities (checked in the game by Project OPFOR's wake_probe.py). The key light's direction (towards the
    light: the shaders take saturate(dot(normal, outdoorLightDir))): 'noon' - a fixed high sun, NOON_DIR (default);
    'weather' - from the state's sun theta / phi, by the formula measured on Wake Island (for mp_005's Sunny state,
    phi 53, it points below the horizon: the measurement does not carry over)."""
    ol, sun, pp = weather['outdoorlight'], weather['sun'], weather['postprocess']
    if sun_from == 'weather':
        th, ph = math.radians(sun['Theta'][0]), math.radians(sun['Phi'][0])
        d = (-math.cos(th) * math.sin(ph), -math.cos(ph), -math.sin(th) * math.sin(ph), 0.0)
    else:
        n = np.array(NOON_DIR) / np.linalg.norm(NOON_DIR)
        d = (float(n[0]), float(n[1]), float(n[2]), 0.0)
    key = [c * ol['KeyColorIntensity'][0] for c in ol['KeyColor'][:3]]
    values = dict(FIXED,
                  outdoorLightDir=d,
                  outdoorLightKeyColor=(*key, 0.0),
                  outdoorLightTopColor=(*[c * ol['TopColorIntensity'][0] for c in ol['TopColor'][:3]], 0.0),
                  outdoorLightBottomColor=(*[c * ol['BottomColorIntensity'][0] for c in ol['BottomColor'][:3]], 0.0),
                  outdoorLightKeySpecularColorAndHemisphereVisibility=(
                      *[c * ol['KeySpecularScale'][0] for c in key], 1.0))
    post: dict[str, Any] = {k: pp.get(k, [1.0])[0] for k in ('MiddleGray', 'MinExposure', 'MaxExposure')}
    post.update({k: pp.get(k, [1.0, 1.0, 1.0])[:3] for k in ('Brightness', 'Contrast', 'Saturation')})
    return values, post


def tonemap(hdr: np.ndarray, mask: np.ndarray, post: dict[str, Any]) -> tuple[np.ndarray, float]:
    """BFBC2's post-process without bloom / flare: exposure = middle grey / the drawn pixels' log-average
    luminance, clamped to the state's range (mp_005: fixed 1); the filmic curve (gamma included); saturation,
    contrast, brightness. (LDR image 0..1, exposure)."""
    w = np.array([0.3, 0.59, 0.11], np.float32)
    lum = np.maximum(hdr @ w, 1e-5)
    avg = float(np.exp(np.log(lum[mask]).mean())) if mask.any() else 1.0
    exposure = min(max(post['MiddleGray'] / (avg + 0.001), post['MinExposure']), post['MaxExposure'])
    x = np.maximum(hdr * exposure - 0.004, 0.0)
    y = np.minimum(x * (6.2 * x + 0.5) / (x * (6.2 * x + 1.7) + 0.06), 1.0)
    grey = (y @ w)[..., None]
    y = grey + (y - grey) * np.array(post['Saturation'], np.float32)
    y = (y - 0.5) * np.array(post['Contrast'], np.float32) + 0.5
    return np.clip(y * np.array(post['Brightness'], np.float32), 0, 1), exposure


# ------------------------------------------------------------------------------------------------ shaders

def _chunks(dxbc: bytes) -> dict[bytes, bytes]:
    n = struct.unpack_from('<I', dxbc, 28)[0]
    out = {}
    for i in range(n):
        off = struct.unpack_from('<I', dxbc, 32 + 4 * i)[0]
        out[dxbc[off:off + 4]] = dxbc[off + 8:off + 8 + struct.unpack_from('<I', dxbc, off + 4)[0]]
    return out


def resources(dxbc: bytes) -> list[tuple[str, int, int, int]]:
    """[(name, type, bind point, dimension)] a DXBC shader binds (type 2 texture, 3 sampler; dimension 4 2D,
    9 cube)."""
    r = _chunks(dxbc).get(b'RDEF', b'')
    if len(r) < 28:
        return []
    nb, bo = struct.unpack_from('<2I', r, 8)
    out = []
    for i in range(nb):
        no, typ, _, dim, _, bp, _, _ = struct.unpack_from('<8I', r, bo + 32 * i)
        out.append((r[no:r.index(b'\0', no)].decode('latin-1'), typ, bp, dim))
    return out


def compiler(dxbc: bytes) -> str:
    """The compiler that built a DXBC shader (its RDEF creator string): DICE's are 'Microsoft (R) HLSL Shader
    Compiler 9.27.952.3022'; shaders translated from BF1943 (pcbuild.py, d3dcompiler_47) say 10.x."""
    r = _chunks(dxbc).get(b'RDEF', b'')
    if len(r) < 28:
        return ''
    o = struct.unpack_from('<I', r, 24)[0]
    return r[o:r.index(b'\0', o)].decode('latin-1') if o < len(r) else ''


def origin(vs: S.Shader, ps: S.Shader) -> str:
    """'dice' (both shaders built by DICE's compiler: BFBC2's own, merged or cloned) or 'translated'."""
    return 'dice' if all('9.27' in compiler(s.code) for s in (vs, ps)) else 'translated'


def vs_elements(tail: bytes) -> list[tuple[int, int, int, int, int]]:
    """[(semantic index, DXGI format, slot, byte offset, per instance)] of a Dx10/11 vertex shader's input
    elements (section 4)."""
    n = struct.unpack_from('<I', tail)[0]
    o = 4 + n
    m = struct.unpack_from('<I', tail, o)[0]
    o += 4
    return [struct.unpack_from('<7I', tail, o + 0x1c * j)[1:6] for j in range(m)]


def sampler_states(block: bytes) -> list[tuple[int, bytes]]:
    """[(sampler register, D3D11_SAMPLER_DESC bytes)] of a shader's own parameter block: its 60-byte sampler-state
    records are a u32 register, the 52-byte D3D11_SAMPLER_DESC and 4 zero bytes."""
    if len(block) < 0x20:
        return []
    start = struct.unpack_from('<I', block, 0x14)[0]
    out = []
    for i in range(block[0x1e]):
        o = start + P.SAMPLER_STATE * i
        if o + 56 <= len(block):
            out.append((struct.unpack_from('<I', block, o)[0], block[o + 4:o + 56]))
    return out


def texture_problem(pc: bytes) -> str:
    """'' when shaderview can upload a texture resource as it is, else why not."""
    if len(pc) < 92:
        return 'too short'
    _, typ, fmt, _, w, h, _, mips = struct.unpack_from('<8I', pc, 0)
    if fmt not in TEX_FORMATS:
        return f'format {fmt}'
    if not 1 <= mips <= 15 or not w or not h:
        return f'{w}x{h}, {mips} mips'
    if TEX_FORMATS[fmt] and (w % 4 or h % 4):
        return f'{w}x{h} block format'
    sizes = struct.unpack_from('<15I', pc, 32)
    if 92 + sum(sizes[:mips]) * (6 if typ == 1 else 1) > len(pc):
        return 'levels run past the data'
    return ''


# ------------------------------------------------------------------------------------------------ preparing draws

@dataclass
class Draw:
    part: str
    entry: str
    program: int
    origin: str
    vs: bytes
    ps: bytes
    elements: list[tuple[int, int, int, int, int]]
    vertex_offset: int
    stride: int
    indices: int
    first: int
    kind: int                                         # program key byte 0x33: 0 rigid, 1 bones, 4 skinned, 9 parts
    bones: np.ndarray | None
    material: list[tuple[str, int, int, tuple[float, ...]]]
    textures: list[tuple[int, str]]                   # (t slot, texture name) bound
    samplers: list[tuple[int, bytes]]
    flags: list[str] = field(default_factory=list)


@dataclass
class Prepared:
    mesh: str
    draws: list[Draw]
    flags: list[str]
    notes: list[str]
    vertices: bytes = b''
    indices: bytes = b''
    wide: bool = False
    lo: np.ndarray | None = None
    hi: np.ndarray | None = None
    textures: dict[str, bytes] = field(default_factory=dict)      # texture name -> resource bytes to upload
    shader_set: str = ''


def _flag(flags: list[str], notes: list[str], kind: str, note: str) -> None:
    if kind not in flags:
        flags.append(kind)
    notes.append(f'{kind}: {note}')


def part_bones(ms: dict[str, Any], p: dict[str, Any]) -> np.ndarray | None:
    """A composite mesh part's bone matrices at rest: matrix k = the mesh set's transform of composite part k (a
    vertex's blend index is the composite part's number; composite vehicles store parts about their own origins).
    3 rows per bone; None for a mesh set without composite parts."""
    cp = ms['lods'][0].get('parts') if ms['lods'] else None
    if not cp or not p.get('bones'):
        return None
    out = np.tile(np.eye(4)[:3], (60, 1))
    for k in range(min(len(cp), 60)):
        xf = cp[k][3]
        r = np.array(xf[:9], np.float64).reshape(3, 3).T
        out[3 * k:3 * k + 3] = np.hstack([r, np.array(xf[9:12], np.float64)[:, None]])
    return out


def _positions(b: bytes, md: dict[str, Any], part: dict[str, Any], layout: bytes) -> np.ndarray | None:
    els, _ = meshdata.elements(layout)
    pos = [e for e in els if e[1] == 1 and e[3] == 0]
    if not pos:
        return None
    f, _, off, _ = pos[0]
    n, stride = part['vertices'], part['stride']
    base = md['data'] + part['vertex_offset']
    if n == 0 or base + n * stride > len(b):
        return None
    raw = np.frombuffer(b, np.uint8, n * stride, base).reshape(n, stride)
    if f in (3, 4):
        return raw[:, off:off + 12].copy().view('<f4').reshape(n, 3).astype(np.float64)
    if f in (7, 8):
        return raw[:, off:off + 6].copy().view('<f2').reshape(n, 3).astype(np.float64)
    return None


def _pick_program(db: S.ShaderDb, progs: list[int], lg: bytes) -> tuple[int | None, bool]:
    """(program, its layout is the part's) the engine draws a part with: main pass, not instanced, no point light,
    the part's vertex layout. None when the entry has no main-pass program at all."""
    main = [k for k in progs if db.keys[k][0x30] == 0 and db.keys[k][0x34] == 0]
    keyed = [k for k in main if db.keys[k][0x20:0x30] == lg]
    pick = [k for k in keyed if db.keys[k][0x38] == 0] or keyed
    if pick:
        return pick[0], True
    rest = [k for k in main if db.keys[k][0x38] == 0] or main
    return (rest[0], False) if rest else (None, False)


def prepare(files: Files, db: S.ShaderDb, mesh: str, envmap: str = ENVMAP) -> Prepared:
    """Everything needed to draw a mesh's LOD 0 with a database, and the problems found on the way."""
    low = mesh.lower()
    flags: list[str] = []
    notes: list[str] = []
    out = Prepared(mesh, [], flags, notes)
    msb, kind = files.read(low + '.res'), files.kind(low + '.res')
    ldata = files.read(low + '_lod0_data.res')
    if msb is None or ldata is None or kind is None:
        _flag(flags, notes, 'no-mesh', 'mesh set or its LOD 0 data not found')
        return out
    try:
        ms = meshset.parse(msb, kind, False)
        md = meshdata.parse(ldata, False)
    except Exception as e:                                  # noqa: BLE001 - reported as a finding
        _flag(flags, notes, 'no-mesh', f'unreadable: {e}')
        return out
    if not ms['lods']:
        _flag(flags, notes, 'no-mesh', 'mesh set has no LOD')
        return out
    out.vertices = ldata[md['data']:md['data'] + md['vertex_bytes']]
    out.indices = ldata[md['data'] + md['vertex_bytes']:]
    out.wide = bool(md['wide_indices'])
    parts = {n.lower(): g for n, g in db.parts}
    lays = {r[:16]: r[16:] for r in db.layouts}
    ents = {n.lower(): g for n, g in db.entries}
    ep = db.entry_programs()
    sets = ms['lods'][0]['sets']
    names = lambda s: [x.decode('latin-1') if isinstance(x, bytes) else x for x in s[1]]      # noqa: E731
    # the shader set the database draws (a level picks one: desert / snow ...): the first with every entry
    chosen = next((s for s in sets if all(f'{low}/{n}'.lower() in ents for n in names(s))), None)
    chosen = chosen or max(sets, key=lambda s: sum(f'{low}/{n}'.lower() in ents for n in names(s)))
    shaders = names(chosen)
    out.shader_set = chosen[0].decode('latin-1') if isinstance(chosen[0], bytes) else str(chosen[0])
    pts = []
    for i, p in enumerate(md['parts']):
        pn = p['name'].decode('latin-1')
        if p['triangles'] == 0 or i >= len(shaders):
            continue
        lg = parts.get(f'{low}_lod0/{pn}/standard'.lower())
        if lg is None or lg not in lays:
            _flag(flags, notes, 'no-layout', f'{pn}: no vertex layout in the database')
            continue
        entry = f'{mesh}/{shaders[i]}'
        g = ents.get(entry.lower())
        if g is None:
            _flag(flags, notes, 'no-entry', f'{pn}: no entry {shaders[i]}')
            continue
        k, keyed = _pick_program(db, ep.get(g, []), lg)
        if k is None:
            _flag(flags, notes, 'no-program', f'{pn}: entry {shaders[i]} has no main-pass program')
            continue
        if not keyed:
            if '_zonly' in pn.lower():                   # (..._ZOnly, ..._ZOnly_split1 ...)
                continue                                   # depth-only part: drawn in the depth passes only
            _flag(flags, notes, 'layout-fallback', f'{pn}: no program for its vertex layout in {shaders[i]} '
                  '(the engine draws it with something else)')
        v, ps, _, b2 = db.program_refs(k)
        vs_s, ps_s = db.vs[v], db.ps[ps]
        pos = _positions(ldata, md, p, lays[lg])
        if pos is not None:
            pts.append(pos)
        d = Draw(pn, entry, k, origin(vs_s, ps_s), vs_s.code, ps_s.code,
                 [e for e in vs_elements(vs_s.tail) if e[2] == 0], p['vertex_offset'], p['stride'],
                 p['triangles'] * 3, p['first_index'], db.keys[k][0x33], part_bones(ms, p),
                 S.block_constants(db.params[b2]), [], [])
        d.samplers = sampler_states(db.params[ps_s.param])
        records = {s: (c, n) for s, c, n in P.block_texture_list(db.params[b2])}
        engine = {s for _, s in S.block_engine_textures(db.params[b2])}
        for name, typ, bp, dim in resources(ps_s.code):
            if typ != 2 or bp == 0:
                continue
            if name == 'texture_outdoorLightReflectionEnvmap' and bp not in records:
                tn = envmap
            elif bp in records:
                tn = records[bp][1]
            else:
                if bp not in engine and name.startswith('texture_Texture'):
                    d.flags.append('bound-empty')
                    _flag(flags, notes, 'bound-empty', f'{pn}: t{bp} ({name}) has no texture')
                continue
            if tn not in out.textures:
                b = files.read(tn + '.res')
                if b is None:
                    out.textures[tn] = b''
                else:
                    why = texture_problem(b)
                    out.textures[tn] = b if not why else b''
                    if why:
                        _flag(flags, notes, 'bad-texture', f'{tn}: {why}')
            if not out.textures[tn]:
                if tn != envmap:
                    d.flags.append('missing-texture')
                    _flag(flags, notes, 'missing-texture', f'{pn}: t{bp} {tn} not found')
                continue
            cube = struct.unpack_from('<I', out.textures[tn], 4)[0] == 1
            if cube != (dim == 9):
                d.flags.append('dimension')
                _flag(flags, notes, 'dimension', f'{pn}: t{bp} {name} is {"a cube" if dim == 9 else "2D"}, '
                      f'{tn} is {"a cube" if cube else "2D"}')
                continue
            d.textures.append((bp, tn))
        out.draws.append(d)
    if pts:
        allp = np.concatenate(pts)
        allp = allp[np.isfinite(allp).all(1)]
        if len(allp):
            out.lo, out.hi = allp.min(0), allp.max(0)
    if not out.draws and not flags:
        _flag(flags, notes, 'no-entry', 'no part to draw')
    return out


# ------------------------------------------------------------------------------------------------ camera, constants

def look(eye: np.ndarray, target: np.ndarray, fov: float, near: float, far: float) -> tuple[np.ndarray, np.ndarray]:
    """(view, view-projection), rows (clip = M @ [p, 1]; D3D, z 0..1). The game's data is right-handed: screen
    right is forward x up."""
    f = target - eye
    f = f / np.linalg.norm(f)
    r = np.cross(f, [0.0, 1.0, 0.0])
    r = r / np.linalg.norm(r)
    u = np.cross(r, f)
    v = np.eye(4)
    v[0, :3], v[1, :3], v[2, :3] = r, u, f
    v[:3, 3] = -v[:3, :3] @ eye
    t = 1 / math.tan(fov / 2)
    p = np.zeros((4, 4))
    p[0, 0], p[1, 1] = t, t
    p[2, 2], p[2, 3] = far / (far - near), -near * far / (far - near)
    p[3, 2] = 1
    return v, p @ v


def camera(lo: np.ndarray, hi: np.ndarray, azimuth: float, elevation: float) -> tuple[np.ndarray, np.ndarray,
                                                                                        np.ndarray]:
    """(eye, view, view-projection) of an orbit camera around the bounds: azimuth 0 looks at the mesh's front
    (from +z, where BFBC2's models face), elevation up from the horizon."""
    centre = (lo + hi) / 2
    radius = max(float(np.linalg.norm(hi - lo)) / 2, 0.05)
    az, el = math.radians(azimuth), math.radians(elevation)
    eye = centre + radius * 2.4 * np.array([math.cos(el) * math.sin(az), math.sin(el), math.cos(el) * math.cos(az)])
    view, vp = look(eye, centre, math.radians(45), radius * 0.05, radius * 10)
    return eye, view, vp


def cbuffer(code: bytes, values: dict[str, Any], material: Iterable[tuple[str, int, int, tuple[float, ...]]]) -> bytes:
    """A shader's $Globals: engine values by name; the material's constants by register, only those inside the
    shader's own 'constants' array (a program's block serves both shaders, each its own registers)."""
    cbs, _ = P.rdef(code)
    g = cbs.get('$Globals', [])
    size = max([off + sz for _, off, sz, _ in g] or [16])
    buf = np.zeros((size + 15) // 16 * 4, np.float32)
    arr = [(off // 16, (off + sz) // 16) for n, off, sz, _ in g if n == 'constants']
    for name, off, sz, _ in g:
        v = values.get(name)
        if v is None:
            continue
        a = np.asarray(v, np.float32).ravel()[:sz // 4]
        buf[off // 4:off // 4 + len(a)] = a
    for _, reg, _, vals in material:
        if arr and arr[0][0] <= reg < arr[0][1]:
            buf[reg * 4:reg * 4 + 4] = vals
    return buf.tobytes()


def bone_values(d: Draw) -> dict[str, Any]:
    if d.kind == 9:                       # destructible composite: boneMatrices[i].x > 0 shows part i
        ones = np.ones((180, 4))
        return {'boneMatrices': ones, 'boneVectors': ones}
    b = d.bones if d.bones is not None else np.tile(np.eye(4)[:3], (60, 1))
    return {'boneMatrices': b, 'boneVectors': b}


# ------------------------------------------------------------------------------------------------ shaderview

def shaderview(build: bool = True) -> str:
    """shaderview.exe: the suite's build\\ folder, else native\\; built into build\\ when neither has one."""
    for p in (os.path.join(BUILD, 'shaderview.exe'), os.path.join(NATIVE, 'shaderview.exe')):
        if os.path.isfile(p) and os.path.getmtime(p) >= os.path.getmtime(os.path.join(NATIVE, 'shaderview.cpp')):
            return p
    if build:
        os.makedirs(BUILD, exist_ok=True)
        r = subprocess.run(['cmd', '/c', os.path.join(NATIVE, 'build.bat'), BUILD], capture_output=True, text=True)
        p = os.path.join(BUILD, 'shaderview.exe')
        if r.returncode == 0 and os.path.isfile(p):
            return p
        raise RenderQaError('shaderview.exe could not be built (needs the Visual Studio C++ tools): '
                            + (r.stdout + r.stderr).strip()[-400:])
    raise RenderQaError('shaderview.exe not built: run bfbc2-shader-tools\\native\\build.bat')


def job_path(path: str) -> str:
    """Quote a path for shaderview's std::quoted parser (including Windows backslashes)."""
    return '"' + path.replace('\\', '\\\\').replace('"', '\\"') + '"'


def run_job(job: str, warp: bool = False) -> tuple[list[str], bool]:
    """Run a shaderview job at below-normal priority: (its output lines, whether WARP was used). The GPU first;
    the software rasterizer when the GPU device fails (a busy or lost device)."""
    exe = shaderview()
    flags = BELOW_NORMAL if os.name == 'nt' else 0
    for use_warp in ([True] if warp else [False, True]):
        r = subprocess.run([exe, job] + (['-warp'] if use_warp else []), capture_output=True, text=True,
                           creationflags=flags)
        if r.returncode == 0:
            return r.stdout.splitlines(), use_warp
        if r.returncode not in (3, 4) or use_warp:
            raise RenderQaError(f'shaderview failed ({r.returncode}): {r.stderr.strip()[-400:]}')
    raise RenderQaError('shaderview failed')


@dataclass
class Rendered:
    mesh: str
    views: dict[str, np.ndarray]                     # LDR RGB 0..1 (background composited)
    masks: dict[str, np.ndarray]
    hdr: dict[str, np.ndarray]
    counts: list[tuple[str, int, int]]               # per draw (part, shaded samples, covered samples)
    exposure: float
    warp: bool


def render(prep: Prepared, values: dict[str, Any], post: dict[str, Any], size: int = 256,
           bounds: tuple[np.ndarray, np.ndarray] | None = None, work: str | None = None,
           warp: bool = False) -> Rendered | None:
    """Draw a prepared mesh in every view (shaderview); None when it has nothing to draw."""
    if not prep.draws:
        return None
    lo, hi = bounds or (prep.lo, prep.hi)
    if lo is None or hi is None:
        return None
    if work:
        os.makedirs(work, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix='renderqa_', dir=work)
    try:
        return _render(prep, values, post, size, lo, hi, tmp, warp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def _render(prep: Prepared, values: dict[str, Any], post: dict[str, Any], size: int, lo: np.ndarray,
            hi: np.ndarray, tmp: str, warp: bool) -> Rendered:
    vb, ib = os.path.join(tmp, 'm.vb'), os.path.join(tmp, 'm.ib')
    open(vb, 'wb').write(prep.vertices)
    open(ib, 'wb').write(prep.indices)
    texfiles = {}
    for i, (name, b) in enumerate(prep.textures.items()):
        if b:
            texfiles[name] = os.path.join(tmp, f't{i}.res')
            open(texfiles[name], 'wb').write(b)
    lines: list[str] = []
    for k, d in enumerate(prep.draws):
        open(os.path.join(tmp, f'v{k}.dxbc'), 'wb').write(d.vs)
        open(os.path.join(tmp, f'p{k}.dxbc'), 'wb').write(d.ps)
    outs = {}
    for vi, (view, az, el) in enumerate(VIEWS):
        eye, vmat, vp = camera(lo, hi, az, el)
        base = dict(values, viewProjMatrix=vp, worldViewProjMatrix=vp, worldMatrix=np.eye(4), viewMatrix=vmat,
                    cameraPos=(*eye, 0.0), invScreenSize=(1 / size, 1 / size, 0.0, 0.0))
        out = os.path.join(tmp, f'{view}.raw')
        outs[view] = out
        lines += [f'frame {job_path(out)} {size} {size} 0 0 0 0', 'depth 1', f'count {1 if vi == 0 else 0}']
        for k, d in enumerate(prep.draws):
            vals = dict(base, **bone_values(d))
            cv, cp = os.path.join(tmp, f'cv{k}_{vi}.bin'), os.path.join(tmp, f'cp{k}_{vi}.bin')
            open(cv, 'wb').write(cbuffer(d.vs, vals, d.material))
            open(cp, 'wb').write(cbuffer(d.ps, vals, d.material))
            lines += [f'vs {job_path(os.path.join(tmp, f"v{k}.dxbc"))}', f'ps {job_path(os.path.join(tmp, f"p{k}.dxbc"))}']
            lines += [f'el {si} {fmt} {slot} {off} {inst}' for si, fmt, slot, off, inst in d.elements]
            lines += [f'vb {job_path(vb)} {d.vertex_offset} {d.stride}',
                      f'ib {job_path(ib)} 0 {32 if prep.wide else 16} {d.indices} {d.first}', f'cb vs {job_path(cv)}', f'cb ps {job_path(cp)}']
            lines += [f'samp {reg} {desc.hex()}' for reg, desc in d.samplers if reg < 16]
            lines += [f'texres {slot} {job_path(texfiles[tn])}' for slot, tn in d.textures if tn in texfiles]
            lines.append('draw')
        lines.append('end')
    job = os.path.join(tmp, 'job.txt')
    open(job, 'w').write('\n'.join(lines) + '\n')
    output, used_warp = run_job(job, warp)
    counts = []
    for line in output:
        if line.startswith('count '):
            _, n, shaded, covered = line.split()
            counts.append((prep.draws[int(n)].part, int(shaded), int(covered)))
    views, masks, hdrs = {}, {}, {}
    exposure = 1.0
    for view, _, _ in VIEWS:
        raw = np.fromfile(outs[view], '<f4').reshape(size, size, 4)
        depth = np.fromfile(outs[view] + '.depth', '<f4').reshape(size, size)
        mask = depth < 1.0
        hdr = raw[..., :3].astype(np.float32)
        bad = ~np.isfinite(hdr).all(-1)
        hdr = np.where(bad[..., None], 100.0, np.nan_to_num(hdr))     # NaN / inf: the game shows these white
        ldr, exposure = tonemap(hdr, mask & ~bad, post)
        ldr = np.where(mask[..., None], ldr, np.array(BACKGROUND, np.float32))
        views[view], masks[view], hdrs[view] = ldr, mask, np.where(bad[..., None], np.nan, hdr)
    return Rendered(prep.mesh, views, masks, hdrs, counts, exposure, used_warp)


# ------------------------------------------------------------------------------------------------ checks

LUM = np.array([0.2126, 0.7152, 0.0722], np.float32)


def image_flags(r: Rendered) -> tuple[list[str], list[str], dict[str, Any]]:
    """(flags, notes, numbers) of a rendered mesh: what its pictures show."""
    flags: list[str] = []
    notes: list[str] = []
    m = r.masks['threequarter']
    img = r.views['threequarter']
    nums: dict[str, Any] = {'coverage': float(m.mean()), 'exposure': r.exposure, 'warp': r.warp}
    for part, shaded, covered in r.counts:
        if covered >= 50 and shaded < 0.02 * covered:
            _flag(flags, notes, 'alpha-discards-all', f'{part}: pixel shader keeps {shaded} of {covered} samples')
    if m.sum() < 20:
        _flag(flags, notes, 'nothing-drawn', f'{int(m.sum())} pixels drawn')
        return flags, notes, nums
    px = img[m]
    lum = px @ LUM
    nan = sum(int((~np.isfinite(h).all(-1) & r.masks[v]).sum()) for v, h in r.hdr.items())
    nums.update(mean_lum=float(lum.mean()), std_lum=float(lum.std()), mean_rgb=[float(x) for x in px.mean(0)],
                nan_pixels=nan)
    magenta = float(((px[:, 0] > 0.75) & (px[:, 2] > 0.75) & (px[:, 1] < 0.3)).mean())
    nums['magenta'] = magenta
    if nan > 0.002 * m.sum():
        _flag(flags, notes, 'nan', f'{nan} pixels NaN / infinite (the game shows them white, blooming)')
    if lum.mean() < 0.03:
        _flag(flags, notes, 'black', f'mean luminance {lum.mean():.3f}')
    if magenta > 0.05:
        _flag(flags, notes, 'magenta', f'{magenta:.0%} of the pixels magenta')
    if lum.std() < 0.012 and lum.mean() >= 0.03:
        _flag(flags, notes, 'flat', f'luminance spread {lum.std():.4f} (one constant colour)')
    if lum.mean() > 0.85:
        _flag(flags, notes, 'blown-out', f'mean luminance {lum.mean():.2f} (chrome / blown out)')
    return flags, notes, nums


def compare_images(a: Rendered, b: Rendered) -> dict[str, Any]:
    """Per view and overall: coverage overlap (IoU), mean absolute difference and PSNR over the pixels both drew,
    mean colour of each and their difference."""
    out: dict[str, Any] = {}
    for view, _, _ in VIEWS:
        ma, mb = a.masks[view], b.masks[view]
        union = ma | mb
        both = ma & mb
        iou = float(both.sum() / union.sum()) if union.any() else 1.0
        if both.any():
            da, db_ = a.views[view][both], b.views[view][both]
            diff = np.abs(da - db_)
            mae = float(diff.mean())
            mse = float((diff ** 2).mean())
            psnr = 99.0 if mse == 0 else float(10 * math.log10(1.0 / mse))
            ca, cb = da.mean(0), db_.mean(0)
        elif not union.any():                               # neither drew anything: equal
            mae, psnr = 0.0, 99.0
            ca = cb = np.zeros(3)
        else:
            mae, psnr = 1.0, 0.0
            ca = a.views[view][ma].mean(0) if ma.any() else np.zeros(3)
            cb = b.views[view][mb].mean(0) if mb.any() else np.zeros(3)
        out[view] = {'iou': iou, 'mae': mae, 'psnr': psnr, 'mean_truth': [float(x) for x in ca],
                     'mean_test': [float(x) for x in cb], 'lum_ratio': float((cb @ LUM + 1e-4) / (ca @ LUM + 1e-4))}
    out['iou'] = min(out[v]['iou'] for v, _, _ in VIEWS)
    out['mae'] = max(out[v]['mae'] for v, _, _ in VIEWS)
    out['psnr'] = min(out[v]['psnr'] for v, _, _ in VIEWS)
    out['lum_ratio'] = out['threequarter']['lum_ratio']
    return out


def severity(flags: Iterable[str], cmp: dict[str, Any] | None = None) -> float:
    s = float(sum(WEIGHT.get(f, 1) for f in set(flags)))
    if cmp:
        s += 100 * cmp['mae'] + 30 * (1 - cmp['iou']) + 10 * abs(math.log(max(cmp['lum_ratio'], 1e-3)))
    return round(s, 3)


# ------------------------------------------------------------------------------------------------ pictures

def _font() -> Any:
    from PIL import ImageFont
    for name in ('consola.ttf', 'arial.ttf'):
        try:
            return ImageFont.truetype(name, 11)
        except OSError:
            continue
    return ImageFont.load_default()


def _tile(img: np.ndarray) -> Any:
    from PIL import Image
    return Image.fromarray((np.clip(img, 0, 1) * 255 + 0.5).astype(np.uint8))


def diff_image(a: Rendered, b: Rendered, view: str) -> np.ndarray:
    """Heat map: black = equal; red = brighter / only in the tested picture; blue = darker / only in the truth."""
    ma, mb = a.masks[view], b.masks[view]
    d = (b.views[view] @ LUM) - (a.views[view] @ LUM)
    absd = np.abs(b.views[view] - a.views[view]).mean(-1)
    img = np.zeros(ma.shape + (3,), np.float32)
    both = ma & mb
    img[..., 0] = np.where(both, np.clip(absd * 4 * (d >= 0), 0, 1), 0)
    img[..., 2] = np.where(both, np.clip(absd * 4 * (d < 0), 0, 1), 0)
    img[..., 1] = np.where(both, np.clip(absd * 2, 0, 1) * 0.5, 0)
    img[mb & ~ma] = (1.0, 0.2, 0.2)
    img[ma & ~mb] = (0.2, 0.4, 1.0)
    return img


def sheet(rows: Sequence[tuple[str, Sequence[np.ndarray]]], path: str, tile: int = 192,
          header: Sequence[str] = ()) -> None:
    """A contact sheet: one row per mesh (its label on top, its tiles beside each other)."""
    from PIL import Image, ImageDraw
    font = _font()
    cols = max((len(t) for _, t in rows), default=1)
    label_h = 30
    head_h = 16 if header else 0
    img = Image.new('RGB', (cols * tile, head_h + len(rows) * (tile + label_h)), (24, 26, 30))
    dr = ImageDraw.Draw(img)
    for c, h in enumerate(header):
        dr.text((c * tile + 4, 2), h, fill=(200, 200, 200), font=font)
    for r, (label, tiles) in enumerate(rows):
        y = head_h + r * (tile + label_h)
        for li, line in enumerate(label.split('\n')[:2]):
            dr.text((4, y + 2 + 13 * li), line[:cols * tile // 6], fill=(255, 230, 120) if li == 0 else (230, 160, 160),
                    font=font)
        for c, t in enumerate(tiles):
            img.paste(_tile(t).resize((tile, tile)), (c * tile, y + label_h))
    img.save(path)


def _short(mesh: str) -> str:
    return mesh.replace('\\', '/').split('/')[-1]


def _safe(mesh: str) -> str:
    return re.sub(r'[^A-Za-z0-9_.-]+', '_', mesh.strip('/'))[-120:]


# ------------------------------------------------------------------------------------------------ truth databases

def truth_index(root: str, cache: str | None = None) -> dict[str, list[str]]:
    """{mesh (lower case): [level names]} - the BFBC2 levels whose Dx11 database has entries for the mesh (stock
    levels only: Onslaught / OPFOR levels and any level an OPFOR install step put into the game copy left out).
    Cached in the suite's build\\ folder."""
    bundles = {}
    for lv, b in S.level_bundles(root).items():
        folder = os.path.dirname(b)
        if re.match(r'(ons_|opfor_)', lv) or any(re.fullmatch(r'opfor-.+-installed\.json', f.lower())
                                                 for f in os.listdir(folder)):
            continue
        bundles[lv] = b
    sig = sorted((lv, b, os.path.getsize(b), int(os.path.getmtime(b))) for lv, b in bundles.items())
    path = cache or os.path.join(BUILD, 'renderqa_truth.pkl')
    try:
        with open(path, 'rb') as fh:
            c = pickle.load(fh)
        if c['sig'] == sig:
            return c['index']
    except Exception:                                       # noqa: BLE001 - missing or old: rebuilt
        pass
    print('indexing the levels\' Dx11 shader databases (once; cached) ...', file=sys.stderr)
    index: dict[str, list[str]] = {}
    for lv in sorted(bundles):
        try:
            db, _ = S.load(f'level:{lv}', 11, root)
        except Exception:                                   # noqa: BLE001 - a level without a Dx11 database
            continue
        for m in db.mesh_names():
            index.setdefault(m, []).append(lv)
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, 'wb') as fh:
        pickle.dump({'sig': sig, 'index': index}, fh)
    return index


class Databases:
    """Level databases loaded once each."""

    def __init__(self, root: str | None) -> None:
        self.root = root
        self.cache: dict[str, S.ShaderDb] = {}

    def get(self, spec: str) -> S.ShaderDb:
        if spec not in self.cache:
            db, _ = S.load(spec, 11 if spec.lower().startswith('level:') else None, self.root)
            if db.dx not in (10, 11):
                raise RenderQaError(f'{spec}: a Dx{db.dx} database (Dx10 or Dx11 needed)')
            self.cache[spec] = db
        return self.cache[spec]


# ------------------------------------------------------------------------------------------------ runs

@dataclass
class Settings:
    out: str
    size: int = 256
    warp: bool = False
    envmap: str = ENVMAP
    weather: str = WEATHER
    log: Callable[[str], None] = print
    sun: str = 'noon'


def _light(files: Files, s: Settings) -> tuple[dict[str, Any], dict[str, Any], str]:
    try:
        w, label = read_weather(files, s.weather)
    except (RenderQaError, OSError) as e:
        s.log(f'weather: {e}; mp_005 Sunny values built in')
        w, label = MP005_SUNNY, 'built-in mp_005 Sunny'
    values, post = light(w, s.sun)
    return values, post, f'{label}, sun {s.sun}'


def render_one(files: Files, db: S.ShaderDb, mesh: str, s: Settings, values: dict[str, Any], post: dict[str, Any],
               bounds: tuple[np.ndarray, np.ndarray] | None = None) -> tuple[Prepared, Rendered | None, dict[str, Any]]:
    prep = prepare(files, db, mesh, s.envmap)
    rec: dict[str, Any] = {'mesh': mesh, 'flags': list(prep.flags), 'notes': list(prep.notes),
                           'shader_set': prep.shader_set,
                           'parts': [{'part': d.part, 'entry': d.entry, 'program': d.program, 'origin': d.origin,
                                      'kind': d.kind, 'textures': [f't{a} {b}' for a, b in d.textures],
                                      'flags': d.flags} for d in prep.draws]}
    rec['origin'] = 'translated' if any(d.origin == 'translated' for d in prep.draws) else \
        ('dice' if prep.draws else '')
    r = None
    try:
        r = render(prep, values, post, s.size, bounds, s.out, s.warp)
    except RenderQaError as e:
        _flag(rec['flags'], rec['notes'], 'render-failed', str(e))
    if r is not None:
        f, n, nums = image_flags(r)
        for x in f:
            if x not in rec['flags']:
                rec['flags'].append(x)
        rec['notes'] += n
        rec['image'] = nums
        rec['counts'] = r.counts
    elif prep.draws and 'render-failed' not in rec['flags']:
        _flag(rec['flags'], rec['notes'], 'nothing-drawn', 'no bounds')
    return prep, r, rec


def _blank(size: int) -> np.ndarray:
    return np.tile(np.array([0.12, 0.0, 0.12], np.float32), (size, size, 1))


def _write(s: Settings, kind: str, records: list[dict[str, Any]], rows: list[tuple[float, str, list[np.ndarray]]],
           header: Sequence[str], meta: dict[str, Any]) -> None:
    os.makedirs(s.out, exist_ok=True)
    order = sorted(range(len(records)), key=lambda i: -records[i]['severity'])
    json.dump({'kind': kind, 'meta': meta, 'results': [records[i] for i in order]},
              open(os.path.join(s.out, 'results.json'), 'w'), indent=1, default=float)
    per = 14
    sheets = []
    for n in range(0, len(order), per):
        page = [(rows[i][1], rows[i][2]) for i in order[n:n + per]]
        p = os.path.join(s.out, f'sheet_{n // per + 1:02d}.png')
        sheet(page, p, header=header)
        sheets.append(p)
    # the same, only meshes drawn with translated shaders (BF1943's Xbox shaders compiled for PC)
    tr = [i for i in order if records[i].get('origin') == 'translated']
    if tr and len(tr) < len(order):
        for n in range(0, len(tr), per):
            p = os.path.join(s.out, f'translated_{n // per + 1:02d}.png')
            sheet([(rows[i][1], rows[i][2]) for i in tr[n:n + per]], p, header=header)
            sheets.append(p)
    lines = [f'renderqa {kind}: {len(records)} meshes', *(f'{k}: {v}' for k, v in meta.items()), '']
    counts: dict[str, int] = {}
    for r in records:
        for f in r['flags']:
            counts[f] = counts.get(f, 0) + 1
    lines.append('flags: ' + (', '.join(f'{k} {v}' for k, v in sorted(counts.items(), key=lambda x: -x[1])) or 'none'))
    lines.append('')
    for i in order:
        r = records[i]
        c = r.get('compare')
        extra = f'  iou {c["iou"]:.3f} mae {c["mae"]:.4f} psnr {c["psnr"]:.1f} lum x{c["lum_ratio"]:.2f}' if c else ''
        lines.append(f'{r["severity"]:7.2f}  {r["mesh"]}  [{r.get("origin", "")}] {" ".join(r["flags"]) or "ok"}{extra}')
        for note in r['notes'][:8]:
            lines.append(f'           {note}')
    lines += ['', 'sheets (worst first):', *sheets]
    open(os.path.join(s.out, 'summary.txt'), 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    s.log(f'{len(records)} meshes -> {os.path.join(s.out, "summary.txt")}, {len(sheets)} sheets')


def run_render(files: Files, db: S.ShaderDb, meshes: Sequence[str], s: Settings, label: str) -> list[dict[str, Any]]:
    values, post, wl = _light(files, s)
    records, rows = [], []
    for i, m in enumerate(meshes):
        s.log(f'[{i + 1}/{len(meshes)}] {m}')
        _, r, rec = render_one(files, db, m, s, values, post)
        rec['severity'] = severity(rec['flags'])
        tiles = [r.views[v] for v, _, _ in VIEWS] if r else [_blank(s.size)] * 3
        if r:
            _tile(np.concatenate(tiles, 1)).save(os.path.join(s.out, _safe(m) + '.png'))
        records.append(rec)
        rows.append((rec['severity'], f'{_short(m)}  [{rec["origin"]}]\n{" ".join(rec["flags"]) or "ok"}', tiles))
    _write(s, 'render', records, rows, [v for v, _, _ in VIEWS], {'database': label, 'weather': wl})
    return records


def run_compare(files: Files, db: S.ShaderDb, meshes: Sequence[str], s: Settings, label: str,
                truth: str | None, dbs: Databases, tindex: dict[str, list[str]] | None) -> list[dict[str, Any]]:
    """Each mesh: DICE's entry (truth database, stock files) against the tested database (staged files first)."""
    values, post, wl = _light(files, s)
    stock = StockFiles(files)
    records, rows = [], []
    for i, m in enumerate(meshes):
        spec = truth
        if spec is None and tindex is not None:
            lv = tindex.get(m.lower())
            spec = f'level:{lv[0]}' if lv else None
        s.log(f'[{i + 1}/{len(meshes)}] {m}  (truth {spec})')
        if spec is None:
            _, r, rec = render_one(files, db, m, s, values, post)
            rec['severity'] = severity(rec['flags'])
            rec['truth'] = None
            records.append(rec)
            tiles = [r.views[v] for v, _, _ in VIEWS] if r else [_blank(s.size)] * 3
            if r:
                _tile(np.concatenate(tiles, 1)).save(os.path.join(s.out, _safe(m) + '.png'))
            rows.append((rec['severity'], f'{_short(m)}  [{rec["origin"]}] (no truth)\n{" ".join(rec["flags"]) or "ok"}',
                         [_blank(s.size)] * 3 + tiles))
            continue
        tprep, tr, trec = render_one(stock, dbs.get(spec), m, s, values, post)
        bounds = (tprep.lo, tprep.hi) if tprep.lo is not None and tprep.hi is not None else None
        _, r, rec = render_one(files, db, m, s, values, post, bounds)
        rec['truth'] = {'database': spec, 'flags': trec['flags'], 'notes': trec['notes'], 'image': trec.get('image'),
                        'shader_set': trec['shader_set']}
        rec['staged_mesh'] = files.staged(m.lower() + '_lod0_data.res')
        # problems DICE's own entry has too are not the conversion's
        own = [f for f in rec['flags'] if f not in trec['flags']]
        cmp = compare_images(tr, r) if tr and r else None
        rec['compare'] = cmp
        rec['severity'] = severity(own, cmp) if cmp else severity(own) + 20
        records.append(rec)
        ts = [tr.views[v] for v, _, _ in VIEWS] if tr else [_blank(s.size)] * 3
        ws = [r.views[v] for v, _, _ in VIEWS] if r else [_blank(s.size)] * 3
        ds = [diff_image(tr, r, v) for v, _, _ in VIEWS] if tr and r else [_blank(s.size)] * 3
        if tr and r:
            _tile(np.concatenate([np.concatenate(ts, 1), np.concatenate(ws, 1), np.concatenate(ds, 1)], 0)).save(
                os.path.join(s.out, _safe(m) + '.png'))
        stat = f'iou {cmp["iou"]:.2f} mae {cmp["mae"]:.3f} lum x{cmp["lum_ratio"]:.2f}' if cmp else 'not compared'
        rows.append((rec['severity'], f'{_short(m)}  [{rec["origin"]}{", staged mesh" if rec["staged_mesh"] else ""}] '
                     f'vs {spec}  {stat}\n{" ".join(own) or "ok"}', [ts[0], ws[0], ds[0], ts[1], ws[1], ds[1]]))
    _write(s, 'compare', records, rows, ['DICE 3/4', 'tested 3/4', 'diff 3/4', 'DICE front', 'tested front',
                                         'diff front'], {'database': label, 'weather': wl})
    return records


def run_validate(files: Files, meshes: Sequence[str], s: Settings, dbs: Databases,
                 tindex: dict[str, list[str]]) -> list[dict[str, Any]]:
    """Stock meshes with their own level's database, beside meshview's texture-only render of the same mesh."""
    import meshview
    values, post, wl = _light(files, s)
    records, rows = [], []
    for i, m in enumerate(meshes):
        lv = tindex.get(m.lower())
        s.log(f'[{i + 1}/{len(meshes)}] {m}  ({lv[0] if lv else "no level draws it"})')
        if not lv:
            continue
        _, r, rec = render_one(files, dbs.get(f'level:{lv[0]}'), m, s, values, post)
        rec['database'] = f'level:{lv[0]}'
        ref = None
        try:
            parts = meshview.load_mesh(files.ix(), m, 0)
            laid = meshview._layout([(m, parts)])
            ref = {v: meshview.render(laid, s.size, v).astype(np.float32) / 255 for v in ('side', 'front', 'top')}
        except Exception as e:                              # noqa: BLE001 - shown as a missing reference
            rec['notes'].append(f'meshview: {e}')
        if r is not None and ref is not None:
            ours = r.views['threequarter'][r.masks['threequarter']]
            mv = ref['side'].reshape(-1, 3)
            mv = mv[np.abs(mv - 40 / 255).sum(1) > 1e-3]               # not meshview's background
            if len(ours) and len(mv):
                a, b = ours.mean(0), mv.mean(0)
                chroma = lambda c: (c - c.mean()) / (np.linalg.norm(c - c.mean()) + 1e-6)     # noqa: E731
                rec['reference'] = {'mean_rgb': [float(x) for x in a], 'meshview_mean_rgb': [float(x) for x in b],
                                    'hue_agreement': float(chroma(a) @ chroma(b))}
        rec['severity'] = severity(rec['flags'])
        records.append(rec)

        def fit(img: np.ndarray) -> np.ndarray:
            h, w = img.shape[:2]
            out = np.full((s.size, s.size, 3), 40 / 255, np.float32)
            sc = min(s.size / h, s.size / w)
            im = np.asarray(_tile(img).resize((max(1, int(w * sc)), max(1, int(h * sc))))).astype(np.float32) / 255
            out[:im.shape[0], :im.shape[1]] = im
            return out
        tiles = [r.views[v] for v, _, _ in VIEWS] if r else [_blank(s.size)] * 3
        tiles += [fit(ref[v]) for v in ('side', 'front', 'top')] if ref else [_blank(s.size)] * 3
        hue = rec.get('reference', {}).get('hue_agreement')
        rows.append((rec['severity'], f'{_short(m)}  {rec["database"]}  hue agreement '
                     f'{hue if hue is None else round(hue, 2)}\n{" ".join(rec["flags"]) or "ok"}', tiles))
    _write(s, 'validate', records, rows, ['harness 3/4', 'harness front', 'harness top', 'meshview side',
                                          'meshview front', 'meshview top'], {'weather': wl})
    return records


# ------------------------------------------------------------------------------------------------ terrain strips

# A VisualTerrain's road blocks (terrain.road_strips): roads and quad decals baked into strips of 28-byte vertices
# (world position, texture coordinate, 4 halves), each drawn with its shader entry's program for BFBC2's strip layout.
STRIP_LAYOUT = bytes.fromhex('bcd37755dfa1714a6f0d126aed6ad4ed')
STRIP_VIEWS = (('top', 0.0, 89.0), ('threequarter', 45.0, 35.0))
# a strip with one of these is wrong (the command exits 1): nothing to draw it with, nothing drawn, NaN, drawn outside
# its own vertices' box (the "sheets across the sky" of a shader built for another vertex layout), a texture another
# level owns (Terrains/<other level>/...) or none found, every pixel discarded
STRIP_FAIL = frozenset({'no-entry', 'no-program', 'nothing-drawn', 'nan', 'out-of-bounds', 'foreign-texture',
                        'missing-texture', 'bad-texture', 'dimension', 'alpha-discards-all', 'render-failed'})


def strip_values(extent: tuple[float, float, float, float]) -> dict[str, tuple[float, ...]]:
    """The terrain renderer's named block constants (pcbuild.ENGINE_NAMED) as estimated for an offline draw - not the
    game's: the height map read through the terrain's extent (x0, z0, x1, z1) (heightmapScaleAndOffsetXZ: world x, z
    to 0..1), flat (STRIP_TEXTURES: height 0, so the shaders' height-map normal points up), no decal lift, the
    dynamic (destruction) mask empty (its atlas parameters a 1:1 tile; its maps 0: nothing destroyed)."""
    x0, z0, x1, z1 = extent
    sx, sz = 1.0 / max(x1 - x0, 1e-3), 1.0 / max(z1 - z0, 1e-3)
    return {'heightmapScaleAndOffsetXZ': (sx, sz, -x0 * sx, -z0 * sz),
            'heightmapDecalOffsetY': (0.0, 0.0, 0.0, 0.0),
            'heightmapPixelSizeAndAspect': (1.0 / 1024, 1.0, 1.0, 0.0),
            'heightmapTexCoordScaleAndOffset': (1.0, 1.0, 0.0, 0.0),
            'heightmapWorldScaleY': (1.0, 0.0, 0.0, 0.0),
            'dynamicMaskAtlasOffset': (0.0, 0.0, 0.0, 0.0),
            'dynamicMaskAtlasParams': (1.0, 1.0, 1.0, 1.0),
            'dynamicMaskTileBorderParams': (1.0, 0.0, 0.0, 0.0)}


# the engine textures a strip's block names, bound as flat float textures: the height map at height 0, the dynamic
# mask's tile map and indirection map 0
STRIP_TEXTURES = {'heightmap': 0.0, 'dynamicMaskTileMap': 0.0, 'dynamicMaskIndirectionMap': 0.0, 'maskTileMap': 0.0}


def strip_cbuffer(code: bytes, values: dict[str, Any], records: Sequence[tuple[str, int, int, tuple[float, ...]]]) \
        -> bytes:
    """A strip shader's $Globals: cbuffer's (engine values by name, the block's material constants by register inside
    'constants'), then each named block constant the engine writes (records naming a key of values) at its register -
    DICE's strip shaders read them inside their 'constants' array, the translated ones as named variables."""
    buf = np.frombuffer(cbuffer(code, values, records), np.float32).copy()
    for name, reg, _, _ in records:
        v = values.get(name)
        if v is not None and reg * 4 + 4 <= len(buf):
            buf[reg * 4:reg * 4 + 4] = np.asarray(v, np.float32).ravel()[:4]
    return buf.tobytes()


def _flat_texture(path: str, value: float) -> str:
    texels = np.full((4, 4, 4), value, np.float32)
    with open(path, 'wb') as f:
        f.write(struct.pack('<III', 4, 4, 1) + texels.tobytes())
    return path


_AUTHORED: set[str] | None = None


def bf1943_authored(name: str) -> bool:
    """Whether a texture is one Battlefield 1943 itself ships outside its levels' own folders (its name among the
    source's resources, its folder no BF1943 level's): a translated BF1943 shader names its own textures, some under another level's folder (Iwo Jima's
    Decal_HaggardWasHere samples Terrains/SP_01_GreenAcres/Textures/HaggardWasHere_A) - that is the source's own
    texture, not one a BFBC2 donor brought from another level. False when no BF1943 source is there."""
    global _AUTHORED
    if _AUTHORED is None:
        try:
            suite_paths.use('bf1943-import')
            import bf1943import
            src = bf1943import.Source(bf1943import.default_source())
            levels = {k.split('/')[1] for k in src.entries if k.startswith('levels/') and k.count('/') == 1}
            levels = {lv[:-4] for lv in levels if lv.endswith('.dbx')}
            # not another BF1943 level's own folder (Wake's ground on Iwo Jima is still another level's texture)
            _AUTHORED = {k[:-4] for k, (_, e) in src.entries.items() if k.endswith('.res') and
                         e.get('type') == 'XenonTexture' and terrain_folder(k) not in levels}
        except Exception:                                   # noqa: BLE001 - no source: nothing counts as authored
            _AUTHORED = set()
    return name.lower().replace('\\', '/') in _AUTHORED


def foreign_texture(name: str, own: str | None) -> bool:
    """A texture of another level's folder than own - and not one BF1943 itself ships (bf1943_authored)."""
    folder = terrain_folder(name)
    return bool(own and folder and folder != own and not bf1943_authored(name))


def terrain_folder(name: str) -> str | None:
    """The level folder a resource name (or a file of an unpacked bundle) lies in (terrains/<folder>/...), lower
    case, or None (also for DICE's shared terrain library, Terrains/_Textures/...)."""
    hits = re.findall(r'(?:^|/)terrains/([^/_][^/]*)/', name.lower().replace('\\', '/'))
    return hits[-1] if hits else None


def prepare_strip(files: Files, db: S.ShaderDb, strip: dict[str, Any], own: str | None, envmap: str = ENVMAP) \
        -> tuple[Draw | None, list[str], list[str], dict[str, Any]]:
    """(the draw, flags, notes, extra) of one strip: its entry's main-pass program for the strip layout (no point
    light), the textures its program block binds, the engine textures it names; extra: the VS / PS named-constant
    records ('vs_named', 'ps_named') and the engine textures by slot ('engine')."""
    flags: list[str] = []
    notes: list[str] = []
    extra: dict[str, Any] = {'engine': {}, 'vs_named': [], 'ps_named': [], 'textures': []}
    ents = {n.lower(): g for n, g in db.entries}
    g = ents.get(strip['shader'].lower())
    if g is None:
        _flag(flags, notes, 'no-entry', f'{strip["shader"]}: not in the database')
        return None, flags, notes, extra
    progs = [k for k in db.entry_programs().get(g, []) if db.keys[k][0x30] == 0 and db.keys[k][0x34] == 0
             and db.keys[k][0x20:0x30] == STRIP_LAYOUT]
    progs = [k for k in progs if db.keys[k][0x38] == 0] or progs
    if not progs:
        _flag(flags, notes, 'no-program', f'{strip["shader"]}: no main-pass program for the strip layout')
        return None, flags, notes, extra
    k = progs[0]
    v, p, _, b2 = db.program_refs(k)
    vs_s, ps_s = db.vs[v], db.ps[p]
    blk = db.params[b2]
    extra['vs_named'] = S.block_constants(db.params[vs_s.param])
    extra['ps_named'] = S.block_constants(blk) + S.block_constants(db.params[ps_s.param])
    d = Draw(strip['shader'], strip['shader'], k, origin(vs_s, ps_s), vs_s.code, ps_s.code,
             [e for e in vs_elements(vs_s.tail) if e[2] == 0], 0, 28, strip['triangles'] * 3, strip['first'],
             0, None, extra['ps_named'], [], sampler_states(db.params[ps_s.param]))
    records = {s_: (c, n) for s_, c, n in P.block_texture_list(blk)}
    engine = {s_: n for n, s_ in S.block_engine_textures(blk)}
    for name, typ, bp, dim in resources(ps_s.code):
        if typ != 2:
            continue
        if bp in records:
            tn = records[bp][1]
        elif name == 'texture_outdoorLightReflectionEnvmap':
            tn = envmap
        elif bp in engine or name[len('texture_'):] in STRIP_TEXTURES:
            extra['engine'][bp] = engine.get(bp, name[len('texture_'):])
            continue
        else:
            if name.startswith('texture_Texture'):
                _flag(flags, notes, 'bound-empty', f't{bp} ({name}) has no texture')
            continue
        extra['textures'].append(tn)
        if foreign_texture(tn, own):
            _flag(flags, notes, 'foreign-texture', f't{bp} {tn}: another level\'s texture')
        b = files.read(tn + '.res')
        why = texture_problem(b) if b is not None else 'not found'
        if why or b is None:
            if tn != envmap:
                _flag(flags, notes, 'missing-texture' if b is None else 'bad-texture', f't{bp} {tn}: {why}')
            continue
        cube = struct.unpack_from('<I', b, 4)[0] == 1
        if cube != (dim == 9):
            _flag(flags, notes, 'dimension', f't{bp} {name}: {tn} {"is" if cube else "is not"} a cube')
            continue
        d.textures.append((bp, tn))
    return d, flags, notes, extra


def _strip_bounds(block: dict[str, Any], strip: dict[str, Any]) -> tuple[np.ndarray, np.ndarray] | None:
    import terrain
    idx = block['indices'][strip['first']:strip['first'] + 3 * strip['triangles']]
    if not len(idx) or idx.max() * 28 + 28 > len(block['vertices']):
        return None
    pos = terrain.road_positions(block['vertices'])[np.unique(idx)].astype(np.float64)
    pos = pos[np.isfinite(pos).all(1)]
    return (pos.min(0), pos.max(0)) if len(pos) else None


def run_strips(files: Files, db: S.ShaderDb, terrain_name: str, pc: bytes, s: Settings, label: str,
               only: Sequence[str] = (), limit: int = 0) -> list[dict[str, Any]]:
    """Every strip of a VisualTerrain drawn alone with its entry's program (estimated engine values, strip_values),
    from above and at three-quarters; each covered pixel's depth taken back to the world and held against the strip's
    own vertices' box. A strip drawn outside it (by more than 0.5 m + 2 % of its size) is 'out-of-bounds'."""
    import terrain
    values, post, wl = _light(files, s)
    blocks = terrain.road_strips(pc)
    parts = [terrain.road_positions(b['vertices']) for b in blocks if b['vertices']]
    allpos = np.concatenate(parts) if parts else np.zeros((1, 3), np.float32)
    lo_all, hi_all = allpos.min(0), allpos.max(0)
    pad = 0.05 * (hi_all - lo_all)
    extent = (float(lo_all[0] - pad[0]), float(lo_all[2] - pad[2]), float(hi_all[0] + pad[0]), float(hi_all[2] + pad[2]))
    values = dict(values, **strip_values(extent))
    own = terrain_folder(terrain_name)
    todo = [(b, k, st) for b in blocks for k, st in enumerate(b['strips'])
            if not only or any(o.lower() in st['shader'].lower() for o in only)]
    if limit:
        todo = todo[:limit]
    os.makedirs(s.out, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix='renderqa_strips_', dir=s.out)
    records, rows = [], []
    try:
        engine_files = {n: _flat_texture(os.path.join(tmp, f'e_{n}.bin'), v) for n, v in STRIP_TEXTURES.items()}
        vbs, ibs = {}, {}
        for b in blocks:
            if b['vertices']:
                vbs[b['block']] = os.path.join(tmp, f'b{b["block"]}.vb')
                ibs[b['block']] = os.path.join(tmp, f'b{b["block"]}.ib')
                open(vbs[b['block']], 'wb').write(b['vertices'])
                open(ibs[b['block']], 'wb').write(b['indices'].astype('<u4').tobytes())
        texfiles: dict[str, str] = {}
        lines: list[str] = []
        jobs = []
        for n, (b, k, st) in enumerate(todo):
            rec: dict[str, Any] = {'mesh': f'block {b["block"]} strip {k}: {st["shader"]}', 'block': b['block'],
                                   'strip': k, 'shader': st['shader'], 'second': st['second'],
                                   'triangles': st['triangles'], 'flags': [], 'notes': []}
            records.append(rec)
            if st['triangles'] == 0:
                rec['notes'].append('no triangles (switched off)')
                continue
            bounds = _strip_bounds(b, st)
            if bounds is None:
                _flag(rec['flags'], rec['notes'], 'nothing-drawn', 'its indices reach no vertex')
                continue
            d, fl, nt, extra = prepare_strip(files, db, st, own, s.envmap)
            rec['flags'] += fl
            rec['notes'] += nt
            rec['textures'] = extra['textures']
            if d is None:
                continue
            rec['origin'] = d.origin
            rec['program'] = d.program
            rec['bounds'] = [list(map(float, bounds[0])), list(map(float, bounds[1]))]
            for _, tn in d.textures:
                if tn not in texfiles:
                    texfiles[tn] = os.path.join(tmp, f't{len(texfiles)}.res')
                    open(texfiles[tn], 'wb').write(files.read(tn + '.res') or b'')
            vf, pf = os.path.join(tmp, f'v{n}.dxbc'), os.path.join(tmp, f'p{n}.dxbc')
            open(vf, 'wb').write(d.vs)
            open(pf, 'wb').write(d.ps)
            lo, hi = bounds
            for vi, (view, az, el) in enumerate(STRIP_VIEWS):
                eye, vmat, vp = camera(lo, hi, az, el)
                vals = dict(values, viewProjMatrix=vp, worldViewProjMatrix=vp, worldMatrix=np.eye(4), viewMatrix=vmat,
                            cameraPos=(*eye, 0.0), invScreenSize=(1 / s.size, 1 / s.size, 0.0, 0.0))
                out = os.path.join(tmp, f's{n}_{view}.raw')
                cv, cp = os.path.join(tmp, f'cv{n}_{vi}.bin'), os.path.join(tmp, f'cp{n}_{vi}.bin')
                open(cv, 'wb').write(strip_cbuffer(d.vs, vals, extra['vs_named']))
                open(cp, 'wb').write(strip_cbuffer(d.ps, vals, extra['ps_named']))
                lines += [f'frame {job_path(out)} {s.size} {s.size} 0 0 0 0', 'depth 1', f'count {1 if vi == 0 else 0}',
                          f'vs {job_path(vf)}', f'ps {job_path(pf)}']
                lines += [f'el {si} {fmt} {slot} {off} {inst}' for si, fmt, slot, off, inst in d.elements]
                lines += [f'vb {job_path(vbs[b["block"]])} 0 28',
                          f'ib {job_path(ibs[b["block"]])} 0 32 {d.indices} {d.first}',
                          f'cb vs {job_path(cv)}', f'cb ps {job_path(cp)}']
                lines += [f'samp {reg} {desc.hex()}' for reg, desc in d.samplers if reg < 16]
                lines += [f'texres {slot} {job_path(texfiles[tn])}' for slot, tn in d.textures]
                lines += [f'tex {slot} 2d {job_path(engine_files.get(nm, engine_files["heightmap"]))}'
                          for slot, nm in extra['engine'].items()]
                lines += ['draw', 'end']
                jobs.append((n, view, out, vp))
        output: list[str] = []
        if lines:
            job = os.path.join(tmp, 'job.txt')
            open(job, 'w').write('\n'.join(lines) + '\n')
            try:
                output, _ = run_job(job, s.warp)
            except RenderQaError as e:
                for rec in records:
                    if 'origin' in rec:
                        _flag(rec['flags'], rec['notes'], 'render-failed', str(e))
        counts = [tuple(int(x) for x in ln.split()[2:]) for ln in output if ln.startswith('count ')]
        drawn = [n for n, (b, k, st) in enumerate(todo) if 'origin' in records[n]]
        for n, c in zip(drawn, counts):
            records[n]['coverage'] = {'shaded': c[0], 'covered': c[1]}
            if c[1] and not c[0]:
                _flag(records[n]['flags'], records[n]['notes'], 'alpha-discards-all', 'every pixel discarded')
        views: dict[int, dict[str, np.ndarray]] = {}
        for n, view, out, vp in jobs:
            rec = records[n]
            if not os.path.isfile(out):
                continue
            raw = np.fromfile(out, '<f4').reshape(s.size, s.size, 4)
            depth = np.fromfile(out + '.depth', '<f4').reshape(s.size, s.size)
            mask = depth < 1.0
            hdr = raw[..., :3]
            if (~np.isfinite(hdr[mask])).any():
                _flag(rec['flags'], rec['notes'], 'nan', f'{view}: NaN / inf output')
            ldr, _ = tonemap(np.nan_to_num(hdr), mask, post)
            views.setdefault(n, {})[view] = np.where(mask[..., None], ldr, np.array(BACKGROUND, np.float32))
            if not mask.any():
                continue
            yy, xx = np.nonzero(mask)
            ndc = np.stack([(xx + 0.5) / s.size * 2 - 1, 1 - (yy + 0.5) / s.size * 2, depth[mask],
                            np.ones(len(xx))], 1)
            world = ndc @ np.linalg.inv(vp).T
            world = world[:, :3] / world[:, 3:4]
            lo, hi = (np.array(x) for x in rec['bounds'])
            excess = np.maximum(np.maximum(lo - world, world - hi), 0.0).max(0)
            tol = 0.5 + 0.02 * float(np.max(hi - lo))
            rec.setdefault('excess', {})[view] = [float(x) for x in excess]
            rec.setdefault('pixels', {})[view] = int(mask.sum())
            if excess.max() > tol:
                _flag(rec['flags'], rec['notes'], 'out-of-bounds',
                      f'{view}: drawn up to {excess.max():.1f} m outside its vertices (y {excess[1]:.1f} m; '
                      f'tolerance {tol:.1f} m)')
        for n, rec in enumerate(records):
            if 'origin' in rec and not rec.get('pixels') and 'render-failed' not in rec['flags']:
                _flag(rec['flags'], rec['notes'], 'nothing-drawn', 'no pixel covered in any view')
            rec['severity'] = severity(rec['flags'])
            rec['fail'] = sorted(set(rec['flags']) & STRIP_FAIL)
            tiles = [views.get(n, {}).get(v, _blank(s.size)) for v, _, _ in STRIP_VIEWS]
            rows.append((rec['severity'], f'{rec["mesh"].rsplit("/", 1)[-1]} [{rec.get("origin", "")}]\n'
                         f'{" ".join(rec["flags"]) or "ok"}', tiles))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    _write(s, 'strips', records, rows, [v for v, _, _ in STRIP_VIEWS],
           {'database': label, 'terrain': terrain_name, 'weather': wl, 'extent': extent,
            'failing': sum(bool(r['fail']) for r in records)})
    return records


def find_terrain(files: Files, spec: str | None, db_spec: str) -> tuple[str, bytes]:
    """(name, bytes) of the VisualTerrain to draw: --terrain (a file, or a resource name staged or in the game copy),
    else for a level:<name> database terrains/<name>/<name>.visual.res."""
    if spec and os.path.isfile(spec):
        return spec.replace('\\', '/'), open(spec, 'rb').read()
    name = spec
    if name is None:
        m = re.match(r'level:(.+)$', db_spec, re.I)
        if not m:
            raise RenderQaError('strips: give --terrain (the VisualTerrain to draw)')
        name = f'terrains/{m.group(1).lower()}/{m.group(1).lower()}.visual.res'
    if not name.lower().endswith('.res'):
        name += '.res'
    b = files.read(name)
    if b is None:
        raise RenderQaError(f'strips: no VisualTerrain {name}')
    return name, b


# ------------------------------------------------------------------------------------------------ terrain patches

# A terrain patch is a grid the engine makes itself, drawn with its PatchMaskShaders entry's program for one of BFBC2's
# two patch layouts: PATCH_LAYOUT - stream 0 the grid point (R16G16_SINT), streams 1 and 2 the patch's mask atlas
# coordinates (R16G16B16A16_SNORM; the second read by DICE's patches of five or more tiled layers) - and
# PATCH_LAYOUT_NO_TILES, the grid point alone, which DICE compiles beside it for the patches whose layers are all drawn
# by rule (no mask tile). The vertex shader reads the height map at each grid point and
# places it with posScale / posOffset / heightmapTexCoordScaleAndOffset (pcbuild.ENGINE_NAMED, named in its own block
# with the height map's engine texture record); the pixel shader reads the mask atlas (maskTileMap), the dynamic
# (destruction) mask and the height map by the engine texture records of the program's block.
PATCH_LAYOUT = bytes.fromhex('3d4fc700fbe098965fbf737fc00cbfac')
PATCH_LAYOUT_NO_TILES = bytes.fromhex('52dc29a19ca95d0c5fb972227e2f7bf7')
PATCH_LAYOUTS = (PATCH_LAYOUT, PATCH_LAYOUT_NO_TILES)
PATCH_FORMATS = {0: 38, 1: 13, 2: 13}  # semantic index -> DXGI format of BFBC2's patch streams (DICE's vertex shaders)
PATCH_OFFSETS = {0: 0, 1: 4, 2: 12}    # ... and its offset in the drawn grid's one interleaved 20-byte vertex
PATCH_STRIDE = 20
PATCH_CELLS = 16                       # the drawn patch: 16 x 16 cells of PATCH_CELL metres at PATCH_ORIGIN
PATCH_CELL = 2.0
PATCH_ORIGIN = (100.0, 50.0, 200.0)
# the engine textures as flat float textures: the height map at PATCH_HEIGHT (the grid's world height is
# PATCH_HEIGHT * posScale.y + posOffset.y), nothing destroyed, every mask tile half covered (each tiled layer drawn)
PATCH_HEIGHT = 0.5
PATCH_TEXTURES = {'heightmap': PATCH_HEIGHT, 'dynamicMaskTileMap': 0.0, 'dynamicMaskIndirectionMap': 0.0,
                  'maskTileMap': 0.5}
# a patch with one of these fails (exit 1): the strips' failures, an unlit (black) picture, an engine constant or
# texture the shader reads that its block does not name (the engine writes only what a record names: the shader would
# read 0 or nothing bound), a vertex input BFBC2's patch streams do not have
PATCH_FAIL = STRIP_FAIL | {'black', 'engine-unrecorded', 'layout'}
PATCH_SHADERS = '/patchmaskshaders/'


def patch_entries(db: S.ShaderDb) -> list[str]:
    """The terrain patch entries of a database (Terrains/<level>/<level>/PatchMaskShaders/<layers>), sorted."""
    return sorted(n for n, _ in db.entries if PATCH_SHADERS in n.lower())


def patch_values(height: float | None = None) -> dict[str, tuple[float, ...]]:
    """The terrain renderer's named constants for the drawn patch: the grid point (x, z) to the world through posScale /
    posOffset, to the height map through heightmapTexCoordScaleAndOffset (the patch is the whole map), the world back
    to the height map through heightmapScaleAndOffsetXZ; the dynamic mask as for the strips (strip_values). height:
    the patch's world height (default PATCH_ORIGIN's + PATCH_HEIGHT) - sea-floor layers shade by height (BF1943's Wake:
    dark below its water at 215 m, DICE's Isla Inocentes below about 65 m)."""
    x0, y0, z0 = _origin(height)
    size = PATCH_CELLS * PATCH_CELL
    v = strip_values((x0, z0, x0 + size, z0 + size))
    # one height scale for both shaders, as the engine's: the vertex shader's height map sample times posScale.y and
    # the pixel shader's times heightmapWorldScaleY (BF1943's sea floor shades by the latter: at 1 every sea-floor
    # patch drew black, its ground at 0.5 m), the patch's world height either way
    scale = (y0 + PATCH_HEIGHT) / PATCH_HEIGHT
    v.update({'posScale': (PATCH_CELL, scale, PATCH_CELL, 0.0), 'posOffset': (x0, 0.0, z0, 0.0),
              'heightmapWorldScaleY': (scale, 0.0, 0.0, 0.0),
              'heightmapTexCoordScaleAndOffset': (1.0 / PATCH_CELLS, 1.0 / PATCH_CELLS, 0.0, 0.0)})
    return v


def _origin(height: float | None) -> tuple[float, float, float]:
    x0, y0, z0 = PATCH_ORIGIN
    return (x0, y0 if height is None else height - PATCH_HEIGHT, z0)


def patch_bounds(height: float | None = None) -> tuple[np.ndarray, np.ndarray]:
    x0, y0, z0 = _origin(height)
    size = PATCH_CELLS * PATCH_CELL
    y = y0 + PATCH_HEIGHT
    return np.array([x0, y, z0]), np.array([x0 + size, y, z0 + size])


def patch_grid(n: int = PATCH_CELLS) -> tuple[bytes, bytes]:
    """(vertices, 32-bit indices) of an n x n patch, the three streams interleaved in one 20-byte vertex: the grid point
    (two int16) at 0, the mask atlas coordinates (u, v, u, v as four snorm16, 0..1 across the patch) at 4 and 12."""
    k = np.arange(n + 1)
    zz, xx = np.meshgrid(k, k, indexing='ij')
    v = np.zeros((n + 1, n + 1, PATCH_STRIDE // 2), '<i2')
    v[..., 0], v[..., 1] = xx, zz
    u, w = np.round(xx / n * 32767), np.round(zz / n * 32767)
    for at in (2, 6):
        v[..., at], v[..., at + 1], v[..., at + 2], v[..., at + 3] = u, w, u, w
    i = (zz[:-1, :-1] * (n + 1) + xx[:-1, :-1]).ravel()
    tri = np.stack([i, i + 1, i + n + 1, i + 1, i + n + 2, i + n + 1], 1).astype('<u4')
    return v.tobytes(), tri.tobytes()


def patch_cbuffer(code: bytes, values: dict[str, Any], records: Sequence[tuple[str, int, int, tuple[float, ...]]]) \
        -> bytes:
    """A patch shader's $Globals as the engine fills it: its engine constants by name, the block's material constants,
    and the named terrain constants (pcbuild.ENGINE_NAMED) only where a record of its block names them - the engine
    writes nothing else (a translated shader declaring posScale with no record naming it reads 0, as in the game)."""
    plain = {k: v for k, v in values.items() if k not in P.ENGINE_NAMED}
    return strip_cbuffer(code, dict(plain, **{n: values[n] for n, _, _, _ in records if n in values}), records)


def _unrecorded(code: bytes, names: Iterable[str]) -> list[str]:
    """The engine-named constants (pcbuild.ENGINE_NAMED) a compiled shader declares by name in $Globals that no record
    of its blocks names (DICE's read them inside 'constants', so never by name)."""
    have = set(names)
    return sorted(n for n, _, _, _ in P.rdef(code)[0].get('$Globals', []) if n in P.ENGINE_NAMED and n not in have)


def prepare_patch(files: Files, db: S.ShaderDb, entry: str, layout: bytes, own: str | None, envmap: str = ENVMAP) \
        -> tuple[Draw | None, list[str], list[str], dict[str, Any]]:
    """(the draw, flags, notes, extra) of one patch entry in one patch layout: its main-pass program (no point light),
    the vertex inputs on the drawn grid, the textures the program block binds, the engine textures and samplers of
    both shaders; extra: 'vs_named' / 'ps_named' records, 'engine' / 'vengine' (slot -> engine texture), 'vsamplers'."""
    flags: list[str] = []
    notes: list[str] = []
    extra: dict[str, Any] = {'engine': {}, 'vengine': {}, 'vs_named': [], 'ps_named': [], 'textures': [],
                             'vsamplers': []}
    g = {n.lower(): gg for n, gg in db.entries}.get(entry.lower())
    if g is None:
        _flag(flags, notes, 'no-entry', f'{entry}: not in the database')
        return None, flags, notes, extra
    progs = [k for k in db.entry_programs().get(g, []) if db.keys[k][0x30] == 0 and db.keys[k][0x34] == 0
             and bytes(db.keys[k][0x20:0x30]) == layout]
    progs = [k for k in progs if db.keys[k][0x38] == 0] or progs
    if not progs:
        _flag(flags, notes, 'no-program', f'no main-pass program for the patch layout {layout.hex()[:8]}')
        return None, flags, notes, extra
    k = progs[0]
    v, p, _, b2 = db.program_refs(k)
    vs_s, ps_s = db.vs[v], db.ps[p]
    vblk, blk = db.params[vs_s.param], db.params[b2]
    extra['vs_named'] = S.block_constants(vblk)
    extra['ps_named'] = S.block_constants(blk) + S.block_constants(db.params[ps_s.param])
    for code, recs, stage in ((vs_s.code, extra['vs_named'], 'vs'), (ps_s.code, extra['ps_named'], 'ps')):
        miss = _unrecorded(code, [r[0] for r in recs])
        if miss:
            _flag(flags, notes, 'engine-unrecorded', f'{stage} reads {", ".join(miss)}: no record of its block names them')
    elements = []
    for si, fmt, _slot, _off, inst in vs_elements(vs_s.tail):
        if si not in PATCH_FORMATS or fmt != PATCH_FORMATS[si] or inst:
            _flag(flags, notes, 'layout', f'vertex input TEXCOORD{si} (DXGI {fmt}{" per instance" if inst else ""}): '
                                          'not one of the patch streams')
            continue
        elements.append((si, fmt, 0, PATCH_OFFSETS[si], 0))
    nidx = PATCH_CELLS * PATCH_CELLS * 6
    d = Draw(entry, entry, k, origin(vs_s, ps_s), vs_s.code, ps_s.code, elements, 0, PATCH_STRIDE, nidx, 0, 0, None,
             extra['ps_named'], [], sampler_states(db.params[ps_s.param]))
    extra['vsamplers'] = sampler_states(vblk)
    vengine = {s_: n for n, s_ in S.block_engine_textures(vblk)}
    for name, typ, bp, _dim in resources(vs_s.code):
        if typ != 2:
            continue
        if bp in vengine:
            extra['vengine'][bp] = vengine[bp]
        elif name.startswith('texture_') and name[len('texture_'):] in P.ENGINE_TEXTURES:
            _flag(flags, notes, 'engine-unrecorded', f'vs t{bp} ({name}): no engine texture record of its block')
        else:
            _flag(flags, notes, 'bound-empty', f'vs t{bp} ({name}) has no texture')
    records = {s_: (c, n) for s_, c, n in P.block_texture_list(blk)}
    engine = {s_: n for n, s_ in S.block_engine_textures(blk)}
    for name, typ, bp, dim in resources(ps_s.code):
        if typ != 2:
            continue
        if bp in records:
            tn = records[bp][1]
        elif name == 'texture_outdoorLightReflectionEnvmap':
            tn = envmap
        elif bp in engine:
            extra['engine'][bp] = engine[bp]
            continue
        elif name.startswith('texture_') and name[len('texture_'):] in P.ENGINE_TEXTURES:
            _flag(flags, notes, 'engine-unrecorded', f'ps t{bp} ({name}): no engine texture record of its block')
            continue
        else:
            if name.startswith('texture_Texture'):
                _flag(flags, notes, 'bound-empty', f't{bp} ({name}) has no texture')
            continue
        extra['textures'].append(tn)
        if foreign_texture(tn, own):
            _flag(flags, notes, 'foreign-texture', f't{bp} {tn}: another level\'s texture')
        b = files.read(tn + '.res')
        why = texture_problem(b) if b is not None else 'not found'
        if why or b is None:
            if tn != envmap:
                _flag(flags, notes, 'missing-texture' if b is None else 'bad-texture', f't{bp} {tn}: {why}')
            continue
        cube = struct.unpack_from('<I', b, 4)[0] == 1
        if cube != (dim == 9):
            _flag(flags, notes, 'dimension', f't{bp} {name}: {tn} {"is" if cube else "is not"} a cube')
            continue
        d.textures.append((bp, tn))
    return d, flags, notes, extra


def underwater_patch(entry: str, textures: Any = None) -> bool:
    """A patch made for under the water: a sea-floor layer in its combination (BF1943's '..._SeaFloor' patches), or
    the texture-less deep-ocean floor (Wake_Grey). BF1943's patch shader gates these layers by height."""
    leaf = entry.rsplit('/', 1)[-1].lower()
    return 'seafloor' in leaf or (leaf.endswith('_grey') and not textures)


def run_patches(files: Files, db: S.ShaderDb, s: Settings, label: str, only: Sequence[str] = (),
                limit: int = 0, height: float | None = None) -> list[dict[str, Any]]:
    """Every terrain patch entry of a database drawn on a 16 x 16-cell patch (patch_grid, patch_values) in each patch
    layout it has a program for, from above and at three-quarters; each covered pixel's depth taken back to the world
    and held against the patch's own box (a vertex shader reading posScale / posOffset / the height map where its block
    does not name them draws the grid at the origin: 'out-of-bounds' or 'nothing-drawn'). No program for PATCH_LAYOUT,
    an unlit picture, another level's texture, an engine input no record names are failures too. height: the patch's
    world height (patch_values)."""
    values, post, wl = _light(files, s)
    values = dict(values, **patch_values(height))
    entries = [e for e in patch_entries(db) if not only or any(o.lower() in e.lower() for o in only)]
    if limit:
        entries = entries[:limit]
    lo, hi = patch_bounds(height)
    progs = db.entry_programs()
    ents = {n: g for n, g in db.entries}
    os.makedirs(s.out, exist_ok=True)
    tmp = tempfile.mkdtemp(prefix='renderqa_patches_', dir=s.out)
    records, rows = [], []
    try:
        engine_files = {n: _flat_texture(os.path.join(tmp, f'e_{n}.bin'), v) for n, v in PATCH_TEXTURES.items()}
        vbf, ibf = os.path.join(tmp, 'patch.vb'), os.path.join(tmp, 'patch.ib')
        vb, ib = patch_grid()
        open(vbf, 'wb').write(vb)
        open(ibf, 'wb').write(ib)
        todo = []
        for e in entries:
            have = {bytes(db.keys[k][0x20:0x30]) for k in progs.get(ents[e], [])}
            own = terrain_folder(e)
            for lg in PATCH_LAYOUTS:
                if lg == PATCH_LAYOUT or lg in have:
                    todo.append((e, lg, own))
        texfiles: dict[str, str] = {}
        lines: list[str] = []
        jobs = []
        for n, (e, lg, own) in enumerate(todo):
            tag = 'tiles' if lg == PATCH_LAYOUT else 'no tiles'
            rec: dict[str, Any] = {'mesh': f'{e} [{tag}]', 'entry': e, 'layout': lg.hex(), 'flags': [], 'notes': [],
                                   'bounds': [list(map(float, lo)), list(map(float, hi))]}
            records.append(rec)
            d, fl, nt, extra = prepare_patch(files, db, e, lg, own, s.envmap)
            rec['flags'] += fl
            rec['notes'] += nt
            rec['textures'] = extra['textures']
            if d is None or 'layout' in rec['flags']:          # (an input the grid lacks: no input layout to draw)
                continue
            rec['origin'] = d.origin
            rec['program'] = d.program
            for _, tn in d.textures:
                if tn not in texfiles:
                    texfiles[tn] = os.path.join(tmp, f't{len(texfiles)}.res')
                    open(texfiles[tn], 'wb').write(files.read(tn + '.res') or b'')
            vf, pf = os.path.join(tmp, f'p{n}v.dxbc'), os.path.join(tmp, f'p{n}p.dxbc')
            open(vf, 'wb').write(d.vs)
            open(pf, 'wb').write(d.ps)
            for vi, (view, az, el) in enumerate(STRIP_VIEWS):
                eye, vmat, vp = camera(lo, hi, az, el)
                vals = dict(values, viewProjMatrix=vp, worldViewProjMatrix=vp, worldMatrix=np.eye(4), viewMatrix=vmat,
                            cameraPos=(*eye, 0.0), invScreenSize=(1 / s.size, 1 / s.size, 0.0, 0.0))
                out = os.path.join(tmp, f's{n}_{vi}.raw')
                cv, cp = os.path.join(tmp, f'cv{n}_{vi}.bin'), os.path.join(tmp, f'cp{n}_{vi}.bin')
                open(cv, 'wb').write(patch_cbuffer(d.vs, vals, extra['vs_named']))
                open(cp, 'wb').write(patch_cbuffer(d.ps, vals, extra['ps_named']))
                lines += [f'frame {job_path(out)} {s.size} {s.size} 0 0 0 0', 'depth 1', f'count {1 if vi == 0 else 0}',
                          f'vs {job_path(vf)}', f'ps {job_path(pf)}']
                lines += [f'el {si} {fmt} {slot} {off} {inst}' for si, fmt, slot, off, inst in d.elements]
                lines += [f'vb {job_path(vbf)} 0 {PATCH_STRIDE}', f'ib {job_path(ibf)} 0 32 {d.indices} 0',
                          f'cb vs {job_path(cv)}', f'cb ps {job_path(cp)}']
                lines += [f'samp {reg} {desc.hex()}' for reg, desc in d.samplers if reg < 16]
                lines += [f'vsamp {reg} {desc.hex()}' for reg, desc in extra['vsamplers'] if reg < 16]
                lines += [f'texres {slot} {job_path(texfiles[tn])}' for slot, tn in d.textures]
                lines += [f'tex {slot} 2d {job_path(engine_files.get(nm, engine_files["heightmap"]))}'
                          for slot, nm in extra['engine'].items()]
                lines += [f'vtex {slot} 2d {job_path(engine_files.get(nm, engine_files["heightmap"]))}'
                          for slot, nm in extra['vengine'].items()]
                lines += ['draw', 'end']
                jobs.append((n, view, out, vp))
        output: list[str] = []
        if lines:
            job = os.path.join(tmp, 'job.txt')
            open(job, 'w').write('\n'.join(lines) + '\n')
            try:
                output, _ = run_job(job, s.warp)
            except RenderQaError as e:
                for rec in records:
                    if 'origin' in rec:
                        _flag(rec['flags'], rec['notes'], 'render-failed', str(e))
        counts = [tuple(int(x) for x in ln.split()[2:]) for ln in output if ln.startswith('count ')]
        drawn = [n for n in range(len(todo)) if 'origin' in records[n]]
        for n, c in zip(drawn, counts):
            records[n]['coverage'] = {'shaded': c[0], 'covered': c[1]}
            if c[1] and not c[0]:
                _flag(records[n]['flags'], records[n]['notes'], 'alpha-discards-all', 'every pixel discarded')
        views: dict[int, dict[str, np.ndarray]] = {}
        tol = 0.5 + 0.02 * float(np.max(hi - lo))
        for n, view, out, vp in jobs:
            rec = records[n]
            if not os.path.isfile(out):
                continue
            raw = np.fromfile(out, '<f4').reshape(s.size, s.size, 4)
            depth = np.fromfile(out + '.depth', '<f4').reshape(s.size, s.size)
            mask = depth < 1.0
            hdr = raw[..., :3]
            if (~np.isfinite(hdr[mask])).any():
                _flag(rec['flags'], rec['notes'], 'nan', f'{view}: NaN / inf output')
            ldr, _ = tonemap(np.nan_to_num(hdr), mask, post)
            views.setdefault(n, {})[view] = np.where(mask[..., None], ldr, np.array(BACKGROUND, np.float32))
            if not mask.any():
                continue
            if view == 'top':
                rec['mean_rgb'] = [float(x) for x in np.nan_to_num(hdr[mask]).mean(0)]
            if float(np.nan_to_num(hdr[mask]).max()) <= 1e-4:
                if underwater_patch(rec['entry'], rec.get('textures')):
                    # a sea-floor / deep-ocean patch: BF1943's own shader gates its layers by height and draws black
                    # above the water line (the harness's patch height) - expected, not a failure (user, 2026-11-12)
                    _flag(rec['flags'], rec['notes'], 'black-underwater',
                          f'{view}: black above the water, as BF1943\'s height gate draws a sea-floor patch')
                else:
                    _flag(rec['flags'], rec['notes'], 'black', f'{view}: every covered pixel black')
            yy, xx = np.nonzero(mask)
            ndc = np.stack([(xx + 0.5) / s.size * 2 - 1, 1 - (yy + 0.5) / s.size * 2, depth[mask],
                            np.ones(len(xx))], 1)
            world = ndc @ np.linalg.inv(vp).T
            world = world[:, :3] / world[:, 3:4]
            excess = np.maximum(np.maximum(lo - world, world - hi), 0.0).max(0)
            rec.setdefault('excess', {})[view] = [float(x) for x in excess]
            rec.setdefault('pixels', {})[view] = int(mask.sum())
            if excess.max() > tol:
                _flag(rec['flags'], rec['notes'], 'out-of-bounds',
                      f'{view}: drawn up to {excess.max():.1f} m outside the patch (y {excess[1]:.1f} m; '
                      f'tolerance {tol:.1f} m)')
        for n, rec in enumerate(records):
            if 'origin' in rec and not rec.get('pixels') and 'render-failed' not in rec['flags']:
                _flag(rec['flags'], rec['notes'], 'nothing-drawn', 'no pixel covered in any view')
            rec['severity'] = severity(rec['flags'])
            rec['fail'] = sorted(set(rec['flags']) & PATCH_FAIL)
            tiles = [views.get(n, {}).get(v, _blank(s.size)) for v, _, _ in STRIP_VIEWS]
            rows.append((rec['severity'], f'{rec["mesh"].rsplit("/", 1)[-1]} [{rec.get("origin", "")}]\n'
                         f'{" ".join(rec["flags"]) or "ok"}', tiles))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    _write(s, 'patches', records, rows, [v for v, _, _ in STRIP_VIEWS],
           {'database': label, 'weather': wl, 'patch': {'cells': PATCH_CELLS, 'cell': PATCH_CELL,
                                                        'origin': list(_origin(height)), 'height': float(lo[1])},
            'failing': sum(bool(r['fail']) for r in records)})
    return records


# ------------------------------------------------------------------------------------------------ Project OPFOR: Wake

WAKE_DB =os.path.join('work', 'ww2', 'install', 'level', 'Levels', 'WAKE_ISLAND_S', 'Shaders', 'Dx11_Single.res')


def wake_meshes(root: str, files: Files, log: Callable[[str], None] = print) -> list[str]:
    """The meshes Project OPFOR's wake_audit.drawn_meshes() lists (its module imported from the project's
    src\\tools\\ww2; read only - the files it reads are given this run's index, so it writes nothing)."""
    tools = os.path.join(root, 'src', 'tools', 'ww2')
    if not os.path.isfile(os.path.join(tools, 'wake_audit.py')):
        raise RenderQaError(f'{tools}: no wake_audit.py')
    ix = files.ix()                                     # this suite's modules loaded first: they stay the ones used
    import meshview  # noqa: F401
    import texture  # noqa: F401
    saved = list(sys.path)
    sys.path.append(tools)                              # (its imports put another suite checkout first on sys.path)
    try:
        import wake_audit
        import wake_preview
    finally:
        sys.path[:] = saved
    wf = wake_preview.Files()
    wf._ix = ix
    names = sorted(wake_audit.drawn_meshes(wf, log=log))
    # back to the names' own case where a file has it (entries are looked up case-blind)
    return names


def find_meshes(files: Files, texts: Sequence[str], limit: int = 40) -> list[str]:
    """Mesh names containing any of the texts (staged mesh sets, then the game copy's)."""
    kinds = set(meshset.KINDS)
    out = [k[:-4] for k, t in files.types.items() if t in kinds and any(x.lower() in k for x in texts)]
    if files.root:
        out += [k[:-len('_lod0_data.res')] for k in files.ix().where if k.endswith('_lod0_data.res')
                and any(x.lower() in k for x in texts)]
    return sorted(dict.fromkeys(out))[:limit]


def _priority() -> None:
    if os.name == 'nt':
        try:
            k = ctypes.windll.kernel32
            k.SetPriorityClass(k.GetCurrentProcess(), BELOW_NORMAL)
        except Exception:                                   # noqa: BLE001 - only a courtesy
            pass


def main(argv: Sequence[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description=(__doc__ or '').split('\n')[0])
    ap.add_argument('command', choices=('render', 'compare', 'validate', 'wake', 'strips', 'patches'))
    ap.add_argument('meshes', nargs='*', help='mesh names (or with --find, parts of names)')
    ap.add_argument('--db', help='the database to draw with (render, compare)')
    ap.add_argument('--truth', help="compare: DICE's database (default auto: a BFBC2 level that draws the mesh)")
    ap.add_argument('--stage', action='append', default=[], help='unpacked bundles read before the game copy')
    ap.add_argument('--list', help='a file of mesh names, one per line')
    ap.add_argument('--find', action='store_true', help='the mesh arguments are parts of names')
    ap.add_argument('--only', action='append', default=[],
                    help='wake: only meshes whose name contains this; strips: only strips whose shader does; patches: '
                         'only patch entries whose name does (repeatable)')
    ap.add_argument('--terrain', help='strips: the VisualTerrain (a file or a resource name; default for level:<name>'
                                      ' terrains/<name>/<name>.visual.res)')
    ap.add_argument('--height', type=float, help='patches: the drawn patch\'s world height in metres (default '
                                                  f'{PATCH_ORIGIN[1] + PATCH_HEIGHT}; sea-floor layers shade by height)')
    ap.add_argument('--limit', type=int, default=0, help='at most this many meshes')
    ap.add_argument('--size', type=int, default=256)
    ap.add_argument('--weather', default=WEATHER, help=f'weather folder of the light (default {WEATHER})')
    ap.add_argument('--sun', choices=('noon', 'weather'), default='noon',
                    help="key light direction: a fixed high sun (default) or the weather state's theta / phi")
    ap.add_argument('--envmap', default=ENVMAP, help='the cube the engine binds as outdoorLightReflectionEnvmap')
    ap.add_argument('--warp', action='store_true', help='the software rasterizer instead of the GPU')
    ap.add_argument('--opfor', help='Project OPFOR BFBC2 folder (the game copy)')
    ap.add_argument('-o', '--out', help='output folder')
    a = ap.parse_args(argv)
    _priority()
    try:
        root: str | None = S.find_opfor(a.opfor)
    except S.ShaderDbError:
        root = None
    if root is None and a.command in ('validate', 'wake'):
        raise RenderQaError('Project OPFOR BFBC2 (the game copy) not found: give --opfor')
    files = Files(root, a.stage)
    out = a.out or os.path.join(tempfile.gettempdir(), 'renderqa', a.command)
    os.makedirs(out, exist_ok=True)
    s = Settings(out, a.size, a.warp, a.envmap, a.weather, sun=a.sun)
    meshes = list(a.meshes)
    if a.list:
        meshes += [x.strip() for x in open(a.list, encoding='utf-8') if x.strip() and not x.startswith('#')]
    if a.find:
        meshes = find_meshes(files, meshes)
    dbs = Databases(root)
    if a.command == 'wake':
        assert root is not None
        files.add_stage(os.path.join(root, 'work', 'ww2', 'install'))
        meshes = meshes or wake_meshes(root, files)
        if a.only:
            meshes = [m for m in meshes if any(o.lower() in m.lower() for o in a.only)]
        if a.limit:
            meshes = meshes[:a.limit]
        db = dbs.get(os.path.join(root, WAKE_DB))
        run_compare(files, db, meshes, s, os.path.join(root, WAKE_DB), None, dbs, truth_index(root))
        return 0
    if a.command == 'patches':
        if not a.db:
            raise RenderQaError('patches: --db is needed')
        records = run_patches(files, dbs.get(a.db), s, a.db, a.only, a.limit, a.height)
        bad = [r for r in records if r['fail']]
        print(f'patches: {len(records)} drawn, {len(bad)} failing'
              + ''.join(f'\n  {r["mesh"]}: {" ".join(r["fail"])}' for r in bad[:30]))
        return 1 if bad else 0
    if a.command == 'strips':
        if not a.db:
            raise RenderQaError('strips: --db is needed')
        name, pc = find_terrain(files, a.terrain, a.db)
        records = run_strips(files, dbs.get(a.db), name, pc, s, a.db, a.only, a.limit)
        bad = [r for r in records if r['fail']]
        print(f'strips: {len(records)} drawn, {len(bad)} failing'
              + ''.join(f'\n  {r["mesh"]}: {" ".join(r["fail"])}' for r in bad[:30]))
        return 1 if bad else 0
    if a.limit:
        meshes = meshes[:a.limit]
    if not meshes:
        raise RenderQaError('no mesh named (give names, --list or --find)')
    if a.command == 'validate':
        assert root is not None
        run_validate(files, meshes, s, dbs, truth_index(root))
        return 0
    if not a.db:
        raise RenderQaError(f'{a.command}: --db is needed')
    db = dbs.get(a.db)
    if a.command == 'render':
        run_render(files, db, meshes, s, a.db)
    else:
        tindex = truth_index(root) if a.truth in (None, 'auto') and root else None
        run_compare(files, db, meshes, s, a.db, None if a.truth in (None, 'auto') else a.truth, dbs, tindex)
    return 0


if __name__ == '__main__':
    sys.stdout.reconfigure(errors='replace', line_buffering=True)  # type: ignore[union-attr]
    from bfbc2core import errors
    errors.setup('renderqa', gui=False)
    with errors.guard():
        sys.exit(main())
