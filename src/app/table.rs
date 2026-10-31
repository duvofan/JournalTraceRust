//! The entry table shared by the grid, the tree and the entry window, drawn
//! to look like the C# DataGridView: row header, grid lines, sortable
//! headers, multi-row selection and the right-click menu.

use super::{Actions, Data, Reasons};
use crate::filter::Column;
use crate::i18n::{t, Lang};
use crate::model::{self, Entry};
use eframe::egui;
use egui_extras::{Column as TCol, TableBuilder};
use std::borrow::Cow;
use std::cmp::Ordering;
use std::collections::HashSet;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct Sort {
    pub col: Column,
    pub asc: bool,
}

impl Default for Sort {
    fn default() -> Self {
        Sort { col: Column::Usn, asc: true }
    }
}

/// Selected rows (entry indices) plus the row Shift-click extends from.
#[derive(Default)]
pub(crate) struct Selection {
    set: HashSet<u32>,
    anchor: Option<u32>,
}

impl Selection {
    pub fn clear(&mut self) {
        self.set.clear();
        self.anchor = None;
    }

    pub fn len(&self) -> usize {
        self.set.len()
    }

    pub fn contains(&self, idx: u32) -> bool {
        self.set.contains(&idx)
    }

    pub fn select_all(&mut self, rows: &[u32]) {
        self.set.extend(rows.iter().copied());
        if self.anchor.is_none() {
            self.anchor = rows.first().copied();
        }
    }

    /// Selected rows in the order they are shown.
    pub fn ordered(&self, rows: &[u32]) -> Vec<u32> {
        rows.iter().copied().filter(|r| self.set.contains(r)).collect()
    }

    fn click(&mut self, idx: u32, rows: &[u32], mods: egui::Modifiers) {
        if mods.shift {
            if let Some(anchor) = self.anchor {
                let pos = |v: u32| rows.iter().position(|&r| r == v);
                if let (Some(a), Some(b)) = (pos(anchor), pos(idx)) {
                    if !mods.command {
                        self.set.clear();
                    }
                    self.set.extend(rows[a.min(b)..=a.max(b)].iter().copied());
                    return;
                }
            }
        }
        if mods.command {
            if !self.set.remove(&idx) {
                self.set.insert(idx);
            }
        } else {
            self.set.clear();
            self.set.insert(idx);
        }
        self.anchor = Some(idx);
    }
}

pub(crate) fn col_label(lang: Lang, col: Column) -> &'static str {
    match col {
        Column::Usn => "USN",
        Column::Name => t(lang, "name"),
        Column::Date => t(lang, "date"),
        Column::Reason => t(lang, "reason"),
        Column::Directory => t(lang, "directory"),
        Column::Drive => t(lang, "drive"),
    }
}

/// Adapter so the filter can read a row.
pub(crate) struct RowRef<'a> {
    pub e: &'a Entry,
    pub dir: &'a str,
    pub reason: &'a str,
    pub drive: char,
}

impl crate::filter::FilterRow for RowRef<'_> {
    fn field(&self, col: Column) -> Cow<'_, str> {
        match col {
            Column::Usn => Cow::Owned(self.e.usn.to_string()),
            Column::Name => Cow::Borrowed(&self.e.name),
            Column::Date => Cow::Owned(model::format_filetime(self.e.timestamp)),
            Column::Reason => Cow::Borrowed(self.reason),
            Column::Directory => Cow::Borrowed(self.dir),
            Column::Drive => Cow::Owned(self.drive.to_string()),
        }
    }
}

fn cmp_ci(a: &str, b: &str) -> Ordering {
    a.chars().flat_map(char::to_lowercase).cmp(b.chars().flat_map(char::to_lowercase))
}

