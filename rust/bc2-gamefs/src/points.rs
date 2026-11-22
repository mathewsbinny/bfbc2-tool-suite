//! Restart points kept on disk between processes, by the content of the payload they serve
//! ([`bc2_formats::bf1943::PointsStore`]: the BF1943 source's payloads, round PR1b-1-rest). A file per payload,
//! `<24 hex digits>.points`, written beside its name and moved over it; only a cache: a file that cannot be read or
//! written makes the next read decompress the payload again.

use std::path::{Path, PathBuf};

use bc2_formats::bf1943::{PointsStore, points_name};

/// The points files of one folder.
#[derive(Debug, Clone)]
pub struct Folder(pub PathBuf);

impl Folder {
    /// The points files of `folder`.
    #[must_use]
    pub fn new(folder: &Path) -> Self {
        Self(folder.to_path_buf())
    }
}

impl PointsStore for Folder {
    fn load(&self, key: &[u8]) -> Option<Vec<u8>> {
        std::fs::read(self.0.join(points_name(key))).ok()
    }

    fn save(&self, key: &[u8], bytes: &[u8]) {
        if std::fs::create_dir_all(&self.0).is_ok() {
            let _ = crate::atomic::write_rebuildable(&self.0.join(points_name(key)), &[bytes]);
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn kept_by_content() {
        let dir = std::env::temp_dir().join(format!("bc2-gamefs-points-{}", std::process::id()));
        let f = Folder::new(&dir);
        assert_eq!(f.load(b"k"), None);
        f.save(b"k", b"bytes");
        assert_eq!(f.load(b"k").unwrap(), b"bytes");
        assert!(dir.join(points_name(b"k")).is_file());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
