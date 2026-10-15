mod bands;
mod controller;
mod overlay;

use std::time::{Duration, Instant};

use eframe::egui;
use egui::{Color32, CornerRadius, Key, Modifiers, RichText, Stroke, Vec2};
use global_hotkey::hotkey::{CMD_OR_CTRL, Code, HotKey, Modifiers as HotModifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};

use crate::buttons::{SnesButton, default_key_map, key_name};
use crate::engine::{CalibrationPhase, Engine};
use std::collections::BTreeSet;

use crate::profile::{
    AppSettings, BandTriggerMode, PitchBand, Profile, load_profiles, load_settings, save_profiles,
    save_settings,
};

/// Below this width the main view keeps its layout and scrolls sideways.
const MIN_CONTENT_WIDTH: f32 = 920.0;

pub struct PitchApp {
    engine: Engine,
    profiles: Vec<Profile>,
    selected_id: uuid::Uuid,
    settings: AppSettings,
    editor: bands::EditorState,
    show_settings: bool,
    listening: Option<SnesButton>,
    prev_mods: Modifiers,
    hotkeys: Option<Hotkeys>,
    access_ok: bool,
    access_checked: Instant,
    overlay_placed: bool,
    platform_note: Option<String>,
}

struct Hotkeys {
    _manager: GlobalHotKeyManager,
    output_id: u32,
    overlay_id: u32,
}

const BG: Color32 = Color32::from_rgb(22, 24, 36);
const ACCENT: Color32 = Color32::from_rgb(72, 196, 190);
const ACCENT_FILL: Color32 = Color32::from_rgb(28, 112, 110);

impl PitchApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        apply_style(&cc.egui_ctx);
        let mut profiles = load_profiles();
        if profiles.is_empty() {
            profiles.push(Profile::mario());
        }
        let mut settings = load_settings();
        settings.overlay_edit = false;
        let selected_id = settings
            .selected_profile_id
            .filter(|id| profiles.iter().any(|profile| profile.id == *id))
            .unwrap_or(profiles[0].id);
        let selected = profiles
            .iter()
            .find(|profile| profile.id == selected_id)
            .cloned()
            .unwrap();
        let engine = Engine::new(selected);
        Self {
            engine,
            profiles,
            selected_id,
            settings,
            editor: bands::EditorState::default(),
            show_settings: false,
            listening: None,
            prev_mods: Modifiers::default(),
            hotkeys: Hotkeys::install(),
            access_ok: accessibility_trusted(),
            access_checked: Instant::now(),
            overlay_placed: false,
            platform_note: platform_note(),
        }
    }

    fn selected(&self) -> &Profile {
        self.profiles
            .iter()
            .find(|profile| profile.id == self.selected_id)
            .unwrap_or(&self.profiles[0])
    }

    fn update_selected(&mut self, persist: bool, f: impl FnOnce(&mut Profile)) {
        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.id == self.selected_id)
        {
            f(profile);
            profile.normalize_bands();
        }
        self.push_selected(persist);
    }

    fn push_selected(&mut self, persist: bool) {
        if let Some(profile) = self
            .profiles
            .iter()
            .find(|profile| profile.id == self.selected_id)
            .cloned()
        {
            self.engine.set_profile(profile);
        }
        if persist {
            self.persist();
        }
    }

    fn persist(&mut self) {
        self.settings.selected_profile_id = Some(self.selected_id);
        save_profiles(&self.profiles);
        save_settings(&self.settings);
    }

    fn poll_hotkeys(&mut self, ctx: &egui::Context) {
        let Some(hotkeys) = &self.hotkeys else { return };
        let output_id = hotkeys.output_id;
        let overlay_id = hotkeys.overlay_id;
        let focused = ctx.input(|input| input.viewport().focused.unwrap_or(false));
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if focused || event.state != HotKeyState::Pressed {
                continue;
            }
            if event.id == output_id {
                self.engine.toggle_output();
            } else if event.id == overlay_id {
                self.set_overlay_visible(!self.settings.overlay_visible);
            }
        }
    }

    fn set_overlay_visible(&mut self, visible: bool) {
        self.settings.overlay_visible = visible;
        if !visible {
            self.overlay_placed = false;
            self.settings.overlay_edit = false;
        }
        self.persist();
    }

    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.listening.is_some() {
            return;
        }
        let mut start = false;
        let mut output = false;
        let mut overlay = false;
        let mut test = false;
        ctx.input_mut(|input| {
            output = input.consume_shortcut(&egui::KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::P,
            ));
            overlay = input.consume_shortcut(&egui::KeyboardShortcut::new(
                Modifiers::COMMAND | Modifiers::SHIFT,
                Key::O,
            ));
            start =
                input.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::L));
            test = input.consume_shortcut(&egui::KeyboardShortcut::new(Modifiers::COMMAND, Key::T));
        });
        if start {
            if self.engine.running {
                self.engine.stop();
            } else {
                self.engine.start();
            }
        }
        if output && !self.engine.test_mode {
            self.engine.toggle_output();
        }
        if overlay {
            self.set_overlay_visible(!self.settings.overlay_visible);
        }
        if test {
            self.engine.set_test_mode(!self.engine.test_mode);
        }
    }
}

