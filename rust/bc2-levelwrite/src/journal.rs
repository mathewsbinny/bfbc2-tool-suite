//! The project's autosave as an append-only journal (round OP1, audit AU1 R17 / P3): instead of writing the whole
//! project after every change, the editor appends one line per change to `<project>.mapedit.json.autosave` - an
//! operation added or amended, an undo, a redo, a History jump, a base file recorded - after a first line holding the
//! project as it was when the journal started (opened, saved, or compacted). A save removes the journal.
//!
//! **Crash-safe.** Each line is `<crc32 of the JSON, 8 hex digits> <JSON>\n`, written and flushed to the operating
//! system at once (a killed editor loses nothing written) and synced to the disk at most once a [`SYNC_EVERY`]
//! (a power cut loses at most that). Reading stops at the first line that is cut short, fails its checksum or does
//! not follow from the lines before (an operation number out of order): everything before it is recovered, nothing
//! after it is guessed. **Resumable**: [`Journal::resume`] cuts such a torn end off and appends after the last good
//! line. **Compacted**: after [`COMPACT_LINES`] lines or [`COMPACT_BYTES`] it is rewritten as one first line of the
//! project now (whole or not at all, as a save is).

use std::fs::{File, OpenOptions};
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::ops::Operation;
use crate::project::{BaseFile, MapProject};

/// The journal is synced to the disk at most this often (each line is flushed to the operating system at once).
pub const SYNC_EVERY: Duration = Duration::from_secs(1);
/// It is compacted after this many lines.
pub const COMPACT_LINES: usize = 4096;
/// Or this many bytes.
pub const COMPACT_BYTES: u64 = 8 << 20;

/// One change of the project, as a line of the journal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "e", rename_all = "kebab-case")]
pub enum Entry {
    /// The project the journal starts from (its first line only).
    Start {
        /// The project.
        project: Box<MapProject>,
    },
    /// An operation added (the redo stack cleared).
    Push {
        /// The operation.
        op: Operation,
    },
    /// The last operation changed (a gesture that goes on: still one operation).
    Amend {
        /// The operation as it is now.
        op: Operation,
    },
    /// The last operation undone.
    Undo,
    /// The last undone operation redone.
    Redo,
    /// Undone or redone until this many operations are in the log (History).
    GoTo {
        /// The operations in the log.
        applied: usize,
    },
    /// A base file recorded on its first edit ([`MapProject::note_file`]).
    BaseFile {
        /// Its key.
        key: String,
        /// Its sha1 and size.
        file: BaseFile,
    },
}

/// One line's text: the checksum, the JSON, the end of line.
///
/// # Errors
/// Never in practice (`serde_json` on these types).
pub fn line(e: &Entry) -> io::Result<Vec<u8>> {
    let json = serde_json::to_vec(e).map_err(io::Error::other)?;
    let mut out = format!("{:08x} ", bc2_formats::zlib::crc32(&json)).into_bytes();
    out.extend_from_slice(&json);
    out.push(b'\n');
    Ok(out)
}

/// What a journal's bytes give.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Replay {
    /// The project after its last good line (`None`: not even the first line is whole).
    pub project: Option<MapProject>,
    /// Its good lines.
    pub lines: usize,
    /// The bytes they take (the rest is a torn or damaged end).
    pub good: u64,
    /// Bytes after the last good line (a write cut short, or a damaged line): left out.
    pub torn: bool,
}

/// A change applied to the project; `false` when it does not follow from it (the line is not taken).
fn follow(p: &mut MapProject, e: Entry) -> bool {
    match e {
        Entry::Start { .. } => false,
        Entry::Push { op } => {
            if op.n != p.next {
                return false;
            }
            p.redo.clear();
            p.next = op.n + 1;
            p.operations.push(op);
            true
        }
        Entry::Amend { op } => match p.operations.last_mut() {
            Some(last) if last.n == op.n => {
                *last = op;
                p.redo.clear();
                true
            }
            _ => false,
        },
        Entry::Undo => p.undo().is_some(),
        Entry::Redo => p.redo().is_some(),
        Entry::GoTo { applied } => {
            if applied > p.operations.len() + p.redo.len() {
                return false;
            }
            p.go_to(applied);
            true
        }
        Entry::BaseFile { key, file } => {
            p.base.files.insert(key, file);
            true
        }
    }
}

/// The project a journal's bytes hold, up to their last good line.
#[must_use]
pub fn replay(bytes: &[u8]) -> Replay {
    let mut out = Replay::default();
    let mut at = 0usize;
    while at < bytes.len() {
        let rest = bytes.get(at..).unwrap_or_default();
        let Some(end) = rest.iter().position(|b| *b == b'\n') else {
            break;
        };
        let text = rest.get(..end).unwrap_or_default();
        let entry = text
            .split_at_checked(9)
            .filter(|(crc, _)| crc.get(8) == Some(&b' '))
            .and_then(|(crc, json)| {
                let want = u32::from_str_radix(std::str::from_utf8(crc.get(..8)?).ok()?, 16).ok()?;
                (bc2_formats::zlib::crc32(json) == want).then_some(json)
            })
            .and_then(|json| serde_json::from_slice::<Entry>(json).ok());
        let taken = match (entry, out.project.as_mut()) {
            (Some(Entry::Start { project }), None) => {
                out.project = Some(*project);
                true
            }
            (Some(e), Some(p)) => follow(p, e),
            _ => false,
        };
        if !taken {
            break;
        }
        at += end + 1;
        out.lines += 1;
        out.good = at as u64;
    }
    out.torn = at < bytes.len();
    // as a project file read: its `min_reader` from its log
    if let Some(p) = out.project.as_mut() {
        p.min_reader = p.reader_needed();
    }
    out
}

