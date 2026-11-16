//! Many files at once, on all cores (rayon): a level's partitions, a corpus. Results keep the input order.

use rayon::prelude::*;

use super::error::Result;
use super::model::DbxFile;
use super::reader::parse;
use super::tree::{DbxTree, parse_tree};
use super::xml::to_xml;

/// [`parse`] of each input, in parallel.
///
/// ```
/// let files: Vec<&[u8]> = vec![b"", b"not a dbx"];
/// let out = bc2_formats::dbx::parse_all(&files);
/// assert!(out.iter().all(Result::is_err));
/// ```
#[must_use]
pub fn parse_all<T: AsRef<[u8]> + Sync>(inputs: &[T]) -> Vec<Result<DbxFile>> {
    inputs.par_iter().map(|d| parse(d.as_ref())).collect()
}

/// [`parse_tree`] of each input, in parallel: the compact trees borrow the inputs.
#[must_use]
pub fn parse_tree_all<T: AsRef<[u8]> + Sync>(inputs: &[T]) -> Vec<Result<DbxTree<'_>>> {
    inputs.par_iter().map(|d| parse_tree(d.as_ref())).collect()
}

/// [`to_xml`] of each input, in parallel.
#[must_use]
pub fn to_xml_all<T: AsRef<[u8]> + Sync>(inputs: &[T]) -> Vec<Result<String>> {
    inputs.par_iter().map(|d| to_xml(d.as_ref())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dbx::{Element, write};

    #[test]
    fn keeps_order() {
        let files: Vec<Vec<u8>> = (0..64)
            .map(|i| {
                let mut f = DbxFile::default();
                let e = Element::text(&mut f, "field", &[], &i.to_string());
                f.roots.push(e);
                write(&f).unwrap_or_default()
            })
            .collect();
        let xml = to_xml_all(&files);
        let parsed = parse_all(&files);
        let trees = parse_tree_all(&files);
        for (t, p) in trees.iter().zip(&parsed) {
            assert_eq!(t.as_ref().map(DbxTree::to_file).ok(), p.clone().ok());
        }
        for (i, (x, p)) in xml.iter().zip(&parsed).enumerate() {
            assert_eq!(
                x.as_deref().ok(),
                Some(format!("<?xml version=\"1.0\"?>\r\n<field>{i}</field>\r\n").as_str())
            );
            assert!(p.is_ok());
        }
    }
}
