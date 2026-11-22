//! The AI workspace's navigation (round GR2: the Navigation Studio folded into the editor, then retired): the
//! **Navigation** panel (the files and their figures, the colouring, the layers, the selected area or spawn, the path
//! test, the anomaly overlay, the compare with a reference AI layer), the **Islands and spawns** panel and the
//! **Navigation edits** panel (the tools, the edits of the project's log, the side file, Build with these edits,
//! Install into the game and Restore DICE's original, each install or restore shown as a plan to confirm first); the
//! layers drawn in the views, clicks for the path test and the edit tools, the hover's area.
//!
//! The data is [`crate::nav`]'s and [`crate::navdata`]'s; the drawing [`crate::navview`]'s, in the `view-nav-*` and
//! `view-ref-*` tokens.

#[allow(
    clippy::wildcard_imports,
    reason = "a part of app.rs in a file of its own: its types and helpers"
)]
use super::*;

use std::sync::Arc;

use bc2_levelwrite::navedit::{NavBase, NavEdit};

use crate::error::EditorError;
use crate::nav::{self, NavTool, Plan};
use crate::navdata::{self, Y_OFFSET};
use crate::navview::{self, ColourMode, FileLayer, Lines, OverlayMesh};
use crate::workspace::ViewMode;

/// A box's twelve edges.
fn box_lines(c: [f64; 3], size: f64, colour: Color32, out: &mut Lines) {
    let h = size / 2.0;
    let k = |i: usize| -> [f32; 3] {
        [
            (c[0] + if i & 1 == 0 { -h } else { h }) as f32,
            (c[1] + if i & 2 == 0 { -h } else { h }) as f32,
            (c[2] + if i & 4 == 0 { -h } else { h }) as f32,
        ]
    };
    for (a, b) in [
        (0, 1),
        (2, 3),
        (4, 5),
        (6, 7),
        (0, 2),
        (1, 3),
        (4, 6),
        (5, 7),
        (0, 4),
        (1, 5),
        (2, 6),
        (3, 7),
    ] {
        out.push(([k(a), k(b)], colour));
    }
}

/// Filled boxes as one mesh.
fn boxes_mesh(centres: &[[f64; 3]], size: f64, key: &str, group: &str, colour: Color32) -> Option<OverlayMesh> {
    if centres.is_empty() {
        return None;
    }
    let h = size / 2.0;
    let mut v = Vec::with_capacity(centres.len() * 8);
    let mut t = Vec::with_capacity(centres.len() * 12);
    for c in centres {
        let b = v.len() as u32;
        for i in 0..8usize {
            v.push([
                (c[0] + if i & 1 == 0 { -h } else { h }) as f32,
                (c[1] + if i & 2 == 0 { -h } else { h }) as f32,
                (c[2] + if i & 4 == 0 { -h } else { h }) as f32,
            ]);
        }
        for [a, bb, cc, d] in [
            [0, 1, 3, 2],
            [4, 6, 7, 5],
            [0, 4, 5, 1],
            [2, 3, 7, 6],
            [0, 2, 6, 4],
            [1, 5, 7, 3],
        ] {
            t.push([b + a, b + bb, b + cc]);
            t.push([b + a, b + cc, b + d]);
        }
    }
    Some(OverlayMesh {
        key: navview::key_of(&[key, group, &format!("{colour:?}"), &centres.len().to_string()]),
        group: group.to_owned(),
        colour,
        vertices: Arc::new(v),
        triangles: Arc::new(t),
    })
}

fn line(a: [f64; 3], b: [f64; 3], colour: Color32) -> ([[f32; 3]; 2], Color32) {
    (
        [
            [a[0] as f32, a[1] as f32, a[2] as f32],
            [b[0] as f32, b[1] as f32, b[2] as f32],
        ],
        colour,
    )
}

fn up(p: [f64; 3], dy: f64) -> [f64; 3] {
    [p[0], p[1] + dy, p[2]]
}

fn cross(p: [f64; 3], size: f64, colour: Color32, out: &mut Lines) {
    out.push(line([p[0] - size, p[1], p[2]], [p[0] + size, p[1], p[2]], colour));
    out.push(line([p[0], p[1], p[2] - size], [p[0], p[1], p[2] + size], colour));
}

fn post(p: [f64; 3], height: f64, colour: Color32, out: &mut Lines) {
    out.push(line(p, up(p, height), colour));
}

/// An edit drawn over the scene (`editing.op_segments`): regions as their outline at both heights, seeds and notes
/// as posts with a cross, traversals bottom to top, links and route edges between their points.
fn edit_lines(e: &NavEdit, p: &Palette, out: &mut Lines) {
    let colour = match e.op.as_str() {
        "walkable" | "seed" => p.view_nav_edit_add,
        "unwalkable" | "unseed" => p.view_nav_edit_remove,
        "route-edge" if e.action.as_deref() == Some("remove") => p.view_nav_edit_remove,
        "traversal" | "note" => p.view_nav_edit_mark,
        _ => p.view_nav_edit_link,
    };
    if let Some(r) = &e.region {
        if let (Some(poly), Some(y)) = (&r.polygon, r.y) {
            let n = poly.len();
            for (k, a) in poly.iter().enumerate() {
                let b = poly.get((k + 1) % n).copied().unwrap_or(*a);
                for h in y {
                    out.push(line([a[0], h, a[1]], [b[0], h, b[1]], colour));
                }
                out.push(line([a[0], y[0], a[1]], [a[0], y[1], a[1]], colour));
            }
        } else if let Some(b) = r.cube {
            let c = [
                f64::midpoint(b[0], b[3]),
                f64::midpoint(b[1], b[4]),
                f64::midpoint(b[2], b[5]),
            ];
            let s = [b[3] - b[0], b[4] - b[1], b[5] - b[2]];
            let k = |i: usize| -> [f64; 3] {
                [
                    c[0] + if i & 1 == 0 { -s[0] } else { s[0] } / 2.0,
                    c[1] + if i & 2 == 0 { -s[1] } else { s[1] } / 2.0,
                    c[2] + if i & 4 == 0 { -s[2] } else { s[2] } / 2.0,
                ]
            };
            for (a, bb) in [
                (0, 1),
                (2, 3),
                (4, 5),
                (6, 7),
                (0, 2),
                (1, 3),
                (4, 6),
                (5, 7),
                (0, 4),
                (1, 5),
                (2, 6),
                (3, 7),
            ] {
                out.push(line(k(a), k(bb), colour));
            }
        }
        return;
    }
    match e.op.as_str() {
        "seed" | "unseed" => {
            if let Some(a) = e.at {
                post(a, 2.5, colour, out);
                cross(up(a, 2.5), 0.6, colour, out);
            }
        }
        "note" => {
            if let Some(a) = e.at {
                post(a, 3.5, colour, out);
                cross(up(a, 3.5), 0.4, colour, out);
            }
        }
        "traversal" => {
            if let (Some(b), Some(t)) = (e.bottom, e.top) {
                out.push(line(b, t, colour));
                cross(b, 0.4, colour, out);
                cross(t, 0.8, colour, out);
            }
        }
        _ => {
            if let (Some(a), Some(b)) = (e.a, e.b) {
                out.push(line(up(a, 0.5), up(b, 0.5), colour));
                post(a, 1.5, colour, out);
                post(b, 1.5, colour, out);
            }
        }
    }
}

/// The soldier spawns of a scene: (scene index, position), in scene order.
fn spawns_of(scene: &Scene, shown: &dyn Fn(usize) -> bool) -> Vec<(usize, [f64; 3])> {
    scene
        .entities
        .iter()
        .filter(|e| e.category == "soldier spawn" && shown(e.index))
        .filter_map(|e| e.matrix.map(|m| (e.index, [m[12], m[13], m[14]])))
        .collect()
}

impl ViewState {
    /// Whether the navigation layers are drawn: in the AI workspace, and in the navmesh view mode.
    #[must_use]
    pub fn nav_active(&self) -> bool {
        self.workspace == Workspace::Ai || self.view_mode == ViewMode::Navmesh
    }

    /// The OPFOR folder the navigation reads and writes (the side file, the builds, the installs): the editor's, or
    /// the one a test set.
    #[must_use]
    pub fn nav_root(&self) -> Option<PathBuf> {
        self.navs.root.clone().or_else(|| self.root.clone())
    }

    /// Starts reading the level's navigation data (once).
    pub fn nav_ensure(&mut self) {
        if self.navs.loaded.is_some() || self.navs.reading() {
            return;
        }
        let Some(root) = self.nav_root() else {
            self.navs.loaded = Some(Err(EditorError::Io(
                "No game copy: no navigation data can be read".into(),
            )));
            return;
        };
        let shown = |k: usize| self.doc.as_ref().is_none_or(|d| d.shown(k));
        let scene = self.scene();
        let spawns = spawns_of(scene, &shown);
        let hf = scene.heightfield.clone();
        let partition = self
            .doc
            .as_ref()
            .and_then(Doc::partition)
            .or_else(|| scene.file(0).map(|(_, d)| d.to_vec()));
        self.navs.spawn_list = spawns.iter().map(|(k, _)| *k).collect();
        let lv = self.lv.clone();
        let points: Vec<[f64; 3]> = spawns.iter().map(|(_, p)| *p).collect();
        self.navs.start(&root, &lv, move |navkit, dice| nav::Inputs {
            navkit,
            dice,
            heightfield: hf,
            partition,
            spawns: points,
        });
    }

