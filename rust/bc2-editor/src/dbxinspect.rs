//! The DBX inspector's model (round GR4: `DbxEditGUI` folded into the map editor; its window is
//! [`crate::dbxpanel`]): any binary `.dbx` - a loose file, an entry of a bundle, or the open project's level
//! partition - as DbxEdit holds it ([`bc2_formats::dbxedit::EditFile`]: whatever an edit does not touch is written
//! back byte for byte), its tree, and every edit `DbxEditGUI` made, each through the same `dbxedit` code its command
//! line runs, with `DbxEditGUI`'s messages and edit script lines:
//!
//! | `DbxEditGUI` (`MainWindow`) | here |
//! |---|---|
//! | `Open`, `Save` (the first save keeps `<name>.bak`), Save as | [`DbxDoc::open`], [`DbxDoc::save`] |
//! | the tree: `Label`, the heading, the attributes, the value | [`DbxDoc::label`], [`DbxDoc::heading`], [`DbxDoc::attributes`], [`DbxDoc::value`] |
//! | `ApplyEdit` (attributes, then the value typed like the field), `GoToReference` | [`DbxDoc::apply_edit`], [`DbxDoc::go_to_reference`] |
//! | `Undo` (every edit snapshotted) | [`DbxDoc::undo`] |
//! | Instance menu: remove, graft, copy field, add item | [`DbxDoc::drop_instance`], [`DbxDoc::graft`], [`DbxDoc::copy_field`], [`DbxDoc::add_item`] |
//! | Edit menu: replace, replace prefix, remove a field everywhere, set a bool everywhere, the partition's GUID | [`DbxDoc::replace`], [`DbxDoc::replace_prefix`], [`DbxDoc::drop_field`], [`DbxDoc::set_byte`], [`DbxDoc::partition_guid`] |
//! | Find (Ctrl+F), Find next (F3) | [`DbxDoc::find`] |
//! | Level menu: check, check against a cull level, fix slots | [`DbxDoc::check_level`], [`DbxDoc::fix_slots`] |
//! | Export XML, Import XML, Save edit script | [`DbxDoc::export_xml`], [`DbxDoc::import_xml`], [`DbxDoc::save_script`] |
//!
//! Added here: a bundle's entry opened in memory and saved into a copy of the bundle ([`DbxDoc::save_into_bundle`],
//! FbrbTool's unpack and pack: every other entry byte for byte), the open project's partition, whose field edits go
//! into the project's log as operations ([`project_field`]), and the stored bits of a 4-byte number the XML prints
//! as a whole number beside it ([`denormal_hint`]: DbxEdit's XML shows a tiny decimal as its bits, 7.3e-40 as 524288;
//! the edit still keeps the bits, as DbxEdit does).
//!
//! An element is named by its path of indexes from the file's top level ([`NodePath`]).

use std::path::{Path, PathBuf};

use bc2_formats::dbxedit::{self as dx, CONTAINER, EditFile, Element, NUMBERS, TEXT, Table, editor};
use bc2_levelwrite::{FieldType, FieldValue};

/// An element's place: its index in the top level, then in each container down to it.
pub type NodePath = Vec<usize>;

/// Where the open file came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    /// A loose `.dbx` file.
    File(PathBuf),
    /// An entry of a bundle (read in memory; saved into a copy of the bundle, or as a file).
    Bundle {
        /// The bundle.
        bundle: PathBuf,
        /// The entry's archive name.
        entry: String,
    },
    /// The open project's level partition as its log makes it: its field edits are the project's operations.
    Project {
        /// The partition's name (`levels/MP_005`).
        partition: String,
        /// The project's revision it was read at.
        revision: u64,
    },
}

/// A `.dbx` open in the inspector.
#[derive(Debug, Clone)]
pub struct DbxDoc {
    /// The file as DbxEdit holds it.
    pub file: EditFile,
    /// Where it came from.
    pub source: Source,
    /// The path the window names it by (and saves to): the file, the bundle's entry, the partition.
    pub path: String,
    /// Changed since it was opened or saved.
    pub modified: bool,
    backed_up: bool,
    undo: Vec<Vec<u8>>,
    /// The edits as DbxEdit commands (File > Save edit script).
    pub script: Vec<String>,
    /// The output pane's lines.
    pub log: Vec<String>,
}

/// What an edit did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Applied {
    /// Nothing differed from the file.
    Unchanged,
    /// The file changed (an undo step, a script line).
    Changed,
}

/// The file name of a path as `Path.GetFileName` gives it (either slash).
#[must_use]
pub fn file_name(path: &str) -> &str {
    dx::net::file_name(path)
}

/// `.NET`'s `IndexOf(s, StringComparison.OrdinalIgnoreCase) >= 0`.
fn contains_ignore_case(hay: &str, needle: &str) -> bool {
    needle.is_empty() || dx::net::to_upper(hay).contains(&dx::net::to_upper(needle))
}

/// The first `n` UTF-16 units of `s` (`Substring(0, n)`).
fn utf16_prefix(s: &str, n: usize) -> String {
    let units: Vec<u16> = s.encode_utf16().take(n).collect();
    String::from_utf16_lossy(&units)
}

/// The 32-bit numbers of an element that the XML prints as whole numbers though they may be decimals too small to
/// print (the denormal trap: DbxEdit's XML shows 7.3e-40 as 524288, its bits), with that decimal: each
/// `(index, bits, decimal)`, from 65,536 up. Empty for fields of whole numbers (hashes, ids) and anything else.
#[must_use]
pub fn denormal_hint(t: &Table, e: &Element) -> Vec<(usize, u32, f32)> {
    if e.kind != NUMBERS {
        return Vec::new();
    }
    let field = t.first_value(e).unwrap_or("");
    if bc2_formats::dbx::WHOLE_NUMBER_FIELDS.contains(&field) {
        return Vec::new();
    }
    let mut pos = 0;
    let (Ok(n), Ok(size)) = (
        dx::read_varint(&e.payload, &mut pos),
        dx::read_varint(&e.payload, &mut pos),
    ) else {
        return Vec::new();
    };
    if size != 4 || n <= 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..n as usize {
        let Some(bits) = e
            .payload
            .get(pos + 4 * i..pos + 4 * i + 4)
            .and_then(|b| <[u8; 4]>::try_from(b).ok())
            .map(u32::from_be_bytes)
        else {
            break;
        };
        // small whole numbers (indices, counts) are left out: below 65,536 a decimal would be smaller than 1e-40
        if bits >= 0x1_0000 && bits >> 24 == 0 {
            out.push((i, bits, f32::from_bits(bits)));
        }
    }
    out
}

impl DbxDoc {
    /// The file at `path` (`DbxEditGUI`'s `Open`): a binary `.dbx` only.
    ///
    /// # Errors
    /// It cannot be read, is damaged, or is not a binary `.dbx` (the window's words).
    pub fn open(path: &Path) -> Result<Self, String> {
        let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path.display().to_string();
        let mut d = Self::from_bytes(data, Source::File(path.to_path_buf()), &name)?;
        d.log.push(format!("opened {name}"));
        Ok(d)
    }

