//! The unit settings of a DWG header.

use opendraft_core::{AngleFormat, InsertUnits, LengthFormat, Units};
use opendraft_dwg::DwgHeader;

/// The drawing's units, sanitised so a damaged header still gives a usable
/// set.
pub(super) fn units(header: &DwgHeader) -> Units {
    let defaults = Units::default();
    let precision = |v: u16| u8::try_from(v.min(8)).unwrap_or(8);
    Units {
        length: LengthFormat::from_code(i64::from(header.linear_units)).unwrap_or(defaults.length),
        length_precision: precision(header.linear_precision),
        angle: AngleFormat::from_code(i64::from(header.angular_units)).unwrap_or(defaults.angle),
        angle_precision: precision(header.angular_precision),
        clockwise: header.angles_clockwise,
        base_angle: if header.angle_base.is_finite() {
            header.angle_base
        } else {
            0.0
        },
        insert: InsertUnits::from_code(i64::from(header.insert_units))
            .unwrap_or(InsertUnits::UNITLESS),
    }
    .sanitized()
}
