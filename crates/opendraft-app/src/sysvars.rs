//! The application's system variables (AutoCAD's `GRIDMODE`, `OSMODE`...),
//! bound to [`Settings`](crate::settings::Settings) and the open drawing.
//! The table type lives in `opendraft-commands`; this is the list.
//!
//! Variables that mirror a setting are saved with the user's settings, so
//! they persist with no extra code. Variables saved in the drawing are not
//! built yet (see "System variables" in docs/ARCHITECTURE.md).

use std::sync::OnceLock;

use opendraft_commands::{Kind, SysVar, SysVarTable, Value};

use crate::settings::{
    AUTOSAVE_CHOICES, CROSSHAIR_SIZE_RANGE, GRID_SPACING_RANGE, GRIP_SIZE_RANGE, PICK_SIZE_RANGE,
    SNAP_APERTURE_RANGE,
};
use crate::state::AppState;

fn flag(on: bool) -> Value {
    Value::Int(i64::from(on))
}

fn int(value: Value) -> i64 {
    match value {
        Value::Int(n) => n,
        _ => 0,
    }
}

/// A pixel size the table has already range-checked.
fn size(value: Value) -> u32 {
    u32::try_from(int(value)).unwrap_or(1)
}

fn range(bounds: (u32, u32)) -> (i64, i64) {
    (i64::from(bounds.0), i64::from(bounds.1))
}

