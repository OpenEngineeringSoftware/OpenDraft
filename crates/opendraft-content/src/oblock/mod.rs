//! The `.oblock` file: exactly one block, with what it needs, in a file of its
//! own (the file `WBLOCK` writes). It opens in the Block Editor and nowhere
//! else; a bundle of blocks is an `.odpack`.
//!
//! In its development phase: `format_version` 0, changed in place with no
//! conversion, written the way `.odraft` is.
//!
//! ```text
//! { "format": "oblock", "format_version": 0,
//!   "package": { ... the `opendraft.blocks` payload ... } }
//! ```
//!
//! The payload is the library's own (see `blocks_payload`): a drawing that
//! holds the block's definition and the blocks it nests, with the layers,
//! linetypes and text styles they use, and one entry for each block. Exactly
//! one entry is the block itself; the rest are components. So the library
//! stores a block in the form this file has, and importing or exporting one is
//! a copy. A block's name, description, base point and unit are its
//! definition's; the entry adds a stable id and tags.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use opendraft_core::{BlockDef, BlockId, Document};
use serde_json::{json, Value};

use crate::blocks_payload::{BlockUuid, BlocksPackage, Entry, PayloadError};
use crate::envelope::strict_json::{self, JsonError};
use crate::library::store::write_atomic;

pub use dxf::DxfBlock;

pub const FORMAT: &str = "oblock";
/// 0 is the development phase: see the module notes.
pub const FORMAT_VERSION: u64 = 0;
pub const EXTENSION: &str = "oblock";
/// A file is at most this big, checked before it is parsed.
pub const MAX_BYTES: usize = 32 * 1024 * 1024;
const MAX_DEPTH: usize = 32;
const MAX_VALUES: usize = 4_000_000;

/// Why a file is not a readable `.oblock`, or a block cannot be one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OblockError {
    TooLarge {
        max: usize,
    },
    Json(JsonError),
    /// `format` is missing or is not `oblock`.
    NotOblock,
    /// `format_version` is missing or is not the one this app reads.
    FormatVersion(Option<u64>),
    /// A top-level field that is not `format`, `format_version` or `package`.
    UnknownField(String),
    Payload(PayloadError),
    /// The payload does not hold exactly one block that is not a component.
    NotOneBlock(usize),
    /// The block is not in the drawing it is taken from.
    NoSuchBlock,
    /// Taking the block from its drawing failed.
    Refused(String),
    Io(String),
}

impl std::fmt::Display for OblockError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OblockError::TooLarge { max } => write!(f, "is larger than {} MB", max >> 20),
            OblockError::Json(e) => write!(f, "{e}"),
            OblockError::NotOblock => write!(f, "is not an oblock file"),
            OblockError::FormatVersion(Some(v)) => {
                write!(f, "is oblock version {v}; this app reads {FORMAT_VERSION}")
            }
            OblockError::FormatVersion(None) => write!(f, "has no readable version"),
            OblockError::UnknownField(name) => write!(f, "has an unknown field {name:?}"),
            OblockError::Payload(e) => write!(f, "{e}"),
            OblockError::NotOneBlock(0) => write!(f, "holds no block"),
            OblockError::NotOneBlock(n) => write!(f, "holds {n} blocks, not one"),
            OblockError::NoSuchBlock => write!(f, "There is no such block."),
            OblockError::Refused(why) | OblockError::Io(why) => write!(f, "{why}"),
        }
    }
}

impl std::error::Error for OblockError {}

impl From<PayloadError> for OblockError {
    fn from(e: PayloadError) -> Self {
        OblockError::Payload(e)
    }
}

/// One block and what it needs. Always valid: every way to make one checks it.
#[derive(Debug, Clone)]
pub struct BlockFile {
    package: BlocksPackage,
}

impl BlockFile {
    /// Block `block` of `source`, with the blocks it nests and the layers,
    /// linetypes and text styles they use, and `tags`. What did not carry
    /// over exactly is worded in the second part.
    pub fn from_block(
        source: &Document,
        block: BlockId,
        tags: &[String],
    ) -> Result<(BlockFile, Vec<String>), OblockError> {
        let mut drawing = Document::default();
        let brought = drawing
            .bring_block(source, block, &HashMap::new())
            .map_err(|e| match e {
                opendraft_core::BringError::NoSuchBlock => OblockError::NoSuchBlock,
                other => OblockError::Refused(other.to_string()),
            })?;
        let entries = brought
            .blocks
            .iter()
            .map(|(from, made)| Entry {
                uuid: BlockUuid::new(),
                block: made.id,
                folder: String::new(),
                tags: if *from == block {
                    tags.to_vec()
                } else {
                    Vec::new()
                },
                component: *from != block,
            })
            .collect();
        let warnings = brought.warnings.iter().map(ToString::to_string).collect();
        let file = BlockFile::from_package(BlocksPackage { drawing, entries })?;
        Ok((file, warnings))
    }

