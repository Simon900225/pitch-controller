use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2, pos2};

use crate::engine::Snapshot;
use crate::profile::PitchBand;
use crate::ui::hue_color;

const DEFAULT_W: f32 = 110.0;
const DEFAULT_H: f32 = 320.0;

pub fn show(
    ui: &mut egui::Ui,
    snapshot: &Snapshot,
    bands: &[PitchBand],
    opacity: f32,
    edit_mode: bool,
) {
    let rect = ui.max_rect();
    let scale = {
        let fitted = (rect.width() / DEFAULT_W).min(rect.height() / DEFAULT_H);
        if rect.width() <= 1.0 || rect.height() <= 1.0 {
            1.0
        } else {
            fitted.clamp(0.75, 3.0)
        }
    };
    ui.multiply_opacity(opacity);
    let painter = ui.painter();
    let radius = CornerRadius::same((14.0 * scale.min(1.8)) as u8);
    let fill = Color32::from_black_alpha((0.65 * 255.0) as u8);
    painter.rect_filled(rect, radius, fill);
    painter.rect_stroke(
        rect,
        radius,
        Stroke::new(
            if edit_mode { 2.0 } else { 1.0 },
            if edit_mode {
                Color32::from_rgb(230, 200, 40)
            } else {
                Color32::from_white_alpha(40)
            },
        ),
        egui::StrokeKind::Inside,
    );

    let inner = rect.shrink(10.0 * scale);
    painter.text(
        pos2(inner.center().x, inner.top() + 8.0 * scale),
        egui::Align2::CENTER_CENTER,
        "PITCH",
        egui::FontId::proportional(10.0 * scale),
        Color32::from_white_alpha(180),
    );

    let meter = Rect::from_min_max(
        pos2(inner.left(), inner.top() + 18.0 * scale),
        pos2(inner.right(), inner.bottom() - 36.0 * scale),
    );
    paint_meter(ui, meter, snapshot, bands, scale);

    let note = snapshot
        .note
        .map(crate::pitch_math::note_name)
        .unwrap_or_else(|| "—".to_string());
    let (status, status_color) = status_label(snapshot, bands);
    painter.text(
        pos2(inner.center().x, inner.bottom() - 22.0 * scale),
        egui::Align2::CENTER_CENTER,
        note,
        egui::FontId::monospace(14.0 * scale),
        Color32::WHITE,
    );
    painter.text(
        pos2(inner.center().x, inner.bottom() - 6.0 * scale),
        egui::Align2::CENTER_CENTER,
        status,
        egui::FontId::proportional(10.0 * scale),
        status_color,
    );

    if edit_mode {
        let knob = 16.0;
        for corner in corners(rect, knob) {
            painter.circle_filled(corner, 5.0, Color32::from_rgb(240, 210, 60));
        }
        let move_response = ui.interact(rect, ui.id().with("overlay-move"), Sense::drag());
        if move_response.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let directions = [
            (corners(rect, knob)[0], egui::ResizeDirection::NorthWest),
            (corners(rect, knob)[1], egui::ResizeDirection::NorthEast),
            (corners(rect, knob)[2], egui::ResizeDirection::SouthWest),
            (corners(rect, knob)[3], egui::ResizeDirection::SouthEast),
        ];
        for (index, (center, direction)) in directions.into_iter().enumerate() {
            let knob_rect = Rect::from_center_size(center, Vec2::splat(knob));
            let response = ui.interact(
                knob_rect,
                ui.id().with(("overlay-knob", index)),
                Sense::drag(),
            );
            if response.drag_started() {
                ui.ctx()
                    .send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
            }
        }
    }
}

