//! A decoded Xenos program as HLSL for BFBC2's Dx10 / Dx11 renderers (shader model 4 / 5): a port of
//! `bfbc2-shader-tools/xenos_hlsl.py`'s `translate` (the semantics, the engine constants, the fix-ups: its
//! docstrings). The text is `xenos_hlsl.py`'s, byte for byte; the errors are its `TranslateError`s with their
//! messages ([`TranslateError::Translate`]), and where the Python would stop on another exception (an empty
//! material value, a sampler fix with format fields other than `{}`, a constant name that is not an ASCII
//! identifier) [`TranslateError::Unsupported`] says so: the bindings then run the Python, which raises its own.
//!
//! ```
//! use bc2_shader::{hlsl, xenos};
//! # let mut b = vec![0u8; 0x24 + 32 + 4 * 6];
//! # let put = |b: &mut Vec<u8>, o: usize, v: u32| b[o..o + 4].copy_from_slice(&v.to_be_bytes());
//! # put(&mut b, 0, 0x102a_1100); put(&mut b, 4, 0x24 + 32); put(&mut b, 8, 24); put(&mut b, 0x18, 0x24);
//! # put(&mut b, 0x24 + 4, 24); put(&mut b, 0x24 + 32, 0x0000_1001); put(&mut b, 0x24 + 36, 0x2000);
//! let sh = xenos::parse(&b, Some(true))?;           // a pixel shader: one `exec_end` of one ALU instruction
//! let text = hlsl::translate(&sh, &hlsl::Interface::new(vec![]))?;
//! assert!(text.starts_with("// translated from Xbox 360 (Xenos) microcode\n"));
//! assert!(text.contains("PSOut main(V2P i, bool frontFace : SV_IsFrontFace) {"));
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;

use rayon::prelude::*;
use thiserror::Error;

use crate::pyrepr::float_repr;
use crate::xenos::{Alu, Cf, CfKind, CfType, Dest, Fetch, FetchKind, File, Instr, InstrKind, Operand, Shader};

/// `FLT_MAX`.
pub const FLT_MAX: &str = "3.402823466e+38";
const SHADOW: &str = "outdoorLightShadowmapTexture";
/// `PROBE_X0`, `PROBE_Y0`, `PROBE_W`, `PROBE_H`: the debug bit-bar block.
pub const PROBE: (i32, i32, i32, i32) = (60, 100, 26, 20);
/// `INSTANCE_SEMANTICS`.
pub const INSTANCE_SEMANTICS: [u32; 3] = [13, 14, 15];
const INSTANCE_PRELUDE: &str = "    float4 instW[4], instWVP[4];
    [unroll] for (int k = 0; k < 3; k++)
        instW[k] = vin.inst0 * worldMatrix[k].x + vin.inst1 * worldMatrix[k].y + vin.inst2 * worldMatrix[k].z
                 + float4(0.0, 0.0, 0.0, worldMatrix[k].w);
    instW[3] = float4(0.0, 0.0, 0.0, 1.0);
    [unroll] for (int j = 0; j < 4; j++)
        instWVP[j] = viewProjMatrix[j].x * instW[0] + viewProjMatrix[j].y * instW[1] + viewProjMatrix[j].z * instW[2]
                   + float4(0.0, 0.0, 0.0, viewProjMatrix[j].w);";

/// `HELPERS`: the functions every translation starts with.
pub const HELPERS: &str = r#"float3 tsizeLod(Texture2D t, float lod) {
    uint w, h, n;
    t.GetDimensions(0, w, h, n);
    uint m = (uint)clamp(floor(lod + 0.5), 0.0, float(n - 1));
    t.GetDimensions(m, w, h, n);
    return float3(w, h, m);
}

float4 xcube(float3 d) {
    float3 a = abs(d);
    if (a.z >= a.x && a.z >= a.y) return float4(-d.y, d.z < 0.0 ? -d.x : d.x, 2.0 * d.z, d.z < 0.0 ? 5.0 : 4.0);
    if (a.y >= a.x) return float4(d.y < 0.0 ? -d.z : d.z, d.x, 2.0 * d.y, d.y < 0.0 ? 3.0 : 2.0);
    return float4(-d.y, d.x < 0.0 ? d.z : -d.z, 2.0 * d.x, d.x < 0.0 ? 1.0 : 0.0);
}
// Xenos multiplies as Direct3D 9 does: 0 * anything = 0 (also 0 * inf, 0 * NaN), unlike IEEE (Xenia's
// "legacy multiply"); shaders rely on it when they normalise zero vectors
float4 xmul(float4 a, float4 b) { return (a == 0.0 || b == 0.0) ? 0.0 : a * b; }
float xmul1(float a, float b) { return (a == 0.0 || b == 0.0) ? 0.0 : a * b; }
float xdot4(float4 a, float4 b) { float4 m = xmul(a, b); return m.x + m.y + m.z + m.w; }
float xmax4(float4 v) { return max(max(v.x, v.y), max(v.z, v.w)); }
float xlog(float a) { float t = a == 1.0 ? 0.0 : log2(a); return isinf(t) && t < 0.0 ? -3.402823466e+38 : t; }
float xrcp(float a) { return a == 1.0 ? 1.0 : 1.0 / a; }
float xrcpc(float a) { float t = xrcp(a); return isinf(t) ? (t < 0.0 ? -3.402823466e+38 : 3.402823466e+38) : t; }
float xrcpf(float a) { float t = xrcp(a); return isinf(t) ? (t < 0.0 ? -0.0 : 0.0) : t; }
float xrsq(float a) { return a == 1.0 ? 1.0 : rsqrt(a); }
float xrsqc(float a) { float t = xrsq(a); return isinf(t) ? (t < 0.0 ? -3.402823466e+38 : 3.402823466e+38) : t; }
float xrsqf(float a) { float t = xrsq(a); return isinf(t) ? (t < 0.0 ? -0.0 : 0.0) : t; }
// Native BFBC2 PC object shaders pack near/middle/far cascades into top-left,
// top-right and bottom-left quadrants, with a 255/256 guard scale.
float2 xshadowuv(float3 consoleCoord) {
    float cascade = clamp(floor(consoleCoord.z * 3.0), 0.0, 2.0);
    float2 tile = cascade < 0.5 ? float2(0.0, 0.0) :
                  (cascade < 1.5 ? float2(0.5, 0.0) : float2(0.0, 0.5));
    return (consoleCoord.xy - 0.5) * (0.5 * (255.0 / 256.0)) + 0.25 + tile;
}
float2 tdim(Texture2D t) { uint w, h; t.GetDimensions(w, h); return float2(w, h); }
float3 xcubefetch(float3 c) {
    // tfetchCube's (s, t, face) - s, t in 1..2 from the cube op's sequence - as a direction (Direct3D's face
    // conventions, which the cube op follows: Xenia ucode.h kCube)
    float sc = (c.x - 1.5) * 2.0, tc = (c.y - 1.5) * 2.0;
    int f = (int)round(c.z);
    if (f == 0) return float3(1.0, -tc, -sc);
    if (f == 1) return float3(-1.0, -tc, sc);
    if (f == 2) return float3(sc, 1.0, tc);
    if (f == 3) return float3(sc, -1.0, -tc);
    if (f == 4) return float3(sc, -tc, 1.0);
    return float3(-sc, -tc, -1.0);
}
float4 xsrgb(float4 c) { float3 l = saturate(c.rgb); return float4(l <= 0.0031308 ? l * 12.92 : 1.055 * pow(l, 1.0 / 2.4) - 0.055, c.a); }"#;

