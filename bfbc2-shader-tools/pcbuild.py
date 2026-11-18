"""PC shader back end for translated BF1943 shaders: compile HLSL with d3dcompiler_47 (ctypes), read back the
constants / resources, and build what BFBC2's shader database stores beside the code (binding table, Dx9 vertex
declaration, Dx10/11 input signature + input elements).

Binding table (shader database section 2), as decoded from BFBC2's files:
    u16 k, u16 first material-constant register, k x {u8 engine parameter id, u8 register, u8 index (0xff none),
    u8 array count}
Dx9: register = c<N>; Dx10/11: register = byte offset in cbuffer $Globals / 16.
"""
from __future__ import annotations

import collections
import concurrent.futures as cf
import contextlib
import ctypes
import hashlib
import os
import struct
import threading
import time
from collections.abc import Container, Iterable, Mapping, Sequence
from typing import Any, NamedTuple

Consts = list[tuple[str, int, int]]         # [(name, register, register count)]

ENGINE = {   # engine parameter name -> id (from every BFBC2 Dx9 database's binding tables against the constant tables)
    'viewMatrix': 0, 'viewProjMatrix': 2, 'worldViewProjMatrix': 6, 'worldMatrix': 8, 'viewportZMinMax': 22,
    'boneMatrices': 23, 'boneVectors': 23, 'cameraPos': 24, 'time': 25, 'invScreenSize': 26,
    'pointLightPos': 27, 'pointLightColor': 28, 'pointLightRadius': 29,
    'outdoorLightDir': 30, 'outdoorLightHemisphereDir': 31, 'outdoorLightKeyColor': 32,
    'outdoorLightKeySpecularColorAndHemisphereVisibility': 33, 'outdoorLightTopColor': 34,
    'outdoorLightBottomColor': 35, 'outdoorLightShadowTransform': 36, 'outdoorLightShadowmapSizeAndInvSize': 37,
    'transparentStartAndEndAndClamp': 45,
}
INDEXED = {'pointLightPos', 'pointLightColor', 'pointLightRadius', 'outdoorLightShadowTransform'}
# Named block constants the engine writes itself, by name, at draw time: a parameter block's constant record (FORMAT.md
# section 1) names them at a register of the shader's 'constants' array, the block's value a placeholder (DICE's: 0).
# The terrain renderer's (the baked road / decal strips, the terrain patches) - each name in BFBC2Game.exe and in
# DICE's own PC strip programs (the vertex shader's own block: heightmapScaleAndOffsetXZ, heightmapDecalOffsetY; the
# program's second block: heightmapScaleAndOffsetXZ, heightmapPixelSizeAndAspect, dynamicMaskAtlasOffset /
# AtlasParams / TileBorderParams). A translated shader must read them from its constant buffer at a register its own
# block names: baked in as the Xbox block's placeholder they were 0 (every strip vertex at the decal offset 0, the
# dynamic mask and height lookups at the map's corner). The terrain patches' vertex shaders (their own block: posScale,
# posOffset, heightmapTexCoordScaleAndOffset - DICE's and BF1943's alike, placeholders 0) place each patch's grid in
# the world with them: baked as 0 every patch vertex lay at the origin. The other names beside them in BFBC2Game.exe's
# terrain renderer (posScaleAndOffsetXZ, heightmapWorldOffsetXZ / ScaleXZ) are taken by name too, where a block has them
ENGINE_NAMED = frozenset({
    'heightmapScaleAndOffsetXZ', 'heightmapDecalOffsetY', 'heightmapPixelSizeAndAspect',
    'heightmapTexCoordScaleAndOffset', 'heightmapWorldScaleY', 'heightmapWorldOffsetXZ', 'heightmapWorldScaleXZ',
    'dynamicMaskAtlasOffset', 'dynamicMaskAtlasParams', 'dynamicMaskTileBorderParams',
    'posScale', 'posOffset', 'posScaleAndOffsetXZ',
})
# Engine textures a parameter block names by parameter (no texture name; FORMAT.md section 1 'engine textures'): the
# terrain's height map and dynamic (destruction) mask atlas
ENGINE_TEXTURES = frozenset({'heightmap', 'dynamicMaskTileMap', 'dynamicMaskIndirectionMap', 'maskTileMap'})


