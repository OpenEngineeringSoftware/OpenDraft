use super::*;
use crate::test_support::drawing_with_nested;

#[test]
fn renaming_keeps_the_id_and_the_rest() {
    let (_dir, mut lib) = open();
    let id = add(&mut lib, "valve", 1.0);
    lib.set_tags(id, vec!["a".into()]).expect("tags");
    lib.move_block(id, "Piping").expect("moves");
    lib.rename_block(id, " Gate valve ").expect("renames");
    let block = lib.block(id).expect("same id");
    assert_eq!(block.name, "Gate valve");
    assert_eq!(
        (block.tags, block.folder),
        (vec!["a".into()], "Piping".into())
    );
    assert_eq!(
        reopen(&lib).block(id).map(|b| b.name),
        Some("Gate valve".into())
    );
}

#[test]
fn a_block_cannot_be_renamed_to_the_name_of_one_of_its_own_parts() {
    let (_dir, mut lib) = open();
    let (doc, outer, _) = drawing_with_nested("bolt", "nut");
    let bolt = lib
        .capture(&doc, outer, &CaptureRequest::default(), NOW)
        .expect("captures")
        .id;
    assert!(matches!(
        lib.rename_block(bolt, "nut"),
        Err(LibraryError::Package(_) | LibraryError::Refused(_))
    ));
    assert_eq!(names(&lib.blocks()), ["bolt"]);
    // Its file is as it was.
    assert_eq!(lib.block_file(bolt).expect("file").name(), "bolt");
}

#[test]
fn the_name_a_block_is_renamed_to_is_in_its_file_too() {
    let (_dir, mut lib) = open();
    let id = add(&mut lib, "valve", 1.0);
    lib.set_description(id, "Gate").expect("describes");
    lib.set_tags(id, vec!["water".into()]).expect("tags");
    lib.rename_block(id, "tap").expect("renames");
    let file = reopen(&lib).block_file(id).expect("file");
    assert_eq!(file.name(), "tap");
    assert_eq!(file.definition().expect("def").description, "Gate");
    assert_eq!(file.tags(), ["water"]);
}

#[test]
fn a_name_in_use_is_refused_but_a_change_of_case_is_not() {
    let (_dir, mut lib) = open();
    let a = add(&mut lib, "valve", 1.0);
    add(&mut lib, "nut", 2.0);
    assert_eq!(
        lib.rename_block(a, "NUT").expect_err("taken"),
        LibraryError::NameTaken("NUT".into())
    );
    lib.rename_block(a, "VALVE")
        .expect("same block, other case");
    assert!(matches!(
        lib.rename_block(a, "a|b"),
        Err(LibraryError::InvalidName(_))
    ));
    assert_eq!(lib.block(a).map(|b| b.name), Some("VALVE".into()));
}

#[test]
fn description_and_tags_are_checked_and_kept() {
    let (_dir, mut lib) = open();
    let id = add(&mut lib, "valve", 1.0);
    lib.set_description(id, "For water").expect("describes");
    lib.set_tags(id, vec!["water".into(), "valve".into()])
        .expect("tags");
    assert!(lib.set_tags(id, vec![" x".into()]).is_err());
    assert!(lib.set_description(id, "bad\u{0}").is_err());
    let again = reopen(&lib).block(id).expect("kept");
    assert_eq!(again.description, "For water");
    assert_eq!(again.tags, ["water", "valve"]);
}

#[test]
fn blocks_move_between_folders_and_folders_appear_and_go() {
    let (_dir, mut lib) = open();
    let a = add(&mut lib, "a", 1.0);
    let b = add(&mut lib, "b", 2.0);
    lib.move_block(a, "Piping/Valves").expect("moves");
    assert_eq!(lib.folders(), ["Piping", "Piping/Valves"]);
    assert_eq!(lib.subfolders(""), ["Piping"]);
    assert_eq!(lib.subfolders("piping"), ["Piping/Valves"]);
    assert_eq!(names(&lib.blocks_in("")), ["b"]);
    assert_eq!(names(&lib.blocks_in("Piping/Valves")), ["a"]);
    lib.move_block(a, "").expect("back to the top");
    // A folder the user made stays even when it holds nothing.
    assert_eq!(lib.folders(), ["Piping", "Piping/Valves"]);
    assert!(lib.move_block(b, "x/../y").is_err());
}

