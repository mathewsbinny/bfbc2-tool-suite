//! `wake_install.port_missing`: the resources BF1943's assets our builds name lack (skinned meshes, skeletons, an
//! animation: the flags, the radar dish, the material masks), ported from the user's BF1943 copy at install time
//! into the ports root (`install_ports`, an OURS root) - meshes by the suite's `meshport::Porter` (a mesh with no
//! partition skinned to no soldier), Granny skeletons and animations by `granny::to_pc` - with each mesh's shader
//! plan in `plans.json`. A port is made once a process (every level of a plan ports most of the same assets from the
//! one BF1943 source); a failure is reported for every level that needs it.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};

use bc2_convert::meshport::{PortError, Porter, Source};
use bc2_convert::meshview::Index;
use bc2_pyrt::json::Value;

use super::index::{OurFile, primary_type};
use super::{OMap, Result, pjoin, read, write};

/// A port as the levels use it: each LOD's data, the mesh set, the textures, the plan; or a Granny file.
#[derive(Debug, Clone)]
enum Done {
    Mesh(bc2_convert::meshport::Ported),
    Granny(Vec<u8>),
}

/// The ports made this process, by (kind, source, name, bare): the port or its error (`type`, message).
static PORTED: LazyLock<Mutex<HashMap<(String, String, String, bool), std::result::Result<Done, (String, String)>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// What `port_missing` needs besides our files and the BF1943 source.
pub struct PortsSpec<'a> {
    /// The ports root (written anew).
    pub ports: &'a str,
    /// The BF1943 source's identity (its folder): the memo's key.
    pub source: &'a str,
    /// `PORT_PARTS`' partitions already ported from their templates (the suite's `dbxport.port`): (entry name as the
    /// source spells it, the partition's lower name, bytes).
    pub parts: &'a [(String, String, Vec<u8>)],
}

fn json_plan(r: &bc2_convert::meshport::Ported, name: &str) -> Value {
    let parts: Vec<(String, Value)> = r
        .plan_parts
        .iter()
        .map(|(p, how, donor)| {
            let d = donor.as_ref().map_or(Value::Null, |(m, f)| {
                Value::List(vec![Value::Str(m.clone()), Value::Str(f.clone())])
            });
            (
                p.clone(),
                Value::Dict(vec![("layout".into(), Value::Str(how.clone())), ("donor".into(), d)]),
            )
        })
        .collect();
    let mut textures: Vec<String> = r.textures.iter().map(|(n, _)| n.clone()).collect();
    textures.sort();
    let hex = |b: &[u8]| {
        use std::fmt::Write as _;
        b.iter().fold(String::new(), |mut s, x| {
            let _ = write!(s, "{x:02x}");
            s
        })
    };
    Value::Dict(vec![
        ("mesh".into(), Value::Str(name.to_owned())),
        ("kind".into(), Value::Str(r.kind.clone())),
        ("skinned_to_soldier".into(), Value::Bool(r.soldier)),
        ("parts".into(), Value::Dict(parts)),
        (
            "materials".into(),
            Value::List(r.materials.iter().cloned().map(Value::Str).collect()),
        ),
        (
            "textures".into(),
            Value::List(textures.into_iter().map(Value::Str).collect()),
        ),
        ("renamed".into(), Value::Dict(Vec::new())),
        (
            "pc_layouts".into(),
            Value::List(
                r.pc_layouts
                    .iter()
                    .map(|lod| Value::List(lod.iter().map(|rec| Value::Str(hex(rec))).collect()))
                    .collect(),
            ),
        ),
    ])
}

