//! A mesh's draws with a level's shader database, as BFBC2's Dx11 renderer picks them (`renderqa.prepare`): the mesh
//! set's shader set the database draws, each part's entry (`<mesh>/<shader>`), the entry's main-pass program for the
//! part's vertex layout, the program's compiled shaders and input layout, its second parameter block's material
//! constants and textures, the pixel shader's own sampler states and the program's render state (FORMAT.md 6).

use std::collections::HashMap;
use std::sync::Arc;

use bc2_formats::meshdata::{self, Kind};
use bc2_formats::shaderdb::{self, query};

use crate::dxbc::{self, Reflection};
use crate::files::Files;
use crate::{f32_at, half, u32_at};

/// A 16-byte GUID as the file stores it.
pub type Guid = [u8; 16];

fn guid(b: &[u8]) -> Guid {
    let mut g = [0u8; 16];
    for (d, s) in g.iter_mut().zip(b) {
        *d = *s;
    }
    g
}

/// A compiled shader of the database.
#[derive(Debug, Clone)]
pub struct Shader {
    /// DXBC.
    pub code: Arc<Vec<u8>>,
    /// Its parameter block (sampler states; a terrain vertex shader's engine-named constants).
    pub param: usize,
    /// A vertex shader's input description (section 4's tail).
    pub tail: Vec<u8>,
}

/// A program: its shaders, parameter blocks, render state and key.
#[derive(Debug, Clone)]
pub struct Program {
    /// Vertex shader.
    pub vs: usize,
    /// Pixel shader.
    pub ps: usize,
    /// First parameter block.
    pub block1: usize,
    /// Second parameter block (the material: textures and constants).
    pub block2: usize,
    /// The state word (+0x10) and the alpha-test reference (+0x14).
    pub state: (u32, u32),
    /// The program key (0x58 bytes).
    pub key: Vec<u8>,
}

impl Program {
    /// Byte `at` of the key.
    #[must_use]
    pub fn key_byte(&self, at: usize) -> u8 {
        self.key.get(at).copied().unwrap_or(0)
    }

    /// The vertex layout the program is compiled for.
    #[must_use]
    pub fn layout(&self) -> Guid {
        guid(self.key.get(0x20..0x30).unwrap_or_default())
    }
}

/// What drawing needs of a Dx11 shader database, owned.
#[derive(Debug, Clone, Default)]
pub struct Db {
    /// Entry name (lower case) -> GUID.
    pub entries: HashMap<Vec<u8>, Guid>,
    /// Mesh-part name (lower case) -> vertex layout GUID.
    pub parts: HashMap<Vec<u8>, Guid>,
    /// Vertex layout GUID -> its 0x49-byte record body.
    pub layouts: HashMap<Guid, Vec<u8>>,
    /// Entry GUID -> its programs.
    pub groups: HashMap<Guid, Vec<usize>>,
    /// Programs.
    pub programs: Vec<Program>,
    /// Vertex shaders.
    pub vs: Vec<Shader>,
    /// Pixel shaders.
    pub ps: Vec<Shader>,
    /// Parameter blocks.
    pub params: Vec<Vec<u8>>,
}