fn build() -> Vec<SysVar<AppState>> {
    vec![
        SysVar::int(
            "GRIDMODE",
            "Grid display: 0 off, 1 on",
            (0, 1),
            |s| flag(s.settings.show_grid),
            |s, v| {
                s.settings.show_grid = int(v) == 1;
                Ok(())
            },
        ),
        SysVar::int(
            "VPCONTROL",
            "Viewport controls in the drawing's corner: 0 hidden, 1 shown",
            (0, 1),
            |s| flag(s.settings.show_viewport_controls),
            |s, v| {
                s.settings.show_viewport_controls = int(v) == 1;
                Ok(())
            },
        ),
        SysVar::real(
            "GRIDUNIT",
            "Grid spacing in drawing units",
            GRID_SPACING_RANGE,
            |s| Value::Real(s.settings.grid_spacing),
            |s, v| {
                if let Value::Real(n) = v {
                    s.settings.grid_spacing = n;
                }
                Ok(())
            },
        ),
        SysVar::int(
            "ORTHOMODE",
            "Ortho: 0 off, 1 on",
            (0, 1),
            |s| flag(s.settings.ortho.enabled),
            |s, v| {
                s.settings.set_ortho(int(v) == 1);
                Ok(())
            },
        ),
        SysVar::int(
            "ORTHOLOCK",
            "How firmly ortho holds: 0 an object snap wins, 1 an object snap is held to the axis",
            (0, 1),
            |s| flag(s.settings.ortho.strict),
            |s, v| {
                s.settings.ortho.strict = int(v) == 1;
                Ok(())
            },
        ),
        SysVar::int(
            "SNAPMODE",
            "Snap to the grid: 0 off, 1 on",
            (0, 1),
            |s| flag(s.settings.snap.grid),
            |s, v| {
                s.settings.snap.grid = int(v) == 1;
                Ok(())
            },
        ),
        SysVar::int(
            "CURSORSIZE",
            "Length of the crosshair lines as a percent of the window",
            range(CROSSHAIR_SIZE_RANGE),
            |s| Value::Int(i64::from(s.settings.crosshair_size)),
            |s, v| {
                s.settings.crosshair_size = size(v);
                Ok(())
            },
        ),
        SysVar::int(
            "PICKBOX",
            "Radius in pixels within which a click picks an object",
            range(PICK_SIZE_RANGE),
            |s| Value::Int(i64::from(s.settings.pick_size)),
            |s, v| {
                s.settings.pick_size = size(v);
                Ok(())
            },
        ),
        SysVar::int(
            "APERTURE",
            "Radius in pixels within which the cursor snaps to geometry",
            range(SNAP_APERTURE_RANGE),
            |s| Value::Int(i64::from(s.settings.snap_aperture)),
            |s, v| {
                s.settings.snap_aperture = size(v);
                Ok(())
            },
        ),
        SysVar::int(
            "GRIPSIZE",
            "Size in pixels of a grip square",
            range(GRIP_SIZE_RANGE),
            |s| Value::Int(i64::from(s.settings.grip_size)),
            |s, v| {
                s.settings.grip_size = size(v);
                Ok(())
            },
        ),
        SysVar::int(
            "SAVETIME",
            "Minutes between automatic saves, 0 for off",
            (0, 10),
            |s| Value::Int(i64::from(s.settings.autosave_minutes)),
            |s, v| {
                let minutes = u32::try_from(int(v)).unwrap_or(u32::MAX);
                if !AUTOSAVE_CHOICES.contains(&minutes) {
                    let choices: Vec<_> = AUTOSAVE_CHOICES.iter().map(u32::to_string).collect();
                    return Err(format!("Requires one of {}.", choices.join(", ")));
                }
                s.settings.autosave_minutes = minutes;
                Ok(())
            },
        ),
        SysVar::int(
            "ARRAYASSOCIATIVITY",
            "1 to make arrays that ARRAYEDIT can edit as a whole, 0 for plain copies",
            (0, 1),
            |st| Value::Int(i64::from(!st.defaults().array_plain.get())),
            |st, v| {
                let plain = !matches!(v, Value::Int(1));
                st.defaults().array_plain.change(|p| *p = plain);
                Ok(())
            },
        ),
        SysVar::int(
            "PEDITACCEPT",
            "1 to turn lines and arcs into polylines in PEDIT without asking",
            (0, 1),
            |st| Value::Int(i64::from(st.defaults().pedit.get().accept)),
            |st, v| {
                st.defaults().pedit.change(|m| m.accept = int(v) == 1);
                Ok(())
            },
        ),
        SysVar::int(
            "PLINECONVERTMODE",
            "How PEDIT turns a spline into a polyline: 0 straight pieces, 1 arcs",
            (0, 1),
            |st| Value::Int(i64::from(st.defaults().pedit.get().convert_arcs)),
            |st, v| {
                st.defaults().pedit.change(|m| m.convert_arcs = int(v) == 1);
                Ok(())
            },
        ),
        SysVar::int(
            "AFLAGS",
            "Attribute modes new definitions start with: 1 invisible, 2 constant, 4 verify, 8 preset, 16 lock position",
            (0, i64::from(crate::attribute_memory::AFLAGS_MASK)),
            |st| Value::Int(i64::from(st.defaults().attribute.get().aflags())),
            |st, v| {
                let value = u16::try_from(int(v)).unwrap_or(0);
                st.defaults().attribute.change(|m| m.set_aflags(value));
                Ok(())
            },
        ),
        SysVar::int(
            "ATTREQ",
            "1 to ask for attribute values when a block is inserted, 0 to use the defaults",
            (0, 1),
            |st| Value::Int(i64::from(st.defaults().attribute.get().prompt_on_insert)),
            |st, v| {
                st.defaults().attribute.change(|m| m.prompt_on_insert = int(v) == 1);
                Ok(())
            },
        ),
        SysVar::int(
            "FIELDEVAL",
            "When fields are updated: 1 on open, 2 on save, 4 on plot",
            (0, i64::from(crate::settings::FIELD_EVAL_ALL)),
            |s| Value::Int(i64::from(s.settings.field_eval)),
            |s, v| {
                s.settings.field_eval = u8::try_from(int(v)).unwrap_or(0);
                Ok(())
            },
        ),
        SysVar::real(
            "TEXTSIZE",
            "Height of new text",
            (1.0e-6, 1.0e9),
            |st| Value::Real(st.defaults().text.get().height),
            |st, v| {
                if let Value::Real(n) = v {
                    st.defaults().text.change(|m| m.height = n);
                }
                Ok(())
            },
        ),
        SysVar::int(
            "POLYSIDES",
            "Number of sides POLYGON offers first",
            (3, 1024),
            |st| Value::Int(st.defaults().polygon.get().sides as i64),
            |st, v| {
                let sides = usize::try_from(int(v)).unwrap_or(4);
                st.defaults().polygon.change(|m| m.sides = sides);
                Ok(())
            },
        ),
        SysVar::real(
            "PLINEWID",
            "Width of the next rectangles",
            (0.0, 1.0e9),
            |st| Value::Real(st.defaults().rect.get().width),
            |st, v| {
                if let Value::Real(n) = v {
                    st.defaults().rect.change(|m| m.width = n);
                }
                Ok(())
            },
        ),
        SysVar::real(
            "ELEVATION",
            "Elevation of the next rectangles (kept, not drawn: 2D only)",
            (-1.0e9, 1.0e9),
            |st| Value::Real(st.defaults().rect.get().elevation),
            |st, v| {
                if let Value::Real(n) = v {
                    st.defaults().rect.change(|m| m.elevation = n);
                }
                Ok(())
            },
        ),
        SysVar::real(
            "THICKNESS",
            "Thickness of the next rectangles (kept, not drawn: 2D only)",
            (-1.0e9, 1.0e9),
            |st| Value::Real(st.defaults().rect.get().thickness),
            |st, v| {
                if let Value::Real(n) = v {
                    st.defaults().rect.change(|m| m.thickness = n);
                }
                Ok(())
            },
        ),
        SysVar::read_only(
            "CMDACTIVE",
            "Whether a command is running: 0 no, 1 yes",
            Kind::Int { min: 0, max: 1 },
            |s| flag(s.command_running()),
        ),
        SysVar::read_only("DWGNAME", "Name of the current drawing", Kind::Text, |s| {
            Value::Text(s.docs[s.active].title.clone())
        }),
        SysVar::int(
            "VISRETAIN",
            "Keep xref layer properties on reload: 0 no, 1 yes",
            (0, 1),
            |s| flag(s.session().document().visretain()),
            |s, v| {
                s.perform(opendraft_commands::SetVisretain::new(
                    "VISRETAIN",
                    int(v) == 1,
                ));
                Ok(())
            },
        ),
        SysVar::int(
            "XREFTYPE",
            "Default reference type for a new external reference: 0 attach, 1 overlay",
            (0, 1),
            |s| flag(s.settings.xref_overlay),
            |s, v| {
                s.settings.xref_overlay = int(v) == 1;
                Ok(())
            },
        ),
        SysVar::int(
            "REFPATHTYPE",
            "Default saved path for a new external reference: 0 no path, 1 relative, 2 full",
            (0, 2),
            |s| Value::Int(s.settings.ref_path_type.code()),
            |s, v| {
                s.settings.ref_path_type = crate::settings::RefPathDefault::from_code(int(v));
                Ok(())
            },
        ),
        SysVar::int(
            "BINDTYPE",
            "How Bind names dependent symbols: 0 prefix with $0$, 1 bare and merge",
            (0, 1),
            |s| flag(s.settings.bind_insert),
            |s, v| {
                s.settings.bind_insert = int(v) == 1;
                Ok(())
            },
        ),
    ]
}

