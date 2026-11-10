r"""renderqa.py, the offline render harness: on files built here (quick), and with --game on Project OPFOR BFBC2's
game copy.

Built (no game data): a Dx11 database with one quad mesh's entries (shaders compiled here with d3dcompiler_47, a
parameter block with a texture record, a material constant and a sampler state), its mesh set, LOD data and a
32-bit texture in a staged folder with an fbrb.manifest:
- prepare: the entry, program, layout, texture slot and sampler found; a missing texture, a texture slot with no
  record ('bound empty'), a part whose layout has no program ('layout fallback'), a mesh nothing draws, all flagged;
- shaderview: the quad drawn front on with the texture's colour times the material constant (no sRGB: the header
  flag is off), its coverage; a pixel shader that discards everything flagged from the draw's sample counts;
- the checks on made-up pictures (black, magenta, flat, blown out, NaN), the comparison (equal / shifted pictures),
  texture header checks, sampler-state records, the light derived from a weather state, the camera, the material
  constants placed only in a shader's own constants array, destructible composites' part visibility;
- the command line: a missing database is an error with exit code 1.
With --game: stock meshes drawn with their own level's database must draw and pass the checks, and the same mesh
compared with itself must be identical.

  python test_renderqa.py [--game]
"""
import hashlib
import math
import os
import shutil
import struct
import subprocess
import sys
import tempfile
from collections.abc import Sequence
from typing import Any

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
if _ROOT not in sys.path:
    sys.path.insert(0, _ROOT)
import suite_paths  # noqa: E402

suite_paths.use('shader-tools', 'console-convert', 'core')
import mesh as meshdata  # noqa: E402
import meshset  # noqa: E402
import pcbuild as P  # noqa: E402
import renderqa as R  # noqa: E402
import shaderdb as S  # noqa: E402

MESH = 'Test/Quad_Mesh'
TEX = 'Test/Quad_C'
COLOUR = (200, 100, 50)          # the texture's texels, R G B
TINT = (1.0, 0.5, 0.5, 1.0)      # the material constant the pixel shader multiplies by

VS = """
float4x4 viewProjMatrix;
struct VI { float3 pos : TEXCOORD0; float2 uv : TEXCOORD1; };
struct VO { float4 pos : SV_Position; float2 uv : TEXCOORD0; };
VO main(VI i) { VO o; o.pos = mul(float4(i.pos, 1), viewProjMatrix); o.uv = i.uv; return o; }
"""
PS = """
float4 outdoorLightKeyColor;
float4 constants[1];
SamplerState sampler1 : register(s1);
Texture2D texture_Texture : register(t1);
Texture2D texture_Texture2 : register(t2);
float4 main(float4 pos : SV_Position, float2 uv : TEXCOORD0) : SV_Target {
    float4 c = texture_Texture.Sample(sampler1, uv) * constants[0];
#ifdef SECOND
    c += texture_Texture2.Sample(sampler1, uv) * 0.001;
#endif
#ifdef DISCARD
    clip(-1);
#endif
    return c;
}
"""


class Checks:
    def __init__(self) -> None:
        self.failed = 0

    def check(self, ok: object, what: str, detail: str = '') -> None:
        self.failed += not ok
        print(f'{"ok  " if ok else "FAIL"} {what}' + (f'  ({detail})' if detail and not ok else ''), flush=True)


# ------------------------------------------------------------------------------------------------ built files

