//! TEXTEDIT (and double-clicking a single-line text): change the words of a
//! text already in the drawing. A multiline text opens the in-place editor,
//! as MTEDIT does; a single-line text opens the same editor as one line of
//! plain text, in its own place on the drawing, as TEXT does when it is made.

use opendraft_commands::{Answer, Cancelled, Editor, Options, Picked};
use opendraft_core::{Entity, EntityId, SingleLineText, TextStyle};
use opendraft_ui::key;

use crate::defaults::EditorDefaults;
use crate::mtext::edit_mtext_with_id;

/// The kinds TEXTEDIT edits.
fn is_text(entity: Option<&Entity>) -> bool {
    matches!(entity, Some(Entity::Text(_) | Entity::MText(_)))
}

/// Whether `id` is a text TEXTEDIT edits.
fn is_text_id(ed: &Editor, id: EntityId) -> Result<bool, Cancelled> {
    ed.read(move |doc| is_text(doc.get(id)))
}

/// Change the words of the text `id` in place. Says whether anything
/// changed (one undo step).
fn edit_one(ed: &mut Editor, id: EntityId) -> Result<bool, Cancelled> {
    let found = ed.read(move |doc| match doc.get(id) {
        Some(Entity::Text(text)) => {
            let style = doc.text_style(text.style()).cloned();
            Some(((**text).clone(), style))
        }
        _ => None,
    })?;
    let Some((original, style)) = found else {
        return edit_mtext_with_id(ed, id);
    };
    let Some(typed) = ed.edit_line(Some(id), original.projection())? else {
        return Ok(false);
    };
    let contents = SingleLineText::contents_of_projection(&typed.contents);
    if contents == original.contents {
        return Ok(false);
    }
    let style = style.unwrap_or_else(TextStyle::standard);
    let edited = match original.with_contents(contents, &style) {
        Ok(edited) => edited,
        Err(why) => {
            ed.say(crate::message_language::because(
                key!("The text was not changed: %{why}."),
                why,
            ));
            return Ok(false);
        }
    };
    ed.transaction("Edit text", move |tx| {
        tx.modify(id, |_| Entity::Text(Box::new(edited.clone())));
        Ok(())
    })?;
    Ok(true)
}

/// Mode: keep asking for texts (Multiple), or end after one (Single). Enter
/// keeps the mode as it is.
fn ask_mode(ed: &Editor) -> Result<(), Cancelled> {
    let current = if ed.defaults().text.get().multiple {
        "Multiple"
    } else {
        "Single"
    };
    let options = Options::<String>::new(key!("Enter a text edit mode option"))
        .keyword("Single", "S")
        .keyword("Multiple", "M")
        .default(current.to_owned());
    if let Some(word) = ed.ask_keyword(options)? {
        ed.defaults()
            .text
            .change(|m| m.multiple = word == "Multiple");
    }
    Ok(())
}