impl Db {
    /// Reads a PC `Dx11_Single.res`.
    ///
    /// # Errors
    /// The file does not read as a Dx10 / Dx11 database.
    pub fn parse(data: &[u8]) -> Result<Self, String> {
        let db = shaderdb::parse(data, 11, false).map_err(|e| e.to_string())?;
        let shader = |s: &shaderdb::Shader<'_>| Shader {
            code: Arc::new(s.code.to_vec()),
            param: s.param as usize,
            tail: s.tail.to_vec(),
        };
        let programs = db
            .programs
            .iter()
            .zip(&db.keys)
            .map(|(p, k)| {
                let [vs, ps, b1, b2] = query::program_refs(p, false).unwrap_or_default();
                Program {
                    vs: vs as usize,
                    ps: ps as usize,
                    block1: b1 as usize,
                    block2: b2 as usize,
                    state: (u32_at(p, 0x10), u32_at(p, 0x14)),
                    key: k.to_vec(),
                }
            })
            .collect();
        Ok(Self {
            entries: db
                .entries
                .iter()
                .map(|n| (query::lower(&n.name).into_owned(), guid(n.guid)))
                .collect(),
            parts: db
                .parts
                .iter()
                .map(|n| (query::lower(&n.name).into_owned(), guid(n.guid)))
                .collect(),
            layouts: db
                .layouts
                .iter()
                .map(|r| (guid(r), r.get(16..).unwrap_or_default().to_vec()))
                .collect(),
            groups: db
                .groups
                .iter()
                .map(|(g, ps)| (guid(g), ps.iter().map(|&k| usize::from(k)).collect()))
                .collect(),
            programs,
            vs: db.vs.iter().map(shader).collect(),
            ps: db.ps.iter().map(shader).collect(),
            params: db.params.iter().map(|b| b.to_vec()).collect(),
        })
    }

    /// An entry's programs by its name (any case).
    #[must_use]
    pub fn entry_programs(&self, name: &str) -> Option<&[usize]> {
        let g = self.entries.get(&query::lower(name.as_bytes()).into_owned())?;
        self.groups.get(g).map(Vec::as_slice)
    }

    /// A parameter block.
    #[must_use]
    pub fn param(&self, k: usize) -> &[u8] {
        self.params.get(k).map_or(&[], Vec::as_slice)
    }
}

/// A vertex input element: `TEXCOORD<semantic>`, its DXGI format, stream, byte offset and whether it is per instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Element {
    /// Semantic index.
    pub semantic: u32,
    /// `DXGI_FORMAT`.
    pub format: u32,
    /// Input slot.
    pub slot: u32,
    /// Byte offset.
    pub offset: u32,
    /// 1: per instance.
    pub instance: u32,
}

/// A Dx10 / Dx11 vertex shader's input elements (section 4's tail: `D3D11_INPUT_ELEMENT_DESC`s).
#[must_use]
pub fn vs_elements(tail: &[u8]) -> Vec<Element> {
    let n = u32_at(tail, 0) as usize;
    let at = 4 + n;
    let m = u32_at(tail, at) as usize;
    (0..m.min(32))
        .map(|j| {
            let o = at + 4 + 0x1c * j;
            Element {
                semantic: u32_at(tail, o + 4),
                format: u32_at(tail, o + 8),
                slot: u32_at(tail, o + 12),
                offset: u32_at(tail, o + 16),
                instance: u32_at(tail, o + 20),
            }
        })
        .collect()
}

/// A shader's own parameter block's sampler states: (register, the 52-byte `D3D11_SAMPLER_DESC`).
#[must_use]
pub fn sampler_states(block: &[u8]) -> Vec<(u32, Vec<u8>)> {
    if block.len() < 0x20 {
        return Vec::new();
    }
    let start = u32_at(block, 0x14) as usize;
    let n = block.get(0x1e).copied().map_or(0, usize::from);
    (0..n)
        .filter_map(|i| {
            let o = start + 60 * i;
            Some((u32_at(block, o), block.get(o + 4..o + 56)?.to_vec()))
        })
        .collect()
}

/// D3D11 blend factors (src, dst, op) of the colour and alpha blend modes 0 - 7 (the exe's tables `0x1445C28` /
/// `0x1445C98`, FORMAT.md 6).
const BLEND_COLOUR: [(u32, u32, u32); 8] = [
    (2, 1, 1),
    (2, 6, 1),
    (2, 2, 1),
    (2, 2, 2),
    (2, 2, 3),
    (9, 1, 1),
    (2, 2, 4),
    (2, 2, 5),
];
const BLEND_ALPHA: [(u32, u32, u32); 8] = [
    (2, 1, 1),
    (2, 6, 1),
    (2, 2, 1),
    (2, 2, 2),
    (2, 2, 3),
    (7, 1, 1),
    (2, 2, 4),
    (2, 2, 5),
];

