//! The File References table: columns that sort when their title is clicked
//! and resize when their edge is dragged, as a list or as a tree.

use std::collections::HashSet;

use eframe::egui::{self, Align2, Color32, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2};
use opendraft_ui::{i18n::tr, icon_set, icons, Theme};

use super::actions::Action;
use super::rows::{date, has_children, size, Column, Row, RowKey, Sort};

const ROW: f32 = 22.0;
const HEAD: f32 = 24.0;
const LEAST_COLUMN: f32 = 40.0;
/// Room each level of the tree indents by, and the disclosure triangle takes.
const INDENT: f32 = 14.0;

/// How the table is shown; kept between frames.
#[derive(Clone)]
pub(super) struct View {
    pub tree: bool,
    pub sort: Sort,
    pub widths: [f32; 6],
    pub collapsed: HashSet<RowKey>,
    /// The File References section's share of the palette's height.
    pub list_share: f32,
}

impl Default for View {
    fn default() -> Self {
        Self {
            tree: false,
            sort: Sort::default(),
            widths: [150.0, 90.0, 70.0, 70.0, 125.0, 170.0],
            collapsed: HashSet::new(),
            list_share: 0.5,
        }
    }
}

/// What a frame of the table asked for.
#[derive(Default)]
pub(super) struct Out {
    /// A row was clicked: its position in the row list.
    pub clicked: Option<usize>,
    /// An entry of a row's menu was picked.
    pub action: Option<Action>,
}

type Menu<'a> = &'a mut dyn FnMut(&mut Ui, &Row) -> Option<Action>;

pub(super) fn show(
    ui: &mut Ui,
    theme: &Theme,
    rows: &[Row],
    order: &[usize],
    view: &mut View,
    selected: Option<RowKey>,
    menu: Menu<'_>,
) -> Out {
    let mut out = Out::default();
    let room = ui.available_width();
    egui::ScrollArea::horizontal()
        .id_salt("xref-table-across")
        .auto_shrink([false, false])
        .show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            // The last column takes what the others leave.
            let total: f32 = view.widths.iter().sum();
            let width = total.max(room);
            let mut widths = view.widths;
            widths[5] += width - total;
            header(ui, theme, view, &widths, width);
            egui::ScrollArea::vertical()
                .id_salt("xref-table-down")
                .auto_shrink([false, false])
                .show_rows(ui, ROW, order.len(), |ui, range| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    for &i in &order[range] {
                        let lit = selected == Some(rows[i].key);
                        row(
                            ui, theme, rows, i, lit, view, &widths, width, &mut out, &mut *menu,
                        );
                    }
                });
        });
    out
}

fn header(ui: &mut Ui, theme: &Theme, view: &mut View, widths: &[f32; 6], width: f32) {
    let c = &theme.colors;
    let (rect, _) = ui.allocate_exact_size(egui::vec2(width, HEAD), Sense::hover());
    ui.painter().rect_filled(rect, 0.0, c.tab_strip());
    let mut x = rect.left();
    for (n, column) in Column::ALL.into_iter().enumerate() {
        let cell = Rect::from_min_size(Pos2::new(x, rect.top()), egui::vec2(widths[n], HEAD));
        let click = ui.interact(cell, ui.id().with(("xref-col", n)), Sense::click());
        if click.hovered() {
            ui.painter().rect_filled(cell, 0.0, c.hover);
        }
        if click.clicked() {
            view.sort = view.sort.clicked(column);
        }
        let painter = ui.painter().with_clip_rect(cell);
        painter.text(
            cell.left_center() + egui::vec2(6.0, 0.0),
            Align2::LEFT_CENTER,
            tr(column.title()),
            FontId::proportional(theme.font_size_body),
            c.text,
        );
        if view.sort.column == column {
            arrow(
                &painter,
                cell.right_center() - egui::vec2(12.0, 0.0),
                view.sort.ascending,
                c.text_weak,
            );
        }
        let edge = Rect::from_min_size(
            Pos2::new(cell.right() - 3.0, rect.top()),
            egui::vec2(6.0, HEAD),
        );
        let drag = ui
            .interact(edge, ui.id().with(("xref-edge", n)), Sense::drag())
            .on_hover_cursor(egui::CursorIcon::ResizeHorizontal);
        if drag.dragged() {
            view.widths[n] = (view.widths[n] + drag.drag_delta().x).max(LEAST_COLUMN);
        }
        ui.painter().line_segment(
            [
                cell.right_top() + egui::vec2(0.0, 5.0),
                cell.right_bottom() - egui::vec2(0.0, 5.0),
            ],
            Stroke::new(1.0_f32, c.border_strong),
        );
        x += widths[n];
    }
    ui.painter().line_segment(
        [rect.left_bottom(), rect.right_bottom()],
        Stroke::new(1.0_f32, c.border),
    );
}

