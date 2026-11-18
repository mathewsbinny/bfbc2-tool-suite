"""Xenos (Xbox 360 GPU) shader microcode -> HLSL for BFBC2's Dx10 / Dx11 renderers (shader model 4 / 5).

Front end: xenos.py (decoder). Instruction semantics follow Xenia's ucode.h documentation (BSD-3); cube-map fetches
keep the direction the cube instruction was given, as XenosRecomp does (MIT).

translate(sh, iface) -> HLSL source with entry point 'main'. iface (Interface) tells the translator what the PC side
provides: the vertex inputs (VS), the interpolators shared by the VS and PS, material constant values, and per-sampler
fix-ups. Engine constants keep their names (pcbuild binds them by name); block constants the engine writes by name
(Interface.named_vs / named_ps: the terrain's heightmap and dynamic mask parameters) are $Globals variables under their
names, which the build records in the PC parameter blocks at the registers the compiler gives them.

The translation runs in the suite's Rust core (rust/bc2-shader, hlsl.rs; round E3f), which it needs
(bfbc2core.native.require: NativeMissing saying how to get it). Its Python twin (translate_python, _Emitter) was
retired on 2026-11-18 after the full test tier (rust/BENCHMARKS.md, "Retired Python"); its results, recorded before
it was deleted, are the oracle (tests/test_xenos_native.py: xenos_golden.json, xenos_golden_game.json). Any tree is
taken: xenos.parse_shader's (its program kept in Rust), one whose microcode a caller read or edited, the Python
decoder's (xenos.parse_shader_python, the format reference), copies and pickles, or one built by hand of any class
(the attributes the translation reads, by name). What the Rust core cannot take (an interface value of another type
than Interface's comments say, a literal that is not four 32-bit floats, an unknown operation's name, an empty
material value, a sampler fix with format fields other than {}) raises ValueError saying what; TranslateError is a
program the translator refuses, with its reason.
"""
from __future__ import annotations

import os
import sys
from collections.abc import Iterable, Sequence
from types import ModuleType
from typing import TYPE_CHECKING, Any

_HERE = os.path.dirname(os.path.abspath(__file__))
if _HERE not in sys.path:                           # xenos.py beside this file (the Rust core's programs)
    sys.path.insert(0, _HERE)
import xenos  # noqa: E402

if TYPE_CHECKING:
    from xenos import Obj


# engine constants: natural size in float4 registers on PC (Dx10/11 cbuffer); anything else is a material constant
ENGINE_SIZE = {
    'viewMatrix': 4, 'viewProjMatrix': 4, 'worldViewProjMatrix': 4, 'worldMatrix': 4, 'viewportZMinMax': 1,
    'cameraPos': 1, 'time': 1, 'invScreenSize': 1, 'pointLightPos0': 1, 'pointLightColor0': 1, 'pointLightRadius0': 1,
    'outdoorLightDir': 1, 'outdoorLightHemisphereDir': 1, 'outdoorLightKeyColor': 1,
    'outdoorLightKeySpecularColorAndHemisphereVisibility': 1, 'outdoorLightTopColor': 1, 'outdoorLightBottomColor': 1,
    'outdoorLightShadowTransform0': 4, 'outdoorLightShadowTransform1': 4, 'outdoorLightShadowTransform2': 4,
    'outdoorLightShadowmapSizeAndInvSize': 1, 'transparentStartAndEndAndClamp': 1,
    'boneMatrices': 180, 'boneVectors': 180,
}
SHADOW = 'outdoorLightShadowmapTexture'
PROBE_X0, PROBE_Y0, PROBE_W, PROBE_H = 60, 100, 26, 20     # the bit-bar block (screen pixels)
# instanced draws (BFBC2 StreamInstancingFloat): the per-instance rows' TEXCOORD semantics (inst0..inst2)
INSTANCE_SEMANTICS = (13, 14, 15)


class TranslateError(Exception):
    pass