pub(crate) fn sort_rows(rows: &mut [u32], data: &Data, reasons: &Reasons, sort: Sort) {
    let entries = &data.result.entries;
    rows.sort_by(|&a, &b| {
        let (ea, eb) = (&entries[a as usize], &entries[b as usize]);
        let o = match sort.col {
            Column::Usn => ea.usn.cmp(&eb.usn),
            Column::Name => cmp_ci(&ea.name, &eb.name),
            Column::Date => ea.timestamp.cmp(&eb.timestamp),
            Column::Reason => reasons.get(ea.reason).cmp(reasons.get(eb.reason)),
            Column::Directory => {
                cmp_ci(data.result.dir_path(ea), data.result.dir_path(eb))
            }
            Column::Drive => ea.vol.cmp(&eb.vol),
        }
        .then(ea.usn.cmp(&eb.usn));
        if sort.asc {
            o
        } else {
            o.reverse()
        }
    });
}

/// Clicking a header toggles direction on the same column, otherwise sorts
/// ascending by the new one.
pub(crate) fn toggle_sort(sort: &mut Sort, col: Column) {
    if sort.col == col {
        sort.asc = !sort.asc;
    } else {
        *sort = Sort { col, asc: true };
    }
}

/// Tab separated rows, the way DataGridView copies a selection.
pub(crate) fn rows_tsv(data: &Data, reasons: &Reasons, rows: &[u32]) -> String {
    let mut out = String::new();
    for &i in rows {
        let e = &data.result.entries[i as usize];
        if !out.is_empty() {
            out.push_str("\r\n");
        }
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}",
            e.usn,
            data.result.drive(e),
            e.name,
            model::format_filetime(e.timestamp),
            reasons.get(e.reason),
            data.result.dir_path(e)
        ));
    }
    out
}

fn reason_color(ui: &egui::Ui, mask: u32) -> Option<egui::Color32> {
    let dark = ui.visuals().dark_mode;
    if mask & model::REASON_FILE_DELETE != 0 {
        Some(if dark { egui::Color32::from_rgb(255, 120, 110) } else { egui::Color32::from_rgb(180, 30, 20) })
    } else if mask & model::REASON_FILE_CREATE != 0 {
        Some(if dark { egui::Color32::from_rgb(120, 220, 130) } else { egui::Color32::from_rgb(20, 130, 40) })
    } else if mask & (model::REASON_RENAME_OLD_NAME | model::REASON_RENAME_NEW_NAME) != 0 {
        Some(if dark { egui::Color32::from_rgb(240, 200, 90) } else { egui::Color32::from_rgb(150, 100, 0) })
    } else {
        None
    }
}

pub(crate) struct TableOpts {
    pub id: egui::Id,
    pub show_dir: bool,
    pub allow_info: bool,
    pub max_height: f32,
    /// Ctrl+C / Ctrl+A act on this table (only the main layout's table).
    pub keyboard: bool,
}

fn grid_stroke(ui: &egui::Ui) -> egui::Stroke {
    egui::Stroke::new(1.0_f32, ui.visuals().widgets.noninteractive.bg_stroke.color.gamma_multiply(0.7))
}

/// Vertical grid line on the right edge of a cell.
fn paint_cell_border(ui: &egui::Ui) {
    let rect = ui.max_rect();
    let x = rect.right() + ui.spacing().item_spacing.x / 2.0;
    ui.painter().vline(x, rect.y_range().expand(ui.spacing().item_spacing.y), grid_stroke(ui));
}

