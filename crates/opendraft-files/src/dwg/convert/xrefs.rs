//! External references in a DWG: their empty roots, handles and records.

use std::collections::HashSet;

use opendraft_core::{Document, XrefDef, XrefKind, XrefLayer, XrefReport};
use opendraft_dwg::{Drawing, DwgHandle};

use super::point;
use crate::block_import::{ImportedBlocks, PendingBlock};

/// The block handles an `INSERT` must mark as a reference.
pub(super) fn handles(drawing: &Drawing) -> HashSet<DwgHandle> {
    drawing.xrefs.iter().map(|xref| xref.handle).collect()
}

/// An empty root for each reference, so its `INSERT`s resolve.
pub(super) fn pending(drawing: &Drawing) -> Vec<PendingBlock<DwgHandle>> {
    drawing
        .xrefs
        .iter()
        .map(|xref| PendingBlock {
            unit: Default::default(),
            key: xref.handle,
            name: xref.name.clone(),
            base: point(xref.base),
            description: String::new(),
            items: Vec::new(),
            dynamic: None,
        })
        .collect()
}

/// The records, with layers bound by the `<xref>|<layer>` name prefix (DWG
/// does not expose the xref-dependent flag in the files this reads).
pub(super) fn records(
    drawing: &Drawing,
    document: &Document,
    blocks: &ImportedBlocks<DwgHandle>,
) -> Vec<XrefDef> {
    let mut out = Vec::new();
    for xref in &drawing.xrefs {
        let Some(root) = blocks.block(&xref.handle) else {
            continue;
        };
        let prefix = format!("{}|", root.name);
        let mut layers: Vec<XrefLayer> = document
            .layers()
            .iter()
            .filter_map(|layer| {
                let source = layer.name.strip_prefix(&prefix)?;
                (!source.is_empty()).then(|| XrefLayer {
                    source: source.to_owned(),
                    layer: layer.id,
                })
            })
            .collect();
        layers.sort_by_key(|l| l.source.to_lowercase());
        out.push(XrefDef {
            block: root.id,
            path: xref.path.clone(),
            path_type: crate::xref::classify(&xref.path),
            kind: if xref.overlaid {
                XrefKind::Overlay
            } else {
                XrefKind::Attach
            },
            unloaded: true,
            stamp: None,
            layers,
            blocks: Vec::new(),
            report: XrefReport::default(),
        });
    }
    out
}
