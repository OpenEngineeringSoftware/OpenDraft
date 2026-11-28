//! Types for Bind and XBIND: naming dependent symbols, and the plan that
//! makes them local.

use std::sync::Arc;

use crate::block::BlockDef;
use crate::{BlockId, Layer, LayerId, LinetypeTable, TextStyle, XrefDef};

/// How Bind names the xref-dependent symbols it makes local (`BINDTYPE`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BindType {
    /// `xref$0$name`: every symbol keeps a name of its own.
    #[default]
    Bind,
    /// The bare name when it is free; an existing layer is merged into.
    Insert,
}

impl BindType {
    pub fn from_code(code: i64) -> Self {
        if code == 1 {
            Self::Insert
        } else {
            Self::Bind
        }
    }

    pub fn code(self) -> i64 {
        match self {
            Self::Bind => 0,
            Self::Insert => 1,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Bind => key!("Bind"),
            Self::Insert => key!("Insert"),
        }
    }
}

/// One dependent symbol XBIND makes local.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum XbindSymbol {
    /// A host layer bound to the reference's source layer.
    Layer { xref: BlockId, layer: LayerId },
    /// A block definition local to the reference (and the locals it holds).
    Block { xref: BlockId, block: BlockId },
    /// A text style the reference added as `<xref>|<style>`.
    TextStyle {
        xref: BlockId,
        style: crate::TextStyleId,
    },
    /// A linetype the reference added as `<xref>|<linetype>`.
    Linetype {
        xref: BlockId,
        linetype: crate::LinetypeId,
    },
}

/// Why a reference cannot be bound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BindError {
    /// The block is not an external reference (or is gone).
    NotAnXref(BlockId),
    /// A symbol XBIND was asked for does not belong to the reference.
    NotDependent { xref: BlockId, what: String },
    /// The snapshot holds something a host block cannot: a hatch, a
    /// construction line or a ray.
    UnsupportedContent {
        reference: String,
        what: &'static str,
    },
}

impl std::fmt::Display for BindError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAnXref(block) => {
                write!(f, "block {} is not an external reference", block.0)
            }
            Self::NotDependent { xref, what } => {
                write!(f, "\"{what}\" is not dependent on block {}", xref.0)
            }
            Self::UnsupportedContent { reference, what } => write!(
                f,
                "external reference \"{reference}\" holds a {what}, which a host block cannot; \
                 bind is not available. Keep the source drawing."
            ),
        }
    }
}

impl std::error::Error for BindError {}

/// What `plan_bind`/`plan_xbind` computed. The caller applies it as one undo
/// step; nothing here has changed the drawing.
#[derive(Clone, Debug, PartialEq)]
pub struct BindPlan {
    /// The whole block table after binding.
    pub blocks: Vec<Arc<BlockDef>>,
    /// Layer records that changed (renamed, or a merged linetype).
    pub layers: Vec<Layer>,
    /// Bound layers merged into existing ones, to remove.
    pub removed_layers: Vec<LayerId>,
    /// The whole text-style table, when a style was renamed.
    pub text_styles: Option<Vec<TextStyle>>,
    pub current_text_style: crate::TextStyleId,
    pub next_text_style: u64,
    /// The whole linetype table, when a linetype was renamed.
    pub linetypes: Option<LinetypeTable>,
    /// The whole layout table, when frozen-layer lists were remapped.
    pub layouts: Option<Vec<crate::Layout>>,
    /// Top-level objects whose geometry changed.
    pub entities: Vec<(crate::EntityId, crate::Entity)>,
    /// Top-level objects whose display attributes changed.
    pub attributes: Vec<(crate::EntityId, crate::Attributes)>,
    /// The external-reference table after binding.
    pub xrefs: Vec<XrefDef>,
    /// What was renamed or merged, for the command line.
    pub notes: Vec<String>,
}
