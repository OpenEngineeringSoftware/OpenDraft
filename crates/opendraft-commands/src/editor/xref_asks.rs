//! Asking the app to read an external reference's source or to open its
//! attach dialog. Kept apart from the other asks only to keep `editor.rs`
//! within the size rule.

use opendraft_core::{XrefAttachDraft, XrefLoadRequest, XrefPlan};

use super::{Cancelled, Editor, Options, Prompt, Reply, Wants};

impl Editor {
    /// Ask the app to read and plan an external reference's source. The
    /// answer is the plan, or why it failed; the command goes on either way.
    pub fn ask_xref_plan(
        &self,
        request: XrefLoadRequest,
    ) -> Result<Result<XrefPlan, String>, Cancelled> {
        let options = Options::<()>::new("Reading the external reference");
        let mut prompt = Prompt::new(Wants::XrefLoad, &options, None);
        prompt.xref_load = Some(Box::new(request));
        match self.ask(prompt)? {
            Reply::XrefPlan(plan) => Ok(Ok(*plan)),
            Reply::XrefFailed(why) => Ok(Err(why)),
            _ => Err(Cancelled),
        }
    }

    /// Open the Attach External Reference dialog on `draft`.
    pub fn ask_xref_attach(&self, draft: XrefAttachDraft) -> Result<XrefAttachDraft, Cancelled> {
        let options = Options::<()>::new("Attach External Reference");
        let mut prompt = Prompt::new(Wants::XrefAttachDialog, &options, None);
        prompt.xref_attach = Some(Box::new(draft));
        match self.ask(prompt)? {
            Reply::XrefAttach(draft) => Ok(*draft),
            _ => Err(Cancelled),
        }
    }
}
