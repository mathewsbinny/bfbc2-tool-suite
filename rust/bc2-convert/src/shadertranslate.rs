//! The BF1943 shader translation's driver (round `OR3c`): Project OPFOR's `wake_translate.translate_db` walks a PC
//! shader database's entries and, for each mesh-part layout and permutation an entry has, picks BF1943's program to
//! translate; `foreign_entries` / `add_missing_entries` choose the entries made for BF1943 materials the database
//! lacks; `ww2_char_shaders` matches the soldiers' entries to BF1943's and offers BF1943's programs as the skinning
//! kinds our parts draw with. All of that is here, over [`ShaderDb`]s; the translation itself (`bc2_shader::hlsl`),
//! the HLSL compiles (`pcbuild`, `d3dcompiler_47`) and OPFOR's render-state rules stay with the caller, which takes
//! each [`Program`] of the plan in order.
//!
//! **The plan** ([`plan`]): the entries to translate (the filters, excludes, `only`, the materials kept on their
//! BFBC2 donor, the meshes drawn with DICE's own shaders), then per entry
//! - BF1943's programs: main ones by point-light flag (key byte 0x38), depth-only ones (pass 5);
//! - the layouts it is drawn with: its mesh's parts (`Standard` / `StreamInstancingFloat`), else the terrain strip
//!   layout (BF1943's in its byte order among the entry's keys), else a terrain patch's two layouts, else the parts
//!   the install will register ([`Options::planned`]);
//! - per layout and the template's permutations of the same instancing: the BF1943 program whose vertex shader
//!   reads what the layout has - by layout ([`element_map`]: the two layouts paired by kind; each fetch in the
//!   layout's own Xenos format), by role ([`by_role`]: a program of another vertex format of the material) or by
//!   element order (no BF1943 layout) - best by the permutation's mesh kind, its variant without the fade, no fade,
//!   by layout before by role, every input found;
//! - the program's renderer fragment ([`Program::fragment`]): the terrain's own for strips and patches, BF1943's by
//!   name for a mesh, and its key and id.
//!
//! An entry stops where the Python stopped it ([`Fail`]: the `TranslateError`, `XenosError`, `KeyError` or
//! `ValueError` it raised there, with the programs before it); what the Python could not have run at all (a record
//! too short, a vertex shader the decoder calls malformed) is a [`Fatal`].
//!
//! The soldiers ([`Options::soldier`], `ww2_char_shaders.soldier_wiring`): DICE's soldier layouts wired by meaning
//! ([`soldier_element_map`]) instead of by pairing kinds.
// Indexing here is into the plan's own tables at indices they hand out (a pool's console element, a key checked long
// enough, a vertex shader parsed before it is read, a template's inputs checked to hold the semantics read, a slice
// at a position `find` returned); the records' bytes from the databases go through `get`.
#![allow(clippy::indexing_slicing)]
// The maps are std's: the bindings and the tests make them.
#![allow(clippy::implicit_hasher)]

use std::collections::{BTreeMap, HashMap, HashSet};

use bc2_formats::shaderdb::ShaderDb;
use bc2_formats::shaderdb::query::{lower, md5, swap_guid};

use crate::meshconv::{Element, elements};

/// BF1943 console layout formats -> the Xenos vertex fetch format a program reads them with (`XENOS_FETCH`).
#[must_use]
pub const fn xenos_fetch(f: u8) -> Option<u8> {
    match f {
        3 => Some(57),
        43 => Some(16),
        6 => Some(31),
        12 => Some(6),
        7 => Some(32),
        _ => None,
    }
}

/// A PC layout element format's DXGI format (`pcbuild.DXGI`).
#[must_use]
pub const fn dxgi(f: u8) -> Option<u32> {
    Some(match f {
        1 => 41,
        2 => 16,
        3 => 6,
        4 => 2,
        5 => 54,
        6 => 34,
        7 | 8 => 10,
        12 => 30,
        15 => 38,
        19 => 37,
        21 => 13,
        _ => return None,
    })
}

/// Bytes of a DXGI vertex format (`DXGI_BYTES`; 0 for one not listed).
#[must_use]
pub const fn dxgi_bytes(f: u32) -> u32 {
    match f {
        41 | 34 | 30 | 38 | 37 => 4,
        16 | 10 | 13 => 8,
        6 => 12,
        2 => 16,
        54 => 2,
        _ => 0,
    }
}

/// A program key's variant byte (key byte 0x30 + 7) and its dissolve-fade bit.
pub const VARIANT: usize = 7;
/// The variant's fade bit.
pub const FADE: u8 = 0x01;

/// How an entry stopped, as the Python raised it inside `translate_db`'s entry (the entry keeps its programs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fail {
    /// `xenos_hlsl.TranslateError(message)`.
    Translate(String),
    /// `xenos.XenosError(message)` (BF1943's vertex shader).
    Xenos(String),
    /// `KeyError(bytes)`: a GUID or layout not in its table.
    KeyBytes(Vec<u8>),
    /// `KeyError(int)`: a format or offset not in its table.
    KeyInt(i64),
    /// `ValueError(message)`: not caught by `translate_db` (the run stops).
    Value(String),
}

/// What the Python could not have run (`IndexError`, `struct.error`, a decoder failure other than `XenosError`).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Fatal {
    /// A record or table too short for what is read of it.
    #[error("{0}: a record is too short or an index is past its section")]
    Short(&'static str),
    /// BF1943's vertex shader the decoder cannot read (not a `XenosError`).
    #[error("BF1943's vertex shader {0}: {1}")]
    Shader(usize, String),
    /// Two soldier part kinds on one layout (`soldier_layouts`: `SystemExit`).
    #[error("{0}")]
    Exit(String),
    /// The caller's planned parts failed.
    #[error("{0}")]
    Planned(String),
}

type Res<T> = Result<T, Fatal>;

/// One vertex input of a translated program: (stream, byte offset, DXGI format, semantic index).
pub type Input = (u32, u32, u32, u32);

/// One program of the plan, in the order `translate_db` makes them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Program {
    /// The PC layout GUID.
    pub layout: Vec<u8>,
    /// Instanced (`StreamInstancingFloat`).
    pub instanced: bool,
    /// The template's permutation (key bytes 0x30-).
    pub perm: Vec<u8>,
    /// The template program (the PC database's index).
    pub template: usize,
    /// BF1943's program.
    pub source: usize,
    /// BF1943's vertex shader, pixel shader, and two blocks (big-endian record).
    pub refs: [u32; 4],
    /// The vertex inputs.
    pub inputs: Vec<Input>,
    /// `{semantic index: BF1943 vertex offset}`, by semantic index.
    pub src: Vec<(u32, u32)>,
    /// 1 wired by layout or order, 0 by role.
    pub rank: u8,
    /// "layout", "role" or "order" (the wiring record's `by`).
    pub by: &'static str,
    /// BF1943's mesh kind (key byte 0x33).
    pub kind: u8,
    /// A terrain strip program.
    pub strip: bool,
    /// A terrain patch program.
    pub patch: bool,
    /// The renderer fragment, or why there is none.
    pub fragment: Result<Vec<u8>, Fail>,
    /// The translated program's key (its fragment's place zero where there is none).
    pub key: Vec<u8>,
    /// The program's id (`md5(name utf-8 + layout + key[0x30:])`).
    pub pid: [u8; 16],
}

/// One entry of the plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// The entry's name (latin-1).
    pub name: Vec<u8>,
    /// Its GUID.
    pub guid: Vec<u8>,
    /// Its programs, in order.
    pub programs: Vec<Program>,
    /// Where it stopped (after its programs).
    pub error: Option<Fail>,
}

/// The parts the install will register for a mesh (`planned_parts`): (part key `mesh_lodN/part`, layout, instanced).
pub type PlannedParts = HashMap<Vec<u8>, Vec<(Vec<u8>, Vec<u8>, bool)>>;

/// `translate_db`'s arguments.
pub struct Options {
    /// The renderer (10 / 11).
    pub dx: u32,
    /// An entry's lower name holds one of them (lower).
    pub filters: Vec<Vec<u8>>,
    /// ... and none of them (lower).
    pub exclude: Vec<Vec<u8>>,
    /// Material leaves kept on their BFBC2 donor (`KEEP_DONOR`).
    pub keep_donor: Vec<Vec<u8>>,
    /// Only these entries (names as given).
    pub only: Option<HashSet<Vec<u8>>>,
    /// Meshes (lower) drawn with DICE's own shaders (`dice_drawn`).
    pub dice_twins: HashSet<Vec<u8>>,
    /// BF1943's mesh part layouts `{part (lower): record body}`, in their order.
    pub bf_layouts: Vec<(Vec<u8>, Vec<u8>)>,
    /// The terrain strip layout.
    pub strip_layout: Vec<u8>,
    /// The terrain patch layouts of this renderer (tiled, grid only).
    pub patch_layouts: Vec<Vec<u8>>,
    /// The terrain patch entries' name part (`/PatchMaskShaders/`).
    pub patches: Vec<u8>,
    /// The renderer fragment names of the terrain's patches and strips on this renderer.
    pub terrain_fragments: (Vec<u8>, Vec<u8>),
    /// DICE's soldier layouts wired by meaning (`ww2_char_shaders.soldier_wiring`).
    pub soldier: bool,
}

/// The parts the install will register (`planned_parts`), asked for once, when an entry has no other layout.
pub type Planned<'f> = &'f mut dyn FnMut() -> Result<PlannedParts, String>;

fn latin_utf8(b: &[u8]) -> Vec<u8> {
    b.iter().map(|&c| char::from(c)).collect::<String>().into_bytes()
}

fn rsplit_last(b: &[u8]) -> &[u8] {
    b.iter().rposition(|&c| c == b'/').map_or(b, |k| &b[k + 1..])
}

fn rsplit_first(b: &[u8]) -> &[u8] {
    b.iter().rposition(|&c| c == b'/').map_or(b, |k| &b[..k])
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    needle.is_empty() || hay.windows(needle.len()).any(|w| w == needle)
}

fn find(hay: &[u8], needle: &[u8], from: usize) -> Option<usize> {
    if from > hay.len() {
        return None;
    }
    if needle.is_empty() {
        return Some(from);
    }
    hay[from..]
        .windows(needle.len())
        .position(|w| w == needle)
        .map(|k| k + from)
}

/// `mesh_of(entry)`: the mesh (lower) an entry's name begins with (up to `_mesh` before `/objects/` or
/// `/terrains/`, else up to `/objects/` from the ninth character, else up to its last `/`).
#[must_use]
pub fn mesh_of(entry: &[u8]) -> Vec<u8> {
    let low = lower(entry).into_owned();
    let a = find(&low, b"_mesh/objects/", 0);
    let b = find(&low, b"_mesh/terrains/", 0);
    let k = match (a, b) {
        (Some(x), Some(y)) => Some(x.min(y) + 5),
        (Some(x), None) | (None, Some(x)) => Some(x + 5),
        (None, None) => find(&low, b"/objects/", 8),
    };
    match k {
        Some(k) if k > 0 => low[..k].to_vec(),
        _ => rsplit_first(&low).to_vec(),
    }
}

