//! egui front end. Mirrors the C# FormMain: menu bar, status screen, Data
//! Grid / Directory Tree layouts and a status bar.

mod demo;
mod grid;
mod table;
mod timeline;
mod tree;
mod windows;

use crate::i18n::{t, Lang};
use crate::model::{self, Entry, FileStory, Ref, ScanResult};
use crate::ntfs::{self, Volume, Win32Error};
use crate::export;
use crate::scan::{self, ScanOutput, SharedProgress};
use eframe::egui;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

#[cfg(has_background)]
const BACKGROUND_PNG: Option<&[u8]> = Some(include_bytes!("../../assets/background.png"));
#[cfg(not(has_background))]
const BACKGROUND_PNG: Option<&[u8]> = None;

use grid::GridState;
use tree::TreeState;
use windows::{AboutWindow, DriveChoice, DriveWindow, InfoWindow};

const ICON_PNG: &[u8] = include_bytes!("../../assets/icon.png");

pub fn run() -> eframe::Result {
    run_with(None)
}

/// Opens the UI with generated entries; no admin rights or NTFS needed.
pub fn run_demo() -> eframe::Result {
    run_with(Some(demo::generate(200_000)))
}

fn run_with(initial: Option<ScanOutput>) -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("JournalTrace")
            .with_inner_size([1150.0, 720.0])
            .with_min_inner_size([720.0, 420.0])
            .with_icon(Arc::new(icon_data())),
        ..Default::default()
    };
    eframe::run_native(
        "JournalTrace",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, initial)))),
    )
}

/// Window and taskbar icon, from the same picture as the exe icon.
fn icon_data() -> egui::IconData {
    match image::load_from_memory_with_format(ICON_PNG, image::ImageFormat::Png) {
        Ok(img) => {
            let img = img.to_rgba8();
            egui::IconData { width: img.width(), height: img.height(), rgba: img.into_raw() }
        }
        Err(_) => egui::IconData::default(),
    }
}

/// Loaded scan plus the open volumes used for on-demand path lookups.
pub(crate) struct Data {
    pub result: ScanResult,
    /// Same order as `result.volumes`; empty for demo data.
    pub volumes: Vec<Arc<Volume>>,
    /// Create-to-delete time of files that were both created and deleted
    /// inside the journal, for the "short-lived" filter.
    lifetimes: HashMap<Ref, i64>,
}

impl Data {
    fn new(result: ScanResult, volumes: Vec<Arc<Volume>>) -> Data {
        let mut data = Data { result, volumes, lifetimes: HashMap::new() };
        let keys: Vec<Ref> = data.result.by_file.keys().copied().collect();
        data.update_lifetimes(keys);
        data
    }

    fn update_lifetimes(&mut self, keys: impl IntoIterator<Item = Ref>) {
        for key in keys {
            let Some(history) = self.result.by_file.get(&key) else { continue };
            match FileStory::of(&self.result.entries, history).lifetime() {
                Some(l) => self.lifetimes.insert(key, l),
                None => self.lifetimes.remove(&key),
            };
        }
    }

    pub fn lifetime(&self, e: &Entry) -> Option<i64> {
        self.lifetimes.get(&e.file_key()).copied()
    }

    pub fn handle(&self, e: &Entry) -> Option<&Arc<Volume>> {
        self.volumes.get(e.vol as usize)
    }
}

/// Reason mask -> display text, per language. Filled for every mask in the
/// scan up front so tables and filters can read it without `&mut`.
pub(crate) struct Reasons {
    lang: Lang,
    map: HashMap<u32, String>,
}

impl Reasons {
    fn new(lang: Lang) -> Self {
        Reasons { lang, map: HashMap::new() }
    }

    fn prepare(&mut self, lang: Lang, data: &Data) {
        if lang != self.lang {
            self.lang = lang;
            self.map.clear();
        }
        self.add(&data.result.entries);
    }

    fn add(&mut self, entries: &[model::Entry]) {
        let names = crate::i18n::reason_names(self.lang);
        for e in entries {
            self.map.entry(e.reason).or_insert_with(|| model::reason_text(e.reason, names));
        }
    }

