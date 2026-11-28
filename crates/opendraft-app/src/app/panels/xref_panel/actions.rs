//! What the palette's toolbar and row menu can do, and when each is allowed.

use opendraft_ui::{icons, key};

use super::rows::Row;
use crate::state::AppState;
use crate::xref_cmd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Attach,
    Reload,
    ReloadAll,
    Refresh,
    Unload,
    Detach,
    Path,
    Bind,
    Clip,
}

impl Action {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Attach => key!("Attach"),
            Self::Reload => key!("Reload"),
            Self::ReloadAll => key!("Reload all"),
            Self::Refresh => key!("Refresh list"),
            Self::Unload => key!("Unload"),
            Self::Detach => key!("Detach"),
            Self::Path => key!("Change path"),
            Self::Bind => key!("Bind"),
            Self::Clip => key!("Clip"),
        }
    }

    pub(super) fn icon(self) -> char {
        match self {
            Self::Attach => icons::XREF_PANEL_ATTACH,
            Self::Reload | Self::ReloadAll | Self::Refresh => icons::XREF_PANEL_RELOAD,
            Self::Unload => icons::XREF_PANEL_UNLOAD,
            Self::Detach => icons::XREF_PANEL_DETACH,
            Self::Path => icons::XREF_PANEL_PATH,
            Self::Bind => icons::XREF_PANEL_BIND,
            Self::Clip => icons::XREF_PANEL_CLIP,
        }
    }

    /// Why the action cannot be used on `row` (`None` when it can).
    pub(super) fn refused(self, row: Option<&Row>) -> Option<&'static str> {
        match self {
            Self::Attach | Self::ReloadAll | Self::Refresh => None,
            _ => match row {
                None => Some(key!("Select an external reference first.")),
                // A reference inside another is part of it; only the one the
                // drawing attaches can be changed.
                Some(r) if r.is_nested() && self != Self::Reload => Some(key!(
                    "This reference is inside another. Select the one that attaches it."
                )),
                Some(_) => None,
            },
        }
    }

    /// Start the command for this action on `row`'s reference. `Refresh` is
    /// the panel's own and does nothing here.
    pub(super) fn run(self, state: &mut AppState, row: Option<&Row>) {
        let block = row.map(|r| r.key.block);
        match (self, block) {
            (Self::Attach, _) => state.start_command("XATTACH", xref_cmd::attach_dialog),
            (Self::ReloadAll, _) => state.start_command("XREFRELOADALL", xref_cmd::reload_all),
            (Self::Reload, Some(b)) => {
                state.start_command("XREFRELOAD", move |ed| xref_cmd::reload_block(ed, b));
            }
            (Self::Unload, Some(b)) => {
                state.start_command("XREFUNLOAD", move |ed| xref_cmd::unload_block(ed, b));
            }
            (Self::Detach, Some(b)) => {
                state.start_command("XREFDETACH", move |ed| xref_cmd::detach_block(ed, b));
            }
            (Self::Path, Some(b)) => {
                state.start_command("XREFPATH", move |ed| xref_cmd::edit_path_block(ed, b));
            }
            (Self::Bind, Some(b)) => {
                let kind = state.xref_defaults().bind_type;
                state.start_command("XBIND", move |ed| {
                    crate::xref_apply::bind(ed, vec![b], kind)
                });
            }
            (Self::Clip, Some(b)) => {
                state.start_command("XCLIP", move |ed| {
                    let inserts = ed.read(move |doc| doc.xref_references(b))?;
                    ed.select(inserts);
                    crate::xclip_cmd::xclip(ed)
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::{BlockId, XrefKind};

    use super::super::rows::{Health, RowKey};
    use super::*;

    fn row(nested: Option<usize>) -> Row {
        Row {
            key: RowKey {
                block: BlockId(1),
                nested,
            },
            parent: None,
            depth: 0,
            name: "a".to_owned(),
            health: Health::Loaded,
            size: None,
            kind: XrefKind::Attach,
            date: None,
            path: String::new(),
            found: String::new(),
        }
    }

    #[test]
    fn actions_on_a_reference_need_one_selected_and_not_a_nested_one() {
        assert!(Action::Attach.refused(None).is_none());
        assert!(Action::ReloadAll.refused(None).is_none());
        assert!(Action::Detach.refused(None).is_some());
        assert!(Action::Detach.refused(Some(&row(None))).is_none());
        assert!(Action::Detach.refused(Some(&row(Some(0)))).is_some());
        assert!(Action::Reload.refused(Some(&row(Some(0)))).is_none());
    }
}
