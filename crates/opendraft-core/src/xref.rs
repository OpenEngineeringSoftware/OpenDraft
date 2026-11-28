//! External references (XREF): a block table record whose definition comes
//! from another drawing, and the metadata that is not geometry.
//!
//! The displayed snapshot is an ordinary root [`BlockDef`] in the block
//! table, placed by an [`Insert`](crate::Insert) marked `is_xref`. The
//! [`XrefDef`] here keeps the reference's identity: the saved path, the
//! attach/overlay kind, the unloaded flag, the source stamp and the host
//! layers bound to the source's layers. Definitions local to the referenced
//! drawing live on the record, not in the host block table.
//!
//! These types are semantic only: no file, path or interface type enters
//! them, so commands and the drawing model can hold them without a loader.

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::block::block_name_problem;
use crate::xref_clip::XrefClip;
use crate::{BlockDef, BlockId, Document, Layer, LayerId, LinetypeTable, Point, TextStyle};

/// Longest saved path stored, in characters (DXF group 1's practical limit).
pub const MAX_XREF_PATH: usize = 32_767;

/// Most report notes kept from one load; a hostile source cannot grow the
/// palette without bound.
pub const MAX_XREF_NOTES: usize = 64;

/// Most distinct skipped kinds kept in one report.
pub const MAX_XREF_SKIPPED_KINDS: usize = 256;

/// How the referenced drawing is brought in. AutoCAD calls these attachments
/// and overlays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XrefKind {
    Attach,
    Overlay,
}

impl XrefKind {
    pub fn is_overlay(self) -> bool {
        matches!(self, Self::Overlay)
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Attach => key!("Attach"),
            Self::Overlay => key!("Overlay"),
        }
    }
}

/// The saved path's form. The string itself is stored verbatim.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum XrefPathType {
    Full,
    #[default]
    Relative,
    NoPath,
}

impl XrefPathType {
    pub fn label(self) -> &'static str {
        match self {
            Self::Full => key!("Full"),
            Self::Relative => key!("Relative"),
            Self::NoPath => key!("NoPath"),
        }
    }
}

/// One source layer and the host layer bound to it. Layer 0 is never bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct XrefLayer {
    /// The layer's name in the referenced drawing.
    pub source: String,
    /// The xref-dependent host layer `<xref name>|<source>`.
    pub layer: LayerId,
}

/// Source file metadata, for the palette's "changed on disk" state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct XrefStamp {
    pub length: u64,
    /// Unix seconds; 0 when the platform did not say.
    pub modified: i64,
}

/// What the last load did. Informational; never used to decide validity.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct XrefReport {
    pub model_entities: u64,
    pub local_blocks: u64,
    pub skipped: BTreeMap<String, u64>,
    pub notes: Vec<String>,
}

impl XrefReport {
    /// Keep the report within the bounds the palette can show.
    pub fn capped(mut self) -> Self {
        if self.skipped.len() > MAX_XREF_SKIPPED_KINDS {
            let kept: BTreeMap<String, u64> = self
                .skipped
                .into_iter()
                .take(MAX_XREF_SKIPPED_KINDS)
                .collect();
            self.skipped = kept;
        }
        if self.notes.len() > MAX_XREF_NOTES {
            self.notes.truncate(MAX_XREF_NOTES);
        }
        self
    }

    /// The skipped count for `kind`, raised by `n`.
    pub fn skip(&mut self, kind: &str, n: u64) {
        if self.skipped.len() < MAX_XREF_SKIPPED_KINDS || self.skipped.contains_key(kind) {
            *self.skipped.entry(kind.to_owned()).or_insert(0) += n;
        }
    }
}

/// One external reference: the block table record's identity and the
/// external state that is not geometry.
#[derive(Clone, Debug, PartialEq)]
pub struct XrefDef {
    /// The root block in `Document.blocks`.
    pub block: BlockId,
    /// The saved path exactly as DXF group 1 spells it (possibly empty).
    pub path: String,
    pub path_type: XrefPathType,
    pub kind: XrefKind,
    /// The user unloaded it: the root block is empty and nothing draws.
    pub unloaded: bool,
    /// The source file when the snapshot was made; `None` if never loaded.
    pub stamp: Option<XrefStamp>,
    /// Host layers bound to source layers, excluding source layer 0.
    pub layers: Vec<XrefLayer>,
    /// Definitions local to the referenced drawing, not in `Document.blocks`.
    /// Root entities and these definitions refer to them by `Arc` and unique
    /// `BlockId`.
    pub blocks: Vec<Arc<BlockDef>>,
    /// The `XCLIP` boundary, in the root block's coordinates.
    pub clip: Option<XrefClip>,
    pub report: XrefReport,
}

impl XrefDef {
    /// Nothing was ever read for this record: it came from a DXF or DWG
    /// file, which keep the reference and not its snapshot. A reference the
    /// user unloaded keeps the stamp of the file it last read.
    pub fn never_loaded(&self) -> bool {
        self.unloaded && self.stamp.is_none()
    }