    /// A package that holds exactly one block that is not a component.
    pub fn from_package(package: BlocksPackage) -> Result<BlockFile, OblockError> {
        let roots = package.entries.iter().filter(|e| !e.component).count();
        if roots != 1 {
            return Err(OblockError::NotOneBlock(roots));
        }
        package.validate()?;
        Ok(BlockFile { package })
    }

    /// The payload, as the library holds it.
    pub fn package(&self) -> &BlocksPackage {
        &self.package
    }

    pub fn into_package(self) -> BlocksPackage {
        self.package
    }

    /// The drawing that holds the block and the blocks it nests.
    pub fn drawing(&self) -> &Document {
        &self.package.drawing
    }

    fn root_entry(&self) -> &Entry {
        // `from_package` checked there is one.
        self.package
            .entries
            .iter()
            .find(|e| !e.component)
            .unwrap_or(&self.package.entries[0])
    }

    /// The block's id in `drawing()`.
    pub fn root(&self) -> BlockId {
        self.root_entry().block
    }

    pub fn uuid(&self) -> BlockUuid {
        self.root_entry().uuid
    }

    pub fn tags(&self) -> &[String] {
        &self.root_entry().tags
    }

    pub fn definition(&self) -> Option<&BlockDef> {
        self.package.drawing.block(self.root()).map(|b| &**b)
    }

    pub fn name(&self) -> &str {
        self.definition().map_or("", |d| d.name.as_str())
    }

    /// The same block after it was edited: `drawing` is the container's
    /// drawing as the Block Editor left it. A block it gained is a component;
    /// one it lost leaves the file. The block itself must still be there.
    pub fn with_drawing(&self, drawing: Document) -> Result<BlockFile, OblockError> {
        let root = self.root();
        if drawing.block(root).is_none() {
            return Err(OblockError::NoSuchBlock);
        }
        let mut entries: Vec<Entry> = self
            .package
            .entries
            .iter()
            .filter(|e| drawing.block(e.block).is_some())
            .cloned()
            .collect();
        let known: HashSet<BlockId> = entries.iter().map(|e| e.block).collect();
        for def in drawing.blocks().iter().filter(|b| !known.contains(&b.id)) {
            entries.push(Entry {
                uuid: BlockUuid::new(),
                block: def.id,
                folder: String::new(),
                tags: Vec::new(),
                component: true,
            });
        }
        BlockFile::from_package(BlocksPackage { drawing, entries })
    }

    /// Reads a file's bytes: the size cap, then the strict parse, then the
    /// format, then the payload and its rules.
    pub fn read(bytes: &[u8]) -> Result<BlockFile, OblockError> {
        if bytes.len() > MAX_BYTES {
            return Err(OblockError::TooLarge { max: MAX_BYTES });
        }
        let value = strict_json::parse(bytes, MAX_DEPTH, MAX_VALUES).map_err(OblockError::Json)?;
        let Value::Object(mut top) = value else {
            return Err(OblockError::NotOblock);
        };
        if top.remove("format").as_ref().and_then(Value::as_str) != Some(FORMAT) {
            return Err(OblockError::NotOblock);
        }
        let version = top.remove("format_version").and_then(|v| v.as_u64());
        if version != Some(FORMAT_VERSION) {
            return Err(OblockError::FormatVersion(version));
        }
        let package = top.remove("package").ok_or(OblockError::NotOblock)?;
        if let Some(name) = top.keys().next() {
            return Err(OblockError::UnknownField(name.clone()));
        }
        BlockFile::from_package(BlocksPackage::from_value(&package)?)
    }

    /// The file's bytes: indented so it can be read and compared by eye.
    /// What is written is read back first, so this never produces a file that
    /// `read` would refuse.
    pub fn to_bytes(&self) -> Result<Vec<u8>, OblockError> {
        let file = json!({
            "format": FORMAT,
            "format_version": FORMAT_VERSION,
            "package": self.package.to_value()?,
        });
        let mut bytes =
            serde_json::to_vec_pretty(&file).map_err(|e| OblockError::Io(e.to_string()))?;
        bytes.push(b'\n');
        BlockFile::read(&bytes)?;
        Ok(bytes)
    }

    /// Writes the file to `path`: to a temporary file beside it, then renamed
    /// over the old one, so a crash leaves the old file or the new one.
    pub fn save(&self, path: &Path) -> Result<(), OblockError> {
        let bytes = self.to_bytes()?;
        write_atomic(path, &bytes).map_err(|e| OblockError::Io(e.to_string()))
    }

    /// Reads the file at `path`.
    pub fn load(path: &Path) -> Result<BlockFile, OblockError> {
        let len = std::fs::metadata(path)
            .map_err(|e| OblockError::Io(e.to_string()))?
            .len();
        if len > MAX_BYTES as u64 {
            return Err(OblockError::TooLarge { max: MAX_BYTES });
        }
        let bytes = std::fs::read(path).map_err(|e| OblockError::Io(e.to_string()))?;
        BlockFile::read(&bytes)
    }
}

mod dxf;
#[cfg(test)]
mod dxf_tests;
#[cfg(test)]
mod tests;
