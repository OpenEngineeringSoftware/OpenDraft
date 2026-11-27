//! LAYOFF, LAYFRZ and LAYLCK: turn off, freeze or lock the layer of an
//! object. All three work the same way: objects already selected have their
//! layers changed at once; otherwise each object picked changes its layer,
//! until Enter, with Undo taking back the last one. (AutoCAD's LAYLCK takes
//! one object only; here it goes on like the other two.)

use opendraft_commands::Wording;
use opendraft_commands::{Answer, Cancelled, Editor, ModifyLayers, Options, Picked};
use opendraft_core::{EntityId, Layer, LayerId};
use opendraft_ui::key;

/// What the command does to a layer.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Switch {
    Off,
    Freeze,
    Lock,
}

impl Switch {
    fn prompt(self) -> &'static str {
        match self {
            Switch::Off => key!("Select an object on the layer to be turned off"),
            Switch::Freeze => key!("Select an object on the layer to be frozen"),
            Switch::Lock => key!("Select an object on the layer to be locked"),
        }
    }

    /// The layer already is what the command would make it.
    fn done(self, layer: &Layer) -> bool {
        match self {
            Switch::Off => !layer.on,
            Switch::Freeze => layer.frozen,
            Switch::Lock => layer.locked,
        }
    }

    fn apply(self, layer: Layer) -> Layer {
        match self {
            Switch::Off => Layer { on: false, ..layer },
            Switch::Freeze => Layer {
                frozen: true,
                ..layer
            },
            Switch::Lock => Layer {
                locked: true,
                ..layer
            },
        }
    }

    fn step(self) -> &'static str {
        match self {
            Switch::Off => "Turn layer off",
            Switch::Freeze => "Freeze layer",
            Switch::Lock => "Lock layer",
        }
    }

    /// "turned off", "frozen", "locked".
    /// "Layer "%{name}" is already off.", to say nothing changed.
    fn already(self) -> &'static str {
        match self {
            Switch::Off => key!("Layer \"%{name}\" is already turned off."),
            Switch::Freeze => key!("Layer \"%{name}\" is already frozen."),
            Switch::Lock => key!("Layer \"%{name}\" is already locked."),
        }
    }

    /// "Layer "%{name}" has been turned off.", to say it changed.
    fn done_text(self) -> &'static str {
        match self {
            Switch::Off => key!("Layer \"%{name}\" has been turned off."),
            Switch::Freeze => key!("Layer \"%{name}\" has been frozen."),
            Switch::Lock => key!("Layer \"%{name}\" has been locked."),
        }
    }

    fn undone(self) -> &'static str {
        match self {
            Switch::Off => key!("Layer turned on again."),
            Switch::Freeze => key!("Layer thawed again."),
            Switch::Lock => key!("Layer unlocked again."),
        }
    }
}

/// LAYOFF: turn off the layer of each object picked, or of the objects
/// selected beforehand. Turning off the current layer is asked first.
pub fn turn_off_layers(ed: &mut Editor) -> Result<(), Cancelled> {
    switch_layers(ed, Switch::Off)
}

/// LAYFRZ: freeze the layer of each object picked, or of the objects
/// selected beforehand. The current layer cannot be frozen.
pub fn freeze_layers(ed: &mut Editor) -> Result<(), Cancelled> {
    switch_layers(ed, Switch::Freeze)
}

/// LAYLCK: lock the layer of each object picked, or of the objects selected
/// beforehand.
pub fn lock_layers(ed: &mut Editor) -> Result<(), Cancelled> {
    switch_layers(ed, Switch::Lock)
}

fn switch_layers(ed: &mut Editor, switch: Switch) -> Result<(), Cancelled> {
    let chosen = ed.pickset().to_vec();
    if !chosen.is_empty() {
        for layer in layers_of(ed, chosen.clone())? {
            switch_one(ed, switch, layer)?;
        }
        ed.deselect(chosen);
        return Ok(());
    }
    let mut changed = 0_usize;
    loop {
        let options = Options::<Picked>::new(switch.prompt())
            .keyword("Undo", "U")
            .allow_empty();
        match ed.ask_object(options)? {
            Answer::Value(Picked { id, .. }) => {
                if let Some(layer) = layers_of(ed, vec![id])?.into_iter().next() {
                    if switch_one(ed, switch, layer)? {
                        changed += 1;
                    }
                }
            }
            Answer::Keyword(_) if changed > 0 => {
                ed.undo()?;
                changed -= 1;
                ed.say(switch.undone());
            }
            Answer::Keyword(_) => ed.say(key!("Nothing to undo.")),
            Answer::Enter => return Ok(()),
        }
    }
}

/// The layers `ids` are on, each once, in the drawing's layer order.
fn layers_of(ed: &Editor, ids: Vec<EntityId>) -> Result<Vec<Layer>, Cancelled> {
    ed.read(move |doc| {
        let on: Vec<LayerId> = ids
            .iter()
            .filter_map(|id| doc.object(*id))
            .map(|o| o.attrs.layer)
            .collect();
        doc.layers()
            .iter()
            .filter(|l| on.contains(&l.id))
            .cloned()
            .collect()
    })
}

