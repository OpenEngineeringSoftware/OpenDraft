//! Undoable changes to the external-reference table and `VISRETAIN`.

use opendraft_core::{Document, XrefDef};

use crate::{Action, Changes};

/// Replace the whole external-reference table (an attach, reload, unload,
/// detach or path edit). The blocks, layers and inserts that belong to the
/// reference are changed by other actions in the same transaction.
pub struct SetXrefs {
    name: String,
    /// Applying and reverting both swap, so each leaves the other state ready.
    xrefs: Vec<XrefDef>,
}

impl SetXrefs {
    pub fn new(name: impl Into<String>, xrefs: Vec<XrefDef>) -> Self {
        Self {
            name: name.into(),
            xrefs,
        }
    }

    fn swap(&mut self, doc: &mut Document) {
        self.xrefs = doc.set_xrefs(std::mem::take(&mut self.xrefs));
    }
}

impl Action for SetXrefs {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn apply(&mut self, doc: &mut Document) {
        self.swap(doc);
    }

    fn revert(&mut self, doc: &mut Document) {
        self.swap(doc);
    }

    fn changes(&self) -> Changes {
        Changes::xrefs()
    }
}

/// Set `VISRETAIN` (whether host xref-layer properties survive a reload).
pub struct SetVisretain {
    name: String,
    keep: bool,
}

impl SetVisretain {
    pub fn new(name: impl Into<String>, keep: bool) -> Self {
        Self {
            name: name.into(),
            keep,
        }
    }
}

impl Action for SetVisretain {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn apply(&mut self, doc: &mut Document) {
        doc.set_visretain(self.keep);
    }

    fn revert(&mut self, doc: &mut Document) {
        doc.set_visretain(!self.keep);
    }

    fn changes(&self) -> Changes {
        Changes::xrefs()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_table_changes_and_comes_back() {
        let mut doc = Document::default();
        let mut action = SetXrefs::new("Attach external reference", Vec::new());
        action.apply(&mut doc);
        assert!(doc.xrefs().is_empty());
        action.revert(&mut doc);
        assert!(doc.xrefs().is_empty());
        assert_eq!(action.changes(), Changes::xrefs());
        assert_eq!(doc.validate(), Ok(()));
    }

    #[test]
    fn visretain_changes_and_comes_back() {
        let mut doc = Document::default();
        assert!(doc.visretain());
        let mut action = SetVisretain::new("Keep xref layer properties", false);
        action.apply(&mut doc);
        assert!(!doc.visretain());
        action.revert(&mut doc);
        assert!(doc.visretain());
        assert!(action.changes().xrefs);
    }
}
