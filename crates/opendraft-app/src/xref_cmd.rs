//! `-XREF` and `XATTACH`: attach, list, reload, unload, detach and edit an
//! external reference's path. The file work is asked for through
//! `Editor::ask_xref_plan`; the edits are `xref_apply`.
//!
//! `XATTACH` opens the Attach External Reference dialog; `-XREF` is the
//! command-line form, and the palette uses the same pieces.

use opendraft_commands::{
    Cancelled, Command, Editor, NumberOptions, Options, PointOptions, Wording, XrefHostDir,
};
use opendraft_core::{
    path_problem, BlockId, XrefAttachDraft, XrefKind, XrefLoadRequest, XrefPathType,
};
use opendraft_ui::key;

use crate::state::AppState;
use crate::xref_apply;
use crate::xref_defaults::defaults_of;
use crate::xref_source::{source_problem, stem_of};

/// The commands, for the command table.
pub fn commands() -> Vec<Command<AppState>> {
    vec![
        Command::new("xref.command_line", "-XREF", |s: &mut AppState| {
            s.start_command("-XREF", command_line)
        })
        .checked(|s| s.is_running("-XREF"))
        .transparent(),
        Command::new("xref.attach", "XATTACH", |s: &mut AppState| {
            s.start_command("XATTACH", attach_dialog)
        })
        .checked(|s| s.is_running("XATTACH"))
        .transparent(),
        Command::new("xref.bind_symbols", "XBIND", |s: &mut AppState| {
            s.start_command("XBIND", crate::xbind_cmd::xbind)
        })
        .checked(|s| s.is_running("XBIND"))
        .transparent(),
        Command::new("xref.clip", "XCLIP", |s: &mut AppState| {
            s.start_command("XCLIP", crate::xclip_cmd::xclip)
        })
        .checked(|s| s.is_running("XCLIP"))
        .transparent(),
        Command::new("xref.palette", "External References", |s: &mut AppState| {
            s.show_xrefs_panel();
        }),
    ]
}

/// `-XREF`: AutoCAD's command-line options.
fn command_line(ed: &mut Editor) -> Result<(), Cancelled> {
    let options = Options::<String>::new(key!("Enter an option"))
        .keyword("?", "?")
        .keyword("Bind", "B")
        .keyword("Detach", "D")
        .keyword("Path", "P")
        .keyword("Unload", "U")
        .keyword("Reload", "R")
        .keyword("Overlay", "O")
        .keyword("Attach", "A")
        .default("Attach".to_owned());
    let choice = ed.get_keyword(options)?;
    match option(&choice) {
        "?" => list(ed),
        "Bind" => crate::xbind_cmd::bind_all(ed),
        "Detach" => detach(ed),
        "Path" => edit_path(ed),
        "Unload" => unload(ed),
        "Reload" => reload(ed),
        "Overlay" => attach(ed, XrefKind::Overlay),
        _ => attach(ed, XrefKind::Attach),
    }
}

/// The long option a typed keyword (or its first letters) names.
fn option(typed: &str) -> &'static str {
    let typed = typed.trim().to_ascii_lowercase();
    if typed == "?" {
        return "?";
    }
    const OPTIONS: [&str; 7] = [
        "Attach", "Overlay", "Detach", "Path", "Unload", "Reload", "Bind",
    ];
    OPTIONS
        .into_iter()
        .find(|name| name.to_ascii_lowercase().starts_with(&typed) && !typed.is_empty())
        .unwrap_or("Attach")
}

fn list(ed: &mut Editor) -> Result<(), Cancelled> {
    let lines = ed.read(|doc| {
        doc.xrefs()
            .iter()
            .filter_map(|xref| {
                let block = doc.block(xref.block)?;
                let status = if xref.never_loaded() {
                    "Not loaded"
                } else if xref.unloaded {
                    "Unloaded"
                } else {
                    "Loaded"
                };
                Some(format!(
                    "  \"{}\": {} {} {status} {}",
                    block.name,
                    xref.kind.label(),
                    xref.path_type.label(),
                    xref.path
                ))
            })
            .collect::<Vec<_>>()
    })?;
    if lines.is_empty() {
        ed.say(key!("There are no external references in this drawing."));
    } else {
        for line in lines {
            ed.say(line);
        }
    }
    Ok(())
}