    pub fn get(&self, mask: u32) -> &str {
        self.map.get(&mask).map(String::as_str).unwrap_or("")
    }
}

/// Things a table or window asked for this frame; applied after drawing.
#[derive(Default)]
pub(crate) struct Actions {
    pub open_info: Option<u32>,
    /// Directory to open, and the file to select in it when it still exists.
    pub open_dir: Option<(String, Option<String>)>,
    pub copy: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Layout {
    None,
    Grid,
    Tree,
}

struct App {
    lang: Lang,
    layout: Layout,
    is_admin: bool,

    selected_drive: Option<char>,
    all_drives: bool,
    show_timeline: bool,
    drive_window: DriveWindow,
    about_window: AboutWindow,
    info_windows: Vec<InfoWindow>,

    scan: Option<SharedProgress>,
    phase: i32,
    fraction: f32,
    error: Option<Win32Error>,

    data: Option<Data>,
    reasons: Reasons,
    grid: GridState,
    tree: Option<TreeState>,
    live: Vec<scan::Live>,
    scan_drive: Option<(char, usize, usize)>,

    background: Option<egui::TextureHandle>,
    background_pending: Arc<Mutex<Option<egui::ColorImage>>>,
    notice: Option<String>,
}

impl App {
    fn new(cc: &eframe::CreationContext<'_>, initial: Option<ScanOutput>) -> Self {
        setup_style(&cc.egui_ctx);

        // Decode the picture off the UI thread so the window opens at once.
        let pending = Arc::new(Mutex::new(None));
        if let Some(bytes) = BACKGROUND_PNG {
            let pending = pending.clone();
            let ctx = cc.egui_ctx.clone();
            std::thread::spawn(move || {
                if let Some(img) = decode_png(bytes) {
                    *pending.lock().unwrap() = Some(img);
                    ctx.request_repaint();
                }
            });
        }

        let default_drive = ntfs::list_ntfs_drives()
            .iter()
            .find(|d| d.letter == 'C')
            .map(|d| d.letter);

        // Same language as Windows: Turkish, Portuguese, otherwise English.
        let lang = Lang::from_langid(ntfs::ui_language());
        let mut app = App {
            lang,
            layout: Layout::None,
            is_admin: ntfs::is_admin(),
            selected_drive: default_drive,
            all_drives: false,
            show_timeline: false,
            drive_window: DriveWindow::default(),
            about_window: AboutWindow::new(&cc.egui_ctx),
            info_windows: Vec::new(),
            scan: None,
            phase: 0,
            fraction: 0.0,
            error: None,
            data: None,
            reasons: Reasons::new(lang),
            grid: GridState::default(),
            tree: None,
            live: Vec::new(),
            scan_drive: None,
            background: None,
            background_pending: pending,
            notice: None,
        };
        if let Some(out) = initial {
            app.load(out);
        }
        app
    }

    fn load(&mut self, out: ScanOutput) {
        let data = Data::new(out.result, out.volumes);
        self.reasons.prepare(self.lang, &data);
        self.grid = GridState::new(&data, &self.reasons, self.show_timeline);
        self.tree = None;
        self.live.clear();
        self.info_windows.clear();
        self.data = Some(data);
        self.phase = 6;
        self.layout = Layout::Grid;
    }

    fn start_scan(&mut self, ctx: &egui::Context) {
        let letters: Vec<char> = if self.all_drives {
            ntfs::list_ntfs_drives().iter().map(|d| d.letter).collect()
        } else {
            self.selected_drive.into_iter().collect()
        };
        if letters.is_empty() {
            self.notice = Some(t(self.lang, "nodrive").to_string());
            return;
        }
        self.data = None;
        self.tree = None;
        self.live.clear();
        self.scan_drive = None;
        self.grid = GridState::default();
        self.info_windows.clear();
        self.layout = Layout::None;
        self.error = None;
        self.notice = None;
        let ctx = ctx.clone();
        self.scan = Some(scan::start(letters, move || ctx.request_repaint()));
    }

