//! Tests of the library, on real folders.

use std::fs;

use super::*;
use crate::test_support::{drawing_with, drawing_with_nested};

mod capture;
mod organise;
mod search;
mod trash;

pub(super) const NOW: u64 = 1_000_000;

pub(super) fn open() -> (tempfile::TempDir, Library) {
    let dir = tempfile::tempdir().expect("temp dir");
    let lib = Library::open(&dir.path().join("Library")).expect("opens");
    (dir, lib)
}

pub(super) fn reopen(lib: &Library) -> Library {
    Library::open(lib.root()).expect("reopens")
}

/// Captures a new one-circle block called `name` at the top level.
pub(super) fn add(lib: &mut Library, name: &str, radius: f64) -> BlockUuid {
    let (doc, id) = drawing_with(name, radius);
    lib.capture(&doc, id, &CaptureRequest::default(), NOW)
        .expect("captures")
        .id
}

pub(super) fn names(blocks: &[BlockInfo]) -> Vec<&str> {
    blocks.iter().map(|b| b.name.as_str()).collect()
}

fn file_of(lib: &Library) -> std::path::PathBuf {
    lib.root().join(db::FILE)
}

fn backup_of(lib: &Library) -> std::path::PathBuf {
    lib.root().join(db::BACKUP)
}

#[test]
fn a_new_library_is_empty_and_makes_its_file() {
    let (_dir, lib) = open();
    assert!(lib.blocks().is_empty());
    assert!(lib.notices().is_empty());
    assert!(file_of(&lib).is_file());
}

#[test]
fn what_is_captured_is_there_after_closing_and_opening() {
    let (_dir, mut lib) = open();
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let request = CaptureRequest {
        description: Some("An M8 bolt".into()),
        tags: vec!["fixing".into()],
        folder: "Fixings/Metric".into(),
        ..Default::default()
    };
    let id = lib
        .capture(&doc, outer, &request, NOW)
        .expect("captures")
        .id;
    lib.set_favourite(id, true).expect("favourite");
    lib.touch_recent(id).expect("recent");
    lib.create_folder("Empty/Inside").expect("folder");

    let again = reopen(&lib);
    let block = again.block(id).expect("kept");
    assert_eq!(block.name, "bolt");
    assert_eq!(block.description, "An M8 bolt");
    assert_eq!(block.tags, ["fixing"]);
    assert_eq!(block.folder, "Fixings/Metric");
    assert_eq!(block.components, ["nut"]);
    assert!(block.favourite);
    assert_eq!(again.blocks().len(), 1);
    assert_eq!(again.recents().len(), 1);
    assert!(again.folders().contains(&"Empty/Inside".to_owned()));
}

#[test]
fn a_block_is_stored_whole_and_read_back_as_a_block_file() {
    let (_dir, mut lib) = open();
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let id = lib
        .capture(&doc, outer, &CaptureRequest::default(), NOW)
        .expect("captures")
        .id;
    let file = lib.block_file(id).expect("a block file");
    assert_eq!(file.name(), "bolt");
    assert_eq!(file.uuid(), id);
    assert_eq!(file.drawing().blocks().len(), 2);
    let (source, root) = lib.source(id).expect("a source");
    assert_eq!(source.block(root).map(|b| b.name.as_str()), Some("bolt"));
    assert!(lib.source(BlockUuid::new()).is_none());
    assert_eq!(
        lib.block_file(BlockUuid::new()).err(),
        Some(LibraryError::NotFound)
    );
}

#[test]
fn the_backup_is_the_library_as_it_was_when_last_opened() {
    let (_dir, mut lib) = open();
    add(&mut lib, "one", 1.0);
    let mut lib = reopen(&lib);
    assert!(backup_of(&lib).is_file());
    add(&mut lib, "two", 2.0);
    let backup = Connection::open(backup_of(&lib)).expect("opens");
    let count: i64 = backup
        .query_row("SELECT count(*) FROM blocks", [], |r| r.get(0))
        .expect("counts");
    assert_eq!(count, 1);
}

#[test]
fn no_temporary_file_or_journal_is_left_behind() {
    let (_dir, mut lib) = open();
    add(&mut lib, "one", 1.0);
    lib.set_favourite(lib.blocks()[0].id, true).expect("saves");
    drop(reopen(&lib));
    let leftovers: Vec<_> = fs::read_dir(lib.root())
        .expect("lists")
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp") || n.ends_with("-journal") || n.ends_with("-wal"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");
}

#[test]
fn a_damaged_file_is_replaced_by_the_backup_and_kept_aside() {
    let (_dir, mut lib) = open();
    add(&mut lib, "one", 1.0);
    let mut lib = {
        let again = reopen(&lib);
        drop(lib);
        again
    };
    add(&mut lib, "two", 2.0);
    let root = lib.root().to_owned();
    drop(lib);
    fs::write(root.join(db::FILE), b"this is not a database at all").expect("damages");

    let mut again = Library::open(&root).expect("opens from the backup");
    assert_eq!(names(&again.blocks()), ["one"]);
    assert_eq!(again.notices().len(), 1);
    assert!(again.notices()[0].contains("backup"));
    assert!(root.join(db::DAMAGED).exists());

    // Working on from the backup does not lose it.
    add(&mut again, "three", 3.0);
    let last = reopen(&again);
    assert_eq!(names(&last.blocks()), ["one", "three"]);
    assert!(last.notices().is_empty());
}

