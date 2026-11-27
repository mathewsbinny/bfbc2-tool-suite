//! What the console port's stages share (round OR9): `console_common.py`'s file and cache helpers as the stages use
//! them, and a stage's result.

use std::path::Path;

use super::{OnsError, R, Source, exists, fbrb, join, read, rmtree, stamp_of, stamp_path, write};

/// A stage's run: the lines it printed, the game copy's writes (in order) for the ledger's one commit, and the
/// refusal it stops with after printing (`SystemExit` in OPFOR's wrapper).
#[derive(Debug, Default)]
pub struct StageOut {
    /// The lines printed, in order.
    pub printed: Vec<String>,
    /// (the game copy's path, what goes there), in the order the ledger writes them.
    pub writes: Vec<(String, Source)>,
    /// The stage stopped after printing: this message, nothing to install.
    pub fail: Option<String>,
    /// The lines printed once the ledger's commit is done.
    pub after: Vec<String>,
}

/// `open(path, encoding='utf-8').read()`: strict UTF-8, universal newlines.
pub(crate) fn read_text_utf8(path: &str) -> R<String> {
    let b = read(path)?;
    let t = String::from_utf8(b).map_err(|e| OnsError::Data(format!("{path}: {e}")))?;
    Ok(t.replace("\r\n", "\n").replace('\r', "\n"))
}

/// `open(path, 'w', encoding='utf-8').write(text)` on Windows: `\n` written as `\r\n`.
pub(crate) fn write_text_utf8(path: &str, text: &str) -> R<()> {
    write(path, text.replace('\n', "\r\n").as_bytes())
}

/// `console_common.find(folder, rel)`: a file under `folder`, its relative path matched without case (`os.walk`'s
/// order: a folder's files, then its sub-folders).
pub(crate) fn find(folder: &str, rel: &str) -> R<String> {
    fn walk(d: &str, base: &str, want: &str) -> Option<String> {
        let rd = std::fs::read_dir(d).ok()?;
        let mut dirs = Vec::new();
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            let p = join(d, &name);
            if e.file_type().is_ok_and(|t| t.is_dir()) {
                dirs.push(p);
            } else {
                let r = super::rel(&p, base).replace('\\', "/").to_lowercase();
                if r == want {
                    return Some(p);
                }
            }
        }
        dirs.iter().find_map(|s| walk(s, base, want))
    }
    let want = rel.replace('\\', "/").to_lowercase();
    walk(folder, folder, &want).ok_or_else(|| OnsError::Exit(format!("{rel} not found in {folder}")))
}

/// `console_common.unpack_cached(bundle, out)`: FbrbTool's unpack, again whenever the bundle changed since (the stamp
/// beside `out`).
pub(crate) fn unpack_cached(bundle: &str, out: &str) -> R<()> {
    let st = stamp_path(out);
    let stamp = stamp_of(&[bundle])?;
    if exists(out) && Path::new(&st).is_file() && read_text_utf8(&st).ok().as_deref() == Some(stamp.as_str()) {
        return Ok(());
    }
    rmtree(out)?;
    if Path::new(&st).is_file() {
        std::fs::remove_file(&st).map_err(|e| super::io_err(&st, &e))?;
    }
    fbrb(&["unpack", bundle, out])?;
    write(&st, stamp.as_bytes())
}

/// `os.walk(folder)`'s files, every one (a folder's files, then its sub-folders).
pub(crate) fn walk_all(d: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(d) else { return };
    let mut dirs = Vec::new();
    for e in rd.flatten() {
        let p = join(d, &e.file_name().to_string_lossy());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            dirs.push(p);
        } else {
            out.push(p);
        }
    }
    for s in dirs {
        walk_all(&s, out);
    }
}