    /// Waits for the navigation data (tests).
    pub fn nav_wait(&mut self, gpu: &GpuCtx, palette: &Palette) {
        self.nav_ensure();
        self.navs.wait(std::time::Duration::from_secs(600));
        self.nav_refresh(gpu, palette);
    }

    /// The views again after the navigation changed (a layer, the colouring, an edit, the path test).
    pub fn nav_refresh(&mut self, gpu: &GpuCtx, palette: &Palette) {
        self.apply_look(palette);
        self.refresh(gpu, palette);
    }

    /// Each frame: the worker's answer, the running command, the project's edits changed (an undo).
    pub(super) fn nav_poll(&mut self, gpu: &GpuCtx, palette: &Palette) {
        // a running command's end, whichever workspace is shown (round DF1: Install and Restore wait for it)
        if let Some(j) = self.navs.job.as_mut()
            && j.running()
            && let Some(ok) = j.poll()
        {
            self.navs.message = job_message(j, ok);
        }
        if !self.nav_active() {
            return;
        }
        if self.navs.loaded.is_none() && !self.navs.reading() {
            self.nav_ensure();
            if self.navs.loaded.is_some() {
                self.nav_refresh(gpu, palette);
            }
        }
        let mut again = self.navs.poll() || std::mem::take(&mut self.navs.dirty);
        let rev = self.doc.as_ref().map(|d| d.revision);
        if rev != self.navs.drawn {
            self.navs.drawn = rev;
            again = true;
        }
        if again {
            self.nav_refresh(gpu, palette);
        }
    }