def name_hash(name: str) -> int:
    """The u32 a parameter block stores beside a constant's or engine texture's name: Adler-32 of the name in lower
    case (every record of BFBC2's PC and BF1943's Xbox databases read: 'z' 0x007b007b, 'heightmap' 0x127303b8,
    'heightmapScaleAndOffsetXZ' 0x85b00a6c)."""
    import zlib
    return zlib.adler32(name.lower().encode('latin-1'))


def engine_param(name: str) -> tuple[int, int] | None:
    """(id, index) of an engine constant name, or None (a material constant)."""
    if name in ENGINE and name not in INDEXED:
        return ENGINE[name], 0xff
    base = name.rstrip('0123456789')
    if base in INDEXED and base != name:
        return ENGINE[base], int(name[len(base):])
    return None


# ------------------------------------------------------------------------------------------------ d3dcompiler

_d3d: ctypes.WinDLL | None = None


def _lib() -> ctypes.WinDLL:
    global _d3d
    if _d3d is None:
        with _lock:
            if _d3d is None:
                _d3d = ctypes.WinDLL('d3dcompiler_47.dll')
    return _d3d


def _blob(p: int | None) -> bytes:
    if not p:
        return b''
    vt = ctypes.cast(ctypes.cast(p, ctypes.POINTER(ctypes.c_void_p))[0], ctypes.POINTER(ctypes.c_void_p))
    getptr = ctypes.WINFUNCTYPE(ctypes.c_void_p, ctypes.c_void_p)(vt[3])
    getsize = ctypes.WINFUNCTYPE(ctypes.c_size_t, ctypes.c_void_p)(vt[4])
    release = ctypes.WINFUNCTYPE(ctypes.c_ulong, ctypes.c_void_p)(vt[2])
    data = ctypes.string_at(getptr(p), getsize(p))
    release(p)
    return data


class CompileError(Exception):
    pass


FLAGS = 1 << 15          # D3DCOMPILE_OPTIMIZATION_LEVEL3


def compile_hlsl_uncached(src: str, entry: str, profile: str, defines: Mapping[str, object] | None = None,
                          name: str = 'translated.hlsl') -> bytes:
    """D3DCompile -> bytecode (D3D9 tokens for vs/ps_3_0; DXBC for 4_0 / 5_0), always compiled: compile_hlsl's
    compiler. ctypes lets go of the interpreter for the call, so threads compile side by side."""
    d = _lib()
    macros: ctypes.Array[ctypes.c_char_p] | None = None
    if defines:
        arr = (ctypes.c_char_p * (2 * len(defines) + 2))()
        for i, (k, v) in enumerate(defines.items()):
            arr[2 * i], arr[2 * i + 1] = k.encode(), str(v).encode()
        macros = arr
    code, err = ctypes.c_void_p(), ctypes.c_void_p()
    s = src.encode()
    hr = d.D3DCompile(s, len(s), name.encode(), macros, None, entry.encode(), profile.encode(), FLAGS, 0,
                      ctypes.byref(code), ctypes.byref(err))
    msg = _blob(err.value).decode('latin-1', 'replace')
    if hr != 0:
        raise CompileError(f'{profile} {entry}: {msg}')
    return _blob(code.value)