/// A program's render state as BFBC2's Dx11 renderer applies it (`renderqa.RenderState`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools, reason = "the state word's flags, one each")]
pub struct RenderState {
    /// The state word.
    pub word: u32,
    /// The alpha-test reference (x 255).
    pub alpha_ref: u32,
    /// Colour blend mode 0 - 7.
    pub colour_mode: u32,
    /// Alpha blend mode 0 - 7.
    pub alpha_mode: u32,
    /// Alpha to coverage.
    pub alpha_to_coverage: bool,
    /// Depth test (`LESS_EQUAL`).
    pub depth_test: bool,
    /// Depth write.
    pub depth_write: bool,
    /// No culling.
    pub two_sided: bool,
    /// Alpha test (compiled into DICE's pixel shaders on Dx10 / 11).
    pub alpha_test: bool,
    /// Transparent.
    pub transparent: bool,
}

fn mode(word: u32, first: u32) -> u32 {
    (0..7).find(|k| word & (first << k) != 0).map_or(0, |k| k + 1)
}

impl RenderState {
    /// The state of a program's words; `None` for (0, 0) (no state recorded: drawn opaque, no culling).
    #[must_use]
    pub fn of(word: u32, reference: u32) -> Option<Self> {
        if word == 0 && reference == 0 {
            return None;
        }
        let transparent = word & 1 != 0;
        Some(Self {
            word,
            alpha_ref: reference & 0xff,
            colour_mode: mode(word, 0x400),
            alpha_mode: mode(word, 0x20000),
            alpha_to_coverage: word & 0x20 != 0,
            depth_test: word & 0x80 != 0,
            depth_write: word & 4 != 0 || (!transparent && word & 0x100 != 0),
            two_sided: word & 0x40 != 0,
            alpha_test: word & 0x10 != 0,
            transparent,
        })
    }

    /// Whether it blends.
    #[must_use]
    pub fn blend(&self) -> bool {
        self.colour_mode != 0 || self.alpha_mode != 0
    }

    /// `(src, dst, op)` for colour and for alpha, as D3D11 numbers.
    #[must_use]
    pub fn factors(&self) -> [(u32, u32, u32); 2] {
        let pick = |t: &[(u32, u32, u32); 8], m: u32| t.get(m as usize).copied().unwrap_or((2, 1, 1));
        [
            pick(&BLEND_COLOUR, self.colour_mode),
            pick(&BLEND_ALPHA, self.alpha_mode),
        ]
    }
}

/// Named material constants whose stored value is a placeholder the engine writes over at draw time (FORMAT.md
/// section 1's constant records name them; DICE stores 0 or 1 there): the water's screen and depth factors
/// (`g_screenSizeInv`, `g_depthScaleFactors`: [`crate::water`]) and the trees' `sunDir` (the Overgrowth leaves and
/// bark, stored (1, 1, 1): the engine's sun direction, `outdoorLightDir`).
pub const ENGINE_NAMED: [&str; 3] = ["g_screenSizeInv", "g_depthScaleFactors", "sunDir"];

