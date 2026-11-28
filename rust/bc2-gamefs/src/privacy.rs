//! Text the suite shows or shares with others, without the user's personal details (round OR15, IN1): the doctor's
//! report and a shared map project never carry the PC's name, the user's name or an absolute path of the user's
//! folders.
//!
//! - [`anonymise`]: the user's profile folder written `%USERPROFILE%`, the PC's and the user's names `<this PC>` /
//!   `<user>` (case-insensitive, whole words of a path or text);
//! - [`is_absolute_path`]: a string that names a place on this PC (`C:\...`, `\\server\...`, `/home/...`);
//! - [`leaks`]: what of those a text still holds (the checks after an anonymisation).

/// The PC's name and the user's name, as the environment has them (empty when unset).
fn names() -> (String, String) {
    let get = |k: &str| {
        std::env::var(k)
            .ok()
            .filter(|v| !v.trim().is_empty())
            .unwrap_or_default()
    };
    (
        if get("COMPUTERNAME").is_empty() {
            get("HOSTNAME")
        } else {
            get("COMPUTERNAME")
        },
        if get("USERNAME").is_empty() {
            get("USER")
        } else {
            get("USERNAME")
        },
    )
}

fn replace_ci(text: &str, what: &str, with: &str) -> String {
    if what.is_empty() {
        return text.to_owned();
    }
    let low = text.to_lowercase();
    let w = what.to_lowercase();
    if low.len() != text.len() {
        // a lower-casing that changed the length (rare letters): exact matches only
        return text.replace(what, with);
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while let Some(k) = low.get(i..).and_then(|s| s.find(&w)) {
        let at = i + k;
        let before = text.get(..at).and_then(|s| s.chars().next_back());
        let after = text.get(at + w.len()..).and_then(|s| s.chars().next());
        let word = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric() || c == '_');
        out.push_str(text.get(i..at).unwrap_or_default());
        if word(before) || word(after) {
            out.push_str(text.get(at..at + w.len()).unwrap_or_default());
        } else {
            out.push_str(with);
        }
        i = at + w.len();
    }
    out.push_str(text.get(i..).unwrap_or_default());
    out
}

/// `text` with the user's profile folder as `%USERPROFILE%`, the PC's name as `<this PC>` and the user's name as
/// `<user>`.
#[must_use]
pub fn anonymise(text: &str) -> String {
    let mut out = text.to_owned();
    if let Some(home) = std::env::var("USERPROFILE").ok().filter(|h| h.len() > 3) {
        out = replace_ci(&out, &home, "%USERPROFILE%");
        out = replace_ci(&out, &home.replace('\\', "/"), "%USERPROFILE%");
        out = replace_ci(&out, &home.replace('\\', "\\\\"), "%USERPROFILE%");
    }
    let (pc, user) = names();
    out = replace_ci(&out, &pc, "<this PC>");
    replace_ci(&out, &user, "<user>")
}

/// Whether `s` names a place on a PC: a drive path (`C:\`, `C:/`), a share (`\\server`), or a root path (`/x`).
#[must_use]
pub fn is_absolute_path(s: &str) -> bool {
    let b = s.as_bytes();
    (b.len() >= 3
        && b.first().is_some_and(u8::is_ascii_alphabetic)
        && b.get(1) == Some(&b':')
        && matches!(b.get(2), Some(b'\\' | b'/')))
        || s.starts_with("\\\\")
        || (s.starts_with('/') && s.len() > 1 && !s.starts_with("//"))
}

/// What of the user's details `text` still holds: the PC's name, the user's name, the profile folder (empty: none).
#[must_use]
pub fn leaks(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let (pc, user) = names();
    let low = text.to_lowercase();
    if let Some(home) = std::env::var("USERPROFILE").ok().filter(|h| h.len() > 3)
        && (low.contains(&home.to_lowercase()) || low.contains(&home.replace('\\', "/").to_lowercase()))
    {
        out.push("the user's profile folder".to_owned());
    }
    for (what, name) in [("the PC's name", pc), ("the user's name", user)] {
        if !name.is_empty() && replace_ci(text, &name, "\u{0}") != text {
            out.push(what.to_owned());
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_replaced_as_whole_words() {
        assert_eq!(
            replace_ci("by DESKTOP-1 at desktop-1x", "desktop-1", "<pc>"),
            "by <pc> at desktop-1x"
        );
        assert_eq!(replace_ci(r"C:\Users\Ann\x", r"c:\users\ann", "%U%"), r"%U%\x");
        assert!(is_absolute_path(r"C:\Games") && is_absolute_path("d:/x") && is_absolute_path(r"\\nas\share"));
        assert!(!is_absolute_path("levels/mp_005") && !is_absolute_path("heightmap.png") && !is_absolute_path("C:"));
    }
}
