//! Block-table records the converter uses: named blocks and external
//! references, and collecting the latter from the decoded block table.

use super::*;

/// A named block: a definition and the entities it owns.
#[derive(Clone, Debug, PartialEq)]
pub struct DwgBlock {
    pub handle: DwgHandle,
    pub name: String,
    /// The point of the block's coordinates an insert puts at its insertion
    /// point.
    pub base: [f64; 2],
    pub description: String,
    /// In the block's own coordinates, in drawing order.
    pub entities: Vec<DwgEntity>,
}

/// An external reference's block record: its saved path and kind.
#[derive(Clone, Debug, PartialEq)]
pub struct DwgXref {
    pub handle: DwgHandle,
    pub name: String,
    pub base: [f64; 2],
    pub path: String,
    /// True for an overlay, false for an attachment.
    pub overlaid: bool,
}

/// Every xref block record, in the block table's order.
pub(super) fn collect(pass: &Pass, report: &mut ImportReport) -> Vec<DwgXref> {
    let mut out = Vec::new();
    for (&handle, block) in &pass.blocks {
        if !(block.is_xref || block.overlaid) {
            continue;
        }
        let base = [block.base[0], block.base[1]];
        if base
            .iter()
            .all(|v| v.is_finite() && v.abs() <= MAX_ABS_COORDINATE)
        {
            out.push(DwgXref {
                handle,
                name: block.name.clone(),
                base,
                path: block.path.clone(),
                overlaid: block.overlaid,
            });
        } else {
            report.skip(Skipped::InvalidGeometry);
        }
    }
    out
}
