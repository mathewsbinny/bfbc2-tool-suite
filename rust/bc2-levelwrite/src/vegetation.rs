//! Vegetation, rocks and props on a new terrain (round TG4, spec `transforms/terrain_gen.md` TGEN-VEG-1 - 4): what
//! DICE's levels place as model-group members, read per level, and a placement of the generator's
//! ([`bc2_terragen::vegetation`]) written into the level's own groups as member edits (round SG1's
//! [`crate::files::FileEdit`]s), so the export's group staging, cull tree, physics and checks apply unchanged.
//!
//! **Kinds.** A member's kind comes from its group and its blueprint ([`kind_of`]): every member of a forest
//! (`ForestEntityData`) is a tree, of a middlegrowth group a bush; a static or rigid group's member is a rock when its
//! blueprint's name says so (rock, cliff, stone, boulder: TGEN-BIOME-2's rule) and a prop when its blueprint is
//! natural ground clutter (under `vegetation/`, `objects/vegetation/` or a terrain's own folder: stumps, logs, dead
//! trees, debris of plants). Everything else (buildings, man-made props, gameplay) is left where the base has it.

use std::collections::BTreeMap;

use bc2_terragen::vegetation::Kind;

use crate::export::{Found, Game, read_found};
use crate::members::{self, GroupEntity};
use crate::ops::Rows;
use crate::write::{LevelWriteError, LevelWriter, Result};

fn bad(why: impl Into<String>) -> LevelWriteError {
    LevelWriteError::Refused(why.into())
}

/// A member's kind from its group entity's type (`Entity.ForestEntityData` ...) and its `Assets` reference, `None`
/// when the vegetation step leaves it where it is.
#[must_use]
pub fn kind_of(group_type: &str, asset: &str) -> Option<Kind> {
    if group_type.ends_with(".ForestEntityData") {
        return Some(Kind::Forest);
    }
    if group_type.ends_with("MiddlegrowthModelGroupEntityData") {
        return Some(Kind::Middlegrowth);
    }
    let low = asset.to_ascii_lowercase();
    let name = members::asset_name(asset).to_ascii_lowercase();
    if ["rock", "cliff", "stone", "boulder"].iter().any(|w| name.contains(w)) {
        return Some(Kind::Rock);
    }
    if low.starts_with("vegetation/") || low.starts_with("objects/vegetation/") || low.starts_with("terrains/") {
        return Some(Kind::Prop);
    }
    None
}

/// One model group of a level with its members as its file holds them.
#[derive(Debug, Clone)]
pub struct Group {
    /// The group entity's type (`Entity.ForestEntityData` ...).
    pub class: String,
    /// The group file's name in the level's folder, lower case.
    pub file: String,
    /// The group instance's guid in its file (upper case).
    pub instance: String,
    /// Per member type (the partition's order): its first place and its instance count.
    pub runs: Vec<(usize, usize)>,
    /// Each member in file order: its `Assets` reference and its transform.
    pub members: Vec<(String, Rows)>,
}

impl Group {
    /// The member type holding the member at `place`.
    #[must_use]
    pub fn type_at(&self, place: usize) -> Option<usize> {
        self.runs.iter().position(|&(f, c)| (f..f + c).contains(&place))
    }

    /// The kind of each member type (its first member's), `None` for types the vegetation step leaves.
    #[must_use]
    pub fn type_kinds(&self) -> Vec<Option<Kind>> {
        self.runs
            .iter()
            .map(|&(f, _)| self.members.get(f).and_then(|(a, _)| kind_of(&self.class, a)))
            .collect()
    }
}

/// The group instance of a group file: its first instance with `Assets`.
#[must_use]
pub fn group_instance(f: &LevelWriter) -> Option<String> {
    f.d.instances()
        .into_iter()
        .find(|&e| f.d.child(e, b"Assets").is_some())
        .and_then(|e| f.d.attr(e, b"guid"))
        .map(|g| String::from_utf8_lossy(g).to_ascii_uppercase())
}

