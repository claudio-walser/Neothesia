use std::path::Path;

use midi_file::MidiNote;
use midi_file::MidiTrack;
use neothesia_core::fingering::{FingeringStore, HandAssignment, NoteKey, resolve_hands};

use crate::context::Context;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum PlayerConfig {
    Mute,
    Auto,
    Human,
}

#[derive(Debug, Clone)]
pub struct TrackConfig {
    pub track_id: usize,
    pub player: PlayerConfig,
    pub visible: bool,
    pub hand: HandAssignment,
    /// Opacity of the track's notes in the waterfall while it is muted
    pub muted_opacity: f32,
}

impl TrackConfig {
    pub const DEFAULT_MUTED_OPACITY: f32 = 0.15;

    /// Opacity of the track's notes in the waterfall
    pub fn note_opacity(&self) -> f32 {
        match self.player {
            PlayerConfig::Mute => self.muted_opacity,
            PlayerConfig::Auto | PlayerConfig::Human => 1.0,
        }
    }
}

#[derive(Default, Debug, Clone)]
pub struct SongConfig {
    pub tracks: Box<[TrackConfig]>,
}

impl SongConfig {
    fn new(tracks: &[MidiTrack], fingering: &FingeringStore) -> Self {
        let tracks: Vec<_> = tracks
            .iter()
            .map(|t| {
                let is_drums = t.has_drums && !t.has_other_than_drums;
                TrackConfig {
                    track_id: t.track_id,
                    player: PlayerConfig::Auto,
                    visible: !is_drums,
                    hand: fingering.hand(t.track_id),
                    muted_opacity: TrackConfig::DEFAULT_MUTED_OPACITY,
                }
            })
            .collect();
        Self {
            tracks: tracks.into(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Song {
    pub file: midi_file::MidiFile,
    pub config: SongConfig,
    /// User fingering choices, persisted next to the MIDI file
    pub fingering: FingeringStore,
}

impl Song {
    /// Song without a file on disk, fingering changes are not persisted
    pub fn new(file: midi_file::MidiFile) -> Self {
        Self::with_fingering(file, FingeringStore::default())
    }

    pub fn from_path(file: midi_file::MidiFile, path: &Path) -> Self {
        Self::with_fingering(file, FingeringStore::load(path))
    }

    fn with_fingering(mut file: midi_file::MidiFile, fingering: FingeringStore) -> Self {
        for (key, to_track) in &fingering.moves {
            if !move_note(&mut file, key, *to_track) {
                log::warn!("Could not move note {key:?} to track {to_track}");
            }
        }

        let config = SongConfig::new(&file.tracks, &fingering);
        Self {
            file,
            config,
            fingering,
        }
    }

    /// Parts that get fingering: visible, non-drum tracks with their hand assignment
    pub fn fingering_parts(&self) -> Vec<(usize, HandAssignment)> {
        self.config
            .tracks
            .iter()
            .filter(|t| t.visible)
            .filter(|t| {
                let track = &self.file.tracks[t.track_id];
                !(track.has_drums && !track.has_other_than_drums)
            })
            .map(|t| (t.track_id, t.hand))
            .collect()
    }

    /// Tracks that can be assigned to a hand: non-drum tracks, regardless of visibility
    fn hand_parts(&self) -> Vec<(usize, HandAssignment)> {
        self.config
            .tracks
            .iter()
            .filter(|t| {
                let track = &self.file.tracks[t.track_id];
                !(track.has_drums && !track.has_other_than_drums)
            })
            .map(|t| (t.track_id, t.hand))
            .collect()
    }

    /// Effective hand of every hand track, `Auto` is resolved where possible
    pub fn resolved_hands(&self) -> Vec<(usize, HandAssignment)> {
        resolve_hands(&self.file.tracks, &self.hand_parts())
    }

    /// Sets the hand of a track and persists it.
    ///
    /// When the song has exactly two hand tracks, the other one gets the opposite hand
    /// (or goes back to `Auto` together with this one).
    pub fn set_hand(&mut self, track_id: usize, hand: HandAssignment) {
        let parts = self.resolved_hands();

        let mut changes = vec![(track_id, hand)];

        let other = match parts.as_slice() {
            [(a, _), (b, _)] if *a == track_id => Some(*b),
            [(a, _), (b, _)] if *b == track_id => Some(*a),
            _ => None,
        };

        if let Some(other) = other {
            let other_hand = match hand {
                HandAssignment::Auto => HandAssignment::Auto,
                HandAssignment::Left => HandAssignment::Right,
                HandAssignment::Right => HandAssignment::Left,
            };
            changes.push((other, other_hand));
        }

        for (track_id, hand) in changes {
            self.config.tracks[track_id].hand = hand;
            self.fingering.set_hand(track_id, hand);
        }
        self.fingering.save();
    }

    /// Moves a single note to another track and persists it
    pub fn move_note(&mut self, note: &MidiNote, to_track: usize) -> bool {
        if !move_note(&mut self.file, &NoteKey::new(note), to_track) {
            return false;
        }
        self.fingering.move_note(note, to_track);
        self.fingering.save();
        true
    }

    /// Non-drum tracks with notes, the ones a note can be moved to
    pub fn note_tracks(&self) -> Vec<usize> {
        self.file
            .tracks
            .iter()
            .filter(|t| !(t.has_drums && !t.has_other_than_drums))
            .filter(|t| !t.notes.is_empty())
            .map(|t| t.track_id)
            .collect()
    }

    pub fn from_env(ctx: &Context) -> Option<Self> {
        let path = std::env::args()
            .nth(1)
            .map(std::path::PathBuf::from)
            .or_else(|| ctx.config.last_opened_song().cloned())?;

        let midi_file = midi_file::MidiFile::new(&path).ok()?;
        Some(Self::from_path(midi_file, &path))
    }
}

fn move_note(file: &mut midi_file::MidiFile, key: &NoteKey, to_track: usize) -> bool {
    let Some(start) = file.tracks.get(key.track).and_then(|t| {
        t.notes
            .iter()
            .find(|n| n.note == key.note && n.start.as_micros() as u64 == key.start_us)
            .map(|n| n.start)
    }) else {
        return false;
    };
    file.move_note(key.track, start, key.note, to_track)
}