/// The constant records of a block the engine writes by name ([`ENGINE_NAMED`]).
#[must_use]
pub fn engine_named(constants: &[query::Constant<'_>]) -> Vec<(u8, String)> {
    constants
        .iter()
        .filter_map(|c| {
            let n: String = c.name.iter().map(|&b| char::from(b)).collect();
            ENGINE_NAMED.contains(&n.as_str()).then_some((c.register, n))
        })
        .collect()
}

/// Draw classes, drawn in this order (`renderqa.OPAQUE / TESTED / BLENDED`).
pub const OPAQUE: u8 = 1;
/// Alpha-tested, alpha to coverage or discarding.
pub const TESTED: u8 = 2;
/// Blended or transparent: back to front, after the rest.
pub const BLENDED: u8 = 3;

/// A draw's class (`renderqa.draw_class`).
#[must_use]
pub fn draw_class(state: Option<&RenderState>, ps: &[u8]) -> u8 {
    if state.is_some_and(|s| s.blend() || s.transparent) {
        return BLENDED;
    }
    if state.is_some_and(|s| s.alpha_test || s.alpha_to_coverage) || dxbc::discards(ps) {
        return TESTED;
    }
    OPAQUE
}

/// One part of a mesh drawn with one program.
#[derive(Debug, Clone)]
pub struct Draw {
    /// The mesh part.
    pub part: String,
    /// Its entry.
    pub entry: String,
    /// The program.
    pub program: usize,
    /// Vertex shader code.
    pub vs: Arc<Vec<u8>>,
    /// Pixel shader code.
    pub ps: Arc<Vec<u8>>,
    /// The vertex shader's reflection.
    pub vs_reflect: Arc<Reflection>,
    /// The pixel shader's reflection.
    pub ps_reflect: Arc<Reflection>,
    /// Its input layout (stream 0).
    pub elements: Vec<Element>,
    /// Where its vertices start, and their stride.
    pub vertex_offset: u32,
    /// Bytes per vertex.
    pub stride: u32,
    /// Indices drawn.
    pub index_count: u32,
    /// The first index.
    pub first_index: u32,
    /// The program key's mesh kind (0 rigid, 1 bones, 4 skinned, 9 destructible composite).
    pub kind: u8,
    /// Bone rows at rest (3 per bone), for composite parts.
    pub bones: Option<Vec<[f32; 4]>>,
    /// The material constants: (register, values).
    pub material: Vec<(u8, [f32; 4])>,
    /// Constants written at their registers in the vertex shader's `$Globals` whatever its `constants` array: the
    /// engine-named block constants of a terrain shader (`renderqa.patch_cbuffer`).
    pub vs_extra: Vec<(u8, [f32; 4])>,
    /// The same for the pixel shader.
    pub ps_extra: Vec<(u8, [f32; 4])>,
    /// Block constants the engine writes by name at draw time ([`ENGINE_NAMED`]): (register, name), the frame's value
    /// of that name written over the block's placeholder in both shaders' `constants`.
    pub named: Vec<(u8, String)>,
    /// Bound textures: (t slot, texture name; `engine:<name>` for a texture the engine makes).
    pub textures: Vec<(u32, String)>,
    /// The vertex shader's own textures (else it sees the pixel shader's).
    pub vs_textures: Vec<(u32, String)>,
    /// The pixel shader's sampler states: (s register, `D3D11_SAMPLER_DESC` bytes).
    pub samplers: Vec<(u32, Vec<u8>)>,
    /// The vertex shader's own sampler states (else it sees the pixel shader's).
    pub vs_samplers: Vec<(u32, Vec<u8>)>,
    /// The render state.
    pub state: Option<RenderState>,
    /// The part's mean vertex position (blended draws' order).
    pub centre: Option<[f32; 3]>,
    /// The part's position bounds in the mesh's space (before its bones), for culling; `None`: not known (drawn
    /// whatever the camera sees).
    pub bounds: Option<([f32; 3], [f32; 3])>,
    /// [`OPAQUE`], [`TESTED`] or [`BLENDED`].
    pub class: u8,
}

/// A mesh's draws and everything they read.
#[derive(Debug, Clone, Default)]
pub struct Prepared {
    /// The mesh's name.
    pub mesh: String,
    /// The draws.
    pub draws: Vec<Draw>,
    /// What was found on the way (`renderqa`'s flags with notes).
    pub notes: Vec<String>,
    /// LOD 0's vertex data.
    pub vertices: Arc<Vec<u8>>,
    /// LOD 0's index data.
    pub indices: Arc<Vec<u8>>,
    /// 32-bit indices.
    pub wide: bool,
    /// The drawn parts' position bounds.
    pub bounds: Option<([f32; 3], [f32; 3])>,
    /// The shader set drawn (its name in the mesh set, `path/GUID`).
    pub shader_set: String,
}

/// Texture resources by name, read once (`None`: missing or not drawable as stored).
pub type TextureFiles = HashMap<String, Option<Arc<Vec<u8>>>>;

/// Why a texture resource cannot be uploaded as stored (`renderqa.texture_problem`), or `None`.
#[must_use]
pub fn texture_problem(pc: &[u8]) -> Option<String> {
    if pc.len() < 92 {
        return Some("too short".into());
    }
    let (typ, fmt, w, h, mips) = (
        u32_at(pc, 4),
        u32_at(pc, 8),
        u32_at(pc, 16),
        u32_at(pc, 20),
        u32_at(pc, 28),
    );
    let block = match fmt {
        0 | 18 => 8,
        1 | 2 | 19 | 20 => 16,
        13 | 9 | 10 => 0,
        _ => return Some(format!("format {fmt}")),
    };
    if !(1..=15).contains(&mips) || w == 0 || h == 0 {
        return Some(format!("{w}x{h}, {mips} mips"));
    }
    if block != 0 && (w % 4 != 0 || h % 4 != 0) {
        return Some(format!("{w}x{h} block format"));
    }
    let levels: u64 = (0..mips as usize).map(|m| u64::from(u32_at(pc, 32 + 4 * m))).sum();
    if 92 + levels * if typ == 1 { 6 } else { 1 } > pc.len() as u64 {
        return Some("levels run past the data".into());
    }
    None
}

/// A texture's bytes from the cache, read on first use.
pub fn texture<'a>(files: &Files, cache: &'a mut TextureFiles, name: &str) -> Option<&'a Arc<Vec<u8>>> {
    cache
        .entry(name.to_owned())
        .or_insert_with(|| {
            files
                .read(&format!("{name}.res"))
                .filter(|b| texture_problem(b).is_none())
                .map(Arc::new)
        })
        .as_ref()
}