def block(textures: Sequence[tuple[int, str]] = (), constants: Sequence[tuple[str, int, tuple[float, ...]]] = (),
          samplers: Sequence[tuple[int, bytes]] = (), nregs: int = 0) -> bytes:
    """A parameter block (shaderdb FORMAT.md section 1)."""
    tex = b''.join(bytes((s, 0, 0, 0)) + n.encode().ljust(0x80, b'\0') + bytes(8) for s, n in textures)
    con = b''.join(n.encode().ljust(0x20, b'\0') + struct.pack('<IBBH4f', 0, r, 4, 0, *v) for n, r, v in constants)
    smp = b''.join(struct.pack('<I', r) + d + bytes(4) for r, d in samplers)
    te = 0x20 + len(tex)
    ce = te + len(con)
    total = ce + len(smp)
    total += (-total) % 16
    hdr = struct.pack('<6I', total, 0x20, 0x20, te, ce, ce)
    hdr += bytes((nregs, nregs, 0, len(textures), len(constants), 0, len(samplers), 0))
    out = hdr + tex + con + smp
    return out + bytes(total - len(out))


def sampler_desc(address: int = 1) -> bytes:
    """A D3D11_SAMPLER_DESC: trilinear, the address mode given (1 wrap), LOD 0 .. max."""
    return struct.pack('<4IfII4f2f', 0x15, address, address, address, 0.0, 1, 1, 0, 0, 0, 0, 0.0, 3.4e38)


def layout_body() -> bytes:
    """A vertex layout record body (0x49 bytes): float3 position, float2 texture coordinate; stride 20."""
    b = bytearray(0x49)
    b[0:4] = bytes((3, 1, 0, 0))
    b[4:8] = bytes((2, 0, 12, 0))
    b[64] = 2
    b[69] = 20
    return bytes(b)


def build_db(variants: dict[str, dict[str, Any]], layout: bytes) -> S.ShaderDb:
    """A Dx11 database: an entry per variant ('<MESH>/Test/Shaders/<variant>'), each with one program whose pixel
    shader is compiled with the variant's defines; 'layout' in a variant: the program's key names another layout."""
    db = S.ShaderDb(11)
    db.version, db.flag = 120, 0
    els = [(0, 0, 6, 0, False), (0, 12, 16, 1, False)]          # float3 TEXCOORD0, float2 TEXCOORD1
    vs = P.compile_hlsl(VS, 'main', 'vs_5_0')
    db.params = [block(), block(samplers=[(1, sampler_desc())], nregs=2), block()]
    db.bindings = [struct.pack('<HH', 0, 0)]
    db.regs = [struct.pack('<I', 0)]
    db.vs = [S.Shader(hashlib.md5(b'vs').digest(), vs, 0, 0, 0, P.dx11_layout(vs, els))]
    other = hashlib.md5(b'other layout').digest()
    db.layouts = [layout + layout_body(), other + layout_body()]
    db.parts = [(f'{MESH}_lod0/part0/Standard', layout)]
    for i, (name, v) in enumerate(sorted(variants.items())):
        ps = P.compile_hlsl(PS, 'main', 'ps_5_0', v.get('defines'))
        db.ps.append(S.Shader(hashlib.md5(name.encode()).digest(), ps, 1, 0, 0))
        db.params.append(block([(1, TEX)], [('Tint', 1, TINT)]))
        entry = f'{MESH}/Test/Shaders/{name}'
        g = db.name_guid(entry)
        db.entries.append((entry, g))
        prog = hashlib.md5(b'program' + name.encode()).digest()
        db.programs.append(prog + bytes(16) + struct.pack('<4I', 0, i, 2, len(db.params) - 1))
        db.keys.append(g + bytes(16) + (other if v.get('layout') else layout) + bytes(0x28))
        db.groups.append((g, [i]))
    order = sorted(range(len(db.programs)), key=lambda k: db.programs[k])
    db.programs = [db.programs[k] for k in order]
    db.keys = [db.keys[k] for k in order]
    db.groups = sorted([(g, [order.index(ix[0])]) for g, ix in db.groups])
    return S.ShaderDb.parse(db.to_bytes(), 10)