    /// The navigation's meshes and lines for the views (only the layers shown).
    pub(super) fn nav_look(&mut self, palette: &Palette) -> (Vec<OverlayMesh>, Lines) {
        let mut meshes: Vec<OverlayMesh> = Vec::new();
        let mut lines: Lines = Vec::new();
        if !self.nav_active() {
            return (meshes, lines);
        }
        self.nav_ensure();
        let theme = format!("{:?}", palette.view_nav_main);
        let hf = self.scene().heightfield.clone();
        let n = &mut self.navs;
        let mut used: Vec<String> = Vec::new();
        if let Some(Ok(d)) = &n.loaded {
            for f in &d.files {
                let group = format!("{} ground", f.name);
                if !n.hidden.contains(&group) {
                    let mode = if n.colour == ColourMode::Difference && d.same.is_none() {
                        ColourMode::Island
                    } else {
                        n.colour
                    };
                    let key = format!("{group}|{}|{theme}", mode.name());
                    let got = n.cache.entry(key.clone()).or_insert_with(|| {
                        Arc::new(navview::ground_meshes(
                            &f.data,
                            &f.data.path.display().to_string(),
                            &group,
                            mode,
                            d.same_of(f.name),
                            f.name == "DICE",
                            hf.as_ref(),
                            palette,
                        ))
                    });
                    meshes.extend(got.iter().cloned());
                    used.push(key);
                }
                let lad_group = format!("{} ladder ground", f.name);
                if !n.hidden.contains(&lad_group) {
                    let cells: Vec<usize> = f
                        .data
                        .cell_area
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| f.data.areas.get(**a as usize).is_some_and(|x| x.ladder))
                        .map(|(k, _)| k)
                        .collect();
                    if !cells.is_empty() {
                        let (v, t) = navview::cell_quads(&f.data, &cells, hf.as_ref(), navview::LIFT + 0.05);
                        meshes.push(OverlayMesh {
                            key: navview::key_of(&[&lad_group, &theme, &f.data.path.display().to_string()]),
                            group: lad_group,
                            colour: palette.view_nav_ladder,
                            vertices: Arc::new(v),
                            triangles: Arc::new(t),
                        });
                    }
                }
                for layer in FileLayer::ALL {
                    if matches!(layer, FileLayer::Ground | FileLayer::LadderGround) {
                        continue;
                    }
                    let name = format!("{} {}", f.name, layer.name());
                    if n.hidden.contains(&name) {
                        continue;
                    }
                    let key = format!("{name}|{theme}");
                    let got = n
                        .lines
                        .entry(key)
                        .or_insert_with(|| Arc::new(navview::file_lines(&f.data, layer, layer.colour(palette))));
                    lines.extend(got.iter().copied());
                }
            }
            // DICE's ground NavKit lacks
            let missing = "DICE ground missing in NavKit";
            if let (Some(dice), Some((_, same_r))) = (d.file("DICE"), &d.same)
                && !n.hidden.contains(missing)
            {
                let cells: Vec<usize> = same_r
                    .iter()
                    .enumerate()
                    .filter(|(_, s)| !**s)
                    .map(|(k, _)| k)
                    .collect();
                let key = format!("{missing}|{theme}");
                let got = n.cache.entry(key.clone()).or_insert_with(|| {
                    let (v, t) = navview::cell_quads(&dice.data, &cells, hf.as_ref(), navview::LIFT + 0.03);
                    Arc::new(vec![OverlayMesh {
                        key: navview::key_of(&[missing, &theme, &dice.data.path.display().to_string()]),
                        group: missing.to_owned(),
                        colour: palette.view_nav_missing,
                        vertices: Arc::new(v),
                        triangles: Arc::new(t),
                    }])
                });
                meshes.extend(got.iter().cloned());
                used.push(key);
            }
            // the anomaly overlay
            if let (Some(an), Some(nk)) = (&d.anomalies, d.file("NavKit")) {
                if !n.hidden.contains("anomaly: spawners on small islands") {
                    for a in &an.spawners_on_small_islands {
                        box_lines(up(a.pos(), 0.9), 3.0, palette.view_status_error, &mut lines);
                    }
                }
                if !n.hidden.contains("anomaly: orders without a node") {
                    for a in &an.orders_without_node {
                        box_lines(up(a.pos(), 1.2), 3.0, palette.view_status_warn, &mut lines);
                    }
                }
                if !n.hidden.contains("anomaly: sliver areas") {
                    let areas: std::collections::HashSet<usize> = an
                        .sliver_areas
                        .iter()
                        .filter_map(|a| match a {
                            navdata::Anomaly::SliverArea { area, .. } => Some(*area),
                            _ => None,
                        })
                        .collect();
                    let cells: Vec<usize> = nk
                        .data
                        .cell_area
                        .iter()
                        .enumerate()
                        .filter(|(_, a)| areas.contains(&(**a as usize)))
                        .map(|(k, _)| k)
                        .collect();
                    if !cells.is_empty() {
                        let (v, t) = navview::cell_quads(&nk.data, &cells, hf.as_ref(), navview::LIFT + 0.05);
                        meshes.push(OverlayMesh {
                            key: navview::key_of(&[
                                "anomaly: sliver areas",
                                &theme,
                                &nk.data.path.display().to_string(),
                            ]),
                            group: "anomaly: sliver areas".into(),
                            colour: palette.view_nav_border,
                            vertices: Arc::new(v),
                            triangles: Arc::new(t),
                        });
                    }
                }
                if !n.hidden.contains("anomaly: lonely nodes") {
                    let pts: Vec<[f64; 3]> = an.lonely_nodes.iter().map(|a| up(a.pos(), -(Y_OFFSET - 0.4))).collect();
                    meshes.extend(boxes_mesh(
                        &pts,
                        0.7,
                        &theme,
                        "anomaly: lonely nodes",
                        palette.view_nav_small,
                    ));
                }
            }
        }
        n.cache.retain(|k, _| used.contains(k) || !k.contains('|'));
        // the compare overlay: a reference level's AI layer
        if let Some(c) = &n.compare {
            let items = c.items();
            let mut filled: HashMap<&'static str, Vec<[f64; 3]>> = HashMap::new();
            for it in &items {
                if n.hidden.contains(it.layer) {
                    continue;
                }
                let colour = match it.layer {
                    l if l.contains("AI spawners") => palette.view_ref_ai,
                    l if l.contains("human spawners") => palette.view_ref_human,
                    "reference area orders" => palette.view_ref_order,
                    "reference patrol orders" => palette.view_ref_patrol,
                    _ => palette.view_ref_objective,
                };
                match it.kind {
                    "spawner" if it.layer.ends_with("(disabled)") => {
                        box_lines(up(it.pos, 0.9), 1.4, colour, &mut lines);
                    }
                    "spawner" => filled.entry(it.layer).or_default().push(up(it.pos, 0.9)),
                    "area order" => {
                        filled.entry(it.layer).or_default().push(up(it.pos, 1.2));
                        lines.push(line(it.pos, up(it.pos, 1.2), colour));
                    }
                    "objective" => filled.entry(it.layer).or_default().push(up(it.pos, 0.6)),
                    _ => {}
                }
            }
            if !n.hidden.contains("reference patrol orders") {
                for pt in c.layer.patrols.iter().filter(|p| p.points.len() > 1) {
                    for (a, b) in pt.points.iter().zip(pt.points.iter().skip(1)) {
                        lines.push(line(up(*a, 0.5), up(*b, 0.5), palette.view_ref_patrol));
                    }
                }
            }
            #[allow(
                clippy::iter_over_hash_type,
                reason = "reference layers drawn as points of distinct colours (no output is written)"
            )]
            for (layer, pts) in &filled {
                let (size, colour) = match *layer {
                    l if l.contains("AI spawners") => (1.4, palette.view_ref_ai),
                    l if l.contains("human spawners") => (1.4, palette.view_ref_human),
                    "reference area orders" => (1.0, palette.view_ref_order),
                    _ => (1.2, palette.view_ref_objective),
                };
                meshes.extend(boxes_mesh(pts, size, &c.spec, layer, colour));
            }
            if let Some(pd) = &c.pathdata
                && !n.hidden.contains("reference path data outlines")
            {
                for a in &pd.areas {
                    for l in &a.outline {
                        for (a, b) in l.iter().zip(l.iter().skip(1)) {
                            let p =
                                |v: [f32; 3]| [f64::from(v[0]), f64::from(v[1]) - (Y_OFFSET - 0.15), f64::from(v[2])];
                            lines.push(line(p(*a), p(*b), palette.view_ref_outline));
                        }
                    }
                }
            }
            if let Some((k, Some(near))) = c.selected
                && let (Some(it), Some(nk)) = (items.get(k), n.data().and_then(|d| d.file("NavKit")))
                && let Some(node) = nk.data.node_pos.get(near.1)
            {
                let colour = if near.0 <= navdata::R_GATHER {
                    palette.view_status_ok
                } else {
                    palette.view_nav_missing
                };
                lines.push(line(up(it.pos, 0.3), up(*node, -(Y_OFFSET - 0.3)), colour));
            }
        }
        // the path test: its points, each file's route or where the islands come closest
        for p in &n.path.points {
            for (dx, dy, dz) in [(0.6, 0.0, 0.0), (0.0, 1.5, 0.0), (0.0, 0.0, 0.6)] {
                lines.push(line(
                    [p[0] - dx, p[1] - dy, p[2] - dz],
                    [p[0] + dx, p[1] + dy, p[2] + dz],
                    palette.view_nav_edit_mark,
                ));
            }
        }
        if let Some(d) = n.data() {
            for (name, r) in &n.path.routes {
                let Some(f) = d.file(name) else { continue };
                let colour = if *name == "NavKit" {
                    palette.view_nav_route_navkit
                } else {
                    palette.view_nav_route_dice
                };
                let lift = if *name == "NavKit" { 0.35 } else { 0.25 };
                let g = |k: usize| f.data.node_pos.get(k).map(|q| [q[0], q[1] - Y_OFFSET + lift, q[2]]);
                if r.found {
                    let chain: Vec<[f64; 3]> = r.nodes.iter().filter_map(|k| g(*k)).collect();
                    for (a, b) in chain.iter().zip(chain.iter().skip(1)) {
                        lines.push(line(*a, *b, colour));
                    }
                    if let (Some(a), Some(b), Some(s), Some(e)) =
                        (n.path.points.first(), n.path.points.get(1), chain.first(), chain.last())
                    {
                        lines.push(line(*a, *s, colour));
                        lines.push(line(*e, *b, colour));
                    }
                } else if let Some((a, b, _)) = r.gap
                    && let (Some(a), Some(b)) = (g(a), g(b))
                {
                    lines.push(line(a, b, palette.view_nav_missing));
                    lines.push(line(a, up(a, 3.0), palette.view_nav_missing));
                    lines.push(line(b, up(b, 3.0), palette.view_nav_missing));
                }
            }
        }
        // the edits, and the drawing in progress
        if let Some(d) = &self.doc {
            for e in &d.project.nav_edits().edits {
                edit_lines(e, palette, &mut lines);
            }
        }
        let n = &self.navs;
        for p in &n.clicks {
            cross(*p, 0.5, palette.view_nav_edit_mark, &mut lines);
            post(*p, 1.0, palette.view_nav_edit_mark, &mut lines);
        }
        if n.clicks.len() >= 2 && n.polygon && matches!(n.tool, Some(NavTool::Walkable | NavTool::Unwalkable)) {
            let y = n.clicks.iter().map(|p| p[1]).sum::<f64>() / n.clicks.len() as f64 + 0.3;
            for (a, b) in n.clicks.iter().zip(n.clicks.iter().skip(1)) {
                lines.push(line([a[0], y, a[2]], [b[0], y, b[2]], palette.view_nav_edit_mark));
            }
        }
        (meshes, lines)
    }

    /// The point on the navigation ground along a ray (the first cell it passes within 0.6 m of), for clicks and the
    /// hover where no scene surface is under the pointer.
    #[must_use]
    pub fn nav_ray_point(&self, o: V3, d: V3) -> Option<V3> {
        let data = self.navs.data()?;
        let mut t = 0.0;
        while t < 1500.0 {
            let p = [o[0] + d[0] * t, o[1] + d[1] * t, o[2] + d[2] * t];
            for f in &data.files {
                if let Some((k, _)) = f.cell_at(p)
                    && let Some(y) = f.data.cell_y.get(k)
                    && (p[1] - y).abs() < 0.6
                {
                    return Some([p[0], *y, p[2]]);
                }
            }
            if let Some(hf) = &self.scene().heightfield
                && p[1] < hf.height(p[0], p[2]) - 1.0
            {
                return None;
            }
            t += 0.4;
        }
        None
    }

    /// A click in a view while the navigation is drawn: the path test's points, an edit tool's clicks, a reference
    /// marker (`marker`: the one under the pointer), else the area under it is selected. `true` when the click was
    /// the navigation's (nothing else selects).
    pub(super) fn nav_click(
        &mut self,
        gpu: &GpuCtx,
        palette: &Palette,
        point: Option<V3>,
        marker: Option<usize>,
    ) -> bool {
        if !self.nav_active() {
            return false;
        }
        if self.navs.path.active {
            if let Some(p) = point {
                self.navs.path_click(p);
                self.message = match self.navs.path.points.len() {
                    1 => "Path test: now click the end".to_owned(),
                    _ => "Path test: the routes are in the Navigation panel".to_owned(),
                };
                self.nav_refresh(gpu, palette);
            }
            return true;
        }
        if let Some(tool) = self.navs.tool
            && self.workspace == Workspace::Ai
            && self.doc.is_some()
        {
            if let Some(p) = point {
                self.navs.clicks.push(p);
                self.nav_try_commit(gpu, palette, tool, false);
                self.nav_refresh(gpu, palette);
            }
            return true;
        }
        if let Some(k) = marker {
            let near = self
                .navs
                .compare
                .as_ref()
                .and_then(|c| c.items().get(k).map(|i| i.pos))
                .and_then(|p| {
                    let nk = self.navs.data()?.file("NavKit")?;
                    navdata::NavIndex::new(&nk.data).nearest_node(p, 1e9, None)
                });
            if let Some(c) = self.navs.compare.as_mut() {
                c.selected = Some((k, near));
            }
            self.nav_refresh(gpu, palette);
            return true;
        }
        if let Some(c) = self.navs.compare.as_mut()
            && c.selected.take().is_some()
        {
            self.nav_refresh(gpu, palette);
        }
        self.navs.selected = point.and_then(|p| self.navs.area_at(p));
        false
    }

    /// Makes the edit of the drawing when it is complete (`finish`: a polygon's Enter).
    pub(super) fn nav_try_commit(&mut self, gpu: &GpuCtx, palette: &Palette, tool: NavTool, finish: bool) {
        let n = &self.navs;
        let Some(edit) = nav::edit_of(
            tool,
            &n.clicks,
            n.polygon,
            finish,
            n.margin,
            n.traversal,
            n.two_way,
            &n.note,
            &n.by,
        ) else {
            if tool == NavTool::Note && n.note.trim().is_empty() && !n.clicks.is_empty() {
                self.navs.message = "Type the note first".into();
                self.navs.clicks.clear();
            }
            return;
        };
        self.navs.clicks.clear();
        let kind = self.nav_kind();
        let problems = nav::problems(&edit, &kind);
        if !problems.is_empty() {
            self.navs.message = format!("Not added: {}", problems.join("; "));
            return;
        }
        let line = nav::summary(&edit);
        self.commit(gpu, palette, Op::NavAdd { edit }, "");
        self.navs.message = format!("Added: {line}");
    }

    /// The base kind of the project's edits: a loaded side file's, else the file edited (`navkit` / `dice`).
    #[must_use]
    pub fn nav_kind(&self) -> String {
        self.doc
            .as_ref()
            .and_then(|d| d.project.nav_edits().base)
            .map_or_else(|| self.navs.kind().to_owned(), |b| b.kind)
    }

    /// The navigation edits of the project.
    #[must_use]
    pub fn nav_edits(&self) -> Vec<NavEdit> {
        self.doc.as_ref().map_or_else(Vec::new, |d| d.project.nav_edits().edits)
    }

    /// The side file's base: a loaded one's, else the edited file's SHA-1 and name.
    fn nav_base(&self) -> NavBase {
        if let Some(b) = self.doc.as_ref().and_then(|d| d.project.nav_edits().base) {
            return b;
        }
        let file = match self.navs.kind() {
            "dice" => self.navs.found.1.clone(),
            _ => self.navs.found.0.clone(),
        };
        NavBase {
            kind: self.navs.kind().to_owned(),
            sha1: file.as_deref().map_or_else(String::new, nav::sha1_of),
            file: file
                .as_deref()
                .and_then(Path::file_name)
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
        }
    }

    /// The level's name as the side file names it (`Levels/<NAME>`).
    fn nav_level_name(&self) -> String {
        let name = self.scene().name.clone();
        if name.to_lowercase().starts_with("levels/") {
            name
        } else {
            format!("Levels/{}", if name.is_empty() { self.lv.to_uppercase() } else { name })
        }
    }

    /// Writes the project's edits as the level's side file; its path.
    ///
    /// # Errors
    /// No game copy, or the file cannot be written.
    pub fn nav_save_side(&mut self) -> Result<PathBuf, EditorError> {
        let root = self.nav_root().ok_or(EditorError::Io("No game copy".into()))?;
        let path = nav::side_file(&root, &self.lv);
        let text = nav::side_text(&self.nav_level_name(), &self.nav_base(), &self.nav_edits());
        nav::write_side(&path, &text)?;
        self.navs.message = format!("Saved {} edit(s) to {}", self.nav_edits().len(), path.display());
        Ok(path)
    }

    /// Reads the level's side file into the project (one operation: its edits replace the list).
    pub fn nav_reload(&mut self, gpu: &GpuCtx, palette: &Palette) {
        let Some(root) = self.nav_root() else { return };
        let path = nav::side_file(&root, &self.lv);
        match nav::read_side(&path) {
            Ok((base, edits)) => {
                let n = edits.len();
                self.commit(
                    gpu,
                    palette,
                    Op::NavLoad {
                        from: path.display().to_string(),
                        base,
                        edits,
                    },
                    "",
                );
                self.navs.message = format!("Loaded {n} edit(s) from {}", path.display());
            }
            Err(e) => self.navs.message = format!("Not loaded: {e}"),
        }
    }

    /// Where Build with these edits writes.
    #[must_use]
    pub fn nav_out(&self) -> Option<PathBuf> {
        Some(nav::build_out(&self.nav_root()?, &self.lv, &self.nav_kind()))
    }

    /// Build with these edits: the side file saved, then `navkit apply` in the background.
    pub fn nav_build(&mut self) {
        let (Some(root), Some(out)) = (self.nav_root(), self.nav_out()) else {
            return;
        };
        let side = match self.nav_save_side() {
            Ok(p) => p,
            Err(e) => {
                self.navs.message = format!("Not built: {e}");
                return;
            }
        };
        let args = vec![
            "apply".to_owned(),
            self.lv.to_uppercase(),
            "--navtune".to_owned(),
            side.display().to_string(),
            "--out".to_owned(),
            out.display().to_string(),
        ];
        match nav::start_navkit(&root, &args, "apply") {
            Ok(mut j) => {
                // a build cut short (the editor closed) leaves no half-written file behind (round DF1)
                j.partial = Some(out.clone());
                self.navs.job = Some(j);
                self.navs.message = if self.nav_kind() == "dice" {
                    "Applying the edits to a copy of DICE's file ...".into()
                } else {
                    "Building with the edits (a full rebuild: about 1-8 minutes) ...".into()
                };
            }
            Err(e) => self.navs.message = format!("navkit did not start: {e}"),
        }
    }

    /// Reads the built file as NavKit's (Open the result).
    pub fn nav_open_result(&mut self, gpu: &GpuCtx, palette: &Palette) {
        if let Some(out) = self.nav_out().filter(|p| p.is_file()) {
            self.navs.candidate = Some(out);
            self.navs.reset();
            self.nav_ensure();
            self.nav_refresh(gpu, palette);
        }
    }

    /// Shows the install plan, to confirm.
    pub fn nav_plan_install(&mut self) {
        if let (Some(root), Some(out)) = (self.nav_root(), self.nav_out()) {
            self.navs.confirm = Some(nav::install_plan(&root, &self.lv, &out));
        }
    }

    /// Shows the restore plan, to confirm.
    pub fn nav_plan_restore(&mut self) {
        if let Some(root) = self.nav_root() {
            self.navs.confirm = Some(nav::restore_plan(&root, &self.lv));
        }
    }

    /// The plan confirmed (runs it, waiting for it: install and restore are quick) or cancelled.
    pub fn nav_confirm(&mut self, yes: bool) {
        let Some(plan) = self.navs.confirm.take() else { return };
        if !yes || plan.blocked.is_some() {
            self.navs.message = format!("{} cancelled", plan.what);
            return;
        }
        let Some(root) = self.nav_root() else { return };
        // round DF1 (audit AU4-CON-2): never beside a running build (it writes the file the install copies), and
        // its Job is never dropped
        if self.navs.job.as_ref().is_some_and(nav::Job::running) {
            self.navs.message = format!("{} not started: {NAV_RUNNING}", plan.what);
            return;
        }
        // round DF1 (audit AU4-CQ-6): run in the background like the build, its end reported by nav_poll
        match nav::start_navkit(&root, &plan.args, plan.what) {
            Ok(j) => {
                self.navs.message = format!("navkit {} ...", plan.what);
                self.navs.job = Some(j);
            }
            Err(e) => self.navs.message = format!("navkit did not start: {e}"),
        }
    }

    /// The navigation's actions; `false` for actions that are not theirs.
    pub(super) fn nav_action(&mut self, gpu: &GpuCtx, palette: &Palette, action: Action) -> bool {
        match action {
            Action::PathTest => {
                self.navs.path.active = !self.navs.path.active;
                if self.navs.path.active {
                    self.navs.tool = None;
                    self.navs.clicks.clear();
                    self.measures.measuring = false;
                    if let Some(p) = self.menus.menu_point.take() {
                        self.navs.path.points.clear();
                        self.navs.path_click(p);
                    }
                    self.message = "Path test: click the start, then the end".into();
                } else {
                    self.message = "Path test off".into();
                }
                self.nav_ensure();
                self.nav_refresh(gpu, palette);
            }
            Action::NavTool(t) => {
                self.navs.tool = if self.navs.tool == Some(t) { None } else { Some(t) };
                self.navs.clicks.clear();
                self.navs.path.active = false;
                self.measures.measuring = false;
                // from the context menu: the point there is the first click
                if let (Some(t), Some(p)) = (self.navs.tool, self.menus.menu_point.take()) {
                    self.navs.clicks.push(p);
                    self.nav_try_commit(gpu, palette, t, false);
                }
                self.message = self.navs.tool.map_or_else(
                    || "Navigation tool off".to_owned(),
                    |t| format!("{}: {}", t.name(), t.how(self.navs.polygon)),
                );
                self.nav_refresh(gpu, palette);
            }
            Action::NavColour(m) => {
                self.navs.colour = m;
                self.message = format!("The ground coloured by {}", m.name().to_lowercase());
                self.nav_refresh(gpu, palette);
            }
            Action::NavSaveEdits => {
                if let Err(e) = self.nav_save_side() {
                    self.navs.message = format!("Not saved: {e}");
                }
            }
            Action::NavReloadEdits => {
                self.nav_reload(gpu, palette);
                self.nav_refresh(gpu, palette);
            }
            Action::NavBuild => self.nav_build(),
            Action::NavInstall => self.nav_plan_install(),
            Action::NavRestore => self.nav_plan_restore(),
            _ => return false,
        }
        true
    }

    /// Why a navigation action cannot run now, or `None` (with the general reasons in `why_not_in`).
    #[must_use]
    pub fn nav_why_not(&self, action: Action) -> Option<&'static str> {
        let loaded = self.navs.data();
        let editing = self.doc.is_some() && self.workspace == Workspace::Ai;
        match action {
            Action::PathTest if loaded.is_none_or(|d| d.files.is_empty()) => {
                Some("The level has no navigation data read (show the navmesh first)")
            }
            Action::NavColour(ColourMode::Difference) if loaded.is_none_or(|d| d.same.is_none()) => {
                Some("Needs both NavKit's build and DICE's file")
            }
            Action::NavColour(ColourMode::Anomaly) if loaded.is_none_or(|d| d.file("NavKit").is_none()) => {
                Some("Needs a NavKit build of the level")
            }
            Action::NavColour(_) if loaded.is_none() => Some("The level has no navigation data read"),
            Action::NavTool(_) | Action::NavSaveEdits | Action::NavReloadEdits | Action::NavBuild if !editing => {
                Some("Edit a project in the AI workspace first")
            }
            Action::NavTool(t) if t.region() && self.nav_kind() == "dice" => {
                Some("Region edits need a NavKit base (DICE's data takes only links, route edges, traversals, notes)")
            }
            Action::NavBuild | Action::NavInstall | Action::NavRestore
                if self.navs.job.as_ref().is_some_and(nav::Job::running) =>
            {
                Some(NAV_RUNNING)
            }
            Action::NavInstall if self.nav_out().is_none_or(|p| !p.is_file()) => Some("Build with these edits first"),
            Action::NavRestore
                if self
                    .nav_root()
                    .is_none_or(|r| nav::restore_plan(&r, &self.lv).blocked.is_some()) =>
            {
                Some("Nothing to restore: no NavKit file is installed")
            }
            _ => None,
        }
    }
}