/// `mesh.elements(layout)[0]`, or [`Fatal::Short`].
fn els(layout: &[u8]) -> Res<Vec<Element>> {
    elements(layout).map(|e| e.0).ok_or(Fatal::Short("a vertex layout"))
}

/// `vertex_elements(layout)`: a PC layout's stream-0 elements in offset order, an element lying inside the one
/// before it left out.
///
/// # Errors
/// [`Fatal::Short`]: the record is too short.
pub fn vertex_elements(layout: &[u8]) -> Res<Vec<Element>> {
    let mut s: Vec<Element> = els(layout)?.into_iter().filter(|e| e.3 == 0).collect();
    s.sort_by_key(|e| e.2);
    let (mut out, mut end) = (Vec::new(), 0u32);
    for e in s {
        if u32::from(e.2) < end {
            continue;
        }
        out.push(e);
        end = u32::from(e.2) + dxgi(e.0).map_or(0, dxgi_bytes);
    }
    Ok(out)
}

const fn pc_format(f: u8) -> u8 {
    match f {
        0x2B | 0x0D => 0x08,
        _ => f,
    }
}

/// `{BF1943 vertex offset: PC element}` (a map: later offsets replace earlier ones as the dictionary did).
pub type ElementMap = BTreeMap<u8, Element>;

/// Console element indices by kind (format as the PC's, usage, stream), in order.
type Pools = Vec<((u8, u8, u8), std::collections::VecDeque<usize>)>;

/// `element_map(bf_layout, pc_layout)`: the two layouts paired by kind (`mesh.pair_by_kind(leftovers=True)`; when a
/// PC element has no console one, the k-th of a kind still takes the k-th, `None` when BF1943 data is left over).
///
/// # Errors
/// [`Fatal::Short`]: a record is too short.
pub fn element_map(bf: Option<&[u8]>, pc: &[u8]) -> Res<Option<ElementMap>> {
    let Some(bf) = bf else { return Ok(None) };
    let (ec, ep) = (els(bf)?, els(pc)?);
    let mut pools: Pools = Vec::new();
    let pool_of = |pools: &mut Pools| {
        for (i, &(f, u, _, s)) in ec.iter().enumerate() {
            let k = (pc_format(f), u, s);
            match pools.iter_mut().find(|(x, _)| *x == k) {
                Some((_, v)) => v.push_back(i),
                None => pools.push((k, std::collections::VecDeque::from([i]))),
            }
        }
    };
    pool_of(&mut pools);
    let mut order: Vec<Option<usize>> = Vec::with_capacity(ep.len());
    let mut paired = true;
    for &(f, u, _, s) in &ep {
        let Some(i) = pools
            .iter_mut()
            .find(|(x, _)| *x == (f, u, s))
            .and_then(|p| p.1.pop_front())
        else {
            paired = false;
            break;
        };
        order.push(Some(i));
    }
    if !paired {
        pools.clear();
        pool_of(&mut pools);
        order = ep
            .iter()
            .map(|&(f, u, _, s)| {
                pools
                    .iter_mut()
                    .find(|(x, _)| *x == (f, u, s))
                    .and_then(|p| p.1.pop_front())
            })
            .collect();
        if pools.iter().any(|(_, v)| !v.is_empty()) {
            return Ok(None);
        }
    }
    let mut out = ElementMap::new();
    for (i, e) in order.into_iter().zip(&ep) {
        if let Some(i) = i
            && e.3 == 0
            && ec[i].3 == 0
        {
            out.insert(ec[i].2, *e);
        }
    }
    Ok(Some(out))
}

/// `ww2_char_shaders.soldier_layouts(db)`: `{stream-0 elements: 1p (true) / 3p (false)}` of the layouts our soldier
/// parts draw with (`characters/ww2/`, not the rigid kit bags, more than three elements).
///
/// # Errors
/// [`Fatal::Exit`]: one layout for first- and third-person parts; [`Fatal::Short`]: a part's layout is not there or
/// is short (the Python's `KeyError` / `IndexError` outside the translation).
pub fn soldier_layouts(db: &ShaderDb<'_>) -> Res<HashMap<Vec<Element>, bool>> {
    let lays: HashMap<&[u8], &[u8]> = db
        .layouts
        .iter()
        .map(|r| (&r[..16.min(r.len())], &r[16.min(r.len())..]))
        .collect();
    let mut out: HashMap<Vec<Element>, bool> = HashMap::new();
    for p in &db.parts {
        let low = lower(&p.name);
        let tech = rsplit_last(&p.name);
        if !low.starts_with(b"characters/ww2/") || !(tech == b"Standard" || tech == b"StreamInstancingFloat") {
            continue;
        }
        if low.starts_with(b"characters/ww2/3p/kitbags/") {
            continue;
        }
        let rec = lays.get(p.guid).ok_or(Fatal::Short("a soldier part's layout"))?;
        let e: Vec<Element> = els(rec)?.into_iter().filter(|e| e.3 == 0).collect();
        if e.len() <= 3 {
            continue;
        }
        let first = contains(&low, b"/1p/");
        if *out.entry(e).or_insert(first) != first {
            return Err(Fatal::Exit(format!(
                "{}: one vertex layout for first- and third-person soldier parts",
                String::from_utf8_lossy(&latin_utf8(&p.name))
            )));
        }
    }
    Ok(out)
}

/// `ww2_char_shaders.soldier_element_map(bf, pc, kind)`: a BF1943 soldier vertex in DICE's soldier layout as
/// soldiers.py re-lays it; `None` when an element has no place.
///
/// # Errors
/// [`Fail::KeyInt`]: DICE's layout has no element at 16, 40 or 48 where one is wanted; [`Fatal::Short`].
pub fn soldier_element_map(bf: &[u8], pc: &[u8], first_person: bool) -> Res<Result<Option<ElementMap>, Fail>> {
    let eb = els(bf)?;
    let ep: Vec<Element> = els(pc)?.into_iter().filter(|e| e.3 == 0).collect();
    let mut by_usage: HashMap<u8, Element> = HashMap::new();
    let mut at: HashMap<u8, Element> = HashMap::new();
    for e in &ep {
        at.insert(e.2, *e);
    }
    for e in &ep {
        if e.1 != 7 {
            by_usage.entry(e.1).or_insert(*e);
        }
    }
    let mut out = ElementMap::new();
    let mut coords: Vec<(usize, Element)> = eb
        .iter()
        .copied()
        .enumerate()
        .filter(|(_, e)| e.1 == 0 && e.3 == 0)
        .collect();
    coords.sort_by_key(|(_, e)| e.2);
    let half2: Vec<usize> = coords.iter().filter(|(_, e)| e.0 == 6).map(|(i, _)| *i).collect();
    for &(_, u, off, s) in &eb {
        if s != 0 || u == 7 {
            continue;
        }
        if (1..=5).contains(&u) {
            let Some(e) = by_usage.get(&u) else { return Ok(Ok(None)) };
            out.insert(off, *e);
        }
    }
    for &(i, e) in &coords {
        let want = if e.0 == 8 {
            Some(16)
        } else if half2.first() == Some(&i) {
            Some(40)
        } else if first_person && half2.len() > 1 && half2[1] == i {
            Some(48)
        } else if at.contains_key(&52) {
            Some(52)
        } else {
            None
        };
        if let Some(w) = want {
            let Some(pe) = at.get(&w) else {
                return Ok(Err(Fail::KeyInt(i64::from(w))));
            };
            out.insert(e.2, *pe);
        }
    }
    Ok(Ok(Some(out)))
}

/// `by_role(bfel, fetch, bf_fmt)`: each fetch (in offset order) takes the next unused layout element read in the
/// same Xenos format; `None` when a fetch has none.
#[must_use]
pub fn by_role(bfel: &[(u32, u32)], fetch: &HashMap<u32, u8>, bf_fmt: &BTreeMap<u8, u8>) -> Option<Vec<(u32, u32)>> {
    let mut pools: Vec<(u8, std::collections::VecDeque<u32>)> = Vec::new();
    for (&off, &f) in bf_fmt {
        if let Some(x) = xenos_fetch(f) {
            match pools.iter_mut().find(|(k, _)| *k == x) {
                Some((_, v)) => v.push_back(u32::from(off)),
                None => pools.push((x, std::collections::VecDeque::from([u32::from(off)]))),
            }
        }
    }
    let mut out: BTreeMap<u32, u32> = BTreeMap::new();
    for &(bo, ui) in bfel {
        let x = *fetch.get(&bo)?;
        let pool = pools.iter_mut().find(|(k, _)| *k == x)?;
        out.insert(ui, pool.1.pop_front()?);
    }
    Some(out.into_iter().collect())
}

/// `patch_inputs(tail)`: `{semantic index: (stream, byte offset, DXGI format)}` of a Dx10 / Dx11 vertex shader's
/// per-vertex input elements.
///
/// # Errors
/// [`Fatal::Short`]: the tail is short.
pub fn patch_inputs(tail: &[u8]) -> Res<BTreeMap<u32, (u32, u32, u32)>> {
    let word = |at: usize| -> Res<u32> {
        tail.get(at..at + 4)
            .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .ok_or(Fatal::Short("a vertex shader's input description"))
    };
    let signature = word(0)? as usize;
    let start = 4 + signature;
    let count = word(start)? as usize;
    let mut out = BTreeMap::new();
    for j in 0..count {
        let rec = start + 4 + 0x1c * j;
        let (ui, fmt, stream, off, inst) = (
            word(rec + 4)?,
            word(rec + 8)?,
            word(rec + 12)?,
            word(rec + 16)?,
            word(rec + 20)?,
        );
        word(rec + 24)?;
        if inst == 0 {
            out.insert(ui, (stream, off, fmt));
        }
    }
    Ok(out)
}

fn u32_be(b: &[u8], o: usize) -> Res<u32> {
    b.get(o..o + 4)
        .map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
        .ok_or(Fatal::Short("a BF1943 program record"))
}

fn byte(b: &[u8], o: usize, what: &'static str) -> Res<u8> {
    b.get(o).copied().ok_or(Fatal::Short(what))
}

fn slice(b: &[u8], r: std::ops::Range<usize>) -> &[u8] {
    b.get(r.start.min(b.len())..r.end.min(b.len())).unwrap_or_default()
}