    fn poll_scan(&mut self) {
        let Some(shared) = self.scan.clone() else { return };
        let mut p = shared.lock().unwrap();
        self.phase = p.phase;
        self.fraction = p.fraction;
        self.error = p.error;
        self.scan_drive = p.drive.map(|d| (d, p.drive_pos, p.drive_count));
        if !p.skipped.is_empty() {
            let list: Vec<String> = p.skipped.iter().map(|(d, e)| format!("{d}:\\ ({e})")).collect();
            self.notice = Some(format!("{} {}", t(self.lang, "skipped"), list.join(", ")));
        }
        if let Some(out) = p.output.take() {
            drop(p);
            self.scan = None;
            self.load(out);
        } else if p.phase < 0 {
            drop(p);
            self.scan = None;
        }
    }

    fn toggle_live(&mut self, ctx: &egui::Context) {
        if !self.live.is_empty() {
            self.live.clear();
            return;
        }
        let Some(data) = &self.data else { return };
        for (vol, volume) in data.volumes.iter().enumerate() {
            let vol = vol as u8;
            let known = data.result.dirs.keys().filter(|(v, _)| *v == vol).map(|(_, id)| *id).collect();
            let journal = data.result.volumes[vol as usize].journal.clone();
            let ctx = ctx.clone();
            self.live.push(scan::start_live(vol, volume.clone(), journal, known, move || ctx.request_repaint()));
        }
    }

    fn poll_live(&mut self) {
        let mut batches = Vec::new();
        let mut stopped = None;
        for live in &self.live {
            while let Ok(msg) = live.rx.try_recv() {
                match msg {
                    scan::LiveMsg::Batch(b) => batches.push(b),
                    scan::LiveMsg::Stopped(e) => stopped = Some(e),
                }
            }
        }
        if let Some(e) = stopped {
            self.live.clear();
            self.notice = Some(format!("{} {}", t(self.lang, "livestopped"), e));
        }
        let Some(data) = &mut self.data else { return };
        for b in batches {
            let new_dirs = !b.dirs.is_empty();
            let range = data.result.append(b.vol, b.entries, b.dirs, b.journal);
            let touched: Vec<Ref> =
                data.result.entries[range.start as usize..].iter().map(|e| e.file_key()).collect();
            data.update_lifetimes(touched);
            self.reasons.add(&data.result.entries[range.start as usize..]);
            self.grid.append(data, &self.reasons, range.clone());
            if let Some(tree) = &mut self.tree {
                if new_dirs {
                    *tree = tree.rebuild(data, &self.reasons);
                } else {
                    tree.append(data, &self.reasons, range);
                }
            }
        }
    }

    fn set_lang(&mut self, lang: Lang) {
        if lang == self.lang {
            return;
        }
        self.lang = lang;
        if let Some(data) = &self.data {
            self.reasons.prepare(lang, data);
            // Reason text changed, so text filters on it must run again.
            self.grid.apply(data, &self.reasons);
            if let Some(tree) = &mut self.tree {
                tree.resort(data, &self.reasons);
            }
        }
    }

    /// Rows and selection of the layout on screen.
    fn current_rows(&self) -> (&[u32], &table::Selection) {
        match (&self.layout, &self.tree) {
            (Layout::Tree, Some(tree)) => (tree.rows(), &tree.selection),
            _ => (self.grid.rows(), &self.grid.selection),
        }
    }

