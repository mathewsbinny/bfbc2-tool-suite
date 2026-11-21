//! The DBX inspector's window (round GR4: `DbxEditGUI`, the suite's last C# window, folded into the map editor): any
//! binary `.dbx` - a file (typed, or dropped on the window), an entry of a bundle, one of the open level's files, or
//! the open project's level partition - as a tree of instances (bold), fields, lists and values; the selected
//! element's attributes and value edited and applied; references followed; `DbxEditGUI`'s Instance, Edit and Level
//! menus; find; the output pane (a line naming a guid goes to its instance); save (the first save keeps `.bak`),
//! save as, save into a copy of the bundle, export and import XML, the edit script. The model and every edit are
//! [`crate::dbxinspect`]'s; this file only draws it, in the suite's design language (the editor's theme).
//!
//! The project's partition is read as the project's log makes it: a field edit there becomes an operation of the
//! project ([`DbxPanel::show`] returns it; the app commits it, so it is in History, undone with Ctrl+Z and saved
//! with the project); its other menus are left out (they would bypass the log) - Save as makes a file of it to edit.
//! It opens from the command palette ("DBX inspector"), the start screen, and `bfbc2-editor --dbx <file>`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use bc2_formats::dbxedit::EditFile;
use bc2_levelwrite::ops::FieldSet;
use bc2_levelwrite::{FieldType, Op};
use eframe::egui::{self, CornerRadius, Margin, RichText, Sense, Stroke, TextStyle, Vec2};

use crate::dbxinspect::{self, Applied, DbxDoc, NodePath, Source};
use crate::edit::Doc;
use crate::theme::{Metrics, Palette};

/// A question the window asks before a command runs (`DbxEditGUI`'s prompts and file dialogs, in the window).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ask {
    /// File > Open (a path).
    Open,
    /// Open an entry of a bundle (the bundle, then its entry).
    OpenBundle,
    /// Open one of the open level's files.
    LevelFile,
    /// Save as.
    SaveAs,
    /// Save into a copy of the bundle.
    SaveIntoBundle,
    /// Export XML.
    ExportXml,
    /// Import XML.
    ImportXml,
    /// Save edit script.
    SaveScript,
    /// Replace string.
    Replace,
    /// Replace string prefix.
    ReplacePrefix,
    /// Remove field everywhere.
    DropField,
    /// Set bool / byte field everywhere.
    SetByte,
    /// Partition GUID.
    PartitionGuid,
    /// Add array item.
    AddItem,
    /// Remove instance (a confirmation).
    DropInstance,
    /// Graft instance from another file (the file, then its instance).
    Graft,
    /// Copy field from another file (the file, its instance, the field).
    CopyField,
    /// Check level against the level whose cull trees it uses.
    CheckSlots,
    /// Fix cull slots.
    FixSlots,
}

impl Ask {
    fn title(self) -> &'static str {
        match self {
            Self::Open => "Open a binary .dbx file",
            Self::OpenBundle => "Open from a bundle",
            Self::LevelFile => "Open one of this level's files",
            Self::SaveAs => "Save as",
            Self::SaveIntoBundle => "Save into a copy of the bundle",
            Self::ExportXml => "Export XML",
            Self::ImportXml => "Import XML (the open file gives the types)",
            Self::SaveScript => "Save edit script",
            Self::Replace => "Replace string",
            Self::ReplacePrefix => "Replace string prefix",
            Self::DropField => "Remove field everywhere",
            Self::SetByte => "Set bool / byte field everywhere",
            Self::PartitionGuid => "Partition GUID",
            Self::AddItem => "Add array item",
            Self::DropInstance => "Remove instance",
            Self::Graft => "Graft instance from another file",
            Self::CopyField => "Copy field from another file",
            Self::CheckSlots => "Check level against the cull-tree level",
            Self::FixSlots => "Fix cull slots",
        }
    }

    /// The fields' labels (`DbxEditGUI`'s prompt texts).
    fn labels(self) -> &'static [&'static str] {
        match self {
            Self::Open | Self::SaveAs | Self::SaveIntoBundle => &["File:"],
            Self::OpenBundle => &["Bundle (.fbrb):", "Filter:"],
            Self::LevelFile => &["Filter:"],
            Self::ExportXml | Self::ImportXml => &["XML file:"],
            Self::SaveScript => &["Script file (.cmd):"],
            Self::Replace => &["Every string exactly equal to:", "becomes:"],
            Self::ReplacePrefix => &[
                "Strings starting with (case is ignored):",
                "get this start instead:",
                "Except strings starting with (separate several with |; may be empty):",
            ],
            Self::DropField => &["Remove every field, array or complex named:"],
            Self::SetByte => &["Field name:", "Value (true, false or 0-255):"],
            Self::PartitionGuid => &["The file's own GUID (a copy under a new name needs a new one):"],
            Self::AddItem => &["Reference of the new item (an instance GUID, or partition/GUID):"],
            Self::DropInstance => &[],
            Self::Graft | Self::CopyField => &["From which file:", "Filter:"],
            Self::CheckSlots | Self::FixSlots => &["Level whose cull trees this file uses:"],
        }
    }
}

/// A question being asked: its fields, and a list to choose from (bundle entries, instances, fields).
#[derive(Debug, Clone)]
pub struct Prompt {
    /// Which.
    pub ask: Ask,
    /// The fields' texts.
    pub fields: Vec<String>,
    /// The list: (key, text); its choice.
    pub list: Vec<(String, String)>,
    /// The chosen key.
    pub chosen: Option<String>,
    /// For Copy field: the instance chosen (the list then holds its fields).
    pub instance: Option<String>,
    /// A file read for the command (graft, copy field).
    pub other: Option<EditFile>,
    /// What went wrong, in words.
    pub error: Option<String>,
}

