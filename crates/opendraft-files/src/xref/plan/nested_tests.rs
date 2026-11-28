//! A reference inside the source: read from its own file, carried as a local
//! block, named like AutoCAD names it.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use opendraft_core::{
    Attributes, BlockDef, BlockEntity, BlockObjectId, Document, Entity, Insert, Layer, Point,
    XrefDef, XrefKind, XrefLayer, XrefLoadRequest, XrefPathType, XrefPlan, XrefReport,
};

use super::super::source::XrefLimits;
use super::plan_xref;
use crate::{FileFormat, NativeFormat};

fn folder(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("od-xref-nested-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn write(doc: &Document, path: &Path) {
    std::fs::write(path, NativeFormat.write(doc).unwrap().bytes).unwrap();
}

/// A drawing with layer "Walls" and one line on it.
fn walls() -> Document {
    let mut doc = Document::default();
    let layer = doc.allocate_layer_id();
    doc.push_layer(Layer::new(layer, "Walls"));
    doc.add_with(
        Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(3.0, 0.0),
        },
        Attributes::on_layer(layer),
    );
    doc
}

/// A drawing with its own line on layer "Rooms", and a reference called
/// "walls" to `path` of the given kind, with that reference's layer
/// "walls|Walls" bound, placed at (5, 0).
fn site(path: &str, kind: XrefKind) -> Document {
    let mut doc = Document::default();
    let rooms = doc.allocate_layer_id();
    doc.push_layer(Layer::new(rooms, "Rooms"));
    let bound = doc.allocate_layer_id();
    doc.push_layer(Layer::new(bound, "walls|Walls"));
    doc.add_with(
        Entity::Line {
            start: Point::new(0.0, 1.0),
            end: Point::new(1.0, 1.0),
        },
        Attributes::on_layer(rooms),
    );
    let id = doc.allocate_block_id();
    let root = Arc::new(BlockDef {
        id,
        name: "walls".to_owned(),
        base: Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities: vec![BlockEntity::new(
            BlockObjectId(0),
            Entity::Line {
                start: Point::new(0.0, 0.0),
                end: Point::new(3.0, 0.0),
            },
            Attributes::on_layer(bound),
        )],
        dynamic: None,
    });
    doc.push_block(root.clone());
    doc.set_xrefs(vec![XrefDef {
        block: id,
        path: path.to_owned(),
        path_type: XrefPathType::Relative,
        kind,
        unloaded: false,
        stamp: None,
        layers: vec![XrefLayer {
            source: "Walls".to_owned(),
            layer: bound,
        }],
        blocks: Vec::new(),
        clip: None,
        report: XrefReport::default(),
    }]);
    doc.add(Entity::Insert(Box::new(Insert::new_xref(
        root,
        Point::new(5.0, 0.0),
    ))));
    doc.validate().unwrap();
    doc
}