class Interface:
    def __init__(self, interpolators: Sequence[int], vs_inputs: dict[int, str] | None = None,
                 material: dict[str, Any] | None = None, sampler_fix: dict[int, str] | None = None,
                 colour_scale: float = 1.0, instanced: bool = False, bones_as_world: bool = False,
                 material_regs: dict[int, Sequence[float]] | None = None, pc_shadow_atlas: bool = False,
                 sampler_registers: dict[int, int] | None = None, named_vs: dict[int, str] | None = None,
                 named_ps: dict[int, str] | None = None, fetch_registers: dict[int, int] | None = None) -> None:
        self.sampler_registers = sampler_registers or {}
        # texture fetch constant -> the constant table's sampler register, where they differ: a vertex shader's
        # samplers are fetched from other fetch constants than its table's s<n> (BF1943's terrain patch vertex shaders
        # fetch the height map, the table's sampler_heightmap s0, from tf22 - the register its block's engine texture
        # record names)
        self.fetch_registers = fetch_registers or {}
        # register -> name of a block constant the engine writes at draw time (pcbuild.ENGINE_NAMED: the terrain's
        # heightmapScaleAndOffsetXZ, dynamicMaskAtlasParams ...), per stage (each shader has its own 'constants'
        # array): declared in $Globals under its name, after the engine's own constants, instead of baked in as the
        # block's placeholder value - the build names it in the PC block at the register the compiler gives it
        self.named_vs = named_vs or {}
        self.named_ps = named_ps or {}
        self.pc_shadow_atlas = pc_shadow_atlas       # BFBC2 PC packs the three shadow cascades into atlas quadrants
        self.bones_as_world = bones_as_world        # BFBC2 composite (kind 9): parts drawn with the world transform
        self.trace_nan = False                      # debug: output the first instruction whose result is NaN / inf
        self.probe: list[str] | None = None         # debug: [constant expressions] drawn as bit bars (PROBE_*)
        self.instanced = instanced                  # VS: the world transform comes per instance (BFBC2 stream 1)
        self.interpolators = interpolators          # [usage index] the VS writes (TEXCOORDn), shared VS -> PS
        self.vs_inputs = vs_inputs or {}            # VS: usage index -> HLSL type of the PC element ('float4')
        self.material = material or {}              # name -> (x, y, z, w)
        # register -> (x, y, z, w): a parameter block's constants by the register they set. A block names each value
        # (sunDir, TrackOffset, fadeFactor ...), the compiled shader declares one array, 'constants', over those
        # registers: every block constant of BF1943's Wake database lies inside the vertex or pixel shader's
        # 'constants' array and none matches a constant table name, so only the register finds the value
        self.material_regs = material_regs or {}
        self.sampler_fix = sampler_fix or {}        # sampler register -> swizzle applied to fetched values ('xxxw'), or
        #                                             a wrapping expression with {} ('xsrgb({})': an sRGB view's value re-encoded)
        self.colour_scale = colour_scale


def require() -> ModuleType:
    """The Rust core, which translates (bfbc2core.native.require): NativeMissing, saying how to get it, when it is not
    built or downloaded or was built from other sources."""
    root = os.path.dirname(_HERE)
    if root not in sys.path:
        sys.path.insert(0, root)
    import suite_paths
    suite_paths.use('core')
    from bfbc2core import native
    return native.require('The Xbox 360 shader translator (bfbc2-shader-tools/xenos_hlsl.py)', 'xenos_tree_program')


def _program(m: Any, sh: Obj) -> Any:
    """The Rust core's program of a tree: kept from the decode (xenos.parse_shader's untouched tree), else the tree
    handed over attribute by attribute (ValueError for one it cannot hold)."""
    p = xenos.native_program(sh)
    return p if p is not None else m.xenos_tree_program(sh)


def _result(status: int, text: str) -> str | TranslateError:
    if status == 0:
        return text
    if status == 1:
        return TranslateError(text)
    raise ValueError(f'xenos_hlsl.translate: {text}')


def translate(sh: Obj, iface: Interface) -> str:
    """The program as HLSL (entry point 'main'); TranslateError for a program the translator refuses."""
    m = require()
    got = _result(*m.xenos_translate(_program(m, sh), iface))
    if isinstance(got, TranslateError):
        raise got
    return got


def translate_many(jobs: Iterable[tuple[Obj, Interface]], errors: str = 'raise') -> list[Any]:
    """translate of each (shader, iface), in order, on all cores at once (the interpreter released). errors='raise'
    raises the first TranslateError (in input order), 'return' puts each in its place; a ValueError (a tree or
    interface the translator cannot take) is raised either way."""
    m = require()
    out = []
    for got in m.xenos_translate_many([(_program(m, sh), iface) for sh, iface in jobs]):
        r = _result(*got)
        if errors != 'return' and isinstance(r, TranslateError):
            raise r
        out.append(r)
    return out


def helpers() -> str:
    """The HLSL helper functions every translation starts with (xcube, xmul, tsizeLod, xshadowuv, xcubefetch ...):
    tests compile pieces of a translation with them."""
    return str(require().xenos_helpers())