/// A BF1943 vertex shader as the plan reads it: its input usage indices (sorted), its fetches `(offset, semantic)`
/// sorted, and each fetch's Xenos format by offset.
type Parsed = (Vec<u32>, Vec<(u32, u32)>, HashMap<u32, u8>);

/// A candidate's score: its mesh kind, its variant, no fade, by layout (not role), every input found.
type Score = (bool, bool, bool, u8, bool);
/// The best candidate so far: BF1943's program, its inputs, its wiring, its score.
type Best = (usize, Vec<Input>, Vec<(u32, u32)>, Score);

/// The plan's tables of the two databases.
struct Tables<'d> {
    wnames: HashMap<&'d [u8], &'d [u8]>,
    wg: HashMap<&'d [u8], &'d [u16]>,
    pg: HashMap<&'d [u8], &'d [u16]>,
    layouts: HashMap<&'d [u8], &'d [u8]>,
    w_layouts: HashMap<&'d [u8], &'d [u8]>,
    w_fragments: HashMap<&'d [u8], &'d [u8]>,
    db_fragments: HashMap<&'d [u8], &'d [u8]>,
}

fn split_layouts<'d>(recs: &[&'d [u8]]) -> HashMap<&'d [u8], &'d [u8]> {
    recs.iter().map(|r| (slice(r, 0..16), &r[16.min(r.len())..])).collect()
}

struct Planner<'d, 'w, 'o, 'p> {
    w: &'d ShaderDb<'w>,
    db: &'d ShaderDb<'w>,
    t: Tables<'d>,
    o: &'o Options,
    planned_fn: Planned<'p>,
    bf_by_mesh: HashMap<Vec<u8>, Vec<&'o [u8]>>,
    parsed: HashMap<u32, Parsed>,
    planned: Option<PlannedParts>,
    soldier: Option<HashMap<Vec<Element>, bool>>,
}

/// `re.match(r'(.+?)_lod\d+/', k)`'s group.
fn lod_mesh(k: &[u8]) -> Option<&[u8]> {
    for p in 1..k.len() {
        if k[p - 1] == b'\n' {
            return None;
        }
        if k[p..].starts_with(b"_lod") {
            let rest = &k[p + 4..];
            let digits = rest.iter().take_while(|c| c.is_ascii_digit()).count();
            if digits > 0 && rest.get(digits) == Some(&b'/') {
                return Some(&k[..p]);
            }
        }
    }
    None
}

macro_rules! fail {
    ($programs:expr, $f:expr) => {
        return Ok(($programs, Some($f)))
    };
}

impl Planner<'_, '_, '_, '_> {
    fn element_map(&mut self, bf: Option<&[u8]>, pc: &[u8]) -> Res<Result<Option<ElementMap>, Fail>> {
        if let Some(kinds) = &self.soldier {
            let key: Vec<Element> = els(pc)?.into_iter().filter(|e| e.3 == 0).collect();
            if let (Some(&first), Some(bf)) = (kinds.get(&key), bf)
                && els(bf)?.len() > 3
            {
                match soldier_element_map(bf, pc, first)? {
                    Ok(Some(m)) => return Ok(Ok(Some(m))),
                    Ok(None) => {}
                    Err(f) => return Ok(Err(f)),
                }
            }
        }
        Ok(Ok(element_map(bf, pc)?))
    }

    fn vertex_shader(&mut self, bvs: u32) -> Res<Result<(), Fail>> {
        if self.parsed.contains_key(&bvs) {
            return Ok(Ok(()));
        }
        let i = bvs as usize;
        let sh = self.w.vs.get(i).ok_or(Fatal::Short("BF1943's vertex shaders"))?;
        let tree = match bc2_shader::xenos::parse(sh.code, Some(false)) {
            Ok(t) => t,
            Err(bc2_shader::xenos::XenosError::Format(m)) => return Ok(Err(Fail::Xenos(m))),
            Err(e) => return Err(Fatal::Shader(i, e.to_string())),
        };
        let tail = sh.tail;
        let nk = u32_be(tail, 0)? as usize;
        let mut fetch = HashMap::new();
        let mut bfel = Vec::new();
        for j in 0..nk {
            let b = 4 + 12 * j;
            let rec = tail
                .get(b..b + 12)
                .ok_or(Fatal::Short("a BF1943 vertex shader's elements"))?;
            let st = u16::from_be_bytes([rec[0], rec[1]]);
            let off = u32::from(u16::from_be_bytes([rec[2], rec[3]]));
            let ty = u32::from_be_bytes([rec[4], rec[5], rec[6], rec[7]]);
            let ui = u32::from(rec[10]);
            if st != 0xff {
                fetch.insert(off, (ty & 0x3f) as u8);
                bfel.push((off, ui));
            }
        }
        bfel.sort_unstable();
        let mut ins: Vec<u32> = tree.header.vertex_elements.iter().map(|e| e.usage_index).collect();
        ins.sort_unstable();
        self.parsed.insert(bvs, (ins, bfel, fetch));
        Ok(Ok(()))
    }

    fn fragment(&self, patch: bool, strip: bool, bi: usize) -> Res<Result<Vec<u8>, Fail>> {
        if strip || patch {
            let (name, what) = if patch {
                (&self.o.terrain_fragments.0, "patch")
            } else {
                (&self.o.terrain_fragments.1, "strip")
            };
            return Ok(match self.t.db_fragments.get(name.as_slice()) {
                Some(g) => Ok(g.to_vec()),
                None => Err(Fail::Value(format!(
                    "Dx{}: the database has no fragment {} for its terrain {what}es",
                    self.o.dx,
                    String::from_utf8_lossy(&latin_utf8(name))
                ))),
            });
        }
        let key = self.w.keys.get(bi).ok_or(Fatal::Short("BF1943's program keys"))?;
        Ok(mesh_fragment(&self.t.w_fragments, &self.t.db_fragments, key))
    }

