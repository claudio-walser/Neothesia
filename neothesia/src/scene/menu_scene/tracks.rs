use midi_file::MidiTrack;
use neothesia_core::fingering::HandAssignment;
use nuon::TextAlign;
use std::{hash::Hash, time::Instant};

use crate::{
    context::Context,
    locals,
    song::{PlayerConfig, TrackConfig},
};

use super::{icons, neo_btn_icon, state};

pub const CARD_W: f32 = 344.0;
pub const CARD_H: f32 = 126.0;

impl super::MenuScene {
    pub fn tracks_page_ui(&mut self, ctx: &mut Context, ui: &mut nuon::Ui) {
        let win_w = ctx.window_state.logical_size.width;
        let win_h = ctx.window_state.logical_size.height;
        let bottom_bar_h = 60.0;

        nuon::translate().x(0.0).y(win_h).build(ui, |ui| {
            // Bottom Margin
            nuon::translate().y(-10.0).add_to_current(ui);

            nuon::translate().y(-bottom_bar_h).add_to_current(ui);

            let gap = 10.0;
            let w = 80.0;
            let h = bottom_bar_h;

            nuon::translate().x(0.0).build(ui, |ui| {
                nuon::translate().x(gap).add_to_current(ui);

                if neo_btn_icon(ui, w, h, icons::left_arrow_icon()) {
                    self.state.go_back();
                }

                nuon::translate().x(-w - gap).add_to_current(ui);
            });

            nuon::translate().x(win_w).build(ui, |ui| {
                nuon::translate().x(-w - gap).add_to_current(ui);

                if neo_btn_icon(ui, w, h, icons::play_icon()) {
                    state::play(&self.state, ctx);
                }
            });
        });

        if let Some(song) = self.state.song.as_mut() {
            nuon::scroll("tracks_scroll")
                .scissor_size(win_w, (win_h - bottom_bar_h).max(0.0))
                .build(ui, |ui| {
                    let gap = 14.0;
                    let top_margin = 60.0;

                    let mut tracks: Vec<&MidiTrack> = song
                        .file
                        .tracks
                        .iter()
                        .filter(|t| !t.notes.is_empty())
                        .collect();

                    // Lay the cards out like the hands on the keyboard: left hand first,
                    // right hand last, everything else keeps its file order in between
                    let resolved = song.resolved_hands();
                    let resolved_hand = |track_id: usize| {
                        resolved
                            .iter()
                            .find(|(id, _)| *id == track_id)
                            .map(|(_, hand)| *hand)
                    };
                    let hand_rank = |track_id: usize| match resolved_hand(track_id) {
                        Some(HandAssignment::Left) => 0,
                        Some(HandAssignment::Right) => 2,
                        _ => 1,
                    };
                    tracks.sort_by_key(|t| hand_rank(t.track_id));

                    let layout = CardsLayout::new(win_w, tracks.len());
                    let columns = layout.columns as usize;
                    let origin_x = nuon::center_x(win_w, layout.width);

                    let mut event = None;

                    for (slot, track) in tracks.iter().enumerate() {
                        let col = slot % columns;
                        let row = slot / columns;
                        let target = (
                            origin_x + col as f32 * (CARD_W + gap),
                            top_margin + row as f32 * (CARD_H + gap),
                        );

                        let id = nuon::Id::hash_with(|h| {
                            "track_card".hash(h);
                            track.track_id.hash(h);
                        });

                        let (x, y) = ui.data::<CardAnimation>(id).step(target);

                        let config = &song.config.tracks[track.track_id];

                        nuon::translate().pos(x, y).build(ui, |ui| {
                            let hand = resolved_hand(track.track_id).unwrap_or(config.hand);
                            let paired = resolved.len() == 2 && hand != HandAssignment::Auto;
                            match self::track_card(ctx, ui, id, track, config, hand, paired) {
                                TrackCardEvent::Idle => {}
                                e => event = Some((track.track_id, e)),
                            }
                        });
                    }

                    let rows = tracks.len().div_ceil(columns);

                    match event {
                        Some((track_id, TrackCardEvent::PlayerConfig(player))) => {
                            song.config.tracks[track_id].player = player;
                        }
                        Some((track_id, TrackCardEvent::SetMutedOpacity(opacity))) => {
                            let track = &mut song.config.tracks[track_id];
                            track.player = PlayerConfig::Mute;
                            track.muted_opacity = opacity;
                        }
                        Some((track_id, TrackCardEvent::SetVisible(visible))) => {
                            song.config.tracks[track_id].visible = visible;
                        }
                        Some((track_id, TrackCardEvent::SetHand(hand))) => {
                            song.set_hand(track_id, hand);
                        }
                        Some((_, TrackCardEvent::Idle)) | None => {}
                    }

                    // Content height for the scroll area
                    nuon::translate()
                        .y(top_margin + rows as f32 * (CARD_H + gap))
                        .add_to_current(ui);
                });
        }
    }
}

