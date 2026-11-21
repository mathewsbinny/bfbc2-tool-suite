//! A 32-bit Windows executable as the crash reader and exeref see it: pefile's `PE(path, fast_load=True)` and its
//! `get_memory_mapped_image()` (the headers and each section's raw data at its virtual address), the image base and
//! the sections (name, virtual address, virtual size, characteristics). Moved from `bc2-harness` (round QW2, audit AU2
//! A5): a file format, which the menu tools read too.
//!
//! The mapping follows pefile 2024.8.26's rules for well-formed images and refuses ([`PeError::Refused`]) where
//! pefile adjusts or warns (a section alignment under a page, raw data past the file, sections out of order ...):
//! the caller then reads the file with pefile, the reference, which also raises pefile's own errors.

use std::fmt;

/// Why an image is not read here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PeError {
    /// Not a PE32 image this reader maps the way pefile does (the reason): pefile decides.
    Refused(&'static str),
}

impl fmt::Display for PeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused(why) => write!(f, "not mapped here: {why}"),
        }
    }
}

impl std::error::Error for PeError {}

/// A section header's fields the tools read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Section {
    /// The name, NULs stripped from the end, as Latin-1.
    pub name: String,
    /// The virtual address (relative to the image base).
    pub virtual_address: u32,
    /// `Misc_VirtualSize`.
    pub virtual_size: u32,
    /// `SizeOfRawData`.
    pub raw_size: u32,
    /// `PointerToRawData`.
    pub raw_pointer: u32,
    /// `Characteristics`.
    pub characteristics: u32,
}

/// The mapped image.
#[derive(Debug, Clone)]
pub struct Image {
    /// `OPTIONAL_HEADER.ImageBase`.
    pub base: u32,
    /// `get_memory_mapped_image()`.
    pub data: Vec<u8>,
    /// The sections in file order.
    pub sections: Vec<Section>,
}

/// `IMAGE_SCN_MEM_EXECUTE`.
pub const EXECUTE: u32 = 0x2000_0000;