def mesh_files(shader: str) -> dict[str, tuple[str, bytes]]:
    """{name: (type, bytes)} of the quad mesh (one part drawn with shader) and its texture."""
    verts = b''.join(struct.pack('<5f', x, y, 0.0, u, v) for x, y, u, v in
                     ((-1, -1, 0, 1), (1, -1, 1, 1), (1, 1, 1, 0), (-1, 1, 0, 0)))
    idx = struct.pack('<6H', 0, 1, 2, 0, 2, 3)
    md = {'h0': 0, 'h1': b'\0\0', 'lists': [[], [], [], []],
          'parts': [{'name': b'part0', 'triangles': 2, 'vertices': 4, 'first_index': 0, 'vertex_offset': 0,
                     'stride': 20, 'k': 0, 'fa': 0, 'fb': 0, 'bones': []}]}
    data = meshdata.write(md, verts, idx)
    ms = meshset.write({'a': 0, 'z': 0, 'lod_settings': b'', 'lods': [
        {'x': 0, 'shaders': 1, 'sets': [(b'Default', [shader.encode()])], 'bounds': (-1, -1, 0, 1, 1, 0),
         'size': len(verts) + len(idx) + 12, 'extra': 0, 'flag': 0}]}, 'RigidMeshSet')
    texels = bytes((COLOUR[2], COLOUR[1], COLOUR[0], 255)) * 16
    tex = struct.pack('<8I', 0, 0, 9, 0, 4, 4, 1, 1) + struct.pack('<15I', len(texels), *[0] * 14) + texels
    return {MESH + '.res': ('RigidMeshSet', ms), MESH + '_lod0_data.res': ('MeshData', data), TEX + '.res': ('ITexture', tex)}


def stage(folder: str, files: dict[str, tuple[str, bytes]]) -> str:
    os.makedirs(folder, exist_ok=True)
    lines = ['name|type']
    for name, (typ, b) in files.items():
        p = os.path.join(folder, *name.split('/'))
        os.makedirs(os.path.dirname(p), exist_ok=True)
        open(p, 'wb').write(b)
        lines.append(f'{name}|{typ}')
    open(os.path.join(folder, 'fbrb.manifest'), 'w', encoding='utf-8').write('\n'.join(lines) + '\n')
    return folder


def fake(img: np.ndarray, counts: Sequence[tuple[str, int, int]] = ()) -> R.Rendered:
    m = np.zeros(img.shape[:2], bool)
    m[8:56, 8:56] = True
    views = {v: img for v, _, _ in R.VIEWS}
    return R.Rendered('x', views, {v: m for v, _, _ in R.VIEWS}, {v: img for v, _, _ in R.VIEWS}, list(counts), 1.0,
                      False)


# ------------------------------------------------------------------------------------------------ tests

