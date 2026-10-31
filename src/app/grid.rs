//! Data Grid layout: every entry in one table with the search box, column
//! picker, reason check boxes and the timeline.

use super::table::{self, col_label, RowRef, Selection, Sort, TableOpts};
use super::timeline::Timeline;
use super::{Actions, Data, Reasons};
use crate::filter::{Column, Filter, MatchMode};
use crate::i18n::{t, Lang};
use crate::model;
use eframe::egui;

/// Reason check boxes. The C# version matched these against English reason
/// text, so they broke in Portuguese; here they test the raw flag.
const CHECKS: [(u32, &str); 5] = [
    (model::REASON_FILE_DELETE, "deleted"),
    (model::REASON_RENAME_NEW_NAME, "renamednew"),
    (model::REASON_RENAME_OLD_NAME, "renamedold"),
    (model::REASON_STREAM_CHANGE, "streamchange"),
    (model::REASON_BASIC_INFO_CHANGE, "basicinfochange"),
];

/// "Short-lived": created and deleted within a minute (FILETIME ticks).
const SHORT_LIVED: i64 = 60 * 10_000_000;

/// Delay after the last keystroke before searching.
const TYPE_DELAY: f64 = 0.3;

pub(crate) struct GridState {
    query: String,
    column: Option<Column>,
    match_mode: MatchMode,
    checks: [bool; 5],
    short_lived: bool,
    /// [start, end) in FILETIME ticks, set by clicking the timeline.
    time_range: Option<(i64, i64)>,
    typed_at: Option<f64>,
    rows: Vec<u32>,
    sort: Sort,
    pub selection: Selection,
    pub timeline: Timeline,
    pub show_timeline: bool,
}

impl Default for GridState {
    fn default() -> Self {
        GridState {
            query: String::new(),
            column: Some(Column::Name), // C# default was "name"
            match_mode: MatchMode::Contains,
            checks: [false; 5],
            short_lived: false,
            time_range: None,
            typed_at: None,
            rows: Vec::new(),
            sort: Sort::default(),
            selection: Selection::default(),
            timeline: Timeline::default(),
            show_timeline: false,
        }
    }
}

impl GridState {
    pub fn new(data: &Data, reasons: &Reasons, show_timeline: bool) -> GridState {
        let mut grid =
            GridState { timeline: Timeline::build(&data.result.entries), show_timeline, ..Default::default() };
        grid.apply(data, reasons);
        grid
    }

    pub fn rows(&self) -> &[u32] {
        &self.rows
    }

    pub fn apply(&mut self, data: &Data, reasons: &Reasons) {
        self.rows = self.matching(data, reasons, 0..data.result.entries.len() as u32);
        self.selection.clear();
        if self.sort != Sort::default() {
            table::sort_rows(&mut self.rows, data, reasons, self.sort);
        }
    }

    /// Adds rows that live monitoring appended, keeping the current filter.
    pub fn append(&mut self, data: &Data, reasons: &Reasons, new: std::ops::Range<u32>) {
        self.timeline.add(&data.result.entries[new.start as usize..new.end as usize]);
        let added = self.matching(data, reasons, new);
        if added.is_empty() {
            return;
        }
        self.rows.extend(added);
        if self.sort != Sort::default() {
            table::sort_rows(&mut self.rows, data, reasons, self.sort);
        }
    }

    fn matching(&self, data: &Data, reasons: &Reasons, range: std::ops::Range<u32>) -> Vec<u32> {
        let filter = Filter::parse_with(&self.query, self.column, self.match_mode);
        let required: Vec<u32> =
            CHECKS.iter().zip(self.checks).filter(|(_, on)| *on).map(|((flag, _), _)| *flag).collect();
        let r = &data.result;
        let keep = |i: &u32| {
            let e = &r.entries[*i as usize];
            required.iter().all(|f| e.reason & f != 0)
                && self.time_range.is_none_or(|(a, b)| e.timestamp >= a && e.timestamp < b)
                && (!self.short_lived || data.lifetime(e).is_some_and(|l| l <= SHORT_LIVED))
                && (filter.is_empty()
                    || filter.matches(&RowRef { e, dir: r.dir_path(e), reason: reasons.get(e.reason), drive: r.drive(e) }))
        };

        // Large scans: split across CPU cores, keeping the original order.
        let len = (range.end - range.start) as usize;
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(16);
        if len < 50_000 || threads < 2 {
            return range.filter(keep).collect();
        }
        let chunk = len.div_ceil(threads) as u32;
        std::thread::scope(|s| {
            let keep = &keep;
            let parts: Vec<_> = (0..threads as u32)
                .map(|k| {
                    let start = (range.start + k * chunk).min(range.end);
                    let end = (start + chunk).min(range.end);
                    s.spawn(move || (start..end).filter(keep).collect::<Vec<u32>>())
                })
                .collect();
            parts.into_iter().flat_map(|h| h.join().unwrap()).collect()
        })
    }

