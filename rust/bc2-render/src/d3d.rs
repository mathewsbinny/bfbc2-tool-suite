//! The Direct3D 11 device that draws a [`Frame`] with DICE's compiled shaders and reads it back (`shaderview.cpp` in
//! Rust): the frame is drawn into a 32-bit float colour target with a 32-bit depth and 8-bit stencil buffer, each
//! draw with its program's own state (blend, depth, rasterizer: `FrontCounterClockwise`, culling from the state
//! word), writing its draw class into the stencil where it keeps a sample (the coverage the tonemap's exposure and
//! the background use). Defaults per draw, as `shaderview`'s: t0 a 1 x 1 shadow map of depth 1 (lit), s0 a
//! comparison sampler, s1 trilinear anisotropic wrap, the program's own sampler states over them.
//!
//! GPU objects are made once and kept: shaders by their DXBC checksum, input layouts by shader and elements, vertex
//! and index buffers by their data (the `Arc`'s address and length: keep the data alive while the device is used, or
//! call [`Device::forget`]), textures by name, states by their description.
#![allow(
    unsafe_code,
    reason = "Direct3D 11 through the windows crate: every call is a foreign call"
)]

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use windows::Win32::Foundation::HMODULE;
use windows::Win32::Graphics::Direct3D::{
    D3D_DRIVER_TYPE, D3D_DRIVER_TYPE_HARDWARE, D3D_DRIVER_TYPE_WARP, D3D_FEATURE_LEVEL_11_0,
    D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST, D3D_SRV_DIMENSION_TEXTURE2D, D3D_SRV_DIMENSION_TEXTURECUBE,
};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_CONSTANT_BUFFER, D3D11_BIND_DEPTH_STENCIL, D3D11_BIND_INDEX_BUFFER, D3D11_BIND_RENDER_TARGET,
    D3D11_BIND_SHADER_RESOURCE, D3D11_BIND_VERTEX_BUFFER, D3D11_BLEND, D3D11_BLEND_DESC, D3D11_BLEND_OP,
    D3D11_BUFFER_DESC, D3D11_CLEAR_DEPTH, D3D11_CLEAR_STENCIL, D3D11_COMPARISON_ALWAYS, D3D11_COMPARISON_FUNC,
    D3D11_COMPARISON_LESS_EQUAL, D3D11_CPU_ACCESS_READ, D3D11_CREATE_DEVICE_FLAG, D3D11_CULL_MODE,
    D3D11_DEPTH_STENCIL_DESC, D3D11_DEPTH_STENCILOP_DESC, D3D11_DEPTH_WRITE_MASK_ALL, D3D11_DEPTH_WRITE_MASK_ZERO,
    D3D11_FILL_SOLID, D3D11_FILTER, D3D11_FILTER_ANISOTROPIC, D3D11_FILTER_COMPARISON_MIN_MAG_LINEAR_MIP_POINT,
    D3D11_INPUT_ELEMENT_DESC, D3D11_INPUT_PER_INSTANCE_DATA, D3D11_INPUT_PER_VERTEX_DATA, D3D11_MAP_READ,
    D3D11_MAPPED_SUBRESOURCE, D3D11_RASTERIZER_DESC, D3D11_RENDER_TARGET_BLEND_DESC, D3D11_RESOURCE_MISC_TEXTURECUBE,
    D3D11_SAMPLER_DESC, D3D11_SDK_VERSION, D3D11_SHADER_RESOURCE_VIEW_DESC, D3D11_SHADER_RESOURCE_VIEW_DESC_0,
    D3D11_STENCIL_OP_KEEP, D3D11_STENCIL_OP_REPLACE, D3D11_SUBRESOURCE_DATA, D3D11_TEX2D_SRV, D3D11_TEXCUBE_SRV,
    D3D11_TEXTURE_ADDRESS_CLAMP, D3D11_TEXTURE_ADDRESS_MODE, D3D11_TEXTURE_ADDRESS_WRAP, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_DEFAULT, D3D11_USAGE_IMMUTABLE, D3D11_USAGE_STAGING, D3D11_VIEWPORT, D3D11CreateDevice,
    ID3D11BlendState, ID3D11Buffer, ID3D11DepthStencilState, ID3D11DepthStencilView, ID3D11Device, ID3D11DeviceContext,
    ID3D11InputLayout, ID3D11PixelShader, ID3D11RasterizerState, ID3D11RenderTargetView, ID3D11SamplerState,
    ID3D11ShaderResourceView, ID3D11Texture2D, ID3D11VertexShader,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_FORMAT, DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB, DXGI_FORMAT_BC1_UNORM,
    DXGI_FORMAT_BC1_UNORM_SRGB, DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB, DXGI_FORMAT_BC3_UNORM,
    DXGI_FORMAT_BC3_UNORM_SRGB, DXGI_FORMAT_BC5_UNORM, DXGI_FORMAT_D32_FLOAT_S8X24_UINT, DXGI_FORMAT_R8_UNORM,
    DXGI_FORMAT_R16_UINT, DXGI_FORMAT_R16G16B16A16_FLOAT, DXGI_FORMAT_R32_FLOAT, DXGI_FORMAT_R32_UINT,
    DXGI_FORMAT_R32G32B32A32_FLOAT, DXGI_SAMPLE_DESC,
};
use windows::core::s;

