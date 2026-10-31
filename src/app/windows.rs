//! Secondary windows: drive selection, entry information and credits.

use super::table::{self, Selection, Sort, TableOpts};
use super::{decode_png, Actions, Data, Reasons};
use crate::i18n::{t, Lang};
use crate::model::{self, FileStory};
use crate::ntfs::{self, DriveInfo};
use eframe::egui;

pub(crate) enum DriveChoice {
    One(char),
    All,
}

#[derive(Default)]
pub(crate) struct DriveWindow {
    open: bool,
    drives: Vec<DriveInfo>,
    /// Index into `drives`, or `drives.len()` for "all drives".
    picked: Option<usize>,
}

impl DriveWindow {
    pub fn show_window(&mut self) {
        self.drives = ntfs::list_ntfs_drives();
        self.picked = None;
        self.open = true;
    }

    /// Returns the choice when the user presses Select.
    pub fn show(&mut self, ctx: &egui::Context, lang: Lang) -> Option<DriveChoice> {
        let mut chosen = None;
        let mut open = self.open;
        egui::Window::new(t(lang, "formdrive"))
            .id(egui::Id::new("drive_window"))
            .open(&mut open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_top(|ui| {
                    ui.vertical(|ui| {
                        ui.set_min_width(150.0);
                        for (i, d) in self.drives.iter().enumerate() {
                            let text = if d.label.is_empty() { d.root() } else { format!("{} ({})", d.root(), d.label) };
                            if ui.selectable_label(self.picked == Some(i), text).clicked() {
                                self.picked = Some(i);
                            }
                        }
                        if self.drives.len() > 1 {
                            ui.separator();
                            let all = self.drives.len();
                            let text = format!("{} ({})", t(lang, "alldrives"), all);
                            if ui.selectable_label(self.picked == Some(all), text).clicked() {
                                self.picked = Some(all);
                            }
                        }
                    });
                    if let Some(d) = self.picked.and_then(|i| self.drives.get(i)) {
                        // A vertical separator here stretches to the screen height.
                        ui.add_space(12.0);
                        egui::Grid::new("drive_info").num_columns(2).show(ui, |ui| {
                            let gb = |b: u64| format!("{:.1} GB", b as f64 / 1e9);
                            for (key, value) in [
                                ("name", d.root()),
                                ("format", d.format.clone()),
                                ("type", d.kind.to_string()),
                                ("root", d.root()),
                                ("totalsize", gb(d.total_size)),
                                ("totalfreespace", gb(d.total_free)),
                            ] {
                                ui.label(t(lang, key));
                                ui.strong(value);
                                ui.end_row();
                            }
                        });
                    }
                });
                ui.separator();
                let can_select = self.picked.is_some();
                if ui.add_enabled(can_select, egui::Button::new(t(lang, "select"))).clicked() {
                    chosen = match self.picked {
                        Some(i) if i == self.drives.len() => Some(DriveChoice::All),
                        Some(i) => self.drives.get(i).map(|d| DriveChoice::One(d.letter)),
                        None => None,
                    };
                }
            });
        self.open = open && chosen.is_none();
        chosen
    }
}

pub(crate) struct InfoWindow {
    pub idx: u32,
    pub open: bool,
    current_path: Option<String>,
    story: FileStory,
    rows: Vec<u32>,
    sort: Sort,
    selection: Selection,
}

impl InfoWindow {
    pub fn new(idx: u32, data: &Data) -> InfoWindow {
        let e = &data.result.entries[idx as usize];
        let current_path = data.handle(e).and_then(|v| v.resolve_path(e.file_ref));
        let rows = data.result.file_history(e).to_vec();
        let story = FileStory::of(&data.result.entries, &rows);
        InfoWindow { idx, open: true, current_path, story, rows, sort: Sort::default(), selection: Selection::default() }
    }

    fn id(&self) -> egui::Id {
        egui::Id::new(("info_window", self.idx))
    }

