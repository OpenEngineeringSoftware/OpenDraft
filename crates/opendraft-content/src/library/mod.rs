//! The local library (specification 10): the user's own blocks, kept in a
//! folder of files and nothing else. No network, no account.
//!
//! ```text
//! Library/
//!   personal.odpack   the user's blocks: one `local/personal` package, rewritten
//!                     whole on every change (the one before kept as personal.bak)
//!   library.json      what the user decided: empty folders, favourites, recents
//!   trash/<n>.odpack  deleted blocks, one small package each
//! ```
//!
//! Every change is made on a copy and written before it is kept, so a failed
//! write leaves both the files and the library in memory as they were.

use std::fs;
use std::path::{Path, PathBuf};

use crate::blocks_payload::folder;
use crate::blocks_payload::{BlockUuid, BlocksPackage};

mod capture;
mod error;
mod import;
mod info;
mod organise;
mod search;
mod state;
pub(crate) mod store;
mod trash;

pub use capture::{CaptureRequest, Captured, NameTaken};
pub use error::LibraryError;
pub use import::{importable, Candidate, ImportReport};
pub use info::BlockInfo;
pub use search::{fold, Query};
pub use state::{TrashRecord, MAX_RECENTS, TRASH_DAYS};

use state::State;

/// A library open on a folder.
#[derive(Debug)]
pub struct Library {
    root: PathBuf,
    package: BlocksPackage,
    revision: u32,
    state: State,
    notices: Vec<String>,
}

/// A change in progress, on copies. See [`Library::edit`].
pub(crate) struct Edit {
    pub package: BlocksPackage,
    pub state: State,
    pub root: PathBuf,
    pub package_changed: bool,
    pub state_changed: bool,
    /// Files made along the way, removed again if the edit fails.
    pub created: Vec<PathBuf>,
    /// Files to remove once the edit has been kept.
    pub removed: Vec<PathBuf>,
}

impl Library {
    /// Opens the library in `root`, making the folder if it is not there.
    /// What had to be put right on the way is in [`Library::notices`].
    pub fn open(root: &Path) -> Result<Library, LibraryError> {
        fs::create_dir_all(root.join(store::TRASH))?;
        let mut notices = Vec::new();
        let (package, revision) = open_package(root, &mut notices)?;
        let mut state = open_state(root, &mut notices);
        tidy(&mut state, &package, root);
        Ok(Library {
            root: root.to_owned(),
            package,
            revision,
            state,
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

    /// The drawing that holds every block, and the block a library block is
    /// in it: the source for inserting (`Document::bring_block`).
    pub fn source(
        &self,
        id: BlockUuid,
    ) -> Option<(&opendraft_core::Document, opendraft_core::BlockId)> {
        let entry = self.package.entry(id)?;
        Some((&self.package.drawing, entry.block))
    }

    #[cfg(test)]
    pub(crate) fn package(&self) -> &BlocksPackage {
        &self.package
    }

    /// Makes `f`'s change on copies, writes what it touched and, only when that
    /// worked, keeps it. On an error nothing changes, in memory or on disk.
    pub(crate) fn edit<T>(
        &mut self,
        f: impl FnOnce(&mut Edit) -> Result<T, LibraryError>,
    ) -> Result<T, LibraryError> {
        let mut edit = Edit {
            package: self.package.clone(),
            state: self.state.clone(),
            root: self.root.clone(),
            package_changed: false,
            state_changed: false,
            created: Vec::new(),
            removed: Vec::new(),
        };
        let revision = self.revision.saturating_add(1);
        let result = f(&mut edit).and_then(|value| {
            if edit.package_changed {
                store::save_package(&self.root, &edit.package, revision)?;
            }
            if edit.state_changed {
                store::save_state(&self.root, &edit.state)?;
            }
            Ok(value)
        });
        match result {
            Ok(value) => {
                if edit.package_changed {
                    self.revision = revision;
                }
                self.package = edit.package;
                self.state = edit.state;
                for path in &edit.removed {
                    let _ = fs::remove_file(path);
                }
                Ok(value)
            }
            Err(error) => {
                for path in &edit.created {
                    let _ = fs::remove_file(path);
                }
                Err(error)
            }
        }
    }
}

/// The personal package: the file, else the backup (saying so), else a new
/// empty one. A damaged file is moved aside, never overwritten, and when
/// neither file can be read the library is not opened, so nothing is lost by
/// opening it.
fn open_package(
    root: &Path,
    notices: &mut Vec<String>,
) -> Result<(BlocksPackage, u32), LibraryError> {
    let primary = store::personal(root);
    let backup = root.join(store::BACKUP);
    let why_not = match store::read_package(&primary) {
        Ok(Some(found)) => return Ok(found),
        Ok(None) => None,
        Err(why) => Some(why),
    };
    match (why_not, store::read_package(&backup)) {
        (None, Ok(None)) => Ok((BlocksPackage::empty(), 0)),
        (None, Ok(Some(found))) => {
            notices.push(
                "Your block library file was missing; it was restored from its backup.".into(),
            );
            Ok(found)
        }
        (Some(why), Ok(Some(found))) => {
            let _ = fs::rename(&primary, root.join(store::DAMAGED));
            notices.push(format!(
                "Your block library file was damaged ({why}); it was restored from its backup. The damaged file is kept as {}.",
                store::DAMAGED
            ));
            Ok(found)
        }
        (Some(why), Ok(None)) => Err(LibraryError::Corrupt(format!(
            "{} {why}, and there is no backup. Nothing was changed.",
            store::PERSONAL
        ))),
        (_, Err(why)) => Err(LibraryError::Corrupt(format!(
            "{} cannot be read and the backup {why}. Nothing was changed.",
            store::PERSONAL
        ))),
    }
}

fn open_state(root: &Path, notices: &mut Vec<String>) -> State {
    let path = root.join(store::STATE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(_) => return State::default(),
    };
    match serde_json::from_slice(&bytes) {
        Ok(state) => state,
        Err(why) => {
            let _ = fs::rename(&path, root.join(store::STATE_BAD));
            notices.push(format!(
                "library.json could not be read ({why}); your empty folders, favourites and Trash list were reset. The old file is kept as {}.",
                store::STATE_BAD
            ));
            State::default()
        }
    }
}

/// Drops what refers to nothing: a favourite whose block is gone, a folder
/// name that breaks the rules, a Trash record without its file.
fn tidy(state: &mut State, package: &BlocksPackage, root: &Path) {
    let known = |id: &BlockUuid| package.entry(*id).is_some();
    state.favourites.retain(known);
    state.recents.retain(known);
    state
        .folders
        .retain(|f| !f.is_empty() && folder::check_path(f, folder::MAX_LIBRARY_DEPTH).is_ok());
    state
        .trash
        .retain(|t| store::trash_file(root, t.id).exists());
    state.next_trash = state
        .trash
        .iter()
        .map(|t| t.id + 1)
        .fold(state.next_trash, u64::max);
}

#[cfg(test)]
mod tests;
