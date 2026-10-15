const NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

/// MIDI note number for a frequency in Hz. A4 = 69 = 440 Hz.
pub fn note_number(hz: f64) -> f64 {
    69.0 + 12.0 * (hz / 440.0).log2()
}

pub fn frequency_hz(note: f64) -> f64 {
    440.0 * 2.0_f64.powf((note - 69.0) / 12.0)
}

pub fn note_name(note: f64) -> String {
    let rounded = note.round() as i32;
    let index = ((rounded % 12) + 12) % 12;
    let octave = (rounded / 12) - 1;
    format!("{}{octave}", NAMES[index as usize])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a4_is_440() {
        assert!((frequency_hz(69.0) - 440.0).abs() < 0.001);
        assert!((note_number(440.0) - 69.0).abs() < 0.001);
    }

    #[test]
    fn note_names() {
        assert_eq!(note_name(69.0), "A4");
        assert_eq!(note_name(60.0), "C4");
    }
}