/// Every model group of a partition with its members, each group file read by `file` (its lower-case name in the
/// level's folder); groups whose file is not there are left out.
///
/// # Errors
/// A group file that does not parse, or whose members its member types do not count.
pub fn groups(w: &LevelWriter, file: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Vec<Group>> {
    let mut out = Vec::new();
    for g in members::group_entities(w) {
        if let Some(x) = group_of(w, &g, file)? {
            out.push(x);
        }
    }
    Ok(out)
}

fn group_of(w: &LevelWriter, g: &GroupEntity, file: &dyn Fn(&str) -> Option<Vec<u8>>) -> Result<Option<Group>> {
    let name = members::file_of(&g.name);
    let Some(bytes) = file(&name) else {
        return Ok(None);
    };
    let f = LevelWriter::parse(&bytes)?;
    let Some(instance) = group_instance(&f) else {
        return Ok(None);
    };
    let runs: Vec<(usize, usize)> = members::runs(w, g)?.iter().map(|r| (r.first, r.count)).collect();
    let n = members::member_count(&f, &instance)?;
    if runs.iter().map(|r| r.1).sum::<usize>() != n {
        return Err(bad(format!(
            "{name}: its member types count other than its {n} members"
        )));
    }
    let mut members = Vec::with_capacity(n);
    for k in 0..n {
        let asset = members::asset_at(&f, &instance, k).unwrap_or_default();
        let rows = crate::files::member_transform(&f, &instance, k)
            .ok_or_else(|| bad(format!("{name}: member {k} has no transform")))?;
        members.push((asset, rows));
    }
    Ok(Some(Group {
        class: w.type_of(g.entity),
        file: name,
        instance,
        runs,
        members,
    }))
}

/// A level of the game copy as the vegetation statistics read it: its partition, its groups, its terrain's main
/// height field and its water level (the water mesh's median vertex height).
#[derive(Debug, Clone)]
pub struct LevelGround {
    /// The level (lower case).
    pub level: String,
    /// The terrain's stem (`terrains/mp_005/mp_005`).
    pub terrain: String,
    /// The partition.
    pub partition: LevelWriter,
    /// The groups.
    pub groups: Vec<Group>,
    /// The main height field.
    pub heights: bc2_formats::terrain::Heightfield,
    /// The water level, when the terrain has a water mesh.
    pub water: Option<f64>,
}

impl LevelGround {
    /// A level of the game copy.
    ///
    /// # Errors
    /// The level's partition, groups or terrain do not read, or it names no terrain.
    pub fn read(game: &Game, lv: &str) -> crate::export::Result<Self> {
        let low = lv.to_ascii_lowercase();
        let part = crate::export::level_partition(game, &low)?;
        let w = LevelWriter::parse(&part)?;
        let hf = crate::export::heightfield_name(&w).ok_or_else(|| bad(format!("{low} names no terrain")))?;
        // (a LevelWriteError's `?` converts into the export's error)
        let stem = hf.strip_suffix(".heightfield-000.res").unwrap_or(&hf).to_owned();
        let level_live = game.live(&game.loaded(&low, "level"))?;
        let want: Vec<String> = members::group_entities(&w)
            .iter()
            .map(|g| format!("levels/{low}/{}", members::file_of(&g.name)))
            .collect();
        let found: Vec<(&String, &Found)> = want.iter().filter_map(|n| level_live.get(n).map(|f| (n, f))).collect();
        let data = read_found(&found.iter().map(|x| x.1).collect::<Vec<_>>())?;
        let files: BTreeMap<String, Vec<u8>> = found
            .iter()
            .map(|x| x.0.rsplit('/').next().unwrap_or_default().to_owned())
            .zip(data)
            .collect();
        let groups = groups(&w, &|n| files.get(n).cloned())?;
        let mut bundles = game.loaded(&low, "terrain");
        bundles.extend(game.loaded(&low, "level"));
        let live = game.live(&bundles)?;
        let get = |n: &str| -> crate::export::Result<Option<Vec<u8>>> {
            match live.get(n) {
                Some(f) => Ok(read_found(&[f])?.pop()),
                None => Ok(None),
            }
        };
        let main = get(&hf)?.ok_or_else(|| bad(format!("{low}: {hf} is in none of its bundles")))?;
        let heights = bc2_formats::terrain::parse(&main).map_err(|e| bad(format!("{hf}: {e}")))?;
        let water = get(&format!("{stem}.water.mesh.res"))?
            .and_then(|m| bc2_formats::water::parse_mesh(&m).ok())
            .and_then(|m| {
                let mut ys: Vec<f32> = m.vertices.iter().map(|v| v[1]).collect();
                ys.sort_by(f32::total_cmp);
                ys.get(ys.len() / 2).map(|y| f64::from(*y))
            });
        Ok(Self {
            level: low,
            terrain: stem,
            partition: w,
            groups,
            heights,
            water,
        })
    }
}

/// The ground of a main height field and a water level.
#[must_use]
pub fn ground_of(h: &bc2_formats::terrain::Heightfield, water: Option<f64>) -> bc2_terragen::vegetation::Ground {
    let (cx, _) = h.cell();
    bc2_terragen::vegetation::Ground {
        n: h.size as usize,
        cell: cx,
        x0: f64::from(h.min[0]),
        z0: f64::from(h.min[2]),
        heights: h.heights(),
        water,
    }
}

/// DICE's levels of the game copy: the level folders of `Dist\win32\levels` and `Package\levels` whose level bundle
/// is dated before 2013 (OPFOR's own ports are newer), the shared `*_common` folders left out, each name once, sorted.
#[must_use]
pub fn dice_levels(game: &Game) -> Vec<String> {
    // 2013-01-01 in hours since 1970
    let before = std::time::UNIX_EPOCH + std::time::Duration::from_hours(376_944);
    let mut out: Vec<String> = [
        game.root.join("Dist").join("win32").join("levels"),
        game.root.join("Package").join("levels"),
    ]
    .iter()
    .filter_map(|l| std::fs::read_dir(l).ok())
    .flat_map(Iterator::flatten)
    .filter(|e| {
        e.path()
            .join("level-00.fbrb")
            .metadata()
            .and_then(|m| m.modified())
            .is_ok_and(|t| t < before)
    })
    .map(|e| e.file_name().to_string_lossy().to_ascii_lowercase())
    .filter(|n| !n.ends_with("_common"))
    .collect();
    out.sort();
    out.dedup();
    out
}

impl LevelGround {
    /// The terrain's name (`mp_005`).
    #[must_use]
    pub fn terrain_name(&self) -> &str {
        self.terrain.rsplit('/').next().unwrap_or(&self.terrain)
    }

    /// The ground for the statistics.
    #[must_use]
    pub fn ground(&self) -> bc2_terragen::vegetation::Ground {
        ground_of(&self.heights, self.water)
    }

    /// Every member the vegetation step places, with its kind and its member type numbered over the level's groups.
    #[must_use]
    pub fn instances(&self) -> Vec<bc2_terragen::vegetation::Instance> {
        let mut out = Vec::new();
        let mut ty0 = 0;
        for g in &self.groups {
            let kinds = g.type_kinds();
            for (place, (_, rows)) in g.members.iter().enumerate() {
                let Some(t) = g.type_at(place) else { continue };
                if let Some(Some(kind)) = kinds.get(t) {
                    out.push(bc2_terragen::vegetation::Instance {
                        kind: *kind,
                        ty: ty0 + t,
                        rows: *rows,
                    });
                }
            }
            ty0 += g.runs.len();
        }
        out
    }
}

/// One of DICE's multiplayer terrains as the biome statistics measure it.
#[derive(Debug, Clone)]
pub struct TerrainStats {
    /// The terrain (`mp_005`).
    pub terrain: String,
    /// Its biome (TGEN-BIOME-1).
    pub biome: bc2_terragen::Biome,
    /// The level measured: the terrain's multiplayer level with the most forest and middlegrowth instances.
    pub level: String,
    /// The terrain's multiplayer levels, whose combat areas together are its playable area.
    pub levels: Vec<String>,
    /// The statistics.
    pub stats: bc2_terragen::vegetation::LevelStats,
}

/// Whether a level is a multiplayer one (`mp_*`, `bc1_*`, `nam_*`: not a campaign `sp_*` level).
#[must_use]
pub fn is_multiplayer(lv: &str) -> bool {
    !lv.to_ascii_lowercase().starts_with("sp_")
}

/// The distance outside the union of `polys` (0 inside); with no polygon, the distance outside the central square of
/// half the terrain's side (TGEN-PLAY-1: DICE's combat areas lie near the middle).
pub fn outside_fn(polys: Vec<Vec<[f64; 2]>>, side: f64) -> impl Fn(f64, f64) -> f64 + Sync {
    move |x: f64, z: f64| {
        if polys.is_empty() {
            bc2_terragen::vegetation::central(side)(x, z)
        } else {
            crate::validate::outside_by([x, z], &polys)
        }
    }
}

/// A multiplayer level of a terrain: its name, its forest and middlegrowth instances, its combat areas.
type TerrainLevel = (String, usize, Vec<Vec<[f64; 2]>>);

/// DICE's multiplayer terrains of the game copy with their vegetation statistics (TGEN-VEG-2): per terrain of
/// TGEN-BIOME-1's table that has multiplayer levels, its level with the most forest and middlegrowth instances
/// measured on the terrain's ground, the playable area the union of all its multiplayer levels' combat areas.
///
/// # Errors
/// A level that does not read.
pub fn dice_terrain_stats(game: &Game) -> crate::export::Result<Vec<TerrainStats>> {
    let mut by_terrain: BTreeMap<String, Vec<TerrainLevel>> = BTreeMap::new();
    for lv in dice_levels(game).into_iter().filter(|l| is_multiplayer(l)) {
        let Ok(part) = crate::export::level_partition(game, &lv) else {
            continue;
        };
        let w = LevelWriter::parse(&part)?;
        let Some(hf) = crate::export::heightfield_name(&w) else {
            continue;
        };
        let t = hf
            .strip_suffix(".heightfield-000.res")
            .and_then(|s| s.rsplit('/').next())
            .unwrap_or_default()
            .to_owned();
        if bc2_terragen::Biome::of_dice_terrain(&t).is_none() {
            continue;
        }
        let mut veg = 0usize;
        for g in members::group_entities(&w) {
            let ty = w.type_of(g.entity);
            if kind_of(&ty, "").is_some() {
                veg += members::runs(&w, &g)?.iter().map(|r| r.count).sum::<usize>();
            }
        }
        let polys = crate::validate::combat_outlines(&w);
        by_terrain.entry(t).or_default().push((lv, veg, polys));
    }
    let mut out = Vec::new();
    for (t, levels) in by_terrain {
        let Some(rep) = levels.iter().max_by_key(|l| (l.1, std::cmp::Reverse(l.0.clone()))) else {
            continue;
        };
        let lg = LevelGround::read(game, &rep.0)?;
        let g = lg.ground();
        let polys: Vec<Vec<[f64; 2]>> = levels.iter().flat_map(|l| l.2.iter().cloned()).collect();
        let outside = outside_fn(polys, g.side());
        let stats = bc2_terragen::vegetation::measure(&g, &lg.instances(), &outside);
        out.push(TerrainStats {
            biome: bc2_terragen::Biome::of_dice_terrain(&t).unwrap_or(bc2_terragen::Biome::Arid),
            terrain: t,
            level: rep.0.clone(),
            levels: levels.iter().map(|l| l.0.clone()).collect(),
            stats,
        });
    }
    Ok(out)
}

// ------------------------------------------------------------------------------------------------ a new terrain's

/// What [`place_on`] made: the member edits (one multi-file operation of SG1's edits), the placement and the report.
#[derive(Debug, Clone)]
pub struct Placement {
    /// The member edits: base members moved, deleted and copied (round SG1's [`crate::files::FileEdit`]s).
    pub edits: Vec<crate::files::FileEdit>,
    /// The instances placed per [`Kind::ALL`].
    pub placed: [usize; 4],
    /// The base's instances of the kinds the step replaced, per [`Kind::ALL`].
    pub replaced: [usize; 4],
    /// The placement itself (each instance's kind, type over the level's groups and transform).
    pub instances: Vec<bc2_terragen::vegetation::Placed>,
    /// Lines for the export's report.
    pub notes: Vec<String>,
}

/// The poses of a member type's instances on the base's ground (their yaw taken out), in the frame they keep: the
/// ground's normal when their up axes lie nearer it than the vertical (a median over the type's instances), else the
/// vertical. Instances standing more than 1 m over the ground (on a structure) are left out when any other is there.
/// Each pose with its instance's place among `rows` (an added member copies that instance: its record, TGEN-VEG-4).
#[must_use]
pub fn poses_of(
    rows: &[Rows],
    base: &bc2_terragen::vegetation::Ground,
) -> Vec<(usize, bc2_terragen::vegetation::Pose)> {
    use bc2_terragen::vegetation::{Pose, quantile};
    let angle = |r: &Rows, n: [f64; 3]| {
        let u = r[1].map(f64::from);
        let l = (u[0] * u[0] + u[1] * u[1] + u[2] * u[2]).sqrt().max(1e-12);
        ((u[0] * n[0] + u[1] * n[1] + u[2] * n[2]) / l).clamp(-1.0, 1.0).acos()
    };
    let mut to_v: Vec<f64> = rows.iter().map(|r| angle(r, [0.0, 1.0, 0.0])).collect();
    let mut to_n: Vec<f64> = rows
        .iter()
        .map(|r| angle(r, base.normal(f64::from(r[3][0]), f64::from(r[3][2]))))
        .collect();
    to_v.sort_by(f64::total_cmp);
    to_n.sort_by(f64::total_cmp);
    let normal = quantile(&to_n, 0.5) + 0.01 < quantile(&to_v, 0.5);
    let all: Vec<(usize, Pose)> = rows
        .iter()
        .enumerate()
        .map(|(k, r)| (k, Pose::of(r, base, normal)))
        .collect();
    let low: Vec<(usize, Pose)> = all.iter().filter(|p| p.1.sink <= 1.0).copied().collect();
    if low.is_empty() { all } else { low }
}

/// The vegetation, rocks and natural props of a level placed again on a new terrain (round TG4, TGEN-VEG-1 - 3): the
/// members of the kinds [`kind_of`] names, in every group of `w` (the partition as the project's log leaves it; its
/// group files read by `files`, the log's member orders `orders`), replaced by a placement on `ground` that falls on
/// the biome's statistics ([`bc2_terragen::vegetation::place`]), each type's poses its own instances' on the base's
/// ground `base`, `density` scaling every band's count (at 1 the clusters calibrated to DICE's,
/// [`bc2_terragen::vegetation::place_biome`]). The edits move the type's members first (each keeps its leaf and
/// record), then copy the DICE instance whose pose each further instance took, then delete the ones left over
/// (never a type's last).
///
/// # Errors
/// A group file that does not read or whose members its types do not count.
#[allow(
    clippy::too_many_arguments,
    clippy::too_many_lines,
    clippy::cast_precision_loss,
    reason = "the level, the two grounds and the settings; types, placement and edits in order; counts below 2^52"
)]
pub fn place_on(
    w: &LevelWriter,
    files: &dyn Fn(&str) -> Option<Vec<u8>>,
    orders: &members::Orders,
    base: &bc2_terragen::vegetation::Ground,
    ground: &bc2_terragen::vegetation::Ground,
    biome: bc2_terragen::Biome,
    density: f64,
    seed: u64,
) -> Result<Placement> {
    use crate::files::FileEdit;
    use bc2_terragen::vegetation::{BiomeStats, TypeChoice, place_biome};
    let gs = groups(w, files)?;
    // every type of the kinds, numbered over the groups: (group, type) by its number
    let mut types: Vec<TypeChoice> = Vec::new();
    let mut at: Vec<(usize, usize)> = Vec::new();
    // per type, each pose's instance (its place in the type's run)
    let mut sources: Vec<Vec<usize>> = Vec::new();
    let mut replaced = [0usize; 4];
    for (gi, g) in gs.iter().enumerate() {
        for (t, k) in g.type_kinds().into_iter().enumerate() {
            let Some(kind) = k else { continue };
            let (first, count) = g.runs.get(t).copied().unwrap_or((0, 0));
            let rows: Vec<Rows> = g.members.iter().skip(first).take(count).map(|m| m.1).collect();
            if let Some(r) = replaced.get_mut(kind.index()) {
                *r += count;
            }
            let poses = poses_of(&rows, base);
            sources.push(poses.iter().map(|p| p.0).collect());
            types.push(TypeChoice {
                kind,
                ty: at.len(),
                weight: count as f64,
                poses: poses.into_iter().map(|p| p.1).collect(),
            });
            at.push((gi, t));
        }
    }
    let stats = BiomeStats::dice(biome);
    let polys = crate::validate::combat_outlines(w);
    let areas = polys.len();
    let outside = outside_fn(polys, ground.side());
    let (_, instances) = place_biome(ground, biome, &types, &outside, seed, density);
    let mut placed = [0usize; 4];
    for p in &instances {
        if let Some(c) = placed.get_mut(p.kind.index()) {
            *c += 1;
        }
    }
    // the edits, group by group, type by type
    let mut edits = Vec::new();
    for (gi, g) in gs.iter().enumerate() {
        let key = (g.file.clone(), g.instance.clone());
        let order: members::Order = orders
            .get(&key)
            .cloned()
            .unwrap_or_else(|| (0..g.members.len()).map(members::Member::Base).collect());
        let mut next = order
            .iter()
            .filter_map(|m| match m {
                members::Member::Added { added } => Some(*added),
                members::Member::Base(_) => None,
            })
            .max()
            .unwrap_or(0);
        for (t, &(first, count)) in g.runs.iter().enumerate() {
            let Some(ty) = at.iter().position(|&(a, b)| a == gi && b == t) else {
                continue;
            };
            let new: Vec<&bc2_terragen::vegetation::Placed> = instances.iter().filter(|p| p.ty == ty).collect();
            let cur: Vec<(members::Member, &(String, Rows))> = (first..first + count)
                .filter_map(|k| Some((*order.get(k)?, g.members.get(k)?)))
                .collect();
            // a kind the biome's DICE terrains do not place stays as the base has it
            let Some(&(first_member, _)) = cur.first() else {
                continue;
            };
            if new.is_empty() {
                continue;
            }
            // the type's members moved first (each keeps its own record and leaf)
            for ((m, (_, rows)), to) in cur.iter().zip(&new) {
                edits.push(FileEdit::MemberTransform {
                    file: g.file.clone(),
                    instance: g.instance.clone(),
                    item: *m,
                    from: *rows,
                    to: to.rows,
                });
            }
            // then the ones more: each a copy of the DICE instance whose pose it took (its asset item and record:
            // DICE's records of one type may differ, TGEN-VEG-4), before any is deleted
            for to in new.iter().skip(cur.len()) {
                next += 1;
                let src = sources
                    .get(ty)
                    .and_then(|s| s.get(to.pose))
                    .and_then(|&k| cur.get(k))
                    .map_or(first_member, |c| c.0);
                edits.push(FileEdit::MemberAdd {
                    file: g.file.clone(),
                    instance: g.instance.clone(),
                    item: members::Member::Added { added: next },
                    like: src,
                    rows: to.rows,
                });
            }
            // then the ones left over
            for (m, (asset, rows)) in cur.iter().skip(new.len()) {
                edits.push(FileEdit::MemberDelete {
                    file: g.file.clone(),
                    instance: g.instance.clone(),
                    item: *m,
                    asset: asset.clone(),
                    rows: *rows,
                });
            }
        }
    }
    let notes = vec![format!(
        "vegetation: {} forest, {} middlegrowth, {} rock and {} prop instances placed for {} biome (DICE's {} \
         terrains' statistics, TGEN-VEG-2; the playable area {}), in place of the base's {} / {} / {} / {} \
         ({} member edits over {} types)",
        placed[0],
        placed[1],
        placed[2],
        placed[3],
        biome.name(),
        stats.terrains,
        if areas == 0 {
            "the terrain's central square (the level has no combat area)".to_owned()
        } else {
            format!("the level's {areas} combat area(s)")
        },
        replaced[0],
        replaced[1],
        replaced[2],
        replaced[3],
        edits.len(),
        types.len()
    )];
    Ok(Placement {
        edits,
        placed,
        replaced,
        instances,
        notes,
    })
}
