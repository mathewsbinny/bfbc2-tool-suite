//! The live draw list on the device (round R5c): each (mesh, draw) resolved once into its GPU objects (a material),
//! the frame's constant buffers uploaded with one `Map` into a ring bound by offset (Direct3D 11.1
//! `VSSetConstantBuffers1`; without it, one `UpdateSubresource` per draw as before), and the draws submitted in the
//! list's order binding only what changed since the last draw - the bound state at each draw is the one
//! [`Device::render`] sets, so the picture is the same.
//!
//! Also here: picking by replaying the frame's draws into the stencil (the object's number, a byte a replay, at one
//! pixel: DICE's own shaders decide coverage, discards included), the selection mask and outline drawn on the game
//! frame with its depth, and the finished frame shared with the editor's graphics device (an NT handle to a
//! `R8G8B8A8_UNORM_SRGB` texture; no read back).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use windows::Win32::Foundation::{CloseHandle, HANDLE};
use windows::Win32::Graphics::Direct3D::Fxc::D3DCompile;
use windows::Win32::Graphics::Direct3D::ID3DBlob;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_INDEX_BUFFER, D3D11_BIND_RENDER_TARGET, D3D11_BIND_SHADER_RESOURCE,
    D3D11_BIND_VERTEX_BUFFER, D3D11_BUFFER_DESC, D3D11_CLEAR_DEPTH, D3D11_CLEAR_STENCIL, D3D11_CPU_ACCESS_WRITE,
    D3D11_FEATURE_D3D11_OPTIONS, D3D11_FEATURE_DATA_D3D11_OPTIONS, D3D11_MAP_READ, D3D11_MAP_WRITE_DISCARD,
    D3D11_MAPPED_SUBRESOURCE, D3D11_QUERY_DESC, D3D11_QUERY_EVENT, D3D11_RESOURCE_MISC_SHARED,
    D3D11_RESOURCE_MISC_SHARED_NTHANDLE, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT, D3D11_USAGE_DYNAMIC,
    D3D11_VIEWPORT, ID3D11BlendState, ID3D11Buffer, ID3D11DepthStencilState, ID3D11DeviceContext1, ID3D11InputLayout,
    ID3D11PixelShader, ID3D11Query, ID3D11RasterizerState, ID3D11SamplerState, ID3D11ShaderResourceView,
    ID3D11Texture2D, ID3D11VertexShader,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_R8G8B8A8_UNORM_SRGB, DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R32_UINT, DXGI_SAMPLE_DESC,
};
use windows::Win32::Graphics::Dxgi::{DXGI_SHARED_RESOURCE_READ, DXGI_SHARED_RESOURCE_WRITE, IDXGIResource1};
use windows::core::{Interface, PCSTR, PCWSTR, s};

use super::{Device, err, some, state_keys};
use crate::live::{Live, Step};
use crate::material::{Prepared, TextureFiles};
use crate::post::Pass;
use crate::terrain::RawTexture;

/// What a submit draws for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// The picture.
    Normal,
    /// Picking: no colour; byte `k` of each draw's pick number into the stencil where it keeps the pixel (draws that
    /// write no depth leave the stencil alone), inside the scissor.
    Pick(u8),
    /// The selection mask: no colour, no depth written; stencil bit 0x80 where the draw lies at the frame's depth
    /// (seen), 0x40 where it lies behind it (hidden).
    Mask(bool),
}

/// One (mesh, draw)'s GPU objects and bindings.
struct Material {
    _keep: Arc<Prepared>,
    vs: ID3D11VertexShader,
    ps: ID3D11PixelShader,
    layout: ID3D11InputLayout,
    vb: [Option<ID3D11Buffer>; 1],
    stride: u32,
    offset: u32,
    ib: ID3D11Buffer,
    format: DXGI_FORMAT,
    index_count: u32,
    first_index: u32,
    ps_srv: [Option<ID3D11ShaderResourceView>; 32],
    vs_srv: [Option<ID3D11ShaderResourceView>; 32],
    ps_ss: [Option<ID3D11SamplerState>; 16],
    vs_ss: [Option<ID3D11SamplerState>; 16],
    /// Textures the frame makes (the scene's linear depth ...), looked up at each draw: (vertex stage, slot, key).
    frame_textures: Vec<(bool, u32, String)>,
    keys: ([u32; 9], [u32; 3], u32),
    states: (ID3D11BlendState, ID3D11DepthStencilState, ID3D11RasterizerState),
    class: u8,
}

/// A texture shared with another device.
struct Shared {
    tex: ID3D11Texture2D,
    handle: HANDLE,
}

/// The device's live state.
#[derive(Default)]
pub(super) struct Gpu {
    mats: Vec<Material>,
    ids: HashMap<(usize, u16), u32>,
    /// `Some(None)`: probed, no constant-buffer offsets.
    ctx1: Option<Option<ID3D11DeviceContext1>>,
    ring: Option<ID3D11Buffer>,
    ring_size: usize,
    shared: Vec<Shared>,
    shared_size: [u32; 2],
    shared_next: usize,
    shared_generation: u32,
    outline: Option<(ID3D11VertexShader, ID3D11PixelShader)>,
    query: Option<ID3D11Query>,
}