def built(c: Checks) -> None:
    tmp = tempfile.mkdtemp(prefix='renderqa_test_')
    try:
        _built(c, tmp)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def _built(c: Checks, tmp: str) -> None:
    layout = hashlib.md5(b'layout').digest()
    db = build_db({'Plain': {}, 'Second': {'defines': {'SECOND': 1}}, 'Discard': {'defines': {'DISCARD': 1}},
                   'Fallback': {'layout': True}}, layout)
    c.check(not db.check(), 'built database passes shaderdb\'s checks', '; '.join(db.check()))

    def files_for(shader: str, with_texture: bool = True) -> R.Files:
        fs = mesh_files(shader)
        if not with_texture:
            del fs[TEX + '.res']
        return R.Files(None, [stage(os.path.join(tmp, f'{shader.replace("/", "_")}_{with_texture}'), fs)])

    plain = files_for('Test/Shaders/Plain')
    prep = R.prepare(plain, db, MESH)
    c.check(len(prep.draws) == 1 and not prep.flags, 'prepare: one draw, nothing flagged', str(prep.notes))
    d = prep.draws[0]
    c.check(d.textures == [(1, TEX)], 'prepare: the texture record bound to t1', str(d.textures))
    c.check(d.samplers == [(1, sampler_desc())], 'prepare: the pixel shader\'s sampler state', str(d.samplers))
    c.check(d.material == [('Tint', 1, 4, TINT)] and d.origin == 'translated', 'prepare: material constant; shaders '
            'compiled here count as translated', f'{d.material} {d.origin}')
    lo, hi = prep.lo, prep.hi
    c.check(lo is not None and hi is not None and np.allclose(lo, (-1, -1, 0)) and np.allclose(hi, (1, 1, 0)),
            'prepare: bounds from the positions', f'{lo} {hi}')
    p2 = R.prepare(files_for('Test/Shaders/Plain', False), db, MESH)
    c.check('missing-texture' in p2.flags, 'a texture not in the files: missing-texture', str(p2.notes))
    p3 = R.prepare(files_for('Test/Shaders/Second'), db, MESH)
    c.check('bound-empty' in p3.flags and 't2' in ' '.join(p3.notes), 'a texture slot without a record: bound-empty',
            str(p3.notes))
    p4 = R.prepare(files_for('Test/Shaders/Fallback'), db, MESH)
    c.check('layout-fallback' in p4.flags, 'a program for another layout only: layout-fallback', str(p4.notes))
    p5 = R.prepare(files_for('Test/Shaders/Nothing'), db, MESH)
    c.check('no-entry' in p5.flags, 'a shader the database has no entry for: no-entry', str(p5.notes))
    p6 = R.prepare(R.Files(None), db, MESH)
    c.check('no-mesh' in p6.flags, 'a mesh that is not there: no-mesh', str(p6.notes))

    try:
        R.shaderview()
    except R.RenderQaError as e:
        print(f'skip: drawing (shaderview.exe: {e})')
        return
    values, post = R.light(R.MP005_SUNNY)
    r = R.render(prep, values, post, 64, work=tmp)
    c.check(r is not None, 'render: drawn')
    if r is None:
        return
    front = r.masks['front']
    c.check(front.mean() > 0.2, 'render: the quad covers the front view', f'{front.mean():.3f}')
    want = np.array(COLOUR, np.float32) / 255 * np.array(TINT[:3], np.float32)
    got = r.hdr['front'][32, 32]
    c.check(np.allclose(got, want, atol=0.01), 'render: texel colour times the material constant (no sRGB view)',
            f'{got} vs {want}')
    c.check(r.masks['top'].mean() < 0.05, 'render: seen edge-on from above, the quad nearly vanishes',
            f'{r.masks["top"].mean():.3f}')
    c.check(len(r.counts) == 1 and r.counts[0][1] == r.counts[0][2] > 0, 'render: the draw\'s samples all shaded',
            str(r.counts))
    flags, _, _ = R.image_flags(r)
    c.check('alpha-discards-all' not in flags and 'nothing-drawn' not in flags, 'image checks: a plain draw passes',
            str(flags))
    pd = R.prepare(files_for('Test/Shaders/Discard'), db, MESH)
    rd = R.render(pd, values, post, 64, work=tmp)
    fd = R.image_flags(rd)[0] if rd else []
    c.check('alpha-discards-all' in fd and 'nothing-drawn' in fd, 'a pixel shader discarding everything: '
            'alpha-discards-all', f'{fd} {rd.counts if rd else None}')
    cmp = R.compare_images(r, r)
    c.check(cmp['iou'] == 1 and cmp['mae'] == 0 and cmp['psnr'] == 99, 'compare: a picture with itself is equal',
            str(cmp))
    out = os.path.join(tmp, 'run')
    s = R.Settings(out, 64, log=lambda x: None)
    recs = R.run_render(plain, db, [MESH], s, 'built')
    c.check(len(recs) == 1 and os.path.isfile(os.path.join(out, 'sheet_01.png'))
            and os.path.isfile(os.path.join(out, 'results.json')), 'render run: results, summary and sheet written')