    #[expect(clippy::too_many_lines, reason = "translate_db's entry, step for step")]
    fn entry(&mut self, name: &[u8], g: &[u8]) -> Res<(Vec<Program>, Option<Fail>)> {
        let mut out: Vec<Program> = Vec::new();
        let wguid = self.t.wnames[name];
        let Some(&wprogs) = self.t.wg.get(wguid) else {
            fail!(out, Fail::KeyBytes(wguid.to_vec()))
        };
        let mut bf_main: Vec<(u8, Vec<usize>)> = Vec::new();
        let mut bf_z: Vec<usize> = Vec::new();
        for &i in wprogs {
            let i = usize::from(i);
            let key = self.w.keys.get(i).ok_or(Fatal::Short("BF1943's program keys"))?;
            let k0 = byte(key, 0x30, "a BF1943 program key")?;
            if k0 == 0 {
                let k8 = byte(key, 0x38, "a BF1943 program key")?;
                match bf_main.iter_mut().find(|(x, _)| *x == k8) {
                    Some((_, v)) => v.push(i),
                    None => bf_main.push((k8, vec![i])),
                }
            } else if k0 == 5 {
                bf_z.push(i);
            }
        }
        if bf_main.is_empty() {
            fail!(out, Fail::Translate("BF1943 has no main program".into()));
        }
        let low = lower(name).into_owned();
        // layout guid -> instanced, in insertion order; layout guid -> BF1943's layout of a part using it
        let mut part_l: Vec<(Vec<u8>, bool)> = Vec::new();
        let mut part_bf: BTreeMap<Vec<u8>, Vec<u8>> = BTreeMap::new();
        let set_l = |part_l: &mut Vec<(Vec<u8>, bool)>, lg: &[u8], inst: bool| match part_l
            .iter_mut()
            .find(|(g, _)| g.as_slice() == lg)
        {
            Some((_, v)) => *v = inst,
            None => part_l.push((lg.to_vec(), inst)),
        };
        let bf_layouts: HashMap<&[u8], &[u8]> = self
            .o
            .bf_layouts
            .iter()
            .map(|(k, r)| (k.as_slice(), r.as_slice()))
            .collect();
        for p in &self.db.parts {
            let nl = lower(&p.name);
            if let Some(k) = find(&nl, b"_lod", 0)
                && k > 0
                && low.starts_with(&nl[..k])
                && low.get(k) == Some(&b'/')
            {
                if !p.name.contains(&b'/') {
                    return Err(Fatal::Short("a part name without a technique"));
                }
                let tech = rsplit_last(&p.name);
                if tech == b"Standard" || tech == b"StreamInstancingFloat" {
                    set_l(&mut part_l, p.guid, tech == b"StreamInstancingFloat");
                    let mut want = rsplit_first(&nl).to_vec();
                    want.extend_from_slice(b"/standard");
                    if let Some(rec) = bf_layouts.get(want.as_slice()) {
                        part_bf.entry(p.guid.to_vec()).or_insert_with(|| rec.to_vec());
                    }
                }
            }
        }
        let mut strip = false;
        let xl = || -> Res<HashSet<&[u8]>> {
            wprogs
                .iter()
                .map(|&i| {
                    self.w
                        .keys
                        .get(usize::from(i))
                        .map(|k| slice(k, 32..48))
                        .ok_or(Fatal::Short("BF1943's keys"))
                })
                .collect()
        };
        if part_l.is_empty() {
            let sx = swap_guid(&self.o.strip_layout);
            if xl()?.contains(sx.as_slice()) && self.t.layouts.contains_key(self.o.strip_layout.as_slice()) {
                set_l(&mut part_l, &self.o.strip_layout.clone(), false);
                let Some(rec) = self.t.w_layouts.get(sx.as_slice()) else {
                    fail!(out, Fail::KeyBytes(sx))
                };
                part_bf.insert(self.o.strip_layout.clone(), rec.to_vec());
                strip = true;
            }
        }
        let mut patch = false;
        if part_l.is_empty() && contains(name, &self.o.patches) && !self.o.patch_layouts.is_empty() {
            let pls = self.o.patch_layouts.clone();
            if xl()?.contains(swap_guid(&pls[0]).as_slice()) {
                let Some(&progs) = self.t.pg.get(g) else {
                    fail!(out, Fail::KeyBytes(g.to_vec()))
                };
                let mut have = HashSet::new();
                for &i in progs {
                    let k = self
                        .db
                        .keys
                        .get(usize::from(i))
                        .ok_or(Fatal::Short("the program keys"))?;
                    have.insert(slice(k, 0x20..0x30));
                }
                for lg in &pls {
                    if have.contains(lg.as_slice()) {
                        set_l(&mut part_l, lg, false);
                    }
                }
                patch = !part_l.is_empty();
            }
        }
        let mo = mesh_of(name);
        if part_l.is_empty() {
            if self.planned.is_none() {
                self.planned = Some((self.planned_fn)().map_err(Fatal::Planned)?);
            }
            if let Some(list) = self.planned.as_ref().and_then(|p| p.get(&mo)) {
                for (key, lg, inst) in list.clone() {
                    set_l(&mut part_l, &lg, inst);
                    let mut want = key.clone();
                    want.extend_from_slice(b"/standard");
                    if let Some(rec) = bf_layouts.get(want.as_slice()) {
                        part_bf.entry(lg.clone()).or_insert_with(|| rec.to_vec());
                    }
                }
            }
        }
        if part_l.is_empty() {
            fail!(out, Fail::Translate("no mesh parts in the database".into()));
        }
        let own: Vec<&[u8]> = self.bf_by_mesh.get(&mo).cloned().unwrap_or_default();
        let lgs: Vec<Vec<u8>> = part_l.iter().map(|(g, _)| g.clone()).collect();
        for lg in &lgs {
            if !part_bf.contains_key(lg) && !own.is_empty() {
                let Some(&pc) = self.t.layouts.get(lg.as_slice()) else {
                    fail!(out, Fail::KeyBytes(lg.clone()))
                };
                let mut best: Option<(usize, &[u8])> = None;
                for r in &own {
                    match self.element_map(Some(r), pc)? {
                        Err(f) => fail!(out, f),
                        Ok(Some(m)) if !m.is_empty() => {
                            if best.is_none_or(|b| m.len() > b.0) {
                                best = Some((m.len(), r));
                            }
                        }
                        Ok(_) => {}
                    }
                }
                if let Some((_, r)) = best {
                    part_bf.insert(lg.clone(), r.to_vec());
                }
            }
        }
        let Some(&tprogs) = self.t.pg.get(g) else {
            fail!(out, Fail::KeyBytes(g.to_vec()))
        };
        let mut templ: BTreeMap<(Vec<u8>, Vec<u8>), usize> = BTreeMap::new();
        for &i in tprogs {
            let i = usize::from(i);
            let k = self.db.keys.get(i).ok_or(Fatal::Short("the program keys"))?;
            let tl = if patch {
                slice(k, 0x20..0x30).to_vec()
            } else {
                Vec::new()
            };
            templ.entry((tl, slice(k, 0x30..k.len()).to_vec())).or_insert(i);
        }
        let mut part_fmt: HashMap<Vec<u8>, BTreeMap<u8, u8>> = HashMap::new();
        for (lg, rec) in &part_bf {
            let mut m = BTreeMap::new();
            for (f, _, off, st) in els(rec)? {
                if st == 0 {
                    m.insert(off, f);
                }
            }
            part_fmt.insert(lg.clone(), m);
        }
        let mut sorted_l = part_l.clone();
        sorted_l.sort();
        for (lg, inst) in sorted_l {
            let (pcel, at_offset) = if patch {
                (None, None)
            } else {
                let Some(&pc) = self.t.layouts.get(lg.as_slice()) else {
                    fail!(out, Fail::KeyBytes(lg.clone()))
                };
                let pcel = vertex_elements(pc)?;
                let pb = part_bf.get(&lg).cloned();
                match self.element_map(pb.as_deref(), pc)? {
                    Err(f) => fail!(out, f),
                    Ok(m) => (Some(pcel), m),
                }
            };
            for ((tl, perm), &ti) in &templ {
                let p4 = byte(perm, 4, "a program key")?;
                if p4 != u8::from(inst) || (patch && *tl != lg) {
                    continue;
                }
                let cands: Vec<usize> = if byte(perm, 0, "a program key")? == 5 {
                    bf_z.clone()
                } else {
                    let p8 = byte(perm, 8, "a program key")?;
                    bf_main
                        .iter()
                        .find(|(x, _)| *x == p8)
                        .map_or_else(|| bf_main[0].1.clone(), |v| v.1.clone())
                };
                let mut best: Option<Best> = None;
                for bi in cands {
                    let prog = self.w.programs.get(bi).ok_or(Fatal::Short("BF1943's programs"))?;
                    let bvs = u32_be(prog, 0x20)?;
                    if let Err(f) = self.vertex_shader(bvs)? {
                        fail!(out, f);
                    }
                    let (ins, bfel, fetch) = self.parsed[&bvs].clone();
                    let mut src: BTreeMap<u32, u32> = BTreeMap::new();
                    for &(bo, ui) in &bfel {
                        src.insert(ui, bo);
                    }
                    let mut src: Vec<(u32, u32)> = src.into_iter().collect();
                    let mut rank = 1u8;
                    let (full, inputs): (bool, Vec<Input>);
                    if patch {
                        let tvs = bc2_formats::shaderdb::merge::program_refs(self.db, ti)
                            .map_err(|_| Fatal::Short("a template program"))?[0];
                        let tail = self.db.vs.get(tvs).ok_or(Fatal::Short("the vertex shaders"))?.tail;
                        let mut tin = patch_inputs(tail)?;
                        if lg == self.o.patch_layouts[0]
                            && let Some(&(_, _, f1)) = tin.get(&1)
                        {
                            tin.entry(2).or_insert((2, 0, f1));
                        }
                        if !ins.iter().all(|ui| tin.contains_key(ui)) {
                            continue;
                        }
                        full = true;
                        inputs = ins
                            .iter()
                            .map(|ui| {
                                let (s, o, f) = tin[ui];
                                (s, o, f, *ui)
                            })
                            .collect();
                    } else if let Some(at) = &at_offset {
                        let bf_fmt = part_fmt.get(&lg).cloned().unwrap_or_default();
                        let mismatch = bfel.iter().any(|(bo, _)| {
                            let Ok(b8) = u8::try_from(*bo) else { return false };
                            bf_fmt
                                .get(&b8)
                                .is_some_and(|&f| xenos_fetch(f).unwrap_or(fetch[bo]) != fetch[bo])
                        });
                        if mismatch {
                            match by_role(&bfel, &fetch, &bf_fmt) {
                                None => continue,
                                Some(s) => src = s,
                            }
                            rank = 0;
                        }
                        full = src
                            .iter()
                            .all(|(_, bo)| u8::try_from(*bo).is_ok_and(|b| at.contains_key(&b)));
                        let mut v = Vec::new();
                        for &(ui, bo) in &src {
                            let Some(e) = u8::try_from(bo).ok().and_then(|b| at.get(&b)) else {
                                continue;
                            };
                            if !ins.contains(&ui) {
                                continue;
                            }
                            let Some(f) = dxgi(e.0) else {
                                fail!(out, Fail::KeyInt(i64::from(e.0)))
                            };
                            v.push((u32::from(e.3), u32::from(e.2), f, ui));
                        }
                        inputs = v;
                    } else {
                        let pcel = pcel.as_ref().ok_or(Fatal::Short("a layout"))?;
                        full = bfel.len() == pcel.len();
                        let mut v = Vec::new();
                        for (&(_, ui), &(f, _, off, st)) in bfel.iter().zip(pcel) {
                            if !ins.contains(&ui) {
                                continue;
                            }
                            let Some(d) = dxgi(f) else {
                                fail!(out, Fail::KeyInt(i64::from(f)))
                            };
                            v.push((u32::from(st), u32::from(off), d, ui));
                        }
                        inputs = v;
                    }
                    if inputs.len() < ins.len() {
                        continue;
                    }
                    let wk = self.w.keys.get(bi).ok_or(Fatal::Short("BF1943's program keys"))?;
                    let bk = byte(wk, 0x30 + VARIANT, "a BF1943 program key")?;
                    let score = (
                        byte(wk, 0x33, "a BF1943 program key")? == byte(perm, 3, "a program key")?,
                        bk == byte(perm, VARIANT, "a program key")? & !FADE,
                        bk & FADE == 0,
                        rank,
                        full,
                    );
                    if best.as_ref().is_none_or(|b| score > b.3) {
                        best = Some((bi, inputs, src, score));
                    }
                }
                let Some((bi, inputs, src, score)) = best else { continue };
                let prog = self.w.programs[bi];
                let refs = [
                    u32_be(prog, 0x20)?,
                    u32_be(prog, 0x24)?,
                    u32_be(prog, 0x28)?,
                    u32_be(prog, 0x2c)?,
                ];
                let kind = byte(self.w.keys[bi], 0x33, "a BF1943 program key")?;
                let fragment = self.fragment(patch, strip, bi)?;
                let template = self.db.keys.get(ti).ok_or(Fatal::Short("the program keys"))?;
                let key = program_key(template, &lg, kind, fragment.as_ref().ok().map(Vec::as_slice))
                    .ok_or(Fatal::Short("a program key"))?;
                let mut id = latin_utf8(name);
                id.extend_from_slice(&lg);
                id.extend_from_slice(slice(&key, 0x30..key.len()));
                let by = if at_offset.is_some() {
                    if score.3 == 1 { "layout" } else { "role" }
                } else {
                    "order"
                };
                out.push(Program {
                    layout: lg.clone(),
                    instanced: inst,
                    perm: perm.clone(),
                    template: ti,
                    source: bi,
                    refs,
                    inputs,
                    src,
                    rank: score.3,
                    by,
                    kind,
                    strip,
                    patch,
                    fragment,
                    key,
                    pid: md5(&id),
                });
            }
        }
        if out.is_empty() {
            fail!(out, Fail::Translate("no program could be made for its layouts".into()));
        }
        Ok((out, None))
    }
}

/// `source_fragment`: the renderer fragment of a translated mesh program - BF1943's own program's (its key's bytes
/// 0x10-0x20), by name in the PC database (`w_fragments`: BF1943's GUID -> name, `db_fragments`: name -> the PC
/// GUID; the last of each kept). The engine looks a mesh's programs up by the fragment its draw asks for (a palm or
/// bush of the Overgrowth system: `LODFade` / `LODFade_Shadow`; a plain mesh: zero, kept zero), so a program filed
/// under another is never found. [`Fail::Translate`] when BF1943's fragment has no PC counterpart (the Xbox emitter
/// fragment).
///
/// # Errors
/// [`Fail::Translate`]: no PC fragment of BF1943's name.
pub fn mesh_fragment<S: std::hash::BuildHasher>(
    w_fragments: &HashMap<&[u8], &[u8], S>,
    db_fragments: &HashMap<&[u8], &[u8], S>,
    source_key: &[u8],
) -> Result<Vec<u8>, Fail> {
    let g = slice(source_key, 0x10..0x20);
    if g.len() == 16 && g.iter().all(|&b| b == 0) {
        return Ok(g.to_vec());
    }
    let name = w_fragments.get(g).copied();
    if let Some(pc) = name.and_then(|n| db_fragments.get(n)) {
        return Ok(pc.to_vec());
    }
    let shown = match name {
        Some(n) if !n.is_empty() => String::from_utf8_lossy(&latin_utf8(n)).into_owned(),
        _ => bc2_formats::shaderdb::merge::hex(g),
    };
    Err(Fail::Translate(format!("no PC renderer fragment for BF1943's {shown}")))
}

