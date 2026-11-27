//! Organising: rename, describe, tag, move, folders, favourites, recents
//! (specification 10.4). A block's id never changes.

use rusqlite::{params, Transaction};

use super::capture::{check_description, check_name, check_tags};
use super::db::{self, Columns};
use super::error::LibraryError;
use super::state::MAX_RECENTS;
use super::Library;
use crate::blocks_payload::folder;
use crate::blocks_payload::BlockUuid;

/// Adds `path` and the folders above it that the table does not have yet.
pub(super) fn add_folder(tx: &Transaction, path: &str) -> Result<(), LibraryError> {
    let mut stmt = tx.prepare("SELECT path FROM folders")?;
    let have: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(0))?
        .collect::<Result<_, _>>()?;
    for f in folder::with_ancestors(path) {
        if !have.iter().any(|h| folder::same(h, &f)) {
            tx.execute("INSERT INTO folders (path) VALUES (?1)", params![f])?;
        }
    }
    Ok(())
}

impl Library {
    /// Gives a block a new name. Its id, folder and tags stay.
    pub fn rename_block(&mut self, id: BlockUuid, name: &str) -> Result<(), LibraryError> {
        let name = name.trim().to_owned();
        check_name(&name)?;
        let mut file = db::read_file(&self.conn, id)?;
        self.edit(true, |tx, _| {
            if db::find_by_name(tx, &name)?.is_some_and(|other| other != id) {
                return Err(LibraryError::NameTaken(name));
            }
            file.set_name(&name)?;
            db::update_block(tx, &Columns::of(&file)?)
        })
    }

    pub fn set_description(&mut self, id: BlockUuid, text: &str) -> Result<(), LibraryError> {
        check_description(text)?;
        let mut file = db::read_file(&self.conn, id)?;
        file.set_description(text)?;
        self.edit(true, |tx, _| db::update_block(tx, &Columns::of(&file)?))
    }

    pub fn set_tags(&mut self, id: BlockUuid, tags: Vec<String>) -> Result<(), LibraryError> {
        check_tags(&tags)?;
        let mut file = db::read_file(&self.conn, id)?;
        file.set_tags(tags)?;
        self.edit(true, |tx, _| db::update_block(tx, &Columns::of(&file)?))
    }

    /// Puts a block in `folder` (`""` for the top level), making the folder if
    /// it is new.
    pub fn move_block(&mut self, id: BlockUuid, to: &str) -> Result<(), LibraryError> {
        folder::check_path(to, folder::MAX_LIBRARY_DEPTH)?;
        self.edit(false, |tx, _| {
            let changed = tx.execute(
                "UPDATE blocks SET folder = ?2 WHERE id = ?1",
                params![id.to_string(), to],
            )?;
            if changed == 0 {
                return Err(LibraryError::NotFound);
            }
            add_folder(tx, to)
        })
    }

    /// Makes a folder (and the ones above it that are missing).
    pub fn create_folder(&mut self, path: &str) -> Result<(), LibraryError> {
        if path.is_empty() {
            return Err(LibraryError::Folder(folder::FolderError::BadName(
                String::new(),
            )));
        }
        folder::check_path(path, folder::MAX_LIBRARY_DEPTH)?;
        self.edit(false, |tx, _| add_folder(tx, path))
    }

    /// Renames or moves a folder, with everything in it: `to` is its new full
    /// path. Refused when `to` is inside the folder itself, or when a folder
    /// of that name is there already.
    pub fn rename_folder(&mut self, from: &str, to: &str) -> Result<(), LibraryError> {
        if from.is_empty() {
            return Err(LibraryError::Folder(folder::FolderError::BadName(
                String::new(),
            )));
        }
        folder::check_path(to, folder::MAX_LIBRARY_DEPTH)?;
        if to.is_empty() || (folder::is_within(to, from) && !folder::same(to, from)) {
            return Err(LibraryError::Refused(
                "A folder cannot go inside itself.".into(),
            ));
        }
        let before = self.folders();
        if !before.iter().any(|f| folder::same(f, from)) {
            return Err(LibraryError::NotFound);
        }
        if !folder::same(from, to) && before.iter().any(|f| folder::same(f, to)) {
            return Err(LibraryError::Refused(format!(
                "There is already a folder {to}."
            )));
        }
        // Every path the change makes is checked before anything is written.
        let moved: Vec<(BlockUuid, String)> = self
            .index
            .blocks
            .iter()
            .filter_map(|b| folder::reroot(&b.folder, from, to).map(|p| (b.id, p)))
            .collect();
        for (_, path) in &moved {
            folder::check_path(path, folder::MAX_LIBRARY_DEPTH)?;
        }
        self.edit(false, |tx, index| {
            for (id, path) in &moved {
                tx.execute(
                    "UPDATE blocks SET folder = ?2 WHERE id = ?1",
                    params![id.to_string(), path],
                )?;
            }
            for made in &index.folders {
                if let Some(path) = folder::reroot(made, from, to) {
                    tx.execute("DELETE FROM folders WHERE path = ?1", params![made])?;
                    tx.execute(
                        "INSERT OR IGNORE INTO folders (path) VALUES (?1)",
                        params![path],
                    )?;
                }
            }
            add_folder(tx, to)
        })
    }

    pub fn set_favourite(&mut self, id: BlockUuid, favourite: bool) -> Result<(), LibraryError> {
        if self.block(id).is_none() {
            return Err(LibraryError::NotFound);
        }
        self.edit(false, |tx, _| {
            if favourite {
                tx.execute(
                    "UPDATE blocks SET favourite = \
                     (SELECT coalesce(max(favourite), 0) + 1 FROM blocks) WHERE id = ?1",
                    params![id.to_string()],
                )?;
            } else {
                tx.execute(
                    "UPDATE blocks SET favourite = 0 WHERE id = ?1",
                    params![id.to_string()],
                )?;
            }
            Ok(())
        })
    }

    /// Notes that a block was just inserted: it goes to the front of Recent.
    pub fn touch_recent(&mut self, id: BlockUuid) -> Result<(), LibraryError> {
        if self.block(id).is_none() {
            return Err(LibraryError::NotFound);
        }
        self.edit(false, |tx, index| {
            let mut list = vec![id];
            list.extend(index.recents.iter().copied().filter(|r| *r != id));
            list.truncate(MAX_RECENTS);
            tx.execute("DELETE FROM recents", [])?;
            for (position, recent) in list.iter().enumerate() {
                tx.execute(
                    "INSERT INTO recents (position, id) VALUES (?1, ?2)",
                    params![position as i64, recent.to_string()],
                )?;
            }
            Ok(())
        })
    }
}
