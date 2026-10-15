use std::collections::HashSet;

use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, Vec2, pos2, vec2};
use uuid::Uuid;

use crate::engine::PitchPoint;
use crate::pitch_math;
use crate::profile::PitchBand;
use crate::ui::hue_color;

const ABS_LOW: f64 = 36.0;
const ABS_HIGH: f64 = 96.0;
const MIN_SPAN: f64 = 6.0;
const MAX_SPAN: f64 = 60.0;
const MIN_BAND: f64 = 0.5;
const LEADING: f32 = 44.0;
const TRAILING: f32 = 8.0;
const V_INSET: f32 = 8.0;
const GAP: f32 = 6.0;

pub struct EditorState {
    pub view_low: f64,
    pub view_high: f64,
    pub selected: Option<Uuid>,
    move_drag: Option<MoveDrag>,
    pan_drag: Option<PanDrag>,
    fitted_for: Option<Uuid>,
}

struct MoveDrag {
    id: Uuid,
    low: f64,
    high: f64,
    dy: f32,
}

struct PanDrag {
    low: f64,
    high: f64,
    dy: f32,
}

impl EditorState {
    pub fn reset_fit(&mut self) {
        self.fitted_for = None;
    }
}

impl Default for EditorState {
    fn default() -> Self {
        Self {
            view_low: 52.0,
            view_high: 80.0,
            selected: None,
            move_drag: None,
            pan_drag: None,
            fitted_for: None,
        }
    }
}

pub enum EditorAction {
    SetNotes {
        id: Uuid,
        low: f64,
        high: f64,
        persist: bool,
    },
    Add,
    Delete(Uuid),
    /// Move the band by `delta` columns. Negative is left, positive is right.
    Reorder { id: Uuid, delta: i32 },
}

pub fn show(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    profile_id: Uuid,
    bands: &[PitchBand],
    active: &HashSet<Uuid>,
    history: &[PitchPoint],
    now: f64,
    current_note: Option<f64>,
) -> Vec<EditorAction> {
    if state.fitted_for != Some(profile_id) {
        fit(state, bands);
        state.fitted_for = Some(profile_id);
        if state.selected.is_none() {
            state.selected = bands.first().map(|band| band.id);
        }
    }

    let mut actions = Vec::new();
    let frame = egui::Frame::new()
        .fill(Color32::from_white_alpha(8))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(14)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(14));
    frame.show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(egui::RichText::new("Pitch bands").strong().size(15.0));
            if ui.small_button("−").on_hover_text("Zoom out").clicked() {
                zoom(state, 1.25);
            }
            if ui.small_button("+").on_hover_text("Zoom in").clicked() {
                zoom(state, 0.8);
            }
            if ui.small_button("Fit").clicked() {
                fit(state, bands);
            }
            if ui.small_button("Reset").on_hover_text("Show C3–C6").clicked() {
                set_view(state, 48.0, 84.0);
            }
            ui.label(
                egui::RichText::new(format!(
                    "{} – {}",
                    pitch_math::note_name(state.view_low),
                    pitch_math::note_name(state.view_high)
                ))
                .small()
                .monospace()
                .weak(),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if bands.len() > 1
                    && let Some(id) = state.selected
                    && ui.button("Delete").clicked()
                {
                    actions.push(EditorAction::Delete(id));
                    state.fitted_for = None;
                }
                if ui.button("Add band").clicked() {
                    actions.push(EditorAction::Add);
                }
            });
        });

        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 280.0), Sense::hover());
        paint(ui, state, bands, active, history, now, current_note, rect);
        actions.extend(interact(ui, state, bands, rect));

        ui.add_space(4.0);
        ui.label(
            egui::RichText::new(
                "Drag a band to move it, or its edges to resize. Left and right arrow keys move the selected band. Drag empty space or scroll to pan. Overlapping bands combine their buttons. Hold Alt for fine control, Alt-scroll to zoom.",
            )
            .small()
            .weak(),
        );
    });
    actions
}

