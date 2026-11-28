//! A cancelled command gets a moment to do what it does when cancelled
//! before it is dropped: PLINE adds the polyline drawn so far, on Esc and
//! when another command takes over, as in AutoCAD.

use std::time::Duration;

use opendraft_commands::Runner;

use crate::state::AppState;

/// How long a cancelled command is waited for, per request, to end.
const SETTLE: Duration = Duration::from_millis(100);

impl AppState {
    /// Cancel `runner`, letting what it keeps go into the drawing it ran in.
    pub(crate) fn settle_cancelled(&mut self, runner: &mut Runner) {
        // A read for the command (an external reference's source) goes too.
        self.cancel_xref_job();
        match self.docs.get_mut(self.run_doc) {
            Some(doc) => runner.cancel_and_settle(&mut doc.session, SETTLE),
            None => runner.cancel(),
        }
    }
}