impl Drop for Gpu {
    fn drop(&mut self) {
        for s in &self.shared {
            // SAFETY: a handle this device made and still owns
            unsafe { CloseHandle(s.handle).ok() };
        }
    }
}

/// What a live frame cost.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct LiveStats {
    /// Draw calls made.
    pub draws: usize,
    /// Materials made this frame (first use).
    pub made: usize,
    /// Seconds making materials.
    pub making: f64,
    /// Seconds uploading the constant buffers.
    pub upload: f64,
    /// Seconds making the draw calls.
    pub submit: f64,
    /// Seconds submitting the post-process.
    pub post: f64,
}

/// The finished frame in a texture another device opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SharedFrame {
    /// The NT handle (`IDXGIResource1::CreateSharedHandle`), valid while the device lives.
    pub handle: isize,
    /// Which of the textures (three, used in turn) holds this frame.
    pub slot: usize,
    /// Pixels.
    pub size: [u32; 2],
    /// Changes when the textures are made again (a new size): the other device opens them again.
    pub generation: u32,
}

const NONE: usize = usize::MAX;

/// The raw address of a bound object (0: none).
fn addr<T: Interface>(o: Option<&T>) -> usize {
    o.map_or(0, |x| x.as_raw() as usize)
}

/// What is bound, as addresses ([`NONE`]: not known).
struct Bound {
    layout: usize,
    vb: (usize, u32, u32),
    ib: (usize, i32),
    vs: usize,
    ps: usize,
    cbv: (usize, u32, u32),
    cbp: (usize, u32, u32),
    pss: [usize; 16],
    vss: [usize; 16],
    psr: [usize; 32],
    vsr: [usize; 32],
    blend: usize,
    depth: (usize, u32),
    raster: usize,
}

impl Bound {
    /// After `ClearState`: nothing.
    fn cleared() -> Self {
        Self {
            layout: 0,
            vb: (0, 0, 0),
            ib: (0, 0),
            vs: 0,
            ps: 0,
            cbv: (0, 0, 0),
            cbp: (0, 0, 0),
            pss: [0; 16],
            vss: [0; 16],
            psr: [0; 32],
            vsr: [0; 32],
            blend: 0,
            depth: (0, 0),
            raster: 0,
        }
    }

    /// Not known (after a pass bound its own).
    fn unknown() -> Self {
        Self {
            layout: NONE,
            vb: (NONE, 0, 0),
            ib: (NONE, 0),
            vs: NONE,
            ps: NONE,
            cbv: (NONE, 0, 0),
            cbp: (NONE, 0, 0),
            pss: [NONE; 16],
            vss: [NONE; 16],
            psr: [NONE; 32],
            vsr: [NONE; 32],
            blend: NONE,
            depth: (NONE, 0),
            raster: NONE,
        }
    }
}

/// The range of slots whose binding differs, recording the new ones.
fn changed<const N: usize>(last: &mut [usize; N], now: &[usize; N]) -> Option<(usize, usize)> {
    let lo = (0..N).find(|&i| last[i] != now[i])?;
    let hi = (0..N).rfind(|&i| last[i] != now[i])?;
    last.copy_from_slice(now);
    Some((lo, hi))
}

fn addrs<T: Interface, const N: usize>(a: &[Option<T>; N]) -> [usize; N] {
    std::array::from_fn(|i| addr(a[i].as_ref()))
}

/// A texture the frame makes (its view changes with the frame's size), not a file.
fn frame_texture(name: &str) -> bool {
    name == crate::water::DEPTH || name.starts_with("post:") || name.starts_with("scene:")
}

const OUTLINE_HLSL: &str = r"
Texture2D<uint2> stencil : register(t0);
cbuffer C : register(b0) { float4 seen; float4 hidden; };
float4 vs(uint id : SV_VertexID) : SV_Position {
    float2 p = float2((id << 1) & 2, id & 2);
    return float4(p * float2(2, -2) + float2(-1, 1), 0, 1);
}
float4 ps(float4 pos : SV_Position) : SV_Target {
    int2 c = int2(pos.xy);
    uint here = stencil.Load(int3(c, 0)).g;
    if (here & 0x80) discard;
    uint near = 0;
    [unroll] for (int y = -2; y <= 2; y++) {
        [unroll] for (int x = -2; x <= 2; x++) {
            near |= stencil.Load(int3(c + int2(x, y), 0)).g;
        }
    }
    if (near & 0x80) return seen;
    if ((near & 0x40) && !(here & 0x40)) return hidden;
    discard;
    return seen;
}
";