fn paint(
    ui: &egui::Ui,
    state: &EditorState,
    bands: &[PitchBand],
    active: &HashSet<Uuid>,
    history: &[PitchPoint],
    now: f64,
    current_note: Option<f64>,
    rect: Rect,
) {
    let painter = ui.painter_at(rect);
    painter.rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(10, 12, 18));
    painter.rect_stroke(
        rect,
        CornerRadius::same(10),
        Stroke::new(1.0, Color32::from_white_alpha(10)),
        egui::StrokeKind::Inside,
    );
    let plot = plot_rect(rect);
    let span = (state.view_high - state.view_low).max(0.01);

    let start = state.view_low.ceil() as i32;
    let end = state.view_high.floor() as i32;
    for note in start..=end {
        let y = y_of(note as f64, state.view_low, span, plot);
        let strong = note % 12 == 0;
        painter.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(1.0, Color32::from_white_alpha(if strong { 42 } else { 12 })),
        );
    }

    let step = axis_step(span);
    let mut note = (state.view_low / step).ceil() * step;
    while note <= state.view_high {
        painter.text(
            pos2(rect.left() + 6.0, y_of(note, state.view_low, span, plot)),
            egui::Align2::LEFT_CENTER,
            pitch_math::note_name(note),
            egui::FontId::monospace(10.0),
            Color32::from_white_alpha(170),
        );
        note += step;
    }

    let n = bands.len().max(1) as f32;
    let col_w = ((plot.width() - GAP * (n - 1.0)) / n).max(8.0);
    for (index, band) in bands.iter().enumerate() {
        let x = plot.left() + index as f32 * (col_w + GAP);
        let clipped_low = band.low_note.max(state.view_low);
        let clipped_high = band.high_note.min(state.view_high);
        if clipped_low >= clipped_high {
            continue;
        }
        let top = y_of(clipped_high, state.view_low, span, plot);
        let bottom = y_of(clipped_low, state.view_low, span, plot);
        let band_rect = Rect::from_min_max(pos2(x, top), pos2(x + col_w, bottom.max(top + 6.0)));
        let selected = state.selected == Some(band.id);
        let is_active = active.contains(&band.id);
        let alpha = if is_active {
            0.65
        } else if selected {
            0.42
        } else {
            0.25
        };
        painter.rect_filled(band_rect, CornerRadius::same(8), hue_color(band.hue, alpha));
        painter.rect_stroke(
            band_rect,
            CornerRadius::same(8),
            Stroke::new(
                if selected { 2.0 } else { 1.0 },
                if selected {
                    Color32::WHITE
                } else {
                    hue_color(band.hue, 0.7)
                },
            ),
            egui::StrokeKind::Inside,
        );
        if band_rect.height() > 30.0 {
            let buttons = band
                .buttons
                .iter()
                .map(|button| button.short_label())
                .collect::<Vec<_>>()
                .join("+");
            let suffix = match band.trigger {
                crate::profile::BandTriggerMode::Hold => String::new(),
                crate::profile::BandTriggerMode::Tap => " · tap".to_string(),
                crate::profile::BandTriggerMode::Repeating => {
                    format!(" · {}ms", band.repeat_interval_ms)
                }
            };
            painter.text(
                pos2(band_rect.center().x, band_rect.top() + 14.0),
                egui::Align2::CENTER_CENTER,
                &band.name,
                egui::FontId::proportional(11.0),
                Color32::WHITE,
            );
            if !buttons.is_empty() {
                painter.text(
                    pos2(band_rect.center().x, band_rect.top() + 28.0),
                    egui::Align2::CENTER_CENTER,
                    format!("{buttons}{suffix}"),
                    egui::FontId::proportional(9.0),
                    Color32::from_white_alpha(220),
                );
            }
        }
    }

    let mut previous: Option<Pos2> = None;
    for point in history {
        let Some(note) = point.note else {
            previous = None;
            continue;
        };
        if note < state.view_low || note > state.view_high {
            previous = None;
            continue;
        }
        let t = ((point.time - (now - 4.0)) / 4.0) as f32;
        if !(0.0..=1.0).contains(&t) {
            previous = None;
            continue;
        }
        let p = pos2(
            plot.left() + t * plot.width(),
            y_of(note, state.view_low, span, plot),
        );
        if let Some(prev) = previous {
            painter.line_segment([prev, p], Stroke::new(2.0, Color32::from_white_alpha(230)));
        }
        previous = Some(p);
    }

    if let Some(note) = current_note
        && (state.view_low..=state.view_high).contains(&note)
    {
        let y = y_of(note, state.view_low, span, plot);
        painter.line_segment(
            [pos2(plot.left(), y), pos2(plot.right(), y)],
            Stroke::new(2.0, Color32::from_rgb(0, 210, 220)),
        );
    }

    let above = bands.iter().any(|band| band.low_note >= state.view_high);
    let below = bands.iter().any(|band| band.high_note <= state.view_low);
    if above {
        painter.text(
            pos2(rect.center().x, rect.top() + 14.0),
            egui::Align2::CENTER_CENTER,
            "Bands above — zoom out or pan up",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(230, 160, 40),
        );
    }
    if below {
        painter.text(
            pos2(rect.center().x, rect.bottom() - 14.0),
            egui::Align2::CENTER_CENTER,
            "Bands below — zoom out or pan down",
            egui::FontId::proportional(11.0),
            Color32::from_rgb(230, 160, 40),
        );
    }
}

