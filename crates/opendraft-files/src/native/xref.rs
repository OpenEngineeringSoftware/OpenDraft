//! The native file's external-reference records.
//!
//! The root block is written in the drawing's ordinary `blocks` list, with its
//! snapshot; this record keeps the reference's identity (path, kind, unloaded
//! state, source stamp, bound layers and report) and the definitions local to
//! the referenced drawing, which are not host blocks.

use std::collections::BTreeMap;
use std::sync::Arc;

use opendraft_core::{
    BlockDef, Document, XrefDef, XrefKind, XrefLayer, XrefPathType, XrefReport, XrefStamp,
};
use serde::{Deserialize, Serialize};

use super::block::{BlockDto, Blocks};
use super::dimension_style::StylePool;
use super::limits;
use crate::FileError;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum PathTypeDto {
    Full,
    #[default]
    Relative,
    #[serde(rename = "none")]
    NoPath,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
enum KindDto {
    #[default]
    Attach,
    Overlay,
}

#[derive(Serialize, Deserialize)]
pub(super) struct XrefDto {
    /// The root block's id in `blocks`.
    block: u64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    path: String,
    #[serde(default)]
    path_type: PathTypeDto,
    #[serde(default)]
    kind: KindDto,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    unloaded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    stamp: Option<StampDto>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "limits::table_rows"
    )]
    layers: Vec<BoundLayerDto>,
    /// Definitions local to the referenced drawing; not host blocks.
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "limits::table_rows"
    )]
    blocks: Vec<BlockDto>,
    /// The `XCLIP` boundary, when the reference has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clip: Option<ClipDto>,
    #[serde(default, skip_serializing_if = "ReportDto::is_empty")]
    report: ReportDto,
}

#[derive(Serialize, Deserialize)]
struct ClipDto {
    #[serde(deserialize_with = "limits::table_rows")]
    points: Vec<[f64; 2]>,
    #[serde(default = "yes")]
    enabled: bool,
}

#[derive(Serialize, Deserialize)]
struct BoundLayerDto {
    source: String,
    layer: u64,
}

#[derive(Serialize, Deserialize)]
struct StampDto {
    length: u64,
    #[serde(default)]
    modified: i64,
}

#[derive(Default, Serialize, Deserialize)]
struct ReportDto {
    #[serde(default)]
    model_entities: u64,
    #[serde(default)]
    local_blocks: u64,
    #[serde(
        default,
        skip_serializing_if = "BTreeMap::is_empty",
        deserialize_with = "limits::map_rows"
    )]
    skipped: BTreeMap<String, u64>,
    #[serde(
        default,
        skip_serializing_if = "Vec::is_empty",
        deserialize_with = "limits::table_rows"
    )]
    notes: Vec<String>,
}

impl ReportDto {
    fn is_empty(&self) -> bool {
        self.model_entities == 0
            && self.local_blocks == 0
            && self.skipped.is_empty()
            && self.notes.is_empty()
    }
}

impl PathTypeDto {
    fn of(kind: XrefPathType) -> Self {
        match kind {
            XrefPathType::Full => Self::Full,
            XrefPathType::Relative => Self::Relative,
            XrefPathType::NoPath => Self::NoPath,
        }
    }

    fn into_core(self) -> XrefPathType {
        match self {
            Self::Full => XrefPathType::Full,
            Self::Relative => XrefPathType::Relative,
            Self::NoPath => XrefPathType::NoPath,
        }
    }
}

impl KindDto {
    fn of(kind: XrefKind) -> Self {
        match kind {
            XrefKind::Attach => Self::Attach,
            XrefKind::Overlay => Self::Overlay,
        }
    }

    fn into_core(self) -> XrefKind {
        match self {
            Self::Attach => XrefKind::Attach,
            Self::Overlay => XrefKind::Overlay,
        }
    }
}