/// Why a navkit command waits for the one running (round DF1, audit AU4-CON-2).
pub const NAV_RUNNING: &str =
    "A navkit command is running: wait for it (an install beside a build would copy a half-written file)";

/// What the status says when a navkit command ended: a build's end, an install's or a restore's last lines.
fn job_message(j: &nav::Job, ok: bool) -> String {
    if j.what == "apply" {
        return if ok {
            format!("navkit {}: done", j.what)
        } else {
            format!("navkit {}: failed (its output below)", j.what)
        };
    }
    let out = j.output();
    let tail: Vec<&str> = out
        .iter()
        .map(String::as_str)
        .filter(|l| !l.trim().is_empty())
        .collect();
    let last = tail.iter().rev().take(2).rev().copied().collect::<Vec<_>>().join(" / ");
    if ok {
        last
    } else {
        format!("navkit {} failed: {last}", j.what)
    }
}

/// A colour swatch before a label.
fn swatch(ui: &mut egui::Ui, colour: Color32) {
    let (r, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
    ui.painter().rect_filled(r.shrink(1.0), CornerRadius::same(2), colour);
}

/// A section title.
fn title(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add_space(6.0);
    ui.label(caption(text, p.ink_muted));
}

fn muted(ui: &mut egui::Ui, p: &Palette, text: &str) {
    ui.add(egui::Label::new(RichText::new(text).text_style(style("small")).color(p.ink_muted)).wrap());
}

impl Tabs<'_> {
    /// A button that runs an action (enabled when it can run, else its reason as the tooltip); its rect is kept by
    /// name for the tests.
    fn nav_button(&mut self, ui: &mut egui::Ui, action: Action, label: &str, primary: bool) -> bool {
        let why = why_not_in(Some(self.view), action);
        let text = if primary {
            RichText::new(label).color(self.palette.on_accent)
        } else {
            RichText::new(label).color(self.palette.ink)
        };
        let mut b = egui::Button::new(text);
        if primary && why.is_none() {
            b = b.fill(self.palette.accent);
        }
        let resp = ui.add_enabled(why.is_none(), b);
        self.view.navs.rects.push((label.to_owned(), resp.rect));
        let help = actions::entry(action).map_or("", |e| e.help);
        let clicked = resp.clicked();
        if let Some(w) = why {
            resp.on_disabled_hover_text(w);
        } else {
            resp.on_hover_text(help);
        }
        if clicked {
            self.view.pending.push(action);
        }
        clicked
    }

    fn layer_toggle(&mut self, ui: &mut egui::Ui, name: &str, colour: Color32, label: &str) {
        let mut on = self.view.navs.shown(name);
        ui.horizontal(|ui| {
            swatch(ui, colour);
            if ui
                .checkbox(&mut on, RichText::new(label).color(self.palette.ink))
                .changed()
            {
                if on {
                    self.view.navs.hidden.remove(name);
                } else {
                    self.view.navs.hidden.insert(name.to_owned());
                }
                self.view.navs.dirty = true;
            }
        });
    }

    /// The Navigation panel.
    pub(super) fn navigation(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        self.view.navs.rects.clear();
        let status = if self.view.navs.reading() {
            "reading ...".to_owned()
        } else {
            self.view.navs.data().map_or_else(String::new, |d| {
                format!("{} file(s), {:.1} s", d.files.len(), d.seconds)
            })
        };
        self.header(ui, "Navigation", &status);
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_2 as i8))
                .show(ui, |ui| {
                    if !self.view.nav_active() {
                        muted(
                            ui,
                            &p,
                            "The bots' navigation is drawn in the AI workspace and in the navmesh view mode.",
                        );
                        self.nav_button(ui, Action::SetViewMode(ViewMode::Navmesh), "Show the navmesh", true);
                        return;
                    }
                    match &self.view.navs.loaded {
                        None => {
                            muted(ui, &p, "Reading the level's path data ...");
                            return;
                        }
                        Some(Err(e)) => {
                            status_line(ui, &p, "Information", p.status_info, &e.to_string());
                        }
                        Some(Ok(_)) => {}
                    }
                    self.nav_files(ui);
                    self.nav_colouring(ui);
                    self.nav_layers(ui);
                    self.nav_selection(ui);
                    self.nav_path_test(ui);
                    self.nav_anomalies(ui);
                    self.nav_compare(ui);
                });
        });
        if std::mem::take(&mut self.view.navs.dirty) {
            self.view.nav_refresh(self.gpu, &p);
        }
    }

    fn nav_files(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let Some(d) = self.view.navs.data() else { return };
        title(ui, &p, "Files");
        for f in &d.files {
            let s = navdata::summary(&f.data);
            let label = if f.name == "DICE" {
                f.data
                    .path
                    .parent()
                    .and_then(Path::parent)
                    .and_then(Path::file_name)
                    .map_or_else(String::new, |n| format!("{} (game copy)", n.to_string_lossy()))
            } else {
                f.data
                    .path
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            };
            ui.label(
                RichText::new(format!("{}: {label}", f.name))
                    .text_style(style("body-strong"))
                    .color(p.ink),
            )
            .on_hover_text(f.data.path.display().to_string());
            muted(
                ui,
                &p,
                &format!(
                    "{} areas ({} detailed), {} cells; {} nodes, {} edges, {} islands ({} large)",
                    group_digits(s.areas as u64),
                    group_digits(s.detailed_areas as u64),
                    group_digits(s.cells as u64),
                    group_digits(s.nodes as u64),
                    group_digits(s.edges as u64),
                    s.islands,
                    s.large_islands
                ),
            );
            if s.ladder_links > 0 {
                muted(
                    ui,
                    &p,
                    &format!(
                        "{} ladder links, {} cells of ladder ground",
                        s.ladder_links,
                        group_digits(s.ladder_ground_cells as u64)
                    ),
                );
            }
        }
        if let Some(df) = d.diff {
            ui.label(RichText::new(format!("DICE ground reproduced: {:.1} %", df.reproduced)).color(p.ink));
            ui.label(RichText::new(format!("NavKit extra ground: about +{:.1} % of DICE's", df.extra)).color(p.ink));
            if df.ladder > 0.0 {
                ui.label(RichText::new(format!("NavKit ladder ground: {:.2} %", df.ladder)).color(p.ink));
            }
            muted(ui, &p, "A cell estimate; navparity.py gives the exact figures.");
        }
    }

    fn nav_colouring(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        title(ui, &p, "Colour the ground by");
        ui.horizontal_wrapped(|ui| {
            for mode in ColourMode::ALL {
                let action = Action::NavColour(mode);
                let why = why_not_in(Some(self.view), action);
                let active = self.view.navs.colour == mode;
                let r = ui.add_enabled(why.is_none(), egui::Button::selectable(active, mode.name()));
                self.view.navs.rects.push((format!("colour {}", mode.name()), r.rect));
                let clicked = r.clicked();
                if let Some(w) = why {
                    r.on_disabled_hover_text(w);
                } else {
                    r.on_hover_text(mode.help());
                }
                if clicked {
                    self.view.pending.push(action);
                }
            }
        });
    }

    fn nav_layers(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let Some(d) = self.view.navs.data() else { return };
        let files: Vec<(&'static str, bool, bool)> = d
            .files
            .iter()
            .map(|f| {
                (
                    f.name,
                    f.data.areas.iter().any(|a| a.ladder),
                    f.data.ladder_edge.iter().any(|l| *l),
                )
            })
            .collect();
        let both = d.same.is_some();
        let anomalies = d.anomalies.is_some();
        title(ui, &p, "Layers");
        for (name, ladder_ground, ladder_links) in files {
            for layer in FileLayer::ALL {
                if (layer == FileLayer::LadderGround && !ladder_ground)
                    || (layer == FileLayer::LadderLinks && !ladder_links)
                {
                    continue;
                }
                let n = format!("{name} {}", layer.name());
                self.layer_toggle(ui, &n, layer.colour(&p), &n);
            }
        }
        if both {
            self.layer_toggle(
                ui,
                "DICE ground missing in NavKit",
                p.view_nav_missing,
                "DICE ground missing in NavKit",
            );
        }
        if anomalies {
            for (n, c) in [
                ("anomaly: spawners on small islands", p.view_status_error),
                ("anomaly: orders without a node", p.view_status_warn),
                ("anomaly: sliver areas", p.view_nav_border),
                ("anomaly: lonely nodes", p.view_nav_small),
            ] {
                self.layer_toggle(ui, n, c, n);
            }
        }
    }

    fn nav_selection(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let v = &*self.view;
        let Some(d) = v.navs.data() else { return };
        // a soldier spawn selected: its island in each file
        let spawn = v
            .selected
            .and_then(|k| v.navs.spawn_list.iter().position(|s| *s == k).map(|i| (k, i)));
        if let Some((k, i)) = spawn {
            title(ui, &p, "Selected spawn");
            if let Some(e) = v.scene().entities.get(k) {
                ui.label(RichText::new(model::display_name(e)).color(p.ink));
            }
            for f in &d.files {
                let text = match f.spawns.get(i) {
                    Some((Some(isl), Some(dist))) => format!(
                        "{}: island {isl}{}, route node {dist:.1} m away",
                        f.name,
                        if *isl == 0 { " (main network)" } else { "" }
                    ),
                    _ => format!("{}: no route node within 15 m", f.name),
                };
                ui.label(RichText::new(text).color(p.ink));
            }
        }
        let Some((name, k)) = v.navs.selected else { return };
        let Some(f) = d.file(name) else { return };
        let Some(a) = f.data.areas.get(k) else { return };
        title(ui, &p, "Selected area");
        ui.label(
            RichText::new(format!(
                "{name} area {k}: sector 0x{:04X}, area {}, tile {},{}, layer {}",
                a.sector, a.index, a.tile.0, a.tile.1, a.layer
            ))
            .color(p.ink),
        );
        muted(
            ui,
            &p,
            &format!(
                "pitch {:.2} m{}, {} cells, height {:.1} - {:.1} m; {} outline lines, {} portals, {} border portals",
                a.pitch,
                if a.detailed() { " (detailed zone)" } else { "" },
                group_digits(a.cells as u64),
                a.min_y,
                a.max_y,
                a.outline.len(),
                a.portals.len(),
                a.borders.len()
            ),
        );
        if a.ladder {
            ui.horizontal(|ui| {
                swatch(ui, p.view_nav_ladder);
                ui.label(RichText::new("Ladder ground: reached only through a ladder link").color(p.ink));
            });
        }
        if let Ok(i) = usize::try_from(a.island)
            && let Some(isl) = f.data.islands.get(i)
        {
            muted(
                ui,
                &p,
                &format!(
                    "island {i}{}: {} nodes, {} cells",
                    if i == 0 { " (main network)" } else { "" },
                    group_digits(isl.nodes as u64),
                    group_digits(isl.cells as u64)
                ),
            );
        }
    }

    fn nav_path_test(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        title(ui, &p, "Path test");
        ui.horizontal(|ui| {
            let label = if self.view.navs.path.active {
                "Path test on"
            } else {
                "Path test"
            };
            self.nav_button(ui, Action::PathTest, label, self.view.navs.path.active);
            if ui.button("Clear").clicked() {
                self.view.navs.path = nav::PathTest::default();
                self.view.navs.dirty = true;
            }
        });
        let v = &*self.view;
        for (label, q) in ["start", "end"].iter().zip(&v.navs.path.points) {
            muted(ui, &p, &format!("{label}: {:.1}, {:.1}, {:.1}", q[0], q[1], q[2]));
        }
        let mut fly: Option<V3> = None;
        for (name, r) in &v.navs.path.routes {
            let colour = if *name == "NavKit" {
                p.view_nav_route_navkit
            } else {
                p.view_nav_route_dice
            };
            ui.horizontal(|ui| {
                swatch(ui, colour);
                ui.label(RichText::new(*name).text_style(style("body-strong")).color(p.ink));
            });
            if r.found {
                ui.label(
                    RichText::new(format!(
                        "cost {:.1} m over {} nodes (length {:.1} m, straight {:.1} m, x{:.2})",
                        r.cost,
                        r.nodes.len(),
                        r.length,
                        r.straight,
                        r.cost / r.straight.max(1e-9)
                    ))
                    .color(p.ink),
                );
                muted(
                    ui,
                    &p,
                    &format!(
                        "joined at nodes {:.1} m and {:.1} m from the points; island {}",
                        r.start_join,
                        r.end_join,
                        r.start_island.unwrap_or(0)
                    ),
                );
            } else {
                ui.label(RichText::new(format!("No route: {}", r.reason)).color(p.ink));
                if let Some((a, b, _)) = r.gap
                    && let Some(f) = v.navs.data().and_then(|d| d.file(name))
                    && let (Some(x), Some(y)) = (f.data.node_pos.get(a), f.data.node_pos.get(b))
                    && ui.small_button("Fly to the gap").clicked()
                {
                    fly = Some([
                        f64::midpoint(x[0], y[0]),
                        f64::midpoint(x[1], y[1]) - Y_OFFSET,
                        f64::midpoint(x[2], y[2]),
                    ]);
                }
            }
        }
        if let Some(c) = v.navs.cost_change() {
            ui.label(RichText::new(format!("NavKit's route costs {c:+.1} % against DICE's")).color(p.ink));
        }
        muted(
            ui,
            &p,
            "Over the route graph only; the game also smooths the route through the areas.",
        );
        if let Some(at) = fly {
            self.fly_to(at, 40.0);
        }
    }

    fn fly_to(&mut self, at: V3, distance: f64) {
        #[allow(clippy::iter_over_hash_type, reason = "every viewport is focused alike")]
        for vp in self.view.viewports.values_mut() {
            vp.camera.focus(at, Some(distance));
            vp.dirty = true;
        }
    }

    fn nav_anomalies(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let Some(d) = self.view.navs.data() else { return };
        let (Some(an), Some(nk)) = (&d.anomalies, d.file("NavKit")) else {
            return;
        };
        title(ui, &p, "Anomaly overlay");
        muted(
            ui,
            &p,
            &format!(
                "{} small islands (under 25 nodes); colour by anomaly: the main network, islands of 25+ nodes, smaller \
                 ones. Read against DICE's own file, not as absolutes.",
                an.small_islands.len()
            ),
        );
        let counts = [
            ("anomaly: spawners on small islands", an.spawners_on_small_islands.len()),
            ("anomaly: orders without a node", an.orders_without_node.len()),
            ("anomaly: sliver areas", an.sliver_areas.len()),
            ("anomaly: lonely nodes", an.lonely_nodes.len()),
        ];
        let report = nav::anomaly_report(&self.view.lv, nk, an);
        for (n, c) in counts {
            muted(ui, &p, &format!("{}: {c}", n.trim_start_matches("anomaly: ")));
        }
        if ui
            .button("Export JSON")
            .on_hover_text("Writes work\\navstudio\\<level>_anomalies.json")
            .clicked()
            && let Some(root) = self.view.nav_root()
        {
            match nav::save_export(&root, &format!("{}_anomalies.json", self.view.lv), &report) {
                Ok(path) => self.view.navs.anomaly_saved = Some(path),
                Err(e) => self.view.navs.message = e.to_string(),
            }
        }
        if let Some(s) = &self.view.navs.anomaly_saved {
            muted(ui, &p, &s.display().to_string());
        }
    }

    fn nav_compare(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        title(ui, &p, "Compare with a reference AI layer");
        muted(
            ui,
            &p,
            "Another level's AI layer on this level's terrain: level:<level>, xbox:<level> (DICE's Xbox 360 Onslaught \
             levels) or a level partition's path.",
        );
        let mut open = false;
        ui.horizontal(|ui| {
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.view.navs.compare_input)
                    .hint_text("level:ons_mp_002")
                    .desired_width(180.0),
            );
            if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                open = true;
            }
            if ui.button("Compare").clicked() {
                open = true;
            }
            if self.view.navs.compare.is_some() && ui.button("Clear").clicked() {
                self.view.navs.compare = None;
                self.view.navs.dirty = true;
            }
        });
        if open && let Some(root) = self.view.nav_root() {
            match nav::open_compare(&self.view.navs.compare_input, &root) {
                Ok(c) => {
                    self.view.navs.compare = Some(c);
                    self.view.navs.compare_error = None;
                    self.view.navs.dirty = true;
                }
                Err(e) => self.view.navs.compare_error = Some(e.to_string()),
            }
        }
        if let Some(e) = &self.view.navs.compare_error {
            status_line(ui, &p, "Error", p.status_error, e);
        }
        let Some(c) = &self.view.navs.compare else { return };
        let l = &c.layer;
        ui.label(RichText::new(&c.label).color(p.ink));
        muted(
            ui,
            &p,
            &format!(
                "HasAI {}; SectorOffsets {}",
                l.has_ai.map_or("none".to_owned(), |b| b.to_string()),
                c.offsets.map_or("none".to_owned(), |(x, z)| format!("{x}, {z}"))
            ),
        );
        muted(
            ui,
            &p,
            &format!(
                "{} spawners ({} AI enabled), {} area orders, {} patrols, {} objectives",
                l.spawners.len(),
                l.spawners
                    .iter()
                    .filter(|s| s.enabled && s.player_type.as_deref() == Some("SSAiPlayer"))
                    .count(),
                l.area_orders.len(),
                l.patrols.len(),
                l.objectives.len()
            ),
        );
        for (label, colour, names) in [
            (
                "AI spawners",
                p.view_ref_ai,
                &["reference AI spawners (enabled)", "reference AI spawners (disabled)"][..],
            ),
            (
                "human spawners",
                p.view_ref_human,
                &[
                    "reference human spawners (enabled)",
                    "reference human spawners (disabled)",
                ][..],
            ),
            (
                "orders",
                p.view_ref_order,
                &["reference area orders", "reference patrol orders"][..],
            ),
            ("objectives", p.view_ref_objective, &["reference objectives"][..]),
            ("path data", p.view_ref_outline, &["reference path data outlines"][..]),
        ] {
            let mut on = names.iter().all(|n| self.view.navs.shown(n));
            ui.horizontal(|ui| {
                swatch(ui, colour);
                if ui.checkbox(&mut on, RichText::new(label).color(p.ink)).changed() {
                    for n in names {
                        if on {
                            self.view.navs.hidden.remove(*n);
                        } else {
                            self.view.navs.hidden.insert((*n).to_owned());
                        }
                    }
                    self.view.navs.dirty = true;
                }
            });
        }
        let dis = [
            "reference AI spawners (disabled)",
            "reference human spawners (disabled)",
        ];
        let mut only = dis.iter().all(|n| !self.view.navs.shown(n));
        if ui
            .checkbox(&mut only, RichText::new("Enabled spawners only").color(p.ink))
            .changed()
        {
            for n in dis {
                if only {
                    self.view.navs.hidden.insert(n.to_owned());
                } else {
                    self.view.navs.hidden.remove(n);
                }
            }
            self.view.navs.dirty = true;
        }
        let Some(c) = &self.view.navs.compare else { return };
        if let Some((k, near)) = c.selected
            && let Some(it) = c.items().get(k)
        {
            ui.label(RichText::new(format!("reference {}: {}", it.kind, it.name)).color(p.ink));
            muted(
                ui,
                &p,
                &format!("at {:.1}, {:.1}, {:.1}", it.pos[0], it.pos[1], it.pos[2]),
            );
            match near {
                _ if self.view.navs.data().and_then(|d| d.file("NavKit")).is_none() => {
                    muted(ui, &p, "No NavKit build of this level");
                }
                None => muted(ui, &p, "No NavKit route node on its floor"),
                Some((dist, _, size, main)) => muted(
                    ui,
                    &p,
                    &format!(
                        "Nearest NavKit route node on its floor: {dist:.1} m ({} R = 15 m), in a network of {} nodes{}",
                        if dist <= 15.0 { "within" } else { "beyond" },
                        group_digits(size as u64),
                        if main { " (the main one)" } else { "" }
                    ),
                ),
            }
        }
        if ui
            .button("Export JSON")
            .on_hover_text("Writes work\\navstudio\\<level>_vs_<source>.json")
            .clicked()
            && let Some(root) = self.view.nav_root()
        {
            let nk = self.view.navs.data().and_then(|d| d.file("NavKit"));
            let rep = nav::compare_report(&self.view.lv, c, nk);
            let slug: String = c
                .spec
                .to_lowercase()
                .chars()
                .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
                .collect();
            let name = format!("{}_vs_{slug}.json", self.view.lv);
            match nav::save_export(&root, &name, &rep) {
                Ok(path) => {
                    if let Some(c) = self.view.navs.compare.as_mut() {
                        c.saved = Some(path);
                    }
                }
                Err(e) => self.view.navs.compare_error = Some(e.to_string()),
            }
        }
        if let Some(s) = self.view.navs.compare.as_ref().and_then(|c| c.saved.as_ref()) {
            muted(ui, &p, &s.display().to_string());
        }
    }

    /// The Islands and spawns panel.
    pub(super) fn nav_islands(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let v = &*self.view;
        let Some(d) = v.navs.data() else {
            self.header(ui, "Islands and spawns", "");
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_3 as i8))
                .show(ui, |ui| {
                    muted(
                        ui,
                        &p,
                        "Show the navmesh (or open the AI workspace) on a level with path data to see its islands.",
                    );
                });
            return;
        };
        let k = v.navs.islands_file.min(d.files.len().saturating_sub(1));
        let Some(f) = d.files.get(k) else { return };
        self.header(ui, "Islands and spawns", &format!("{} islands", f.data.islands.len()));
        let mut pick = None;
        let mut fly: Option<(V3, f64)> = None;
        let mut select = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new()
                .inner_margin(Margin::same(m.space_2 as i8))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        for (i, file) in d.files.iter().enumerate() {
                            if ui.add(egui::Button::selectable(i == k, file.name)).clicked() {
                                pick = Some(i);
                            }
                        }
                    });
                    let bad: Vec<(usize, Option<u32>, Option<f64>)> = f
                        .spawns
                        .iter()
                        .enumerate()
                        .filter(|(_, (isl, _))| *isl != Some(0))
                        .map(|(i, (isl, dd))| (i, *isl, *dd))
                        .collect();
                    ui.label(
                        RichText::new(format!(
                            "{} soldier spawns; {} not on the main network",
                            f.spawns.len(),
                            bad.len()
                        ))
                        .color(p.ink),
                    );
                    for (i, isl, dd) in bad.iter().take(100) {
                        let Some(&ek) = v.navs.spawn_list.get(*i) else { continue };
                        let name = v.scene().entities.get(ek).map_or_else(String::new, model::display_name);
                        ui.horizontal(|ui| {
                            if ui.small_button("Fly").clicked() {
                                select = Some(ek);
                            }
                            let what = match (isl, dd) {
                                (Some(isl), Some(dist)) => format!("island {isl} (route node {dist:.0} m away)"),
                                _ => "no route node within 15 m".to_owned(),
                            };
                            ui.label(RichText::new(format!("{name}: {what}")).color(p.ink));
                        });
                    }
                    title(ui, &p, "Islands, largest first (island 0 is the main network)");
                    let gaps = f.gaps();
                    for (i, isl) in f.data.islands.iter().take(300).enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("Fly").clicked() {
                                fly = Some((
                                    [isl.centre[0], isl.centre[1] - Y_OFFSET, isl.centre[2]],
                                    if i == 0 { 300.0 } else { 40.0 },
                                ));
                            }
                            let gap = if i > 0 {
                                gaps.get(i)
                                    .map_or_else(String::new, |g| format!(", {g:.0} m from the main network"))
                            } else {
                                String::new()
                            };
                            ui.label(
                                RichText::new(format!(
                                    "{i}: {} nodes, {} cells{gap}",
                                    group_digits(isl.nodes as u64),
                                    group_digits(isl.cells as u64)
                                ))
                                .color(p.ink),
                            );
                        });
                    }
                });
        });
        if let Some(i) = pick {
            self.view.navs.islands_file = i;
        }
        if let Some((at, dist)) = fly {
            self.fly_to(at, dist);
        }
        if let Some(ek) = select {
            self.view.select(self.gpu, Some(ek), true);
            self.view.focus_selected();
        }
    }

    /// The Navigation edits panel.
    pub(super) fn nav_edits(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let m = self.metrics;
        let edits = self.view.nav_edits();
        self.header(ui, "Navigation edits", &format!("{} edit(s)", edits.len()));
        let mut drop: Option<(usize, NavEdit)> = None;
        let mut fly: Option<V3> = None;
        egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
            egui::Frame::new().inner_margin(Margin::same(m.space_2 as i8)).show(ui, |ui| {
                if self.view.doc.is_none() {
                    muted(
                        ui,
                        &p,
                        "Navigation edits are a project's: make one (New map from an existing map) or open one, then \
                         the AI workspace.",
                    );
                    return;
                }
                let kind = self.view.nav_kind();
                let side = self.view.nav_root().map(|r| nav::side_file(&r, &self.view.lv));
                let target = if kind == "dice" {
                    "DICE's data (only links, route edges, traversals and notes)"
                } else {
                    "NavKit's build (every edit; regions and seeds by a rebuild)"
                };
                muted(ui, &p, &format!("Editing {target}."));
                if let Some(s) = &side {
                    muted(ui, &p, &format!("Side file: {}", s.display()));
                }
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Recorded as").color(p.ink));
                    ui.add(egui::TextEdit::singleline(&mut self.view.navs.by).desired_width(140.0));
                });
                title(ui, &p, "Tools");
                ui.horizontal_wrapped(|ui| {
                    for t in NavTool::ALL {
                        let action = Action::NavTool(t);
                        let why = why_not_in(Some(self.view), action);
                        let r = ui.add_enabled(
                            why.is_none(),
                            egui::Button::selectable(self.view.navs.tool == Some(t), t.name()),
                        );
                        self.view.navs.rects.push((format!("tool {}", t.name()), r.rect));
                        let clicked = r.clicked();
                        if let Some(w) = why {
                            r.on_disabled_hover_text(w);
                        } else {
                            r.on_hover_text(t.how(self.view.navs.polygon));
                        }
                        if clicked {
                            self.view.pending.push(action);
                        }
                    }
                });
                let n = &mut self.view.navs;
                match n.tool {
                    Some(t @ (NavTool::Walkable | NavTool::Unwalkable)) => {
                        ui.horizontal(|ui| {
                            if ui.add(egui::Button::selectable(n.polygon, "Polygon")).clicked() {
                                n.polygon = true;
                                n.clicks.clear();
                            }
                            if ui.add(egui::Button::selectable(!n.polygon, "Box")).clicked() {
                                n.polygon = false;
                                n.clicks.clear();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Height range below / above (m)").color(p.ink));
                            ui.add(egui::DragValue::new(&mut n.margin[0]).speed(0.1).range(0.0..=50.0));
                            ui.add(egui::DragValue::new(&mut n.margin[1]).speed(0.1).range(0.0..=50.0));
                        });
                        muted(ui, &p, &format!("{} ({} point(s))", t.how(n.polygon), n.clicks.len()));
                    }
                    Some(NavTool::Traversal) => {
                        ui.horizontal(|ui| {
                            for (i, k) in navtune_kinds().iter().enumerate() {
                                if ui.add(egui::Button::selectable(n.traversal == i, *k)).clicked() {
                                    n.traversal = i;
                                }
                            }
                        });
                        ui.checkbox(&mut n.two_way, RichText::new("Two-way").color(p.ink));
                        muted(ui, &p, &format!("Click the bottom, then the top ({} of 2)", n.clicks.len()));
                    }
                    Some(NavTool::Note) => {
                        ui.add(egui::TextEdit::singleline(&mut n.note).hint_text("the note").desired_width(220.0));
                        muted(ui, &p, "Then click where it belongs");
                    }
                    Some(t @ (NavTool::EdgeAdd | NavTool::EdgeRemove)) => {
                        ui.checkbox(&mut n.two_way, RichText::new("Two-way").color(p.ink));
                        muted(ui, &p, &format!("{} ({} of 2)", t.how(true), n.clicks.len()));
                    }
                    Some(t) => muted(ui, &p, t.how(true)),
                    None => muted(ui, &p, "Choose a tool; clicks in the view then draw instead of selecting."),
                }
                if !n.message.is_empty() {
                    ui.add(egui::Label::new(RichText::new(n.message.clone()).color(p.ink)).wrap());
                }
                title(ui, &p, "Side file and build");
                ui.horizontal_wrapped(|ui| {
                    self.nav_button(ui, Action::Undo, "Undo", false);
                    self.nav_button(ui, Action::Redo, "Redo", false);
                    self.nav_button(ui, Action::NavSaveEdits, "Save side file", false);
                    self.nav_button(ui, Action::NavReloadEdits, "Reload side file", false);
                });
                let build = if kind == "dice" {
                    "Apply to a copy of DICE's file"
                } else {
                    "Build with these edits"
                };
                ui.horizontal_wrapped(|ui| {
                    self.nav_button(ui, Action::NavBuild, build, true);
                });
                if let Some(out) = self.view.nav_out() {
                    muted(ui, &p, &format!("\u{2192} {}", out.display()));
                }
                let job_state = self.view.navs.job.as_ref().map(|j| (j.what.clone(), j.outcome(), j.output()));
                if let Some((what, done, out)) = job_state {
                    let (word, colour, text) = match done {
                        None => ("Running", p.status_info, format!("navkit {what} ..."),),
                        Some(true) => ("Done", p.status_ok, format!("navkit {what}: done")),
                        Some(false) => ("Error", p.status_error, format!("navkit {what}: failed")),
                    };
                    status_line(ui, &p, word, colour, &text);
                    for l in out.iter().rev().take(6).rev() {
                        ui.label(RichText::new(l).text_style(style("small")).monospace().color(p.ink_muted));
                    }
                    if done == Some(true) && what == "apply" {
                        ui.horizontal_wrapped(|ui| {
                            if ui.button("Open the result").clicked() {
                                self.view.navs.open_result = true;
                            }
                        });
                    }
                    if let Some(out) = self.view.nav_out() {
                        muted(ui, &p, &format!("Report: {}.report.txt", out.display()));
                    }
                }
                ui.horizontal_wrapped(|ui| {
                    self.nav_button(ui, Action::NavInstall, "Install into the game\u{2026}", false);
                    self.nav_button(ui, Action::NavRestore, "Restore DICE's original\u{2026}", false);
                });
                title(ui, &p, &format!("{} edit(s)", edits.len()));
                for (k, e) in edits.iter().enumerate() {
                    ui.horizontal(|ui| {
                        if ui.small_button("Fly").clicked() {
                            fly = e.anchor();
                        }
                        let r = ui.small_button("\u{2715}").on_hover_text("Remove this edit (one operation; undo brings it back)");
                        self.view.navs.rects.push((format!("remove {k}"), r.rect));
                        if r.clicked() {
                            drop = Some((k, e.clone()));
                        }
                        ui.label(RichText::new(format!("{}. {}", k + 1, nav::summary(e))).color(p.ink));
                    });
                }
            });
        });
        let palette = p;
        if let Some((index, edit)) = drop {
            self.view.commit(self.gpu, &palette, Op::NavRemove { index, edit }, "");
            self.view.nav_refresh(self.gpu, &palette);
        }
        if let Some(at) = fly {
            self.fly_to(at, 30.0);
        }
        if std::mem::take(&mut self.view.navs.open_result) {
            self.view.nav_open_result(self.gpu, &palette);
        }
    }

    /// Over a view: the hover's area (the area under the pointer, NavKit's first).
    pub(super) fn nav_overlay(&mut self, ui: &egui::Ui, rect: Rect) {
        // Enter finishes a region's outline
        let typing = ui.ctx().memory(|mem| mem.focused().is_some());
        if !typing
            && ui.input(|i| i.key_pressed(Key::Enter))
            && let Some(t @ (NavTool::Walkable | NavTool::Unwalkable)) = self.view.navs.tool
            && self.view.navs.polygon
            && self.view.navs.clicks.len() >= 3
        {
            let palette = self.palette;
            self.view.nav_try_commit(self.gpu, &palette, t, true);
            self.view.nav_refresh(self.gpu, &palette);
        }
        if !self.view.nav_active() || self.view.navs.data().is_none() {
            return;
        }
        let Some(h) = ui.input(|i| i.pointer.hover_pos()).filter(|p| rect.contains(*p)) else {
            self.view.navs.hover = None;
            return;
        };
        let moved = ui.input(|i| i.pointer.delta() != Vec2::ZERO);
        if moved && let Some((o, d)) = self.view.menus.pointer_ray {
            let at = self.view.nav_ray_point(o, d);
            self.view.navs.hover = at
                .and_then(|p| self.view.navs.area_at(p))
                .and_then(|(f, k)| self.view.navs.describe_area(f, k));
        }
        if let Some(text) = &self.view.navs.hover {
            let p = self.palette;
            let painter = ui.painter().with_clip_rect(rect);
            let g = painter.layout_no_wrap(text.clone(), FontId::proportional(12.0), p.ink);
            let r = Rect::from_min_size(h + Vec2::new(14.0, 14.0), g.size()).expand(4.0);
            painter.rect_filled(r, CornerRadius::same(3), p.scene_label);
            painter.galley(r.min + Vec2::splat(4.0), g, p.ink);
        }
    }

    /// The reference marker nearest a screen point (within 12 points), among the shown ones.
    pub(super) fn nav_marker_at(&self, rect: Rect, cam: &Camera, at: Pos2) -> Option<usize> {
        let c = self.view.navs.compare.as_ref()?;
        let view = Tabs::view_of(rect, cam);
        let mut best: Option<(f32, usize)> = None;
        for (k, it) in c.items().iter().enumerate() {
            if !self.view.navs.shown(it.layer) {
                continue;
            }
            let Some(s) = view.project(up(it.pos, 0.9)) else {
                continue;
            };
            let d = (rect.min + Vec2::new(s[0] as f32, s[1] as f32)).distance(at);
            if d < 12.0 && best.is_none_or(|(b, _)| d < b) {
                best = Some((d, k));
            }
        }
        best.map(|(_, k)| k)
    }
}

