use std::collections::BTreeSet;

use crate::buttons::SnesButton;

/// Diffs button sets and only emits key-down / key-up for changes.
#[derive(Default)]
pub struct DiffingInputSink {
    pressed: BTreeSet<SnesButton>,
}

impl DiffingInputSink {
    pub fn apply(&mut self, next: &BTreeSet<SnesButton>) -> Vec<(SnesButton, bool)> {
        let mut events = Vec::new();
        let to_release: Vec<_> = self.pressed.difference(next).copied().collect();
        let to_press: Vec<_> = next.difference(&self.pressed).copied().collect();
        for button in to_release {
            events.push((button, false));
        }
        for button in to_press {
            events.push((button, true));
        }
        self.pressed = next.clone();
        events
    }

    pub fn release_all(&mut self) -> Vec<(SnesButton, bool)> {
        self.apply(&BTreeSet::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diffing_emits_only_changes() {
        let mut sink = DiffingInputSink::default();
        let mut next = BTreeSet::from([SnesButton::Right, SnesButton::B]);
        let events = sink.apply(&next);
        assert_eq!(
            events
                .iter()
                .map(|(button, _)| *button)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([SnesButton::Right, SnesButton::B])
        );
        assert!(events.iter().all(|(_, down)| *down));

        next = BTreeSet::from([SnesButton::Right, SnesButton::Y]);
        let events = sink.apply(&next);
        let released: BTreeSet<_> = events
            .iter()
            .filter(|(_, down)| !down)
            .map(|(b, _)| *b)
            .collect();
        let pressed: BTreeSet<_> = events
            .iter()
            .filter(|(_, down)| *down)
            .map(|(b, _)| *b)
            .collect();
        assert_eq!(released, BTreeSet::from([SnesButton::B]));
        assert_eq!(pressed, BTreeSet::from([SnesButton::Y]));

        let events = sink.release_all();
        assert_eq!(
            events
                .iter()
                .map(|(button, _)| *button)
                .collect::<BTreeSet<_>>(),
            BTreeSet::from([SnesButton::Right, SnesButton::Y])
        );
        assert!(events.iter().all(|(_, down)| !down));
    }

    #[test]
    fn idempotent_apply_produces_no_events() {
        let mut sink = DiffingInputSink::default();
        sink.apply(&BTreeSet::from([SnesButton::A]));
        let events = sink.apply(&BTreeSet::from([SnesButton::A]));
        assert!(events.is_empty());
    }
}
