//! `Editor` of `DbxEdit.cs` (FORMAT.md section 5): the edit commands. Each returns a count or whether it found what
//! it changes; the command line prints. Strings are interned in DbxEdit's order, so the string table (and so the
//! bytes) come out as DbxEdit's.

use std::collections::{HashMap, HashSet};

use super::fault::{Fault, Result};
use super::file::{EditFile, Element, NUMBERS, TEXT, Table, read_varint, varint_bytes};
use super::import::encode_value;
use super::net;

/// `replace=<old>|<new>`: every string exactly `from` becomes `to`. Returns the strings changed.
pub fn replace(file: &mut EditFile, from: &str, to: &str) -> usize {
    let hits: Vec<usize> = (0..file.table.strings.len())
        .filter(|&i| file.table.strings.get(i).is_some_and(|s| s == from))
        .collect();
    for &i in &hits {
        file.table.set(i, to.to_owned());
    }
    hits.len()
}

/// `replaceprefix=<old>|<new>[|<except>...]`: strings starting with `from` (any case), but none of `except`, get
/// `to` as their start (lower-cased when the start they had was lower case).
pub fn replace_prefix(file: &mut EditFile, from: &str, to: &str, except: &[String]) -> usize {
    let mut hits = 0;
    for i in 0..file.table.strings.len() {
        let Some(s) = file.table.strings.get(i) else { continue };
        let Some(head_len) = net::starts_with_ignore_case(s, from) else {
            continue;
        };
        if except
            .iter()
            .any(|x| !x.is_empty() && net::starts_with_ignore_case(s, x).is_some())
        {
            continue;
        }
        let head = s.get(..head_len).unwrap_or("");
        let start = if head == net::to_lower(head) {
            net::to_lower(to)
        } else {
            to.to_owned()
        };
        let new = format!("{start}{}", s.get(head_len..).unwrap_or(""));
        file.table.set(i, new);
        hits += 1;
    }
    hits
}

fn refers_to(target: &str, guid: &str) -> bool {
    net::same_ignore_case(target, guid) || net::ends_with_ignore_case(target, &format!("/{guid}"))
}

fn drop_in(t: &mut Table, list: &mut Vec<Element>, guid: &str, references: &mut usize) -> Result<usize> {
    let mut removed = 0;
    let mut i = list.len();
    while i > 0 {
        i -= 1;
        let Some(e) = list.get_mut(i) else { continue };
        let tag = t.tag(e)?.to_owned();
        if tag == "instance" && t.get(e, "guid")?.is_some_and(|g| net::same_ignore_case(g, guid)) {
            list.remove(i);
            removed += 1;
            continue;
        }
        if t.get(e, "ref")?.is_some_and(|r| refers_to(r, guid)) {
            *references += 1;
            if tag == "item" {
                list.remove(i);
                continue;
            }
            for a in &mut e.attributes {
                if t.s(a.0)? == "ref" {
                    a.1 = t.intern("null");
                }
            }
        }
        if let Some(children) = &mut e.children {
            removed += drop_in(t, children, guid, references)?;
        }
    }
    Ok(removed)
}

/// `drop=<guid>`: removes the instance with that guid; array items referring to it are removed, single references
/// to it become `null`. Returns (instances removed, references met).
///
/// # Errors
/// A damaged string index.
pub fn drop(file: &mut EditFile, guid: &str) -> Result<(usize, usize)> {
    let mut references = 0;
    let removed = drop_in(&mut file.table, &mut file.top_level, guid, &mut references)?;
    Ok((removed, references))
}

fn drop_field_in(t: &Table, list: &mut Vec<Element>, name: &str) -> Result<usize> {
    let mut removed = 0;
    let mut i = list.len();
    while i > 0 {
        i -= 1;
        let Some(e) = list.get_mut(i) else { continue };
        if t.tag(e)? != "instance" && t.get(e, "name")? == Some(name) {
            list.remove(i);
            removed += 1;
        } else if let Some(children) = &mut e.children {
            removed += drop_field_in(t, children, name)?;
        }
    }
    Ok(removed)
}

/// `dropfield=<name>`: removes every element (but instances) named `name`. Returns the elements removed.
///
/// # Errors
/// A damaged string index.
pub fn drop_field(file: &mut EditFile, name: &str) -> Result<usize> {
    drop_field_in(&file.table, &mut file.top_level, name)
}

fn set_byte_in(t: &Table, list: &mut [Element], name: &str, value: u8) -> Result<usize> {
    let mut set = 0;
    for e in list {
        if let Some(children) = &mut e.children {
            set += set_byte_in(t, children, name, value)?;
        } else if e.kind != TEXT && e.kind != NUMBERS && e.payload.len() == 2 && t.get(e, "name")? == Some(name) {
            let first = e.payload.first().copied().unwrap_or(0);
            e.payload = vec![first, value];
            set += 1;
        }
    }
    Ok(set)
}

/// `set=<field>|<value>`: every bool / byte field named `name` set to `value`. Returns the fields set.
///
/// # Errors
/// A damaged string index.
pub fn set_byte(file: &mut EditFile, name: &str, value: u8) -> Result<usize> {
    set_byte_in(&file.table, &mut file.top_level, name, value)
}