    fn export(&mut self, only_selected: bool) {
        let Some(data) = &self.data else { return };
        let Some(path) = rfd::FileDialog::new()
            .set_file_name("journal.html")
            .add_filter("HTML", &["html"])
            .add_filter("CSV", &["csv"])
            .add_filter("JSON", &["json"])
            .save_file()
        else {
            return;
        };
        let (all, selection) = self.current_rows();
        let selected;
        let rows: &[u32] = if only_selected {
            selected = selection.ordered(all);
            &selected
        } else {
            all
        };
        let lang = self.lang;
        let header = ["USN", t(lang, "drive"), t(lang, "name"), t(lang, "date"), t(lang, "reason"), t(lang, "directory")];
        let reasons = &self.reasons;
        let iter = rows.iter().map(|&i| {
            let e = &data.result.entries[i as usize];
            export::ExportRow {
                usn: e.usn,
                drive: data.result.drive(e),
                name: &e.name,
                date: model::format_filetime(e.timestamp).into(),
                reason: reasons.get(e.reason),
                directory: data.result.dir_path(e),
                reason_mask: e.reason,
            }
        });
        let ext = path.extension().and_then(|x| x.to_str()).unwrap_or("").to_ascii_lowercase();
        let written = match ext.as_str() {
            "json" => export::write_json(&path, iter),
            "html" | "htm" => {
                let now = chrono::Local::now().format("%d/%m/%Y %H:%M:%S").to_string();
                let labels = export::HtmlLabels {
                    title: "JournalTrace",
                    generated: t(lang, "generated"),
                    generated_at: &now,
                    rows: t(lang, "countall"),
                    deleted: t(lang, "deleted"),
                    created: t(lang, "created"),
                    renamed: t(lang, "renamed"),
                    search: t(lang, "filterhint"),
                    header,
                };
                export::write_html(&path, &labels, iter)
            }
            _ => export::write_csv(&path, header, iter),
        };
        self.notice = Some(match written {
            Ok(n) => format!("{} {} ({})", t(lang, "exportdone"), n, path.display()),
            Err(e) => format!("{} {}", t(lang, "exportfailed"), e),
        });
    }

    fn apply_actions(&mut self, ctx: &egui::Context, actions: Actions) {
        if let Some(text) = actions.copy {
            ctx.copy_text(text);
        }
        if let Some((dir, file)) = actions.open_dir {
            let mut cmd = std::process::Command::new("explorer.exe");
            match file {
                // explorer wants `/select,"path"` as one raw argument.
                Some(file) => {
                    use std::os::windows::process::CommandExt;
                    cmd.raw_arg(format!("/select,\"{file}\""));
                }
                None => {
                    cmd.arg(dir);
                }
            }
            let _ = cmd.spawn();
        }
        if let (Some(idx), Some(data)) = (actions.open_info, &self.data) {
            if let Some(w) = self.info_windows.iter_mut().find(|w| w.idx == idx) {
                w.open = true;
                ctx.move_to_top(w.layer_id());
            } else {
                self.info_windows.push(InfoWindow::new(idx, data));
            }
        }
    }

    fn menu_bar(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let scanning = self.scan.is_some();
        let has_data = self.data.is_some();
        egui::menu::bar(ui, |ui| {
            ui.add_enabled_ui(!scanning, |ui| {
                ui.menu_button(t(lang, "drive"), |ui| {
                    if ui.button(t(lang, "select")).clicked() {
                        self.drive_window.show_window();
                        ui.close_menu();
                    }
                    if ui.button(t(lang, "scan")).clicked() {
                        self.start_scan(ui.ctx());
                        ui.close_menu();
                    }
                    if ui.button(t(lang, "scanall")).clicked() {
                        self.all_drives = true;
                        self.start_scan(ui.ctx());
                        ui.close_menu();
                    }
                    ui.separator();
                    let can_live = self.data.as_ref().is_some_and(|d| !d.volumes.is_empty());
                    let mut on = !self.live.is_empty();
                    if ui.add_enabled(can_live, egui::Checkbox::new(&mut on, t(lang, "live"))).clicked() {
                        self.toggle_live(ui.ctx());
                        ui.close_menu();
                    }
                });
            });
            ui.add_enabled_ui(has_data && !scanning, |ui| {
                ui.menu_button(t(lang, "file"), |ui| {
                    if ui.button(t(lang, "exportas")).clicked() {
                        ui.close_menu();
                        self.export(false);
                    }
                    let any_selected = self.current_rows().1.len() > 0;
                    if ui.add_enabled(any_selected, egui::Button::new(t(lang, "exportselected"))).clicked() {
                        ui.close_menu();
                        self.export(true);
                    }
                });
            });
            ui.add_enabled_ui(has_data && !scanning, |ui| {
                ui.menu_button(t(lang, "layout"), |ui| {
                    if ui.radio(self.layout == Layout::Grid, t(lang, "datagrid")).clicked() {
                        self.layout = Layout::Grid;
                        ui.close_menu();
                    }
                    if ui.radio(self.layout == Layout::Tree, t(lang, "directorytree")).clicked() {
                        self.layout = Layout::Tree;
                        ui.close_menu();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.show_timeline, t(lang, "timeline")).changed() {
                        self.grid.show_timeline = self.show_timeline;
                    }
                });
            });
            ui.menu_button(t(lang, "language"), |ui| {
                for l in Lang::ALL {
                    if ui.radio(self.lang == l, l.native_name()).clicked() {
                        self.set_lang(l);
                        ui.close_menu();
                    }
                }
            });
            if ui.button(t(lang, "info")).clicked() {
                self.about_window.open = true;
            }
        });
    }

