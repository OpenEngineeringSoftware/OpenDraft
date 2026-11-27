//! TEXT/DTEXT: one line of text placed at a point, with a height, rotation
//! and justification, typed in place on the drawing (-TEXT asks it all on the
//! command line). The object is a real single-line `TEXT` entity; the shared
//! text engine draws its projection.

use opendraft_commands::{Answer, Cancelled, Editor, NumberOptions, Options, PointOptions};
use opendraft_core::block::{TextHorizontal, TextVertical};
use opendraft_core::{Entity, Point, SingleLineText, TextAlignmentMode, TextStyleId};
use opendraft_ui::key;

use crate::defaults::EditorDefaults;
use crate::draw::{add, MIN_SIZE};
use opendraft_commands::Wording;

const JUSTIFY: [(&str, &str, TextHorizontal, TextVertical); 13] = [
    ("Left", "L", TextHorizontal::Left, TextVertical::Baseline),
    (
        "Center",
        "C",
        TextHorizontal::Center,
        TextVertical::Baseline,
    ),
    ("Middle", "M", TextHorizontal::Center, TextVertical::Middle),
    ("Right", "R", TextHorizontal::Right, TextVertical::Baseline),
    ("TL", "TL", TextHorizontal::Left, TextVertical::Top),
    ("TC", "TC", TextHorizontal::Center, TextVertical::Top),
    ("TR", "TR", TextHorizontal::Right, TextVertical::Top),
    ("ML", "ML", TextHorizontal::Left, TextVertical::Middle),
    ("MC", "MC", TextHorizontal::Center, TextVertical::Middle),
    ("MR", "MR", TextHorizontal::Right, TextVertical::Middle),
    ("BL", "BL", TextHorizontal::Left, TextVertical::Bottom),
    ("BC", "BC", TextHorizontal::Center, TextVertical::Bottom),
    ("BR", "BR", TextHorizontal::Right, TextVertical::Bottom),
];

#[derive(Clone, Copy)]
enum PlacementMode {
    Justified(TextHorizontal, TextVertical),
    Aligned(TextAlignmentMode),
}

fn choose_justification(ed: &Editor) -> Result<PlacementMode, Cancelled> {
    let mut options = Options::<String>::new(key!("Enter an option"))
        .keyword("Align", "A")
        .keyword("Fit", "F");
    for (name, key, _, _) in JUSTIFY {
        options = options.keyword(name, key);
    }
    let chosen = ed.get_keyword(options)?;
    if chosen == "Align" {
        return Ok(PlacementMode::Aligned(TextAlignmentMode::Align));
    }
    if chosen == "Fit" {
        return Ok(PlacementMode::Aligned(TextAlignmentMode::Fit));
    }
    JUSTIFY
        .iter()
        .find(|(name, _, _, _)| *name == chosen)
        .map(|(_, _, horizontal, vertical)| PlacementMode::Justified(*horizontal, *vertical))
        .ok_or(Cancelled)
}

fn choose_style(ed: &Editor, current: TextStyleId) -> Result<TextStyleId, Cancelled> {
    let current_name = ed.read(move |doc| {
        doc.text_style(current)
            .map(|s| s.name.clone())
            .unwrap_or_else(|| "Standard".to_owned())
    })?;
    loop {
        let answer = ed.ask_string(Options::<String>::worded(
            key!("Enter style name <%{name}>"),
            &[("name", &current_name)],
        ))?;
        let name = match answer {
            Answer::Value(name) => name,
            Answer::Enter => return Ok(current),
            Answer::Keyword(_) => continue,
        };
        let key = name.trim().to_owned();
        if let Some(id) = ed.read(move |doc| doc.text_style_named(&key).map(|s| s.id))? {
            return Ok(id);
        }
        ed.say(Wording::new(
            key!("Text style \"%{name}\" does not exist."),
            &[("name", name.trim())],
        ));
    }
}

