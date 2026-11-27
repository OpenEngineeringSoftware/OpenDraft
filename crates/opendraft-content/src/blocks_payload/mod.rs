//! The `opendraft.blocks` payload (specification 11), in its development
//! phase: `kind_version` 0, changed in place with no conversion, written the
//! way `.odraft` is.
//!
//! ```text
//! "package": {
//!   "drawing": { ... an .odraft drawing holding the block definitions ... },
//!   "blocks":  [ { "id": "<uuid>", "block": <id in the drawing>,
//!                  "folder": "CL600", "tags": ["valve"], "component": false } ]
//! }
//! ```
//!
//! The blocks' geometry, layers, linetypes, text styles, attributes and
//! dynamic schemas are the drawing's, read and written by the one `.odraft`
//! codec, so a new kind of entity is added once. A block's name, description,
//! base point and unit live in its definition only; `blocks` adds what a
//! library needs and a definition has no place for: a stable id, the folder,
//! tags, and whether it is a component (a block another block contains, shown
//! only under its parent). Every block of the drawing has exactly one entry.
//!
//! Whatever is read is validated before it is used: the drawing by
//! `Document::validate` (inside the `.odraft` reader), the rest here.

use std::collections::HashSet;

use opendraft_core::block::block_name_problem;
use opendraft_core::{BlockId, Document};
use opendraft_files::{FileFormat, NativeFormat};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::envelope::text::{check_line, check_paragraphs};
use crate::envelope::{Envelope, Header};

pub mod folder;
mod names;
mod uuid_id;

pub use uuid_id::BlockUuid;

pub const KIND: &str = "opendraft.blocks";
/// 0 is the development phase: see the module notes.
pub const KIND_VERSION: u32 = 0;

pub const MAX_TAGS: usize = 32;
pub const MAX_TAG: usize = 32;
pub const MAX_DESCRIPTION: usize = 8 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadError {
    /// The header's `kind` is not `opendraft.blocks`.
    WrongKind(String),
    /// Written by a newer (or older development) build.
    KindVersion { found: u32, supported: u32 },
    /// The payload's own fields are wrong.
    Malformed(String),
    /// The embedded drawing would not read or write.
    Drawing(String),
    /// Read, but breaks a rule.
    Invalid(String),
}

impl std::fmt::Display for PayloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PayloadError::WrongKind(kind) => write!(f, "is {kind:?}, not blocks"),
            PayloadError::KindVersion { found, supported } => write!(
                f,
                "has block data version {found}; this app reads {supported}"
            ),
            PayloadError::Malformed(m) => write!(f, "has unreadable block data: {m}"),
            PayloadError::Drawing(m) => write!(f, "has a damaged drawing of blocks: {m}"),
            PayloadError::Invalid(m) => write!(f, "has invalid block data: {m}"),
        }
    }
}

impl std::error::Error for PayloadError {}

/// What a library knows of one block beyond its definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub uuid: BlockUuid,
    /// The block's id in the package's drawing.
    pub block: BlockId,
    /// Path inside the package (`""` for the top level); none for a
    /// component.
    pub folder: String,
    pub tags: Vec<String>,
    pub component: bool,
}

