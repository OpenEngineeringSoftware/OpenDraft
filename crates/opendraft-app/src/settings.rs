//! User settings, persisted between runs through eframe's storage as one
//! JSON value. A new setting is a field here (with its default and, if it has
//! a range, a line in `sanitized`) and a control in `settings_ui.rs`; nothing
//! to add for saving. Fields missing from a save take their default.

use opendraft_canvas::{Ortho, SelectionStyle, SnapModes, Tracking};
use opendraft_ui::Palette;
use serde::{Deserialize, Serialize};

/// The storage key holding all settings as one JSON value.
const KEY: &str = "settings";

/// Interface scale choices, in percent.
pub const UI_SCALES: [u32; 4] = [100, 110, 125, 150];
/// Autosave intervals offered, in minutes (0 is off).
pub const AUTOSAVE_CHOICES: [u32; 5] = [0, 1, 2, 5, 10];
pub const GRID_SPACING_RANGE: (f64, f64) = (1.0, 1000.0);
/// Allowed sizes, in screen points, of the pick, snap and grip targets.
pub const PICK_SIZE_RANGE: (u32, u32) = (1, 50);
pub const SNAP_APERTURE_RANGE: (u32, u32) = (1, 50);
pub const GRIP_SIZE_RANGE: (u32, u32) = (1, 20);
/// Allowed crosshair length, as a percent of the drawing window.
pub const CROSSHAIR_SIZE_RANGE: (u32, u32) = (1, 100);
/// Allowed mouse wheel zoom speed, in percent of the normal rate.
pub const ZOOM_SPEED_RANGE: (u32, u32) = (25, 400);
/// Every moment `FIELDEVAL` can name: on open, on save, on plot.
pub const FIELD_EVAL_ALL: u8 = 7;

/// How a layout's sheet is shown: white paper with dark lines, as it will be
/// printed, or the dark look of model space.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LayoutPaper {
    #[default]
    Light,
    Dark,
}

/// The saved-path form a new external reference starts with (`REFPATHTYPE`),
/// numbered as AutoCAD does: 0 no path, 1 relative, 2 full.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RefPathDefault {
    NoPath,
    #[default]
    Relative,
    Full,
}

impl RefPathDefault {
    pub fn code(self) -> i64 {
        match self {
            Self::NoPath => 0,
            Self::Relative => 1,
            Self::Full => 2,
        }
    }

    pub fn from_code(code: i64) -> Self {
        match code {
            0 => Self::NoPath,
            2 => Self::Full,
            _ => Self::Relative,
        }
    }