fn interact(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    bands: &[PitchBand],
    rect: Rect,
) -> Vec<EditorAction> {
    let mut actions = Vec::new();
    let plot = plot_rect(rect);
    let span = (state.view_high - state.view_low).max(0.01);
    let alt = ui.ctx().input(|input| input.modifiers.alt);
    let scroll = ui.ctx().input(|input| input.smooth_scroll_delta.y);
    let plot_hover = ui.rect_contains_pointer(rect);
    if plot_hover && scroll.abs() > 0.1 {
        if alt {
            zoom(state, if scroll > 0.0 { 0.92 } else { 1.08 });
        } else {
            pan(state, f64::from(-scroll) * span * 0.002);
        }
        ui.ctx()
            .input_mut(|input| input.smooth_scroll_delta = Vec2::ZERO);
    }

    let background = ui.interact(rect, ui.id().with("band-pan"), Sense::drag());
    if background.drag_started() {
        state.pan_drag = Some(PanDrag {
            low: state.view_low,
            high: state.view_high,
            dy: 0.0,
        });
    }
    if background.dragged()
        && let Some(pan_drag) = &mut state.pan_drag
    {
        pan_drag.dy += background.drag_delta().y;
        let usable = (rect.height() - 16.0).max(1.0);
        let delta = f64::from(-pan_drag.dy) / f64::from(usable) * (pan_drag.high - pan_drag.low);
        let mut low = pan_drag.low + delta;
        let mut high = pan_drag.high + delta;
        if low < ABS_LOW {
            high += ABS_LOW - low;
            low = ABS_LOW;
        }
        if high > ABS_HIGH {
            low -= high - ABS_HIGH;
            high = ABS_HIGH;
        }
        state.view_low = low;
        state.view_high = high;
    }
    if background.drag_stopped() {
        state.pan_drag = None;
    }

    let n = bands.len().max(1) as f32;
    let col_w = ((plot.width() - GAP * (n - 1.0)) / n).max(8.0);
    for (index, band) in bands.iter().enumerate() {
        let x = plot.left() + index as f32 * (col_w + GAP);
        let clipped_low = band.low_note.max(state.view_low);
        let clipped_high = band.high_note.min(state.view_high);
        if clipped_low >= clipped_high {
            continue;
        }
        let top = y_of(clipped_high, state.view_low, span, plot);
        let bottom = y_of(clipped_low, state.view_low, span, plot);
        let band_rect = Rect::from_min_max(pos2(x, top), pos2(x + col_w, (bottom).max(top + 6.0)));
        let body = ui.interact(
            band_rect,
            ui.id().with(("band", band.id)),
            Sense::click_and_drag(),
        );
        if body.clicked() || body.drag_started() {
            state.selected = Some(band.id);
        }
        if body.drag_started() {
            state.move_drag = Some(MoveDrag {
                id: band.id,
                low: band.low_note,
                high: band.high_note,
                dy: 0.0,
            });
            state.pan_drag = None;
        }
        if body.dragged()
            && let Some(drag) = &mut state.move_drag
            && drag.id == band.id
        {
            drag.dy += body.drag_delta().y;
            let usable = (plot.height()).max(1.0);
            let mut delta = snap(f64::from(-drag.dy) / f64::from(usable) * span, alt);
            delta = delta.clamp(ABS_LOW - drag.low, ABS_HIGH - drag.high);
            actions.push(EditorAction::SetNotes {
                id: band.id,
                low: drag.low + delta,
                high: drag.high + delta,
                persist: false,
            });
        }
        if body.drag_stopped()
            && state
                .move_drag
                .as_ref()
                .is_some_and(|drag| drag.id == band.id)
        {
            state.move_drag = None;
            actions.push(EditorAction::SetNotes {
                id: band.id,
                low: band.low_note,
                high: band.high_note,
                persist: true,
            });
        }

        if band.high_note <= state.view_high {
            let handle = Rect::from_center_size(
                pos2(band_rect.center().x, band_rect.top()),
                vec2(col_w, 12.0),
            );
            actions.extend(edge(ui, state, band, handle, plot, span, alt, false));
        }
        if band.low_note >= state.view_low {
            let handle = Rect::from_center_size(
                pos2(band_rect.center().x, band_rect.bottom()),
                vec2(col_w, 12.0),
            );
            actions.extend(edge(ui, state, band, handle, plot, span, alt, true));
        }
    }
    if let Some(action) = reorder_with_arrows(ui, state, bands) {
        actions.push(action);
    }
    actions
}