    /// An entry of a bundle, read in memory.
    ///
    /// # Errors
    /// The bundle or entry cannot be read, or the entry is not a binary `.dbx`.
    pub fn open_bundle_entry(bundle: &Path, entry: &str) -> Result<Self, String> {
        let data = bc2_formats::fbrb::read_path(bundle, &[entry])
            .map_err(|e| format!("{}: {entry}: {e}", bundle.display()))?
            .into_iter()
            .next()
            .ok_or_else(|| format!("{}: no entry {entry}", bundle.display()))?;
        let name = format!("{}|{entry}", bundle.display());
        let mut d = Self::from_bytes(
            data,
            Source::Bundle {
                bundle: bundle.to_path_buf(),
                entry: entry.to_owned(),
            },
            &name,
        )?;
        d.log.push(format!("opened {entry} from {}", bundle.display()));
        Ok(d)
    }

    /// A file's bytes, named `name`.
    ///
    /// # Errors
    /// Damaged, or not a binary `.dbx`.
    pub fn from_bytes(data: Vec<u8>, source: Source, name: &str) -> Result<Self, String> {
        let file = EditFile::parse(data).map_err(|e| format!("{name}: {}", e.message))?;
        if !file.is_binary {
            return Err(format!(
                "{name}\n\nis not a binary .dbx file (it may be plain XML already)."
            ));
        }
        Ok(Self {
            file,
            source,
            path: name.to_owned(),
            modified: false,
            backed_up: false,
            undo: Vec::new(),
            script: Vec::new(),
            log: Vec::new(),
        })
    }

    /// The bundle entries that may be `.dbx` files (every live entry but known non-DBX types), in archive order.
    ///
    /// # Errors
    /// The bundle's header cannot be read.
    pub fn bundle_entries(bundle: &Path) -> Result<Vec<String>, String> {
        let h = bc2_formats::fbrb::entries_path(bundle).map_err(|e| format!("{}: {e}", bundle.display()))?;
        Ok(h.entries
            .iter()
            .filter(|e| !e.is_deleted() && e.name.to_ascii_lowercase().ends_with(".dbx"))
            .map(|e| e.name.clone())
            .collect())
    }

    /// The file's own name (the title, the check's heading).
    #[must_use]
    pub fn name(&self) -> &str {
        match &self.source {
            Source::Bundle { entry, .. } => file_name(entry),
            Source::Project { partition, .. } => file_name(partition),
            Source::File(_) => file_name(&self.path),
        }
    }

    /// Whether edits go into the open project (its level partition) rather than this file.
    #[must_use]
    pub fn is_project(&self) -> bool {
        matches!(self.source, Source::Project { .. })
    }

    /// How many edits Undo can take back.
    #[must_use]
    pub fn undo_steps(&self) -> usize {
        self.undo.len()
    }

    fn t(&self) -> &Table {
        &self.file.table
    }

    /// The element at `path`.
    #[must_use]
    pub fn element(&self, path: &[usize]) -> Option<&Element> {
        let (first, rest) = path.split_first()?;
        let mut e = self.file.top_level.get(*first)?;
        for &i in rest {
            e = e.children.as_ref()?.get(i)?;
        }
        Some(e)
    }

    fn element_mut(&mut self, path: &[usize]) -> Option<&mut Element> {
        let (first, rest) = path.split_first()?;
        let mut e = self.file.top_level.get_mut(*first)?;
        for &i in rest {
            e = e.children.as_mut()?.get_mut(i)?;
        }
        Some(e)
    }

    /// An attribute of an element (`Get`), `None` when it has none or the file is damaged there.
    #[must_use]
    pub fn get(&self, e: &Element, name: &str) -> Option<String> {
        self.t().get(e, name).ok().flatten().map(str::to_owned)
    }

    /// An element's tag.
    #[must_use]
    pub fn tag(&self, e: &Element) -> String {
        self.t().tag(e).unwrap_or("").to_owned()
    }

    /// An element's value as the XML writes it (`ValueText`; empty for a container or a damaged value).
    #[must_use]
    pub fn value(&self, e: &Element) -> String {
        if e.is_container() {
            return String::new();
        }
        dx::value_text(self.t(), e).unwrap_or_default()
    }

    /// An element's attributes, `(name, value)`.
    #[must_use]
    pub fn attributes(&self, e: &Element) -> Vec<(String, String)> {
        e.attributes
            .iter()
            .map(|&(k, v)| {
                (
                    self.t().s(k).unwrap_or("").to_owned(),
                    self.t().s(v).unwrap_or("").to_owned(),
                )
            })
            .collect()
    }

    /// The tree's row of an element (`DbxEditGUI`'s `Label`): the partition and its guid, an instance's short type and
    /// id (or guid), a reference `name -> ref`, a container `name  [count]`, a value `name = value` (90 characters).
    #[must_use]
    pub fn label(&self, e: &Element) -> String {
        let tag = self.tag(e);
        if tag == "partition" {
            return format!("partition {}", self.get(e, "guid").unwrap_or_default());
        }
        if tag == "instance" {
            let ty = self.get(e, "type").unwrap_or_default();
            let short = ty.rfind('.').map_or(ty.as_str(), |i| ty.get(i + 1..).unwrap_or(""));
            let id = self.get(e, "id").or_else(|| self.get(e, "guid")).unwrap_or_default();
            return format!("{short}   {id}");
        }
        let head = self.get(e, "name").unwrap_or(tag);
        if let Some(r) = self.get(e, "ref") {
            return format!("{head} -> {r}");
        }
        if e.is_container() {
            return format!("{head}  [{}]", e.kids().len());
        }
        let mut v = self.value(e);
        if v.encode_utf16().count() > 90 {
            v = utf16_prefix(&v, 90) + "...";
        }
        format!("{head} = {}", v.replace(['\r', '\n'], " "))
    }

    /// The value pane's heading: the tag and the kind (`container`, `text`, `N numbers of S bytes`, `bool / byte`).
    #[must_use]
    pub fn heading(&self, e: &Element) -> String {
        let kind = if e.is_container() {
            "container".to_owned()
        } else if e.kind == TEXT {
            "text".to_owned()
        } else if e.kind == NUMBERS {
            let mut p = 0;
            let n = dx::read_varint(&e.payload, &mut p).unwrap_or(0);
            let size = dx::read_varint(&e.payload, &mut p).unwrap_or(0);
            format!("{n} number{} of {size} bytes", if n == 1 { "" } else { "s" })
        } else {
            "bool / byte".to_owned()
        };
        format!("{}  ({kind})", self.tag(e))
    }

