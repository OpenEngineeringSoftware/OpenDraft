//! The commands that make, place and describe blocks: BLOCK and INSERT, the
//! attribute commands, and their command-line (`-`) forms.

use opendraft_commands::Command;

use crate::attdef_cmd::define_attribute;
use crate::attribute_dialog_cmd::{attdef_dialog, eattedit_dialog};
use crate::attribute_edit::edit_attributes;
use crate::attribute_sync_cmd::sync_attributes;
use crate::block_cmd::define_block;
use crate::block_dialog_cmd::define_block_dialog;
use crate::content_manager::Place;
use crate::dynamic_block_cmd::{baction, bactionset, bparameter, resetblock};
use crate::dynamic_lookup_cmd::blookuptable;
use crate::dynamic_visibility_cmd::{bvhide, bvshow, bvstate};
use crate::field_cmd::{insert_field, update_field};
use crate::field_dialog_cmd::insert_field_dialog;
use crate::insert_cmd::insert_block;
use crate::library_add_cmd::library_add;
use crate::library_insert_cmd::library_insert;
use crate::state::AppState;

/// As `editor_command!` in `commands.rs`.
macro_rules! editor {
    ($id:literal, $label:literal, $name:literal, $run:path) => {
        Command::new($id, $label, |s: &mut AppState| s.start_command($name, $run))
            .checked(|s: &AppState| s.is_running($name))
            .transparent()
    };
}

/// The commands, for the command table.
pub fn commands() -> Vec<Command<AppState>> {
    vec![
        editor!("block.create", "Create Block", "BLOCK", define_block_dialog),
        editor!("block.create_line", "-BLOCK", "-BLOCK", define_block),
        editor!("block.insert", "Insert Block", "INSERT", insert_block),
        Command::new(
            "block.content_manager",
            "Content Manager",
            |s: &mut AppState| {
                s.command_dialogs.content.open_on(Place::default());
            },
        ),
        Command::new("block.wblock", "Write Block", |s: &mut AppState| s.wblock())
            .checked(|s: &AppState| s.is_running("WBLOCK"))
            .transparent(),
        editor!("block.library_add", "Add to Library", "LIBADD", library_add),
        editor!(
            "block.library_insert",
            "Insert from Library",
            "LIBINSERT",
            library_insert
        ),
        editor!("block.attdef", "Define Attributes", "ATTDEF", attdef_dialog),
        editor!("block.attdef_line", "-ATTDEF", "-ATTDEF", define_attribute),
        editor!(
            "block.attedit",
            "Edit Attributes",
            "EATTEDIT",
            eattedit_dialog
        ),
        editor!(
            "block.attedit_line",
            "-ATTEDIT",
            "-ATTEDIT",
            edit_attributes
        ),
        editor!(
            "block.attsync",
            "Sync Attributes",
            "ATTSYNC",
            sync_attributes
        ),
        editor!("block.field", "Insert Field", "FIELD", insert_field_dialog),
        editor!("block.field_line", "-FIELD", "-FIELD", insert_field),
        editor!(
            "block.updatefield",
            "Update Fields",
            "UPDATEFIELD",
            update_field
        ),
        editor!(
            "block.parameter",
            "Block Parameter",
            "BPARAMETER",
            bparameter
        )
        .when(AppState::in_block_editor),
        editor!("block.action", "Block Action", "BACTION", baction).when(AppState::in_block_editor),
        editor!(
            "block.actionset",
            "Action Objects",
            "BACTIONSET",
            bactionset
        )
        .when(AppState::in_block_editor),
        editor!(
            "block.lookuptable",
            "Lookup Table",
            "BLOOKUPTABLE",
            blookuptable
        )
        .when(AppState::in_block_editor),
        editor!("block.vstate", "Visibility States", "BVSTATE", bvstate)
            .when(AppState::in_block_editor),
        editor!("block.vhide", "Hide Objects", "BVHIDE", bvhide).when(AppState::in_block_editor),
        editor!("block.vshow", "Show Objects", "BVSHOW", bvshow).when(AppState::in_block_editor),
        editor!("block.reset", "Reset Block", "RESETBLOCK", resetblock)
            .when(|s: &AppState| !s.in_block_editor()),
    ]
}