    fn status_bar(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        ui.horizontal_wrapped(|ui| {
            let dots = || "...".to_string();
            let r = self.data.as_ref().map(|d| &d.result);
            let field = |ui: &mut egui::Ui, key: &'static str, value: String| {
                ui.label(t(lang, key));
                ui.strong(value);
                ui.separator();
            };
            field(ui, "countfirst", r.and_then(|r| r.oldest_timestamp()).map(model::format_filetime).unwrap_or_else(dots));
            field(ui, "countall", r.map(|r| r.entries.len().to_string()).unwrap_or_else(dots));
            field(ui, "countfiles", r.map(|r| r.file_count().to_string()).unwrap_or_else(dots));
            field(ui, "countdirectories", r.map(|r| r.dirs.len().to_string()).unwrap_or_else(dots));
            if let Some(r) = r {
                let per_drive = |f: &dyn Fn(&model::VolumeInfo) -> String| {
                    if r.volumes.len() == 1 {
                        f(&r.volumes[0])
                    } else {
                        r.volumes.iter().map(|v| format!("{}: {}", v.letter, f(v))).collect::<Vec<_>>().join("  ")
                    }
                };
                field(ui, "journalid", per_drive(&|v| format!("{:#x}", v.journal.journal_id)));
                field(ui, "maxsize", per_drive(&|v| format!("{:.1} MB", v.journal.maximum_size as f64 / 1_048_576.0)));
            }
            let drive_text = if self.all_drives {
                t(lang, "alldrives").to_string()
            } else {
                self.selected_drive.map(|c| format!("{c}:\\")).unwrap_or_else(|| t(lang, "nodrive").to_string())
            };
            field(ui, "selecteddrive", drive_text);
            if !self.live.is_empty() {
                let (r, _) = ui.allocate_exact_size(egui::vec2(10.0, 10.0), egui::Sense::hover());
                ui.painter().circle_filled(r.center(), 4.0, egui::Color32::from_rgb(80, 210, 100));
                ui.label(t(lang, "liveon"));
                ui.separator();
            }
            if let Some(n) = &self.notice {
                ui.label(n);
            }
        });
    }

    fn status_screen(&mut self, ui: &mut egui::Ui) {
        let lang = self.lang;
        let rect = ui.max_rect();
        match &self.background {
            Some(tex) => paint_background(ui, rect, tex),
            None => paint_emblem(ui, rect),
        }

        let phase = if self.scan.is_none() && self.data.is_none() && self.phase == 0 { 0 } else { self.phase };
        let (title_key, desc_key) = status_keys(phase);
        ui.vertical_centered(|ui| {
            ui.add_space((rect.height() / 2.0 - 70.0).max(0.0));
            let title = egui::RichText::new(t(lang, title_key)).size(24.0);
            ui.label(if phase < 0 { title.color(ui.visuals().error_fg_color) } else { title });
            ui.label(t(lang, desc_key));
            if let (Some((d, pos, count)), true) = (self.scan_drive, self.scan.is_some()) {
                ui.strong(format!("{d}:\\  ({pos}/{count})"));
            }
            if let Some(err) = self.error {
                ui.label(egui::RichText::new(err.to_string()).color(ui.visuals().error_fg_color));
            }
            if self.scan.is_some() || phase > 0 {
                let overall = if phase >= 6 {
                    1.0
                } else if phase > 0 {
                    ((phase - 1) as f32 + self.fraction) / 5.0
                } else {
                    0.0
                };
                ui.add_space(6.0);
                ui.add(egui::ProgressBar::new(overall).desired_width(320.0).show_percentage());
            }
            if !self.is_admin {
                ui.add_space(10.0);
                ui.label(egui::RichText::new(t(lang, "notadmin")).color(ui.visuals().warn_fg_color));
            }
        });
    }
}