/// A small triangle: pointing up for ascending.
fn arrow(painter: &egui::Painter, at: Pos2, ascending: bool, colour: Color32) {
    let (w, h) = (4.0, 3.0);
    let tip = if ascending { -h } else { h };
    painter.add(egui::Shape::convex_polygon(
        vec![
            at + egui::vec2(0.0, tip),
            at + egui::vec2(-w, -tip),
            at + egui::vec2(w, -tip),
        ],
        colour,
        Stroke::NONE,
    ));
}

#[allow(clippy::too_many_arguments)]
fn row(
    ui: &mut Ui,
    theme: &Theme,
    rows: &[Row],
    i: usize,
    selected: bool,
    view: &mut View,
    widths: &[f32; 6],
    width: f32,
    out: &mut Out,
    menu: Menu<'_>,
) {
    let c = &theme.colors;
    let r = &rows[i];
    let (rect, response) = ui.allocate_exact_size(egui::vec2(width, ROW), Sense::click());
    if selected {
        ui.painter().rect_filled(rect, 0.0, c.accent);
    } else if response.hovered() {
        ui.painter().rect_filled(rect, 0.0, c.hover);
    }
    let ink = if selected { c.text_on_accent } else { c.text };
    let weak = if selected {
        c.text_on_accent
    } else {
        c.text_weak
    };
    let font = FontId::proportional(theme.font_size_body);

    let mut x = rect.left();
    let mut toggle = None;
    for (n, column) in Column::ALL.into_iter().enumerate() {
        let cell = Rect::from_min_size(Pos2::new(x, rect.top()), egui::vec2(widths[n], ROW));
        x += widths[n];
        let painter = ui.painter().with_clip_rect(cell);
        let at = |dx: f32| cell.left_center() + egui::vec2(dx, 0.0);
        let text = |dx: f32, s: String, colour: Color32| {
            painter.text(at(dx), Align2::LEFT_CENTER, s, font.clone(), colour);
        };
        match column {
            Column::Name => {
                let mut dx = 6.0;
                if view.tree {
                    dx += r.depth as f32 * INDENT;
                    if has_children(rows, i) {
                        let open = !view.collapsed.contains(&r.key);
                        let centre = at(dx + 5.0);
                        triangle(&painter, centre, open, weak);
                        toggle = Some(Rect::from_center_size(centre, egui::vec2(14.0, ROW)));
                    }
                    dx += INDENT;
                }
                let icon = if r.health.is_problem() {
                    icons::XREF_PANEL_FILE_MISSING
                } else {
                    icons::XREF_PANEL_FILE
                };
                icon_set::paint(
                    &painter,
                    at(dx + 7.0),
                    Align2::CENTER_CENTER,
                    icon,
                    14.0,
                    ink,
                );
                text(dx + 20.0, r.name.clone(), ink);
            }
            Column::Status => text(6.0, tr(r.health.label()).into_owned(), ink),
            Column::Size => text(6.0, r.size.map_or_else(String::new, size), weak),
            Column::Type => text(6.0, tr(r.kind.label()).into_owned(), ink),
            Column::Date => text(6.0, r.date.map_or_else(String::new, date), weak),
            Column::Path => text(6.0, r.path.clone(), weak),
        }
    }

    let on_toggle = response
        .interact_pointer_pos()
        .zip(toggle)
        .is_some_and(|(p, t)| t.contains(p));
    if response.clicked() {
        if on_toggle {
            if !view.collapsed.remove(&r.key) {
                view.collapsed.insert(r.key);
            }
        } else {
            out.clicked = Some(i);
        }
    }
    if response.secondary_clicked() {
        out.clicked = Some(i);
    }
    response.context_menu(|ui| {
        if let Some(action) = menu(ui, r) {
            out.action = Some(action);
            ui.close_menu();
        }
    });
}

/// A disclosure triangle: pointing down when `open`, else right.
fn triangle(painter: &egui::Painter, at: Pos2, open: bool, colour: Color32) {
    let points = if open {
        [(-4.0, -2.0), (4.0, -2.0), (0.0, 3.0)]
    } else {
        [(-2.0, -4.0), (-2.0, 4.0), (3.0, 0.0)]
    };
    painter.add(egui::Shape::convex_polygon(
        points
            .iter()
            .map(|(x, y)| at + egui::vec2(*x, *y))
            .collect(),
        colour,
        Stroke::NONE,
    ));
}