use crate::frame::{DrawCall, Frame, Rendered};
use crate::material::{Element, TextureFiles};
use crate::terrain::RawTexture;
use crate::{f32_at, u32_at};

struct Target {
    size: [u32; 2],
    rt: ID3D11Texture2D,
    rtv: ID3D11RenderTargetView,
    ds: ID3D11Texture2D,
    dsv: ID3D11DepthStencilView,
    rt_read: ID3D11Texture2D,
    ds_read: ID3D11Texture2D,
}

/// The device and everything it keeps.
#[allow(clippy::struct_field_names, reason = "the D3D11 device beside its context")]
pub struct Device {
    device: ID3D11Device,
    ctx: ID3D11DeviceContext,
    /// The software rasterizer (WARP) is used.
    pub warp: bool,
    shadow: ID3D11ShaderResourceView,
    defaults: [ID3D11SamplerState; 2],
    vs: HashMap<[u8; 16], ID3D11VertexShader>,
    ps: HashMap<[u8; 16], ID3D11PixelShader>,
    layouts: HashMap<([u8; 16], Vec<[u32; 5]>), ID3D11InputLayout>,
    buffers: HashMap<(usize, usize, u32), ID3D11Buffer>,
    cbuffers: HashMap<(usize, u8), ID3D11Buffer>,
    textures: HashMap<String, Option<ID3D11ShaderResourceView>>,
    samplers: HashMap<Vec<u8>, ID3D11SamplerState>,
    blends: HashMap<[u32; 9], ID3D11BlendState>,
    depths: HashMap<[u32; 3], ID3D11DepthStencilState>,
    rasters: HashMap<u32, ID3D11RasterizerState>,
    target: Option<Target>,
}

fn err(what: &str) -> impl Fn(windows::core::Error) -> String + '_ {
    move |e| format!("{what}: {e}")
}

fn some<T>(v: Option<T>, what: &str) -> Result<T, String> {
    v.ok_or_else(|| format!("{what}: nothing returned"))
}

/// A DXBC shader's checksum (bytes 4..20), its key.
fn checksum(code: &[u8]) -> [u8; 16] {
    let mut k = [0u8; 16];
    for (d, s) in k.iter_mut().zip(code.iter().skip(4)) {
        *d = *s;
    }
    k
}

/// A `D3D11_SAMPLER_DESC` from its 52 stored bytes.
fn sampler_desc(b: &[u8]) -> D3D11_SAMPLER_DESC {
    let i = |k: usize| u32_at(b, 4 * k) as i32;
    let f = |k: usize| f32_at(b, 4 * k);
    D3D11_SAMPLER_DESC {
        Filter: D3D11_FILTER(i(0)),
        AddressU: D3D11_TEXTURE_ADDRESS_MODE(i(1)),
        AddressV: D3D11_TEXTURE_ADDRESS_MODE(i(2)),
        AddressW: D3D11_TEXTURE_ADDRESS_MODE(i(3)),
        MipLODBias: f(4),
        MaxAnisotropy: u32_at(b, 20),
        ComparisonFunc: D3D11_COMPARISON_FUNC(i(6)),
        BorderColor: [f(7), f(8), f(9), f(10)],
        MinLOD: f(11),
        MaxLOD: f(12),
    }
}

