//! A library kept in the first format (`personal.odpack`, `library.json` and
//! `trash/<n>.odpack`) is read once into the database when there is no
//! database yet, and its files are kept as `*.migrated`. Nothing here writes
//! the old format.

use std::fs;
use std::path::Path;

use rusqlite::{params, Connection};
use serde::Deserialize;

use super::db::{self, Columns};
use super::error::LibraryError;
use super::organise::add_folder;
use crate::blocks_payload::{BlockUuid, BlocksPackage};
use crate::envelope::Envelope;
use crate::oblock::BlockFile;

const PERSONAL: &str = "personal.odpack";
const BACKUP: &str = "personal.bak";
const STATE: &str = "library.json";
const TRASH: &str = "trash";

/// `library.json` as the first format wrote it.
#[derive(Debug, Default, Deserialize)]
struct State {
    #[serde(default)]
    folders: Vec<String>,
    #[serde(default)]
    favourites: Vec<BlockUuid>,
    #[serde(default)]
    recents: Vec<BlockUuid>,
    #[serde(default)]
    trash: Vec<TrashRecord>,
}

#[derive(Debug, Deserialize)]
struct TrashRecord {
    id: u64,
    uuid: BlockUuid,
    folder: String,
    deleted: u64,
}

fn read_package(path: &Path) -> Result<Option<BlocksPackage>, String> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let envelope = Envelope::read(&bytes).map_err(|e| e.to_string())?;
    BlocksPackage::from_envelope(&envelope)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The block `uuid` of `package` as a block file.
fn block_file(package: &BlocksPackage, uuid: BlockUuid) -> Option<BlockFile> {
    let entry = package.entry(uuid)?;
    let (file, _) = BlockFile::from_block(&package.drawing, entry.block, &entry.tags).ok()?;
    file.with_uuid(uuid).ok()
}

