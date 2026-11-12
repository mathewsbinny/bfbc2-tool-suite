"""Pixel-level regressions for BF1943 texture, mask, emitter and shader fidelity."""
import struct
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np

SUITE = Path(__file__).resolve().parents[1]
if str(SUITE) not in sys.path:
    sys.path.insert(0, str(SUITE))
import suite_paths

suite_paths.use('model-tools', 'console-convert', 'shader-tools', 'bf1943-import', 'core')
import emitteratlas as EA
import pcbuild as P
import renderqa as RQ
import texqa
import texture
import texwrite
import xdiff
import xenos
import xenos_exec as XE
import xenos_hlsl as X


class AssetAccuracy(unittest.TestCase):
    def test_compiled_engine_texture_slots(self):
        # Material t0/t1 must not acquire a donor's shadow/reflection bindings.
        for profile in ('ps_4_0', 'ps_5_0'):
            plain = P.compile_hlsl('Texture2D texture_Texture5:register(t0);'
                                   'float4 main(float4 p:SV_Position):SV_Target {'
                                   'return texture_Texture5.Load(int3(p.xy,0));}', 'main', profile)
            self.assertEqual(P.resource_table_dxbc(plain), bytes(4))
            src = ('Texture2D texture_outdoorLightShadowmapTexture:register(t3);'
                   'TextureCube texture_outdoorLightReflectionEnvmap:register(t7);'
                   'SamplerState sampler1:register(s1);'
                   'float4 main(float4 p:SV_Position):SV_Target {'
                   'return texture_outdoorLightShadowmapTexture.Load(int3(p.xy,0))+'
                   'texture_outdoorLightReflectionEnvmap.SampleLevel(sampler1,float3(p.xy,1),0);}')
            code = P.compile_hlsl(src, 'main', profile)
            table = P.resource_table_dxbc(code)
            self.assertEqual(struct.unpack_from('<I', table)[0], 2)
            self.assertEqual({table[i:i+4] for i in range(4, len(table), 4)},
                             {bytes((38, 0x40, 3, 255)), bytes((39, 0x42, 7, 255))})
        # An optimized-away engine texture must not remain bound over a material.
        unused = P.compile_hlsl('Texture2D texture_outdoorLightShadowmapTexture:register(t0);'
                                'float4 main():SV_Target{return 1;}', 'main', 'ps_5_0')
        self.assertEqual(P.resource_table_dxbc(unused), bytes(4))

    def test_opposing_color_variation(self):
        # This block's red and green vary in opposite directions, orthogonal to (1,1,1).
        img = np.zeros((4, 4, 3), np.uint8)
        img[:2] = [255, 0, 0]
        img[2:] = [0, 255, 0]
        got = texture.decode(texwrite.dxt1(img), 4, 4, 0)
        self.assertLess(float(((img.astype(float) - got) ** 2).mean()), 2)
        rgba = np.concatenate([img, np.full((4, 4, 1), 255, np.uint8)], 2)
        rgba[0, 0, 3] = 0
        data = texwrite.dxt1(rgba)
        got, _ = texqa.decode_level(data, 4, 4, 0)
        self.assertEqual(got[0, 0, 3], 0)
        live = rgba[..., 3] != 0
        self.assertLess(float(((img[live].astype(float) - got[..., :3][live]) ** 2).mean()), 2)

    def test_principal_axis_is_not_largest_diagonal(self):
        # The strongest RGB channel alone can still be a minor eigenvector.
        # Green/blue together carry more variance; a single coordinate seed misses it.
        img = np.array([[128 + r, 128 + g, 128 - g] for r in (-50, 50) for g in (-45, 45)] * 4,
                       np.uint8).reshape(4, 4, 3)
        got = texture.decode(texwrite.dxt1(img), 4, 4, 0)
        self.assertGreaterEqual(np.ptp(got[..., 1]), 80)
        self.assertLess(np.ptp(got[..., 0]), 20)

    def test_mask_alpha_palette(self):
        mask = np.array([0, 255, 40, 80, 120, 160, 200, 240] * 2, np.uint8).reshape(4, 4)
        img = np.full((4, 4, 4), 180, np.uint8)
        img[..., 3] = mask
        encoded = texwrite.dxt5(img)
        self.assertLessEqual(encoded[0], encoded[1])
        np.testing.assert_array_equal(texwrite.alpha5(encoded, 4, 4), mask)
        # Never trade away accuracy when eight-value mode was already exact.
        img[..., 3] = 77
        np.testing.assert_array_equal(texwrite.alpha5(texwrite.dxt5(img), 4, 4), 77)

    def test_null_atlas_references(self):
        info = EA.Info('i', 'TextureInfo1', {}, dict.fromkeys(EA.UV + EA.NM, 0.25),
                       1, 1, False, 'null', 'null')
        self.assertFalse(EA.drawn(info, 'alpha'))
        self.assertFalse(EA.drawn(info, 'normals'))
        info.texture = 'textures/a/00000000-0000-0000-0000-000000000001'
        self.assertTrue(EA.drawn(info, 'alpha'))
        # Older/minimal emitter documents may omit these authoring refs; their UVs still need validation.
        info.texture = info.normalmap = None
        self.assertTrue(EA.drawn(info, 'alpha'))
        self.assertTrue(EA.drawn(info, 'normals'))

    def test_invalid_source_mips(self):
        for fmt in (0, 3, 5, 10):
            for w, h, mips in [(0, 8, 1), (8, 8, 0), (8, 8, 16), (8, 8, 5)]:
                source = struct.pack('>8I', 3, 0, fmt, 0, w, h, 0, mips) + bytes(60)
                with self.subTest(fmt=fmt, w=w, mips=mips), self.assertRaises(texture.ConvertError):
                    texture.convert(source)

    def fetch(self, **overrides):
        values = dict(fetch_kind='texture', op='tfetch2D', has_dest=True, const_index=2,
                      src=0, src_swizzle='xy', dst=1, dst_swizzle='xyzw', address=1,
                      offset_x=0.0, offset_y=0.0, mag_filter='linear', min_filter='linear',
                      use_register_lod=False, use_computed_lod=True, lod_bias=0.0,
                      unnormalized=False, predicated=False)
        values.update(overrides)
        return xenos.Obj(**values)

    def expression(self, ins, pixel=True):
        em = X._Emitter(xenos.Obj(is_pixel=pixel), X.Interface([]))
        em.samplers = {2: ('tx', 'Texture2D', 'sampler_Texture2')}
        em.fetch(ins)
        return '\n'.join(em.lines)

    def test_explicit_sampler_routing_preserves_addressing_for_each_fetch_form(self):
        for overrides, operation in (({}, 'Sample'), ({'lod_bias': 0.5}, 'SampleBias'),
                                     ({'use_computed_lod': False}, 'SampleLevel')):
            em = X._Emitter(xenos.Obj(is_pixel=True), X.Interface([], sampler_registers={2: 3}))
            em.samplers = {2: ('tx', 'Texture2D', 'sampler_Texture2')}
            em.fetch(self.fetch(**overrides))
            code = '\n'.join(em.lines)
            self.assertIn(operation+'(sampler3,', code)
            self.assertNotIn(operation+'(sampler1,', code)
        em = X._Emitter(xenos.Obj(is_pixel=True), X.Interface([], sampler_registers={2: 4}))
        em.samplers = {2: ('tx', 'TextureCube', 'sampler_Texture2')}
        em.cube_dir_set = False
        em.fetch(self.fetch(src_swizzle='xyz'))
        self.assertIn('Sample(sampler4,', '\n'.join(em.lines))

    def test_shader_lod_selection(self):
        fixed = self.expression(self.fetch(use_computed_lod=False, lod_bias=1.5))
        self.assertIn('SampleLevel(sampler1, r0.xy, 1.5)', fixed)
        explicit = self.expression(self.fetch(use_register_lod=True, lod_bias=0.5))
        self.assertIn('SampleLevel(sampler1, r0.xy, (texLod + 0.5))', explicit)
        vertex = self.expression(self.fetch(), pixel=False)
        self.assertIn('SampleLevel', vertex)
        P.compile_hlsl('Texture2D tx; SamplerState sampler1; float4 main(float4 p : POSITION) : SV_Position {'
                       'float4 r0 = p, r1 = 0; float texLod = 0; ' + vertex + 'return r1;}', 'main', 'vs_5_0')
        point = self.expression(self.fetch(use_register_lod=True, mag_filter='point', min_filter='point'))
        self.assertIn('tsizeLod', point)
        self.assertIn('fetchDim1.z', point)
        with self.assertRaises(X.TranslateError):
            self.expression(self.fetch(use_register_gradients=True))

    def test_native_mips_and_vertex_textures(self):
        # Known colors per mip make a wrong mip or an unbound vertex texture unambiguous.
        resource = texwrite.resource(9, 0, 4, 4, [bytes([0, 0, 255, 255]) * 16,
                                                bytes([0, 255, 0, 255]) * 4, bytes([255, 0, 0, 255])])
        vhead = 'struct V2P { float4 pos : SV_Position; float4 t0 : TEXCOORD0; };'
        vs_plain = vhead + ' V2P main(float4 p : TEXCOORD0) { V2P o; o.pos=p; o.t0=float4(.5,.5,0,0); return o; }'
        vs_sample = ('Texture2D tx : register(t2); SamplerState sampler1 : register(s1);' + vhead +
                     ' V2P main(float4 p : TEXCOORD0) { V2P o; o.pos=p; '
                     'o.t0=tx.SampleLevel(sampler1,float2(.5,.5),1); return o; }')
        ps_plain = vhead + ' float4 main(V2P i) : SV_Target0 { return i.t0; }'
        verts = np.array([[-1,-1,.5,1], [-1,3,.5,1], [3,-1,.5,1]], np.float32)
        with tempfile.TemporaryDirectory(prefix='asset mip render ') as tmp:
            def put(name, data):
                path = Path(tmp) / name
                path.write_bytes(data)
                return RQ.job_path(str(path))
            tex = put('texture.res', resource)
            vb = put('vb.bin', verts.tobytes())
            ib = put('ib.bin', np.arange(3, dtype=np.uint32).tobytes())
            desc = bytearray.fromhex(xdiff.sampler_desc())
            struct.pack_into('<f', desc, 48, 2.0)  # Permit the full mip chain, rather than xdiff's level-zero fixture.
            variants = [('vertex', vs_sample, ps_plain, [0,1,0,1])]
            for point in (False, True):
                ins = self.fetch(use_register_lod=True, lod_bias=0.5,
                                 mag_filter='point' if point else 'linear', min_filter='point' if point else 'linear')
                ps = ('Texture2D tx : register(t2); SamplerState sampler1 : register(s1);' +
                      '\n'.join(X.HELPERS) + vhead + ' float4 main(V2P i) : SV_Target0 {'
                      'float4 r0=i.t0,r1=0; float texLod=.5;' + self.expression(ins) + 'return r1; }')
                variants.append((f'pixel_{point}', vs_plain, ps, [0,1,0,1]))
            for name, vs_src, ps_src, expected in variants:
                with self.subTest(stage=name):
                    out = Path(tmp) / (name + '.raw')
                    job = [f'frame {RQ.job_path(str(out))} 4 4 -1 -1 -1 -1',
                           f'vs {put(name + "vs", P.compile_hlsl(vs_src, "main", "vs_5_0"))}',
                           f'ps {put(name + "ps", P.compile_hlsl(ps_src, "main", "ps_5_0"))}',
                           'el 0 2 0 0 0', f'vb {vb} 0 16', f'ib {ib} 0 32 3 0', f'texres 2 {tex}',
                           f'samp 1 {desc.hex()}', 'draw', 'end']
                    path = Path(tmp) / 'job.txt'
                    path.write_text('\n'.join(job))
                    RQ.run_job(str(path), warp=True)
                    np.testing.assert_array_equal(np.fromfile(out, np.float32).reshape(-1,4),
                                                  np.tile(expected, (16,1)))

    def test_composite_visibility_palette_keeps_part_flags(self):
        # Lamppost VS613 gates its vertices on boneMatrices[a0].x, then uses
        # worldMatrix separately for geometry. Visibility must not become the
        # placement rotation's first matrix element.
        for name in ('boneMatrices', 'boneVectors'):
            constant = xenos.Obj(name=name, regset='float4', reg=21, count=180,
                                 type=xenos.Obj(cls='vector', rows=1, cols=4, elements=180))
            shader = xenos.Obj(is_pixel=False, ctab=xenos.Obj(constants=[constant]))
            emitter = X._Emitter(shader, X.Interface([], bones_as_world=True))
            expression = emitter.const(21, 'a0')
            self.assertEqual(expression, f'{name}[a0 + 0]')
            code = (f'float4 {name}[180];float4 worldMatrix[3];'
                    'struct V2P{float4 pos:SV_Position;float4 t0:TEXCOORD0;};'
                    'float4 main(V2P i):SV_Target0{int a0=(int)i.t0.x;'
                    f'float visible=({expression}).x>0.0?1.0:0.0;'
                    'return float4(visible,1.0-visible,0,1);}')
            flags = np.zeros((180, 4), np.float32)
            flags[[0, 2], 0] = 1
            inputs = np.array([[0,0,0,0],[1,0,0,0],[2,0,0,0],[3,0,0,0]], np.float32)
            sh = xenos.Obj(header=xenos.Obj(interpolators=[xenos.Obj(usage_index=0, reg=0)]))
            expected = np.array([[1,0,0,1],[0,1,0,1],[1,0,0,1],[0,1,0,1]], np.float32)
            for rotation in (np.eye(4, dtype=np.float32),
                             np.diag([-1,1,-1,1]).astype(np.float32),
                             np.array([[0,0,1,0],[0,1,0,0],[-1,0,0,0],[0,0,0,1]], np.float32)):
                cs = {'inputs': {0:inputs}, 'engine': {name:flags, 'worldMatrix':rotation}, 'textures':{}}
                with self.subTest(palette=name, world_x=float(rotation[0,0])):
                    with tempfile.TemporaryDirectory(prefix='composite visibility ') as tmp:
                        got, _ = xdiff.run_hlsl(sh, cs, 4, tmp, code)
                    np.testing.assert_array_equal(got, expected)

    def test_composite_matrix_palette_still_uses_pc_world_transform(self):
        constant = xenos.Obj(name='boneMatrices', regset='float4', reg=21, count=180,
                             type=xenos.Obj(cls='matrix_columns', rows=4, cols=3, elements=60))
        shader = xenos.Obj(is_pixel=False, ctab=xenos.Obj(constants=[constant]))
        emitter = X._Emitter(shader, X.Interface([], bones_as_world=True))
        self.assertEqual([emitter.const(21+k, 'a0') for k in range(3)],
                         [f'worldMatrix[{k}]' for k in range(3)])
        self.assertEqual(emitter.const(24, 'a0'), 'boneMatrices[a0 + 3]')
        # Keep the existing matrix adaptation operational, including rotation
        # and translation, while the separate vector visibility test runs.
        code = ('float4 worldMatrix[3];'
                'struct V2P{float4 pos:SV_Position;float4 t0:TEXCOORD0;};'
                'float4 main(V2P i):SV_Target0{return '+emitter.const(21, 'a0')+';}')
        sh = xenos.Obj(header=xenos.Obj(interpolators=[xenos.Obj(usage_index=0, reg=0)]))
        world = np.array([[-1,0,0,3],[0,1,0,4],[0,0,-1,5],[0,0,0,1]], np.float32)
        cs = {'inputs':{0:np.zeros((4,4), np.float32)}, 'engine':{'worldMatrix':world}, 'textures':{}}
        with tempfile.TemporaryDirectory(prefix='composite matrix ') as tmp:
            got, _ = xdiff.run_hlsl(sh, cs, 4, tmp, code)
        np.testing.assert_array_equal(got, np.tile(world[0], (4,1)))

    def test_pc_shadow_cascade_atlas(self):
        ins = self.fetch(src_swizzle_raw=36, mag_filter='point', min_filter='point')
        iface = X.Interface([], pc_shadow_atlas=True)
        em = X._Emitter(xenos.Obj(is_pixel=True), iface)
        em.samplers = {2: ('tx', 'Texture2D', X.SHADOW)}
        em.fetch(ins)
        body = '\n'.join(em.lines)
        self.assertIn('xshadowuv(r0.xyz)', body)
        # An atlas with a unique colour per quadrant makes a wrong cascade
        # observable on the GPU, including the previously discarded selector.
        texels = np.zeros((1, 8, 8, 4), np.float32)
        texels[0, :4, :4] = [1,0,0,1]
        texels[0, :4, 4:] = [0,1,0,1]
        texels[0, 4:, :4] = [0,0,1,1]
        texels[0, 4:, 4:] = [1,1,0,1]
        uv = np.array([[.5,.5,1/6,0], [.5,.5,.5,0], [.5,.5,5/6,0], [.5,.5,1/6,0]], np.float32)
        code = ('Texture2D tx:register(t2); SamplerState sampler1:register(s1);\n' +
                '\n'.join(X.HELPERS) +
                '\nstruct V2P {float4 pos:SV_Position;float4 t0:TEXCOORD0;};'
                'float4 main(V2P i):SV_Target0 {float4 r0=i.t0,r1=0;float texLod=0;'+body+'return r1;}')
        sh = xenos.Obj(header=xenos.Obj(interpolators=[xenos.Obj(usage_index=0,reg=0)]))
        cs = {'inputs': {0:uv}, 'engine':{}, 'textures':{2:XE.Texture(texels,address='clamp',point=True)}}
        # Real Wake shaders select xyz, yzw, xyw and xzw. The layer must
        # follow the encoded third component, rather than a fixed .z.
        for raw in (36, 57, 52, 56):
            sw = [(raw >> (2*k)) & 3 for k in range(3)]
            coords = np.zeros_like(uv)
            coords[:, sw] = uv[:, :3]
            variant = code.replace('xshadowuv(r0.xyz)',
                                   'xshadowuv(r0.' + ''.join('xyzw'[k] for k in sw) + ')')
            cs['inputs'] = {0: coords}
            with tempfile.TemporaryDirectory(prefix='shadow atlas ') as tmp:
                got, _ = xdiff.run_hlsl(sh,cs,4,tmp,variant)
            np.testing.assert_array_equal(got,np.array([[1,0,0,1],[0,1,0,1],[0,0,1,1],[1,0,0,1]]))
        # The raw console translation remains available for differential tests.
        default = X._Emitter(xenos.Obj(is_pixel=True), X.Interface([]))
        default.samplers = em.samplers
        default.fetch(ins)
        self.assertNotIn('xshadowuv', '\n'.join(default.lines))

    def test_unnormalized_fetch_pixels(self):
        ins = self.fetch(unnormalized=True, use_computed_lod=False, mag_filter='point', min_filter='point')
        body = self.expression(ins)
        code = ('Texture2D tx : register(t2); SamplerState sampler1 : register(s1);\n' + '\n'.join(X.HELPERS) +
                '\nstruct V2P { float4 pos : SV_Position; float4 t0 : TEXCOORD0; };\n'
                'float4 main(V2P i) : SV_Target0 { float4 r0 = i.t0, r1 = 0; float texLod = 0;\n' + body +
                '\nreturn r1; }')
        texels = np.array([[[[1, 0, 0, 1], [0, 1, 0, 1], [0, 0, 1, 1], [1, 1, 1, 1]]]], np.float32)
        uv = np.array([[0.5, 0.5, 0, 0], [1.5, 0.5, 0, 0], [2.5, 0.5, 0, 0], [3.5, 0.5, 0, 0]], np.float32)
        sh = xenos.Obj(header=xenos.Obj(interpolators=[xenos.Obj(usage_index=0, reg=0)]))
        cs = {'inputs': {0: uv}, 'engine': {}, 'textures': {2: XE.Texture(texels, address='clamp', point=True)}}
        with tempfile.TemporaryDirectory(prefix='asset accuracy ') as tmp:
            got, _ = xdiff.run_hlsl(sh, cs, 4, tmp, code)
        np.testing.assert_array_equal(got, texels.reshape(4, 4))
        machine = XE._Machine(xenos.Obj(is_pixel=True, header=xenos.Obj(program_cntl=(15 << 8)), literals={}),
                              4, {}, cs['textures'])
        machine.r[0] = uv
        machine.fetch(ins, np.ones(4, bool))
        np.testing.assert_array_equal(machine.r[1], texels.reshape(4, 4))


if __name__ == '__main__':
    unittest.main()
