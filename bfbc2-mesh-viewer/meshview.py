r"""meshview.py - look at the game's models: a mesh's parts with their colour textures, as a page you open in a
browser (orbit, zoom, a switch per part) and as a picture.

  python meshview.py <mesh>... [--out <folder>] [--lod N] [--png] [--size W] [--view side|top|front]
                                                                                 [--level <lv>] [--opfor <folder>]
  python meshview.py find <text> [--opfor <folder>]          mesh names containing <text>

<mesh> is a mesh asset name as the game's files give it, e.g. Objects/Weapons/Handheld/Beach/M1Garand/M1Garand_Mesh
(any case; the .dbx / _lod0_data.res endings may be given). Several meshes are drawn side by side in one page.
Default output: <OPFOR>\work\tools_out\meshview\<last name part>.html (and .png with --png). The page draws with
three.js from a CDN (needs an internet connection once); the models and textures are inside the page itself, which
is written into your work folder only - it contains game content, so do not share it.

How a mesh is read (the console converter's FORMAT.md sections 3-4): <mesh>_lod<N>_data.res (MeshData) names its parts,
their vertex and index ranges; each part's vertex layout is in the shader database of a level that draws the mesh
(section 8.3: '<mesh>_lod<N>/<part>/standard' -> layout); the mesh blueprint (<mesh>.dbx) gives each surface name its
shader, and the database entry '<mesh>/<shader>' names the shader's textures (the colour map is drawn). Positions are
the parts' rest positions (skinned meshes: the pose they were modelled in). Nothing in the game copy is written.
"""
import base64
import io
import os
import pickle
import re
import struct
import sys

import numpy as np

HERE = os.path.dirname(os.path.abspath(__file__))
SUITE = os.path.dirname(HERE)
sys.path[:0] = [os.path.join(SUITE, 'core'), os.path.join(SUITE, 'bfbc2-shader-tools'),
                os.path.join(SUITE, 'bfbc2-console-convert')]
from bfbc2core import dbx, fbrb  # noqa: E402
import mesh as meshdata  # noqa: E402
import shaderdb  # noqa: E402
import texture  # noqa: E402

SIZE = dict(meshdata.SIZE)
COLOUR = re.compile(r'(?i)_(d|dm|cm|c|col|color|colour|diff|diffuse|alb)$')
NOT_COLOUR = re.compile(r'(?i)^textures/(sky|_general|_mask)/|dirtmask|_(n|nm|s|spec|m|mask|ao|cube|env)$')
CACHE_VERSION = 3


class ViewError(Exception):
    pass


# ------------------------------------------------------------------------------------------------ game copy index

def _bundles(root):
    """Stock bundles - levels, async (weapons, characters ...) and the rest - Package before Dist (OPFOR's own
    builds and Onslaught levels left out)."""
    out = []
    for w in ('Package', os.path.join('Dist', 'win32')):
        top = os.path.join(root, 'game', w)
        for dp, dn, fn in os.walk(top):
            dn.sort()
            rel = os.path.relpath(dp, top).replace(os.sep, '/').lower()
            if re.match(r'levels/(ons_|opfor_)', rel):
                continue
            for f in sorted(fn):
                if f.lower().endswith('.fbrb'):
                    out.append((rel, os.path.join(dp, f)))
    return out