#[test]
fn an_empty_folder_can_be_made_and_is_kept() {
    let (_dir, mut lib) = open();
    lib.create_folder("Structural/Steel").expect("creates");
    assert_eq!(lib.folders(), ["Structural", "Structural/Steel"]);
    assert_eq!(reopen(&lib).folders(), ["Structural", "Structural/Steel"]);
    assert!(lib.create_folder("").is_err());
    assert!(lib.create_folder("a:b").is_err());
}

#[test]
fn renaming_a_folder_takes_everything_in_it_along() {
    let (_dir, mut lib) = open();
    let a = add(&mut lib, "a", 1.0);
    let b = add(&mut lib, "b", 2.0);
    lib.move_block(a, "Old/Inner").expect("moves");
    lib.move_block(b, "Old").expect("moves");
    lib.create_folder("Old/Empty").expect("creates");
    lib.rename_folder("Old", "New/Place").expect("renames");
    assert_eq!(
        lib.block(a).map(|x| x.folder),
        Some("New/Place/Inner".into())
    );
    assert_eq!(lib.block(b).map(|x| x.folder), Some("New/Place".into()));
    assert_eq!(
        lib.folders(),
        ["New", "New/Place", "New/Place/Empty", "New/Place/Inner"]
    );
    assert_eq!(reopen(&lib).folders(), lib.folders());
}

#[test]
fn a_folder_cannot_go_inside_itself_or_onto_another() {
    let (_dir, mut lib) = open();
    lib.create_folder("A/B").expect("creates");
    lib.create_folder("C").expect("creates");
    assert!(matches!(
        lib.rename_folder("A", "A/B/A"),
        Err(LibraryError::Refused(_))
    ));
    assert!(matches!(
        lib.rename_folder("A", "c"),
        Err(LibraryError::Refused(_))
    ));
    assert_eq!(lib.rename_folder("Nope", "X"), Err(LibraryError::NotFound));
    lib.rename_folder("A", "a").expect("only the case changes");
    assert_eq!(lib.folders(), ["a", "a/B", "C"]);
}

#[test]
fn a_move_that_goes_too_deep_is_refused_whole() {
    let (_dir, mut lib) = open();
    let id = add(&mut lib, "a", 1.0);
    lib.move_block(id, "a/b/c/d").expect("moves");
    lib.create_folder("1/2/3/4/5/6/7").expect("creates");
    assert!(lib.rename_folder("a", "1/2/3/4/5/6/7/8").is_err());
    assert_eq!(lib.block(id).map(|b| b.folder), Some("a/b/c/d".into()));
}

#[test]
fn favourites_and_recents_are_kept_in_order_and_capped() {
    let (_dir, mut lib) = open();
    let ids: Vec<_> = (0..25)
        .map(|i| add(&mut lib, &format!("b{i}"), 1.0 + f64::from(i)))
        .collect();
    for id in &ids {
        lib.touch_recent(*id).expect("recent");
    }
    lib.touch_recent(ids[3]).expect("again");
    let recents = lib.recents();
    assert_eq!(recents.len(), MAX_RECENTS);
    assert_eq!(recents[0].name, "b3");
    assert_eq!(recents[1].name, "b24");

    lib.set_favourite(ids[1], true).expect("favourite");
    lib.set_favourite(ids[0], true).expect("favourite");
    assert_eq!(names(&lib.favourites()), ["b1", "b0"]);
    lib.set_favourite(ids[1], false).expect("not any more");
    assert_eq!(names(&reopen(&lib).favourites()), ["b0"]);
    assert_eq!(
        lib.set_favourite(BlockUuid::new(), true),
        Err(LibraryError::NotFound)
    );
}
