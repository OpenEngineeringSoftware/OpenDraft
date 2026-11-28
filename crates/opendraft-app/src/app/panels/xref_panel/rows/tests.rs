use opendraft_core::{XrefDef, XrefNested, XrefPathType, XrefReport, XrefStamp};

use super::*;

fn record(stamp: Option<XrefStamp>) -> XrefDef {
    XrefDef {
        block: BlockId(0),
        path: "base.dwg".to_owned(),
        path_type: XrefPathType::Relative,
        kind: XrefKind::Attach,
        unloaded: false,
        stamp,
        layers: Vec::new(),
        blocks: Vec::new(),
        clip: None,
        report: XrefReport::default(),
    }
}

#[test]
fn dates_are_read_in_utc() {
    assert_eq!(date(0), "1970-01-01 00:00");
    assert_eq!(date(1_700_000_000), "2023-11-14 22:13");
}

#[test]
fn sizes_use_the_unit_that_fits() {
    assert_eq!(size(3), "3 B");
    assert_eq!(size(2048), "2.0 KB");
    assert_eq!(size(3 * 1024 * 1024), "3.0 MB");
}

#[test]
fn health_follows_the_record_the_inserts_and_the_source() {
    let dir = std::env::temp_dir().join(format!("od-xref-panel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("base.dwg");
    std::fs::write(&file, b"abc").unwrap();
    let meta = std::fs::metadata(&file).unwrap();
    let when = modified(&meta).unwrap_or(0);

    let fresh = record(Some(XrefStamp {
        length: 3,
        modified: when,
    }));
    assert_eq!(
        health(&fresh, false, true, true, Some(&meta)),
        Health::Loaded
    );
    assert_eq!(health(&fresh, true, true, false, None), Health::Unresolved);
    assert_eq!(
        health(&fresh, false, true, true, None),
        Health::ShowingSaved
    );
    assert_eq!(health(&fresh, true, true, true, None), Health::NotFound);
    assert_eq!(
        health(&fresh, false, false, true, Some(&meta)),
        Health::Unreferenced
    );

    let mut unloaded = fresh.clone();
    unloaded.unloaded = true;
    assert_eq!(health(&unloaded, true, true, true, None), Health::Unloaded);

    let changed = record(Some(XrefStamp {
        length: 4,
        modified: when,
    }));
    assert_eq!(
        health(&changed, false, true, true, Some(&meta)),
        Health::Changed
    );
    assert_eq!(modified(&meta), Some(when));
    let _ = std::fs::remove_dir_all(dir);
}

fn row(name: &str, parent: Option<usize>, size: Option<u64>) -> Row {
    Row {
        key: RowKey {
            block: BlockId(0),
            nested: parent.map(|_| 0),
        },
        parent,
        depth: usize::from(parent.is_some()),
        name: name.to_owned(),
        health: Health::Loaded,
        size,
        kind: XrefKind::Overlay,
        date: None,
        path: String::new(),
        found: String::new(),
    }
}

#[test]
fn the_list_is_flat_and_sorted_by_the_column_either_way() {
    let rows = [
        row("site", None, Some(30)),
        row("Walls", Some(0), Some(10)),
        row("util", None, Some(20)),
    ];
    let none = HashSet::new();
    let by_name = Sort::default();
    assert_eq!(order(&rows, false, by_name, &none), [0, 2, 1]);
    let by_size = by_name.clicked(Column::Size);
    assert_eq!(order(&rows, false, by_size, &none), [1, 2, 0]);
    let by_size = by_size.clicked(Column::Size);
    assert_eq!(order(&rows, false, by_size, &none), [0, 2, 1]);
}

#[test]
fn the_tree_puts_each_reference_before_what_its_source_attaches() {
    let rows = [
        row("site", None, None),
        row("walls", Some(0), None),
        row("alpha", None, None),
        row("floors", Some(1), None),
    ];
    let mut collapsed = HashSet::new();
    assert_eq!(
        order(&rows, true, Sort::default(), &collapsed),
        [2, 0, 1, 3]
    );
    collapsed.insert(rows[1].key);
    assert_eq!(order(&rows, true, Sort::default(), &collapsed), [2, 0, 1]);
    assert!(has_children(&rows, 0) && !has_children(&rows, 3));
}

#[test]
fn nested_references_become_rows_under_their_parents() {
    let nested = |name: &str, parent: Option<usize>, loaded: bool| XrefNested {
        name: name.to_owned(),
        parent,
        path: format!("{name}.dwg"),
        kind: XrefKind::Attach,
        found: if loaded {
            format!("/x/{name}.dwg")
        } else {
            String::new()
        },
        stamp: loaded.then_some(XrefStamp {
            length: 5,
            modified: 7,
        }),
        loaded,
    };
    let mut doc = Document::default();
    doc.push_block(std::sync::Arc::new(opendraft_core::BlockDef {
        id: BlockId(0),
        name: "site".to_owned(),
        base: opendraft_core::Point::default(),
        description: String::new(),
        unit: Default::default(),
        entities: Vec::new(),
        dynamic: None,
    }));
    let mut xref = record(None);
    xref.report.nested = vec![
        nested("floors", None, true),
        nested("walls", Some(0), false),
    ];
    doc.set_xrefs(vec![xref]);
    let list = rows(&doc, None);
    assert_eq!(list.len(), 3);
    assert_eq!(
        (list[1].name.as_str(), list[1].parent, list[1].depth),
        ("floors", Some(0), 1)
    );
    assert_eq!(
        (list[2].name.as_str(), list[2].parent, list[2].depth),
        ("walls", Some(1), 2)
    );
    assert_eq!(list[1].health, Health::Loaded);
    assert_eq!(list[2].health, Health::NotLoaded);
    assert_eq!(list[1].size, Some(5));
    assert_eq!(list[1].key.nested, Some(0));
}
