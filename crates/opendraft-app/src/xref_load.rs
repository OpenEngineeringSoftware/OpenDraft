//! The app's side of `Wants::XrefLoad`: read the source and plan it on a
//! worker, then answer the waiting command.
//!
//! A command never touches the file system itself. The job gets a copy of
//! the host drawing and a cancel flag, so a large source never freezes the
//! window and giving up on the command gives up on the read. A failure is a
//! message rather than a change.

use std::path::Path;
use std::sync::atomic::AtomicBool;

use opendraft_commands::{Reply, Wants};
use opendraft_core::{Document, XrefLoadRequest};
use opendraft_files::{plan_xref, read_source, resolve, XrefLimits};

use crate::io::Done;
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

    /// Start the read the running command is waiting for, if none is under
    /// way for that request, and collect any that has finished.
    pub(crate) fn sync_xref_load(&mut self) {
        let Some(request) = self
            .waiting()
            .filter(|ask| ask.wants == Wants::XrefLoad)
            .and_then(|ask| ask.xref_load.as_deref().cloned())
        else {
            return;
        };
        let running = self
            .xref_job
            .as_ref()
            .is_some_and(|(id, started)| *started == request && self.io.is_live(*id));
        if !running {
            if let Some((id, _)) = self.xref_job.take() {
                self.io.cancel_job(id);
            }
            let Some(doc) = self.docs.get(self.run_doc) else {
                self.reply(Reply::XrefFailed(
                    "there is no drawing to attach it to".to_owned(),
                ));
                return;
            };
            let host = doc.session.document().clone();
            let mut planned = request.clone();
            planned.host_path = doc
                .path
                .as_ref()
                .map(|path| path.to_string_lossy().into_owned());
            let key = request.clone();
            let id = self
                .io
                .submit_cancellable(move |cancelled| Done::XrefPlanned {
                    request: Box::new(key),
                    reply: Box::new(read_and_plan(&host, &planned, cancelled)),
                });
            self.xref_job = Some((id, request));
        }
        self.poll_io();
    }

    /// A finished read: answer the command if it still waits for it.
    pub(crate) fn finish_xref_plan(&mut self, request: &XrefLoadRequest, reply: Reply) {
        if self
            .xref_job
            .as_ref()
            .is_some_and(|(_, job)| job == request)
        {
            self.xref_job = None;
        }
        let wanted = self
            .waiting()
            .filter(|ask| ask.wants == Wants::XrefLoad)
            .and_then(|ask| ask.xref_load.as_deref())
            .is_some_and(|waiting| waiting == request);
        if wanted {
            self.reply(reply);
        }
    }
}

fn read_and_plan(host: &Document, request: &XrefLoadRequest, cancelled: &AtomicBool) -> Reply {
    let host_dir = request.host_dir.as_deref().map(Path::new);
    let path = match resolve(&request.source_path, host_dir) {
        Ok(path) => path,
        Err(why) => return Reply::XrefFailed(why.to_string()),
    };
    let source = match read_source(&path, cancelled, &XrefLimits::default()) {
        Ok(source) => source,
        Err(why) => return Reply::XrefFailed(why.to_string()),
    };
    match plan_xref(
        host,
        &source.document,
        request,
        &XrefLimits::default(),
        cancelled,
    ) {
        Ok(mut plan) => {
            plan.xref.stamp = source.stamp;
            Reply::XrefPlan(Box::new(plan))
        }
        Err(why) => Reply::XrefFailed(why.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::{XrefKind, XrefPathType};

    use super::*;

    fn request(name: &str) -> XrefLoadRequest {
        XrefLoadRequest {
            block: None,
            name: name.to_owned(),
            source_path: "/work/base.dwg".to_owned(),
            path_type: XrefPathType::Full,
            kind: XrefKind::Attach,
            host_dir: None,
            host_path: None,
        }
    }

    #[test]
    fn a_result_for_another_request_is_dropped_and_the_command_keeps_waiting() {
        let mut state = AppState::new();
        let wanted = request("base");
        let asked = wanted.clone();
        state.start_command("-XREF", move |ed| {
            let _ = ed.ask_xref_plan(asked)?;
            Ok(())
        });
        state.settle_command();
        assert_eq!(state.waiting().map(|ask| ask.wants), Some(Wants::XrefLoad));

        state.finish_xref_plan(&request("other"), Reply::XrefFailed("old".to_owned()));
        state.settle_command();
        assert_eq!(
            state.waiting().map(|ask| ask.wants),
            Some(Wants::XrefLoad),
            "a stale answer does not end the wait"
        );

        state.finish_xref_plan(&wanted, Reply::XrefFailed("gone".to_owned()));
        state.settle_command();
        assert!(!state.command_running());
    }

    #[test]
    fn a_job_the_user_gave_up_on_is_started_again() {
        let mut state = AppState::new();
        state.start_command("-XREF", |ed| {
            let _ = ed.ask_xref_plan(request("base"))?;
            Ok(())
        });
        state.settle_command();
        state.xref_job = Some((u64::MAX, request("base")));
        state.sync_xref_load();
        state.settle_command();
        assert!(
            !state.command_running(),
            "the dead job was replaced and its answer (a missing file) ended the command"
        );
    }
}
