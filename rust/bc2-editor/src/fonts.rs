//! The interface fonts: IBM Plex Sans (regular and semibold) and IBM Plex Mono, built into the editor.
//!
//! Source: the official IBM Plex releases on GitHub (github.com/IBM/plex), `@ibm/plex-sans@1.1.0`
//! (ibm-plex-sans.zip, SHA-256 fb365d91...c69200) and `@ibm/plex-mono@2.5.0` (ibm-plex-mono.zip, SHA-256
//! 6d23f012...bc481c), `fonts/complete/ttf`. Licence: SIL Open Font License 1.1 (`fonts/OFL.txt`, shipped beside
//! the fonts, as the licence asks). egui's own default fonts are not used.

use std::sync::Arc;

use eframe::egui::{FontData, FontDefinitions, FontFamily};

/// The family name of IBM Plex Sans semibold (weight 600: titles, labels, buttons).
pub const SANS_SEMIBOLD: &str = "plex-sans-semibold";

const SANS: &[u8] = include_bytes!("../fonts/IBMPlexSans-Regular.ttf");
const SANS_600: &[u8] = include_bytes!("../fonts/IBMPlexSans-SemiBold.ttf");
const MONO: &[u8] = include_bytes!("../fonts/IBMPlexMono-Regular.ttf");

/// The editor's fonts: `Proportional` is IBM Plex Sans, `Monospace` IBM Plex Mono, [`SANS_SEMIBOLD`] IBM Plex
/// Sans semibold; each family falls back to the others for a glyph it lacks.
#[must_use]
pub fn definitions() -> FontDefinitions {
    let mut d = FontDefinitions::empty();
    for (name, data) in [("plex-sans", SANS), (SANS_SEMIBOLD, SANS_600), ("plex-mono", MONO)] {
        d.font_data
            .insert(name.to_owned(), Arc::new(FontData::from_static(data)));
    }
    d.families
        .insert(FontFamily::Proportional, vec!["plex-sans".into(), "plex-mono".into()]);
    d.families
        .insert(FontFamily::Monospace, vec!["plex-mono".into(), "plex-sans".into()]);
    d.families.insert(
        FontFamily::Name(SANS_SEMIBOLD.into()),
        vec![SANS_SEMIBOLD.into(), "plex-sans".into(), "plex-mono".into()],
    );
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_are_the_plex_files() {
        // TrueType files (version 1.0 tag), not an HTML error page saved under a .ttf name
        for f in [SANS, SANS_600, MONO] {
            assert!(f.len() > 100_000);
            assert_eq!(f.get(..4), Some(&[0u8, 1, 0, 0][..]));
        }
        let d = definitions();
        assert_eq!(d.font_data.len(), 3);
        assert_eq!(d.families.len(), 3);
    }
}
