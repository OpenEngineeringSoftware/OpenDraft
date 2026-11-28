//! The External References palette: what each reference is, where it came
//! from, whether the source is still there, and one action per operation.
//!
//! Status is derived from the record, the inserts and the source file's
//! metadata; it is cached per drawing revision so a frame does not stat every
//! file. Any edit or the Refresh button makes a new cache.

use std::path::Path;
use std::time::UNIX_EPOCH;

use eframe::egui;
use opendraft_core::{BlockId, Document, XrefDef, XrefKind};
use opendraft_files::resolve;
use opendraft_ui::key;

use super::PanelViewer;
use crate::state::AppState;
use crate::xref_cmd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Health {
    Loaded,
    Unloaded,
    Unreferenced,
    NotFound,
    ShowingSaved,
    Changed,
    Unresolved,
}

impl Health {
    fn label(self) -> &'static str {
        match self {
            Self::Loaded => key!("Loaded"),
            Self::Unloaded => key!("Unloaded"),
            Self::Unreferenced => key!("Unreferenced"),
            Self::NotFound => key!("Not Found"),
            Self::ShowingSaved => key!("Not Found — showing saved copy"),
            Self::Changed => key!("Changed on disk"),
            Self::Unresolved => key!("Unresolved"),
        }
    }
}

#[derive(Clone, Debug)]
struct Row {
    block: BlockId,
    name: String,
    health: Health,
    size: Option<u64>,
    kind: XrefKind,
    date: Option<i64>,
    path: String,
}

#[derive(Clone, Default)]
struct Cache {
    uid: u64,
    revision: u64,
    rows: Vec<Row>,
}

impl PanelViewer<'_> {
    pub(super) fn xrefs(&mut self, ui: &mut egui::Ui) {
        let uid = self.state.docs[self.state.active].uid;
        let revision = self.state.docs[self.state.active].session.revision();
        let id = egui::Id::new(("xref-palette", uid));
        let cached = ui
            .data(|data| data.get_temp::<Cache>(id))
            .filter(|cache| cache.uid == uid && cache.revision == revision);
        let cache = match cached {
            Some(cache) => cache,
            None => {
                let built = {
                    let doc = &self.state.docs[self.state.active];
                    let host_dir = doc.path.as_deref().and_then(Path::parent);
                    Cache {
                        uid,
                        revision,
                        rows: rows(doc.session.document(), host_dir),
                    }
                };
                ui.data_mut(|data| data.insert_temp(id, built.clone()));
                built
            }
        };
        let selected_id = egui::Id::new(("xref-selected", uid));
        let mut selected: Option<BlockId> = ui
            .data(|data| data.get_temp::<Option<BlockId>>(selected_id))
            .flatten();
        if selected.is_some_and(|block| !cache.rows.iter().any(|r| r.block == block)) {
            selected = None;
        }

        ui.horizontal_wrapped(|ui| {
            if ui.button(key!("Attach")).clicked() {
                self.state
                    .start_command("XATTACH", |ed| xref_cmd::attach(ed, XrefKind::Attach));
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new(key!("Reload")))
                .clicked()
            {
                if let Some(block) = selected {
                    self.state
                        .start_command("XREFRELOAD", move |ed| xref_cmd::reload_block(ed, block));
                }
            }
            if ui.button(key!("Reload all")).clicked() {
                self.state
                    .start_command("XREFRELOADALL", xref_cmd::reload_all);
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new(key!("Unload")))
                .clicked()
            {
                if let Some(block) = selected {
                    self.state
                        .start_command("XREFUNLOAD", move |ed| xref_cmd::unload_block(ed, block));
                }
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new(key!("Detach")))
                .clicked()
            {
                if let Some(block) = selected {
                    self.state
                        .start_command("XREFDETACH", move |ed| xref_cmd::detach_block(ed, block));
                }
            }
            if ui
                .add_enabled(selected.is_some(), egui::Button::new(key!("Path")))
                .clicked()
            {
                if let Some(block) = selected {
                    self.state
                        .start_command("XREFPATH", move |ed| xref_cmd::edit_path_block(ed, block));
                }
            }
            ui.add_enabled(false, egui::Button::new(key!("Bind")))
                .on_disabled_hover_text(key!("Binding external references is not built yet."));
            if ui.button(key!("Refresh")).clicked() {
                ui.data_mut(|data| data.remove::<Cache>(id));
                ui.ctx().request_repaint();
            }
        });
        ui.separator();

        if cache.rows.is_empty() {
            ui.label(key!("There are no external references in this drawing."));
            return;
        }
        let mut clicked = None;
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new(("xref-grid", uid))
                    .striped(true)
                    .num_columns(6)
                    .show(ui, |ui| {
                        for title in [
                            key!("Reference Name"),
                            key!("Status"),
                            key!("Size"),
                            key!("Type"),
                            key!("Date"),
                            key!("Saved Path"),
                        ] {
                            ui.strong(title);
                        }
                        ui.end_row();
                        for row in &cache.rows {
                            let is_selected = selected == Some(row.block);
                            if ui.selectable_label(is_selected, &row.name).clicked() {
                                selected = Some(row.block);
                                clicked = Some(row.block);
                            }
                            ui.label(row.health.label());
                            ui.label(row.size.map_or_else(String::new, size));
                            ui.label(row.kind.label());
                            ui.label(row.date.map_or_else(String::new, date));
                            ui.add(egui::Label::new(row.path.as_str()).truncate());
                            ui.end_row();
                        }
                    });
            });
        if let Some(block) = clicked {
            let ids = self.state.docs[self.state.active]
                .session
                .document()
                .xref_references(block);
            self.state.select_ids(ids, false);
        }
        ui.data_mut(|data| data.insert_temp(selected_id, selected));
    }
}

