//! The library's SQLite file: the schema, opening it safely, and reading it
//! into the index the rest of the library works from.
//!
//! One file holds everything (`library.sqlite`): a row for each block with its
//! `.oblock` bytes and the columns the window lists and searches, the folders
//! the user made, the recents, and the Trash. A change is one transaction, so
//! it is whole or not there; the journal is the default one (a rollback
//! journal beside the file while a change is made, nothing left after), not
//! WAL, so a synced folder never sees a half-written sidecar.

use std::fs;
use std::path::Path;

use opendraft_core::{Entity, InsertUnits};
use rusqlite::{params, Connection, OpenFlags, Transaction};

use super::error::LibraryError;
use super::state::TrashRecord;
use super::BlockInfo;
use crate::blocks_payload::BlockUuid;
use crate::oblock::BlockFile;

pub const FILE: &str = "library.sqlite";
pub const BACKUP: &str = "library.sqlite.bak";
pub const DAMAGED: &str = "library.sqlite.damaged";

/// `PRAGMA application_id`: "ODBL".
const APPLICATION_ID: i64 = 0x4F44_424C;
/// `PRAGMA user_version`. 0 is the development phase: the schema is changed in
/// place and an older file is not converted.
const SCHEMA_VERSION: i64 = 0;

const SCHEMA: &str = "
CREATE TABLE blocks (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    name_key    TEXT NOT NULL UNIQUE,
    description TEXT NOT NULL,
    tags        TEXT NOT NULL,
    folder      TEXT NOT NULL,
    unit        INTEGER NOT NULL,
    dynamic     INTEGER NOT NULL,
    attributes  INTEGER NOT NULL,
    favourite   INTEGER NOT NULL DEFAULT 0,
    components  TEXT NOT NULL,
    data        BLOB NOT NULL
);
CREATE TABLE folders (path TEXT PRIMARY KEY);
CREATE TABLE recents (position INTEGER PRIMARY KEY, id TEXT NOT NULL);
CREATE TABLE trash (
    id      INTEGER PRIMARY KEY,
    uuid    TEXT NOT NULL,
    name    TEXT NOT NULL,
    folder  TEXT NOT NULL,
    deleted INTEGER NOT NULL,
    data    BLOB NOT NULL
);
";

/// What is kept in memory of the file: everything but the blocks' bytes. It is
/// read again after each change that was written.
#[derive(Debug, Default)]
pub(crate) struct Index {
    pub blocks: Vec<BlockInfo>,
    /// The folders the user made, so one can exist with nothing in it.
    pub folders: Vec<String>,
    /// In the order they were made favourites.
    pub favourites: Vec<BlockUuid>,
    /// Most recent first.
    pub recents: Vec<BlockUuid>,
    pub trash: Vec<TrashRecord>,
}

fn pragmas(conn: &Connection) -> rusqlite::Result<()> {
    conn.busy_timeout(std::time::Duration::from_secs(5))?;
    conn.pragma_update(None, "synchronous", "FULL")?;
    conn.pragma_update(None, "journal_mode", "DELETE")?;
    Ok(())
}

/// Opens the file at `path` for work, making the schema when it is a new one.
pub(super) fn open(path: &Path) -> Result<Connection, LibraryError> {
    let conn = Connection::open(path)?;
    pragmas(&conn)?;
    let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    let application: i64 = conn.pragma_query_value(None, "application_id", |r| r.get(0))?;
    if version == 0 && application == 0 {
        let tables: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type = 'table'",
            [],
            |r| r.get(0),
        )?;
        if tables == 0 {
            conn.execute_batch(SCHEMA)?;
            conn.pragma_update(None, "application_id", APPLICATION_ID)?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
    }
    Ok(conn)
}

/// Why a file is not a library the app can use.
#[derive(Debug)]
pub(super) enum Fault {
    /// It is broken: the backup may stand in for it.
    Damaged(String),
    /// It is whole but not ours to change (another version, or not a block
    /// library at all): never replaced.
    Foreign(String),
}

impl std::fmt::Display for Fault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (Fault::Damaged(m) | Fault::Foreign(m)) = self;
        write!(f, "{m}")
    }
}