/// Smoothly moves a track card towards its slot when the order changes
#[derive(Default)]
struct CardAnimation {
    pos: Option<(f32, f32)>,
    last_frame: Option<Instant>,
}

impl CardAnimation {
    /// Higher is snappier
    const SPEED: f32 = 12.0;

    fn step(&mut self, target: (f32, f32)) -> (f32, f32) {
        let now = Instant::now();
        let dt = self
            .last_frame
            .map(|last| now.duration_since(last).as_secs_f32())
            .unwrap_or(0.0)
            // Don't jump after a long pause (e.g. coming back to this page)
            .min(0.1);
        self.last_frame = Some(now);

        let pos = match self.pos {
            // First time we see this card, no animation
            None => target,
            Some((x, y)) => {
                let t = 1.0 - (-Self::SPEED * dt).exp();
                let x = x + (target.0 - x) * t;
                let y = y + (target.1 - y) * t;
                let snap = |v: f32, target: f32| if (v - target).abs() < 0.5 { target } else { v };
                (snap(x, target.0), snap(y, target.1))
            }
        };

        self.pos = Some(pos);
        pos
    }
}

struct CardsLayout {
    columns: u8,
    width: f32,
}

impl CardsLayout {
    fn new(w: f32, tracks_count: usize) -> Self {
        const GAP: f32 = 14.0;

        const LAYOUT_1: f32 = CARD_W;
        const LAYOUT_2: f32 = LAYOUT_1 + GAP + CARD_W;
        const LAYOUT_3: f32 = LAYOUT_2 + GAP + CARD_W;

        let columns = if w > LAYOUT_3 {
            3
        } else if w > LAYOUT_2 {
            2
        } else {
            1
        };

        let columns = columns.min(tracks_count).max(1) as u8;

        Self {
            columns,
            width: match columns {
                3 => LAYOUT_3,
                2 => LAYOUT_2,
                _ => LAYOUT_1,
            },
        }
    }
}

#[derive(Debug)]
enum TrackCardEvent {
    PlayerConfig(PlayerConfig),
    /// Mutes the track and sets how visible its notes stay in the waterfall
    SetMutedOpacity(f32),
    SetVisible(bool),
    SetHand(HandAssignment),
    Idle,
}

