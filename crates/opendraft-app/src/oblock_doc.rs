//! An `.oblock` file open in a tab. It opens in the Block Editor and nowhere
//! else: the tab holds a small drawing that is only the file's container (the
//! block and what it nests), set aside exactly as when a block of a drawing is
//! edited (`block_editor.rs`), and the editor never leaves it. Save is BSAVE
//! plus writing the file; BCLOSE closes the tab.

use std::path::{Path, PathBuf};

use opendraft_commands::Session;
use opendraft_content::oblock::{BlockFile, EXTENSION};
use opendraft_ui::key;

use crate::files::display_name;
use crate::state::{AppState, Doc};
use opendraft_commands::Wording;

/// What a block file tab remembers of its file: the block as it was read or
/// last saved, which keeps its id and tags when the edited block is written.
pub struct Link {
    pub file: BlockFile,
}

/// Whether `path` names a block file.
pub fn is_oblock(path: &Path) -> bool {
    path.extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(EXTENSION))
}

/// `path` as a block file's: a name with no extension gets `.oblock`; any
/// other extension is refused.
pub fn oblock_path(path: PathBuf) -> Result<PathBuf, String> {
    match path.extension() {
        None => Ok(path.with_extension(EXTENSION)),
        Some(_) if is_oblock(&path) => Ok(path),
        Some(_) => Err(format!("A block file's name ends in .{EXTENSION}.")),
    }
}

pub fn oblock_filter() -> (&'static str, &'static [&'static str]) {
    ("OpenDraft block", &[EXTENSION])
}

/// Writes `file` as a DXF to `path` (AutoCAD's WBLOCK layout, with our marker:
/// see `BlockFile::to_dxf`). Returns what did not carry across.
pub(crate) fn write_block_dxf(file: &BlockFile, path: &Path) -> Result<Vec<String>, String> {
    let (bytes, notes) = file.to_dxf().map_err(|e| e.to_string())?;
    std::fs::write(path, bytes).map_err(|e| format!("Could not write {}: {e}", path.display()))?;
    Ok(notes)
}

impl Doc {
    /// Writes the block the editor holds into the file, as it is saved in the
    /// container. Nothing is written when it is not a block file's tab.
    pub(crate) fn write_oblock(&mut self) -> Result<(), String> {
        let (Some(link), Some(edit), Some(path)) = (
            self.oblock.as_mut(),
            self.block_edit.as_ref(),
            self.path.clone(),
        ) else {
            return Err("This is not a block file.".to_owned());
        };
        let next = link
            .file
            .with_drawing(edit.drawing.document().clone())
            .map_err(|e| format!("Could not save {}: {e}", path.display()))?;
        next.save(&path)
            .map_err(|e| format!("Could not save {}: {e}", path.display()))?;
        link.file = next;
        Ok(())
    }
}

impl AppState {
    /// Open the block file at `path` in a tab, in the Block Editor. It is read
    /// at once: a block file is small beside a drawing. Returns whether it
    /// opened; a failure is said in the status bar.
    pub(crate) fn open_oblock(&mut self, path: &Path) -> bool {
        let file = match BlockFile::load(path) {
            Ok(file) => file,
            Err(error) => {
                self.tell(Wording::new(
                    key!("Could not open %{name}: %{error}"),
                    &[
                        ("name", &path.display().to_string()),
                        ("error", &error.to_string()),
                    ],
                ));
                return false;
            }
        };
        self.abort_command();
        let root = file.root();
        let title = display_name(path);
        let session = Session::with_document(file.drawing().clone());
        let doc = self.install_document(title.clone(), session);
        doc.path = Some(path.to_owned());
        doc.oblock = Some(Box::new(Link { file }));
        self.open_block_editor(root);
        self.commit_recent(path);
        true
    }

    /// Export the block file in the active tab as a DXF, for other CAD
    /// programs. It is the block as last saved: changes in the editor are not
    /// in it, so they are asked to be saved first.
    pub(crate) fn export_oblock_dxf(&mut self) -> bool {
        let doc = &self.docs[self.active];
        let Some(link) = doc.oblock.as_ref() else {
            return false;
        };
        if doc.is_modified() {
            self.tell("Save the block first (BSAVE), then export it.");
            return false;
        }
        let file = link.file.clone();
        let suggested = format!("{}.dxf", file.name());
        let directory = self.folder_for_dialogs(doc.path.as_deref());
        let Some(chosen) = self.picker.save(
            "Export DXF",
            &suggested,
            directory.as_deref(),
            opendraft_files::dxf_filter(),
        ) else {
            return false;
        };
        let path = chosen.with_extension("dxf");
        match write_block_dxf(&file, &path) {
            Ok(notes) => {
                for note in notes {
                    self.cmdline.say(note);
                }
                self.tell(Wording::new(
                    key!("Exported %{name}"),
                    &[("name", &display_name(&path))],
                ));
                true
            }
            Err(why) => {
                self.tell(why);
                false
            }
        }
    }

    /// Save the block file in tab `index`: BSAVE, which writes the file too.
    pub(crate) fn save_oblock(&mut self, index: usize) -> bool {
        let uid = self.docs[index].uid;
        let asked = self.confirm.as_ref().and_then(|c| c.saving()) == Some(uid);
        let saved = self.bsave_at(index);
        if saved && asked {
            self.next_pending();
        }
        saved
    }

    /// Ask where to save the block file in tab `index`, and save it there.
    pub(crate) fn save_oblock_as(&mut self, index: usize) -> bool {
        let doc = &self.docs[index];
        let name = doc
            .oblock
            .as_ref()
            .map_or_else(|| doc.title.clone(), |l| l.file.name().to_owned());
        let suggested = format!("{name}.{EXTENSION}");
        let directory = self.folder_for_dialogs(doc.path.as_deref());
        let Some(chosen) = self.picker.save(
            "Save block as",
            &suggested,
            directory.as_deref(),
            oblock_filter(),
        ) else {
            return false;
        };
        let path = match oblock_path(chosen) {
            Ok(path) => path,
            Err(why) => {
                self.tell(why);
                return false;
            }
        };
        let doc = &mut self.docs[index];
        let before = (
            doc.path.take(),
            std::mem::replace(&mut doc.title, display_name(&path)),
        );
        doc.path = Some(path);
        if self.save_oblock(index) {
            return true;
        }
        let doc = &mut self.docs[index];
        (doc.path, doc.title) = before;
        false
    }
}

#[cfg(test)]
#[path = "oblock_doc_tests.rs"]
mod tests;