/// Whether the file at `path` is a library the app can use, and if not, why.
/// It is only read.
pub(super) fn check(path: &Path) -> Result<(), Fault> {
    let damaged = |e: rusqlite::Error| Fault::Damaged(e.to_string());
    let conn =
        Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(damaged)?;
    let verdict: String = conn
        .query_row("PRAGMA quick_check", [], |r| r.get(0))
        .map_err(damaged)?;
    if verdict != "ok" {
        return Err(Fault::Damaged(verdict));
    }
    let application: i64 = conn
        .pragma_query_value(None, "application_id", |r| r.get(0))
        .map_err(damaged)?;
    let version: i64 = conn
        .pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(damaged)?;
    if application != APPLICATION_ID {
        return Err(Fault::Foreign("it is not a block library".into()));
    }
    if version != SCHEMA_VERSION {
        return Err(Fault::Foreign(format!(
            "it is library version {version}; this app reads {SCHEMA_VERSION}"
        )));
    }
    Ok(())
}

/// Keeps a copy of the library beside it, made only when the library changed
/// since the last copy. The copy is whole before it takes the place of the
/// old one.
pub(super) fn refresh_backup(root: &Path, conn: &Connection) {
    let (main, backup) = (root.join(FILE), root.join(BACKUP));
    let modified = |p: &Path| fs::metadata(p).and_then(|m| m.modified()).ok();
    if let (Some(a), Some(b)) = (modified(&main), modified(&backup)) {
        if b >= a {
            return;
        }
    }
    let temp = root.join(format!("{BACKUP}.tmp"));
    let _ = fs::remove_file(&temp);
    let made = conn
        .execute("VACUUM INTO ?1", params![temp.to_string_lossy()])
        .is_ok()
        && check(&temp).is_ok()
        && fs::rename(&temp, &backup).is_ok();
    if !made {
        let _ = fs::remove_file(&temp);
    }
}

/// What a block row holds, made from the block's file.
pub(super) struct Columns {
    pub id: BlockUuid,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    pub unit: InsertUnits,
    pub dynamic: bool,
    pub has_attributes: bool,
    pub components: Vec<String>,
    pub data: Vec<u8>,
}

impl Columns {
    pub fn of(file: &BlockFile) -> Result<Columns, LibraryError> {
        let def = file
            .definition()
            .ok_or_else(|| LibraryError::Package("the block is not in its file".into()))?;
        let mut components = Vec::new();
        let mut seen = std::collections::HashSet::new();
        collect_components(file, def.id, &mut seen, &mut components);
        let data = file
            .to_bytes()
            .map_err(|e| LibraryError::Package(e.to_string()))?;
        Ok(Columns {
            id: file.uuid(),
            name: def.name.clone(),
            description: def.description.clone(),
            tags: file.tags().to_vec(),
            unit: def.unit,
            dynamic: def.is_dynamic(),
            has_attributes: def
                .entities
                .iter()
                .any(|e| matches!(e.entity, Entity::AttributeDefinition(_))),
            components,
            data,
        })
    }
}

fn collect_components(
    file: &BlockFile,
    block: opendraft_core::BlockId,
    seen: &mut std::collections::HashSet<opendraft_core::BlockId>,
    out: &mut Vec<String>,
) {
    let Some(def) = file.drawing().block(block) else {
        return;
    };
    for inner in def.nested_blocks() {
        if seen.insert(inner) {
            if let Some(d) = file.drawing().block(inner) {
                out.push(d.name.clone());
            }
            collect_components(file, inner, seen, out);
        }
    }
}

fn json(list: &[String]) -> String {
    serde_json::to_string(list).unwrap_or_else(|_| "[]".into())
}

fn unjson(text: &str) -> Vec<String> {
    serde_json::from_str(text).unwrap_or_default()
}

/// The key a name is unique by.
pub(super) fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// Adds a block's row.
pub(super) fn insert_block(
    tx: &Transaction,
    columns: &Columns,
    folder: &str,
) -> Result<(), LibraryError> {
    tx.execute(
        "INSERT INTO blocks (id, name, name_key, description, tags, folder, unit, dynamic, \
         attributes, favourite, components, data) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, ?10, ?11)",
        params![
            columns.id.to_string(),
            columns.name,
            name_key(&columns.name),
            columns.description,
            json(&columns.tags),
            folder,
            columns.unit.code(),
            columns.dynamic,
            columns.has_attributes,
            json(&columns.components),
            columns.data,
        ],
    )?;
    Ok(())
}