#[test]
fn when_nothing_can_be_read_nothing_is_touched() {
    let (_dir, mut lib) = open();
    add(&mut lib, "one", 1.0);
    let root = lib.root().to_owned();
    drop(reopen(&lib));
    drop(lib);
    let (main, backup) = (root.join(db::FILE), root.join(db::BACKUP));
    fs::write(&main, b"damaged").expect("damages");
    fs::write(&backup, b"damaged too").expect("damages");

    let error = Library::open(&root).expect_err("refused");
    assert!(matches!(error, LibraryError::Corrupt(_)), "{error:?}");
    assert_eq!(fs::read(&main).expect("kept"), b"damaged");
    assert_eq!(fs::read(&backup).expect("kept"), b"damaged too");
}

#[test]
fn a_damaged_file_with_no_backup_is_refused_and_untouched() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().join("Library");
    fs::create_dir_all(&root).expect("dir");
    fs::write(root.join(db::FILE), b"damaged").expect("damages");
    let error = Library::open(&root).expect_err("refused");
    assert!(matches!(error, LibraryError::Corrupt(_)), "{error:?}");
    assert_eq!(fs::read(root.join(db::FILE)).expect("kept"), b"damaged");
}

#[test]
fn a_missing_file_with_a_backup_is_restored() {
    let (_dir, mut lib) = open();
    add(&mut lib, "one", 1.0);
    let root = lib.root().to_owned();
    drop(reopen(&lib));
    drop(lib);
    fs::remove_file(root.join(db::FILE)).expect("removes");
    let again = Library::open(&root).expect("restores");
    assert_eq!(names(&again.blocks()), ["one"]);
    assert!(again.notices()[0].contains("missing"));
}

#[test]
fn a_database_that_is_not_a_library_or_is_newer_is_refused() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().join("Library");
    fs::create_dir_all(&root).expect("dir");
    {
        let other = Connection::open(root.join(db::FILE)).expect("makes");
        other
            .execute_batch("CREATE TABLE notes (text TEXT); INSERT INTO notes VALUES ('hi');")
            .expect("fills");
    }
    let error = Library::open(&root).expect_err("not ours");
    assert!(matches!(error, LibraryError::Unsupported(_)), "{error:?}");
    assert!(root.join(db::FILE).exists() && !root.join(db::DAMAGED).exists());

    let (_dir2, lib) = open();
    let root = lib.root().to_owned();
    drop(lib);
    {
        let conn = Connection::open(root.join(db::FILE)).expect("opens");
        conn.pragma_update(None, "user_version", 7).expect("sets");
    }
    let error = Library::open(&root).expect_err("newer");
    assert!(matches!(error, LibraryError::Unsupported(_)), "{error:?}");
    assert!(error.to_string().contains("version 7"), "{error}");
    // A newer library is never swapped for the backup.
    assert!(!root.join(db::DAMAGED).exists());
}

#[test]
fn a_failed_write_changes_nothing_in_memory_or_in_the_file() {
    let (_dir, mut lib) = open();
    add(&mut lib, "one", 1.0);
    // A library that cannot be written to.
    lib.connection()
        .pragma_update(None, "query_only", true)
        .expect("sets");

    let (doc, id) = drawing_with("two", 2.0);
    let error = lib
        .capture(&doc, id, &CaptureRequest::default(), NOW)
        .expect_err("cannot write");
    assert!(matches!(error, LibraryError::Io(_)), "{error:?}");
    assert_eq!(names(&lib.blocks()), ["one"]);
    assert_eq!(names(&reopen(&lib).blocks()), ["one"]);

    lib.connection()
        .pragma_update(None, "query_only", false)
        .expect("sets");
    add(&mut lib, "two", 2.0);
    assert_eq!(lib.blocks().len(), 2);
}

#[test]
fn what_refers_to_nothing_is_dropped_on_opening() {
    let (_dir, mut lib) = open();
    let id = add(&mut lib, "one", 1.0);
    lib.set_favourite(id, true).expect("favourite");
    lib.touch_recent(id).expect("recent");
    lib.delete_block(id, NOW).expect("deletes");
    let again = reopen(&lib);
    assert!(again.favourites().is_empty() && again.recents().is_empty());
    assert_eq!(again.trash_items().len(), 1);
}

#[test]
fn a_change_to_one_block_does_not_touch_the_bytes_of_another() {
    let (_dir, mut lib) = open();
    let a = add(&mut lib, "a", 1.0);
    let b = add(&mut lib, "b", 2.0);
    let bytes = |lib: &Library, id: BlockUuid| {
        let key = id.to_string();
        lib.connection()
            .query_row("SELECT data FROM blocks WHERE id = ?1", [key], |r| {
                r.get::<_, Vec<u8>>(0)
            })
            .expect("row")
    };
    let before = bytes(&lib, b);
    lib.rename_block(a, "renamed").expect("renames");
    lib.set_tags(a, vec!["x".into()]).expect("tags");
    lib.move_block(a, "Somewhere").expect("moves");
    assert_eq!(bytes(&lib, b), before);
}

#[test]
fn a_library_of_many_blocks_lists_searches_and_reopens() {
    let (_dir, mut lib) = open();
    for i in 0..300 {
        add(&mut lib, &format!("block {i:03}"), 1.0 + f64::from(i));
    }
    assert_eq!(lib.blocks().len(), 300);
    let found = lib.search(&Query {
        text: "block 299".into(),
        ..Default::default()
    });
    assert_eq!(names(&found), ["block 299"]);
    assert_eq!(reopen(&lib).blocks().len(), 300);
}