# ------------------------------------------------------------------------------------------------ cache, batches
# Round E3d (2026-11-17). OPFOR's round 0 profile: 2,444 compiles (147 s) in one WW2 level build and 3,652 (365 s,
# 84 % of the step) in wake_translate, one at a time; of the 2,810 shaders wake_translate --hlsl last wrote, 790
# sources were distinct. So compile_hlsl remembers what it compiled (by a hash of the source, entry, profile,
# defines, name, flags and the compiler DLL's own bytes): in memory for the process, and on disk in
# %LOCALAPPDATA%\bfbc2-tool-suite\cache\hlsl (BFBC2_HLSL_CACHE: another folder, or 0 for none; use_cache()), each
# file written whole (a temporary file, then a rename) and checked when read (a damaged one is a miss). The compiler
# is deterministic (the same bytes from every compile, sequential or threaded: tests/test_pcbuild_batch.py), so a
# cached result is the compile's. Round E3p: a hit marks its file used and the folder is kept under
# BFBC2_HLSL_CACHE_MB (default 1024), the least recently used deleted first (bfbc2core.caches). compile_many compiles a batch on all cores (the results in input order) and
# prefetch starts one without waiting: compile_hlsl then waits for a compile of its source already under way.

class CompileJob(NamedTuple):
    src: str
    entry: str = 'main'
    profile: str = 'ps_5_0'
    defines: Mapping[str, object] | None = None
    name: str = 'translated.hlsl'


CACHE_VERSION = b'BC2HLSL1'
MEMORY_LIMIT = 256 << 20          # bytes of compiled code kept in memory; past it the memory cache starts over
_lock = threading.RLock()
_memo: dict[bytes, bytes] = {}
_memo_size = 0
_inflight: dict[bytes, cf.Future[bytes]] = {}
_pool: cf.ThreadPoolExecutor | None = None
_compiler: str | None = None
_cache_setting: str | bool | None = None          # use_cache(): None = the environment's
stats: collections.Counter[str] = collections.Counter()    # compiled, memory, disk, joined, failed


def compiler_id() -> str:
    """SHA-256 of the d3dcompiler_47.dll this process loaded: the cache's compiler identity."""
    global _compiler
    if _compiler is None:
        d = _lib()
        buf = ctypes.create_unicode_buffer(32768)
        n = ctypes.windll.kernel32.GetModuleFileNameW(ctypes.c_void_p(d._handle), buf, len(buf))
        path = buf.value[:n]
        with open(path, 'rb') as fh:
            _compiler = hashlib.sha256(fh.read()).hexdigest()
    return _compiler


def use_cache(setting: str | bool | None = True) -> None:
    """The disk cache: True (the default folder), False (none), a folder, or None (as BFBC2_HLSL_CACHE says).
    The memory cache stays on; clear_memory() empties it."""
    global _cache_setting
    _cache_setting = setting


def cache_dir() -> str | None:
    """The disk cache's folder for this compiler, or None when off."""
    s: str | bool | None = _cache_setting
    if s is None:
        env = os.environ.get('BFBC2_HLSL_CACHE', '')
        s = False if env == '0' else (env or True)
    if s is False:
        return None
    if s is True:
        base = os.environ.get('LOCALAPPDATA')
        if not base:
            return None
        s = os.path.join(base, 'bfbc2-tool-suite', 'cache', 'hlsl')
    return os.path.join(str(s), compiler_id()[:16])


def clear_memory() -> None:
    global _memo_size
    with _lock:
        _memo.clear()
        _memo_size = 0


def _key(src: str, entry: str, profile: str, defines: Mapping[str, object] | None, name: str) -> bytes:
    h = hashlib.sha256(CACHE_VERSION)
    parts = [compiler_id(), str(FLAGS), entry, profile, name, src]
    for k, v in (defines or {}).items():
        parts += [k, str(v)]
    for p in parts:
        b = p.encode()
        h.update(len(b).to_bytes(8, 'little'))
        h.update(b)
    return h.digest()


def _remember(key: bytes, code: bytes) -> None:
    global _memo_size
    with _lock:
        if key not in _memo:
            if _memo_size + len(code) > MEMORY_LIMIT:
                _memo.clear()
                _memo_size = 0
            _memo[key] = code
            _memo_size += len(code)