/// The window's state.
#[derive(Default)]
pub struct DbxPanel {
    /// Shown.
    pub open: bool,
    /// The file open.
    pub doc: Option<DbxDoc>,
    /// The selected element.
    pub selected: Option<NodePath>,
    /// The containers shown open.
    pub expanded: HashSet<NodePath>,
    /// The attribute values being edited (the selected element's).
    pub attr_edit: Vec<String>,
    /// The value being edited.
    pub value_edit: String,
    /// The find box, the matches and the one shown.
    pub find: String,
    found: Vec<NodePath>,
    found_at: Option<usize>,
    /// The status line (a match's place) instead of the file's summary.
    status: Option<String>,
    /// A question being asked.
    pub prompt: Option<Prompt>,
    /// The last error, shown until the next action.
    pub error: Option<String>,
    /// The open level's own files (from its folders), read when first listed.
    level_files: Option<(String, Vec<PathBuf>)>,
    /// The tree's rows: (path, depth), made again when the tree or the file changes.
    rows: Option<Vec<(NodePath, usize)>>,
    /// Scroll the tree to the selected row.
    reveal: bool,
    /// Buttons and rows drawn this frame (the tests click them): (label, rectangle).
    pub rects: Vec<(String, egui::Rect)>,
}

/// What the window can see of the open level: its name, its folders, the game copy's bundles of it, the project.
pub struct LevelLink<'a> {
    /// The level (`mp_005`).
    pub lv: &'a str,
    /// Its folders (its own, then its shared bundles').
    pub bases: &'a [PathBuf],
    /// The project open on it.
    pub doc: Option<&'a Doc>,
}

/// The bytes of a `.dbx` file read from disk, parsed.
fn read_dbx(path: &str) -> Result<EditFile, String> {
    let f = EditFile::parse(std::fs::read(path).map_err(|e| format!("{path}: {e}"))?).map_err(|e| e.message)?;
    if f.is_binary {
        Ok(f)
    } else {
        Err(format!("{path}: not a binary .dbx file."))
    }
}

/// Every `.dbx` under the folders (for the level's file list), sorted.
fn dbx_under(folders: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    let mut stack: Vec<PathBuf> = folders.to_vec();
    while let Some(d) = stack.pop() {
        let Ok(rd) = std::fs::read_dir(&d) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            match e.file_type() {
                Ok(t) if t.is_dir() => stack.push(p),
                Ok(t) if t.is_file() && p.extension().is_some_and(|x| x.eq_ignore_ascii_case("dbx")) => out.push(p),
                _ => {}
            }
        }
    }
    out.sort();
    out
}

impl DbxPanel {
    /// Opens a file in the window (it is shown).
    pub fn open_file(&mut self, path: &Path) {
        self.open = true;
        match DbxDoc::open(path) {
            Ok(d) => self.set_doc(d),
            Err(e) => self.error = Some(e),
        }
    }

    /// Opens an entry of a bundle.
    pub fn open_bundle_entry(&mut self, bundle: &Path, entry: &str) {
        self.open = true;
        match DbxDoc::open_bundle_entry(bundle, entry) {
            Ok(d) => self.set_doc(d),
            Err(e) => self.error = Some(e),
        }
    }

    /// Opens the project's level partition (as its log makes it).
    pub fn open_project(&mut self, doc: &Doc) {
        self.open = true;
        let Some(bytes) = doc.partition() else {
            self.error = Some("This project has no level partition.".into());
            return;
        };
        let name = doc.project.base.partition.clone();
        match DbxDoc::from_bytes(
            bytes,
            Source::Project {
                partition: name.clone(),
                revision: doc.revision,
            },
            &format!("project: {name}"),
        ) {
            Ok(mut d) => {
                d.log.push(format!(
                    "opened the project's {name} (field edits go into the project's History)"
                ));
                self.set_doc(d);
            }
            Err(e) => self.error = Some(e),
        }
    }

    /// The file shown from here on; the tree opens at its partition.
    pub fn set_doc(&mut self, d: DbxDoc) {
        self.expanded.clear();
        self.expanded.insert(vec![0]);
        self.found.clear();
        self.found_at = None;
        self.status = None;
        self.error = None;
        let log = self.doc.take().map_or_else(Vec::new, |o| o.log);
        let mut d = d;
        let mut all = log;
        all.append(&mut d.log);
        d.log = all;
        self.doc = Some(d);
        self.select(None);
        self.rows = None;
    }

    /// Selects an element (its value and attributes go to the edit boxes; its parents open).
    pub fn select(&mut self, path: Option<NodePath>) {
        if let Some(p) = &path {
            for n in 1..p.len() {
                self.expanded.insert(p.get(..n).unwrap_or_default().to_vec());
            }
            self.rows = None;
            self.reveal = true;
        }
        self.selected = path;
        self.load_edit();
    }

    /// The edit boxes from the selected element (`DbxEditGUI`'s `ShowElement`).
    fn load_edit(&mut self) {
        let (attrs, value) = match (&self.doc, &self.selected) {
            (Some(d), Some(p)) => d.element(p).map_or_else(
                || (Vec::new(), String::new()),
                |e| (d.attributes(e).into_iter().map(|a| a.1).collect(), d.value(e)),
            ),
            _ => (Vec::new(), String::new()),
        };
        self.attr_edit = attrs;
        self.value_edit = value;
    }

    /// The file changed: the tree is made again, the selection kept by its place where it still exists.
    fn changed(&mut self) {
        self.rows = None;
        if let (Some(d), Some(p)) = (&self.doc, &self.selected)
            && d.element(p).is_none()
        {
            let mut q = p.clone();
            while !q.is_empty() && d.element(&q).is_none() {
                q.pop();
            }
            self.selected = (!q.is_empty()).then_some(q);
        }
        self.load_edit();
    }

