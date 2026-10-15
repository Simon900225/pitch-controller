use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::buttons::{SnesButton, default_key_code, default_key_map};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BandTriggerMode {
    Hold,
    Tap,
    Repeating,
}

impl BandTriggerMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Hold => "Hold",
            Self::Tap => "Single press",
            Self::Repeating => "Repeat",
        }
    }

    pub fn help(self, repeat_ms: i32) -> String {
        match self {
            Self::Hold => "Buttons stay down while your pitch is in this band.".to_string(),
            Self::Tap => "Buttons press once when your pitch enters this band, then release until you leave and enter again.".to_string(),
            Self::Repeating => format!(
                "Buttons press again every {repeat_ms} ms while your pitch stays in this band."
            ),
        }
    }
}

impl Default for BandTriggerMode {
    fn default() -> Self {
        Self::Hold
    }
}

fn default_repeat() -> i32 {
    200
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PitchBand {
    pub id: Uuid,
    pub name: String,
    /// Inclusive lower bound in MIDI note numbers.
    pub low_note: f64,
    /// Exclusive upper bound in MIDI note numbers. Bands may overlap.
    pub high_note: f64,
    pub buttons: BTreeSet<SnesButton>,
    /// Hue 0...1.
    pub hue: f64,
    #[serde(default)]
    pub trigger: BandTriggerMode,
    #[serde(default = "default_repeat")]
    pub repeat_interval_ms: i32,
}

impl PitchBand {
    pub fn new(
        name: impl Into<String>,
        low_note: f64,
        high_note: f64,
        buttons: BTreeSet<SnesButton>,
        hue: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            low_note,
            high_note,
            buttons,
            hue,
            trigger: BandTriggerMode::Hold,
            repeat_interval_ms: 200,
        }
    }

    pub fn contains(&self, note: f64) -> bool {
        note >= self.low_note && note < self.high_note
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Profile {
    pub id: Uuid,
    pub name: String,
    pub bands: Vec<PitchBand>,
    /// Virtual key codes keyed by button name. These are macOS virtual key codes
    /// so a profile written by the Swift app still loads.
    pub key_map: BTreeMap<String, u16>,
    pub gate_rms: f32,
    pub hysteresis_semitones: f64,
    pub min_hold_ms: i32,
    pub confidence_threshold: f32,
}

impl Profile {
    pub fn new(name: impl Into<String>, mut bands: Vec<PitchBand>) -> Self {
        bands.sort_by(|a, b| {
            a.low_note
                .partial_cmp(&b.low_note)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        Self {
            id: Uuid::new_v4(),
            name: name.into(),
            bands,
            key_map: default_key_map(),
            gate_rms: 0.02,
            hysteresis_semitones: 0.4,
            min_hold_ms: 80,
            confidence_threshold: 0.75,
        }
    }

    pub fn mario() -> Self {
        let mut profile = Self::new(
            "Mario",
            vec![
                PitchBand::new("Walk", 55.0, 60.0, set([SnesButton::Right]), 0.55),
                PitchBand::new(
                    "Run",
                    60.0,
                    65.0,
                    set([SnesButton::Right, SnesButton::Y]),
                    0.35,
                ),
                PitchBand::new(
                    "Run+Jump",
                    65.0,
                    70.0,
                    set([SnesButton::Right, SnesButton::Y, SnesButton::B]),
                    0.12,
                ),
                PitchBand::new("Jump", 70.0, 78.0, set([SnesButton::B]), 0.95),
            ],
        );
        profile.min_hold_ms = 100;
        profile
    }

    pub fn empty() -> Self {
        Self::new(
            "New Profile",
            vec![
                PitchBand::new("Band 1", 55.0, 62.0, set([SnesButton::Right]), 0.55),
                PitchBand::new("Band 2", 62.0, 70.0, set([SnesButton::B]), 0.12),
            ],
        )
    }

    pub fn key_code(&self, button: SnesButton) -> u16 {
        self.key_map
            .get(button.as_str())
            .copied()
            .unwrap_or_else(|| default_key_code(button))
    }

    pub fn set_key_code(&mut self, code: u16, button: SnesButton) {
        self.key_map.insert(button.as_str().to_string(), code);
    }

    /// Shift a band by `delta` columns. Negative moves left, positive moves right.
    /// Stops at either end instead of wrapping.
    pub fn move_band(&mut self, id: Uuid, delta: i32) -> bool {
        if delta == 0 || self.bands.is_empty() {
            return false;
        }
        let Some(index) = self.bands.iter().position(|band| band.id == id) else {
            return false;
        };
        let last = self.bands.len() as i32 - 1;
        let target = (index as i32 + delta).clamp(0, last) as usize;
        if target == index {
            return false;
        }
        let band = self.bands.remove(index);
        self.bands.insert(target, band);
        true
    }

    /// Band order is the editor's column order, so bands are not re-sorted here.
    pub fn normalize_bands(&mut self) {
        for band in &mut self.bands {
            if band.high_note < band.low_note {
                band.high_note = band.low_note;
            }
        }
    }
}

pub fn set(buttons: impl IntoIterator<Item = SnesButton>) -> BTreeSet<SnesButton> {
    buttons.into_iter().collect()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub selected_profile_id: Option<Uuid>,
    #[serde(default = "default_opacity")]
    pub overlay_opacity: f64,
    #[serde(default = "default_true")]
    pub overlay_visible: bool,
    #[serde(default)]
    pub overlay_edit: bool,
    #[serde(default = "default_overlay_x")]
    pub overlay_x: f32,
    #[serde(default = "default_overlay_y")]
    pub overlay_y: f32,
    #[serde(default = "default_overlay_w")]
    pub overlay_w: f32,
    #[serde(default = "default_overlay_h")]
    pub overlay_h: f32,
}

fn default_opacity() -> f64 {
    0.9
}
fn default_true() -> bool {
    true
}
fn default_overlay_x() -> f32 {
    40.0
}
fn default_overlay_y() -> f32 {
    120.0
}
fn default_overlay_w() -> f32 {
    110.0
}
fn default_overlay_h() -> f32 {
    320.0
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            selected_profile_id: None,
            overlay_opacity: default_opacity(),
            overlay_visible: true,
            overlay_edit: false,
            overlay_x: default_overlay_x(),
            overlay_y: default_overlay_y(),
            overlay_w: default_overlay_w(),
            overlay_h: default_overlay_h(),
        }
    }
}

pub fn app_dir() -> PathBuf {
    directories::BaseDirs::new()
        .map(|dirs| dirs.data_dir().join("PitchController"))
        .unwrap_or_else(|| PathBuf::from("PitchController"))
}

pub fn load_profiles() -> Vec<Profile> {
    let path = app_dir().join("profiles.json");
    if !path.exists() {
        let defaults = vec![Profile::mario()];
        save_profiles(&defaults);
        return defaults;
    }
    match fs::read(&path)
        .ok()
        .and_then(|data| serde_json::from_slice::<Vec<Profile>>(&data).ok())
    {
        Some(profiles) if !profiles.is_empty() => profiles,
        _ => vec![Profile::mario()],
    }
}

pub fn save_profiles(profiles: &[Profile]) {
    if let Ok(data) = serde_json::to_vec_pretty(profiles) {
        write_atomic(&app_dir().join("profiles.json"), &data);
    }
}

pub fn load_settings() -> AppSettings {
    let path = app_dir().join("settings.json");
    fs::read(&path)
        .ok()
        .and_then(|data| serde_json::from_slice::<AppSettings>(&data).ok())
        .unwrap_or_default()
}

pub fn save_settings(settings: &AppSettings) {
    if let Ok(data) = serde_json::to_vec_pretty(settings) {
        write_atomic(&app_dir().join("settings.json"), &data);
    }
}

fn write_atomic(path: &Path, data: &[u8]) {
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let tmp = path.with_extension("json.tmp");
    if fs::write(&tmp, data).is_ok() {
        let _ = fs::rename(&tmp, path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mario_profile_has_expected_bands() {
        let mario = Profile::mario();
        assert_eq!(mario.bands.len(), 4);
        assert_eq!(mario.bands[0].buttons, set([SnesButton::Right]));
        assert_eq!(
            mario.bands[1].buttons,
            set([SnesButton::Right, SnesButton::Y])
        );
        assert_eq!(
            mario.bands[2].buttons,
            set([SnesButton::Right, SnesButton::Y, SnesButton::B])
        );
        assert_eq!(mario.bands[3].buttons, set([SnesButton::B]));
        assert!(
            mario
                .bands
                .iter()
                .all(|band| band.trigger == BandTriggerMode::Hold)
        );
    }

    #[test]
    fn decodes_band_without_trigger_as_hold() {
        let id = Uuid::new_v4();
        let json = format!(
            r#"{{"id":"{id}","name":"Walk","lowNote":55,"highNote":60,"buttons":["right"],"hue":0.5}}"#
        );
        let band: PitchBand = serde_json::from_str(&json).unwrap();
        assert_eq!(band.trigger, BandTriggerMode::Hold);
        assert_eq!(band.repeat_interval_ms, 200);
        assert_eq!(band.buttons, set([SnesButton::Right]));

        let encoded = serde_json::to_vec(&band).unwrap();
        let decoded: PitchBand = serde_json::from_slice(&encoded).unwrap();
        assert_eq!(decoded, band);
    }

    #[test]
    fn move_band_shifts_column_and_stops_at_the_ends() {
        let mut profile = Profile::mario();
        let ids: Vec<_> = profile.bands.iter().map(|band| band.id).collect();

        assert!(profile.move_band(ids[1], -1));
        assert_eq!(profile.bands[0].id, ids[1]);
        assert_eq!(profile.bands[1].id, ids[0]);

        assert!(!profile.move_band(ids[1], -1));
        assert_eq!(profile.bands[0].id, ids[1]);

        assert!(profile.move_band(ids[2], 5));
        assert_eq!(profile.bands.last().unwrap().id, ids[2]);
        assert!(!profile.move_band(ids[2], 1));
        assert!(!profile.move_band(Uuid::new_v4(), 1));
        assert!(!profile.move_band(ids[0], 0));
    }
}