/// The payload as the app holds it.
#[derive(Debug, Clone)]
pub struct BlocksPackage {
    pub drawing: Document,
    pub entries: Vec<Entry>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageDto {
    drawing: Value,
    blocks: Vec<EntryDto>,
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryDto {
    id: String,
    block: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    folder: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    tags: Vec<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    component: bool,
}

impl BlocksPackage {
    pub fn empty() -> Self {
        BlocksPackage {
            drawing: Document::default(),
            entries: Vec::new(),
        }
    }

    pub fn entry(&self, uuid: BlockUuid) -> Option<&Entry> {
        self.entries.iter().find(|e| e.uuid == uuid)
    }

    pub fn entry_for_block(&self, block: BlockId) -> Option<&Entry> {
        self.entries.iter().find(|e| e.block == block)
    }

    /// Reads a package's payload from its envelope and validates it.
    pub fn from_envelope(envelope: &Envelope) -> Result<BlocksPackage, PayloadError> {
        let header = &envelope.header;
        if header.kind.to_string() != KIND {
            return Err(PayloadError::WrongKind(header.kind.to_string()));
        }
        if header.kind_version != KIND_VERSION {
            return Err(PayloadError::KindVersion {
                found: header.kind_version,
                supported: KIND_VERSION,
            });
        }
        BlocksPackage::from_value(&envelope.package)
    }

    /// Reads the payload itself (an envelope's `package`, or an `.oblock`'s)
    /// and validates it.
    pub fn from_value(package: &Value) -> Result<BlocksPackage, PayloadError> {
        let dto =
            PackageDto::deserialize(package).map_err(|e| PayloadError::Malformed(e.to_string()))?;
        let bytes =
            serde_json::to_vec(&dto.drawing).map_err(|e| PayloadError::Drawing(e.to_string()))?;
        let loaded = NativeFormat
            .read(&bytes)
            .map_err(|e| PayloadError::Drawing(e.to_string()))?;
        let entries = dto
            .blocks
            .into_iter()
            .map(|e| {
                let uuid = BlockUuid::parse(&e.id)
                    .ok_or_else(|| PayloadError::Malformed(format!("{:?} is not an id", e.id)))?;
                Ok(Entry {
                    uuid,
                    block: BlockId(e.block),
                    folder: e.folder,
                    tags: e.tags,
                    component: e.component,
                })
            })
            .collect::<Result<Vec<_>, PayloadError>>()?;
        let package = BlocksPackage {
            drawing: loaded.document,
            entries,
        };
        package.validate()?;
        Ok(package)
    }

    /// The package as an envelope with `header` (whose kind this sets).
    pub fn to_envelope(&self, mut header: Header) -> Result<Envelope, PayloadError> {
        header.kind_version = KIND_VERSION;
        Ok(Envelope {
            header,
            package: self.to_value()?,
        })
    }

    /// The payload itself, validated first.
    pub fn to_value(&self) -> Result<Value, PayloadError> {
        self.validate()?;
        let encoded = NativeFormat
            .write(&self.drawing)
            .map_err(|e| PayloadError::Drawing(e.to_string()))?;
        let drawing: Value = serde_json::from_slice(&encoded.bytes)
            .map_err(|e| PayloadError::Drawing(e.to_string()))?;
        let dto = PackageDto {
            drawing,
            blocks: self
                .entries
                .iter()
                .map(|e| EntryDto {
                    id: e.uuid.to_string(),
                    block: e.block.0,
                    folder: e.folder.clone(),
                    tags: e.tags.clone(),
                    component: e.component,
                })
                .collect(),
        };
        serde_json::to_value(dto).map_err(|e| PayloadError::Malformed(e.to_string()))
    }

    /// The rules that are not the drawing's own (`Document::validate` covers
    /// those): ids, folders, tags, plain text, and nothing that names a file.
    pub fn validate(&self) -> Result<(), PayloadError> {
        let invalid = |m: String| Err(PayloadError::Invalid(m));
        let mut uuids = HashSet::new();
        let mut blocks = HashSet::new();
        for entry in &self.entries {
            if !uuids.insert(entry.uuid) {
                return invalid(format!("id {} is used twice", entry.uuid));
            }
            if !blocks.insert(entry.block) {
                return invalid(format!("block {} has two entries", entry.block.0));
            }
            let Some(def) = self.drawing.block(entry.block) else {
                return invalid(format!(
                    "entry {} names a block that is not there",
                    entry.uuid
                ));
            };
            if entry.component && !entry.folder.is_empty() {
                return invalid(format!("component {:?} has a folder", def.name));
            }
            folder::check_path(&entry.folder, folder::MAX_LIBRARY_DEPTH)
                .map_err(|e| PayloadError::Invalid(e.to_string()))?;
            if entry.tags.len() > MAX_TAGS {
                return invalid(format!("{:?} has more than {MAX_TAGS} tags", def.name));
            }
            for tag in &entry.tags {
                if check_line(tag, MAX_TAG).is_err() || tag.trim() != tag {
                    return invalid(format!("tag {tag:?} of {:?} is not valid", def.name));
                }
            }
        }
        for def in self.drawing.blocks() {
            if !blocks.contains(&def.id) {
                return invalid(format!("block {:?} has no entry", def.name));
            }
            if let Some(problem) = block_name_problem(&def.name) {
                return invalid(format!("{:?}: {problem}", def.name));
            }
            check_line(&def.name, 255)
                .map_err(|e| PayloadError::Invalid(format!("block name {:?} {e}", def.name)))?;
            if !def.description.is_empty() {
                check_paragraphs(&def.description, MAX_DESCRIPTION).map_err(|e| {
                    PayloadError::Invalid(format!("the description of {:?} {e}", def.name))
                })?;
            }
        }
        names::check_shown_names(&self.drawing).map_err(PayloadError::Invalid)
    }
}

#[cfg(test)]
mod tests;