    pub fn to_core(self) -> opendraft_core::XrefPathType {
        match self {
            Self::NoPath => opendraft_core::XrefPathType::NoPath,
            Self::Relative => opendraft_core::XrefPathType::Relative,
            Self::Full => opendraft_core::XrefPathType::Full,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub dynamic_input: crate::dynamic_settings::DynamicInputSettings,
    pub palette: Palette,
    /// The paper of layouts; model space keeps the palette's background.
    pub layout_paper: LayoutPaper,
    /// The colour of light paper (white unless chosen).
    pub layout_paper_colour: crate::paper_colour::PaperColour,
    pub show_grid: bool,
    /// The navigation gizmo in the top right corner of the drawing.
    pub show_nav_gizmo: bool,
    /// Dialogs and floating panels open as their own operating system
    /// windows, which can be moved to another monitor, instead of inside the
    /// main window. On unless turned off.
    pub popout_windows: bool,
    /// The `[-][Top][2D Wireframe]` label in the drawing's top-left corner
    /// (`VPCONTROL`).
    pub show_viewport_controls: bool,
    /// Grid spacing in drawing units.
    pub grid_spacing: f64,
    /// Interface scale in percent; one of [`UI_SCALES`].
    pub ui_scale_percent: u32,
    pub snap: SnapModes,
    /// Ortho mode (`ORTHOMODE`) and whether it overrides object snaps.
    pub ortho: Ortho,
    /// Object snap tracking (F11) and polar tracking (F10).
    pub tracking: Tracking,
    /// What a click on empty space and a drag do: clear the selection and
    /// make a box (the default), or start a window and draw a lasso.
    pub selection_style: SelectionStyle,
    /// Radius, in screen points, within which a click picks an object
    /// (`PICKBOX`).
    pub pick_size: u32,
    /// Radius within which the cursor snaps to geometry (`APERTURE`).
    pub snap_aperture: u32,
    /// Side of a grip square and reach of a press on it (`GRIPSIZE`).
    pub grip_size: u32,
    /// Crosshair length as a percent of the drawing window (`CURSORSIZE`).
    pub crosshair_size: u32,
    /// Crosshair colour (red, green, blue); off-white if none.
    pub crosshair_color: Option<[u8; 3]>,
    /// Mouse wheel zoom speed in percent of the normal rate.
    pub zoom_speed: u32,
    /// Minutes between recovery copies of unsaved drawings; 0 is off.
    pub autosave_minutes: u32,
    /// When data fields are brought up to date (`FIELDEVAL`): 1 on open,
    /// 2 on save, 4 on plot.
    pub field_eval: u8,
    /// The Plot dialog's last output device by name (OpenDraft PDF, a printer...),
    /// and the resolution of a sheet printed as a picture.
    pub plot_device: String,
    pub plot_dpi: u32,
    /// Open a PDF or SVG plot in the system's viewer once it is written
    /// (the Plot dialog's "Open in viewer when done").
    pub plot_view_when_done: bool,
    /// The margin on every side of a PDF or SVG plot of a new drawing, or of
    /// a drawing whose printer is not on this computer (the Files page).
    pub plot_margins_mm: f64,
    /// The experimental MCP automation endpoint is on. Off by default.
    pub automation_enabled: bool,
    /// Folders automation may open and write files in. Nothing outside
    /// them is reachable, and there are none until the user adds one.
    pub automation_roots: Vec<std::path::PathBuf>,
    /// Folders chosen on the Files page; none means the default (see
    /// `locations.rs`).
    pub templates_dir: Option<std::path::PathBuf>,
    pub recovery_dir: Option<std::path::PathBuf>,
    pub library_dir: Option<std::path::PathBuf>,
    /// May the program ask GitHub, once a day, whether a newer version
    /// exists? `None` until the user has answered (see `updates`).
    pub update_checks: Option<bool>,
    /// The drawing cache and the frame-time readout (the Performance page).
    pub performance: crate::performance_settings::PerformanceSettings,
    /// The interface language (a code of `language::LANGUAGES`); `None`
    /// follows the system's.
    pub language: Option<String>,
    /// What a new external reference defaults to (`XREFTYPE`): attached, or
    /// overlaid when true.
    pub xref_overlay: bool,
    /// The saved path form a new external reference defaults to
    /// (`REFPATHTYPE`).
    pub ref_path_type: RefPathDefault,
    /// Bind names dependent symbols bare and merges layers (`BINDTYPE` 1);
    /// false prefixes them with `$0$`.
    pub bind_insert: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            dynamic_input: Default::default(),
            palette: Palette::default(),
            layout_paper: LayoutPaper::default(),
            layout_paper_colour: Default::default(),
            show_grid: true,
            show_nav_gizmo: true,
            popout_windows: true,
            show_viewport_controls: true,
            grid_spacing: 10.0,
            ui_scale_percent: 100,
            snap: SnapModes::default(),
            ortho: Ortho::default(),
            tracking: Tracking::default(),
            selection_style: SelectionStyle::default(),
            pick_size: 4,
            snap_aperture: 12,
            grip_size: 7,
            crosshair_size: 5,
            crosshair_color: None,
            zoom_speed: 100,
            autosave_minutes: 2,
            field_eval: FIELD_EVAL_ALL,
            plot_device: String::new(),
            plot_dpi: 300,
            plot_view_when_done: true,
            plot_margins_mm: 5.0,
            automation_enabled: false,
            automation_roots: Vec::new(),
            templates_dir: None,
            recovery_dir: None,
            library_dir: None,
            update_checks: None,
            performance: Default::default(),
            language: None,
            xref_overlay: false,
            ref_path_type: RefPathDefault::default(),
            bind_insert: false,
        }
    }
}