    /// The nearest instance at or above `path` (`InstanceOf`).
    #[must_use]
    pub fn instance_of(&self, path: &[usize]) -> Option<NodePath> {
        (1..=path.len())
            .rev()
            .map(|n| path.get(..n).unwrap_or_default())
            .find(|p| self.element(p).is_some_and(|e| self.tag(e) == "instance"))
            .map(<[usize]>::to_vec)
    }

    /// The guid of the instance at or above `path` (Copy GUID).
    #[must_use]
    pub fn guid_of(&self, path: &[usize]) -> Option<String> {
        let i = self.instance_of(path)?;
        self.element(&i).and_then(|e| self.get(e, "guid"))
    }

    /// The path of the instance with this guid (`Instance(guid)`: the partition's first, any case).
    #[must_use]
    pub fn instance_path(&self, guid: &str) -> Option<NodePath> {
        let p = self.file.top_level.first()?;
        p.kids()
            .iter()
            .position(|e| {
                self.tag(e) == "instance" && self.get(e, "guid").is_some_and(|g| dx::net::same_ignore_case(&g, guid))
            })
            .map(|i| vec![0, i])
    }

    fn snapshot(&mut self) -> Result<(), String> {
        let b = self.bytes()?;
        self.undo.push(b);
        self.modified = true;
        Ok(())
    }

    /// The file's bytes now (`Serialize`).
    ///
    /// # Errors
    /// An element with more than 15 attributes, nesting too deep.
    pub fn bytes(&self) -> Result<Vec<u8>, String> {
        self.file.serialize().map_err(|e| e.message)
    }

    fn restore(&mut self, bytes: Vec<u8>) {
        if let Ok(f) = EditFile::parse(bytes) {
            self.file = f;
        }
    }

    /// Edit > Undo: the file as it was before the last edit (its script line goes too).
    pub fn undo(&mut self) {
        let Some(b) = self.undo.pop() else {
            self.log.push("Nothing to undo.".into());
            return;
        };
        self.restore(b);
        self.script.pop();
        self.modified = !self.undo.is_empty();
        self.log.push("Undone.".into());
    }

    /// Apply (`ApplyEdit`): the element's attributes set to `attrs` (by position), then its value to `value` (the
    /// value box's text) typed like the field, as the XML import types a changed value - a decimal rounded to the
    /// nearest 4-byte value is said in the output. Nothing changes when nothing differs.
    ///
    /// # Errors
    /// The value is not one of the field's type (the file is left as it was), or no such element.
    pub fn apply_edit(&mut self, path: &[usize], attrs: &[String], value: &str) -> Result<Applied, String> {
        let before = self.bytes()?;
        match self.apply_inner(path, attrs, value) {
            Ok(Some(rounded)) => {
                self.undo.push(before);
                self.modified = true;
                let e = self.element(path).cloned().unwrap_or_default();
                if let Some(v) = rounded {
                    self.log.push(format!("Rounded to the nearest 4-byte value: {v}"));
                }
                let inst = self.instance_of(path);
                let name = self.get(&e, "name");
                let direct = inst
                    .as_ref()
                    .is_some_and(|i| i.len() + 1 == path.len() && i.as_slice() != path);
                let line = match (&inst, name) {
                    (Some(i), Some(n)) if direct && !e.is_container() => {
                        let guid = self.element(i).and_then(|x| self.get(x, "guid")).unwrap_or_default();
                        format!("setfield={guid}|{n}|{}", self.value(&e))
                    }
                    _ => {
                        let where_ = inst
                            .as_ref()
                            .and_then(|i| self.element(i))
                            .map_or_else(|| "partition".to_owned(), |x| self.get(x, "guid").unwrap_or_default());
                        format!("# edited {} in {where_} (no command form)", self.label(&e))
                    }
                };
                self.script.push(line);
                Ok(Applied::Changed)
            }
            Ok(None) => Ok(Applied::Unchanged),
            Err(e) => {
                self.restore(before);
                Err(e)
            }
        }
    }

    /// `Ok(None)` nothing differed; `Ok(Some(rounded))` changed, with the rounded value's text when one was.
    #[allow(clippy::option_option, reason = "changed or not, then rounded or not")]
    fn apply_inner(&mut self, path: &[usize], attrs: &[String], value: &str) -> Result<Option<Option<String>>, String> {
        let mut e = self.element(path).cloned().ok_or("no such element")?;
        let mut changed = false;
        for (i, a) in attrs.iter().enumerate() {
            let Some(slot) = e.attributes.get(i).copied() else {
                break;
            };
            if self.t().s(slot.1).ok() != Some(a.as_str()) {
                let id = self.file.table.intern(a);
                if let Some(s) = e.attributes.get_mut(i) {
                    s.1 = id;
                }
                changed = true;
            }
        }
        let mut rounded = None;
        if !e.is_container() && value != dx::value_text(self.t(), &e).unwrap_or_default() {
            let (payload, r) = dx::encode_value_rounded(&mut self.file, &e, value).map_err(|f| f.message)?;
            e.payload = payload;
            changed = true;
            if r {
                rounded = Some(dx::value_text(self.t(), &e).unwrap_or_default());
            }
        }
        if !changed {
            return Ok(None);
        }
        if let Some(slot) = self.element_mut(path) {
            *slot = e;
        }
        Ok(Some(rounded))
    }

    /// Go to reference: the instance a reference names, when this file has it (else the output says where it
    /// points).
    pub fn go_to_reference(&mut self, path: &[usize]) -> Option<NodePath> {
        let r = self.element(path).and_then(|e| self.get(e, "ref"))?;
        if let Some(p) = self.instance_path(&r) {
            return Some(p);
        }
        self.log.push(format!(
            "Reference {r}{}",
            if r.find('/').is_some_and(|i| i > 0) {
                " points into another file."
            } else {
                " is not in this file."
            }
        ));
        None
    }

    /// Instance > Remove instance: the instance at or above `path`; list entries pointing to it go, other
    /// references to it become `null`.
    ///
    /// # Errors
    /// A damaged file.
    pub fn drop_instance(&mut self, path: &[usize]) -> Result<(), String> {
        let Some(guid) = self.guid_of(path) else { return Ok(()) };
        self.snapshot()?;
        let (n, refs) = editor::drop(&mut self.file, &guid).map_err(|e| e.message)?;
        self.script.push(format!("drop={guid}"));
        self.log
            .push(format!("drop {guid}: {n} instance(s), {refs} reference(s)"));
        Ok(())
    }