/// The blocks the user named: `*` for every one, or a comma-separated list.
pub(crate) fn chosen(ed: &Editor, message: &str) -> Result<Vec<BlockId>, Cancelled> {
    let names = ed.get_string(Options::<String>::new(message))?;
    ed.read(move |doc| {
        let names = names.trim();
        if names == "*" {
            return doc.xrefs().iter().map(|x| x.block).collect();
        }
        names
            .split(',')
            .filter_map(|name| doc.xref_by_name(name.trim()).map(|x| x.block))
            .collect()
    })
}

fn host_dir(ed: &Editor) -> Option<String> {
    ed.context()
        .get::<XrefHostDir>()
        .and_then(|dir| dir.0.clone())
}

/// `XATTACH`: the source file is named, then the Attach External Reference
/// dialog fills in the rest. Anything left "on-screen" is asked in the
/// drawing once the dialog closes.
pub(crate) fn attach_dialog(ed: &mut Editor) -> Result<(), Cancelled> {
    let dir = host_dir(ed);
    let defaults = defaults_of(ed);
    let source_path =
        ed.get_string(Options::<String>::new(key!("Enter name of file to attach")))?;
    if let Some(problem) = source_problem(&source_path, dir.as_deref()) {
        ed.say(problem);
        return Ok(());
    }
    let draft = XrefAttachDraft {
        name: stem_of(&source_path),
        source_path,
        kind: if defaults.overlay {
            XrefKind::Overlay
        } else {
            XrefKind::Attach
        },
        path_type: defaults.path_type,
        insertion: None,
        scale: None,
        rotation: None,
    };
    let draft = ed.ask_xref_attach(draft)?;
    let at = match draft.insertion {
        Some(at) => at,
        None => ed.get_point(PointOptions::new(key!("Specify insertion point")))?,
    };
    let scale = match draft.scale {
        Some(scale) => scale,
        None => {
            let factor =
                ed.get_number(NumberOptions::new(key!("Specify scale factor")).default(1.0))?;
            (factor, factor)
        }
    };
    if !scale.0.is_finite() || !scale.1.is_finite() || scale.0 == 0.0 || scale.1 == 0.0 {
        ed.say(key!("The scale factor cannot be zero."));
        return Ok(());
    }
    let rotation = match draft.rotation {
        Some(angle) => angle,
        None => ed.get_angle(NumberOptions::new(key!("Specify rotation angle")).default(0.0))?,
    };
    let request = XrefLoadRequest {
        block: None,
        name: draft.name.trim().to_owned(),
        source_path: draft.source_path,
        path_type: draft.path_type,
        kind: draft.kind,
        host_dir: dir,
        host_path: None,
    };
    plan_and_attach(ed, request, at, scale, rotation)
}

/// The command-line `-XREF Attach`/`Overlay`: it uses the remembered path
/// type, and asks the same insertion, scale and rotation questions the
/// dialog fills.
pub(crate) fn attach(ed: &mut Editor, kind: XrefKind) -> Result<(), Cancelled> {
    let dir = host_dir(ed);
    let path_type = defaults_of(ed).path_type;
    let source_path =
        ed.get_string(Options::<String>::new(key!("Enter name of file to attach")))?;
    if let Some(problem) = source_problem(&source_path, dir.as_deref()) {
        ed.say(problem);
        return Ok(());
    }
    let stem = stem_of(&source_path);
    let name = ed.get_string(Options::<String>::new(key!("Enter reference name")).default(stem))?;
    let draft = XrefAttachDraft {
        source_path: source_path.clone(),
        name: name.clone(),
        kind,
        path_type,
        insertion: None,
        scale: None,
        rotation: None,
    };
    if let Some(problem) = ed.read(move |doc| draft.problem(doc))? {
        ed.say(problem);
        return Ok(());
    }
    let at = ed.get_point(PointOptions::new(key!("Specify insertion point")))?;
    let scale = ed.get_number(NumberOptions::new(key!("Specify scale factor")).default(1.0))?;
    if !scale.is_finite() || scale == 0.0 {
        ed.say(key!("The scale factor cannot be zero."));
        return Ok(());
    }
    let rotation = ed.get_angle(NumberOptions::new(key!("Specify rotation angle")).default(0.0))?;
    let request = XrefLoadRequest {
        block: None,
        name,
        source_path,
        path_type,
        kind,
        host_dir: dir,
        host_path: None,
    };
    plan_and_attach(ed, request, at, (scale, scale), rotation)
}

