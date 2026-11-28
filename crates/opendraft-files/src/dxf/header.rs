//! What a DXF header sets on the drawing when it is read.

use opendraft_core::Document;

/// Apply the header's groups (code, value) to `document`: its units, linetype
/// scale, point style and the Model tab's coordinate system. Returns whether
/// that system was 3D or mirrored and so left as World.
pub(super) fn apply<'a>(
    groups: impl Iterator<Item = (i32, &'a str)> + Clone,
    document: &mut Document,
) -> bool {
    document.set_units(super::units::read_header(groups.clone()));
    super::linetypes::apply_header(groups.clone(), document);
    super::point_style::apply_header(groups.clone(), document);
    super::modify_settings::apply_header(groups.clone(), document);
    let for_xrefs = groups.clone();
    let tilted = super::ccs::apply_header(groups, document);
    apply_xrefs(for_xrefs, document);
    tilted
}

/// `$INSBASE` and `$VISRETAIN`, which an external-reference snapshot needs.
fn apply_xrefs<'a>(groups: impl Iterator<Item = (i32, &'a str)>, document: &mut Document) {
    let mut name = "";
    let mut base = opendraft_core::Point::default();
    let mut visretain = None;
    for (code, value) in groups {
        if code == 9 {
            name = value;
            continue;
        }
        match (name, code) {
            ("$INSBASE", 10) => base.x = value.trim().parse().unwrap_or(0.0),
            ("$INSBASE", 20) => base.y = value.trim().parse().unwrap_or(0.0),
            ("$VISRETAIN", 70) => visretain = value.trim().parse::<i32>().ok(),
            _ => {}
        }
    }
    if base.x.is_finite() && base.y.is_finite() {
        document.set_insert_base(base);
    }
    if let Some(keep) = visretain {
        document.set_visretain(keep != 0);
    }
}