impl Settings {
    /// Turn ortho on or off. Ortho and polar tracking exclude each other, so
    /// turning it on turns polar tracking off.
    pub fn set_ortho(&mut self, on: bool) {
        self.ortho.enabled = on;
        if on {
            self.tracking.polar = false;
        }
    }

    /// Turn polar tracking on or off; on turns ortho off.
    pub fn set_polar(&mut self, on: bool) {
        self.tracking.polar = on;
        if on {
            self.ortho.enabled = false;
        }
    }

    /// Ask for a folder and approve it for automation (once). Nothing is
    /// approved if the user cancels.
    pub fn approve_folder(&mut self, picker: &mut dyn crate::picker::FilePicker) {
        if let Some(folder) = picker.folder("Approve a folder for assistants") {
            if !self.automation_roots.contains(&folder) {
                self.automation_roots.push(folder);
            }
        }
    }

    /// Ask for a folder for `location` and use it. Nothing changes if the
    /// user cancels.
    pub fn choose_folder(
        &mut self,
        location: crate::locations::Location,
        picker: &mut dyn crate::picker::FilePicker,
    ) {
        if let Some(folder) = picker.folder(location.label()) {
            location.set(self, Some(folder));
        }
    }

    /// Load saved settings; anything missing or unrecognized falls back to
    /// the default.
    pub fn load(storage: Option<&dyn eframe::Storage>) -> Self {
        let Some(storage) = storage else {
            return Self::default();
        };
        let settings = match storage.get_string(KEY) {
            Some(text) => Self::from_json(&text),
            None => Self::default(),
        };
        settings.sanitized()
    }

    /// Settings from their JSON text. Each saved field is tried on its own,
    /// so one that is bad (or from a newer version) costs only itself; fields
    /// that are absent or unknown are left at the default.
    pub fn from_json(text: &str) -> Self {
        use serde_json::Value;
        let default = Self::default();
        let Ok(Value::Object(saved)) = serde_json::from_str(text) else {
            return default;
        };
        let Ok(Value::Object(mut merged)) = serde_json::to_value(&default) else {
            return default;
        };
        for (key, value) in saved {
            let Some(old) = merged.insert(key.clone(), value) else {
                merged.remove(&key);
                continue;
            };
            if serde_json::from_value::<Self>(Value::Object(merged.clone())).is_err() {
                merged.insert(key, old);
            }
        }
        serde_json::from_value(Value::Object(merged)).unwrap_or(default)
    }

    pub fn save(&self, storage: &mut dyn eframe::Storage) {
        // A path that is not valid text cannot be written as JSON, and one
        // such folder must not cost the user every other setting.
        let mut saved = self.clone();
        saved.automation_roots.retain(|p| p.to_str().is_some());
        for folder in [
            &mut saved.templates_dir,
            &mut saved.recovery_dir,
            &mut saved.library_dir,
        ] {
            if folder.as_ref().is_some_and(|p| p.to_str().is_none()) {
                *folder = None;
            }
        }
        if let Ok(text) = serde_json::to_string(&saved) {
            storage.set_string(KEY, text);
        }
    }

    /// Force every value into its valid range (also guards against NaN).
    pub fn sanitized(mut self) -> Self {
        self.dynamic_input = self.dynamic_input.sanitized();
        self.performance = self.performance.sanitized();
        // A language this version does not offer follows the system.
        self.language = self
            .language
            .filter(|code| crate::language::is_offered(code));
        let (min, max) = GRID_SPACING_RANGE;
        self.grid_spacing = if self.grid_spacing.is_finite() {
            self.grid_spacing.clamp(min, max)
        } else {
            Self::default().grid_spacing
        };
        for (value, (min, max)) in [
            (&mut self.pick_size, PICK_SIZE_RANGE),
            (&mut self.snap_aperture, SNAP_APERTURE_RANGE),
            (&mut self.grip_size, GRIP_SIZE_RANGE),
            (&mut self.crosshair_size, CROSSHAIR_SIZE_RANGE),
            (&mut self.zoom_speed, ZOOM_SPEED_RANGE),
        ] {
            *value = (*value).clamp(min, max);
        }
        self.field_eval &= FIELD_EVAL_ALL;
        self.plot_dpi = self.plot_dpi.clamp(72, 1200);
        if !crate::print_backend::safe_name(&self.plot_device) {
            self.plot_device.clear();
        }
        // Never both: a saved file with both has ortho win.
        if self.ortho.enabled {
            self.tracking.polar = false;
        }
        let t = &mut self.tracking;
        if !(t.increment.is_finite() && (0.01..=180.0).contains(&t.increment)) {
            t.increment = Tracking::default().increment;
        }
        t.additional.retain(|a| a.is_finite());
        t.additional.truncate(10);
        t.path_display = t.path_display.min(3);
        if !AUTOSAVE_CHOICES.contains(&self.autosave_minutes) {
            self.autosave_minutes = Self::default().autosave_minutes;
        }
        for folder in [
            &mut self.templates_dir,
            &mut self.recovery_dir,
            &mut self.library_dir,
        ] {
            if folder.as_ref().is_some_and(|p| p.as_os_str().is_empty()) {
                *folder = None;
            }
        }
        if !UI_SCALES.contains(&self.ui_scale_percent) {
            self.ui_scale_percent = Self::default().ui_scale_percent;
        }
        self
    }