impl eframe::App for PitchApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        Color32::TRANSPARENT.to_normalized_gamma_f32()
    }

    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if self.access_checked.elapsed() > Duration::from_millis(1500) {
            self.access_ok = accessibility_trusted();
            self.access_checked = Instant::now();
        }
        if let Some(bands) = self.engine.poll() {
            self.update_selected(true, |profile| profile.bands = bands);
        }
        self.poll_hotkeys(ctx);
        if self.engine.running || self.settings.overlay_visible {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.handle_shortcuts(ui.ctx());
        let bg = BG;
        egui::Panel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(egui::Margin::same(10)),
            )
            .show(ui, |ui| {
                self.toolbar(ui);
            });
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(bg)
                    .inner_margin(egui::Margin::same(16)),
            )
            .show(ui, |ui| {
                egui::ScrollArea::both()
                    .id_salt("main")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        // Keep the layout intact once the window is narrower than the
                        // page, and scroll to the rest instead of clipping it.
                        ui.set_width(ui.available_width().max(MIN_CONTENT_WIDTH));
                        self.body(ui);
                    });
            });
        self.viewports(ui.ctx());
    }
}

impl PitchApp {
    fn toolbar(&mut self, ui: &mut egui::Ui) {
        if !self.access_ok {
            ui.horizontal(|ui| {
                ui.colored_label(
                    Color32::YELLOW,
                    "Accessibility permission is required to send key presses.",
                );
                if ui.button("Open Settings").clicked() {
                    open_accessibility_settings();
                }
                if ui.button("Check Again").clicked() {
                    self.access_ok = accessibility_trusted();
                }
            });
        }
        if let Some(note) = &self.platform_note {
            ui.colored_label(Color32::from_rgb(230, 160, 40), note);
        }
        if let Some(error) = &self.engine.audio_error {
            ui.colored_label(Color32::from_rgb(230, 90, 70), error);
        }
        if let Some(error) = &self.engine.snapshot().input_error {
            ui.colored_label(Color32::from_rgb(230, 90, 70), error);
        }

        egui::ScrollArea::horizontal()
            .id_salt("toolbar")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    self.toolbar_controls(ui);
                });
            });
    }

    fn toolbar_controls(&mut self, ui: &mut egui::Ui) {
        let mut selected = self.selected_id;
        egui::ComboBox::from_id_salt("profile")
            .width(180.0)
            .selected_text(self.selected().name.clone())
            .show_ui(ui, |ui| {
                for profile in &self.profiles {
                    ui.selectable_value(&mut selected, profile.id, profile.name.clone());
                }
            });
        if selected != self.selected_id {
            self.selected_id = selected;
            self.editor.selected = None;
            self.editor.reset_fit();
            self.push_selected(true);
        }
        if ui.button("+").on_hover_text("Add profile").clicked() {
            let mut profile = Profile::empty();
            profile.name = format!("Profile {}", self.profiles.len() + 1);
            self.selected_id = profile.id;
            self.profiles.push(profile);
            self.editor.selected = None;
            self.editor.reset_fit();
            self.push_selected(true);
        }
        if ui
            .add_enabled(self.profiles.len() > 1, egui::Button::new("Delete"))
            .clicked()
        {
            self.profiles
                .retain(|profile| profile.id != self.selected_id);
            self.selected_id = self.profiles[0].id;
            self.editor.selected = None;
            self.editor.reset_fit();
            self.push_selected(true);
        }
        let mut name = self.selected().name.clone();
        if ui
            .add(
                egui::TextEdit::singleline(&mut name)
                    .desired_width(140.0)
                    .hint_text("Profile name"),
            )
            .changed()
        {
            self.update_selected(true, |profile| profile.name = name);
        }

        ui.separator();
        let mut test = self.engine.test_mode;
        if ui
            .checkbox(&mut test, "Test mode")
            .on_hover_text("Buttons light up, but no keys are sent")
            .changed()
        {
            self.engine.set_test_mode(test);
        }

        let phase = self.engine.snapshot().calibration_phase;
        let calibrating = self.engine.snapshot().calibrating;
        let label = match phase {
            CalibrationPhase::Idle => "Calibrate",
            CalibrationPhase::Low => "Sing LOW…",
            CalibrationPhase::High => "Sing HIGH…",
            CalibrationPhase::Done => "Calibrated",
        };
        if ui
            .add_enabled(!calibrating, egui::Button::new(label))
            .clicked()
        {
            self.engine.begin_calibration();
        }

        let running = self.engine.running;
        let start = if running { "Stop" } else { "Start" };
        let start_fill = if running {
            Color32::from_rgb(148, 64, 68)
        } else {
            ACCENT_FILL
        };
        if ui.add(egui::Button::new(start).fill(start_fill)).clicked() {
            if running {
                self.engine.stop();
            } else {
                self.engine.start();
            }
        }
        let output_label = if self.engine.output_enabled {
            "Output On"
        } else {
            "Output Off"
        };
        let mut output_button = egui::Button::new(output_label);
        if self.engine.output_enabled {
            output_button = output_button.fill(Color32::from_rgb(46, 122, 78));
        }
        if ui
            .add_enabled(!self.engine.test_mode, output_button)
            .on_hover_text(format!(
                "{cmd}⇧P toggles output while playing",
                cmd = command_label()
            ))
            .clicked()
        {
            self.engine.toggle_output();
        }
        if ui
            .button(if self.settings.overlay_visible {
                "Hide Overlay"
            } else {
                "Show Overlay"
            })
            .clicked()
        {
            self.set_overlay_visible(!self.settings.overlay_visible);
        }
        if ui.button("Settings").clicked() {
            self.show_settings = true;
        }
    }

    fn body(&mut self, ui: &mut egui::Ui) {
        self.status(ui);
        ui.add_space(8.0);
        controller::show(ui, &self.engine.snapshot().pressed);
        ui.add_space(8.0);

        let snapshot = self.engine.snapshot().clone();
        let profile_id = self.selected_id;
        let bands = self.selected().bands.clone();
        let actions = bands::show(
            ui,
            &mut self.editor,
            profile_id,
            &bands,
            &snapshot.active_band_ids,
            &snapshot.history,
            snapshot.now,
            snapshot.note,
        );
        for action in actions {
            match action {
                bands::EditorAction::SetNotes {
                    id,
                    low,
                    high,
                    persist,
                } => {
                    self.update_selected(persist, |profile| {
                        if let Some(band) = profile.bands.iter_mut().find(|band| band.id == id) {
                            band.low_note = low;
                            band.high_note = high;
                        }
                    });
                }
                bands::EditorAction::Add => {
                    let center = ((self.editor.view_low + self.editor.view_high) / 2.0).round();
                    let low = (center - 2.0).clamp(36.0, 92.0);
                    let hues = [0.55, 0.35, 0.12, 0.95, 0.75];
                    let count = self.selected().bands.len();
                    let band = PitchBand::new(
                        format!("Band {}", count + 1),
                        low,
                        low + 4.0,
                        BTreeSet::new(),
                        hues[count % hues.len()],
                    );
                    let id = band.id;
                    self.update_selected(true, |profile| profile.bands.push(band));
                    self.editor.selected = Some(id);
                }
                bands::EditorAction::Delete(id) => {
                    self.update_selected(true, |profile| {
                        profile.bands.retain(|band| band.id != id)
                    });
                    self.editor.selected = self.selected().bands.first().map(|band| band.id);
                }
                bands::EditorAction::Reorder { id, delta } => {
                    self.update_selected(true, |profile| {
                        profile.move_band(id, delta);
                    });
                }
            }
        }

        ui.add_space(8.0);
        if let Some(id) = self.editor.selected {
            self.button_editor(ui, id);
        }
    }

    fn status(&self, ui: &mut egui::Ui) {
        let snap = self.engine.snapshot();
        let gate = self.selected().gate_rms;
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                metric(
                    ui,
                    "Pitch",
                    snap.note
                        .map(crate::pitch_math::note_name)
                        .unwrap_or_else(|| "—".into()),
                    58.0,
                );
                metric(
                    ui,
                    "Hz",
                    snap.frequency_hz
                        .map(|hz| format!("{hz:.1}"))
                        .unwrap_or_else(|| "—".into()),
                    72.0,
                );
                metric(ui, "Conf", format!("{:.2}", snap.confidence), 52.0);
                metric(
                    ui,
                    "Level",
                    format!("{:.0}%", (snap.rms / 0.1).min(1.0) * 100.0),
                    52.0,
                );
                ui.allocate_ui_with_layout(
                    Vec2::new(150.0, 36.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        level_meter(ui, snap.rms, gate);
                    },
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if snap.calibrating {
                        let text = if snap.calibration_phase == CalibrationPhase::Low {
                            "Hold your lowest comfortable pitch…"
                        } else {
                            "Hold your highest comfortable pitch…"
                        };
                        status_chip(ui, text, Color32::from_rgb(230, 168, 64));
                    } else {
                        let active: Vec<_> = self
                            .selected()
                            .bands
                            .iter()
                            .filter(|band| snap.active_band_ids.contains(&band.id))
                            .map(|band| band.name.as_str())
                            .collect();
                        if active.is_empty() {
                            status_chip(
                                ui,
                                if snap.voiced { "No band" } else { "Silence" },
                                Color32::from_rgb(150, 156, 176),
                            );
                        } else {
                            let buttons = crate::buttons::buttons_label(&snap.pressed);
                            status_chip(ui, &format!("{}  {buttons}", active.join(" + ")), ACCENT);
                        }
                    }
                });
            });
        });
    }

    fn button_editor(&mut self, ui: &mut egui::Ui, band_id: uuid::Uuid) {
        let Some(band) = self
            .selected()
            .bands
            .iter()
            .find(|band| band.id == band_id)
            .cloned()
        else {
            return;
        };
        card().show(ui, |ui| {
            ui.horizontal(|ui| {
                section_title(ui, &format!("Buttons for {}", band.name));
                let mut name = band.name.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut name).desired_width(160.0))
                    .changed()
                {
                    self.update_selected(true, |profile| {
                        if let Some(band) = profile.bands.iter_mut().find(|band| band.id == band_id)
                        {
                            band.name = name;
                        }
                    });
                }
            });
            ui.add_space(2.0);
            ui.label(RichText::new("While this band is active").small().weak());
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for mode in [
                    BandTriggerMode::Hold,
                    BandTriggerMode::Tap,
                    BandTriggerMode::Repeating,
                ] {
                    let selected = band.trigger == mode;
                    let mut button = egui::Button::new(mode.label());
                    if selected {
                        button = button.fill(ACCENT_FILL);
                    }
                    if ui.add(button).clicked() && !selected {
                        self.update_selected(true, |profile| {
                            if let Some(band) =
                                profile.bands.iter_mut().find(|band| band.id == band_id)
                            {
                                band.trigger = mode;
                            }
                        });
                    }
                }
            });
            if band.trigger == BandTriggerMode::Repeating {
                ui.horizontal(|ui| {
                    ui.label("Every");
                    let mut ms = band.repeat_interval_ms as f32;
                    if ui
                        .add(egui::Slider::new(&mut ms, 50.0..=2000.0).step_by(10.0))
                        .changed()
                    {
                        let ms = ms.round() as i32;
                        self.update_selected(true, |profile| {
                            if let Some(band) =
                                profile.bands.iter_mut().find(|band| band.id == band_id)
                            {
                                band.repeat_interval_ms = ms;
                            }
                        });
                    }
                    ui.monospace(format!("{ms} ms"));
                });
            }
            ui.weak(band.trigger.help(band.repeat_interval_ms));
            ui.add_space(2.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = Vec2::new(6.0, 6.0);
                for button in SnesButton::ALL {
                    let on = band.buttons.contains(&button);
                    let label = if on {
                        RichText::new(button.display_name())
                            .strong()
                            .color(Color32::from_rgb(16, 18, 28))
                    } else {
                        RichText::new(button.display_name())
                    };
                    let mut widget = egui::Button::new(label).min_size(Vec2::new(52.0, 26.0));
                    if on {
                        widget = widget.fill(hue_color(band.hue, 0.95));
                    }
                    if ui.add(widget).clicked() {
                        self.update_selected(true, |profile| {
                            if let Some(band) =
                                profile.bands.iter_mut().find(|band| band.id == band_id)
                            {
                                if !band.buttons.insert(button) {
                                    band.buttons.remove(&button);
                                }
                            }
                        });
                    }
                }
            });
        });
    }

    fn viewports(&mut self, ctx: &egui::Context) {
        if self.show_settings {
            let builder = egui::ViewportBuilder::default()
                .with_title("Settings")
                .with_inner_size([480.0, 560.0]);
            ctx.show_viewport_immediate(
                egui::ViewportId::from_hash_of("settings"),
                builder,
                |ui, _class| {
                    if ui.ctx().input(|input| input.viewport().close_requested()) {
                        self.show_settings = false;
                    }
                    egui::CentralPanel::default()
                        .frame(
                            egui::Frame::new()
                                .fill(BG)
                                .inner_margin(egui::Margin::same(16)),
                        )
                        .show(ui, |ui| self.settings_ui(ui));
                },
            );
        }

        if !self.settings.overlay_visible {
            return;
        }
        let mut builder = egui::ViewportBuilder::default()
            .with_title("Pitch")
            .with_decorations(false)
            .with_transparent(true)
            .with_always_on_top()
            .with_has_shadow(false)
            .with_taskbar(false)
            .with_mouse_passthrough(!self.settings.overlay_edit)
            .with_min_inner_size([100.0, 200.0])
            .with_max_inner_size([1200.0, 2000.0])
            .with_resizable(self.settings.overlay_edit);
        if !self.overlay_placed {
            builder = builder
                .with_position(egui::pos2(self.settings.overlay_x, self.settings.overlay_y))
                .with_inner_size([self.settings.overlay_w, self.settings.overlay_h]);
            self.overlay_placed = true;
        }
        let opacity = self.settings.overlay_opacity as f32;
        let edit = self.settings.overlay_edit;
        let bands = self.selected().bands.clone();
        let snapshot = self.engine.snapshot().clone();
        ctx.show_viewport_immediate(
            egui::ViewportId::from_hash_of("overlay"),
            builder,
            |ui, _class| {
                if ui.ctx().input(|input| input.viewport().close_requested()) {
                    self.settings.overlay_visible = false;
                    self.overlay_placed = false;
                }
                egui::CentralPanel::default()
                    .frame(egui::Frame::NONE)
                    .show(ui, |ui| {
                        overlay::show(ui, &snapshot, &bands, opacity, edit);
                    });
                let pointer_down = ui.ctx().input(|input| input.pointer.any_down());
                let outer = ui.ctx().input(|input| input.viewport().outer_rect);
                let inner = ui.ctx().input(|input| input.viewport().inner_rect);
                if edit
                    && !pointer_down
                    && let Some(outer) = outer
                    && let Some(inner) = inner
                    && inner.width() > 1.0
                {
                    let moved = (outer.min.x - self.settings.overlay_x).abs() > 0.5
                        || (outer.min.y - self.settings.overlay_y).abs() > 0.5
                        || (inner.width() - self.settings.overlay_w).abs() > 0.5
                        || (inner.height() - self.settings.overlay_h).abs() > 0.5;
                    if moved {
                        self.settings.overlay_x = outer.min.x;
                        self.settings.overlay_y = outer.min.y;
                        self.settings.overlay_w = inner.width();
                        self.settings.overlay_h = inner.height();
                        save_settings(&self.settings);
                    }
                }
            },
        );
    }

    fn settings_ui(&mut self, ui: &mut egui::Ui) {
        self.capture_key(ui);
        egui::ScrollArea::vertical().show(ui, |ui| {
            self.settings_body(ui);
        });
    }

    fn settings_body(&mut self, ui: &mut egui::Ui) {
        section_title(ui, "Emulator key mapping");
        ui.label(
            RichText::new("Click Set, then press the key your emulator uses for that button.")
                .small()
                .weak(),
        );
        ui.add_space(4.0);
        for button in SnesButton::ALL {
            ui.horizontal(|ui| {
                ui.label(button.display_name());
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let listening = self.listening == Some(button);
                    if ui
                        .button(if listening { "Listening…" } else { "Set" })
                        .clicked()
                    {
                        self.listening = Some(button);
                    }
                    ui.label(
                        RichText::new(key_name(self.selected().key_code(button)))
                            .monospace()
                            .color(Color32::from_rgb(210, 214, 226)),
                    );
                });
            });
        }
        if ui.button("Reset to defaults").clicked() {
            self.update_selected(true, |profile| profile.key_map = default_key_map());
        }
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        section_title(ui, "Detection");
        let profile = self.selected().clone();
        slider(ui, "Noise gate", profile.gate_rms, 0.001, 0.1, |value| {
            self.update_selected(true, |profile| profile.gate_rms = value);
        });
        level_meter(ui, self.engine.snapshot().rms, profile.gate_rms);
        slider_f64(
            ui,
            "Hysteresis (semitones)",
            profile.hysteresis_semitones,
            0.0,
            2.0,
            |value| {
                self.update_selected(true, |profile| profile.hysteresis_semitones = value);
            },
        );
        let mut hold = profile.min_hold_ms as f32;
        ui.horizontal(|ui| {
            ui.label("Min hold (ms)");
            if ui
                .add(egui::Slider::new(&mut hold, 0.0..=300.0).step_by(10.0))
                .changed()
            {
                let hold = hold.round() as i32;
                self.update_selected(true, |profile| profile.min_hold_ms = hold);
            }
        });
        slider(
            ui,
            "Confidence threshold",
            profile.confidence_threshold,
            0.4,
            0.95,
            |value| {
                self.update_selected(true, |profile| profile.confidence_threshold = value);
            },
        );
        ui.add_space(8.0);
        ui.separator();
        ui.add_space(4.0);
        section_title(ui, "Gameplay overlay");
        let mut visible = self.settings.overlay_visible;
        if ui
            .checkbox(&mut visible, "Show overlay while listening")
            .changed()
        {
            self.set_overlay_visible(visible);
        }
        let mut edit = self.settings.overlay_edit;
        if ui.checkbox(&mut edit, "Edit overlay position").changed() {
            self.settings.overlay_edit = edit;
            if edit {
                self.settings.overlay_visible = true;
            }
        }
        let mut opacity = self.settings.overlay_opacity as f32;
        if ui
            .add(egui::Slider::new(&mut opacity, 0.3..=1.0).text("Opacity"))
            .changed()
        {
            self.settings.overlay_opacity = f64::from(opacity);
            save_settings(&self.settings);
        }
        ui.add_space(4.0);
        ui.label(
            RichText::new(format!(
                "{cmd}⇧O shows the overlay. {cmd}⇧P toggles output. Drag the overlay while edit mode is on; corners resize it.",
                cmd = command_label()
            ))
            .small()
            .weak(),
        );
    }

    fn capture_key(&mut self, ui: &mut egui::Ui) {
        if self.listening.is_none() {
            return;
        }
        let events: Vec<_> = ui.ctx().input(|input| {
            input
                .events
                .iter()
                .filter_map(|event| match event {
                    egui::Event::Key {
                        key,
                        physical_key,
                        pressed: true,
                        repeat: false,
                        ..
                    } => Some(physical_key.unwrap_or(*key)),
                    _ => None,
                })
                .collect()
        });
        if let Some(code) = events.iter().find_map(|key| code_for_key(*key)) {
            let button = self.listening.take().unwrap();
            self.update_selected(true, |profile| profile.set_key_code(code, button));
            return;
        }
        let mods = ui.ctx().input(|input| input.modifiers);
        let code = if mods.shift && !self.prev_mods.shift {
            Some(56)
        } else if mods.command && !self.prev_mods.command && events.is_empty() {
            Some(55)
        } else if mods.ctrl && !self.prev_mods.ctrl && events.is_empty() {
            Some(59)
        } else if mods.alt && !self.prev_mods.alt && events.is_empty() {
            Some(58)
        } else {
            None
        };
        self.prev_mods = mods;
        if let Some(code) = code
            && let Some(button) = self.listening.take()
        {
            self.update_selected(true, |profile| profile.set_key_code(code, button));
        }
    }
}

