//! The Gameplay workspace's auto layout (round A1): the project's mode layer laid out on the level's ground in DICE's
//! style by `bc2_levelwrite::autolayout`, offered as a proposal - drawn over the views and listed with its findings and
//! DICE's statistics - and committed only when the user accepts it, as one move operation of the log (undone in one
//! step, every object edited afterwards as any other).
//!
//! The proposal is made on a worker thread from a copy of the project's partition and the base level's site (the
//! Problems panel's, [`crate::validation::Live::site`]); a proposal made at another revision of the document than the
//! current one is stale and cannot be accepted.

use std::sync::Arc;
use std::sync::mpsc::{Receiver, TryRecvError, channel};

use bc2_levelwrite::autolayout::{self, Ask, Proposal};
use bc2_levelwrite::placement::Mode;
use bc2_levelwrite::site::{Routes, Site};
use eframe::egui::Color32;

use crate::edit::Doc;
use crate::overlays::Segment;
use crate::theme::Palette;

/// The auto layout's state.
#[derive(Default)]
pub struct AutoLayout {
    /// The seed the next proposal uses.
    pub seed: u64,
    job: Option<(u64, Receiver<Result<Proposal, String>>)>,
    /// The last proposal and the document revision it was made at.
    pub proposal: Option<(u64, Proposal)>,
    /// What happened last, in words.
    pub message: String,
    /// How long the last proposal took (seconds).
    pub seconds: f64,
    started: Option<std::time::Instant>,
}

/// Why a proposal cannot be made for the document (`None`: it can).
#[must_use]
pub fn why_not(doc: Option<&Doc>) -> Option<&'static str> {
    let Some(d) = doc else {
        return Some("Open a project first (New map from an existing map, or Open a project)");
    };
    if d.writer().is_none() {
        return Some("The project's level partition does not parse");
    }
    let Some(c) = d.project.mode() else {
        return Some("Choose the export's target (era and mode) first: the auto layout lays out that mode's layer");
    };
    if Mode::of_key(&c.mode).is_none() {
        return Some("The auto layout lays out Conquest, Rush, Squad Rush and Squad Deathmatch");
    }
    None
}

impl AutoLayout {
    /// Whether a proposal is being made.
    #[must_use]
    pub fn busy(&self) -> bool {
        self.job.is_some()
    }

    /// Starts a proposal for the document over the base level's site (and its path data's routes, once the AI
    /// workspace has read them: every spawn and objective on the main network too).
    pub fn start(&mut self, doc: &Doc, site: Arc<Site>, routes: Option<Arc<Routes>>) {
        let (Some(w), Some(c)) = (doc.writer(), doc.project.mode()) else {
            return;
        };
        let Some(ask) = Ask::of_choice(c, self.seed) else {
            return;
        };
        let w = w.clone();
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = autolayout::propose_with(&w, &site, routes.as_deref(), ask).map_err(|e| e.to_string());
            let _ = tx.send(r);
        });
        self.job = Some((doc.revision, rx));
        self.started = Some(std::time::Instant::now());
        self.message = format!("Laying out the layer (seed {}) ...", self.seed);
    }

    /// Takes a finished proposal; whether one arrived (the views' look is made again).
    pub fn poll(&mut self) -> bool {
        let Some((rev, rx)) = &self.job else { return false };
        match rx.try_recv() {
            Ok(r) => {
                let rev = *rev;
                self.job = None;
                self.seconds = self.started.take().map_or(0.0, |t| t.elapsed().as_secs_f64());
                match r {
                    Ok(p) => {
                        self.message = format!("{} ({:.1} s)", p.describe(), self.seconds);
                        self.proposal = Some((rev, p));
                    }
                    Err(e) => {
                        self.message = format!("No layout proposed: {e}");
                        self.proposal = None;
                    }
                }
                true
            }
            Err(TryRecvError::Empty) => false,
            Err(TryRecvError::Disconnected) => {
                self.job = None;
                self.message = "The auto layout stopped without a proposal".into();
                true
            }
        }
    }

    /// The proposal when it was made on the document as it is now.
    #[must_use]
    pub fn current(&self, doc: &Doc) -> Option<&Proposal> {
        self.proposal
            .as_ref()
            .filter(|(rev, _)| *rev == doc.revision)
            .map(|x| &x.1)
    }

    /// Forgets the proposal (discarded or accepted).
    pub fn clear(&mut self) {
        self.proposal = None;
    }

    /// The proposal drawn over the views: where each moved flag, base, M-COM and spawn point would stand (a ring),
    /// a line from where it stands now, and the combat areas' new outlines (the outline points in order).
    #[must_use]
    pub fn lines(&self, doc: Option<&Doc>, p: &Palette) -> Vec<(Segment, Color32)> {
        let (Some(d), Some((_, prop))) = (doc, self.proposal.as_ref()) else {
            return Vec::new();
        };
        let Some(w) = d.writer() else { return Vec::new() };
        let kind = |g: &str| -> String {
            w.find(g)
                .map_or_default(|e| w.type_of(e))
                .rsplit('.')
                .next()
                .unwrap_or_default()
                .to_owned()
        };
        let lift = |v: [f32; 3]| [v[0], v[1] + 0.5, v[2]];
        let mut out = Vec::new();
        let ring = |c: [f32; 3], r: f32, col: Color32, out: &mut Vec<(Segment, Color32)>| {
            let n = 24;
            for k in 0..n {
                let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
                let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
                out.push((
                    [
                        [c[0] + r * a0.cos(), c[1], c[2] + r * a0.sin()],
                        [c[0] + r * a1.cos(), c[1], c[2] + r * a1.sin()],
                    ],
                    col,
                ));
            }
        };
        let by_guid: std::collections::HashMap<&str, [f32; 3]> =
            prop.moves.iter().map(|m| (m.guid.as_str(), lift(m.to[3]))).collect();
        for m in &prop.moves {
            let to = lift(m.to[3]);
            match kind(&m.guid).as_str() {
                "CapturePointEntityData" => {
                    // a ring and a 40 m pin at each flag and base, a line from where it stands now
                    ring(to, 20.0, p.accent, &mut out);
                    ring(to, 19.0, p.accent, &mut out);
                    out.push(([to, [to[0], to[1] + 40.0, to[2]]], p.accent));
                    out.push(([lift(m.from[3]), to], p.ink_muted));
                }
                "ReferenceObjectData" => {
                    ring(to, 8.0, p.status_warn, &mut out);
                    out.push(([to, [to[0], to[1] + 25.0, to[2]]], p.status_warn));
                }
                "AlternateSpawnEntityData" | "SoldierSpawnEntityData" => ring(to, 2.5, p.status_info, &mut out),
                "AreaGeometryEntityData" => {
                    // the outline to its Next point
                    if let Some(e) = w.find(&m.guid)
                        && let Some(n) =
                            w.d.child(e, b"Next")
                                .and_then(|x| w.d.attr(x, b"ref"))
                                .map(|r| String::from_utf8_lossy(r).to_ascii_uppercase())
                        && let Some(q) = by_guid.get(n.as_str())
                    {
                        out.push(([to, *q], p.accent));
                    }
                }
                _ => {}
            }
        }
        out
    }
}
