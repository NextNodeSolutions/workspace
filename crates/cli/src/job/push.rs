mod packing;
mod source;
mod tree;

use std::error::Error;
use std::path::Path;
use std::process::Command;

use base64::{Engine, engine::general_purpose::STANDARD};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::environment::Environment;
use super::{output, process::capture};
use source::Source;
use tree::Tree;

pub(super) const MAX_PUSH_BYTES: usize = 40 * 1024 * 1024;
const MAX_FILE_LIST_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct Upload {
    pub(super) bytes: Vec<u8>,
    provenance: Vec<u8>,
}

#[derive(Serialize)]
struct Provenance {
    commit: String,
    scope_base64: String,
    members_base64: Vec<String>,
    archive_sha256: String,
}

#[derive(Debug, thiserror::Error)]
enum PushError {
    #[error("--push DIR must select a committed directory inside a git work tree")]
    NotWorkTree,
    #[error("--push contains a submodule; export without gitlinks before retrying")]
    Submodule,
    #[error("--push contains an unsafe or unsupported committed path or symlink")]
    UnsafePath,
    #[error("git returned invalid committed source objects while preparing --push")]
    InvalidObject,
    #[error("could not encode the committed --push archive")]
    Archive(#[from] std::io::Error),
}

pub(super) fn archive(
    path: Option<&Path>,
    environment: &Environment,
) -> Result<Option<Upload>, Box<dyn Error>> {
    let Some(path) = path else {
        return Ok(None);
    };
    let source = Source::resolve(path)?;
    let tree = Tree::read(&source)?;
    tree.validate_links()?;
    let archive = packing::archive(&tree)?;
    environment.reject_bytes(&archive)?;
    environment.reject_bytes(source.scope())?;
    let mut gzip = Command::new("gzip");
    gzip.args(["-n", "-c"]);
    let bytes = capture("gzip", gzip, archive, MAX_PUSH_BYTES)?;
    let provenance = Provenance {
        commit: source.commit().to_owned(),
        scope_base64: STANDARD.encode(source.scope()),
        members_base64: tree.members().map(|name| STANDARD.encode(name)).collect(),
        archive_sha256: format!("{:x}", Sha256::digest(&bytes)),
    };
    let provenance = serde_json::to_vec(&provenance)?;
    environment.reject_bytes(&provenance)?;
    Ok(Some(Upload { bytes, provenance }))
}

impl Upload {
    pub(super) fn save(&self, directory: &Path) -> Result<(), Box<dyn Error>> {
        output::write(&directory.join("source.json"), &self.provenance)?;
        Ok(())
    }
}
