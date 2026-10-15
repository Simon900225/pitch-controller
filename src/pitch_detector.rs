#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PitchDetectionResult {
    pub frequency_hz: Option<f64>,
    pub confidence: f32,
    pub rms: f32,
}

impl PitchDetectionResult {
    fn silent(rms: f32) -> Self {
        Self {
            frequency_hz: None,
            confidence: 0.0,
            rms,
        }
    }
}

/// YIN pitch detector. The difference function matches the macOS app:
/// each lag is summed over `buffer_size / 2` samples.
pub struct PitchDetector {
    pub sample_rate: f64,
    pub buffer_size: usize,
    pub min_frequency: f64,
    pub max_frequency: f64,
    min_period: usize,
    max_period: usize,
    yin_buffer: Vec<f32>,
    difference: Vec<f32>,
}

impl PitchDetector {
    pub fn new(
        sample_rate: f64,
        buffer_size: usize,
        min_frequency: f64,
        max_frequency: f64,
    ) -> Self {
        let half = buffer_size / 2;
        let min_period = (sample_rate / max_frequency).floor() as usize;
        let min_period = min_period.max(2);
        let max_period = half.min((sample_rate / min_frequency).floor() as usize);
        Self {
            sample_rate,
            buffer_size,
            min_frequency,
            max_frequency,
            min_period,
            max_period,
            yin_buffer: vec![0.0; half],
            difference: vec![0.0; half],
        }
    }

    pub fn with_sample_rate(sample_rate: f64) -> Self {
        Self::new(sample_rate, 2048, 70.0, 1000.0)
    }

    pub fn detect(&mut self, samples: &[f32]) -> PitchDetectionResult {
        if samples.len() < self.buffer_size {
            return PitchDetectionResult::silent(0.0);
        }
        let half = self.buffer_size / 2;
        let samples = &samples[..self.buffer_size];

        let mut sum_sq = 0.0_f32;
        for sample in samples {
            sum_sq += sample * sample;
        }
        let rms = (sum_sq / self.buffer_size as f32).sqrt();

        if self.min_period >= self.max_period || self.max_period > half {
            return PitchDetectionResult::silent(rms);
        }

        for tau in 0..half {
            let mut sum = 0.0_f32;
            let mut i = 0;
            while i < half {
                let delta = samples[i] - samples[i + tau];
                sum += delta * delta;
                i += 1;
            }
            self.difference[tau] = sum;
        }

        self.yin_buffer[0] = 1.0;
        let mut running_sum = 0.0_f32;
        for tau in 1..half {
            running_sum += self.difference[tau];
            self.yin_buffer[tau] = if running_sum == 0.0 {
                1.0
            } else {
                self.difference[tau] * tau as f32 / running_sum
            };
        }

        let threshold = 0.15_f32;
        let mut tau_estimate = -1_i32;
        let mut tau = self.min_period;
        while tau < self.max_period {
            if self.yin_buffer[tau] < threshold {
                while tau + 1 < self.max_period && self.yin_buffer[tau + 1] < self.yin_buffer[tau] {
                    tau += 1;
                }
                tau_estimate = tau as i32;
                break;
            }
            tau += 1;
        }

        if tau_estimate < 0 {
            let mut min_value = self.yin_buffer[self.min_period];
            let mut min_index = self.min_period;
            for i in (self.min_period + 1)..self.max_period {
                if self.yin_buffer[i] < min_value {
                    min_value = self.yin_buffer[i];
                    min_index = i;
                }
            }
            if min_value < 0.5 {
                tau_estimate = min_index as i32;
            }
        }

        if tau_estimate <= 0 {
            return PitchDetectionResult {
                frequency_hz: None,
                confidence: 0.0,
                rms,
            };
        }

        let tau_estimate = tau_estimate as usize;
        let better_tau = self.parabolic(tau_estimate);
        let frequency = self.sample_rate / f64::from(better_tau);
        let confidence = (1.0 - self.yin_buffer[tau_estimate]).clamp(0.0, 1.0);
        if !(self.min_frequency..=self.max_frequency).contains(&frequency) {
            return PitchDetectionResult {
                frequency_hz: None,
                confidence,
                rms,
            };
        }
        PitchDetectionResult {
            frequency_hz: Some(frequency),
            confidence,
            rms,
        }
    }

    fn parabolic(&self, tau: usize) -> f32 {
        if tau == 0 || tau + 1 >= self.yin_buffer.len() {
            return tau as f32;
        }
        let s0 = self.yin_buffer[tau - 1];
        let s1 = self.yin_buffer[tau];
        let s2 = self.yin_buffer[tau + 1];
        let denom = 2.0 * s1 - s2 - s0;
        if denom.abs() <= 1e-6 {
            return tau as f32;
        }
        let adjustment = (s2 - s0) / (2.0 * denom);
        tau as f32 + adjustment
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(frequency: f64, sample_rate: f64, count: usize) -> Vec<f32> {
        (0..count)
            .map(|i| (2.0 * std::f64::consts::PI * frequency * i as f64 / sample_rate).sin() as f32)
            .collect()
    }

    fn saw(frequency: f64, sample_rate: f64, count: usize) -> Vec<f32> {
        (0..count)
            .map(|i| {
                let phase = (i as f64 * frequency / sample_rate) % 1.0;
                (2.0 * phase - 1.0) as f32
            })
            .collect()
    }

    #[test]
    fn detects_sine_within_one_percent() {
        let sample_rate = 44_100.0;
        let frequency = 220.0;
        let buffer_size = 2048;
        let samples = sine(frequency, sample_rate, buffer_size);
        let mut detector = PitchDetector::new(sample_rate, buffer_size, 70.0, 1000.0);
        let result = detector.detect(&samples);
        let detected = result.frequency_hz.expect("pitch");
        let error = (detected - frequency).abs() / frequency;
        assert!(
            error < 0.01,
            "detected {detected} Hz, expected {frequency} Hz"
        );
        assert!(result.confidence > 0.5);
        assert!(result.rms > 0.01);
    }

    #[test]
    fn detects_sawtooth_near_expected_frequency() {
        let sample_rate = 44_100.0;
        let frequency = 330.0;
        let buffer_size = 2048;
        let samples = saw(frequency, sample_rate, buffer_size);
        let mut detector = PitchDetector::new(sample_rate, buffer_size, 70.0, 1000.0);
        let result = detector.detect(&samples);
        let detected = result.frequency_hz.expect("pitch");
        let error = (detected - frequency).abs() / frequency;
        assert!(
            error < 0.03,
            "detected {detected} Hz, expected {frequency} Hz"
        );
    }

    #[test]
    fn silence_yields_no_pitch() {
        let samples = vec![0.0; 2048];
        let mut detector = PitchDetector::with_sample_rate(44_100.0);
        let result = detector.detect(&samples);
        assert!(result.frequency_hz.is_none());
        assert!(result.rms.abs() < 0.0001);
    }
}