    /// Instance > Graft instance from another file: instance `guid` of `source` (the file at `source_path`) and
    /// every instance of it that it references, before the instance at or above `at` (else at the end). Returns the
    /// grafted instance's path.
    ///
    /// # Errors
    /// A damaged file.
    pub fn graft(
        &mut self,
        source_path: &str,
        source: &EditFile,
        guid: &str,
        at: Option<&[usize]>,
    ) -> Result<Option<NodePath>, String> {
        let before = at.and_then(|p| self.guid_of(p));
        self.snapshot()?;
        let n = editor::graft(&mut self.file, source, guid, before.as_deref()).map_err(|e| e.message)?;
        self.script.push(format!(
            "graft={source_path}|{guid}{}",
            before.as_ref().map_or_else(String::new, |b| format!("|{b}"))
        ));
        self.log.push(format!(
            "graft {guid} from {}: {n} instance(s){}",
            file_name(source_path),
            if before.is_some() {
                " (inserted before the selected instance)"
            } else {
                ""
            }
        ));
        Ok(self.instance_path(guid))
    }

    /// Instance > Copy field from another file: field `field` of instance `source_guid` of `source` into the
    /// instance at or above `at` (replacing its field of that name, or added at its end).
    ///
    /// # Errors
    /// A damaged file.
    pub fn copy_field(
        &mut self,
        at: &[usize],
        source_path: &str,
        source: &EditFile,
        source_guid: &str,
        field: &str,
    ) -> Result<(), String> {
        let Some(guid) = self.guid_of(at) else {
            self.log.push("Select the instance to copy the field into.".into());
            return Ok(());
        };
        self.snapshot()?;
        let ok = editor::copy_field(&mut self.file, source, source_guid, field, &guid).map_err(|e| e.message)?;
        if !ok {
            self.undo.pop();
            self.log.push(format!("copyfield {field}: NOT FOUND"));
            return Ok(());
        }
        self.script
            .push(format!("copyfield={source_path}|{source_guid}|{field}|{guid}"));
        self.log.push(format!("copyfield {field}: copied into {guid}"));
        Ok(())
    }

    /// Whether `path` is an array field of an instance (Add array item's target).
    #[must_use]
    pub fn is_instance_array(&self, path: &[usize]) -> bool {
        let Some(inst) = self.instance_of(path) else {
            return false;
        };
        inst.len() + 1 == path.len() && self.element(path).is_some_and(|e| self.tag(e) == "array")
    }

    /// Instance > Add array item: a reference `reference` added at the end of the array field at `path` (an item
    /// shaped like its first).
    ///
    /// # Errors
    /// A damaged file.
    pub fn add_item(&mut self, path: &[usize], reference: &str) -> Result<(), String> {
        if !self.is_instance_array(path) {
            self.log
                .push("Select an array field of an instance (a direct field, e.g. Objects).".into());
            return Ok(());
        }
        if reference.is_empty() {
            return Ok(());
        }
        self.snapshot()?;
        let guid = self.guid_of(path).unwrap_or_default();
        let array = self.element(path).and_then(|e| self.get(e, "name")).unwrap_or_default();
        if !editor::add_item(&mut self.file, &guid, &array, reference, None).map_err(|e| e.message)? {
            self.undo.pop();
            self.log
                .push("additem: the array has no item to copy the shape from".into());
            return Ok(());
        }
        self.script.push(format!("additem={guid}|{array}|{reference}"));
        self.log.push(format!("additem {reference} -> {array}"));
        Ok(())
    }

    /// Edit > Replace string: every string exactly `from` becomes `to` (re-pointing references).
    ///
    /// # Errors
    /// The file cannot be written (its snapshot).
    pub fn replace(&mut self, from: &str, to: &str) -> Result<(), String> {
        if from.is_empty() {
            return Ok(());
        }
        self.snapshot()?;
        let n = editor::replace(&mut self.file, from, to);
        if n == 0 {
            self.undo.pop();
            self.log.push(format!("replace: not found: {from}"));
            return Ok(());
        }
        self.script.push(format!("replace={from}|{to}"));
        self.log.push(format!("replace {from} -> {to}: {n} string(s)"));
        Ok(())
    }

    /// Edit > Replace string prefix: strings starting with `from` (any case) and none of `except` (`|`-separated)
    /// get `to` as their start.
    ///
    /// # Errors
    /// The file cannot be written (its snapshot).
    pub fn replace_prefix(&mut self, from: &str, to: &str, except: &str) -> Result<(), String> {
        if from.is_empty() {
            return Ok(());
        }
        self.snapshot()?;
        let list: Vec<String> = except.split('|').filter(|x| !x.is_empty()).map(str::to_owned).collect();
        let n = editor::replace_prefix(&mut self.file, from, to, &list);
        self.script.push(format!(
            "replaceprefix={from}|{to}{}",
            if list.is_empty() {
                String::new()
            } else {
                format!("|{}", list.join("|"))
            }
        ));
        self.log.push(format!("replaceprefix {from} -> {to}: {n} string(s)"));
        Ok(())
    }

    /// Edit > Remove field everywhere: every field, array or complex named `name`.
    ///
    /// # Errors
    /// A damaged file.
    pub fn drop_field(&mut self, name: &str) -> Result<(), String> {
        if name.is_empty() {
            return Ok(());
        }
        self.snapshot()?;
        let n = editor::drop_field(&mut self.file, name).map_err(|e| e.message)?;
        self.script.push(format!("dropfield={name}"));
        self.log.push(format!("dropfield {name}: {n} element(s)"));
        Ok(())
    }

    /// Edit > Set bool / byte field everywhere: `true`, `false` or 0-255.
    ///
    /// # Errors
    /// A damaged file.
    pub fn set_byte(&mut self, name: &str, value: &str) -> Result<(), String> {
        if name.is_empty() {
            return Ok(());
        }
        let b = match value {
            "true" => 1,
            "false" => 0,
            v => {
                let Ok(b) = dx::net::parse_byte(v) else {
                    self.log.push("set: not true, false or 0-255".into());
                    return Ok(());
                };
                b
            }
        };
        self.snapshot()?;
        let n = editor::set_byte(&mut self.file, name, b).map_err(|e| e.message)?;
        if n == 0 {
            self.undo.pop();
            self.log.push(format!("set: no bool/byte field {name}"));
            return Ok(());
        }
        self.script.push(format!("set={name}|{value}"));
        self.log.push(format!("set {name} = {value}: {n} field(s)"));
        Ok(())
    }

    /// The partition's own guid.
    #[must_use]
    pub fn partition_guid_now(&self) -> String {
        self.file
            .top_level
            .first()
            .and_then(|p| self.get(p, "guid"))
            .unwrap_or_default()
    }

    /// Edit > Partition GUID: the file's own guid (a copy under a new name needs a new one).
    ///
    /// # Errors
    /// A damaged file.
    pub fn partition_guid(&mut self, guid: &str) -> Result<(), String> {
        if guid.is_empty() || guid == self.partition_guid_now() {
            return Ok(());
        }
        self.snapshot()?;
        editor::set_partition_guid(&mut self.file, guid).map_err(|e| e.message)?;
        self.script.push(format!("partguid={guid}"));
        self.log.push(format!("partguid {guid}"));
        Ok(())
    }

