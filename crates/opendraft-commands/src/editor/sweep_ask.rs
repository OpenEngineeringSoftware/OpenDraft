//! Asking for an object, or a sweep across many: TRIM and EXTEND. And for
//! their edges, with options.

use opendraft_core::EntityId;

use super::{Answer, Cancelled, Editor, Hint, Options, Picked, Prompt, Reply, Swept, Wants};

impl Editor {
    /// Ask the user to select objects, like
    /// [`ask_selection`](Self::ask_selection), but `options`' keywords
    /// answer too, and `default` is shown as the prompt's default
    /// ("<select all>").
    pub fn ask_selection_or_keyword(
        &self,
        options: impl Into<Options<()>>,
        default: &str,
    ) -> Result<Answer<Vec<EntityId>>, Cancelled> {
        let options = options.into();
        let hint = Some(Hint::Text(default.to_owned()));
        match self.ask(Prompt::new(Wants::Selection, &options, hint))? {
            Reply::Selection(ids) if !ids.is_empty() || options.allow_empty => {
                Ok(Answer::Value(ids))
            }
            Reply::Keyword(label) if options.keywords.iter().any(|k| k.label == label) => {
                Ok(Answer::Keyword(label))
            }
            _ => Err(Cancelled),
        }
    }

    /// Ask for an object as [`ask_object`](Self::ask_object) does, but a
    /// click in empty space or a path dragged from there answers too (TRIM
    /// and EXTEND take a crossing window or a freehand path that way).
    pub fn ask_object_or_sweep(
        &self,
        options: impl Into<Options<Swept>>,
    ) -> Result<Answer<Swept>, Cancelled> {
        self.ask_value(
            Wants::Object,
            &options.into().sweep(),
            |_| Hint::Text(String::new()),
            |r| match r {
                Reply::Object { id, at, shift } => Some(Swept::Object(Picked { id, at, shift })),
                Reply::Missed { at, shift } => Some(Swept::Missed { at, shift }),
                Reply::Path { points, shift } => Some(Swept::Path { points, shift }),
                _ => None,
            },
        )
    }
}
