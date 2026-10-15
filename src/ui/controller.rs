use eframe::egui;
use egui::{Color32, CornerRadius, Pos2, Rect, Sense, Stroke, pos2, vec2};

use crate::buttons::SnesButton;
use std::collections::BTreeSet;

pub fn show(ui: &mut egui::Ui, pressed: &BTreeSet<SnesButton>) {
    let width = ui.available_width().clamp(360.0, 560.0);
    let scale = width / 520.0;
    let height = 240.0 * scale;
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
    let pad = Rect::from_center_size(rect.center(), vec2(width, height));
    let painter = ui.painter();
    let shell_radius = CornerRadius::same((32.0 * scale) as u8);

    let shadow = pad
        .translate(vec2(0.0, 7.0 * scale))
        .shrink2(vec2(14.0 * scale, 4.0 * scale));
    painter.rect_filled(shadow, shell_radius, Color32::from_black_alpha(80));

    painter.rect_filled(pad, shell_radius, Color32::from_rgb(198, 196, 208));
    painter.rect_stroke(
        pad,
        shell_radius,
        Stroke::new(1.0, Color32::from_black_alpha(50)),
        egui::StrokeKind::Inside,
    );

    let plate = pad.shrink2(vec2(26.0 * scale, 34.0 * scale));
    painter.rect_filled(
        plate,
        CornerRadius::same((20.0 * scale) as u8),
        Color32::from_rgb(124, 86, 168),
    );

    painter.text(
        pos2(pad.center().x, pad.top() + 28.0 * scale),
        egui::Align2::CENTER_CENTER,
        "PITCH",
        egui::FontId::proportional(11.0 * scale),
        Color32::from_white_alpha(70),
    );

    let left = pos2(pad.left() + 90.0 * scale, pad.center().y + 8.0 * scale);
    dpad(painter, left, scale, pressed);

    cap(
        painter,
        pos2(left.x - 28.0 * scale, pad.top() + 48.0 * scale),
        "L",
        pressed.contains(&SnesButton::L),
        scale,
        Color32::from_rgb(214, 212, 224),
    );

    pill(
        painter,
        pos2(pad.center().x - 36.0 * scale, pad.center().y + 10.0 * scale),
        "SELECT",
        pressed.contains(&SnesButton::Select),
        scale,
    );
    pill(
        painter,
        pos2(pad.center().x + 36.0 * scale, pad.center().y + 10.0 * scale),
        "START",
        pressed.contains(&SnesButton::Start),
        scale,
    );

    let face = pos2(pad.right() - 100.0 * scale, pad.center().y + 16.0 * scale);
    cap(
        painter,
        pos2(face.x + 36.0 * scale, pad.top() + 48.0 * scale),
        "R",
        pressed.contains(&SnesButton::R),
        scale,
        Color32::from_rgb(214, 212, 224),
    );
    face_button(
        painter,
        face + vec2(0.0, -34.0 * scale),
        "X",
        Color32::from_rgb(70, 120, 220),
        pressed.contains(&SnesButton::X),
        scale,
    );
    face_button(
        painter,
        face + vec2(-34.0 * scale, 0.0),
        "Y",
        Color32::from_rgb(70, 170, 90),
        pressed.contains(&SnesButton::Y),
        scale,
    );
    face_button(
        painter,
        face + vec2(34.0 * scale, 0.0),
        "A",
        Color32::from_rgb(210, 70, 70),
        pressed.contains(&SnesButton::A),
        scale,
    );
    face_button(
        painter,
        face + vec2(0.0, 34.0 * scale),
        "B",
        Color32::from_rgb(214, 186, 55),
        pressed.contains(&SnesButton::B),
        scale,
    );
}

