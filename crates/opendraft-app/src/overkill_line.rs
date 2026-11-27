//! -OVERKILL: the command-line form of OVERKILL. After the objects it loops
//! on `Enter an option <accept> [Ignore/Tolerance/...]`, shares its settings
//! with the dialog, and tidies with the same `run`.

use opendraft_commands::{Cancelled, Editor, Options};
use opendraft_core::overkill::{Ignore, Settings};
use opendraft_ui::key;

use crate::defaults::EditorDefaults;
use crate::overkill_cmd::run;
use opendraft_commands::Wording;

fn yes_no(on: bool) -> &'static str {
    if on {
        "Yes"
    } else {
        "No"
    }
}

/// The settings in one line, for the command line.
fn describe(ed: &Editor, s: &Settings) -> Result<Wording, Cancelled> {
    let units = ed.read(|doc| *doc.units())?;
    let names = [
        (s.ignore.color, "Color"),
        (s.ignore.layer, "Layer"),
        (s.ignore.linetype, "Ltype"),
        (s.ignore.linetype_scale, "Ltscale"),
        (s.ignore.lineweight, "LWeight"),
    ];
    let ignored: Vec<&str> = names.iter().filter(|n| n.0).map(|n| n.1).collect();
    let ignored = if ignored.is_empty() {
        key!("None").to_owned()
    } else {
        ignored.join(", ")
    };
    let yes = |on| Wording::plain(yes_no(on));
    Ok(Wording::new(
        key!("Tolerance = %{tolerance}, Ignore = %{ignore}, Optimize polylines = %{optimize}, Combine partial overlap = %{overlap}, Combine end to end = %{ends}, Associativity = %{associative}"),
        &[("tolerance", &opendraft_core::units::format_length(&units, s.tolerance))],
    )
    .part("ignore", Wording::plain(ignored))
    .part("optimize", yes(s.optimize_polylines))
    .part("overlap", yes(s.overlapping))
    .part("ends", yes(s.end_to_end))
    .part("associative", yes(s.maintain_associative)))
}

/// Yes or No, with `current` as the default.
fn ask_yes_no(ed: &Editor, message: &str, current: bool) -> Result<bool, Cancelled> {
    let options = Options::<String>::new(message)
        .keyword("Yes", "Y")
        .keyword("No", "N")
        .default(yes_no(current).to_owned());
    Ok(match ed.ask_keyword(options)?.as_deref() {
        Some("Yes") => true,
        Some("No") => false,
        _ => current,
    })
}

/// Choose properties to ignore, until Enter. `None` clears them all and
/// `All` ignores every one.
fn ask_ignore(ed: &Editor, ignore: &mut Ignore) -> Result<(), Cancelled> {
    loop {
        let options = Options::<String>::new(key!("Enter property to ignore, Enter when done"))
            .keyword("None", "N")
            .keyword("All", "A")
            .keyword("Color", "C")
            .keyword("LAyer", "LA")
            .keyword("Ltype", "LT")
            .keyword("Ltscale", "LTS")
            .keyword("LWeight", "LW");
        match ed.ask_keyword(options)?.as_deref() {
            Some("None") => *ignore = Ignore::default(),
            Some("All") => {
                *ignore = Ignore {
                    layer: true,
                    color: true,
                    linetype: true,
                    linetype_scale: true,
                    lineweight: true,
                }
            }
            Some("Color") => ignore.color = !ignore.color,
            Some("LAyer") => ignore.layer = !ignore.layer,
            Some("Ltype") => ignore.linetype = !ignore.linetype,
            Some("Ltscale") => ignore.linetype_scale = !ignore.linetype_scale,
            Some("LWeight") => ignore.lineweight = !ignore.lineweight,
            _ => return Ok(()),
        }
    }
}

/// -OVERKILL.
pub fn overkill_prompt(ed: &mut Editor) -> Result<(), Cancelled> {
    let chosen = ed.pickset_or_select(key!("Select objects"))?;
    let mut settings = ed.defaults().overkill.get();
    loop {
        let line = describe(ed, &settings)?;
        ed.say(line);
        let options = Options::<String>::new(key!("Enter an option <accept>"))
            .keyword("Ignore", "I")
            .keyword("Tolerance", "T")
            .keyword(key!("Optimize polylines"), "O")
            .keyword(key!("Combine partial overlap"), "C")
            .keyword("combine End to end", "E")
            .keyword("Associativity", "A");
        match ed.ask_keyword(options)?.as_deref() {
            Some("Ignore") => ask_ignore(ed, &mut settings.ignore)?,
            Some("Tolerance") => {
                let ask =
                    Options::<f64>::new(key!("Specify tolerance")).default(settings.tolerance);
                let value = ed.get_distance(ask)?;
                if value.is_finite() && value >= 0.0 {
                    settings.tolerance = value;
                } else {
                    ed.say(key!("The tolerance must be zero or more."));
                }
            }
            Some(key!("Optimize polylines")) => {
                settings.optimize_polylines =
                    ask_yes_no(ed, key!("Optimize polylines"), settings.optimize_polylines)?;
            }
            Some(key!("Combine partial overlap")) => {
                settings.overlapping =
                    ask_yes_no(ed, key!("Combine partial overlap"), settings.overlapping)?;
            }
            Some("combine End to end") => {
                settings.end_to_end =
                    ask_yes_no(ed, key!("Combine end to end"), settings.end_to_end)?;
            }
            Some("Associativity") => {
                settings.maintain_associative = ask_yes_no(
                    ed,
                    key!("Maintain associative objects"),
                    settings.maintain_associative,
                )?;
            }
            _ => break,
        }
    }
    ed.defaults().overkill.change(|m| *m = settings);
    run(ed, chosen, settings)
}