    pub fn pointer_sizes(&self) -> opendraft_canvas::PointerSizes {
        opendraft_canvas::PointerSizes {
            pick: f64::from(self.pick_size),
            snap: f64::from(self.snap_aperture),
            grip: f64::from(self.grip_size),
        }
    }

    pub fn crosshair(&self) -> opendraft_canvas::Crosshair {
        opendraft_canvas::Crosshair {
            size: self.crosshair_size,
            color: self
                .crosshair_color
                .map(|[r, g, b]| eframe::egui::Color32::from_rgb(r, g, b)),
        }
    }

    pub fn zoom_factor(&self) -> f32 {
        self.ui_scale_percent as f32 / 100.0
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use eframe::Storage;

    use super::*;

    #[derive(Default)]
    struct MemoryStorage(HashMap<String, String>);

    impl eframe::Storage for MemoryStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }

        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.to_owned(), value);
        }

        fn flush(&mut self) {}
    }

    #[test]
    fn defaults_when_nothing_is_saved() {
        assert_eq!(Settings::load(None), Settings::default());
        assert_eq!(
            Settings::load(Some(&MemoryStorage::default())),
            Settings::default()
        );
    }

    #[test]
    fn save_then_load_round_trips() {
        let mut storage = MemoryStorage::default();
        let settings = Settings {
            dynamic_input: crate::dynamic_settings::DynamicInputSettings {
                dynmode: -2,
                pointer_absolute: true,
                dynamic_prompts: false,
                pointer_cartesian: true,
                pointer_visibility: 2,
            },
            palette: Palette::Slate,
            layout_paper: LayoutPaper::Dark,
            layout_paper_colour: crate::paper_colour::PaperColour::Cream,
            show_grid: false,
            show_nav_gizmo: false,
            popout_windows: false,
            show_viewport_controls: false,
            grid_spacing: 2.5,
            ui_scale_percent: 125,
            snap: SnapModes {
                enabled: false,
                endpoint: false,
                midpoint: true,
                center: false,
                quadrant: true,
                nearest: false,
                grid: true,
                tangent: true,
                tooltip: false,
                ..SnapModes::default()
            },
            ortho: Ortho {
                enabled: true,
                strict: true,
            },
            tracking: Tracking {
                otrack: false,
                polar: false,
                increment: 22.5,
                additional: vec![15.0, 37.5],
                use_additional: true,
                all_polar: true,
                shift_to_acquire: true,
                path_display: 2,
                tooltips: false,
            },
            selection_style: SelectionStyle::WindowLasso,
            pick_size: 9,
            snap_aperture: 20,
            grip_size: 12,
            crosshair_size: 40,
            crosshair_color: Some([10, 200, 30]),
            zoom_speed: 250,
            autosave_minutes: 10,
            field_eval: 2,
            plot_device: "Office".into(),
            plot_dpi: 600,
            plot_view_when_done: false,
            plot_margins_mm: 2.5,
            automation_enabled: true,
            automation_roots: vec!["/tmp/a".into(), "C:\\Plans".into()],
            templates_dir: Some("D:\\Templates".into()),
            recovery_dir: Some("D:\\Recovery".into()),
            library_dir: Some("D:\\Library".into()),
            update_checks: Some(true),
            performance: crate::performance_settings::PerformanceSettings {
                drawing_cache: opendraft_canvas::CacheMode::Off,
                memory_limit_mb: 1024,
                while_moving: opendraft_canvas::Motion::Exact,
                settle_delay_ms: 250,
                draw_as: opendraft_canvas::Render::Triangles,
                show_frame_time: true,
            },
            language: Some("fr".into()),
            xref_overlay: true,
            ref_path_type: RefPathDefault::Full,
            bind_insert: true,
        };
        settings.save(&mut storage);
        assert_eq!(Settings::load(Some(&storage)), settings);
        assert!(storage.get_string(KEY).is_some());
    }

    #[test]
    fn a_bad_field_costs_only_itself() {
        let text = r#"{"palette":"hot-pink","show_grid":"maybe","grid_spacing":4.0,
            "ui_scale_percent":9000,"autosave_minutes":7,"snap":{"endpoint":"perhaps"},
            "automation_enabled":true,"from_the_future":[1,2]}"#;
        let loaded = Settings::from_json(text).sanitized();
        let default = Settings::default();
        assert_eq!(loaded.palette, default.palette);
        assert_eq!(loaded.show_grid, default.show_grid);
        assert_eq!(loaded.snap, default.snap);
        assert_eq!(loaded.ui_scale_percent, default.ui_scale_percent);
        assert_eq!(loaded.autosave_minutes, default.autosave_minutes);
        assert_eq!(loaded.grid_spacing, 4.0, "the good fields survive");
        assert!(loaded.automation_enabled);
    }

    #[test]
    fn dialogs_and_panels_pop_out_unless_the_user_turned_it_off() {
        assert!(Settings::default().popout_windows);
        assert!(Settings::from_json(r#"{"grid_spacing":3.0}"#).popout_windows);
        assert!(!Settings::from_json(r#"{"popout_windows":false}"#).popout_windows);
    }

    #[test]
    fn fields_missing_from_an_older_save_take_their_default() {
        let loaded = Settings::from_json(r#"{"grid_spacing":3.0}"#);
        assert_eq!(
            loaded,
            Settings {
                grid_spacing: 3.0,
                ..Settings::default()
            }
        );
        assert_eq!(Settings::from_json("not json"), Settings::default());
        assert_eq!(Settings::from_json("[1]"), Settings::default());
    }

    #[test]
    fn layout_paper_is_light_unless_saved_dark() {
        assert_eq!(Settings::default().layout_paper, LayoutPaper::Light);
        let dark = Settings::from_json(r#"{"layout_paper":"dark","grid_spacing":3.0}"#);
        assert_eq!(dark.layout_paper, LayoutPaper::Dark);
        let text = serde_json::to_string(&dark).unwrap();
        assert!(text.contains(r#""layout_paper":"dark""#), "{text}");
        // An unknown value costs only itself.
        let odd = Settings::from_json(r#"{"layout_paper":"sepia","grid_spacing":3.0}"#);
        assert_eq!(
            (odd.layout_paper, odd.grid_spacing),
            (LayoutPaper::Light, 3.0)
        );
    }

    #[test]
    fn paper_colour_is_white_unless_chosen() {
        use crate::paper_colour::PaperColour;
        // Saves from before the choice existed have white paper.
        let old = Settings::from_json(r#"{"layout_paper":"light"}"#);
        assert_eq!(old.layout_paper_colour, PaperColour::White);
        let grey = Settings::from_json(r#"{"layout_paper_colour":"light_grey"}"#);
        assert_eq!(grey.layout_paper_colour, PaperColour::LightGrey);
        let odd = Settings::from_json(r#"{"layout_paper_colour":"purple","grid_spacing":3.0}"#);
        assert_eq!(
            (odd.layout_paper_colour, odd.grid_spacing),
            (PaperColour::White, 3.0)
        );
        // Not a name: a colour of one's own is not offered.
        let rgb = Settings::from_json(r#"{"layout_paper_colour":{"custom":[0,0,0]}}"#);
        assert_eq!(rgb.layout_paper_colour, PaperColour::White);
    }

    #[test]
    fn choosing_a_folder_sets_it_and_a_cancel_changes_nothing() {
        use crate::locations::Location;
        use crate::picker::{Script, ScriptedPicker};
        let script = Script::default();
        let mut picker = ScriptedPicker(script.clone());
        let mut settings = Settings::default();
        script.will_pick_folder(Some("plans".into()));
        script.will_pick_folder(None);
        for _ in 0..2 {
            settings.choose_folder(Location::Templates, &mut picker);
        }
        assert_eq!(settings.templates_dir, Some("plans".into()));
    }

    #[test]
    fn approving_a_folder_adds_it_once_and_a_cancel_adds_nothing() {
        use crate::picker::{Script, ScriptedPicker};
        let script = Script::default();
        let mut picker = ScriptedPicker(script.clone());
        let mut settings = Settings::default();
        script.will_pick_folder(Some("plans".into()));
        script.will_pick_folder(Some("plans".into()));
        script.will_pick_folder(None);
        for _ in 0..3 {
            settings.approve_folder(&mut picker);
        }
        assert_eq!(
            settings.automation_roots,
            [std::path::PathBuf::from("plans")]
        );
    }

    #[test]
    fn an_empty_folder_is_the_default() {
        let loaded = Settings::from_json(r#"{"templates_dir":""}"#).sanitized();
        assert_eq!(loaded.templates_dir, None);
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let grid = |value: &str| {
            let mut storage = MemoryStorage::default();
            storage.set_string(KEY, format!(r#"{{"grid_spacing":{value}}}"#));
            Settings::load(Some(&storage)).grid_spacing
        };
        assert_eq!(grid("0.0001"), GRID_SPACING_RANGE.0);
        assert_eq!(grid("1e9"), GRID_SPACING_RANGE.1);
        assert_eq!(grid("null"), Settings::default().grid_spacing);
    }

    #[test]
    fn pointer_sizes_are_clamped_and_old_saves_keep_the_old_sizes() {
        let loaded =
            Settings::from_json(r#"{"pick_size":0,"snap_aperture":500,"grip_size":3}"#).sanitized();
        assert_eq!(
            (loaded.pick_size, loaded.snap_aperture, loaded.grip_size),
            (PICK_SIZE_RANGE.0, SNAP_APERTURE_RANGE.1, 3)
        );
        let sizes = Settings::from_json("{}").pointer_sizes();
        assert_eq!(sizes, opendraft_canvas::PointerSizes::default());
    }

    #[test]
    fn zoom_speed_is_clamped() {
        let speed = |text: &str| Settings::from_json(text).sanitized().zoom_speed;
        assert_eq!(speed("{}"), 100);
        assert_eq!(speed(r#"{"zoom_speed":0}"#), ZOOM_SPEED_RANGE.0);
        assert_eq!(speed(r#"{"zoom_speed":99999}"#), ZOOM_SPEED_RANGE.1);
    }

    #[test]
    fn zoom_factor_matches_percent() {
        let settings = Settings {
            ui_scale_percent: 125,
            ..Settings::default()
        };
        assert_eq!(settings.zoom_factor(), 1.25);
    }

    #[test]
    fn ortho_and_polar_tracking_exclude_each_other() {
        let mut s = Settings::default();
        assert!(!s.ortho.enabled && !s.tracking.polar);
        s.set_polar(true);
        assert!(s.tracking.polar && !s.ortho.enabled);
        s.set_ortho(true);
        assert!(s.ortho.enabled && !s.tracking.polar);
        s.set_ortho(false);
        assert!(!s.ortho.enabled && !s.tracking.polar);
        // A saved file with both has ortho win; a bad increment is fixed.
        let mut both = Settings::default();
        both.ortho.enabled = true;
        both.tracking.polar = true;
        both.tracking.increment = f64::NAN;
        both.tracking.path_display = 9;
        let fixed = both.sanitized();
        assert!(fixed.ortho.enabled && !fixed.tracking.polar);
        assert_eq!(fixed.tracking.increment, 90.0);
        assert_eq!(fixed.tracking.path_display, 3);
    }
}