/// Why a program has no translation.
#[derive(Debug, Error, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TranslateError {
    /// `xenos_hlsl.TranslateError` with its message.
    #[error("{0}")]
    Translate(String),
    /// Where `xenos_hlsl.py` would stop on another exception: the caller runs the Python.
    #[error("not translated here: {0}")]
    Unsupported(&'static str),
}

type Result<T> = std::result::Result<T, TranslateError>;

fn terr(m: impl Into<String>) -> TranslateError {
    TranslateError::Translate(m.into())
}

/// `ENGINE_SIZE`: an engine constant's natural size in float4 registers.
#[must_use]
pub fn engine_size(name: &str) -> Option<u32> {
    Some(match name {
        "viewMatrix"
        | "viewProjMatrix"
        | "worldViewProjMatrix"
        | "worldMatrix"
        | "outdoorLightShadowTransform0"
        | "outdoorLightShadowTransform1"
        | "outdoorLightShadowTransform2" => 4,
        "viewportZMinMax"
        | "cameraPos"
        | "time"
        | "invScreenSize"
        | "pointLightPos0"
        | "pointLightColor0"
        | "pointLightRadius0"
        | "outdoorLightDir"
        | "outdoorLightHemisphereDir"
        | "outdoorLightKeyColor"
        | "outdoorLightKeySpecularColorAndHemisphereVisibility"
        | "outdoorLightTopColor"
        | "outdoorLightBottomColor"
        | "outdoorLightShadowmapSizeAndInvSize"
        | "transparentStartAndEndAndClamp" => 1,
        "boneMatrices" | "boneVectors" => 180,
        _ => return None,
    })
}

fn instanced_name(name: &str) -> Option<&'static str> {
    match name {
        "worldMatrix" => Some("instW"),
        "worldViewProjMatrix" => Some("instWVP"),
        _ => None,
    }
}

/// `xenos_hlsl.Interface`: what the PC side provides. The bindings fill it from the Python object; a material
/// value is its rows (a flat tuple of numbers is one row, as `xenos_hlsl` reads it).
#[derive(Debug, Clone, PartialEq)]
pub struct Interface {
    /// `interpolators`: TEXCOORD usage indexes shared by the VS and PS.
    pub interpolators: Vec<i64>,
    /// `vs_inputs`: usage index -> HLSL type.
    pub vs_inputs: Vec<(i64, String)>,
    /// `material`: name -> rows.
    pub material: HashMap<String, Vec<Vec<f64>>>,
    /// `sampler_fix`: sampler register -> swizzle, or a wrapping expression with `{}`.
    pub sampler_fix: HashMap<i64, String>,
    /// `colour_scale`.
    pub colour_scale: f64,
    /// `instanced`.
    pub instanced: bool,
    /// `bones_as_world`.
    pub bones_as_world: bool,
    /// `material_regs`: register -> values.
    pub material_regs: HashMap<i64, Vec<f64>>,
    /// `pc_shadow_atlas`.
    pub pc_shadow_atlas: bool,
    /// `sampler_registers`.
    pub sampler_registers: HashMap<i64, i64>,
    /// `named_vs`.
    pub named_vs: HashMap<i64, String>,
    /// `named_ps`.
    pub named_ps: HashMap<i64, String>,
    /// `fetch_registers`.
    pub fetch_registers: HashMap<i64, i64>,
    /// `trace_nan` (debug).
    pub trace_nan: bool,
    /// `probe` (debug): None when empty.
    pub probe: Option<Vec<String>>,
}

impl Interface {
    /// `Interface(interpolators)` with every other field at its default.
    #[must_use]
    pub fn new(interpolators: Vec<i64>) -> Self {
        Self {
            interpolators,
            vs_inputs: Vec::new(),
            material: HashMap::new(),
            sampler_fix: HashMap::new(),
            colour_scale: 1.0,
            instanced: false,
            bones_as_world: false,
            material_regs: HashMap::new(),
            pc_shadow_atlas: false,
            sampler_registers: HashMap::new(),
            named_vs: HashMap::new(),
            named_ps: HashMap::new(),
            fetch_registers: HashMap::new(),
            trace_nan: false,
            probe: None,
        }
    }
}

/// `_f`: a number as HLSL (NaN 0, infinities +-`FLT_MAX`, else Python's repr with a `.0` on whole numbers).
#[must_use]
pub fn hlsl_float(v: f64) -> String {
    if v.is_nan() {
        return "0.0".into();
    }
    if v.is_infinite() {
        return if v > 0.0 { FLT_MAX.into() } else { format!("-{FLT_MAX}") };
    }
    float_repr(v)
}

fn floats(v: &[f64]) -> String {
    v.iter().map(|&x| hlsl_float(x)).collect::<Vec<_>>().join(", ")
}

fn py_true(b: bool) -> &'static str {
    if b { "true" } else { "false" }
}

/// `str.format(e)` for a sampler fix: `{{`, `}}` and one `{}`; anything else is the Python's to raise.
fn format_fix(fix: &str, e: &str) -> Result<String> {
    let mut out = String::with_capacity(fix.len() + e.len());
    let mut used = false;
    let mut it = fix.chars().peekable();
    while let Some(c) = it.next() {
        match c {
            '{' => match it.next() {
                Some('{') => out.push('{'),
                Some('}') if !used => {
                    out.push_str(e);
                    used = true;
                }
                _ => return Err(TranslateError::Unsupported("a sampler fix with other format fields")),
            },
            '}' => {
                if it.next() == Some('}') {
                    out.push('}');
                } else {
                    return Err(TranslateError::Unsupported("a sampler fix with a single '}'"));
                }
            }
            c => out.push(c),
        }
    }
    Ok(out)
}

fn is_identifier(s: &str) -> Option<bool> {
    if !s.is_ascii() {
        return None;
    }
    let mut b = s.bytes();
    Some(
        b.next().is_some_and(|c| c == b'_' || c.is_ascii_alphabetic())
            && b.all(|c| c == b'_' || c.is_ascii_alphanumeric()),
    )
}

