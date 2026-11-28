//! The `XREFTYPE`, `REFPATHTYPE` and `BINDTYPE` defaults a reference command
//! starts from, read when the command starts.

use std::sync::Arc;

use opendraft_commands::{Context, Editor, XrefHostDir};
use opendraft_core::{BindType, XrefPathType};

use crate::state::AppState;

/// The defaults read when a command starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct XrefDefaults {
    pub overlay: bool,
    pub path_type: XrefPathType,
    pub bind_type: BindType,
}

impl Default for XrefDefaults {
    fn default() -> Self {
        Self {
            overlay: false,
            path_type: XrefPathType::Relative,
            bind_type: BindType::Bind,
        }
    }
}

/// The defaults this command was started with.
pub(crate) fn defaults_of(ed: &Editor) -> XrefDefaults {
    ed.context()
        .get::<XrefDefaults>()
        .map(|defaults| *defaults)
        .unwrap_or_default()
}

impl AppState {
    /// `context` with what the reference commands read: the drawing's
    /// folder and the defaults above.
    pub(crate) fn with_xref_context(&self, context: Context) -> Context {
        context
            .with(Arc::new(XrefHostDir(self.xref_host_dir())))
            .with(Arc::new(self.xref_defaults()))
    }

    /// The reference type, path type and bind type a command starts from,
    /// read when it starts (`XREFTYPE`, `REFPATHTYPE`, `BINDTYPE`).
    pub(crate) fn xref_defaults(&self) -> XrefDefaults {
        XrefDefaults {
            overlay: self.settings.xref_overlay,
            path_type: self.settings.ref_path_type.to_core(),
            bind_type: BindType::from_code(self.settings.bind_insert as i64),
        }
    }
}
