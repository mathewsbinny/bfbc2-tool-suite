//! Entry data: the payload read as `fbrb.read` streams it, and whole bundles.

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use super::error::{FbrbError, Result};
use super::gzip::Member;
use super::header::{Entry, Header, read_header};

/// The payload is read (and, zipped, decompressed) this many bytes at a time, as the reference does: a damaged
/// stream is noticed at the same point.
pub const CHUNK: usize = 1 << 20;

/// Fills `buf` from `r` as far as it can (Python's buffered `read(n)`: short only at the end of the file).
///
/// # Errors
/// The reader's error (but `Interrupted`, which is retried).
pub fn read_full<R: Read>(r: &mut R, buf: &mut [u8]) -> Result<usize> {
    let mut n = 0;
    while n < buf.len() {
        let Some(rest) = buf.get_mut(n..) else { break };
        match r.read(rest) {
            Ok(0) => break,
            Ok(k) => n += k,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(n)
}

/// Collects the bytes of some entries from the payload as it streams past.
pub struct Collector<'e> {
    /// The entries, sorted by offset, each with the index of its output.
    order: Vec<(&'e Entry, usize)>,
    out: Vec<Vec<u8>>,
    /// Every entry before this one has all of its bytes (with overlapping entries a later one may finish first).
    first_open: usize,
    pos: u64,
}

impl<'e> Collector<'e> {
    /// A collector of `entries` (their bytes in this order).
    #[must_use]
    pub fn new(entries: &[&'e Entry]) -> Self {
        let mut order: Vec<(&Entry, usize)> = entries.iter().copied().zip(0..).collect();
        order.sort_by_key(|(e, _)| e.offset);
        Self {
            order,
            out: entries.iter().map(|e| Vec::with_capacity(e.size as usize)).collect(),
            first_open: 0,
            pos: 0,
        }
    }

    /// The bytes of the payload at the current position.
    pub fn take(&mut self, data: &[u8]) {
        let (lo, hi) = (self.pos, self.pos + data.len() as u64);
        while self.order.get(self.first_open).is_some_and(|(e, _)| e.end() <= lo) {
            self.first_open += 1;
        }
        for (e, i) in self.order.iter().skip(self.first_open) {
            if u64::from(e.offset) >= hi {
                break;
            }
            let (a, b) = (u64::from(e.offset).max(lo), e.end().min(hi));
            if a < b
                && let (Some(src), Some(dst)) = (data.get((a - lo) as usize..(b - lo) as usize), self.out.get_mut(*i))
            {
                dst.extend_from_slice(src);
            }
        }
        self.pos = hi;
    }

    /// Each entry's bytes.
    ///
    /// # Errors
    /// [`FbrbError::PayloadEnds`] when the payload ended before an entry's last byte.
    pub fn finish(self) -> Result<Vec<Vec<u8>>> {
        let complete = self
            .order
            .iter()
            .all(|(e, i)| self.out.get(*i).is_some_and(|o| o.len() as u64 == u64::from(e.size)));
        if complete {
            Ok(self.out)
        } else {
            Err(FbrbError::PayloadEnds)
        }
    }
}

/// The data of `entries` from a stored (unzipped) payload: each entry read where it is, in payload order (the
/// reference streams the payload from its start; the bytes, and [`FbrbError::PayloadEnds`] for an entry the file
/// ends before, are the same).
fn read_stored<R: Read + Seek>(r: &mut R, header: &Header, entries: &[&Entry]) -> Result<Vec<Vec<u8>>> {
    let mut order: Vec<(&Entry, usize)> = entries.iter().copied().zip(0..).collect();
    order.sort_by_key(|(e, _)| e.offset);
    let mut out: Vec<Vec<u8>> = vec![Vec::new(); entries.len()];
    // an entry past the end of the file is refused before its room is taken (a damaged size may say 4 GB)
    let file_end = r.seek(SeekFrom::End(0))?;
    let mut at = None;
    let mut short = false;
    for (e, i) in order {
        if e.size == 0 {
            continue; // an empty entry is whole wherever it is
        }
        let start = header.payload_start + u64::from(e.offset);
        if start + u64::from(e.size) > file_end {
            short = true;
            continue;
        }
        if at != Some(start) {
            r.seek(SeekFrom::Start(start))?;
        }
        let mut b = vec![0; e.size as usize];
        let n = read_full(r, &mut b)?;
        at = Some(start + n as u64);
        short |= n < b.len();
        if let Some(o) = out.get_mut(i) {
            *o = b;
        }
    }
    if short { Err(FbrbError::PayloadEnds) } else { Ok(out) }
}

/// The data of `entries` from the payload of the bundle `r` holds (`header` read from it). A zipped payload is
/// decompressed up to the last byte any of them needs, 1 MiB of compressed bytes at a time, as the reference does
/// (so a damaged stream is refused at the same point); a stored one is read entry by entry where they are (round
/// E3e; [`stream_payload`] reads it from the start as the reference does, the same bytes).
///
/// # Errors
/// [`FbrbError::PayloadEnds`] when the payload ends before an entry's data, a gzip error for a damaged zipped
/// payload, [`FbrbError::Io`].
pub fn read_payload<R: Read + Seek>(r: &mut R, header: &Header, entries: &[&Entry]) -> Result<Vec<Vec<u8>>> {
    let end = entries.iter().map(|e| e.end()).max().unwrap_or(0);
    if header.zipped || end == 0 {
        stream_payload(r, header, entries)
    } else {
        read_stored(r, header, entries)
    }
}

/// [`read_payload`] exactly as the reference reads: the payload streamed from its start, zipped or stored, up to
/// the last byte the entries need, 1 MiB at a time (kept as the stored path's reference).
///
/// # Errors
/// As [`read_payload`].
pub fn stream_payload<R: Read + Seek>(r: &mut R, header: &Header, entries: &[&Entry]) -> Result<Vec<Vec<u8>>> {
    let end = entries.iter().map(|e| e.end()).max().unwrap_or(0);
    let mut c = Collector::new(entries);
    if end == 0 {
        return c.finish();
    }
    r.seek(SeekFrom::Start(header.payload_start))?;
    let mut member = header.zipped.then(Member::new);
    let mut buf = vec![0; CHUNK];
    while c.pos < end {
        let n = read_full(r, &mut buf)?;
        let chunk = buf.get(..n).unwrap_or_default();
        if n == 0 {
            break;
        }
        match member.as_mut() {
            Some(m) => {
                m.feed(chunk, &mut |d: &[u8]| c.take(d))?;
                if m.done() {
                    break; // what follows the member is never read
                }
            }
            None => c.take(chunk),
        }
    }
    c.finish()
}

/// The data of the entries named `names` (any case; *deleted* markers are not entries), in the order given
/// (`fbrb.read`). A name listed twice in the bundle reads its later entry.
///
/// ```
/// use bc2_formats::fbrb;
/// let bundle = fbrb::build(&[("Levels/A.dbx", "<non-resource>", b"abc"), ("B.res", "Mesh", b"de")], true);
/// let data = fbrb::read(&mut std::io::Cursor::new(&bundle), &["b.RES", "levels/a.dbx"])?;
/// assert_eq!(data, vec![b"de".to_vec(), b"abc".to_vec()]);
/// # Ok::<(), fbrb::FbrbError>(())
/// ```
///
/// # Errors
/// [`FbrbError::MissingEntry`] for the first name that is not an entry (before any data is read), and the
/// errors of [`read_header`] and [`read_payload`].
pub fn read<R: Read + Seek, S: AsRef<str>>(r: &mut R, names: &[S]) -> Result<Vec<Vec<u8>>> {
    let header = read_header(r)?;
    let by = header.names();
    let wanted = names
        .iter()
        .enumerate()
        .map(|(index, n)| {
            by.get(&n.as_ref().to_ascii_lowercase())
                .copied()
                .ok_or(FbrbError::MissingEntry { index })
        })
        .collect::<Result<Vec<_>>>()?;
    read_payload(r, &header, &wanted)
}

/// Every entry of a bundle with its data, in archive order (a *deleted* marker's data is empty): an unpack in
/// memory.
///
/// # Errors
/// The errors of [`read_header`] and [`read_payload`].
pub fn extract<R: Read + Seek>(r: &mut R) -> Result<(Header, Vec<Vec<u8>>)> {
    let header = read_header(r)?;
    let live: Vec<&Entry> = header.entries.iter().filter(|e| !e.is_deleted()).collect();
    let mut data = read_payload(r, &header, &live)?.into_iter();
    let all = header
        .entries
        .iter()
        .map(|e| {
            if e.is_deleted() {
                Vec::new()
            } else {
                data.next().unwrap_or_default()
            }
        })
        .collect();
    Ok((header, all))
}

/// [`read_header`] of the file at `path` (only the header is read: fast).
///
/// # Errors
/// As [`read_header`].
pub fn entries_path(path: impl AsRef<Path>) -> Result<Header> {
    read_header(&mut File::open(path)?)
}

/// [`read`] from the file at `path`, without the cache: the payload decompressed from its start every time.
///
/// # Errors
/// As [`read`].
pub fn read_path_uncached<S: AsRef<str>>(path: impl AsRef<Path>, names: &[S]) -> Result<Vec<Vec<u8>>> {
    read(&mut File::open(path)?, names)
}

/// [`extract`] of the file at `path`, without the cache.
///
/// # Errors
/// As [`extract`].
pub fn extract_path_uncached(path: impl AsRef<Path>) -> Result<(Header, Vec<Vec<u8>>)> {
    extract(&mut File::open(path)?)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use std::io::Cursor;

    use super::*;
    use crate::fbrb::build;

    fn items() -> Vec<(&'static str, &'static str, Vec<u8>)> {
        vec![
            ("Levels/Test.dbx", "<non-resource>", b"level".repeat(100)),
            ("Gone.dbx", "*deleted*", vec![]),
            ("Big.res", "Mesh", (0..3_000_000u32).map(|i| (i % 253) as u8).collect()),
            ("Empty.txt", "Text", vec![]),
        ]
    }

    fn bundle(zipped: bool) -> Vec<u8> {
        let it = items();
        let refs: Vec<(&str, &str, &[u8])> = it.iter().map(|(n, t, d)| (*n, *t, d.as_slice())).collect();
        build(&refs, zipped)
    }

    #[test]
    fn reads_entries_both_layouts() {
        for zipped in [true, false] {
            let b = bundle(zipped);
            let data = read(&mut Cursor::new(&b), &["big.res", "LEVELS/test.dbx", "empty.txt"]).unwrap();
            let it = items();
            assert_eq!(data, vec![it[2].2.clone(), it[0].2.clone(), vec![]]);
            assert_eq!(
                read(&mut Cursor::new(&b), &[] as &[&str]).unwrap(),
                Vec::<Vec<u8>>::new()
            );
            assert!(matches!(
                read(&mut Cursor::new(&b), &["levels/test.dbx", "gone.dbx"]),
                Err(FbrbError::MissingEntry { index: 1 })
            ));
            let (h, all) = extract(&mut Cursor::new(&b)).unwrap();
            assert_eq!(h.zipped, zipped);
            assert_eq!(all.len(), 4);
            for ((_, _, d), got) in it.iter().zip(&all) {
                assert_eq!(d, got);
            }
        }
    }

    #[test]
    fn short_payload_is_refused() {
        for zipped in [true, false] {
            let b = bundle(zipped);
            let cut = &b[..b.len() - 20];
            assert!(read(&mut Cursor::new(cut), &["big.res"]).is_err());
            // an entry before the cut still reads
            assert!(read(&mut Cursor::new(cut), &["levels/test.dbx"]).is_ok());
        }
    }

    #[test]
    fn overlapping_entries() {
        let e = |offset, size| Entry {
            name: String::new(),
            type_name: String::new(),
            flags: 0,
            offset,
            size,
            size2: size,
        };
        let (a, b, c) = (e(0, 10), e(2, 3), e(9, 1));
        let mut col = Collector::new(&[&a, &b, &c]);
        for chunk in (0u8..10).collect::<Vec<_>>().chunks(3) {
            col.take(chunk);
        }
        assert_eq!(
            col.finish().unwrap(),
            vec![(0..10).collect::<Vec<u8>>(), vec![2, 3, 4], vec![9]]
        );
    }

    #[test]
    fn files_and_batches() {
        let dir = std::env::temp_dir().join(format!("bc2_fbrb_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("t.fbrb");
        std::fs::write(&p, bundle(true)).unwrap();
        assert_eq!(entries_path(&p).unwrap().entries.len(), 4);
        assert_eq!(read_path_uncached(&p, &["empty.txt"]).unwrap(), vec![Vec::<u8>::new()]);
        assert_eq!(extract_path_uncached(&p).unwrap().1.len(), 4);
        let missing = dir.join("missing.fbrb");
        assert!(matches!(read_path_uncached(&missing, &["x"]), Err(FbrbError::Io(_))));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// Stored payloads are read where each entry is: the same bytes and errors as streaming them from the start, for
    /// overlapping and out-of-order entries and a file cut anywhere.
    #[test]
    #[allow(clippy::many_single_char_names)]
    fn stored_reads_seek() {
        let e = |offset, size| Entry {
            name: String::new(),
            type_name: String::new(),
            flags: 0,
            offset,
            size,
            size2: size,
        };
        let b = bundle(false);
        let h = read_header(&mut b.as_slice()).unwrap();
        let payload = b.len() as u32 - h.payload_start as u32;
        let ents = [
            e(0, 10),
            e(5, 20),
            e(500, 0),
            e(3_000_400, 100),
            e(payload - 7, 7),
            e(payload - 7, 8),
            e(payload + 9, 0),
            e(100, 3_000_000),
        ];
        let picks: [&[usize]; 7] = [&[0], &[1, 0], &[3, 2, 1], &[4], &[5], &[6, 0], &[7, 1, 4, 0, 2]];
        for cut in [
            b.len(),
            b.len() - 1,
            b.len() - 8,
            h.payload_start as usize + 12,
            h.payload_start as usize,
        ] {
            let file = &b[..cut];
            for pick in picks {
                let want: Vec<&Entry> = pick.iter().map(|&i| &ents[i]).collect();
                let a = read_payload(&mut Cursor::new(file), &h, &want);
                let s = stream_payload(&mut Cursor::new(file), &h, &want);
                match (a, s) {
                    (Ok(x), Ok(y)) => assert_eq!(x, y),
                    (Err(FbrbError::PayloadEnds), Err(FbrbError::PayloadEnds)) => {}
                    (x, y) => unreachable!("cut {cut}, {pick:?}: {x:?} != {y:?}"),
                }
            }
        }
    }

    /// A reader that always fails.
    struct Broken;

    impl Read for Broken {
        fn read(&mut self, _: &mut [u8]) -> std::io::Result<usize> {
            Err(std::io::Error::other("broken"))
        }
    }

    /// A reader that returns a few bytes at a time and is interrupted once.
    struct Trickle<'a>(&'a [u8], bool);

    impl Read for Trickle<'_> {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            if !self.1 {
                self.1 = true;
                return Err(std::io::Error::from(std::io::ErrorKind::Interrupted));
            }
            let n = buf.len().min(self.0.len()).min(7);
            buf[..n].copy_from_slice(&self.0[..n]);
            self.0 = &self.0[n..];
            Ok(n)
        }
    }

    #[test]
    fn short_reads_fill_chunks() {
        let data: Vec<u8> = (0..100u8).collect();
        let mut buf = [0u8; 64];
        assert_eq!(read_full(&mut Trickle(&data, false), &mut buf).unwrap(), 64);
        assert_eq!(&buf[..], &data[..64]);
        assert!(matches!(read_full(&mut Broken, &mut buf), Err(FbrbError::Io(_))));
    }
}
