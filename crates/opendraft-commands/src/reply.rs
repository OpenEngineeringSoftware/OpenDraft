//! What the interface sends back to a command's prompt.

use opendraft_core::block::attribute_draft::{DefinitionDraft, EditorDraft};
use opendraft_core::block::block_draft::BlockDraft;
use opendraft_core::block::{DataField, FieldDraft};
use opendraft_core::{overkill, EntityId, MText, Point, XrefAttachDraft, XrefPlan};

/// The button that closed the Block Definition dialog. The pick buttons hide
/// it while the user picks in the drawing, and it opens again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockDialogButton {
    Ok,
    PickPoint,
    SelectObjects,
}

/// What the UI sends back.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    Point(Point),
    Number(f64),
    Text(String),
    /// The text the editor was closed with.
    MText(Box<MText>),
    /// The settings the OVERKILL dialog was closed with.
    Overkill(overkill::Settings),
    /// The Attribute Definition dialog, accepted.
    AttributeDefinition(Box<DefinitionDraft>),
    /// The Attribute Editor: what it holds, and whether it is
    /// closing (OK) or staying open (Apply).
    AttributeEditor(Box<EditorDraft>, bool),
    /// The Attribute Editor, closed to choose another block reference.
    AttributeEditorSelect(Box<EditorDraft>),
    /// The Attribute Editor, closed to choose a data field for a value.
    AttributeEditorField(Box<EditorDraft>),
    /// The Block Definition dialog, closed by a button.
    BlockDialog(Box<BlockDraft>, BlockDialogButton),
    /// The Field dialog, accepted: the field it made.
    Field(Box<DataField>),
    /// The Field dialog, closed to pick an object for an object field.
    FieldPick(Box<FieldDraft>),
    /// The source was read and planned: what to apply.
    XrefPlan(Box<XrefPlan>),
    /// The source could not be read or planned, worded for the user.
    XrefFailed(String),
    /// The Attach External Reference dialog, accepted.
    XrefAttach(Box<XrefAttachDraft>),
    /// The label of a chosen keyword.
    Keyword(String),
    Selection(Vec<EntityId>),
    /// An object clicked, where, and whether Shift was held.
    Object {
        id: EntityId,
        at: Point,
        shift: bool,
    },
    /// At a [`crate::Options::sweep`] prompt: a click in empty space.
    Missed {
        at: Point,
        shift: bool,
    },
    /// At a [`crate::Options::sweep`] prompt: a path dragged from empty space.
    Path {
        points: Vec<Point>,
        shift: bool,
    },
    /// Enter with nothing typed.
    Enter,
    Cancel,
    /// Esc at a prompt that asked for [`crate::Options::esc_backs`]: that ask fails
    /// with `Cancelled`, and the command goes on.
    Back,
}
