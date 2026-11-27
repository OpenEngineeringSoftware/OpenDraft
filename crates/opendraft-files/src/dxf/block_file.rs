//! A block file as a DXF, the way AutoCAD's WBLOCK writes one: the block's
//! objects in model space, the blocks they nest as block definitions, the base
//! point in `$INSBASE` and the units in `$INSUNITS`. AutoCAD opens it, shows
//! the symbol, and inserting the file gives one block named after the file.
//!
//! OpenDraft's own file also carries a marker AutoCAD ignores: an `XRECORD`
//! in the named object dictionary (`OPENDRAFT_BLOCK`) holding text the caller
//! chose (the block's name, id, description and tags). Reading our own file
//! back then gives the exact block, with nothing guessed from the file's name.

use opendraft_core::{BlockId, Document, Point};

use super::write::Out;
use super::write2000::{hex, BlockExtras};
use crate::{Encoded, FileError};

/// The key of the marker in the named object dictionary.
pub(super) const MARKER_KEY: &str = "OPENDRAFT_BLOCK";
/// The first text of the marker's record, which tells it from any other.
const MARKER_TAG: &str = "opendraft.block";
/// A text group is kept well under DXF's limit of 2049 characters.
const CHUNK: usize = 1000;

/// What a block file's DXF says beyond its drawing: the base point, and the
/// marker's text when OpenDraft wrote the file.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockHeader {
    pub insbase: Point,
    pub marker: Option<String>,
}

/// Block `root` of `container` as a DXF (see the module notes). `marker` is
/// written into the file and comes back from [`block_header`]; it must be ASCII
/// and one line.
pub fn export_block(
    container: &Document,
    root: BlockId,
    marker: &str,
) -> Result<Encoded, FileError> {
    let def = container
        .block(root)
        .ok_or_else(|| FileError::Invalid("the block is not in its drawing".into()))?;
    let drawing = container
        .block_as_drawing(root)
        .ok_or_else(|| FileError::Invalid("the block is not in its drawing".into()))?;
    if !marker.is_ascii() || marker.contains(['\n', '\r']) {
        return Err(FileError::Invalid(
            "the marker is not one line of ASCII".into(),
        ));
    }
    let extras = BlockExtras {
        insbase: def.base,
        marker,
    };
    super::write_document(&drawing, Some(&extras))
}

/// The marker's record: its tag, then the text in groups of `CHUNK`.
pub(super) fn write_marker(out: &mut Out, handle: u32, owner: u32, text: &str) {
    out.pair(0, "XRECORD");
    out.pair(5, hex(handle));
    out.pair(330, hex(owner));
    out.pair(100, "AcDbXrecord");
    out.pair(280, 1);
    out.pair(1, MARKER_TAG);
    let mut rest = text;
    while !rest.is_empty() {
        let (head, tail) = rest.split_at(rest.len().min(CHUNK));
        out.pair(1, head);
        rest = tail;
    }
}

/// The base point and marker of a DXF's bytes, whatever program wrote it: a
/// missing or unreadable `$INSBASE` is the origin, and a file with no marker
/// has none.
pub fn block_header(bytes: &[u8]) -> BlockHeader {
    super::read::block_header(bytes)
}

/// The marker's text from a record's code-1 groups, if it is ours.
pub(super) fn marker_text<'a>(texts: impl Iterator<Item = &'a str>) -> Option<String> {
    let mut texts = texts;
    if texts.next()? != MARKER_TAG {
        return None;
    }
    Some(texts.collect())
}

#[cfg(test)]
mod tests {
    use opendraft_core::{
        Attributes, BlockDef, BlockEntity, BlockObjectId, Document, Entity, InsertUnits, Point,
    };

    use super::*;
    use crate::FileFormat;

    /// A drawing with block "Valve": a circle and a line, base point (5, 5),
    /// in millimetres.
    fn with_valve() -> (Document, BlockId) {
        let mut doc = Document::default();
        let layer = doc.layers()[0].id;
        let entities = vec![
            BlockEntity::new(
                BlockObjectId(0),
                Entity::Circle {
                    center: Point::new(5.0, 5.0),
                    radius: 2.0,
                },
                Attributes::on_layer(layer),
            ),
            BlockEntity::new(
                BlockObjectId(1),
                Entity::Line {
                    start: Point::new(3.0, 5.0),
                    end: Point::new(7.0, 5.0),
                },
                Attributes::on_layer(layer),
            ),
        ];
        let id = doc.allocate_block_id();
        doc.push_block(std::sync::Arc::new(BlockDef {
            unit: mm(),
            id,
            name: "Valve".into(),
            base: Point::new(5.0, 5.0),
            description: String::new(),
            entities,
            dynamic: None,
        }));
        (doc, id)
    }

    fn mm() -> InsertUnits {
        InsertUnits::from_code(4).expect("millimetres")
    }

    fn text(encoded: &Encoded) -> String {
        String::from_utf8_lossy(&encoded.bytes).into_owned()
    }

    #[test]
    fn the_objects_are_in_model_space_and_the_block_is_not_a_definition() {
        let (doc, id) = with_valve();
        let encoded = export_block(&doc, id, "{}").expect("export");
        let read = crate::DxfFormat.read(&encoded.bytes).expect("read back");
        assert_eq!(
            read.document.objects().len(),
            2,
            "the symbol is in model space"
        );
        assert!(
            read.document.block_named("Valve").is_none(),
            "inserting the file must not nest the block in itself"
        );
        assert_eq!(read.document.units().insert, mm());
        let circle = read.document.objects()[0].entity.bounds();
        assert_eq!(
            (circle.min, circle.max),
            (Point::new(3.0, 3.0), Point::new(7.0, 7.0))
        );
    }

    #[test]
    fn the_base_point_is_in_insbase_and_the_marker_comes_back() {
        let (doc, id) = with_valve();
        let encoded = export_block(&doc, id, "{\"name\":\"Valve\"}").expect("export");
        let header = block_header(&encoded.bytes);
        assert_eq!(header.insbase, Point::new(5.0, 5.0));
        assert_eq!(header.marker.as_deref(), Some("{\"name\":\"Valve\"}"));
        assert!(text(&encoded).contains("OPENDRAFT_BLOCK"));
    }

    #[test]
    fn a_long_marker_is_split_and_joined_again() {
        let (doc, id) = with_valve();
        let marker = "x".repeat(CHUNK * 3 + 17);
        let encoded = export_block(&doc, id, &marker).expect("export");
        assert_eq!(block_header(&encoded.bytes).marker, Some(marker));
    }

    #[test]
    fn a_marker_that_is_not_one_line_of_ascii_is_refused() {
        let (doc, id) = with_valve();
        assert!(export_block(&doc, id, "two\nlines").is_err());
        assert!(export_block(&doc, id, "caf\u{e9}").is_err());
    }

    #[test]
    fn a_drawing_without_the_marker_has_none_and_an_unknown_block_is_refused() {
        let (doc, id) = with_valve();
        let plain = crate::DxfFormat.write(&doc).expect("a drawing");
        let header = block_header(&plain.bytes);
        assert_eq!(header.marker, None);
        assert_eq!(header.insbase, Point::new(0.0, 0.0));
        assert!(export_block(&doc, BlockId(id.0 + 9), "{}").is_err());
        assert_eq!(block_header(b"not a dxf").marker, None);
    }
}
