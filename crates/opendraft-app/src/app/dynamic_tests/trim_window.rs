use super::*;

fn left(pos: Pos2, pressed: bool) -> Event {
    Event::PointerButton {
        pos,
        button: egui::PointerButton::Primary,
        pressed,
        modifiers: Modifiers::NONE,
    }
}

/// One event a frame, a fiftieth of a second apart.
fn play(app: &mut OpenDraftApp, ctx: &egui::Context, events: Vec<Event>, time: &mut f64) {
    for event in events {
        *time += 0.02;
        frame(app, ctx, vec![event], *time);
    }
    *time += 0.02;
    frame(app, ctx, Vec::new(), *time);
}

fn click(app: &mut OpenDraftApp, ctx: &egui::Context, at: Pos2, time: &mut f64) {
    let events = vec![Event::PointerMoved(at), left(at, true), left(at, false)];
    play(app, ctx, events, time);
    // Let the command take the answer and ask again, then show that.
    app.state.settle_command();
    play(app, ctx, Vec::new(), time);
}

#[test]
fn the_click_that_ends_a_trim_window_does_not_start_another() {
    let (mut app, ctx) = (app(), egui::Context::default());
    // Dynamic input off: its fields follow the pointer and would take the
    // second click.
    app.state.settings.dynamic_input.toggle();
    // The first frame, at time 0, sets up the theme and fonts.
    frame(&mut app, &ctx, Vec::new(), 0.0);
    let mut time = 0.0;
    app.state.start_command("Trim", crate::edges::trim_objects);
    app.state.settle_command();
    play(&mut app, &ctx, Vec::new(), &mut time);
    let canvas = app.state.docs[0].dynamic_input.canvas.unwrap();

    click(&mut app, &ctx, canvas.center(), &mut time);
    assert!(app
        .state
        .cmd_prompt()
        .text
        .starts_with("Specify opposite corner"));
    click(
        &mut app,
        &ctx,
        canvas.center() + Vec2::new(80.0, 40.0),
        &mut time,
    );

    let prompt = app.state.cmd_prompt().text;
    assert!(
        prompt.starts_with("Quick mode, select object to trim"),
        "{prompt} after {:?}",
        app.state.cmdline.log
    );
    assert!(app.state.command_running());
}
