//! Messages in the language of the interface (issue #138). A command says
//! its messages in English, as outside callers read them (the status line
//! goes back to automation); they are translated where they are shown.

use opendraft_commands::Wording;

use crate::state::AppState;
use opendraft_ui::key;

/// A message in the language of the interface.
pub fn shown(wording: &Wording) -> String {
    wording.shown(&|text| rust_i18n::t!(text))
}

/// `text` with its field `%{why}` filled by `why`, a reason that is
/// translated with it when it is a translated text itself.
pub fn because(text: &str, why: impl std::fmt::Display) -> Wording {
    Wording::new(text, &[]).part("why", Wording::plain(why.to_string()))
}

/// A line said in English, in the language of the interface: by its
/// wording when it has one that still gives this line.
pub fn shown_line(line: &str, wording: Option<&Wording>) -> String {
    match wording {
        Some(wording) if wording.english() == line => shown(wording),
        _ => rust_i18n::t!(line).into_owned(),
    }
}

/// The status bar's message in the language of the interface.
pub fn shown_message(state: &AppState) -> String {
    shown_line(&state.message, state.message_wording.as_ref())
}

impl AppState {
    /// Says `message` in the command line and the status bar.
    pub fn tell(&mut self, message: impl Into<Wording>) {
        let wording = message.into();
        self.set_message(wording.clone());
        self.cmdline.say(wording);
    }

    /// Shows "Undo: Circle" (`text` with the step's `name`) in the status
    /// bar; the name is translated when it is a text translated elsewhere.
    pub fn say_step(&mut self, text: &str, name: impl Into<String>) {
        let step = Wording::plain(name.into());
        self.set_message(Wording::new(text, &[]).part("name", step));
    }

    /// Says in the status bar that the command `id` is not built yet.
    pub fn say_not_built(&mut self, id: &str) {
        self.set_message(Wording::new(
            key!("Not implemented yet: %{id}"),
            &[("id", id)],
        ));
    }

    /// Says in the status bar that the app closes once its files are written.
    pub fn say_closing_later(&mut self) {
        self.message = key!("Closing when the files are written...").to_owned();
    }

    /// Shows `message` in the status bar.
    pub fn set_message(&mut self, message: impl Into<Wording>) {
        let wording = message.into();
        self.message = wording.english();
        let worded = !(wording.values.is_empty() && wording.parts.is_empty());
        self.message_wording = worded.then_some(wording);
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use opendraft_commands::Wording;

    use crate::cmdline::CommandLine;
    use crate::text::objects;

    /// The files whose messages are marked for translation; the others
    /// follow in later steps of issue #138.
    const MARKED: &[&str] = &[
        "arc_cmd",
        "circle",
        "ellipse_cmd",
        "spline_cmd",
        "polyline_cmd",
        "polyline_arc",
        "rectangle",
        "rect_variants",
        "polygon_cmd",
        "donut_cmd",
        "xline_cmd",
        "point_cmd",
        "draw",
        "divide_cmd",
        "boundary_cmd",
        "modify",
        "scale_rotate",
        "stretch",
        "offset",
        "corners",
        "lengthen",
        "edges",
        "edges_sweep",
        "break_cmd",
        "join",
        "explode",
        "array_cmd",
        "array_path",
        "pedit_cmd",
        "pedit_vertex",
        "pedit_join",
        "erase",
        "match_props",
        "grips",
        "selection",
        "once_input",
        "zoom_cmd",
        "view_twist_cmd",
        "ccs_cmd",
        "measure_cmd",
        "attdef_cmd",
        "attribute_dialog_cmd",
        "attribute_edit",
        "attribute_rows",
        "attribute_sync_cmd",
        "autosave",
        "block_cmd",
        "block_dialog_cmd",
        "block_editor",
        "canvas_style",
        "clipboard",
        "cmdline",
        "commands",
        "crash_log",
        "dim_override_cmd",
        "dim_style_table",
        "dim_vars",
        "dimension_angular_cmd",
        "dimension_centre_cmd",
        "dimension_chain_cmd",
        "dimension_cmd",
        "dimension_edit_cmd",
        "dimension_jogged_cmd",
        "dimension_space_cmd",
        "field_cmd",
        "field_dialog_cmd",
        "file_io",
        "files",
        "hatch_cmd",
        "hatch_edit",
        "icon_library",
        "insert_cmd",
        "interact",
        "large_file",
        "layer_cmd",
        "layer_switch",
        "layers",
        "layout_cmd",
        "layouts",
        "leader_cmd",
        "linetype_vars",
        "mleader_cmd",
        "mleader_style_table",
        "mtext",
        "object_styles",
        "overkill_cmd",
        "overkill_line",
        "plot_job",
        "plot_line",
        "plot_pick",
        "plot_run",
        "plot_state",
        "point_vars",
        "purge_cmd",
        "purge_line",
        "single_line_text_cmd",
        "state",
        "sysvar_line",
        "template_state",
        "text_edit_cmd",
        "text_style_draft",
        "units_state",
        "view_freeze",
    ];

    fn french(wording: &Wording) -> String {
        wording.shown(&|text| Cow::Owned(rust_i18n::t!(text, locale = "fr").into_owned()))
    }

    #[test]
    fn a_message_is_said_in_english_and_shown_translated() {
        let said = Wording::new("Moved %{objects}", &[]).part("objects", objects(3));
        assert_eq!(said.english(), "Moved 3 objects");
        assert_eq!(french(&said), "3 objets déplacé(s)");
        let one = Wording::new("Moved %{objects}", &[]).part("objects", objects(1));
        assert_eq!(one.english(), "Moved 1 object");
    }

    #[test]
    fn the_log_keeps_english_and_shows_each_line_by_its_wording() {
        let mut cmdline = CommandLine::default();
        cmdline.say("No previous view saved.");
        cmdline.say(Wording::new("%{count} selected", &[("count", "4")]));
        assert_eq!(cmdline.log, ["No previous view saved.", "4 selected"]);
        cmdline.log.push("typed by hand".to_owned());
        let shown: Vec<String> = cmdline.shown_log().collect();
        assert_eq!(
            shown,
            ["No previous view saved.", "4 selected", "typed by hand"]
        );
    }

    #[test]
    fn the_marked_files_say_nothing_unmarked() {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut unmarked = Vec::new();
        for name in MARKED {
            let text = std::fs::read_to_string(src.join(format!("{name}.rs"))).unwrap();
            let production = text.split("mod tests {").next().unwrap_or_default();
            for (no, line) in production.lines().enumerate() {
                let squeezed: String = line.split_whitespace().collect();
                if squeezed.contains(".say(\"") || squeezed.contains(".say(format!") {
                    unmarked.push(format!("{name}.rs:{}", no + 1));
                }
            }
        }
        assert!(
            unmarked.is_empty(),
            "say(key!(..)) or a Wording: {unmarked:#?}"
        );
    }
}