impl XrefDto {
    /// The file's record for `xref`. `known` holds every block that could be
    /// named by an insert, so the read-back check passes for local blocks.
    pub(super) fn of(
        xref: &XrefDef,
        known: &Blocks,
        styles: &mut StylePool,
    ) -> Result<Self, FileError> {
        let blocks = xref
            .blocks
            .iter()
            .map(|b| BlockDto::from_block_in(b, styles, known))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            block: xref.block.0,
            path: xref.path.clone(),
            path_type: PathTypeDto::of(xref.path_type),
            kind: KindDto::of(xref.kind),
            unloaded: xref.unloaded,
            stamp: xref.stamp.map(|s| StampDto {
                length: s.length,
                modified: s.modified,
            }),
            layers: xref
                .layers
                .iter()
                .map(|l| BoundLayerDto {
                    source: l.source.clone(),
                    layer: l.layer.0,
                })
                .collect(),
            blocks,
            clip: xref.clip.as_ref().map(|clip| ClipDto {
                points: clip.boundary.iter().map(|p| [p.x, p.y]).collect(),
                enabled: clip.enabled,
            }),
            report: ReportDto {
                model_entities: xref.report.model_entities,
                local_blocks: xref.report.local_blocks,
                skipped: xref.report.skipped.clone(),
                notes: xref.report.notes.clone(),
            },
        })
    }

    /// This record as the drawing's, with its already-read local definitions.
    pub(super) fn into_record(
        self,
        roots: &Blocks,
        locals: Vec<Arc<BlockDef>>,
    ) -> Result<XrefDef, FileError> {
        if !roots.contains_key(&self.block) {
            return Err(FileError::Invalid(format!(
                "an external reference points at block {}, which is not in the drawing",
                self.block
            )));
        }
        Ok(XrefDef {
            block: opendraft_core::BlockId(self.block),
            path: self.path,
            path_type: self.path_type.into_core(),
            kind: self.kind.into_core(),
            unloaded: self.unloaded,
            stamp: self.stamp.map(|s| XrefStamp {
                length: s.length,
                modified: s.modified,
            }),
            layers: self
                .layers
                .into_iter()
                .map(|l| XrefLayer {
                    source: l.source,
                    layer: opendraft_core::LayerId(l.layer),
                })
                .collect(),
            blocks: locals,
            clip: self.clip.map(|clip| opendraft_core::XrefClip {
                boundary: clip
                    .points
                    .into_iter()
                    .map(|[x, y]| opendraft_core::Point::new(x, y))
                    .collect(),
                enabled: clip.enabled,
            }),
            report: XrefReport {
                model_entities: self.report.model_entities,
                local_blocks: self.report.local_blocks,
                skipped: self.report.skipped,
                notes: self.report.notes,
            },
        })
    }

    /// The local definitions this record carries, to be read with
    /// [`load_blocks_with`](super::block::load_blocks_with).
    pub(super) fn local_blocks(&self) -> &[BlockDto] {
        &self.blocks
    }
}

/// Whether a point is the origin, for leaving `INSBASE` out.
pub(super) fn is_origin(p: &[f64; 2]) -> bool {
    p[0] == 0.0 && p[1] == 0.0
}

/// Whether `VISRETAIN` is true, for leaving it out.
pub(super) fn is_true(value: &bool) -> bool {
    *value
}

/// `VISRETAIN`'s value when a file does not say.
pub(super) fn yes() -> bool {
    true
}

/// `XCLIPFRAME`'s initial value, for leaving it out.
pub(super) fn two() -> u8 {
    2
}

/// Whether `XCLIPFRAME` is the initial value, for leaving it out.
pub(super) fn is_two(value: &u8) -> bool {
    *value == 2
}

/// Every block an insert may name while a drawing is written: the host table
/// and every reference's local definitions.
pub(super) fn known_blocks(document: &Document) -> Blocks {
    let mut known: Blocks = document
        .blocks()
        .iter()
        .map(|b| (b.id.0, b.clone()))
        .collect();
    for xref in document.xrefs() {
        for block in &xref.blocks {
            known.insert(block.id.0, block.clone());
        }
    }
    known
}
