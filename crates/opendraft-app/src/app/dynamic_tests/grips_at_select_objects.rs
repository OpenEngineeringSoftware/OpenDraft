use super::*;

use opendraft_core::Entity;

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

#[test]
fn a_command_asking_for_objects_offers_no_grips() {
    let (mut app, ctx) = (app(), egui::Context::default());
    app.state.settings.dynamic_input.toggle();
    frame(&mut app, &ctx, Vec::new(), 0.0);
    let mut time = 0.0;
    app.state.add_entity(Entity::Line {
        start: Point::new(0.0, 0.0),
        end: Point::new(100.0, 0.0),
    });
    // MOVE with nothing selected asks for objects; the line is picked there.
    app.state.start_command("Move", crate::modify::move_objects);
    app.state.settle_command();
    let line = app.state.docs[0].session.document().objects()[0].id;
    app.state.select_ids([line], false);
    play(&mut app, &ctx, Vec::new(), &mut time);
    assert!(app.state.cmd_prompt().text.starts_with("Select objects"));

    // A press on the line's end, where its grip would be, starts no grip
    // edit: MOVE is still asking for objects and the line is unchanged.
    let doc = &app.state.docs[0];
    let canvas = doc.dynamic_input.canvas.unwrap();
    let end = doc
        .view
        .camera
        .world_to_screen(Point::new(100.0, 0.0), canvas);
    let drag = end + Vec2::new(40.0, 30.0);
    let events = vec![
        Event::PointerMoved(end),
        left(end, true),
        Event::PointerMoved(drag),
        left(drag, false),
    ];
    play(&mut app, &ctx, events, &mut time);
    app.state.settle_command();
    play(&mut app, &ctx, Vec::new(), &mut time);
    let prompt = app.state.cmd_prompt().text;
    assert!(
        prompt.starts_with("Select objects"),
        "{prompt} after {:?}",
        app.state.cmdline.log
    );
    assert_eq!(
        app.state.docs[0].session.document().objects()[0].entity,
        Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(100.0, 0.0),
        }
    );
}
