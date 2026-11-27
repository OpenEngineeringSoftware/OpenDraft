//! Renaming a block of the drawing, from the Content Manager's "This
//! drawing". Only the definition's name changes, so every insert of the
//! block follows, in the drawing and inside other blocks (see
//! `Document::renamed_block`).

use opendraft_commands::{ModifyEntities, SetBlocks};
use opendraft_core::BlockId;

use crate::state::AppState;

const STEP: &str = "Rename block";

impl AppState {
    /// Renames block `id` of the active drawing to `name`, in one undo step.
    /// Returns its old name (`None` when the name is unchanged), or why it
    /// was refused (the drawing is then left as it was).
    pub(crate) fn rename_block(
        &mut self,
        id: BlockId,
        name: &str,
    ) -> Result<Option<String>, String> {
        let name = name.trim();
        if self.in_block_editor() {
            return Err("Close the Block Editor before renaming a block.".to_owned());
        }
        let old = match self.session().document().block(id) {
            Some(block) if block.name == name => return Ok(None),
            Some(block) => block.name.clone(),
            None => return Err("That block is no longer in the drawing.".to_owned()),
        };
        // A command working on the block would hold its old record.
        self.interrupt_command();
        let change = self.session().document().renamed_block(id, name)?;
        let session = &mut self.docs[self.active].session;
        let _ = session.transaction(STEP, |tx| {
            tx.perform(SetBlocks::new(STEP, change.blocks));
            if !change.objects.is_empty() {
                tx.perform(ModifyEntities::new(STEP, change.objects));
            }
            Ok::<(), ()>(())
        });
        Ok(Some(old))
    }
}

#[cfg(test)]
#[path = "block_rename_tests.rs"]
mod tests;
