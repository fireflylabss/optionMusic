//! Compact mini-player window.
//!
//! Resizing an existing window below `window_min_size` (900x600) is clamped
//! by the platform backend in this GPUI rev, so the toggle instead opens a
//! small `WindowKind::Floating` window (NSPanel at floating level on macOS,
//! i.e. always-on-top) and closes the previous one. Both windows re-host the
//! SAME `Entity<RootView>` — playback, queue, covers and focus state survive
//! the swap untouched. `Render` picks the layout per window from the viewport
//! height, so the two kinds can coexist briefly (or permanently — e.g. a
//! second full window opened via New Window while the mini floats).

use gpui::{
    App, Bounds, Context, MouseButton, Role, TitlebarOptions, Window, WindowBounds,
    WindowDecorations, WindowKind, WindowOptions, div, point, prelude::*, px, size,
};

use crate::ToggleMiniPlayer;
use crate::icons;
use crate::model::*;
use crate::theme::*;
use crate::view::RootView;

const MINI_WIDTH: f32 = 400.0;
const MINI_HEIGHT: f32 = 96.0;
/// The main window's minimum content height is 600px, so any `RootView`
/// window shorter than this is a mini window.
const MINI_WINDOW_MAX_HEIGHT: f32 = 200.0;

pub(crate) fn is_mini_window(window: &Window) -> bool {
    f32::from(window.viewport_size().height) < MINI_WINDOW_MAX_HEIGHT
}

/// `ToggleMiniPlayer` action handler — registered as an app-level action so
/// it fires in either window kind via the bubble phase. The swap is deferred
/// because it cannot run while the dispatching window is being updated.
pub(crate) fn toggle_mini_player(_: &ToggleMiniPlayer, cx: &mut App) {
    cx.defer(toggle_active_window);
}

/// Swaps the active `RootView` window between the full and mini kind.
///
/// Must run outside any window update — a window is popped out of
/// `App::windows` while it updates, so `handle.update`/`handle.entity` fail
/// on it, and `cx.open_window` draws immediately (re-borrowing the entity).
/// All callers reach here through `cx.defer`.
pub(crate) fn toggle_active_window(cx: &mut App) {
    let Some(active) = cx.active_window().or_else(|| cx.windows().first().copied()) else {
        return;
    };
    let Some(handle) = active.downcast::<RootView>() else {
        return;
    };
    let Ok(mini) = handle.update(cx, |_, window, _| is_mini_window(window)) else {
        return;
    };
    let Ok(entity) = handle.entity(cx) else {
        return;
    };
    let focus = entity.read(cx).focus_handle.clone();
    let options = if mini {
        main_window_options(cx)
    } else {
        mini_window_options(cx)
    };
    match cx.open_window(options, move |window, cx| {
        focus.focus(window, cx);
        entity
    }) {
        Ok(_) => {
            // Keep the old window when closing it fails so a usable window always remains.
            if let Err(err) = handle.update(cx, |_, window, _| window.remove_window()) {
                eprintln!("optionmusic: could not close the swapped-out window: {err}");
            }
        }
        Err(err) => eprintln!("optionmusic: could not open the toggled window: {err}"),
    }
}

