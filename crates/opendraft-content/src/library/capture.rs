//! Capture: a block of a drawing becomes a block of the library
//! (specification 10.4). The block and the blocks it nests are copied, with
//! their layers, linetypes and text styles, into a block file of its own; the
//! library's other blocks are never changed by a capture except when the user
//! chose to replace one.

use opendraft_core::block::block_name_problem;
use opendraft_core::{BlockId, Document};
use rusqlite::Transaction;

use super::db::{self, Columns};
use super::error::LibraryError;
use super::trash::trash_row;
use super::Library;
use crate::blocks_payload::folder;
use crate::blocks_payload::{BlockUuid, MAX_DESCRIPTION, MAX_TAG, MAX_TAGS};
use crate::envelope::text::{check_line, check_paragraphs};
use crate::oblock::BlockFile;

/// What to do when the library already has a block of the name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum NameTaken {
    /// Stop and say so.
    #[default]
    Cancel,
    /// Add the new one as `Name_2`.
    KeepBoth,
    /// Move the old one to the Trash and add the new one.
    Replace,
}

#[derive(Clone, Debug, Default)]
pub struct CaptureRequest {
    /// The library name; the block's own when `None`.
    pub name: Option<String>,
    /// The block's own when `None`.
    pub description: Option<String>,
    pub tags: Vec<String>,
    /// Where it goes (`""` for the top level).
    pub folder: String,
    pub if_name_taken: NameTaken,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Captured {
    pub id: BlockUuid,
    pub name: String,
    /// An old block of the name went to the Trash.
    pub replaced: bool,
    /// What did not carry over exactly, worded for the user.
    pub warnings: Vec<String>,
}

impl Library {
    /// Copies block `block` of `source` (and what it nests) into the library.
    /// `now` is seconds since 1970, for the Trash when a block is replaced.
    pub fn capture(
        &mut self,
        source: &Document,
        block: BlockId,
        request: &CaptureRequest,
        now: u64,
    ) -> Result<Captured, LibraryError> {
        check_tags(&request.tags)?;
        if let Some(text) = &request.description {
            check_description(text)?;
        }
        folder::check_path(&request.folder, folder::MAX_LIBRARY_DEPTH)?;
        let (mut file, warnings) = BlockFile::from_block(source, block, &request.tags)?;
        if let Some(name) = &request.name {
            check_name(name.trim())?;
            file.set_name(name)?;
        }
        if let Some(text) = &request.description {
            file.set_description(text)?;
        }
        let (folder, if_taken) = (request.folder.clone(), request.if_name_taken);
        self.edit(true, |tx, _| {
            let added = add_file(tx, file, &folder, if_taken, now)?;
            Ok(Captured {
                id: added.id,
                name: added.name,
                replaced: added.replaced,
                warnings,
            })
        })
    }
}

pub(super) fn check_tags(tags: &[String]) -> Result<(), LibraryError> {
    let bad = |t: &str| LibraryError::InvalidName(format!("{t:?} is not a valid tag."));
    if tags.len() > MAX_TAGS {
        return Err(LibraryError::InvalidName(format!(
            "A block can have at most {MAX_TAGS} tags."
        )));
    }
    for tag in tags {
        if check_line(tag, MAX_TAG).is_err() || tag.trim() != tag {
            return Err(bad(tag));
        }
    }
    Ok(())
}

pub(super) fn check_name(name: &str) -> Result<(), LibraryError> {
    if let Some(problem) = block_name_problem(name) {
        return Err(LibraryError::InvalidName(problem.to_owned()));
    }
    check_line(name, 255).map_err(|e| LibraryError::InvalidName(format!("The block name {e}.")))
}

pub(super) fn check_description(text: &str) -> Result<(), LibraryError> {
    if text.is_empty() {
        return Ok(());
    }
    check_paragraphs(text, MAX_DESCRIPTION)
        .map_err(|e| LibraryError::InvalidName(format!("The description {e}.")))
}

/// What `add_file` added.
pub(super) struct Added {
    pub id: BlockUuid,
    pub name: String,
    pub replaced: bool,
}

/// A name no block has, made from `base`: `base_2`, `base_3`...
fn unused_name(tx: &Transaction, base: &str) -> Result<String, LibraryError> {
    for n in 2.. {
        let name = format!("{base}_{n}");
        if db::find_by_name(tx, &name)?.is_none() {
            return Ok(name);
        }
    }
    unreachable!("there are not that many blocks")
}

/// Adds `file` as a block in `folder`. A name the library has is settled as
/// `if_taken` says. The block keeps its id unless the library has a block of
/// that id (a Trash item restored twice, say).
pub(super) fn add_file(
    tx: &Transaction,
    mut file: BlockFile,
    folder: &str,
    if_taken: NameTaken,
    now: u64,
) -> Result<Added, LibraryError> {
    let mut name = file.name().to_owned();
    check_name(&name)?;
    folder::check_path(folder, folder::MAX_LIBRARY_DEPTH)?;
    let mut replaced = false;
    if let Some(old) = db::find_by_name(tx, &name)? {
        match if_taken {
            NameTaken::Cancel => return Err(LibraryError::NameTaken(name)),
            NameTaken::KeepBoth => {
                name = unused_name(tx, &name)?;
                file.set_name(&name)?;
            }
            NameTaken::Replace => {
                trash_row(tx, old, now)?;
                replaced = true;
            }
        }
    }
    if db::exists(tx, file.uuid())? {
        file = file.with_uuid(BlockUuid::new())?;
    }
    let columns = Columns::of(&file)?;
    db::insert_block(tx, &columns, folder)?;
    Ok(Added {
        id: columns.id,
        name,
        replaced,
    })
}