/// The rows for the drawing, reading each source's metadata once.
fn rows(doc: &Document, host_dir: Option<&Path>) -> Vec<Row> {
    let mut out = Vec::new();
    for xref in doc.xrefs() {
        let Some(root) = doc.block(xref.block) else {
            continue;
        };
        let resolved = if xref.path.trim().is_empty() {
            None
        } else {
            resolve(&xref.path, host_dir).ok()
        };
        let meta = resolved
            .as_ref()
            .and_then(|path| std::fs::metadata(path).ok());
        let referenced = !doc.xref_references(xref.block).is_empty();
        let health = health(
            xref,
            root.entities.is_empty(),
            referenced,
            resolved.is_some(),
            meta.as_ref(),
        );
        let (size, date) = match &meta {
            Some(meta) => (Some(meta.len()), modified(meta)),
            None => (
                xref.stamp.map(|stamp| stamp.length),
                xref.stamp.map(|stamp| stamp.modified).filter(|s| *s > 0),
            ),
        };
        out.push(Row {
            block: xref.block,
            name: root.name.clone(),
            health,
            size,
            kind: xref.kind,
            date,
            path: xref.path.clone(),
        });
    }
    out
}

fn health(
    xref: &XrefDef,
    root_empty: bool,
    referenced: bool,
    resolved: bool,
    meta: Option<&std::fs::Metadata>,
) -> Health {
    if !referenced {
        return Health::Unreferenced;
    }
    if xref.unloaded {
        return Health::Unloaded;
    }
    if !resolved {
        return Health::Unresolved;
    }
    let Some(meta) = meta else {
        return if xref.stamp.is_some() && !root_empty {
            Health::ShowingSaved
        } else {
            Health::NotFound
        };
    };
    match xref.stamp {
        Some(stamp)
            if stamp.length != meta.len()
                || (stamp.modified > 0
                    && modified(meta).is_some_and(|now| now != stamp.modified)) =>
        {
            Health::Changed
        }
        _ => Health::Loaded,
    }
}

fn modified(meta: &std::fs::Metadata) -> Option<i64> {
    meta.modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|since| since.as_secs() as i64)
}

fn size(bytes: u64) -> String {
    const K: f64 = 1024.0;
    let value = bytes as f64;
    if value < K {
        format!("{bytes} B")
    } else if value < K * K {
        format!("{:.1} KB", value / K)
    } else {
        format!("{:.1} MB", value / (K * K))
    }
}

/// A date and time from Unix seconds, in UTC.
fn date(seconds: i64) -> String {
    let days = seconds.div_euclid(86_400);
    let rest = seconds.rem_euclid(86_400);
    let (year, month, day) = civil(days);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}",
        rest / 3600,
        rest % 3600 / 60
    )
}

/// The civil date of a count of days from 1970-01-01 (Howard Hinnant's
/// algorithm, as `chrono` and the C++ standard library use).
fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (if month <= 2 { year + 1 } else { year }, month, day)
}

impl AppState {
    /// Show the palette (the `xref.palette` command).
    pub(crate) fn show_xrefs_panel(&mut self) {
        self.layout_request = Some(crate::dock::LayoutRequest::Toggle(
            crate::dock::PanelTab::Xrefs,
        ));
    }
}

#[cfg(test)]
mod tests {
    use opendraft_core::{XrefDef, XrefPathType, XrefReport, XrefStamp};

    use super::*;

    fn record(stamp: Option<XrefStamp>) -> XrefDef {
        XrefDef {
            block: BlockId(0),
            path: "base.dwg".to_owned(),
            path_type: XrefPathType::Relative,
            kind: XrefKind::Attach,
            unloaded: false,
            stamp,
            layers: Vec::new(),
            blocks: Vec::new(),
            report: XrefReport::default(),
        }
    }

    #[test]
    fn dates_are_read_in_utc() {
        assert_eq!(date(0), "1970-01-01 00:00");
        assert_eq!(date(1_700_000_000), "2023-11-14 22:13");
    }

    #[test]
    fn sizes_use_the_unit_that_fits() {
        assert_eq!(size(3), "3 B");
        assert_eq!(size(2048), "2.0 KB");
        assert_eq!(size(3 * 1024 * 1024), "3.0 MB");
    }

    #[test]
    fn health_follows_the_record_the_inserts_and_the_source() {
        let dir = std::env::temp_dir().join(format!("od-xref-panel-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("base.dwg");
        std::fs::write(&file, b"abc").unwrap();
        let meta = std::fs::metadata(&file).unwrap();
        let when = modified(&meta).unwrap_or(0);

        let fresh = record(Some(XrefStamp {
            length: 3,
            modified: when,
        }));
        assert_eq!(
            health(&fresh, false, true, true, Some(&meta)),
            Health::Loaded
        );
        assert_eq!(health(&fresh, true, true, false, None), Health::Unresolved);
        assert_eq!(
            health(&fresh, false, true, true, None),
            Health::ShowingSaved
        );
        assert_eq!(health(&fresh, true, true, true, None), Health::NotFound);
        assert_eq!(
            health(&fresh, false, false, true, Some(&meta)),
            Health::Unreferenced
        );

        let mut unloaded = fresh.clone();
        unloaded.unloaded = true;
        assert_eq!(health(&unloaded, true, true, true, None), Health::Unloaded);

        let changed = record(Some(XrefStamp {
            length: 4,
            modified: when,
        }));
        assert_eq!(
            health(&changed, false, true, true, Some(&meta)),
            Health::Changed
        );
        assert_eq!(modified(&meta), Some(when));
        let _ = std::fs::remove_dir_all(dir);
    }
}