/// ~400x96 floating strip anchored near the bottom of the primary display
/// (centered when no display info is available).
fn mini_window_options(cx: &mut App) -> WindowOptions {
    let window_size = size(px(MINI_WIDTH), px(MINI_HEIGHT));
    let bounds = cx
        .primary_display()
        .map(|display| {
            let display_bounds = display.visible_bounds();
            Bounds::new(
                point(
                    display_bounds.center().x - window_size.width / 2.0,
                    display_bounds.origin.y + display_bounds.size.height
                        - window_size.height
                        - px(24.0),
                ),
                window_size,
            )
        })
        .unwrap_or_else(|| Bounds::centered(None, window_size, cx));
    WindowOptions {
        focus: true,
        titlebar: Some(TitlebarOptions {
            title: Some("optionMusic".into()),
            #[cfg(target_os = "macos")]
            appears_transparent: true,
            #[cfg(target_os = "macos")]
            traffic_light_position: Some(point(px(9.0), px(9.0))),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        kind: WindowKind::Floating,
        is_movable: true,
        app_owns_titlebar_drag: true,
        is_resizable: false,
        is_minimizable: true,
        window_decorations: Some(WindowDecorations::Client),
        app_id: Some("optionmusic".into()),
        ..Default::default()
    }
}

/// Same chrome as `open_main_window`, for the expand-back path.
fn main_window_options(cx: &mut App) -> WindowOptions {
    let prefs = DesktopPrefs::default();
    let prefs = optionmusic::config::desktop_preferences_raw()
        .and_then(|raw| serde_json::from_str::<DesktopPrefs>(&raw).ok())
        .unwrap_or(prefs);
    let width = prefs.window_w.clamp(900.0, 3840.0);
    let height = prefs.window_h.clamp(600.0, 2160.0);
    let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
    WindowOptions {
        focus: true,
        titlebar: Some(TitlebarOptions {
            title: Some("optionMusic".into()),
            #[cfg(target_os = "macos")]
            appears_transparent: true,
            #[cfg(target_os = "macos")]
            traffic_light_position: Some(point(px(12.0), px(12.0))),
            ..Default::default()
        }),
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        is_movable: true,
        app_owns_titlebar_drag: true,
        window_decorations: Some(WindowDecorations::Client),
        window_min_size: Some(size(px(900.0), px(600.0))),
        app_id: Some("optionmusic".into()),
        ..Default::default()
    }
}

impl RootView {
    /// Compact player layout for windows shorter than `MINI_WINDOW_MAX_HEIGHT`:
    /// a single row (drag rail under the traffic lights, cover, truncating
    /// title/artist, transport, expand button) above a thin seek strip.
    pub(crate) fn mini_shell(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let paused = self.playback.stopped || self.playback.paused;
        let track = self.playback.current.clone();
        let (name, meta) = match &track {
            Some(track) => (track.name.clone(), track_meta(track)),
            None => (
                "Choose a track to start listening".to_string(),
                String::new(),
            ),
        };
        let glyph = track
            .as_ref()
            .map(|track| cover_glyph(&track.name))
            .unwrap_or_else(|| "o".into());
        let progress = self
            .playback
            .duration
            .filter(|d| *d > 0.0)
            .map_or(0.0, |d| (self.playback.position / d).clamp(0.0, 1.0));
        let play_icon = icons::styled(
            if paused {
                icons::play(px(20.0))
            } else {
                icons::pause(px(20.0))
            },
            tokens.black,
            tokens.white,
        );

        div()
            .id("optionmusic-mini")
            .accessibility_id("optionmusic.mini")
            .role(Role::Application)
            .aria_label("optionMusic mini player")
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(Self::play_pause))
            .on_action(cx.listener(Self::next_track))
            .on_action(cx.listener(Self::previous_track))
            .on_action(cx.listener(Self::stop))
            .on_action(cx.listener(Self::volume_up))
            .on_action(cx.listener(Self::volume_down))
            .on_action(cx.listener(Self::mute))
            .on_action(cx.listener(Self::favorite_current))
            .on_action(cx.listener(Self::seek_back))
            .on_action(cx.listener(Self::seek_forward))
            .on_action(cx.listener(Self::blur_list_action))
            .size_full()
            .flex()
            .flex_col()
            .bg(tokens.bg)
            .text_color(tokens.ink)
            .font_family(
                "Inter, system-ui, -apple-system, BlinkMacSystemFont, 'Segoe UI', sans-serif",
            )
            .child(
                div()
                    .id("mini-row")
                    .accessibility_id("optionmusic.mini.row")
                    .role(Role::Group)
                    .aria_label("Now playing")
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .items_center()
                    .pr(px(10.0))
                    .child(
                        // Rail the macOS traffic lights float over; doubles as
                        // the window drag grip.
                        div()
                            .id("mini-rail")
                            .accessibility_id("optionmusic.mini.rail")
                            .h_full()
                            .w(px(66.0))
                            .flex_shrink_0()
                            .border_r_1()
                            .border_color(tokens.border)
                            .cursor_default()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    |_this: &mut RootView,
                                     _event: &gpui::MouseDownEvent,
                                     window: &mut Window,
                                     _cx: &mut Context<RootView>| {
                                        window.start_window_move();
                                    },
                                ),
                            ),
                    )
                    .child(
                        div().px(px(10.0)).flex_shrink_0().child(self.cover_element(
                            "cover-mini",
                            self.current_cover.clone(),
                            glyph,
                            px(48.0),
                            tokens,
                        )),
                    )
                    .child(
                        div()
                            .id("mini-track")
                            .accessibility_id("optionmusic.mini.track")
                            .flex_1()
                            .min_w(px(0.0))
                            .h_full()
                            .flex()
                            .flex_col()
                            .justify_center()
                            .gap(px(2.0))
                            .overflow_hidden()
                            .cursor_default()
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(
                                    |_this: &mut RootView,
                                     _event: &gpui::MouseDownEvent,
                                     window: &mut Window,
                                     _cx: &mut Context<RootView>| {
                                        window.start_window_move();
                                    },
                                ),
                            )
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(tokens.ink)
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(tokens.mute)
                                    .truncate()
                                    .child(meta),
                            ),
                    )
                    .child(self.transport_button(
                        "mini-prev",
                        "Previous",
                        icons::styled(icons::skip_back(px(18.0)), tokens.mute, tokens.white),
                        false,
                        |this: &mut RootView, _window, cx: &mut Context<RootView>| {
                            this.do_previous(cx)
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.primary_button(
                        "mini-play",
                        if paused { "Play" } else { "Pause" },
                        play_icon,
                        |this: &mut RootView, _window, cx: &mut Context<RootView>| {
                            this.do_play_pause(cx)
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.transport_button(
                        "mini-next",
                        "Next",
                        icons::styled(icons::skip_forward(px(18.0)), tokens.mute, tokens.white),
                        false,
                        |this: &mut RootView, _window, cx: &mut Context<RootView>| {
                            this.do_next(cx)
                        },
                        tokens,
                        cx,
                    ))
                    .child(self.icon_button(
                        "mini-expand",
                        "Expand",
                        icons::styled(icons::maximize(px(16.0)), tokens.mute, tokens.ink),
                        |_this: &mut RootView,
                         _window: &mut Window,
                         cx: &mut Context<RootView>| {
                            cx.defer(toggle_active_window)
                        },
                        tokens,
                        cx,
                    )),
            )
            .child(self.slider_track(
                "seek-mini",
                "Seek",
                progress as f32,
                &self.seek_focus,
                tokens,
                cx,
                |this: &mut RootView, fraction: f64, cx: &mut Context<RootView>| {
                    this.do_seek(fraction, cx)
                },
            ))
    }
}
