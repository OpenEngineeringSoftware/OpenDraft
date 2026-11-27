//! What the ribbon asks the app about a command: whether it is enabled, its
//! shortcut, and whether its button looks pressed.

use egui_dock::DockState;
use opendraft_commands::Registry;
use opendraft_ui::ribbon::CommandSource;

use crate::dock::{self, PanelTab};
use crate::state::AppState;

/// Lets the ribbon ask the registry about enabled state and shortcuts.
pub(super) struct Source<'a> {
    pub(super) registry: &'a Registry<AppState>,
    pub(super) state: &'a AppState,
    pub(super) dock: &'a DockState<PanelTab>,
    pub(super) detached: &'a crate::detached::Detached,
}

impl CommandSource for Source<'_> {
    fn is_enabled(&self, command: &str) -> bool {
        self.registry.is_enabled(command, self.state)
    }

    fn shortcut_text(&self, command: &str) -> Option<&str> {
        self.registry.get(command)?.shortcut
    }

    fn is_active(&self, command: &str) -> bool {
        match dock::panel_of(command) {
            Some(panel) => dock::is_open(self.dock, panel) || self.detached.contains(panel),
            None => self.registry.is_checked(command, self.state),
        }
    }
}
