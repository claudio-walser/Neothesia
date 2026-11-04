/// Per track opacity of everything drawn for its notes (notes, note names, fingering),
/// e.g. to fade out muted tracks. Tracks that were never set are fully opaque.
#[derive(Debug, Clone, Default)]
pub struct TrackOpacity(Vec<f32>);

impl TrackOpacity {
    pub fn get(&self, track_id: usize) -> f32 {
        self.0.get(track_id).copied().unwrap_or(1.0)
    }

    pub fn set(&mut self, track_id: usize, opacity: f32) {
        if self.0.len() <= track_id {
            self.0.resize(track_id + 1, 1.0);
        }
        self.0[track_id] = opacity.clamp(0.0, 1.0);
    }

    /// `color` with its alpha scaled by the opacity of `track_id`
    pub fn apply(&self, track_id: usize, color: glyphon::Color) -> glyphon::Color {
        let alpha = color.a() as f32 * self.get(track_id);
        glyphon::Color::rgba(color.r(), color.g(), color.b(), alpha.round() as u8)
    }
}