/// `port_missing(ours, index, src, report)`: writes the ports root - the resources of our mesh / skeleton /
/// animation partitions that lack them, and `PORT_PARTS`' partitions with theirs - and its `plans.json`.
///
/// # Errors
/// A file that cannot be read or written (a port that fails is reported, not raised).
#[allow(clippy::too_many_lines, reason = "port_missing, step for step")]
pub fn port_missing(
    spec: &PortsSpec<'_>,
    ours: &OMap<OurFile>,
    src: &dyn Source,
    ix: &dyn Fn() -> std::result::Result<Index, String>,
    report: &mut Vec<String>,
) -> Result<()> {
    super::stage::rmtree(spec.ports)?;
    std::fs::create_dir_all(spec.ports).map_err(|e| format!("{}: {e}", spec.ports))?;
    let put = |name: &str, data: &[u8]| write(&pjoin(spec.ports, name), data);
    let mut want: Vec<(String, String)> = Vec::new();
    for (low, (_, path)) in ours.iter() {
        let Some(stem) = low.strip_suffix(".dbx") else { continue };
        if ours.contains(&format!("{stem}.res")) {
            continue;
        }
        let (t, _) = primary_type(&read(path)?)?;
        if let Some(t) = t
            && (t.ends_with("MeshAsset") || t.ends_with("SkeletonAsset") || t == "Animation.AnimationAsset")
        {
            want.push((stem.to_owned(), t));
        }
    }
    for (entry, low, data) in spec.parts {
        put(entry, data)?;
        want.push((low.clone(), "Animation.SkeletonAsset".to_owned()));
    }
    want.sort();
    let mut index: Option<Index> = None;
    let mut plans: Vec<(String, Value)> = Vec::new();
    let (mut done, mut failed) = (Vec::new(), Vec::new());
    for (p, t) in &want {
        let k = format!("{p}.res");
        let Some((_, src_name)) = src.entry(&k) else {
            failed.push(format!("{p}: no resource in BF1943's data"));
            continue;
        };
        let name = src_name
            .get(..src_name.len().saturating_sub(4))
            .unwrap_or_default()
            .to_owned();
        let mesh = t.ends_with("MeshAsset");
        let bare = mesh && src.entry(&format!("{p}.dbx")).is_none();
        let key = (
            if mesh { "mesh" } else { "granny" }.to_owned(),
            spec.source.to_owned(),
            if mesh { name.clone() } else { k.clone() },
            bare,
        );
        let got = PORTED.lock().unwrap_or_else(PoisonError::into_inner).get(&key).cloned();
        let got = if let Some(g) = got {
            g
        } else {
            let g = if mesh {
                if index.is_none() {
                    index = Some(ix().map_err(|e| format!("the mesh index: {e}"))?);
                }
                let Some(ixr) = index.as_ref() else { continue };
                let mut porter = Porter::new(src, ixr);
                porter
                    .port_with(&name, bare)
                    .map(Done::Mesh)
                    .map_err(|e: PortError| (e.kind, e.msg))
            } else {
                match src.data(&k) {
                    Some(b) => bc2_convert::granny::to_pc::<std::collections::hash_map::RandomState>(&b, None)
                        .map(Done::Granny)
                        .map_err(|e| ("GrannyError".to_owned(), e.0)),
                    None => Err(("KeyError".to_owned(), format!("'{k}'"))),
                }
            };
            PORTED
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .insert(key, g.clone());
            g
        };
        match got {
            Err((kind, msg)) => failed.push(format!("{p}: {kind}: {msg}")),
            Ok(Done::Granny(b)) => {
                put(&format!("{name}.res"), &b)?;
                done.push(format!("{name} ({})", t.rsplit('.').next().unwrap_or_default()));
            }
            Ok(Done::Mesh(r)) => {
                for (lod, b) in r.lods.iter().enumerate() {
                    put(&format!("{name}_lod{lod}_data.res"), b)?;
                }
                put(&format!("{name}.res"), &r.meshset)?;
                for (tn, b) in &r.textures {
                    if !ours.contains(&tn.to_lowercase()) {
                        put(tn, b)?;
                    }
                }
                plans.push((name.clone(), json_plan(&r, &name)));
                done.push(format!("{name} ({})", t.rsplit('.').next().unwrap_or_default()));
            }
        }
    }
    let text = bc2_pyrt::json::dumps(&Value::Dict(vec![("shader_plans".into(), Value::Dict(plans))]), Some(1));
    // json.dump into a file opened in text mode: its line ends as CRLF
    write(&pjoin(spec.ports, "plans.json"), text.replace('\n', "\r\n").as_bytes())?;
    report.push(format!(
        "ported from BF1943 at install: {} ({}); {} failed",
        done.len(),
        done.join(", "),
        failed.len()
    ));
    for f in failed {
        report.push(format!("  FAILED {f}"));
    }
    Ok(())
}