/// The text being assembled: each line followed by a newline (`'\n'.join(lines) + '\n'`).
struct Text(String);

impl Text {
    fn push(&mut self, line: impl AsRef<str>) {
        self.0.push_str(line.as_ref());
        self.0.push('\n');
    }

    fn extend<S: AsRef<str>>(&mut self, lines: impl IntoIterator<Item = S>) {
        for l in lines {
            self.push(l);
        }
    }

    fn append(&mut self, lines: &mut Vec<String>) {
        self.extend(lines.drain(..));
    }
}

struct Sampler {
    tex: String,
    typ: &'static str,
    name: String,
}

struct Emitter<'a> {
    sh: &'a Shader,
    iface: &'a Interface,
    ps: bool,
    lines: String,
    depth: usize,
    cmap: HashMap<i64, String>,
    globals: Vec<String>,
    named: Vec<String>,
    statics: Vec<String>,
    textures: Vec<String>,
    used_regs: HashSet<i64>,
    samplers: HashMap<i64, Sampler>,
    vfetch_element: HashMap<u32, u32>,
    block_pred: Option<bool>,
    pred_written: bool,
    pending_a0: Option<String>,
    cube_dir_set: bool,
    need_world: bool,
    cube_given: Option<bool>,
}

impl<'a> Emitter<'a> {
    fn new(sh: &'a Shader, iface: &'a Interface) -> Self {
        Self {
            sh,
            iface,
            ps: sh.is_pixel,
            lines: String::with_capacity(16 * 1024),
            depth: 1,
            cmap: HashMap::new(),
            globals: Vec::new(),
            named: Vec::new(),
            statics: Vec::new(),
            textures: Vec::new(),
            used_regs: HashSet::new(),
            samplers: HashMap::new(),
            vfetch_element: HashMap::new(),
            block_pred: None,
            pred_written: false,
            pending_a0: None,
            cube_dir_set: false,
            need_world: false,
            cube_given: None,
        }
    }

    fn w(&mut self, s: impl AsRef<str>) {
        for _ in 0..self.depth {
            self.lines.push_str("    ");
        }
        self.lines.push_str(s.as_ref());
        self.lines.push('\n');
    }

