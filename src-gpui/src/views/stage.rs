use std::ops::Range;
use std::path::Path;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt as _, AnyElement, BoxShadow, Context, MouseButton, MouseDownEvent,
    Pixels, Role, Window, div, prelude::*, pulsating_between, px, uniform_list,
};
use optionmusic::controller::TrackDto;

use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;

impl RootView {
    pub(crate) fn queue_row(
        &self,
        track: &TrackDto,
        index: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = track.id.clone();
        let menu_id = track.id.clone();
        let focused =
            self.focused_list == Some(FocusedList::Queue) && self.focused_row == Some(index);
        div()
            .id(format!("queue-row-{index}"))
            .accessibility_id(format!("optionmusic.queue.{index}"))
            .role(Role::ListItem)
            .aria_label(track.name.clone())
            .aria_position_in_set(index + 1)
            .aria_size_of_set(self.playback.queue.len())
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(40.0))
            .px(px(4.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .gap(px(6.0))
            .text_color(tokens.ink_2)
            .hover(|style| style.bg(tokens.hover))
            .when(focused, |this| {
                this.bg(tokens.selected)
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.focus_list(FocusedList::Queue, index, window, cx);
                    this.play_track(&id, cx);
                },
            ))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(
                    move |this: &mut RootView,
                          event: &MouseDownEvent,
                          window: &mut Window,
                          cx: &mut Context<RootView>| {
                        this.focus_list(FocusedList::Queue, index, window, cx);
                        this.open_context_menu(menu_id.clone(), event.position, true, window, cx);
                        cx.stop_propagation();
                    },
                ),
            )
            .child(
                div()
                    .w(px(24.0))
                    .text_center()
                    .text_color(tokens.faint)
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(format!("{:02}", index + 1)),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(tokens.ink)
                            .child(track.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(tokens.mute)
                            .child(track_meta(track)),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_none()
                    .child(self.icon_button(
                        format!("queue-up-{index}"),
                        "Move up",
                        icons::styled(icons::arrow_up(px(12.0)), tokens.mute, tokens.ink),
                        move |this: &mut RootView, _w: &mut Window, cx: &mut Context<RootView>| {
                            this.queue_move_at(index, -1, cx);
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.icon_button(
                        format!("queue-down-{index}"),
                        "Move down",
                        icons::styled(icons::arrow_down(px(12.0)), tokens.mute, tokens.ink),
                        move |this: &mut RootView, _w: &mut Window, cx: &mut Context<RootView>| {
                            this.queue_move_at(index, 1, cx);
                        },
                        tokens,
                        cx,
                    )),
            )
    }

    /// Panel header: status dot + label on the left, and a mouse-accessible
    /// collapse button on the right (same as the `t` shortcut).
    pub(crate) fn stage_header(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let has_track = self.playback.current.is_some();
        let playing = has_track && !self.playback.paused && !self.playback.stopped;
        let label = if has_track && !playing {
            "PAUSED"
        } else {
            "NOW PLAYING"
        };

        let dot = div().size(px(7.0)).rounded(px(100.0)).bg(if playing {
            tokens.white
        } else {
            tokens.faint
        });
        let dot: AnyElement = if playing {
            dot.with_animation(
                "playing-pulse",
                Animation::new(Duration::from_millis(1200))
                    .repeat()
                    .with_easing(pulsating_between(0.35, 1.0)),
                |dot, alpha| dot.opacity(alpha),
            )
            .into_any_element()
        } else {
            dot.into_any_element()
        };

        div()
            .id("stage-header")
            .accessibility_id("optionmusic.stage.header")
            .h(px(38.0))
            .px(px(14.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .border_b_1()
            .border_color(tokens.border)
            .child(dot)
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.faint)
                    .child(label),
            )
            .child(div().flex_1())
            .child(self.icon_button(
                "stage-close",
                "Hide now playing",
                icons::styled(icons::close(px(15.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, window: &mut Window, cx: &mut Context<RootView>| {
                    this.do_toggle_stage(cx);
                    this.save_desktop_prefs(window, cx);
                },
                tokens,
                cx,
            ))
    }

    /// Large square cover with a hover play/pause overlay. `size` is computed
    /// from the free vertical space so the control cluster always fits.
    pub(crate) fn stage_hero(
        &self,
        size: Pixels,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let track = self.playback.current.clone();
        let (glyph, cover) = match &track {
            Some(track) => (cover_glyph(&track.name), self.current_cover.clone()),
            None => ("o".into(), None),
        };
        let paused = self.playback.stopped || self.playback.paused;

        div()
            .id("stage-hero")
            .accessibility_id("optionmusic.stage.hero")
            .w_full()
            .flex()
            .justify_center()
            .p(px(14.0))
            .child(
                self.cover_element("cover-hero", cover, glyph, size, tokens)
                    .child(
                        div()
                            .id("stage-cover-overlay")
                            .accessibility_id("optionmusic.stage.cover")
                            .role(Role::Button)
                            .aria_label(if paused { "Play" } else { "Pause" })
                            .absolute()
                            .inset_0()
                            .rounded(px(8.0))
                            .cursor_pointer()
                            .opacity(0.0)
                            .hover(|style| style.opacity(1.0))
                            .on_click(cx.listener(
                                |this: &mut RootView,
                                 _: &gpui::ClickEvent,
                                 _window: &mut Window,
                                 cx: &mut Context<RootView>| {
                                    this.do_play_pause(cx);
                                },
                            ))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(
                                div()
                                    .absolute()
                                    .inset_0()
                                    .rounded(px(8.0))
                                    .bg(tokens.black.opacity(0.45)),
                            )
                            .child(
                                div()
                                    .size(px(54.0))
                                    .rounded(px(100.0))
                                    .bg(tokens.white)
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .shadow(vec![
                                        BoxShadow::new(
                                            px(0.),
                                            px(6.),
                                            tokens.black.opacity(0.5).into(),
                                        )
                                        .blur_radius(px(18.)),
                                    ])
                                    .child(icons::styled(
                                        if paused {
                                            icons::play(px(20.0))
                                        } else {
                                            icons::pause(px(20.0))
                                        },
                                        tokens.black,
                                        tokens.black,
                                    )),
                            ),
                    ),
            )
    }

    /// Compact hero used while a bottom panel is open: small cover plus
    /// title/artist in one row so the queue/lyrics list keeps vertical space.
    pub(crate) fn stage_compact_hero(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let track = self.playback.current.clone();
        let favorited = track
            .as_ref()
            .is_some_and(|track| self.playback.favorites.iter().any(|id| id == &track.id));
        let (title, meta, glyph, cover) = match &track {
            Some(track) => (
                track.name.clone(),
                track_meta(track),
                cover_glyph(&track.name),
                self.current_cover.clone(),
            ),
            None => (
                "Nothing playing".to_string(),
                "Pick a track from the library".to_string(),
                "o".into(),
                None,
            ),
        };

        let mut row = div()
            .id("stage-hero-compact")
            .accessibility_id("optionmusic.stage.hero.compact")
            .px(px(14.0))
            .py(px(10.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(self.cover_element("cover-compact", cover, glyph, px(52.0), tokens))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(tokens.ink)
                            .truncate()
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .truncate()
                            .child(meta),
                    ),
            );

        if let Some(track) = track {
            let id = track.id.clone();
            row = row.child(self.icon_button(
                "stage-fav-compact",
                if favorited {
                    "Remove from favorites"
                } else {
                    "Favorite"
                },
                icons::styled(
                    if favorited {
                        icons::heart(px(15.0))
                    } else {
                        icons::heart_off(px(15.0))
                    },
                    if favorited { tokens.ink } else { tokens.mute },
                    tokens.ink,
                ),
                move |this: &mut RootView, _: &mut Window, cx: &mut Context<RootView>| {
                    this.do_toggle_favorite(&id, cx);
                },
                tokens,
                cx,
            ));
        }
        row
    }

    /// Expanded metadata block: title with an inline favorite button, artist,
    /// album, and a small spec line (format · duration · track number).
    pub(crate) fn stage_meta(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let track = self.playback.current.clone();

        let mut block = div()
            .id("stage-meta")
            .accessibility_id("optionmusic.stage.meta")
            .px(px(18.0))
            .pt(px(2.0))
            .flex()
            .flex_col()
            .gap(px(4.0));

        let Some(track) = track else {
            return block
                .child(
                    div()
                        .text_size(px(16.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(tokens.ink)
                        .child("Nothing playing"),
                )
                .child(
                    div()
                        .text_size(px(12.0))
                        .text_color(tokens.mute)
                        .line_height(gpui::relative(1.4))
                        .child("Pick a track from the library — or drop files anywhere."),
                );
        };

        let favorited = self.playback.favorites.iter().any(|id| id == &track.id);
        let id = track.id.clone();

        block = block.child(
            div()
                .flex()
                .items_center()
                .gap(px(6.0))
                .child(
                    div()
                        .flex_1()
                        .min_w(px(0.0))
                        .text_size(px(16.0))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(tokens.ink)
                        .truncate()
                        .child(track.name.clone()),
                )
                .child(self.icon_button(
                    "stage-fav",
                    if favorited {
                        "Remove from favorites"
                    } else {
                        "Favorite"
                    },
                    icons::styled(
                        if favorited {
                            icons::heart(px(15.0))
                        } else {
                            icons::heart_off(px(15.0))
                        },
                        if favorited { tokens.ink } else { tokens.mute },
                        tokens.ink,
                    ),
                    move |this: &mut RootView, _: &mut Window, cx: &mut Context<RootView>| {
                        this.do_toggle_favorite(&id, cx);
                    },
                    tokens,
                    cx,
                )),
        );

        block = block.child(
            div()
                .text_size(px(13.0))
                .text_color(tokens.ink_2)
                .truncate()
                .child(track_meta(&track)),
        );

        let album = track.album.trim();
        if !album.is_empty() && album != track_meta(&track).as_str() {
            block = block.child(
                div()
                    .text_size(px(12.0))
                    .text_color(tokens.mute)
                    .truncate()
                    .child(album.to_string()),
            );
        }

        let spec = track_spec(&track, self.playback.duration);
        if !spec.is_empty() {
            block = block.child(
                div()
                    .pt(px(2.0))
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(tokens.faint)
                    .truncate()
                    .child(spec),
            );
        }

        block
    }

    /// Bottom control cluster: seek bar, transport row, and the queue/lyrics
    /// panel toggles (highlighted while their panel is open).
    pub(crate) fn stage_controls(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let queue_open = matches!(self.stage_panel, Some(StagePanel::Queue));
        let lyrics_open = matches!(self.stage_panel, Some(StagePanel::Lyrics));

        div()
            .id("stage-controls")
            .accessibility_id("optionmusic.stage.controls")
            .px(px(18.0))
            .pt(px(14.0))
            .pb(px(16.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .child(self.scrub_bar("scrub-stage", tokens, cx))
            .child(self.transport_cluster("stage", tokens, cx))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(
                        self.stage_action(
                            "Queue",
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(icons::styled(
                                    icons::list(px(14.0)),
                                    tokens.mute,
                                    tokens.ink,
                                ))
                                .child("Queue"),
                            tokens,
                            cx,
                            |this: &mut RootView,
                             window: &mut Window,
                             cx: &mut Context<RootView>| {
                                this.do_toggle_queue(cx);
                                this.save_desktop_prefs(window, cx);
                            },
                        )
                        .when(queue_open, |this| {
                            this.bg(tokens.lift_2)
                                .text_color(tokens.ink)
                                .border_color(tokens.border_strong)
                        }),
                    )
                    .child(
                        self.stage_action(
                            "Lyrics",
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .child(icons::styled(
                                    icons::lyrics(px(14.0)),
                                    tokens.mute,
                                    tokens.ink,
                                ))
                                .child("Lyrics"),
                            tokens,
                            cx,
                            |this: &mut RootView,
                             window: &mut Window,
                             cx: &mut Context<RootView>| {
                                this.do_toggle_lyrics(cx);
                                this.save_desktop_prefs(window, cx);
                            },
                        )
                        .when(lyrics_open, |this| {
                            this.bg(tokens.lift_2)
                                .text_color(tokens.ink)
                                .border_color(tokens.border_strong)
                        }),
                    ),
            )
    }

    pub(crate) fn stage_queue(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let queue_count = self.playback.queue.len();
        let empty = queue_count == 0;

        div()
            .id("stage-queue")
            .accessibility_id("optionmusic.stage.queue")
            .flex_1()
            .min_h(px(140.0))
            .border_t_1()
            .border_color(tokens.border)
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.faint)
                    .child("UP NEXT")
                    .child(
                        div()
                            .text_color(tokens.mute)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child(format!("{queue_count}")),
                    )
                    .child(div().flex_1())
                    .child(self.icon_button(
                        "queue-jump",
                        "Jump to current",
                        icons::styled(icons::locate(px(14.0)), tokens.mute, tokens.ink),
                        |this: &mut RootView, _w, cx: &mut Context<RootView>| {
                            this.queue_jump_to_current(cx)
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.icon_button(
                        "queue-clear",
                        "Clear queue",
                        icons::styled(icons::close(px(15.0)), tokens.mute, tokens.ink),
                        |this: &mut RootView, window, cx: &mut Context<RootView>| {
                            this.request_confirm(
                                crate::model::ConfirmRequest {
                                    title: "Clear queue?".into(),
                                    detail: "Playback stops when the queue ends.".into(),
                                    confirm_label: "Clear".into(),
                                    action: crate::model::ConfirmAction::ClearQueue,
                                },
                                cx,
                            );
                            this.overlay_focus.focus(window, cx);
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.icon_button(
                        "queue-close",
                        "Close",
                        icons::styled(icons::close(px(15.0)), tokens.mute, tokens.ink),
                        |this: &mut RootView, window, cx: &mut Context<RootView>| {
                            this.do_toggle_queue(cx);
                            this.save_desktop_prefs(window, cx);
                        },
                        tokens,
                        cx,
                    )),
            )
            .child(if empty {
                div()
                    .flex_1()
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(12.0))
                    .text_color(tokens.faint)
                    .child("Queue is empty — add with + on any track.")
                    .into_any_element()
            } else {
                div()
                    .id("queue-list")
                    .accessibility_id("optionmusic.queue")
                    .key_context("TrackList")
                    .track_focus(&self.list_focus)
                    .on_action(cx.listener(Self::list_up))
                    .on_action(cx.listener(Self::list_down))
                    .on_action(cx.listener(Self::list_first))
                    .on_action(cx.listener(Self::list_last))
                    .on_action(cx.listener(Self::on_list_activate))
                    .role(Role::List)
                    .aria_label("Up next")
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "queue-items",
                            queue_count,
                            cx.processor(
                                move |this: &mut RootView,
                                      range: Range<usize>,
                                      _window: &mut Window,
                                      cx: &mut Context<RootView>| {
                                    range
                                        .filter_map(|index| {
                                            let id = this.playback.queue.get(index)?;
                                            let track = this.track_by_id(id)?;
                                            Some(this.queue_row(track, index, tokens, cx))
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .size_full()
                        .track_scroll(&self.queue_scroll_handle),
                    )
                    .into_any_element()
            })
    }

    /// Right-hand "now playing" column: header, cover, metadata and a bottom
    /// control cluster. While a panel (queue/lyrics) is open the hero collapses
    /// into a compact strip and the panel takes the remaining height.
    pub(crate) fn stage(
        &self,
        tokens: MusicTokens,
        stage_h: f32,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        // Square cover sized so header + metadata + controls always fit.
        let cover_px = (stage_h - 296.0).clamp(150.0, 268.0);

        let mut stage = div()
            .id("stage")
            .accessibility_id("optionmusic.stage")
            .role(Role::Complementary)
            .aria_label("Now playing")
            .flex()
            .flex_col()
            .w(px(300.0))
            .flex_none()
            .border_l_1()
            .border_color(tokens.border)
            .bg(tokens.bg)
            .overflow_hidden()
            .child(self.stage_header(tokens, cx));

        if self.stage_panel.is_some() {
            stage = stage
                .child(self.stage_compact_hero(tokens, cx))
                .child(self.stage_controls(tokens, cx));
        } else {
            stage = stage
                .child(self.stage_hero(px(cover_px), tokens, cx))
                .child(div().flex_1().min_h(px(10.0)))
                .child(self.stage_meta(tokens, cx))
                .child(self.stage_controls(tokens, cx));
        }

        if matches!(self.stage_panel, Some(StagePanel::Queue)) {
            stage = stage.child(self.stage_queue(tokens, cx));
        }
        if matches!(self.stage_panel, Some(StagePanel::Lyrics)) {
            stage = stage.child(self.stage_lyrics(tokens, cx));
        }

        stage
    }

    /// Lyrics panel for the current track (loaded off-thread in
    /// `RootView::load_lyrics`).
    pub(crate) fn stage_lyrics(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let content: AnyElement = match &self.lyrics {
            Some(text) => div()
                .id("lyrics-text")
                .accessibility_id("optionmusic.lyrics")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .track_scroll(&self.lyrics_scroll)
                .text_size(px(12.0))
                .text_color(tokens.ink_2)
                .line_height(gpui::relative(1.55))
                .child(text.clone())
                .into_any_element(),
            None => div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(12.0))
                .text_color(tokens.faint)
                .child(if self.playback.current.is_some() {
                    "No lyrics for this track."
                } else {
                    "Play a track to see lyrics."
                })
                .into_any_element(),
        };

        div()
            .id("stage-lyrics")
            .accessibility_id("optionmusic.stage.lyrics")
            .role(Role::Group)
            .aria_label("Lyrics")
            .flex_1()
            .min_h(px(140.0))
            .border_t_1()
            .border_color(tokens.border)
            .p(px(12.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .px(px(6.0))
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.faint)
                    .child("LYRICS")
                    .child(div().flex_1())
                    .child(self.icon_button(
                        "lyrics-close",
                        "Close",
                        icons::styled(icons::close(px(15.0)), tokens.mute, tokens.ink),
                        |this: &mut RootView, window, cx: &mut Context<RootView>| {
                            this.do_toggle_lyrics(cx);
                            this.save_desktop_prefs(window, cx);
                        },
                        tokens,
                        cx,
                    )),
            )
            .child(content)
    }
}

/// Compact spec line for the stage: "FLAC · 3:41 · TRK 03".
fn track_spec(track: &TrackDto, duration: Option<f64>) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(ext) = Path::new(&track.path)
        .extension()
        .and_then(|ext| ext.to_str())
        .map(str::trim)
        .filter(|ext| !ext.is_empty())
    {
        parts.push(ext.to_uppercase());
    }
    if let Some(secs) = duration.or(track.duration_secs).filter(|s| *s > 0.0) {
        parts.push(fmt_time(secs));
    }
    if let Some(disc) = track.disc_number.filter(|d| *d > 1) {
        parts.push(format!("DISC {disc}"));
    }
    if let Some(number) = track.track_number {
        parts.push(format!("TRK {number:02}"));
    }
    parts.join(" · ")
}
