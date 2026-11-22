//! The Gameplay workspace's auto layout (round A1): the project's mode layer laid out on the level's ground in DICE's
//! style by `bc2_levelwrite::autolayout`, offered as a proposal - drawn over the views and listed with its findings and
//! DICE's statistics - and committed only when the user accepts it, as one move operation of the log (undone in one
//! step, every object edited afterwards as any other).
//!
//! The proposal is made on a worker thread from a copy of the project's partition and the base level's site (the
//! Problems panel's, [`crate::validation::Live::site`]); a proposal made at another revision of the document than the
//! current one is stale and cannot be accepted.
//!
//! Round A2: the proposals come several at a time (the best distinct places or turns of the layer, side by side in the
//! panel, each with its statistics; one drawn in the views at a time), each with its vehicles; the objects the user
//! pinned ([`bc2_levelwrite::ops::Op::Pin`], the project's organisation) stay where they stand.

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
    job: Option<(u64, Receiver<Result<Vec<Proposal>, String>>)>,
    /// The last proposals (best first) and the document revision they were made at.
    pub proposals: Option<(u64, Vec<Proposal>)>,
    /// The proposal drawn in the views and offered for Accept (an index into the proposals).
    pub shown: usize,
    /// How many proposals the next Propose makes (1 to 4; 0 is taken as 3).
    pub count: usize,
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
        let pins = doc.org.pinned.clone();
        let k = if self.count == 0 { 3 } else { self.count.min(4) };
        let (tx, rx) = channel();
        std::thread::spawn(move || {
            let r = autolayout::propose_all(&w, &site, routes.as_deref(), ask, &pins, k).map_err(|e| e.to_string());
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
                    Ok(ps) => {
                        self.message = format!(
                            "{} proposal{} in {:.1} s; the best: {}",
                            ps.len(),
                            if ps.len() == 1 { "" } else { "s" },
                            self.seconds,
                            ps.first().map_or_else(String::new, Proposal::describe)
                        );
                        self.shown = 0;
                        self.proposals = Some((rev, ps));
                    }
                    Err(e) => {
                        self.message = format!("No layout proposed: {e}");
                        self.proposals = None;
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

    /// The proposal shown, when it was made on the document as it is now.
    #[must_use]
    pub fn current(&self, doc: &Doc) -> Option<&Proposal> {
        self.all(doc).get(self.shown)
    }

    /// Every proposal of the last Propose, when they were made on the document as it is now (else none).
    #[must_use]
    pub fn all(&self, doc: &Doc) -> &[Proposal] {
        self.proposals
            .as_ref()
            .filter(|(rev, _)| *rev == doc.revision)
            .map_or(&[], |x| x.1.as_slice())
    }

    /// Whether proposals are held (current or stale).
    #[must_use]
    pub fn holds(&self) -> bool {
        self.proposals.is_some()
    }

    /// Forgets the proposals (discarded or accepted).
    pub fn clear(&mut self) {
        self.proposals = None;
        self.shown = 0;
    }

    /// The proposal drawn over the views: where each moved flag, base, M-COM and spawn point would stand (a ring),
    /// a line from where it stands now, and the combat areas' new outlines (the outline points in order).
    #[must_use]
    pub fn lines(&self, doc: Option<&Doc>, p: &Palette) -> Vec<(Segment, Color32)> {
        let Some(d) = doc else { return Vec::new() };
        let Some(prop) = self.proposals.as_ref().and_then(|x| x.1.get(self.shown)) else {
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
        // round A2: the vehicles - a ring of their room's size, a line to the capture point they join, the way they face
        let v = &prop.vehicles;
        let vehicle_rows = v
            .moves
            .iter()
            .map(|m| (m.guid.as_str(), m.to))
            .chain(v.places.iter().map(|x| (x.guid.as_str(), x.transform)));
        for (g, rows) in vehicle_rows {
            let at = lift(rows[3]);
            let Some(veh) = v.vehicles.iter().find(|x| x.guid.eq_ignore_ascii_case(g)) else {
                continue;
            };
            let r = match veh.class.as_str() {
                "heli" | "plane" => 7.0,
                "tank" | "ifv" | "ifv_aa" => 4.0,
                _ => 2.5,
            };
            ring(at, r, p.status_ok, &mut out);
            let f = rows[2];
            out.push((
                [at, [at[0] + f[0] * r * 1.6, at[1], at[2] + f[2] * r * 1.6]],
                p.status_ok,
            ));
            let cp = by_guid
                .get(veh.owner.as_str())
                .copied()
                .or_else(|| w.transform(&veh.owner).map(|x| lift(x[3])));
            if let Some(c) = cp {
                out.push(([at, c], p.ink_muted));
            }
        }
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