fn paint_meter(ui: &egui::Ui, rect: Rect, snapshot: &Snapshot, bands: &[PitchBand], scale: f32) {
    let painter = ui.painter();
    painter.rect_filled(
        rect,
        CornerRadius::same((10.0 * scale.min(1.6)) as u8),
        Color32::from_black_alpha(160),
    );
    let (low, high) = note_span(bands);
    let span = (high - low).max(0.01);
    let count = bands.len().max(1) as f32;
    let gap = 2.0;
    let col_w = ((rect.width() - gap * (count - 1.0)) / count).max(2.0);
    for (index, band) in bands.iter().enumerate() {
        let y_high = y_of(band.high_note, low, span, rect);
        let y_low = y_of(band.low_note, low, span, rect);
        let band_rect = Rect::from_min_max(
            pos2(rect.left() + index as f32 * (col_w + gap), y_high),
            pos2(
                rect.left() + index as f32 * (col_w + gap) + col_w,
                y_low.max(y_high + 2.0),
            ),
        );
        let alpha = if snapshot.active_band_ids.contains(&band.id) {
            0.85
        } else {
            0.35
        };
        painter.rect_filled(band_rect, CornerRadius::same(4), hue_color(band.hue, alpha));
        if band_rect.height() > 28.0 {
            painter.text(
                band_rect.center(),
                egui::Align2::CENTER_CENTER,
                &band.name,
                egui::FontId::proportional((9.0 * scale).clamp(6.0, 12.0)),
                Color32::WHITE,
            );
        }
    }

    if let Some(note) = snapshot.note {
        if note < low {
            edge_marker(painter, rect, false);
        } else if note >= high {
            edge_marker(painter, rect, true);
        } else {
            let y = y_of(note, low, span, rect);
            painter.rect_filled(
                Rect::from_min_max(pos2(rect.left(), y - 1.5), pos2(rect.right(), y + 1.5)),
                CornerRadius::ZERO,
                Color32::from_rgb(0, 210, 220),
            );
        }
    }
}

fn edge_marker(painter: &egui::Painter, rect: Rect, above: bool) {
    let y = if above {
        rect.top() + 14.0
    } else {
        rect.bottom() - 14.0
    };
    painter.text(
        pos2(rect.center().x, y),
        egui::Align2::CENTER_CENTER,
        if above { "▲" } else { "▼" },
        egui::FontId::proportional(16.0),
        Color32::from_rgb(0, 220, 230),
    );
}

fn status_label(snapshot: &Snapshot, bands: &[PitchBand]) -> (String, Color32) {
    let (low, high) = note_span(bands);
    if let Some(note) = snapshot.note {
        if note < low {
            return ("too low".to_string(), Color32::from_rgb(0, 210, 220));
        }
        if note >= high {
            return ("too high".to_string(), Color32::from_rgb(0, 210, 220));
        }
    }
    let names: Vec<_> = bands
        .iter()
        .filter(|band| snapshot.active_band_ids.contains(&band.id))
        .map(|band| band.name.as_str())
        .collect();
    if names.is_empty() {
        ("none".to_string(), Color32::from_white_alpha(200))
    } else {
        (names.join(" + "), Color32::from_white_alpha(200))
    }
}

fn note_span(bands: &[PitchBand]) -> (f64, f64) {
    let low = bands.iter().map(|band| band.low_note).reduce(f64::min);
    let high = bands.iter().map(|band| band.high_note).reduce(f64::max);
    match (low, high) {
        (Some(low), Some(high)) if high - low > 0.01 => (low, high),
        _ => (48.0, 84.0),
    }
}

fn y_of(note: f64, low: f64, span: f64, rect: Rect) -> f32 {
    let t = ((note.clamp(low, low + span) - low) / span) as f32;
    rect.top() + (1.0 - t) * rect.height()
}

fn corners(rect: Rect, inset: f32) -> [Pos2; 4] {
    [
        pos2(rect.left() + inset, rect.top() + inset),
        pos2(rect.right() - inset, rect.top() + inset),
        pos2(rect.left() + inset, rect.bottom() - inset),
        pos2(rect.right() - inset, rect.bottom() - inset),
    ]
}