fn dpad(painter: &egui::Painter, center: Pos2, scale: f32, pressed: &BTreeSet<SnesButton>) {
    let arm = Color32::from_rgb(46, 46, 46);
    painter.rect_filled(
        Rect::from_center_size(center, vec2(36.0 * scale, 110.0 * scale)),
        CornerRadius::same((8.0 * scale) as u8),
        arm,
    );
    painter.rect_filled(
        Rect::from_center_size(center, vec2(110.0 * scale, 36.0 * scale)),
        CornerRadius::same((8.0 * scale) as u8),
        arm,
    );
    let any = pressed.contains(&SnesButton::Up)
        || pressed.contains(&SnesButton::Down)
        || pressed.contains(&SnesButton::Left)
        || pressed.contains(&SnesButton::Right);
    if any {
        painter.rect_stroke(
            Rect::from_center_size(center, vec2(110.0 * scale, 110.0 * scale)),
            CornerRadius::same(8),
            Stroke::new(2.0, Color32::from_rgba_unmultiplied(0, 220, 230, 120)),
            egui::StrokeKind::Outside,
        );
    }
    let step = 36.0 * scale;
    dir(
        painter,
        center + vec2(0.0, -step),
        "▲",
        pressed.contains(&SnesButton::Up),
        scale,
    );
    dir(
        painter,
        center + vec2(0.0, step),
        "▼",
        pressed.contains(&SnesButton::Down),
        scale,
    );
    dir(
        painter,
        center + vec2(-step, 0.0),
        "◀",
        pressed.contains(&SnesButton::Left),
        scale,
    );
    dir(
        painter,
        center + vec2(step, 0.0),
        "▶",
        pressed.contains(&SnesButton::Right),
        scale,
    );
}

fn dir(painter: &egui::Painter, center: Pos2, glyph: &str, pressed: bool, scale: f32) {
    let fill = if pressed {
        Color32::from_rgb(0, 210, 220)
    } else {
        Color32::from_rgb(72, 72, 72)
    };
    painter.rect_filled(
        Rect::from_center_size(center, vec2(28.0 * scale, 28.0 * scale)),
        CornerRadius::same(4),
        fill,
    );
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        glyph,
        egui::FontId::proportional(10.0 * scale),
        if pressed {
            Color32::BLACK
        } else {
            Color32::from_white_alpha(180)
        },
    );
}

fn face_button(
    painter: &egui::Painter,
    center: Pos2,
    label: &str,
    color: Color32,
    pressed: bool,
    scale: f32,
) {
    let radius = 18.0 * scale * if pressed { 1.08 } else { 1.0 };
    let fill = if pressed {
        color
    } else {
        color.gamma_multiply(0.55)
    };
    painter.circle_filled(center, radius, fill);
    painter.circle_stroke(
        center,
        radius,
        Stroke::new(1.0, Color32::from_white_alpha(90)),
    );
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(13.0 * scale),
        Color32::from_black_alpha(190),
    );
}

fn cap(
    painter: &egui::Painter,
    center: Pos2,
    label: &str,
    pressed: bool,
    scale: f32,
    color: Color32,
) {
    let fill = if pressed {
        color
    } else {
        color.gamma_multiply(0.45)
    };
    let rect = Rect::from_center_size(center, vec2(44.0 * scale, 22.0 * scale));
    painter.rect_filled(rect, CornerRadius::same((11.0 * scale) as u8), fill);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(10.0 * scale),
        Color32::WHITE,
    );
}

fn pill(painter: &egui::Painter, center: Pos2, label: &str, pressed: bool, scale: f32) {
    let fill = if pressed {
        Color32::from_rgb(230, 140, 40)
    } else {
        Color32::from_rgb(64, 64, 64)
    };
    let rect = Rect::from_center_size(center, vec2(54.0 * scale, 18.0 * scale));
    painter.rect_filled(rect, CornerRadius::same((9.0 * scale) as u8), fill);
    painter.text(
        center,
        egui::Align2::CENTER_CENTER,
        label,
        egui::FontId::proportional(8.0 * scale),
        Color32::from_white_alpha(230),
    );
}