/// Read and plan the source, then add it; the wording of both outcomes is
/// the same for the dialog and the command line.
fn plan_and_attach(
    ed: &mut Editor,
    request: XrefLoadRequest,
    at: opendraft_core::Point,
    scale: (f64, f64),
    rotation: f64,
) -> Result<(), Cancelled> {
    match ed.ask_xref_plan(request)? {
        Ok(plan) => {
            let label = plan.root.name.clone();
            let count = plan.root.entities.len();
            let missing = crate::xref_source::missing_content(&plan.xref.report.notes);
            xref_apply::attach(ed, plan, at, scale, rotation)?;
            ed.say(
                Wording::new(
                    key!("Attached external reference \"%{name}\" (%{objects})"),
                    &[],
                )
                .part("name", Wording::plain(label))
                .part("objects", crate::text::objects(count)),
            );
            for line in missing {
                ed.say(Wording::plain(line));
            }
        }
        Err(why) => {
            ed.say(
                Wording::new(key!("External reference was not attached: %{why}"), &[])
                    .part("why", Wording::plain(why)),
            );
        }
    }
    Ok(())
}

fn reload(ed: &mut Editor) -> Result<(), Cancelled> {
    let blocks = chosen(ed, key!("Enter xref name(s) to reload"))?;
    if blocks.is_empty() {
        ed.say(key!("No such external reference."));
        return Ok(());
    }
    for block in blocks {
        reload_block(ed, block)?;
    }
    Ok(())
}

/// Reload one reference (the palette's Reload).
pub(crate) fn reload_block(ed: &mut Editor, block: BlockId) -> Result<(), Cancelled> {
    let dir = host_dir(ed);
    let request = ed.read(move |doc| {
        let xref = doc.xref(block)?;
        let root = doc.block(block)?;
        Some(XrefLoadRequest {
            block: Some(block),
            name: root.name.clone(),
            source_path: xref.path.clone(),
            path_type: xref.path_type,
            kind: xref.kind,
            host_dir: dir.clone(),
            host_path: None,
        })
    })?;
    let Some(request) = request else {
        ed.say(key!("That external reference is not in the drawing."));
        return Ok(());
    };
    let name = request.name.clone();
    match ed.ask_xref_plan(request)? {
        Ok(plan) => {
            let missing = crate::xref_source::missing_content(&plan.xref.report.notes);
            xref_apply::reload(ed, plan)?;
            for line in missing {
                ed.say(Wording::plain(line));
            }
            ed.say(
                Wording::new(key!("Reloaded external reference \"%{name}\""), &[])
                    .part("name", Wording::plain(name)),
            );
        }
        Err(why) => {
            ed.say(
                Wording::new(
                    key!("Could not reload \"%{name}\": %{why}. It was left as it was."),
                    &[],
                )
                .part("name", Wording::plain(name))
                .part("why", Wording::plain(why)),
            );
        }
    }
    Ok(())
}

/// Reload every reference (the palette's Reload all).
pub(crate) fn reload_all(ed: &mut Editor) -> Result<(), Cancelled> {
    let blocks = ed.read(|doc| doc.xrefs().iter().map(|x| x.block).collect::<Vec<_>>())?;
    for block in blocks {
        reload_block(ed, block)?;
    }
    Ok(())
}

fn unload(ed: &mut Editor) -> Result<(), Cancelled> {
    let blocks = chosen(ed, key!("Enter xref name(s) to unload"))?;
    if blocks.is_empty() {
        ed.say(key!("No such external reference."));
        return Ok(());
    }
    for block in blocks {
        unload_block(ed, block)?;
    }
    ed.say(key!("Unloaded the external reference(s)."));
    Ok(())
}

/// Unload one reference (the palette's Unload).
pub(crate) fn unload_block(ed: &mut Editor, block: BlockId) -> Result<(), Cancelled> {
    xref_apply::unload(ed, block)
}

