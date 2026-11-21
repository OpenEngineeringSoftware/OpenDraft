//! What TRIM and EXTEND remember between uses, as AutoCAD's
//! `TRIMEXTENDMODE`: Quick or Standard mode. Saved with the user's settings,
//! not in the drawing.

use opendraft_commands::{SysVar, Value};

use crate::state::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Memory {
    /// Quick mode (AutoCAD's default): every object is an edge without
    /// asking, and TRIM erases what no edge crosses. Standard mode asks for
    /// the edges first.
    pub quick: bool,
}

const DEFAULT: Memory = Memory { quick: true };

impl Default for Memory {
    fn default() -> Self {
        DEFAULT
    }
}

const KEY: &str = "trim_extend_quick";

/// Take the saved setting, keeping the default when it is missing.
pub fn load(storage: Option<&dyn eframe::Storage>) -> Memory {
    match storage.and_then(|s| s.get_string(KEY)).as_deref() {
        Some("0") => Memory { quick: false },
        _ => DEFAULT,
    }
}

/// Write the setting for the next run.
pub fn save(m: &Memory, storage: &mut dyn eframe::Storage) {
    storage.set_string(KEY, if m.quick { "1" } else { "0" }.to_owned());
}

/// `TRIMEXTENDMODE`.
pub fn variables() -> Vec<SysVar<AppState>> {
    vec![SysVar::int(
        "TRIMEXTENDMODE",
        "TRIM and EXTEND mode: 0 Standard (choose the edges), 1 Quick (every object is an edge)",
        (0, 1),
        |st| Value::Int(i64::from(st.defaults().trim.get().quick)),
        |st, v| {
            let quick = matches!(v, Value::Int(1));
            st.defaults().trim.change(|m| m.quick = quick);
            Ok(())
        },
    )]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quick_is_the_default_and_the_variable_changes_it() {
        assert!(load(None).quick);
        let (table, mut state) = (crate::sysvars::table(), AppState::new());
        assert_eq!(table.get("TRIMEXTENDMODE", &state), Ok(Value::Int(1)));
        table.set("TRIMEXTENDMODE", "0", &mut state).unwrap();
        assert!(!state.defaults().trim.get().quick);
        assert!(table.set("TRIMEXTENDMODE", "2", &mut state).is_err());
    }
}
