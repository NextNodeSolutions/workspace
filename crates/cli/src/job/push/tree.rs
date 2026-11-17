mod links;

use std::collections::BTreeMap;
use std::error::Error;

use super::{MAX_PUSH_BYTES, PushError, source::Source};

const DIRECTORY_MODE: &[u8] = b"040000";
const FILE_MODE: &[u8] = b"100644";
const EXECUTABLE_MODE: &[u8] = b"100755";
const SYMLINK_MODE: &[u8] = b"120000";
const SUBMODULE_MODE: &[u8] = b"160000";
const MAX_PATH_BYTES: usize = 4096;
const SHA1_HEX_LENGTH: usize = 40;
const SHA256_HEX_LENGTH: usize = 64;

pub(super) enum Kind {
    Directory,
    File,
    Executable,
    Symlink,
}

pub(super) struct Entry {
    pub(super) kind: Kind,
    pub(super) contents: Vec<u8>,
    object: String,
}

pub(super) struct Tree {
    pub(super) entries: BTreeMap<Vec<u8>, Entry>,
}

impl Tree {
    pub(super) fn read(source: &Source) -> Result<Self, Box<dyn Error>> {
        let listing = source.listing()?;
        let mut entries = BTreeMap::new();
        for record in listing
            .split(|byte| *byte == 0)
            .filter(|record| !record.is_empty())
        {
            let (name, entry) = parse(record)?;
            if entries.insert(name, entry).is_some() {
                return Err(PushError::InvalidObject.into());
            }
        }
        let requests = entries
            .values()
            .filter(|entry| !matches!(entry.kind, Kind::Directory))
            .flat_map(|entry| entry.object.bytes().chain(std::iter::once(b'\n')))
            .collect();
        let contents = source.run(&["cat-file", "--batch"], requests, MAX_PUSH_BYTES)?;
        fill(&mut entries, &contents)?;
        Ok(Self { entries })
    }

    pub(super) fn members(&self) -> impl Iterator<Item = &[u8]> {
        self.entries.keys().map(Vec::as_slice)
    }

    pub(super) fn validate_links(&self) -> Result<(), PushError> {
        links::validate(&self.entries)
    }
}

fn parse(record: &[u8]) -> Result<(Vec<u8>, Entry), PushError> {
    let separator = record
        .iter()
        .position(|byte| *byte == b'\t')
        .ok_or(PushError::InvalidObject)?;
    let name = &record[separator + 1..];
    if name.len() > MAX_PATH_BYTES
        || name
            .split(|byte| *byte == b'/')
            .any(|part| matches!(part, b"" | b"." | b".."))
    {
        return Err(PushError::UnsafePath);
    }
    let mut fields = record[..separator].split(|byte| *byte == b' ');
    let kind = match fields.next() {
        Some(DIRECTORY_MODE) => Kind::Directory,
        Some(FILE_MODE) => Kind::File,
        Some(EXECUTABLE_MODE) => Kind::Executable,
        Some(SYMLINK_MODE) => Kind::Symlink,
        Some(SUBMODULE_MODE) => return Err(PushError::Submodule),
        _ => return Err(PushError::InvalidObject),
    };
    let expected_type = if matches!(kind, Kind::Directory) {
        b"tree".as_slice()
    } else {
        b"blob"
    };
    if fields.next() != Some(expected_type) {
        return Err(PushError::InvalidObject);
    }
    let object = fields.next().ok_or(PushError::InvalidObject)?;
    if !matches!(object.len(), SHA1_HEX_LENGTH | SHA256_HEX_LENGTH)
        || !object.iter().all(u8::is_ascii_hexdigit)
        || fields.next().is_some()
    {
        return Err(PushError::InvalidObject);
    }
    let object = String::from_utf8(object.to_vec()).map_err(|_| PushError::InvalidObject)?;
    Ok((
        name.to_vec(),
        Entry {
            kind,
            contents: Vec::new(),
            object,
        },
    ))
}

fn fill(entries: &mut BTreeMap<Vec<u8>, Entry>, mut remaining: &[u8]) -> Result<(), PushError> {
    for entry in entries
        .values_mut()
        .filter(|entry| !matches!(entry.kind, Kind::Directory))
    {
        let newline = remaining
            .iter()
            .position(|byte| *byte == b'\n')
            .ok_or(PushError::InvalidObject)?;
        let header =
            std::str::from_utf8(&remaining[..newline]).map_err(|_| PushError::InvalidObject)?;
        let size = header
            .strip_prefix(&format!("{} blob ", entry.object))
            .ok_or(PushError::InvalidObject)?
            .parse::<usize>()
            .map_err(|_| PushError::InvalidObject)?;
        remaining = &remaining[newline + 1..];
        entry.contents = remaining
            .get(..size)
            .ok_or(PushError::InvalidObject)?
            .to_vec();
        remaining = remaining
            .get(size..)
            .and_then(|tail| tail.strip_prefix(b"\n"))
            .ok_or(PushError::InvalidObject)?;
    }
    if !remaining.is_empty() {
        return Err(PushError::InvalidObject);
    }
    Ok(())
}
