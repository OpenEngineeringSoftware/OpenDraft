//! Snapping to a clipped reference: only the part it draws offers points.

use std::sync::Arc;

use opendraft_core::block::{BlockDef, BlockEntity, BlockObjectId, Insert};
use opendraft_core::{
    Attributes, BlockId, Document, Entity, LayerId, Object, Point, XrefClip, XrefDef, XrefKind,
    XrefPathType, XrefReport,
};

use super::*;

fn p(x: f64, y: f64) -> Point {
    Point::new(x, y)
}

/// A drawing whose one reference is a line from x=0 to x=10, clipped to
/// x 0..4.
fn clipped() -> (Document, BlockId) {
    let mut doc = Document::default();
    let root_id = doc.allocate_block_id();
    let root = Arc::new(BlockDef {
        id: root_id,
        name: "base".to_owned(),
        base: Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities: vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Line {
                start: p(0.0, 0.0),
                end: p(10.0, 0.0),
            },
            Attributes::on_layer(LayerId::DEFAULT),
        )],
        dynamic: None,
    });
    doc.push_block(root.clone());
    doc.set_xrefs(vec![XrefDef {
        block: root_id,
        path: "base.dwg".to_owned(),
        path_type: XrefPathType::Relative,
        kind: XrefKind::Attach,
        unloaded: false,
        stamp: None,
        layers: Vec::new(),
        blocks: Vec::new(),
        clip: Some(XrefClip::new(vec![
            p(0.0, -1.0),
            p(4.0, -1.0),
            p(4.0, 1.0),
            p(0.0, 1.0),
        ])),
        report: XrefReport::default(),
    }]);
    doc.add_with(
        Entity::Insert(Box::new(Insert::new_xref(root, Point::default()))),
        Attributes::on_layer(LayerId::DEFAULT),
    );
    doc.validate().unwrap();
    (doc, root_id)
}

fn probe(cursor: Point, tolerance: f64, modes: &SnapModes) -> Probe<'_> {
    Probe {
        cursor,
        tolerance,
        grid_step: None,
        grid_frame: opendraft_core::Ccs::WORLD,
        modes,
        from: None,
        along: None,
        defer: true,
        tracks: &[],
        track: None,
    }
}

/// Only `kind` runs, and there is no grid.
fn only(kind: SnapKind) -> SnapModes {
    let mut modes = SnapModes {
        grid: false,
        ..SnapModes::default()
    };
    for other in SnapKind::OBJECT {
        modes.set(other, other == kind);
    }
    modes
}

#[test]
fn the_hidden_end_of_a_clipped_reference_is_not_offered() {
    let (doc, _root) = clipped();
    let object = &doc.objects()[0];
    let modes = only(SnapKind::Endpoint);
    let hidden = snap_all_in(&doc, [object], &probe(p(9.5, 0.0), 1.0, &modes));
    assert!(
        hidden.iter().all(|snap| snap.point != p(10.0, 0.0)),
        "{hidden:?}"
    );
    let at_edge = snap_all_in(&doc, [object], &probe(p(4.0, 0.0), 1.0, &modes));
    assert!(
        at_edge.iter().any(|snap| snap.point == p(4.0, 0.0)),
        "{at_edge:?}"
    );
    // Without the drawing the whole snapshot is still what an insert offers.
    let plain = snap_all([object], &probe(p(9.5, 0.0), 1.0, &modes));
    assert!(plain.iter().any(|snap| snap.point == p(10.0, 0.0)));
}

#[test]
fn a_crossing_beyond_the_boundary_is_not_offered() {
    let (mut doc, _root) = clipped();
    for x in [3.0, 9.0] {
        doc.add(Entity::Line {
            start: p(x, -2.0),
            end: p(x, 2.0),
        });
    }
    let modes = only(SnapKind::Intersection);
    let objects: Vec<&Object> = doc.objects().iter().collect();
    let outside = snap_all_in(&doc, objects.clone(), &probe(p(9.0, 0.0), 1.0, &modes));
    assert!(
        outside.iter().all(|snap| snap.point != p(9.0, 0.0)),
        "{outside:?}"
    );
    let inside = snap_all_in(&doc, objects, &probe(p(3.0, 0.0), 1.0, &modes));
    assert!(
        inside.iter().any(|snap| snap.point == p(3.0, 0.0)),
        "{inside:?}"
    );
}