/// TEXTEDIT: change the words of a text in place. A text already selected is
/// edited and the command ends. Otherwise it asks for a text, edits it and,
/// in Multiple mode (the default), asks for the next, until Enter. Undo takes
/// back the last edit; Mode switches between Single and Multiple. Only the
/// words change, each edit one undo step; Esc ends the command.
pub fn edit_text(ed: &mut Editor) -> Result<(), Cancelled> {
    if let [id] = ed.pickset() {
        let id = *id;
        if is_text_id(ed, id)? {
            return edit_one(ed, id).map(|_| ());
        }
    }
    let mut edits = 0usize;
    loop {
        let options = Options::<Picked>::new(key!("Select an annotation object"))
            .keyword("Undo", "U")
            .keyword("Mode", "M");
        match ed.ask_object(options)? {
            Answer::Value(picked) if is_text_id(ed, picked.id)? => {
                if edit_one(ed, picked.id)? {
                    edits += 1;
                }
                if !ed.defaults().text.get().multiple {
                    return Ok(());
                }
            }
            Answer::Value(_) => ed.say(key!("That is not a text object.")),
            Answer::Keyword(word) if word == "Undo" => {
                if edits == 0 {
                    ed.say(key!("Nothing to undo."));
                } else {
                    ed.undo()?;
                    edits -= 1;
                }
            }
            Answer::Keyword(_) => ask_mode(ed)?,
            Answer::Enter => return Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use opendraft_commands::Wants;
    use opendraft_core::block::{TextHorizontal, TextVertical};
    use opendraft_core::{MText, Point, TextStyleId};

    use super::*;
    use crate::mtext_editor::Outcome;
    use crate::state::AppState;

    fn room() -> SingleLineText {
        let mut text =
            SingleLineText::new(Point::new(10.0, 20.0), 2.5, "ROOM 1", TextStyleId::STANDARD);
        text.placement.rotation = 0.25;
        text.placement.horizontal = TextHorizontal::Center;
        text.placement.vertical = TextVertical::Middle;
        text
    }

    fn state_with(entity: Entity) -> AppState {
        let mut state = AppState::new();
        state.add_entity(entity);
        state
    }

    fn start(state: &mut AppState) {
        state.start_command("TEXTEDIT", edit_text);
        state.settle_command();
    }

    /// The editor is open on the text; type into it and close it.
    fn edit_in_place(state: &mut AppState, typed: &str, outcome: Outcome) {
        let ask = state.waiting().expect("the editor waits");
        assert_eq!(ask.wants, Wants::MTextEditor);
        state.sync_mtext_editor();
        assert!(state.mtext_editing());
        let active = state.active;
        if let Some(editor) = state.docs[active].mtext_editor.as_mut() {
            if !typed.is_empty() {
                editor.insert(typed, 0.0);
            }
        }
        state.finish_mtext_editor(outcome);
        state.settle_command();
    }

    fn first(state: &AppState) -> Entity {
        state.session().document().objects()[0].entity.clone()
    }

    #[test]
    fn the_words_change_in_place_and_one_undo_brings_them_back() {
        let mut state = state_with(Entity::Text(Box::new(room())));
        state.select_all();
        start(&mut state);
        let ask = state.waiting().expect("the editor waits");
        assert!(ask.single_line, "one line of plain text");
        assert_eq!(ask.mtext.as_ref().expect("a draft").contents, "ROOM 1");
        // The caret starts at the end: backspace the 1, type 2.
        state.sync_mtext_editor();
        let active = state.active;
        state.docs[active]
            .mtext_editor
            .as_mut()
            .expect("open")
            .backspace(false, 0.0);
        edit_in_place(&mut state, "2", Outcome::Accept);
        assert!(!state.command_running() && !state.mtext_editing());
        let Entity::Text(edited) = first(&state) else {
            panic!("still a single-line text");
        };
        assert_eq!(edited.contents, "ROOM 2");
        assert_eq!(edited.placement, room().placement, "only the words change");
        state.undo();
        let Entity::Text(back) = first(&state) else {
            panic!("still a single-line text");
        };
        assert_eq!(back.contents, "ROOM 1");
    }

    #[test]
    fn unchanged_or_cancelled_words_change_nothing() {
        let mut state = state_with(Entity::Text(Box::new(room())));
        let saved = state.session().document().clone();
        let steps = state.session().history().position();
        state.select_all();
        start(&mut state);
        edit_in_place(&mut state, "", Outcome::Accept);
        assert_eq!(*state.session().document(), saved);
        assert_eq!(state.session().history().position(), steps, "no undo step");
        state.select_all();
        start(&mut state);
        edit_in_place(&mut state, "!!", Outcome::Cancel);
        assert_eq!(*state.session().document(), saved);
        assert!(!state.command_running() && !state.mtext_editing());
    }

    #[test]
    fn a_clicked_text_is_edited_and_other_objects_are_refused() {
        let mut state = state_with(Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(5.0, 0.0),
        });
        state.add_entity(Entity::Text(Box::new(room())));
        let ids: Vec<_> = state
            .session()
            .document()
            .objects()
            .iter()
            .map(|o| o.id)
            .collect();
        start(&mut state);
        assert_eq!(
            state.waiting().expect("asks").message,
            "Select an annotation object"
        );
        state.answer_object(Point::new(1.0, 0.0), Some(ids[0]), false);
        state.settle_command();
        assert_eq!(state.waiting().expect("asks again").wants, Wants::Object);
        state.answer_object(Point::new(10.0, 20.0), Some(ids[1]), false);
        state.settle_command();
        let ask = state.waiting().expect("the editor waits");
        assert_eq!((ask.wants, ask.single_line), (Wants::MTextEditor, true));
    }

    #[test]
    fn a_multiline_text_opens_the_multiline_editor() {
        let mut state = state_with(Entity::MText(Box::new(MText::new(
            Point::new(5.0, 5.0),
            2.0,
            "Hello",
            TextStyleId::STANDARD,
        ))));
        state.select_all();
        start(&mut state);
        let ask = state.waiting().expect("the editor waits");
        assert_eq!((ask.wants, ask.single_line), (Wants::MTextEditor, false));
    }

    /// Pick `id` at the prompt and edit it: backspace its last letter and
    /// type `typed`.
    fn pick_and_retype(state: &mut AppState, id: EntityId, typed: &str) {
        state.answer_object(Point::new(0.0, 0.0), Some(id), false);
        state.settle_command();
        let active = state.active;
        state.sync_mtext_editor();
        state.docs[active]
            .mtext_editor
            .as_mut()
            .expect("open")
            .backspace(false, 0.0);
        edit_in_place(state, typed, Outcome::Accept);
    }

    fn words(state: &AppState) -> Vec<String> {
        state
            .session()
            .document()
            .objects()
            .iter()
            .filter_map(|o| match &o.entity {
                Entity::Text(t) => Some(t.contents.clone()),
                _ => None,
            })
            .collect()
    }

    fn two_rooms() -> (AppState, Vec<EntityId>) {
        let mut state = state_with(Entity::Text(Box::new(room())));
        let mut second = room();
        second.contents = "ROOM 3".into();
        state.add_entity(Entity::Text(Box::new(second)));
        let ids = state
            .session()
            .document()
            .objects()
            .iter()
            .map(|o| o.id)
            .collect();
        let active = state.active;
        state.docs[active].selection.clear();
        (state, ids)
    }

    #[test]
    fn it_keeps_asking_until_enter_so_several_texts_are_corrected_in_a_row() {
        let (mut state, ids) = two_rooms();
        start(&mut state);
        pick_and_retype(&mut state, ids[0], "A");
        let ask = state.waiting().expect("asks for the next text");
        assert_eq!(ask.wants, Wants::Object);
        assert_eq!(ask.message, "Select an annotation object");
        pick_and_retype(&mut state, ids[1], "B");
        assert_eq!(words(&state), ["ROOM A", "ROOM B"]);
        state.cmd_submit("");
        state.settle_command();
        assert!(!state.command_running(), "Enter ends it");
        state.undo();
        assert_eq!(words(&state), ["ROOM A", "ROOM 3"], "each edit is one step");
    }

    #[test]
    fn undo_takes_back_the_last_edit_and_nothing_more() {
        let (mut state, ids) = two_rooms();
        start(&mut state);
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(words(&state), ["ROOM 1", "ROOM 3"], "nothing to undo yet");
        assert!(state.command_running());
        pick_and_retype(&mut state, ids[0], "A");
        pick_and_retype(&mut state, ids[1], "B");
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(words(&state), ["ROOM A", "ROOM 3"]);
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(words(&state), ["ROOM 1", "ROOM 3"]);
        state.cmd_submit("u");
        state.settle_command();
        assert_eq!(words(&state), ["ROOM 1", "ROOM 3"], "only its own edits");
        assert!(state.command_running());
    }

    #[test]
    fn single_mode_ends_after_one_text_and_is_remembered() {
        let (mut state, ids) = two_rooms();
        start(&mut state);
        state.cmd_submit("m");
        state.settle_command();
        state.cmd_submit("s");
        state.settle_command();
        assert!(!state.defaults().text.get().multiple);
        pick_and_retype(&mut state, ids[0], "A");
        assert!(!state.command_running(), "Single ends after one text");
        start(&mut state);
        assert_eq!(state.waiting().expect("asks").wants, Wants::Object);
        state.cmd_submit("m");
        state.settle_command();
        state.cmd_submit("m");
        state.settle_command();
        assert!(state.defaults().text.get().multiple);
    }

    #[test]
    fn a_selected_text_is_edited_once_whatever_the_mode() {
        let mut state = state_with(Entity::Text(Box::new(room())));
        state.select_all();
        start(&mut state);
        edit_in_place(&mut state, "", Outcome::Accept);
        assert!(!state.command_running(), "no prompt for a next text");
    }
}
