//! Python's `str` rules the install texts depend on: `isspace` / `strip`, `splitlines` (with and without the line
//! ends) and the latin-1 code page `package.mft` and `bundleManifest` are read and written in (every byte one
//! character, so a byte outside the level's lines stays as it was).

/// `str.isspace` for one character: Unicode's `White_Space` and the separators `\x1c`-`\x1f` Python also counts.
#[must_use]
pub fn is_space(c: char) -> bool {
    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
}

/// `str.strip()`.
#[must_use]
pub fn strip(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `str.rstrip()`.
#[must_use]
pub fn rstrip(s: &str) -> &str {
    s.trim_end_matches(is_space)
}

/// `str.rstrip('\r\n')`.
#[must_use]
pub fn rstrip_nl(s: &str) -> &str {
    s.trim_end_matches(['\r', '\n'])
}

fn is_break(c: char) -> bool {
    matches!(
        c,
        '\n' | '\r' | '\u{0b}' | '\u{0c}' | '\u{1c}' | '\u{1d}' | '\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
    )
}

/// `str.splitlines(keepends=True)`: every line with its end (`\r\n` one end).
#[must_use]
pub fn splitlines_keep(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut it = s.char_indices().peekable();
    while let Some((i, c)) = it.next() {
        if is_break(c) {
            let mut end = i + c.len_utf8();
            if c == '\r' && it.peek().is_some_and(|&(_, n)| n == '\n') {
                it.next();
                end += 1;
            }
            out.push(s.get(start..end).unwrap_or_default());
            start = end;
        }
    }
    if start < s.len() {
        out.push(s.get(start..).unwrap_or_default());
    }
    out
}

/// `str.splitlines()`: the lines without their ends.
#[must_use]
pub fn splitlines(s: &str) -> Vec<&str> {
    splitlines_keep(s)
        .into_iter()
        .map(|l| l.trim_end_matches(is_break))
        .collect()
}

/// `bytes.decode('latin-1')`.
#[must_use]
pub fn latin1(b: &[u8]) -> String {
    b.iter().map(|&x| char::from(x)).collect()
}

/// `str.encode('latin-1')`; None when a character is outside it (Python raises).
#[must_use]
pub fn to_latin1(s: &str) -> Option<Vec<u8>> {
    s.chars().map(|c| u8::try_from(u32::from(c)).ok()).collect()
}

/// The path's last part (`os.path.basename` of a Windows path: after the last `\` or `/`).
#[must_use]
pub fn basename(p: &str) -> &str {
    p.rsplit(['\\', '/']).next().unwrap_or(p)
}

/// `ntpath.relpath(path, start)`: path relative to start with `\` separators (`..` up to their common folder, the parts
/// compared without regard to case); path itself when they are on different drives.
#[must_use]
pub fn relpath(path: &str, start: &str) -> String {
    let parts = |p: &str| -> Vec<String> {
        p.split(['\\', '/'])
            .filter(|x| !x.is_empty() && *x != ".")
            .map(str::to_owned)
            .collect()
    };
    let (a, b) = (parts(path), parts(start));
    if a.first().map(|x| x.to_lowercase()) != b.first().map(|x| x.to_lowercase()) {
        return path.to_owned();
    }
    let common = a
        .iter()
        .zip(&b)
        .take_while(|(x, y)| x.to_lowercase() == y.to_lowercase())
        .count();
    let mut out: Vec<String> = vec!["..".to_owned(); b.len() - common];
    out.extend(a.iter().skip(common).cloned());
    if out.is_empty() { ".".to_owned() } else { out.join("\\") }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splitlines_as_python() {
        assert_eq!(splitlines_keep("a\r\nb\rc\nd"), ["a\r\n", "b\r", "c\n", "d"]);
        assert_eq!(splitlines_keep("a\n\n"), ["a\n", "\n"]);
        assert_eq!(splitlines("a\r\nb\u{0b}c"), ["a", "b", "c"]);
        assert_eq!(splitlines_keep(""), Vec::<&str>::new());
        assert_eq!(strip("\u{1c} x \u{a0}"), "x");
        assert_eq!(to_latin1(&latin1(&[0, 0x85, 0xff])), Some(vec![0, 0x85, 0xff]));
        assert_eq!(basename(r"C:\g\Dist\win32\package.mft"), "package.mft");
        assert_eq!(
            relpath(r"C:\o\game\Package\levels\x", r"C:\O"),
            r"game\Package\levels\x"
        );
        assert_eq!(relpath(r"C:\t\p\game", r"C:\o\s"), r"..\..\t\p\game");
        assert_eq!(relpath(r"D:\x", r"C:\o"), r"D:\x");
    }
}