/// `translated_program_key`: the template's key with the translation's renderer fragment (when there is one), vertex
/// layout and mesh kind - never the template's fragment (on a database translated before it carries whatever the
/// last run wrote; a donor's is the donor material's draw). `None` for a key too short.
#[must_use]
pub fn program_key(template: &[u8], layout: &[u8], kind: u8, fragment: Option<&[u8]>) -> Option<Vec<u8>> {
    if template.len() < 0x34 || layout.len() != 16 || fragment.is_some_and(|f| f.len() != 16) {
        return None;
    }
    let mut key = template.to_vec();
    if let Some(f) = fragment {
        key[0x10..0x20].copy_from_slice(f);
    }
    key[0x20..0x30].copy_from_slice(layout);
    key[0x33] = kind;
    Some(key)
}

/// `translate_db`'s todo list and each entry's programs ([`Program`]), in the database's entry order.
///
/// # Errors
/// [`Fatal`]: what the Python could not have run (module docs).
pub fn plan(w: &ShaderDb<'_>, db: &ShaderDb<'_>, o: &Options, planned: Planned<'_>) -> Res<Vec<Entry>> {
    let t = Tables {
        wnames: w.entries.iter().map(|e| (&*e.name, e.guid)).collect(),
        wg: w.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect(),
        pg: db.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect(),
        layouts: split_layouts(&db.layouts),
        w_layouts: split_layouts(&w.layouts),
        w_fragments: w.fragments.iter().map(|f| (f.guid, &*f.name)).collect(),
        db_fragments: db.fragments.iter().map(|f| (&*f.name, f.guid)).collect(),
    };
    let mut todo: Vec<(&[u8], &[u8])> = Vec::new();
    for e in &db.entries {
        let name: &[u8] = &e.name;
        let low = lower(name);
        let leaf = rsplit_last(&low);
        if !t.wnames.contains_key(name)
            || !o.filters.iter().any(|f| contains(&low, f))
            || o.exclude.iter().any(|x| contains(&low, x))
            || o.keep_donor.iter().any(|k| contains(leaf, k))
            || o.only.as_ref().is_some_and(|only| !only.contains(name))
            || o.dice_twins.contains(&mesh_of(name))
        {
            continue;
        }
        todo.push((name, e.guid));
    }
    let soldier = if o.soldier { Some(soldier_layouts(db)?) } else { None };
    let mut bf_by_mesh: HashMap<Vec<u8>, Vec<&[u8]>> = HashMap::new();
    for (k, r) in &o.bf_layouts {
        if let Some(m) = lod_mesh(k) {
            bf_by_mesh.entry(m.to_vec()).or_default().push(r);
        }
    }
    let mut p = Planner {
        w,
        db,
        t,
        o,
        planned_fn: planned,
        bf_by_mesh,
        parsed: HashMap::new(),
        planned: None,
        soldier,
    };
    let mut out = Vec::with_capacity(todo.len());
    for (name, g) in todo {
        let (programs, error) = p.entry(name, g)?;
        out.push(Entry {
            name: name.to_vec(),
            guid: g.to_vec(),
            programs,
            error,
        });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ entries made

fn meshes_with_parts(db: &ShaderDb<'_>) -> HashSet<Vec<u8>> {
    db.parts
        .iter()
        .filter_map(|p| {
            let nl = lower(&p.name);
            find(&nl, b"_lod", 0).map(|k| nl[..k].to_vec())
        })
        .collect()
}

fn kept_on_donor(name: &[u8], keep_donor: &[Vec<u8>]) -> bool {
    let low = lower(name);
    let leaf = rsplit_last(&low);
    keep_donor.iter().any(|k| contains(leaf, k))
}

fn wanted(low: &[u8], filters: &[Vec<u8>], exclude: &[Vec<u8>]) -> bool {
    filters.iter().any(|f| contains(low, f)) && !exclude.iter().any(|x| contains(low, x))
}

/// `add_missing_entries`' choice: `(BF1943 material, the sibling entry it is cloned from)` for each BF1943 material
/// of `w` the database lacks whose mesh is a ported one (`ported`, lower) with parts here, in `w`'s entry order.
#[must_use]
pub fn missing_entries(
    w: &ShaderDb<'_>,
    db: &ShaderDb<'_>,
    filters: &[Vec<u8>],
    exclude: &[Vec<u8>],
    keep_donor: &[Vec<u8>],
    ported: &HashSet<Vec<u8>>,
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let have: HashSet<Vec<u8>> = db.entries.iter().map(|e| lower(&e.name).into_owned()).collect();
    let mut by_mesh: HashMap<Vec<u8>, Vec<&[u8]>> = HashMap::new();
    for e in &db.entries {
        by_mesh.entry(mesh_of(&e.name)).or_default().push(&e.name);
    }
    let parts = meshes_with_parts(db);
    let mut out = Vec::new();
    for e in &w.entries {
        let low = lower(&e.name);
        if have.contains(low.as_ref()) || !wanted(&low, filters, exclude) || kept_on_donor(&e.name, keep_donor) {
            continue;
        }
        let mo = mesh_of(&e.name);
        let sib = by_mesh
            .get(&mo)
            .and_then(|v| v.iter().find(|s| !kept_on_donor(s, keep_donor)));
        let Some(sib) = sib else { continue };
        if !parts.contains(&mo) || !ported.contains(&mo) {
            continue;
        }
        out.push((e.name.to_vec(), sib.to_vec()));
    }
    out
}

/// One step of `foreign_entries`' choice, in its order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Foreign {
    /// `(level database, entry, template, cutout)`: the entry is made from the template.
    Make(String, Vec<u8>, Vec<u8>, bool),
    /// `(entry, the template given)`: the template chosen on another database is not in this one (left out).
    Missing(Vec<u8>, Option<Vec<u8>>),
}

/// What `foreign_entries` asks of the caller while it chooses.
pub trait ForeignSource {
    /// The materials (lower) the staged mesh set of a mesh (lower) names.
    ///
    /// # Errors
    /// The caller's failure (the run stops).
    fn materials(&mut self, mesh: &[u8]) -> Result<HashSet<Vec<u8>>, String>;
    /// A pixel shader's compiled code.
    ///
    /// # Errors
    /// The caller's failure (the run stops).
    fn pixel_code(&mut self, ps: usize) -> Result<Vec<u8>, String>;
}

/// The words of a material that BF1943 alpha-tests (`CUTOUT_WORDS`).
pub const CUTOUT_WORDS: [&[u8]; 8] = [
    b"frond", b"leaf", b"leaves", b"grass", b"bush", b"fern", b"shadow", b"plant",
];

/// `wake_install.other_level_variant(material, siblings)`: another BF1943 level's variant (`<stem>_<tag>`, the tag
/// one of `tags`) of a material whose Wake variant (`<stem>_wake`) is among the siblings.
#[must_use]
pub fn other_level_variant<'s>(
    material: &[u8],
    siblings: impl IntoIterator<Item = &'s Vec<u8>>,
    tags: &HashSet<Vec<u8>>,
) -> bool {
    let leaf = lower(rsplit_last(material)).into_owned();
    let (stem, tag) = match leaf.iter().rposition(|&c| c == b'_') {
        Some(k) => (&leaf[..k], &leaf[k + 1..]),
        None => (&leaf[..0], leaf.as_slice()),
    };
    if stem.is_empty() || !tags.contains(tag) {
        return false;
    }
    let mut want = stem.to_vec();
    want.extend_from_slice(b"_wake");
    siblings
        .into_iter()
        .any(|s| lower(rsplit_last(s)).as_ref() == want.as_slice())
}

/// `foreign_entries`' arguments besides the databases.
pub struct ForeignOptions<'a> {
    /// An entry's lower name holds one of them (lower).
    pub filters: &'a [Vec<u8>],
    /// ... and none of them (lower).
    pub exclude: &'a [Vec<u8>],
    /// The meshes the level draws as BF1943's ports (`needs_ported.txt`, as written).
    pub ported: &'a HashSet<Vec<u8>>,
    /// The other levels' material variant tags (`wake_install.OTHER_LEVEL_VARIANTS`).
    pub variant_tags: &'a HashSet<Vec<u8>>,
    /// The templates chosen on another renderer's database.
    pub templates: Option<&'a HashMap<Vec<u8>, Vec<u8>>>,
}

/// `foreign_entries`' choice: for the ported meshes Wake's BF1943 database (`wake` entries' names) has no entries
/// for, the entries other BF1943 level databases (`levels`: their names in order, each with its entries) define,
/// each with its template - a BFBC2 vegetation entry of the same alpha test, the same material leaf first, then the
/// one with most programs, then by name ([`ForeignOptions::templates`]: those chosen on another renderer's
/// database).
///
/// # Errors
/// The caller's failure, or a program record or key not there or too short.
pub fn foreign_entries(
    wake: &[Vec<u8>],
    db: &ShaderDb<'_>,
    levels: &[(String, Vec<Vec<u8>>)],
    o: &ForeignOptions<'_>,
    source: &mut dyn ForeignSource,
) -> Result<Vec<Foreign>, String> {
    let mut have: HashSet<Vec<u8>> = db.entries.iter().map(|e| lower(&e.name).into_owned()).collect();
    let wake_low: HashSet<Vec<u8>> = wake.iter().map(|n| lower(n).into_owned()).collect();
    let wake_meshes: HashSet<Vec<u8>> = wake.iter().map(|n| mesh_of(n)).collect();
    let parts = meshes_with_parts(db);
    let pg: HashMap<&[u8], &[u16]> = db.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect();
    let db_names: HashSet<&[u8]> = db.entries.iter().map(|e| &*e.name).collect();
    let mut used: HashMap<Vec<u8>, HashSet<Vec<u8>>> = HashMap::new();
    let mut discards: HashMap<Vec<u8>, Option<bool>> = HashMap::new();
    let mut ps_cut: HashMap<usize, bool> = HashMap::new();
    let mut out = Vec::new();
    for (k, names) in levels {
        for n in names {
            let low = lower(n).into_owned();
            let mo = mesh_of(n);
            if have.contains(&low)
                || wake_low.contains(&low)
                || wake_meshes.contains(&mo)
                || !parts.contains(&mo)
                || !o.ported.contains(&mo)
                || !wanted(&low, o.filters, o.exclude)
            {
                continue;
            }
            if !used.contains_key(&mo) {
                let m = source.materials(&mo)?;
                used.insert(mo.clone(), m);
            }
            let mats = &used[&mo];
            let material = low.get(mo.len() + 1..).unwrap_or_default();
            if !mats.contains(material) || other_level_variant(material, mats, o.variant_tags) {
                continue;
            }
            let leaf = rsplit_last(&low);
            let want = CUTOUT_WORDS.iter().any(|t| contains(leaf, t));
            let tmpl = if let Some(templates) = o.templates {
                match templates.get(n) {
                    Some(t) if db_names.contains(t.as_slice()) => t.clone(),
                    t => {
                        out.push(Foreign::Missing(n.clone(), t.cloned()));
                        continue;
                    }
                }
            } else {
                let mut best: Option<(bool, i64, &[u8])> = None;
                for e in &db.entries {
                    if !lower(&e.name).starts_with(b"objects/vegetation/") {
                        continue;
                    }
                    let g = e.guid.to_vec();
                    if !discards.contains_key(&g) {
                        let mut flags = HashSet::new();
                        for &i in pg.get(e.guid).copied().unwrap_or_default() {
                            let i = usize::from(i);
                            let key = db.keys.get(i).ok_or("a program key is not there")?;
                            if *key.get(0x30).ok_or("a program key is short")? != 0 {
                                continue;
                            }
                            let p = bc2_formats::shaderdb::merge::program_refs(db, i)
                                .map_err(|_| "a program record is short".to_string())?[1];
                            if let std::collections::hash_map::Entry::Vacant(v) = ps_cut.entry(p) {
                                v.insert(discards_code(&source.pixel_code(p)?));
                            }
                            flags.insert(ps_cut[&p]);
                        }
                        let one = if flags.len() == 1 {
                            flags.into_iter().next()
                        } else {
                            None
                        };
                        discards.insert(g.clone(), one);
                    }
                    if discards[&g] != Some(want) {
                        continue;
                    }
                    let programs = i64::try_from(pg.get(e.guid).map_or(0, |v| v.len())).unwrap_or(i64::MAX);
                    let c = (lower(rsplit_last(&e.name)).as_ref() != leaf, -programs, &*e.name);
                    if best.is_none_or(|b| c < b) {
                        best = Some(c);
                    }
                }
                let Some((_, _, t)) = best else { continue };
                t.to_vec()
            };
            have.insert(low);
            out.push(Foreign::Make(k.clone(), n.clone(), tmpl, want));
        }
    }
    Ok(out)
}

