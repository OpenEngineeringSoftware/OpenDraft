use opendraft_core::{BlockId, Document};

use super::*;
use crate::test_support::{add_block, drawing_with, drawing_with_nested, radius};

fn capture(lib: &mut Library, doc: &Document, id: BlockId, request: &CaptureRequest) -> Captured {
    lib.capture(doc, id, request, NOW).expect("captures")
}

#[test]
fn a_block_comes_in_with_what_it_contains_shown_only_under_it() {
    let (_dir, mut lib) = open();
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let captured = capture(&mut lib, &doc, outer, &CaptureRequest::default());
    assert_eq!(captured.name, "bolt");
    assert!(!captured.replaced);
    // Two blocks in its file, one in the list.
    assert_eq!(names(&lib.blocks()), ["bolt"]);
    assert_eq!(lib.blocks()[0].components, ["nut"]);
    let file = lib.block_file(captured.id).expect("its file");
    assert_eq!(file.drawing().blocks().len(), 2);
    assert!(file.drawing().validate().is_ok());
    // The layer came with it.
    assert!(file.drawing().layers().iter().any(|l| l.name == "Symbols"));
}

#[test]
fn the_request_names_describes_tags_and_files_the_block() {
    let (_dir, mut lib) = open();
    let (doc, id) = drawing_with("v1", 5.0);
    let request = CaptureRequest {
        name: Some("  Gate valve ".into()),
        description: Some("A gate valve.\nDN50.".into()),
        tags: vec!["valve".into(), "piping".into()],
        folder: "Piping".into(),
        ..Default::default()
    };
    let captured = capture(&mut lib, &doc, id, &request);
    assert_eq!(captured.name, "Gate valve");
    let block = lib.block(captured.id).expect("there");
    assert_eq!(block.name, "Gate valve");
    assert_eq!(block.description, "A gate valve.\nDN50.");
    assert_eq!(
        (block.tags, block.folder),
        (vec!["valve".into(), "piping".into()], "Piping".into())
    );
    assert_eq!(lib.folders(), ["Piping"]);
}

#[test]
fn bad_names_tags_and_folders_are_refused_before_anything_is_written() {
    let (_dir, mut lib) = open();
    let (doc, id) = drawing_with("v1", 5.0);
    let try_with = |lib: &mut Library, change: fn(&mut CaptureRequest)| {
        let mut request = CaptureRequest::default();
        change(&mut request);
        lib.capture(&doc, id, &request, NOW).expect_err("refused")
    };
    assert!(matches!(
        try_with(&mut lib, |r| r.name = Some("a/b".into())),
        LibraryError::InvalidName(_)
    ));
    assert!(matches!(
        try_with(&mut lib, |r| r.name = Some("  ".into())),
        LibraryError::InvalidName(_)
    ));
    assert!(matches!(
        try_with(&mut lib, |r| r.tags = vec!["a\nb".into()]),
        LibraryError::InvalidName(_)
    ));
    assert!(matches!(
        try_with(&mut lib, |r| r.folder = "a//b".into()),
        LibraryError::Folder(_)
    ));
    assert!(matches!(
        try_with(&mut lib, |r| r.description = Some("x\u{202E}".into())),
        LibraryError::InvalidName(_)
    ));
    assert!(lib.blocks().is_empty());
    assert!(reopen(&lib).blocks().is_empty());
}

#[test]
fn a_block_that_is_not_there_is_not_found() {
    let (_dir, mut lib) = open();
    let (doc, _) = drawing_with("v1", 5.0);
    let error = lib
        .capture(&doc, BlockId(999), &CaptureRequest::default(), NOW)
        .expect_err("missing");
    assert_eq!(error, LibraryError::NotFound);
}

