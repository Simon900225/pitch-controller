use crate::pitch_detector::PitchDetectionResult;
use crate::pitch_math;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SmoothedPitch {
    pub note: Option<f64>,
    pub frequency_hz: Option<f64>,
    pub confidence: f32,
    pub rms: f32,
    pub is_voiced: bool,
}

pub struct PitchSmoother {
    pub gate_rms: f32,
    pub confidence_threshold: f32,
    pub alpha: f64,
    history: Vec<f64>,
    history_size: usize,
    smoothed_note: Option<f64>,
}

impl PitchSmoother {
    pub fn new() -> Self {
        Self {
            gate_rms: 0.02,
            confidence_threshold: 0.75,
            alpha: 0.35,
            history: Vec::new(),
            history_size: 5,
            smoothed_note: None,
        }
    }

    pub fn with_thresholds(gate_rms: f32, confidence_threshold: f32) -> Self {
        Self {
            gate_rms,
            confidence_threshold,
            ..Self::new()
        }
    }

    pub fn reset(&mut self) {
        self.history.clear();
        self.smoothed_note = None;
    }

    pub fn process(&mut self, result: PitchDetectionResult) -> SmoothedPitch {
        let voiced = result.rms >= self.gate_rms
            && result.confidence >= self.confidence_threshold
            && result.frequency_hz.is_some();

        let Some(hz) = result.frequency_hz.filter(|_| voiced) else {
            self.history.clear();
            self.smoothed_note = None;
            return SmoothedPitch {
                note: None,
                frequency_hz: None,
                confidence: result.confidence,
                rms: result.rms,
                is_voiced: false,
            };
        };

        let raw = pitch_math::note_number(hz);
        self.history.push(raw);
        if self.history.len() > self.history_size {
            self.history.remove(0);
        }
        let median = median(&self.history);
        self.smoothed_note = Some(match self.smoothed_note {
            Some(previous) => previous + self.alpha * (median - previous),
            None => median,
        });
        let note = self.smoothed_note.unwrap();
        SmoothedPitch {
            note: Some(note),
            frequency_hz: Some(pitch_math::frequency_hz(note)),
            confidence: result.confidence,
            rms: result.rms,
            is_voiced: true,
        }
    }
}

impl Default for PitchSmoother {
    fn default() -> Self {
        Self::new()
    }
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}
