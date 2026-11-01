//! Strict timing mode: judges press and release time of every play along note.
//!
//! Times are song seconds, the same coordinate system as note starts in the waterfall.

use std::collections::HashMap;

use midi_file::MidiNote;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Judgement {
    /// Not judged yet
    Pending,
    /// Pressed in time, waiting for the release
    Pressed,
    Hit,
    Miss,
    /// Jumped over by rewinding, not judged
    Skipped,
}

#[derive(Debug)]
struct JudgedNote {
    start: f32,
    end: f32,
    key: u8,
    judgement: Judgement,
}

pub struct TimingJudge {
    /// Parallel to the waterfall note list, `None` for notes that are not played by the user
    notes: Vec<Option<JudgedNote>>,
    /// Start of every note, sorted, for binary search
    starts: Vec<f32>,
    /// Key -> note currently held for it
    held: HashMap<u8, usize>,
    /// Every note before this index is final
    cursor: usize,
    /// Notes whose judgement changed since the last [`TimingJudge::take_changes`]
    changes: Vec<(usize, Judgement)>,
}

impl TimingJudge {
    /// `notes` must be sorted by start time
    pub fn new<'a>(
        notes: impl Iterator<Item = &'a MidiNote>,
        is_judged: impl Fn(&MidiNote) -> bool,
    ) -> Self {
        let notes: Vec<&MidiNote> = notes.collect();
        let starts = notes.iter().map(|n| n.start.as_secs_f32()).collect();
        let notes = notes
            .into_iter()
            .map(|note| {
                is_judged(note).then_some(JudgedNote {
                    start: note.start.as_secs_f32(),
                    end: note.end.as_secs_f32(),
                    key: note.note,
                    judgement: Judgement::Pending,
                })
            })
            .collect();

        Self {
            notes,
            starts,
            held: HashMap::new(),
            cursor: 0,
            changes: Vec::new(),
        }
    }

    fn set(&mut self, id: usize, judgement: Judgement) {
        if let Some(note) = &mut self.notes[id]
            && note.judgement != judgement
        {
            note.judgement = judgement;
            self.changes.push((id, judgement));
        }
    }

    pub fn press(&mut self, key: u8, time: f32, tolerance: f32) {
        let from = self
            .starts
            .partition_point(|&start| start < time - tolerance);
        let to = self
            .starts
            .partition_point(|&start| start <= time + tolerance);

        let candidate = (from..to)
            .filter_map(|id| Some((id, self.notes[id].as_ref()?)))
            .filter(|(_, n)| n.key == key && n.judgement == Judgement::Pending)
            .min_by(|(_, a), (_, b)| (a.start - time).abs().total_cmp(&(b.start - time).abs()))
            .map(|(id, _)| id);

        if let Some(id) = candidate {
            self.set(id, Judgement::Pressed);
            self.held.insert(key, id);
        }
    }

    pub fn release(&mut self, key: u8, time: f32, tolerance: f32) {
        let Some(id) = self.held.remove(&key) else {
            return;
        };
        let Some(note) = &self.notes[id] else {
            return;
        };

        if note.judgement == Judgement::Pressed {
            let judgement = if (note.end - time).abs() <= tolerance {
                Judgement::Hit
            } else {
                Judgement::Miss
            };
            self.set(id, judgement);
        }
    }

    /// Marks notes that were never pressed, or held for too long
    pub fn update(&mut self, time: f32, tolerance: f32) {
        while self.cursor < self.notes.len()
            && self.notes[self.cursor].as_ref().is_none_or(|n| {
                matches!(
                    n.judgement,
                    Judgement::Hit | Judgement::Miss | Judgement::Skipped
                )
            })
        {
            self.cursor += 1;
        }

        for id in self.cursor..self.notes.len() {
            let Some(note) = &self.notes[id] else {
                continue;
            };

            if note.start - tolerance > time {
                break;
            }

            let expired = match note.judgement {
                Judgement::Pending => note.start + tolerance < time,
                Judgement::Pressed => note.end + tolerance < time,
                _ => false,
            };

            if expired {
                self.set(id, Judgement::Miss);
            }
        }
    }

    /// Playback position jumped: everything from `time` on is judged again
    pub fn seek(&mut self, time: f32) {
        self.held.clear();
        self.cursor = 0;

        for id in 0..self.notes.len() {
            let Some(note) = &self.notes[id] else {
                continue;
            };

            let judgement = if note.start >= time {
                Judgement::Pending
            } else if matches!(note.judgement, Judgement::Pending | Judgement::Pressed) {
                Judgement::Skipped
            } else {
                continue;
            };

            self.set(id, judgement);
        }
    }

    pub fn take_changes(&mut self) -> std::vec::Drain<'_, (usize, Judgement)> {
        self.changes.drain(..)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    const TOL: f32 = 0.1;

    fn note(start_ms: u64, end_ms: u64, key: u8) -> MidiNote {
        MidiNote {
            id: 0,
            start: Duration::from_millis(start_ms),
            end: Duration::from_millis(end_ms),
            duration: Duration::from_millis(end_ms - start_ms),
            note: key,
            velocity: 100,
            channel: 0,
            track_id: 0,
            track_color_id: 0,
        }
    }

    fn judge(notes: &[MidiNote]) -> TimingJudge {
        TimingJudge::new(notes.iter(), |_| true)
    }

    fn changes(judge: &mut TimingJudge) -> Vec<(usize, Judgement)> {
        judge.take_changes().collect()
    }

    #[test]
    fn exact_press_and_release_is_hit() {
        let mut j = judge(&[note(1000, 2000, 60)]);
        j.press(60, 1.05, TOL);
        j.release(60, 1.95, TOL);
        assert_eq!(
            changes(&mut j),
            [(0, Judgement::Pressed), (0, Judgement::Hit)]
        );
    }

    #[test]
    fn released_too_early_is_miss() {
        let mut j = judge(&[note(1000, 2000, 60)]);
        j.press(60, 1.0, TOL);
        j.release(60, 1.5, TOL);
        assert_eq!(changes(&mut j).last(), Some(&(0, Judgement::Miss)));
    }

    #[test]
    fn held_too_long_is_miss() {
        let mut j = judge(&[note(1000, 2000, 60)]);
        j.press(60, 1.0, TOL);
        j.update(2.05, TOL);
        assert_eq!(changes(&mut j).last(), Some(&(0, Judgement::Pressed)));
        j.update(2.2, TOL);
        assert_eq!(changes(&mut j).last(), Some(&(0, Judgement::Miss)));
        // A late release does not change the verdict
        j.release(60, 2.3, TOL);
        assert!(changes(&mut j).is_empty());
    }

    #[test]
    fn pressed_too_early_or_late_is_ignored_then_missed() {
        let mut j = judge(&[note(1000, 2000, 60)]);
        j.press(60, 0.8, TOL);
        j.release(60, 0.85, TOL);
        assert!(changes(&mut j).is_empty());
        j.update(1.2, TOL);
        assert_eq!(changes(&mut j), [(0, Judgement::Miss)]);
        j.press(60, 1.2, TOL);
        assert!(changes(&mut j).is_empty());
    }

    #[test]
    fn wrong_key_does_not_count() {
        let mut j = judge(&[note(1000, 2000, 60)]);
        j.press(62, 1.0, TOL);
        assert!(changes(&mut j).is_empty());
    }

    #[test]
    fn repeated_notes_pick_the_closest() {
        let mut j = judge(&[note(1000, 1100, 60), note(1150, 1300, 60)]);
        j.press(60, 1.14, TOL);
        assert_eq!(changes(&mut j), [(1, Judgement::Pressed)]);
    }

    #[test]
    fn unjudged_notes_are_ignored() {
        let notes = [note(1000, 2000, 48), note(1000, 2000, 60)];
        let mut j = TimingJudge::new(notes.iter(), |n| n.note == 60);
        j.update(3.0, TOL);
        assert_eq!(changes(&mut j), [(1, Judgement::Miss)]);
    }

    #[test]
    fn seek_back_resets_later_notes() {
        let mut j = judge(&[note(1000, 2000, 60), note(3000, 4000, 62)]);
        j.update(5.0, TOL);
        changes(&mut j);
        j.seek(2.5);
        assert_eq!(changes(&mut j), [(1, Judgement::Pending)]);
    }
}
