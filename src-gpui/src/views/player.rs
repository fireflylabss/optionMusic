use std::path::PathBuf;

use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;
use gpui::{Context, Role, SharedString, Window, div, prelude::*, px};

impl RootView {
    pub(crate) fn player_track(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let track = self.playback.current.clone();
        let (name, meta, favorited, cover, id): (
            SharedString,
            SharedString,
            bool,
            Option<PathBuf>,
            Option<String>,
        ) = match &track {
            Some(track) => {
                let favorited = self.playback.favorites.iter().any(|id| id == &track.id);
                (
                    track.name.clone().into(),
                    track_meta(track).into(),
                    favorited,
                    self.current_cover.clone(),
                    Some(track.id.clone()),
                )
            }
            None => (
                "Choose a track to start listening".into(),
                SharedString::default(),
                false,
                None,
                None,
            ),
        };
        let glyph = track
            .as_ref()
            .map(|track| cover_glyph(&track.name))
            .unwrap_or_else(|| "o".into());

        let mut element = div()
            .id("player-track")
            .accessibility_id("optionmusic.player.track")
            .flex()
            .items_center()
            .gap(px(12.0))
            .min_w(px(0.0))
            .flex_1()
            .child(self.cover_element("cover-player", cover, glyph, px(48.0), tokens))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .child(
                        div()
                            .text_size(px(13.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(tokens.ink)
                            .child(name),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .child(meta),
                    ),
            );

        if let Some(id) = id {
            element = element.child(self.icon_button(
                "fav-player",
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
                    this.do_toggle_favorite(&id, cx);
                },
                tokens,
                cx,
            ));
        }

        element
    }

    pub(crate) fn player_center(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("player-center")
            .accessibility_id("optionmusic.player.center")
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.0))
            .min_w(px(0.0))
            .child(self.transport_cluster("player", tokens, cx))
            .child(self.scrub_bar("scrub-player", tokens, cx))
    }

    pub(crate) fn player_volume(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let mute_icon = icons::styled(
            if self.playback.muted {
                icons::volume_x(px(15.0))
            } else {
                icons::volume(px(15.0))
            },
            tokens.mute,
            tokens.ink,
        );

        div()
            .id("player-volume")
            .accessibility_id("optionmusic.player.volume")
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .w(px(168.0))
            .min_w(px(0.0))
            .flex_1()
            .child(self.icon_button(
                "vol-mute",
                "Mute",
                mute_icon,
                |this: &mut RootView, _window, cx: &mut Context<RootView>| this.do_toggle_mute(cx),
                tokens,
                cx,
            ))
            .child(div().w(px(84.0)).child(self.slider_track(
                "volume",
                "Volume",
                self.playback.volume as f32 / 100.0,
                &self.volume_focus,
                tokens,
                cx,
                |this: &mut RootView, fraction: f64, cx: &mut Context<RootView>| {
                    this.do_set_volume(fraction, cx)
                },
            )))
            .child(
                div()
                    .min_w(px(36.0))
                    .text_size(px(12.0))
                    .text_color(tokens.mute)
                    .text_right()
                    .child(format!("{}%", self.playback.volume)),
            )
    }

    pub(crate) fn player_bar(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("player-bar")
            .accessibility_id("optionmusic.player")
            .role(Role::Toolbar)
            .aria_label("Player controls")
            .h(px(78.0))
            .px(px(20.0))
            .border_t_1()
            .border_color(tokens.border)
            .bg(tokens.bg)
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(self.player_track(tokens, cx))
            .child(self.player_center(tokens, cx))
            .child(self.player_volume(tokens, cx))
    }
}