/// Positions of a part's vertices (stream 0, usage 1, float3 or half), for its bounds and centre.
fn positions(ldata: &[u8], data: usize, part: &meshdata::MeshPart, layout: &[u8]) -> Vec<[f32; 3]> {
    let count = layout.get(64).copied().map_or(0, usize::from);
    let Some((fmt, off)) = (0..count.min(16)).find_map(|i| {
        let e = layout.get(4 * i..4 * i + 4)?;
        if e.get(1) == Some(&1) && e.get(3) == Some(&0) {
            Some((*e.first()?, usize::from(*e.get(2)?)))
        } else {
            None
        }
    }) else {
        return Vec::new();
    };
    let [_, n, _, vo] = part.counts;
    let stride = usize::from(part.bytes[0]);
    let base = data + vo as usize;
    (0..n as usize)
        .filter_map(|v| {
            let at = base + v * stride + off;
            match fmt {
                3 | 4 => Some([f32_at(ldata, at), f32_at(ldata, at + 4), f32_at(ldata, at + 8)]),
                7 | 8 => {
                    let h = |k: usize| -> Option<f32> {
                        Some(half(u16::from_le_bytes([*ldata.get(at + k)?, *ldata.get(at + k + 1)?])))
                    };
                    Some([h(0)?, h(2)?, h(4)?])
                }
                _ => None,
            }
        })
        .collect()
}

/// The program the engine draws a part with (`renderqa._pick_program`): main pass, not instanced, no point light,
/// the part's layout; `(program, its layout is the part's)`.
#[must_use]
pub fn pick_program(db: &Db, progs: &[usize], layout: &Guid) -> Option<(usize, bool)> {
    let p = |k: &usize| db.programs.get(*k);
    let main: Vec<usize> = progs
        .iter()
        .copied()
        .filter(|k| p(k).is_some_and(|p| p.key_byte(0x30) == 0 && p.key_byte(0x34) == 0))
        .collect();
    let keyed: Vec<usize> = main
        .iter()
        .copied()
        .filter(|k| p(k).is_some_and(|p| &p.layout() == layout))
        .collect();
    let no_light = |v: &[usize]| -> Vec<usize> {
        v.iter()
            .copied()
            .filter(|k| p(k).is_some_and(|p| p.key_byte(0x38) == 0))
            .collect()
    };
    if !keyed.is_empty() {
        let pick = no_light(&keyed);
        return Some((*pick.first().or(keyed.first())?, true));
    }
    let rest = no_light(&main);
    rest.first().or(main.first()).map(|&k| (k, false))
}

