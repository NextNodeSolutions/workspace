use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::{ffi::OsStr, path::Path};

use tar::{Builder, EntryType, Header};

use super::tree::{Entry, Kind, Tree};
use super::{MAX_PUSH_BYTES, PushError};

const FILE_PERMISSIONS: u32 = 0o644;
const EXECUTABLE_PERMISSIONS: u32 = 0o755;
const SYMLINK_PERMISSIONS: u32 = 0o777;
const ARCHIVE_TIMESTAMP: u64 = 0;
const ARCHIVE_OWNER: u64 = 0;

pub(super) fn archive(tree: &Tree) -> Result<Vec<u8>, PushError> {
    let mut archive = Builder::new(ArchiveBuffer(Vec::new()));
    for (name, entry) in &tree.entries {
        append(&mut archive, name, entry)?;
    }
    Ok(archive.into_inner()?.0)
}

fn append(archive: &mut Builder<ArchiveBuffer>, name: &[u8], entry: &Entry) -> io::Result<()> {
    let mut header = Header::new_gnu();
    let (kind, mode) = match entry.kind {
        Kind::Directory => (EntryType::Directory, EXECUTABLE_PERMISSIONS),
        Kind::File => (EntryType::Regular, FILE_PERMISSIONS),
        Kind::Executable => (EntryType::Regular, EXECUTABLE_PERMISSIONS),
        Kind::Symlink => (EntryType::Symlink, SYMLINK_PERMISSIONS),
    };
    header.set_entry_type(kind);
    header.set_mode(mode);
    header.set_mtime(ARCHIVE_TIMESTAMP);
    header.set_uid(ARCHIVE_OWNER);
    header.set_gid(ARCHIVE_OWNER);
    let name = Path::new(OsStr::from_bytes(name));
    if matches!(entry.kind, Kind::Symlink) {
        header.set_size(0);
        return archive.append_link(
            &mut header,
            name,
            Path::new(OsStr::from_bytes(&entry.contents)),
        );
    }
    header.set_size(u64::try_from(entry.contents.len()).expect("bounded source length fits u64"));
    archive.append_data(&mut header, name, entry.contents.as_slice())
}

struct ArchiveBuffer(Vec<u8>);

impl Write for ArchiveBuffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if bytes.len() > MAX_PUSH_BYTES - self.0.len() {
            return Err(io::Error::other("--push archive exceeds its size limit"));
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