fn compile(entry: PCSTR, target: PCSTR) -> Result<Vec<u8>, String> {
    let mut code: Option<ID3DBlob> = None;
    let mut errors: Option<ID3DBlob> = None;
    // SAFETY: the source outlives the call; the blobs are read within their own sizes
    unsafe {
        let r = D3DCompile(
            OUTLINE_HLSL.as_ptr().cast(),
            OUTLINE_HLSL.len(),
            PCSTR::null(),
            None,
            None,
            entry,
            target,
            0,
            0,
            &raw mut code,
            Some(&raw mut errors),
        );
        if let Err(e) = r {
            let msg = errors.map_or_else(String::new, |b| {
                String::from_utf8_lossy(std::slice::from_raw_parts(
                    b.GetBufferPointer().cast::<u8>(),
                    b.GetBufferSize(),
                ))
                .into_owned()
            });
            return Err(format!("D3DCompile (selection outline): {e} {msg}"));
        }
        let b = some(code, "D3DCompile")?;
        Ok(std::slice::from_raw_parts(b.GetBufferPointer().cast::<u8>(), b.GetBufferSize()).to_vec())
    }
}

impl Device {
    /// Direct3D 11.1's context when it binds constant buffers by offset.
    fn ctx1(&mut self) -> Option<ID3D11DeviceContext1> {
        if self.gpu.ctx1.is_none() {
            let mut o = D3D11_FEATURE_DATA_D3D11_OPTIONS::default();
            // SAFETY: the out structure is the feature's own, of its size
            let ok = unsafe {
                self.device.CheckFeatureSupport(
                    D3D11_FEATURE_D3D11_OPTIONS,
                    (&raw mut o).cast(),
                    size_of::<D3D11_FEATURE_DATA_D3D11_OPTIONS>() as u32,
                )
            }
            .is_ok()
                && o.ConstantBufferOffsetting.as_bool()
                && std::env::var_os("BC2_RENDER_NO_CB_OFFSETS").is_none();
            self.gpu.ctx1 = Some(if ok { self.ctx.cast().ok() } else { None });
        }
        self.gpu.ctx1.clone().flatten()
    }

    /// Whether constant buffers are bound by offset in one ring (else one update per draw).
    pub fn ring_constants(&mut self) -> bool {
        self.ctx1().is_some()
    }

    /// The material of an instance's draw, made on first use.
    fn material(
        &mut self,
        p: &Arc<Prepared>,
        k: u16,
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<u32, String> {
        let key = (Arc::as_ptr(p) as usize, k);
        if let Some(&id) = self.gpu.ids.get(&key) {
            return Ok(id);
        }
        let d = some(p.draws.get(usize::from(k)), "draw")?;
        let (vs, ps, layout) = self.shaders_of(&d.vs, &d.ps, &d.elements)?;
        let vb = self.buffer(&p.vertices, D3D11_BIND_VERTEX_BUFFER.0 as u32)?;
        let ib = self.buffer(&p.indices, D3D11_BIND_INDEX_BUFFER.0 as u32)?;
        let mut ps_ss: [Option<ID3D11SamplerState>; 16] = Default::default();
        ps_ss[0] = Some(self.defaults[0].clone());
        ps_ss[1] = Some(self.defaults[1].clone());
        for (reg, desc) in &d.samplers {
            if let Some(slot) = ps_ss.get_mut(*reg as usize) {
                *slot = self.sampler(desc);
            }
        }
        let mut vs_ss = ps_ss.clone();
        for (reg, desc) in &d.vs_samplers {
            if let Some(slot) = vs_ss.get_mut(*reg as usize) {
                *slot = self.sampler(desc);
            }
        }
        let mut frame_textures = Vec::new();
        let mut ps_srv: [Option<ID3D11ShaderResourceView>; 32] = Default::default();
        ps_srv[0] = Some(self.shadow.clone());
        for (slot, name) in &d.textures {
            if let Some(s) = ps_srv.get_mut(*slot as usize) {
                if frame_texture(name) {
                    *s = None;
                    frame_textures.push((false, *slot, name.clone()));
                    frame_textures.push((true, *slot, name.clone()));
                } else {
                    *s = self.view(name, files, raw);
                }
            }
        }
        let mut vs_srv = ps_srv.clone();
        for (slot, name) in &d.vs_textures {
            if let Some(s) = vs_srv.get_mut(*slot as usize) {
                frame_textures.retain(|(v, sl, _)| !(*v && sl == slot));
                if frame_texture(name) {
                    *s = None;
                    frame_textures.push((true, *slot, name.clone()));
                } else {
                    *s = self.view(name, files, raw);
                }
            }
        }
        let keys = state_keys(d.state.as_ref());
        let states = self.keyed_states(keys.0, keys.1, keys.2)?;
        let id = self.gpu.mats.len() as u32;
        self.gpu.mats.push(Material {
            _keep: Arc::clone(p),
            vs,
            ps,
            layout,
            vb: [Some(vb)],
            stride: d.stride,
            offset: d.vertex_offset,
            ib,
            format: if p.wide { DXGI_FORMAT_R32_UINT } else { DXGI_FORMAT_R16_UINT },
            index_count: d.index_count,
            first_index: d.first_index,
            ps_srv,
            vs_srv,
            ps_ss,
            vs_ss,
            frame_textures,
            keys,
            states,
            class: d.class,
        });
        self.gpu.ids.insert(key, id);
        Ok(id)
    }

    /// Materials kept (each mesh draw's GPU objects).
    #[must_use]
    pub fn material_count(&self) -> usize {
        self.gpu.mats.len()
    }

    /// Drops the materials (and with them the meshes they keep alive).
    pub fn forget_materials(&mut self) {
        self.gpu.mats.clear();
        self.gpu.ids.clear();
    }

    /// The frame's constants into the ring (one `Map`); false without constant-buffer offsets.
    fn upload(&mut self, arena: &[u8]) -> Result<bool, String> {
        if self.ctx1().is_none() {
            return Ok(false);
        }
        let need = arena.len().max(256);
        if self.gpu.ring.is_none() || self.gpu.ring_size < need {
            let size = need.next_power_of_two().max(1 << 20);
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: size as u32,
                Usage: D3D11_USAGE_DYNAMIC,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                CPUAccessFlags: D3D11_CPU_ACCESS_WRITE.0 as u32,
                ..Default::default()
            };
            let mut b = None;
            // SAFETY: a valid description; no initial data
            unsafe { self.device.CreateBuffer(&raw const desc, None, Some(&raw mut b)) }
                .map_err(err("CreateBuffer (constant ring)"))?;
            self.gpu.ring = Some(some(b, "constant ring")?);
            self.gpu.ring_size = size;
        }
        let ring = some(self.gpu.ring.clone(), "ring")?;
        let mut m = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the ring is a dynamic buffer of `ring_size` >= arena.len() bytes, mapped for writing
        unsafe {
            self.ctx
                .Map(&ring, 0, D3D11_MAP_WRITE_DISCARD, 0, Some(&raw mut m))
                .map_err(err("Map (constant ring)"))?;
            std::ptr::copy_nonoverlapping(arena.as_ptr(), m.pData.cast::<u8>(), arena.len());
            self.ctx.Unmap(&ring, 0);
        }
        Ok(true)
    }