def checks(c: Checks) -> None:
    grey = np.full((64, 64, 3), 0.5, np.float32)
    f = R.image_flags(fake(grey))[0]
    c.check('flat' in f and 'black' not in f, 'one constant colour: flat', str(f))
    f = R.image_flags(fake(np.zeros((64, 64, 3), np.float32)))[0]
    c.check('black' in f, 'black: black', str(f))
    mag = np.zeros((64, 64, 3), np.float32)
    mag[..., 0] = mag[..., 2] = 1
    c.check('magenta' in R.image_flags(fake(mag))[0], 'magenta: magenta')
    c.check('blown-out' in R.image_flags(fake(np.ones((64, 64, 3), np.float32)))[0], 'white: blown-out')
    rng = np.random.default_rng(1)
    tex = rng.random((64, 64, 3)).astype(np.float32) * 0.6 + 0.2
    f = R.image_flags(fake(tex))[0]
    c.check(not f, 'a textured picture: nothing flagged', str(f))
    nan = tex.copy()
    nan[10:30, 10:30] = np.nan
    r = fake(tex)
    r.hdr = {v: nan for v in r.hdr}
    c.check('nan' in R.image_flags(r)[0], 'NaN pixels: nan')
    f = R.image_flags(fake(tex, [('leaves', 0, 400), ('trunk', 300, 300)]))[0]
    c.check('alpha-discards-all' in f, 'a draw keeping 0 of 400 samples: alpha-discards-all', str(f))
    a, b = fake(tex), fake(tex * 0.5)
    cmp = R.compare_images(a, b)
    c.check(cmp['iou'] == 1 and cmp['mae'] > 0.1 and cmp['lum_ratio'] < 0.6, 'compare: a darker picture differs',
            str(cmp['threequarter']))
    c.check(R.severity(['black'], cmp) > R.severity(['black']) > R.severity([]), 'severity: flags and differences add')
    # textures
    ok = struct.pack('<8I', 0, 0, 0, 1, 8, 8, 1, 1) + struct.pack('<15I', 32, *[0] * 14) + bytes(32)
    c.check(R.texture_problem(ok) == '', 'texture check: an 8x8 DXT1 is fine', R.texture_problem(ok))
    c.check(R.texture_problem(ok[:50]) != '', 'texture check: a short file is refused')
    odd = struct.pack('<8I', 0, 0, 0, 1, 6, 8, 1, 1) + struct.pack('<15I', 32, *[0] * 14) + bytes(32)
    c.check('block' in R.texture_problem(odd), 'texture check: a block format 6 wide is refused')
    c.check('format' in R.texture_problem(struct.pack('<8I', 0, 0, 77, 0, 4, 4, 1, 1) + bytes(60 + 16)),
            'texture check: an unknown format is refused')
    c.check(R.texture_problem(ok[:-1]) != '', 'texture check: levels past the end are refused')
    # sampler states, light, camera, constants, bones
    blk = block(samplers=[(0, sampler_desc(3)), (2, sampler_desc())])
    c.check(R.sampler_states(blk) == [(0, sampler_desc(3)), (2, sampler_desc())], 'sampler-state records read')
    values, post = R.light(R.MP005_SUNNY)
    d = np.array(values['outdoorLightDir'][:3])
    th, ph = math.radians(63.666), math.radians(53.248)
    c.check(abs(np.linalg.norm(d) - 1) < 1e-6 and d[1] < 0 and abs(d[0] + math.cos(th) * math.sin(ph)) < 1e-9,
            'light: the sun direction from theta / phi (unit, pointing down)', str(d))
    c.check(np.allclose(values['outdoorLightKeyColor'][:3], np.array([1.0, 0.971, 0.914]) * 1.576),
            'light: key colour times its intensity')
    ldr, exposure = R.tonemap(np.full((4, 4, 3), 0.18, np.float32), np.ones((4, 4), bool), post)
    c.check(exposure == 1.0 and 0.4 < ldr.mean() < 0.6, 'tone map: mp_005 fixed exposure, mid grey stays mid',
            f'{exposure} {ldr.mean()}')
    eye, view, vp = R.camera(np.array([-1.0, -1, -1]), np.array([1.0, 1, 1]), 0.0, 0.0)
    c.check(eye[2] > 3 and abs(eye[0]) < 1e-9, 'camera: azimuth 0 looks at the front (+z)', str(eye))
    clip = vp @ np.array([0.0, 0, 0, 1])
    c.check(abs(clip[0] / clip[3]) < 1e-9 and 0 < clip[2] / clip[3] < 1, 'camera: the centre in the middle of the '
            'picture, inside the depth range')
    ps = P.compile_hlsl(PS, 'main', 'ps_5_0')
    cb = np.frombuffer(R.cbuffer(ps, {'outdoorLightKeyColor': (1, 2, 3, 4)}, [('Tint', 1, 4, TINT), ('Far', 7, 4, TINT)]),
                       np.float32)
    c.check(np.allclose(cb[:4], (1, 2, 3, 4)) and np.allclose(cb[4:8], TINT) and len(cb) == 8,
            'constants: engine value by name, material constant in the shader\'s own constants array only', str(cb))
    dd = R.Draw('p', 'e', 0, 'dice', b'', b'', [], 0, 0, 0, 0, 9, None, [], [], [])
    c.check(np.all(R.bone_values(dd)['boneMatrices'] == 1), 'destructible composite (kind 9): every part shown')
    dd.kind = 1
    c.check(np.allclose(R.bone_values(dd)['boneMatrices'][:3], np.eye(4)[:3]), 'other kinds: rest-pose bones')