    /// Apply: the edit boxes into the file - or, for the project's partition, an operation of the project (returned).
    pub fn apply(&mut self, project: Option<&Doc>) -> Option<Op> {
        self.error = None;
        let (Some(d), Some(p)) = (self.doc.as_mut(), self.selected.clone()) else {
            return None;
        };
        if d.is_project() {
            let Some(pd) = project else {
                self.error = Some("The project is no longer open.".into());
                return None;
            };
            let ty_of = |guid: &str, field: &str| -> Option<FieldType> {
                let k = pd
                    .scene
                    .entities
                    .iter()
                    .find(|e| e.guid.eq_ignore_ascii_case(guid))?
                    .index;
                pd.fields(k).into_iter().find(|r| r.path == field).map(|r| r.ty)
            };
            return match dbxinspect::project_field(d, &p, &self.attr_edit, &self.value_edit, ty_of) {
                Ok(Some((guid, field, to))) => {
                    let k = pd
                        .scene
                        .entities
                        .iter()
                        .find(|e| e.guid.eq_ignore_ascii_case(&guid))
                        .map(|e| e.index);
                    let from = k
                        .and_then(|k| pd.fields(k).into_iter().find(|r| r.path == field))
                        .and_then(|r| r.value);
                    d.log.push(format!("set {field} of {guid} in the project"));
                    Some(Op::SetField {
                        items: vec![FieldSet { guid, field, from, to }],
                    })
                }
                Ok(None) => None,
                Err(e) => {
                    self.error = Some(e);
                    None
                }
            };
        }
        match d.apply_edit(&p, &self.attr_edit, &self.value_edit) {
            Ok(Applied::Changed) => self.changed(),
            Ok(Applied::Unchanged) => {}
            Err(e) => {
                self.error = Some(format!("Value not accepted: {e}"));
                self.changed();
            }
        }
        None
    }

    /// Find (Enter in the find box): the matches, the first selected.
    pub fn run_find(&mut self) {
        let Some(d) = self.doc.as_mut() else { return };
        if self.find.is_empty() {
            return;
        }
        self.found = d.find(&self.find.clone());
        self.found_at = None;
        self.find_next();
    }

    /// Find next (F3).
    pub fn find_next(&mut self) {
        if self.found.is_empty() {
            return;
        }
        let i = self.found_at.map_or(0, |i| (i + 1) % self.found.len());
        self.found_at = Some(i);
        let p = self.found.get(i).cloned();
        self.select(p);
        self.status = Some(format!("match {} of {} for '{}'", i + 1, self.found.len(), self.find));
    }

    /// The status line: the file's summary, or the match shown.
    #[must_use]
    pub fn status_text(&self) -> String {
        if let Some(s) = &self.status {
            return s.clone();
        }
        match &self.doc {
            None => "Open a binary .dbx file (Open, or drop it on the window).".into(),
            Some(d) => format!(
                "{} instances, {} strings{}   {}",
                d.file.instances().map_or(0, Iterator::count),
                d.file.table.strings.len(),
                if d.modified { " - modified" } else { "" },
                d.path
            ),
        }
    }

    /// Starts a question.
    pub fn ask(&mut self, ask: Ask) {
        let n = ask.labels().len();
        let mut fields = vec![String::new(); n];
        if let Some(d) = &self.doc {
            let sel = self.selected.as_ref().and_then(|p| d.element(p));
            match ask {
                Ask::SaveAs => {
                    if let Some(f) = fields.first_mut() {
                        *f = match &d.source {
                            Source::File(p) => p.display().to_string(),
                            _ => String::new(),
                        };
                    }
                }
                Ask::ExportXml | Ask::ImportXml => {
                    if let (Some(f), Source::File(p)) = (fields.first_mut(), &d.source) {
                        *f = p.with_extension("xml").display().to_string();
                    }
                }
                Ask::SaveScript => {
                    if let (Some(f), Source::File(p)) = (fields.first_mut(), &d.source) {
                        let stem = p
                            .file_stem()
                            .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
                        *f = p.with_file_name(format!("{stem}_edits.cmd")).display().to_string();
                    }
                }
                Ask::PartitionGuid => {
                    if let Some(f) = fields.first_mut() {
                        *f = d.partition_guid_now();
                    }
                }
                Ask::DropField | Ask::SetByte => {
                    if let (Some(f), Some(e)) = (fields.first_mut(), sel) {
                        *f = d.get(e, "name").unwrap_or_default();
                    }
                    if ask == Ask::SetByte
                        && let Some(f) = fields.get_mut(1)
                    {
                        "true".clone_into(f);
                    }
                }
                _ => {}
            }
        }
        self.prompt = Some(Prompt {
            ask,
            fields,
            list: Vec::new(),
            chosen: None,
            instance: None,
            other: None,
            error: None,
        });
    }