_caches_mod: Any = None


def _caches() -> Any:
    """bfbc2core.caches (the size cap and its least-recently-used pruning: BFBC2_HLSL_CACHE_MB, default 1 GB), or
    None when this file runs outside the suite."""
    global _caches_mod
    if _caches_mod is None:
        try:
            import sys
            suite = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
            if suite not in sys.path:
                sys.path.insert(0, suite)
            import suite_paths
            suite_paths.use('core')
            from bfbc2core import caches
            _caches_mod = caches
        except ImportError:
            _caches_mod = False
    return _caches_mod or None


def _disk_path(folder: str, key: bytes) -> str:
    k = key.hex()
    return os.path.join(folder, k[:2], k + '.bin')


def _disk_get(key: bytes) -> bytes | None:
    folder = cache_dir()
    if folder is None:
        return None
    try:
        with open(_disk_path(folder, key), 'rb') as fh:
            data = fh.read()
    except OSError:
        return None
    head = len(CACHE_VERSION)
    code = data[head + 64:]
    if data[:head] != CACHE_VERSION or data[head:head + 32] != key or \
            data[head + 32:head + 64] != hashlib.sha256(code).digest():
        return None                                     # damaged or foreign: a miss, compiled and written again
    c = _caches()
    if c is not None:
        c.touch(_disk_path(folder, key))                # used now: the pruning deletes the least recently used
    return code


def _disk_put(key: bytes, code: bytes) -> None:
    folder = cache_dir()
    if folder is None:
        return
    path = _disk_path(folder, key)
    tmp = f'{path}.tmp-{os.getpid()}-{threading.get_ident()}'
    try:
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(tmp, 'wb') as fh:
            fh.write(CACHE_VERSION + key + hashlib.sha256(code).digest() + code)
        for i in range(6):          # another process reading the same entry for a moment (Windows): wait, retry
            try:
                os.replace(tmp, path)
                c = _caches()
                if c is not None:   # the folder kept under its cap (all compilers' folders: cache_dir's parent)
                    c.maybe_prune('hlsl', os.path.dirname(folder))
                return
            except PermissionError:
                if os.path.exists(path):
                    break           # an equal entry is there already (same key, same bytes)
                time.sleep(0.05 * (i + 1))
    except OSError:
        pass                        # a cache that cannot be written is no cache, never an error
    with contextlib.suppress(OSError):
        os.remove(tmp)


def _compile_store(key: bytes, job: CompileJob) -> bytes:
    try:
        code = compile_hlsl_uncached(job.src, job.entry, job.profile, job.defines, job.name)
    except BaseException:
        with _lock:
            stats['failed'] += 1
            _inflight.pop(key, None)
        raise
    _remember(key, code)            # remembered before it leaves _inflight: never neither
    with _lock:
        stats['compiled'] += 1
        _inflight.pop(key, None)
    _disk_put(key, code)
    return code


def workers() -> int:
    """Compile threads: BFBC2_HLSL_WORKERS, else the CPU threads."""
    env = os.environ.get('BFBC2_HLSL_WORKERS', '')
    return max(1, int(env)) if env.isdigit() else max(1, os.cpu_count() or 1)


def _executor() -> cf.ThreadPoolExecutor:
    global _pool
    with _lock:
        if _pool is None:
            _pool = cf.ThreadPoolExecutor(workers(), thread_name_prefix='hlsl')
        return _pool


def compile_hlsl(src: str, entry: str, profile: str, defines: Mapping[str, object] | None = None,
                 name: str = 'translated.hlsl') -> bytes:
    """D3DCompile -> bytecode (D3D9 tokens for vs/ps_3_0; DXBC for 4_0 / 5_0); the same source, entry, profile,
    defines and name compiled before (this process, or any with the same compiler and cache) is not compiled again,
    and one under way (prefetch, compile_many) is waited for. Raises CompileError."""
    key = _key(src, entry, profile, defines, name)
    with _lock:                     # one look at both: a finished compile is remembered before it leaves _inflight
        code = _memo.get(key)
        fut = _inflight.get(key) if code is None else None
        if code is not None or fut is not None:
            stats['memory' if code is not None else 'joined'] += 1
    if code is not None:
        return code
    if fut is not None:
        return fut.result()
    code = _disk_get(key)
    if code is not None:
        with _lock:
            stats['disk'] += 1
        _remember(key, code)
        return code
    return _compile_store(key, CompileJob(src, entry, profile, defines, name))


