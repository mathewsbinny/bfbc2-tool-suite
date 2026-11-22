//! `Editor` of `DbxEdit.cs` (FORMAT.md section 5): the edit commands. Each returns a count or whether it found what
//! it changes; the command line prints. Strings are interned in DbxEdit's order, so the string table (and so the
//! bytes) come out as DbxEdit's.

use std::collections::{HashMap, HashSet};

use super::fault::{Fault, Result};
use super::file::{
    EditDbx, NUMBERS, NodeId, Parts, TEXT, Table, instance, instances, intern, kids, partition_children,
    partition_children_mut, read_varint, varint_bytes,
};
use super::import::encode_value;
use super::net;

/// The text of string `i` (strings read by DbxEdit's reader are text; a string of other bytes is skipped).
fn string_text(file: &EditDbx, i: usize) -> Option<&str> {
    file.strings().get(i).and_then(|b| std::str::from_utf8(b).ok())
}

/// `replace=<old>|<new>`: every string exactly `from` becomes `to`. Returns the strings changed.
pub fn replace(file: &mut EditDbx, from: &str, to: &str) -> usize {
    let hits: Vec<usize> = (0..file.strings().len())
        .filter(|&i| file.strings().get(i).is_some_and(|s| s.as_slice() == from.as_bytes()))
        .collect();
    for &i in &hits {
        file.set_string(i, to.as_bytes().to_vec());
    }
    hits.len()
}

/// `replaceprefix=<old>|<new>[|<except>...]`: strings starting with `from` (any case), but none of `except`, get
/// `to` as their start (lower-cased when the start they had was lower case).
pub fn replace_prefix(file: &mut EditDbx, from: &str, to: &str, except: &[String]) -> usize {
    let mut hits = 0;
    for i in 0..file.strings().len() {
        let Some(s) = string_text(file, i) else { continue };
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
        file.set_string(i, new.into_bytes());
        hits += 1;
    }
    hits
}

fn refers_to(target: &str, guid: &str) -> bool {
    net::same_ignore_case(target, guid) || net::ends_with_ignore_case(target, &format!("/{guid}"))
}

/// The elements of a list: the roots (`None`) or a container's children.
fn list(file: &EditDbx, parent: Option<NodeId>) -> Vec<NodeId> {
    match parent {
        None => file.roots().to_vec(),
        Some(p) => kids(file, p).to_vec(),
    }
}

fn remove_at(file: &mut EditDbx, parent: Option<NodeId>, i: usize) {
    let l = match parent {
        None => Some(file.roots_mut()),
        Some(p) => file.children_mut(p).ok(),
    };
    if let Some(l) = l
        && i < l.len()
    {
        l.remove(i);
    }
}

fn drop_in(file: &mut EditDbx, parent: Option<NodeId>, guid: &str, references: &mut usize) -> Result<usize> {
    let mut removed = 0;
    let items = list(file, parent);
    let mut i = items.len();
    while i > 0 {
        i -= 1;
        let Some(&e) = items.get(i) else { continue };
        let t = Table(file);
        let tag = t.tag(e)?.to_owned();
        if tag == "instance" && t.get(e, "guid")?.is_some_and(|g| net::same_ignore_case(g, guid)) {
            remove_at(file, parent, i);
            removed += 1;
            continue;
        }
        if t.get(e, "ref")?.is_some_and(|r| refers_to(r, guid)) {
            *references += 1;
            if tag == "item" {
                remove_at(file, parent, i);
                continue;
            }
            for k in 0..file.attr_ids(e).len() {
                let Some(&(name, _)) = file.attr_ids(e).get(k) else {
                    continue;
                };
                if Table(file).s(name)? == "ref" {
                    let null = intern(file, "null");
                    if let Some(a) = file.attr_ids_mut(e).ok().and_then(|a| a.get_mut(k)) {
                        a.1 = null;
                    }
                }
            }
        }
        if file.is_container(e) {
            removed += drop_in(file, Some(e), guid, references)?;
        }
    }
    Ok(removed)
}

