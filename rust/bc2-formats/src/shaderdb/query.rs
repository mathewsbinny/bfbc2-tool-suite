//! The shader database's logic beyond reading and writing (round E4b): what a parameter block names, a program's
//! indices, the meshes the names name, the merge's needed-name test and the entries' coverage of a needs list - ports
//! of `bfbc2-shader-tools/shaderdb.py`'s `block_textures`, `block_constants`, `block_engine_textures`,
//! `ShaderDb.program_refs`, `texture_kind`, `part_mesh`, `ShaderDb.mesh_names`, `needed_test` and `coverage`,
//! giving the same answers (names are latin-1 bytes; Python's `str.lower` of a latin-1 string is [`lower`]).
//!
//! | Python (`shaderdb.py`) | here |
//! |---|---|
//! | `block_textures(block, e)` | [`block_textures`] |
//! | `block_constants(block, e)` | [`block_constants`] |
//! | `block_engine_textures(block, e)` | [`block_engine_textures`] |
//! | `ShaderDb.program_refs(i)` | [`program_refs`] |
//! | `texture_kind(name)` | [`texture_kind`] |
//! | `part_mesh(name)`, `ShaderDb.mesh_names()` | [`part_mesh`] |
//! | `needed_test(need)` | [`Needed`] |
//! | `coverage(db, meshes)` | [`Coverage`] |
//!
//! ```
//! use bc2_formats::shaderdb::query::{Needed, part_mesh, texture_kind};
//!
//! assert_eq!(part_mesh(b"Objects/Box/Box_Mesh_lod0/Part/Standard").as_deref(), Some(&b"objects/box/box_mesh"[..]));
//! assert_eq!(texture_kind(b"Textures/Box_Nm"), "normal");
//! let need = Needed::new([b"objects/box".as_slice()]);
//! assert!(need.test(b"Objects/Box/Box_Mesh/Shaders/Metal"));
//! assert!(!need.test(b"Objects/Boxes/Box_Mesh/Shaders/Metal"));
//! ```

use std::borrow::Cow;
use std::collections::{HashMap, HashSet};

/// A parameter block's header: size, five offsets (`u32`), then count bytes (FORMAT.md "Section 1").
pub const PARAM_HEADER: usize = 0x20;
/// A texture record: `u8` sampler, `u8` cube, two zero bytes, a 0x80-byte name, eight zero bytes.
pub const TEXTURE_RECORD: usize = 0x8c;
/// A texture name's zero-padded field.
pub const TEXTURE_FIELD: usize = 0x80;
/// A named constant: 0x20-byte name, `u32` hash, `u8` register, `u8` components, two zero bytes, four floats.
pub const CONSTANT_RECORD: usize = 0x38;
/// A texture the engine binds: 0x20-byte parameter name, `u32` hash, `u16` sampler, two zero bytes.
pub const ENGINE_TEXTURE_RECORD: usize = 0x28;

fn u32_at(b: &[u8], at: usize, be: bool) -> Option<u32> {
    let c = *b.get(at..at + 4)?.first_chunk::<4>()?;
    Some(if be {
        u32::from_be_bytes(c)
    } else {
        u32::from_le_bytes(c)
    })
}

fn u16_at(b: &[u8], at: usize, be: bool) -> Option<u16> {
    let c = *b.get(at..at + 2)?.first_chunk::<2>()?;
    Some(if be {
        u16::from_be_bytes(c)
    } else {
        u16::from_le_bytes(c)
    })
}

/// The bytes of a zero-padded field up to its first zero (`field.split(b'\0', 1)[0]`).
fn until_zero(field: &[u8]) -> &[u8] {
    field
        .iter()
        .position(|&c| c == 0)
        .map_or(field, |n| field.get(..n).unwrap_or_default())
}

/// The block's six header words, or `None` for a block shorter than its header.
fn header(block: &[u8], be: bool) -> Option<[u64; 6]> {
    if block.len() < PARAM_HEADER {
        return None;
    }
    let mut w = [0u64; 6];
    for (k, v) in w.iter_mut().enumerate() {
        *v = u64::from(u32_at(block, 4 * k, be)?);
    }
    Some(w)
}

/// A texture record of a parameter block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TextureRef<'a> {
    /// The shader's sampler register (s<N>).
    pub sampler: u8,
    /// 1 for a cube map, else 0.
    pub cube: u8,
    /// The texture's name (latin-1, up to its terminator).
    pub name: &'a [u8],
}