/// Change one layer as an undo step, saying what happened. Returns whether
/// it changed.
fn switch_one(ed: &Editor, switch: Switch, layer: Layer) -> Result<bool, Cancelled> {
    let name = layer.name.clone();
    if switch.done(&layer) {
        ed.say(Wording::new(switch.already(), &[("name", &name)]));
        return Ok(false);
    }
    let current = ed.read(|doc| doc.current_layer())? == layer.id;
    if current && switch == Switch::Freeze {
        ed.say(Wording::new(
            key!("Cannot freeze layer \"%{name}\". It is the current layer."),
            &[("name", &name)],
        ));
        return Ok(false);
    }
    if current && switch == Switch::Off {
        let options = Options::<String>::worded(
            key!("Really want layer \"%{name}\" (the CURRENT layer) off?"),
            &[("name", &name)],
        )
        .keyword("Yes", "Y")
        .keyword("No", "N")
        .default("No".to_owned());
        if ed.get_keyword(options)? != "Yes" {
            return Ok(false);
        }
    }
    let changed = switch.apply(layer);
    ed.transaction(switch.step(), move |tx| {
        tx.perform(ModifyLayers::new(switch.step(), vec![changed]));
        Ok(())
    })?;
    ed.say(Wording::new(switch.done_text(), &[("name", &name)]));
    Ok(true)
}

#[cfg(test)]
mod tests {
    use opendraft_commands::ModifyAttributes;
    use opendraft_core::{Attributes, Entity, Point};

    use super::*;
    use crate::state::AppState;

    fn line(y: f64) -> Entity {
        Entity::Line {
            start: Point::new(0.0, y),
            end: Point::new(10.0, y),
        }
    }

    /// A line on layer 0 (current), one on "Layer1" and one on "Layer2".
    fn drawing() -> (AppState, [EntityId; 3], [LayerId; 2]) {
        let mut state = AppState::new();
        for y in [0.0, 5.0, 10.0] {
            state.add_entity(line(y));
        }
        let (one, two) = (state.new_layer(), state.new_layer());
        let ids = {
            let objects = state.session().document().objects();
            [objects[0].id, objects[1].id, objects[2].id]
        };
        state.perform(ModifyAttributes::new(
            "Spread",
            vec![
                (ids[1], Attributes::on_layer(one)),
                (ids[2], Attributes::on_layer(two)),
            ],
        ));
        (state, ids, [one, two])
    }

    fn layer(state: &AppState, id: LayerId) -> Layer {
        state.session().document().layer(id).cloned().unwrap()
    }

    fn pick(state: &mut AppState, id: EntityId) {
        state.answer_object(Point::new(1.0, 0.0), Some(id), false);
        state.settle_command();
    }

    fn said(state: &AppState) -> &str {
        state.cmdline.log.last().map_or("", String::as_str)
    }

    fn start(
        state: &mut AppState,
        name: &'static str,
        run: fn(&mut Editor) -> Result<(), Cancelled>,
    ) {
        state.start_command(name, run);
        state.settle_command();
    }

    type Run = fn(&mut Editor) -> Result<(), Cancelled>;
    type Case = (&'static str, Run, fn(&Layer) -> bool);

    /// Each command, and whether a layer shows what it does.
    fn cases() -> [Case; 3] {
        [
            ("LAYOFF", turn_off_layers, |l| !l.on),
            ("LAYFRZ", freeze_layers, |l| l.frozen),
            ("LAYLCK", lock_layers, |l| l.locked),
        ]
    }

    #[test]
    fn each_command_goes_on_picking_until_enter_and_undoes_the_last() {
        let cases = cases();
        for (name, run, changed) in cases {
            let (mut state, ids, [one, two]) = drawing();
            start(&mut state, name, run);
            pick(&mut state, ids[1]);
            pick(&mut state, ids[2]);
            assert!(changed(&layer(&state, one)), "{name}");
            assert!(
                changed(&layer(&state, two)),
                "{name}: goes on after the first"
            );
            state.answer_keyword("Undo");
            state.settle_command();
            assert!(
                !changed(&layer(&state, two)),
                "{name}: Undo takes back the last"
            );
            assert!(changed(&layer(&state, one)), "{name}");
            state.answer_enter();
            state.settle_command();
            assert!(!state.command_running(), "{name}: Enter ends it");
        }
    }

    #[test]
    fn objects_selected_first_have_their_layers_changed_at_once() {
        let cases = cases();
        for (name, run, changed) in cases {
            let (mut state, ids, [one, two]) = drawing();
            state.select_ids([ids[1], ids[2]], false);
            start(&mut state, name, run);
            assert!(!state.command_running(), "{name}: no question asked");
            assert!(
                changed(&layer(&state, one)) && changed(&layer(&state, two)),
                "{name}"
            );
            assert!(
                state.selection().is_empty(),
                "{name}: nothing left selected"
            );
        }
    }

    #[test]
    fn the_current_layer_is_asked_about_refused_or_locked() {
        let (mut state, ids, _) = drawing();
        // LAYOFF asks, and No (the default) leaves it on.
        start(&mut state, "LAYOFF", turn_off_layers);
        pick(&mut state, ids[0]);
        assert!(state.cmd_prompt().text.contains("CURRENT layer"));
        state.answer_enter();
        state.settle_command();
        assert!(layer(&state, LayerId::DEFAULT).on);
        pick(&mut state, ids[0]);
        state.answer_keyword("Yes");
        state.settle_command();
        assert!(!layer(&state, LayerId::DEFAULT).on);
        state.answer_enter();
        state.settle_command();

        // LAYFRZ refuses it.
        let (mut state, ids, _) = drawing();
        start(&mut state, "LAYFRZ", freeze_layers);
        pick(&mut state, ids[0]);
        assert!(said(&state).contains("It is the current layer"));
        assert!(!layer(&state, LayerId::DEFAULT).frozen);

        // LAYLCK locks it.
        let (mut state, ids, _) = drawing();
        start(&mut state, "LAYLCK", lock_layers);
        pick(&mut state, ids[0]);
        assert!(layer(&state, LayerId::DEFAULT).locked);
        assert_eq!(said(&state), "Layer \"0\" has been locked.");
    }
}