/// A journal's file read.
///
/// # Errors
/// The file does not read.
pub fn read(path: &Path) -> io::Result<Replay> {
    Ok(replay(&std::fs::read(path)?))
}

/// The journal being written.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
    file: File,
    lines: usize,
    bytes: u64,
    synced: Instant,
    unsynced: bool,
}

impl Journal {
    /// A new journal at `path` starting from `project` (written whole or not at all, and synced).
    ///
    /// # Errors
    /// The file cannot be written or opened.
    pub fn start(path: &Path, project: &MapProject) -> io::Result<Self> {
        let first = line(&Entry::Start {
            project: Box::new(project.clone()),
        })?;
        crate::project::write_bytes(path, &first)?;
        Self::open(path, 1, first.len() as u64)
    }

    fn open(path: &Path, lines: usize, bytes: u64) -> io::Result<Self> {
        let file = OpenOptions::new().append(true).open(path)?;
        Ok(Self {
            path: path.to_owned(),
            file,
            lines,
            bytes,
            synced: Instant::now(),
            unsynced: false,
        })
    }

    /// The journal at `path` taken up again: its project as its good lines leave it, a torn end cut off, new lines
    /// appended after them. `None` when not even its first line is whole.
    ///
    /// # Errors
    /// The file does not read or cannot be cut or opened.
    pub fn resume(path: &Path) -> io::Result<Option<(Self, MapProject, Replay)>> {
        let r = read(path)?;
        let Some(p) = r.project.clone() else { return Ok(None) };
        if r.torn {
            let f = OpenOptions::new().write(true).open(path)?;
            f.set_len(r.good)?;
            f.sync_all()?;
        }
        Ok(Some((Self::open(path, r.lines, r.good)?, p, r)))
    }

    /// Appends a change: written and flushed to the operating system at once, synced to the disk when the last sync
    /// is [`SYNC_EVERY`] old.
    ///
    /// # Errors
    /// The line cannot be written.
    pub fn append(&mut self, e: &Entry) -> io::Result<()> {
        let l = line(e)?;
        self.file.write_all(&l)?;
        self.file.flush()?;
        self.lines += 1;
        self.bytes += l.len() as u64;
        self.unsynced = true;
        if self.synced.elapsed() >= SYNC_EVERY {
            self.sync()?;
        }
        Ok(())
    }

    /// Syncs what was appended to the disk.
    ///
    /// # Errors
    /// The sync fails.
    pub fn sync(&mut self) -> io::Result<()> {
        if self.unsynced {
            self.file.sync_data()?;
            self.unsynced = false;
        }
        self.synced = Instant::now();
        Ok(())
    }

    /// Whether it should be compacted ([`COMPACT_LINES`], [`COMPACT_BYTES`]).
    #[must_use]
    pub fn needs_compaction(&self) -> bool {
        self.lines > COMPACT_LINES || self.bytes > COMPACT_BYTES
    }

    /// Rewritten as one first line of `project` (whole or not at all).
    ///
    /// # Errors
    /// The file cannot be written or opened again (the old journal is then still whole).
    pub fn compact(&mut self, project: &MapProject) -> io::Result<()> {
        // the new file is moved over this one (Rust opens files with delete sharing on Windows, so the open
        // handle does not stop the move); this handle then goes
        let path = self.path.clone();
        *self = Self::start(&path, project)?;
        Ok(())
    }

    /// Its lines.
    #[must_use]
    pub fn lines(&self) -> usize {
        self.lines
    }

    /// Its size in bytes.
    #[must_use]
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    /// Its file.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for Journal {
    fn drop(&mut self) {
        let _ = self.sync();
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing)]
mod tests {
    use super::*;
    use crate::Op;

    fn project() -> MapProject {
        MapProject::new("Journal", "test", "levels/TEST", b"base", crate::Profile::Bfbc2)
    }

