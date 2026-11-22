//! Settings files (`opfor.ini`, `maps.ini`) kept across disable and enable: a key the user (or the launcher) changed
//! while a mod was out is carried onto the mod's file when it comes back, and a mod's own keys alone go back when it
//! goes out. Built on the WW2 install's ini rules ([`crate::text`]: `ini_value`, `ini_with_key`, `ini_without_key`).

use crate::text::{ini_value, ini_with_key, ini_without_key};

/// Every `(section, key)` of an ini text, lower case, in order (comments and lines without `=` skipped).
#[must_use]
pub fn keys(text: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    let mut sec = String::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            sec = t.trim_start_matches('[').trim_end_matches(']').trim().to_lowercase();
            continue;
        }
        if t.starts_with(';') || t.starts_with('#') {
            continue;
        }
        if let Some((k, _)) = t.split_once('=') {
            let k = k.trim().to_lowercase();
            if !k.is_empty() && !out.iter().any(|(s, x)| *s == sec && *x == k) {
                out.push((sec.clone(), k));
            }
        }
    }
    out
}

/// `onto` with every key whose value differs between `base` and `now` (added, changed or removed) set as in `now`:
/// the edits made to `base` since, carried onto another state of the file. Equal to `onto` when `base == now`.
#[must_use]
pub fn rebase(base: &str, now: &str, onto: &str) -> String {
    if base == now {
        return onto.to_owned();
    }
    let mut all = keys(base);
    for k in keys(now) {
        if !all.contains(&k) {
            all.push(k);
        }
    }
    let mut out = onto.to_owned();
    for (sec, key) in all {
        let (b, n) = (ini_value(base, &sec, &key), ini_value(now, &sec, &key));
        if b == n {
            continue;
        }
        out = match n {
            Some(v) => ini_with_key(&out, &sec, &key, &v),
            None => ini_without_key(&out, &sec, &key),
        };
    }
    out
}

/// `current` with `owned` keys set to their values in `prior` (a key `prior` lacks taken out): the file as it would
/// be without the owner's install, every other edit made since kept (`Guard::ini_refresh`'s rule, for any owner of
/// [`super::catalog::OWNED_INI_KEYS`]).
#[must_use]
pub fn without_owned(current: &str, prior: &str, owned: &[(&str, &str)]) -> String {
    let mut out = current.to_owned();
    for (sec, key) in owned {
        let (p, c) = (ini_value(prior, sec, key), ini_value(&out, sec, key));
        if p == c {
            continue;
        }
        out = match p {
            Some(v) => ini_with_key(&out, sec, key, &v),
            None => ini_without_key(&out, sec, key),
        };
    }
    out
}

/// `text` with `[section] key=value`.
#[must_use]
pub fn set(text: &str, section: &str, key: &str, value: &str) -> String {
    if ini_value(text, section, key).as_deref() == Some(value) {
        return text.to_owned();
    }
    ini_with_key(text, section, key, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    const BASE: &str = "[dev]\r\nlevel=Levels/MP_005GR\r\n[patches]\r\nvietnam=1\r\n";

    #[test]
    fn rebase_carries_the_edits_made_since() {
        let now = "[dev]\r\nlevel=Levels/MP_005GR\r\n[patches]\r\nvietnam=0\r\n[input]\r\nprompts=pad\r\n";
        let onto = "[dev]\r\nlevel=Levels/WAKE_ISLAND_S\r\n[patches]\r\nvietnam=1\r\n";
        let r = rebase(BASE, now, onto);
        assert_eq!(ini_value(&r, "dev", "level").as_deref(), Some("Levels/WAKE_ISLAND_S"));
        assert_eq!(ini_value(&r, "patches", "vietnam").as_deref(), Some("0"));
        assert_eq!(ini_value(&r, "input", "prompts").as_deref(), Some("pad"));
        assert_eq!(rebase(BASE, BASE, onto), onto);
    }

    #[test]
    fn owned_keys_alone_go_back() {
        let cur = "[dev]\r\nlevel=Levels/WAKE_ISLAND_S\r\n[patches]\r\nvietnam=0\r\n[gamemode]\r\npacific_levels=a\r\n";
        let r = without_owned(cur, BASE, &[("dev", "level"), ("gamemode", "pacific_levels")]);
        assert_eq!(ini_value(&r, "dev", "level").as_deref(), Some("Levels/MP_005GR"));
        assert_eq!(ini_value(&r, "gamemode", "pacific_levels"), None);
        assert_eq!(ini_value(&r, "patches", "vietnam").as_deref(), Some("0"));
        assert_eq!(set(BASE, "patches", "vietnam", "1"), BASE);
        assert_eq!(
            ini_value(&set(BASE, "patches", "vietnam", "0"), "patches", "vietnam").as_deref(),
            Some("0")
        );
    }
}