    /// Level > Check level (F5), or against the level whose cull trees this one uses (`slots`: its path and file):
    /// the findings in the output, errors first.
    ///
    /// # Errors
    /// A damaged file.
    pub fn check_level(&mut self, slots: Option<(&str, &EditFile)>) -> Result<(), String> {
        let results = dx::level_check(&self.file, slots.map(|s| s.1), &[]).map_err(|e| e.message)?;
        self.log.push(String::new());
        self.log.push(format!(
            "=== check {}{}",
            self.name(),
            slots.map_or_else(String::new, |(p, _)| format!(" (cull slots against {})", file_name(p)))
        ));
        let errors = results.iter().filter(|x| x.error).count();
        for x in results
            .iter()
            .filter(|x| x.error)
            .chain(results.iter().filter(|x| !x.error))
        {
            self.log.push(x.to_string());
        }
        self.log.push(format!(
            "{errors} error(s), {} warning(s){}",
            results.len() - errors,
            if results.is_empty() {
                ""
            } else {
                " - double-click a line to go to its instance"
            }
        ));
        Ok(())
    }

    /// Level > Fix cull slots: each object takes the slot of the same object in the level `from` (at `from_path`)
    /// whose cull trees this file uses.
    ///
    /// # Errors
    /// A damaged file.
    pub fn fix_slots(&mut self, from_path: &str, from: &EditFile) -> Result<(), String> {
        self.snapshot()?;
        let r = dx::fix_slots(&mut self.file, from).map_err(|e| e.message)?;
        if r.changed == 0 {
            self.undo.pop();
        } else {
            self.script
                .push(format!("# fixslots --from {from_path} (DbxEdit fixslots)"));
        }
        self.log.push(format!(
            "fixslots: {} slot(s) changed; by guid {}, by blueprint and position {}, nearest within 50 m {}, no \
             counterpart (slot 0) {}",
            r.changed, r.by_guid, r.by_position, r.by_nearest, r.no_counterpart
        ));
        for u in r.unmatched.iter().take(30) {
            self.log.push(format!("  no counterpart: {u}"));
        }
        Ok(())
    }

    /// Find: every element whose tag, an attribute's value or its value holds `text` (any case), in tree order;
    /// the output says how many.
    pub fn find(&mut self, text: &str) -> Vec<NodePath> {
        let mut found = Vec::new();
        if text.is_empty() {
            return found;
        }
        let mut path = Vec::new();
        for (i, e) in self.file.top_level.iter().enumerate() {
            path.push(i);
            self.search(e, text, &mut path, &mut found);
            path.pop();
        }
        self.log.push(format!(
            "find '{text}': {} match(es){}",
            found.len(),
            if found.is_empty() { "" } else { " - F3 for the next" }
        ));
        found
    }

    fn search(&self, e: &Element, s: &str, path: &mut Vec<usize>, into: &mut Vec<NodePath>) {
        let t = self.t();
        let hit = contains_ignore_case(t.tag(e).unwrap_or(""), s)
            || e.attributes
                .iter()
                .any(|&(_, v)| contains_ignore_case(t.s(v).unwrap_or(""), s))
            || (!e.is_container() && contains_ignore_case(&self.value(e), s));
        if hit {
            into.push(path.clone());
        }
        for (i, c) in e.kids().iter().enumerate() {
            path.push(i);
            self.search(c, s, path, into);
            path.pop();
        }
    }

    /// File > Export XML: DbxEdit's XML of the file as it is now, to `target`.
    ///
    /// # Errors
    /// It cannot be written.
    pub fn export_xml(&mut self, target: &Path) -> Result<(), String> {
        let xml = dx::convert(&self.bytes()?).map_err(|e| e.message)?;
        std::fs::write(target, xml.as_bytes()).map_err(|e| format!("{}: {e}", target.display()))?;
        self.log.push(format!("exported {}", target.display()));
        Ok(())
    }

    /// File > Import XML: the XML at `xml` read back, typed from the open file (as `DbxEdit import --like`).
    ///
    /// # Errors
    /// The XML cannot be read or a value does not fit its type (the file stays as it was).
    pub fn import_xml(&mut self, xml: &Path) -> Result<(), String> {
        let xml_path = xml.display().to_string();
        let reference = EditFile::parse(self.bytes()?).map_err(|e| e.message)?;
        let (result, report) = dx::import(
            || Ok(dx::net::decode_text(&dx::net::read_bytes(&xml_path)?)),
            &xml_path,
            Some(&reference),
            &[],
        )
        .map_err(|e| e.message)?;
        let bytes = result.serialize().map_err(|e| e.message)?;
        self.snapshot()?;
        self.restore(bytes);
        self.script.push(format!("# imported {xml_path}"));
        self.log.push(format!(
            "import {xml_path}: {} unchanged, {} changed, {} new, {} removed{}",
            report.kept,
            report.changed,
            report.added,
            report.removed,
            if report.rounded > 0 {
                format!("; {} decimal(s) rounded", report.rounded)
            } else {
                String::new()
            }
        ));
        for g in report.guesses.iter().take(50) {
            self.log.push(format!("  guess: {g}"));
        }
        if !report.guesses.is_empty() {
            self.log
                .push(format!("{} value(s) typed by guess - check them", report.guesses.len()));
        }
        Ok(())
    }

    /// File > Save (`target` this file) or Save as: the first save over the file it was opened from keeps the
    /// original as `<name>.bak`. A bundle's entry or the project's partition saved this way becomes a loose file.
    ///
    /// # Errors
    /// It cannot be written.
    pub fn save(&mut self, target: &Path) -> Result<(), String> {
        let t = target.display().to_string();
        let same = matches!(&self.source, Source::File(_)) && t == self.path;
        if same && !self.backed_up && target.is_file() {
            let bak = PathBuf::from(format!("{t}.bak"));
            if !bak.exists() {
                std::fs::copy(target, &bak).map_err(|e| format!("{}: {e}", bak.display()))?;
                self.log.push(format!("original kept as {}.bak", file_name(&t)));
            }
        }
        self.backed_up = true;
        dx::write_atomic(&t, &self.bytes()?).map_err(|e| e.message)?;
        self.log.push(format!("saved {t}"));
        self.path = t;
        self.source = Source::File(target.to_path_buf());
        self.modified = false;
        Ok(())
    }

