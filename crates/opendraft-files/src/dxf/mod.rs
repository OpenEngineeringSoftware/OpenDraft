//! AutoCAD DXF: an exchange format for drawings, not one the app can keep a
//! drawing in without loss. Opening a DXF is an import (the result is a new
//! unsaved drawing) and saving one is an export; the notes and warnings
//! returned say what did not carry over.

pub(crate) mod block_file;
mod blocks;
mod ccs;
mod dimensions;
mod dimstyle_codec;
mod header;
#[cfg(test)]
mod linetype_tests;
mod linetypes;
mod model_freeze;
mod modify_settings;
mod point_style;
mod read;
mod units;
mod write;
mod write2000;

use opendraft_core::Document;

use crate::{Encoded, FileError, FileFormat, Loaded};

/// The dynamic blocks a DXF holds, read into plain values (not yet used to
/// build blocks).
pub mod dynamic {
    pub use super::read::dynamic::{
        dynamic_blocks, Action, Column, DynamicBlocks, DynamicDefinition, DynamicInstance, Node,
        NodeKind, NodeState, Parameter, Table,
    };
}

pub struct DxfFormat;

/// The largest DXF read, as for DWG: the reader holds the whole file and
/// its groups in memory.
const MAX_FILE_BYTES: u64 = 1 << 30;

impl FileFormat for DxfFormat {
    fn name(&self) -> &'static str {
        "AutoCAD DXF"
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["dxf"]
    }

    fn read(&self, bytes: &[u8]) -> Result<Loaded, FileError> {
        read::read(bytes)
    }

    fn check_size(&self, len: u64) -> Result<(), FileError> {
        if len > MAX_FILE_BYTES {
            return Err(FileError::Unsupported(
                "this DXF exceeds the safe file size limit".into(),
            ));
        }
        Ok(())
    }

    fn write(&self, document: &Document) -> Result<Encoded, FileError> {
        write_document(document, None)
    }
}

/// The DXF of `document`: for a drawing, or (with `block`) for a block file.
fn write_document(
    document: &Document,
    block: Option<&write2000::BlockExtras>,
) -> Result<Encoded, FileError> {
    // DXF cannot keep a dynamic block's parameters and actions, so each
    // reference is written as the block it draws now.
    let materialized = document.materialized_static();
    let document = materialized.as_ref().map_or(document, |m| &m.document);
    if let Some(tag) = blocks::first_field(document) {
        return Err(FileError::Unsupported(format!(
            "attribute \"{tag}\" holds a data field, which DXF export cannot keep yet; save as .odraft"
        )));
    }
    let (flat, count) = dimensions::exploded(document);
    // A block file's DXF is always R2000, which has the objects its marker
    // lives in.
    let mut encoded = match block {
        Some(_) => write2000::write_with(&flat, block)?,
        None => write::write(&flat)?,
    };
    if count > 0 {
        encoded.notes.push(format!(
            "{count} arc length, jogged or jog line dimension{} written as plain lines, arcs and multiline text: they have no DIMENSION to map onto yet.",
            if count == 1 { " was" } else { "s were" }
        ));
    }
    let real = flat
        .objects()
        .iter()
        .filter(|o| matches!(&o.entity, opendraft_core::Entity::Dimension(_)))
        .count();
    if real > 0 {
        encoded.notes.push(
            "Dimensions were written as DIMENSION entities with their pictures as anonymous blocks and their styles in the DIMSTYLE table; they have not been reopened in AutoCAD.".to_owned(),
        );
    }
    if flat
        .objects()
        .iter()
        .any(|o| matches!(o.entity, opendraft_core::Entity::MLeader(_)))
    {
        encoded.notes.push(
            "Multileaders were written as DXF 2007 MULTILEADER objects following the layout of AutoCAD 2018; they have not been reopened in AutoCAD, arrowheads other than closed filled are written as closed filled, and override flags and preview graphics are not written.".to_owned(),
        );
    }
    if let Some(m) = &materialized {
        let (count, noun) = if m.references > 0 {
            (m.references, "dynamic block reference")
        } else {
            (m.definitions, "dynamic block")
        };
        encoded.notes.push(format!(
            "{} {} written as ordinary blocks of the shape they have now: the parameters and actions are not in the DXF file.",
            crate::plural(count as u64, noun),
            if count == 1 { "was" } else { "were" }
        ));
    }
    Ok(encoded)
}

#[cfg(test)]
mod dynamic_tests;
