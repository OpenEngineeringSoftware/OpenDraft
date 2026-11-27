//! Renaming a block of the drawing, or changing its unit, from the Content
//! Manager's "This drawing". Only the definition changes, so every insert
//! of the block follows, in the drawing and inside other blocks (see
//! `Document::renamed_block` and `Document::block_in_unit`).

use opendraft_commands::{ModifyEntities, SetBlocks};
use opendraft_core::block::Redefinition;
use opendraft_core::{BlockId, InsertUnits};

use crate::state::AppState;

const STEP: &str = "Rename block";
const UNIT_STEP: &str = "Block unit";

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
            return Err("Close the Block Editor before changing a block.".to_owned());
        }
        let old = match self.session().document().block(id) {
            Some(block) if block.name == name => return Ok(None),
            Some(block) => block.name.clone(),
            None => return Err("That block is no longer in the drawing.".to_owned()),
        };
        // A command working on the block would hold its old record.
        self.interrupt_command();
        let change = self.session().document().renamed_block(id, name)?;
        self.record_block_change(STEP, change);
        Ok(Some(old))
    }

    /// Puts block `id` of the active drawing in `unit`, in one undo step.
    /// Returns the sentence to show, `None` when the unit is unchanged, or
    /// why it was refused. Inserts already placed keep their size.
    pub(crate) fn set_block_unit(
        &mut self,
        id: BlockId,
        unit: InsertUnits,
    ) -> Result<Option<String>, String> {
        if self.in_block_editor() {
            return Err("Close the Block Editor before changing a block.".to_owned());
        }
        let name = match self.session().document().block(id) {
            Some(block) if block.unit == unit => return Ok(None),
            Some(block) => block.name.clone(),
            None => return Err("That block is no longer in the drawing.".to_owned()),
        };
        self.interrupt_command();
        let change = self.session().document().block_in_unit(id, unit)?;
        self.record_block_change(UNIT_STEP, change);
        let now = match unit {
            InsertUnits::UNITLESS => "unitless".to_owned(),
            unit => format!("in {}", unit.label().to_lowercase()),
        };
        Ok(Some(format!(
            "\"{name}\" is now {now}; the blocks already placed keep their size."
        )))
    }

    /// The new block table and the inserts that follow it, as one undo step.
    fn record_block_change(&mut self, step: &str, change: Redefinition) {
        let session = &mut self.docs[self.active].session;
        let _ = session.transaction(step, |tx| {
            tx.perform(SetBlocks::new(step, change.blocks));
            if !change.objects.is_empty() {
                tx.perform(ModifyEntities::new(step, change.objects));
            }
            Ok::<(), ()>(())
        });
    }
}

#[cfg(test)]
#[path = "block_rename_tests.rs"]
mod tests;