/// A composite part's bone rows at rest (`renderqa.part_bones`): row block k is composite part k's transform.
fn part_bones(set: &meshdata::MeshSet, part: &meshdata::MeshPart) -> Option<Vec<[f32; 4]>> {
    let (cp, _) = set.lods.first()?.parts.as_ref()?;
    if part.bones.is_empty() {
        return None;
    }
    let mut out = identity_bones();
    for (k, p) in cp.iter().take(60).enumerate() {
        let xf = &p.3;
        for (row, slot) in out.iter_mut().skip(3 * k).take(3).enumerate() {
            // r = M^T of the stored row-major 3 x 3, then the translation
            *slot = [
                xf.get(row).copied().unwrap_or(0.0),
                xf.get(3 + row).copied().unwrap_or(0.0),
                xf.get(6 + row).copied().unwrap_or(0.0),
                xf.get(9 + row).copied().unwrap_or(0.0),
            ];
        }
    }
    Some(out)
}

/// 60 identity bones (3 rows each).
#[must_use]
pub fn identity_bones() -> Vec<[f32; 4]> {
    (0..180)
        .map(|r| {
            let mut row = [0.0; 4];
            if let Some(v) = row.get_mut(r % 3) {
                *v = 1.0;
            }
            row
        })
        .collect()
}

/// Everything needed to draw a mesh's LOD 0 with a database, and the problems found (`renderqa.prepare`). `envmap`:
/// the cube bound as `outdoorLightReflectionEnvmap` when the material names none.
#[must_use]
pub fn prepare(files: &Files, db: &Db, mesh: &str, envmap: &str, cache: &mut TextureFiles) -> Prepared {
    prepare_set(files, db, mesh, None, envmap, cache)
}

