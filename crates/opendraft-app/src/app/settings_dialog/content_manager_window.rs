//! Showing the Content Manager window and starting what it asks for.

use std::sync::Arc;

use eframe::egui;

use super::OpenDraftApp;
use crate::content_manager::{Env, Request};
use crate::library_add_cmd::library_add;
use crate::library_insert_cmd::library_insert;

impl OpenDraftApp {
    /// Whether keys belong to text being typed that a focused field may not
    /// show: the in-place text editor, or a name or the list of units in the
    /// Content Manager (on Escape egui drops the field's focus before the
    /// window sees the key, and Escape must close the field or the list, not
    /// cancel in the drawing).
    pub(in crate::app) fn typing_a_name_or_text(&self) -> bool {
        self.state.mtext_editing() || self.state.command_dialogs.content.typing_a_name()
    }

    /// The Content Manager, while it is open. It is modeless: the drawing
    /// stays usable beside it, and a block it inserts is placed by LIBINSERT
    /// on the command line.
    pub(in crate::app) fn content_manager_window(&mut self, ctx: &egui::Context) {
        if !self.state.command_dialogs.content.open {
            return;
        }
        let mut window = std::mem::take(&mut self.state.command_dialogs.content);
        let defaults = Arc::clone(self.state.defaults());
        let requests = {
            let session = self.state.session();
            let env = Env {
                library: &defaults.library,
                document: session.document(),
                revision: session.revision(),
            };
            window.show(ctx, &self.theme, &env)
        };
        self.state.command_dialogs.content = window;
        for request in requests {
            match request {
                Request::Insert(id) => {
                    defaults.library.queue_pick(id);
                    self.state.start_command("LIBINSERT", library_insert);
                }
                Request::AddObjects => self.state.start_command("LIBADD", library_add),
                Request::PickImport => self.state.pick_blocks_to_import(),
                Request::RenameDrawingBlock(id, name) => {
                    let before = self.state.session().revision();
                    let result = self.state.rename_block(id, &name);
                    let after = self.state.session().revision();
                    let window = &mut self.state.command_dialogs.content;
                    window.drawing_block_renamed(result, &name, (before, after));
                }
                Request::SetDrawingBlockUnit(id, unit) => {
                    let before = self.state.session().revision();
                    let result = self.state.set_block_unit(id, unit);
                    let after = self.state.session().revision();
                    let window = &mut self.state.command_dialogs.content;
                    window.drawing_block_changed(result, (before, after));
                }
            }
        }
    }
}
