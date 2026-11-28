//! Commands, undo/redo and the command registry. No UI dependencies.
//!
//! Every user action (ribbon button, menu item, shortcut, canvas tool) is a
//! named command in a [`Registry`]. Commands that change a drawing do so
//! through an [`Action`] run by the [`Session`], which makes them undoable.

mod action;
mod answer;
mod array_action;
mod associations;
mod block_action;
mod ccs_action;
mod changes;
mod context;
mod corner;
mod dim_style_action;
mod dim_styles_action;
mod editor;
mod history;
mod layer_action;
mod layout_action;
mod library_action;
mod linetype_action;
mod mleader_style_action;
mod order_action;
mod paste_action;
mod point_style_action;
mod polyline_curve_action;
mod prompt;
mod properties_action;
mod registry;
mod reply;
mod runner;
mod session;
mod sysvar;
mod text_style_action;
mod transaction;
mod units_action;
mod wording;
mod xref_action;

pub use action::{Action, AddCopies, AddEntity, ModifyAttributes, ModifyEntities, RemoveEntities};
pub use array_action::SetArrays;
pub use block_action::{SetBlockEditorDynamic, SetBlocks};
pub use ccs_action::{SetCcs, SetNamedCcs};
pub use changes::Changes;
pub use context::{Context, XrefHostDir};
pub use dim_style_action::SetDimStyle;
pub use dim_styles_action::SetDimStyles;
pub use editor::{AttributeEditorAnswer, Editor, FieldAnswer};
pub use history::{History, Provenance, Step, StepInfo};
pub use layer_action::{AddLayer, ModifyLayers, RemoveLayer, SetCurrentLayer, SetNewObjectStyle};
pub use layout_action::{SetLayouts, SetModelPlot};
pub use library_action::{InsertFromLibrary, LibraryInsertReport, LibraryInsertSink};
pub use linetype_action::SetLinetypes;
pub use mleader_style_action::SetMLeaderStyles;
pub use order_action::{Place, ReorderObjects};
pub use paste_action::{PasteDrawing, WarningSink};
pub use point_style_action::SetPointStyle;
pub use polyline_curve_action::SetPolylineCurve;
pub use prompt::{
    Answer, BlockDialogButton, Cancelled, DynamicDimensions, Ghost, GhostStyle, Hint, Keyword,
    Marker, NumberOptions, NumberOrPoint, Options, Picked, PointOptions, Prompt, Reply, Swept,
    TextOptions, Wants, Wording,
};
pub use properties_action::SetProperties;
pub use registry::{
    always, never, AutomationPolicy, Command, CommandEffect, ExecuteError, Registry, RegistryError,
};
pub use runner::{run_scripted, Outcome, Runner, Transcript};
pub use session::{CheckedError, HistoryError, Session};
pub use sysvar::{Kind, SysVar, SysVarError, SysVarTable, TableError, Value};
pub use text_style_action::SetTextStyles;
pub use transaction::Transaction;
pub use units_action::SetUnits;
pub use xref_action::{SetVisretain, SetXclipFrame, SetXrefs};
