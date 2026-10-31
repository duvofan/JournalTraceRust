//! Directory Tree layout: directories on the left, changes inside the
//! selected one on the right.

use super::table::{self, Selection, Sort, TableOpts};
use super::{Actions, Data, Reasons};
use crate::i18n::{t, Lang};
use crate::model::Ref;
use eframe::egui;
use std::collections::{HashMap, HashSet};

struct Node {
    name: String,
    /// Full path, used to keep the selection when the tree is rebuilt.
    path: String,
    children: Vec<u32>,
    /// Parent references whose path ends at this node. Usually one; two when
    /// a deleted directory was recreated under the same name.
    refs: Vec<Ref>,
    exists: bool,
}

pub(crate) struct TreeState {
    nodes: Vec<Node>,
    roots: Vec<u32>,
    selected_node: Option<u32>,
    rows: Vec<u32>,
    sort: Sort,
    pub selection: Selection,
}

impl TreeState {
    /// O(n) build with a (parent, name) map; the C# version scanned every
    /// child list linearly for each path part.
    pub fn build(data: &Data) -> TreeState {
        let mut nodes: Vec<Node> = Vec::new();
        let mut roots: Vec<u32> = Vec::new();
        let mut index: HashMap<(u32, &str), u32> = HashMap::new();
        const ROOT: u32 = u32::MAX;

        for (&dir_ref, info) in &data.result.dirs {
            let mut parent = ROOT;
            for part in info.path.split('\\').filter(|p| !p.is_empty()) {
                let idx = match index.get(&(parent, part)) {
                    Some(&i) => i,
                    None => {
                        let i = nodes.len() as u32;
                        let path = if parent == ROOT {
                            part.to_string()
                        } else {
                            format!("{}\\{}", nodes[parent as usize].path, part)
                        };
                        nodes.push(Node {
                            name: part.to_string(),
                            path,
                            children: Vec::new(),
                            refs: Vec::new(),
                            exists: false,
                        });
                        index.insert((parent, part), i);
                        if parent == ROOT {
                            roots.push(i);
                        } else {
                            nodes[parent as usize].children.push(i);
                        }
                        i
                    }
                };
                parent = idx;
            }
            if parent != ROOT {
                let n = &mut nodes[parent as usize];
                n.refs.push(dir_ref);
                n.exists |= info.exists;
            }
        }

        let key = |nodes: &[Node], i: &u32| nodes[*i as usize].name.to_lowercase();
        let mut order: Vec<Vec<u32>> = nodes.iter().map(|n| n.children.clone()).collect();
        for list in &mut order {
            list.sort_by_cached_key(|i| key(&nodes, i));
        }
        for (n, list) in nodes.iter_mut().zip(order) {
            n.children = list;
        }
        roots.sort_by_cached_key(|i| key(&nodes, i));

        TreeState {
            nodes,
            roots,
            selected_node: None,
            rows: Vec::new(),
            sort: Sort::default(),
            selection: Selection::default(),
        }
    }

    /// Rebuilds after live monitoring found new directories, keeping the
    /// selected directory, sort order and (through path-based ids) which
    /// nodes are expanded.
    pub fn rebuild(&self, data: &Data, reasons: &Reasons) -> TreeState {
        let mut tree = TreeState::build(data);
        tree.sort = self.sort;
        if let Some(path) = self.selected_node.map(|n| &self.nodes[n as usize].path) {
            if let Some(i) = tree.nodes.iter().position(|n| &n.path == path) {
                tree.select(i as u32, data, reasons);
            }
        }
        tree
    }

    pub fn rows(&self) -> &[u32] {
        &self.rows
    }

    pub fn resort(&mut self, data: &Data, reasons: &Reasons) {
        table::sort_rows(&mut self.rows, data, reasons, self.sort);
    }

    /// Adds live rows that belong to the selected directory.
    pub fn append(&mut self, data: &Data, reasons: &Reasons, new: std::ops::Range<u32>) {
        let Some(node) = self.selected_node else { return };
        let refs: HashSet<Ref> = self.nodes[node as usize].refs.iter().copied().collect();
        let before = self.rows.len();
        self.rows.extend(new.filter(|&i| refs.contains(&data.result.entries[i as usize].dir_key())));
        if self.rows.len() != before {
            self.resort(data, reasons);
        }
    }

    fn select(&mut self, node: u32, data: &Data, reasons: &Reasons) {
        self.selected_node = Some(node);
        self.selection.clear();
        self.rows = self.nodes[node as usize]
            .refs
            .iter()
            .filter_map(|r| data.result.by_dir.get(r))
            .flatten()
            .copied()
            .collect();
        self.resort(data, reasons);
    }

    fn show_node(&self, ui: &mut egui::Ui, idx: u32, parent_id: egui::Id, clicked: &mut Option<u32>) {
        let node = &self.nodes[idx as usize];
        let is_sel = self.selected_node == Some(idx);
        let mut text = egui::RichText::new(&node.name);
        if node.refs.is_empty() {
            text = text.weak();
        } else if !node.exists {
            text = text.italics().color(ui.visuals().warn_fg_color);
        }
        if node.children.is_empty() {
            if ui.selectable_label(is_sel, text).clicked() {
                *clicked = Some(idx);
            }
            return;
        }
        let id = parent_id.with(&node.name);
        egui::collapsing_header::CollapsingState::load_with_default_open(ui.ctx(), id, false)
            .show_header(ui, |ui| {
                if ui.selectable_label(is_sel, text).clicked() {
                    *clicked = Some(idx);
                }
            })
            .body(|ui| {
                for &c in &node.children {
                    self.show_node(ui, c, id, clicked);
                }
            });
    }

    pub fn show(&mut self, ui: &mut egui::Ui, lang: Lang, data: &Data, reasons: &Reasons, actions: &mut Actions) {
        let mut clicked = None;
        egui::SidePanel::left("tree_panel").resizable(true).default_width(320.0).show_inside(ui, |ui| {
            egui::ScrollArea::both().auto_shrink([false, false]).show(ui, |ui| {
                let root_id = egui::Id::new("tree_root");
                for &r in &self.roots {
                    self.show_node(ui, r, root_id, &mut clicked);
                }
            });
        });
        if let Some(node) = clicked {
            self.select(node, data, reasons);
        }

        egui::CentralPanel::default().show_inside(ui, |ui| {
            if self.selected_node.is_some() && self.rows.is_empty() {
                ui.label(t(lang, "nodata"));
                return;
            }
            if self.selected_node.is_none() {
                return;
            }
            let opts = TableOpts {
                id: egui::Id::new("tree_table"),
                show_dir: false,
                allow_info: true,
                max_height: ui.available_height(),
                keyboard: true,
            };
            if let Some(col) =
                table::entry_table(ui, lang, data, reasons, &self.rows, self.sort, &mut self.selection, opts, actions)
            {
                table::toggle_sort(&mut self.sort, col);
                self.resort(data, reasons);
            }
        });
    }
}