fn plan_of(host: &Document, source: &Document, file: &Path) -> XrefPlan {
    let request = XrefLoadRequest {
        block: None,
        name: "site".to_owned(),
        source_path: file.to_string_lossy().into_owned(),
        path_type: XrefPathType::Full,
        kind: XrefKind::Attach,
        host_dir: None,
        host_path: None,
    };
    plan_xref(
        host,
        source,
        &request,
        &XrefLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap()
}

/// The plan applied the way the app applies it, then checked.
fn applied(mut host: Document, plan: &XrefPlan) -> Document {
    for layer in &plan.added_layers {
        host.push_layer(layer.clone());
    }
    host.set_xrefs(vec![plan.xref.clone()]);
    host.push_block(plan.root.clone());
    host.add(Entity::Insert(Box::new(Insert::new_xref(
        plan.root.clone(),
        Point::default(),
    ))));
    host.validate().unwrap();
    host
}

fn has_note(plan: &XrefPlan, text: &str) -> bool {
    plan.xref.report.notes.iter().any(|n| n.contains(text))
}

#[test]
fn an_attached_reference_is_read_and_carried_as_a_local_block() {
    let dir = folder("carried");
    write(&walls(), &dir.join("walls.odraft"));
    let source = site("walls.odraft", XrefKind::Attach);
    let file = dir.join("site.odraft");
    write(&source, &file);

    let plan = plan_of(&Document::default(), &source, &file);
    let names: Vec<&str> = plan.added_layers.iter().map(|l| l.name.as_str()).collect();
    assert_eq!(names.len(), 2, "no layer twice: {names:?}");
    assert!(names.contains(&"site|Rooms") && names.contains(&"site|walls|Walls"));
    let sources: Vec<&str> = plan.xref.layers.iter().map(|l| l.source.as_str()).collect();
    assert_eq!(sources, ["Rooms", "walls|Walls"]);

    assert_eq!(plan.xref.blocks.len(), 1);
    let local = &plan.xref.blocks[0];
    assert_eq!(local.name, "walls");
    assert_eq!(local.entities.len(), 1);
    let bound = plan.xref.layer_for("walls|Walls").unwrap();
    assert_eq!(local.entities[0].attrs.layer, bound);

    // The root: its own line, and an ordinary insert of the local block.
    assert_eq!(plan.root.entities.len(), 2);
    let inserts: Vec<_> = plan
        .root
        .entities
        .iter()
        .filter_map(|e| match &e.entity {
            Entity::Insert(i) => Some(i),
            _ => None,
        })
        .collect();
    assert_eq!(inserts.len(), 1);
    assert!(!inserts[0].is_xref);
    assert_eq!(inserts[0].block.id, local.id);
    assert_eq!(inserts[0].insertion, Point::new(5.0, 0.0));
    assert!(has_note(
        &plan,
        "Loaded nested external reference \"walls\""
    ));

    let nested = &plan.xref.report.nested;
    assert_eq!(nested.len(), 1);
    assert_eq!((nested[0].name.as_str(), nested[0].parent), ("walls", None));
    assert!(nested[0].loaded && nested[0].found.ends_with("walls.odraft"));
    assert!(nested[0].stamp.is_some_and(|s| s.length > 0));

    let doc = applied(Document::default(), &plan);
    assert_eq!(doc.xrefs().len(), 1, "one record: the nesting is inside it");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_reference_that_leads_back_is_not_followed() {
    let dir = folder("cycle");
    let source = site("site.odraft", XrefKind::Attach);
    let file = dir.join("site.odraft");
    write(&source, &file);

    let plan = plan_of(&Document::default(), &source, &file);
    assert!(plan.xref.blocks.is_empty());
    assert_eq!(plan.root.entities.len(), 1, "only its own line");
    assert!(has_note(&plan, "lead back"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn two_references_that_lead_to_each_other_stop() {
    let dir = folder("pair");
    // walls.odraft is a site that refers to site.odraft, which refers back.
    write(
        &site("site.odraft", XrefKind::Attach),
        &dir.join("walls.odraft"),
    );
    let source = site("walls.odraft", XrefKind::Attach);
    let file = dir.join("site.odraft");
    write(&source, &file);

    let plan = plan_of(&Document::default(), &source, &file);
    assert!(has_note(&plan, "lead back"), "{:?}", plan.xref.report.notes);
    applied(Document::default(), &plan);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_overlay_is_not_carried_through_an_attachment() {
    let dir = folder("overlay");
    write(&walls(), &dir.join("walls.odraft"));
    let source = site("walls.odraft", XrefKind::Overlay);
    let file = dir.join("site.odraft");
    write(&source, &file);

    let plan = plan_of(&Document::default(), &source, &file);
    assert!(plan.xref.blocks.is_empty());
    assert!(has_note(&plan, "an overlay is not carried"));
    // Still listed, as one that was not read.
    assert_eq!(plan.xref.report.nested.len(), 1);
    assert!(!plan.xref.report.nested[0].loaded);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_reference_whose_file_is_missing_is_noted_and_left_out() {
    let dir = folder("missing");
    let source = site("gone.odraft", XrefKind::Attach);
    let file = dir.join("site.odraft");
    write(&source, &file);

    let plan = plan_of(&Document::default(), &source, &file);
    assert!(plan.xref.blocks.is_empty());
    assert_eq!(plan.root.entities.len(), 1);
    assert!(has_note(&plan, "was not loaded"));
    assert!(!plan.xref.report.nested[0].loaded);
    assert!(plan.xref.report.nested[0].found.is_empty());
    // Its layer copy in the source is not made a layer of the host either.
    assert!(plan
        .added_layers
        .iter()
        .all(|l| l.name != "site|walls|Walls"));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn a_reload_keeps_the_nested_layers() {
    let dir = folder("reload");
    write(&walls(), &dir.join("walls.odraft"));
    let source = site("walls.odraft", XrefKind::Attach);
    let file = dir.join("site.odraft");
    write(&source, &file);

    let first = plan_of(&Document::default(), &source, &file);
    let host = applied(Document::default(), &first);
    let root = first.xref.block;
    let request = XrefLoadRequest {
        block: Some(root),
        name: "site".to_owned(),
        source_path: file.to_string_lossy().into_owned(),
        path_type: XrefPathType::Full,
        kind: XrefKind::Attach,
        host_dir: None,
        host_path: None,
    };
    let again = plan_xref(
        &host,
        &source,
        &request,
        &XrefLimits::default(),
        &AtomicBool::new(false),
    )
    .unwrap();
    assert!(
        again.added_layers.is_empty(),
        "the bound layers were reused"
    );
    assert_eq!(
        again.xref.layer_for("walls|Walls"),
        first.xref.layer_for("walls|Walls")
    );
    let _ = std::fs::remove_dir_all(dir);
}
