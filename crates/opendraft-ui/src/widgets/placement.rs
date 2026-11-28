//! Putting a window of its own where it was meant to be.
//!
//! egui turns the position and size in a viewport's builder into pixels with
//! the scale of the *primary* monitor, because it does not know which monitor
//! the window will open on, and its later correction does not always land. A
//! window meant for a monitor of another scale opens in the wrong place, or
//! off every screen. So a position is kept in the pixels of the whole screen
//! (the same whatever monitor the window is on), left out of the builder, and
//! applied by commands from inside the window, once the window exists and its
//! own scale is known.

use egui::{Context, Id, Pos2};

/// How many frames to keep trying before giving up (a window the system
/// refuses to place must not hold the app in a loop).
const TRIES: u8 = 30;

/// How near, in pixels, counts as there.
const CLOSE: f32 = 2.0;

/// What `settle` last aimed at, how often it has tried, and whether it is done.
#[derive(Clone, Copy, PartialEq)]
struct State {
    aim: Pos2,
    tries: u8,
    done: bool,
}

/// Move the window `ctx` is drawing so that its outer top-left corner is at
/// `at`, in pixels of the whole screen. Call it each frame from inside the
/// window's own callback; true once the window is there (or the tries are
/// used up), when it may be left alone. `key` is the window's own.
pub fn settle(ctx: &Context, key: Id, at: Pos2) -> bool {
    let mut state = ctx
        .data(|d| d.get_temp::<State>(key))
        .filter(|s| s.aim == at)
        .unwrap_or(State {
            aim: at,
            tries: 0,
            done: false,
        });
    if !state.done {
        let ppp = ctx.pixels_per_point();
        match ctx.input(|i| i.viewport().outer_rect) {
            Some(outer)
                if ((outer.min.to_vec2() * ppp) - at.to_vec2())
                    .abs()
                    .max_elem()
                    <= CLOSE =>
            {
                state.done = true;
            }
            Some(_) if state.tries < TRIES => {
                // The command is in this window's points, which are pixels
                // over this window's own scale.
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(at / ppp));
                state.tries += 1;
                ctx.request_repaint();
            }
            Some(_) => state.done = true,
            None => ctx.request_repaint(),
        }
    }
    ctx.data_mut(|d| d.insert_temp(key, state));
    state.done
}

/// Forget where `key` was put, so the next window with it is placed afresh.
pub fn forget(ctx: &Context, key: Id) {
    ctx.data_mut(|d| d.remove::<State>(key));
}