/// Where a text starts and how it lies, before its height and words.
struct Start {
    at: Point,
    style: TextStyleId,
    mode: PlacementMode,
    second: Option<Point>,
}

/// The start point (with the Justify and Style options) and, for Align and
/// Fit, the second endpoint.
fn ask_start(ed: &Editor) -> Result<Start, Cancelled> {
    let mut style = ed.read(|doc| doc.current_text_style())?;
    let mut mode = PlacementMode::Justified(TextHorizontal::Left, TextVertical::Baseline);
    let at = loop {
        let options = PointOptions::new(key!("Specify start point of text"))
            .keyword("Justify", "J")
            .keyword("Style", "S");
        match ed.ask_point(options)? {
            Answer::Value(point) => break point,
            Answer::Keyword(word) if word == "Justify" => {
                mode = choose_justification(ed)?;
            }
            Answer::Keyword(_) => style = choose_style(ed, style)?,
            Answer::Enter => return Err(Cancelled),
        }
    };
    let second = if matches!(mode, PlacementMode::Aligned(_)) {
        Some(loop {
            let point =
                ed.get_point(PointOptions::new(key!("Specify second endpoint of text")).base(at))?;
            if (point.x - at.x).hypot(point.y - at.y) > MIN_SIZE {
                break point;
            }
            ed.say(key!("Requires a point different from the first endpoint."));
        })
    } else {
        None
    };
    Ok(Start {
        at,
        style,
        mode,
        second,
    })
}

/// The text height, unless the style fixes it or Align works it out. The
/// answer is a plain number (`base` draws a rubber band to the pointer).
fn ask_height(ed: &Editor, start: &Start, rubber_band: bool) -> Result<f64, Cancelled> {
    let style = start.style;
    let fixed = ed.read(move |doc| doc.text_style(style).and_then(|s| s.fixed_height))?;
    let needs_height = !matches!(start.mode, PlacementMode::Aligned(TextAlignmentMode::Align));
    match (needs_height, fixed) {
        (false, _) => Ok(1.0),
        (true, Some(height)) => Ok(height),
        (true, None) => loop {
            let remembered = ed.defaults().text.get().height;
            let options = NumberOptions::new(key!("Specify height")).default(remembered);
            let height = if rubber_band {
                ed.get_distance(options.base(start.at))?
            } else {
                ed.get_number(options)?
            };
            if height > MIN_SIZE {
                ed.defaults().text.change(|m| m.height = height);
                return Ok(height);
            }
            ed.say(key!("Requires a positive value."));
        },
    }
}

/// Make the text from what was asked, and add it as one undo step.
fn add_text(
    ed: &mut Editor,
    start: &Start,
    height: f64,
    rotation: f64,
    contents: String,
) -> Result<(), Cancelled> {
    if contents.trim().is_empty() {
        return Ok(());
    }
    let style = start.style;
    let mut text = SingleLineText::new(start.at, height, contents, style);
    match start.mode {
        PlacementMode::Justified(horizontal, vertical) => {
            text.placement.rotation = rotation;
            text.placement.horizontal = horizontal;
            text.placement.vertical = vertical;
        }
        PlacementMode::Aligned(alignment) => {
            let metrics = ed
                .read(move |doc| doc.text_style(style).cloned())?
                .ok_or(Cancelled)?;
            let second = start.second.ok_or(Cancelled)?;
            text.resolve_alignment(alignment, second, &metrics)
                .map_err(|why| {
                    ed.say(why);
                    Cancelled
                })?;
        }
    }
    add(ed, Entity::Text(Box::new(text)), "Single-line Text")
}

