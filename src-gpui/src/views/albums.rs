use std::ops::Range;

use gpui::{AnyElement, Context, Role, Window, div, prelude::*, px, uniform_list};

use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;

impl RootView {
    /// Albums index or album detail, depending on `selected_album`.
    pub(crate) fn albums_page(&self, tokens: MusicTokens, cx: &mut Context<Self>) -> AnyElement {
        if self.selected_album.is_some() {
            self.album_detail(tokens, cx)
        } else {
            self.albums_view(tokens, cx)
        }
    }

    /// Grid-free list of albums — cover thumb, name, artist, track count.
    /// Enter/click opens the album detail.
    pub(crate) fn albums_view(&self, tokens: MusicTokens, cx: &mut Context<Self>) -> AnyElement {
        let set_size = self.filtered_albums.len();
        if set_size == 0 {
            return self
                .empty_state(
                    "No albums found",
                    "Albums appear once tags are enriched (runs automatically after scan).",
                    icons::disc(px(28.0)).text_color(tokens.mute),
                    tokens,
                )
                .into_any_element();
        }
        div()
            .id("albums-list")
            .accessibility_id("optionmusic.albums")
            .role(Role::List)
            .aria_label("Albums")
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
            .overflow_hidden()
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "album-items",
                            set_size,
                            cx.processor(
                                move |this: &mut RootView,
                                      range: Range<usize>,
                                      _window: &mut Window,
                                      cx: &mut Context<RootView>| {
                                    range
                                        .filter_map(|index| {
                                            this.filtered_albums.get(index).map(|album| {
                                                this.album_row(album, index, set_size, tokens, cx)
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .size_full()
                        .track_scroll(&self.album_scroll_handle),
                    )
                    .child(self.scrollbar(
                        "albums-scrollbar",
                        &self.album_scroll_handle,
                        tokens,
                        cx,
                    )),
            )
            .into_any_element()
    }

    pub(crate) fn album_row(
        &self,
        album: &AlbumEntry,
        index: usize,
        set_size: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let key = album.key.clone();
        let focused =
            self.focused_list == Some(FocusedList::Albums) && self.focused_row == Some(index);
        // Representative cover: first track's art, lazily resolved.
        let cover = album
            .track_ids
            .first()
            .and_then(|id| self.track_by_id(id))
            .and_then(|t| self.cover_for(t, cx));
        div()
            .id(format!("album-row-{index}"))
            .accessibility_id(format!("optionmusic.album.{index}"))
            .role(Role::ListItem)
            .aria_label(format!("{} — {}", album.name, album.artist))
            .aria_position_in_set(index + 1)
            .aria_size_of_set(set_size)
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(56.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .cursor_pointer()
            .rounded(px(6.0))
            .border_t_1()
            .border_color(tokens.border)
            .hover(|style| style.bg(tokens.hover))
            .active(|style| style.opacity(0.8))
            .when(focused, |this| {
                this.bg(tokens.selected).shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.focus_list(FocusedList::Albums, index, window, cx);
                    this.select_album(key.clone(), cx);
                },
            ))
            .child(self.cover_element(
                format!("cover-album-{}", album.key),
                cover,
                cover_glyph(&album.name),
                px(40.0),
                tokens,
            ))
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
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(tokens.ink)
                            .child(album.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .child(format!(
                                "{} · {} track{}",
                                album.artist,
                                album.track_ids.len(),
                                if album.track_ids.len() == 1 { "" } else { "s" }
                            )),
                    ),
            )
            .child(icons::chevron_right(px(14.0)).text_color(tokens.mute))
    }

    /// Album detail: header (cover, name, artist, play) + ordered track list.
    pub(crate) fn album_detail(&self, tokens: MusicTokens, cx: &mut Context<Self>) -> AnyElement {
        let Some(key) = self.selected_album.clone() else {
            return div().into_any_element();
        };
        let album = self
            .albums_index
            .iter()
            .find(|a| a.key == key)
            .cloned()
            .unwrap_or(AlbumEntry {
                key: key.clone(),
                name: "Album".into(),
                artist: "".into(),
                track_ids: Vec::new(),
            });
        let cover = album
            .track_ids
            .first()
            .and_then(|id| self.track_by_id(id))
            .and_then(|t| self.cover_for(t, cx));
        let play_ids = album.track_ids.clone();
        let count = self.album_tracks.len();
        let header = div()
            .id("album-detail-header")
            .accessibility_id("optionmusic.album.detail.header")
            .px(px(12.0))
            .py(px(14.0))
            .flex()
            .items_center()
            .gap(px(14.0))
            .border_b_1()
            .border_color(tokens.border)
            .child(self.icon_button(
                "back-album",
                "Back",
                icons::styled(icons::skip_back(px(16.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, _window, cx: &mut Context<RootView>| this.clear_album(cx),
                tokens,
                cx,
            ))
            .child(self.cover_element(
                format!("cover-album-hero-{key}"),
                cover,
                cover_glyph(&album.name),
                px(64.0),
                tokens,
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .flex_1()
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(16.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(tokens.ink)
                            .child(album.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(tokens.mute)
                            .child(format!(
                                "{} · {} track{}",
                                album.artist,
                                count,
                                if count == 1 { "" } else { "s" }
                            )),
                    ),
            )
            .child(
                div()
                    .id("album-play")
                    .role(Role::Button)
                    .aria_label("Play album")
                    .h(px(30.0))
                    .px(px(14.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(100.0))
                    .bg(tokens.white)
                    .text_color(tokens.black)
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .cursor_pointer()
                    .hover(|style| style.bg(tokens.ink))
                    .active(|style| style.opacity(0.85))
                    .on_click(cx.listener(
                        move |this: &mut RootView,
                              _: &gpui::ClickEvent,
                              _window: &mut Window,
                              cx: &mut Context<RootView>| {
                            this.play_album(play_ids.clone(), cx);
                        },
                    ))
                    .child(icons::play(px(12.0)))
                    .child("Play"),
            );

        let tracks = self.album_tracks.clone();
        self.tracks_view(
            FocusedList::AlbumTracks,
            "album-detail",
            "album-tracks",
            album.name.clone(),
            tracks,
            &self.album_scroll_handle,
            header,
            tokens,
            cx,
        )
        .into_any_element()
    }
}