/// The texture records of a parameter block, in block order (`block_textures`); empty for a block not laid out so.
#[must_use]
pub fn block_textures(block: &[u8], be: bool) -> Vec<TextureRef<'_>> {
    let Some([size, _, _, tex_end, _, _]) = header(block, be) else {
        return Vec::new();
    };
    let n = block.get(0x1b).copied().map_or(0, usize::from);
    if size != block.len() as u64 || tex_end != (PARAM_HEADER + n * TEXTURE_RECORD) as u64 || tex_end > size {
        return Vec::new();
    }
    (0..n)
        .filter_map(|i| {
            let o = PARAM_HEADER + i * TEXTURE_RECORD;
            let rec = block.get(o..o + 4 + TEXTURE_FIELD)?;
            Some(TextureRef {
                sampler: *rec.first()?,
                cube: *rec.get(1)?,
                name: until_zero(rec.get(4..)?),
            })
        })
        .collect()
}

/// A named shader constant of a parameter block.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Constant<'a> {
    /// Its name.
    pub name: &'a [u8],
    /// Its register (c<N>).
    pub register: u8,
    /// How many of the four values are used.
    pub components: u8,
    /// The values the block sets.
    pub values: [f32; 4],
}

/// The named constants of a parameter block (`block_constants`).
#[must_use]
pub fn block_constants(block: &[u8], be: bool) -> Vec<Constant<'_>> {
    let Some([size, _, _, start, end, _]) = header(block, be) else {
        return Vec::new();
    };
    let n = block.get(0x1c).copied().map_or(0, u64::from);
    if size != block.len() as u64 || end != start + n * CONSTANT_RECORD as u64 || end > size {
        return Vec::new();
    }
    let f = |at: usize| u32_at(block, at, be).map(f32::from_bits);
    (0..n as usize)
        .filter_map(|i| {
            let o = start as usize + i * CONSTANT_RECORD;
            Some(Constant {
                name: until_zero(block.get(o..o + 0x20)?),
                register: *block.get(o + 0x24)?,
                components: *block.get(o + 0x25)?,
                values: [f(o + 0x28)?, f(o + 0x2c)?, f(o + 0x30)?, f(o + 0x34)?],
            })
        })
        .collect()
}

/// A program block's material registers `[first, end)` (header bytes 0x19 and 0x18, FORMAT.md "Section 1"): where
/// the shader reads the block's constants. On DICE's Direct3D 10 / 11 programs this is the shader's `constants` array
/// (the `$Globals` offset / 16 to its end); OPFOR's translated BF1943 water names the same registers `waterMaterial`.
/// `None` for a block shorter than its header or whose first register lies past its end.
#[must_use]
pub fn block_registers(block: &[u8]) -> Option<[u8; 2]> {
    if block.len() < PARAM_HEADER {
        return None;
    }
    let (end, first) = (*block.get(0x18)?, *block.get(0x19)?);
    (first <= end).then_some([first, end])
}

/// The textures the engine binds itself, `(parameter name, sampler)` (`block_engine_textures`).
#[must_use]
pub fn block_engine_textures(block: &[u8], be: bool) -> Vec<(&[u8], u16)> {
    let Some([size, _, _, _, start, end]) = header(block, be) else {
        return Vec::new();
    };
    let n = block.get(0x1d).copied().map_or(0, u64::from);
    if size != block.len() as u64 || end != start + n * ENGINE_TEXTURE_RECORD as u64 || end > size {
        return Vec::new();
    }
    (start as usize..end as usize)
        .step_by(ENGINE_TEXTURE_RECORD)
        .filter_map(|o| Some((until_zero(block.get(o..o + 0x20)?), u16_at(block, o + 0x24, be)?)))
        .collect()
}

/// A program record's (vertex shader, pixel shader, parameter block, parameter block) indices (`program_refs`), or
/// `None` for a record too short to hold them.
#[must_use]
pub fn program_refs(program: &[u8], be: bool) -> Option<[u32; 4]> {
    Some([
        u32_at(program, 0x20, be)?,
        u32_at(program, 0x24, be)?,
        u32_at(program, 0x28, be)?,
        u32_at(program, 0x2c, be)?,
    ])
}

/// A latin-1 character as Python's `str.lower` gives it: A-Z, and À-Þ but for ×, moved by 0x20.
#[must_use]
pub const fn lower_byte(c: u8) -> u8 {
    match c {
        b'A'..=b'Z' | 0xc0..=0xd6 | 0xd8..=0xde => c + 0x20,
        _ => c,
    }
}