impl Device {
    /// A device on the GPU, or on WARP when `warp` or when the GPU's fails.
    ///
    /// # Errors
    /// No Direct3D 11 device could be made.
    pub fn new(warp: bool) -> Result<Self, String> {
        let make = |kind: D3D_DRIVER_TYPE| -> Option<(ID3D11Device, ID3D11DeviceContext)> {
            let (mut device, mut ctx) = (None, None);
            // SAFETY: out pointers to locals; the feature level list outlives the call
            unsafe {
                D3D11CreateDevice(
                    None,
                    kind,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG(0),
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&raw mut device),
                    None,
                    Some(&raw mut ctx),
                )
            }
            .ok()?;
            Some((device?, ctx?))
        };
        let (on_warp, (device, ctx)) = match (!warp).then(|| make(D3D_DRIVER_TYPE_HARDWARE)).flatten() {
            Some(d) => (false, d),
            None => (true, some(make(D3D_DRIVER_TYPE_WARP), "D3D11CreateDevice")?),
        };
        let mut sc = D3D11_SAMPLER_DESC {
            Filter: D3D11_FILTER_COMPARISON_MIN_MAG_LINEAR_MIP_POINT,
            AddressU: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressV: D3D11_TEXTURE_ADDRESS_CLAMP,
            AddressW: D3D11_TEXTURE_ADDRESS_CLAMP,
            ComparisonFunc: D3D11_COMPARISON_LESS_EQUAL,
            MaxLOD: f32::MAX,
            ..Default::default()
        };
        let compare = Self::make_sampler(&device, &sc)?;
        sc.Filter = D3D11_FILTER_ANISOTROPIC;
        sc.MaxAnisotropy = 8;
        sc.ComparisonFunc = D3D11_COMPARISON_FUNC(0);
        sc.AddressU = D3D11_TEXTURE_ADDRESS_WRAP;
        sc.AddressV = D3D11_TEXTURE_ADDRESS_WRAP;
        sc.AddressW = D3D11_TEXTURE_ADDRESS_WRAP;
        let aniso = Self::make_sampler(&device, &sc)?;
        let one = 1f32.to_le_bytes();
        let shadow = Self::make_texture(&device, 1, 1, 1, 1, DXGI_FORMAT_R32_FLOAT, false, &[(&one, 4)])?;
        Ok(Self {
            device,
            ctx,
            warp: on_warp,
            shadow,
            defaults: [compare, aniso],
            vs: HashMap::new(),
            ps: HashMap::new(),
            layouts: HashMap::new(),
            buffers: HashMap::new(),
            cbuffers: HashMap::new(),
            textures: HashMap::new(),
            samplers: HashMap::new(),
            blends: HashMap::new(),
            depths: HashMap::new(),
            rasters: HashMap::new(),
            target: None,
        })
    }

    /// Drops the kept buffers and textures (the data they were made from may be gone).
    pub fn forget(&mut self) {
        self.buffers.clear();
        self.textures.clear();
    }

    fn make_sampler(device: &ID3D11Device, d: &D3D11_SAMPLER_DESC) -> Result<ID3D11SamplerState, String> {
        let mut s = None;
        // SAFETY: a valid description; the out pointer is a local
        unsafe { device.CreateSamplerState(d, Some(&raw mut s)) }.map_err(err("CreateSamplerState"))?;
        some(s, "CreateSamplerState")
    }

    /// A texture from its levels (data, row pitch), face by face.
    #[allow(clippy::too_many_arguments, reason = "a texture's description")]
    fn make_texture(
        device: &ID3D11Device,
        w: u32,
        h: u32,
        mips: u32,
        faces: u32,
        format: DXGI_FORMAT,
        cube: bool,
        levels: &[(&[u8], u32)],
    ) -> Result<ID3D11ShaderResourceView, String> {
        let desc = D3D11_TEXTURE2D_DESC {
            Width: w,
            Height: h,
            MipLevels: mips,
            ArraySize: faces,
            Format: format,
            SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: D3D11_BIND_SHADER_RESOURCE.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: if cube {
                D3D11_RESOURCE_MISC_TEXTURECUBE.0 as u32
            } else {
                0
            },
        };
        let init: Vec<D3D11_SUBRESOURCE_DATA> = levels
            .iter()
            .map(|(d, pitch)| D3D11_SUBRESOURCE_DATA {
                pSysMem: d.as_ptr().cast(),
                SysMemPitch: *pitch,
                SysMemSlicePitch: 0,
            })
            .collect();
        let mut t: Option<ID3D11Texture2D> = None;
        // SAFETY: `init` has one entry per subresource, each pointing into `levels`, which outlive the call
        unsafe { device.CreateTexture2D(&raw const desc, Some(init.as_ptr()), Some(&raw mut t)) }
            .map_err(err("CreateTexture2D"))?;
        let t = some(t, "CreateTexture2D")?;
        let view = D3D11_SHADER_RESOURCE_VIEW_DESC {
            Format: format,
            ViewDimension: if cube {
                D3D_SRV_DIMENSION_TEXTURECUBE
            } else {
                D3D_SRV_DIMENSION_TEXTURE2D
            },
            Anonymous: if cube {
                D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    TextureCube: D3D11_TEXCUBE_SRV {
                        MostDetailedMip: 0,
                        MipLevels: mips,
                    },
                }
            } else {
                D3D11_SHADER_RESOURCE_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_SRV {
                        MostDetailedMip: 0,
                        MipLevels: mips,
                    },
                }
            },
        };
        let mut v = None;
        // SAFETY: a valid view of the texture just made
        unsafe { device.CreateShaderResourceView(&t, Some(&raw const view), Some(&raw mut v)) }
            .map_err(err("CreateShaderResourceView"))?;
        some(v, "CreateShaderResourceView")
    }

    /// A BFBC2 PC texture resource uploaded as the game stores it (`shaderview`'s `texres`): its block format, its
    /// mips, an sRGB view when header flag bit 0 is set, a cube when its type is 1.
    fn texture_res(&self, d: &[u8]) -> Result<ID3D11ShaderResourceView, String> {
        let (typ, fmt, flags, w, h, mips) = (
            u32_at(d, 4),
            u32_at(d, 8),
            u32_at(d, 12),
            u32_at(d, 16),
            u32_at(d, 20),
            u32_at(d, 28),
        );
        let srgb = flags & 1 != 0;
        let pick = |a, b| if srgb { b } else { a };
        let (format, block, bpp) = match fmt {
            0 | 18 => (pick(DXGI_FORMAT_BC1_UNORM, DXGI_FORMAT_BC1_UNORM_SRGB), 8, 0),
            1 => (pick(DXGI_FORMAT_BC2_UNORM, DXGI_FORMAT_BC2_UNORM_SRGB), 16, 0),
            2 => (pick(DXGI_FORMAT_BC3_UNORM, DXGI_FORMAT_BC3_UNORM_SRGB), 16, 0),
            19 | 20 => (DXGI_FORMAT_BC5_UNORM, 16, 0),
            13 => (DXGI_FORMAT_R16G16B16A16_FLOAT, 0, 8),
            9 => (pick(DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_FORMAT_B8G8R8A8_UNORM_SRGB), 0, 4),
            10 => (DXGI_FORMAT_R8_UNORM, 0, 1),
            _ => return Err(format!("texture format {fmt} not supported")),
        };
        if !(1..=15).contains(&mips) || w == 0 || h == 0 {
            return Err(format!("{w}x{h}, {mips} mip levels"));
        }
        let cube = typ == 1;
        let faces = if cube { 6 } else { 1 };
        let mut levels = Vec::new();
        let mut pos = 92usize;
        for _ in 0..faces {
            for m in 0..mips {
                let size = u32_at(d, 32 + 4 * m as usize) as usize;
                let (lw, lh) = ((w >> m).max(1), (h >> m).max(1));
                let mut pitch = if block > 0 { lw.div_ceil(4) * block } else { lw * bpp };
                let rows = if block > 0 { lh.div_ceil(4) } else { lh };
                if block == 0 && rows > 0 && size >= (pitch * rows) as usize {
                    pitch = (size / rows as usize) as u32; // L8: rows padded
                }
                let data = d
                    .get(pos..pos + size)
                    .filter(|s| s.len() >= (pitch * rows) as usize)
                    .ok_or_else(|| format!("level {m} runs past the data"))?;
                levels.push((data, pitch));
                pos += size;
            }
        }
        Self::make_texture(&self.device, w, h, mips, faces, format, cube, &levels)
    }

    fn texture(
        &mut self,
        name: &str,
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Option<ID3D11ShaderResourceView> {
        if !self.textures.contains_key(name) {
            let made = if let Some(r) = raw.get(name) {
                Self::make_texture(
                    &self.device,
                    r.width,
                    r.height,
                    1,
                    1,
                    DXGI_FORMAT(r.format as i32),
                    false,
                    &[(&r.data, r.width * r.texel)],
                )
                .ok()
            } else {
                files
                    .get(name)
                    .and_then(Option::as_ref)
                    .and_then(|b| self.texture_res(b).ok())
            };
            self.textures.insert(name.to_owned(), made);
        }
        self.textures.get(name).cloned().flatten()
    }

    fn buffer(&mut self, data: &Arc<Vec<u8>>, bind: u32) -> Result<ID3D11Buffer, String> {
        let key = (Arc::as_ptr(data) as usize, data.len(), bind);
        if let Some(b) = self.buffers.get(&key) {
            return Ok(b.clone());
        }
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: (data.len().max(16)) as u32,
            Usage: D3D11_USAGE_IMMUTABLE,
            BindFlags: bind,
            ..Default::default()
        };
        let mut padded;
        let bytes: &[u8] = if data.len() < 16 {
            padded = data.to_vec();
            padded.resize(16, 0);
            &padded
        } else {
            data
        };
        let init = D3D11_SUBRESOURCE_DATA {
            pSysMem: bytes.as_ptr().cast(),
            SysMemPitch: 0,
            SysMemSlicePitch: 0,
        };
        let mut b = None;
        // SAFETY: `bytes` holds ByteWidth bytes and outlives the call
        unsafe {
            self.device
                .CreateBuffer(&raw const desc, Some(&raw const init), Some(&raw mut b))
        }
        .map_err(err("CreateBuffer"))?;
        let b = some(b, "CreateBuffer")?;
        self.buffers.insert(key, b.clone());
        Ok(b)
    }

    /// A constant buffer of `data`'s size for a stage (0 vertex, 1 pixel), filled with it.
    fn cbuffer(&mut self, data: &[u8], stage: u8) -> Result<Option<ID3D11Buffer>, String> {
        if data.is_empty() {
            return Ok(None);
        }
        let size = data.len().div_ceil(16) * 16;
        let key = (size, stage);
        if !self.cbuffers.contains_key(&key) {
            let desc = D3D11_BUFFER_DESC {
                ByteWidth: size as u32,
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
                ..Default::default()
            };
            let mut b = None;
            // SAFETY: a valid description; no initial data
            unsafe { self.device.CreateBuffer(&raw const desc, None, Some(&raw mut b)) }
                .map_err(err("CreateBuffer (constants)"))?;
            self.cbuffers.insert(key, some(b, "CreateBuffer (constants)")?);
        }
        let b = some(self.cbuffers.get(&key).cloned(), "constants")?;
        let mut full = data.to_vec();
        full.resize(size, 0);
        // SAFETY: `full` holds the whole buffer
        unsafe { self.ctx.UpdateSubresource(&b, 0, None, full.as_ptr().cast(), 0, 0) };
        Ok(Some(b))
    }

    fn sampler(&mut self, desc: &[u8]) -> Option<ID3D11SamplerState> {
        if !self.samplers.contains_key(desc) {
            let s = Self::make_sampler(&self.device, &sampler_desc(desc)).ok()?;
            self.samplers.insert(desc.to_vec(), s);
        }
        self.samplers.get(desc).cloned()
    }

    fn shaders(&mut self, d: &DrawCall) -> Result<(ID3D11VertexShader, ID3D11PixelShader, ID3D11InputLayout), String> {
        let (kv, kp) = (checksum(&d.vs), checksum(&d.ps));
        if !self.vs.contains_key(&kv) {
            let mut s = None;
            // SAFETY: the code is a whole DXBC container
            unsafe { self.device.CreateVertexShader(&d.vs, None, Some(&raw mut s)) }
                .map_err(err("CreateVertexShader"))?;
            self.vs.insert(kv, some(s, "CreateVertexShader")?);
        }
        if !self.ps.contains_key(&kp) {
            let mut s = None;
            // SAFETY: as above
            unsafe { self.device.CreatePixelShader(&d.ps, None, Some(&raw mut s)) }
                .map_err(err("CreatePixelShader"))?;
            self.ps.insert(kp, some(s, "CreatePixelShader")?);
        }
        let els: Vec<[u32; 5]> = d
            .elements
            .iter()
            .map(|e: &Element| [e.semantic, e.format, e.slot, e.offset, e.instance])
            .collect();
        let lk = (kv, els);
        if !self.layouts.contains_key(&lk) {
            let descs: Vec<D3D11_INPUT_ELEMENT_DESC> = d
                .elements
                .iter()
                .map(|e| D3D11_INPUT_ELEMENT_DESC {
                    SemanticName: s!("TEXCOORD"),
                    SemanticIndex: e.semantic,
                    Format: DXGI_FORMAT(e.format as i32),
                    InputSlot: e.slot,
                    AlignedByteOffset: e.offset,
                    InputSlotClass: if e.instance != 0 {
                        D3D11_INPUT_PER_INSTANCE_DATA
                    } else {
                        D3D11_INPUT_PER_VERTEX_DATA
                    },
                    InstanceDataStepRate: e.instance,
                })
                .collect();
            let mut l = None;
            // SAFETY: the descriptions name a static semantic string; the code is the vertex shader's
            unsafe { self.device.CreateInputLayout(&descs, &d.vs, Some(&raw mut l)) }
                .map_err(err("CreateInputLayout"))?;
            self.layouts.insert(lk.clone(), some(l, "CreateInputLayout")?);
        }
        Ok((
            some(self.vs.get(&kv).cloned(), "vs")?,
            some(self.ps.get(&kp).cloned(), "ps")?,
            some(self.layouts.get(&lk).cloned(), "layout")?,
        ))
    }

    /// The program's states (`renderqa.state_lines`; no state recorded: opaque, no culling).
    fn states(
        &mut self,
        d: &DrawCall,
    ) -> Result<(ID3D11BlendState, ID3D11DepthStencilState, ID3D11RasterizerState), String> {
        let (blend, depth, cull) = match &d.state {
            None => ([0, 0, 2, 1, 1, 2, 1, 1, 15], [1, 1, 2], 1),
            Some(st) => {
                let [(cs, cd, co), (a_s, ad, ao)] = st.factors();
                (
                    [
                        u32::from(st.alpha_to_coverage),
                        u32::from(st.blend()),
                        cs,
                        cd,
                        co,
                        a_s,
                        ad,
                        ao,
                        15,
                    ],
                    [u32::from(st.depth_test), u32::from(st.depth_write), 4],
                    if st.two_sided { 1 } else { 3 },
                )
            }
        };
        if !self.blends.contains_key(&blend) {
            let [a2c, en, cs, cd, co, a_s, ad, ao, mask] = blend;
            let rt = D3D11_RENDER_TARGET_BLEND_DESC {
                BlendEnable: (en != 0).into(),
                SrcBlend: D3D11_BLEND(cs as i32),
                DestBlend: D3D11_BLEND(cd as i32),
                BlendOp: D3D11_BLEND_OP(co as i32),
                SrcBlendAlpha: D3D11_BLEND(a_s as i32),
                DestBlendAlpha: D3D11_BLEND(ad as i32),
                BlendOpAlpha: D3D11_BLEND_OP(ao as i32),
                RenderTargetWriteMask: mask as u8,
            };
            let desc = D3D11_BLEND_DESC {
                AlphaToCoverageEnable: (a2c != 0).into(),
                IndependentBlendEnable: false.into(),
                RenderTarget: [rt; 8],
            };
            let mut s = None;
            // SAFETY: a valid description
            unsafe { self.device.CreateBlendState(&raw const desc, Some(&raw mut s)) }
                .map_err(err("CreateBlendState"))?;
            self.blends.insert(blend, some(s, "CreateBlendState")?);
        }
        if !self.depths.contains_key(&depth) {
            let [en, wr, func] = depth;
            let face = D3D11_DEPTH_STENCILOP_DESC {
                StencilFailOp: D3D11_STENCIL_OP_KEEP,
                StencilDepthFailOp: D3D11_STENCIL_OP_KEEP,
                StencilPassOp: D3D11_STENCIL_OP_REPLACE,
                StencilFunc: D3D11_COMPARISON_ALWAYS,
            };
            let desc = D3D11_DEPTH_STENCIL_DESC {
                DepthEnable: (en != 0).into(),
                DepthWriteMask: if wr != 0 {
                    D3D11_DEPTH_WRITE_MASK_ALL
                } else {
                    D3D11_DEPTH_WRITE_MASK_ZERO
                },
                DepthFunc: D3D11_COMPARISON_FUNC(func as i32),
                StencilEnable: true.into(),
                StencilReadMask: 0xff,
                StencilWriteMask: 0xff,
                FrontFace: face,
                BackFace: face,
            };
            let mut s = None;
            // SAFETY: a valid description
            unsafe { self.device.CreateDepthStencilState(&raw const desc, Some(&raw mut s)) }
                .map_err(err("CreateDepthStencilState"))?;
            self.depths.insert(depth, some(s, "CreateDepthStencilState")?);
        }
        if !self.rasters.contains_key(&cull) {
            let desc = D3D11_RASTERIZER_DESC {
                FillMode: D3D11_FILL_SOLID,
                CullMode: D3D11_CULL_MODE(cull as i32),
                FrontCounterClockwise: true.into(),
                DepthClipEnable: true.into(),
                ..Default::default()
            };
            let mut s = None;
            // SAFETY: a valid description
            unsafe { self.device.CreateRasterizerState(&raw const desc, Some(&raw mut s)) }
                .map_err(err("CreateRasterizerState"))?;
            self.rasters.insert(cull, some(s, "CreateRasterizerState")?);
        }
        Ok((
            some(self.blends.get(&blend).cloned(), "blend")?,
            some(self.depths.get(&depth).cloned(), "depth")?,
            some(self.rasters.get(&cull).cloned(), "raster")?,
        ))
    }

    fn make_target(&self, size: [u32; 2]) -> Result<Target, String> {
        let tex = |format, bind: u32, staging: bool| -> Result<ID3D11Texture2D, String> {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: size[0],
                Height: size[1],
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC { Count: 1, Quality: 0 },
                Usage: if staging {
                    D3D11_USAGE_STAGING
                } else {
                    D3D11_USAGE_DEFAULT
                },
                BindFlags: bind,
                CPUAccessFlags: if staging { D3D11_CPU_ACCESS_READ.0 as u32 } else { 0 },
                MiscFlags: 0,
            };
            let mut t = None;
            // SAFETY: a valid description; no initial data
            unsafe { self.device.CreateTexture2D(&raw const desc, None, Some(&raw mut t)) }
                .map_err(err("CreateTexture2D (target)"))?;
            some(t, "CreateTexture2D (target)")
        };
        let rt = tex(DXGI_FORMAT_R32G32B32A32_FLOAT, D3D11_BIND_RENDER_TARGET.0 as u32, false)?;
        let ds = tex(
            DXGI_FORMAT_D32_FLOAT_S8X24_UINT,
            D3D11_BIND_DEPTH_STENCIL.0 as u32,
            false,
        )?;
        let (mut rtv, mut dsv) = (None, None);
        // SAFETY: views of the textures just made
        unsafe {
            self.device
                .CreateRenderTargetView(&rt, None, Some(&raw mut rtv))
                .map_err(err("CreateRenderTargetView"))?;
            self.device
                .CreateDepthStencilView(&ds, None, Some(&raw mut dsv))
                .map_err(err("CreateDepthStencilView"))?;
        }
        Ok(Target {
            size,
            rt_read: tex(DXGI_FORMAT_R32G32B32A32_FLOAT, 0, true)?,
            ds_read: tex(DXGI_FORMAT_D32_FLOAT_S8X24_UINT, 0, true)?,
            rt,
            ds,
            rtv: some(rtv, "rtv")?,
            dsv: some(dsv, "dsv")?,
        })
    }

    /// Draws a frame and reads it back. `files`: texture resources by name; `raw`: the engine's own textures.
    ///
    /// # Errors
    /// A Direct3D call failed (a shader the device refuses, a lost device).
    #[allow(
        clippy::too_many_lines,
        reason = "one draw's set-up, in the order shaderview makes it"
    )]
    pub fn render(
        &mut self,
        frame: &Frame,
        files: &TextureFiles,
        raw: &HashMap<String, RawTexture>,
    ) -> Result<Rendered, String> {
        let t0 = Instant::now();
        let size = [frame.width.max(1), frame.height.max(1)];
        if self.target.as_ref().is_none_or(|t| t.size != size) {
            self.target = Some(self.make_target(size)?);
        }
        let (rtv, dsv) = {
            let t = some(self.target.as_ref(), "target")?;
            (t.rtv.clone(), t.dsv.clone())
        };
        let ctx = self.ctx.clone();
        // SAFETY: views made by this device; the arrays outlive each call
        unsafe {
            ctx.ClearState();
            ctx.ClearRenderTargetView(&rtv, &frame.clear);
            ctx.ClearDepthStencilView(&dsv, D3D11_CLEAR_DEPTH.0 | D3D11_CLEAR_STENCIL.0, 1.0, 0);
            ctx.OMSetRenderTargets(Some(&[Some(rtv.clone())]), &dsv);
            ctx.RSSetViewports(Some(&[D3D11_VIEWPORT {
                TopLeftX: 0.0,
                TopLeftY: 0.0,
                Width: size[0] as f32,
                Height: size[1] as f32,
                MinDepth: 0.0,
                MaxDepth: 1.0,
            }]));
            ctx.IASetPrimitiveTopology(D3D_PRIMITIVE_TOPOLOGY_TRIANGLELIST);
        }
        let mut drawn = 0;
        for d in &frame.draws {
            let (vs, ps, layout) = self.shaders(d)?;
            let vb = self.buffer(&d.vertices, D3D11_BIND_VERTEX_BUFFER.0 as u32)?;
            let ib = self.buffer(&d.indices, D3D11_BIND_INDEX_BUFFER.0 as u32)?;
            let cv = self.cbuffer(&d.cb_vs, 0)?;
            let cp = self.cbuffer(&d.cb_ps, 1)?;
            let mut ss: [Option<ID3D11SamplerState>; 16] = Default::default();
            ss[0] = Some(self.defaults[0].clone());
            ss[1] = Some(self.defaults[1].clone());
            for (reg, desc) in &d.samplers {
                if let Some(slot) = ss.get_mut(*reg as usize) {
                    *slot = self.sampler(desc);
                }
            }
            let mut vss = ss.clone();
            for (reg, desc) in &d.vs_samplers {
                if let Some(slot) = vss.get_mut(*reg as usize) {
                    *slot = self.sampler(desc);
                }
            }
            let mut srv: [Option<ID3D11ShaderResourceView>; 32] = Default::default();
            srv[0] = Some(self.shadow.clone());
            for (slot, name) in &d.textures {
                if let Some(s) = srv.get_mut(*slot as usize) {
                    *s = self.texture(name, files, raw);
                }
            }
            let mut vsrv = srv.clone();
            for (slot, name) in &d.vs_textures {
                if let Some(s) = vsrv.get_mut(*slot as usize) {
                    *s = self.texture(name, files, raw);
                }
            }
            let (blend, depth, raster) = self.states(d)?;
            let stride = d.stride;
            let offset = d.vertex_offset;
            let vbs = [Some(vb)];
            // SAFETY: objects made by this device; every array outlives its call
            unsafe {
                ctx.IASetInputLayout(&layout);
                ctx.IASetVertexBuffers(
                    0,
                    1,
                    Some(vbs.as_ptr()),
                    Some(&raw const stride),
                    Some(&raw const offset),
                );
                ctx.IASetIndexBuffer(
                    &ib,
                    if d.wide {
                        DXGI_FORMAT_R32_UINT
                    } else {
                        DXGI_FORMAT_R16_UINT
                    },
                    0,
                );
                ctx.VSSetShader(&vs, None);
                ctx.PSSetShader(&ps, None);
                ctx.VSSetConstantBuffers(0, Some(&[cv]));
                ctx.PSSetConstantBuffers(0, Some(&[cp]));
                ctx.PSSetSamplers(0, Some(&ss));
                ctx.VSSetSamplers(0, Some(&vss));
                ctx.VSSetShaderResources(0, Some(&vsrv));
                ctx.PSSetShaderResources(0, Some(&srv));
                ctx.OMSetBlendState(&blend, None, 0xffff_ffff);
                ctx.OMSetDepthStencilState(&depth, u32::from(d.class));
                ctx.RSSetState(&raster);
                ctx.DrawIndexed(d.index_count, d.first_index, 0);
            }
            drawn += 1;
        }
        let submit = t0.elapsed().as_secs_f64();
        let t1 = Instant::now();
        let t = some(self.target.as_ref(), "target")?;
        let (w, h) = (size[0] as usize, size[1] as usize);
        let mut out = Rendered {
            width: size[0],
            height: size[1],
            hdr: Vec::with_capacity(w * h * 4),
            depth: Vec::with_capacity(w * h),
            classes: Vec::with_capacity(w * h),
            draws: drawn,
            submit,
            readback: 0.0,
        };
        // SAFETY: staging copies of this device's targets, mapped for reading; rows read within RowPitch
        unsafe {
            ctx.CopyResource(&t.rt_read, &t.rt);
            ctx.CopyResource(&t.ds_read, &t.ds);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(&t.rt_read, 0, D3D11_MAP_READ, 0, Some(&raw mut m))
                .map_err(err("Map"))?;
            let base: *const f32 = m.pData.cast();
            for y in 0..h {
                // a mapped row of R32G32B32A32_FLOAT starts 16-byte aligned (RowPitch is a multiple of 16)
                let row = std::slice::from_raw_parts(base.add(y * m.RowPitch as usize / 4), w * 4);
                out.hdr.extend_from_slice(row);
            }
            ctx.Unmap(&t.rt_read, 0);
            let mut m = D3D11_MAPPED_SUBRESOURCE::default();
            ctx.Map(&t.ds_read, 0, D3D11_MAP_READ, 0, Some(&raw mut m))
                .map_err(err("Map (depth)"))?;
            let base: *const u8 = m.pData.cast();
            for y in 0..h {
                let row = std::slice::from_raw_parts(base.add(y * m.RowPitch as usize), w * 8);
                for px in row.as_chunks::<8>().0 {
                    out.depth.push(f32_at(px, 0));
                    out.classes.push(px[4]);
                }
            }
            ctx.Unmap(&t.ds_read, 0);
        }
        out.readback = t1.elapsed().as_secs_f64();
        Ok(out)
    }
}