    pub fn owns_block(&self, id: BlockId) -> bool {
        self.blocks.iter().any(|b| b.id == id)
    }

    pub fn owns_layer(&self, layer: LayerId) -> bool {
        self.layers.iter().any(|l| l.layer == layer)
    }

    /// The host layer bound to source layer `source`, ignoring case.
    pub fn layer_for(&self, source: &str) -> Option<LayerId> {
        self.layers
            .iter()
            .find(|l| l.source.eq_ignore_ascii_case(source))
            .map(|l| l.layer)
    }
}

/// A planned xref load. Built by `opendraft-files` from a source `Document`,
/// applied by a command as actions. It owns no file or interface type.
#[derive(Clone, Debug, PartialEq)]
pub struct XrefPlan {
    pub xref: XrefDef,
    /// The new root block; the same id as the existing xref on reload.
    pub root: Arc<BlockDef>,
    /// Layers the host lacks.
    pub added_layers: Vec<Layer>,
    /// Existing bound layers to reset when `VISRETAIN` is 0.
    pub changed_layers: Vec<Layer>,
    /// Full replacement text-style table, when the source needed styles.
    pub text_styles: Option<TextStylePlan>,
    /// Full replacement linetype table, when the source needed linetypes.
    pub linetypes: Option<LinetypeTable>,
    /// Source unit / host unit, for the Attach dialog's factor.
    pub unit_factor: f64,
}

/// What a load adds to the text-style table, which is replaced as a whole.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStylePlan {
    pub styles: Vec<TextStyle>,
    pub current: crate::TextStyleId,
    pub next_id: u64,
}

/// Why an xref operation cannot be accepted.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum XrefProblem {
    EmptyName,
    BadName(String),
    NameTaken(String),
    EmptyPath,
    PathHasNul,
    PathTooLong,
    UnsupportedPath(String),
    NotAnXref(BlockId),
    MissingRoot(BlockId),
    LayerZeroBound,
    DuplicateSourceLayer(String),
    LayerBoundElsewhere(String),
    LayerMissing(LayerId),
    CurrentLayerBound(LayerId),
    LocalBlockShared(BlockId),
    LocalBlockMissing(BlockId),
    Cycle(String),
    SelfReference,
}

impl std::fmt::Display for XrefProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::EmptyName => write!(f, "an external reference needs a name"),
            Self::BadName(name) => write!(f, "\"{name}\" cannot be an external reference name"),
            Self::NameTaken(name) => write!(
                f,
                "a block or external reference called \"{name}\" already exists"
            ),
            Self::EmptyPath => write!(f, "an external reference needs a path"),
            Self::PathHasNul => write!(f, "the path contains a control character"),
            Self::PathTooLong => write!(f, "the path is longer than {MAX_XREF_PATH} characters"),
            Self::UnsupportedPath(path) => {
                write!(f, "the path \"{path}\" is not a local file path")
            }
            Self::NotAnXref(block) => write!(f, "block {} is not an external reference", block.0),
            Self::MissingRoot(block) => {
                write!(f, "the root block {} is not in the drawing", block.0)
            }
            Self::LayerZeroBound => write!(f, "layer 0 cannot be bound to an external reference"),
            Self::DuplicateSourceLayer(name) => {
                write!(f, "source layer \"{name}\" is bound twice")
            }
            Self::LayerBoundElsewhere(name) => {
                write!(f, "layer \"{name}\" belongs to another reference")
            }
            Self::LayerMissing(layer) => write!(f, "layer {} does not exist", layer.0),
            Self::CurrentLayerBound(layer) => write!(
                f,
                "layer {} is an external reference layer and cannot be current",
                layer.0
            ),
            Self::LocalBlockShared(block) => {
                write!(f, "block {} is owned by two external references", block.0)
            }
            Self::LocalBlockMissing(block) => {
                write!(f, "block {} is not part of the reference", block.0)
            }
            Self::Cycle(name) => write!(f, "block \"{name}\" contains itself"),
            Self::SelfReference => {
                write!(f, "a drawing cannot reference itself")
            }
        }
    }
}

impl std::error::Error for XrefProblem {}

/// What the app is asked to turn into a [`XrefPlan`].
#[derive(Clone, Debug, PartialEq)]
pub struct XrefLoadRequest {
    /// The existing root on reload; `None` for a new attach.
    pub block: Option<BlockId>,
    pub name: String,
    pub source_path: String,
    pub path_type: XrefPathType,
    pub kind: XrefKind,
    /// The host drawing's folder, for relative paths.
    pub host_dir: Option<String>,
    /// The host drawing's own file, to refuse attaching itself. The app
    /// fills it in; a command leaves it `None`.
    pub host_path: Option<String>,
}

/// The Attach External Reference dialog's fields.
#[derive(Clone, Debug, PartialEq)]
pub struct XrefAttachDraft {
    pub source_path: String,
    /// Defaults to the file's stem; editable.
    pub name: String,
    pub kind: XrefKind,
    pub path_type: XrefPathType,
    /// `None` asks on screen.
    pub insertion: Option<Point>,
    /// `None` asks on screen.
    pub scale: Option<(f64, f64)>,
    /// `None` asks on screen.
    pub rotation: Option<f64>,
}