fn status_keys(phase: i32) -> (&'static str, &'static str) {
    match phase {
        1 => ("statustitle1", "statusdesc1"),
        2 => ("statustitle2", "statusdesc2"),
        3 => ("statustitle3", "statusdesc3"),
        4 => ("statustitle4", "statusdesc4"),
        5 => ("statustitle5", "statusdesc5"),
        6 => ("statustitle6", "statusdesc6"),
        -1 => ("statustitle-1", "statusdesc-1"),
        -2 => ("statustitle-2", "statusdesc-2"),
        -3 => ("statustitle-3", "statusdesc-3"),
        -4 => ("statustitle-4", "statusdesc-4"),
        -5 => ("statustitle-5", "statusdesc-5"),
        _ => ("statustitle0", "statusdesc0"),
    }
}

/// Dark theme close to the C# WinForms look: Segoe UI, square corners,
/// Windows blue selection, DataGridView-like greys.
fn setup_style(ctx: &egui::Context) {
    use egui::{Color32, CornerRadius, FontFamily, FontId, Stroke, TextStyle};

    let mut fonts = egui::FontDefinitions::default();
    for (name, file) in [("segoe", "segoeui.ttf"), ("segoe_symbol", "seguisym.ttf")] {
        if let Ok(bytes) = std::fs::read(format!(r"C:\Windows\Fonts\{file}")) {
            fonts.font_data.insert(name.into(), Arc::new(egui::FontData::from_owned(bytes)));
        }
    }
    // Segoe UI first, its symbol font next, egui's bundled fonts as fallback.
    let mut pos = 0;
    for name in ["segoe", "segoe_symbol"] {
        if fonts.font_data.contains_key(name) {
            fonts.families.entry(FontFamily::Proportional).or_default().insert(pos, name.into());
            pos += 1;
        }
    }
    ctx.set_fonts(fonts);

    let mut v = egui::Visuals::dark();
    v.panel_fill = Color32::from_rgb(30, 30, 30);
    v.window_fill = Color32::from_rgb(37, 37, 38);
    v.extreme_bg_color = Color32::from_rgb(24, 24, 24);
    v.faint_bg_color = Color32::from_rgb(38, 38, 38);
    v.selection.bg_fill = Color32::from_rgb(0, 120, 215);
    v.selection.stroke = Stroke::new(1.0_f32, Color32::WHITE);
    v.hyperlink_color = Color32::from_rgb(90, 165, 255);
    v.window_corner_radius = CornerRadius::same(3);
    v.menu_corner_radius = CornerRadius::same(2);
    for w in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        w.corner_radius = CornerRadius::same(2);
    }
    v.widgets.noninteractive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(63, 63, 70));
    v.widgets.inactive.weak_bg_fill = Color32::from_rgb(51, 51, 55);
    v.widgets.inactive.bg_fill = Color32::from_rgb(51, 51, 55);
    v.widgets.inactive.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(67, 67, 70));
    v.widgets.hovered.weak_bg_fill = Color32::from_rgb(62, 62, 66);
    v.widgets.hovered.bg_fill = Color32::from_rgb(62, 62, 66);
    v.widgets.hovered.bg_stroke = Stroke::new(1.0_f32, Color32::from_rgb(0, 120, 215));
    v.widgets.active.bg_fill = Color32::from_rgb(0, 100, 180);
    v.widgets.active.weak_bg_fill = Color32::from_rgb(0, 100, 180);
    ctx.set_visuals(v);

    ctx.style_mut(|s| {
        s.text_styles.insert(TextStyle::Body, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Button, FontId::proportional(13.5));
        s.text_styles.insert(TextStyle::Small, FontId::proportional(11.0));
        s.text_styles.insert(TextStyle::Heading, FontId::proportional(22.0));
        s.spacing.item_spacing = egui::vec2(6.0, 4.0);
        s.spacing.button_padding = egui::vec2(8.0, 3.0);
    });
}

