//! The file helpers of the console port's stages 8 - 10 (round OR11): `console_common`'s and the stages' own small
//! steps on an unpacked bundle - DbxEdit's XML of a partition, the manifest's lines read and written as the Python
//! read and wrote them, the XML files dropped before a pack, a folder walk that follows junctions as `os.walk` and
//! `glob` do.

use std::path::Path;

use super::port::{read_text_utf8, walk_all};
use super::{R, dbx, io_err, join, stem4, write};

/// `console_common.xml_of(dbx)`: DbxEdit's XML of a .dbx (written beside it), read as text.
pub(crate) fn xml_of(path: &str) -> R<String> {
    dbx(&["xml", path])?;
    read_text_utf8(&format!("{}.xml", stem4(path)))
}

/// `os.path.join(src, *rel.split('/'))`.
pub(crate) fn under(src: &str, rel: &str) -> String {
    rel.split('/').fold(src.to_owned(), |acc, p| join(&acc, p))
}

/// A manifest's lines (`open(man, encoding='utf-8').read().splitlines()`).
pub(crate) fn manifest_lines(man: &str) -> R<Vec<String>> {
    Ok(bc2_pyrt::py::splitlines(&read_text_utf8(man)?)
        .into_iter()
        .map(str::to_owned)
        .collect())
}

/// A manifest written as the stages write it: `open(man, 'w', encoding='utf-8', newline='\n')`, lines and a last `\n`.
pub(crate) fn write_manifest(man: &str, lines: &[String]) -> R<()> {
    write(man, (lines.join("\n") + "\n").as_bytes())
}

/// Every `.xml` file under `src` deleted (the stages' clean-up before a pack).
pub(crate) fn drop_xml(src: &str) -> R<()> {
    let mut files = Vec::new();
    walk_all(src, &mut files);
    for f in files.iter().filter(|f| f.ends_with(".xml")) {
        std::fs::remove_file(f).map_err(|e| io_err(f, &e))?;
    }
    Ok(())
}

/// A folder Project OPFOR installed a level into: it holds an install record `opfor-<name>-installed.json` (the WW2
/// levels, `ww2_common`; the rule of the suite's stock index, `bc2_convert::meshview::cache::bundles`). Its bundles are
/// not the user's stock PC bundles (spec XCONS-ONS-10).
pub(crate) fn opfor_installed(dir: &str) -> bool {
    std::fs::read_dir(dir).is_ok_and(|rd| {
        rd.flatten().any(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.starts_with("opfor-") && n.ends_with("-installed.json") && n.len() > "opfor--installed.json".len()
        })
    })
}

/// The `.fbrb` files under `top` (any case, as Windows' `glob` matches), every folder followed - a junction too, as
/// `glob(os.path.join(top, '**', '*.fbrb'), recursive=True)` follows it; names starting with a dot left out. Unsorted.
pub(crate) fn glob_fbrb(top: &str, out: &mut Vec<String>) {
    let Ok(rd) = std::fs::read_dir(top) else { return };
    for e in rd.flatten() {
        let name = e.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') {
            continue;
        }
        let p = join(top, &name);
        if Path::new(&p).is_dir() {
            glob_fbrb(&p, out);
        } else if name.to_lowercase().ends_with(".fbrb") {
            out.push(p);
        }
    }
}