/// `drop=<guid>`: removes the instance with that guid; array items referring to it are removed, single references
/// to it become `null`. Returns (instances removed, references met).
///
/// # Errors
/// A damaged string index.
pub fn drop(file: &mut EditDbx, guid: &str) -> Result<(usize, usize)> {
    let mut references = 0;
    let removed = drop_in(file, None, guid, &mut references)?;
    Ok((removed, references))
}

fn drop_field_in(file: &mut EditDbx, parent: Option<NodeId>, name: &str) -> Result<usize> {
    let mut removed = 0;
    let items = list(file, parent);
    let mut i = items.len();
    while i > 0 {
        i -= 1;
        let Some(&e) = items.get(i) else { continue };
        let t = Table(file);
        if t.tag(e)? != "instance" && t.get(e, "name")? == Some(name) {
            remove_at(file, parent, i);
            removed += 1;
        } else if file.is_container(e) {
            removed += drop_field_in(file, Some(e), name)?;
        }
    }
    Ok(removed)
}

/// `dropfield=<name>`: removes every element (but instances) named `name`. Returns the elements removed.
///
/// # Errors
/// A damaged string index.
pub fn drop_field(file: &mut EditDbx, name: &str) -> Result<usize> {
    drop_field_in(file, None, name)
}

fn set_byte_in(file: &mut EditDbx, parent: Option<NodeId>, name: &str, value: u8) -> Result<usize> {
    let mut set = 0;
    for e in list(file, parent) {
        if file.is_container(e) {
            set += set_byte_in(file, Some(e), name, value)?;
            continue;
        }
        let kind = file.kind(e);
        if kind != TEXT && kind != NUMBERS && file.payload(e).len() == 2 && Table(file).get(e, "name")? == Some(name) {
            let first = file.payload(e).first().copied().unwrap_or(0);
            file.set_payload(e, vec![first, value]).map_err(|_| Fault::null())?;
            set += 1;
        }
    }
    Ok(set)
}

/// `set=<field>|<value>`: every bool / byte field named `name` set to `value`. Returns the fields set.
///
/// # Errors
/// A damaged string index.
pub fn set_byte(file: &mut EditDbx, name: &str, value: u8) -> Result<usize> {
    set_byte_in(file, None, name, value)
}

/// The index of the first instance with this guid among the partition's children.
fn instance_index(file: &EditDbx, guid: &str) -> Result<Option<usize>> {
    let t = Table(file);
    for (i, &e) in partition_children(file)?.iter().enumerate() {
        if t.tag(e)? == "instance" && t.get(e, "guid")?.is_some_and(|g| net::same_ignore_case(g, guid)) {
            return Ok(Some(i));
        }
    }
    Ok(None)
}

/// `setfield=<guid>|<field>|<value>`: a text or numbers field of that instance set (numbers typed like the field's
/// current value, as the import does). Returns whether the field was there.
///
/// # Errors
/// The value is not a number of the field's type, or a damaged file.
pub fn set_field(file: &mut EditDbx, guid: &str, field: &str, value: &str) -> Result<bool> {
    let Some(ii) = instance_index(file, guid)? else {
        return Ok(false);
    };
    let inst = *partition_children(file)?.get(ii).ok_or_else(Fault::null)?;
    let Some(children) = file.children(inst) else {
        return Ok(false);
    };
    let t = Table(file);
    let mut found = None;
    for &c in children {
        if t.get(c, "name")? == Some(field) && (file.kind(c) == TEXT || file.kind(c) == NUMBERS) {
            found = Some(c);
            break;
        }
    }
    let Some(e) = found else { return Ok(false) };
    let payload = if file.kind(e) == TEXT {
        varint_bytes(intern(file, value) as i32)
    } else {
        encode_value(file, e, value)?
    };
    file.set_payload(e, payload).map_err(|_| Fault::null())?;
    Ok(true)
}