def _job(j: CompileJob | Sequence[Any]) -> CompileJob:
    return j if isinstance(j, CompileJob) else CompileJob(*j)


def prefetch(jobs: Iterable[CompileJob | Sequence[Any]]) -> list[bytes]:
    """Start compiling the jobs not cached or under way, on the compile threads, and return at once: a later
    compile_hlsl of the same job takes the result (or waits for it). The jobs' keys, in order."""
    keys = []
    for j in map(_job, jobs):
        key = _key(j.src, j.entry, j.profile, j.defines, j.name)
        keys.append(key)
        with _lock:
            if key in _memo or key in _inflight:
                continue
        if _disk_get(key) is not None:      # read again (and counted) when asked for
            continue
        with _lock:
            if key not in _inflight:
                _inflight[key] = _executor().submit(_compile_store, key, j)
    return keys


def compile_many(jobs: Iterable[CompileJob | Sequence[Any]],
                 errors: str = 'raise') -> list[bytes | CompileError]:
    """Compile a batch on all cores: each job a CompileJob or (src, entry, profile[, defines[, name]]); the results
    in input order, the bytes compile_hlsl gives (a source repeated in the batch is compiled once). A failed job:
    errors='raise' raises the first failure's CompileError (in input order) once all are done; 'return' puts it in
    its place."""
    js = [_job(j) for j in jobs]
    prefetch(js)
    out: list[bytes | CompileError] = []
    first: CompileError | None = None
    for j in js:
        try:
            out.append(compile_hlsl(j.src, j.entry, j.profile, j.defines, j.name))
        except CompileError as e:
            first = first or e
            out.append(e)
    if first is not None and errors == 'raise':
        raise first
    return out


def input_signature(dxbc: bytes) -> bytes:
    out = ctypes.c_void_p()
    hr = _lib().D3DGetInputSignatureBlob(dxbc, len(dxbc), ctypes.byref(out))
    if hr != 0:
        raise CompileError('D3DGetInputSignatureBlob failed')
    return _blob(out.value)


def disassemble(code: bytes) -> str:
    out = ctypes.c_void_p()
    hr = _lib().D3DDisassemble(code, len(code), 0, None, ctypes.byref(out))
    return _blob(out.value).decode('latin-1') if hr == 0 else ''


# ------------------------------------------------------------------------------------------------ reflection

def ctab9(code: bytes) -> list[tuple[str, str, int, int]]:
    """D3D9 bytecode -> [(name, regset 'bool'|'int4'|'float4'|'sampler', register, count)]"""
    o = 4
    while o + 4 <= len(code):
        tok = struct.unpack_from('<I', code, o)[0]
        if tok & 0xffff != 0xfffe:
            break
        n = tok >> 16
        if code[o + 4:o + 8] == b'CTAB':
            base = o + 8
            _, _, _, nconst, cinfo, _, _ = struct.unpack_from('<7I', code, base)
            out = []
            for i in range(nconst):
                name, rs, ri, rc, _, _, _ = struct.unpack_from('<IHHHHII', code, base + cinfo + 20 * i)
                nm = code[base + name:code.index(b'\0', base + name)].decode()
                out.append((nm, ['bool', 'int4', 'float4', 'sampler'][rs], ri, rc))
            return out
        o += 4 + 4 * n
    return []