/// The traversal kinds, as the panel offers them.
fn navtune_kinds() -> [&'static str; 4] {
    bc2_navbuild::navtune::TRAVERSALS
}

impl EditorApp {
    /// The confirm dialog of an install or restore plan: what it will do and which of the game copy's files it
    /// touches; Install / Restore runs it, Cancel drops it.
    pub(super) fn nav_confirm_modal(&mut self, ctx: &egui::Context, p: &Palette) {
        let Mode::View(v) = &mut self.mode else { return };
        let Some(plan) = v.navs.confirm.clone() else { return };
        let mut answer: Option<bool> = None;
        let mut rects = Vec::new();
        egui::Modal::new(egui::Id::new("nav confirm")).show(ctx, |ui| {
            ui.set_width(560.0);
            let verb = if plan.what == "install" { "Install" } else { "Restore" };
            ui.label(
                RichText::new(format!("{verb} the AI path data of {}?", v.lv.to_uppercase()))
                    .text_style(style("title"))
                    .color(p.ink),
            );
            ui.add_space(6.0);
            plan_lines(ui, p, &plan);
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                let ok = ui.add_enabled(
                    plan.blocked.is_none(),
                    egui::Button::new(RichText::new(verb).color(p.on_accent)).fill(p.accent),
                );
                rects.push(("confirm".to_owned(), ok.rect));
                if ok.clicked() {
                    answer = Some(true);
                }
                let no = ui.button("Cancel");
                rects.push(("cancel".to_owned(), no.rect));
                if no.clicked() {
                    answer = Some(false);
                }
            });
        });
        v.navs.rects.extend(rects);
        if let Some(a) = answer {
            v.nav_confirm(a);
            // the installed file changed: read the level's path data again
            v.navs.reset();
        }
    }
}

/// A plan's lines and the game copy's files it touches.
fn plan_lines(ui: &mut egui::Ui, p: &Palette, plan: &Plan) {
    if let Some(b) = &plan.blocked {
        status_line(ui, p, "Warning", p.status_warn, b);
        return;
    }
    for l in &plan.lines {
        ui.label(RichText::new(format!("\u{2022} {l}")).color(p.ink));
    }
    ui.add_space(4.0);
    ui.label(caption("Writes in the game copy", p.ink_muted));
    for t in &plan.touches {
        ui.label(
            RichText::new(t.display().to_string())
                .text_style(style("small"))
                .monospace()
                .color(p.ink_muted),
        );
    }
}
