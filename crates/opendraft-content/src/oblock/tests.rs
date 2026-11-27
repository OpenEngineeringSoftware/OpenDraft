use serde_json::{json, Value};

use super::*;
use crate::test_support::{drawing_with, drawing_with_nested, radius};

fn bolt() -> BlockFile {
    let (doc, id) = drawing_with("bolt", 5.0);
    BlockFile::from_block(&doc, id, &["fixing".to_owned()])
        .expect("a block")
        .0
}

fn edited(bytes: &[u8], change: impl FnOnce(&mut Value)) -> Vec<u8> {
    let mut value: Value = serde_json::from_slice(bytes).expect("json");
    change(&mut value);
    serde_json::to_vec(&value).expect("bytes")
}

#[test]
fn a_block_round_trips_through_its_bytes() {
    let file = bolt();
    let back = BlockFile::read(&file.to_bytes().expect("bytes")).expect("read");
    assert_eq!(back.name(), "bolt");
    assert_eq!(back.uuid(), file.uuid());
    assert_eq!(back.tags(), ["fixing".to_owned()]);
    assert_eq!(radius(back.definition().expect("def")), 5.0);
    assert_eq!(back.package().entries.len(), 1);
}

#[test]
fn the_same_file_is_the_same_bytes() {
    let file = bolt();
    assert_eq!(file.to_bytes().expect("a"), file.to_bytes().expect("b"));
}

#[test]
fn nested_blocks_come_along_as_components() {
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let (file, _) = BlockFile::from_block(&doc, outer, &[]).expect("a block");
    let back = BlockFile::read(&file.to_bytes().expect("bytes")).expect("read");
    assert_eq!(back.name(), "bolt");
    let roots = back
        .package()
        .entries
        .iter()
        .filter(|e| !e.component)
        .count();
    assert_eq!(roots, 1);
    assert_eq!(back.drawing().blocks().len(), 2);
    assert!(back.drawing().block_named("nut").is_some());
}

#[test]
fn a_block_that_is_not_there_is_refused() {
    let (doc, id) = drawing_with("bolt", 1.0);
    let missing = BlockId(id.0 + 99);
    assert_eq!(
        BlockFile::from_block(&doc, missing, &[]).err(),
        Some(OblockError::NoSuchBlock)
    );
}

#[test]
fn a_package_of_two_blocks_is_not_one_block() {
    let (doc, a) = drawing_with("one", 1.0);
    let mut doc = doc;
    let b = crate::test_support::add_block(&mut doc, "two", "Symbols", 2.0, &[]).id;
    let entry = |block| Entry {
        uuid: BlockUuid::new(),
        block,
        folder: String::new(),
        tags: Vec::new(),
        component: false,
    };
    let package = BlocksPackage {
        drawing: doc,
        entries: vec![entry(a), entry(b)],
    };
    assert_eq!(
        BlockFile::from_package(package).err(),
        Some(OblockError::NotOneBlock(2))
    );
    let empty = BlocksPackage::empty();
    assert_eq!(
        BlockFile::from_package(empty).err(),
        Some(OblockError::NotOneBlock(0))
    );
}

#[test]
fn a_file_of_another_format_is_refused() {
    let bytes = bolt().to_bytes().expect("bytes");
    let other = edited(&bytes, |v| v["format"] = json!("odpack"));
    assert_eq!(BlockFile::read(&other).err(), Some(OblockError::NotOblock));
    assert_eq!(
        BlockFile::read(b"[1, 2]").err(),
        Some(OblockError::NotOblock)
    );
}

#[test]
fn another_version_is_refused() {
    let bytes = bolt().to_bytes().expect("bytes");
    let other = edited(&bytes, |v| v["format_version"] = json!(1));
    assert_eq!(
        BlockFile::read(&other).err(),
        Some(OblockError::FormatVersion(Some(1)))
    );
    let none = edited(&bytes, |v| {
        v.as_object_mut().expect("object").remove("format_version");
    });
    assert_eq!(
        BlockFile::read(&none).err(),
        Some(OblockError::FormatVersion(None))
    );
}

#[test]
fn an_unknown_top_level_field_is_refused() {
    let bytes = bolt().to_bytes().expect("bytes");
    let other = edited(&bytes, |v| v["extra"] = json!(1));
    assert_eq!(
        BlockFile::read(&other).err(),
        Some(OblockError::UnknownField("extra".into()))
    );
}

#[test]
fn a_duplicate_key_and_a_huge_file_are_refused() {
    let dup = br#"{"format":"oblock","format":"oblock","format_version":0,"package":{}}"#;
    assert!(matches!(
        BlockFile::read(dup).err(),
        Some(OblockError::Json(_))
    ));
    let big = vec![b' '; MAX_BYTES + 1];
    assert_eq!(
        BlockFile::read(&big).err(),
        Some(OblockError::TooLarge { max: MAX_BYTES })
    );
}

#[test]
fn a_damaged_payload_is_refused_not_guessed() {
    let bytes = bolt().to_bytes().expect("bytes");
    let other = edited(&bytes, |v| v["package"]["blocks"] = json!([]));
    assert!(matches!(
        BlockFile::read(&other).err(),
        Some(OblockError::Payload(_))
    ));
    let other = edited(&bytes, |v| v["package"]["blocks"][0]["block"] = json!(9999));
    assert!(matches!(
        BlockFile::read(&other).err(),
        Some(OblockError::Payload(_))
    ));
}

#[test]
fn an_edited_drawing_keeps_the_block_and_adds_what_it_gained() {
    let file = bolt();
    let mut drawing = file.drawing().clone();
    let extra = crate::test_support::add_block(&mut drawing, "washer", "Symbols", 3.0, &[]);
    let after = file.with_drawing(drawing).expect("edited");
    assert_eq!(after.uuid(), file.uuid());
    assert_eq!(after.tags(), file.tags());
    let washer = after
        .package()
        .entry_for_block(extra.id)
        .expect("an entry for the washer");
    assert!(washer.component);
}

#[test]
fn an_edited_drawing_without_the_block_is_refused() {
    let file = bolt();
    assert_eq!(
        file.with_drawing(Document::default()).err(),
        Some(OblockError::NoSuchBlock)
    );
}

#[test]
fn a_block_is_saved_and_loaded_through_a_file() {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("bolt.oblock");
    let file = bolt();
    file.save(&path).expect("save");
    let back = BlockFile::load(&path).expect("load");
    assert_eq!(back.uuid(), file.uuid());
    assert!(!dir.path().join("bolt.oblock.tmp").exists());
    assert!(matches!(
        BlockFile::load(&dir.path().join("none.oblock")).err(),
        Some(OblockError::Io(_))
    ));
}