    /// Runs the question's command with its answers (OK).
    pub fn answer(&mut self, level: Option<&LevelLink<'_>>, scratch: &Path) {
        let Some(mut pr) = self.prompt.take() else { return };
        let f = |i: usize| pr.fields.get(i).cloned().unwrap_or_default();
        let sel = self.selected.clone().unwrap_or_default();
        let done: Result<bool, String> = (|| {
            match pr.ask {
                Ask::Open => {
                    self.open_file(Path::new(f(0).trim()));
                    return Ok(self.error.is_none());
                }
                Ask::OpenBundle => {
                    let Some(entry) = pr.chosen.clone() else {
                        return Err("Choose an entry.".into());
                    };
                    self.open_bundle_entry(Path::new(f(0).trim()), &entry);
                    return Ok(self.error.is_none());
                }
                Ask::LevelFile => {
                    let Some(file) = pr.chosen.clone() else {
                        return Err("Choose a file.".into());
                    };
                    self.open_file(Path::new(&file));
                    return Ok(self.error.is_none());
                }
                _ => {}
            }
            let _ = level;
            let Some(d) = self.doc.as_mut() else { return Ok(true) };
            match pr.ask {
                Ask::SaveAs => d.save(Path::new(f(0).trim()))?,
                Ask::SaveIntoBundle => d.save_into_bundle(Path::new(f(0).trim()), scratch)?,
                Ask::ExportXml => d.export_xml(Path::new(f(0).trim()))?,
                Ask::ImportXml => d.import_xml(Path::new(f(0).trim()))?,
                Ask::SaveScript => d.save_script(Path::new(f(0).trim()))?,
                Ask::Replace => d.replace(&f(0), &f(1))?,
                Ask::ReplacePrefix => d.replace_prefix(&f(0), &f(1), &f(2))?,
                Ask::DropField => d.drop_field(&f(0))?,
                Ask::SetByte => d.set_byte(&f(0), &f(1))?,
                Ask::PartitionGuid => d.partition_guid(&f(0))?,
                Ask::AddItem => d.add_item(&sel, &f(0))?,
                Ask::DropInstance => d.drop_instance(&sel)?,
                Ask::Graft => {
                    let (Some(src), Some(guid)) = (pr.other.as_ref(), pr.chosen.clone()) else {
                        return Err("Read the file, then choose the instance to copy.".into());
                    };
                    let at = (!sel.is_empty()).then_some(sel.as_slice());
                    let p = d.graft(f(0).trim(), src, &guid, at)?;
                    self.selected = p;
                }
                Ask::CopyField => {
                    let (Some(src), Some(inst), Some(field)) =
                        (pr.other.as_ref(), pr.instance.clone(), pr.chosen.clone())
                    else {
                        return Err("Read the file, choose the instance, then the field.".into());
                    };
                    d.copy_field(&sel, f(0).trim(), src, &inst, &field)?;
                }
                Ask::CheckSlots | Ask::FixSlots => {
                    let path = f(0).trim().to_owned();
                    let other = read_dbx(&path)?;
                    if pr.ask == Ask::CheckSlots {
                        d.check_level(Some((&path, &other)))?;
                    } else {
                        d.fix_slots(&path, &other)?;
                    }
                }
                Ask::Open | Ask::OpenBundle | Ask::LevelFile => {}
            }
            Ok(true)
        })();
        match done {
            Ok(true) => self.changed(),
            Ok(false) => {}
            Err(e) => {
                pr.error = Some(e);
                self.prompt = Some(pr);
            }
        }
    }

    /// The tree's rows (made again after a change).
    fn rows(&mut self) -> &[(NodePath, usize)] {
        if self.rows.is_none() {
            let mut out = Vec::new();
            if let Some(d) = &self.doc {
                fn walk(
                    l: &[bc2_formats::dbxedit::Element],
                    prefix: &mut Vec<usize>,
                    open: &HashSet<NodePath>,
                    out: &mut Vec<(NodePath, usize)>,
                ) {
                    for (i, e) in l.iter().enumerate() {
                        prefix.push(i);
                        out.push((prefix.clone(), prefix.len() - 1));
                        if e.is_container() && open.contains(prefix.as_slice()) {
                            walk(e.kids(), prefix, open, out);
                        }
                        prefix.pop();
                    }
                }
                walk(&d.file.top_level, &mut Vec::new(), &self.expanded, &mut out);
            }
            self.rows = Some(out);
        }
        self.rows.as_deref().unwrap_or_default()
    }