/// A latin-1 name as Python's `str.lower` gives it.
#[must_use]
pub fn lower(name: &[u8]) -> Cow<'_, [u8]> {
    if name.iter().all(|&c| lower_byte(c) == c) {
        Cow::Borrowed(name)
    } else {
        Cow::Owned(name.iter().map(|&c| lower_byte(c)).collect())
    }
}

/// What a texture is for, guessed from its name's suffix (`texture_kind`): "normal", "specular", "gloss",
/// "occlusion", "color" or "".
#[must_use]
pub fn texture_kind(name: &[u8]) -> &'static str {
    const KINDS: [(&str, &[&[u8]]); 5] = [
        ("normal", &[b"_nm", b"_n", b"_normal", b"_norm"]),
        ("specular", &[b"_sp", b"_spec", b"_s"]),
        ("gloss", &[b"_gm", b"_g"]),
        ("occlusion", &[b"_occlusion", b"_oc", b"_ao"]),
        ("color", &[b"_c", b"_df", b"_d", b"_color", b"_diffuse", b"_cm"]),
    ];
    let low = lower(name);
    KINDS
        .iter()
        .find(|(_, ends)| ends.iter().any(|e| low.ends_with(e)))
        .map_or("", |(k, _)| k)
}

/// The mesh (lower case) a mesh-part name `<mesh>_lod<N>/<part>/<technique>` belongs to (`part_mesh`; the names
/// `ShaderDb.mesh_names` gives), or `None`. Python's `re.match(r'(.+)_lod\d+$', ..., re.I)`: `.` takes no line feed,
/// `$` matches before a final one.
#[must_use]
pub fn part_mesh(name: &[u8]) -> Option<Vec<u8>> {
    let mut slashes = name
        .iter()
        .enumerate()
        .filter(|(_, c)| **c == b'/')
        .map(|(i, _)| i)
        .rev();
    let (_, second) = (slashes.next()?, slashes.next()?);
    let lod = name.get(..second)?;
    let lod = lod.strip_suffix(b"\n").unwrap_or(lod);
    if lod.contains(&b'\n') {
        return None;
    }
    let digits = lod.iter().rev().take_while(|c| c.is_ascii_digit()).count();
    let rest = lod.get(..lod.len() - digits)?;
    if digits == 0 || rest.len() < 5 {
        return None;
    }
    let (mesh, tag) = rest.split_at(rest.len() - 4);
    tag.eq_ignore_ascii_case(b"_lod").then(|| lower(mesh).into_owned())
}

/// The prefixes of a name that end before one of its `/` (each, from the shortest), then the whole name: the paths
/// `needed` and `coverage` match a needed path against.
pub fn path_prefixes(name: &[u8]) -> impl Iterator<Item = &[u8]> {
    name.iter()
        .enumerate()
        .filter(|(_, c)| **c == b'/')
        .map(|(i, _)| i)
        .chain(std::iter::once(name.len()))
        .filter_map(|n| name.get(..n))
}

/// The merge's needed-name test (`needed_test`): does a name, lower-cased, equal a needed path or start with one
/// followed by `/`?
#[derive(Debug, Clone, Default)]
pub struct Needed {
    want: HashSet<Vec<u8>>,
}

impl Needed {
    /// The test for these needed paths (latin-1, as given: `needed_test` does not lower them).
    pub fn new<'a>(need: impl IntoIterator<Item = &'a [u8]>) -> Self {
        Self {
            want: need.into_iter().map(<[u8]>::to_vec).collect(),
        }
    }

    /// `needed_test(need)(name)`.
    #[must_use]
    pub fn test(&self, name: &[u8]) -> bool {
        if self.want.is_empty() {
            return false;
        }
        let low = lower(name);
        let mut prefixes = path_prefixes(&low);
        prefixes.any(|p| self.want.contains(p))
    }
}

/// `coverage(db, meshes)`: for each mesh (as given), the number of entries whose lower-cased name is the mesh or
/// starts with it followed by `/`.
#[derive(Debug, Clone, Default)]
pub struct Coverage {
    slot: HashMap<Vec<u8>, usize>,
    /// The count of each distinct mesh, in the order [`Coverage::new`] first saw it.
    pub counts: Vec<u32>,
}