impl Hotkeys {
    fn install() -> Option<Self> {
        let manager = GlobalHotKeyManager::new().ok()?;
        let mods = CMD_OR_CTRL | HotModifiers::SHIFT;
        let output = HotKey::new(Some(mods), Code::KeyP);
        let overlay = HotKey::new(Some(mods), Code::KeyO);
        manager.register(output).ok()?;
        if manager.register(overlay).is_err() {
            let _ = manager.unregister(output);
            return None;
        }
        Some(Self {
            output_id: output.id(),
            overlay_id: overlay.id(),
            _manager: manager,
        })
    }
}

fn apply_style(ctx: &egui::Context) {
    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.style_mut_of(egui::Theme::Dark, |style| {
        let text = Color32::from_rgb(226, 228, 236);
        let raised = Color32::from_rgb(40, 44, 62);
        let raised_hover = Color32::from_rgb(52, 58, 82);
        let radius = CornerRadius::same(6);

        style.visuals.dark_mode = true;
        style.visuals.override_text_color = None;
        style.visuals.weak_text_alpha = 0.62;
        style.visuals.window_fill = BG;
        style.visuals.panel_fill = BG;
        style.visuals.extreme_bg_color = Color32::from_rgb(12, 14, 22);
        style.visuals.faint_bg_color = Color32::from_white_alpha(8);
        style.visuals.code_bg_color = Color32::from_rgb(32, 36, 52);
        style.visuals.hyperlink_color = ACCENT;
        style.visuals.selection.bg_fill = Color32::from_rgba_unmultiplied(52, 168, 164, 56);
        style.visuals.selection.stroke = Stroke::new(1.0, ACCENT);
        style.visuals.window_corner_radius = CornerRadius::same(10);
        style.visuals.window_stroke = Stroke::new(1.0, Color32::from_white_alpha(18));
        style.visuals.widgets.noninteractive.fg_stroke =
            Stroke::new(1.0, Color32::from_rgb(186, 190, 206));
        style.visuals.widgets.noninteractive.bg_stroke =
            Stroke::new(1.0, Color32::from_white_alpha(16));
        style.visuals.widgets.noninteractive.corner_radius = radius;

        style.visuals.widgets.inactive.corner_radius = radius;
        style.visuals.widgets.inactive.weak_bg_fill = raised;
        style.visuals.widgets.inactive.bg_fill = raised;
        style.visuals.widgets.inactive.fg_stroke = Stroke::new(1.0, text);
        style.visuals.widgets.inactive.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(10));

        style.visuals.widgets.hovered.corner_radius = radius;
        style.visuals.widgets.hovered.weak_bg_fill = raised_hover;
        style.visuals.widgets.hovered.bg_fill = raised_hover;
        style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0, Color32::WHITE);
        style.visuals.widgets.hovered.bg_stroke = Stroke::new(1.0, Color32::from_white_alpha(36));

        style.visuals.widgets.active.corner_radius = radius;
        style.visuals.widgets.active.weak_bg_fill = Color32::from_rgb(30, 34, 50);
        style.visuals.widgets.active.bg_fill = Color32::from_rgb(30, 34, 50);
        style.visuals.widgets.active.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        style.visuals.widgets.open.corner_radius = radius;
        style.visuals.widgets.open.weak_bg_fill = raised_hover;
        style.visuals.widgets.open.bg_fill = raised_hover;
        style.visuals.widgets.open.fg_stroke = Stroke::new(1.0, Color32::WHITE);

        style.spacing.item_spacing = Vec2::new(8.0, 6.0);
        style.spacing.button_padding = Vec2::new(10.0, 4.0);
        style.spacing.interact_size.y = 26.0;
    });
}