def _chunks(dxbc: bytes) -> dict[bytes, bytes]:
    n = struct.unpack_from('<I', dxbc, 28)[0]
    out = {}
    for i in range(n):
        off = struct.unpack_from('<I', dxbc, 32 + 4 * i)[0]
        out[dxbc[off:off + 4]] = dxbc[off + 8:off + 8 + struct.unpack_from('<I', dxbc, off + 4)[0]]
    return out


def _cstr(b: bytes, o: int) -> str:
    return b[o:b.index(b'\0', o)].decode()


def rdef(dxbc: bytes) -> tuple[dict[str, list[tuple[str, int, int, bool]]], list[tuple[str, int, int, int]]]:
    """DXBC -> ({cbuffer name: [(var, offset, size, used)]}, [(resource name, type, bind point, count)])"""
    r = _chunks(dxbc)[b'RDEF']
    ncb, cbo, nb, bo, _, _, _ = struct.unpack_from('<7I', r, 0)
    sm5 = r[28:32] == b'RD11'
    vsize = 40 if sm5 else 24
    cbs = {}
    for i in range(ncb):
        no, nv, vo, size, flags, typ = struct.unpack_from('<6I', r, cbo + 24 * i)
        vs = []
        for j in range(nv):
            vn, start, sz, vf = struct.unpack_from('<4I', r, vo + vsize * j)
            vs.append((_cstr(r, vn), start, sz, bool(vf & 2)))
        cbs[_cstr(r, no)] = vs
    res = []
    for i in range(nb):
        no, typ, ret, dim, ns, bp, bc, fl = struct.unpack_from('<8I', r, bo + 32 * i)
        res.append((_cstr(r, no), typ, bp, bc))
    return cbs, res


# ------------------------------------------------------------------------------------------------ binding tables

def binding_table(consts: Iterable[tuple[str, int, int]], first: int, bone_div: int = 1) -> bytes:
    """consts: [(name, register, register count)] the shader declares. -> binding table bytes (section 2)."""
    recs = []
    for name, reg, cnt in consts:
        p = engine_param(name)
        if p is None:
            continue
        pid, idx = p
        n = cnt // bone_div if name in ('boneMatrices', 'boneVectors') else 1
        recs.append(bytes((pid, reg, idx, min(n, 255))))
    return struct.pack('<HH', len(recs), first) + b''.join(recs)


def resource_table_dxbc(code: bytes) -> bytes:
    """Section 3 engine textures, using this shader's compiled resource slots.

    A donor's table can overwrite a material texture with its shadow/reflection
    resource, or omit a reflection used by the translated shader. Native PC
    records encode engine id, texture kind (0x40 2D / 0x42 cube), slot, index.
    Unused resources are absent from RDEF and need no engine binding.
    """
    engine = {'texture_outdoorLightShadowmapTexture': (38, 0x40),
              'texture_outdoorLightReflectionEnvmap': (39, 0x42)}
    records = []
    for name, typ, slot, count in rdef(code)[1]:
        if typ != 2 or name not in engine:
            continue
        if count != 1 or not 0 <= slot <= 255:
            raise ValueError(f'unsupported engine texture binding: {name} t{slot}[{count}]')
        pid, kind = engine[name]
        records.append(bytes((pid, kind, slot, 0xff)))
    return struct.pack('<I', len(records)) + b''.join(records)


def consts_dx9(code: bytes) -> Consts:
    return [(n, r, c) for n, rs, r, c in ctab9(code) if rs == 'float4']


