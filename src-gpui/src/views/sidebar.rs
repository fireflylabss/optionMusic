use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, MouseButton, MouseDownEvent, Role, SharedString,
    Svg, Window, bounce, div, ease_in_out, prelude::*, px, rgb,
};
use optionmusic::controller::TrackDto;

use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;

fn shortcut_hint(index: usize) -> SharedString {
    if cfg!(target_os = "macos") {
        format!("⌘{index}").into()
    } else {
        format!("ctrl+{index}").into()
    }
}

/// Equalizer bars shown over the current track's cover in the up-next list.
/// Bars loop on a shared clock with staggered periods so they drift out of
/// phase; GPUI freezes them at the resting pose when reduced motion is on.
fn playing_indicator(track_id: &str, playing: bool, tokens: MusicTokens) -> AnyElement {
    const DURATIONS: [u64; 3] = [640, 920, 1140];
    const RESTING: [f32; 3] = [5.0, 9.0, 6.5];

    let mut bars = div().flex().items_end().gap(px(2.5)).h(px(14.0));
    for (i, duration) in DURATIONS.iter().enumerate() {
        let bar = div().w(px(3.0)).rounded(px(1.5)).bg(tokens.ink);
        let bar = if playing {
            bar.with_animation(
                format!("eq-{track_id}-{i}"),
                Animation::new(Duration::from_millis(*duration))
                    .repeat_synced()
                    .with_easing(bounce(ease_in_out)),
                move |el, delta| el.h(px(4.0 + delta * 9.0)),
            )
            .into_any_element()
        } else {
            bar.h(px(RESTING[i])).into_any_element()
        };
        bars = bars.child(bar);
    }

    div()
        .absolute()
        .inset_0()
        .rounded(px(8.0))
        .bg(tokens.black.opacity(0.55))
        .flex()
        .items_center()
        .justify_center()
        .child(bars)
        .into_any_element()
}