impl Coverage {
    /// Counters for these meshes; a repeated mesh shares its first counter. Returns the counters and, for each mesh
    /// given, its counter's index.
    pub fn new<'a>(meshes: impl IntoIterator<Item = &'a [u8]>) -> (Self, Vec<usize>) {
        let mut c = Self::default();
        let mut at = Vec::new();
        for m in meshes {
            let next = c.slot.len();
            let k = *c.slot.entry(m.to_vec()).or_insert(next);
            if k == c.counts.len() {
                c.counts.push(0);
            }
            at.push(k);
        }
        (c, at)
    }

    /// Counts one entry name.
    pub fn add(&mut self, name: &[u8]) {
        self.add_lower(&lower(name));
    }

    /// Counts one entry name already lower-cased ([`lower`]).
    pub fn add_lower(&mut self, low: &[u8]) {
        for p in path_prefixes(low) {
            if let Some(&k) = self.slot.get(p)
                && let Some(n) = self.counts.get_mut(k)
            {
                *n += 1;
            }
        }
    }
}

/// The MD5 digest of `data` (RFC 1321): an entry's GUID is MD5 of its name.
#[must_use]
#[expect(clippy::many_single_char_names, reason = "RFC 1321's names")]
pub fn md5(data: &[u8]) -> [u8; 16] {
    const S: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14, 20, 5, 9, 14,
        20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6,
        10, 15, 21,
    ];
    let k: [u32; 64] = std::array::from_fn(|i| ((f64::from(i as u32) + 1.0).sin().abs() * 4_294_967_296.0) as u32);
    let mut msg = data.to_vec();
    msg.push(0x80);
    while msg.len() % 64 != 56 {
        msg.push(0);
    }
    msg.extend(((data.len() as u64).wrapping_mul(8)).to_le_bytes());
    let mut h: [u32; 4] = [0x6745_2301, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for chunk in msg.as_chunks::<64>().0 {
        let m: [u32; 16] = std::array::from_fn(|i| {
            let w = chunk
                .get(4 * i..4 * i + 4)
                .and_then(|s| s.first_chunk::<4>())
                .copied()
                .unwrap_or_default();
            u32::from_le_bytes(w)
        });
        let [mut a, mut b, mut c, mut d] = h;
        for (i, (&s, &ki)) in S.iter().zip(&k).enumerate() {
            let (f, g) = match i / 16 {
                0 => ((b & c) | (!b & d), i),
                1 => ((d & b) | (!d & c), (5 * i + 1) % 16),
                2 => (b ^ c ^ d, (3 * i + 5) % 16),
                _ => (c ^ (b | !d), (7 * i) % 16),
            };
            let f = f
                .wrapping_add(a)
                .wrapping_add(ki)
                .wrapping_add(m.get(g).copied().unwrap_or_default());
            a = d;
            d = c;
            c = b;
            b = b.wrapping_add(f.rotate_left(s));
        }
        for (x, y) in h.iter_mut().zip([a, b, c, d]) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = [0u8; 16];
    for (o, w) in out.as_chunks_mut::<4>().0.iter_mut().zip(h) {
        *o = w.to_le_bytes();
    }
    out
}

/// A GUID in the other byte order (`swap_guid`): its first three fields reversed, the rest kept - Python's slices
/// `g[3::-1] + g[5:3:-1] + g[7:5:-1] + g[8:]`, any length.
#[must_use]
pub fn swap_guid(g: &[u8]) -> Vec<u8> {
    let mut out: Vec<u8> = g.iter().take(4).rev().copied().collect();
    out.extend([5usize, 4, 7, 6].iter().filter_map(|&i| g.get(i)));
    out.extend(g.get(8..).unwrap_or_default());
    out
}

/// What [`check`] reads of a database: the records and names as they are, the groups' program indices as numbers.
#[derive(Debug, Default)]
pub struct CheckInput<'a> {
    /// Program and vertex layout GUIDs are kept in big-endian field order (Xbox 360): sorted by their PC order.
    pub big_endian: bool,
    /// The byte order the program records' indices are read in (`ShaderDb.e`).
    pub numbers_big_endian: bool,
    /// Section 1's count.
    pub params: usize,
    /// Section 4's count.
    pub vs: usize,
    /// Section 5's count.
    pub ps: usize,
    /// Section 6a.
    pub programs: Vec<&'a [u8]>,
    /// Section 6b.
    pub keys: Vec<&'a [u8]>,
    /// Section 7.
    pub layouts: Vec<&'a [u8]>,
    /// Section 8.1: (latin-1 name, GUID).
    pub entries: Vec<(&'a [u8], &'a [u8])>,
    /// Section 8.2's GUIDs.
    pub fragments: Vec<&'a [u8]>,
    /// Section 8.3's GUIDs.
    pub parts: Vec<&'a [u8]>,
    /// Section 8.4.
    pub groups: Vec<(&'a [u8], Vec<u32>)>,
}

/// Entry groups index programs with `u16`.
pub const MAX_PROGRAMS: usize = 0x10000;

/// A GUID in PC byte order (`ShaderDb.pc_guid`).
fn pc(be: bool, g: &[u8]) -> Cow<'_, [u8]> {
    if be { Cow::Owned(swap_guid(g)) } else { Cow::Borrowed(g) }
}