#[test]
fn a_taken_name_can_cancel_keep_both_or_replace() {
    let (_dir, mut lib) = open();
    let old = add(&mut lib, "valve", 1.0);
    let (doc, id) = drawing_with("Valve", 9.0);

    let error = lib
        .capture(&doc, id, &CaptureRequest::default(), NOW)
        .expect_err("taken");
    assert_eq!(error, LibraryError::NameTaken("Valve".into()));
    assert_eq!(lib.blocks().len(), 1);

    let both = CaptureRequest {
        if_name_taken: NameTaken::KeepBoth,
        ..Default::default()
    };
    let second = capture(&mut lib, &doc, id, &both);
    assert_eq!(second.name, "Valve_2");
    assert_eq!(names(&lib.blocks()), ["valve", "Valve_2"]);

    let replace = CaptureRequest {
        if_name_taken: NameTaken::Replace,
        ..Default::default()
    };
    let third = capture(&mut lib, &doc, id, &replace);
    assert!(third.replaced);
    assert_eq!(third.name, "Valve");
    assert!(lib.block(old).is_none());
    assert_eq!(names(&lib.blocks()), ["Valve", "Valve_2"]);
    // The old one is in the Trash, not gone.
    assert_eq!(lib.trash_items().len(), 1);
    assert_eq!(lib.trash_items()[0].name, "valve");
}

#[test]
fn a_part_is_private_to_its_block_so_its_name_is_not_taken() {
    let (_dir, mut lib) = open();
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let bolt = capture(&mut lib, &doc, outer, &CaptureRequest::default()).id;
    // A block of the library called "nut" is another thing from the bolt's nut.
    let (other, id) = drawing_with("nut", 7.0);
    let nut = capture(&mut lib, &other, id, &CaptureRequest::default());
    assert!(!nut.replaced);
    assert_eq!(nut.name, "nut");
    assert_eq!(names(&lib.blocks()), ["bolt", "nut"]);
    assert!(lib.trash_items().is_empty());
    let inside = lib.block_file(bolt).expect("the bolt");
    let part = inside.drawing().block_named("nut").expect("its own nut");
    assert_eq!(radius(part), 1.0);
}

#[test]
fn each_block_keeps_its_own_copy_of_a_shared_part() {
    let (_dir, mut lib) = open();
    let (a, bolt, _) = drawing_with_nested("bolt", "nut");
    let mut b = opendraft_core::Document::default();
    let big_nut = add_block(&mut b, "nut", "Symbols", 5.0, &[]);
    let screw = add_block(&mut b, "screw", "Symbols", 2.0, &[&big_nut]);
    let bolt = capture(&mut lib, &a, bolt, &CaptureRequest::default()).id;
    let screw = capture(&mut lib, &b, screw.id, &CaptureRequest::default()).id;

    let nut_of = |id| {
        let file = lib.block_file(id).expect("a block");
        radius(file.drawing().block_named("nut").expect("a nut"))
    };
    assert_eq!((nut_of(bolt), nut_of(screw)), (1.0, 5.0));
    assert_eq!(names(&lib.blocks()), ["bolt", "screw"]);
}

#[test]
fn a_block_keeps_the_layers_it_uses_and_no_others() {
    let (_dir, mut lib) = open();
    let (a, ida) = drawing_with("one", 1.0);
    let (b, idb) = drawing_with("two", 2.0);
    let one = capture(&mut lib, &a, ida, &CaptureRequest::default()).id;
    let two = capture(&mut lib, &b, idb, &CaptureRequest::default()).id;
    for id in [one, two] {
        let file = lib.block_file(id).expect("a block");
        let layers: Vec<_> = file
            .drawing()
            .layers()
            .iter()
            .filter(|l| l.name == "Symbols")
            .collect();
        assert_eq!(layers.len(), 1);
    }
}

#[test]
fn the_source_drawing_is_never_changed() {
    let (_dir, mut lib) = open();
    let (doc, id) = drawing_with("one", 1.0);
    let blocks_before = doc.blocks().len();
    capture(&mut lib, &doc, id, &CaptureRequest::default());
    assert_eq!(doc.blocks().len(), blocks_before);
    assert_eq!(doc.block(id).map(|b| b.name.as_str()), Some("one"));
}