class Index:
    """Which bundle holds each entry (first stock bundle), and every level's Dx9 shader database facts: part name ->
    vertex layout record, entry name -> texture names. Cached in work\\tools_out\\meshview_index.pkl, rebuilt when a
    bundle changes."""

    def __init__(self, root):
        self.root = root
        bundles = _bundles(root)
        sig = [(b, os.path.getsize(b), int(os.path.getmtime(b))) for _, b in bundles]
        path = os.path.join(root, 'work', 'tools_out', 'meshview_index.pkl')
        try:
            c = pickle.load(open(path, 'rb'))
            if c['version'] == CACHE_VERSION and c['sig'] == sig:
                self.__dict__.update(c['data'])
                return
        except (OSError, EOFError, KeyError, pickle.UnpicklingError):
            pass
        print('indexing the game copy (once; cached) ...', file=sys.stderr)
        self.where, self.parts, self.entries = {}, {}, {}
        for lv, b in bundles:
            try:
                ents = fbrb.entries(b)
            except Exception:                                   # noqa: BLE001 - an unreadable bundle is skipped
                continue
            for e in ents:
                if not e.deleted:
                    self.where.setdefault(e.name.lower(), (b, e.name))
            if not any(e.type.lower() == 'dx9shaderdatabase' for e in ents):
                continue
            try:
                db, _ = shaderdb.load(b, 9)
            except Exception:                                   # noqa: BLE001 - a database it cannot read
                continue
            layouts = {r[:16]: r[16:] for r in db.layouts}
            for n, g in db.parts:
                if g in layouts:
                    self.parts.setdefault(n.lower(), layouts[g])
            groups = db.entry_programs()
            for n, g in db.entries:
                if n.lower() in self.entries:
                    continue
                tex = []
                for pi in groups.get(g, []):
                    _, _, a, c = db.program_refs(pi)
                    for blk in (db.params[a], db.params[c]):
                        for s in re.findall(rb'[A-Za-z0-9_/\.\- ]{6,}', blk):
                            s = s.decode()
                            if '/' in s and s not in tex:
                                tex.append(s)
                self.entries[n.lower()] = tex
        os.makedirs(os.path.dirname(path), exist_ok=True)
        pickle.dump({'version': CACHE_VERSION, 'sig': sig,
                     'data': {'where': self.where, 'parts': self.parts, 'entries': self.entries}}, open(path, 'wb'))

    def read(self, name):
        hit = self.where.get(name.lower())
        if not hit:
            return None
        return fbrb.read(hit[0], [hit[1]])[hit[1]]


# ------------------------------------------------------------------------------------------------ reading a mesh

def _surfaces(ix, name):
    """{surface name (lower): shader asset path} from the mesh blueprint."""
    out = {}
    b = ix.read(name + '.dbx')
    if b is not None:
        x = dbx.to_xml(b)
        for m in re.finditer(r'<field name="Name">([^<]*)</field>\s*<field ref="([^"]+)/[0-9a-fA-F-]{36}" name="Shader"', x):
            out[m.group(1).lower()] = m.group(2)
    if not out:                     # no blueprint: the database's entries for the mesh, by the shader's last name
        pre = name.lower() + '/'
        for e in ix.entries:
            if e.startswith(pre):
                out[e[len(pre):].rsplit('/', 1)[-1]] = e[len(pre):]
    return out


def _texture(ix, mesh_name, shader):
    """(texture name, RGB array) of the colour map of entry '<mesh>/<shader>', or (None, None)."""
    tex = ix.entries.get(f'{mesh_name}/{shader}'.lower(), [])
    pick = [t for t in tex if COLOUR.search(t) and not NOT_COLOUR.search(t)]
    pick += [t for t in tex if not NOT_COLOUR.search(t) and t not in pick]
    for t in pick:
        b = ix.read(t + '.res')
        if b is None:
            continue
        try:
            _, _, fmt = struct.unpack_from('<3I', b, 0)
            lv = texture.levels(b)
            _, _, w, h, data = lv[0]
            if fmt not in texture.FORMATS:
                continue
            return t, texture.decode(data, w, h, fmt).clip(0, 255).astype(np.uint8)
        except Exception:                                       # noqa: BLE001 - a texture it cannot decode
            continue
    return None, None


def _values(raw, fmt, count):
    if fmt == 0x03:
        return np.frombuffer(raw, '<f4').reshape(count, -1)[:, :3]
    if fmt == 0x04:
        return np.frombuffer(raw, '<f4').reshape(count, -1)[:, :4]
    if fmt in (0x05, 0x06, 0x07, 0x08):
        return np.frombuffer(raw, '<f2').reshape(count, -1).astype(np.float32)
    return None


