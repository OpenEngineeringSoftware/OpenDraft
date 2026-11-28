//! Which dialogs are open.

use opendraft_core::block::attribute_draft::{DefinitionDraft, EditorDraft};
use opendraft_core::block::block_draft::BlockDraft;
use opendraft_core::block::FieldDraft;
use opendraft_core::drawing_properties::DrawingProperties;

use crate::state::AppState;

/// The print backend, shared with the threads that talk to it.
#[derive(Clone)]
pub struct PrintHandle(pub std::sync::Arc<dyn crate::print_backend::PrintBackend>);

impl Default for PrintHandle {
    fn default() -> Self {
        Self(std::sync::Arc::from(crate::print_backend::system_backend()))
    }
}

/// What the dialogs a command opens hold while they are open.
#[derive(Default)]
pub struct CommandDialogs {
    /// Where paper plots go; the printers of this machine unless a test says otherwise.
    pub print: PrintHandle,
    pub definition: Option<DefinitionDraft>,
    pub editor: Option<EditorDraft>,
    pub block: Option<BlockDraft>,
    pub field: Option<FieldDraft>,
    /// The Attach External Reference dialog, while it is open.
    pub xref_attach: Option<opendraft_core::XrefAttachDraft>,
    /// The Drawing Properties dialog, while it is open.
    pub properties: Option<DrawingProperties>,
    /// The Plot dialog (`PLOT`, `PREVIEW`), while it is open.
    pub plot: Option<crate::plot_job::PlotDialog>,
    /// How far its preview is zoomed in, and the sharp picture of the part in view.
    pub plot_zoom: crate::plot_zoom::Zoom,
    /// What finished -PLOT commands asked for.
    pub plot_requests: crate::plot_line::PlotRequests,
    /// Plot files written for the Plot dialog with "Open in viewer when done",
    /// for the window to open in the system's viewer.
    pub plots_to_view: Vec<std::path::PathBuf>,
    /// The Content Manager window (it never takes the keyboard: it is modeless).
    pub content: crate::content_manager::Window,
}

impl AppState {
    /// A dialog is open and owns the keyboard.
    pub fn modal_open(&self) -> bool {
        self.show_settings
            || !self.large_opens.is_empty()
            || self.show_units
            || self.command_dialogs.properties.is_some()
            || self.show_point_style
            || self.power_select.open
            || self.overkill_draft.is_some()
            || self.command_dialogs.definition.is_some()
            || self.command_dialogs.editor.is_some()
            || self.command_dialogs.block.is_some()
            || self.command_dialogs.field.is_some()
            || self.command_dialogs.xref_attach.is_some()
            || (self.purge.open && !self.command_running())
            || self.show_linetypes
            || self.show_text_styles
            || self.show_dim_style
            || self.show_dim_manager
            || self.mleader_dialogs.style
            || self.mleader_dialogs.manager
            || self.page_setup.is_some()
            || self
                .command_dialogs
                .plot
                .as_ref()
                .is_some_and(|d| d.picking.is_none())
    }
}
