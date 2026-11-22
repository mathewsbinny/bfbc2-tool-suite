//! Live validation (round V1): the export's rules checked on the project after every change, for the Problems panel
//! (`bc2_levelwrite::validate`: the rules of the edit phase and DICE's placement tendencies from round L1).
//!
//! [`Live`] keeps the last result with the document's revision it was made at and is made again when the revision
//! moves on (an operation applied, an undo, a redo, a History jump); what does not change while a project is open -
//! the base level's partition and its own DBX check, the mode's donor partition read from the game copy - is read
//! once. [`rows`] puts the document's own problems (OP1's typed problems: operations that do not apply, the base
//! changed) and the live findings together, errors first, each with its rule and the objects it names.

use std::collections::{HashMap, HashSet};

use bc2_levelwrite::rules::{Finding, Severity};
use bc2_levelwrite::validate;
use bc2_levelwrite::write::LevelWriter;

use crate::edit::Doc;

/// The last validation of a document.
#[derive(Default)]
pub struct Live {
    /// (project id, revision) the findings were made at.
    at: Option<(String, u64)>,
    /// The base level's partition and its own DBX check, read once per project.
    base: Option<(String, Option<LevelWriter>, HashSet<String>)>,
    /// Donor levels' partitions (and their sha1), read once.
    donors: HashMap<String, Option<(LevelWriter, String)>>,
    findings: Vec<Finding>,
    /// How long the last validation took (milliseconds).
    pub took_ms: f64,
}

impl Live {
    /// Validates the document again if it changed since the last time; whether it did.
    pub fn refresh(&mut self, doc: &Doc) -> bool {
        let key = (doc.project.id.clone(), doc.revision);
        if self.at.as_ref() == Some(&key) {
            return false;
        }
        let t = std::time::Instant::now();
        if self.base.as_ref().is_none_or(|b| b.0 != doc.project.id) {
            let w = doc.base_bytes().and_then(|b| LevelWriter::parse(b).ok());
            let check = w.as_ref().map_or_default(validate::base_check);
            self.base = Some((doc.project.id.clone(), w, check));
        }
        if let Some(c) = doc.project.mode()
            && !self.donors.contains_key(&c.donor)
        {
            let d = doc
                .game()
                .and_then(|g| bc2_levelwrite::export::level_partition(g, &c.donor).ok())
                .and_then(|b| {
                    let sha1 = bc2_levelwrite::project::sha1_hex(&b);
                    LevelWriter::parse(&b).ok().map(|w| (w, sha1))
                });
            self.donors.insert(c.donor.clone(), d);
        }
        let donor = doc
            .project
            .mode()
            .and_then(|c| self.donors.get(&c.donor))
            .and_then(Option::as_ref)
            .map(|(w, s)| (w, s.as_str()));
        let (base, base_check) = self.base.as_ref().map_or((None, None), |b| (b.1.as_ref(), Some(&b.2)));
        let level = doc.writer();
        self.findings = if doc.base_bytes().is_none() {
            // the demo scene: no partition, nothing the export would write
            Vec::new()
        } else {
            validate::validate(validate::Input {
                project: &doc.project,
                level,
                base,
                donor,
                game: None,
                languages: None,
                base_check,
                mode: None,
            })
        };
        self.at = Some(key);
        self.took_ms = t.elapsed().as_secs_f64() * 1000.0;
        true
    }

    /// The findings of the last validation.
    #[must_use]
    pub fn findings(&self) -> &[Finding] {
        &self.findings
    }
}

/// One row of the Problems panel.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    /// How bad.
    pub severity: Severity,
    /// The rule (`XVAL-EDIT-3`, `L1-RU-004`).
    pub code: String,
    /// In words.
    pub text: String,
    /// The objects it names (upper-case guids): "Go to" selects the first the scene has.
    pub objects: Vec<String>,
}

/// The document's problems and the live findings as the panel lists them: errors, then warnings, then notes (each in
/// the order found).
#[must_use]
pub fn rows(doc: &Doc, live: &Live) -> Vec<Row> {
    let mut out: Vec<Row> = doc
        .problems
        .iter()
        .map(|p| Row {
            severity: match p.severity {
                crate::error::Severity::Error => Severity::Error,
                crate::error::Severity::Warning => Severity::Warning,
                crate::error::Severity::Note => Severity::Note,
            },
            code: if p.code == crate::error::ProblemCode::NotApplied {
                bc2_levelwrite::rules::apply_code(&p.text).to_owned()
            } else {
                p.code.rule().to_owned()
            },
            text: p.text.clone(),
            objects: p.objects.clone(),
        })
        .collect();
    out.extend(live.findings().iter().map(|f| Row {
        severity: f.severity,
        code: f.code.clone(),
        text: f.message.clone(),
        objects: f.objects.clone(),
    }));
    out.sort_by_key(|r| std::cmp::Reverse(r.severity));
    out
}

/// The rows' counts: (errors, warnings, notes).
#[must_use]
pub fn counts(rows: &[Row]) -> (usize, usize, usize) {
    let n = |s: Severity| rows.iter().filter(|r| r.severity == s).count();
    (n(Severity::Error), n(Severity::Warning), n(Severity::Note))
}