/// Whether a compiled Dx10 / Dx11 shader discards (`'discard' in pcbuild.disassemble(code)`): its shader chunk
/// (SHDR / SHEX) holds a `discard_z` / `discard_nz` instruction (opcode 13). Code that is not DXBC, or whose
/// chunk does not walk, is answered false, as the disassembler's empty text was.
#[must_use]
pub fn discards_code(code: &[u8]) -> bool {
    let u = |o: usize| code.get(o..o + 4).map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if code.get(..4) != Some(b"DXBC".as_slice()) {
        return false;
    }
    let Some(n) = u(28) else { return false };
    for c in 0..n as usize {
        let Some(off) = u(32 + 4 * c) else { return false };
        let off = off as usize;
        let tag = code.get(off..off + 4);
        if tag != Some(b"SHDR".as_slice()) && tag != Some(b"SHEX".as_slice()) {
            continue;
        }
        let Some(size) = u(off + 4) else { return false };
        let start = off + 8;
        let Some(len) = u(start + 4) else { return false };
        let end = (start + 4 * len as usize).min(start + size as usize);
        let mut p = start + 8;
        while p < end {
            let Some(tok) = u(p) else { return false };
            let op = tok & 0x7ff;
            if op == 0x0d {
                return true;
            }
            let n = if op == 0x35 {
                u(p + 4).unwrap_or(0)
            } else {
                (tok >> 24) & 0x7f
            };
            if n == 0 {
                return false;
            }
            p += 4 * n as usize;
        }
    }
    false
}

// ------------------------------------------------------------------------------------------------ the soldiers

fn replace(s: &[u8], from: &[u8], to: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(s.len());
    let mut i = 0;
    while i < s.len() {
        if !from.is_empty() && s[i..].starts_with(from) {
            out.extend_from_slice(to);
            i += from.len();
        } else {
            out.push(s[i]);
            i += 1;
        }
    }
    out
}

fn norm(name: &[u8]) -> Vec<u8> {
    replace(&lower(name), b"/shaders/", b"/")
}

/// `ww2_char_shaders.match_entries(w, db)`: `(our entry, BF1943's)` for the soldier entries (`characters/ww2/` ->
/// `characters/beach/`, Shaders folders dropped) and the weapon entries (`weapons`: our mesh -> BF1943's) of `db`
/// that `w` has, in `db`'s entry order (a name met again keeps its place, its last match).
#[must_use]
pub fn match_entries(
    w: &ShaderDb<'_>,
    db: &ShaderDb<'_>,
    keep_donor: &[Vec<u8>],
    weapons: &[(Vec<u8>, Vec<u8>)],
) -> Vec<(Vec<u8>, Vec<u8>)> {
    let mut bf: HashMap<Vec<u8>, &[u8]> = HashMap::new();
    for e in &w.entries {
        bf.entry(norm(&e.name)).or_insert(&e.name);
    }
    let mut out: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for e in &db.entries {
        let low = lower(&e.name);
        if kept_on_donor(&e.name, keep_donor) {
            continue;
        }
        let hit = if low.starts_with(b"characters/ww2/") {
            bf.get(&replace(&norm(&e.name), b"characters/ww2/", b"characters/beach/"))
        } else {
            weapons
                .iter()
                .find(|(ours, _)| {
                    low.starts_with(ours)
                        && (low[ours.len()..].starts_with(b"/") || low[ours.len()..].starts_with(b"_lod"))
                })
                .and_then(|(ours, theirs)| {
                    let mut n = theirs.clone();
                    n.extend_from_slice(&low[ours.len()..]);
                    bf.get(&norm(&n))
                })
        };
        if let Some(h) = hit {
            match out.iter_mut().find(|(n, _)| n.as_slice() == &*e.name) {
                Some((_, v)) => *v = h.to_vec(),
                None => out.push((e.name.to_vec(), h.to_vec())),
            }
        }
    }
    out
}

/// A program's permutation as `ww2_char_shaders.perms` keeps it: (pass, layout, instanced, point light, kind,
/// fragment).
pub type Perm = (u8, Vec<u8>, u8, u8, u8, Vec<u8>);

/// `ww2_char_shaders.perms(db, names)`: each named entry's program permutations, by name.
///
/// # Errors
/// An entry's group or a key is not there or is short.
pub fn perms(db: &ShaderDb<'_>, names: &HashSet<Vec<u8>>) -> Result<HashMap<Vec<u8>, HashSet<Perm>>, String> {
    let pg: HashMap<&[u8], &[u16]> = db.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect();
    let mut out = HashMap::new();
    for e in &db.entries {
        if !names.contains(&*e.name) {
            continue;
        }
        let progs = pg
            .get(e.guid)
            .ok_or_else(|| format!("an entry has no group: {}", bc2_formats::shaderdb::merge::hex(e.guid)))?;
        let mut set = HashSet::new();
        for &i in *progs {
            let k = db
                .keys
                .get(usize::from(i))
                .filter(|k| k.len() > 0x38)
                .ok_or("a program key is short")?;
            set.insert((
                k[0x30],
                k[0x20..0x30].to_vec(),
                k[0x34],
                k[0x38],
                k[0x33],
                k[0x10..0x20].to_vec(),
            ));
        }
        out.insert(e.name.to_vec(), set);
    }
    Ok(out)
}

/// `ww2_char_shaders.part_users(db, names)`: per name the layouts of its mesh's parts (`Standard` /
/// `StreamInstancingFloat`; a mesh is a part name before `_lod`, the entry's name beginning with it and `/`).
#[must_use]
pub fn part_users(db: &ShaderDb<'_>, names: &[Vec<u8>]) -> Vec<(Vec<u8>, HashSet<Vec<u8>>)> {
    let mut by_mesh: HashMap<Vec<u8>, HashSet<Vec<u8>>> = HashMap::new();
    for p in &db.parts {
        if !p.name.contains(&b'/') {
            continue;
        }
        let nl = lower(&p.name);
        let tech = rsplit_last(&p.name);
        if let Some(k) = find(&nl, b"_lod", 0)
            && k > 0
            && (tech == b"Standard" || tech == b"StreamInstancingFloat")
        {
            by_mesh.entry(nl[..k].to_vec()).or_default().insert(p.guid.to_vec());
        }
    }
    names
        .iter()
        .map(|n| {
            let low = lower(n);
            let mut set = HashSet::new();
            #[expect(clippy::iter_over_hash_type, reason = "a union: the order does not matter")]
            for (m, v) in &by_mesh {
                if low.starts_with(m) && low.get(m.len()) == Some(&b'/') {
                    set.extend(v.iter().cloned());
                }
            }
            (n.clone(), set)
        })
        .collect()
}

/// `ww2_char_shaders.entry_mesh(name)`: the mesh (lower) an entry's name begins with, up to `/characters/` or
/// `/objects/` (after its first character), else [`mesh_of`].
#[must_use]
pub fn entry_mesh(name: &[u8]) -> Vec<u8> {
    let low = lower(name).into_owned();
    let k = [find(&low, b"/characters/", 1), find(&low, b"/objects/", 1)]
        .into_iter()
        .flatten()
        .filter(|&k| k > 0)
        .min();
    k.map_or_else(|| mesh_of(name), |k| low[..k].to_vec())
}

/// What [`offer_kinds`] adds to BF1943's database.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Offered {
    /// `(the program copied, its new key)`, appended to the programs and keys in order.
    pub added: Vec<(usize, Vec<u8>)>,
    /// The groups afterwards, in their order.
    pub groups: Vec<(Vec<u8>, Vec<usize>)>,
    /// `(our entry, why)` of the entries that cannot have a needed kind.
    pub bad: Vec<(Vec<u8>, String)>,
    /// The log lines.
    pub log: Vec<String>,
}

/// The skinned kinds (key byte 0x33).
pub const KINDS: [u8; 2] = [1, 4];