    pub fn show(&mut self, ui: &mut egui::Ui, lang: Lang, data: &Data, reasons: &Reasons, actions: &mut Actions) {
        let mut run = false;
        let now = ui.input(|i| i.time);
        let search_id = egui::Id::new("grid_search");
        if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F)) {
            ui.memory_mut(|m| m.request_focus(search_id));
        }
        ui.horizontal(|ui| {
            // One height for the text field, combo boxes and buttons so the
            // row lines up; egui otherwise sizes each to its own content.
            ui.spacing_mut().interact_size.y = 24.0;
            let edit = ui.add(
                egui::TextEdit::singleline(&mut self.query)
                    .id(search_id)
                    .hint_text("name:.exe!!svchost;reason:delete;drive:C")
                    .desired_width(360.0),
            );
            if edit.changed() {
                self.typed_at = Some(now);
            }
            if edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                run = true;
            }
            let selected_text = self.column.map_or(t(lang, "allcolumns"), |c| col_label(lang, c));
            egui::ComboBox::from_id_salt("search_column").selected_text(selected_text).show_ui(ui, |ui| {
                run |= ui.selectable_value(&mut self.column, None, t(lang, "allcolumns")).clicked();
                for c in [Column::Name, Column::Date, Column::Reason, Column::Directory, Column::Drive, Column::Usn] {
                    run |= ui.selectable_value(&mut self.column, Some(c), col_label(lang, c)).clicked();
                }
            });
            egui::ComboBox::from_id_salt("match_mode").selected_text(mode_label(lang, self.match_mode)).show_ui(ui, |ui| {
                for m in MatchMode::ALL {
                    run |= ui.selectable_value(&mut self.match_mode, m, mode_label(lang, m)).clicked();
                }
            });
            if ui.button(t(lang, "search")).clicked() {
                run = true;
            }
            if ui.button(t(lang, "clear")).clicked() {
                self.query.clear();
                self.checks = [false; 5];
                self.short_lived = false;
                self.time_range = None;
                run = true;
            }
            ui.separator();
            ui.label(format!("{} {} / {}", t(lang, "shown"), self.rows.len(), data.result.entries.len()));
            if self.selection.len() > 1 {
                ui.separator();
                ui.label(format!("{} {}", t(lang, "selected"), self.selection.len()));
            }
        });
        ui.horizontal_wrapped(|ui| {
            ui.label(t(lang, "reasonfilters"));
            for (i, (_, key)) in CHECKS.iter().enumerate() {
                run |= ui.checkbox(&mut self.checks[i], t(lang, key)).changed();
            }
            ui.separator();
            run |= ui.checkbox(&mut self.short_lived, t(lang, "shortlived")).changed();
            if let Some((a, b)) = self.time_range {
                ui.separator();
                ui.label(t(lang, "timefilter"));
                ui.strong(format!("{} – {}", short_time(a), short_time(b)));
                if ui.small_button(t(lang, "clear")).clicked() {
                    self.time_range = None;
                    run = true;
                }
            }
        });

        // Search as you type, once typing pauses.
        if let Some(t0) = self.typed_at {
            if now - t0 >= TYPE_DELAY {
                self.typed_at = None;
                run = true;
            } else {
                ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(TYPE_DELAY - (now - t0)));
            }
        }

        if self.show_timeline {
            if let Some(range) = self.timeline.show(ui, self.time_range) {
                self.time_range = range;
                run = true;
            }
        }
        ui.separator();

        if run {
            self.typed_at = None;
            self.apply(data, reasons);
        }

        let opts = TableOpts {
            id: egui::Id::new("grid_table"),
            show_dir: true,
            allow_info: true,
            max_height: ui.available_height(),
            keyboard: true,
        };
        if let Some(col) =
            table::entry_table(ui, lang, data, reasons, &self.rows, self.sort, &mut self.selection, opts, actions)
        {
            table::toggle_sort(&mut self.sort, col);
            table::sort_rows(&mut self.rows, data, reasons, self.sort);
        }
    }
}

fn mode_label(lang: Lang, mode: MatchMode) -> &'static str {
    t(
        lang,
        match mode {
            MatchMode::Contains => "contains",
            MatchMode::Exact => "exact",
            MatchMode::StartsWith => "startswith",
            MatchMode::EndsWith => "endswith",
        },
    )
}

/// "dd/mm/yyyy HH:MM" for chips and the timeline.
pub(crate) fn short_time(ft: i64) -> String {
    let mut s = model::format_filetime(ft);
    s.truncate(16);
    s
}
