//! What a DXF import tells the user it could not keep or understand, in the
//! order the drawing was read.

use super::Importer;

impl Importer {
    /// Everything skipped or approximated, one line each.
    pub(super) fn warnings(&self) -> Vec<String> {
        let mut warnings = self.ltypes.warnings();
        if !self.skipped.is_empty() {
            let total: usize = self.skipped.values().sum();
            let kinds: Vec<String> = self
                .skipped
                .iter()
                .map(|(k, n)| format!("{n} {k}"))
                .collect();
            warnings.push(format!(
                "Skipped {total} unsupported object{}: {}.",
                if total == 1 { "" } else { "s" },
                kinds.join(", ")
            ));
        }
        if self.paper_space > 0 {
            warnings.push(format!("Skipped {} paper space objects.", self.paper_space));
        }
        if self.invisible > 0 {
            let objects = crate::plural(self.invisible as u64, "object");
            warnings.push(format!("Skipped {objects} marked invisible."));
        }
        if self.damaged > 0 {
            warnings.push(format!("Skipped {} damaged objects.", self.damaged));
        }
        warnings.extend(super::clips::warning(self.clipped));
        warnings.extend(self.blocks.warnings());
        warnings.extend(super::super::ccs::warning(self.tilted_ccs));
        warnings
    }
}
