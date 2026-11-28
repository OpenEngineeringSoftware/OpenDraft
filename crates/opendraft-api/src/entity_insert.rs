//! A block reference's wire form.

use opendraft_core::Insert;

use crate::entity::{bounded, num, AttributeValueDto, GeometryDto};
use crate::limits::POINTS_PER_ENTITY_MAX;

/// The insert's fields, its attributes capped and bounded like other text.
pub(crate) fn geometry(i: &Insert) -> GeometryDto {
    GeometryDto::Insert {
        block_id: i.block.id.0.to_string(),
        block_name: bounded(&i.block.name).0,
        insertion: i.insertion.into(),
        scale_x: num(i.scale_x),
        scale_y: num(i.scale_y),
        rotation: num(i.rotation),
        attributes: i
            .attributes
            .iter()
            .take(POINTS_PER_ENTITY_MAX)
            .map(|r| AttributeValueDto {
                tag: bounded(&r.tag).0,
                value: bounded(&r.value.display()).0,
                invisible: r.flags.invisible(),
                field: r.value.has_fields(),
            })
            .collect(),
        xref: i.is_xref,
    }
}
