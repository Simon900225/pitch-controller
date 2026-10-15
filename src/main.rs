mod audio;
mod band_mapper;
mod buttons;
mod engine;
mod input_sink;
mod pitch_detector;
mod pitch_math;
mod pitch_smoother;
mod profile;
mod ui;

use eframe::egui;

fn main() -> eframe::Result {
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Pitch Controller")
            .with_inner_size([1100.0, 820.0])
            .with_min_inner_size([480.0, 420.0])
            // eframe only enables transparent surfaces for any viewport if the root asks for it.
            .with_transparent(true),
        ..Default::default()
    };
    eframe::run_native(
        "Pitch Controller",
        options,
        Box::new(|cc| Ok(Box::new(ui::PitchApp::new(cc)))),
    )
}
