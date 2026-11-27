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

/// `console_common.xml_of(dbx)`: DbxEdit's XML of a .dbx (written beside it), read as text.
pub(crate) fn xml_of(path: &str) -> R<String> {
    super::dbx(&["xml", path])?;
    read_text_utf8(&format!("{}.xml", super::stem4(path)))
}

/// `console_soldiers.xml(dbx)`: the XML beside a .dbx, DbxEdit's made first only when there is none (an XML already
/// there is read as it is).
pub(crate) fn xml_cached(path: &str) -> R<String> {
    let x = format!("{}.xml", super::stem4(path));
    if !Path::new(&x).is_file() {
        super::dbx(&["xml", path])?;
    }
    read_text_utf8(&x)
}

/// Every `.xml` file under `src` deleted (the stages' clean-up before a pack).
pub(crate) fn drop_xml(src: &str) -> R<()> {
    let mut files = Vec::new();
    walk_all(src, &mut files);
    for f in files.iter().filter(|f| f.ends_with(".xml")) {
        std::fs::remove_file(f).map_err(|e| super::io_err(f, &e))?;
    }
    Ok(())
}

/// A manifest written as the stages write it: `open(man, 'w', encoding='utf-8', newline='\n')`, lines and a last `\n`.
pub(crate) fn write_manifest(man: &str, lines: &[String]) -> R<()> {
    write(man, (lines.join("\n") + "\n").as_bytes())
}

/// A manifest's lines (`open(man, encoding='utf-8').read().splitlines()`).
pub(crate) fn manifest_lines(man: &str) -> R<Vec<String>> {
    Ok(bc2_pyrt::py::splitlines(&read_text_utf8(man)?)
        .into_iter()
        .map(str::to_owned)
        .collect())
}

/// `'|'.join(cols[:3] + [str(size)] * (len(cols) - 3))` for the manifest line of `rel` (lower case) when it has 4
/// columns or more: the stages' size update of an edited partition.
pub(crate) fn set_sizes(lines: &mut [String], rels: &[&str], size_of: &dyn Fn(&str) -> R<u64>) -> R<()> {
    for l in lines.iter_mut() {
        let cols: Vec<&str> = l.split('|').collect();
        if cols.len() >= 4 && rels.contains(&cols[0].to_lowercase().as_str()) {
            let sz = size_of(cols[0])?.to_string();
            let mut out: Vec<&str> = cols[..3].to_vec();
            out.extend(std::iter::repeat_n(sz.as_str(), cols.len() - 3));
            *l = out.join("|");
        }
    }
    Ok(())
}

/// `os.path.join(src, *rel.split('/'))`.
pub(crate) fn under(src: &str, rel: &str) -> String {
    rel.split('/').fold(src.to_owned(), |acc, p| join(&acc, p))
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
