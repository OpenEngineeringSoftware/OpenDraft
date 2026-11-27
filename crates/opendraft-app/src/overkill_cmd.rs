//! OVERKILL: delete duplicate objects and merge lines and arcs that overlap
//! or meet end to end. The maths is `opendraft_core::overkill`; this asks which
//! objects and which settings (the dialog, or `-OVERKILL` in
//! `overkill_line.rs`) and applies the outcome as one undo step.

use opendraft_commands::{Cancelled, Editor, ModifyEntities, RemoveEntities};
use opendraft_core::overkill::{overkill, prepare, Settings};
use opendraft_core::EntityId;
use opendraft_ui::key;

use crate::defaults::EditorDefaults;
use opendraft_commands::Wording;

/// OVERKILL: select the objects (or use those already selected), then the
/// Object Comparison Settings dialog opens; OK tidies them. Objects only
/// compare against ones on the same layer, colour, linetype, linetype scale
/// and lineweight unless those are ignored.
pub fn overkill_objects(ed: &mut Editor) -> Result<(), Cancelled> {
    let chosen = ed.pickset_or_select(key!("Select objects"))?;
    let settings = ed.ask_overkill(ed.defaults().overkill.get())?;
    ed.defaults().overkill.change(|m| *m = settings);
    run(ed, chosen, settings)
}

/// Tidy `chosen` with `settings`: the part both front ends share.
pub(crate) fn run(
    ed: &mut Editor,
    chosen: Vec<EntityId>,
    settings: Settings,
) -> Result<(), Cancelled> {
    let prepared = ed.read(move |doc| prepare(doc, &chosen, &settings, false))?;
    if prepared.skipped > 0 {
        ed.say(Wording::count(
            prepared.skipped,
            key!("%{count} object on locked or hidden layers left alone."),
            key!("%{count} objects on locked or hidden layers left alone."),
        ));
    }
    let outcome = match overkill(&prepared.items, &settings) {
        Ok(outcome) => outcome,
        Err(why) => {
            ed.say(crate::message_language::because(
                key!("OVERKILL did nothing: %{why}."),
                why,
            ));
            return Ok(());
        }
    };
    if outcome.removed.is_empty() && outcome.changed.is_empty() {
        ed.say(key!("No duplicate or overlapping objects found."));
        return Ok(());
    }
    let merged = outcome.changed.len();
    let deleted = outcome.removed.len();
    let tied = prepared.protected();
    let ids = prepared.ids;
    ed.transaction("Overkill", move |tx| {
        let changes = outcome
            .changed
            .into_iter()
            .map(|(i, entity)| (ids[i], entity))
            .collect::<Vec<_>>();
        if !changes.is_empty() {
            tx.perform(ModifyEntities::new("Overkill", changes));
        }
        let gone: Vec<EntityId> = outcome.removed.iter().map(|i| ids[*i]).collect();
        if !gone.is_empty() {
            tx.perform(RemoveEntities::new(gone));
        }
        Ok(())
    })?;
    let said = Wording::new(key!("%{deleted} deleted, %{merged} combined."), &[]);
    let said = said.part("deleted", crate::text::objects(deleted));
    ed.say(said.part("merged", crate::text::objects(merged)));
    if tied > 0 {
        ed.say(Wording::count(
            tied,
            key!("%{count} object that other objects are tied to was not deleted or changed."),
            key!("%{count} objects that other objects are tied to were not deleted or changed."),
        ));
    }
    Ok(())
}

#[cfg(test)]
#[path = "overkill_cmd_tests.rs"]
mod tests;