    /// The window: shown while [`DbxPanel::open`]. Returns a field edit of the project's partition, as the project's
    /// operation, for the app to commit.
    pub fn show(&mut self, ctx: &egui::Context, p: &Palette, m: &Metrics, level: Option<&LevelLink<'_>>) -> Option<Op> {
        if !self.open {
            return None;
        }
        // the project's partition follows the project (an edit committed, undone, redone)
        if let (Some(d), Some(l)) = (&self.doc, level)
            && let (Source::Project { revision, .. }, Some(pd)) = (&d.source, l.doc)
            && *revision != pd.revision
        {
            let (sel, open, log) = (self.selected.clone(), self.expanded.clone(), d.log.clone());
            self.open_project(pd);
            if let Some(d) = self.doc.as_mut() {
                d.log = log;
            }
            self.expanded = open;
            self.select(sel);
        }
        // a file dropped on the editor opens here
        let dropped: Vec<PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if let Some(f) = dropped
            .iter()
            .find(|f| f.extension().is_some_and(|x| x.eq_ignore_ascii_case("dbx")))
        {
            self.open_file(f);
        }
        self.rects.clear();
        let mut op = None;
        let mut open = true;
        let frame = egui::Frame::new()
            .fill(p.surface_1)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(m.radius_lg as u8))
            .shadow(ctx.global_style().visuals.window_shadow)
            .inner_margin(Margin::same(m.space_3 as i8));
        let title = match &self.doc {
            Some(d) => format!("DBX inspector - {}{}", d.name(), if d.modified { " *" } else { "" }),
            None => "DBX inspector".to_owned(),
        };
        egui::Window::new(RichText::new(title).text_style(TextStyle::Name("section".into())))
            .id(egui::Id::new("dbx inspector"))
            .open(&mut open)
            .frame(frame)
            .default_size(Vec2::new(1180.0, 760.0))
            .min_size(Vec2::new(640.0, 420.0))
            .default_pos(egui::pos2(60.0, 60.0))
            .collapsible(false)
            .resizable(true)
            .show(ctx, |ui| {
                op = self.contents(ui, p, m, level);
            });
        if !open {
            self.open = false;
        }
        op
    }

    fn button(&mut self, ui: &mut egui::Ui, p: &Palette, label: &str, enabled: bool, tip: &str) -> bool {
        let r = ui
            .add_enabled(enabled, egui::Button::new(RichText::new(label).color(p.ink)))
            .on_hover_text(tip);
        self.rects.push((label.to_owned(), r.rect));
        r.clicked()
    }

    fn contents(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, level: Option<&LevelLink<'_>>) -> Option<Op> {
        let has = self.doc.is_some();
        let project = self.doc.as_ref().is_some_and(DbxDoc::is_project);
        let file_edits = has && !project;
        let bundle = matches!(self.doc.as_ref().map(|d| &d.source), Some(Source::Bundle { .. }));
        // ---- the toolbar: files, then edits
        ui.horizontal_wrapped(|ui| {
            if self.button(
                ui,
                p,
                "Open\u{2026}",
                true,
                "A binary .dbx file on disk (or drop one on the window)",
            ) {
                self.ask(Ask::Open);
            }
            if self.button(
                ui,
                p,
                "From bundle\u{2026}",
                true,
                "An entry of a bundle (.fbrb), read in memory",
            ) {
                self.ask(Ask::OpenBundle);
            }
            if let Some(l) = level {
                if self.button(
                    ui,
                    p,
                    "Level file\u{2026}",
                    true,
                    "One of the open level's own .dbx files",
                ) {
                    if self.level_files.as_ref().is_none_or(|(lv, _)| lv != l.lv) {
                        self.level_files = Some((l.lv.to_owned(), dbx_under(l.bases)));
                    }
                    self.ask(Ask::LevelFile);
                }
                if let Some(pd) = l.doc
                    && self.button(
                        ui,
                        p,
                        "Project partition",
                        true,
                        "The project's level partition as its edits make it: field edits go into the project",
                    )
                {
                    self.open_project(pd);
                }
            }
            ui.separator();
            let is_file = matches!(self.doc.as_ref().map(|d| &d.source), Some(Source::File(_)));
            if self.button(
                ui,
                p,
                "Save",
                is_file && file_edits,
                "Save over the file (the first save keeps <name>.bak)",
            ) && let Some(d) = self.doc.as_mut()
                && let Source::File(f) = d.source.clone()
                && let Err(e) = d.save(&f)
            {
                self.error = Some(e);
            }
            if self.button(ui, p, "Save as\u{2026}", has, "Save as a .dbx file") {
                self.ask(Ask::SaveAs);
            }
            if self.button(
                ui,
                p,
                "Save into bundle\u{2026}",
                bundle,
                "A copy of the bundle with this entry's edits",
            ) {
                self.ask(Ask::SaveIntoBundle);
            }
            if self.button(
                ui,
                p,
                "Export XML\u{2026}",
                has,
                "DbxEdit's XML of the file as it is now",
            ) {
                self.ask(Ask::ExportXml);
            }
            if self.button(
                ui,
                p,
                "Import XML\u{2026}",
                file_edits,
                "Read an edited XML back, typed from this file",
            ) {
                self.ask(Ask::ImportXml);
            }
            if self.button(
                ui,
                p,
                "Save edit script\u{2026}",
                file_edits,
                "The edits as DbxEdit commands (bfbc2.exe dbxedit edit)",
            ) {
                self.ask(Ask::SaveScript);
            }
            ui.separator();
            let undo = self.doc.as_ref().is_some_and(|d| d.undo_steps() > 0);
            if self.button(ui, p, "Undo", undo, "Take back the last edit") {
                if let Some(d) = self.doc.as_mut() {
                    d.undo();
                }
                self.changed();
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.menu_button(RichText::new("Instance").color(p.ink), |ui| {
                let sel = self.selected.is_some();
                if ui
                    .add_enabled(file_edits && sel, egui::Button::new("Remove instance\u{2026}"))
                    .clicked()
                {
                    self.ask(Ask::DropInstance);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        file_edits,
                        egui::Button::new("Graft instance from another file\u{2026}"),
                    )
                    .clicked()
                {
                    self.ask(Ask::Graft);
                    ui.close();
                }
                if ui
                    .add_enabled(
                        file_edits && sel,
                        egui::Button::new("Copy field from another file\u{2026}"),
                    )
                    .clicked()
                {
                    self.ask(Ask::CopyField);
                    ui.close();
                }
                let array = self
                    .doc
                    .as_ref()
                    .zip(self.selected.as_ref())
                    .is_some_and(|(d, s)| d.is_instance_array(s));
                if ui
                    .add_enabled(file_edits && array, egui::Button::new("Add array item\u{2026}"))
                    .clicked()
                {
                    self.ask(Ask::AddItem);
                    ui.close();
                }
                if ui.add_enabled(has && sel, egui::Button::new("Copy GUID")).clicked() {
                    if let Some(g) = self
                        .doc
                        .as_ref()
                        .zip(self.selected.as_ref())
                        .and_then(|(d, s)| d.guid_of(s))
                    {
                        ui.ctx().copy_text(g);
                    }
                    ui.close();
                }
            });
            ui.menu_button(RichText::new("Edit").color(p.ink), |ui| {
                for (label, ask) in [
                    ("Replace string\u{2026}", Ask::Replace),
                    ("Replace string prefix\u{2026}", Ask::ReplacePrefix),
                    ("Remove field everywhere\u{2026}", Ask::DropField),
                    ("Set bool / byte field everywhere\u{2026}", Ask::SetByte),
                    ("Partition GUID\u{2026}", Ask::PartitionGuid),
                ] {
                    if ui.add_enabled(file_edits, egui::Button::new(label)).clicked() {
                        self.ask(ask);
                        ui.close();
                    }
                }
            });
            ui.menu_button(RichText::new("Level").color(p.ink), |ui| {
                if ui.add_enabled(has, egui::Button::new("Check level")).clicked() {
                    if let Some(d) = self.doc.as_mut()
                        && let Err(e) = d.check_level(None)
                    {
                        self.error = Some(e);
                    }
                    ui.close();
                }
                if ui
                    .add_enabled(has, egui::Button::new("Check level against cull-tree level\u{2026}"))
                    .clicked()
                {
                    self.ask(Ask::CheckSlots);
                    ui.close();
                }
                if ui
                    .add_enabled(file_edits, egui::Button::new("Fix cull slots\u{2026}"))
                    .clicked()
                {
                    self.ask(Ask::FixSlots);
                    ui.close();
                }
            });
            ui.separator();
            let r = ui.add_enabled(
                has,
                egui::TextEdit::singleline(&mut self.find)
                    .hint_text("Find text, GUID, type or value")
                    .desired_width(260.0),
            );
            self.rects.push(("find".into(), r.rect));
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                self.run_find();
            }
            if self.button(
                ui,
                p,
                "Find",
                has && !self.find.is_empty(),
                "Every element whose tag, attributes or value hold the text",
            ) {
                self.run_find();
            }
            if self.button(ui, p, "Next", !self.found.is_empty(), "The next match (F3)")
                || (has && ui.input(|i| i.key_pressed(egui::Key::F3)))
            {
                self.find_next();
            }
        });
        if let Some(e) = &self.error {
            ui.label(RichText::new(e).color(p.status_error));
        }
        if self.prompt.is_some() {
            self.prompt_ui(ui, p, m, level);
        }
        ui.add_space(m.space_2);
        // ---- the output and the status line below; the tree and the selected element above
        let mut op = None;
        egui::Panel::bottom("dbx output")
            .resizable(true)
            .default_size(150.0)
            .frame(egui::Frame::new().fill(p.surface_1))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(self.status_text())
                        .text_style(TextStyle::Name("small".into()))
                        .color(p.ink_muted),
                );
                self.output_ui(ui, p);
            });
        egui::Panel::right("dbx element")
            .resizable(true)
            .default_size(470.0)
            .frame(
                egui::Frame::new()
                    .fill(p.surface_1)
                    .inner_margin(Margin::same(m.space_2 as i8)),
            )
            .show(ui, |ui| {
                op = self.element_ui(ui, p, m, level);
            });
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.surface_2))
            .show(ui, |ui| self.tree_ui(ui, p, m));
        op
    }

    fn prompt_ui(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, level: Option<&LevelLink<'_>>) {
        let Some(mut pr) = self.prompt.take() else { return };
        let mut ok = false;
        let mut cancel = false;
        egui::Frame::new()
            .fill(p.surface_2)
            .stroke(Stroke::new(1.0, p.line))
            .corner_radius(CornerRadius::same(m.radius_md as u8))
            .inner_margin(Margin::same(m.space_3 as i8))
            .show(ui, |ui| {
                ui.label(RichText::new(pr.ask.title()).text_style(TextStyle::Name("title".into())).color(p.ink));
                if pr.ask == Ask::DropInstance {
                    let what = self
                        .doc
                        .as_ref()
                        .zip(self.selected.as_ref())
                        .and_then(|(d, s)| d.instance_of(s).and_then(|i| d.element(&i).map(|e| d.label(e))))
                        .unwrap_or_default();
                    ui.label(
                        RichText::new(format!(
                            "Remove instance {what}?\n\nList entries pointing to it are removed, other references to it \
                             become null."
                        ))
                        .color(p.ink),
                    );
                }
                for (i, label) in pr.ask.labels().iter().enumerate() {
                    ui.label(RichText::new(*label).color(p.ink_muted));
                    if let Some(f) = pr.fields.get_mut(i) {
                        let r = ui.add(egui::TextEdit::singleline(f).desired_width(f32::INFINITY));
                        self.rects.push((format!("field {i}"), r.rect));
                    }
                }
                // the lists: a bundle's entries, the level's files, another file's instances and fields
                let filter_at = match pr.ask {
                    Ask::OpenBundle | Ask::Graft | Ask::CopyField => Some(1),
                    Ask::LevelFile => Some(0),
                    _ => None,
                };
                if let Some(fi) = filter_at {
                    ui.horizontal(|ui| {
                        let read = match pr.ask {
                            Ask::OpenBundle => Some("List entries"),
                            Ask::Graft | Ask::CopyField => Some("Read file"),
                            _ => None,
                        };
                        if let Some(label) = read
                            && ui.button(label).clicked()
                        {
                            let path = pr.fields.first().cloned().unwrap_or_default();
                            pr.error = None;
                            pr.chosen = None;
                            pr.instance = None;
                            match pr.ask {
                                Ask::OpenBundle => match DbxDoc::bundle_entries(Path::new(path.trim())) {
                                    Ok(e) => pr.list = e.into_iter().map(|x| (x.clone(), x)).collect(),
                                    Err(e) => pr.error = Some(e),
                                },
                                _ => match read_dbx(path.trim()) {
                                    Ok(f) => {
                                        pr.list = instances(&f);
                                        pr.other = Some(f);
                                    }
                                    Err(e) => pr.error = Some(e),
                                },
                            }
                        }
                    });
                    if pr.ask == Ask::LevelFile {
                        pr.list = self.level_files.as_ref().map_or_else(Vec::new, |(_, l)| {
                                l.iter()
                                    .map(|x| {
                                        let shown = level
                                            .and_then(|lv| lv.bases.iter().find_map(|b| x.strip_prefix(b).ok()))
                                            .map_or_else(|| x.display().to_string(), |r| r.display().to_string());
                                        (x.display().to_string(), shown)
                                    })
                                    .collect()
                            });
                    }
                    let filter = pr.fields.get(fi).cloned().unwrap_or_default().to_lowercase();
                    let shown: Vec<(String, String)> = pr
                        .list
                        .iter()
                        .filter(|(_, t)| filter.is_empty() || t.to_lowercase().contains(&filter))
                        .take(500)
                        .cloned()
                        .collect();
                    if !pr.list.is_empty() {
                        ui.label(
                            RichText::new(format!("{} of {}", shown.len(), pr.list.len()))
                                .text_style(TextStyle::Name("small".into()))
                                .color(p.ink_muted),
                        );
                    }
                    egui::ScrollArea::vertical().max_height(180.0).id_salt("dbx prompt list").show(ui, |ui| {
                        for (key, text) in &shown {
                            let on = pr.chosen.as_deref() == Some(key.as_str());
                            let r = ui.selectable_label(on, RichText::new(text).text_style(TextStyle::Name("value".into())));
                            if r.clicked() {
                                pr.chosen = Some(key.clone());
                            }
                            if r.double_clicked() {
                                ok = true;
                            }
                        }
                    });
                    // Copy field: the instance first, then its fields
                    if pr.ask == Ask::CopyField
                        && pr.instance.is_none()
                        && let (Some(inst), Some(f)) = (pr.chosen.clone(), pr.other.as_ref())
                        && ui.button("Choose this instance's field").clicked()
                    {
                        pr.list = fields_of(f, &inst);
                        pr.instance = Some(inst);
                        pr.chosen = None;
                    }
                }
                if let Some(e) = &pr.error {
                    ui.label(RichText::new(e).color(p.status_error));
                }
                ui.horizontal(|ui| {
                    let r = ui.button(RichText::new("OK").color(p.ink));
                    self.rects.push(("OK".into(), r.rect));
                    ok |= r.clicked();
                    cancel = ui.button("Cancel").clicked();
                });
            });
        if cancel {
            return;
        }
        self.prompt = Some(pr);
        if ok {
            let scratch = std::env::temp_dir().join(format!("bc2-editor-dbx-bundle-{}", std::process::id()));
            self.answer(level, &scratch);
        }
    }

    fn tree_ui(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics) {
        if self.doc.is_none() {
            ui.centered_and_justified(|ui| {
                ui.label(
                    RichText::new("Open a binary .dbx file: Open, From bundle, or drop it on the window.")
                        .color(p.ink_muted),
                );
            });
            return;
        }
        let row_h = m.row;
        let reveal = std::mem::take(&mut self.reveal);
        let selected = self.selected.clone();
        let rows = self.rows().to_vec();
        let at = selected.as_ref().and_then(|s| rows.iter().position(|(r, _)| r == s));
        let mut toggle = None;
        let mut pick = None;
        let mut sa = egui::ScrollArea::both().auto_shrink([false, false]).id_salt("dbx tree");
        if reveal && let Some(i) = at {
            sa = sa.vertical_scroll_offset((i as f32 * row_h - 120.0).max(0.0));
        }
        let Some(d) = self.doc.as_ref() else { return };
        sa.show_rows(ui, row_h, rows.len(), |ui, range| {
            for (path, depth) in rows.get(range).unwrap_or_default() {
                let Some(e) = d.element(path) else { continue };
                let label = d.label(e);
                let instance = d.tag(e) == "instance";
                let on = selected.as_ref() == Some(path);
                let (rect, resp) =
                    ui.allocate_exact_size(Vec2::new(ui.available_width().max(400.0), row_h), Sense::click());
                if on {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(m.radius_sm as u8), p.accent_subtle);
                } else if resp.hovered() {
                    ui.painter()
                        .rect_filled(rect, CornerRadius::same(m.radius_sm as u8), p.surface_3);
                }
                let x = rect.left() + m.space_2 + *depth as f32 * 14.0;
                if e.is_container() && !e.kids().is_empty() {
                    let open = self.expanded.contains(path);
                    let ar = egui::Rect::from_min_size(egui::pos2(x, rect.top()), Vec2::new(12.0, row_h));
                    // the disclosure triangle, drawn (the interface font has no arrow glyphs)
                    let c = ar.center();
                    let pts = if open {
                        vec![
                            c + Vec2::new(-4.0, -2.0),
                            c + Vec2::new(4.0, -2.0),
                            c + Vec2::new(0.0, 3.0),
                        ]
                    } else {
                        vec![
                            c + Vec2::new(-2.0, -4.0),
                            c + Vec2::new(3.0, 0.0),
                            c + Vec2::new(-2.0, 4.0),
                        ]
                    };
                    ui.painter()
                        .add(egui::Shape::convex_polygon(pts, p.ink_muted, Stroke::NONE));
                    if resp.clicked() && resp.interact_pointer_pos().is_some_and(|q| ar.contains(q)) {
                        toggle = Some(path.clone());
                    }
                }
                let colour = if d.get(e, "ref").is_some() {
                    p.accent_text
                } else {
                    p.ink
                };
                let font = if instance {
                    TextStyle::Name("body-strong".into())
                } else {
                    TextStyle::Name("body".into())
                };
                let font_id = font.resolve(ui.style());
                ui.painter().text(
                    egui::pos2(x + 14.0, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    label,
                    font_id,
                    colour,
                );
                if resp.clicked() && toggle.is_none() {
                    pick = Some(path.clone());
                }
                if resp.double_clicked() && e.is_container() {
                    toggle = Some(path.clone());
                }
                self.rects.push((format!("row {}", join(path)), rect));
            }
        });
        if let Some(t) = toggle {
            if !self.expanded.remove(&t) {
                self.expanded.insert(t);
            }
            self.rows = None;
        }
        if let Some(s) = pick {
            self.status = None;
            self.select(Some(s));
            self.reveal = false;
        }
    }

    fn element_ui(&mut self, ui: &mut egui::Ui, p: &Palette, m: &Metrics, level: Option<&LevelLink<'_>>) -> Option<Op> {
        let (Some(d), Some(path)) = (self.doc.as_ref(), self.selected.clone()) else {
            ui.label(RichText::new("Select an element of the tree to see and edit it.").color(p.ink_muted));
            return None;
        };
        let e = d.element(&path).cloned()?;
        let heading = d.heading(&e);
        let names: Vec<String> = d.attributes(&e).into_iter().map(|a| a.0).collect();
        let reference = d.get(&e, "ref").filter(|r| r != "null");
        let hints = dbxinspect::denormal_hint(&d.file.table, &e);
        let project = d.is_project();
        ui.label(
            RichText::new(heading)
                .text_style(TextStyle::Name("title".into()))
                .color(p.ink),
        );
        ui.add_space(m.space_1);
        ui.label(
            RichText::new("ATTRIBUTES")
                .text_style(TextStyle::Name("label".into()))
                .color(p.ink_muted),
        );
        egui::Grid::new("dbx attributes")
            .num_columns(2)
            .striped(true)
            .show(ui, |ui| {
                for (i, n) in names.iter().enumerate() {
                    ui.label(RichText::new(n).color(p.ink_muted));
                    if let Some(v) = self.attr_edit.get_mut(i) {
                        let r = ui.add(
                            egui::TextEdit::singleline(v)
                                .font(TextStyle::Name("value".into()))
                                .desired_width(f32::INFINITY),
                        );
                        self.rects.push((format!("attr {n}"), r.rect));
                    }
                    ui.end_row();
                }
            });
        ui.add_space(m.space_2);
        let mut op = None;
        if !e.is_container() {
            ui.label(
                RichText::new("VALUE (NUMBERS SEPARATED BY /)")
                    .text_style(TextStyle::Name("label".into()))
                    .color(p.ink_muted),
            );
            let r = ui.add(
                egui::TextEdit::multiline(&mut self.value_edit)
                    .font(TextStyle::Name("value".into()))
                    .desired_rows(5)
                    .desired_width(f32::INFINITY),
            );
            self.rects.push(("value".into(), r.rect));
            for (i, bits, f) in hints.iter().take(8) {
                ui.label(
                    RichText::new(format!(
                        "Number {} prints as {bits}: a whole number, or the tiny decimal {f:e} stored in the same bits \
                         (0x{bits:08X}); an edit keeps whichever it is",
                        i + 1
                    ))
                    .text_style(TextStyle::Name("small".into()))
                    .color(p.status_info),
                );
            }
        }
        ui.horizontal(|ui| {
            let tip = if project {
                "Set the field in the project (an operation in its History)"
            } else {
                "Store the attributes and the value in the file"
            };
            if self.button(ui, p, "Apply", true, tip) {
                op = self.apply(level.and_then(|l| l.doc));
            }
            if self.button(
                ui,
                p,
                "Go to reference",
                reference.is_some(),
                "The instance the reference names",
            ) {
                let target = self.doc.as_mut().and_then(|d| d.go_to_reference(&path));
                if target.is_some() {
                    self.select(target);
                }
            }
            if self.button(ui, p, "Revert", true, "The boxes back to the file's values") {
                self.load_edit();
            }
        });
        if project {
            ui.label(
                RichText::new(
                    "The project's partition: field edits go into the project (History, undo, save with the project). \
                     Save as makes a file of it for the other edits.",
                )
                .text_style(TextStyle::Name("small".into()))
                .color(p.ink_muted),
            );
        }
        op
    }

    fn output_ui(&mut self, ui: &mut egui::Ui, p: &Palette) {
        let Some(d) = self.doc.as_ref() else { return };
        let mut go = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .stick_to_bottom(true)
            .id_salt("dbx output")
            .show(ui, |ui| {
                for line in &d.log {
                    let colour = if line.starts_with("ERROR") {
                        p.status_error
                    } else if line.starts_with("WARN") {
                        p.status_warn
                    } else {
                        p.ink
                    };
                    let r = ui.add(
                        egui::Label::new(
                            RichText::new(line)
                                .text_style(TextStyle::Name("value".into()))
                                .color(colour),
                        )
                        .sense(Sense::click()),
                    );
                    if r.double_clicked() {
                        go = d.guid_on_line(line);
                    }
                }
            });
        if go.is_some() {
            self.select(go);
        }
    }
}