    /// Save into bundle: a copy of the bundle the entry came from, written to `out`, with this entry's bytes
    /// (FbrbTool's unpack, then pack: every other entry, the layout and the manifest's sizes as they were).
    /// `scratch` is an empty folder to unpack into (removed afterwards).
    ///
    /// # Errors
    /// Not a bundle's entry, `out` is the bundle itself, or the bundle cannot be unpacked, packed or written.
    pub fn save_into_bundle(&mut self, out: &Path, scratch: &Path) -> Result<(), String> {
        let Source::Bundle { bundle, entry } = self.source.clone() else {
            return Err("not an entry of a bundle".into());
        };
        if crate::app::same_file(&bundle, out) {
            return Err(format!(
                "{} is the bundle it was read from: save into a copy (another file name)",
                out.display()
            ));
        }
        let bytes = self.bytes()?;
        let _ = std::fs::remove_dir_all(scratch);
        let r = (|| {
            bc2_formats::fbrb::unpack_path(&bundle, scratch).map_err(|e| e.to_string())?;
            let file = bc2_formats::fbrb::entry_path(scratch, &entry).map_err(|e| e.to_string())?;
            std::fs::write(&file, &bytes).map_err(|e| format!("{}: {e}", file.display()))?;
            bc2_formats::fbrb::pack_path(scratch, out).map_err(|e| e.to_string())
        })();
        let _ = std::fs::remove_dir_all(scratch);
        let stats = r?;
        self.modified = false;
        self.log.push(format!(
            "saved into {} ({entry}; {} entries)",
            out.display(),
            stats.entries
        ));
        Ok(())
    }

    /// File > Save edit script: the edits as DbxEdit commands, to replay on another file with
    /// `bfbc2.exe dbxedit edit <in.dbx> <out.dbx> @<script>`.
    ///
    /// # Errors
    /// It cannot be written.
    pub fn save_script(&mut self, target: &Path) -> Result<(), String> {
        if self.script.is_empty() {
            self.log.push("No edits to save as a script yet.".into());
            return Ok(());
        }
        let t = target.display().to_string();
        let mut text = format!(
            "# DbxEdit commands for {} - replay with: bfbc2.exe dbxedit edit <in.dbx> <out.dbx> @{}\r\n",
            self.name(),
            file_name(&t)
        );
        for l in &self.script {
            text.push_str(l);
            text.push_str("\r\n");
        }
        std::fs::write(target, text.as_bytes()).map_err(|e| format!("{t}: {e}"))?;
        self.log.push(format!("saved {} edit(s) to {t}", self.script.len()));
        Ok(())
    }

    /// The guid named on an output line (double-click: go to its instance).
    #[must_use]
    pub fn guid_on_line(&self, line: &str) -> Option<NodePath> {
        let b = line.as_bytes();
        let shape = |s: &[u8]| {
            s.len() == 36
                && s.iter().enumerate().all(|(i, c)| match i {
                    8 | 13 | 18 | 23 => *c == b'-',
                    _ => c.is_ascii_hexdigit(),
                })
        };
        (0..b.len().saturating_sub(35))
            .filter_map(|i| b.get(i..i + 36))
            .find(|s| shape(s))
            .and_then(|s| std::str::from_utf8(s).ok())
            .and_then(|g| self.instance_path(g))
    }

    /// Every element's path, in tree order.
    #[must_use]
    pub fn all_paths(&self) -> Vec<NodePath> {
        fn walk(l: &[Element], prefix: &mut Vec<usize>, out: &mut Vec<NodePath>) {
            for (i, e) in l.iter().enumerate() {
                prefix.push(i);
                out.push(prefix.clone());
                walk(e.kids(), prefix, out);
                prefix.pop();
            }
        }
        let mut out = Vec::new();
        walk(&self.file.top_level, &mut Vec::new(), &mut out);
        out
    }
}

/// A field edit of the project's partition as the project's operation takes it: the element at `path` of `doc`
/// (the project's partition) with its attributes and value as typed, made into the instance's guid, the field's
/// path (`Name`, or `Struct.Field`) and a [`FieldValue`] of the field's schema type `ty` that writes exactly the
/// bits DbxEdit's typing gives (a decimal keeps its bits, a denormal included; a value the schema's type cannot
/// carry exactly is refused).
///
/// # Errors
/// Not a field of an instance, a value not of the field's type, or one the project's operation cannot carry.
pub fn project_field(
    doc: &DbxDoc,
    path: &[usize],
    attrs: &[String],
    value: &str,
    ty_of: impl Fn(&str, &str) -> Option<FieldType>,
) -> Result<Option<(String, String, FieldValue)>, String> {
    let inst = doc
        .instance_of(path)
        .filter(|i| i.as_slice() != path)
        .ok_or("Only an instance's fields go into the project (its objects are edited in the Objects workspace)")?;
    let guid = doc
        .element(&inst)
        .and_then(|e| doc.get(e, "guid"))
        .ok_or("the instance has no guid")?;
    let field = match path.len() - inst.len() {
        1 => doc.element(path).and_then(|e| doc.get(e, "name")),
        2 => {
            let parent = doc.element(path.get(..path.len() - 1).unwrap_or_default());
            let p = parent
                .filter(|p| doc.tag(p) != "array")
                .and_then(|p| doc.get(p, "name"));
            let n = doc.element(path).and_then(|e| doc.get(e, "name"));
            p.zip(n).map(|(p, n)| format!("{p}.{n}"))
        }
        _ => None,
    }
    .ok_or("Only an instance's fields (and its structures' fields) go into the project")?;
    let ty = ty_of(&guid, &field).ok_or_else(|| format!("{field} is not a field the project edits"))?;
    // the edit made on a copy, as the file would take it
    let mut scratch = doc.clone();
    scratch.source = Source::File(PathBuf::new());
    if scratch.apply_edit(path, attrs, value)? == Applied::Unchanged {
        return Ok(None);
    }
    let e = scratch.element(path).ok_or("no such element")?;
    let old = doc.element(path).ok_or("no such element")?;
    if e.attributes.len() != old.attributes.len()
        || e.attributes
            .iter()
            .zip(&old.attributes)
            .any(|(a, b)| scratch.t().s(a.0).ok() != doc.t().s(b.0).ok())
    {
        return Err("attributes cannot change through the project".into());
    }
    let changed_attr: Vec<String> = e
        .attributes
        .iter()
        .zip(&old.attributes)
        .filter(|(a, b)| scratch.t().s(a.1).ok() != doc.t().s(b.1).ok())
        .map(|(a, _)| scratch.t().s(a.0).unwrap_or("").to_owned())
        .collect();
    let v = field_value(&ty, &scratch.file.table, e, &changed_attr)?;
    Ok(Some((guid, field, v)))
}

/// The bits of a 4-byte number, exactly, as a value the project's operation writes back unchanged.
fn exact_single(bits: u32) -> Result<f64, String> {
    let f = f64::from(f32::from_bits(bits));
    if (f as f32).to_bits() == bits {
        Ok(f)
    } else {
        Err(format!(
            "0x{bits:08X} is not a decimal the project can store exactly (NaN)"
        ))
    }
}

/// The 4-byte numbers of an element (the count, then each one's bits).
fn fours(e: &Element) -> Option<Vec<u32>> {
    let mut p = 0;
    let n = dx::read_varint(&e.payload, &mut p).ok()?;
    let size = dx::read_varint(&e.payload, &mut p).ok()?;
    if size != 4 && n != 0 {
        return None;
    }
    (0..n.max(0) as usize)
        .map(|i| {
            e.payload
                .get(p + 4 * i..p + 4 * i + 4)
                .and_then(|b| <[u8; 4]>::try_from(b).ok())
                .map(u32::from_be_bytes)
        })
        .collect()
}

