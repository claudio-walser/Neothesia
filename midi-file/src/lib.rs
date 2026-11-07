mod file;
pub mod playback;
pub mod program_track;
pub mod tempo_track;
mod track;

pub use file::*;
pub use midly;
pub use playback::*;
pub use track::*;

pub static INSTRUMENT_NAMES: [&str; 128] = [
    "Acoustic Grand Piano",
    "Bright Acoustic Piano",
    "Electric Grand Piano",
    "Honky-tonk Piano",
    "Electric Piano 1",
    "Electric Piano 2",
    "Harpsichord",
    "Clavi",
    "Celesta",
    "Glockenspiel",
    "Music Box",
    "Vibraphone",
    "Marimba",
    "Xylophone",
    "Tubular Bells",
    "Dulcimer",
    "Drawbar Organ",
    "Percussive Organ",
    "Rock Organ",
    "Church Organ",
    "Reed Organ",
    "Accordion",
    "Harmonica",
    "Tango Accordion",
    "Acoustic Guitar (nylon)",
    "Acoustic Guitar (steel)",
    "Electric Guitar (jazz)",
    "Electric Guitar (clean)",
    "Electric Guitar (muted)",
    "Overdriven Guitar",
    "Distortion Guitar",
    "Guitar harmonics",
    "Acoustic Bass",
    "Electric Bass (finger)",
    "Electric Bass (pick)",
    "Fretless Bass",
    "Slap Bass 1",
    "Slap Bass 2",
    "Synth Bass 1",
    "Synth Bass 2",
    "Violin",
    "Viola",
    "Cello",
    "Contrabass",
    "Tremolo Strings",
    "Pizzicato Strings",
    "Orchestral Harp",
    "Timpani",
    "String Ensemble 1",
    "String Ensemble 2",
    "SynthStrings 1",
    "SynthStrings 2",
    "Choir Aahs",
    "Voice Oohs",
    "Synth Voice",
    "Orchestra Hit",
    "Trumpet",
    "Trombone",
    "Tuba",
    "Muted Trumpet",
    "French Horn",
    "Brass Section",
    "SynthBrass 1",
    "SynthBrass 2",
    "Soprano Sax",
    "Alto Sax",
    "Tenor Sax",
    "Baritone Sax",
    "Oboe",
    "English Horn",
    "Bassoon",
    "Clarinet",
    "Piccolo",
    "Flute",
    "Recorder",
    "Pan Flute",
    "Blown Bottle",
    "Shakuhachi",
    "Whistle",
    "Ocarina",
    "Lead 1 (square)",
    "Lead 2 (sawtooth)",
    "Lead 3 (calliope)",
    "Lead 4 (chiff)",
    "Lead 5 (charang)",
    "Lead 6 (voice)",
    "Lead 7 (fifths)",
    "Lead 8 (bass + lead)",
    "Pad 1 (new age)",
    "Pad 2 (warm)",
    "Pad 3 (polysynth)",
    "Pad 4 (choir)",
    "Pad 5 (bowed)",
    "Pad 6 (metallic)",
    "Pad 7 (halo)",
    "Pad 8 (sweep)",
    "FX 1 (rain)",
    "FX 2 (soundtrack)",
    "FX 3 (crystal)",
    "FX 4 (atmosphere)",
    "FX 5 (brightness)",
    "FX 6 (goblins)",
    "FX 7 (echoes)",
    "FX 8 (sci-fi)",
    "Sitar",
    "Banjo",
    "Shamisen",
    "Koto",
    "Kalimba",
    "Bag pipe",
    "Fiddle",
    "Shanai",
    "Tinkle Bell",
    "Agogo",
    "Steel Drums",
    "Woodblock",
    "Taiko Drum",
    "Melodic Tom",
    "Synth Drum",
    "Reverse Cymbal",
    "Guitar Fret Noise",
    "Breath Noise",
    "Seashore",
    "Bird Tweet",
    "Telephone Ring",
    "Helicopter",
    "Applause",
    "Gunshot",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load() {
        let _midi = MidiFile::new("../test.mid").unwrap();
    }

    #[test]
    fn move_note() {
        let mut midi = MidiFile::new("../test.mid").unwrap();
        let with_notes: Vec<usize> = midi
            .tracks
            .iter()
            .filter(|t| !t.notes.is_empty())
            .map(|t| t.track_id)
            .collect();
        let [from, to, ..] = with_notes[..] else {
            panic!("test.mid needs two tracks with notes");
        };

        let note = midi.tracks[from].notes[3].clone();
        let (from_notes, to_notes) = (midi.tracks[from].notes.len(), midi.tracks[to].notes.len());
        let (from_events, to_events) =
            (midi.tracks[from].events.len(), midi.tracks[to].events.len());

        assert!(midi.move_note(from, note.start, note.note, to));

        assert_eq!(midi.tracks[from].notes.len(), from_notes - 1);
        assert_eq!(midi.tracks[to].notes.len(), to_notes + 1);
        assert_eq!(midi.tracks[from].events.len(), from_events - 2);
        assert_eq!(midi.tracks[to].events.len(), to_events + 2);

        let moved = midi.tracks[to]
            .notes
            .iter()
            .find(|n| n.start == note.start && n.note == note.note)
            .unwrap();
        assert_eq!(moved.track_id, to);
        assert_eq!(moved.end, note.end);

        for track in [from, to] {
            let track = &midi.tracks[track];
            assert!(track.notes.iter().enumerate().all(|(i, n)| n.id == i));
            assert!(
                track
                    .events
                    .windows(2)
                    .all(|w| w[0].timestamp <= w[1].timestamp)
            );
            assert!(track.events.iter().all(|e| e.track_id == track.track_id));
        }

        // And back again
        assert!(midi.move_note(to, note.start, note.note, from));
        assert_eq!(midi.tracks[from].notes.len(), from_notes);
        assert_eq!(midi.tracks[to].events.len(), to_events);
    }
}