fn track_card(
    ctx: &Context,
    ui: &mut nuon::Ui,
    id: impl Into<nuon::Id>,
    track: &MidiTrack,
    config: &TrackConfig,
    // Effective hand, `Auto` only when it can't be resolved (e.g. a single two-hand track)
    hand: HandAssignment,
    // One of exactly two hand tracks, the other one always plays the opposite hand
    paired: bool,
) -> TrackCardEvent {
    let card_w = CARD_W;
    let card_h = CARD_H;

    let pad = 16.0;

    let id = id.into();

    let track_color = if !config.visible {
        nuon::Color::new_u8(102, 102, 102, 1.0)
    } else {
        let color_id = track.track_color_id % ctx.config.color_schema().len();
        let color = &ctx.config.color_schema()[color_id].base;
        nuon::Color::new_u8(color.0, color.1, color.2, 1.0)
    };

    let title = if track.has_drums && !track.has_other_than_drums {
        "Percussion"
    } else {
        let instrument_id = track
            .programs
            .last()
            .map(|p| p.program as usize)
            .unwrap_or(0);
        midi_file::INSTRUMENT_NAMES[instrument_id]
    };

    let subtitle = format!("{} Notes", track.notes.len());

    nuon::quad()
        .size(card_w, card_h)
        .color([37, 35, 42])
        .border_radius([12.0; 4])
        .build(ui);

    let inner_card_w = card_w - pad * 2.0;
    let _inner_card_h = card_h - pad * 2.0;

    let icon_size = 40.0;

    let mut res = TrackCardEvent::Idle;

    nuon::translate().pos(pad, pad).build(ui, |ui| {
        let accent = track_color;
        let accent_hover = nuon::Color::new(
            (accent.r + 0.05).min(1.0),
            (accent.g + 0.05).min(1.0),
            (accent.b + 0.05).min(1.0),
            1.0,
        );

        let regular = nuon::Color::from([74, 68, 88]);
        let regular_hover = nuon::Color::from([87, 81, 101]);

        if nuon::button()
            .id(nuon::Id::hash_with(|h| {
                id.as_raw().hash(h);
                "visible".hash(h);
            }))
            .size(icon_size, icon_size)
            .color(accent)
            .hover_color(accent_hover)
            .preseed_color(accent)
            .border_radius([255.0; 4])
            .build(ui)
        {
            res = TrackCardEvent::SetVisible(!config.visible);
        }

        let btn_w = inner_card_w / 3.0;

        let is_drums = track.has_drums && !track.has_other_than_drums;
        let show_hand = ctx.config.fingering() && !is_drums;
        let hand_btn_w = 70.0;

        if show_hand {
            // Paired tracks just toggle between left and right, the button names the side
            // the card moves to. Other tracks keep `Auto` in the cycle (e.g. a single track
            // for both hands needs the auto split) and show their current hand.
            let (label, next) = match hand {
                HandAssignment::Left if paired => {
                    (locals::menu::hand_right(), HandAssignment::Right)
                }
                HandAssignment::Right if paired => {
                    (locals::menu::hand_left(), HandAssignment::Left)
                }
                HandAssignment::Left => (locals::menu::hand_left(), HandAssignment::Right),
                HandAssignment::Right => (locals::menu::hand_right(), HandAssignment::Auto),
                HandAssignment::Auto => (locals::menu::hand_auto(), HandAssignment::Left),
            };

            if nuon::button()
                .id(nuon::Id::hash_with(|h| {
                    id.as_raw().hash(h);
                    "hand".hash(h);
                }))
                .x(inner_card_w - hand_btn_w)
                .y((icon_size - 30.0) / 2.0)
                .size(hand_btn_w, 30.0)
                .color(regular)
                .hover_color(regular_hover)
                .preseed_color(regular)
                .border_radius([255.0; 4])
                .label(label)
                .build(ui)
            {
                res = TrackCardEvent::SetHand(next);
            }
        }

        let labels_x = icon_size + 15.0;
        nuon::translate().x(labels_x).build(ui, |ui| {
            let label_h = icon_size / 2.0;
            let label_w = if show_hand {
                inner_card_w - labels_x - hand_btn_w - 5.0
            } else {
                inner_card_w - labels_x
            };

            nuon::label()
                .size(label_w, label_h)
                .text(title)
                .text_justify(TextAlign::Start)
                .font_size(16.0)
                .build(ui);

            nuon::label()
                .y(label_h)
                .size(label_w, label_h)
                .text(subtitle)
                .text_justify(TextAlign::Start)
                .font_size(14.0)
                .build(ui);
        });

        nuon::translate().y(icon_size + 15.0).build(ui, |ui| {
            let color = |m: PlayerConfig| {
                if m == config.player { accent } else { regular }
            };
            let hover_color = |m: PlayerConfig| {
                if m == config.player {
                    accent_hover
                } else {
                    regular_hover
                }
            };

            let muted = config.player == PlayerConfig::Mute;
            match mute_button(
                ui,
                nuon::Id::hash_with(|h| {
                    id.as_raw().hash(h);
                    "mute".hash(h);
                }),
                btn_w,
                40.0,
                muted,
                config.muted_opacity,
                color(PlayerConfig::Mute),
                hover_color(PlayerConfig::Mute),
            ) {
                MuteButtonEvent::Idle => {}
                MuteButtonEvent::Clicked => {
                    res = TrackCardEvent::PlayerConfig(PlayerConfig::Mute);
                }
                MuteButtonEvent::Opacity(opacity) => {
                    res = TrackCardEvent::SetMutedOpacity(opacity);
                }
            }

            if nuon::button()
                .id(nuon::Id::hash_with(|h| {
                    id.as_raw().hash(h);
                    "auto".hash(h);
                }))
                .x(btn_w)
                .size(btn_w, 40.0)
                .color(color(PlayerConfig::Auto))
                .hover_color(hover_color(PlayerConfig::Auto))
                .preseed_color(color(PlayerConfig::Auto))
                .border_radius([0.0; 4])
                .label("Auto Play")
                .build(ui)
            {
                res = TrackCardEvent::PlayerConfig(PlayerConfig::Auto);
            }

            if nuon::button()
                .id(nuon::Id::hash_with(|h| {
                    id.as_raw().hash(h);
                    "human".hash(h);
                }))
                .x(btn_w * 2.0)
                .size(btn_w, 40.0)
                .color(color(PlayerConfig::Human))
                .hover_color(hover_color(PlayerConfig::Human))
                .preseed_color(color(PlayerConfig::Human))
                .border_radius([0.0, 255.0, 255.0, 0.0])
                .label("Play Along")
                .build(ui)
            {
                res = TrackCardEvent::PlayerConfig(PlayerConfig::Human);
            }
        });
    });

    res
}