    pub fn layer_id(&self) -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, self.id())
    }

    pub fn show(&mut self, ctx: &egui::Context, lang: Lang, data: &Data, reasons: &Reasons, actions: &mut Actions) {
        let id = self.id();
        let InfoWindow { idx, open, current_path, story, rows, sort, selection } = self;
        let e = &data.result.entries[*idx as usize];
        egui::Window::new(format!("{} - {}", t(lang, "formentryinfo"), e.name))
            .id(id)
            .open(open)
            .default_size([820.0, 460.0])
            .show(ctx, |ui| {
                egui::Grid::new(("info_grid", *idx)).num_columns(2).spacing([16.0, 4.0]).show(ui, |ui| {
                    ui.label("USN");
                    ui.strong(e.usn.to_string());
                    ui.end_row();
                    ui.label(t(lang, "name"));
                    ui.strong(&e.name);
                    ui.end_row();
                    ui.label(t(lang, "drive"));
                    ui.strong(format!("{}:\\", data.result.drive(e)));
                    ui.end_row();
                    ui.label(t(lang, "fileid"));
                    ui.strong(e.file_ref.to_string());
                    ui.end_row();
                    ui.label(t(lang, "currentfile"));
                    match current_path {
                        Some(p) => ui.strong(p.as_str()),
                        None => ui.label(egui::RichText::new(t(lang, "filedeleted")).color(ui.visuals().error_fg_color)),
                    };
                    ui.end_row();
                    ui.label(t(lang, "directory"));
                    let dir = data.result.dir_path(e);
                    if data.result.dir_exists(e) {
                        ui.strong(dir);
                    } else {
                        ui.strong(format!("{dir} {}", t(lang, "deleteddir")));
                    }
                    ui.end_row();
                    ui.label(t(lang, "reason"));
                    ui.strong(reasons.get(e.reason));
                    ui.end_row();
                    ui.label(t(lang, "date"));
                    ui.strong(model::format_filetime(e.timestamp));
                    ui.end_row();
                    if story.names.len() > 1 {
                        ui.label(t(lang, "namehistory"));
                        ui.strong(story.names.join("  →  "));
                        ui.end_row();
                    }
                    if let Some(c) = story.created {
                        ui.label(t(lang, "createdat"));
                        ui.strong(model::format_filetime(c));
                        ui.end_row();
                    }
                    if let Some(d) = story.deleted {
                        ui.label(t(lang, "deletedat"));
                        ui.strong(model::format_filetime(d));
                        ui.end_row();
                    }
                    if let Some(l) = story.lifetime() {
                        ui.label(t(lang, "lifetime"));
                        let text = egui::RichText::new(model::format_duration(l)).strong();
                        // Under a minute is what the "short-lived" filter looks for.
                        ui.label(if l <= 60 * 10_000_000 { text.color(ui.visuals().warn_fg_color) } else { text });
                        ui.end_row();
                    }
                });
                ui.separator();
                let opts = TableOpts {
                    id: egui::Id::new(("info_table", *idx)),
                    show_dir: true,
                    allow_info: false,
                    max_height: ui.available_height().max(150.0),
                    keyboard: false,
                };
                if let Some(col) = table::entry_table(ui, lang, data, reasons, rows, *sort, selection, opts, actions) {
                    table::toggle_sort(sort, col);
                    table::sort_rows(rows, data, reasons, *sort);
                }
            });
    }
}

struct Person {
    name: &'static str,
    desc_key: &'static str,
    dreamincode: Option<&'static str>,
    github: Option<&'static str>,
    youtube: Option<&'static str>,
}

const PEOPLE: [Person; 5] = [
    Person {
        name: "ponei",
        desc_key: "poneidesc",
        dreamincode: None,
        github: Some("ponei"),
        youtube: Some("channel/UC8XWpvtxj23ks_qdtXx_y4g"),
    },
    Person { name: "Lordyzagat", desc_key: "lordyzagatdesc", dreamincode: None, github: None, youtube: Some("c/Lordyzagat") },
    Person { name: "nolanblew", desc_key: "nolandesc", dreamincode: None, github: Some("nolanblew/"), youtube: None },
    Person {
        name: "StCroixSkipper",
        desc_key: "skipperdesc",
        dreamincode: Some("forums/blog/1017-stcroixskippers"),
        github: None,
        youtube: None,
    },
    Person { name: "duvo", desc_key: "duvodesc", dreamincode: None, github: None, youtube: None },
];

pub(crate) struct AboutWindow {
    pub open: bool,
    icons: [Option<egui::TextureHandle>; 3], // dreamincode, github, youtube
}

impl AboutWindow {
    pub fn new(ctx: &egui::Context) -> AboutWindow {
        let load = |name: &str, bytes: &[u8]| {
            decode_png(bytes).map(|img| ctx.load_texture(name, img, egui::TextureOptions::LINEAR))
        };
        AboutWindow {
            open: false,
            icons: [
                load("dreamincode", include_bytes!("../../assets/dreamincode.png")),
                load("github", include_bytes!("../../assets/github.png")),
                load("youtube", include_bytes!("../../assets/youtube.png")),
            ],
        }
    }

    pub fn show(&mut self, ctx: &egui::Context, lang: Lang) {
        let icons = &self.icons;
        egui::Window::new(t(lang, "info"))
            .id(egui::Id::new("about_window"))
            .open(&mut self.open)
            .resizable(false)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.heading("JournalTrace");
                    ui.label(format!("{} {}", t(lang, "version"), env!("CARGO_PKG_VERSION")));
                    ui.hyperlink("https://github.com/ponei/JournalTrace");
                });
                ui.separator();
                egui::Grid::new("people").num_columns(3).spacing([16.0, 8.0]).show(ui, |ui| {
                    for p in &PEOPLE {
                        ui.strong(p.name);
                        ui.label(t(lang, p.desc_key));
                        ui.horizontal(|ui| {
                            let links = [
                                (0, "https://www.dreamincode.net/", p.dreamincode),
                                (1, "https://github.com/", p.github),
                                (2, "https://www.youtube.com/", p.youtube),
                            ];
                            for (icon, base, path) in links {
                                let Some(path) = path else { continue };
                                let url = format!("{base}{path}");
                                let clicked = match &icons[icon] {
                                    Some(tex) => ui
                                        .add(egui::ImageButton::new(egui::load::SizedTexture::new(tex.id(), [20.0, 20.0])))
                                        .on_hover_text(&url)
                                        .clicked(),
                                    None => ui.link(&url).clicked(),
                                };
                                if clicked {
                                    ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                                }
                            }
                        });
                        ui.end_row();
                    }
                });
            });
    }
}