/// `ww2_char_shaders.offer_kinds(w, entries, need, single)`: BF1943's programs of a material copied as the skinning
/// kind our parts draw with that BF1943 did not compile, for a material whose parts are all single-bone.
/// `entries`: our entry -> its name in `w`; `need`: our entry -> its (pass, kind)s.
///
/// # Errors
/// An entry, group or key is not there or is short.
pub fn offer_kinds(
    w: &ShaderDb<'_>,
    entries: &[(Vec<u8>, Vec<u8>)],
    need: &HashMap<Vec<u8>, Vec<(u8, u8)>>,
    single: &HashMap<Vec<u8>, bool>,
) -> Result<Offered, String> {
    let wn: HashMap<&[u8], &[u8]> = w.entries.iter().map(|e| (&*e.name, e.guid)).collect();
    let mut groups: Vec<(Vec<u8>, Vec<usize>)> = Vec::new();
    for (g, idx) in &w.groups {
        let v: Vec<usize> = idx.iter().map(|&i| usize::from(i)).collect();
        match groups.iter_mut().find(|(x, _)| x.as_slice() == *g) {
            Some((_, old)) => *old = v,
            None => groups.push((g.to_vec(), v)),
        }
    }
    let mut keys: Vec<Vec<u8>> = w.keys.iter().map(|k| k.to_vec()).collect();
    let kb = |keys: &Vec<Vec<u8>>, i: usize, o: usize| -> Result<u8, String> {
        keys.get(i)
            .and_then(|k| k.get(o))
            .copied()
            .ok_or_else(|| "a program key is short".into())
    };
    let leaf = |b: &[u8]| String::from_utf8_lossy(&latin_utf8(rsplit_last(b))).into_owned();
    let mut out = Offered::default();
    let mut done: HashSet<(Vec<u8>, u8, u8)> = HashSet::new();
    let mut sorted: Vec<&(Vec<u8>, Vec<u8>)> = entries.iter().collect();
    sorted.sort();
    for (ours, bfn) in sorted {
        let g = wn
            .get(bfn.as_slice())
            .ok_or("an entry is not in BF1943's database")?
            .to_vec();
        let gi = groups
            .iter()
            .position(|(x, _)| *x == g)
            .ok_or("an entry has no group")?;
        let mut have = HashSet::new();
        for &i in &groups[gi].1 {
            have.insert((kb(&keys, i, 0x30)?, kb(&keys, i, 0x33)?));
        }
        let mut wanted: Vec<(u8, u8)> = need.get(ours).cloned().unwrap_or_default();
        wanted.sort_unstable();
        wanted.dedup();
        let one = single.get(ours).copied().unwrap_or(false);
        for (ps, kind) in wanted {
            if have.contains(&(ps, kind)) || !KINDS.contains(&kind) {
                continue;
            }
            let mut other = Vec::new();
            for &i in &groups[gi].1 {
                if kb(&keys, i, 0x30)? == ps && KINDS.contains(&kb(&keys, i, 0x33)?) {
                    other.push(i);
                }
            }
            if other.is_empty() || !one {
                out.bad.push((
                    ours.clone(),
                    format!(
                        "BF1943 has no kind-{kind} program for pass {ps}{}{}",
                        if other.is_empty() {
                            " nor another skinned one"
                        } else {
                            ""
                        },
                        if one {
                            ""
                        } else {
                            " and its parts are not all single-bone"
                        }
                    ),
                ));
                break;
            }
            if !done.insert((g.clone(), ps, kind)) {
                continue;
            }
            for i in other {
                let mut key = keys[i].clone();
                *key.get_mut(0x33).ok_or("a program key is short")? = kind;
                keys.push(key.clone());
                let n = w.programs.len() + out.added.len();
                out.added.push((i, key));
                groups[gi].1.push(n);
            }
            out.log.push(format!(
                "  {} ({}): BF1943's pass-{ps} programs offered as kind {kind} (single-bone parts)",
                leaf(ours),
                leaf(&entry_mesh(ours))
            ));
        }
    }
    out.groups = w
        .groups
        .iter()
        .map(|(g, _)| {
            let v = groups
                .iter()
                .find(|(x, _)| x.as_slice() == *g)
                .map_or_else(Vec::new, |x| x.1.clone());
            (g.to_vec(), v)
        })
        .collect();
    Ok(out)
}

/// `ww2_char_shaders.entry_blocks(w, names)`: the parameter blocks the programs of the named entries use (their two
/// blocks and their shaders' own).
///
/// # Errors
/// An entry, group, program or shader is not there or is short.
pub fn entry_blocks(w: &ShaderDb<'_>, names: &[Vec<u8>]) -> Result<std::collections::BTreeSet<u32>, String> {
    let wn: HashMap<&[u8], &[u8]> = w.entries.iter().map(|e| (&*e.name, e.guid)).collect();
    let groups: HashMap<&[u8], &[u16]> = w.groups.iter().map(|(g, idx)| (*g, idx.as_slice())).collect();
    let mut out = std::collections::BTreeSet::new();
    for n in names {
        let g = wn.get(n.as_slice()).ok_or("an entry is not in the database")?;
        for &i in *groups.get(g).ok_or("an entry has no group")? {
            let r = bc2_formats::shaderdb::merge::program_refs(w, usize::from(i))
                .map_err(|_| "a program record is short".to_string())?;
            let (b1, b2) = (
                u32::try_from(r[2]).unwrap_or(u32::MAX),
                u32::try_from(r[3]).unwrap_or(u32::MAX),
            );
            out.insert(b1);
            out.insert(b2);
            out.insert(w.vs.get(r[0]).ok_or("a vertex shader is not there")?.param);
            out.insert(w.ps.get(r[1]).ok_or("a pixel shader is not there")?.param);
        }
    }
    Ok(out)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, reason = "the tests' own fixtures")]
mod tests {
    use std::borrow::Cow;

    use bc2_formats::shaderdb::Named;

    use super::*;

    /// A 0x49-byte layout record body: elements, count at 64, stride at 69.
    fn layout(els: &[Element], stride: u8) -> Vec<u8> {
        let mut r = vec![0u8; 0x49];
        for (i, e) in els.iter().enumerate() {
            r[4 * i..4 * i + 4].copy_from_slice(&[e.0, e.1, e.2, e.3]);
        }
        r[64] = u8::try_from(els.len()).unwrap();
        r[69] = stride;
        r
    }