impl RootView {
    pub(crate) fn side_link(
        &self,
        page: Page,
        label: impl Into<SharedString>,
        icon: Svg,
        count: Option<usize>,
        hint: SharedString,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let active = self.page == page;
        let label: SharedString = label.into();
        let group: SharedString = format!("nav-hover-{label}").into();
        let icon = icon
            .text_color(if active { tokens.ink } else { tokens.ink_2 })
            .hover(|style| style.text_color(tokens.ink));
        let count_text: SharedString = count.map(|n| n.to_string()).unwrap_or_default().into();

        div()
            .id(format!("nav-{label}"))
            .accessibility_id(format!("optionmusic.nav.{label}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .group(group.clone())
            .relative()
            .h(px(36.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .gap(px(10.0))
            .text_color(if active { tokens.ink } else { tokens.ink_2 })
            .when(active, |this| this.bg(tokens.selected))
            .when(!active, |this| {
                this.hover(|style| style.bg(tokens.hover).text_color(tokens.ink))
            })
            .active(|style| style.opacity(0.8))
            .focus_visible(|style| {
                style
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.navigate(page, window, cx);
                },
            ))
            .when(active, |this| {
                this.child(
                    div()
                        .absolute()
                        .left(px(-8.0))
                        .top(px(8.0))
                        .w(px(3.0))
                        .h(px(20.0))
                        .rounded(px(1.5))
                        .bg(tokens.ink),
                )
            })
            .child(
                div()
                    .w(px(20.0))
                    .h(px(20.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .truncate()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(label),
            )
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(tokens.faint)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .group_hover(group.clone(), |style| style.opacity(0.0))
                    .child(count_text),
            )
            .child(
                div()
                    .absolute()
                    .right(px(10.0))
                    .opacity(0.0)
                    .text_size(px(10.0))
                    .text_color(tokens.mute)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .group_hover(group, |style| style.opacity(1.0))
                    .child(hint),
            )
    }

    pub(crate) fn side_track(
        &self,
        track: &TrackDto,
        playing: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let id = track.id.clone();
        let current = self
            .playback
            .current
            .as_ref()
            .is_some_and(|current| current.id == id);

        div()
            .id(format!("next-{id}"))
            .accessibility_id(format!("optionmusic.next.{id}"))
            .role(Role::Button)
            .aria_label(track.name.clone())
            .h(px(48.0))
            .px(px(8.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .gap(px(10.0))
            .when(current, |this| this.bg(tokens.selected))
            .when(!current, |this| this.hover(|style| style.bg(tokens.hover)))
            .active(|style| style.opacity(0.8))
            .focus_visible(|style| {
                style
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.play_track(&id, cx);
                },
            ))
            .child(
                div()
                    .relative()
                    .size(px(36.0))
                    .flex_none()
                    .child(self.cover_element(
                        format!("cover-next-{}", track.id),
                        None,
                        cover_glyph(&track.name),
                        px(36.0),
                        tokens,
                    ))
                    .when(current, |this| {
                        this.child(playing_indicator(&track.id, playing, tokens))
                    }),
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
                            .truncate()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(tokens.ink)
                            .child(track.name.clone()),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(tokens.mute)
                            .child(track_meta(track)),
                    ),
            )
    }

    pub(crate) fn side_action(
        &self,
        label: impl Into<SharedString>,
        icon: Svg,
        active: bool,
        hint: Option<SharedString>,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        let icon = icon
            .text_color(if active { tokens.ink } else { tokens.mute })
            .hover(|style| style.text_color(tokens.ink));
        div()
            .id(format!("side-action-{label}"))
            .accessibility_id(format!("optionmusic.side.action.{label}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .h(px(32.0))
            .w_full()
            .px(px(10.0))
            .rounded(px(8.0))
            .text_color(if active { tokens.ink } else { tokens.mute })
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .flex()
            .items_center()
            .cursor_pointer()
            .gap(px(10.0))
            .when(active, |this| this.bg(tokens.selected))
            .when(!active, |this| {
                this.hover(|style| style.bg(tokens.hover).text_color(tokens.ink))
            })
            .active(|style| style.opacity(0.8))
            .focus_visible(|style| {
                style
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    handler(this, window, cx);
                },
            ))
            .child(
                div()
                    .w(px(18.0))
                    .h(px(18.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(icon),
            )
            .child(div().flex_1().min_w(px(0.0)).truncate().child(label))
            .when_some(hint, |this, hint| {
                this.child(
                    div()
                        .text_size(px(10.0))
                        .text_color(tokens.faint)
                        .font_weight(gpui::FontWeight::MEDIUM)
                        .child(hint),
                )
            })
    }

    pub(crate) fn titlebar(
        &self,
        window: &mut Window,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let maximized = window.is_maximized();
        let maximize_icon = if maximized {
            icons::restore(px(16.0))
        } else {
            icons::maximize(px(16.0))
        };

        let title_area = div()
            .id("titlebar-title")
            .accessibility_id("optionmusic.titlebar.title")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .gap(px(10.0))
            // Clear the native traffic lights on macOS.
            .px(if cfg!(target_os = "macos") {
                px(80.0)
            } else {
                px(16.0)
            })
            .cursor_default()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |_this: &mut RootView,
                          event: &MouseDownEvent,
                          window: &mut Window,
                          _cx: &mut Context<RootView>| {
                        if event.click_count == 2 {
                            window.zoom_window();
                        } else {
                            window.start_window_move();
                        }
                    },
                ),
            )
            .child(
                div()
                    .size(px(24.0))
                    .rounded(px(6.0))
                    .bg(tokens.white)
                    .text_color(tokens.black)
                    .flex()
                    .items_center()
                    .justify_center()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .child("o"),
            )
            .child(
                div()
                    .text_size(px(14.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(tokens.ink)
                    .child("optionMusic"),
            );

        let controls = div()
            .id("titlebar-controls")
            .accessibility_id("optionmusic.titlebar.controls")
            .role(Role::Group)
            .aria_label("Window controls")
            .flex()
            .items_center()
            .gap(px(4.0))
            .pr(px(10.0))
            .child(self.icon_button(
                "win-min",
                "Minimize",
                icons::styled(icons::minus(px(16.0)), tokens.mute, tokens.ink),
                |_this, window, _cx| window.minimize_window(),
                tokens,
                cx,
            ))
            .child(self.icon_button(
                "win-max",
                "Maximize",
                icons::styled(maximize_icon, tokens.mute, tokens.ink),
                |_this, window, _cx| window.zoom_window(),
                tokens,
                cx,
            ))
            .child(self.icon_button(
                "win-close",
                "Close",
                icons::styled(icons::close(px(16.0)), tokens.mute, rgb(0xff5f57)),
                |_this, window, _cx| window.remove_window(),
                tokens,
                cx,
            ));

        div()
            .id("titlebar")
            .accessibility_id("optionmusic.titlebar")
            .w_full()
            .h(px(40.0))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(tokens.border)
            .bg(tokens.panel)
            .child(title_area)
            // macOS uses native traffic lights (transparent titlebar); the
            // custom min/max/close buttons are for other platforms only.
            .when(!cfg!(target_os = "macos"), |this| this.child(controls))
    }

    pub(crate) fn sidebar(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let tracks_count = self.library.len();
        let favorites_count = self.playback.favorites.len();
        let artists_count = self.artists_index.len();
        let albums_count = self.albums_index.len();
        let playlists_count = self.playlists.len();
        let playing =
            self.playback.current.is_some() && !self.playback.paused && !self.playback.stopped;

        let stats = format!(
            "{} · {}",
            if tracks_count == 1 {
                "1 song".to_string()
            } else {
                format!("{tracks_count} songs")
            },
            if self.folders_count_cache == 1 {
                "1 folder".to_string()
            } else {
                format!("{} folders", self.folders_count_cache)
            },
        );

        let up_next: Vec<&TrackDto> = self
            .playback
            .queue
            .iter()
            .filter_map(|id| self.track_by_id(id))
            .take(6)
            .collect();

        let next_list = if up_next.is_empty() {
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(10.0))
                .child(icons::music(px(22.0)).text_color(tokens.lift_2))
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .items_center()
                        .gap(px(2.0))
                        .child(
                            div()
                                .text_size(px(11.0))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(tokens.mute)
                                .child("Nothing up next"),
                        )
                        .child(
                            div()
                                .text_size(px(10.0))
                                .text_color(tokens.faint)
                                .child("Queued tracks show up here"),
                        ),
                )
                .into_any_element()
        } else {
            div()
                .flex()
                .flex_col()
                .gap(px(2.0))
                .children(
                    up_next
                        .into_iter()
                        .map(|track| self.side_track(track, playing, tokens, cx)),
                )
                .into_any_element()
        };

        div()
            .id("sidebar")
            .accessibility_id("optionmusic.sidebar")
            .role(Role::Complementary)
            .aria_label("Sidebar")
            .w(px(248.0))
            .h_full()
            .flex()
            .flex_col()
            .border_r_1()
            .border_color(tokens.border)
            .bg(tokens.panel)
            .child(
                div()
                    .id("side-nav")
                    .accessibility_id("optionmusic.side.nav")
                    .role(Role::Navigation)
                    .aria_label("Library views")
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(10.0))
                    .pt(px(12.0))
                    .pb(px(8.0))
                    .child(self.side_link(
                        Page::Library,
                        "Library",
                        icons::folder(px(18.0)),
                        Some(tracks_count),
                        shortcut_hint(1),
                        tokens,
                        cx,
                    ))
                    .child(self.side_link(
                        Page::Artists,
                        "Artists",
                        icons::user(px(18.0)),
                        Some(artists_count),
                        shortcut_hint(2),
                        tokens,
                        cx,
                    ))
                    .child(self.side_link(
                        Page::Albums,
                        "Albums",
                        icons::disc(px(18.0)),
                        Some(albums_count),
                        shortcut_hint(3),
                        tokens,
                        cx,
                    ))
                    .child(self.side_link(
                        Page::Playlists,
                        "Playlists",
                        icons::list(px(18.0)),
                        Some(playlists_count),
                        shortcut_hint(4),
                        tokens,
                        cx,
                    ))
                    .child(self.side_link(
                        Page::Shelves,
                        "Shelves",
                        icons::grid(px(18.0)),
                        None,
                        shortcut_hint(6),
                        tokens,
                        cx,
                    ))
                    .child(self.side_link(
                        Page::Favorites,
                        "Favorites",
                        icons::heart(px(18.0)),
                        Some(favorites_count),
                        shortcut_hint(5),
                        tokens,
                        cx,
                    )),
            )
            .child(div().mx(px(10.0)).h(px(1.0)).bg(tokens.border))
            .child(
                div()
                    .id("side-next")
                    .accessibility_id("optionmusic.side.next")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px(px(10.0))
                    .pt(px(12.0))
                    .pb(px(10.0))
                    .child(
                        div()
                            .h(px(20.0))
                            .px(px(8.0))
                            .mb(px(4.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(self.section_label("UP NEXT", tokens))
                            .when(!self.playback.queue.is_empty(), |this| {
                                this.child(
                                    div()
                                        .text_size(px(10.0))
                                        .font_weight(gpui::FontWeight::MEDIUM)
                                        .text_color(tokens.mute)
                                        .child(format!("{}", self.playback.queue.len())),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .overflow_hidden()
                            .child(next_list),
                    ),
            )
            .child(
                div()
                    .id("side-actions")
                    .accessibility_id("optionmusic.side.actions")
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(10.0))
                    .pt(px(8.0))
                    .pb(px(10.0))
                    .border_t_1()
                    .border_color(tokens.border)
                    .child(
                        self.side_action(
                            "Search",
                            icons::search(px(15.0)),
                            self.search_active,
                            Some(
                                if cfg!(target_os = "macos") {
                                    "⌘F"
                                } else {
                                    "ctrl+f"
                                }
                                .into(),
                            ),
                            tokens,
                            cx,
                            |this, window, cx| {
                                if !this.search_active {
                                    this.do_toggle_search(window, cx);
                                }
                            },
                        ),
                    )
                    .child(self.side_action(
                        "Add folder",
                        icons::folder(px(15.0)),
                        false,
                        None,
                        tokens,
                        cx,
                        |this, _window, cx| this.add_folders(cx),
                    ))
                    .child(self.side_action(
                        "Settings",
                        icons::settings(px(15.0)),
                        self.settings_open,
                        None,
                        tokens,
                        cx,
                        |this, window, cx| {
                            this.settings_open = !this.settings_open;
                            this.context_menu = None;
                            if this.settings_open {
                                this.overlay_focus.focus(window, cx);
                            }
                            cx.notify();
                        },
                    ))
                    .child(
                        div()
                            .px(px(10.0))
                            .pt(px(8.0))
                            .text_size(px(10.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(tokens.faint)
                            .child(stats),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, px, size};

    use crate::NavArtists;
    use crate::model::Page;
    use crate::view::RootView;

    /// Render smoke test: the whole sidebar tree (nav, accent indicator,
    /// up-next rows incl. the looping equalizer, footer) must lay out and
    /// paint without panicking, and nav actions must switch pages.
    #[gpui::test]
    fn sidebar_renders_and_navigates(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(1200.), px(760.)), |window, cx| {
            RootView::new(window, cx)
        });
        cx.run_until_parked();

        window
            .update(cx, |view, _window, cx| {
                assert!(view.page == Page::Library);

                // Seed playback so UP NEXT rows render, with the first row
                // marked as now-playing to exercise the equalizer overlay.
                if view.library.len() >= 2 {
                    let first = view.library[0].clone();
                    let second = view.library[1].clone();
                    view.playback.queue = vec![first.id.clone(), second.id.clone()];
                    view.playback.current = Some(first);
                    view.playback.paused = false;
                    view.playback.stopped = false;
                }
                cx.notify();
            })
            .unwrap();

        // Drive a few frames so the sidebar paints its live state (the
        // looping equalizer requests a frame each paint).
        for _ in 0..4 {
            window
                .update(cx, |_view, window, cx| window.simulate_next_frame(cx))
                .unwrap();
        }
        cx.run_until_parked();

        // dispatch_action defers to the end of the effect cycle, so the
        // executor must be pumped before the page change is observable.
        window
            .update(cx, |_view, window, cx| {
                window.dispatch_action(Box::new(NavArtists), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, _window, _cx| {
                assert!(view.page == Page::Artists);
            })
            .unwrap();
    }
}
