//! Stats page — desktop parity with `msc stats`: a totals header (rendered
//! by `catalog()` as the page subtitle), then Top tracks / Top artists
//! sections and a scrollable play-history list. Data is re-read from disk
//! on every navigate (see `refresh_stats`) so plays recorded by other
//! sessions show up without a restart.

use std::ops::Range;
use std::path::Path;
use std::rc::Rc;

use gpui::{AnyElement, Context, Role, Window, div, prelude::*, px, uniform_list};
use optionmusic::controller::StatTrackView;
use optionmusic::history::HistoryEntry;

use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;

/// `msc stats` tops out at `-n 10` by default; history reads the newest 50.
const STATS_TOP: usize = 10;
const STATS_HISTORY: usize = 50;
const TRACK_ROW_H: f32 = 42.0;
const TRACK_ROWS_VISIBLE: usize = 6;
const ARTIST_ROWS_SHOWN: usize = 6;
const ARTIST_ROW_H: f32 = 30.0;

impl RootView {
    /// Stats and history live in files other sessions can write — reload
    /// them each time the page opens, and on playback changes while shown.
    pub(crate) fn refresh_stats(&mut self) {
        let Some(controller) = self.controller.as_ref() else {
            return;
        };
        self.stats_cache = Some(Rc::new(controller.stats_view(STATS_TOP)));
        self.history_cache = controller.history_entries(STATS_HISTORY).into();
    }