/// An edited element as a [`FieldValue`] of type `ty` that sets exactly its bits; `attrs` the attributes the edit
/// changed (only a reference's `ref` can change).
fn field_value(ty: &FieldType, t: &Table, e: &Element, attrs: &[String]) -> Result<FieldValue, String> {
    let refused = || format!("a {} field does not take this value exactly", ty.name());
    if let FieldType::Ref = ty {
        return match attrs {
            [a] if a == "ref" => Ok(FieldValue::Ref(t.get(e, "ref").ok().flatten().unwrap_or("").to_owned())),
            _ => Err("a reference changes by its ref attribute".into()),
        };
    }
    if !attrs.is_empty() {
        return Err("attributes cannot change through the project".into());
    }
    match ty {
        FieldType::Text if e.kind == TEXT => Ok(FieldValue::Text(dx::value_text(t, e).map_err(|f| f.message)?)),
        FieldType::Bool | FieldType::Byte if e.kind != TEXT && e.kind != NUMBERS && e.kind != CONTAINER => {
            let v = *e.payload.get(1).ok_or_else(refused)?;
            match ty {
                FieldType::Bool if v <= 1 => Ok(FieldValue::Bool(v == 1)),
                FieldType::Byte => Ok(FieldValue::Byte(v)),
                _ => Err(format!("{v} is not true or false")),
            }
        }
        FieldType::Int if e.kind == NUMBERS => match fours(e).as_deref() {
            Some([b]) => Ok(FieldValue::Int(i64::from(*b as i32))),
            _ => Err(refused()),
        },
        FieldType::Float if e.kind == NUMBERS => match fours(e).as_deref() {
            Some([b]) => Ok(FieldValue::Float(exact_single(*b)?)),
            _ => Err(refused()),
        },
        FieldType::Vector { count, whole } if e.kind == NUMBERS => {
            let v = fours(e).ok_or_else(refused)?;
            if v.len() != *count {
                return Err(format!("{} values where the field has {count}", v.len()));
            }
            Ok(FieldValue::Vector(
                v.iter()
                    .map(|&b| {
                        if *whole {
                            Ok(f64::from(b as i32))
                        } else {
                            exact_single(b)
                        }
                    })
                    .collect::<Result<_, _>>()?,
            ))
        }
        FieldType::UnknownNumber { count } if e.kind == NUMBERS => {
            let v = fours(e).ok_or_else(refused)?;
            if v.len() == *count && v.iter().all(|&b| b == 0) {
                Ok(if *count == 1 {
                    FieldValue::Int(0)
                } else {
                    FieldValue::Vector(vec![0.0; *count])
                })
            } else {
                Err(
                    "whether it holds whole numbers or decimals is not known from the level's data, so only 0 can \
                     be written"
                        .into(),
                )
            }
        }
        _ => Err(refused()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::indexing_slicing, clippy::panic)]
mod tests {
    use super::*;

    /// A small file: a partition with two instances, a text, numbers (a decimal, a denormal), a bool, a reference,
    /// an array of references.
    pub(crate) fn sample() -> Vec<u8> {
        let mut f = EditFile::empty();
        let t = &mut f.table;
        let (partition, instance, field, array, item, complex) = (
            t.intern("partition"),
            t.intern("instance"),
            t.intern("field"),
            t.intern("array"),
            t.intern("item"),
            t.intern("complex"),
        );
        let (guid, ty, id, name, r) = (
            t.intern("guid"),
            t.intern("type"),
            t.intern("id"),
            t.intern("name"),
            t.intern("ref"),
        );
        let g1 = t.intern("11111111-1111-1111-1111-111111111111");
        let g2 = t.intern("22222222-2222-2222-2222-222222222222");
        let gp = t.intern("AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA");
        let (t1, t2) = (t.intern("Entity.Thing"), t.intern("Entity.Other"));
        let (n_name, n_scale, n_tiny, n_on, n_link, n_list, n_pos, n_x) = (
            t.intern("Name"),
            t.intern("Scale"),
            t.intern("Tiny"),
            t.intern("Enabled"),
            t.intern("Link"),
            t.intern("Objects"),
            t.intern("Pos"),
            t.intern("X"),
        );
        let text = t.intern("crate");
        let numbers = |vals: &[u32]| {
            let mut p = dx::varint_bytes(vals.len() as i32);
            p.extend(dx::varint_bytes(4));
            for v in vals {
                p.extend(v.to_be_bytes());
            }
            p
        };
        let value = |tag, attrs: Vec<(i32, i32)>, kind, payload| Element {
            tag,
            kind,
            attributes: attrs,
            payload,
            ..Element::default()
        };
        let cont = |tag, attrs, children| Element {
            tag,
            kind: CONTAINER,
            attributes: attrs,
            children: Some(children),
            ..Element::default()
        };
        let i1 = cont(
            instance,
            vec![(guid, g1), (ty, t1), (id, text)],
            vec![
                value(field, vec![(name, n_name)], TEXT, dx::varint_bytes(text)),
                value(
                    field,
                    vec![(name, n_scale)],
                    NUMBERS,
                    numbers(&[1.5f32.to_bits(), 2.0f32.to_bits()]),
                ),
                value(field, vec![(name, n_tiny)], NUMBERS, numbers(&[0x0008_0000])),
                value(field, vec![(name, n_on)], 6, vec![1, 0]),
                value(field, vec![(name, n_link), (r, g2)], TEXT, dx::varint_bytes(0)),
                cont(
                    array,
                    vec![(name, n_list)],
                    vec![value(item, vec![(r, g2)], TEXT, dx::varint_bytes(0))],
                ),
                cont(
                    complex,
                    vec![(name, n_pos)],
                    vec![value(field, vec![(name, n_x)], NUMBERS, numbers(&[3.0f32.to_bits()]))],
                ),
            ],
        );
        let i2 = cont(instance, vec![(guid, g2), (ty, t2)], vec![]);
        f.top_level.push(cont(partition, vec![(guid, gp)], vec![i1, i2]));
        f.serialize().unwrap()
    }

    fn doc() -> DbxDoc {
        DbxDoc::from_bytes(sample(), Source::File(PathBuf::from("x.dbx")), "x.dbx").unwrap()
    }

    #[test]
    fn labels_and_headings_as_dbxeditgui() {
        let d = doc();
        let l = |p: &[usize]| d.label(d.element(p).unwrap());
        assert_eq!(l(&[0]), "partition AAAAAAAA-AAAA-AAAA-AAAA-AAAAAAAAAAAA");
        assert_eq!(l(&[0, 0]), "Thing   crate");
        assert_eq!(l(&[0, 1]), "Other   22222222-2222-2222-2222-222222222222");
        assert_eq!(l(&[0, 0, 1]), "Scale = 1.5/2");
        assert_eq!(l(&[0, 0, 2]), "Tiny = 524288");
        assert_eq!(l(&[0, 0, 3]), "Enabled = false");
        assert_eq!(l(&[0, 0, 4]), "Link -> 22222222-2222-2222-2222-222222222222");
        assert_eq!(l(&[0, 0, 5]), "Objects  [1]");
        assert_eq!(
            d.heading(d.element(&[0, 0, 1]).unwrap()),
            "field  (2 numbers of 4 bytes)"
        );
        assert_eq!(
            d.heading(d.element(&[0, 0, 2]).unwrap()),
            "field  (1 number of 4 bytes)"
        );
        assert_eq!(d.heading(d.element(&[0, 0, 3]).unwrap()), "field  (bool / byte)");
        let hint = denormal_hint(d.t(), d.element(&[0, 0, 2]).unwrap());
        assert_eq!(hint.len(), 1);
        assert_eq!(hint[0].1, 0x0008_0000);
        assert_eq!(denormal_hint(d.t(), d.element(&[0, 0, 1]).unwrap()), vec![]);
    }

    #[test]
    fn edits_round_trip_and_undo() {
        let mut d = doc();
        let original = d.bytes().unwrap();
        let attrs = |d: &DbxDoc, p: &[usize]| -> Vec<String> {
            d.attributes(d.element(p).unwrap()).into_iter().map(|a| a.1).collect()
        };
        // the same text again: nothing changes, and the denormal keeps its bits
        let a = attrs(&d, &[0, 0, 2]);
        assert_eq!(d.apply_edit(&[0, 0, 2], &a, "524288").unwrap(), Applied::Unchanged);
        assert_eq!(d.bytes().unwrap(), original);
        // a decimal rounded, said in the output
        let a = attrs(&d, &[0, 0, 1]);
        assert_eq!(
            d.apply_edit(&[0, 0, 1], &a, "3.14159265358979/2").unwrap(),
            Applied::Changed
        );
        assert!(
            d.log.last().unwrap().starts_with("Rounded to the nearest 4-byte value"),
            "{:?}",
            d.log
        );
        assert_eq!(
            d.script.last().unwrap(),
            "setfield=11111111-1111-1111-1111-111111111111|Scale|3.14159274/2"
        );
        // a value not of the field's type: refused, the file as it was
        let before = d.bytes().unwrap();
        assert!(d.apply_edit(&[0, 0, 1], &a, "abc").is_err());
        assert_eq!(d.bytes().unwrap(), before);
        // the same edit through the command line's setfield gives the same bytes
        let mut cli = EditFile::parse(original.clone()).unwrap();
        editor::set_field(
            &mut cli,
            "11111111-1111-1111-1111-111111111111",
            "Scale",
            "3.14159265358979/2",
        )
        .unwrap();
        assert_eq!(cli.serialize().unwrap(), d.bytes().unwrap());
        // a field of a structure: no command form
        let a = attrs(&d, &[0, 0, 6, 0]);
        d.apply_edit(&[0, 0, 6, 0], &a, "4").unwrap();
        assert_eq!(
            d.script.last().unwrap(),
            "# edited X = 4 in 11111111-1111-1111-1111-111111111111 (no command form)"
        );
        d.undo();
        d.undo();
        assert_eq!(d.bytes().unwrap(), original);
        assert!(!d.modified);
        d.undo();
        assert_eq!(d.log.last().unwrap(), "Nothing to undo.");
    }

    #[test]
    fn menus_and_find() {
        let mut d = doc();
        assert_eq!(d.find("crate").len(), 2);
        assert_eq!(d.log.last().unwrap(), "find 'crate': 2 match(es) - F3 for the next");
        assert_eq!(d.go_to_reference(&[0, 0, 4]), Some(vec![0, 1]));
        d.replace("nothing", "x").unwrap();
        assert_eq!(d.log.last().unwrap(), "replace: not found: nothing");
        assert_eq!(d.undo_steps(), 0);
        d.set_byte("Enabled", "true").unwrap();
        assert_eq!(d.log.last().unwrap(), "set Enabled = true: 1 field(s)");
        d.add_item(&[0, 0, 5], "11111111-1111-1111-1111-111111111111").unwrap();
        assert_eq!(d.element(&[0, 0, 5]).unwrap().kids().len(), 2);
        d.partition_guid("BBBBBBBB-BBBB-BBBB-BBBB-BBBBBBBBBBBB").unwrap();
        d.drop_instance(&[0, 1]).unwrap();
        assert_eq!(
            d.log.last().unwrap(),
            "drop 22222222-2222-2222-2222-222222222222: 1 instance(s), 2 reference(s)"
        );
        assert_eq!(d.script.len(), 4);
        assert_eq!(
            d.guid_on_line("x 11111111-1111-1111-1111-111111111111 y"),
            Some(vec![0, 0])
        );
    }

    #[test]
    fn project_fields_keep_their_bits() {
        let d = doc();
        let attrs =
            |p: &[usize]| -> Vec<String> { d.attributes(d.element(p).unwrap()).into_iter().map(|a| a.1).collect() };
        let ty = |_: &str, f: &str| match f {
            "Tiny" | "Pos.X" => Some(FieldType::Float),
            "Scale" => Some(FieldType::Vector { count: 2, whole: false }),
            "Link" => Some(FieldType::Ref),
            _ => None,
        };
        // a denormal edited: its bits, not 524289.0
        let (g, f, v) = project_field(&d, &[0, 0, 2], &attrs(&[0, 0, 2]), "524289", ty)
            .unwrap()
            .unwrap();
        assert_eq!(
            (g.as_str(), f.as_str()),
            ("11111111-1111-1111-1111-111111111111", "Tiny")
        );
        let FieldValue::Float(x) = v else { panic!() };
        assert_eq!((x as f32).to_bits(), 0x0008_0001);
        assert!(
            project_field(&d, &[0, 0, 2], &attrs(&[0, 0, 2]), "524288", ty)
                .unwrap()
                .is_none()
        );
        assert!(project_field(&d, &[0, 0, 1], &attrs(&[0, 0, 1]), "1", ty).is_err());
        let (_, f, _) = project_field(&d, &[0, 0, 6, 0], &attrs(&[0, 0, 6, 0]), "5", ty)
            .unwrap()
            .unwrap();
        assert_eq!(f, "Pos.X");
        let mut a = attrs(&[0, 0, 4]);
        a[1] = "null".into();
        let (_, _, v) = project_field(&d, &[0, 0, 4], &a, "", ty).unwrap().unwrap();
        assert_eq!(v, FieldValue::Ref("null".into()));
        assert!(project_field(&d, &[0, 0, 3], &attrs(&[0, 0, 3]), "true", ty).is_err());
    }
}
