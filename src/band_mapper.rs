use std::collections::{BTreeMap, BTreeSet, HashSet};

use uuid::Uuid;

use crate::buttons::SnesButton;
use crate::profile::{BandTriggerMode, PitchBand, Profile};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BandMapperResult {
    pub active_band_ids: HashSet<Uuid>,
    pub buttons: BTreeSet<SnesButton>,
}

/// Maps smoothed pitch to active bands (bands may overlap; their buttons are combined)
/// with edge hysteresis and a minimum hold time.
pub struct BandMapper {
    pub bands: Vec<PitchBand>,
    pub hysteresis_semitones: f64,
    pub min_hold_ms: i32,
    current_ids: HashSet<Uuid>,
    hold_until: f64,
    pulse_until: BTreeMap<Uuid, f64>,
    repeat_start: BTreeMap<Uuid, f64>,
}

impl BandMapper {
    /// How long a tap or one repeat cycle holds the key down.
    pub const PRESS_DURATION: f64 = 0.04;

    pub fn new(bands: Vec<PitchBand>, hysteresis_semitones: f64, min_hold_ms: i32) -> Self {
        Self {
            bands,
            hysteresis_semitones,
            min_hold_ms,
            current_ids: HashSet::new(),
            hold_until: f64::NEG_INFINITY,
            pulse_until: BTreeMap::new(),
            repeat_start: BTreeMap::new(),
        }
    }

    pub fn configure(&mut self, profile: &Profile) {
        self.bands = profile.bands.clone();
        self.hysteresis_semitones = profile.hysteresis_semitones;
        self.min_hold_ms = profile.min_hold_ms;
        self.current_ids
            .retain(|id| self.bands.iter().any(|band| band.id == *id));
    }

    pub fn reset(&mut self) {
        self.current_ids.clear();
        self.hold_until = f64::NEG_INFINITY;
        self.pulse_until.clear();
        self.repeat_start.clear();
    }

    pub fn map(&mut self, note: Option<f64>, now: f64) -> BandMapperResult {
        let previous_ids = self.current_ids.clone();
        let active_ids = if let Some(note) = note {
            let active = self.active_bands(note);
            let ids: HashSet<Uuid> = active.iter().map(|band| band.id).collect();
            if ids != self.current_ids {
                let mut buttons = BTreeSet::new();
                for band in &active {
                    buttons.extend(band.buttons.iter().copied());
                }
                if self.min_hold_ms > 0 && !buttons.is_empty() {
                    self.hold_until = now + f64::from(self.min_hold_ms) / 1000.0;
                }
            }
            self.current_ids = ids.clone();
            ids
        } else if now < self.hold_until {
            self.current_ids.clone()
        } else {
            self.current_ids.clear();
            HashSet::new()
        };

        let buttons = self.momentary_buttons(&active_ids, &previous_ids, now);
        BandMapperResult {
            active_band_ids: active_ids,
            buttons,
        }
    }

    fn momentary_buttons(
        &mut self,
        active_ids: &HashSet<Uuid>,
        previous_ids: &HashSet<Uuid>,
        now: f64,
    ) -> BTreeSet<SnesButton> {
        let known: HashSet<Uuid> = self.bands.iter().map(|band| band.id).collect();
        self.pulse_until.retain(|id, _| known.contains(id));
        self.repeat_start.retain(|id, _| known.contains(id));

        let mut down = BTreeSet::new();
        for band in &self.bands {
            let active = active_ids.contains(&band.id);
            let was_active = previous_ids.contains(&band.id);
            match band.trigger {
                BandTriggerMode::Hold => {
                    self.pulse_until.remove(&band.id);
                    self.repeat_start.remove(&band.id);
                    if active {
                        down.extend(band.buttons.iter().copied());
                    }
                }
                BandTriggerMode::Tap => {
                    self.repeat_start.remove(&band.id);
                    if active && !was_active {
                        self.pulse_until.insert(band.id, now + Self::PRESS_DURATION);
                    }
                    if self
                        .pulse_until
                        .get(&band.id)
                        .is_some_and(|until| now < *until)
                    {
                        down.extend(band.buttons.iter().copied());
                    } else {
                        self.pulse_until.remove(&band.id);
                    }
                }
                BandTriggerMode::Repeating => {
                    self.pulse_until.remove(&band.id);
                    if !active {
                        self.repeat_start.remove(&band.id);
                        continue;
                    }
                    if !was_active || !self.repeat_start.contains_key(&band.id) {
                        self.repeat_start.insert(band.id, now);
                    }
                    let interval = Self::PRESS_DURATION
                        .max(f64::from(band.repeat_interval_ms.max(1)) / 1000.0);
                    let started = self.repeat_start.get(&band.id).copied().unwrap_or(now);
                    let elapsed = (now - started).max(0.0);
                    let phase = elapsed % interval;
                    let pulse = Self::PRESS_DURATION.min(interval * 0.45);
                    if phase < pulse {
                        down.extend(band.buttons.iter().copied());
                    }
                }
            }
        }
        down
    }