fn card() -> egui::Frame {
    egui::Frame::new()
        .fill(Color32::from_white_alpha(8))
        .stroke(Stroke::new(1.0, Color32::from_white_alpha(14)))
        .corner_radius(CornerRadius::same(12))
        .inner_margin(egui::Margin::same(12))
}

fn section_title(ui: &mut egui::Ui, text: &str) {
    ui.label(RichText::new(text).strong().size(15.0));
}

fn status_chip(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(color.gamma_multiply(0.22))
        .corner_radius(CornerRadius::same(8))
        .inner_margin(egui::Margin::symmetric(10, 5))
        .show(ui, |ui| {
            ui.label(RichText::new(text).color(color).strong());
        });
}

fn metric(ui: &mut egui::Ui, title: &str, value: String, width: f32) {
    ui.allocate_ui_with_layout(
        Vec2::new(width, 36.0),
        egui::Layout::top_down(egui::Align::LEFT),
        |ui| {
            ui.label(RichText::new(title.to_uppercase()).size(10.0).weak());
            ui.label(RichText::new(value).monospace().strong().size(16.0));
        },
    );
}

fn level_meter(ui: &mut egui::Ui, level: f32, gate: f32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(140.0, 8.0), egui::Sense::hover());
    let painter = ui.painter();
    painter.rect_filled(rect, CornerRadius::same(4), Color32::from_white_alpha(22));
    let width = rect.width() * (level / 0.15).clamp(0.0, 1.0);
    let bar = egui::Rect::from_min_size(rect.min, Vec2::new(width.max(2.0), rect.height()));
    let color = if level >= gate {
        Color32::from_rgb(70, 180, 90)
    } else {
        Color32::from_rgb(220, 150, 40)
    };
    painter.rect_filled(bar, CornerRadius::same(5), color);
    let gate_x = rect.left() + rect.width() * (gate / 0.15).clamp(0.0, 1.0);
    painter.line_segment(
        [
            egui::pos2(gate_x, rect.top()),
            egui::pos2(gate_x, rect.bottom()),
        ],
        egui::Stroke::new(2.0, Color32::from_white_alpha(180)),
    );
}