/// TEXT: the start point (or Justify/Style), then the rotation as a second
/// point from it (or a typed angle), then the height as a plain number, then
/// the words typed in place on the drawing. Empty words add nothing; an
/// accepted line is one undo step.
pub fn draw_text(ed: &mut Editor) -> Result<(), Cancelled> {
    let start = ask_start(ed)?;
    // Level with the coordinate system's X axis unless turned.
    let level = ed.read(|doc| doc.ccs().angle)?;
    let rotation = match start.second {
        Some(second) => (second.y - start.at.y).atan2(second.x - start.at.x),
        None => ed.get_angle(
            NumberOptions::new(key!("Specify rotation of text"))
                .default(level)
                .base(start.at),
        )?,
    };
    let height = ask_height(ed, &start, false)?;
    // Align works its height out afterwards; the draft shows at the usual one.
    let aligns = matches!(start.mode, PlacementMode::Aligned(TextAlignmentMode::Align));
    let shown = if aligns {
        ed.defaults().text.get().height
    } else {
        height
    };
    let mut draft = SingleLineText::new(start.at, shown, "", start.style);
    draft.placement.rotation = rotation;
    if let PlacementMode::Justified(horizontal, vertical) = start.mode {
        draft.placement.horizontal = horizontal;
        draft.placement.vertical = vertical;
    }
    let Some(typed) = ed.edit_line(None, draft.projection())? else {
        return Ok(());
    };
    let contents = SingleLineText::contents_of_projection(&typed.contents);
    add_text(ed, &start, height, rotation, contents)
}

