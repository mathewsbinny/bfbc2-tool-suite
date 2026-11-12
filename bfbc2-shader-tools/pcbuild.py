"""PC shader back end for translated BF1943 shaders: compile HLSL with d3dcompiler_47 (ctypes), read back the
constants / resources, and build what BFBC2's shader database stores beside the code (binding table, Dx9 vertex
declaration, Dx10/11 input signature + input elements).

Binding table (shader database section 2), as decoded from BFBC2's files:
    u16 k, u16 first material-constant register, k x {u8 engine parameter id, u8 register, u8 index (0xff none),
    u8 array count}
Dx9: register = c<N>; Dx10/11: register = byte offset in cbuffer $Globals / 16.
"""
from __future__ import annotations

import ctypes
import struct
from collections.abc import Container, Iterable, Mapping, Sequence

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


def compile_hlsl(src: str, entry: str, profile: str, defines: Mapping[str, object] | None = None,
                 name: str = 'translated.hlsl') -> bytes:
    """D3DCompile -> bytecode (D3D9 tokens for vs/ps_3_0; DXBC for 4_0 / 5_0)."""
    d = _lib()
    macros: ctypes.Array[ctypes.c_char_p] | None = None
    if defines:
        arr = (ctypes.c_char_p * (2 * len(defines) + 2))()
        for i, (k, v) in enumerate(defines.items()):
            arr[2 * i], arr[2 * i + 1] = k.encode(), str(v).encode()
        macros = arr
    code, err = ctypes.c_void_p(), ctypes.c_void_p()
    s = src.encode()
    flags = 1 << 15          # D3DCOMPILE_OPTIMIZATION_LEVEL3
    hr = d.D3DCompile(s, len(s), name.encode(), macros, None, entry.encode(), profile.encode(), flags, 0,
                      ctypes.byref(code), ctypes.byref(err))
    msg = _blob(err.value).decode('latin-1', 'replace')
    if hr != 0:
        raise CompileError(f'{profile} {entry}: {msg}')
    return _blob(code.value)


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