    fn named(n: &str, g: u8) -> Named<'static> {
        Named {
            name: Cow::Owned(n.as_bytes().to_vec()),
            guid: Box::leak(Box::new([g; 16])),
        }
    }

    #[test]
    fn mesh_of_cases() {
        assert_eq!(mesh_of(b"Objects/A/B_Mesh/Objects/A/Shaders/M"), b"objects/a/b_mesh");
        assert_eq!(mesh_of(b"Foo/Bar_mesh/terrains/mp_004/x"), b"foo/bar_mesh");
        assert_eq!(mesh_of(b"Objects/Tree/Tree/objects/x/y"), b"objects/tree/tree");
        assert_eq!(mesh_of(b"Terrains/Wake/Patch/X"), b"terrains/wake/patch");
        assert_eq!(mesh_of(b"plain"), b"plain");
        assert_eq!(mesh_of(b"\xc9/X"), b"\xe9");
    }

    #[test]
    fn lod_mesh_is_the_shortest_prefix() {
        assert_eq!(lod_mesh(b"a_lod0/b_lod1/standard"), Some(b"a".as_slice()));
        assert_eq!(lod_mesh(b"_lod0/x"), None);
        assert_eq!(lod_mesh(b"a_lodx/b_lod12/s"), Some(b"a_lodx/b".as_slice()));
        assert_eq!(lod_mesh(b"a\nb_lod0/x"), None);
    }

    #[test]
    fn vertex_elements_drop_an_element_inside_the_one_before() {
        // float4 position (16 bytes) with an R16 at 12, then a texcoord at 16
        let r = layout(&[(4, 1, 0, 0), (5, 7, 12, 0), (6, 0, 16, 0), (4, 8, 0, 1)], 20);
        assert_eq!(vertex_elements(&r).unwrap(), vec![(4, 1, 0, 0), (6, 0, 16, 0)]);
        assert!(vertex_elements(&[0u8; 10]).is_err());
    }

    #[test]
    fn element_map_pairs_by_kind_and_falls_back() {
        let bf = layout(&[(3, 1, 0, 0), (0x2b, 3, 12, 0), (6, 0, 16, 0)], 20);
        let pc = layout(&[(3, 1, 0, 0), (6, 0, 12, 0), (8, 3, 16, 0)], 24);
        let m = element_map(Some(&bf), &pc).unwrap().unwrap();
        assert_eq!(m[&12], (8, 3, 16, 0));
        assert_eq!(m[&16], (6, 0, 12, 0));
        // more PC texcoords than BF1943's: the extra one read by nothing
        let pc2 = layout(&[(3, 1, 0, 0), (6, 0, 12, 0), (6, 0, 16, 0), (8, 3, 20, 0)], 28);
        let m = element_map(Some(&bf), &pc2).unwrap().unwrap();
        assert_eq!(m.len(), 3);
        // BF1943 data without a place
        let pc3 = layout(&[(3, 1, 0, 0), (7, 9, 12, 0)], 20);
        assert_eq!(element_map(Some(&bf), &pc3).unwrap(), None);
        assert_eq!(element_map(None, &pc).unwrap(), None);
    }

    #[test]
    fn soldier_element_map_by_meaning() {
        let bf = layout(
            &[
                (3, 1, 0, 0),
                (6, 0, 12, 0),
                (6, 0, 16, 0),
                (8, 0, 20, 0),
                (12, 2, 28, 0),
            ],
            32,
        );
        let pc = layout(
            &[
                (3, 1, 0, 0),
                (8, 0, 16, 0),
                (12, 2, 24, 0),
                (6, 0, 40, 0),
                (6, 0, 48, 0),
                (6, 0, 52, 0),
            ],
            56,
        );
        let m = soldier_element_map(&bf, &pc, true).unwrap().unwrap().unwrap();
        assert_eq!(m[&12].2, 40);
        assert_eq!(m[&16].2, 48);
        assert_eq!(m[&20].2, 16);
        assert_eq!(m[&28].2, 24);
        let m = soldier_element_map(&bf, &pc, false).unwrap().unwrap().unwrap();
        assert_eq!(m[&16].2, 52);
        let no40 = layout(&[(3, 1, 0, 0), (8, 0, 16, 0), (12, 2, 24, 0)], 56);
        assert_eq!(soldier_element_map(&bf, &no40, true).unwrap(), Err(Fail::KeyInt(40)));
        let no_normal = layout(&[(3, 1, 0, 0), (6, 0, 40, 0)], 56);
        assert_eq!(soldier_element_map(&bf, &no_normal, true).unwrap(), Ok(None));
    }

    #[test]
    fn by_role_takes_the_next_element_of_the_fetch_format() {
        let bfel = vec![(0, 0), (12, 1), (24, 2)];
        let fetch: HashMap<u32, u8> = [(0, 57), (12, 31), (24, 31)].into_iter().collect();
        let fmt: BTreeMap<u8, u8> = [(0, 3), (16, 6), (32, 6), (40, 6)].into_iter().collect();
        assert_eq!(by_role(&bfel, &fetch, &fmt), Some(vec![(0, 0), (1, 16), (2, 32)]));
        let short: BTreeMap<u8, u8> = [(0, 3), (16, 6)].into_iter().collect();
        assert_eq!(by_role(&bfel, &fetch, &short), None);
    }

    #[test]
    fn patch_inputs_per_vertex_only() {
        let mut t = Vec::new();
        t.extend(4u32.to_le_bytes());
        t.extend([1, 2, 3, 4]);
        t.extend(3u32.to_le_bytes());
        for rec in [
            [0u32, 0, 38, 0, 0, 0, 0],
            [0, 1, 13, 1, 0, 0, 0],
            [0, 5, 2, 2, 16, 1, 0],
        ] {
            for x in rec {
                t.extend(x.to_le_bytes());
            }
        }
        let got = patch_inputs(&t).unwrap();
        assert_eq!(
            got.into_iter().collect::<Vec<_>>(),
            vec![(0, (0, 0, 38)), (1, (1, 0, 13))]
        );
        assert!(patch_inputs(&t[..t.len() - 1]).is_err());
    }

    fn dxbc(tokens: &[u32]) -> Vec<u8> {
        let mut shdr = Vec::new();
        shdr.extend(0x0040_0050u32.to_le_bytes());
        shdr.extend(u32::try_from(tokens.len() + 2).unwrap().to_le_bytes());
        for t in tokens {
            shdr.extend(t.to_le_bytes());
        }
        let mut out = b"DXBC".to_vec();
        out.extend([0u8; 16]);
        out.extend(1u32.to_le_bytes());
        out.extend(0u32.to_le_bytes());
        out.extend(1u32.to_le_bytes());
        out.extend(36u32.to_le_bytes());
        out.extend(b"SHEX");
        out.extend(u32::try_from(shdr.len()).unwrap().to_le_bytes());
        out.extend(shdr);
        out
    }

    #[test]
    fn discards_finds_the_instruction() {
        // dcl_immediateConstantBuffer (customdata, 3 dwords), mov (3), discard_nz (3), ret (1)
        let with = dxbc(&[0x35, 3, 7, 0x0300_0036, 0, 0, 0x0300_000d, 0, 0, 0x0100_003e]);
        assert!(discards_code(&with));
        let without = dxbc(&[0x35, 3, 0x0d, 0x0300_0036, 0, 0, 0x0100_003e]);
        assert!(!discards_code(&without));
        assert!(!discards_code(b"not dxbc"));
        assert!(!discards_code(&dxbc(&[0x0000_0036, 0x0300_000d])));
    }

    #[test]
    fn fragments_by_name_and_keys() {
        let (xbox, pc, emitter) = ([1u8; 16], [2u8; 16], [5u8; 16]);
        let w: HashMap<&[u8], &[u8]> = [
            (&xbox[..], &b"Systems/Overgrowth/LODFade"[..]),
            (&emitter[..], &b"E"[..]),
        ]
        .into_iter()
        .collect();
        let d: HashMap<&[u8], &[u8]> = [(&b"Systems/Overgrowth/LODFade"[..], &pc[..])].into_iter().collect();
        let key = |f: [u8; 16]| {
            let mut k = vec![0u8; 88];
            k[16..32].copy_from_slice(&f);
            k
        };
        assert_eq!(mesh_fragment(&w, &d, &key(xbox)), Ok(pc.to_vec()));
        assert_eq!(mesh_fragment(&w, &d, &key([0; 16])), Ok(vec![0; 16]));
        assert_eq!(
            mesh_fragment(&w, &d, &key(emitter)),
            Err(Fail::Translate("no PC renderer fragment for BF1943's E".into()))
        );
        assert_eq!(
            mesh_fragment(&w, &d, &key([9; 16])),
            Err(Fail::Translate(format!(
                "no PC renderer fragment for BF1943's {}",
                "09".repeat(16)
            )))
        );
        // the template's fragment is never kept; the layout and kind are the translation's
        let template = key([7; 16]);
        let made = program_key(&template, &[3; 16], 4, Some(&[0; 16])).unwrap();
        assert_eq!(&made[16..32], &[0; 16]);
        assert_eq!(&made[32..48], &[3; 16]);
        assert_eq!(made[0x33], 4);
        assert_eq!(program_key(&template[..0x30], &[3; 16], 4, None), None);
    }

    #[test]
    fn other_level_variants() {
        let tags: HashSet<Vec<u8>> = [b"iwo".to_vec(), b"guadal".to_vec()].into_iter().collect();
        let sib = vec![b"objects/x/rock_wake".to_vec(), b"objects/x/rock_iwo".to_vec()];
        assert!(other_level_variant(b"objects/x/Rock_Iwo", &sib, &tags));
        assert!(!other_level_variant(b"objects/x/rock_coral", &sib, &tags));
        assert!(!other_level_variant(b"_iwo", &sib, &tags));
    }

    fn db_with(entries: Vec<Named<'static>>, parts: Vec<Named<'static>>) -> ShaderDb<'static> {
        ShaderDb {
            entries,
            parts,
            ..ShaderDb::default()
        }
    }

    #[test]
    fn missing_entries_only_for_ported_meshes_with_parts() {
        let w = db_with(
            vec![
                named("Objects/T/T_Mesh/Objects/T/Bark", 1),
                named("Objects/T/T_Mesh/Objects/T/Glass", 2),
                named("Objects/U/U_Mesh/Objects/U/Bark", 3),
            ],
            vec![],
        );
        let db = db_with(
            vec![
                named("Objects/T/T_Mesh/Objects/T/Glass", 4),
                named("Objects/T/T_Mesh/Objects/T/Frond", 5),
            ],
            vec![
                named("Objects/T/T_Mesh_lod0/p/Standard", 6),
                named("Objects/U/U_Mesh_lod0/p/Standard", 6),
            ],
        );
        let ported: HashSet<Vec<u8>> = [b"objects/t/t_mesh".to_vec(), b"objects/u/u_mesh".to_vec()]
            .into_iter()
            .collect();
        let keep = vec![b"glass".to_vec()];
        let got = missing_entries(&w, &db, &[b"objects/".to_vec()], &[], &keep, &ported);
        assert_eq!(
            got,
            vec![(
                b"Objects/T/T_Mesh/Objects/T/Bark".to_vec(),
                b"Objects/T/T_Mesh/Objects/T/Frond".to_vec()
            )]
        );
    }

    #[test]
    fn soldier_entries_matched_by_beach_name() {
        let w = db_with(
            vec![
                named("Characters/Beach/3P/US/Mesh/Characters/Beach/3P/US/Body", 1),
                named("Characters/Beach/x/Eye", 2),
            ],
            vec![],
        );
        let db = db_with(
            vec![
                named("Characters/WW2/3P/US/Mesh/Characters/WW2/3P/US/Body", 3),
                named("Characters/WW2/x/Shaders/Eye", 4),
                named("Characters/WW2/x/Glass", 5),
                named("Objects/w/k98_mesh/Objects/w/k98", 6),
            ],
            vec![],
        );
        let weapons = vec![(b"objects/w/k98_mesh".to_vec(), b"characters/beach/x".to_vec())];
        let got = match_entries(&w, &db, &[b"glass".to_vec()], &weapons);
        assert_eq!(got.len(), 2);
        assert_eq!(got[1].1, b"Characters/Beach/x/Eye");
        assert_eq!(
            entry_mesh(b"Characters/WW2/3P/US/Mesh/Characters/WW2/3P/US/Body"),
            b"characters/ww2/3p/us/mesh"
        );
    }

    #[test]
    fn offer_kinds_copies_the_other_kind() {
        let g = [9u8; 16];
        let key = |pass: u8, kind: u8| {
            let mut k = vec![0u8; 88];
            k[0x30] = pass;
            k[0x33] = kind;
            k
        };
        let (k0, k1) = (key(0, 4), key(5, 4));
        let w = ShaderDb {
            entries: vec![Named {
                name: Cow::Borrowed(b"ours"),
                guid: &g,
            }],
            programs: vec![b"p0".as_slice(), b"p1"],
            keys: vec![&k0, &k1],
            groups: vec![(&g, vec![0, 1])],
            ..ShaderDb::default()
        };
        let entries = vec![(b"ours".to_vec(), b"ours".to_vec())];
        let need: HashMap<Vec<u8>, Vec<(u8, u8)>> = [(b"ours".to_vec(), vec![(0, 1), (0, 4)])].into_iter().collect();
        let single: HashMap<Vec<u8>, bool> = [(b"ours".to_vec(), true)].into_iter().collect();
        let got = offer_kinds(&w, &entries, &need, &single).unwrap();
        assert_eq!(got.added, vec![(0, key(0, 1))]);
        assert_eq!(got.groups, vec![(g.to_vec(), vec![0, 1, 2])]);
        assert!(got.bad.is_empty() && got.log.len() == 1);
        let single: HashMap<Vec<u8>, bool> = HashMap::new();
        let got = offer_kinds(&w, &entries, &need, &single).unwrap();
        assert_eq!(
            got.bad[0].1,
            "BF1943 has no kind-1 program for pass 0 and its parts are not all single-bone"
        );
    }

    #[test]
    fn soldier_layouts_by_their_parts() {
        let three = layout(
            &[
                (7, 1, 0, 0),
                (12, 2, 8, 0),
                (12, 3, 12, 0),
                (8, 0, 16, 0),
                (6, 0, 40, 0),
            ],
            64,
        );
        let one = layout(&[(7, 1, 0, 0), (12, 2, 8, 0), (12, 3, 12, 0), (6, 0, 40, 0)], 64);
        let zonly = layout(&[(8, 1, 0, 0), (12, 2, 8, 0), (12, 3, 12, 0)], 16);
        let recs: Vec<Vec<u8>> = [(b'3', &three), (b'1', &one), (b'z', &zonly)]
            .iter()
            .map(|(g, r)| [vec![*g; 16], (*r).clone()].concat())
            .collect();
        let p3 = "Characters/WW2/3P/Us/Mesh";
        let p1 = "Characters/WW2/1P/Us/Mesh";
        let mut db = ShaderDb {
            layouts: recs.iter().map(Vec::as_slice).collect(),
            parts: vec![
                named(&format!("{p3}_lod0/body/Standard"), b'3'),
                named(&format!("{p1}_lod0/arms/StreamInstancingFloat"), b'1'),
                named(&format!("{p3}_lod0/body_ZOnly/Standard"), b'z'),
                named("Characters/WW2/3P/Kitbags/Bag_lod0/b/Standard", b'1'),
                named("Objects/K98_lod0/gun/Standard", b'3'),
            ],
            ..ShaderDb::default()
        };
        let got = soldier_layouts(&db).unwrap();
        let mut kinds: Vec<bool> = got.values().copied().collect();
        kinds.sort_unstable();
        assert_eq!(kinds, vec![false, true]);
        db.parts.push(named(&format!("{p1}_lod1/arms/Standard"), b'3'));
        assert!(matches!(soldier_layouts(&db), Err(Fatal::Exit(m)) if m.contains("first- and third-person")));
    }
}