fn slider(
    ui: &mut egui::Ui,
    label: &str,
    value: f32,
    min: f32,
    max: f32,
    mut changed: impl FnMut(f32),
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut value = value;
        if ui.add(egui::Slider::new(&mut value, min..=max)).changed() {
            changed(value);
        }
    });
}

fn slider_f64(
    ui: &mut egui::Ui,
    label: &str,
    value: f64,
    min: f64,
    max: f64,
    mut changed: impl FnMut(f64),
) {
    ui.horizontal(|ui| {
        ui.label(label);
        let mut value = value;
        if ui.add(egui::Slider::new(&mut value, min..=max)).changed() {
            changed(value);
        }
    });
}

pub fn hue_color(hue: f64, alpha: f32) -> Color32 {
    let color = egui::ecolor::Hsva {
        h: hue.rem_euclid(1.0) as f32,
        s: 0.55,
        v: 0.85,
        a: alpha,
    };
    Color32::from(color)
}

fn command_label() -> &'static str {
    if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    }
}

fn code_for_key(key: Key) -> Option<u16> {
    Some(match key {
        Key::A => 0,
        Key::S => 1,
        Key::D => 2,
        Key::F => 3,
        Key::H => 4,
        Key::G => 5,
        Key::Z => 6,
        Key::X => 7,
        Key::C => 8,
        Key::V => 9,
        Key::B => 11,
        Key::Q => 12,
        Key::W => 13,
        Key::E => 14,
        Key::R => 15,
        Key::Y => 16,
        Key::T => 17,
        Key::Num1 => 18,
        Key::Num2 => 19,
        Key::Num3 => 20,
        Key::Num4 => 21,
        Key::Num6 => 22,
        Key::Num5 => 23,
        Key::Equals => 24,
        Key::Num9 => 25,
        Key::Num7 => 26,
        Key::Minus => 27,
        Key::Num8 => 28,
        Key::Num0 => 29,
        Key::CloseBracket => 30,
        Key::O => 31,
        Key::U => 32,
        Key::OpenBracket => 33,
        Key::I => 34,
        Key::P => 35,
        Key::Enter => 36,
        Key::L => 37,
        Key::J => 38,
        Key::Quote => 39,
        Key::K => 40,
        Key::Semicolon => 41,
        Key::Backslash => 42,
        Key::Comma => 43,
        Key::Slash => 44,
        Key::N => 45,
        Key::M => 46,
        Key::Period => 47,
        Key::Tab => 48,
        Key::Space => 49,
        Key::Backtick => 50,
        Key::Backspace | Key::Delete => 51,
        Key::Escape => 53,
        Key::ArrowLeft => 123,
        Key::ArrowRight => 124,
        Key::ArrowDown => 125,
        Key::ArrowUp => 126,
        _ => return None,
    })
}

fn accessibility_trusted() -> bool {
    #[cfg(target_os = "macos")]
    {
        #[link(name = "ApplicationServices", kind = "framework")]
        unsafe extern "C" {
            fn AXIsProcessTrusted() -> u8;
        }
        unsafe { AXIsProcessTrusted() != 0 }
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

fn open_accessibility_settings() {
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg("x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")
            .spawn();
    }
}

fn platform_note() -> Option<String> {
    #[cfg(target_os = "linux")]
    {
        if std::env::var("XDG_SESSION_TYPE").ok().as_deref() == Some("wayland") {
            return Some(
                "This is a Wayland session. Key injection and global hotkeys need X11, or an XWayland app that accepts synthetic keys."
                    .to_string(),
            );
        }
    }
    None
}