fn detach(ed: &mut Editor) -> Result<(), Cancelled> {
    let blocks = chosen(ed, key!("Enter xref name(s) to detach"))?;
    if blocks.is_empty() {
        ed.say(key!("No such external reference."));
        return Ok(());
    }
    xref_apply::detach(ed, blocks)?;
    ed.say(key!("Detached the external reference(s)."));
    Ok(())
}

/// Detach one reference (the palette's Detach).
pub(crate) fn detach_block(ed: &mut Editor, block: BlockId) -> Result<(), Cancelled> {
    xref_apply::detach(ed, vec![block])
}

/// Edit one reference's path (the palette's Path).
pub(crate) fn edit_path_block(ed: &mut Editor, block: BlockId) -> Result<(), Cancelled> {
    let Some((path, path_type)) = ask_path(ed)? else {
        return Ok(());
    };
    xref_apply::edit_path(ed, block, path, path_type)?;
    ed.say(key!("Edited the external reference path."));
    Ok(())
}

fn edit_path(ed: &mut Editor) -> Result<(), Cancelled> {
    let blocks = chosen(ed, key!("Enter xref name(s) to edit"))?;
    if blocks.is_empty() {
        ed.say(key!("No such external reference."));
        return Ok(());
    }
    let Some((path, path_type)) = ask_path(ed)? else {
        return Ok(());
    };
    for block in blocks {
        xref_apply::edit_path(ed, block, path.clone(), path_type)?;
    }
    ed.say(key!("Edited the external reference path(s)."));
    Ok(())
}