/// Reads the old library in `root`, if there is one, into the new `conn`.
pub(super) fn migrate(
    root: &Path,
    conn: &mut Connection,
    notices: &mut Vec<String>,
) -> Result<(), LibraryError> {
    let (personal, backup) = (root.join(PERSONAL), root.join(BACKUP));
    if !personal.exists() && !backup.exists() {
        return Ok(());
    }
    let package = match read_package(&personal) {
        Ok(Some(package)) => package,
        _ => match read_package(&backup) {
            Ok(Some(package)) => package,
            Ok(None) | Err(_) => {
                notices.push(format!(
                    "Your old block library ({PERSONAL}) could not be read, so it was not moved into the new library. The files were left as they are."
                ));
                return Ok(());
            }
        },
    };
    let state: State = fs::read(root.join(STATE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default();

    let tx = conn.transaction()?;
    let mut moved = 0;
    let mut lost = 0;
    for entry in package.entries.iter().filter(|e| !e.component) {
        let Some(file) = block_file(&package, entry.uuid) else {
            lost += 1;
            continue;
        };
        match Columns::of(&file).and_then(|c| db::insert_block(&tx, &c, &entry.folder)) {
            Ok(()) => moved += 1,
            Err(_) => lost += 1,
        }
    }
    for (order, id) in state.favourites.iter().enumerate() {
        tx.execute(
            "UPDATE blocks SET favourite = ?2 WHERE id = ?1",
            params![id.to_string(), order as i64 + 1],
        )?;
    }
    let known = |id: &BlockUuid| package.entry(*id).is_some_and(|e| !e.component);
    for (position, id) in state.recents.iter().filter(|id| known(id)).enumerate() {
        tx.execute(
            "INSERT INTO recents (position, id) VALUES (?1, ?2)",
            params![position as i64, id.to_string()],
        )?;
    }
    for path in &state.folders {
        if crate::blocks_payload::folder::check_path(
            path,
            crate::blocks_payload::folder::MAX_LIBRARY_DEPTH,
        )
        .is_ok()
        {
            add_folder(&tx, path)?;
        }
    }
    for record in &state.trash {
        let file = read_package(&root.join(TRASH).join(format!("{}.odpack", record.id)))
            .ok()
            .flatten()
            .and_then(|small| block_file(&small, record.uuid));
        let Some(file) = file else {
            lost += 1;
            continue;
        };
        let (Ok(data), Some(def)) = (file.to_bytes(), file.definition()) else {
            lost += 1;
            continue;
        };
        tx.execute(
            "INSERT INTO trash (id, uuid, name, folder, deleted, data) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                record.id as i64,
                record.uuid.to_string(),
                def.name,
                record.folder,
                record.deleted as i64,
                data
            ],
        )?;
    }
    tx.commit()?;

    for name in [PERSONAL, BACKUP, STATE, TRASH] {
        let from = root.join(name);
        if from.exists() {
            let _ = fs::rename(&from, root.join(format!("{name}.migrated")));
        }
    }
    let mut said = format!(
        "Your block library moved into {} ({moved} block{}). The old files are kept with the ending .migrated.",
        db::FILE,
        if moved == 1 { "" } else { "s" }
    );
    if lost > 0 {
        said.push_str(&format!(
            " {lost} item{} could not be moved and stays in the old files.",
            if lost == 1 { "" } else { "s" }
        ));
    }
    notices.push(said);
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::blocks_payload::{Entry, KIND, KIND_VERSION};
    use crate::envelope::header::RawHeader;
    use crate::envelope::{Header, Licence};
    use crate::library::Library;
    use crate::test_support::{drawing_with, drawing_with_nested};

    fn header(revision: u32) -> Header {
        Header::check(&RawHeader {
            name: "local/personal".into(),
            version: format!("0.0.{revision}"),
            title: "My blocks".into(),
            summary: "Blocks kept in your own library".into(),
            description: String::new(),
            kind: KIND.into(),
            kind_version: KIND_VERSION,
            license: Licence::Personal.spdx().into(),
            authors: Vec::new(),
            keywords: Vec::new(),
            repository: None,
            copyright: None,
            thumbnail: None,
        })
        .expect("a header")
    }

    fn write(path: &Path, package: &BlocksPackage) {
        let bytes = package
            .to_envelope(header(1))
            .expect("envelope")
            .to_bytes()
            .expect("bytes");
        fs::write(path, bytes).expect("writes");
    }

    /// An old-format library holding "bolt" (which nests "nut", a component)
    /// and "valve", with a Trash item, a favourite, a recent and a folder.
    fn old_library(root: &Path) -> (BlockUuid, BlockUuid, BlockUuid) {
        fs::create_dir_all(root.join(TRASH)).expect("dir");
        let (mut doc, bolt, nut) = drawing_with_nested("bolt", "nut");
        let valve = crate::test_support::add_block(&mut doc, "valve", "Symbols", 7.0, &[]);
        let entry = |block, folder: &str, component| Entry {
            uuid: BlockUuid::new(),
            block,
            folder: folder.into(),
            tags: if component {
                Vec::new()
            } else {
                vec!["metal".into()]
            },
            component,
        };
        let entries = vec![
            entry(bolt, "Fixings", false),
            entry(nut, "", true),
            entry(valve.id, "", false),
        ];
        let (bolt_id, valve_id) = (entries[0].uuid, entries[2].uuid);
        write(
            &root.join(PERSONAL),
            &BlocksPackage {
                drawing: doc,
                entries,
            },
        );

        let (gone_doc, gone_block) = drawing_with("gone", 3.0);
        let gone = Entry {
            uuid: BlockUuid::new(),
            block: gone_block,
            folder: "Old".into(),
            tags: Vec::new(),
            component: false,
        };
        let gone_id = gone.uuid;
        write(
            &root.join(TRASH).join("4.odpack"),
            &BlocksPackage {
                drawing: gone_doc,
                entries: vec![gone],
            },
        );
        let state = json!({
            "version": 0,
            "folders": ["Empty/Inside"],
            "favourites": [valve_id.to_string(), bolt_id.to_string()],
            "recents": [bolt_id.to_string()],
            "trash": [{"id": 4, "uuid": gone_id.to_string(), "name": "gone",
                       "folder": "Old", "deleted": 123}],
            "next_trash": 5
        });
        fs::write(root.join(STATE), state.to_string()).expect("state");
        (bolt_id, valve_id, gone_id)
    }

    #[test]
    fn an_old_library_is_read_once_with_everything_the_user_decided() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().join("Library");
        let (bolt, valve, gone) = old_library(&root);

        let lib = Library::open(&root).expect("opens");
        let names: Vec<_> = lib.blocks().into_iter().map(|b| b.name).collect();
        assert_eq!(names, ["valve", "bolt"], "by folder, then name");
        let bolt_info = lib.block(bolt).expect("bolt");
        assert_eq!(bolt_info.folder, "Fixings");
        assert_eq!(bolt_info.tags, ["metal"]);
        assert_eq!(bolt_info.components, ["nut"]);
        assert!(lib.folders().contains(&"Empty/Inside".to_owned()));
        let favourites: Vec<_> = lib.favourites().into_iter().map(|b| b.id).collect();
        assert_eq!(favourites, [valve, bolt], "in the order they were made");
        assert_eq!(lib.recents().len(), 1);
        let trash = lib.trash_items();
        assert_eq!(
            (trash.len(), trash[0].uuid, trash[0].deleted),
            (1, gone, 123)
        );
        assert!(lib.source(bolt).is_some());
        assert!(lib.notices()[0].contains("moved into"));

        // The old files are kept, renamed, and a second opening reads the database.
        assert!(root.join("personal.odpack.migrated").exists());
        assert!(root.join("library.json.migrated").exists());
        assert!(root.join("trash.migrated").is_dir());
        assert!(!root.join(PERSONAL).exists());
        let again = Library::open(&root).expect("reopens");
        assert_eq!(again.blocks().len(), 2);
        assert!(again.notices().is_empty());
    }

    #[test]
    fn an_old_library_that_cannot_be_read_is_left_alone() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().join("Library");
        fs::create_dir_all(&root).expect("dir");
        fs::write(root.join(PERSONAL), b"not a package").expect("damages");
        let lib = Library::open(&root).expect("opens empty");
        assert!(lib.blocks().is_empty());
        assert!(lib.notices()[0].contains("could not be read"));
        assert_eq!(
            fs::read(root.join(PERSONAL)).expect("kept"),
            b"not a package"
        );
    }

    #[test]
    fn an_old_backup_is_used_when_the_old_file_is_gone() {
        let dir = tempfile::tempdir().expect("dir");
        let root = dir.path().join("Library");
        old_library(&root);
        fs::rename(root.join(PERSONAL), root.join(BACKUP)).expect("renames");
        let lib = Library::open(&root).expect("opens");
        assert_eq!(lib.blocks().len(), 2);
    }
}