    /// Section heading + muted count, e.g. "TOP TRACKS  10" (same chrome as
    /// the stage's "UP NEXT" header).
    fn stats_section_label(
        &self,
        label: &'static str,
        count: usize,
        tokens: MusicTokens,
    ) -> gpui::Div {
        div()
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(
                div()
                    .text_size(px(11.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.faint)
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(tokens.mute)
                    .child(format!("{count}")),
            )
    }

    /// "TOP TRACKS" row: rank, name + artist, play count, listening time.
    /// Clickable only while the track is still in the library.
    pub(crate) fn stat_track_row(
        &self,
        entry: &StatTrackView,
        index: usize,
        set_size: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let playable = entry.track.is_some();
        let focused =
            self.focused_list == Some(FocusedList::StatsTracks) && self.focused_row == Some(index);
        let track_id = entry.track.as_ref().map(|track| track.id.clone());
        let artist = if entry.stat.artist.trim().is_empty() {
            entry.track.as_ref().map(track_artist).unwrap_or_default()
        } else {
            entry.stat.artist.clone()
        };
        let plays = if entry.stat.plays == 1 {
            "1 play".to_owned()
        } else {
            format!("{} plays", entry.stat.plays)
        };

        let mut row = div()
            .id(format!("stat-track-{index}"))
            .accessibility_id(format!("optionmusic.stats.track.{index}"))
            .role(Role::ListItem)
            .aria_label(format!("{} — {artist}", entry.stat.name))
            .aria_position_in_set(index + 1)
            .aria_size_of_set(set_size)
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(TRACK_ROW_H))
            .px(px(6.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .text_color(tokens.ink_2)
            .when(playable, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(tokens.hover))
                    .active(|style| style.opacity(0.8))
            })
            .when(!playable, |this| this.opacity(0.5))
            .when(focused, |this| {
                this.bg(tokens.selected)
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            });
        if let Some(id) = track_id {
            row = row.on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.focus_list(FocusedList::StatsTracks, index, window, cx);
                    this.play_track(&id, cx);
                },
            ));
        }
        row.child(
            div()
                .w(px(20.0))
                .text_center()
                .text_size(px(10.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(tokens.faint)
                .child(format!("{:>2}", index + 1)),
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
                        .truncate()
                        .child(entry.stat.name.clone()),
                )
                .child(
                    div()
                        .text_size(px(10.0))
                        .text_color(tokens.mute)
                        .truncate()
                        .child(artist),
                ),
        )
        .child(
            div()
                .w(px(76.0))
                .text_right()
                .text_size(px(10.0))
                .text_color(tokens.mute)
                .child(plays),
        )
        .child(
            div()
                .w(px(64.0))
                .text_right()
                .text_size(px(10.0))
                .text_color(tokens.faint)
                .child(fmt_time(entry.stat.total_secs as f64)),
        )
    }

    /// "TOP ARTISTS" row — display only, matching the CLI's columns.
    pub(crate) fn stat_artist_row(
        &self,
        artist: &str,
        plays: u64,
        index: usize,
        tokens: MusicTokens,
    ) -> gpui::Stateful<gpui::Div> {
        let plays_label = if plays == 1 {
            "1 play".to_owned()
        } else {
            format!("{plays} plays")
        };
        div()
            .id(format!("stat-artist-{index}"))
            .accessibility_id(format!("optionmusic.stats.artist.{index}"))
            .h(px(ARTIST_ROW_H))
            .px(px(6.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .w(px(20.0))
                    .text_center()
                    .text_size(px(10.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(tokens.faint)
                    .child(format!("{:>2}", index + 1)),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(tokens.ink_2)
                    .child(artist.to_string()),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(tokens.mute)
                    .child(plays_label),
            )
    }

    /// "HISTORY" row: track name (+ artist when tagged) and how long ago it
    /// played. Clickable only while the file is still in the library.
    pub(crate) fn stat_history_row(
        &self,
        entry: &HistoryEntry,
        index: usize,
        set_size: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let track = self.track_by_id(&entry.id);
        let playable = track.is_some();
        let focused =
            self.focused_list == Some(FocusedList::StatsHistory) && self.focused_row == Some(index);
        let track_id = track.map(|track| track.id.clone());
        let name = track.map(|track| track.name.clone()).unwrap_or_else(|| {
            Path::new(&entry.id)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .unwrap_or("Unknown track")
                .to_string()
        });
        let meta = track
            .map(track_artist)
            .unwrap_or_else(|| "not in library".to_owned());

        let mut row = div()
            .id(format!("stat-history-{index}"))
            .accessibility_id(format!("optionmusic.stats.history.{index}"))
            .role(Role::ListItem)
            .aria_label(format!("{name} — {meta}"))
            .aria_position_in_set(index + 1)
            .aria_size_of_set(set_size)
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(32.0))
            .px(px(6.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .text_color(tokens.ink_2)
            .when(playable, |this| {
                this.cursor_pointer()
                    .hover(|style| style.bg(tokens.hover))
                    .active(|style| style.opacity(0.8))
            })
            .when(!playable, |this| this.opacity(0.5))
            .when(focused, |this| {
                this.bg(tokens.selected)
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            });
        if let Some(id) = track_id {
            row = row.on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.focus_list(FocusedList::StatsHistory, index, window, cx);
                    this.play_track(&id, cx);
                },
            ));
        }
        row.child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .text_size(px(12.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(name),
        )
        .child(
            div()
                .text_size(px(10.0))
                .text_color(tokens.mute)
                .truncate()
                .child(meta),
        )
        .child(
            div()
                .w(px(64.0))
                .text_right()
                .text_size(px(10.0))
                .text_color(tokens.faint)
                .child(relative_time(entry.at)),
        )
    }

    /// Stats page body: totals live in the catalog header subtitle; the body
    /// stacks Top tracks (capped, scrollable), Top artists (capped rows) and
    /// a History list filling the rest of the column.
    pub(crate) fn stats_page(&self, tokens: MusicTokens, cx: &mut Context<Self>) -> AnyElement {
        let stats = self.stats_cache.clone();
        let history = self.history_cache.clone();

        if stats.as_ref().is_none_or(|s| s.total_plays == 0) && history.is_empty() {
            return self
                .empty_state(
                    "No plays recorded yet",
                    "Play something past halfway — counts show up here.",
                    icons::stats(px(28.0)).text_color(tokens.mute),
                    tokens,
                )
                .into_any_element();
        }

        let mut page = div()
            .id("stats-page")
            .accessibility_id("optionmusic.stats")
            .role(Role::Group)
            .aria_label("Stats")
            .key_context("TrackList")
            .track_focus(&self.list_focus)
            .on_action(cx.listener(Self::list_up))
            .on_action(cx.listener(Self::list_down))
            .on_action(cx.listener(Self::list_first))
            .on_action(cx.listener(Self::list_last))
            .on_action(cx.listener(Self::on_list_activate))
            .flex()
            .flex_col()
            .flex_1()
            .min_h_0()
            .gap(px(14.0))
            .overflow_hidden()
            .focus_visible(|style| {
                style
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            });

        if let Some(stats) = stats {
            let top_len = stats.top_tracks.len();
            if top_len > 0 {
                let shown = top_len.min(TRACK_ROWS_VISIBLE) as f32;
                let top_tracks = stats.clone();
                page = page
                    .child(self.stats_section_label("TOP TRACKS", top_len, tokens))
                    .child(
                        div()
                            .id("stats-top")
                            .relative()
                            .h(px(shown * TRACK_ROW_H))
                            .child(
                                uniform_list(
                                    "stats-top-items",
                                    top_len,
                                    cx.processor(
                                        move |this: &mut RootView,
                                              range: Range<usize>,
                                              _window: &mut Window,
                                              cx: &mut Context<RootView>| {
                                            range
                                                .filter_map(|index| {
                                                    top_tracks.top_tracks.get(index).map(|entry| {
                                                        this.stat_track_row(
                                                            entry, index, top_len, tokens, cx,
                                                        )
                                                    })
                                                })
                                                .collect::<Vec<_>>()
                                        },
                                    ),
                                )
                                .size_full()
                                .track_scroll(&self.stats_scroll_handle),
                            )
                            .child(self.scrollbar(
                                "stats-top-scrollbar",
                                &self.stats_scroll_handle,
                                tokens,
                                cx,
                            )),
                    );
            }

            let artists = &stats.top_artists[..stats.top_artists.len().min(ARTIST_ROWS_SHOWN)];
            if !artists.is_empty() {
                let mut rows = div().flex().flex_col().flex_none();
                for (index, (artist, plays)) in artists.iter().enumerate() {
                    rows = rows.child(self.stat_artist_row(artist, *plays, index, tokens));
                }
                page = page
                    .child(self.stats_section_label("TOP ARTISTS", artists.len(), tokens))
                    .child(rows);
            }
        }

        if !history.is_empty() {
            let hist_len = history.len();
            page = page
                .child(self.stats_section_label("HISTORY", hist_len, tokens))
                .child(
                    div()
                        .id("stats-history")
                        .relative()
                        .flex_1()
                        .min_h_0()
                        .child(
                            uniform_list(
                                "stats-history-items",
                                hist_len,
                                cx.processor(
                                    move |this: &mut RootView,
                                          range: Range<usize>,
                                          _window: &mut Window,
                                          cx: &mut Context<RootView>| {
                                        range
                                            .filter_map(|index| {
                                                history.get(index).map(|entry| {
                                                    this.stat_history_row(
                                                        entry, index, hist_len, tokens, cx,
                                                    )
                                                })
                                            })
                                            .collect::<Vec<_>>()
                                    },
                                ),
                            )
                            .size_full()
                            .track_scroll(&self.stats_history_scroll_handle),
                        )
                        .child(self.scrollbar(
                            "stats-history-scrollbar",
                            &self.stats_history_scroll_handle,
                            tokens,
                            cx,
                        )),
                );
        }

        page.into_any_element()
    }
}
