//! ZOOM (`Z`): a window given by two corners, as a click with no option does
//! in AutoCAD, or the Extents or Previous view. The command only asks; the
//! drawing's view is changed on the UI thread, through the drawing's
//! `zoom_to` slot, as soon as the answer is in.

use std::sync::{Arc, Mutex};

use opendraft_commands::{Answer, Cancelled, Editor, Options};
use opendraft_core::{Entity, Point};

use crate::state::AppState;

/// What a ZOOM command asked for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ZoomTo {
    Window(Point, Point),
    Extents,
    Previous,
}

/// Where a running ZOOM leaves its answer for the drawing it started in.
pub type ZoomSlot = Arc<Mutex<Option<ZoomTo>>>;

fn first_corner(prompt: &str) -> Options<Point> {
    Options::new(prompt)
        .keyword("Extents", "E")
        .keyword("Previous", "P")
        .keyword("Window", "W")
        .no_snap()
}

/// The opposite corner: a plain rectangle follows the pointer, with no
/// dimensions and no snaps (a view is not a drawn object). There is no base
/// point, as dynamic input would draw a length and an angle from it.
fn opposite_corner(first: Point) -> Options<Point> {
    Options::new("Specify opposite corner")
        .no_snap()
        .ghost(move |at| {
            vec![Entity::Rectangle {
                min: first,
                max: at,
            }]
        })
}

/// Ask what to zoom to. Enter ends with nothing changed.
fn ask_zoom(ed: &Editor) -> Result<Option<ZoomTo>, Cancelled> {
    let mut prompt = first_corner("Specify first corner of window or [Extents/Previous/Window]");
    loop {
        let first = match ed.ask_point(prompt)? {
            Answer::Value(at) => at,
            Answer::Keyword(k) if k == "Extents" => return Ok(Some(ZoomTo::Extents)),
            Answer::Keyword(k) if k == "Previous" => return Ok(Some(ZoomTo::Previous)),
            Answer::Keyword(_) => {
                // Window: the corners are asked anyway.
                prompt = first_corner("Specify first corner of window");
                continue;
            }
            Answer::Enter => return Ok(None),
        };
        let second = match ed.ask_point(opposite_corner(first))? {
            Answer::Value(at) => at,
            _ => return Err(Cancelled),
        };
        if first.x != second.x || first.y != second.y {
            return Ok(Some(ZoomTo::Window(first, second)));
        }
        ed.say("The window has no size.");
        prompt = first_corner("Specify first corner of window or [Extents/Previous/Window]");
    }
}

impl AppState {
    /// Start ZOOM on the active drawing.
    pub fn start_zoom(&mut self) {
        let slot = Arc::clone(&self.docs[self.active].zoom_to);
        self.start_command("ZOOM", move |ed| {
            if let Some(to) = ask_zoom(ed)? {
                if let Ok(mut place) = slot.lock() {
                    *place = Some(to);
                }
            }
            Ok(())
        });
    }

    /// Change the view of each drawing whose ZOOM has answered.
    pub(crate) fn take_zoom_requests(&mut self) {
        let mut no_previous = false;
        for doc in &mut self.docs {
            let to = match doc.zoom_to.lock() {
                Ok(mut place) => place.take(),
                Err(_) => None,
            };
            match to {
                Some(ZoomTo::Window(a, b)) => doc.view.request_zoom_window(a, b),
                Some(ZoomTo::Extents) => doc.view.request_zoom_extents(),
                Some(ZoomTo::Previous) => no_previous |= !doc.view.zoom_previous(),
                None => {}
            }
        }
        if no_previous {
            self.say_no_previous_view();
        }
    }

    /// ZOOM Previous with nothing to go back to.
    pub(crate) fn say_no_previous_view(&mut self) {
        let line = "No previous view saved.".to_owned();
        self.message.clone_from(&line);
        self.cmdline.say(line);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asked(state: &AppState) -> String {
        state
            .waiting()
            .map(|a| a.message.clone())
            .unwrap_or_default()
    }

    fn zoomed(state: &AppState) -> Option<ZoomTo> {
        *state.docs[state.active].zoom_to.lock().unwrap()
    }

    #[test]
    fn two_corners_zoom_to_their_window() {
        let mut state = AppState::new();
        state.start_zoom();
        state.settle_command();
        assert!(asked(&state).starts_with("Specify first corner of window"));
        assert!(state.waiting().is_some_and(|ask| ask.no_snap));
        state.answer_point(Point::new(10.0, 10.0));
        state.settle_command();
        assert_eq!(asked(&state), "Specify opposite corner");
        // No dimensions and no snaps on the window (it is not drawn).
        let ask = state.waiting().unwrap();
        assert!(ask.no_snap);
        assert_eq!(
            ask.dynamic_dimensions,
            opendraft_commands::DynamicDimensions::None
        );
        assert!(ask.base.is_none());
        state.answer_point(Point::new(50.0, 30.0));
        state.settle_command();
        assert!(state.runner.is_none());
        // The slot was emptied into the view by the time the command ended.
        assert_eq!(zoomed(&state), None);
    }

    #[test]
    fn the_same_point_twice_is_asked_again() {
        let mut state = AppState::new();
        state.start_zoom();
        state.settle_command();
        state.answer_point(Point::new(10.0, 10.0));
        state.settle_command();
        state.answer_point(Point::new(10.0, 10.0));
        state.settle_command();
        assert!(state.runner.is_some());
        assert!(asked(&state).starts_with("Specify first corner of window"));
    }

    #[test]
    fn window_extents_and_previous_answers() {
        let corners = Some(ZoomTo::Window(Point::new(0.0, 0.0), Point::new(4.0, 2.0)));
        for (keys, expected) in [
            (vec!["E"], Some(ZoomTo::Extents)),
            (vec!["P"], Some(ZoomTo::Previous)),
            (vec!["W"], corners),
        ] {
            let ed_answer = std::sync::Arc::new(Mutex::new(None));
            let slot = Arc::clone(&ed_answer);
            let mut state = AppState::new();
            state.start_command("ZOOM", move |ed| {
                *slot.lock().unwrap() = ask_zoom(ed)?;
                Ok(())
            });
            state.settle_command();
            for key in keys {
                assert!(state.answer_typed(key));
                state.settle_command();
            }
            if state.runner.is_some() {
                state.answer_point(Point::new(0.0, 0.0));
                state.settle_command();
                assert_eq!(asked(&state), "Specify opposite corner");
                state.answer_point(Point::new(4.0, 2.0));
                state.settle_command();
            }
            assert_eq!(*ed_answer.lock().unwrap(), expected);
        }
    }

    #[test]
    fn previous_with_nothing_to_go_back_to_says_so() {
        let mut state = AppState::new();
        state.start_zoom();
        state.settle_command();
        assert!(state.answer_typed("P"));
        state.settle_command();
        assert!(state.runner.is_none());
        assert_eq!(state.message, "No previous view saved.");
    }
}