enum MuteButtonEvent {
    Idle,
    Clicked,
    /// The opacity slider was dragged
    Opacity(f32),
}

#[derive(Default)]
struct MuteDrag {
    /// Pointer x where the press started
    start_x: Option<f32>,
    dragging: bool,
}

/// Mute button that doubles as a slider for the opacity of the muted track's notes.
///
/// A click mutes the track, dragging horizontally mutes it and sets the opacity
/// (left edge transparent, right edge fully visible).
#[allow(clippy::too_many_arguments)]
fn mute_button(
    ui: &mut nuon::Ui,
    id: nuon::Id,
    w: f32,
    h: f32,
    muted: bool,
    opacity: f32,
    color: nuon::Color,
    hover_color: nuon::Color,
) -> MuteButtonEvent {
    /// Pointer travel before a press turns into a drag
    const DRAG_THRESHOLD: f32 = 4.0;
    const BAR_PAD: f32 = 16.0;

    let event = nuon::click_area(id).size(w, h).build(ui);
    let x = event.local_pos().x;
    let bar_w = w - BAR_PAD * 2.0;
    let opacity_at = |x: f32| ((x - BAR_PAD) / bar_w).clamp(0.0, 1.0);

    let drag = ui.data::<MuteDrag>(id);
    if event.is_press_start() {
        drag.start_x = Some(x);
        drag.dragging = false;
    }
    if let Some(start_x) = drag.start_x
        && (event.is_pressed() || event.is_press_end())
        && (x - start_x).abs() > DRAG_THRESHOLD
    {
        drag.dragging = true;
    }
    let dragging = drag.dragging;
    if event.is_press_end() {
        *drag = MuteDrag::default();
    }

    let res = if dragging {
        MuteButtonEvent::Opacity(opacity_at(x))
    } else if event.is_clicked() {
        MuteButtonEvent::Clicked
    } else {
        MuteButtonEvent::Idle
    };

    // While dragging show the value under the pointer, the config catches up next frame
    let (muted, opacity) = match res {
        MuteButtonEvent::Opacity(opacity) => (true, opacity),
        _ => (muted, opacity),
    };

    let bg = if event.is_hovered() { hover_color } else { color };
    nuon::quad()
        .size(w, h)
        .color(bg)
        .border_radius([255.0, 0.0, 0.0, 255.0])
        .build(ui);

    let text = if muted {
        format!("Mute {}%", (opacity * 100.0).round() as u32)
    } else {
        "Mute".to_string()
    };
    nuon::label()
        .size(w, if muted { h - 6.0 } else { h })
        .text(text)
        .build(ui);

    if muted {
        let bar_h = 3.0;
        let bar_y = h - 10.0;
        nuon::quad()
            .pos(BAR_PAD, bar_y)
            .size(bar_w, bar_h)
            .color(nuon::Color::new(0.0, 0.0, 0.0, 0.3))
            .border_radius([bar_h / 2.0; 4])
            .build(ui);
        nuon::quad()
            .pos(BAR_PAD, bar_y)
            .size((bar_w * opacity).max(bar_h), bar_h)
            .color(nuon::Color::new(1.0, 1.0, 1.0, 0.9))
            .border_radius([bar_h / 2.0; 4])
            .build(ui);
    }

    res
}