/// The index of the first instance with this guid among the partition's children.
fn instance_index(file: &EditFile, guid: &str) -> Result<Option<usize>> {
    let t = &file.table;
    for (i, e) in file.partition_children()?.iter().enumerate() {
        if t.tag(e)? == "instance" && t.get(e, "guid")?.is_some_and(|g| net::same_ignore_case(g, guid)) {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

fn partition_mut(file: &mut EditFile) -> Result<&mut Vec<Element>> {
    if !file.is_binary {
        return Err(Fault::null());
    }
    let p = file.top_level.first_mut().ok_or_else(Fault::list_index)?;
    p.children.as_mut().ok_or_else(Fault::null)
}

/// `setfield=<guid>|<field>|<value>`: a text or numbers field of that instance set (numbers typed like the field's
/// current value, as the import does). Returns whether the field was there.
///
/// # Errors
/// The value is not a number of the field's type, or a damaged file.
pub fn set_field(file: &mut EditFile, guid: &str, field: &str, value: &str) -> Result<bool> {
    let Some(ii) = instance_index(file, guid)? else {
        return Ok(false);
    };
    let found = {
        let t = &file.table;
        let inst = file.partition_children()?.get(ii).ok_or_else(Fault::null)?;
        let Some(children) = &inst.children else {
            return Ok(false);
        };
        let mut found = None;
        for (ci, c) in children.iter().enumerate() {
            if t.get(c, "name")? == Some(field) && (c.kind == TEXT || c.kind == NUMBERS) {
                found = Some((ci, c.clone()));
                break;
            }
        }
        found
    };
    let Some((ci, e)) = found else { return Ok(false) };
    let payload = if e.kind == TEXT {
        varint_bytes(file.table.intern(value))
    } else {
        encode_value(file, &e, value)?
    };
    if let Some(c) = partition_mut(file)?
        .get_mut(ii)
        .and_then(|i| i.children.as_mut())
        .and_then(|k| k.get_mut(ci))
    {
        c.payload = payload;
    }
    Ok(true)
}

/// `partguid=<guid>`: the partition's own guid (every `guid` attribute of the partition).
///
/// # Errors
/// A file without a partition, or a damaged string index.
pub fn set_partition_guid(file: &mut EditFile, guid: &str) -> Result<()> {
    let t = &mut file.table;
    let p = file.top_level.first_mut().ok_or_else(Fault::list_index)?;
    for a in &mut p.attributes {
        if t.s(a.0)? == "guid" {
            a.1 = t.intern(guid);
        }
    }
    Ok(())
}

/// `additem=<guid>|<array>|<ref>[|<before ref>]`: an item referring to `reference` added to array `array` of the
/// instance (at the end, or before the item referring to `before`). Returns whether the array was there.
///
/// # Errors
/// A damaged file (an instance that is not a container).
pub fn add_item(file: &mut EditFile, guid: &str, array: &str, reference: &str, before: Option<&str>) -> Result<bool> {
    let Some(ii) = instance_index(file, guid)? else {
        return Ok(false);
    };
    let (ai, shape, at) = {
        let t = &file.table;
        let inst = file.partition_children()?.get(ii).ok_or_else(Fault::null)?;
        let children = inst.children.as_ref().ok_or_else(Fault::null)?;
        let mut arr = None;
        for (ai, c) in children.iter().enumerate() {
            if t.tag(c)? == "array" && t.get(c, "name")? == Some(array) {
                arr = Some((ai, c));
                break;
            }
        }
        let Some((ai, a)) = arr else { return Ok(false) };
        let Some(items) = &a.children else { return Ok(false) };
        let mut shape = None;
        for c in items {
            if t.tag(c)? == "item" {
                shape = Some(c.clone());
                break;
            }
        }
        let Some(shape) = shape else { return Ok(false) };
        let mut at = items.len();
        if let Some(b) = before {
            for (i, c) in items.iter().enumerate() {
                if t.get(c, "ref")?.is_some_and(|r| net::same_ignore_case(r, b)) {
                    at = i;
                    break;
                }
            }
        }
        (ai, shape, at)
    };
    let name = file.table.intern("ref");
    let value = file.table.intern(reference);
    let item = Element {
        tag: shape.tag,
        kind: shape.kind,
        attributes: vec![(name, value)],
        container_byte: shape.container_byte,
        children: shape.is_container().then(Vec::new),
        payload: shape.payload,
    };
    if let Some(items) = partition_mut(file)?
        .get_mut(ii)
        .and_then(|i| i.children.as_mut())
        .and_then(|k| k.get_mut(ai))
        .and_then(|a| a.children.as_mut())
    {
        items.insert(at.min(items.len()), item);
    }
    Ok(true)
}

/// Copies an element of another file into this one: strings interned in DbxEdit's order (tag, each attribute's name
/// then value, a text value, then the children).
fn import_element(t: &mut Table, source: &Table, e: &Element) -> Result<Element> {
    let tag = t.intern(source.s(e.tag)?);
    let mut attributes = Vec::with_capacity(e.attributes.len());
    for &(k, v) in &e.attributes {
        let name = t.intern(source.s(k)?);
        attributes.push((name, t.intern(source.s(v)?)));
    }
    let payload = if e.kind == TEXT {
        let mut pos = 0;
        varint_bytes(t.intern(source.s(read_varint(&e.payload, &mut pos)?)?))
    } else {
        e.payload.clone()
    };
    let children = match &e.children {
        Some(kids) => {
            let mut out = Vec::with_capacity(kids.len());
            for c in kids {
                out.push(import_element(t, source, c)?);
            }
            Some(out)
        }
        None => None,
    };
    Ok(Element {
        tag,
        kind: e.kind,
        attributes,
        container_byte: e.container_byte,
        children,
        payload,
    })
}

/// The first element named `field` below `e` that is not an instance (each element checked before its children).
fn find_field<'e>(t: &Table, e: &'e Element, field: &str) -> Result<Option<&'e Element>> {
    let Some(kids) = &e.children else { return Ok(None) };
    for c in kids {
        if t.tag(c)? != "instance" && t.get(c, "name")? == Some(field) {
            return Ok(Some(c));
        }
        if let Some(d) = find_field(t, c, field)? {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

fn replace_field(t: &Table, e: &mut Element, field: &str, replacement: &mut Option<Element>) -> Result<bool> {
    let Some(kids) = &mut e.children else { return Ok(false) };
    for c in kids.iter_mut() {
        if t.get(c, "name")? == Some(field) {
            if let Some(r) = replacement.take() {
                *c = r;
            }
            return Ok(true);
        }
        if replace_field(t, c, field, replacement)? {
            return Ok(true);
        }
    }
    Ok(false)
}

/// `copyfield=<src.dbx>|<src guid>|<field>|<guid>`: copies a field (with arrays) from another file's instance into
/// instance `guid`, replacing the field of that name or added at its end. Returns whether both were found.
///
/// # Errors
/// A damaged file.
pub fn copy_field(file: &mut EditFile, source: &EditFile, source_guid: &str, field: &str, guid: &str) -> Result<bool> {
    let Some(from) = source.instance(source_guid)? else {
        return Ok(false);
    };
    let Some(ti) = instance_index(file, guid)? else {
        return Ok(false);
    };
    let Some(original) = find_field(&source.table, from, field)? else {
        return Ok(false);
    };
    let copy = import_element(&mut file.table, &source.table, original)?;
    let t = &file.table;
    let p = file.top_level.first_mut().ok_or_else(Fault::list_index)?;
    let to = p
        .children
        .as_mut()
        .and_then(|k| k.get_mut(ti))
        .ok_or_else(Fault::null)?;
    let mut slot = Some(copy);
    if !replace_field(t, to, field, &mut slot)? {
        let kids = to.children.as_mut().ok_or_else(Fault::null)?;
        if let Some(c) = slot {
            kids.push(c);
        }
    }
    Ok(true)
}

/// Local references (36 characters, no `/`) below `e`, upper-cased, each once, in document order.
fn local_references(t: &Table, e: &Element, found: &mut Vec<String>, seen: &mut HashSet<String>) -> Result<()> {
    if let Some(r) = t.get(e, "ref")?
        && r.encode_utf16().count() == 36
        && !r.contains('/')
    {
        let up = net::to_upper(r);
        if seen.insert(up.clone()) {
            found.push(up);
        }
    }
    for c in e.kids() {
        local_references(t, c, found, seen)?;
    }
    Ok(())
}

/// `graft=<src.dbx>|<guid>[|<before guid>]`: copies instance `guid` of `source` and, depth first, every instance of
/// `source` it references locally that this file does not have yet; inserted before instance `before`, or at the end
/// of the partition. Returns the instances copied.
///
/// # Errors
/// A damaged file, or an instance without a guid.
pub fn graft(file: &mut EditFile, source: &EditFile, guid: &str, before: Option<&str>) -> Result<usize> {
    let mut at = file.partition_children()?.len();
    if let Some(b) = before
        && let Some(i) = instance_index(file, b)?
    {
        at = i;
    }
    let mut available: HashMap<String, &Element> = HashMap::new();
    for e in source.instances()? {
        let e = e?;
        let g = source.table.get(e, "guid")?.ok_or_else(Fault::null)?;
        available.insert(net::to_upper(g), e);
    }
    let mut present = HashSet::new();
    for e in file.instances()? {
        let e = e?;
        present.insert(net::to_upper(file.table.get(e, "guid")?.ok_or_else(Fault::null)?));
    }
    let mut pending = vec![net::to_upper(guid)];
    let mut copied = 0;
    while let Some(next) = pending.pop() {
        if present.contains(&next) {
            continue;
        }
        let Some(&e) = available.get(&next) else { continue };
        present.insert(next);
        let copy = import_element(&mut file.table, &source.table, e)?;
        partition_mut(file)?.insert(at, copy);
        at += 1;
        copied += 1;
        let mut refs = Vec::new();
        local_references(&source.table, e, &mut refs, &mut HashSet::new())?;
        pending.extend(refs);
    }
    Ok(copied)
}
