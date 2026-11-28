//! Anonymous blocks a drawing places. AutoCAD stores a modified dynamic block
//! as an anonymous block (`*U`), which a reference then places like any other;
//! the pictures of dimensions and hatches are anonymous too but nothing places
//! them. Only the placed ones are imported, as ordinary blocks.

use std::collections::BTreeSet;

use crate::handles::DwgHandle;
use crate::pass::{Pass, Stored};
use crate::tables::BlockData;

/// One of the spaces' own blocks (`*Model_Space`, `*Paper_Space`, ...).
pub(crate) fn is_space(block: &BlockData) -> bool {
    let name = block.name.to_ascii_lowercase();
    name.starts_with("*model_space") || name.starts_with("*paper_space")
}

/// A block a reference can place and the importer can build: not an external
/// reference and not a space.
pub(crate) fn is_importable(block: &BlockData) -> bool {
    !block.is_xref && !is_space(block)
}

/// An importable block the user did not name.
pub(crate) fn is_anonymous(block: &BlockData) -> bool {
    is_importable(block) && (block.anonymous || block.name.starts_with('*'))
}

/// The anonymous blocks placed by the entities `roots` name, directly or
/// through other anonymous blocks.
pub(crate) fn placed<'a>(
    pass: &Pass,
    roots: impl IntoIterator<Item = &'a Vec<DwgHandle>>,
) -> BTreeSet<DwgHandle> {
    let mut found = BTreeSet::new();
    let mut work: Vec<&Vec<DwgHandle>> = roots.into_iter().collect();
    while let Some(handles) = work.pop() {
        for handle in handles {
            let Some(Stored::Insert(d)) = pass.records.get(handle).map(|r| &r.stored) else {
                continue;
            };
            let Some((target, block)) = d.block.and_then(|h| Some((h, pass.blocks.get(&h)?)))
            else {
                continue;
            };
            if is_anonymous(block) && found.insert(target) {
                work.push(&block.entities);
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entities::InsertData;
    use crate::pass::{Look, Record};

    fn block(name: &str, anonymous: bool, entities: Vec<DwgHandle>) -> BlockData {
        BlockData {
            name: name.into(),
            anonymous,
            is_xref: false,
            overlaid: false,
            path: String::new(),
            base: [0.0; 3],
            description: String::new(),
            entities,
            layout: None,
        }
    }

    fn insert_of(block: u64) -> Record {
        Record {
            look: Look {
                layer: None,
                color_index: 256,
                true_color: None,
                linetype_flags: 0,
                linetype: None,
                linetype_scale: 1.0,
                lineweight: 0,
                invisible: false,
            },
            stored: Stored::Insert(InsertData {
                insertion: [0.0; 3],
                scale: [1.0; 3],
                rotation: 0.0,
                extrusion: [0.0, 0.0, 1.0],
                block: Some(DwgHandle(block)),
                attributes: 0,
                attribute_handles: Vec::new(),
            }),
        }
    }

    #[test]
    fn only_placed_anonymous_blocks_are_found_and_chains_are_followed() {
        let mut pass = Pass::default();
        // Entity 1 places *U1; *U1 holds entity 2, which places *U2.
        // *D1 (a dimension's picture) is placed by nothing.
        pass.blocks
            .insert(DwgHandle(0x10), block("*U1", true, vec![DwgHandle(2)]));
        pass.blocks
            .insert(DwgHandle(0x11), block("*U2", true, vec![]));
        pass.blocks
            .insert(DwgHandle(0x12), block("*D1", true, vec![]));
        pass.blocks
            .insert(DwgHandle(0x13), block("Named", false, vec![]));
        pass.records.insert(DwgHandle(1), insert_of(0x10));
        pass.records.insert(DwgHandle(2), insert_of(0x11));
        pass.records.insert(DwgHandle(3), insert_of(0x13));
        let roots = vec![DwgHandle(1), DwgHandle(3)];
        let found = placed(&pass, [&roots]);
        assert_eq!(
            found.into_iter().collect::<Vec<_>>(),
            [DwgHandle(0x10), DwgHandle(0x11)]
        );
    }

    #[test]
    fn spaces_and_xrefs_are_never_anonymous_blocks() {
        assert!(!is_importable(&block("*Paper_Space0", true, vec![])));
        let mut xref = block("*U1", true, vec![]);
        xref.is_xref = true;
        assert!(!is_anonymous(&xref));
        assert!(is_anonymous(&block("*U1", false, vec![])));
        assert!(!is_anonymous(&block("Door", false, vec![])));
    }
}
