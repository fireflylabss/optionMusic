use std::path::PathBuf;

use gpui::{
    AnyElement, BoxShadow, Context, Entity, Role, SharedString, Window, anchored, div, prelude::*,
    px,
};
use optionmusic::eq::EqPreset;

use crate::icons;
use crate::model::*;
use crate::search_input::SearchInput;
use crate::theme::*;
use crate::view::RootView;

impl RootView {
    pub(crate) fn context_menu_panel(
        &self,
        menu: ContextMenuState,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let favorited = self
            .playback
            .favorites
            .iter()
            .any(|id| id == &menu.track_id);

        let separator = || div().h(px(1.0)).mx(px(4.0)).my(px(4.0)).bg(tokens.border);

        let mut panel = self.menu_shell("context-menu", "Track actions", 200.0, tokens, cx);

        // Rendered from `menu_items()` so mouse and arrow-key selection
        // always address the same actions.
        for (index, action) in self.menu_items().iter().copied().enumerate() {
            let label: SharedString = match action {
                MenuAction::Play => "Play",
                MenuAction::PlayNext => "Play next",
                MenuAction::QueueAdd => "Add to queue",
                MenuAction::QueueRemove => "Remove from queue",
                MenuAction::Favorite => {
                    if favorited {
                        "Unlike"
                    } else {
                        "Like"
                    }
                }
                MenuAction::AddToPlaylist => "Add to playlist…",
                MenuAction::EditTags => "Edit tags…",
                MenuAction::CopyPath => "Copy path",
                MenuAction::Reveal => "Reveal in folder",
            }
            .into();
            let id: &'static str = match action {
                MenuAction::Play => "play",
                MenuAction::PlayNext => "play-next",
                MenuAction::QueueAdd => "queue-add",
                MenuAction::QueueRemove => "queue-remove",
                MenuAction::Favorite => "favorite",
                MenuAction::AddToPlaylist => "add-to-playlist",
                MenuAction::EditTags => "edit-tags",
                MenuAction::CopyPath => "copy-path",
                MenuAction::Reveal => "reveal",
            };
            // Visual grouping: library actions, then playlist/tag, then file.
            if matches!(action, MenuAction::AddToPlaylist | MenuAction::CopyPath) {
                panel = panel.child(separator());
            }
            let selected = self.menu_selection == index;
            panel = panel.child(self.menu_item(
                id,
                label,
                selected,
                tokens,
                cx,
                move |this, window, cx| this.dispatch_menu_action(action, window, cx),
            ));
        }

