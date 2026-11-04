use std::rc::Rc;

use crate::{TransformUniform, Uniform, config::Config, render::TrackOpacity};
use midi_file::{MidiNote, MidiTrack};
use wgpu_jumpstart::{Color, Gpu};

mod live;
mod pipeline;
pub use live::LiveWaterfallRenderer;
use pipeline::{NoteInstance, WaterfallPipeline};

#[derive(Clone)]
pub struct NoteList {
    pub(crate) inner: Rc<[MidiNote]>,
}

impl NoteList {
    fn new(tracks: &[MidiTrack], hidden_tracks: &[usize]) -> Self {
        let mut notes: Vec<_> = tracks
            .iter()
            .filter(|track| !hidden_tracks.contains(&track.track_id))
            .flat_map(|track| track.notes.iter().cloned())
            .collect();
        // We want to render newer notes on top of old notes
        notes.sort_unstable_by_key(|note| note.start);

        Self {
            inner: notes.into(),
        }
    }

    fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn get(&self, id: usize) -> Option<&MidiNote> {
        self.inner.get(id)
    }

    /// Notes sorted by start time
    pub fn iter(&self) -> std::slice::Iter<'_, MidiNote> {
        self.inner.iter()
    }
}

pub struct WaterfallRenderer {
    notes_pipeline: WaterfallPipeline,
    notes: NoteList,
    /// Per [`NoteList`] index: instance index and its schema color
    instances: Vec<Option<(usize, [f32; 3])>>,
    /// Per [`NoteList`] index: color replacing the schema color
    color_overrides: Vec<Option<Color>>,
    track_opacity: TrackOpacity,
    instances_dirty: bool,
    device: wgpu::Device,
    queue: wgpu::Queue,
}

impl WaterfallRenderer {
    pub fn new(
        gpu: &Gpu,
        tracks: &[MidiTrack],
        hidden_tracks: &[usize],
        config: &Config,
        transform_uniform: &Uniform<TransformUniform>,
        layout: piano_layout::KeyboardLayout,
    ) -> Self {
        let notes = NoteList::new(tracks, hidden_tracks);

        let notes_pipeline = WaterfallPipeline::new(gpu, transform_uniform, notes.len());
        let mut notes = Self {
            notes_pipeline,
            instances: Vec::new(),
            color_overrides: vec![None; notes.len()],
            track_opacity: TrackOpacity::default(),
            instances_dirty: false,
            notes,
            device: gpu.device.clone(),
            queue: gpu.queue.clone(),
        };
        notes
            .notes_pipeline
            .set_speed(&gpu.queue, config.animation_speed());
        notes.resize(config, layout);
        notes
    }

    pub fn pipeline(&mut self) -> &mut WaterfallPipeline {
        &mut self.notes_pipeline
    }

    pub fn notes(&self) -> &NoteList {
        &self.notes
    }

    pub fn set_notes(
        &mut self,
        notes: impl Into<Rc<[MidiNote]>>,
        config: &Config,
        layout: piano_layout::KeyboardLayout,
    ) {
        self.notes = NoteList {
            inner: notes.into(),
        };
        self.color_overrides = vec![None; self.notes.len()];
        self.resize(config, layout);
    }

    pub fn resize(&mut self, config: &Config, layout: piano_layout::KeyboardLayout) {
        let range_start = layout.range.start() as usize;

        self.notes_pipeline.clear();
        self.instances.clear();

        let mut longer_than_range = false;
        for (note_id, note) in self.notes.inner.iter().enumerate() {
            if layout.range.contains(note.note) && note.channel != 9 {
                let key = &layout.keys[note.note as usize - range_start];

                let color_schema = config.color_schema();

                let color = &color_schema[note.track_color_id % color_schema.len()];
                let color = if key.kind().is_sharp() {
                    color.dark
                } else {
                    color.base
                };
                let color: Color = color.into();
                let color = color.into_linear_rgb();
                self.instances
                    .push(Some((self.notes_pipeline.instances().len(), color)));
                let color = self.color_overrides[note_id]
                    .map(|c| c.into_linear_rgb())
                    .unwrap_or(color);

                let h = if note.duration.as_secs_f32() >= 0.1 {
                    note.duration.as_secs_f32()
                } else {
                    0.1
                };

                let alpha = self.track_opacity.get(note.track_id);
                self.notes_pipeline.instances().push(NoteInstance {
                    position: [key.x(), note.start.as_secs_f32()],
                    size: [key.width() - 1.0, h - 0.01], // h - 0.01 to make a little gap bettwen successive notes
                    color,
                    radius: key.width() * 0.2,
                    alpha,
                });
            } else {
                self.instances.push(None);
                longer_than_range = true;
            }
        }

        if longer_than_range {
            log::warn!(
                "Midi wider than giver range: {range_start}-{}",
                layout.range.end()
            );
        }

        self.notes_pipeline.prepare(&self.device, &self.queue);
    }

    pub fn update(&mut self, time: f32) {
        self.notes_pipeline.update_time(&self.queue, time);

        if self.instances_dirty {
            self.instances_dirty = false;
            self.notes_pipeline.prepare(&self.device, &self.queue);
        }
    }

    /// Sets the opacity of all notes of a track, e.g. to fade out muted tracks
    pub fn set_track_opacity(&mut self, track_id: usize, opacity: f32) {
        self.track_opacity.set(track_id, opacity);
        let opacity = self.track_opacity.get(track_id);

        for (note, instance) in self.notes.inner.iter().zip(&self.instances) {
            if let Some((instance_id, _)) = instance
                && note.track_id == track_id
            {
                self.notes_pipeline.instances()[*instance_id].alpha = opacity;
                self.instances_dirty = true;
            }
        }
    }

    /// Recolors a single note, `None` restores the color schema color
    pub fn set_note_color(&mut self, note_id: usize, color: Option<Color>) {
        let Some(slot) = self.color_overrides.get_mut(note_id) else {
            return;
        };
        *slot = color;

        if let Some(Some((instance_id, schema_color))) = self.instances.get(note_id) {
            self.notes_pipeline.instances()[*instance_id].color =
                color.map(|c| c.into_linear_rgb()).unwrap_or(*schema_color);
            self.instances_dirty = true;
        }
    }

    #[profiling::function]
    pub fn render<'rpass>(&'rpass mut self, render_pass: &mut wgpu::RenderPass<'rpass>) {
        self.notes_pipeline.render(render_pass);
    }
}