def load_mesh(ix, name, lod=0):
    """[{name, surface, positions (n,3), normals, uvs, indices (m,3), texture name, image}] of the mesh's LOD."""
    data_name = f'{name}_lod{lod}_data.res'
    b = ix.read(data_name)
    if b is None:
        raise ViewError(f'{data_name}: in no stock bundle (meshview.py find <text> lists mesh names)')
    m = meshdata.parse(b, False)
    surfaces = _surfaces(ix, name)
    vbase = m['data']
    ibase = vbase + m['vertex_bytes']
    wide = m['wide_indices'] or False
    out = []
    for p in m['parts']:
        pname = p['name'].decode('latin-1')
        if pname.lower().endswith('_zonly'):                     # depth-only copies: nothing to see
            continue
        rec = ix.parts.get(f'{name}_lod{lod}/{pname}/standard'.lower())
        if rec is None:
            print(f'  {pname}: no vertex layout in any level\'s shader database - skipped', file=sys.stderr)
            continue
        els, stride = meshdata.elements(rec)
        isz = 4 if wide else 2
        idx = np.frombuffer(b, '<u4' if wide else '<u2', p['triangles'] * 3, ibase + p['first_index'] * isz).astype(np.int64)
        lo, hi = int(idx.min()), int(idx.max())
        n = hi - lo + 1
        start = vbase + p['vertex_offset'] + lo * stride
        block = np.frombuffer(b, np.uint8, n * stride, start).reshape(n, stride)
        attr = {}
        for f, usage, off, stream in els:
            if stream != 0 or f not in SIZE:
                continue
            raw = np.ascontiguousarray(block[:, off:off + SIZE[f]]).tobytes()
            v = _values(raw, f, n)
            if v is not None and usage not in attr:
                attr[usage] = v
        if 1 not in attr:
            print(f'  {pname}: no position element - skipped', file=sys.stderr)
            continue
        surf = max((s for s in surfaces if pname.lower().endswith(s)), key=len, default=None)
        tname, img = _texture(ix, name, surfaces[surf]) if surf else (None, None)
        out.append({'name': pname, 'surface': surf, 'positions': attr[1][:, :3].astype(np.float32),
                    'normals': attr[4][:, :3].astype(np.float32) if 4 in attr else None,
                    'uvs': attr[0][:, :2].astype(np.float32) if 0 in attr else None,
                    'indices': (idx - lo).reshape(-1, 3), 'texture': tname, 'image': img})
    return out


# ------------------------------------------------------------------------------------------------ output

def _png(img):
    from PIL import Image
    buf = io.BytesIO()
    Image.fromarray(img).save(buf, 'PNG')
    return buf.getvalue()


def _long_axis(laid):
    pts = np.concatenate([p['positions'] + off for _, parts, off in laid for p in parts])
    return 0 if np.ptp(pts[:, 0]) >= np.ptp(pts[:, 2]) else 2


def _layout(models):
    """The models one above the other (y), each centred on its own bounds: [(model name, parts, offset)]."""
    out, y = [], 0.0
    for name, parts in models:
        pts = np.concatenate([p['positions'] for p in parts])
        lo, hi = pts.min(0), pts.max(0)
        off = -(lo + hi) / 2
        off[1] = y - hi[1]
        out.append((name, parts, off.astype(np.float32)))
        y -= (hi[1] - lo[1]) + max(0.02, 0.15 * float(np.max(hi - lo)))
    return out