def consts_dxbc(code: bytes) -> Consts:
    cbs, _ = rdef(code)
    return [(n, off // 16, (size + 15) // 16) for n, off, size, used in cbs.get('$Globals', [])]


def first_material_register(consts: Iterable[tuple[str, int, int]], material: Container[str]) -> int:
    """The register the material constants start at: every engine constant below it (BFBC2's layout puts the
    material's 'constants' array after the engine's)."""
    m = [r for n, r, c in consts if n in material]
    if m:
        return min(m)
    return max([r + c for n, r, c in consts] or [0])


# ------------------------------------------------------------------------------------------------ vertex input

def dx9_decl(elements: Iterable[tuple[int, int, int, int]]) -> bytes:
    """[(stream, offset, D3DDECLTYPE, usage index)] -> section 4 Dx9 tail (u32 k + D3DVERTEXELEMENT9s, END last)."""
    out = [struct.pack('<HHBBBB', s, o, t, 0, 5, ui) for s, o, t, ui in elements]
    out.append(struct.pack('<HHBBBB', 0xff, 0, 17, 0, 0, 0))
    return struct.pack('<I', len(out)) + b''.join(out)


def dx11_layout(dxbc: bytes, elements: Sequence[tuple[int, int, int, int, object]]) -> bytes:
    """[(stream, offset, DXGI format, usage index, per-instance)] -> section 4 Dx10/11 tail."""
    sig = input_signature(dxbc)
    out = struct.pack('<I', len(sig)) + sig + struct.pack('<I', len(elements))
    for s, o, f, ui, inst in elements:
        out += struct.pack('<7I', 0, ui, f, s, o, 1 if inst else 0, 1 if inst else 0)
    out += struct.pack('<I', len(elements)) + b''.join(b'TEXCOORD\0' for _ in elements)
    return out


# ------------------------------------------------------------------------------------------------ parameter blocks

TEX_RECORD = 0x8c
SAMPLER_STATE = 60          # one sampler-state record (FORMAT.md section 1)

# BFBC2 layout element format (section 7) -> DXGI_FORMAT, from the Dx10/11 vertex shaders' input elements against
# their programs' layouts; integer formats are declared as integer vertex-shader inputs
DXGI = {1: 41, 2: 16, 3: 6, 4: 2, 5: 54, 6: 34, 7: 10, 8: 10, 12: 30, 15: 38, 19: 37, 21: 13}
INT_FORMATS = {30: 'uint4', 38: 'int4'}


def block_texture_list(block: bytes, e: str = '<') -> list[tuple[int, int, str]]:
    """[(sampler register, cube flag, texture name)] of a parameter block (e '>' for an Xbox database's)."""
    out = []
    for k in range(block[0x1b]):
        o = 0x20 + TEX_RECORD * k
        out.append((block[o], block[o + 1], block[o + 4:o + 4 + 0x80].split(b'\0')[0].decode('latin-1')))
    return out


Named = tuple[str, int, int, tuple[float, ...]]          # (constant name, register, components used, value)


def param_block(textures: Sequence[tuple[int, int, str]], constants: Sequence[Named],
                engine_textures: Sequence[tuple[str, int]], samplers: bytes, nsamp: int, nregs: int,
                first: int | None = None) -> bytes:
    """A PC parameter block (FORMAT.md section 1): texture records (sampler, cube, name), named constant records
    (name, register, components, value; their Adler-32 hash), engine texture records (parameter name, texture slot),
    the sampler-state records as given; 0x18 the shader's $Globals size, 0x19 its first material register (DICE's:
    where the block's named constants start; the size when it has none)."""
    body = b''
    for reg, cube, name in textures:
        nm = name.encode('latin-1')[:0x7f]
        body += bytes((reg, cube, 0, 0)) + nm + bytes(0x80 - len(nm)) + bytes(8)
    tex_end = 0x20 + len(body)
    for name, reg, comps, vals in constants:
        nm = name.encode('latin-1')
        if len(nm) >= 0x20 or not 0 <= reg <= 255 or not 1 <= comps <= 4:
            raise ValueError(f'constant record {name} c{reg} ({comps}) does not fit')
        body += nm + bytes(0x20 - len(nm)) + struct.pack('<IBBxx4f', name_hash(name), reg, comps, *vals)
    const_end = 0x20 + len(body)
    for name, slot in engine_textures:
        nm = name.encode('latin-1')
        if len(nm) >= 0x20:
            raise ValueError(f'engine texture record {name} does not fit')
        body += nm + bytes(0x20 - len(nm)) + struct.pack('<IHxx', name_hash(name), slot)
    eng_end = 0x20 + len(body)
    total = eng_end + len(samplers)
    total += (-total) % 16
    hdr = struct.pack('<6I', total, 0x20, 0x20, tex_end, const_end, eng_end)
    n = min(nregs, 255)
    hdr += bytes((n, n if first is None else min(first, n), 0, len(textures), len(constants), len(engine_textures),
                  nsamp, 0))
    out = hdr + body + samplers
    return out + bytes(total - len(out))


def _samplers(donor: bytes) -> tuple[bytes, int]:
    _, _, _, _, _, ee = struct.unpack_from('<6I', donor)
    nsamp = donor[0x1e]
    return donor[ee:ee + SAMPLER_STATE * nsamp], nsamp


def program_block(donor: bytes, textures: Sequence[tuple[int, int, str]], nregs: int,
                  constants: Sequence[Named] = (), engine_textures: Sequence[tuple[str, int]] = (),
                  first: int | None = None) -> bytes:
    """A PC program parameter block: the donor's sampler states, these textures, the named constants and engine
    textures given (none: as before - textures only); bytes 0x18/0x19 the shader's $Globals size in registers and its
    first named register."""
    samp, nsamp = _samplers(donor)
    return param_block(textures, constants, engine_textures, samp, nsamp, nregs, first)


def own_block(donor: bytes, nregs: int, constants: Sequence[Named] | None = None, first: int | None = None,
              engine_textures: Sequence[tuple[str, int]] = ()) -> bytes:
    """A shader's own parameter block: the donor's, its $Globals size set. constants (the shader's engine-named
    constants at its own registers): the block rebuilt with them in place of the donor's constant records (a DICE
    strip vertex shader's own block names heightmapScaleAndOffsetXZ / heightmapDecalOffsetY at the donor's registers,
    not the translation's), and with engine_textures (parameter, the shader's texture slot) in place of the donor's
    engine texture records (a terrain patch vertex shader reads the height map: DICE's block names it at the donor's
    slot)."""
    if constants is None and not engine_textures:
        b = bytearray(donor)
        b[0x18] = b[0x19] = min(nregs, 255)
        if first is not None:
            b[0x19] = min(first, b[0x18])
        return bytes(b)
    samp, nsamp = _samplers(donor)
    import shaderdb
    textures = shaderdb.block_textures(donor)
    return param_block(textures, constants or [], engine_textures, samp, nsamp, nregs, first)


def named_registers(code: bytes, wanted: Mapping[str, tuple[int, Sequence[float]]]) -> list[Named]:
    """[(name, register, components, value)] of the engine-named constants (wanted: {name: (components, value)}) a
    compiled DXBC shader declares in $Globals, at the registers the compiler gave them."""
    out = []
    for name, off, _size, _ in rdef(code)[0].get('$Globals', []):
        if name in wanted:
            comps, vals = wanted[name]
            out.append((name, off // 16, comps, tuple(float(v) for v in vals)))
    return sorted(out, key=lambda r: r[1])


def engine_texture_slots(code: bytes, names: Iterable[str]) -> list[tuple[str, int]]:
    """[(engine texture parameter, texture slot)] of a compiled DXBC shader: its texture_<name> resources (unused ones
    are absent from RDEF and need no record)."""
    want = set(names)
    return sorted(((n[len('texture_'):], bp) for n, typ, bp, c in rdef(code)[1]
                   if typ == 2 and n.startswith('texture_') and n[len('texture_'):] in want), key=lambda r: r[1])


def globals_registers(dxbc: bytes) -> int:
    """$Globals size in float4 registers of a DXBC shader."""
    cbs, _ = rdef(dxbc)
    return max([(off + size + 15) // 16 for _, off, size, _ in cbs.get('$Globals', [])] or [0])
