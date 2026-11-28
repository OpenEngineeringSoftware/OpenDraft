//! The app's side of `Wants::XrefLoad`: read the source and plan it, then
//! answer the waiting command.
//!
//! The work is done between frames, so a command never touches the file
//! system itself. A source that takes long enough to notice will move to a
//! worker in a later step; for now the plan is built before the next frame,
//! and a failure is a message rather than a change.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use opendraft_commands::{Reply, Wants};
use opendraft_core::XrefLoadRequest;
use opendraft_files::{plan_xref, read_source, resolve, XrefLimits, XrefPlanRequest};

use crate::state::AppState;

impl AppState {
    /// The folder of the active drawing, for resolving a relative xref path.
    /// `None` when it has never been saved.
    pub(crate) fn xref_host_dir(&self) -> Option<String> {
        self.docs
            .get(self.active)?
            .path
            .as_ref()?
            .parent()
            .map(|dir| dir.to_string_lossy().into_owned())
    }

    /// Answer the running command's request for an external-reference plan,
    /// when it has one outstanding.
    pub(crate) fn sync_xref_load(&mut self) {
        let Some(request) = self
            .waiting()
            .filter(|ask| ask.wants == Wants::XrefLoad)
            .and_then(|ask| ask.xref_load.as_deref().cloned())
        else {
            return;
        };
        let reply = self.read_and_plan(&request);
        self.reply(reply);
    }

    fn read_and_plan(&self, request: &XrefLoadRequest) -> Reply {
        let host_dir = request.host_dir.as_deref().map(Path::new);
        let path = match resolve(&request.source_path, host_dir) {
            Ok(path) => path,
            Err(why) => return Reply::XrefFailed(why.to_string()),
        };
        let source = match read_source(&path, &AtomicBool::new(false), &XrefLimits::default()) {
            Ok(source) => source,
            Err(why) => return Reply::XrefFailed(why.to_string()),
        };
        let Some(doc) = self.docs.get(self.run_doc) else {
            return Reply::XrefFailed("there is no drawing to attach it to".to_owned());
        };
        let plan_request = XrefPlanRequest {
            block: request.block,
            name: request.name.clone(),
            source_path: request.source_path.clone(),
            path_type: request.path_type,
            kind: request.kind,
            host_dir: request.host_dir.clone(),
            host_path: doc
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned()),
        };
        match plan_xref(
            doc.session.document(),
            &source.document,
            &plan_request,
            &XrefLimits::default(),
            &AtomicBool::new(false),
        ) {
            Ok(mut plan) => {
                plan.xref.stamp = source.stamp;
                Reply::XrefPlan(Box::new(plan))
            }
            Err(why) => Reply::XrefFailed(why.to_string()),
        }
    }
}