def render(laid, size=1200, view='side'):
    """A z-buffered picture (RGB array) of the laid-out models: textured, lit by the normals. side: the models'
    long horizontal axis across, up up; top: from above; front: along the long axis."""
    la = _long_axis(laid)
    other = 2 - la
    axes = {'side': (la, 1, other, 1), 'top': (la, other, 1, -1), 'front': (other, 1, la, -1)}[view]
    tris = []
    for _, parts, off in laid:
        for p in parts:
            pos = p['positions'] + off
            tris.append((pos, p))
    allp = np.concatenate([t[0][np.unique(t[1]['indices'])] for t in tris])     # the vertices triangles use
    a, bax, dax, sgn = axes
    lo = np.array([allp[:, a].min(), allp[:, bax].min()])
    hi = np.array([allp[:, a].max(), allp[:, bax].max()])
    scale = (size - 40) / max(float(np.max(hi - lo)), 1e-6)
    w = size
    h = int((hi[1] - lo[1]) * scale) + 40
    img = np.full((h, w, 3), 40, np.uint8)
    zb = np.full((h, w), np.inf)
    light = np.array([0.4, 0.8, 0.45]) / np.linalg.norm([0.4, 0.8, 0.45])
    for pos, p in tris:
        sx = (pos[:, a] - lo[0]) * scale + 20
        sy = h - ((pos[:, bax] - lo[1]) * scale + 20)
        sz = -sgn * pos[:, dax]
        uv = p['uvs']
        tex = p['image']
        for t in p['indices']:
            x0, x1, x2 = sx[t]
            y0, y1, y2 = sy[t]
            area = (x1 - x0) * (y2 - y0) - (x2 - x0) * (y1 - y0)
            if abs(area) < 1e-9:
                continue
            xa, xb = int(max(min(x0, x1, x2), 0)), int(min(max(x0, x1, x2), w - 1)) + 1
            ya, yb = int(max(min(y0, y1, y2), 0)), int(min(max(y0, y1, y2), h - 1)) + 1
            if xa >= xb or ya >= yb:
                continue
            gx, gy = np.meshgrid(np.arange(xa, xb) + 0.5, np.arange(ya, yb) + 0.5)
            w0 = ((x1 - gx) * (y2 - gy) - (x2 - gx) * (y1 - gy)) / area
            w1 = ((x2 - gx) * (y0 - gy) - (x0 - gx) * (y2 - gy)) / area
            w2 = 1 - w0 - w1
            inside = (w0 >= 0) & (w1 >= 0) & (w2 >= 0)
            if not inside.any():
                continue
            z = w0 * sz[t[0]] + w1 * sz[t[1]] + w2 * sz[t[2]]
            sub = zb[ya:yb, xa:xb]
            hit = inside & (z < sub)
            if not hit.any():
                continue
            sub[hit] = z[hit]
            if p['normals'] is not None:
                nrm = np.cross(pos[t[1]] - pos[t[0]], pos[t[2]] - pos[t[0]])
                nl = np.linalg.norm(nrm)
                shade = 0.35 + 0.65 * abs(float(nrm @ light) / nl) if nl > 0 else 1.0
            else:
                shade = 1.0
            if tex is not None and uv is not None:
                u = w0 * uv[t[0], 0] + w1 * uv[t[1], 0] + w2 * uv[t[2], 0]
                v = w0 * uv[t[0], 1] + w1 * uv[t[1], 1] + w2 * uv[t[2], 1]
                tx = (np.mod(u, 1.0) * (tex.shape[1] - 1)).astype(int)
                ty = (np.mod(v, 1.0) * (tex.shape[0] - 1)).astype(int)
                col = tex[ty[hit], tx[hit]].astype(np.float32)
            else:
                col = np.array([[170, 170, 170]], np.float32)
            img[ya:yb, xa:xb][hit] = np.clip(col * shade, 0, 255).astype(np.uint8)
    return img


