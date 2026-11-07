use crate::{MidiNote, MidiTrack, program_track::ProgramTrack, tempo_track::TempoTrack};
use midly::{Format, Smf, Timing};
use std::{fs, path::Path, sync::Arc};

#[derive(Debug, Clone)]
pub struct MidiFile {
    pub name: String,
    pub format: Format,
    pub tracks: Arc<[MidiTrack]>,
    pub program_track: ProgramTrack,
    pub tempo_track: TempoTrack,
    pub measures: Arc<[std::time::Duration]>,
}

impl MidiFile {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let name = path
            .as_ref()
            .file_name()
            .ok_or(String::from("File not found"))?
            .to_string_lossy()
            .to_string();

        let data = match fs::read(path) {
            Ok(buff) => buff,
            Err(_) => return Err(String::from("Could Not Open File")),
        };

        let smf = match Smf::parse(&data) {
            Ok(smf) => smf,
            Err(_) => return Err(String::from("Midi Parsing Error (midly lib)")),
        };

        Self::from_parsed_smf(name, &smf)
    }

    pub fn from_smf(name: impl Into<String>, smf: &Smf<'_>) -> Result<Self, String> {
        Self::from_parsed_smf(name.into(), smf)
    }

    fn from_parsed_smf(name: String, smf: &Smf<'_>) -> Result<Self, String> {
        let u_per_quarter_note: u16 = match smf.header.timing {
            Timing::Metrical(t) => t.as_int(),
            Timing::Timecode(_fps, _u) => {
                return Err(String::from("Midi With Timecode Timing, Not Supported!"));
            }
        };

        if smf.tracks.is_empty() {
            return Err(String::from("Midi File Has No Tracks"));
        }

        let tempo_track = TempoTrack::build(&smf.tracks, u_per_quarter_note);

        let mut track_color_id = 0;
        let tracks: Vec<MidiTrack> = smf
            .tracks
            .iter()
            .enumerate()
            .map(|(id, events)| {
                let track = MidiTrack::new(id, track_color_id, &tempo_track, events);

                if !track.notes.is_empty() {
                    track_color_id += 1;
                }

                track
            })
            .collect();

        let measures = {
            let last_note_end = tracks
                .iter()
                .fold(std::time::Duration::ZERO, |last, track| {
                    if let Some(note) = track.notes.last() {
                        last.max(note.start + note.duration)
                    } else {
                        last
                    }
                });

            let mut masures = Vec::new();
            let mut time = std::time::Duration::ZERO;
            let mut id = 0;
            while time <= last_note_end {
                time = tempo_track.pulses_to_duration(id * u_per_quarter_note as u64 * 4);
                masures.push(time);
                id += 1;
            }

            masures
        };

        let program_track = ProgramTrack::new(&tracks);

        Ok(Self {
            name,
            format: smf.header.format,
            tracks: tracks.into(),
            program_track,
            tempo_track,
            measures: measures.into(),
        })
    }
}

impl MidiFile {
    /// Moves a single note (and its NoteOn/NoteOff events) from one track to another.
    ///
    /// The note is identified by its track, start time and key. Returns `false` if no such
    /// note exists or the tracks are invalid. Note ids of both tracks are recomputed.
    pub fn move_note(
        &mut self,
        from_track: usize,
        start: std::time::Duration,
        key: u8,
        to_track: usize,
    ) -> bool {
        if from_track == to_track
            || from_track >= self.tracks.len()
            || to_track >= self.tracks.len()
        {
            return false;
        }

        let mut tracks: Vec<MidiTrack> = self.tracks.to_vec();

        let Some(note_idx) = tracks[from_track]
            .notes
            .iter()
            .position(|n| n.note == key && n.start == start)
        else {
            return false;
        };

        let mut note = tracks[from_track].notes[note_idx].clone();
        let to_color = tracks[to_track].track_color_id;

        // Events of the note
        let is_key = |e: &crate::MidiEvent, on: bool| match e.message {
            midly::MidiMessage::NoteOn { key: k, .. } => on && k.as_int() == key,
            midly::MidiMessage::NoteOff { key: k, .. } => !on && k.as_int() == key,
            _ => false,
        };

        let mut from_events: Vec<crate::MidiEvent> = tracks[from_track].events.to_vec();
        let Some(on_idx) = from_events
            .iter()
            .position(|e| e.timestamp == note.start && is_key(e, true))
        else {
            return false;
        };
        let mut on_event = from_events.remove(on_idx);
        let off_event = from_events
            .iter()
            .enumerate()
            .skip(on_idx)
            .find(|(_, e)| e.timestamp == note.end && is_key(e, false))
            .map(|(i, _)| i)
            .map(|i| from_events.remove(i));

        let mut to_events: Vec<crate::MidiEvent> = tracks[to_track].events.to_vec();
        on_event.track_id = to_track;
        on_event.track_color_id = to_color;
        // NoteOn after anything at the same time (eg. a NoteOff of the same key)
        let at = to_events.partition_point(|e| e.timestamp <= on_event.timestamp);
        to_events.insert(at, on_event);
        if let Some(mut off_event) = off_event {
            off_event.track_id = to_track;
            off_event.track_color_id = to_color;
            // NoteOff before anything at the same time (eg. a NoteOn of the same key)
            let at = to_events.partition_point(|e| e.timestamp < off_event.timestamp);
            to_events.insert(at, off_event);
        }

        // Notes, kept in the order they were built in (by end time)
        let mut from_notes: Vec<MidiNote> = tracks[from_track].notes.to_vec();
        from_notes.remove(note_idx);

        let mut to_notes: Vec<MidiNote> = tracks[to_track].notes.to_vec();
        note.track_id = to_track;
        note.track_color_id = to_color;
        let at = to_notes.partition_point(|n| n.end <= note.end);
        to_notes.insert(at, note);

        for (id, n) in from_notes.iter_mut().enumerate() {
            n.id = id;
        }
        for (id, n) in to_notes.iter_mut().enumerate() {
            n.id = id;
        }

        tracks[from_track].notes = from_notes.into();
        tracks[from_track].events = from_events.into();
        tracks[to_track].notes = to_notes.into();
        tracks[to_track].events = to_events.into();

        self.tracks = tracks.into();
        true
    }
}