/// Draws `rows` (indices into the entries). Returns the header the user
/// clicked, if any, so the caller can re-sort its own row list.
#[allow(clippy::too_many_arguments)]
pub(crate) fn entry_table(
    ui: &mut egui::Ui,
    lang: Lang,
    data: &Data,
    reasons: &Reasons,
    rows: &[u32],
    sort: Sort,
    sel: &mut Selection,
    opts: TableOpts,
    actions: &mut Actions,
) -> Option<Column> {
    let cols: &[Column] = if opts.show_dir {
        &[Column::Usn, Column::Drive, Column::Name, Column::Date, Column::Reason, Column::Directory]
    } else {
        &[Column::Usn, Column::Name, Column::Date, Column::Reason]
    };
    let row_h = ui.text_style_height(&egui::TextStyle::Body) + 6.0;
    let mut clicked_header = None;

    if opts.keyboard && !ui.ctx().wants_keyboard_input() {
        let (copy, all) = ui.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.modifiers.command && i.key_pressed(egui::Key::A),
            )
        });
        if all {
            sel.select_all(rows);
        }
        if copy && sel.len() > 0 {
            actions.copy = Some(rows_tsv(data, reasons, &sel.ordered(rows)));
        }
    }

    ui.push_id(opts.id, |ui| {
        let header_fill = ui.visuals().widgets.inactive.weak_bg_fill;
        let mut b = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .sense(egui::Sense::click())
            .cell_layout(egui::Layout::left_to_right(egui::Align::Center))
            .max_scroll_height(opts.max_height)
            .auto_shrink([false, false])
            .column(TCol::exact(14.0)) // DataGridView row header
            .column(TCol::initial(100.0).at_least(50.0).clip(true));
        if opts.show_dir {
            b = b.column(TCol::initial(56.0).at_least(40.0).clip(true));
        }
        b = b
            .column(TCol::initial(220.0).at_least(60.0).clip(true))
            .column(TCol::initial(130.0).at_least(60.0).clip(true));
        b = if opts.show_dir {
            b.column(TCol::initial(300.0).at_least(60.0).clip(true))
                .column(TCol::remainder().at_least(100.0).clip(true))
        } else {
            b.column(TCol::remainder().at_least(100.0).clip(true))
        };

        b.header(row_h + 2.0, |mut header| {
            header.col(|ui| {
                ui.painter().rect_filled(ui.max_rect().expand(2.0), 0.0, header_fill);
                paint_cell_border(ui);
            });
            for &c in cols {
                header.col(|ui| {
                    // The whole cell is the click target, not just the text.
                    let rect = ui.max_rect();
                    let resp = ui
                        .interact(rect, ui.id().with(("sort_header", c)), egui::Sense::click())
                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                    let fill = if resp.hovered() { ui.visuals().widgets.hovered.weak_bg_fill } else { header_fill };
                    ui.painter().rect_filled(rect.expand2(egui::vec2(3.0, 2.0)), 0.0, fill);
                    paint_cell_border(ui);
                    ui.add(egui::Label::new(egui::RichText::new(col_label(lang, c)).strong()).selectable(false));
                    if sort.col == c {
                        paint_sort_arrow(ui, rect, sort.asc);
                    }
                    if resp.clicked() {
                        clicked_header = Some(c);
                    }
                });
            }
        })
        .body(|body| {
            body.rows(row_h, rows.len(), |mut row| {
                let idx = rows[row.index()];
                let e = &data.result.entries[idx as usize];
                row.set_selected(sel.contains(idx));
                let current = sel.anchor == Some(idx);

                let mut responses = Vec::with_capacity(cols.len() + 1);
                let (_, resp) = row.col(|ui| {
                    paint_cell_border(ui);
                    if current {
                        paint_row_marker(ui);
                    }
                });
                responses.push((None, resp));
                for &c in cols {
                    let (_, resp) = row.col(|ui| {
                        paint_cell_border(ui);
                        let text = cell_text(data, reasons, e, c);
                        let mut rich = egui::RichText::new(text.as_ref());
                        if c == Column::Reason {
                            if let Some(color) = reason_color(ui, e.reason) {
                                rich = rich.color(color);
                            }
                        }
                        if c == Column::Directory && !data.result.dir_exists(e) {
                            rich = rich.italics().weak();
                        }
                        ui.add(egui::Label::new(rich).truncate().selectable(false));
                    });
                    responses.push((Some(c), resp));
                }

                for (c, resp) in responses {
                    if resp.clicked() {
                        let mods = resp.ctx.input(|i| i.modifiers);
                        sel.click(idx, rows, mods);
                    }
                    // Right-click keeps a multi-row selection that includes this row.
                    if resp.secondary_clicked() && !sel.contains(idx) {
                        sel.click(idx, rows, egui::Modifiers::NONE);
                    }
                    if resp.double_clicked() && opts.allow_info {
                        actions.open_info = Some(idx);
                    }
                    resp.context_menu(|ui| row_menu(ui, lang, data, reasons, c, e, idx, rows, sel, opts.allow_info, actions));
                }
            });
        });
    });
    clicked_header
}