PAGE = """<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1">
<title>%(title)s</title>
<style>
:root { --bg: #1d1f22; --panel: #2a2d31; --text: #e6e6e6; --muted: #9aa0a6; }
html, body { margin: 0; height: 100%%; background: var(--bg); color: var(--text); font: 13px system-ui, sans-serif; }
#panel { position: absolute; top: 8px; left: 8px; max-height: calc(100%% - 16px); overflow: auto; background: var(--panel);
         padding: 8px 10px; border-radius: 6px; max-width: 420px; }
#panel h2 { font-size: 13px; margin: 8px 0 4px; } #panel label { display: block; white-space: nowrap; }
#panel small { color: var(--muted); } canvas { display: block; }
</style>
<script type="importmap">{"imports": {"three": "https://cdn.jsdelivr.net/npm/three@0.160.0/build/three.module.js",
"three/addons/": "https://cdn.jsdelivr.net/npm/three@0.160.0/examples/jsm/"}}</script>
</head><body><div id="panel"><b>%(title)s</b><br><small>drag: orbit - right drag: pan - wheel: zoom</small>
<label><input type="checkbox" id="wire"> wireframe</label><div id="parts"></div></div>
<script type="module">
import * as THREE from 'three';
import { OrbitControls } from 'three/addons/controls/OrbitControls.js';
const MODELS = %(models)s;
const b64 = s => Uint8Array.from(atob(s), c => c.charCodeAt(0)).buffer;
const renderer = new THREE.WebGLRenderer({ antialias: true });
renderer.setPixelRatio(devicePixelRatio); renderer.setSize(innerWidth, innerHeight);
document.body.appendChild(renderer.domElement);
const scene = new THREE.Scene(); scene.background = new THREE.Color(0x1d1f22);
scene.add(new THREE.HemisphereLight(0xffffff, 0x404040, 1.2));
const sun = new THREE.DirectionalLight(0xffffff, 1.6); sun.position.set(2, 3, 2); scene.add(sun);
const camera = new THREE.PerspectiveCamera(40, innerWidth / innerHeight, 0.001, 1000);
const controls = new OrbitControls(camera, renderer.domElement);
const box = new THREE.Box3(), mats = [], list = document.getElementById('parts');
for (const m of MODELS) {
  const h = document.createElement('h2'); h.textContent = m.name; list.appendChild(h);
  for (const p of m.parts) {
    const g = new THREE.BufferGeometry();
    g.setAttribute('position', new THREE.BufferAttribute(new Float32Array(b64(p.pos)), 3));
    if (p.uv) g.setAttribute('uv', new THREE.BufferAttribute(new Float32Array(b64(p.uv)), 2));
    g.setIndex(new THREE.BufferAttribute(new Uint32Array(b64(p.idx)), 1));
    g.computeVertexNormals();
    const mat = new THREE.MeshStandardMaterial({ color: p.tex ? 0xffffff : 0xaaaaaa, roughness: 0.7, side: THREE.DoubleSide });
    if (p.tex) { const t = new THREE.TextureLoader().load('data:image/png;base64,' + p.tex); t.flipY = false;
                 t.wrapS = t.wrapT = THREE.RepeatWrapping; t.colorSpace = THREE.SRGBColorSpace; mat.map = t; }
    mats.push(mat);
    const mesh = new THREE.Mesh(g, mat); mesh.position.set(...m.offset); scene.add(mesh); box.expandByObject(mesh);
    const l = document.createElement('label'), c = document.createElement('input');
    c.type = 'checkbox'; c.checked = true; c.onchange = () => { mesh.visible = c.checked; };
    l.append(c, ' ' + p.name + ' '); const s = document.createElement('small');
    s.textContent = `${p.tris} tris` + (p.texname ? ' - ' + p.texname.split('/').pop() : ''); l.append(s); list.appendChild(l);
  }
}
const c = box.getCenter(new THREE.Vector3()), r = box.getSize(new THREE.Vector3()).length();
camera.position.set(c.x, c.y + r * 0.2, c.z + r * 1.1); controls.target.copy(c); controls.update();
document.getElementById('wire').onchange = e => mats.forEach(m => { m.wireframe = e.target.checked; });
addEventListener('resize', () => { camera.aspect = innerWidth / innerHeight; camera.updateProjectionMatrix();
  renderer.setSize(innerWidth, innerHeight); });
renderer.setAnimationLoop(() => { controls.update(); renderer.render(scene, camera); });
</script></body></html>
"""


