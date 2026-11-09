"""PC shader back end for translated BF1943 shaders: compile HLSL with d3dcompiler_47 (ctypes), read back the
constants / resources, and build what BFBC2's shader database stores beside the code (binding table, Dx9 vertex
declaration, Dx10/11 input signature + input elements).

Binding table (shader database section 2), as decoded from BFBC2's files:
    u16 k, u16 first material-constant register, k x {u8 engine parameter id, u8 register, u8 index (0xff none),
    u8 array count}
Dx9: register = c<N>; Dx10/11: register = byte offset in cbuffer $Globals / 16.
"""
import ctypes
import struct

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


def engine_param(name):
    """(id, index) of an engine constant name, or None (a material constant)."""
    if name in ENGINE and name not in INDEXED:
        return ENGINE[name], 0xff
    base = name.rstrip('0123456789')
    if base in INDEXED and base != name:
        return ENGINE[base], int(name[len(base):])
    return None


# ------------------------------------------------------------------------------------------------ d3dcompiler

_d3d = None


def _lib():
    global _d3d
    if _d3d is None:
        _d3d = ctypes.WinDLL('d3dcompiler_47.dll')
    return _d3d


def _blob(p):
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


def compile_hlsl(src, entry, profile, defines=None, name='translated.hlsl'):
    """D3DCompile -> bytecode (D3D9 tokens for vs/ps_3_0; DXBC for 4_0 / 5_0)."""
    d = _lib()
    macros = None
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


def input_signature(dxbc):
    out = ctypes.c_void_p()
    hr = _lib().D3DGetInputSignatureBlob(dxbc, len(dxbc), ctypes.byref(out))
    if hr != 0:
        raise CompileError('D3DGetInputSignatureBlob failed')
    return _blob(out.value)


def disassemble(code):
    out = ctypes.c_void_p()
    hr = _lib().D3DDisassemble(code, len(code), 0, None, ctypes.byref(out))
    return _blob(out.value).decode('latin-1') if hr == 0 else ''


# ------------------------------------------------------------------------------------------------ reflection

def ctab9(code):
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


def _chunks(dxbc):
    n = struct.unpack_from('<I', dxbc, 28)[0]
    out = {}
    for i in range(n):
        off = struct.unpack_from('<I', dxbc, 32 + 4 * i)[0]
        out[dxbc[off:off + 4]] = dxbc[off + 8:off + 8 + struct.unpack_from('<I', dxbc, off + 4)[0]]
    return out


def _cstr(b, o):
    return b[o:b.index(b'\0', o)].decode()


def rdef(dxbc):
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

def binding_table(consts, first, bone_div=1):
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


def consts_dx9(code):
    return [(n, r, c) for n, rs, r, c in ctab9(code) if rs == 'float4']


def consts_dxbc(code):
    cbs, _ = rdef(code)
    return [(n, off // 16, (size + 15) // 16) for n, off, size, used in cbs.get('$Globals', [])]


def first_material_register(consts, material):
    """The register the material constants start at: every engine constant below it (BFBC2's layout puts the
    material's 'constants' array after the engine's)."""
    m = [r for n, r, c in consts if n in material]
    if m:
        return min(m)
    return max([r + c for n, r, c in consts] or [0])


# ------------------------------------------------------------------------------------------------ vertex input

def dx9_decl(elements):
    """[(stream, offset, D3DDECLTYPE, usage index)] -> section 4 Dx9 tail (u32 k + D3DVERTEXELEMENT9s, END last)."""
    out = [struct.pack('<HHBBBB', s, o, t, 0, 5, ui) for s, o, t, ui in elements]
    out.append(struct.pack('<HHBBBB', 0xff, 0, 17, 0, 0, 0))
    return struct.pack('<I', len(out)) + b''.join(out)


def dx11_layout(dxbc, elements):
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


def block_texture_list(block, e='<'):
    """[(sampler register, cube flag, texture name)] of a parameter block (e '>' for an Xbox database's)."""
    out = []
    for k in range(block[0x1b]):
        o = 0x20 + TEX_RECORD * k
        out.append((block[o], block[o + 1], block[o + 4:o + 4 + 0x80].split(b'\0')[0].decode('latin-1')))
    return out


def program_block(donor, textures, nregs):
    """A PC program parameter block: the donor's sampler states, these textures, no constants; bytes 0x18/0x19 the
    shader's $Globals size in registers."""
    _, _, _, _, _, ee = struct.unpack_from('<6I', donor)
    nsamp = donor[0x1e]
    samp = donor[ee:ee + SAMPLER_STATE * nsamp]
    body = b''
    for reg, cube, name in textures:
        nm = name.encode('latin-1')[:0x7f]
        body += bytes((reg, cube, 0, 0)) + nm + bytes(0x80 - len(nm)) + bytes(8)
    end = 0x20 + len(body)
    total = end + len(samp)
    total += (-total) % 16
    hdr = struct.pack('<6I', total, 0x20, 0x20, end, end, end)
    hdr += bytes((nregs, nregs, 0, len(textures), 0, 0, nsamp, 0))
    out = hdr + body + samp
    return out + bytes(total - len(out))


def own_block(donor, nregs):
    """A shader's own parameter block: the donor's, its $Globals size set."""
    b = bytearray(donor)
    b[0x18] = b[0x19] = min(nregs, 255)
    return bytes(b)


def globals_registers(dxbc):
    """$Globals size in float4 registers of a DXBC shader."""
    cbs, _ = rdef(dxbc)
    return max([(off + size + 15) // 16 for _, off, size, _ in cbs.get('$Globals', [])] or [0])