fn u16_at(d: &[u8], o: usize) -> Result<u16, PeError> {
    d.get(o..o + 2)
        .and_then(|b| <[u8; 2]>::try_from(b).ok())
        .map(u16::from_le_bytes)
        .ok_or(PeError::Refused("truncated header"))
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, PeError> {
    d.get(o..o + 4)
        .and_then(|b| <[u8; 4]>::try_from(b).ok())
        .map(u32::from_le_bytes)
        .ok_or(PeError::Refused("truncated header"))
}

impl Image {
    /// Maps a file's bytes (see the module's text).
    ///
    /// # Errors
    /// [`PeError::Refused`] where pefile's rules are not followed here.
    pub fn parse(file: &[u8]) -> Result<Self, PeError> {
        if file.get(0..2) != Some(b"MZ") {
            return Err(PeError::Refused("no MZ header"));
        }
        let pe = u32_at(file, 0x3C)? as usize;
        if file.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(PeError::Refused("no PE signature"));
        }
        let fh = pe + 4;
        let nsec = usize::from(u16_at(file, fh + 2)?);
        let opt_size = usize::from(u16_at(file, fh + 16)?);
        let opt = fh + 20;
        if u16_at(file, opt)? != 0x10B {
            return Err(PeError::Refused("not a PE32 optional header"));
        }
        if opt_size < 0xE0 {
            return Err(PeError::Refused("a short optional header"));
        }
        let base = u32_at(file, opt + 28)?;
        let section_alignment = u32_at(file, opt + 32)?;
        let file_alignment = u32_at(file, opt + 36)?;
        let ndirs = u32_at(file, opt + 92)?;
        if section_alignment < 0x1000 || !file_alignment.is_power_of_two() || file_alignment < 0x200 {
            return Err(PeError::Refused("alignments pefile adjusts"));
        }
        if ndirs != 16 {
            return Err(PeError::Refused("not 16 data directories"));
        }
        if nsec == 0 || nsec > 96 {
            return Err(PeError::Refused("a section count pefile warns about"));
        }
        let table = opt + opt_size;
        let table_end = table + 40 * nsec;
        if table_end > file.len() {
            return Err(PeError::Refused("the section table past the file"));
        }
        let mut sections = Vec::with_capacity(nsec);
        for k in 0..nsec {
            let s = table + 40 * k;
            let raw_name = file.get(s..s + 8).ok_or(PeError::Refused("truncated header"))?;
            let end = raw_name.iter().rposition(|&b| b != 0).map_or(0, |p| p + 1);
            let name: String = raw_name
                .get(..end)
                .unwrap_or_default()
                .iter()
                .map(|&b| char::from(b))
                .collect();
            sections.push(Section {
                name,
                virtual_size: u32_at(file, s + 8)?,
                virtual_address: u32_at(file, s + 12)?,
                raw_size: u32_at(file, s + 16)?,
                raw_pointer: u32_at(file, s + 20)?,
                characteristics: u32_at(file, s + 36)?,
            });
        }
        let mut prev_end = 0u64;
        for s in &sections {
            let ptr = u64::from(s.raw_pointer);
            let size = u64::from(s.raw_size);
            if s.virtual_address % section_alignment != 0
                || s.raw_pointer % 0x200 != 0
                || ptr + size > file.len() as u64
                || u64::from(s.virtual_address) < prev_end
                || s.virtual_address >= 0x1000_0000
                || (s.raw_size == 0 && s.virtual_size == 0)
                || (s.raw_pointer == 0 && s.raw_size != 0)
                || u64::from(s.virtual_address) + size.max(u64::from(s.virtual_size)) > 0x1000_0000
            {
                return Err(PeError::Refused("a section pefile adjusts or skips"));
            }
            // the next section must start after this one's raw data (pefile cuts the mapped data back otherwise)
            prev_end = u64::from(s.virtual_address) + size;
        }
        let lowest = sections
            .iter()
            .filter(|s| s.raw_pointer > 0)
            .map(|s| s.raw_pointer as usize)
            .min();
        let header_end = match lowest {
            Some(l) if l >= table_end => l,
            _ => table_end,
        };
        let mut data = file
            .get(..header_end)
            .ok_or(PeError::Refused("truncated header"))?
            .to_vec();
        for s in &sections {
            let va = s.virtual_address as usize;
            if va < data.len() {
                return Err(PeError::Refused("a section over the headers"));
            }
            data.resize(va, 0);
            let p = s.raw_pointer as usize;
            data.extend_from_slice(
                file.get(p..p + s.raw_size as usize)
                    .ok_or(PeError::Refused("raw data"))?,
            );
        }
        Ok(Self { base, data, sections })
    }

    /// The code ranges (absolute): sections with `IMAGE_SCN_MEM_EXECUTE`, `[va, va + virtual size)`.
    #[must_use]
    pub fn code_ranges(&self) -> Vec<(u32, u32)> {
        self.sections
            .iter()
            .filter(|s| s.characteristics & EXECUTE != 0)
            .map(|s| {
                let lo = self.base.wrapping_add(s.virtual_address);
                (lo, lo.wrapping_add(s.virtual_size))
            })
            .collect()
    }

    /// The data ranges exeref searches: sections without execute, except `.rsrc` and `.reloc`.
    #[must_use]
    pub fn data_ranges(&self) -> Vec<(u32, u32)> {
        self.sections
            .iter()
            .filter(|s| s.characteristics & EXECUTE == 0 && s.name != ".rsrc" && s.name != ".reloc")
            .map(|s| {
                let lo = self.base.wrapping_add(s.virtual_address);
                (lo, lo.wrapping_add(s.virtual_size))
            })
            .collect()
    }

    /// The end of the mapped image (absolute).
    #[must_use]
    pub fn end(&self) -> u64 {
        u64::from(self.base) + self.data.len() as u64
    }
}

