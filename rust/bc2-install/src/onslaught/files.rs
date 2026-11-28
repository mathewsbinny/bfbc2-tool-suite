//! The file helpers of the console port's stages 8 - 10 (round OR11) that the shared ones in `super::port` do not
//! cover: the install record of a folder Project OPFOR installed a level into, and a bundle glob that follows
//! junctions as `glob` does. (DbxEdit's XML of a partition, the manifest's lines and the XML clean-up are
//! `super::port`'s, shared with stages 5 - 7 and 11 - 13.)

use std::path::Path;

use super::join;

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