    /// Makes the materials of the steps (first use) and returns their ids.
    fn materials_of(
        &mut self,
        live: &Live,
        steps: &[Step],
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<(Vec<u32>, usize), String> {
        let before = self.gpu.mats.len();
        let mut out = Vec::with_capacity(steps.len());
        for s in steps {
            let inst = some(live.instances.get(s.inst as usize), "instance")?;
            out.push(self.material(&inst.source.prepared, s.draw, files, raw)?);
        }
        Ok((out, self.gpu.mats.len() - before))
    }

    /// Submits steps (the scene's targets bound), the depth pass before step `depth.0`.
    #[allow(clippy::too_many_arguments, reason = "a submit's inputs")]
    fn submit(
        &mut self,
        live: &Live,
        steps: &[Step],
        mats: &[u32],
        ring: bool,
        mode: Mode,
        depth: Option<(usize, &Pass)>,
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<usize, String> {
        let ctx = self.ctx.clone();
        let ctx1 = if ring { self.ctx1() } else { None };
        let ring_buf: [Option<ID3D11Buffer>; 1] = [self.gpu.ring.clone()];
        let mut last = Bound::cleared();
        let mut drawn = 0;
        let mut depth_done = depth.is_none();
        let arena = &live.built.arena;
        for (n, (s, &m)) in steps.iter().zip(mats).enumerate() {
            if let Some((at, p)) = depth
                && !depth_done
                && n >= at
            {
                depth_done = true;
                self.run_pass(p, files, raw)?;
                self.bind_scene()?;
                last = Bound::unknown();
            }
            // the mode's states
            let (keys, class) = {
                let mat = some(self.gpu.mats.get(m as usize), "material")?;
                (mat.keys, mat.class)
            };
            let pick = live.instances.get(s.inst as usize).map_or(0, |i| i.source.pick);
            let (states, sref) = match mode {
                Mode::Normal => (None, u32::from(class)),
                Mode::Pick(k) => {
                    let mut b = keys.0;
                    b[8] = 0;
                    let d = keys.1;
                    let st = self.keyed_states_ex(b, [d[0], d[1], d[2], if d[1] != 0 { 0xff } else { 0 }], keys.2 | 0x100)?;
                    (Some(st), (pick >> (8 * u32::from(k))) & 0xff)
                }
                Mode::Mask(seen) => {
                    let mut b = keys.0;
                    b[8] = 0;
                    let func = if seen { 3 } else { 5 }; // EQUAL, GREATER
                    let st = self.keyed_states_ex(b, [1, 0, func, if seen { 0x80 } else { 0x40 }], keys.2)?;
                    (Some(st), 0xff)
                }
            };
            // SAFETY: the ring holds the frame's constants at the steps' offsets (uploaded before this submit); the
            // fallback's buffers are this device's
            unsafe {
            for (stage, (off, len)) in [(0u8, s.vs), (1, s.ps)] {
                match &ctx1 {
                    Some(c1) => {
                        let first = off / 16;
                        let num = len.div_ceil(256).max(1) * 16;
                        let key = (addr(ring_buf[0].as_ref()), first, num);
                        let slot = if stage == 0 { &mut last.cbv } else { &mut last.cbp };
                        if *slot != key {
                            if stage == 0 {
                                c1.VSSetConstantBuffers1(
                                    0,
                                    1,
                                    Some(ring_buf.as_ptr()),
                                    Some(&raw const first),
                                    Some(&raw const num),
                                );
                            } else {
                                c1.PSSetConstantBuffers1(
                                    0,
                                    1,
                                    Some(ring_buf.as_ptr()),
                                    Some(&raw const first),
                                    Some(&raw const num),
                                );
                            }
                            *slot = key;
                        }
                    }
                    None => {
                        let data = arena.get(off as usize..(off + len) as usize).unwrap_or_default();
                        let b = self.cbuffer(data, stage)?;
                        if stage == 0 {
                            ctx.VSSetConstantBuffers(0, Some(&[b]));
                            last.cbv = (NONE, 0, 0);
                        } else {
                            ctx.PSSetConstantBuffers(0, Some(&[b]));
                            last.cbp = (NONE, 0, 0);
                        }
                    }
                }
            }
            }
            let mat = some(self.gpu.mats.get(m as usize), "material")?;
            let (blend, dss, raster) = states.as_ref().unwrap_or(&mat.states);
            // the frame's own textures, looked up now
            let mut psr_local;
            let mut vsr_local;
            let (psr, vsr) = if mat.frame_textures.is_empty() {
                (&mat.ps_srv, &mat.vs_srv)
            } else {
                psr_local = mat.ps_srv.clone();
                vsr_local = mat.vs_srv.clone();
                for (vstage, slot, name) in &mat.frame_textures {
                    let v = match name.as_str() {
                        crate::post::SCENE => self.target.as_ref().map(|t| t.rt_srv.clone()),
                        crate::post::SCENE_DEPTH => self.target.as_ref().map(|t| t.ds_srv.clone()),
                        _ => self.passes.get(name).map(|t| t.srv.clone()),
                    };
                    let arr = if *vstage { &mut vsr_local } else { &mut psr_local };
                    if let Some(x) = arr.get_mut(*slot as usize) {
                        *x = v;
                    }
                }
                (&psr_local, &vsr_local)
            };
            // SAFETY: objects made by this device; every array outlives its call; the ring holds the frame's
            // constants at the steps' offsets (uploaded before this submit)
            unsafe {
                let a = addr(Some(&mat.layout));
                if last.layout != a {
                    ctx.IASetInputLayout(&mat.layout);
                    last.layout = a;
                }
                let v = (addr(mat.vb[0].as_ref()), mat.stride, mat.offset);
                if last.vb != v {
                    ctx.IASetVertexBuffers(
                        0,
                        1,
                        Some(mat.vb.as_ptr()),
                        Some(&raw const mat.stride),
                        Some(&raw const mat.offset),
                    );
                    last.vb = v;
                }
                let i = (addr(Some(&mat.ib)), mat.format.0);
                if last.ib != i {
                    ctx.IASetIndexBuffer(&mat.ib, mat.format, 0);
                    last.ib = i;
                }
                let a = addr(Some(&mat.vs));
                if last.vs != a {
                    ctx.VSSetShader(&mat.vs, None);
                    last.vs = a;
                }
                let a = addr(Some(&mat.ps));
                if last.ps != a {
                    ctx.PSSetShader(&mat.ps, None);
                    last.ps = a;
                }
                if let Some((lo, hi)) = changed(&mut last.pss, &addrs(&mat.ps_ss)) {
                    ctx.PSSetSamplers(lo as u32, Some(&mat.ps_ss[lo..=hi]));
                }
                if let Some((lo, hi)) = changed(&mut last.vss, &addrs(&mat.vs_ss)) {
                    ctx.VSSetSamplers(lo as u32, Some(&mat.vs_ss[lo..=hi]));
                }
                if let Some((lo, hi)) = changed(&mut last.vsr, &addrs(vsr)) {
                    ctx.VSSetShaderResources(lo as u32, Some(&vsr[lo..=hi]));
                }
                if let Some((lo, hi)) = changed(&mut last.psr, &addrs(psr)) {
                    ctx.PSSetShaderResources(lo as u32, Some(&psr[lo..=hi]));
                }
                let a = addr(Some(blend));
                if last.blend != a {
                    ctx.OMSetBlendState(blend, None, 0xffff_ffff);
                    last.blend = a;
                }
                let dk = (addr(Some(dss)), sref);
                if last.depth != dk {
                    ctx.OMSetDepthStencilState(dss, sref);
                    last.depth = dk;
                }
                let a = addr(Some(raster));
                if last.raster != a {
                    ctx.RSSetState(raster);
                    last.raster = a;
                }
                ctx.DrawIndexed(mat.index_count, mat.first_index, 0);
            }
            drawn += 1;
        }
        if let Some((_, p)) = depth
            && !depth_done
        {
            // every reader of the depth was culled: the pass still runs where the whole list has it (after these)
            self.run_pass(p, files, raw)?;
            self.bind_scene()?;
        }
        Ok(drawn)
    }

    /// Makes the scene's targets of `size` (again) when needed, clears them and binds them.
    fn begin(&mut self, size: [u32; 2], clear: &[f32; 4]) -> Result<(), String> {
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            self.target = Some(self.make_target(size)?);
        }
        let t = some(self.target.as_ref(), "target")?;
        // SAFETY: views made by this device
        unsafe {
            self.ctx.ClearState();
            self.ctx.ClearRenderTargetView(&t.rtv, clear);
            self.ctx
                .ClearDepthStencilView(&t.dsv, D3D11_CLEAR_DEPTH.0 | D3D11_CLEAR_STENCIL.0, 1.0, 0);
        }
        self.bind_scene()
    }

    /// Draws the live list's last built frame ([`Live::build`]) and its post-process, as [`Device::render`] draws
    /// the same draws (read the result with [`Device::read_scene`], [`Device::read_output`] or [`Device::share`]).
    ///
    /// # Errors
    /// A Direct3D call failed.
    #[allow(clippy::too_many_arguments, reason = "a frame's inputs")]
    pub fn render_live(
        &mut self,
        live: &Live,
        size: [u32; 2],
        clear: [f32; 4],
        depth_pass: Option<&Pass>,
        post: &[Pass],
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<LiveStats, String> {
        let size = [size[0].max(1), size[1].max(1)];
        let t0 = Instant::now();
        let steps = &live.built.steps;
        let (mats, made) = self.materials_of(live, steps, files, raw)?;
        let making = t0.elapsed().as_secs_f64();
        let t1 = Instant::now();
        let ring = self.upload(&live.built.arena)?;
        let upload = t1.elapsed().as_secs_f64();
        let t2 = Instant::now();
        self.begin(size, &clear)?;
        let depth = depth_pass.and_then(|p| live.built.depth_at.map(|at| (at, p)));
        let draws = self.submit(live, steps, &mats, ring, Mode::Normal, depth, files, raw)?;
        let submit = t2.elapsed().as_secs_f64();
        let t3 = Instant::now();
        for p in post {
            self.run_pass(p, files, raw)?;
        }
        Ok(LiveStats {
            draws,
            made,
            making,
            upload,
            submit,
            post: t3.elapsed().as_secs_f64(),
        })
    }

    /// Waits until the device has finished everything sent (an event query): the GPU's own time.
    ///
    /// # Errors
    /// A Direct3D call failed.
    pub fn finish(&mut self) -> Result<(), String> {
        if self.gpu.query.is_none() {
            let d = D3D11_QUERY_DESC {
                Query: D3D11_QUERY_EVENT,
                MiscFlags: 0,
            };
            let mut q = None;
            // SAFETY: a valid description
            unsafe { self.device.CreateQuery(&raw const d, Some(&raw mut q)) }.map_err(err("CreateQuery"))?;
            self.gpu.query = q;
        }
        let q = some(self.gpu.query.clone(), "query")?;
        // SAFETY: an event query of this device; its data is one BOOL
        unsafe {
            self.ctx.End(&q);
            self.ctx.Flush();
            loop {
                let mut done = 0i32;
                let r = self.ctx.GetData(&q, Some((&raw mut done).cast()), 4, 0);
                if r.is_err() || done != 0 {
                    break;
                }
                std::thread::yield_now();
            }
        }
        Ok(())
    }

    /// The scene's colour, depth and classes, and a pass's RGBA8 output (`ldr`: its key), read back (proofs).
    ///
    /// # Errors
    /// A Direct3D call failed.
    pub fn read_scene(&mut self, ldr: Option<&Pass>) -> Result<crate::frame::Rendered, String> {
        let ldr = match ldr {
            Some(p) => self.read_rgba8(p)?,
            None => Vec::new(),
        };
        let t = some(self.target.as_ref(), "target")?;
        let [w, h] = t.size;
        let (w, h) = (w as usize, h as usize);
        let mut out = crate::frame::Rendered {
            width: t.size[0],
            height: t.size[1],
            hdr: Vec::with_capacity(w * h * 4),
            depth: Vec::with_capacity(w * h),
            classes: Vec::with_capacity(w * h),
            ldr,
            ..Default::default()
        };
        // SAFETY: staging copies of this device's targets, mapped for reading; rows read within RowPitch
        unsafe {
            self.ctx.CopyResource(&t.rt_read, &t.rt);
            self.ctx.CopyResource(&t.ds_read, &t.ds);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx
                .Map(&t.rt_read, 0, D3D11_MAP_READ, 0, Some(&raw mut m))
                .map_err(err("Map"))?;
            let base: *const f32 = m.pData.cast();
            for y in 0..h {
                out.hdr
                    .extend_from_slice(std::slice::from_raw_parts(base.add(y * m.RowPitch as usize / 4), w * 4));
            }
            self.ctx.Unmap(&t.rt_read, 0);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            self.ctx
                .Map(&t.ds_read, 0, D3D11_MAP_READ, 0, Some(&raw mut m))
                .map_err(err("Map (depth)"))?;
            let base: *const u8 = m.pData.cast();
            for y in 0..h {
                let row = std::slice::from_raw_parts(base.add(y * m.RowPitch as usize), w * 8);
                for px in row.as_chunks::<8>().0 {
                    out.depth.push(f32::from_le_bytes([px[0], px[1], px[2], px[3]]));
                    out.classes.push(px[4]);
                }
            }
            self.ctx.Unmap(&t.ds_read, 0);
        }
        out.draws = 0;
        Ok(out)
    }

    /// A pass's RGBA8 output read back (the copy fallback of the shared texture).
    ///
    /// # Errors
    /// A Direct3D call failed.
    pub fn read_output(&mut self, p: &Pass) -> Result<Vec<u8>, String> {
        self.read_rgba8(p)
    }

    /// The pick number drawn at pixel `at` of the last built frame (0: nothing pickable): the frame's draws replayed
    /// into the stencil, a byte of the number a replay, inside a one-pixel scissor. The scene's colour is kept; its
    /// depth and stencil are not (draw the frame again before reading them).
    ///
    /// # Errors
    /// A Direct3D call failed.
    pub fn pick(
        &mut self,
        live: &Live,
        size: [u32; 2],
        at: [u32; 2],
        depth_pass: Option<&Pass>,
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<u32, String> {
        let size = [size[0].max(1), size[1].max(1)];
        if at[0] >= size[0] || at[1] >= size[1] {
            return Ok(0);
        }
        let most = live.instances.iter().map(|i| i.source.pick).max().unwrap_or(0);
        let rounds = (32 - most.leading_zeros()).div_ceil(8).max(1) as u8;
        let steps = &live.built.steps;
        let (mats, _) = self.materials_of(live, steps, files, raw)?;
        let ring = self.upload(&live.built.arena)?;
        let mut id = 0u32;
        for k in 0..rounds {
            if self.target.as_ref().is_none_or(|t| t.size != size) {
                self.target = Some(self.make_target(size)?);
            }
            let t = some(self.target.as_ref(), "target")?;
            // SAFETY: views made by this device; the scissor rectangle outlives the call
            unsafe {
                self.ctx.ClearState();
                self.ctx
                    .ClearDepthStencilView(&t.dsv, D3D11_CLEAR_DEPTH.0 | D3D11_CLEAR_STENCIL.0, 1.0, 0);
            }
            self.bind_scene()?;
            let rect = windows::Win32::Foundation::RECT {
                left: at[0] as i32,
                top: at[1] as i32,
                right: at[0] as i32 + 1,
                bottom: at[1] as i32 + 1,
            };
            // SAFETY: as above
            unsafe { self.ctx.RSSetScissorRects(Some(&[rect])) };
            let depth = depth_pass.and_then(|p| live.built.depth_at.map(|a| (a, p)));
            self.submit(live, steps, &mats, ring, Mode::Pick(k), depth, files, raw)?;
            let t = some(self.target.as_ref(), "target")?;
            // SAFETY: a staging copy of the depth-stencil, mapped for reading; one texel read inside the mapped rows
            unsafe {
                self.ctx.CopyResource(&t.ds_read, &t.ds);
                let mut m = D3D11_MAPPED_SUBRESOURCE::default();
                self.ctx
                    .Map(&t.ds_read, 0, D3D11_MAP_READ, 0, Some(&raw mut m))
                    .map_err(err("Map (pick)"))?;
                let p: *const u8 = m.pData.cast();
                let b = *p.add(at[1] as usize * m.RowPitch as usize + at[0] as usize * 8 + 4);
                self.ctx.Unmap(&t.ds_read, 0);
                id |= u32::from(b) << (8 * u32::from(k));
            }
        }
        Ok(id)
    }

    /// The selection drawn on the last frame's picture (`out`: the pass whose RGBA8 output is shown): the selected
    /// instances' draws (`picks`) replayed against the frame's depth into the stencil, then an outline where the
    /// selection's edge is (`seen`, over everything) and where its hidden parts' edge is (`hidden`).
    ///
    /// # Errors
    /// A Direct3D call failed or the outline shader did not compile.
    #[allow(clippy::too_many_arguments, reason = "the selection's inputs")]
    pub fn outline(
        &mut self,
        live: &Live,
        picks: &[u32],
        out: &Pass,
        seen: [f32; 4],
        hidden: [f32; 4],
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<(), String> {
        let steps = live.steps_of(picks);
        if steps.is_empty() {
            return Ok(());
        }
        if self.gpu.outline.is_none() {
            let vs = compile(s!("vs"), s!("vs_5_0"))?;
            let ps = compile(s!("ps"), s!("ps_5_0"))?;
            self.gpu.outline = Some((self.vertex_shader(&vs)?, self.pixel_shader(&ps)?));
        }
        let (mats, _) = self.materials_of(live, &steps, files, raw)?;
        let ring = self.ctx1().is_some();
        // SAFETY: the context of this device
        unsafe { self.ctx.ClearState() };
        self.bind_scene()?;
        for seen_pass in [true, false] {
            self.submit(live, &steps, &mats, ring, Mode::Mask(seen_pass), None, files, raw)?;
        }
        let (rtv, _) = self.pass_target(out)?;
        let (vs, ps) = some(self.gpu.outline.clone(), "outline")?;
        let cb: Vec<u8> = seen.iter().chain(&hidden).flat_map(|v| v.to_le_bytes()).collect();
        let cb = self.cbuffer(&cb, 1)?;
        // alpha blended over, no depth
        let (blend, depth, raster) = self.keyed_states_ex([0, 1, 5, 6, 1, 2, 1, 1, 15], [0, 0, 8, 0], 1)?;
        let t = some(self.target.as_ref(), "target")?;
        let srv = [Some(t.stencil_srv.clone())];
        let none: [Option<ID3D11ShaderResourceView>; 1] = [None];
        // SAFETY: objects made by this device; every array outlives its call; the depth-stencil is not bound while
        // the stencil is read
        unsafe {
            self.ctx.ClearState();
            self.ctx.OMSetRenderTargets(Some(&[Some(rtv)]), None);
            self.ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: out.size[0] as f32,
                Height: out.size[1] as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            self.ctx
                .IASetPrimitiveTopology(windows::Win32::Graphics::Direct3D::D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
            self.ctx.VSSetShader(&vs, None);
            self.ctx.PSSetShader(&ps, None);
            self.ctx.PSSetConstantBuffers(0, Some(&[cb]));
            self.ctx.PSSetShaderResources(0, Some(&srv));
            self.ctx.OMSetBlendState(&blend, None, 0xffff_ffff);
            self.ctx.OMSetDepthStencilState(&depth, 0);
            self.ctx.RSSetState(&raster);
            self.ctx.Draw(3, 0);
            self.ctx.PSSetShaderResources(0, Some(&none));
            self.ctx.OMSetRenderTargets(None, None);
        }
        Ok(())
    }

    /// The pass's RGBA8 output copied into a texture shared with another device (three, in turn) and the device
    /// waited for, so the other device reads a finished frame: no read back.
    ///
    /// # Errors
    /// A Direct3D call failed (a driver without NT-handle sharing).
    pub fn share(&mut self, out: &Pass) -> Result<SharedFrame, String> {
        let (_, tex) = self.pass_target(out)?;
        let size = out.size;
        if self.gpu.shared.is_empty() || self.gpu.shared_size != size {
            for s in self.gpu.shared.drain(..) {
                // SAFETY: a handle this device made
                unsafe { CloseHandle(s.handle).ok() };
            }
            for _ in 0..3 {
                let desc = D3D11_TEXTURE2D_DESC {
                    Width: size[0],
                    Height: size[1],
                    MipLevels: 1,
                    ArraySize: 1,
                    Format: DXGI_FORMAT_R8G8B8A8_UNORM_SRGB,
                    SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                    Usage: D3D11_USAGE_DEFAULT,
                    BindFlags: (D3D11_BIND_SHADER_RESOURCE.0 | D3D11_BIND_RENDER_TARGET.0) as u32,
                    CPUAccessFlags: 0,
                    MiscFlags: (D3D11_RESOURCE_MISC_SHARED_NTHANDLE.0 | D3D11_RESOURCE_MISC_SHARED.0) as u32,
                };
                let mut t = None;
                // SAFETY: a valid description; the shared handle is asked of the texture just made
                let (tex2, handle) = unsafe {
                    self.device
                        .CreateTexture2D(&raw const desc, None, Some(&raw mut t))
                        .map_err(err("CreateTexture2D (shared)"))?;
                    let t = some(t, "shared texture")?;
                    let r: IDXGIResource1 = t.cast().map_err(err("IDXGIResource1"))?;
                    let h = r
                        .CreateSharedHandle(
                            None,
                            DXGI_SHARED_RESOURCE_READ.0 | DXGI_SHARED_RESOURCE_WRITE.0,
                            PCWSTR::null(),
                        )
                        .map_err(err("CreateSharedHandle"))?;
                    (t, h)
                };
                self.gpu.shared.push(Shared { tex: tex2, handle });
            }
            self.gpu.shared_size = size;
            self.gpu.shared_generation += 1;
        }
        let slot = self.gpu.shared_next % self.gpu.shared.len();
        self.gpu.shared_next = slot + 1;
        let s = some(self.gpu.shared.get(slot), "shared")?;
        let (dst, handle) = (s.tex.clone(), s.handle);
        // SAFETY: two textures of this device of the same size and format group
        unsafe { self.ctx.CopyResource(&dst, &tex) };
        self.finish()?;
        Ok(SharedFrame {
            handle: handle.0 as isize,
            slot,
            size,
            generation: self.gpu.shared_generation,
        })
    }

    /// Clears the scene's targets (an empty frame of `size`).
    ///
    /// # Errors
    /// A Direct3D call failed.
    pub fn clear(&mut self, size: [u32; 2], clear: [f32; 4]) -> Result<(), String> {
        self.begin([size[0].max(1), size[1].max(1)], &clear)
    }
}
