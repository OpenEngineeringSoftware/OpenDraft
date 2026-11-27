//! What clicks in the window ask for, and carrying them out on the library.

use opendraft_content::blocks_payload::folder;
use opendraft_content::blocks_payload::BlockUuid;
use opendraft_content::library::{CaptureRequest, Library, LibraryError, NameTaken};
use opendraft_core::BlockId;

use super::{Editing, Env, Place, Request, Selected, Window};
use crate::content_library::now;

/// How a block of the drawing is added when the library has its name.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Taken {
    Cancel,
    KeepBoth,
    Replace,
}

#[derive(Clone, Debug)]
pub enum Action {
    Go(Place),
    Back,
    Forward,
    Up,
    Select(Selected),
    Insert(BlockUuid),
    /// Run LIBADD in the command line.
    AddObjects,
    ImportFile,
    RunImport,
    CancelImport,
    AddFromDrawing(String, Taken),
    DropOffer,
    Edit(Option<Editing>),
    RenameBlock(BlockUuid, String),
    /// Rename a block of the drawing, which the app does.
    RenameDrawingBlock(BlockId, String),
    /// Put a block of the drawing in another unit, which the app does.
    SetDrawingBlockUnit(BlockId, opendraft_core::InsertUnits),
    SetDescription(BlockUuid, String),
    SetTags(BlockUuid, Vec<String>),
    Move(BlockUuid, String),
    Favourite(BlockUuid, bool),
    Delete(BlockUuid),
    NewFolder(String),
    RenameFolder(String, String),
    DeleteFolder(String),
    Restore(u64),
    EmptyTrash,
}

impl Window {
    /// Carries out `actions` in order and returns what the app has to start.
    pub(super) fn apply(&mut self, actions: Vec<Action>, env: &Env) -> Vec<Request> {
        let mut requests = Vec::new();
        for action in actions {
            match action {
                Action::Go(place) => self.go(place),
                Action::Back => {
                    if let Some(previous) = self.back.pop() {
                        let here = std::mem::replace(&mut self.place, previous);
                        self.forward.push(here);
                        self.selected = Selected::Nothing;
                    }
                }
                Action::Forward => {
                    if let Some(next) = self.forward.pop() {
                        let here = std::mem::replace(&mut self.place, next);
                        self.back.push(here);
                        self.selected = Selected::Nothing;
                    }
                }
                Action::Up => {
                    if let Place::Mine(path) = &self.place {
                        let up = folder::parent(path).to_owned();
                        self.go(Place::Mine(up));
                    }
                }
                Action::Select(selected) => {
                    // A block's name field belongs to the block it was opened on.
                    let renaming = matches!(
                        self.editing,
                        Some(Editing::RenameBlock { .. } | Editing::RenameDrawingBlock { .. })
                    );
                    if renaming && selected != self.selected {
                        self.editing = None;
                    }
                    self.selected = selected;
                }
                Action::Insert(id) => requests.push(Request::Insert(id)),
                Action::AddObjects => requests.push(Request::AddObjects),
                Action::ImportFile => requests.push(Request::PickImport),
                Action::RenameDrawingBlock(id, name) => {
                    requests.push(Request::RenameDrawingBlock(id, name))
                }
                Action::SetDrawingBlockUnit(id, unit) => {
                    requests.push(Request::SetDrawingBlockUnit(id, unit))
                }
                Action::RunImport => self.run_import(env),
                Action::CancelImport => self.import = None,
                Action::DropOffer => self.taken = None,
                Action::Edit(editing) => self.editing = editing,
                other => self.change_library(other, env),
            }
        }
        requests
    }

    /// The actions that change what the library holds. A failure is said in
    /// the status line and leaves the library as it was.
    fn change_library(&mut self, action: Action, env: &Env) {
        let result = env.library.with(|lib| self.run(lib, action, env));
        self.status = match result {
            Ok(Ok(said)) => said,
            Ok(Err(error)) | Err(error) => error.to_string(),
        };
    }

    fn run(
        &mut self,
        lib: &mut Library,
        action: Action,
        env: &Env,
    ) -> Result<String, LibraryError> {
        self.taken = None;
        Ok(match action {
            Action::RenameBlock(id, name) => {
                lib.rename_block(id, &name)?;
                self.editing = None;
                format!("Renamed to \"{}\".", name.trim())
            }
            Action::SetDescription(id, text) => {
                lib.set_description(id, &text)?;
                "Description saved.".to_owned()
            }
            Action::SetTags(id, tags) => {
                lib.set_tags(id, tags)?;
                "Tags saved.".to_owned()
            }
            Action::Move(id, to) => {
                lib.move_block(id, &to)?;
                if to.is_empty() {
                    "Moved to My blocks.".to_owned()
                } else {
                    format!("Moved to {to}.")
                }
            }
            Action::Favourite(id, on) => {
                lib.set_favourite(id, on)?;
                String::new()
            }
            Action::Delete(id) => {
                let name = lib.block(id).map(|b| b.name).unwrap_or_default();
                lib.delete_block(id, now())?;
                self.selected = Selected::Nothing;
                format!("\"{name}\" moved to the Trash.")
            }
            Action::NewFolder(path) => {
                lib.create_folder(&path)?;
                self.editing = None;
                format!("Folder {path} made.")
            }
            Action::RenameFolder(from, to) => {
                lib.rename_folder(&from, &to)?;
                self.editing = None;
                if let Place::Mine(here) = &self.place {
                    if let Some(moved) = folder::reroot(here, &from, &to) {
                        self.place = Place::Mine(moved);
                    }
                }
                format!("Folder renamed to {to}.")
            }
            Action::DeleteFolder(path) => {
                lib.delete_folder(&path, now())?;
                if let Place::Mine(here) = &self.place {
                    if folder::is_within(here, &path) {
                        self.place = Place::Mine(folder::parent(&path).to_owned());
                    }
                }
                self.selected = Selected::Nothing;
                format!("Folder {path} moved to the Trash.")
            }
            Action::Restore(id) => {
                let done = lib.restore(id, NameTaken::KeepBoth, now())?;
                self.selected = Selected::Block(done.id);
                format!("\"{}\" restored.", done.name)
            }
            Action::EmptyTrash => {
                let n = lib.empty_trash()?;
                self.selected = Selected::Nothing;
                format!("{} removed for good.", crate::text::plural(n, "item"))
            }
            Action::AddFromDrawing(name, how) => self.add_from_drawing(lib, env, &name, how)?,
            _ => String::new(),
        })
    }

    fn add_from_drawing(
        &mut self,
        lib: &mut Library,
        env: &Env,
        name: &str,
        how: Taken,
    ) -> Result<String, LibraryError> {
        let Some(block) = env.document.block_named(name) else {
            return Err(LibraryError::NotFound);
        };
        let source = env.document.extract(&[]);
        let request = CaptureRequest {
            if_name_taken: match how {
                Taken::Cancel => NameTaken::Cancel,
                Taken::KeepBoth => NameTaken::KeepBoth,
                Taken::Replace => NameTaken::Replace,
            },
            ..CaptureRequest::default()
        };
        match lib.capture(&source, block.id, &request, now()) {
            Ok(done) => Ok(format!("\"{}\" added to the library.", done.name)),
            Err(LibraryError::NameTaken(_)) => {
                self.taken = Some(name.to_owned());
                Ok(String::new())
            }
            Err(other) => Err(other),
        }
    }
}