    /// The journal of a session (pushes, an amend, undo, redo, a jump, a base file) and the project after each line.
    fn session() -> (Vec<u8>, Vec<MapProject>) {
        let mut p = project();
        let mut bytes = line(&Entry::Start {
            project: Box::new(p.clone()),
        })
        .unwrap();
        let mut states = vec![p.clone()];
        let step = |p: &mut MapProject, e: Entry, bytes: &mut Vec<u8>, states: &mut Vec<MapProject>| {
            bytes.extend(line(&e).unwrap());
            assert!(follow(p, e));
            states.push(p.clone());
        };
        for k in 0..5u8 {
            let n = p.next;
            let mut q = p.clone();
            q.push(
                Op::Delete {
                    guids: vec![format!("G{k}")],
                },
                "label",
            );
            let op = q.operations.last().unwrap().clone();
            assert_eq!(op.n, n);
            step(&mut p, Entry::Push { op }, &mut bytes, &mut states);
        }
        let mut amended = p.operations.last().unwrap().clone();
        amended.op = Op::Delete {
            guids: vec!["AMENDED".into()],
        };
        step(&mut p, Entry::Amend { op: amended }, &mut bytes, &mut states);
        step(&mut p, Entry::Undo, &mut bytes, &mut states);
        step(&mut p, Entry::Undo, &mut bytes, &mut states);
        step(&mut p, Entry::Redo, &mut bytes, &mut states);
        step(&mut p, Entry::GoTo { applied: 1 }, &mut bytes, &mut states);
        step(
            &mut p,
            Entry::BaseFile {
                key: "group:x.dbx".into(),
                file: BaseFile {
                    sha1: "ab".into(),
                    size: 2,
                },
            },
            &mut bytes,
            &mut states,
        );
        (bytes, states)
    }

    /// Cut at every byte (a write stopped anywhere): the project after the last whole line, never a guess.
    #[test]
    fn a_journal_cut_anywhere_gives_its_last_whole_line() {
        let (bytes, states) = session();
        let ends: Vec<usize> = bytes
            .iter()
            .enumerate()
            .filter(|(_, b)| **b == b'\n')
            .map(|(i, _)| i + 1)
            .collect();
        assert_eq!(ends.len(), states.len());
        for cut in 0..=bytes.len() {
            let r = replay(&bytes[..cut]);
            let whole = ends.iter().filter(|e| **e <= cut).count();
            assert_eq!(r.lines, whole, "cut at {cut}");
            assert_eq!(r.good, ends.get(whole.wrapping_sub(1)).map_or(0, |e| *e as u64));
            assert_eq!(r.torn, cut as u64 > r.good);
            let want = whole.checked_sub(1).map(|k| {
                let mut s = states[k].clone();
                s.min_reader = s.reader_needed();
                s
            });
            assert_eq!(r.project, want, "cut at {cut}");
        }
        // a damaged byte in a middle line: everything before it, nothing after
        let mut bad = bytes.clone();
        bad[ends[3] + 12] ^= 0x20;
        let r = replay(&bad);
        assert_eq!(r.lines, 4);
        assert!(r.torn);
        // a line that does not follow (an operation number out of order) stops the replay too
        let mut p = states[0].clone();
        let mut skip = bytes[..ends[0]].to_vec();
        let mut q = p.clone();
        q.next = 7;
        q.push(
            Op::Delete {
                guids: vec!["X".into()],
            },
            "",
        );
        skip.extend(
            line(&Entry::Push {
                op: q.operations.pop().unwrap(),
            })
            .unwrap(),
        );
        assert_eq!(replay(&skip).lines, 1);
        // nothing to undo: an undo line does not follow either
        assert!(!follow(&mut p, Entry::Undo));
    }

    #[test]
    fn written_resumed_and_compacted() {
        let dir = std::env::temp_dir().join(format!("bc2_levelwrite_journal_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("J.mapedit.json.autosave");
        let (bytes, states) = session();
        let mut j = Journal::start(&path, &states[0]).unwrap();
        assert_eq!(j.lines(), 1);
        // the session's other lines, appended
        let first_end = bytes.iter().position(|b| *b == b'\n').unwrap() + 1;
        for l in bytes[first_end..].split_inclusive(|b| *b == b'\n') {
            let json = &l[9..l.len() - 1];
            j.append(&serde_json::from_slice(json).unwrap()).unwrap();
        }
        drop(j);
        let mut want = states.last().unwrap().clone();
        want.min_reader = want.reader_needed();
        assert_eq!(read(&path).unwrap().project.unwrap(), want);
        // a torn end (half a line) is cut off when resumed, and appending goes on after the last good line
        let mut f = OpenOptions::new().append(true).open(&path).unwrap();
        f.write_all(b"0000abcd {\"e\":\"undo\"").unwrap();
        drop(f);
        assert!(read(&path).unwrap().torn);
        let (mut j, p, r) = Journal::resume(&path).unwrap().unwrap();
        assert!(r.torn && p == want);
        j.append(&Entry::Redo).unwrap();
        drop(j);
        let r = read(&path).unwrap();
        assert!(!r.torn);
        assert_eq!(r.project.unwrap().operations.len(), want.operations.len() + 1);
        // compacted: one line, the same project
        let (mut j, p, _) = Journal::resume(&path).unwrap().unwrap();
        j.compact(&p).unwrap();
        assert_eq!(j.lines(), 1);
        j.append(&Entry::Undo).unwrap();
        drop(j);
        let r = read(&path).unwrap();
        assert_eq!(r.lines, 2);
        assert_eq!(r.project.unwrap().operations.len(), want.operations.len());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
