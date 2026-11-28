//! Clip boundaries (`XCLIP`) in the OBJECTS section: the handles a clipped
//! reference's objects use, and the objects themselves.
//!
//! The boundary is written in the block's own coordinates, as ezdxf does for
//! `set_block_clipping_path`, so both transformation matrices are the
//! identity. The insert's extension dictionary names an `ACAD_FILTER`
//! dictionary, whose `SPATIAL` entry is the `SPATIAL_FILTER`.

use opendraft_core::{Document, Entity, EntityId, XrefClip};

use super::{hex, Handles, Ids};
use crate::dxf::write::Out;

/// The handles a clipped reference's object graph uses.
pub(super) struct ClipHandles {
    pub insert: u32,
    pub dictionary: u32,
    pub filter_dictionary: u32,
    pub filter: u32,
}

impl Ids {
    pub(super) fn clip_handles(&self, id: EntityId) -> Option<&ClipHandles> {
        self.clips
            .iter()
            .find(|(object, _)| *object == id)
            .map(|(_, handles)| handles)
    }
}

/// Reserve the handles of every clipped reference, in object order.
pub(super) fn handles(document: &Document, h: &mut Handles) -> Vec<(EntityId, ClipHandles)> {
    // Xrefs live in model space, so this runs before any object is written and
    // the handles below stay in one block.
    document
        .objects()
        .iter()
        .filter_map(|object| {
            let Entity::Insert(insert) = &object.entity else {
                return None;
            };
            document
                .xref(insert.block.id)
                .and_then(|xref| xref.clip.as_ref())?;
            Some((
                object.id,
                ClipHandles {
                    insert: h.next(),
                    dictionary: h.next(),
                    filter_dictionary: h.next(),
                    filter: h.next(),
                },
            ))
        })
        .collect()
}

/// The extension dictionary, `ACAD_FILTER` dictionary and `SPATIAL_FILTER` of
/// one clipped reference.
pub(super) fn objects(out: &mut Out, handles: &ClipHandles, clip: &XrefClip) {
    let dictionary = |out: &mut Out, handle: u32, owner: u32, name: &str, target: u32| {
        out.pair(0, "DICTIONARY");
        out.pair(5, hex(handle));
        out.pair(330, hex(owner));
        out.pair(100, "AcDbDictionary");
        out.pair(281, 1);
        out.pair(3, name);
        out.pair(350, hex(target));
    };
    dictionary(
        out,
        handles.dictionary,
        handles.insert,
        "ACAD_FILTER",
        handles.filter_dictionary,
    );
    dictionary(
        out,
        handles.filter_dictionary,
        handles.dictionary,
        "SPATIAL",
        handles.filter,
    );
    out.pair(0, "SPATIAL_FILTER");
    out.pair(5, hex(handles.filter));
    out.pair(102, "{ACAD_REACTORS");
    out.pair(330, hex(handles.filter_dictionary));
    out.pair(102, "}");
    out.pair(330, hex(handles.filter_dictionary));
    out.pair(100, "AcDbFilter");
    out.pair(100, "AcDbSpatialFilter");
    out.pair(70, clip.boundary.len() as i32);
    for point in &clip.boundary {
        out.pair(10, point.x);
        out.pair(20, point.y);
    }
    out.pair(210, 0.0);
    out.pair(220, 0.0);
    out.pair(230, 1.0);
    out.pair(11, 0.0);
    out.pair(21, 0.0);
    out.pair(31, 0.0);
    out.pair(71, i32::from(clip.enabled));
    out.pair(72, 0);
    out.pair(73, 0);
    // The inverse insert and clip transforms: identity for block coordinates.
    for _ in 0..2 {
        for value in [1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
            out.pair(40, value);
        }
    }
}