impl XrefAttachDraft {
    /// Why the draft cannot be attached, or `None` when it can. Drawing
    /// checks look at the name only; the source is read separately.
    pub fn problem(&self, doc: &Document) -> Option<String> {
        let name = self.name.trim();
        if let Some(why) = block_name_problem(name) {
            return Some(why.to_owned());
        }
        if doc.block_named(name).is_some() || doc.xref_by_name(name).is_some() {
            return Some(XrefProblem::NameTaken(name.to_owned()).to_string());
        }
        if let Some(why) = path_problem(&self.source_path) {
            return Some(why.to_string());
        }
        if let Some(scale) = self.scale {
            if !scale.0.is_finite() || !scale.1.is_finite() || scale.0 == 0.0 || scale.1 == 0.0 {
                return Some(key!("The scale factor cannot be zero.").to_owned());
            }
        }
        if self
            .insertion
            .is_some_and(|p| !p.x.is_finite() || !p.y.is_finite())
        {
            return Some(key!("The insertion point is not a finite point.").to_owned());
        }
        if self.rotation.is_some_and(|a| !a.is_finite()) {
            return Some(key!("The rotation angle is not a finite number.").to_owned());
        }
        None
    }
}

/// Why `path` cannot be a saved xref path, or `None` when it can.
pub fn path_problem(path: &str) -> Option<XrefProblem> {
    if path.trim().is_empty() {
        return Some(XrefProblem::EmptyPath);
    }
    if path.chars().count() > MAX_XREF_PATH {
        return Some(XrefProblem::PathTooLong);
    }
    if path.chars().any(|c| c == '\0' || c.is_control()) {
        return Some(XrefProblem::PathHasNul);
    }
    // A scheme such as `https://` or `file://` is not a local path.
    if let Some((scheme, _)) = path.split_once("://") {
        let valid = !scheme.is_empty()
            && scheme
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
        if valid {
            return Some(XrefProblem::UnsupportedPath(path.to_owned()));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Point;

    fn draft() -> XrefAttachDraft {
        XrefAttachDraft {
            source_path: "base.dwg".to_owned(),
            name: "base".to_owned(),
            kind: XrefKind::Attach,
            path_type: XrefPathType::Relative,
            insertion: None,
            scale: None,
            rotation: None,
        }
    }

    #[test]
    fn a_plain_draft_has_no_problem() {
        assert_eq!(draft().problem(&Document::default()), None);
    }

    #[test]
    fn names_must_be_usable_and_free() {
        let mut bad = draft();
        bad.name = "  ".to_owned();
        assert!(bad.problem(&Document::default()).is_some());
        bad.name = "a|b".to_owned();
        assert!(bad.problem(&Document::default()).is_some());

        let mut doc = Document::default();
        let block = Arc::new(crate::BlockDef {
            id: crate::BlockId(1),
            name: "base".to_owned(),
            base: Point::default(),
            description: String::new(),
            unit: Default::default(),
            entities: Vec::new(),
            dynamic: None,
        });
        doc.push_block(block);
        assert!(draft().problem(&doc).is_some());
    }

    #[test]
    fn paths_reject_empty_control_long_and_urls() {
        assert_eq!(path_problem(""), Some(XrefProblem::EmptyPath));
        assert_eq!(path_problem("a\0b"), Some(XrefProblem::PathHasNul));
        assert_eq!(path_problem("a\u{1}b"), Some(XrefProblem::PathHasNul));
        assert_eq!(
            path_problem(&"a".repeat(MAX_XREF_PATH + 1)),
            Some(XrefProblem::PathTooLong)
        );
        assert!(matches!(
            path_problem("https://example.com/base.dwg"),
            Some(XrefProblem::UnsupportedPath(_))
        ));
        assert_eq!(path_problem("C:\\work\\base.dwg"), None);
        assert_eq!(path_problem("..\\base.dwg"), None);
    }

    #[test]
    fn scale_and_points_must_be_finite_and_nonzero() {
        let mut bad = draft();
        bad.scale = Some((0.0, 1.0));
        assert!(bad.problem(&Document::default()).is_some());
        bad.scale = Some((f64::NAN, 1.0));
        assert!(bad.problem(&Document::default()).is_some());
        bad.scale = Some((2.0, 2.0));
        assert_eq!(bad.problem(&Document::default()), None);
        bad.insertion = Some(Point::new(f64::INFINITY, 0.0));
        assert!(bad.problem(&Document::default()).is_some());
    }

    #[test]
    fn reports_stay_capped() {
        let mut report = XrefReport::default();
        report.skip("hatch", 2);
        report.skip("hatch", 3);
        assert_eq!(report.skipped.get("hatch"), Some(&5));
        for i in 0..(MAX_XREF_NOTES + 10) {
            report.notes.push(i.to_string());
        }
        assert_eq!(report.capped().notes.len(), MAX_XREF_NOTES);
    }
}