def page(laid, title):
    import json
    models = []
    for name, parts, off in laid:
        ps = []
        for p in parts:
            ps.append({'name': p['name'], 'tris': len(p['indices']), 'texname': p['texture'],
                       'pos': base64.b64encode(p['positions'].astype('<f4').tobytes()).decode(),
                       'uv': base64.b64encode(p['uvs'].astype('<f4').tobytes()).decode() if p['uvs'] is not None else None,
                       'idx': base64.b64encode(p['indices'].astype('<u4').tobytes()).decode(),
                       'tex': base64.b64encode(_png(p['image'])).decode() if p['image'] is not None else None})
        models.append({'name': name.split('/')[-1], 'offset': [float(v) for v in off], 'parts': ps})
    import html
    # the title goes into the markup, the models into a script block: neither may close what holds it
    return PAGE % {'title': html.escape(title), 'models': json.dumps(models).replace('</', '<\\/')}


# ------------------------------------------------------------------------------------------------ command line

def _clean(name):
    name = name.replace('\\', '/')
    return re.sub(r'(?i)(_lod\d+_data)?\.(res|dbx)$', '', name)


def main(argv):
    args, opts, flags = [], {}, set()
    it = iter(argv[1:])
    for a in it:
        if a in ('--png',):
            flags.add(a)
        elif a.startswith('--'):
            opts[a] = next(it, None)
        else:
            args.append(a)
    if not args:
        print(__doc__)
        return 1
    root = shaderdb.find_opfor(opts.get('--opfor'))
    ix = Index(root)
    if args[0] == 'find':
        text = ' '.join(args[1:]).lower()
        hits = sorted({re.sub(r'_lod\d+_data\.res$', '', n) for n in ix.where if n.endswith('_data.res') and text in n})
        print('\n'.join(ix.where[h + '_lod0_data.res'][1][:-len('_lod0_data.res')] if h + '_lod0_data.res' in ix.where
                        else h for h in hits) or f'no mesh name contains "{text}"')
        return 0
    lod = int(opts.get('--lod') or 0)
    models = []
    for a in args:
        name = _clean(a)
        hit = ix.where.get(f'{name}_lod{lod}_data.res'.lower())
        name = hit[1][:-len(f'_lod{lod}_data.res')] if hit else name
        parts = load_mesh(ix, name, lod)
        if not parts:
            raise ViewError(f'{name}: no part could be read')
        models.append((name, parts))
        for p in parts:
            print(f'{name.split("/")[-1]}: {p["name"]:45s} {len(p["indices"]):6d} tris  '
                  f'{(p["texture"] or "(no colour texture)").split("/")[-1]}')
    laid = _layout(models)
    out = opts.get('--out') or os.path.join(root, 'work', 'tools_out', 'meshview')
    os.makedirs(out, exist_ok=True)
    stem = '+'.join(n.split('/')[-1] for n, _ in models)[:120]
    html = os.path.join(out, stem + '.html')
    open(html, 'w', encoding='utf-8').write(page(laid, ' + '.join(n.split('/')[-1] for n, _ in models)))
    print(f'page: {html}')
    if '--png' in flags:
        from PIL import Image
        png = os.path.join(out, f'{stem}_{opts.get("--view") or "side"}.png')
        Image.fromarray(render(laid, int(opts.get('--size') or 1200), opts.get('--view') or 'side')).save(png)
        print(f'picture: {png}')
    return 0


if __name__ == '__main__':
    try:
        sys.exit(main(sys.argv))
    except (ViewError, shaderdb.ShaderDbError, meshdata.ConvertError) as e:
        print(f'error: {e}', file=sys.stderr)
        sys.exit(1)
