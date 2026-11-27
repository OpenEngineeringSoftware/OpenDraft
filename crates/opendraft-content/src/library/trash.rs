//! The Trash (specification 10.4): a deleted block, folder or replaced block
//! goes to the `trash` table with its bytes, never into a drawing, and stays
//! for 30 days or until the Trash is emptied. Restoring puts it back.

use rusqlite::{params, Transaction};

use super::capture::{add_file, Captured, NameTaken};
use super::error::LibraryError;
use super::organise::add_folder;
use super::state::{TrashRecord, TRASH_DAYS};
use super::Library;
use crate::blocks_payload::folder;
use crate::blocks_payload::BlockUuid;
use crate::oblock::BlockFile;

const DAY: u64 = 24 * 60 * 60;

/// Moves the block's row to the Trash, whole, and forgets it as a recent.
pub(super) fn trash_row(
    tx: &Transaction,
    id: BlockUuid,
    now: u64,
) -> Result<TrashRecord, LibraryError> {
    let (name, folder): (String, String) = tx
        .query_row(
            "SELECT name, folder FROM blocks WHERE id = ?1",
            params![id.to_string()],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .map_err(|e| match e {
            rusqlite::Error::QueryReturnedNoRows => LibraryError::NotFound,
            other => other.into(),
        })?;
    tx.execute(
        "INSERT INTO trash (uuid, name, folder, deleted, data) \
         SELECT id, name, folder, ?2, data FROM blocks WHERE id = ?1",
        params![id.to_string(), now as i64],
    )?;
    let trash_id = tx.last_insert_rowid();
    tx.execute("DELETE FROM blocks WHERE id = ?1", params![id.to_string()])?;
    tx.execute("DELETE FROM recents WHERE id = ?1", params![id.to_string()])?;
    Ok(TrashRecord {
        id: u64::try_from(trash_id).unwrap_or(0),
        uuid: id,
        name,
        folder,
        deleted: now,
    })
}

impl Library {
    /// Moves a block to the Trash.
    pub fn delete_block(&mut self, id: BlockUuid, now: u64) -> Result<(), LibraryError> {
        self.edit(true, |tx, _| trash_row(tx, id, now).map(|_| ()))
    }

    /// Moves every block in a folder, and in the folders inside it, to the
    /// Trash and removes the folders.
    pub fn delete_folder(&mut self, path: &str, now: u64) -> Result<(), LibraryError> {
        if path.is_empty() || !self.folders().iter().any(|f| folder::same(f, path)) {
            return Err(LibraryError::NotFound);
        }
        self.edit(true, |tx, index| {
            for block in index
                .blocks
                .iter()
                .filter(|b| folder::is_within(&b.folder, path))
            {
                trash_row(tx, block.id, now)?;
            }
            for made in index.folders.iter().filter(|f| folder::is_within(f, path)) {
                tx.execute("DELETE FROM folders WHERE path = ?1", params![made])?;
            }
            Ok(())
        })
    }

    /// Puts a Trash item back where it was, with its id.
    pub fn restore(
        &mut self,
        trash: u64,
        if_taken: NameTaken,
        now: u64,
    ) -> Result<Captured, LibraryError> {
        self.edit(true, |tx, index| {
            let record = index
                .trash
                .iter()
                .find(|t| t.id == trash)
                .cloned()
                .ok_or(LibraryError::NotFound)?;
            let data: Vec<u8> = tx.query_row(
                "SELECT data FROM trash WHERE id = ?1",
                params![trash as i64],
                |r| r.get(0),
            )?;
            let file = BlockFile::read(&data).map_err(|e| {
                LibraryError::Corrupt(format!("the Trash item {trash} cannot be read: {e}"))
            })?;
            let added = add_file(tx, file, &record.folder, if_taken, now)?;
            add_folder(tx, &record.folder)?;
            tx.execute("DELETE FROM trash WHERE id = ?1", params![trash as i64])?;
            Ok(Captured {
                id: added.id,
                name: added.name,
                replaced: added.replaced,
                warnings: Vec::new(),
            })
        })
    }

    /// Removes items deleted more than 30 days before `now`.
    pub fn purge_trash(&mut self, now: u64) -> Result<usize, LibraryError> {
        self.remove_from_trash(|t| now.saturating_sub(t.deleted) > TRASH_DAYS * DAY)
    }

    pub fn empty_trash(&mut self) -> Result<usize, LibraryError> {
        self.remove_from_trash(|_| true)
    }

    fn remove_from_trash(
        &mut self,
        gone: impl Fn(&TrashRecord) -> bool,
    ) -> Result<usize, LibraryError> {
        self.edit(false, |tx, index| {
            let ids: Vec<u64> = index
                .trash
                .iter()
                .filter(|t| gone(t))
                .map(|t| t.id)
                .collect();
            for id in &ids {
                tx.execute("DELETE FROM trash WHERE id = ?1", params![*id as i64])?;
            }
            Ok(ids.len())
        })
    }
}