fn cell_text<'a>(data: &'a Data, reasons: &'a Reasons, e: &'a Entry, c: Column) -> Cow<'a, str> {
    match c {
        Column::Usn => e.usn.to_string().into(),
        Column::Name => e.name.as_str().into(),
        Column::Date => model::format_filetime(e.timestamp).into(),
        Column::Reason => reasons.get(e.reason).into(),
        Column::Directory => data.result.dir_path(e).into(),
        Column::Drive => data.result.drive(e).to_string().into(),
    }
}

/// Drawn instead of a ▲/▼ glyph so it never depends on the font.
fn paint_sort_arrow(ui: &egui::Ui, rect: egui::Rect, asc: bool) {
    let c = egui::pos2(rect.right() - 10.0, rect.center().y);
    let (w, h) = (4.5, 3.5);
    let points = if asc {
        vec![egui::pos2(c.x - w, c.y + h), egui::pos2(c.x + w, c.y + h), egui::pos2(c.x, c.y - h)]
    } else {
        vec![egui::pos2(c.x - w, c.y - h), egui::pos2(c.x + w, c.y - h), egui::pos2(c.x, c.y + h)]
    };
    let color = ui.visuals().strong_text_color();
    ui.painter().add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
}

/// The ▶ DataGridView shows in the row header of the current row.
fn paint_row_marker(ui: &egui::Ui) {
    let c = ui.max_rect().center();
    let points = vec![egui::pos2(c.x - 3.0, c.y - 4.5), egui::pos2(c.x - 3.0, c.y + 4.5), egui::pos2(c.x + 3.5, c.y)];
    let color = ui.visuals().strong_text_color();
    ui.painter().add(egui::Shape::convex_polygon(points, color, egui::Stroke::NONE));
}

#[allow(clippy::too_many_arguments)]
fn row_menu(
    ui: &mut egui::Ui,
    lang: Lang,
    data: &Data,
    reasons: &Reasons,
    col: Option<Column>,
    e: &Entry,
    idx: u32,
    rows: &[u32],
    sel: &Selection,
    allow_info: bool,
    actions: &mut Actions,
) {
    if let Some(c) = col {
        if ui.button(format!("{} \"{}\"", t(lang, "copy"), col_label(lang, c))).clicked() {
            actions.copy = Some(cell_text(data, reasons, e, c).into_owned());
            ui.close_menu();
        }
    }
    let n = sel.len().max(1);
    if ui.button(format!("{} ({n})", t(lang, "copyrows"))).clicked() {
        let selected = if sel.len() > 0 { sel.ordered(rows) } else { vec![idx] };
        actions.copy = Some(rows_tsv(data, reasons, &selected));
        ui.close_menu();
    }
    ui.separator();
    if ui.add_enabled(allow_info, egui::Button::new(t(lang, "entryinfo"))).clicked() {
        actions.open_info = Some(idx);
        ui.close_menu();
    }
    let exists = data.result.dir_exists(e);
    if ui.add_enabled(exists, egui::Button::new(t(lang, "enterdir"))).clicked() {
        let dir = data.result.dir_path(e).to_owned();
        // Prefer where the file is now (it may have moved since), then the
        // name in this directory; fall back to just opening the directory.
        let file = data
            .handle(e)
            .and_then(|v| v.resolve_path(e.file_ref))
            .or_else(|| {
                let p = std::path::Path::new(&dir).join(&e.name);
                p.exists().then(|| p.to_string_lossy().into_owned())
            });
        actions.open_dir = Some((dir, file));
        ui.close_menu();
    }
}
