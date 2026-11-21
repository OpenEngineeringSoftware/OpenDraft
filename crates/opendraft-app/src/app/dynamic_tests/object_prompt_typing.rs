use super::*;

use opendraft_core::Entity;

/// One event a frame, a fiftieth of a second apart.
fn play(app: &mut OpenDraftApp, ctx: &egui::Context, events: Vec<Event>, time: &mut f64) {
    for event in events {
        *time += 0.02;
        frame(app, ctx, vec![event], *time);
    }
    *time += 0.02;
    frame(app, ctx, Vec::new(), *time);
}

fn upright() -> Entity {
    Entity::Line {
        start: Point::new(0.0, -100.0),
        end: Point::new(0.0, 100.0),
    }
}

/// A cross of two lines, TRIM with every object as an edge, and the top of
/// the upright line clicked away.
fn trimmed(dynamic: bool) -> (OpenDraftApp, egui::Context, f64) {
    let (mut app, ctx) = (app(), egui::Context::default());
    if !dynamic {
        app.state.settings.dynamic_input.toggle();
    }
    frame(&mut app, &ctx, Vec::new(), 0.0);
    let mut time = 0.0;
    app.state.add_entity(Entity::Line {
        start: Point::new(-100.0, 0.0),
        end: Point::new(100.0, 0.0),
    });
    app.state.add_entity(upright());
    app.state.start_command("Trim", crate::edges::trim_objects);
    app.state.settle_command();
    play(&mut app, &ctx, Vec::new(), &mut time);
    let doc = &app.state.docs[0];
    let canvas = doc.dynamic_input.canvas.unwrap();
    let at = doc
        .view
        .camera
        .world_to_screen(Point::new(0.0, 50.0), canvas);
    play(&mut app, &ctx, click(at), &mut time);
    app.state.settle_command();
    play(&mut app, &ctx, Vec::new(), &mut time);
    assert_ne!(second(&app), upright(), "the click trimmed");
    (app, ctx, time)
}

fn second(app: &OpenDraftApp) -> Entity {
    app.state.docs[0].session.document().objects()[1]
        .entity
        .clone()
}

#[test]
fn the_first_letter_typed_at_an_object_prompt_is_kept() {
    for dynamic in [true, false] {
        let (mut app, ctx, mut time) = trimmed(dynamic);
        // Typed in the drawing: nothing has the keyboard yet.
        let mut events = vec![Event::Text("u".into())];
        events.extend(key(Key::Enter));
        play(&mut app, &ctx, events, &mut time);
        app.state.settle_command();
        play(&mut app, &ctx, Vec::new(), &mut time);

        assert_eq!(second(&app), upright(), "dynamic {dynamic}: Undo ran");
        assert!(
            app.state.command_running(),
            "dynamic {dynamic}: TRIM goes on"
        );
    }
}

#[test]
fn letters_typed_one_by_one_at_an_object_prompt_stay_in_order() {
    let (mut app, ctx, mut time) = trimmed(true);
    let events = vec![Event::Text("u".into()), Event::Text("n".into())];
    play(&mut app, &ctx, events, &mut time);
    assert_eq!(app.state.docs[0].dynamic_input.draft, "un");
}
