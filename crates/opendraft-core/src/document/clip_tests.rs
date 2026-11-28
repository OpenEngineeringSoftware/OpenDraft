//! Clipping a reference: what an insert draws inside its `XCLIP` boundary.

use std::sync::Arc;

use crate::block::{BlockDef, BlockEntity, BlockObjectId};
use crate::{
    Attributes, BlockId, Document, Entity, Insert, LayerId, Point, XrefClip, XrefDef, XrefKind,
    XrefPathType, XrefReport,
};

/// A drawing with a reference whose root holds one line from -5 to 15 on the
/// x axis, placed at `(100, 0)`, and the given clip.
fn clipped(clip: Option<XrefClip>) -> (Document, BlockId) {
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
                start: Point::new(-5.0, 0.0),
                end: Point::new(15.0, 0.0),
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
        clip,
        report: XrefReport::default(),
    }]);
    doc.add_with(
        Entity::Insert(Box::new(Insert::new_xref(root, Point::new(100.0, 0.0)))),
        Attributes::on_layer(LayerId::DEFAULT),
    );
    (doc, root_id)
}

fn square() -> Vec<Point> {
    vec![
        Point::new(0.0, 0.0),
        Point::new(10.0, 0.0),
        Point::new(10.0, 10.0),
        Point::new(0.0, 10.0),
    ]
}

fn pieces(doc: &Document) -> Vec<Entity> {
    let object = &doc.objects()[0];
    doc.insert_pieces(object)
        .expect("an insert")
        .into_iter()
        .map(|piece| piece.entity)
        .collect()
}

#[test]
fn an_enabled_clip_cuts_the_pieces_at_the_boundary() {
    let (doc, _root) = clipped(Some(XrefClip::new(square())));
    doc.validate().unwrap();
    let pieces = pieces(&doc);
    assert_eq!(pieces.len(), 1);
    let Entity::Line { start, end } = pieces[0] else {
        panic!("a clipped line was expected: {pieces:?}")
    };
    assert!((start.x - 100.0).abs() < 1e-9, "{start:?}");
    assert!((end.x - 110.0).abs() < 1e-9, "{end:?}");
}

#[test]
fn a_disabled_clip_keeps_the_whole_reference() {
    let mut clip = XrefClip::new(square());
    clip.enabled = false;
    let (doc, _root) = clipped(Some(clip));
    doc.validate().unwrap();
    let pieces = pieces(&doc);
    assert_eq!(pieces.len(), 1);
    let Entity::Line { start, end } = pieces[0] else {
        panic!("the line was expected")
    };
    assert!((start.x - 95.0).abs() < 1e-9 && (end.x - 115.0).abs() < 1e-9);
}

#[test]
fn no_clip_keeps_the_whole_reference() {
    let (doc, _root) = clipped(None);
    doc.validate().unwrap();
    assert_eq!(pieces(&doc).len(), 1);
}

#[test]
fn a_boundary_that_is_not_a_polygon_is_refused() {
    let mut doc = Document::default();
    let root_id = doc.allocate_block_id();
    doc.push_block(Arc::new(BlockDef {
        id: root_id,
        name: "base".to_owned(),
        base: Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities: Vec::new(),
        dynamic: None,
    }));
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
            Point::new(0.0, 0.0),
            Point::new(1.0, 1.0),
        ])),
        report: XrefReport::default(),
    }]);
    let error = doc.validate().unwrap_err().to_string();
    assert!(error.contains("clip boundary"), "{error}");
}

#[test]
fn the_frame_plots_only_when_xclipframe_is_two() {
    let (mut doc, _root) = clipped(Some(XrefClip::new(square())));
    let region = crate::Bounds::from_points(Point::new(90.0, -5.0), Point::new(120.0, 15.0));
    let count = |doc: &Document| {
        crate::plot_model::plot_region(doc, region, 30.0, None)
            .unwrap()
            .paths
    };

    doc.set_xclip_frame(2);
    let plotted = count(&doc);
    assert!(
        plotted.iter().any(|path| path.closed),
        "the frame is plotted"
    );
    doc.set_xclip_frame(1);
    let drawn_only = count(&doc);
    assert!(
        drawn_only.iter().all(|path| !path.closed),
        "XCLIPFRAME 1 draws but does not plot the frame"
    );
    assert_eq!(drawn_only.len() + 1, plotted.len());
}