/// Replaces what a block's file says (its name, description, tags, bytes),
/// keeping its folder and whether it is a favourite.
pub(super) fn update_block(tx: &Transaction, columns: &Columns) -> Result<(), LibraryError> {
    let changed = tx.execute(
        "UPDATE blocks SET name = ?2, name_key = ?3, description = ?4, tags = ?5, unit = ?6, \
         dynamic = ?7, attributes = ?8, components = ?9, data = ?10 WHERE id = ?1",
        params![
            columns.id.to_string(),
            columns.name,
            name_key(&columns.name),
            columns.description,
            json(&columns.tags),
            columns.unit.code(),
            columns.dynamic,
            columns.has_attributes,
            json(&columns.components),
            columns.data,
        ],
    )?;
    if changed == 0 {
        return Err(LibraryError::NotFound);
    }
    Ok(())
}

/// The block's file, read from its row.
pub(super) fn read_file(conn: &Connection, id: BlockUuid) -> Result<BlockFile, LibraryError> {
    let data: Option<Vec<u8>> = conn
        .query_row(
            "SELECT data FROM blocks WHERE id = ?1",
            params![id.to_string()],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    let data = data.ok_or(LibraryError::NotFound)?;
    BlockFile::read(&data)
        .map_err(|e| LibraryError::Corrupt(format!("the block {id} cannot be read: {e}")))
}

/// Whether a block of this name (any case) is there, and its id.
pub(super) fn find_by_name(
    tx: &Transaction,
    name: &str,
) -> Result<Option<BlockUuid>, LibraryError> {
    let found: Option<String> = tx
        .query_row(
            "SELECT id FROM blocks WHERE name_key = ?1",
            params![name_key(name)],
            |r| r.get(0),
        )
        .map(Some)
        .or_else(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => Ok(None),
            other => Err(other),
        })?;
    Ok(found.and_then(|text| BlockUuid::parse(&text)))
}

/// Whether a block with this id is there.
pub(super) fn exists(tx: &Transaction, id: BlockUuid) -> Result<bool, LibraryError> {
    let count: i64 = tx.query_row(
        "SELECT count(*) FROM blocks WHERE id = ?1",
        params![id.to_string()],
        |r| r.get(0),
    )?;
    Ok(count > 0)
}

/// Reads the index from the file.
pub(super) fn load(conn: &Connection) -> Result<Index, LibraryError> {
    let mut index = Index::default();
    let mut favourites: Vec<(i64, BlockUuid)> = Vec::new();
    let mut stmt = conn.prepare(
        "SELECT id, name, description, tags, folder, unit, dynamic, attributes, favourite, \
         components FROM blocks",
    )?;
    let rows = stmt.query_map([], |r| {
        Ok((
            r.get::<_, String>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, String>(4)?,
            r.get::<_, i64>(5)?,
            r.get::<_, bool>(6)?,
            r.get::<_, bool>(7)?,
            r.get::<_, i64>(8)?,
            r.get::<_, String>(9)?,
        ))
    })?;
    for row in rows {
        let (id, name, description, tags, folder, unit, dynamic, attributes, favourite, parts) =
            row?;
        let Some(id) = BlockUuid::parse(&id) else {
            return Err(LibraryError::Corrupt(format!("{id:?} is not a block id")));
        };
        if favourite > 0 {
            favourites.push((favourite, id));
        }
        index.blocks.push(BlockInfo {
            id,
            name,
            description,
            tags: unjson(&tags),
            folder,
            unit: InsertUnits::from_code(unit).unwrap_or(InsertUnits::UNITLESS),
            dynamic,
            has_attributes: attributes,
            favourite: favourite > 0,
            components: unjson(&parts),
        });
    }
    favourites.sort_by_key(|(order, _)| *order);
    index.favourites = favourites.into_iter().map(|(_, id)| id).collect();

    let mut stmt = conn.prepare("SELECT path FROM folders")?;
    index.folders = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;

    let mut stmt = conn.prepare("SELECT id FROM recents ORDER BY position")?;
    let recents: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    index.recents = recents.iter().filter_map(|t| BlockUuid::parse(t)).collect();

    let mut stmt = conn.prepare("SELECT id, uuid, name, folder, deleted FROM trash")?;
    let items = stmt.query_map([], |r| {
        Ok((
            r.get::<_, i64>(0)?,
            r.get::<_, String>(1)?,
            r.get::<_, String>(2)?,
            r.get::<_, String>(3)?,
            r.get::<_, i64>(4)?,
        ))
    })?;
    for item in items {
        let (id, uuid, name, folder, deleted) = item?;
        let Some(uuid) = BlockUuid::parse(&uuid) else {
            continue;
        };
        index.trash.push(TrashRecord {
            id: u64::try_from(id).unwrap_or(0),
            uuid,
            name,
            folder,
            deleted: u64::try_from(deleted).unwrap_or(0),
        });
    }
    Ok(index)
}
