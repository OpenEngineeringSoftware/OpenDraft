//! A block file as a DXF, and a DXF as a block file (see
//! `opendraft_files::export_block` for the layout and why).
//!
//! Our own file carries a marker (the block's name, id, description and tags,
//! as one line of ASCII JSON), so reading it back gives the exact block. A
//! file from anywhere else is read by its model space: with objects in it, the
//! file is one block, named after the file, with the base point from
//! `$INSBASE` and the unit from `$INSUNITS`, and the blocks it holds come
//! along as the blocks it nests. A file with an empty model space (a library
//! drawing in the DesignCenter way) is not one block: its named blocks are
//! offered one by one, by the importer that already does that.

use opendraft_core::block::block_name_problem;
use opendraft_core::BlockDef;
use opendraft_files::{DxfFormat, FileFormat};
use serde_json::{json, Value};

use super::{BlockFile, OblockError};
use crate::blocks_payload::{BlockUuid, MAX_DESCRIPTION, MAX_TAG, MAX_TAGS};
use crate::envelope::text::{check_line, check_paragraphs};

const KIND: &str = "opendraft.block";

/// What reading a DXF as a block found.
#[derive(Debug)]
pub enum DxfBlock {
    /// The file is one block. `warnings` say what did not come across.
    One {
        file: Box<BlockFile>,
        warnings: Vec<String>,
    },
    /// Model space is empty: the file's named blocks are the candidates.
    NoModelSpace,
}

/// What our own marker says of a block.
struct Marker {
    name: String,
    id: BlockUuid,
    description: String,
    tags: Vec<String>,
}

impl BlockFile {
    /// The block as a DXF (bytes) and what did not carry across.
    pub fn to_dxf(&self) -> Result<(Vec<u8>, Vec<String>), OblockError> {
        let def = self.definition().ok_or(OblockError::NoSuchBlock)?;
        let marker = json!({
            "kind": KIND,
            "name": def.name,
            "id": self.uuid().to_string(),
            "description": def.description,
            "tags": self.tags(),
        });
        let encoded =
            opendraft_files::export_block(self.drawing(), self.root(), &ascii(&marker.to_string()))
                .map_err(|e| OblockError::Refused(e.to_string()))?;
        Ok((encoded.bytes, encoded.notes))
    }

    /// A DXF read as one block, or a note that it holds none in model space.
    /// `file_stem` (the file's name without its extension) names a block that
    /// has no marker.
    pub fn from_dxf(bytes: &[u8], file_stem: &str) -> Result<DxfBlock, OblockError> {
        let loaded = DxfFormat
            .read(bytes)
            .map_err(|e| OblockError::Refused(e.to_string()))?;
        let mut document = loaded.document;
        let objects: Vec<_> = document
            .objects()
            .iter()
            .filter(|o| o.layout.is_none())
            .cloned()
            .collect();
        if objects.is_empty() {
            return Ok(DxfBlock::NoModelSpace);
        }
        let header = opendraft_files::block_header(bytes);
        let marker = header.marker.as_deref().and_then(read_marker);
        let wanted = match &marker {
            Some(m) => m.name.clone(),
            None => name_from_file(file_stem),
        };
        let name = document.unused_block_name_like(&wanted);
        let id = document.allocate_block_id();
        let mut def = BlockDef::from_objects(id, &name, header.insbase, &objects);
        def.unit = document.units().insert;
        if let Some(m) = &marker {
            def.description.clone_from(&m.description);
        }
        let change = document
            .redefined_blocks(def)
            .map_err(|why| OblockError::Refused(why.to_owned()))?;
        document.set_blocks(change.blocks);
        let tags = marker.as_ref().map_or(Vec::new(), |m| m.tags.clone());
        let (file, mut warnings) = BlockFile::from_block(&document, id, &tags)?;
        let file = match &marker {
            Some(m) => file.with_uuid(m.id)?,
            None => file,
        };
        warnings.splice(0..0, loaded.warnings);
        Ok(DxfBlock::One {
            file: Box::new(file),
            warnings,
        })
    }

    /// The same block with another stable id (for a file that names its own).
    fn with_uuid(mut self, uuid: BlockUuid) -> Result<BlockFile, OblockError> {
        if let Some(entry) = self.package.entries.iter_mut().find(|e| !e.component) {
            entry.uuid = uuid;
        }
        BlockFile::from_package(self.package)
    }
}

/// A block name made from a file's name: characters a block name cannot hold
/// become `_`, and nothing left is "Block".
pub(super) fn name_from_file(stem: &str) -> String {
    let name: String = stem
        .trim()
        .chars()
        .take(255)
        .map(|c| {
            if block_name_problem(&c.to_string()).is_some() {
                '_'
            } else {
                c
            }
        })
        .collect();
    if name.trim().is_empty() || block_name_problem(&name).is_some() {
        "Block".to_owned()
    } else {
        name
    }
}

/// The marker, when it is ours and every part of it is valid; otherwise the
/// file is read as a foreign one.
fn read_marker(text: &str) -> Option<Marker> {
    let value: Value = serde_json::from_str(text).ok()?;
    if value.get("kind")?.as_str()? != KIND {
        return None;
    }
    let name = value.get("name")?.as_str()?.to_owned();
    if block_name_problem(&name).is_some() || check_line(&name, 255).is_err() {
        return None;
    }
    let id = BlockUuid::parse(value.get("id")?.as_str()?)?;
    let description = value.get("description")?.as_str()?.to_owned();
    if !description.is_empty() && check_paragraphs(&description, MAX_DESCRIPTION).is_err() {
        return None;
    }
    let tags: Vec<String> = value
        .get("tags")?
        .as_array()?
        .iter()
        .map(|t| t.as_str().map(str::to_owned))
        .collect::<Option<_>>()?;
    let valid = tags.len() <= MAX_TAGS
        && tags
            .iter()
            .all(|t| check_line(t, MAX_TAG).is_ok() && t.trim() == t);
    valid.then_some(Marker {
        name,
        id,
        description,
        tags,
    })
}

/// `text` with every character beyond ASCII written as a JSON `\u` escape, so
/// the marker is one line of ASCII.
pub(super) fn ascii(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c);
        } else {
            let mut units = [0u16; 2];
            for unit in c.encode_utf16(&mut units) {
                out.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    out
}