/// Background picture fitted to the right edge, like the C# form drew it.
fn paint_background(ui: &egui::Ui, rect: egui::Rect, tex: &egui::TextureHandle) {
    let [w, h] = tex.size().map(|v| v as f32);
    let aspect = w / h;
    let (mut iw, mut ih) = (rect.height() * aspect, rect.height());
    if iw > rect.width() {
        iw = rect.width();
        ih = iw / aspect;
    }
    let margin = rect.width() * 0.06;
    let img_rect = egui::Rect::from_min_size(
        egui::pos2(rect.right() - iw - margin, rect.center().y - ih / 2.0),
        egui::vec2(iw, ih),
    );
    let uv = egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0));
    ui.painter().image(tex.id(), img_rect, uv, egui::Color32::WHITE);
}

/// Faint "journal under a magnifier" drawing behind the status text, used
/// when no background picture is embedded.
fn paint_emblem(ui: &egui::Ui, rect: egui::Rect) {
    use egui::{pos2, vec2, Stroke};
    let size = (rect.height() * 0.55).min(rect.width() * 0.4);
    if size < 80.0 {
        return;
    }
    let color = ui.visuals().weak_text_color().gamma_multiply(0.25);
    let painter = ui.painter();
    let width = (size / 60.0).max(2.0);

    let page = egui::Rect::from_center_size(
        pos2(rect.right() - size * 0.7, rect.center().y),
        vec2(size * 0.62, size * 0.8),
    );
    painter.rect_stroke(page, 10.0, Stroke::new(width, color), egui::StrokeKind::Inside);
    for i in 0..6 {
        let y = page.top() + page.height() * (0.17 + 0.11 * i as f32);
        let end = if i % 2 == 0 { 0.15 } else { 0.4 };
        painter.line_segment(
            [pos2(page.left() + page.width() * 0.15, y), pos2(page.right() - page.width() * end, y)],
            Stroke::new(width, color),
        );
    }

    let r = size * 0.17;
    let lens = pos2(page.right() - size * 0.04, page.bottom() - size * 0.14);
    painter.circle_filled(lens, r, ui.visuals().panel_fill);
    painter.circle_stroke(lens, r, Stroke::new(width * 1.6, color));
    let d = r * std::f32::consts::FRAC_1_SQRT_2;
    painter.line_segment([lens + vec2(d, d), lens + vec2(d * 2.3, d * 2.3)], Stroke::new(width * 3.0, color));
}

pub(crate) fn decode_png(bytes: &[u8]) -> Option<egui::ColorImage> {
    let img = image::load_from_memory_with_format(bytes, image::ImageFormat::Png).ok()?.to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan();
        self.poll_live();

        // Ctrl + mouse wheel zooms like Ctrl +/-, which egui already handles.
        let zoom = ctx.input(|i| i.zoom_delta());
        if zoom != 1.0 {
            ctx.set_zoom_factor((ctx.zoom_factor() * zoom).clamp(0.6, 2.5));
        }
        if self.background.is_none() {
            if let Some(img) = self.background_pending.lock().unwrap().take() {
                self.background = Some(ctx.load_texture("background", img, egui::TextureOptions::LINEAR));
            }
        }

        egui::TopBottomPanel::top("menu").show(ctx, |ui| self.menu_bar(ui));
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| self.status_bar(ui));

        let mut actions = Actions::default();
        let lang = self.lang;
        egui::CentralPanel::default().show(ctx, |ui| match (&self.data, self.layout) {
            (Some(data), Layout::Grid) => self.grid.show(ui, lang, data, &self.reasons, &mut actions),
            (Some(data), Layout::Tree) => {
                let tree = self.tree.get_or_insert_with(|| TreeState::build(data));
                tree.show(ui, lang, data, &self.reasons, &mut actions);
            }
            _ => self.status_screen(ui),
        });

        match self.drive_window.show(ctx, lang) {
            Some(DriveChoice::One(letter)) => {
                self.selected_drive = Some(letter);
                self.all_drives = false;
            }
            Some(DriveChoice::All) => self.all_drives = true,
            None => {}
        }
        self.about_window.show(ctx, lang);
        if let Some(data) = &self.data {
            for w in &mut self.info_windows {
                w.show(ctx, lang, data, &self.reasons, &mut actions);
            }
        }
        self.info_windows.retain(|w| w.open);

        self.apply_actions(ctx, actions);
    }
}