/// -TEXT: the same text asked on the command line, in the order AutoCAD
/// has used: start point, height, rotation, then the words.
pub fn draw_text_line(ed: &mut Editor) -> Result<(), Cancelled> {
    let start = ask_start(ed)?;
    let height = ask_height(ed, &start, true)?;
    let level = ed.read(|doc| doc.ccs().angle)?;
    let rotation = if start.second.is_some() {
        0.0
    } else {
        ed.get_angle(
            NumberOptions::new(key!("Specify rotation angle of text"))
                .default(level)
                .base(start.at),
        )?
    };
    let contents = match ed.ask_string(Options::<String>::new(key!("Enter text")))? {
        Answer::Value(text) => text,
        Answer::Enter => return Ok(()),
        Answer::Keyword(_) => return Err(Cancelled),
    };
    add_text(ed, &start, height, rotation, contents)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mtext_editor::Outcome;
    use crate::state::AppState;
    use opendraft_commands::Wants;
    fn run(answers: &[&str]) -> AppState {
        let mut state = AppState::new();
        state.start_command("-TEXT", draw_text_line);
        state.settle_command();
        for answer in answers {
            state.cmd_submit(answer);
            state.settle_command();
        }
        state
    }

    /// Drive TEXT to its in-place editor, type `typed` and close it.
    fn in_place(answers: &[&str], typed: &str, outcome: Outcome) -> AppState {
        let mut state = AppState::new();
        state.start_command("TEXT", draw_text);
        state.settle_command();
        for answer in answers {
            state.cmd_submit(answer);
            state.settle_command();
        }
        let ask = state.waiting().expect("the editor waits");
        assert_eq!(ask.wants, Wants::MTextEditor);
        assert!(ask.single_line, "one line of plain text");
        state.sync_mtext_editor();
        let active = state.active;
        if let Some(editor) = state.docs[active].mtext_editor.as_mut() {
            if !typed.is_empty() {
                editor.insert(typed, 0.0);
            }
        }
        state.finish_mtext_editor(outcome);
        state.settle_command();
        state
    }

    #[test]
    fn point_then_a_rotation_vector_then_a_plain_height_then_the_words_in_place() {
        let mut state = AppState::new();
        state.start_command("TEXT", draw_text);
        state.settle_command();
        state.cmd_submit("10,20");
        state.settle_command();
        let ask = state.waiting().expect("asks the rotation");
        assert_eq!(ask.wants, Wants::Angle);
        assert_eq!(
            ask.base,
            Some(Point::new(10.0, 20.0)),
            "a vector from the start"
        );
        state.cmd_submit("10,30");
        state.settle_command();
        let ask = state.waiting().expect("asks the height");
        assert_eq!(ask.wants, Wants::Number, "a plain number");
        assert_eq!(ask.base, None, "no rubber band");

        let mut state = in_place(&["10,20", "10,30", "4"], "Hello {x} world", Outcome::Accept);
        let found = text(&state);
        assert_eq!(found.contents, "Hello {x} world");
        assert_eq!(found.position(), Point::new(10.0, 20.0));
        assert_eq!(found.placement.height, 4.0);
        assert!((found.placement.rotation - std::f64::consts::FRAC_PI_2).abs() < 1e-9);
        assert_eq!(state.session().document().validate(), Ok(()));
        state.undo();
        assert!(state.session().document().is_empty(), "one undo step");
    }

    #[test]
    fn empty_or_cancelled_words_in_place_add_nothing() {
        let empty = in_place(&["0,0", "", "2.5"], "", Outcome::Accept);
        assert!(empty.session().document().is_empty());
        assert!(!empty.command_running());
        let cancelled = in_place(&["0,0", "", "2.5"], "typed", Outcome::Cancel);
        assert!(cancelled.session().document().is_empty());
        assert!(!cancelled.command_running());
    }

    #[test]
    fn in_place_justify_and_align_are_kept() {
        let state = in_place(&["j", "tr", "1,2", "", "2"], "Top right", Outcome::Accept);
        let found = text(&state);
        assert_eq!(found.placement.horizontal, TextHorizontal::Right);
        assert_eq!(found.placement.vertical, TextVertical::Top);
        let state = in_place(
            &["j", "align", "0,0", "12,0"],
            "Wide words",
            Outcome::Accept,
        );
        let found = text(&state);
        let style = state
            .session()
            .document()
            .text_style(TextStyleId::STANDARD)
            .unwrap();
        let width = found.projection().layout(style).actual_width;
        assert!((width - 12.0).abs() < 1e-8, "{width}");
    }

    fn text(state: &AppState) -> &SingleLineText {
        match &state.session().document().objects()[0].entity {
            Entity::Text(text) => text,
            other => panic!("expected text, found {other:?}"),
        }
    }

    #[test]
    fn point_height_rotation_and_words_make_one_text_and_one_undo_step() {
        let mut state = run(&["10,20", "4", "30", "Hello world"]);
        let found = text(&state);
        assert_eq!(found.contents, "Hello world");
        assert_eq!(found.position(), Point::new(10.0, 20.0));
        assert_eq!(found.placement.height, 4.0);
        assert!((found.placement.rotation - 30f64.to_radians()).abs() < 1e-9);
        assert_eq!(state.session().document().validate(), Ok(()));
        state.undo();
        assert!(state.session().document().is_empty());
    }

    #[test]
    fn justification_is_kept_and_an_empty_line_adds_nothing() {
        let state = run(&["j", "tr", "1,2", "", "", "Top right"]);
        let found = text(&state);
        assert_eq!(found.placement.horizontal, TextHorizontal::Right);
        assert_eq!(found.placement.vertical, TextVertical::Top);

        let empty = run(&["0,0", "", "", ""]);
        assert!(empty.session().document().is_empty());
        assert!(!empty.command_running());
    }

    #[test]
    fn align_and_fit_land_exactly_between_their_endpoints() {
        for (word, mode) in [
            ("align", TextAlignmentMode::Align),
            ("fit", TextAlignmentMode::Fit),
        ] {
            let answers = if mode == TextAlignmentMode::Fit {
                vec!["j", word, "0,0", "12,0", "2", "Wide words"]
            } else {
                vec!["j", word, "0,0", "12,0", "Wide words"]
            };
            let state = run(&answers);
            let found = text(&state);
            assert_eq!(found.alignment.unwrap().mode, mode);
            assert_eq!(found.alignment.unwrap().second, Point::new(12.0, 0.0));
            let style = state
                .session()
                .document()
                .text_style(TextStyleId::STANDARD)
                .unwrap();
            let width = found.projection().layout(style).actual_width;
            assert!((width - 12.0).abs() < 1e-8, "{mode:?}: {width}");
        }
    }
}