/// `k[a:b]` as Python slices it.
fn field(k: &[u8], a: usize, b: usize) -> &[u8] {
    k.get(a.min(k.len())..b.min(k.len())).unwrap_or_default()
}

/// `ShaderDb.check()`: FORMAT.md section 4's rules, the problems in the reference's words (empty: consistent); `None`
/// where the reference raises (a program record too short for its indices).
#[must_use]
#[expect(
    clippy::many_single_char_names,
    clippy::too_many_lines,
    reason = "the reference's names, its rules in its order"
)]
pub fn check(db: &CheckInput<'_>) -> Option<Vec<String>> {
    let mut p = Vec::new();
    let n = db.programs.len();
    let latin = |b: &[u8]| b.iter().map(|&c| char::from(c)).collect::<String>();
    if n >= MAX_PROGRAMS {
        p.push(format!("{n} programs: entry groups can only index {MAX_PROGRAMS}"));
    }
    if db.keys.len() != n {
        p.push(format!("{n} programs but {} keys", db.keys.len()));
    }
    let guids: Vec<&[u8]> = db.programs.iter().map(|r| r.get(..16).unwrap_or(r)).collect();
    let sorted_by_pc = |l: &[&[u8]]| {
        l.windows(2)
            .all(|w| matches!(w, [a, b] if pc(db.big_endian, a) <= pc(db.big_endian, b)))
    };
    if !sorted_by_pc(&guids) {
        p.push("programs not sorted by GUID".into());
    }
    let distinct = guids.iter().collect::<HashSet<_>>().len();
    if distinct != n {
        p.push(format!("{} program GUIDs repeated", n - distinct));
    }
    for (i, r) in db.programs.iter().enumerate() {
        let [v, f, a, b] = program_refs(r, db.numbers_big_endian)?.map(|x| x as usize);
        if v >= db.vs || f >= db.ps || a >= db.params || b >= db.params {
            p.push(format!(
                "program {i} names a shader or parameter block past the end of its section"
            ));
            break;
        }
    }
    let name_guid = |nm: &[u8]| {
        let g = md5(nm);
        if db.big_endian { swap_guid(&g) } else { g.to_vec() }
    };
    let bad: Vec<&[u8]> = db
        .entries
        .iter()
        .filter(|(nm, g)| name_guid(nm) != *g)
        .map(|(nm, _)| *nm)
        .collect();
    if let Some(first) = bad.first() {
        p.push(format!(
            "{} entry GUIDs are not MD5(name), e.g. {}",
            bad.len(),
            latin(first)
        ));
    }
    let names = db.entries.iter().map(|(nm, _)| *nm).collect::<HashSet<_>>().len();
    if names != db.entries.len() {
        p.push(format!("{} entry names repeated", db.entries.len() - names));
    }
    let entry_g: HashSet<&[u8]> = db.entries.iter().map(|(_, g)| *g).collect();
    let frag_g: HashSet<&[u8]> = db.fragments.iter().copied().collect();
    let lay_g: HashSet<&[u8]> = db.layouts.iter().map(|r| r.get(..16).unwrap_or(r)).collect();
    if lay_g.len() != db.layouts.len() {
        p.push("vertex layout GUIDs repeated".into());
    }
    let group_g: Vec<&[u8]> = db.groups.iter().map(|(g, _)| *g).collect();
    if !sorted_by_pc(&group_g) {
        p.push("entry groups not sorted by GUID".into());
    }
    let group_set: HashSet<&[u8]> = group_g.iter().copied().collect();
    if group_set != entry_g || group_g.len() != entry_g.len() {
        p.push(format!(
            "entry groups ({}) do not match the entries ({}) one to one",
            group_g.len(),
            entry_g.len()
        ));
    }
    let mut owner: HashMap<u32, &[u8]> = HashMap::new();
    for (g, idx) in &db.groups {
        if !idx.is_sorted() {
            p.push("an entry group's program list is not sorted".into());
            break;
        }
        for &i in idx {
            if i as usize >= n {
                p.push(format!("an entry group names program {i} of {n}"));
                break;
            }
            if owner.contains_key(&i) {
                p.push(format!("program {i} in two entry groups"));
            }
            owner.insert(i, g);
        }
    }
    if owner.len() != n {
        p.push(format!("{} programs in no entry group", n as i64 - owner.len() as i64));
    }
    for (i, k) in db.keys.iter().enumerate() {
        let own = k.get(..16).unwrap_or(k);
        if owner.get(&(i as u32)).is_some_and(|g| *g != own) {
            p.push(format!("program {i}'s key names another entry than its group"));
            break;
        }
    }
    let zero = [0u8; 16];
    let missing_f: HashSet<&[u8]> = db
        .keys
        .iter()
        .map(|k| field(k, 16, 32))
        .filter(|f| !frag_g.contains(f) && *f != zero)
        .collect();
    if !missing_f.is_empty() {
        p.push(format!(
            "{} fragment GUIDs used by program keys have no fragment name",
            missing_f.len()
        ));
    }
    let missing_l: HashSet<&[u8]> = db
        .keys
        .iter()
        .map(|k| field(k, 32, 48))
        .chain(db.parts.iter().copied())
        .filter(|l| !lay_g.contains(l))
        .collect();
    if !missing_l.is_empty() {
        p.push(format!(
            "{} vertex layout GUIDs used by keys or mesh parts are not in section 7",
            missing_l.len()
        ));
    }
    Some(p)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn md5_vectors() {
        let hex = |d: [u8; 16]| {
            d.iter().fold(String::new(), |mut s, b| {
                use std::fmt::Write;
                let _ = write!(s, "{b:02x}");
                s
            })
        };
        assert_eq!(hex(md5(b"")), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(hex(md5(b"abc")), "900150983cd24fb0d6963f7d28e17f72");
        let long = b"12345678901234567890123456789012345678901234567890123456789012345678901234567890";
        assert_eq!(hex(md5(long)), "57edf4a22be3c955ac49da2e2107b67a");
        assert_eq!(
            swap_guid(&[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]),
            vec![3, 2, 1, 0, 5, 4, 7, 6, 8, 9]
        );
        assert_eq!(swap_guid(&[0, 1, 2, 3, 4]), vec![3, 2, 1, 0, 4]);
    }

    /// A parameter block laid out as FORMAT.md section 1 says: textures, then constants, then engine textures.
    fn block(
        be: bool,
        textures: &[(u8, u8, &[u8])],
        constants: &[(&[u8], u8, u8, [f32; 4])],
        engine: &[(&[u8], u16)],
    ) -> Vec<u8> {
        let w32 = |v: u32| if be { v.to_be_bytes() } else { v.to_le_bytes() };
        let tex_end = PARAM_HEADER + textures.len() * TEXTURE_RECORD;
        let c_end = tex_end + constants.len() * CONSTANT_RECORD;
        let e_end = c_end + engine.len() * ENGINE_TEXTURE_RECORD;
        let mut b = Vec::new();
        for v in [e_end, 0, 0, tex_end, c_end, e_end] {
            b.extend(w32(v as u32));
        }
        b.extend([
            0,
            0,
            0,
            textures.len() as u8,
            constants.len() as u8,
            engine.len() as u8,
            0,
            0,
        ]);
        for (s, c, n) in textures {
            b.extend([*s, *c, 0, 0]);
            let mut f = n.to_vec();
            f.resize(TEXTURE_FIELD + 8, 0);
            b.extend(f);
        }
        for (n, r, k, v) in constants {
            let mut f = n.to_vec();
            f.resize(0x20, 0);
            b.extend(f);
            b.extend([1, 2, 3, 4, *r, *k, 0, 0]);
            for x in v {
                b.extend(w32(x.to_bits()));
            }
        }
        for (n, s) in engine {
            let mut f = n.to_vec();
            f.resize(0x20, 0);
            b.extend(f);
            b.extend([9, 9, 9, 9]);
            b.extend(if be { s.to_be_bytes() } else { s.to_le_bytes() });
            b.extend([0, 0]);
        }
        b
    }

    #[test]
    fn blocks() {
        for be in [false, true] {
            let b = block(
                be,
                &[(0, 0, b"Textures/A_C"), (3, 1, b"Sky\xe9")],
                &[(b"tint", 4, 3, [1.0, -2.5, 0.0, f32::MAX])],
                &[(b"heightmap", 7)],
            );
            let t = block_textures(&b, be);
            assert_eq!(t.len(), 2);
            assert_eq!((t[1].sampler, t[1].cube, t[1].name), (3, 1, &b"Sky\xe9"[..]));
            let c = block_constants(&b, be);
            assert_eq!((c[0].name, c[0].register, c[0].components), (&b"tint"[..], 4, 3));
            assert_eq!(c[0].values, [1.0, -2.5, 0.0, f32::MAX]);
            assert_eq!(block_engine_textures(&b, be), vec![(&b"heightmap"[..], 7)]);
            // the other byte order, a cut block, a short one: nothing
            assert_eq!(block_textures(&b, !be), Vec::new());
            assert_eq!(block_textures(&b[..b.len() - 1], be), Vec::new());
            assert_eq!(block_constants(&b[..10], be), Vec::new());
        }
    }

    #[test]
    fn material_registers() {
        let mut b = block(false, &[], &[(b"g_depthScaleFactors", 13, 4, [0.0; 4])], &[]);
        // Wake's water program: $Globals 14 registers, its material from register 10
        (b[0x18], b[0x19]) = (14, 10);
        assert_eq!(block_registers(&b), Some([10, 14]));
        (b[0x18], b[0x19]) = (8, 8);
        assert_eq!(block_registers(&b), Some([8, 8]));
        (b[0x18], b[0x19]) = (3, 5);
        assert_eq!(block_registers(&b), None);
        assert_eq!(block_registers(&b[..0x1f]), None);
    }

    #[test]
    fn program_indices() {
        let mut p = vec![0u8; 0x30];
        p[0x20] = 5;
        p[0x2f] = 1;
        assert_eq!(program_refs(&p, false), Some([5, 0, 0, 1 << 24]));
        assert_eq!(program_refs(&p[..0x2f], false), None);
    }

    #[test]
    fn lowering() {
        assert_eq!(&*lower(b"AbC\xc0\xd7\xde\xdf\xff"), b"abc\xe0\xd7\xfe\xdf\xff");
        assert!(matches!(lower(b"abc"), Cow::Borrowed(_)));
    }

    #[test]
    fn kinds() {
        assert_eq!(texture_kind(b"X_NM"), "normal");
        assert_eq!(texture_kind(b"X_s"), "specular");
        assert_eq!(texture_kind(b"X_Diffuse"), "color");
        assert_eq!(texture_kind(b"X_AO"), "occlusion");
        assert_eq!(texture_kind(b"X"), "");
    }

    #[test]
    fn part_meshes() {
        let p = |n: &[u8]| part_mesh(n);
        assert_eq!(p(b"A/B_LOD12/P/T").as_deref(), Some(&b"a/b"[..]));
        assert_eq!(p(b"A_lod1_lod2/P/T").as_deref(), Some(&b"a_lod1"[..]));
        assert_eq!(p(b"_lod0/P/T"), None);
        assert_eq!(p(b"x_lod/P/T"), None);
        assert_eq!(p(b"x_lod0/T"), None);
        assert_eq!(p(b"x_lod0\n/P/T").as_deref(), Some(&b"x"[..]));
        assert_eq!(p(b"x\n_lod0/P/T"), None);
    }

    #[test]
    fn needed_and_coverage() {
        let n = Needed::new([b"a/b".as_slice(), b"".as_slice()]);
        assert!(n.test(b"A/B"));
        assert!(n.test(b"A/B/c"));
        assert!(!n.test(b"A/Bc"));
        assert!(n.test(b"/x"));
        assert!(!Needed::new([]).test(b""));
        let (mut c, at) = Coverage::new([b"a".as_slice(), b"a/b".as_slice(), b"a".as_slice()]);
        assert_eq!(at, vec![0, 1, 0]);
        for name in [&b"A/B/c"[..], b"a", b"ab"] {
            c.add(name);
        }
        assert_eq!(c.counts, vec![2, 1]);
    }
}