/// [`prepare`] with the shader set the placement names (`MeshShaderSet`: `path/guid`, any case), the set of the mesh
/// set whose name it is; `None` or a name the mesh set does not list: the first set the database draws whole, as
/// [`prepare`] (a note says so for a name not listed).
#[must_use]
#[allow(clippy::too_many_lines, reason = "renderqa.prepare, step by step")]
pub fn prepare_set(
    files: &Files,
    db: &Db,
    mesh: &str,
    shader_set: Option<&str>,
    envmap: &str,
    cache: &mut TextureFiles,
) -> Prepared {
    let low = mesh.to_lowercase();
    let mut out = Prepared {
        mesh: mesh.to_owned(),
        ..Prepared::default()
    };
    let (Some(msb), Some(kind), Some(ldata)) = (
        files.read(&format!("{low}.res")),
        files.kind(&format!("{low}.res")).and_then(Kind::of),
        files.read(&format!("{low}_lod0_data.res")),
    ) else {
        out.notes.push("no-mesh: mesh set or its LOD 0 data not found".into());
        return out;
    };
    let (Ok(set), Ok(md)) = (
        meshdata::parse_set(&msb, kind, false, None),
        meshdata::parse_mesh(&ldata, false),
    ) else {
        out.notes.push("no-mesh: unreadable".into());
        return out;
    };
    let Some(lod) = set.lods.first() else {
        out.notes.push("no-mesh: mesh set has no LOD".into());
        return out;
    };
    let [vbytes, _, wide] = md.sizes;
    out.vertices = Arc::new(
        ldata
            .get(md.data..md.data + vbytes as usize)
            .unwrap_or_default()
            .to_vec(),
    );
    out.indices = Arc::new(ldata.get(md.data + vbytes as usize..).unwrap_or_default().to_vec());
    out.wide = wide != 0;
    let name = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    let has = |s: &(Vec<u8>, Vec<Vec<u8>>)| {
        s.1.iter()
            .filter(|n| {
                db.entries
                    .contains_key(&query::lower(format!("{low}/{}", name(n)).as_bytes()).into_owned())
            })
            .count()
    };
    let named = shader_set.and_then(|want| lod.sets.iter().find(|s| name(&s.0).eq_ignore_ascii_case(want)));
    if let (Some(want), None) = (shader_set, named) {
        out.notes.push(format!(
            "shader-set: {want} is not one of the mesh's sets (the default drawn)"
        ));
    }
    let chosen = named.or_else(|| {
        lod.sets
            .iter()
            .find(|s| has(s) == s.1.len())
            .or_else(|| lod.sets.iter().max_by_key(|s| has(s)))
    });
    out.shader_set = chosen.map_or_else(String::new, |s| name(&s.0));
    let shaders: Vec<String> = chosen.map_or_else(Vec::new, |s| s.1.iter().map(|n| name(n)).collect());
    let (mut lo, mut hi) = ([f32::MAX; 3], [f32::MIN; 3]);
    for (i, p) in md.parts.iter().enumerate() {
        let pn = name(&p.name);
        let [triangles, _, first, vertex_offset] = p.counts;
        let Some(shader) = shaders.get(i).filter(|_| triangles != 0) else {
            continue;
        };
        let part_key = query::lower(format!("{low}_lod0/{pn}/standard").as_bytes()).into_owned();
        let Some((lg, layout)) = db.parts.get(&part_key).and_then(|g| Some((*g, db.layouts.get(g)?))) else {
            out.notes
                .push(format!("no-layout: {pn}: no vertex layout in the database"));
            continue;
        };
        let entry = format!("{mesh}/{shader}");
        let Some(progs) = db.entry_programs(&entry) else {
            out.notes.push(format!("no-entry: {pn}: no entry {shader}"));
            continue;
        };
        let Some((k, keyed)) = pick_program(db, progs, &lg) else {
            out.notes
                .push(format!("no-program: {pn}: entry {shader} has no main-pass program"));
            continue;
        };
        if !keyed {
            if pn.to_lowercase().contains("_zonly") {
                continue;
            }
            out.notes.push(format!(
                "layout-fallback: {pn}: no program for its vertex layout in {shader}"
            ));
        }
        let Some(prog) = db.programs.get(k) else { continue };
        let (Some(vs), Some(ps)) = (db.vs.get(prog.vs), db.ps.get(prog.ps)) else {
            continue;
        };
        let pos = positions(&ldata, md.data, p, layout);
        let mut centre = [0.0f32; 3];
        let mut count = 0.0f32;
        let (mut plo, mut phi) = ([f32::MAX; 3], [f32::MIN; 3]);
        for q in pos.iter().filter(|q| q.iter().all(|v| v.is_finite())) {
            for (((l, h), c), v) in lo.iter_mut().zip(hi.iter_mut()).zip(centre.iter_mut()).zip(q) {
                *l = l.min(*v);
                *h = h.max(*v);
                *c += v;
            }
            for ((l, h), v) in plo.iter_mut().zip(phi.iter_mut()).zip(q) {
                *l = l.min(*v);
                *h = h.max(*v);
            }
            count += 1.0;
        }
        let block2 = db.param(prog.block2);
        let state = RenderState::of(prog.state.0, prog.state.1);
        let ps_reflect = Arc::new(dxbc::reflect(&ps.code));
        let mut d = Draw {
            part: pn.clone(),
            entry: entry.clone(),
            program: k,
            vs: Arc::clone(&vs.code),
            ps: Arc::clone(&ps.code),
            vs_reflect: Arc::new(dxbc::reflect(&vs.code)),
            ps_reflect: Arc::clone(&ps_reflect),
            elements: vs_elements(&vs.tail).into_iter().filter(|e| e.slot == 0).collect(),
            vertex_offset,
            stride: u32::from(p.bytes[0]),
            index_count: triangles * 3,
            first_index: first,
            kind: prog.key_byte(0x33),
            bones: part_bones(&set, p),
            material: query::block_constants(block2, false)
                .iter()
                .map(|c| (c.register, c.values))
                .collect(),
            vs_extra: Vec::new(),
            ps_extra: Vec::new(),
            named: engine_named(&query::block_constants(block2, false)),
            textures: Vec::new(),
            vs_textures: Vec::new(),
            samplers: sampler_states(db.param(ps.param)),
            vs_samplers: Vec::new(),
            state,
            centre: (count > 0.0).then(|| centre.map(|v| v / count)),
            bounds: (count > 0.0).then_some((plo, phi)),
            class: draw_class(state.as_ref(), &ps.code),
        };
        let records: HashMap<u32, String> = query::block_textures(block2, false)
            .iter()
            .map(|t| (u32::from(t.sampler), name(t.name)))
            .collect();
        let engine: Vec<u32> = query::block_engine_textures(block2, false)
            .iter()
            .map(|(_, s)| u32::from(*s))
            .collect();
        for r in ps_reflect.resources.iter().filter(|r| r.kind == 2) {
            if r.point == 0 && r.name.to_lowercase().contains("shadowmap") {
                continue;
            }
            let tn = if r.name == "texture_outdoorLightReflectionEnvmap" && !records.contains_key(&r.point) {
                envmap.to_owned()
            } else if let Some(t) = records.get(&r.point) {
                t.clone()
            } else {
                if !engine.contains(&r.point) && r.name.starts_with("texture_Texture") {
                    out.notes
                        .push(format!("bound-empty: {pn}: t{} ({}) has no texture", r.point, r.name));
                }
                continue;
            };
            let Some(bytes) = texture(files, cache, &tn) else {
                if tn != envmap {
                    out.notes
                        .push(format!("missing-texture: {pn}: t{} {tn} not found", r.point));
                }
                continue;
            };
            let cube = u32_at(bytes, 4) == 1;
            if cube != (r.dim == 9) {
                out.notes.push(format!("dimension: {pn}: t{} {}", r.point, r.name));
                continue;
            }
            d.textures.push((r.point, tn));
        }
        out.draws.push(d);
    }
    if lo.iter().zip(&hi).all(|(a, b)| a <= b) {
        out.bounds = Some((lo, hi));
    }
    if out.draws.is_empty() && out.notes.is_empty() {
        out.notes.push("no-entry: no part to draw".into());
    }
    out
}