    /// While the note sits in the hysteresis margin of a band that is already active, the active set
    /// is frozen; otherwise every band that strictly contains the note is active.
    fn active_bands(&self, note: f64) -> Vec<PitchBand> {
        let loosely: Vec<PitchBand> = self
            .bands
            .iter()
            .filter(|band| {
                self.current_ids.contains(&band.id)
                    && note >= band.low_note - self.hysteresis_semitones
                    && note < band.high_note + self.hysteresis_semitones
            })
            .cloned()
            .collect();
        if loosely.iter().any(|band| !band.contains(note)) {
            return loosely;
        }
        self.bands
            .iter()
            .filter(|band| band.contains(note))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::set;

    fn fixture() -> (BandMapper, Uuid, Uuid, Uuid) {
        let walk = PitchBand::new("Walk", 55.0, 60.0, set([SnesButton::Right]), 0.5);
        let run = PitchBand::new(
            "Run",
            60.0,
            65.0,
            set([SnesButton::Right, SnesButton::Y]),
            0.3,
        );
        let jump = PitchBand::new("Jump", 65.0, 72.0, set([SnesButton::B]), 0.1);
        let ids = (walk.id, run.id, jump.id);
        let mapper = BandMapper::new(vec![walk, run, jump], 0.5, 100);
        (mapper, ids.0, ids.1, ids.2)
    }

    #[test]
    fn maps_note_to_band() {
        let (mut mapper, walk, _, _) = fixture();
        let result = mapper.map(Some(57.0), 0.0);
        assert_eq!(result.active_band_ids, HashSet::from([walk]));
        assert_eq!(result.buttons, set([SnesButton::Right]));
    }

    #[test]
    fn silence_clears_buttons_after_hold() {
        let (mut mapper, _, _, _) = fixture();
        mapper.map(Some(57.0), 0.0);
        let during = mapper.map(None, 0.0);
        assert_eq!(during.buttons, set([SnesButton::Right]));
        let after = mapper.map(None, 0.2);
        assert!(after.active_band_ids.is_empty());
        assert!(after.buttons.is_empty());
    }

    #[test]
    fn hysteresis_keeps_band_near_edge() {
        let (mut mapper, walk, run, _) = fixture();
        mapper.map(Some(59.5), 0.0);
        let result = mapper.map(Some(60.3), 0.1);
        assert_eq!(result.active_band_ids, HashSet::from([walk]));
        let past = mapper.map(Some(60.6), 0.2);
        assert_eq!(past.active_band_ids, HashSet::from([run]));
    }

    #[test]
    fn min_hold_keeps_chirp() {
        let (mut mapper, _, _, _) = fixture();
        let press = mapper.map(Some(68.0), 0.0);
        assert_eq!(press.buttons, set([SnesButton::B]));
        let shortly = mapper.map(None, 0.05);
        assert_eq!(shortly.buttons, set([SnesButton::B]));
    }

    #[test]
    fn overlapping_bands_combine_buttons() {
        let move_band = PitchBand::new("Move", 55.0, 70.0, set([SnesButton::Right]), 0.5);
        let hop = PitchBand::new("Hop", 62.0, 66.0, set([SnesButton::B]), 0.1);
        let move_id = move_band.id;
        let hop_id = hop.id;
        let mut mapper = BandMapper::new(vec![move_band, hop], 0.5, 0);

        assert_eq!(
            mapper.map(Some(58.0), 0.0).buttons,
            set([SnesButton::Right])
        );
        let both = mapper.map(Some(64.0), 0.1);
        assert_eq!(both.active_band_ids, HashSet::from([move_id, hop_id]));
        assert_eq!(both.buttons, set([SnesButton::Right, SnesButton::B]));
        assert_eq!(
            mapper.map(Some(66.3), 0.2).active_band_ids,
            HashSet::from([move_id, hop_id])
        );
        assert_eq!(
            mapper.map(Some(67.0), 0.3).active_band_ids,
            HashSet::from([move_id])
        );
    }

    #[test]
    fn tap_fires_once_per_entry() {
        let mut jump = PitchBand::new("Jump", 65.0, 72.0, set([SnesButton::B]), 0.1);
        jump.trigger = BandTriggerMode::Tap;
        let id = jump.id;
        let mut mapper = BandMapper::new(vec![jump], 0.0, 0);

        assert_eq!(mapper.map(Some(68.0), 0.0).buttons, set([SnesButton::B]));
        let later = BandMapper::PRESS_DURATION + 0.01;
        let held = mapper.map(Some(68.0), later);
        assert_eq!(held.active_band_ids, HashSet::from([id]));
        assert!(held.buttons.is_empty());
        assert!(mapper.map(Some(69.0), later + 0.05).buttons.is_empty());
        assert!(
            mapper
                .map(Some(40.0), later + 0.1)
                .active_band_ids
                .is_empty()
        );
        assert_eq!(
            mapper.map(Some(68.0), later + 0.2).buttons,
            set([SnesButton::B])
        );
    }

    #[test]
    fn tap_completes_after_leaving() {
        let mut jump = PitchBand::new("Jump", 65.0, 72.0, set([SnesButton::B]), 0.1);
        jump.trigger = BandTriggerMode::Tap;
        let mut mapper = BandMapper::new(vec![jump], 0.0, 0);
        mapper.map(Some(68.0), 0.0);
        let left = mapper.map(Some(40.0), 0.01);
        assert!(left.active_band_ids.is_empty());
        assert_eq!(left.buttons, set([SnesButton::B]));
        let done = mapper.map(Some(40.0), BandMapper::PRESS_DURATION + 0.01);
        assert!(done.buttons.is_empty());
    }

    #[test]
    fn repeat_presses_on_interval() {
        let mut jump = PitchBand::new("Jump", 65.0, 72.0, set([SnesButton::B]), 0.1);
        jump.trigger = BandTriggerMode::Repeating;
        jump.repeat_interval_ms = 100;
        let id = jump.id;
        let mut mapper = BandMapper::new(vec![jump], 0.0, 0);

        let entered = mapper.map(Some(68.0), 0.0);
        assert_eq!(entered.buttons, set([SnesButton::B]));
        assert_eq!(entered.active_band_ids, HashSet::from([id]));

        let gap = mapper.map(Some(68.0), BandMapper::PRESS_DURATION + 0.01);
        assert_eq!(gap.active_band_ids, HashSet::from([id]));
        assert!(gap.buttons.is_empty());
        assert_eq!(mapper.map(Some(68.0), 0.105).buttons, set([SnesButton::B]));
        assert!(mapper.map(Some(40.0), 0.16).buttons.is_empty());
    }

    #[test]
    fn hold_and_tap_overlap() {
        let mut move_band = PitchBand::new("Move", 55.0, 70.0, set([SnesButton::Right]), 0.5);
        move_band.trigger = BandTriggerMode::Hold;
        let mut hop = PitchBand::new("Hop", 62.0, 66.0, set([SnesButton::B]), 0.1);
        hop.trigger = BandTriggerMode::Tap;
        let move_id = move_band.id;
        let hop_id = hop.id;
        let mut mapper = BandMapper::new(vec![move_band, hop], 0.0, 0);

        assert_eq!(
            mapper.map(Some(64.0), 0.0).buttons,
            set([SnesButton::Right, SnesButton::B])
        );
        let later = mapper.map(Some(64.0), BandMapper::PRESS_DURATION + 0.02);
        assert_eq!(later.active_band_ids, HashSet::from([move_id, hop_id]));
        assert_eq!(later.buttons, set([SnesButton::Right]));
    }
}
