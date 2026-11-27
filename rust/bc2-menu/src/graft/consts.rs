//! A movie's constant texts swapped (round OR14: Project OPFOR's `ww2\wake_ui.py`, `renamed`'s loop over
//! `aptgraft.Movie.const_entries`, through `bc2_native.AptMovie`).

use std::collections::HashMap;

use crate::error::MenuResult;
use bc2_formats::aptmodel::Val;

use super::{CONST, Movie, latin1, latin1_encode};

/// Each string constant whose text is a key of `keys` given the key's new text (a new `.const` string): {old text:
/// constants changed} in the order first met.
pub fn rename_constants<S: std::hash::BuildHasher>(
    m: &mut Movie,
    keys: &HashMap<String, String, S>,
) -> MenuResult<Vec<(String, usize)>> {
    let e = m.const_entries()?;
    let n = m.vals(e)?.len() / 2;
    let mut done: Vec<(String, usize)> = Vec::new();
    for k in 0..n {
        // Movie.constant(k): ('s', text) for type 1 with a pointer, else (type, value)
        let (t, x) = (m.val(e, 2 * k)?, m.val(e, 2 * k + 1)?);
        let (Val::Num(1), Val::Ref(r)) = (t, x) else {
            continue;
        };
        let old = latin1(m.str_text(r)?);
        let Some(new) = keys.get(&old) else {
            continue;
        };
        let s = m.new_str(CONST, latin1_encode(new)?);
        m.set(e, 2 * k + 1, Val::Ref(s))?;
        match done.iter_mut().find(|(o, _)| *o == old) {
            Some(d) => d.1 += 1,
            None => done.push((old, 1)),
        }
    }
    Ok(done)
}

/// `[movie.const_text(k) for k in range(len(movie.const_entries.vals) // 2)]`.
pub fn const_texts(m: &Movie) -> MenuResult<Vec<String>> {
    let n = m.vals(m.const_entries()?)?.len() / 2;
    (0..n).map(|k| m.const_text(k)).collect()
}