#[cfg(test)]
#[allow(
    clippy::indexing_slicing,
    clippy::chunks_exact_to_as_chunks,
    clippy::trivially_copy_pass_by_ref,
    clippy::float_cmp,
    clippy::unwrap_used,
    clippy::expect_used
)]
mod tests {
    use super::*;

    #[test]
    fn render_states_as_dice_encodes_them() {
        let opaque = RenderState::of(0x180, 0x7f).expect("state");
        assert!(opaque.depth_test && opaque.depth_write && !opaque.blend() && !opaque.two_sided);
        let glass = RenderState::of(0x581, 0x7f).expect("state");
        assert!(glass.blend() && glass.transparent && !glass.depth_write);
        assert_eq!(glass.factors()[0], (2, 6, 1));
        let water = RenderState::of(0x5c5, 0).expect("state");
        assert!(water.depth_write && water.two_sided);
        let multiply = RenderState::of(0x4181, 0).expect("state");
        assert_eq!(multiply.colour_mode, 5);
        assert_eq!(multiply.factors(), [(9, 1, 1), (2, 1, 1)]);
        let alpha_only = RenderState::of(0x20181, 0).expect("state");
        assert_eq!((alpha_only.colour_mode, alpha_only.alpha_mode), (0, 1));
        assert!(RenderState::of(0, 0).is_none());
        assert_eq!(draw_class(Some(&glass), &[]), BLENDED);
        assert_eq!(draw_class(Some(&opaque), &[]), OPAQUE);
        assert_eq!(draw_class(RenderState::of(0x190, 0x7f).as_ref(), &[]), TESTED);
    }

    #[test]
    fn identity_bones_are_three_rows_each() {
        let b = identity_bones();
        assert_eq!(b.len(), 180);
        assert_eq!(b.get(3), Some(&[1.0, 0.0, 0.0, 0.0]));
        assert_eq!(b.get(5), Some(&[0.0, 0.0, 1.0, 0.0]));
    }

    #[test]
    fn texture_header_checks() {
        let mut t = vec![0u8; 92 + 8];
        t[8] = 0; // BC1
        t[16] = 4;
        t[20] = 4;
        t[28] = 1;
        t[32] = 8;
        assert_eq!(texture_problem(&t), None);
        t[8] = 77;
        assert_eq!(texture_problem(&t).as_deref(), Some("format 77"));
        assert!(texture_problem(&[0; 10]).is_some());
    }
}
