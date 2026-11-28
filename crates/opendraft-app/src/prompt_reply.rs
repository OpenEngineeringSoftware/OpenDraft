//! Reading what the user gave a prompt: a clicked point, typed text, a
//! keyword. Pure functions over the prompt (`opendraft_commands::Prompt`), so they
//! are tested without a running command.

use opendraft_commands::{Prompt as Ask, Reply, Wants};
use opendraft_core::units::parse_length;
use opendraft_core::{Ccs, Point};

use crate::cmdline::parse_number;
use crate::coordinates::{parse_point_with_mode, CoordinateMode, Reading};

/// Whether the prompt is answered by clicking a point (so snaps apply).
pub(crate) fn takes_clicked_points(ask: &Ask) -> bool {
    match ask.wants {
        Wants::Point | Wants::AngleOrPoint | Wants::DistanceOrPoint => true,
        Wants::Distance | Wants::Angle => ask.base.is_some(),
        _ => false,
    }
}

/// The reply a placed point gives: itself, or its distance or direction from
/// the prompt's base point. An amount to turn by is measured from the X axis
/// of the coordinate system `ccs`.
pub(crate) fn reply_for_point(ask: &Ask, ccs: Ccs, at: Point) -> Option<Reply> {
    match (ask.wants, ask.base) {
        (Wants::Point | Wants::AngleOrPoint | Wants::DistanceOrPoint, _) => Some(Reply::Point(at)),
        (Wants::Distance, Some(b)) => Some(Reply::Number((at.x - b.x).hypot(at.y - b.y))),
        (Wants::Angle, Some(b)) => {
            let direction = (at.y - b.y).atan2(at.x - b.x);
            let turn = if ask.turn { ccs.angle } else { 0.0 };
            Some(Reply::Number(direction - turn))
        }
        _ => None,
    }
}

/// Read typed text as an answer to `ask`. `origin` is where `@` measures
/// from; `cursor` gives the direction for a bare distance. `None` means the
/// text is not an answer; `Some(Err(message))` is an answer that cannot be
/// used yet.
pub(crate) fn typed_reply(
    ask: &Ask,
    text: &str,
    origin: Point,
    cursor: Option<Point>,
    reading: impl Into<Reading>,
) -> Option<Result<Reply, &'static str>> {
    typed_reply_with_mode(
        ask,
        text,
        origin,
        cursor,
        CoordinateMode::CommandLine,
        reading,
    )
}

pub(crate) fn typed_reply_with_mode(
    ask: &Ask,
    text: &str,
    origin: Point,
    cursor: Option<Point>,
    mode: CoordinateMode,
    reading: impl Into<Reading>,
) -> Option<Result<Reply, &'static str>> {
    let reading = reading.into();
    let text = text.trim();
    let length = parse_length(text);
    let point = parse_point_with_mode(text, origin, mode, reading);
    let by_point = |p: Point| reply_for_point(ask, reading.ccs, p).map(Ok);
    let angle = |text: &str| {
        if ask.turn {
            reading.turn(text)
        } else {
            reading.direction(text)
        }
    };
    let answer = match ask.wants {
        Wants::Point => match (point, length, ask.base) {
            (Some(p), _, _) => Some(Ok(Reply::Point(p))),
            // A bare distance goes along the direction the cursor points.
            (None, Some(n), Some(from)) => Some(match cursor {
                Some(c) if c != from => {
                    let (dx, dy) = (c.x - from.x, c.y - from.y);
                    let len = dx.hypot(dy);
                    Ok(Reply::Point(Point::new(
                        from.x + dx / len * n,
                        from.y + dy / len * n,
                    )))
                }
                _ => Err("Move the cursor to set a direction first."),
            }),
            _ => None,
        },
        Wants::Distance => match (length, point) {
            (Some(n), _) => Some(Ok(Reply::Number(n))),
            (None, Some(p)) => by_point(p),
            _ => None,
        },
        Wants::Number => match (parse_number(text), point) {
            (Some(n), _) => Some(Ok(Reply::Number(n))),
            (None, Some(p)) => by_point(p),
            _ => None,
        },
        Wants::Angle => match (angle(text), point) {
            (Some(angle), _) => Some(Ok(Reply::Number(angle))),
            (None, Some(p)) => by_point(p),
            _ => None,
        },
        Wants::AngleOrPoint => match (angle(text), point) {
            (Some(angle), _) => Some(Ok(Reply::Number(angle))),
            (None, Some(p)) => Some(Ok(Reply::Point(p))),
            _ => None,
        },
        Wants::DistanceOrPoint => match (length, point) {
            (Some(n), _) => Some(Ok(Reply::Number(n))),
            (None, Some(p)) => Some(Ok(Reply::Point(p))),
            _ => None,
        },
        Wants::Text => Some(Ok(Reply::Text(text.to_owned()))),
        Wants::Keyword
        | Wants::Selection
        | Wants::Object
        | Wants::MTextEditor
        | Wants::OverkillDialog
        | Wants::AttributeDefinitionDialog
        | Wants::AttributeEditorDialog
        | Wants::BlockDialog
        | Wants::FieldDialog
        | Wants::XrefLoad
        | Wants::XrefAttachDialog => None,
    };
    answer.or_else(|| keyword_reply(ask, text))
}

/// The keyword `text` picks, by its key or the start of its label, in the
/// language of the interface, or in English after `_`.
pub(crate) fn keyword_reply(ask: &Ask, text: &str) -> Option<Result<Reply, &'static str>> {
    let options = crate::option_language::options(&ask.keywords);
    crate::option_language::pick(&options, text).map(|o| Ok(Reply::Keyword(o.value.to_string())))
}