def cli(c: Checks) -> None:
    tmp = tempfile.mkdtemp(prefix='renderqa_cli_')
    try:
        env = {**os.environ, 'LOCALAPPDATA': tmp}
        r = subprocess.run([sys.executable, os.path.join(HERE, '..', 'renderqa.py'), 'render', 'X/Y_Mesh', '--db',
                            os.path.join(tmp, 'missing', 'Dx11_Single.res'), '-o', os.path.join(tmp, 'o')],
                           capture_output=True, text=True, env=env)
        c.check(r.returncode == 1 and 'missing' in (r.stdout + r.stderr).lower(), 'command line: a missing database '
                'is an error (exit code 1, says so)', (r.stdout + r.stderr)[-300:])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def game(c: Checks) -> None:
    root = S.find_opfor(None)
    files = R.Files(root)
    tindex = R.truth_index(root)
    dbs = R.Databases(root)
    values, post = R.light(R.read_weather(files)[0])
    tmp = tempfile.mkdtemp(prefix='renderqa_game_')
    try:
        s = R.Settings(tmp, 128, log=lambda x: None)
        for m in ('objects/objects/ammocrate/ammocrate_mesh', 'objects/vehicles/land/humvee/humvee_mesh',
                  'objects/vegetation/trees/palmtree_01/palmtree_01_mesh'):
            lv = tindex.get(m)
            c.check(bool(lv), f'{m}: a level draws it', '')
            if not lv:
                continue
            db = dbs.get(f'level:{lv[0]}')
            _, r, rec = R.render_one(files, db, m, s, values, post)
            c.check(r is not None and not rec['flags'], f'{m}: drawn with {lv[0]}\'s database, nothing flagged',
                    str(rec['notes'][:4]))
            if r is not None:
                _, r2, _ = R.render_one(files, db, m, s, values, post)
                cmp = R.compare_images(r, r2) if r2 else None
                c.check(cmp is not None and cmp['mae'] < 1e-4 and cmp['iou'] == 1, f'{m}: drawn twice, equal',
                        str(cmp and cmp['threequarter']))
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


def main(argv: list[str]) -> int:
    c = Checks()
    checks(c)
    built(c)
    cli(c)
    if '--game' in argv:
        game(c)
    print(f'{c.failed} failed' if c.failed else 'all passed')
    return 1 if c.failed else 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