        anchored()
            .position(menu.position)
            .snap_to_window()
            .child(self.overlay_enter(panel, "context-menu-enter"))
    }

    pub(crate) fn settings_dialog(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let dirs: Vec<PathBuf> = self
            .controller
            .as_ref()
            .map(|controller| controller.config.music_dirs.clone())
            .unwrap_or_default();

        let mut list = div()
            .id("settings-dirs")
            .flex()
            .flex_col()
            .gap(px(4.0))
            .max_h(px(160.0));
        if dirs.is_empty() {
            list = list.child(
                div()
                    .text_size(px(12.0))
                    .text_color(tokens.mute)
                    .child("Default: ~/Music"),
            );
        }
        for dir in dirs.iter().take(5) {
            list = list.child(
                div()
                    .h(px(22.0))
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(icons::folder(px(12.0)).text_color(tokens.mute))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .text_size(px(11.5))
                            .text_color(tokens.ink_2)
                            .whitespace_nowrap()
                            .text_ellipsis_middle()
                            .overflow_hidden()
                            .child(dir.display().to_string()),
                    ),
            );
        }
        if dirs.len() > 5 {
            list = list.child(
                div()
                    .text_size(px(11.0))
                    .text_color(tokens.faint)
                    .child(format!("+{} more", dirs.len() - 5)),
            );
        }

        let panel = self
            .dialog_shell("settings-dialog", "Settings", 320.0, tokens, cx)
            .child(self.dialog_header(
                "Settings",
                None,
                "settings-close",
                tokens,
                cx,
                |this, _w, cx| this.close_overlays(cx),
            ))
            .child(self.section_label("MUSIC FOLDERS", tokens))
            .child(list)
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .child(self.dialog_button(
                        "settings-add",
                        "Add folder",
                        false,
                        false,
                        true,
                        tokens,
                        cx,
                        |this, _w, cx| this.add_folders(cx),
                    ))
                    .child(self.dialog_button(
                        "settings-rescan",
                        "Rescan",
                        false,
                        false,
                        true,
                        tokens,
                        cx,
                        |this, _w, cx| this.do_rescan(cx),
                    )),
            )
            .child(div().h(px(1.0)).bg(tokens.border))
            .child(self.section_label("SOUND", tokens))
            .child(self.sound_controls(tokens, cx))
            .child(div().h(px(1.0)).bg(tokens.border))
            .child(self.section_label("DISCORD", tokens))
            .child(
                self.sound_row(
                    "discord-rpc",
                    "Rich Presence",
                    if self
                        .controller
                        .as_ref()
                        .is_some_and(|c| c.config.discord_rpc)
                    {
                        "on"
                    } else {
                        "off"
                    },
                    tokens,
                    cx,
                    |this, cx| this.do_toggle_discord_rpc(cx),
                ),
            )
            .child(div().h(px(1.0)).bg(tokens.border))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(tokens.faint)
                    .child(format!("{} · libmpv2", env!("CARGO_PKG_VERSION"))),
            );

        self.dialog_layer(self.overlay_enter(panel, "settings-enter"))
    }

    // ── Sound section (EQ / speed / pitch / gain) ────────────────

    /// Small toggle chip used for EQ presets.
    fn chip(
        &self,
        id: &'static str,
        label: &'static str,
        active: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(label)
            .aria_selected(active)
            .px(px(7.0))
            .py(px(3.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(if active {
                tokens.border_strong
            } else {
                tokens.border
            })
            .when(active, |this| this.bg(tokens.selected))
            .cursor_pointer()
            .text_size(px(10.0))
            .text_color(if active { tokens.ink } else { tokens.mute })
            .hover(|style| {
                style
                    .text_color(tokens.ink)
                    .border_color(tokens.border_strong)
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _w: &mut Window,
                      cx: &mut Context<RootView>| {
                    handler(this, cx);
                },
            ))
            .child(label)
    }

    /// Label on the left, cycling value button on the right.
    fn sound_row(
        &self,
        id: &'static str,
        label: &'static str,
        value: impl Into<SharedString>,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let value: SharedString = value.into();
        div()
            .id(id)
            .role(Role::Button)
            .aria_label(format!("{label}: {value}"))
            .h(px(24.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .text_size(px(11.0))
            .hover(|style| style.bg(tokens.hover))
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _w: &mut Window,
                      cx: &mut Context<RootView>| {
                    handler(this, cx);
                },
            ))
            .child(div().text_color(tokens.mute).child(label))
            .child(div().flex_1())
            .child(div().text_color(tokens.ink).child(value))
    }

    /// Small −/+ button used by `stepper`.
    fn step_button(
        &self,
        bid: String,
        aria: String,
        sign: &'static str,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(bid)
            .role(Role::Button)
            .aria_label(aria)
            .w(px(20.0))
            .h(px(20.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .border_1()
            .border_color(tokens.border)
            .cursor_pointer()
            .text_size(px(11.0))
            .text_color(tokens.mute)
            .hover(|style| {
                style
                    .text_color(tokens.ink)
                    .border_color(tokens.border_strong)
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _w: &mut Window,
                      cx: &mut Context<RootView>| {
                    handler(this, cx);
                },
            ))
            .child(sign)
    }

    /// − value + stepper row for speed / pitch.
    fn stepper(
        &self,
        id: &'static str,
        label: &'static str,
        display: SharedString,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        on_dec: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
        on_inc: impl Fn(&mut RootView, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .h(px(24.0))
            .px(px(6.0))
            .flex()
            .items_center()
            .text_size(px(11.0))
            .child(div().text_color(tokens.mute).child(label))
            .child(div().flex_1())
            .child(self.step_button(
                format!("{id}-dec"),
                format!("{label} down"),
                "−",
                tokens,
                cx,
                on_dec,
            ))
            .child(
                div()
                    .w(px(46.0))
                    .text_color(tokens.ink)
                    .text_align(gpui::TextAlign::Center)
                    .child(display),
            )
            .child(self.step_button(
                format!("{id}-inc"),
                format!("{label} up"),
                "+",
                tokens,
                cx,
                on_inc,
            ))
    }

    fn sound_controls(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let config = self.controller.as_ref().map(|c| c.config.clone());
        let eq_active = self.playback.eq.clone();
        let speed = self.playback.speed;
        let pitch = self.playback.pitch;

        let mut eq_row = div()
            .id("sound-eq")
            .px(px(6.0))
            .pb(px(2.0))
            .flex()
            .items_center()
            .gap(px(4.0))
            .flex_wrap()
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(tokens.mute)
                    .w(px(52.0))
                    .child("EQ"),
            );
        for preset in EqPreset::ALL {
            let active = eq_active == preset.label();
            eq_row = eq_row.child(self.chip(
                match preset {
                    EqPreset::Off => "eq-off",
                    EqPreset::Bass => "eq-bass",
                    EqPreset::Treble => "eq-treble",
                    EqPreset::Rock => "eq-rock",
                    EqPreset::Vocal => "eq-vocal",
                    EqPreset::Lofi => "eq-lofi",
                },
                preset.label(),
                active,
                tokens,
                cx,
                move |this, cx| this.do_set_eq(preset, cx),
            ));
        }

        let mut controls = div()
            .id("sound-controls")
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(eq_row)
            .child(self.stepper(
                "sound-speed",
                "Speed",
                format!("{speed:.2}×").into(),
                tokens,
                cx,
                |this, cx| this.do_nudge_speed(-0.05, cx),
                |this, cx| this.do_nudge_speed(0.05, cx),
            ))
            .child(self.stepper(
                "sound-pitch",
                "Pitch",
                if pitch == 0.0 {
                    "0".into()
                } else {
                    format!("{pitch:+.0}").into()
                },
                tokens,
                cx,
                |this, cx| this.do_nudge_pitch(-1.0, cx),
                |this, cx| this.do_nudge_pitch(1.0, cx),
            ));

        if (speed - 1.0).abs() > f64::EPSILON || pitch.abs() > f64::EPSILON {
            controls = controls.child(div().px(px(6.0)).child(self.chip(
                "sound-reset",
                "Reset speed & pitch",
                false,
                tokens,
                cx,
                |this, cx| this.do_reset_speed_pitch(cx),
            )));
        }

        if let Some(config) = config {
            controls = controls
                .child(self.sound_row(
                    "sound-replaygain",
                    "ReplayGain",
                    config.replaygain.label(),
                    tokens,
                    cx,
                    |this, cx| this.do_cycle_replaygain(cx),
                ))
                .child(self.sound_row(
                    "sound-excess",
                    "Volume boost",
                    if config.excess_volume { "on" } else { "off" },
                    tokens,
                    cx,
                    |this, cx| this.do_toggle_excess_volume(cx),
                ))
                .child(self.sound_row(
                    "sound-ldm",
                    "Normalize",
                    if config.ldm { "on" } else { "off" },
                    tokens,
                    cx,
                    |this, cx| this.do_toggle_ldm(cx),
                ))
                .child(self.sound_row(
                    "sound-artist-src",
                    "Artists by",
                    config.artist_source.label(),
                    tokens,
                    cx,
                    |this, cx| this.do_cycle_artist_source(cx),
                ));
        }

        controls
    }

    pub(crate) fn status_toast(&self, tokens: MusicTokens, cx: &mut Context<Self>) -> AnyElement {
        let toast = div()
            .id("status-toast")
            .accessibility_id("optionmusic.status")
            .role(Role::Status)
            .aria_label("Status")
            .px(px(14.0))
            .py(px(9.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(tokens.border_strong)
            .bg(tokens.panel)
            .shadow(vec![
                BoxShadow::new(px(0.), px(8.), tokens.black.opacity(0.5).into())
                    .blur_radius(px(24.)),
            ])
            .text_size(px(12.0))
            .text_color(tokens.ink)
            .cursor_pointer()
            .on_click(cx.listener(
                |this: &mut RootView,
                 _: &gpui::ClickEvent,
                 _w: &mut Window,
                 cx: &mut Context<RootView>| {
                    this.status = "".into();
                    this.status_generation += 1;
                    cx.notify();
                    cx.stop_propagation();
                },
            ))
            .child(self.status.clone());
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_end()
            .justify_center()
            .pb(px(100.0))
            .child(self.overlay_enter(toast, "toast-enter"))
            .into_any_element()
    }

    // ── Playlist picker (anchored, keyboard navigable) ───────────

    pub(crate) fn playlist_picker_panel(
        &self,
        picker: PlaylistPickerState,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let mut rows = div()
            .id("picker-list")
            .flex()
            .flex_col()
            .max_h(px(260.0))
            .overflow_y_scroll()
            .track_scroll(&self.picker_scroll);
        for (index, playlist) in self.playlists.iter().enumerate() {
            let selected = picker.selection == index;
            let pid = playlist.id.clone();
            let track_id = picker.track_id.clone();
            rows = rows.child(
                div()
                    .id(format!("picker-{index}"))
                    .role(Role::MenuItem)
                    .aria_label(playlist.name.clone())
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(if selected { tokens.ink } else { tokens.ink_2 })
                    .when(selected, |this| this.bg(tokens.selected))
                    .hover(|style| style.bg(tokens.selected).text_color(tokens.ink))
                    .on_click(cx.listener(
                        move |this: &mut RootView,
                              _: &gpui::ClickEvent,
                              _w: &mut Window,
                              cx: &mut Context<RootView>| {
                            this.playlist_picker = None;
                            if let Some(controller) = this.controller.as_ref() {
                                match controller.playlist_add(&pid, &track_id) {
                                    Ok(_) => {
                                        this.refresh_playlists();
                                        this.set_status("Added to playlist", cx);
                                    }
                                    Err(error) => {
                                        this.set_status(format!("Add failed: {error}"), cx)
                                    }
                                }
                            }
                            cx.stop_propagation();
                        },
                    ))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(playlist.name.clone()),
                    ),
            );
        }
        let new_selected = picker.selection == self.playlists.len();
        let list = self
            .menu_shell("playlist-picker", "Add to playlist", 220.0, tokens, cx)
            .child(
                div()
                    .px(px(9.0))
                    .pt(px(4.0))
                    .pb(px(2.0))
                    .child(self.section_label("ADD TO PLAYLIST", tokens)),
            )
            .child(rows)
            .child(div().h(px(1.0)).mx(px(4.0)).my(px(4.0)).bg(tokens.border))
            .child(
                div()
                    .id("picker-new")
                    .role(Role::MenuItem)
                    .aria_label("New playlist")
                    .h(px(28.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(px(12.0))
                    .text_color(if new_selected {
                        tokens.ink
                    } else {
                        tokens.ink_2
                    })
                    .when(new_selected, |this| this.bg(tokens.selected))
                    .hover(|style| style.bg(tokens.selected).text_color(tokens.ink))
                    .on_click(cx.listener(
                        move |this: &mut RootView,
                              _: &gpui::ClickEvent,
                              window: &mut Window,
                              cx: &mut Context<RootView>| {
                            let track_id = picker.track_id.clone();
                            this.playlist_picker = None;
                            this.open_name_prompt(
                                NameTarget::NewPlaylistForTrack(track_id),
                                window,
                                cx,
                            );
                            cx.stop_propagation();
                        },
                    ))
                    .child(icons::plus(px(12.0)).text_color(tokens.mute))
                    .child("New playlist…"),
            );

        anchored()
            .position(picker.position)
            .snap_to_window()
            .child(self.overlay_enter(list, "picker-enter"))
    }

    // ── Shared single-line name prompt ───────────────────────────

    pub(crate) fn name_prompt_panel(
        &self,
        target: NameTarget,
        window: &Window,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let title: SharedString = match &target {
            NameTarget::NewPlaylist => "New playlist".into(),
            NameTarget::NewPlaylistForTrack(_) => "New playlist for track".into(),
            NameTarget::RenamePlaylist(_) => "Rename playlist".into(),
        };
        let action_label: &'static str = match &target {
            NameTarget::NewPlaylist | NameTarget::NewPlaylistForTrack(_) => "Create",
            NameTarget::RenamePlaylist(_) => "Save",
        };
        let enabled = !self.name_input.read(cx).content.trim().is_empty();
        let panel = self
            .dialog_shell("name-prompt", title.clone(), 340.0, tokens, cx)
            .child(
                self.dialog_header(title, None, "name-close", tokens, cx, |this, _w, cx| {
                    this.on_name_dismiss(cx)
                }),
            )
            .child(self.input_shell(
                "name-input-shell",
                &self.name_input,
                self.name_input.read(cx).focus_handle.is_focused(window),
                tokens,
            ))
            .child(self.hint_row(&[("↵", "confirm"), ("esc", "cancel")], tokens))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .justify_end()
                    .child(self.dialog_button(
                        "name-cancel",
                        "Cancel",
                        false,
                        false,
                        true,
                        tokens,
                        cx,
                        |this, _w, cx| this.on_name_dismiss(cx),
                    ))
                    .child(self.dialog_button(
                        "name-confirm",
                        action_label,
                        true,
                        false,
                        enabled,
                        tokens,
                        cx,
                        |this, _w, cx| this.on_name_submit(cx),
                    )),
            );
        self.dialog_layer(self.overlay_enter(panel, "name-prompt-enter"))
    }

    // ── Confirm dialog ───────────────────────────────────────────

    pub(crate) fn confirm_dialog(
        &self,
        request: ConfirmRequest,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let panel = self
            .dialog_shell("confirm-dialog", request.title.clone(), 360.0, tokens, cx)
            .on_action(cx.listener(Self::menu_enter))
            .on_action(cx.listener(Self::menu_left))
            .on_action(cx.listener(Self::menu_right))
            .child(self.dialog_header(
                request.title.clone(),
                None,
                "confirm-close",
                tokens,
                cx,
                |this, w, cx| this.confirm_cancel(w, cx),
            ))
            .child(
                div()
                    .text_size(px(12.0))
                    .text_color(tokens.mute)
                    .line_height(px(18.0))
                    .child(request.detail.clone()),
            )
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .justify_end()
                    .child(self.dialog_button(
                        "confirm-cancel",
                        "Cancel",
                        false,
                        !self.confirm_accept_selected,
                        true,
                        tokens,
                        cx,
                        |this, w, cx| this.confirm_cancel(w, cx),
                    ))
                    .child(self.dialog_button(
                        "confirm-accept",
                        request.confirm_label.clone(),
                        true,
                        self.confirm_accept_selected,
                        true,
                        tokens,
                        cx,
                        |this, w, cx| this.confirm_accept(w, cx),
                    )),
            )
            .child(self.hint_row(
                &[("← →", "choose"), ("↵", "confirm"), ("esc", "cancel")],
                tokens,
            ));
        self.dialog_layer(self.overlay_enter(panel, "confirm-enter"))
    }

    // ── Tag editor dialog ────────────────────────────────────────

    pub(crate) fn tag_editor_dialog(
        &self,
        window: &Window,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let Some(editor) = self.tag_editor.as_ref() else {
            return div().into_any_element();
        };
        let subtitle = self
            .track_by_id(&editor.track_id)
            .map(|track| SharedString::from(track.name.clone()));
        let field = |field_id: &'static str, label: &'static str, input: &Entity<SearchInput>| {
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(64.0))
                        .text_size(px(11.0))
                        .text_color(tokens.mute)
                        .child(label),
                )
                .child(
                    self.input_shell(
                        field_id,
                        input,
                        input.read(cx).focus_handle.is_focused(window),
                        tokens,
                    )
                    .flex_1(),
                )
        };
        // `field` reads through `cx` — build the rows before the panel so its
        // shared borrow ends before `cx` is borrowed mutably below.
        let fields = [
            field("tagf-title", "Title", &editor.title),
            field("tagf-artist", "Artist", &editor.artist),
            field("tagf-album", "Album", &editor.album),
            field("tagf-track", "Track #", &editor.track_number),
            field("tagf-disc", "Disc #", &editor.disc_number),
            field("tagf-year", "Year", &editor.year),
        ];
        let panel = self
            .dialog_shell("tag-editor", "Edit tags", 420.0, tokens, cx)
            .child(self.dialog_header(
                "Edit tags",
                subtitle,
                "tag-close",
                tokens,
                cx,
                |this, _w, cx| this.close_tag_editor(cx),
            ))
            .children(fields)
            .child(self.hint_row(
                &[("tab", "next field"), ("↵", "save"), ("esc", "cancel")],
                tokens,
            ))
            .child(
                div()
                    .flex()
                    .gap(px(8.0))
                    .justify_end()
                    .child(self.dialog_button(
                        "tag-cancel",
                        "Cancel",
                        false,
                        false,
                        true,
                        tokens,
                        cx,
                        |this, _w, cx| this.close_tag_editor(cx),
                    ))
                    .child(self.dialog_button(
                        "tag-save",
                        "Save",
                        true,
                        false,
                        true,
                        tokens,
                        cx,
                        |this, _w, cx| this.save_tag_editor(cx),
                    )),
            );
        self.dialog_layer(self.overlay_enter(panel, "tag-editor-enter"))
            .into_any_element()
    }
}
