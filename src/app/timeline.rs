//! Changes-per-hour bar chart above the grid. Clicking a bar filters the
//! grid to that time span; clicking it again clears the filter.

use super::grid::short_time;
use crate::model::Entry;
use eframe::egui;
use std::collections::BTreeMap;

const HOUR: i64 = 3600 * 10_000_000;
const HEIGHT: f32 = 64.0;

#[derive(Default)]
pub(crate) struct Timeline {
    /// hour number (FILETIME / HOUR) -> entries in that hour
    hours: BTreeMap<i64, u32>,
}

impl Timeline {
    pub fn build(entries: &[Entry]) -> Timeline {
        let mut t = Timeline::default();
        t.add(entries);
        t
    }

    pub fn add(&mut self, entries: &[Entry]) {
        for e in entries {
            *self.hours.entry(e.timestamp.div_euclid(HOUR)).or_default() += 1;
        }
    }

    /// Draws the chart. Returns `Some(new_range)` when a bar was clicked.
    pub fn show(&self, ui: &mut egui::Ui, selected: Option<(i64, i64)>) -> Option<Option<(i64, i64)>> {
        let (Some((&first, _)), Some((&last, _))) = (self.hours.first_key_value(), self.hours.last_key_value())
        else {
            return None;
        };
        let (rect, resp) =
            ui.allocate_exact_size(egui::vec2(ui.available_width(), HEIGHT), egui::Sense::click());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 2.0, ui.visuals().extreme_bg_color);

        // Merge hours so each bar is at least ~4 px wide.
        let span = last - first + 1;
        let max_bars = ((rect.width() / 4.0) as i64).max(1);
        let group = (span + max_bars - 1) / max_bars;
        let bars = ((span + group - 1) / group) as usize;
        let mut counts = vec![0u32; bars];
        for (&h, &n) in &self.hours {
            counts[((h - first) / group) as usize] += n;
        }
        let max = counts.iter().copied().max().unwrap_or(1).max(1) as f32;
        let bar_w = rect.width() / bars as f32;

        let bucket_range = |i: usize| {
            let start = (first + i as i64 * group) * HOUR;
            (start, start + group * HOUR)
        };
        let hovered = resp.hover_pos().map(|p| (((p.x - rect.left()) / bar_w) as usize).min(bars - 1));

        let base = egui::Color32::from_rgb(0, 120, 215);
        for (i, &n) in counts.iter().enumerate() {
            if n == 0 {
                continue;
            }
            let h = (n as f32 / max).sqrt() * (HEIGHT - 6.0); // sqrt keeps small hours visible
            let x = rect.left() + i as f32 * bar_w;
            let bar = egui::Rect::from_min_max(
                egui::pos2(x + 0.5, rect.bottom() - h - 2.0),
                egui::pos2(x + bar_w.max(1.5) - 0.5, rect.bottom() - 2.0),
            );
            let is_sel = selected == Some(bucket_range(i));
            let color = if is_sel {
                egui::Color32::from_rgb(255, 180, 60)
            } else if hovered == Some(i) {
                egui::Color32::from_rgb(80, 170, 255)
            } else if selected.is_some() {
                base.gamma_multiply(0.45)
            } else {
                base
            };
            painter.rect_filled(bar, 0.0, color);
        }

        let mut result = None;
        if let Some(i) = hovered {
            let (a, b) = bucket_range(i);
            let n = counts[i];
            resp.clone().on_hover_text_at_pointer(format!("{} – {}\n{n}", short_time(a), short_time(b)));
            if resp.clicked() {
                result = Some(if selected == Some((a, b)) { None } else { Some((a, b)) });
            }
        }

        let small = egui::FontId::proportional(10.5);
        let color = ui.visuals().weak_text_color();
        painter.text(rect.left_top() + egui::vec2(4.0, 2.0), egui::Align2::LEFT_TOP, short_time(first * HOUR), small.clone(), color);
        painter.text(
            rect.right_top() + egui::vec2(-4.0, 2.0),
            egui::Align2::RIGHT_TOP,
            short_time((last + 1) * HOUR),
            small,
            color,
        );
        result
    }
}