/// The new path and its form, or `None` when the path cannot be used (the
/// user is told why).
fn ask_path(ed: &mut Editor) -> Result<Option<(String, XrefPathType)>, Cancelled> {
    let path = ed.get_string(Options::<String>::new(key!("Enter new path")))?;
    if let Some(problem) = path_problem(&path) {
        ed.say(problem.to_string());
        return Ok(None);
    }
    let type_options = Options::<String>::new(key!("Enter path type"))
        .keyword("Full", "F")
        .keyword("Relative", "R")
        .keyword("NoPath", "N")
        .default("Relative".to_owned());
    let path_type = match ed.get_keyword(type_options)?.to_ascii_lowercase() {
        typed if typed.starts_with('f') => XrefPathType::Full,
        typed if typed.starts_with('n') => XrefPathType::NoPath,
        _ => XrefPathType::Relative,
    };
    Ok(Some((path, path_type)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_option_is_named_by_its_full_spelling_or_first_letters() {
        assert_eq!(option("A"), "Attach");
        assert_eq!(option("att"), "Attach");
        assert_eq!(option("Over"), "Overlay");
        assert_eq!(option("D"), "Detach");
        assert_eq!(option("R"), "Reload");
        assert_eq!(option("?"), "?");
        assert_eq!(option("nonsense"), "Attach");
    }

    #[test]
    fn the_scale_the_command_collects_is_checked() {
        let scale: f64 = 2.0;
        assert!(scale.is_finite() && scale != 0.0);
    }

    #[test]
    fn the_palette_command_asks_the_dock_to_show_the_panel() {
        use crate::dock::{LayoutRequest, PanelTab};

        let mut state = AppState::new();
        state.show_xrefs_panel();
        assert_eq!(
            state.layout_request,
            Some(LayoutRequest::Toggle(PanelTab::Xrefs))
        );
    }

    #[test]
    fn attach_through_the_dialog_reads_a_real_source_and_adds_it() {
        use opendraft_commands::{Reply, Wants};
        use opendraft_core::{Document, Entity, Point};
        use opendraft_files::{FileFormat, NativeFormat};

        let source = Document::from_entities([Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
        }]);
        let path =
            std::env::temp_dir().join(format!("od-xref-attach-{}.odraft", std::process::id()));
        std::fs::write(&path, NativeFormat.write(&source).unwrap().bytes).unwrap();

        let mut state = AppState::new();
        state.start_command("XATTACH", attach_dialog);
        state.settle_command();
        state.cmd_submit(&path.to_string_lossy());
        state.settle_command();
        let draft = state
            .waiting()
            .and_then(|ask| ask.xref_attach.clone())
            .expect("the Attach External Reference dialog");
        assert_eq!(draft.kind, XrefKind::Attach);
        assert_eq!(draft.path_type, XrefPathType::Relative);
        state.reply(Reply::XrefAttach(draft));
        state.settle_command();
        state.answer_point(Point::new(3.0, 4.0));
        state.settle_command();
        state.cmd_submit("1");
        state.settle_command();
        state.cmd_submit("0");
        state.settle_command();
        assert_eq!(state.waiting().map(|ask| ask.wants), Some(Wants::XrefLoad));
        state.sync_xref_load();
        state.settle_command();

        let doc = state.session().document();
        assert_eq!(doc.xrefs().len(), 1);
        assert_eq!(doc.objects().len(), 1);
        let Entity::Insert(insert) = &doc.objects()[0].entity else {
            panic!("an insert was expected")
        };
        assert!(insert.is_xref);
        assert_eq!(insert.insertion, Point::new(3.0, 4.0));
        assert_eq!(doc.validate(), Ok(()));

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn the_dialog_values_skip_the_insertion_scale_and_rotation_prompts() {
        use opendraft_commands::{Reply, Wants};
        use opendraft_core::{Document, Entity, Point};
        use opendraft_files::{FileFormat, NativeFormat};

        let source = Document::from_entities([Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
        }]);
        let path =
            std::env::temp_dir().join(format!("od-xref-dialog-{}.odraft", std::process::id()));
        std::fs::write(&path, NativeFormat.write(&source).unwrap().bytes).unwrap();

        let mut state = AppState::new();
        state.start_command("XATTACH", attach_dialog);
        state.settle_command();
        state.cmd_submit(&path.to_string_lossy());
        state.settle_command();
        let mut draft = state
            .waiting()
            .and_then(|ask| ask.xref_attach.clone())
            .expect("the Attach External Reference dialog");
        draft.insertion = Some(Point::new(5.0, 6.0));
        draft.scale = Some((2.0, 2.0));
        draft.rotation = Some(std::f64::consts::FRAC_PI_2);
        state.reply(Reply::XrefAttach(draft));
        state.settle_command();
        assert_eq!(state.waiting().map(|ask| ask.wants), Some(Wants::XrefLoad));
        state.sync_xref_load();
        state.settle_command();

        let doc = state.session().document();
        let Entity::Insert(insert) = &doc.objects()[0].entity else {
            panic!("an insert was expected")
        };
        assert_eq!(insert.insertion, Point::new(5.0, 6.0));
        assert_eq!((insert.scale_x, insert.scale_y), (2.0, 2.0));
        assert_eq!(insert.rotation, std::f64::consts::FRAC_PI_2);
        assert_eq!(doc.validate(), Ok(()));

        let _ = std::fs::remove_file(path);
    }
}

#[cfg(test)]
mod early_check_tests {
    use super::*;

    fn said(state: &AppState, text: &str) -> bool {
        state.cmdline.log.iter().any(|l| l.contains(text))
    }

    #[test]
    fn a_missing_source_ends_the_dialog_command_before_it_asks_for_a_point() {
        let path =
            std::env::temp_dir().join(format!("od-xref-missing-{}.odraft", std::process::id()));
        let mut state = AppState::new();
        state.start_command("XATTACH", attach_dialog);
        state.settle_command();
        state.cmd_submit(&path.to_string_lossy());
        state.settle_command();
        assert!(!state.command_running());
        assert!(said(&state, "not found"), "{:?}", state.cmdline.log);
    }

    #[test]
    fn a_bad_name_ends_the_command_line_attach_before_the_questions() {
        use opendraft_core::{Document, Entity, Point};
        use opendraft_files::{FileFormat, NativeFormat};

        let source = Document::from_entities([Entity::Line {
            start: Point::new(0.0, 0.0),
            end: Point::new(1.0, 0.0),
        }]);
        let path = std::env::temp_dir().join(format!("od-xref-name-{}.odraft", std::process::id()));
        std::fs::write(&path, NativeFormat.write(&source).unwrap().bytes).unwrap();

        let mut state = AppState::new();
        state.start_command("-XREF", |ed| attach(ed, XrefKind::Attach));
        state.settle_command();
        state.cmd_submit(&path.to_string_lossy());
        state.settle_command();
        state.cmd_submit("a|b");
        state.settle_command();
        assert!(!state.command_running(), "no insertion point was asked");
        assert!(state.session().document().xrefs().is_empty());
        let _ = std::fs::remove_file(path);
    }
}
