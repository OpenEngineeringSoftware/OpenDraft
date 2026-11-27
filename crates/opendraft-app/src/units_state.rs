//! The drawing's units as the app uses them: reading the active drawing's
//! settings, writing prompt defaults in them, and changing them (the UNITS
//! command and the `LUNITS`... variables).

use opendraft_commands::{Hint, SetProperties, SetUnits, Wants};
use opendraft_core::drawing_properties::DrawingProperties;
use opendraft_core::units::{format_angle, format_length};
use opendraft_core::{AngleFormat, Units};

use crate::coordinates::Reading;
use crate::state::AppState;
use opendraft_ui::key;

impl AppState {
    /// The active drawing's units.
    pub fn units(&self) -> Units {
        *self.session().document().units()
    }

    /// The active drawing's units in the coordinate system of the space
    /// being edited: how typed values are read and shown.
    pub fn reading(&self) -> Reading {
        Reading::of(self.session().document())
    }

    /// Change the active drawing's units as one undo step. Nothing is done
    /// (and nothing recorded) when they are already these.
    pub fn set_units(&mut self, units: Units) {
        let units = units.sanitized();
        if units == self.units() {
            return;
        }
        self.perform(SetUnits::new(units));
        self.message = key!("Units changed").to_owned();
    }
}

impl AppState {
    /// The active drawing's properties.
    pub fn drawing_properties(&self) -> DrawingProperties {
        self.session().document().properties().clone()
    }

    /// Change the active drawing's properties as one undo step. Nothing is
    /// done when they are already these.
    pub fn set_drawing_properties(&mut self, properties: DrawingProperties) {
        let properties = properties.sanitized();
        if properties == self.drawing_properties() {
            return;
        }
        self.perform(SetProperties::new(properties));
        self.message = key!("Drawing properties changed").to_owned();
    }
}

/// An amount turned by, for a message such as "Rotated 2 objects 30.00°":
/// the drawing's angle format (not counted from its base angle), with at
/// least two decimals so a typed 30.5 is not reported as 31.
pub fn angle_message(units: &Units, angle: f64) -> String {
    let precise = Units {
        angle_precision: units.angle_precision.max(2),
        ..*units
    };
    let text = format_angle(&precise, angle + units.from_display_angle(0.0));
    match units.angle {
        AngleFormat::DecimalDegrees => format!("{text}\u{b0}"),
        _ => text,
    }
}

/// A prompt's default as it is written between `<` and `>`: points in the
/// coordinate system, an angle as a direction in it or, when `turn`, as an
/// amount to turn by.
pub fn hint_text(reading: impl Into<Reading>, wants: Wants, turn: bool, hint: &Hint) -> String {
    let reading = reading.into();
    match hint {
        Hint::Text(text) => text.clone(),
        Hint::Point(p) => reading.show_point(*p),
        Hint::Number(n) => match wants {
            Wants::Distance | Wants::DistanceOrPoint => format_length(&reading.units, *n),
            Wants::Angle | Wants::AngleOrPoint if turn => reading.show_turn(*n),
            Wants::Angle | Wants::AngleOrPoint => reading.show_direction(*n),
            _ => n.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::{LengthFormat, Point};

    use super::*;

    #[test]
    fn defaults_are_written_in_the_drawings_units() {
        let units = Units {
            length: LengthFormat::Architectural,
            length_precision: 2,
            ..Units::default()
        };
        assert_eq!(
            hint_text(&units, Wants::Distance, false, &Hint::Number(63.5)),
            "5'-3 1/2\""
        );
        assert_eq!(
            hint_text(
                &units,
                Wants::Angle,
                false,
                &Hint::Number(std::f64::consts::PI)
            ),
            "180"
        );
        assert_eq!(
            hint_text(&units, Wants::Number, false, &Hint::Number(2.5)),
            "2.5"
        );
        assert_eq!(
            hint_text(
                &Units::default(),
                Wants::Point,
                false,
                &Hint::Point(Point::new(1.0, 2.0))
            ),
            "1.0000,2.0000"
        );
        assert_eq!(
            hint_text(&units, Wants::Keyword, false, &Hint::Text("Yes".into())),
            "Yes"
        );
    }

    #[test]
    fn messages_keep_two_decimals_of_an_angle() {
        let angle = 30.5_f64.to_radians();
        assert_eq!(angle_message(&Units::default(), angle), "30.50\u{b0}");
        let radians = Units {
            angle: AngleFormat::Radians,
            ..Units::default()
        };
        assert_eq!(angle_message(&radians, 1.0), "1.00r");
    }

    #[test]
    fn changing_units_is_one_undo_step_and_a_no_op_when_unchanged() {
        let mut state = AppState::new();
        let before = state.units();
        state.set_units(before);
        assert!(!state.session().history().can_undo());
        let architectural = Units {
            length: LengthFormat::Architectural,
            ..before
        };
        state.set_units(architectural);
        assert_eq!(state.units(), architectural);
        assert!(state.session().is_modified());
        state.undo();
        assert_eq!(state.units(), before);
    }
}

#[cfg(test)]
mod property_tests {
    use opendraft_core::drawing_properties::DrawingProperties;

    use crate::state::AppState;

    fn ada() -> DrawingProperties {
        DrawingProperties {
            author: "Ada".into(),
            ..DrawingProperties::default()
        }
    }

    #[test]
    fn changing_the_properties_is_one_undo_step_and_the_same_again_is_none() {
        let mut state = AppState::new();
        state.set_drawing_properties(ada());
        assert_eq!(state.drawing_properties().author, "Ada");
        let before = state.session().revision();
        state.set_drawing_properties(ada());
        assert_eq!(state.session().revision(), before, "nothing to change");
        state.undo();
        assert!(state.drawing_properties().is_empty());
    }

    #[test]
    fn fields_are_given_the_drawings_properties() {
        let mut state = AppState::new();
        state.set_drawing_properties(ada());
        assert_eq!(state.field_context().properties.author, "Ada");
    }
}