/// The table of system variables.
pub fn table() -> &'static SysVarTable<AppState> {
    static TABLE: OnceLock<SysVarTable<AppState>> = OnceLock::new();
    TABLE.get_or_init(|| {
        let mut variables = build();
        variables.extend(crate::dynamic_settings::variables());
        variables.extend(crate::snap_vars::variables());
        variables.extend(crate::units_vars::variables());
        variables.extend(crate::linetype_vars::variables());
        variables.extend(crate::point_vars::variables());
        variables.extend(crate::dim_vars::variables());
        variables.extend(crate::dim_vars_extra::variables());
        variables.extend(crate::spline_memory::variables());
        variables.extend(crate::hatch_memory::variables());
        variables.extend(crate::trim_memory::variables());
        variables.extend(crate::modify_vars::variables());
        SysVarTable::new(variables).unwrap_or_else(|e| panic!("invalid system variable table: {e}"))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variables_read_and_write_the_settings() {
        let (table, mut state) = (table(), AppState::new());
        assert_eq!(table.get("GRIDMODE", &state), Ok(Value::Int(1)));
        table.set("gridmode", "0", &mut state).unwrap();
        assert!(!state.settings.show_grid);
        table.set("GRIDUNIT", "25", &mut state).unwrap();
        assert_eq!(state.settings.grid_spacing, 25.0);
        table.set("OSMODE", "16385", &mut state).unwrap();
        assert!(!state.settings.snap.enabled && state.settings.snap.endpoint);
        table.set("SNAPMODE", "0", &mut state).unwrap();
        assert!(!state.settings.snap.grid);
    }

    #[test]
    fn the_xref_defaults_are_variables() {
        let (table, mut state) = (table(), AppState::new());
        assert_eq!(table.get("XREFTYPE", &state), Ok(Value::Int(0)));
        assert_eq!(table.get("REFPATHTYPE", &state), Ok(Value::Int(1)));
        table.set("XREFTYPE", "1", &mut state).unwrap();
        assert!(state.settings.xref_overlay);
        table.set("REFPATHTYPE", "2", &mut state).unwrap();
        assert_eq!(
            state.settings.ref_path_type,
            crate::settings::RefPathDefault::Full
        );
        assert!(table.set("REFPATHTYPE", "3", &mut state).is_err());
        assert_eq!(
            state.settings.ref_path_type,
            crate::settings::RefPathDefault::Full
        );
        assert_eq!(table.get("BINDTYPE", &state), Ok(Value::Int(0)));
        table.set("BINDTYPE", "1", &mut state).unwrap();
        assert!(state.settings.bind_insert);
    }

    #[test]
    fn vpcontrol_shows_and_hides_the_viewport_controls() {
        let (table, mut state) = (table(), AppState::new());
        assert_eq!(table.get("VPCONTROL", &state), Ok(Value::Int(1)));
        table.set("vpcontrol", "0", &mut state).unwrap();
        assert!(!state.settings.show_viewport_controls);
        assert!(table.set("VPCONTROL", "2", &mut state).is_err());
        table.set("VPCONTROL", "1", &mut state).unwrap();
        assert!(state.settings.show_viewport_controls);
    }

    #[test]
    fn the_pointer_sizes_are_variables_with_ranges() {
        let (table, mut state) = (table(), AppState::new());
        assert_eq!(table.get("PICKBOX", &state), Ok(Value::Int(4)));
        table.set("PICKBOX", "10", &mut state).unwrap();
        table.set("APERTURE", "20", &mut state).unwrap();
        table.set("GRIPSIZE", "9", &mut state).unwrap();
        table.set("CURSORSIZE", "100", &mut state).unwrap();
        assert_eq!(state.settings.crosshair().size, 100);
        assert_eq!(
            state.settings.pointer_sizes(),
            opendraft_canvas::PointerSizes {
                pick: 10.0,
                snap: 20.0,
                grip: 9.0
            }
        );
        assert!(table.set("PICKBOX", "0", &mut state).is_err());
        assert!(table.set("GRIPSIZE", "21", &mut state).is_err());
        assert_eq!(
            state.settings.grip_size, 9,
            "a refused value changes nothing"
        );
    }

    #[test]
    fn savetime_accepts_only_the_intervals_the_settings_offer() {
        let (table, mut state) = (table(), AppState::new());
        assert!(table.set("SAVETIME", "3", &mut state).is_err());
        assert_eq!(state.settings.autosave_minutes, 2);
        table.set("SAVETIME", "5", &mut state).unwrap();
        assert_eq!(state.settings.autosave_minutes, 5);
    }

    #[test]
    fn state_variables_are_read_only() {
        let (table, mut state) = (table(), AppState::new());
        assert_eq!(table.get("CMDACTIVE", &state), Ok(Value::Int(0)));
        assert!(table.set("CMDACTIVE", "1", &mut state).is_err());
        assert!(matches!(
            table.get("DWGNAME", &state),
            Ok(Value::Text(name)) if !name.is_empty()
        ));
    }

    #[test]
    fn the_fillet_and_chamfer_settings_are_variables() {
        let (table, mut state) = (table(), AppState::new());
        table.set("FILLETRAD", "4.5", &mut state).unwrap();
        assert_eq!(table.get("FILLETRAD", &state), Ok(Value::Real(4.5)));
        assert_eq!(state.modify_settings().fillet_radius, 4.5);
        table.set("CHAMMODE", "1", &mut state).unwrap();
        assert!(state.modify_settings().chamfer_by_angle);
        table.set("CHAMFERD", "30", &mut state).unwrap();
        assert_eq!(state.modify_settings().chamfer_angle, 30.0);
        table.set("TRIMMODE", "0", &mut state).unwrap();
        assert!(!state.modify_settings().trim);
        assert!(
            table.set("CHAMFERD", "200", &mut state).is_err(),
            "out of range"
        );
        assert!(table.set("FILLETRAD", "-1", &mut state).is_err());
        assert!(table.set("TRIMMODE", "2", &mut state).is_err());
    }

    #[test]
    fn peditaccept_is_a_variable() {
        let (table, mut state) = (table(), AppState::new());
        assert_eq!(table.get("PEDITACCEPT", &state), Ok(Value::Int(0)));
        table.set("PEDITACCEPT", "1", &mut state).unwrap();
        assert!(state.defaults().pedit.get().accept);
        assert!(table.set("PEDITACCEPT", "2", &mut state).is_err());
    }
}