fn reorder_with_arrows(
    ui: &mut egui::Ui,
    state: &EditorState,
    bands: &[PitchBand],
) -> Option<EditorAction> {
    if state.move_drag.is_some() {
        return None;
    }
    let id = state.selected?;
    // Leave arrow keys to whichever widget currently has them, such as a text field.
    if ui.memory(|memory| memory.focused().is_some()) {
        return None;
    }
    let (left, right) = ui.input_mut(|input| {
        (
            input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
            input.count_and_consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
        )
    });
    let delta = right as i32 - left as i32;
    let index = bands.iter().position(|band| band.id == id)?;
    let last = bands.len() as i32 - 1;
    let target = (index as i32 + delta).clamp(0, last);
    let applied = target - index as i32;
    if applied == 0 {
        return None;
    }
    Some(EditorAction::Reorder { id, delta: applied })
}

fn edge(
    ui: &mut egui::Ui,
    state: &mut EditorState,
    band: &PitchBand,
    handle: Rect,
    plot: Rect,
    span: f64,
    alt: bool,
    low_edge: bool,
) -> Vec<EditorAction> {
    let response = ui.interact(
        handle,
        ui.id().with(("edge", band.id, low_edge)),
        Sense::drag(),
    );
    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }
    if !(response.dragged() || response.drag_stopped()) {
        return Vec::new();
    }
    state.selected = Some(band.id);
    state.pan_drag = None;
    let Some(pointer) = response
        .interact_pointer_pos()
        .or(ui.ctx().pointer_latest_pos())
    else {
        return Vec::new();
    };
    let note = snap(note_for_y(pointer.y, plot, state.view_low, span), alt);
    let (low, high) = if low_edge {
        (
            note.clamp(ABS_LOW, band.high_note - MIN_BAND),
            band.high_note,
        )
    } else {
        (
            band.low_note,
            note.clamp(band.low_note + MIN_BAND, ABS_HIGH),
        )
    };
    vec![EditorAction::SetNotes {
        id: band.id,
        low,
        high,
        persist: response.drag_stopped(),
    }]
}

fn plot_rect(rect: Rect) -> Rect {
    Rect::from_min_max(
        pos2(rect.left() + LEADING, rect.top() + V_INSET),
        pos2(rect.right() - TRAILING, rect.bottom() - V_INSET),
    )
}

fn y_of(note: f64, view_low: f64, span: f64, plot: Rect) -> f32 {
    let t = ((note.clamp(view_low, view_low + span) - view_low) / span) as f32;
    plot.top() + (1.0 - t) * plot.height()
}

fn note_for_y(y: f32, plot: Rect, view_low: f64, span: f64) -> f64 {
    let local = (y - plot.top()).clamp(0.0, plot.height());
    let t = 1.0 - f64::from(local / plot.height().max(1.0));
    view_low + t * span
}

fn axis_step(span: f64) -> f64 {
    if span <= 10.0 {
        1.0
    } else if span <= 18.0 {
        2.0
    } else if span <= 30.0 {
        3.0
    } else {
        4.0
    }
}

fn snap(note: f64, fine: bool) -> f64 {
    if fine { note } else { note.round() }
}

fn zoom(state: &mut EditorState, factor: f64) {
    let focus = (state.view_low + state.view_high) / 2.0;
    let span = ((state.view_high - state.view_low) * factor).clamp(MIN_SPAN, MAX_SPAN);
    let mut low = focus - span / 2.0;
    let mut high = focus + span / 2.0;
    if low < ABS_LOW {
        low = ABS_LOW;
        high = low + span;
    }
    if high > ABS_HIGH {
        high = ABS_HIGH;
        low = high - span;
    }
    set_view(state, low, high);
}

fn pan(state: &mut EditorState, delta: f64) {
    let mut low = state.view_low + delta;
    let mut high = state.view_high + delta;
    if low < ABS_LOW {
        high += ABS_LOW - low;
        low = ABS_LOW;
    }
    if high > ABS_HIGH {
        low -= high - ABS_HIGH;
        high = ABS_HIGH;
    }
    state.view_low = low;
    state.view_high = high;
}

fn set_view(state: &mut EditorState, low: f64, high: f64) {
    let mut low = ABS_LOW.max(low.min(high - MIN_SPAN));
    let mut high = ABS_HIGH.min(high.max(low + MIN_SPAN));
    if high - low > MAX_SPAN {
        let mid = (low + high) / 2.0;
        low = mid - MAX_SPAN / 2.0;
        high = mid + MAX_SPAN / 2.0;
    }
    state.view_low = low;
    state.view_high = high;
}

fn fit(state: &mut EditorState, bands: &[PitchBand]) {
    let Some(min_band) = bands.iter().map(|band| band.low_note).reduce(f64::min) else {
        set_view(state, 48.0, 84.0);
        return;
    };
    let Some(max_band) = bands.iter().map(|band| band.high_note).reduce(f64::max) else {
        set_view(state, 48.0, 84.0);
        return;
    };
    let padding = 2.0_f64.max((max_band - min_band) * 0.15);
    set_view(state, min_band - padding, max_band + padding);
}
