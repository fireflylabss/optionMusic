use std::ops::Range;

use gpui::{
    AnyElement, Context, MouseButton, MouseDownEvent, Role, SharedString, UniformListScrollHandle,
    Window, div, prelude::*, px, uniform_list,
};
use optionmusic::controller::{SmartShelf, TrackDto};
use optionmusic::saved_playlists::SavedPlaylist;

use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;

impl RootView {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn track_row(
        &self,
        list: FocusedList,
        track: &TrackDto,
        index: usize,
        set_size: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let active = self
            .playback
            .current
            .as_ref()
            .is_some_and(|current| current.id == track.id);
        let playing = active && !self.playback.paused && !self.playback.stopped;
        let favorited = self.playback.favorites.iter().any(|id| id == &track.id);
        let focused = self.focused_list == Some(list) && self.focused_row == Some(index);
        let index_element: gpui::AnyElement = if playing {
            icons::styled(icons::play(px(12.0)), tokens.white, tokens.white).into_any_element()
        } else {
            format!("{:02}", index + 1).into_any_element()
        };
        let meta = track_meta(track);
        let folder = folder_label(&track.folder);
        let play_id = track.id.clone();
        let favorite_id = track.id.clone();
        let queue_id = track.id.clone();
        let menu_id = track.id.clone();
        let row_id = track.id.clone();
        let cover = self.cover_for(track, cx);

        div()
            .id(format!("track-row-{row_id}"))
            .accessibility_id(format!("optionmusic.track.{row_id}"))
            .role(Role::ListItem)
            .aria_label(track.name.clone())
            .aria_position_in_set(index + 1)
            .aria_size_of_set(set_size)
            .aria_selected(active)
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(58.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .cursor_pointer()
            .rounded(px(6.0))
            .border_t_1()
            .border_color(tokens.border)
            .when(active, |this| {
                this.bg(tokens.selected).border_color(tokens.border_strong)
            })
            .when(!active, |this| {
                this.hover(|style| style.bg(tokens.hover).border_color(tokens.border_strong))
            })
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
                    this.focus_list(list, index, window, cx);
                    this.play_track(&play_id, cx);
                },
            ))
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(
                    move |this: &mut RootView,
                          event: &MouseDownEvent,
                          window: &mut Window,
                          cx: &mut Context<RootView>| {
                        this.focus_list(list, index, window, cx);
                        this.open_context_menu(menu_id.clone(), event.position, false, window, cx);
                        cx.stop_propagation();
                    },
                ),
            )
            .child(
                div()
                    .w(px(32.0))
                    .h(px(32.0))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .text_color(if active { tokens.white } else { tokens.faint })
                    .child(index_element),
            )
            .child(self.cover_element(
                format!("cover-row-{row_id}"),
                cover,
                cover_glyph(&track.name),
                px(40.0),
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
                            .text_size(px(13.0))
                            .font_weight(if active {
                                gpui::FontWeight::SEMIBOLD
                            } else {
                                gpui::FontWeight::MEDIUM
                            })
                            .text_color(if active { tokens.white } else { tokens.ink_2 })
                            .child(track.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .w(px(140.0))
                    .flex_none()
                    .text_size(px(12.0))
                    .text_color(tokens.faint)
                    .child(folder),
            )
            .child(
                div()
                    .w(px(80.0))
                    .flex_none()
                    .flex()
                    .items_center()
                    .justify_end()
                    .gap(px(4.0))
                    .child(self.icon_button(
                        format!("fav-{row_id}"),
                        "Favorite",
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
                            this.do_toggle_favorite(&favorite_id, cx);
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.icon_button(
                        format!("queue-{row_id}"),
                        "Add to queue",
                        icons::styled(icons::plus(px(15.0)), tokens.mute, tokens.ink),
                        move |this: &mut RootView, _: &mut Window, cx: &mut Context<RootView>| {
                            this.do_add_queue(&queue_id, cx);
                        },
                        tokens,
                        cx,
                    )),
            )
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn tracks_view(
        &self,
        list: FocusedList,
        container_id: impl Into<gpui::ElementId>,
        list_id: impl Into<gpui::ElementId>,
        aria_label: impl Into<SharedString>,
        tracks: std::rc::Rc<[TrackDto]>,
        scroll_handle: &UniformListScrollHandle,
        header: impl IntoElement,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tracks = tracks.clone();
        let set_size = tracks.len();
        div()
            .id(container_id)
            .accessibility_id("optionmusic.tracks")
            .role(Role::List)
            .aria_label(aria_label)
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
            .focus_visible(|style| {
                style
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            list_id,
                            set_size,
                            cx.processor(
                                move |this: &mut RootView,
                                      range: Range<usize>,
                                      _window: &mut Window,
                                      cx: &mut Context<RootView>| {
                                    range
                                        .filter_map(|index| {
                                            tracks.get(index).map(|track| {
                                                this.track_row(
                                                    list, track, index, set_size, tokens, cx,
                                                )
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .size_full()
                        .track_scroll(scroll_handle),
                    )
                    .child(self.scrollbar("tracks-scrollbar", scroll_handle, tokens, cx)),
            )
    }

    pub(crate) fn track_header(&self, tokens: MusicTokens) -> gpui::Stateful<gpui::Div> {
        div()
            .id("track-header")
            .accessibility_id("optionmusic.tracks.header")
            .h(px(34.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(tokens.border)
            .text_color(tokens.faint)
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(div().w(px(32.0)).text_center().child("#"))
            .child(div().w(px(40.0)))
            .child(div().flex_1().child("Title"))
            .child(div().w(px(140.0)).child("Folder"))
            .child(div().w(px(80.0)))
    }

    pub(crate) fn track_list(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tracks = self.filtered_library.clone();
        self.tracks_view(
            FocusedList::Tracks,
            "track-list",
            "track-items",
            "Library tracks",
            tracks,
            &self.scroll_handle,
            self.track_header(tokens),
            tokens,
            cx,
        )
    }

    pub(crate) fn artist_row(
        &self,
        name: &SharedString,
        count: usize,
        index: usize,
        set_size: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let select_name = name.clone();
        let display_name = name.clone();
        let label_name = name.clone();
        let focused =
            self.focused_list == Some(FocusedList::Artists) && self.focused_row == Some(index);
        div()
            .id(format!("artist-row-{index}"))
            .accessibility_id(format!("optionmusic.artist.{index}"))
            .role(Role::ListItem)
            .aria_label(label_name)
            .aria_position_in_set(index + 1)
            .aria_size_of_set(set_size)
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(50.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .cursor_pointer()
            .rounded(px(6.0))
            .border_t_1()
            .border_color(tokens.border)
            .hover(|style| style.bg(tokens.hover))
            .active(|style| style.opacity(0.8))
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
                    this.focus_list(FocusedList::Artists, index, window, cx);
                    this.select_artist(select_name.clone(), cx);
                },
            ))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .size(px(36.0))
                            .rounded_full()
                            .bg(tokens.elevated)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::user(px(16.0)).text_color(tokens.mute)),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(tokens.ink)
                            .child(display_name),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .text_color(tokens.faint)
                            .child(format!(
                                "{} track{}",
                                count,
                                if count == 1 { "" } else { "s" }
                            )),
                    )
                    .child(icons::chevron_right(px(14.0)).text_color(tokens.mute)),
            )
    }

    pub(crate) fn artists_view(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let header = div()
            .id("artists-header")
            .accessibility_id("optionmusic.artists.header")
            .h(px(34.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(tokens.border)
            .text_color(tokens.faint)
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(div().flex_1().child("Artist"))
            .child(div().w(px(80.0)).text_right().child("Tracks"));

        let set_size = self.filtered_artists.len();
        let empty = self.filtered_artists.is_empty();

        if empty {
            return div()
                .id("artists-list")
                .flex()
                .flex_col()
                .flex_1()
                .child(header)
                .child(
                    self.empty_state(
                        "No artists found",
                        "Add music with artist tags to see them here.",
                        icons::user(px(28.0)).text_color(tokens.mute),
                        tokens,
                    )
                    .into_any_element(),
                );
        }

        div()
            .id("artists-list")
            .accessibility_id("optionmusic.artists")
            .role(Role::List)
            .aria_label("Artists")
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
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "artist-items",
                            set_size,
                            cx.processor(
                                move |this: &mut RootView,
                                      range: Range<usize>,
                                      _window: &mut Window,
                                      cx: &mut Context<RootView>| {
                                    range
                                        .filter_map(|index| {
                                            this.filtered_artists.get(index).map(|(name, count)| {
                                                this.artist_row(
                                                    name, *count, index, set_size, tokens, cx,
                                                )
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .size_full()
                        .track_scroll(&self.artist_scroll_handle),
                    )
                    .child(self.scrollbar(
                        "artists-scrollbar",
                        &self.artist_scroll_handle,
                        tokens,
                        cx,
                    )),
            )
    }

    pub(crate) fn artist_detail(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let artist = self.selected_artist.clone().unwrap_or_default();
        let count = self.artist_tracks.len();
        let header = div()
            .id("artist-detail-header")
            .accessibility_id("optionmusic.artist.detail.header")
            .h(px(34.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(tokens.border)
            .child(self.icon_button(
                "back-artist",
                "Back",
                icons::styled(icons::skip_back(px(16.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, _window, cx: &mut Context<RootView>| this.clear_artist(cx),
                tokens,
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.ink)
                    .child(artist.clone()),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(tokens.faint)
                    .child(format!(
                        "{} track{}",
                        count,
                        if count == 1 { "" } else { "s" }
                    )),
            );

        let tracks = self.artist_tracks.clone();
        self.tracks_view(
            FocusedList::ArtistTracks,
            "artist-detail",
            "artist-tracks",
            artist,
            tracks,
            &self.artist_scroll_handle,
            header,
            tokens,
            cx,
        )
    }

    pub(crate) fn search_bar(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let query = self.search_input.read(cx).content.clone();
        let results = if query.is_empty() {
            0
        } else {
            self.filtered_library.len()
        };
        div()
            .id("search-bar")
            .accessibility_id("optionmusic.search.bar")
            .role(Role::Search)
            .aria_label("Filter tracks")
            .h(px(38.0))
            .mb(px(14.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(tokens.focus)
            .bg(tokens.elevated)
            .shadow(focus_shadow(tokens))
            .text_size(px(13.0))
            .child(icons::search(px(16.0)).text_color(tokens.ink))
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .items_center()
                    .child(self.search_input.clone()),
            )
            .when(!query.is_empty(), |this| {
                this.child(
                    div()
                        .text_size(px(11.0))
                        .text_color(tokens.mute)
                        .child(format!(
                            "{} result{}",
                            results,
                            if results == 1 { "" } else { "s" }
                        )),
                )
                .child(self.icon_button(
                    "search-clear",
                    "Clear",
                    icons::styled(icons::close(px(14.0)), tokens.mute, tokens.ink),
                    |this: &mut RootView, _window, cx: &mut Context<RootView>| {
                        this.search_input.update(cx, |input, cx| input.clear(cx));
                    },
                    tokens,
                    cx,
                ))
            })
    }

    pub(crate) fn favorites_view(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        if self.filtered_favorites.is_empty() {
            return div()
                .id("favorites-empty")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(
                    self.empty_state(
                        "No liked tracks yet",
                        "Tap the heart on any track to keep it here.",
                        icons::heart(px(28.0)).text_color(tokens.mute),
                        tokens,
                    )
                    .into_any_element(),
                );
        }
        let tracks = self.filtered_favorites.clone();
        self.tracks_view(
            FocusedList::Favorites,
            "favorites-list",
            "favorite-items",
            "Liked tracks",
            tracks,
            &self.fav_scroll_handle,
            self.track_header(tokens),
            tokens,
            cx,
        )
    }

    pub(crate) fn playlist_row(
        &self,
        playlist: &SavedPlaylist,
        index: usize,
        set_size: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let play_id = playlist.id.clone();
        let delete_id = playlist.id.clone();
        let delete_name = playlist.name.clone();
        let focused =
            self.focused_list == Some(FocusedList::Playlists) && self.focused_row == Some(index);
        div()
            .id(format!("playlist-row-{index}"))
            .accessibility_id(format!("optionmusic.playlist.{index}"))
            .role(Role::ListItem)
            .aria_label(playlist.name.clone())
            .aria_position_in_set(index + 1)
            .aria_size_of_set(set_size)
            .when(focused, |this| this.aria_active_descendant())
            .w_full()
            .h(px(50.0))
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
                this.bg(tokens.selected)
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.focus_list(FocusedList::Playlists, index, window, cx);
                    this.play_playlist(play_id.clone(), window, cx);
                },
            ))
            .child(
                div()
                    .size(px(36.0))
                    .rounded(px(8.0))
                    .bg(tokens.elevated)
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .child(icons::list(px(16.0)).text_color(tokens.mute)),
            )
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
                            .child(playlist.name.clone()),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .child(format!(
                                "{} track{}",
                                playlist.tracks.len(),
                                if playlist.tracks.len() == 1 { "" } else { "s" }
                            )),
                    ),
            )
            .child(self.icon_button(
                format!("playlist-rename-{index}"),
                "Rename playlist",
                icons::styled(icons::pencil(px(13.0)), tokens.mute, tokens.ink),
                {
                    let rename_id = playlist.id.clone();
                    move |this: &mut RootView, window: &mut Window, cx: &mut Context<RootView>| {
                        this.rename_playlist_prompt(rename_id.clone(), window, cx);
                    }
                },
                tokens,
                cx,
            ))
            .child(self.icon_button(
                format!("playlist-export-{index}"),
                "Export .m3u",
                icons::styled(icons::upload(px(14.0)), tokens.mute, tokens.ink),
                {
                    let export_id = playlist.id.clone();
                    move |this: &mut RootView, _w: &mut Window, cx: &mut Context<RootView>| {
                        this.export_m3u_flow(export_id.clone(), cx);
                    }
                },
                tokens,
                cx,
            ))
            .child(self.icon_button(
                format!("playlist-del-{index}"),
                "Delete playlist",
                icons::styled(icons::close(px(14.0)), tokens.mute, tokens.ink),
                move |this: &mut RootView, window: &mut Window, cx: &mut Context<RootView>| {
                    this.request_confirm(
                        ConfirmRequest {
                            title: format!("Delete “{delete_name}”?").into(),
                            detail: "The playlist file is removed; tracks stay in the library."
                                .into(),
                            confirm_label: "Delete".into(),
                            action: ConfirmAction::DeletePlaylist(delete_id.clone()),
                        },
                        cx,
                    );
                    this.overlay_focus.focus(window, cx);
                },
                tokens,
                cx,
            ))
    }

    pub(crate) fn playlists_view(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let header = div()
            .id("playlists-header")
            .accessibility_id("optionmusic.playlists.header")
            .h(px(34.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(tokens.border)
            .text_color(tokens.faint)
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .child(div().w(px(36.0)))
            .child(div().flex_1().child("Playlist"))
            .child(self.icon_button(
                "playlists-import",
                "Import .m3u",
                icons::styled(icons::download(px(14.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, _w, cx: &mut Context<RootView>| this.import_m3u_flow(cx),
                tokens,
                cx,
            ))
            .child(self.icon_button(
                "playlists-new",
                "New playlist",
                icons::styled(icons::plus(px(15.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, window, cx: &mut Context<RootView>| {
                    this.new_playlist_prompt(window, cx)
                },
                tokens,
                cx,
            ))
            .child(div().w(px(4.0)));

        let set_size = self.filtered_playlists.len();
        if set_size == 0 {
            return div()
                .id("playlists-empty")
                .flex()
                .flex_col()
                .flex_1()
                .min_h_0()
                .child(header)
                .child(
                    self.empty_state(
                        "No playlists yet",
                        "Use + above to create one, or import .m3u files — they'll appear here.",
                        icons::list(px(28.0)).text_color(tokens.mute),
                        tokens,
                    )
                    .into_any_element(),
                );
        }

        div()
            .id("playlists-list")
            .accessibility_id("optionmusic.playlists")
            .role(Role::List)
            .aria_label("Playlists")
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
            .child(header)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .child(
                        uniform_list(
                            "playlist-items",
                            set_size,
                            cx.processor(
                                move |this: &mut RootView,
                                      range: Range<usize>,
                                      _window: &mut Window,
                                      cx: &mut Context<RootView>| {
                                    range
                                        .filter_map(|index| {
                                            this.filtered_playlists.get(index).map(|playlist| {
                                                this.playlist_row(
                                                    playlist, index, set_size, tokens, cx,
                                                )
                                            })
                                        })
                                        .collect::<Vec<_>>()
                                },
                            ),
                        )
                        .size_full()
                        .track_scroll(&self.playlist_scroll_handle),
                    )
                    .child(self.scrollbar(
                        "playlists-scrollbar",
                        &self.playlist_scroll_handle,
                        tokens,
                        cx,
                    )),
            )
    }

    pub(crate) fn shelf_row(
        &self,
        shelf: SmartShelf,
        title: &'static str,
        subtitle: &'static str,
        index: usize,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let focused =
            self.focused_list == Some(FocusedList::Shelves) && self.focused_row == Some(index);
        div()
            .id(format!("shelf-row-{index}"))
            .accessibility_id(format!("optionmusic.shelf.{index}"))
            .role(Role::ListItem)
            .aria_label(title)
            .aria_position_in_set(index + 1)
            .aria_size_of_set(3)
            .when(focused, |this| this.aria_active_descendant())
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
                this.bg(tokens.selected)
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.focus_list(FocusedList::Shelves, index, window, cx);
                    this.select_shelf(shelf, cx);
                },
            ))
            .child(
                div()
                    .size(px(36.0))
                    .rounded(px(8.0))
                    .bg(tokens.elevated)
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .child(icons::grid(px(16.0)).text_color(tokens.mute)),
            )
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
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .child(subtitle),
                    ),
            )
            .child(icons::chevron_right(px(14.0)).text_color(tokens.mute))
    }

    pub(crate) fn shelves_view(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("shelves-list")
            .accessibility_id("optionmusic.shelves")
            .role(Role::List)
            .aria_label("Smart shelves")
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
            .child(self.shelf_row(
                SmartShelf::PlayedWeek,
                "Played this week",
                "Tracks from your listening history",
                0,
                tokens,
                cx,
            ))
            .child(self.shelf_row(
                SmartShelf::NoCover,
                "Missing covers",
                "Tracks without embedded or folder art",
                1,
                tokens,
                cx,
            ))
            .child(self.shelf_row(
                SmartShelf::IncompleteAlbums,
                "Incomplete albums",
                "Albums missing track numbers in your library",
                2,
                tokens,
                cx,
            ))
    }

    pub(crate) fn shelf_detail(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let title: &'static str = match self.selected_shelf {
            Some(SmartShelf::PlayedWeek) => "Played this week",
            Some(SmartShelf::NoCover) => "Missing covers",
            Some(SmartShelf::IncompleteAlbums) => "Incomplete albums",
            None => "Shelf",
        };
        let count = self.shelf_tracks.len();
        let header = div()
            .id("shelf-detail-header")
            .accessibility_id("optionmusic.shelf.detail.header")
            .h(px(34.0))
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .border_b_1()
            .border_color(tokens.border)
            .child(self.icon_button(
                "back-shelf",
                "Back",
                icons::styled(icons::skip_back(px(16.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, _window, cx: &mut Context<RootView>| this.clear_shelf(cx),
                tokens,
                cx,
            ))
            .child(
                div()
                    .flex_1()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.ink)
                    .child(title),
            )
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(tokens.faint)
                    .child(format!(
                        "{} track{}",
                        count,
                        if count == 1 { "" } else { "s" }
                    )),
            );

        let tracks = self.shelf_tracks.clone();
        self.tracks_view(
            FocusedList::ShelfTracks,
            "shelf-detail",
            "shelf-tracks",
            title,
            tracks,
            &self.shelf_scroll_handle,
            header,
            tokens,
            cx,
        )
    }

    pub(crate) fn catalog(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let loading = self.controller.is_none();

        let (title, subtitle): (SharedString, SharedString) = match self.page {
            Page::Library => (
                "Library".into(),
                if loading {
                    "Scanning…".into()
                } else if self.library.is_empty() {
                    "No music found".into()
                } else {
                    format!(
                        "{} of {} tracks · {} folders",
                        self.filtered_library.len(),
                        self.library.len(),
                        self.folders_count_cache
                    )
                    .into()
                },
            ),
            Page::Artists => (
                "Artists".into(),
                if loading {
                    "Scanning…".into()
                } else if self.selected_artist.is_some() {
                    self.selected_artist.clone().unwrap_or_default()
                } else if self.library.is_empty() {
                    "No music found".into()
                } else {
                    format!("{} artists", self.artists_index.len()).into()
                },
            ),
            Page::Albums => (
                "Albums".into(),
                if loading {
                    "Scanning…".into()
                } else if self.selected_album.is_some() {
                    self.filtered_albums
                        .iter()
                        .find(|a| Some(&a.key) == self.selected_album.as_ref())
                        .map(|a| a.name.clone())
                        .unwrap_or_else(|| "Album".into())
                } else {
                    format!("{} albums", self.albums_index.len()).into()
                },
            ),
            Page::Playlists => (
                "Playlists".into(),
                format!(
                    "{} playlist{}",
                    self.playlists.len(),
                    if self.playlists.len() == 1 { "" } else { "s" }
                )
                .into(),
            ),
            Page::Favorites => (
                "Favorites".into(),
                format!("{} liked", self.playback.favorites.len()).into(),
            ),
            Page::Shelves => (
                "Shelves".into(),
                if self.selected_shelf.is_some() {
                    match self.selected_shelf {
                        Some(SmartShelf::PlayedWeek) => "Played this week".into(),
                        Some(SmartShelf::NoCover) => "Missing covers".into(),
                        Some(SmartShelf::IncompleteAlbums) => "Incomplete albums".into(),
                        None => "Shelves".into(),
                    }
                } else {
                    "Automatic collections".into()
                },
            ),
        };

        let tools = div()
            .id("catalog-tools")
            .accessibility_id("optionmusic.catalog.tools")
            .flex()
            .items_center()
            .gap(px(6.0))
            .child(self.icon_button(
                "tools-search",
                "Search",
                icons::styled(icons::search(px(15.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, window, cx: &mut Context<RootView>| {
                    this.do_toggle_search(window, cx)
                },
                tokens,
                cx,
            ))
            .child(self.icon_button(
                "tools-queue",
                "Queue",
                icons::styled(icons::list(px(15.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, _window, cx: &mut Context<RootView>| this.do_toggle_queue(cx),
                tokens,
                cx,
            ))
            .child(self.icon_button(
                "tools-rescan",
                "Rescan",
                icons::styled(icons::refresh(px(15.0)), tokens.mute, tokens.ink),
                |this: &mut RootView, _window, cx: &mut Context<RootView>| this.do_rescan(cx),
                tokens,
                cx,
            ));

        let header = div()
            .id("catalog-head")
            .accessibility_id("optionmusic.catalog.head")
            .flex()
            .items_end()
            .justify_between()
            .gap(px(16.0))
            .mb(px(22.0))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .text_size(px(28.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(tokens.mute)
                            .child(subtitle.clone()),
                    ),
            )
            .child(tools);

        let body: AnyElement = if loading {
            self.empty_state(
                "Scanning your library",
                "Looking through your music folders…",
                icons::refresh(px(28.0)).text_color(tokens.mute),
                tokens,
            )
            .into_any_element()
        } else {
            match self.page {
                Page::Library => {
                    if self.filtered_library.is_empty() {
                        if self.library.is_empty() {
                            self.empty_state(
                                "Add a music folder to start",
                                "Drop audio into ~/Music, or choose another folder.",
                                icons::music(px(28.0)).text_color(tokens.mute),
                                tokens,
                            )
                            .into_any_element()
                        } else {
                            self.empty_state(
                                "No tracks match your filter",
                                "Try a different search term.",
                                icons::search(px(28.0)).text_color(tokens.mute),
                                tokens,
                            )
                            .into_any_element()
                        }
                    } else {
                        self.track_list(tokens, cx).into_any_element()
                    }
                }
                Page::Artists => {
                    if self.selected_artist.is_some() {
                        self.artist_detail(tokens, cx).into_any_element()
                    } else {
                        self.artists_view(tokens, cx).into_any_element()
                    }
                }
                Page::Albums => self.albums_page(tokens, cx).into_any_element(),
                Page::Favorites => self.favorites_view(tokens, cx).into_any_element(),
                Page::Playlists => self.playlists_view(tokens, cx).into_any_element(),
                Page::Shelves => {
                    if self.selected_shelf.is_some() {
                        self.shelf_detail(tokens, cx).into_any_element()
                    } else {
                        self.shelves_view(tokens, cx).into_any_element()
                    }
                }
            }
        };

        div()
            .id("catalog")
            .accessibility_id("optionmusic.catalog")
            .role(Role::Main)
            .aria_label("Library tracks")
            .flex()
            .flex_col()
            .flex_1()
            .min_w(px(0.0))
            .min_h_0()
            .p(px(24.0))
            .child(header)
            .when(self.search_active, |this| {
                this.child(self.search_bar(tokens, cx))
            })
            .child(body)
    }
}