/// `partguid=<guid>`: the partition's own guid (every `guid` attribute of the partition).
///
/// # Errors
/// A file without a partition, or a damaged string index.
pub fn set_partition_guid(file: &mut EditDbx, guid: &str) -> Result<()> {
    let p = *file.roots().first().ok_or_else(Fault::list_index)?;
    for k in 0..file.attr_ids(p).len() {
        let Some(&(name, _)) = file.attr_ids(p).get(k) else {
            continue;
        };
        if Table(file).s(name)? == "guid" {
            let v = intern(file, guid);
            if let Some(a) = file.attr_ids_mut(p).ok().and_then(|a| a.get_mut(k)) {
                a.1 = v;
            }
        }
    }
    Ok(())
}

/// `additem=<guid>|<array>|<ref>[|<before ref>]`: an item referring to `reference` added to array `array` of the
/// instance (at the end, or before the item referring to `before`). Returns whether the array was there.
///
/// # Errors
/// A damaged file (an instance that is not a container).
pub fn add_item(file: &mut EditDbx, guid: &str, array: &str, reference: &str, before: Option<&str>) -> Result<bool> {
    let Some(ii) = instance_index(file, guid)? else {
        return Ok(false);
    };
    let (arr, shape, at) = {
        let t = Table(file);
        let inst = *partition_children(file)?.get(ii).ok_or_else(Fault::null)?;
        let children = file.children(inst).ok_or_else(Fault::null)?;
        let mut arr = None;
        for &c in children {
            if t.tag(c)? == "array" && t.get(c, "name")? == Some(array) {
                arr = Some(c);
                break;
            }
        }
        let Some(a) = arr else { return Ok(false) };
        let Some(items) = file.children(a) else {
            return Ok(false);
        };
        let mut shape = None;
        for &c in items {
            if t.tag(c)? == "item" {
                shape = Some(c);
                break;
            }
        }
        let Some(shape) = shape else { return Ok(false) };
        let mut at = items.len();
        if let Some(b) = before {
            for (i, &c) in items.iter().enumerate() {
                if t.get(c, "ref")?.is_some_and(|r| net::same_ignore_case(r, b)) {
                    at = i;
                    break;
                }
            }
        }
        (a, shape, at)
    };
    let name = intern(file, "ref");
    let value = intern(file, reference);
    let item = file.push(Parts {
        tag: file.tag_id(shape),
        kind: file.kind(shape),
        attrs: vec![(name, value)],
        cbyte: file.container_byte(shape),
        children: file.is_container(shape).then(Vec::new),
        payload: file.payload(shape).to_vec(),
    });
    if let Ok(items) = file.children_mut(arr) {
        items.insert(at.min(items.len()), item);
    }
    Ok(true)
}

/// Copies an element of another file into this one: strings interned in DbxEdit's order (tag, each attribute's name
/// then value, a text value, then the children).
fn import_element(file: &mut EditDbx, source: &EditDbx, e: NodeId) -> Result<NodeId> {
    let s = Table(source);
    let tag = intern(file, s.tag(e)?);
    let mut attrs = Vec::with_capacity(source.attr_ids(e).len());
    for &(k, v) in source.attr_ids(e) {
        let name = intern(file, s.s(k)?);
        attrs.push((name, intern(file, s.s(v)?)));
    }
    let payload = if source.kind(e) == TEXT {
        let mut pos = 0;
        let id = read_varint(source.payload(e), &mut pos)?;
        varint_bytes(intern(file, s.s(id as u32)?) as i32)
    } else {
        source.payload(e).to_vec()
    };
    let children = match source.children(e) {
        Some(k) => {
            let mut out = Vec::with_capacity(k.len());
            for &c in k {
                out.push(import_element(file, source, c)?);
            }
            Some(out)
        }
        None => None,
    };
    Ok(file.push(Parts {
        tag,
        kind: source.kind(e),
        attrs,
        cbyte: source.container_byte(e),
        payload,
        children,
    }))
}

/// The first element named `field` below `e` that is not an instance (each element checked before its children).
fn find_field(file: &EditDbx, e: NodeId, field: &str) -> Result<Option<NodeId>> {
    let t = Table(file);
    let Some(k) = file.children(e) else { return Ok(None) };
    for &c in k {
        if t.tag(c)? != "instance" && t.get(c, "name")? == Some(field) {
            return Ok(Some(c));
        }
        if let Some(d) = find_field(file, c, field)? {
            return Ok(Some(d));
        }
    }
    Ok(None)
}

