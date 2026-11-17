use std::collections::{BTreeMap, BTreeSet, VecDeque};

use super::{Entry, Kind, MAX_PATH_BYTES, PushError};

const MAX_LINK_EXPANSIONS: usize = 40;

pub(super) fn validate(entries: &BTreeMap<Vec<u8>, Entry>) -> Result<(), PushError> {
    for (name, entry) in entries {
        if matches!(entry.kind, Kind::Symlink) {
            resolve(name, entries)?;
        }
    }
    Ok(())
}

fn resolve(name: &[u8], entries: &BTreeMap<Vec<u8>, Entry>) -> Result<(), PushError> {
    let mut pending = components(name);
    let mut resolved = Vec::new();
    let mut followed = BTreeSet::new();
    while let Some(part) = pending.pop_front() {
        if part == b"." {
            continue;
        }
        if part == b".." {
            resolved.pop().ok_or(PushError::UnsafePath)?;
            continue;
        }
        resolved.push(part);
        let path = resolved.join(&b'/');
        let entry = entries.get(&path).ok_or(PushError::UnsafePath)?;
        if matches!(entry.kind, Kind::Symlink) {
            let mut target = expand(entry, path, &mut followed)?;
            resolved.pop();
            target.append(&mut pending);
            pending = target;
        } else if !matches!(entry.kind, Kind::Directory) && !pending.is_empty() {
            return Err(PushError::UnsafePath);
        }
    }
    Ok(())
}

fn expand(
    entry: &Entry,
    path: Vec<u8>,
    followed: &mut BTreeSet<Vec<u8>>,
) -> Result<VecDeque<Vec<u8>>, PushError> {
    if followed.len() >= MAX_LINK_EXPANSIONS || !followed.insert(path) {
        return Err(PushError::UnsafePath);
    }
    target(&entry.contents)
}

fn target(bytes: &[u8]) -> Result<VecDeque<Vec<u8>>, PushError> {
    if bytes.is_empty()
        || bytes.len() > MAX_PATH_BYTES
        || bytes.starts_with(b"/")
        || bytes.contains(&0)
    {
        return Err(PushError::UnsafePath);
    }
    Ok(components(bytes))
}

fn components(bytes: &[u8]) -> VecDeque<Vec<u8>> {
    bytes
        .split(|byte| *byte == b'/')
        .filter(|part| !part.is_empty())
        .map(<[u8]>::to_vec)
        .collect()
}