/// A path as text (`0,3,1`).
fn join(path: &[usize]) -> String {
    path.iter().map(ToString::to_string).collect::<Vec<_>>().join(",")
}

/// Another file's instances for a list: (guid, `type   id   guid`).
fn instances(f: &EditFile) -> Vec<(String, String)> {
    let t = &f.table;
    f.instances().map_or_else(
        |_| Vec::new(),
        |it| {
            it.filter_map(Result::ok)
                .filter_map(|i| {
                    let g = t.get(i, "guid").ok().flatten()?.to_owned();
                    let ty = t.get(i, "type").ok().flatten().unwrap_or("");
                    let short = ty.rfind('.').map_or(ty, |k| ty.get(k + 1..).unwrap_or(""));
                    let id = t.get(i, "id").ok().flatten().unwrap_or("");
                    Some((g.clone(), format!("{short}   {id}   {g}")))
                })
                .collect()
        },
    )
}

/// An instance's field names (each once) for a list.
fn fields_of(f: &EditFile, guid: &str) -> Vec<(String, String)> {
    let t = &f.table;
    let mut seen = HashSet::new();
    f.instance(guid).ok().flatten().map_or_else(Vec::new, |i| {
        i.kids()
            .iter()
            .filter_map(|c| t.get(c, "name").ok().flatten())
            .filter(|n| seen.insert((*n).to_owned()))
            .map(|n| (n.to_owned(), n.to_owned()))
            .collect()
    })
}