/// The first element named `field` below `e` (any kind), as (its parent, its index).
fn field_slot(file: &EditDbx, e: NodeId, field: &str) -> Result<Option<(NodeId, usize)>> {
    let t = Table(file);
    let Some(k) = file.children(e) else { return Ok(None) };
    for (i, &c) in k.iter().enumerate() {
        if t.get(c, "name")? == Some(field) {
            return Ok(Some((e, i)));
        }
        if let Some(s) = field_slot(file, c, field)? {
            return Ok(Some(s));
        }
    }
    Ok(None)
}

/// `copyfield=<src.dbx>|<src guid>|<field>|<guid>`: copies a field (with arrays) from another file's instance into
/// instance `guid`, replacing the field of that name or added at its end. Returns whether both were found.
///
/// # Errors
/// A damaged file.
pub fn copy_field(file: &mut EditDbx, source: &EditDbx, source_guid: &str, field: &str, guid: &str) -> Result<bool> {
    let Some(from) = instance(source, source_guid)? else {
        return Ok(false);
    };
    let Some(ti) = instance_index(file, guid)? else {
        return Ok(false);
    };
    let Some(original) = find_field(source, from, field)? else {
        return Ok(false);
    };
    let copy = import_element(file, source, original)?;
    let to = *partition_children(file)?.get(ti).ok_or_else(Fault::null)?;
    match field_slot(file, to, field)? {
        Some((parent, i)) => {
            if let Some(slot) = file.children_mut(parent).ok().and_then(|k| k.get_mut(i)) {
                *slot = copy;
            }
        }
        None => file.children_mut(to).map_err(|_| Fault::null())?.push(copy),
    }
    Ok(true)
}

/// Local references (36 characters, no `/`) below `e`, upper-cased, each once, in document order.
fn local_references(file: &EditDbx, e: NodeId, found: &mut Vec<String>, seen: &mut HashSet<String>) -> Result<()> {
    if let Some(r) = Table(file).get(e, "ref")?
        && r.encode_utf16().count() == 36
        && !r.contains('/')
    {
        let up = net::to_upper(r);
        if seen.insert(up.clone()) {
            found.push(up);
        }
    }
    for &c in kids(file, e) {
        local_references(file, c, found, seen)?;
    }
    Ok(())
}

/// `graft=<src.dbx>|<guid>[|<before guid>]`: copies instance `guid` of `source` and, depth first, every instance of
/// `source` it references locally that this file does not have yet; inserted before instance `before`, or at the end
/// of the partition. Returns the instances copied.
///
/// # Errors
/// A damaged file, or an instance without a guid.
pub fn graft(file: &mut EditDbx, source: &EditDbx, guid: &str, before: Option<&str>) -> Result<usize> {
    let mut at = partition_children(file)?.len();
    if let Some(b) = before
        && let Some(i) = instance_index(file, b)?
    {
        at = i;
    }
    let mut available: HashMap<String, NodeId> = HashMap::new();
    for e in instances(source)? {
        let e = e?;
        let g = Table(source).get(e, "guid")?.ok_or_else(Fault::null)?;
        available.insert(net::to_upper(g), e);
    }
    let mut present = HashSet::new();
    for e in instances(file)? {
        let e = e?;
        present.insert(net::to_upper(Table(file).get(e, "guid")?.ok_or_else(Fault::null)?));
    }
    let mut pending = vec![net::to_upper(guid)];
    let mut copied = 0;
    while let Some(next) = pending.pop() {
        if present.contains(&next) {
            continue;
        }
        let Some(&e) = available.get(&next) else { continue };
        present.insert(next);
        let copy = import_element(file, source, e)?;
        partition_children_mut(file)?.insert(at, copy);
        at += 1;
        copied += 1;
        let mut refs = Vec::new();
        local_references(source, e, &mut refs, &mut HashSet::new())?;
        pending.extend(refs);
    }
    Ok(copied)
}
