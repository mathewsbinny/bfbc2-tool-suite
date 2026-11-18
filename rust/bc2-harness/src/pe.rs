//! A 32-bit Windows executable as the crash reader and exeref see it: pefile's `PE(path, fast_load=True)` and its
//! `get_memory_mapped_image()` (the headers and each section's raw data at its virtual address), the image base and
//! the sections (name, virtual address, virtual size, characteristics).
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