/// The menu tools' view of an executable (`bfbc2-menu-tools/menucommon.py`'s `PEImage`, round F2b): the section
/// table read straight from the file (no mapping), the data sections searched by file offset.
pub mod raw {
    /// Why a file is not read.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum RawError {
        /// No `MZ` (or shorter than 0x40 bytes): the reference's "is not a Windows program".
        NotExe,
        /// No `PE` signature or not PE32: "is not a 32-bit Windows program".
        Not32,
        /// The reference raises another exception (a header past the file).
        Refused,
    }

    /// A section as `PEImage.sections` holds it: (name, va - the base added -, virtual size, file offset, raw size,
    /// flags).
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Section {
        /// The name, NULs stripped from the end, as Latin-1.
        pub name: String,
        /// The image base plus the virtual address.
        pub va: u64,
        /// The virtual size.
        pub vsize: u32,
        /// The file offset.
        pub raw: u32,
        /// The raw size.
        pub rsize: u32,
        /// The characteristics.
        pub flags: u32,
    }

    /// The header fields `PEImage` keeps.
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Header {
        /// `TimeDateStamp`.
        pub timestamp: u32,
        /// `ImageBase`.
        pub base: u32,
        /// The sections in file order.
        pub sections: Vec<Section>,
    }

    fn u16_at(b: &[u8], o: usize) -> Result<u16, RawError> {
        let s: [u8; 2] = b
            .get(o..o + 2)
            .ok_or(RawError::Refused)?
            .try_into()
            .map_err(|_| RawError::Refused)?;
        Ok(u16::from_le_bytes(s))
    }

    fn u32_at(b: &[u8], o: usize) -> Result<u32, RawError> {
        let s: [u8; 4] = b
            .get(o..o + 4)
            .ok_or(RawError::Refused)?
            .try_into()
            .map_err(|_| RawError::Refused)?;
        Ok(u32::from_le_bytes(s))
    }

    /// `PEImage.__init__`'s reading.
    ///
    /// # Errors
    /// [`RawError`].
    pub fn parse(b: &[u8]) -> Result<Header, RawError> {
        if b.get(..2) != Some(b"MZ") || b.len() < 0x40 {
            return Err(RawError::NotExe);
        }
        let pe = u32_at(b, 0x3C)? as usize;
        if b.get(pe..pe + 4) != Some(b"PE\0\0") {
            return Err(RawError::Not32);
        }
        if u16_at(b, pe + 24)? != 0x10B {
            return Err(RawError::Not32);
        }
        let n = usize::from(u16_at(b, pe + 6)?);
        let opt = usize::from(u16_at(b, pe + 20)?);
        let timestamp = u32_at(b, pe + 8)?;
        let base = u32_at(b, pe + 52)?;
        let mut sections = Vec::with_capacity(n);
        for i in 0..n {
            let o = pe + 24 + opt + 40 * i;
            let raw_name = b.get(o..(o + 8).min(b.len())).unwrap_or_default();
            let end = raw_name.iter().rposition(|&c| c != 0).map_or(0, |p| p + 1);
            let name = raw_name
                .get(..end)
                .unwrap_or_default()
                .iter()
                .map(|&c| char::from(c))
                .collect();
            let (vs, va, rs, ra) = (
                u32_at(b, o + 8)?,
                u32_at(b, o + 12)?,
                u32_at(b, o + 16)?,
                u32_at(b, o + 20)?,
            );
            sections.push(Section {
                name,
                va: u64::from(base) + u64::from(va),
                vsize: vs,
                raw: ra,
                rsize: rs,
                flags: u32_at(b, o + 36)?,
            });
        }
        Ok(Header {
            timestamp,
            base,
            sections,
        })
    }

    impl Header {
        /// `data_sections()`: neither code nor `.rsrc` / `.tls`.
        #[must_use]
        pub fn data_sections(&self) -> Vec<&Section> {
            self.sections
                .iter()
                .filter(|s| s.flags & super::EXECUTE == 0 && s.name != ".rsrc" && s.name != ".tls")
                .collect()
        }
    }

    /// `bytes.find`'s range: `[start, end)` clamped to the data.
    fn span(len: usize, start: u32, size: u32) -> (usize, usize) {
        let a = (start as usize).min(len);
        let e = (start as usize).saturating_add(size as usize).min(len);
        (a, e.max(a))
    }

    fn printable(c: u8) -> bool {
        (0x20..=0x7E).contains(&c)
    }

    /// `strings(minimum)`: the printable C strings of the data sections, `{text: [va, ...]}` in the reference's
    /// insertion order (a string starts after a 00 byte or, inside a longer printable run, at a 4-byte aligned
    /// offset).
    #[must_use]
    pub fn strings(b: &[u8], h: &Header, minimum: usize) -> Vec<(String, Vec<u64>)> {
        let mut order: Vec<(String, Vec<u64>)> = Vec::new();
        let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
        for s in h.data_sections() {
            let (pos, end) = span(b.len(), s.raw, s.rsize);
            let mut i = pos;
            while i < end {
                if !b.get(i).copied().is_some_and(printable) {
                    i += 1;
                    continue;
                }
                let st = i;
                while i < end && b.get(i).copied().is_some_and(printable) {
                    i += 1;
                }
                // the match: a run of at least `minimum`, a NUL right after it inside the range
                if i - st < minimum.max(1) || i >= end || b.get(i) != Some(&0) {
                    continue;
                }
                let mut starts = vec![st];
                let mut o = (st + 4) & !3;
                while o + minimum <= i {
                    if o != st {
                        starts.push(o);
                    }
                    o += 4;
                }
                for o in starts {
                    let text: String = b.get(o..i).unwrap_or_default().iter().map(|&c| char::from(c)).collect();
                    let va = s.va + (o - s.raw as usize) as u64;
                    if let Some(&k) = index.get(&text) {
                        if let Some(e) = order.get_mut(k) {
                            e.1.push(va);
                        }
                    } else {
                        index.insert(text.clone(), order.len());
                        order.push((text, vec![va]));
                    }
                }
            }
        }
        order
    }

    /// Every start of `pat` in `b[start..end]`, overlapping ones too (the reference's `find` from `j + 1`).
    fn finds(b: &[u8], start: usize, end: usize, pat: &[u8]) -> Vec<usize> {
        let mut out = Vec::new();
        if pat.is_empty() || end < start + pat.len() {
            return out;
        }
        let hay = b.get(start..end).unwrap_or_default();
        let mut from = 0usize;
        while let Some(p) = memchr::memmem::find(hay.get(from..).unwrap_or_default(), pat) {
            out.push(start + from + p);
            from += p + 1;
        }
        out
    }

    /// `find_cstring(text)`: the virtual addresses of the whole C string `text` in the data sections (ASCII text).
    #[must_use]
    pub fn find_cstring(b: &[u8], h: &Header, text: &[u8]) -> Vec<u64> {
        let mut pat = vec![0u8];
        pat.extend_from_slice(text);
        pat.push(0);
        let mut out = Vec::new();
        for s in h.data_sections() {
            let (a, e) = span(b.len(), s.raw, s.rsize);
            for j in finds(b, a, e, &pat) {
                out.push(s.va + (j + 1 - s.raw as usize) as u64);
            }
        }
        out
    }

    /// `find_dword(value, sections)`: file offsets in the sections (`(file offset, raw size)`) where the
    /// little-endian value is stored, 4-byte aligned in its section.
    #[must_use]
    pub fn find_dword(b: &[u8], sections: &[(u32, u32)], value: u32) -> Vec<usize> {
        let pat = value.to_le_bytes();
        let mut out = Vec::new();
        for &(ra, rs) in sections {
            let (a, e) = span(b.len(), ra, rs);
            out.extend(
                finds(b, a, e, &pat)
                    .into_iter()
                    .filter(|j| (j - ra as usize).is_multiple_of(4)),
            );
        }
        out
    }

    #[cfg(test)]
    #[allow(clippy::indexing_slicing)]
    mod tests {
        use super::*;

        #[test]
        fn header_errors() {
            assert_eq!(parse(b"XX"), Err(RawError::NotExe));
            let mut b = vec![0u8; 0x80];
            b[0] = b'M';
            b[1] = b'Z';
            assert_eq!(parse(&b), Err(RawError::Not32));
        }

        #[test]
        fn string_runs() {
            let h = Header {
                timestamp: 0,
                base: 0x40_0000,
                sections: vec![Section {
                    name: ".rdata".into(),
                    va: 0x40_1000,
                    vsize: 32,
                    raw: 0,
                    rsize: 32,
                    flags: 0,
                }],
            };
            let mut b = vec![0u8; 32];
            b[1..12].copy_from_slice(b"abcdefghijk");
            b[20..22].copy_from_slice(b"xy");
            let got = strings(&b, &h, 3);
            assert_eq!(got.first().map(|x| x.0.as_str()), Some("abcdefghijk"));
            assert_eq!(got.len(), 3); // at 1, 4 and 8
            assert_eq!(find_cstring(&b, &h, b"xy"), vec![0x40_1000 + 20]);
            assert_eq!(find_dword(&b, &[(0, 32)], 0), vec![12, 16, 24, 28]);
        }
    }
}