    // ------------------------------------------------------------------------------------------- declarations
    fn constants(&mut self) -> Result<()> {
        let mut declared: HashSet<String> = HashSet::new();
        for c in self.sh.constants() {
            if !c.regset.is("float4") {
                continue;
            }
            let reg = i64::from(c.reg);
            let count = i64::from(c.count);
            if let Some(size) = engine_size(&c.name) {
                let n = size.max(u32::from(c.count));
                let inst = if self.iface.instanced {
                    instanced_name(&c.name)
                } else {
                    None
                };
                if inst.is_some() {
                    for need in ["worldMatrix", "viewProjMatrix"] {
                        if declared.insert(need.to_owned()) {
                            self.globals.push(format!("float4 {need}[4];"));
                        }
                    }
                } else if declared.insert(c.name.clone()) {
                    self.globals.push(format!("float4 {}[{n}];", c.name));
                }
                for k in 0..count {
                    let e = match inst {
                        Some(i) => format!("{i}[{k}]"),
                        None => format!("{}[{k}]", c.name),
                    };
                    self.cmap.insert(reg + k, e);
                }
                continue;
            }
            let mut vals: Option<Vec<Vec<f64>>> = self.iface.material.get(&c.name).cloned();
            if vals.is_none() && c.name == "colorScale" {
                vals = Some(vec![vec![self.iface.colour_scale; 4]]);
            }
            let regs = &self.iface.material_regs;
            if vals.is_none() && c.name == "constants" && (0..count).any(|k| regs.contains_key(&(reg + k))) {
                vals = Some(
                    (0..count)
                        .map(|k| regs.get(&(reg + k)).cloned().unwrap_or_else(|| vec![0.0; 4]))
                        .collect(),
                );
                let unset: Vec<i64> = (0..count).filter(|k| !regs.contains_key(&(reg + k))).collect();
                if !unset.is_empty() {
                    self.statics.push(format!(
                        "// material constant {}: no value given for {unset:?}, 0",
                        c.name
                    ));
                }
            }
            let vals = if let Some(v) = vals {
                v
            } else {
                self.statics
                    .push(format!("// material constant {}: no value given, 0", c.name));
                vec![vec![0.0; 4]; count as usize]
            };
            if vals.is_empty() {
                return Err(TranslateError::Unsupported("an empty material value"));
            }
            let named = if self.ps {
                &self.iface.named_ps
            } else {
                &self.iface.named_vs
            };
            for k in 0..count {
                if let Some(nm) = named.get(&(reg + k)) {
                    match is_identifier(nm) {
                        Some(true) if engine_size(nm).is_none() => {}
                        // the message holds Python's repr of the name: plain quotes for printable ASCII only
                        Some(_) if nm.bytes().all(|c| (0x20..0x7f).contains(&c) && !b"'\"\\".contains(&c)) => {
                            return Err(terr(format!("engine-named constant '{nm}': not a usable name")));
                        }
                        _ => return Err(TranslateError::Unsupported("an engine-named constant Python must name")),
                    }
                    let decl = format!("float4 {nm};");
                    if !self.named.contains(&decl) {
                        self.named.push(decl);
                    }
                    self.cmap.insert(reg + k, nm.clone());
                    continue;
                }
                let zero = [0.0; 4];
                let v: &[f64] = vals.get(k as usize).map_or(&zero, Vec::as_slice);
                let nm = format!("m_{}_{k}", c.name);
                self.statics
                    .push(format!("static const float4 {nm} = float4({});", floats(v)));
                self.cmap.insert(reg + k, nm);
            }
        }
        let mut lits: Vec<&(i64, [f32; 4])> = self.sh.literals.iter().collect();
        lits.sort_by_key(|l| l.0);
        for (reg, v) in lits {
            let nm = format!("lit{reg}");
            let v: Vec<f64> = v.iter().map(|&x| f64::from(x)).collect();
            self.statics
                .push(format!("static const float4 {nm} = float4({});", floats(&v)));
            self.cmap.insert(*reg, nm);
        }
        for c in self.sh.constants() {
            if !c.regset.is("sampler") {
                continue;
            }
            let name = c.name.clone();
            let tex = format!("texture_{}", name.strip_prefix("sampler_").unwrap_or(&name));
            let typ = if c.type_.type_.is("samplerCube") {
                "TextureCube"
            } else if c.type_.type_.is("sampler3D") {
                "Texture3D"
            } else if c.type_.type_.is("sampler1D") {
                "Texture1D"
            } else {
                "Texture2D"
            };
            self.textures.push(format!("{typ} {tex} : register(t{});", c.reg));
            self.samplers.insert(i64::from(c.reg), Sampler { tex, typ, name });
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------- operands
    fn constant(&mut self, idx: u32, rel: Option<&str>) -> Result<String> {
        let idx = i64::from(idx);
        if let Some(rel) = rel {
            if rel != "a0" {
                return Err(terr(format!("loop-relative constant addressing (c[{idx}+{rel}])")));
            }
            for c in self.sh.constants() {
                let reg = i64::from(c.reg);
                if c.regset.is("float4")
                    && reg <= idx
                    && idx < reg + i64::from(c.count)
                    && engine_size(&c.name).is_some()
                {
                    if self.iface.bones_as_world
                        && (c.name == "boneMatrices" || c.name == "boneVectors")
                        && (c.type_.cls.is("matrix_rows") || c.type_.cls.is("matrix_columns"))
                        && idx - reg < 3
                    {
                        self.need_world = true;
                        let m = if self.iface.instanced { "instW" } else { "worldMatrix" };
                        return Ok(format!("{m}[{}]", idx - reg));
                    }
                    return Ok(format!("{}[a0 + {}]", c.name, idx - reg));
                }
            }
            return Err(terr(format!(
                "relative constant addressing c[{idx}+a0] outside an engine array"
            )));
        }
        Ok(self
            .cmap
            .get(&idx)
            .cloned()
            .unwrap_or_else(|| "float4(0.0, 0.0, 0.0, 0.0)".into()))
    }

    fn reg(&mut self, i: u32) -> String {
        self.used_regs.insert(i64::from(i));
        format!("r{i}")
    }

    fn operand(&mut self, o: &Operand, swz: Option<&str>) -> Result<String> {
        let base = if o.file == File::Temp {
            if o.rel.is_some() {
                return Err(terr("relative temp addressing is not supported"));
            }
            self.reg(o.index)
        } else {
            self.constant(o.index, o.rel)?
        };
        let s = swz.unwrap_or_else(|| o.swizzle());
        let mut e = format!("{base}.{s}");
        if o.abs {
            e = format!("abs({e})");
        }
        if o.negate {
            e = format!("(-{e})");
        }
        Ok(e)
    }

    // ------------------------------------------------------------------------------------------- ALU
    #[allow(clippy::too_many_lines)]
    fn vector(&mut self, ins: &Alu) -> Result<String> {
        let op = ins.vector_op();
        let mut a = Vec::with_capacity(ins.vector_operands.len());
        for o in &ins.vector_operands {
            a.push(self.operand(o, None)?);
        }
        let g = |i: usize| a.get(i).map_or("", String::as_str);
        let cmp2 = |op: &str| match op {
            "eq" | "seq" | "cndeq" => "==",
            "gt" | "sgt" | "cndgt" => ">",
            "ge" | "sge" | "cndge" => ">=",
            _ => "!=",
        };
        Ok(match op {
            "add" => format!("({} + {})", g(0), g(1)),
            "mul" => format!("xmul({}, {})", g(0), g(1)),
            "max" => format!("max({}, {})", g(0), g(1)),
            "min" => format!("min({}, {})", g(0), g(1)),
            "seq" | "sgt" | "sge" | "sne" => format!("float4(({}) {} ({}))", g(0), cmp2(op), g(1)),
            "frc" => format!("frac({})", g(0)),
            "trunc" => format!("trunc({})", g(0)),
            "floor" => format!("floor({})", g(0)),
            "mad" => format!("(xmul({}, {}) + {})", g(0), g(1), g(2)),
            "cndeq" | "cndge" | "cndgt" => format!("xsel(({}) {} 0.0, {}, {})", g(0), cmp2(op), g(1), g(2)),
            "dp4" => format!("xdot4({}, {}).xxxx", g(0), g(1)),
            "dp3" => format!("xdot4(float4(({}).xyz, 0.0), float4(({}).xyz, 0.0)).xxxx", g(0), g(1)),
            "dp2add" => format!(
                "(xdot4(float4(({}).xy, 0.0, 0.0), float4(({}).xy, 0.0, 0.0)) + ({}).x).xxxx",
                g(0),
                g(1),
                g(2)
            ),
            "cube" => {
                let l = format!("cubeDir = float3(({0}).z, ({0}).w, ({0}).x);", g(0));
                self.w(l);
                "xcube(cubeDir)".into()
            }
            "max4" => format!("xmax4({})", g(0)),
            "setp_eq_push" | "setp_ne_push" | "setp_gt_push" | "setp_ge_push" => {
                let cmp = cmp2(op.get(5..7).unwrap_or(""));
                let l = format!("p0_next = (({}).w == 0.0 && ({}).w {cmp} 0.0);", g(0), g(1));
                self.w(l);
                self.pred_written = true;
                format!(
                    "((({0}).x == 0.0 && ({1}).x {cmp} 0.0) ? 0.0 : ({0}).x + 1.0).xxxx",
                    g(0),
                    g(1)
                )
            }
            "kill_eq" | "kill_gt" | "kill_ge" | "kill_ne" => {
                let cmp = cmp2(op.get(5..).unwrap_or(""));
                let l = format!("if (any(({}) {cmp} ({}))) discard;", g(0), g(1));
                self.w(l);
                format!("float4(any(({}) {cmp} ({})).xxxx)", g(0), g(1))
            }
            "dst" => format!("float4(1.0, ({0}).y * ({1}).y, ({0}).z, ({1}).w)", g(0), g(1)),
            "maxa" => {
                self.pending_a0 = Some(format!("a0 = (int)clamp(floor(({}).w + 0.5), -256.0, 255.0);", g(0)));
                format!("max({}, {})", g(0), g(1))
            }
            _ => return Err(terr(format!("vector op {op}"))),
        })
    }

    #[allow(clippy::too_many_lines)]
    fn scalar(&mut self, ins: &Alu) -> Result<String> {
        let op = ins.scalar_op();
        let ops = &ins.scalar_operands;
        if op == "retain_prev" {
            return Ok("ps".into());
        }
        if ops.is_empty() {
            if op == "setp_clr" {
                self.w("p0_next = false;");
                self.pred_written = true;
                return Ok(FLT_MAX.into());
            }
            return Err(terr(format!("scalar op {op}")));
        }
        let comps = |o: &Operand| o.components.clone().unwrap_or_default();
        let (a, b): (String, Option<String>) = if let [o0, o1] = ops.as_slice() {
            let a = self.operand(o0, Some(&comps(o0)))?;
            let b = self.operand(o1, Some(&comps(o1)))?;
            (a, Some(b))
        } else {
            let o = ops.first().ok_or(TranslateError::Unsupported("scalar operands"))?;
            let cs: Vec<char> = comps(o).chars().collect();
            let c0 = cs.first().map_or_default(char::to_string);
            let a = self.operand(o, Some(&c0))?;
            let b = match cs.get(1) {
                Some(c1) => Some(self.operand(o, Some(&c1.to_string()))?),
                None => None,
            };
            (a, b)
        };
        let b = b.as_deref().unwrap_or("None");
        let cmp = |s: &str| match s {
            "eq" | "seqs" | "one" => "==",
            "gt" | "sgts" => ">",
            "ge" | "sges" => ">=",
            _ => "!=",
        };
        Ok(match op {
            "adds" | "addsc" => format!("({a} + {b})"),
            "adds_prev" => format!("({a} + ps)"),
            "muls" | "mulsc" => format!("xmul1({a}, {b})"),
            "muls_prev" => format!("xmul1({a}, ps)"),
            "muls_prev2" => format!(
                "((ps == -{FLT_MAX} || !isfinite(ps) || !isfinite({b}) || {b} <= 0.0) ? -{FLT_MAX} : xmul1({a}, ps))"
            ),
            "maxs" => format!("max({a}, {b})"),
            "mins" => format!("min({a}, {b})"),
            "seqs" | "sgts" | "sges" | "snes" => format!("(({a}) {} 0.0 ? 1.0 : 0.0)", cmp(op)),
            "frcs" => format!("frac({a})"),
            "truncs" => format!("trunc({a})"),
            "floors" => format!("floor({a})"),
            "exp" => format!("exp2({a})"),
            "logc" => format!("xlog({a})"),
            "log" => format!("(({a}) == 1.0 ? 0.0 : log2({a}))"),
            "rcp" | "rcpc" | "rcpf" | "rsq" | "rsqc" | "rsqf" => {
                format!("x{}{}({a})", op.get(..3).unwrap_or(""), op.get(3..).unwrap_or(""))
            }
            "maxas" | "maxasf" => {
                let rnd = if op == "maxas" { " + 0.5" } else { "" };
                self.pending_a0 = Some(format!("a0 = (int)clamp(floor({a}{rnd}), -256.0, 255.0);"));
                format!("max({a}, {b})")
            }
            "subs" | "subsc" => format!("({a} - {b})"),
            "subs_prev" => format!("({a} - ps)"),
            "setp_eq" | "setp_ne" | "setp_gt" | "setp_ge" => {
                let c = cmp(op.get(5..).unwrap_or(""));
                self.w(format!("p0_next = ({a}) {c} 0.0;"));
                self.pred_written = true;
                format!("(({a}) {c} 0.0 ? 0.0 : 1.0)")
            }
            "setp_inv" => {
                self.w(format!("p0_next = ({a}) == 1.0;"));
                self.pred_written = true;
                format!("(({a}) == 1.0 ? 0.0 : (({a}) == 0.0 ? 1.0 : ({a})))")
            }
            "setp_pop" => {
                self.w(format!("p0_next = ({a}) - 1.0 <= 0.0;"));
                self.pred_written = true;
                format!("(({a}) - 1.0 <= 0.0 ? 0.0 : ({a}) - 1.0)")
            }
            "setp_rstr" => {
                self.w(format!("p0_next = ({a}) == 0.0;"));
                self.pred_written = true;
                format!("(({a}) == 0.0 ? 0.0 : ({a}))")
            }
            "kills_eq" | "kills_gt" | "kills_ge" | "kills_ne" | "kills_one" => {
                let c = cmp(op.get(6..).unwrap_or(""));
                let rhs = if op == "kills_one" { "1.0" } else { "0.0" };
                self.w(format!("if (({a}) {c} {rhs}) discard;"));
                format!("(({a}) {c} {rhs} ? 1.0 : 0.0)")
            }
            "sqrt" => format!("sqrt({a})"),
            "sin" => format!("sin({a})"),
            "cos" => format!("cos({a})"),
            _ => return Err(terr(format!("scalar op {op}"))),
        })
    }

    fn out_name(d: &Dest) -> Result<String> {
        let idx = d.index.map_or_else(|| "None".to_owned(), |i| i.to_string());
        match d.file.name() {
            "oPos" => Ok("o.pos".into()),
            "o" => Ok(format!("o.t{idx}")),
            "oC" => Ok(format!("oC{idx}")),
            f => Err(terr(format!("export to {f}"))),
        }
    }

    fn write(&mut self, dest: &Dest, value: &str, is_vector: bool) -> Result<()> {
        let target = if dest.file == File::Temp {
            if dest.rel.is_some() {
                return Err(terr("relative destination"));
            }
            self.reg(dest.index.unwrap_or(0))
        } else {
            Self::out_name(dest)?
        };
        for (c, ch) in "xyzw".chars().zip(dest.write.chars()) {
            match ch {
                '_' => {}
                '0' | '1' => self.w(format!("{target}.{c} = {ch}.0;")),
                _ => {
                    let mut src = if is_vector {
                        format!("{value}.{c}")
                    } else {
                        value.to_owned()
                    };
                    if dest.saturate {
                        src = format!("saturate({src})");
                    }
                    self.w(format!("{target}.{c} = {src};"));
                }
            }
        }
        Ok(())
    }

    fn alu(&mut self, address: u32, ins: &Alu) -> Result<()> {
        self.pred_written = false;
        self.pending_a0 = None;
        let k = address;
        let vec = if ins.vector_nop { None } else { Some(self.vector(ins)?) };
        let sca = if ins.scalar_nop { None } else { Some(self.scalar(ins)?) };
        if let Some(v) = &vec {
            self.w(format!("float4 v{k} = {v};"));
            if self.iface.trace_nan {
                self.w(format!(
                    "if (firstNan < 0.0 && any(isnan(v{k}) || isinf(v{k}))) firstNan = {k}.0;"
                ));
            }
        }
        if let Some(s) = &sca {
            self.w(format!("float s{k} = {s};"));
            if self.iface.trace_nan {
                self.w(format!(
                    "if (firstNan < 0.0 && (isnan(s{k}) || isinf(s{k}))) firstNan = {k}.5;"
                ));
            }
        }
        if let Some(a0) = self.pending_a0.take() {
            self.w(a0);
        }
        if vec.is_some() {
            self.write(&ins.vector_dest, &format!("v{k}"), true)?;
        }
        if sca.is_some() {
            self.write(&ins.scalar_dest, &format!("s{k}"), false)?;
            self.w(format!("ps = s{k};"));
        }
        if self.pred_written {
            self.w("p0 = p0_next;");
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------- fetch
    #[allow(clippy::too_many_lines)]
    fn fetch(&mut self, address: u32, ins: &Fetch) -> Result<()> {
        let t = match &ins.kind {
            FetchKind::Vertex(_) => {
                let el = self
                    .vfetch_element
                    .get(&address)
                    .copied()
                    .ok_or_else(|| terr(format!("vfetch at {address} has no vertex element")))?;
                self.fetch_write(ins, &format!("float4(vin.t{el})"));
                return Ok(());
            }
            FetchKind::Texture(t) => t,
        };
        let op = ins.op.as_str();
        if op == "setTexLOD" {
            let r = self.reg(ins.src);
            let c = t.src_swizzle.chars().next().map_or_default(String::from);
            self.w(format!("texLod = {r}.{c};"));
            return Ok(());
        }
        if !t.has_dest {
            return Ok(());
        }
        let ci = i64::from(t.const_index);
        let reg = self.iface.fetch_registers.get(&ci).copied().unwrap_or(ci);
        let Some(s) = self.samplers.get(&reg) else {
            return Err(terr(format!(
                "fetch from tf{reg}: no sampler s{reg} in the constant table"
            )));
        };
        let (tex, typ, name) = (s.tex.clone(), s.typ, s.name.clone());
        let sampler = format!(
            "sampler{}",
            self.iface.sampler_registers.get(&reg).copied().unwrap_or(1)
        );
        let r = self.reg(ins.src);
        let mut src = format!("{r}.{}", t.src_swizzle);
        if typ != "Texture2D" && typ != "TextureCube" {
            return Err(terr(format!("{typ} sampling is not supported")));
        }
        if t.use_register_gradients {
            return Err(terr("texture fetch with explicit gradients is not supported"));
        }
        if t.unnormalized {
            if typ != "Texture2D" {
                return Err(terr("unnormalized cube sampling is not supported"));
            }
            src = format!("({src} / tdim({tex}))");
        }
        let explicit_lod = t.use_register_lod || !t.use_computed_lod || !self.ps;
        let lod = if t.use_register_lod {
            format!("(texLod + {})", hlsl_float(t.lod_bias))
        } else {
            hlsl_float(t.lod_bias)
        };
        if name == SHADOW && self.iface.pc_shadow_atlas {
            if t.unnormalized {
                return Err(terr("PC shadow atlas requires normalized console coordinates"));
            }
            let swizzle: String = (0..3)
                .map(|k| {
                    ['x', 'y', 'z', 'w']
                        .get(((t.src_swizzle_raw >> (2 * k)) & 3) as usize)
                        .copied()
                        .unwrap_or('x')
                })
                .collect();
            src = format!("xshadowuv({r}.{swizzle})");
        }
        let (o0, o1) = (hlsl_float(t.offset_x), hlsl_float(t.offset_y));
        if op.starts_with("getWeights") {
            self.w(format!(
                "float4 f{address} = float4(frac({src} * tdim({tex}) - 0.5 + float2({o0}, {o1})), 0.0, 0.0);"
            ));
            self.fetch_write(ins, &format!("f{address}"));
            return Ok(());
        }
        if !op.starts_with("tfetch") {
            return Err(terr(format!("fetch op {op}")));
        }
        let sample = |coord: &str| {
            if explicit_lod {
                format!("{tex}.SampleLevel({sampler}, {coord}, {lod})")
            } else if t.lod_bias != 0.0 {
                format!("{tex}.SampleBias({sampler}, {coord}, {})", hlsl_float(t.lod_bias))
            } else {
                format!("{tex}.Sample({sampler}, {coord})")
            }
        };
        let mut e = if typ == "TextureCube" {
            let coord = if self.cube_dir_set {
                format!("xcubefetch({r}.{})", t.src_swizzle)
            } else {
                src.clone()
            };
            sample(&coord)
        } else if name == SHADOW || (t.mag_filter == "point" && t.min_filter == "point") {
            if explicit_lod {
                let dim = format!("fetchDim{address}");
                self.w(format!("float3 {dim} = tsizeLod({tex}, {lod});"));
                format!(
                    "{tex}.Load(int3(clamp(floor({src} * {dim}.xy + float2({o0}, {o1})), 0.0, {dim}.xy - 1.0), {dim}.z))"
                )
            } else {
                format!(
                    "{tex}.Load(int3(clamp(floor({src} * tdim({tex}) + float2({o0}, {o1})), 0.0, tdim({tex}) - 1.0), 0))"
                )
            }
        } else {
            let coord = if t.offset_x == 0.0 && t.offset_y == 0.0 {
                src.clone()
            } else {
                format!("({src} + float2({o0}, {o1}) / tdim({tex}))")
            };
            sample(&coord)
        };
        if let Some(fix) = self.iface.sampler_fix.get(&reg).filter(|f| !f.is_empty()) {
            e = if fix.contains("{}") {
                format_fix(fix, &e)?
            } else {
                format!("({e}).{fix}")
            };
        }
        self.w(format!("float4 f{address} = {e};"));
        self.fetch_write(ins, &format!("f{address}"));
        Ok(())
    }

    fn fetch_write(&mut self, ins: &Fetch, val: &str) {
        let target = self.reg(ins.dst);
        for (c, ch) in "xyzw".chars().zip(ins.dst_swizzle.chars()) {
            match ch {
                '_' | '?' => {}
                '0' | '1' => self.w(format!("{target}.{c} = {ch}.0;")),
                _ => self.w(format!("{target}.{c} = {val}.{ch};")),
            }
        }
    }

    // ------------------------------------------------------------------------------------------- control flow
    fn instr(&mut self, ins: &Instr) -> Result<()> {
        let (predicated, cond) = match &ins.kind {
            InstrKind::Alu(a) => (a.predicated, a.pred_condition),
            InstrKind::Fetch(f) => (f.predicated, f.pred_condition),
        };
        let pred = predicated && self.block_pred != Some(cond);
        if pred {
            self.w(format!("if (p0 == {}) {{", py_true(cond)));
            self.depth += 1;
        }
        match &ins.kind {
            InstrKind::Alu(a) => self.alu(ins.address, a)?,
            InstrKind::Fetch(f) => self.fetch(ins.address, f)?,
        }
        if pred {
            self.depth -= 1;
            self.w("}");
        }
        Ok(())
    }

    fn cfs(&mut self, lst: &[Cf], start: usize, end: usize) -> Result<()> {
        let sh = self.sh;
        let mut i = start;
        while i < end {
            let Some(cf) = lst.get(i) else { break };
            let op = cf.op();
            match op {
                "exec" | "exec_end" | "cond_exec_pred" | "cond_exec_pred_end" => {
                    let (is_end, condition) = match cf.kind {
                        CfKind::Exec { is_end, condition, .. } => (is_end, condition.unwrap_or(0) != 0),
                        _ => (false, false),
                    };
                    let pred_block = op.starts_with("cond_exec_pred");
                    if pred_block {
                        self.w(format!("if (p0 == {}) {{", py_true(condition)));
                        self.depth += 1;
                        self.block_pred = Some(condition);
                    } else {
                        self.block_pred = None;
                    }
                    for &k in cf.instructions.as_deref().unwrap_or(&[]) {
                        if let Some(ins) = sh.instructions.get(k) {
                            self.instr(ins)?;
                        }
                    }
                    if pred_block {
                        self.depth -= 1;
                        self.w("}");
                    }
                    self.block_pred = None;
                    if is_end && i != lst.len() - 1 && lst.iter().skip(i + 1).any(|c| c.op() != "alloc") {
                        return Err(terr("exec_end before the last CF"));
                    }
                    i += 1;
                }
                "alloc" | "nop" => i += 1,
                "cond_jmp" => {
                    let CfKind::Jump {
                        address,
                        type_,
                        condition,
                        ..
                    } = cf.kind
                    else {
                        return Err(TranslateError::Unsupported("a jump without its fields"));
                    };
                    let cond = match type_ {
                        CfType::Unconditional => "false".to_owned(),
                        CfType::Predicated => format!("p0 != {}", py_true(condition != 0)),
                        CfType::Conditional => return Err(terr("bool-constant jump")),
                    };
                    let t = lst.iter().position(|c| c.index == address);
                    let t = match t {
                        Some(t) if t > i && t <= end => t,
                        _ => return Err(terr(format!("jump to cf {address} is not a nested forward jump"))),
                    };
                    self.w(format!("if ({cond}) {{"));
                    self.depth += 1;
                    self.cfs(lst, i + 1, t)?;
                    self.depth -= 1;
                    self.w("}");
                    i = t;
                }
                _ => return Err(terr(format!("control flow {op} is not supported"))),
            }
        }
        Ok(())
    }

    fn has_non_nested_forward_jumps(lst: &[Cf]) -> bool {
        fn check(lst: &[Cf], start: usize, end: usize) -> std::result::Result<(), bool> {
            let mut i = start;
            while i < end {
                let Some(cf) = lst.get(i) else { break };
                if cf.op() == "cond_jmp"
                    && let CfKind::Jump { address, .. } = cf.kind
                {
                    match lst.iter().position(|c| c.index == address) {
                        Some(t) if t > i && t <= end => {
                            check(lst, i + 1, t)?;
                            i = t;
                            continue;
                        }
                        Some(t) if t > i => return Err(true),
                        _ => return Err(false),
                    }
                }
                i += 1;
            }
            Ok(())
        }
        // Only a shader whose every jump the forward emitter takes (forward, not on a bool constant): any other
        // keeps the nested emitter, so it fails with the same error as before.
        let all_forward = lst.iter().enumerate().all(|(i, cf)| match cf.kind {
            CfKind::Jump { address, type_, .. } if cf.op() == "cond_jmp" => {
                !matches!(type_, CfType::Conditional)
                    && lst.iter().position(|c| c.index == address).is_some_and(|t| t > i)
            }
            _ => true,
        });
        all_forward && matches!(check(lst, 0, lst.len()), Err(true))
    }

    /// Forward-only CFG emitter for shaders with overlapping forward jumps (e.g. if/else blocks).
    /// Emits `int _cf = 0;` with `if (_cf <= k)` guards so any forward jump `k -> t` (`t > k`)
    /// is mathematically exact without loops (spec XSH-CF-1). The blocks it takes are the nested emitter's:
    /// a `cond_exec` on a bool constant is refused as there (`control flow cond_exec is not supported`), not run
    /// unconditionally.
    fn cfs_forward(&mut self, lst: &[Cf]) -> Result<()> {
        let sh = self.sh;
        self.w("int _cf = 0;");
        for (i, cf) in lst.iter().enumerate() {
            let op = cf.op();
            match op {
                "exec" | "exec_end" | "cond_exec_pred" | "cond_exec_pred_end" => {
                    let (is_end, condition) = match cf.kind {
                        CfKind::Exec { is_end, condition, .. } => (is_end, condition.unwrap_or(0) != 0),
                        _ => (false, false),
                    };
                    if is_end && i != lst.len() - 1 && lst.iter().skip(i + 1).any(|c| c.op() != "alloc") {
                        return Err(terr("exec_end before the last CF"));
                    }
                    self.w(format!("if (_cf <= {i}) {{"));
                    self.depth += 1;
                    let pred_block = op.starts_with("cond_exec_pred");
                    if pred_block {
                        self.w(format!("if (p0 == {}) {{", py_true(condition)));
                        self.depth += 1;
                        self.block_pred = Some(condition);
                    } else {
                        self.block_pred = None;
                    }
                    for &k in cf.instructions.as_deref().unwrap_or(&[]) {
                        if let Some(ins) = sh.instructions.get(k) {
                            self.instr(ins)?;
                        }
                    }
                    if pred_block {
                        self.depth -= 1;
                        self.w("}");
                    }
                    self.block_pred = None;
                    self.depth -= 1;
                    self.w("}");
                }
                "alloc" | "nop" => {}
                "cond_jmp" => {
                    let CfKind::Jump {
                        address,
                        type_,
                        condition,
                        ..
                    } = cf.kind
                    else {
                        return Err(TranslateError::Unsupported("a jump without its fields"));
                    };
                    let t = match lst.iter().position(|c| c.index == address) {
                        Some(t) if t > i => t,
                        _ => return Err(terr(format!("jump to cf {address} is not a nested forward jump"))),
                    };
                    let jump_cond = match type_ {
                        CfType::Unconditional => "true".to_owned(),
                        CfType::Predicated => format!("p0 == {}", py_true(condition != 0)),
                        CfType::Conditional => return Err(terr("bool-constant jump")),
                    };
                    self.w(format!("if (_cf <= {i} && ({jump_cond})) {{ _cf = {t}; }}"));
                }
                _ => return Err(terr(format!("control flow {op} is not supported"))),
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------------------------------- whole shader
    fn run(mut self) -> Result<String> {
        let sh = self.sh;
        self.block_pred = None;
        self.cube_dir_set = self.cube_given.unwrap_or_else(|| {
            sh.instructions
                .iter()
                .any(|i| matches!(&i.kind, InstrKind::Alu(a) if a.vector_op() == "cube" && !a.vector_nop))
        });
        self.constants()?;
        if !self.ps {
            for el in &sh.header.vertex_elements {
                if el.usage != "TEXCOORD" {
                    return Err(terr(format!("vertex input usage {}", el.usage)));
                }
                self.vfetch_element.insert(el.fetch_address, el.usage_index);
            }
        }
        if Self::has_non_nested_forward_jumps(&sh.cf) {
            self.cfs_forward(&sh.cf)?;
        } else {
            self.cfs(&sh.cf, 0, sh.cf.len())?;
        }
        let body = std::mem::take(&mut self.lines);
        self.assemble(&body)
    }

    #[allow(clippy::too_many_lines)]
    fn assemble(mut self, body: &str) -> Result<String> {
        let (sh, iface) = (self.sh, self.iface);
        let mut out = Text(String::with_capacity(body.len() + HELPERS.len() + 4096));
        out.push("// translated from Xbox 360 (Xenos) microcode");
        out.push("#define xsel(c, a, b) ((c) ? (a) : (b))");
        out.push(HELPERS);
        if self.need_world || (!self.ps && iface.instanced) {
            for need in ["worldMatrix", "viewProjMatrix"] {
                let p = format!("float4 {need}[");
                if !self.globals.iter().any(|g| g.starts_with(&p)) {
                    self.globals.push(format!("float4 {need}[4];"));
                }
            }
        }
        out.append(&mut self.globals);
        out.append(&mut self.named);
        out.append(&mut self.statics);
        out.append(&mut self.textures);
        let mut slots: Vec<i64> = iface.sampler_registers.values().copied().chain([1]).collect();
        slots.sort_unstable();
        slots.dedup();
        for slot in slots {
            if !(0..16).contains(&slot) {
                return Err(terr("Sampler register outside the PC range"));
            }
            out.push(format!("SamplerState sampler{slot} : register(s{slot});"));
        }
        let inter = &iface.interpolators;
        out.push("struct V2P {");
        out.push("    float4 pos : SV_Position;");
        out.extend(inter.iter().map(|k| format!("    float4 t{k} : TEXCOORD{k};")));
        out.push("};");
        let mut regs: Vec<i64> = self
            .used_regs
            .iter()
            .copied()
            .chain(0..i64::from(sh.header.num_temp_regs))
            .collect();
        regs.sort_unstable();
        regs.dedup();
        let mut decl0 = String::from("    float4 ");
        decl0.push_str(
            &regs
                .iter()
                .map(|i| format!("r{i} = 0.0"))
                .collect::<Vec<_>>()
                .join(", "),
        );
        decl0.push(';');
        let decl = [
            decl0,
            "    float ps = 0.0; bool p0 = false; bool p0_next = false; int a0 = 0; float texLod = 0.0;".into(),
            "    float firstNan = -1.0;".into(),
            "    float3 cubeDir = float3(0.0, 0.0, 1.0);".into(),
        ];
        let h = &sh.header;
        if self.ps {
            let outs: Vec<u32> = (0..4).filter(|k| h.outputs() >> k & 1 != 0).collect();
            let mut s = String::from("struct PSOut {");
            for k in &outs {
                let _ = write!(s, " float4 c{k} : SV_Target{k};");
            }
            s.push_str(" };");
            out.push(s);
            out.push("PSOut main(V2P i, bool frontFace : SV_IsFrontFace) {");
            out.extend(decl);
            out.extend(outs.iter().map(|k| format!("    float4 oC{k} = 0.0;")));
            for it in &h.interpolators {
                if !inter.contains(&i64::from(it.usage_index)) {
                    return Err(terr(format!(
                        "PS reads TEXCOORD{}, the VS does not write it",
                        it.usage_index
                    )));
                }
                out.push(format!("    r{} = i.t{};", it.reg, it.usage_index));
            }
            if h.sc_output_screen_xy() {
                let n = h.param_gen_pos();
                out.push(format!("    r{n} = float4(floor(i.pos.xy), 0.0, 0.0);"));
                out.push(format!(
                    "    if (!frontFace) r{n}.x = asfloat(asuint(r{n}.x) | 0x80000000u);"
                ));
            }
            out.0.push_str(body);
            out.push("    PSOut po;");
            if iface.trace_nan {
                out.push("    oC0 = float4(firstNan, 0.0, 0.0, 1.0);   // debug: first instruction giving NaN / inf");
            }
            if let Some(probe) = &iface.probe {
                let (x0, y0, pw, ph) = PROBE;
                out.push(format!(
                    "    float probeVals[{}] = {{ {} }};",
                    probe.len(),
                    probe.join(", ")
                ));
                out.push(format!(
                    "    int2 pc = int2((i.pos.xy - float2({x0}, {y0})) / float2({pw}, {ph}));"
                ));
                out.push(format!(
                    "    if (pc.x >= 0 && pc.x < 32 && pc.y >= 0 && pc.y < {})",
                    probe.len()
                ));
                out.push(
                    "        oC0 = ((asuint(probeVals[pc.y]) >> (31 - pc.x)) & 1) ? float4(2.0, 0.0, 2.0, 1.0) : float4(0.0, 2.0, 0.0, 1.0);");
            }
            out.extend(outs.iter().map(|k| format!("    po.c{k} = oC{k};")));
            out.push("    return po;");
        } else {
            out.push("struct VIn {");
            let mut inputs: Vec<&(i64, String)> = iface.vs_inputs.iter().collect();
            inputs.sort();
            out.extend(inputs.iter().map(|(k, typ)| format!("    {typ} t{k} : TEXCOORD{k};")));
            if iface.instanced {
                out.extend(
                    INSTANCE_SEMANTICS
                        .iter()
                        .enumerate()
                        .map(|(j, s)| format!("    float4 inst{j} : TEXCOORD{s};")),
                );
            }
            out.push("};");
            out.push("V2P main(VIn vin) {");
            out.extend(decl);
            if iface.instanced {
                out.push(INSTANCE_PRELUDE);
            }
            out.push("    V2P o = (V2P)0;");
            for it in &h.interpolators {
                if !inter.contains(&i64::from(it.usage_index)) {
                    return Err(terr(format!(
                        "VS writes TEXCOORD{}, not in the interface",
                        it.usage_index
                    )));
                }
            }
            out.0.push_str(body);
            out.push("    return o;");
        }
        out.push("}");
        Ok(out.0)
    }
}

/// `translate(sh, iface)`: the program as HLSL with entry point `main`.
///
/// # Errors
/// [`TranslateError`]: the Python's `TranslateError`s, or [`TranslateError::Unsupported`] where it would raise
/// another exception.
pub fn translate(sh: &Shader, iface: &Interface) -> Result<String> {
    Emitter::new(sh, iface).run()
}

/// [`translate`] of a program whose instruction list is not its own: a tree a caller handed over (the bindings'
/// `xenos_tree_program`), whose `instructions` hold only the instructions its CFs run, with `cube_dir_set` (any vector
/// `cube` among the tree's own `instructions`, which the fetches' cube coordinates depend on) given.
///
/// # Errors
/// As [`translate`].
pub fn translate_given(sh: &Shader, iface: &Interface, cube_dir_set: bool) -> Result<String> {
    let mut e = Emitter::new(sh, iface);
    e.cube_given = Some(cube_dir_set);
    e.run()
}

/// [`translate`] of many programs on all cores, in input order.
#[must_use]
pub fn translate_all(items: &[(&Shader, &Interface)]) -> Vec<Result<String>> {
    items.par_iter().map(|(sh, iface)| translate(sh, iface)).collect()
}
