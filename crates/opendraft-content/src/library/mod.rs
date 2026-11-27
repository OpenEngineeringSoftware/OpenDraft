//! The local library (specification 10): the user's own blocks, kept in one
//! SQLite file in a folder and nothing else. No network, no account.
//!
//! ```text
//! Library/
//!   library.sqlite      the blocks (each as its `.oblock` bytes, with the
//!                       columns the window lists and searches), the folders the
//!                       user made, favourites, recents and the Trash
//!   library.sqlite.bak  a copy, made when the library changed since the last
//!   *.migrated          a library kept in the first format, read once
//! ```
//!
//! A block is stored whole: the blocks it nests are inside its own bytes, so a
//! block can be read, exported or deleted without touching another. Every
//! change is one transaction, and what the window sees is read again only once
//! it is written, so a failed write leaves the file and the library in memory
//! as they were.

use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::Connection;

use crate::blocks_payload::BlockUuid;
use crate::oblock::BlockFile;

mod capture;
mod db;
mod error;
mod import;
mod info;
mod legacy;
mod organise;
mod search;
mod state;
mod trash;

pub use capture::{CaptureRequest, Captured, NameTaken};
pub use error::LibraryError;
pub use import::{importable, Candidate, ImportReport};
pub use info::BlockInfo;
pub use search::{fold, Query};
pub use state::{TrashRecord, MAX_RECENTS, TRASH_DAYS};

use db::Index;

/// A library open on a folder.
#[derive(Debug)]
pub struct Library {
    root: PathBuf,
    conn: Connection,
    index: Index,
    revision: u32,
    notices: Vec<String>,
}

impl Library {
    /// Opens the library in `root`, making the folder if it is not there.
    /// What had to be put right on the way is in [`Library::notices`].
    pub fn open(root: &Path) -> Result<Library, LibraryError> {
        fs::create_dir_all(root)?;
        let mut notices = Vec::new();
        let fresh = prepare(root, &mut notices)?;
        let mut conn = db::open(&root.join(db::FILE))?;
        if fresh {
            legacy::migrate(root, &mut conn, &mut notices)?;
        }
        let index = db::load(&conn)?;
        db::refresh_backup(root, &conn);
        Ok(Library {
            root: root.to_owned(),
            conn,
            index,
            revision: 0,
            notices,
        })
    }

    /// Rises with every change to the blocks themselves (not to favourites,
    /// recents or folders), so a picture of a block can be kept until it does.
    pub fn revision(&self) -> u32 {
        self.revision
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Things the user should be told about opening the library (a damaged file
    /// that was replaced by its backup, say).
    pub fn notices(&self) -> &[String] {
        &self.notices
    }

    /// The drawing that holds a block and the block in it: the source for
    /// inserting (`Document::bring_block`). It is read from the file each
    /// time, so the library keeps no block's objects in memory.
    pub fn source(
        &self,
        id: BlockUuid,
    ) -> Option<(opendraft_core::Document, opendraft_core::BlockId)> {
        let file = db::read_file(&self.conn, id).ok()?;
        Some((file.drawing().clone(), file.root()))
    }

    /// A block as a block file, to export it or to look at what it holds.
    pub fn block_file(&self, id: BlockUuid) -> Result<BlockFile, LibraryError> {
        db::read_file(&self.conn, id)
    }

    /// Makes `f`'s change in one transaction and, only when that was written,
    /// reads the index again. On an error nothing changes, in memory or in the
    /// file. `blocks_changed` says whether the blocks themselves changed, which
    /// raises the revision (once, however many blocks).
    pub(super) fn edit<T>(
        &mut self,
        blocks_changed: bool,
        f: impl FnOnce(&rusqlite::Transaction, &Index) -> Result<T, LibraryError>,
    ) -> Result<T, LibraryError> {
        let tx = self.conn.transaction()?;
        let value = f(&tx, &self.index)?;
        tx.commit()?;
        self.index = db::load(&self.conn)?;
        if blocks_changed {
            self.revision = self.revision.saturating_add(1);
        }
        Ok(value)
    }

    #[cfg(test)]
    pub(crate) fn connection(&self) -> &Connection {
        &self.conn
    }
}

/// Makes sure the library file, if there is one, can be used: a damaged one is
/// moved aside and the backup put in its place, and when neither can be read
/// the library is not opened, so nothing is lost by opening it. Returns whether
/// there is no library yet.
fn prepare(root: &Path, notices: &mut Vec<String>) -> Result<bool, LibraryError> {
    let (main, backup) = (root.join(db::FILE), root.join(db::BACKUP));
    let usable_backup = || backup.exists().then(|| db::check(&backup));
    match (main.exists(), usable_backup()) {
        (true, _) => match db::check(&main) {
            Ok(()) => Ok(false),
            Err(db::Fault::Foreign(why)) => Err(LibraryError::Unsupported(format!(
                "{}: {why}. It was not changed.",
                db::FILE
            ))),
            Err(db::Fault::Damaged(why)) => match usable_backup() {
                Some(Ok(())) => {
                    let damaged = root.join(db::DAMAGED);
                    let _ = fs::remove_file(&damaged);
                    fs::rename(&main, &damaged)?;
                    fs::copy(&backup, &main)?;
                    notices.push(format!(
                        "Your block library was damaged ({why}); it was restored from its backup. The damaged file is kept as {}.",
                        db::DAMAGED
                    ));
                    Ok(false)
                }
                Some(Err(also)) => Err(LibraryError::Corrupt(format!(
                    "{} {why}, and the backup {also}. Nothing was changed.",
                    db::FILE
                ))),
                None => Err(LibraryError::Corrupt(format!(
                    "{} {why}, and there is no backup. Nothing was changed.",
                    db::FILE
                ))),
            },
        },
        (false, Some(Ok(()))) => {
            fs::copy(&backup, &main)?;
            notices.push(
                "Your block library file was missing; it was restored from its backup.".into(),
            );
            Ok(false)
        }
        (false, Some(Err(why))) => Err(LibraryError::Corrupt(format!(
            "{} is missing and its backup {why}. Nothing was changed.",
            db::FILE
        ))),
        (false, None) => Ok(true),
    }
}

#[cfg(test)]
mod tests;
