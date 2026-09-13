use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Bounds, BoxShadow, Context, ElementId, Empty, Entity,
    MouseButton, MouseDownEvent, ObjectFit, Role, SharedString, UniformListScrollHandle, Window,
    canvas, div, ease_out_quint, img, linear_color_stop, linear_gradient, point, prelude::*, px,
    relative,
};
use optionmusic::controller::LoopMode;

use crate::icons;
use crate::model::*;
use crate::search_input::SearchInput;
use crate::theme::*;
use crate::view::RootView;

impl RootView {
    pub(crate) fn cover_element(
        &self,
        id: impl Into<ElementId>,
        cover: Option<PathBuf>,
        glyph: impl Into<SharedString>,
        size: gpui::Pixels,
        tokens: MusicTokens,
    ) -> gpui::Stateful<gpui::Div> {
        let glyph: SharedString = glyph.into();
        let size_f32 = f32::from(size);
        let cover_background = linear_gradient(
            160.0,
            linear_color_stop(tokens.lift, 0.0),
            linear_color_stop(tokens.lift_2, 1.0),
        );

        let mut element = div()
            .id(id)
            .size(size)
            .rounded(px(8.0))
            .overflow_hidden()
            .border_1()
            .border_color(tokens.border_strong)
            .bg(cover_background)
            .when(size_f32 >= 96.0, |this| this.shadow(cover_shadow(tokens)))
            .hover(|style| style.border_color(tokens.white.opacity(0.18)));

        if let Some(path) = cover {
            let fallback_glyph = glyph.clone();
            let fallback_size = size;
            let fallback_tokens = tokens;
            element = element.child(
                img(path)
                    .object_fit(ObjectFit::Cover)
                    .size(size)
                    .with_fallback(move || {
                        div()
                            .size(fallback_size)
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_size(px(size_f32 * 0.45))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(fallback_tokens.ink_2)
                            .child(fallback_glyph.clone())
                            .into_any_element()
                    }),
            );
        } else {
            element = element
                .flex()
                .items_center()
                .justify_center()
                .text_size(px(size_f32 * 0.45))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(tokens.ink_2)
                .child(glyph.clone());
        }
        element
    }

    pub(crate) fn icon_button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        child: impl IntoElement,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        div()
            .id(id)
            .accessibility_id(format!("optionmusic.icon.{label}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .size(px(32.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(tokens.mute)
            .hover(|style| style.bg(tokens.lift_2).text_color(tokens.ink))
            .active(|style| style.opacity(0.75))
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
                    cx.stop_propagation();
                },
            ))
            .child(child)
    }

    pub(crate) fn transport_button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        child: impl IntoElement,
        active: bool,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        div()
            .id(id)
            .accessibility_id(format!("optionmusic.transport.{label}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .size(px(36.0))
            .rounded(px(100.0))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .text_color(if active { tokens.white } else { tokens.mute })
            .when(active, |this| this.bg(tokens.lift_2))
            .when(!active, |this| {
                this.hover(|style| style.text_color(tokens.white))
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
            .child(child)
    }

    pub(crate) fn primary_button(
        &self,
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        child: impl IntoElement,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        div()
            .id(id)
            .accessibility_id(format!("optionmusic.primary.{label}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .size(px(46.0))
            .rounded(px(100.0))
            .bg(tokens.white)
            .text_color(tokens.black)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .hover(|style| style.bg(tokens.ink))
            .active(|style| style.opacity(0.85))
            .focus_visible(|style| {
                style
                    .border_color(tokens.white.opacity(0.5))
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
            .child(child)
    }

    /// Shared transport row: shuffle, previous, play/pause, next, loop.
    /// `prefix` namespaces element ids so the player bar and the stage can
    /// both render a cluster in the same frame.
    pub(crate) fn transport_cluster(
        &self,
        prefix: &'static str,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let paused = self.playback.stopped || self.playback.paused;
        let play_icon = icons::styled(
            if paused {
                icons::play(px(20.0))
            } else {
                icons::pause(px(20.0))
            },
            tokens.black,
            tokens.white,
        );
        let loop_active = self.playback.loop_mode != LoopMode::Off;

        div()
            .id(format!("{prefix}-transport"))
            .accessibility_id(format!("optionmusic.{prefix}.transport"))
            .role(Role::Group)
            .aria_label("Transport")
            .flex()
            .items_center()
            .justify_center()
            .gap(px(4.0))
            .child(self.transport_button(
                format!("{prefix}-shuffle"),
                "Shuffle",
                icons::styled(
                    icons::shuffle(px(18.0)),
                    if self.playback.shuffled {
                        tokens.white
                    } else {
                        tokens.mute
                    },
                    tokens.white,
                ),
                self.playback.shuffled,
                |this: &mut RootView, _window: &mut Window, cx: &mut Context<RootView>| {
                    this.do_shuffle(cx)
                },
                tokens,
                cx,
            ))
            .child(self.transport_button(
                format!("{prefix}-previous"),
                "Previous",
                icons::styled(icons::skip_back(px(18.0)), tokens.mute, tokens.white),
                false,
                |this: &mut RootView, _window: &mut Window, cx: &mut Context<RootView>| {
                    this.do_previous(cx)
                },
                tokens,
                cx,
            ))
            .child(self.primary_button(
                format!("{prefix}-play"),
                if paused { "Play" } else { "Pause" },
                play_icon,
                |this: &mut RootView, _window: &mut Window, cx: &mut Context<RootView>| {
                    this.do_play_pause(cx)
                },
                tokens,
                cx,
            ))
            .child(self.transport_button(
                format!("{prefix}-next"),
                "Next",
                icons::styled(icons::skip_forward(px(18.0)), tokens.mute, tokens.white),
                false,
                |this: &mut RootView, _window: &mut Window, cx: &mut Context<RootView>| {
                    this.do_next(cx)
                },
                tokens,
                cx,
            ))
            .child(self.transport_button(
                format!("{prefix}-loop"),
                "Loop",
                icons::styled(
                    if self.playback.loop_mode == LoopMode::Track {
                        icons::repeat_1(px(18.0))
                    } else {
                        icons::repeat(px(18.0))
                    },
                    if loop_active {
                        tokens.white
                    } else {
                        tokens.mute
                    },
                    tokens.white,
                ),
                loop_active,
                |this: &mut RootView, _window: &mut Window, cx: &mut Context<RootView>| {
                    this.do_cycle_loop(cx)
                },
                tokens,
                cx,
            ))
    }

    pub(crate) fn stage_action(
        &self,
        label: impl Into<SharedString>,
        child: impl IntoElement,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        div()
            .id(format!("stage-action-{label}"))
            .accessibility_id(format!("optionmusic.stage.action.{label}"))
            .role(Role::Button)
            .aria_label(label.clone())
            .h(px(34.0))
            .flex_1()
            .rounded(px(8.0))
            .border_1()
            .border_color(tokens.border_strong)
            .bg(tokens.panel)
            .text_color(tokens.mute)
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .gap(px(7.0))
            .hover(|style| style.bg(tokens.lift_2).text_color(tokens.ink))
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
                    cx.stop_propagation();
                },
            ))
            .child(child)
    }

    /// Interactive slider track: hit area, bounds capture, click + drag, and
    /// keyboard nudge via the supplied focus handle (Slider key context).
    /// `on_change` receives a 0.0–1.0 fraction.
    pub(crate) fn slider_track(
        &self,
        id: &'static str,
        label: &'static str,
        fraction: f32,
        focus: &gpui::FocusHandle,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        on_change: impl Fn(&mut RootView, f64, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let fraction = fraction.clamp(0.0, 1.0);
        let bounds_cell = Rc::new(RefCell::new(Bounds::default()));
        let down_cell = bounds_cell.clone();
        let down_change = std::rc::Rc::new(on_change);
        let drag_change = down_change.clone();
        let focus_down = focus.clone();

        div()
            .id(id)
            .accessibility_id(format!("optionmusic.slider.{id}"))
            .role(Role::Slider)
            .aria_label(label)
            .aria_numeric_value((fraction * 100.0) as f64)
            .aria_min_numeric_value(0.0)
            .aria_max_numeric_value(100.0)
            .key_context("Slider")
            .track_focus(focus)
            .on_action(cx.listener(Self::slider_left))
            .on_action(cx.listener(Self::slider_right))
            .on_action(cx.listener(Self::slider_home))
            .on_action(cx.listener(Self::slider_end))
            .on_action(cx.listener(Self::blur_list_action))
            .w_full()
            .h(px(14.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .rounded(px(7.0))
            .focus_visible(|style| {
                style
                    .border_1()
                    .border_color(tokens.focus)
                    .shadow(focus_shadow(tokens))
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |this: &mut RootView,
                          event: &MouseDownEvent,
                          window: &mut Window,
                          cx: &mut Context<RootView>| {
                        focus_down.focus(window, cx);
                        let bounds = *down_cell.borrow();
                        if bounds.size.width > px(0.0) {
                            let f = ((event.position.x - bounds.left()) / bounds.size.width) as f64;
                            down_change(this, f, cx);
                        }
                    },
                ),
            )
            .on_drag(SliderDrag(id), move |_, _, _, cx| cx.new(|_| Empty))
            .on_drag_move::<SliderDrag>(cx.listener(
                move |this: &mut RootView,
                      event: &gpui::DragMoveEvent<SliderDrag>,
                      _window: &mut Window,
                      cx: &mut Context<RootView>| {
                    if event.drag(cx).0 != id {
                        return;
                    }
                    let bounds = event.bounds;
                    if bounds.size.width > px(0.0) {
                        let f =
                            ((event.event.position.x - bounds.left()) / bounds.size.width) as f64;
                        drag_change(this, f, cx);
                    }
                },
            ))
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(4.0))
                    .rounded(px(100.0))
                    .bg(tokens.lift_2)
                    .child(
                        div()
                            .absolute()
                            .left_0()
                            .top_0()
                            .h_full()
                            .w(relative(fraction))
                            .rounded(px(100.0))
                            .bg(tokens.white)
                            .shadow(vec![
                                BoxShadow::new(px(0.), px(0.), tokens.white.opacity(0.25).into())
                                    .blur_radius(px(6.)),
                            ]),
                    )
                    .child(
                        canvas(
                            move |bounds, _window, _cx| {
                                *bounds_cell.borrow_mut() = bounds;
                            },
                            |_, _, _, _| {},
                        )
                        .absolute()
                        .inset_0(),
                    ),
            )
    }

    pub(crate) fn scrub_bar(
        &self,
        id: &'static str,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (position, duration) = (self.playback.position, self.playback.duration);
        let progress = duration
            .filter(|d| *d > 0.0)
            .map_or(0.0, |d| (position / d).clamp(0.0, 1.0));
        let seekable = duration.is_some_and(|d| d > 0.0);

        let track = self.slider_track(
            id,
            "Seek",
            progress as f32,
            &self.seek_focus,
            tokens,
            cx,
            |this: &mut RootView, fraction: f64, cx: &mut Context<RootView>| {
                this.do_seek(fraction, cx)
            },
        );

        div()
            .id(format!("{id}-wrap"))
            .accessibility_id(format!("optionmusic.{id}"))
            .w_full()
            .flex()
            .flex_col()
            .gap(px(4.0))
            .when(!seekable, |this| this.opacity(0.5))
            .child(track)
            .child(
                div()
                    .w_full()
                    .flex()
                    .justify_between()
                    .text_size(px(10.0))
                    .text_color(tokens.faint)
                    .font_weight(gpui::FontWeight::MEDIUM)
                    .child(fmt_time(position))
                    .child(
                        duration
                            .map(|d| format!("-{}", fmt_time((d - position).max(0.0))))
                            .unwrap_or_else(|| "--:--".into()),
                    ),
            )
    }

    pub(crate) fn scrollbar(
        &self,
        id: &'static str,
        scroll_handle: &UniformListScrollHandle,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (viewport_h, content_h, ratio) = {
            let state = scroll_handle.0.borrow();
            let Some(item_size) = state.last_item_size else {
                return div().into_any_element();
            };
            let viewport_h = item_size.item.height;
            let content_h = item_size.contents.height;
            if content_h <= viewport_h || viewport_h <= px(0.0) {
                return div().into_any_element();
            }
            let offset_y = state.base_handle.offset().y;
            let max_offset_y = state.base_handle.max_offset().y;
            let ratio = if max_offset_y > px(0.0) {
                (-offset_y / max_offset_y).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (viewport_h, content_h, ratio)
        };
        let mut thumb_h = viewport_h * (viewport_h / content_h);
        thumb_h = thumb_h.max(px(24.0)).min(viewport_h - px(8.0));
        let thumb_top = (viewport_h - thumb_h) * ratio;
        let track_h = viewport_h - px(4.0);

        let bounds_cell = Rc::new(RefCell::new(Bounds::default()));
        let down_cell = bounds_cell.clone();
        let handle = scroll_handle.clone();
        let handle_move = scroll_handle.clone();
        let thumb_h_copy = thumb_h;

        div()
            .id(id)
            .accessibility_id(format!("optionmusic.scrollbar.{id}"))
            .absolute()
            .right(px(2.0))
            .top(px(2.0))
            .w(px(10.0))
            .h(track_h)
            .cursor_pointer()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(
                    move |_this: &mut RootView,
                          event: &MouseDownEvent,
                          _window: &mut Window,
                          cx: &mut Context<RootView>| {
                        let bounds = *down_cell.borrow();
                        let usable = bounds.size.height - thumb_h_copy;
                        if usable <= px(0.0) {
                            return;
                        }
                        let top = (event.position.y - bounds.top() - thumb_h_copy / 2.0)
                            .clamp(px(0.0), usable);
                        let max = handle.0.borrow().base_handle.max_offset().y;
                        handle
                            .0
                            .borrow()
                            .base_handle
                            .set_offset(point(px(0.0), -(top / usable) * max));
                        cx.notify();
                    },
                ),
            )
            .on_drag(ScrollbarDrag(id), move |_, _, _, cx| cx.new(|_| Empty))
            .on_drag_move::<ScrollbarDrag>(cx.listener(
                move |_this: &mut RootView,
                      event: &gpui::DragMoveEvent<ScrollbarDrag>,
                      _window: &mut Window,
                      cx: &mut Context<RootView>| {
                    if event.drag(cx).0 != id {
                        return;
                    }
                    let bounds = event.bounds;
                    let usable = bounds.size.height - thumb_h_copy;
                    if usable <= px(0.0) {
                        return;
                    }
                    let top = (event.event.position.y - bounds.top() - thumb_h_copy / 2.0)
                        .clamp(px(0.0), usable);
                    let max = handle_move.0.borrow().base_handle.max_offset().y;
                    handle_move
                        .0
                        .borrow()
                        .base_handle
                        .set_offset(point(px(0.0), -(top / usable) * max));
                    cx.notify();
                },
            ))
            .child(
                div()
                    .absolute()
                    .top(thumb_top)
                    .left(px(2.0))
                    .right(px(2.0))
                    .h(thumb_h)
                    .rounded(px(3.0))
                    .bg(tokens.mute),
            )
            .child(
                canvas(
                    move |bounds, _window, _cx| {
                        *bounds_cell.borrow_mut() = bounds;
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .inset_0(),
            )
            .into_any_element()
    }

    pub(crate) fn empty_state(
        &self,
        title: impl Into<SharedString>,
        subtitle: impl Into<SharedString>,
        icon: impl IntoElement,
        tokens: MusicTokens,
    ) -> gpui::Stateful<gpui::Div> {
        let title: SharedString = title.into();
        let subtitle: SharedString = subtitle.into();
        div()
            .id("empty")
            .accessibility_id("optionmusic.empty")
            .role(Role::Status)
            .aria_label(title.clone())
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(14.0))
                    .p(px(24.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(tokens.border)
                    .bg(tokens.panel)
                    .shadow(vec![
                        BoxShadow::new(px(0.), px(8.), tokens.black.opacity(0.4).into())
                            .blur_radius(px(24.))
                            .spread_radius(px(-4.)),
                    ])
                    .child(
                        div()
                            .size(px(28.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icon),
                    )
                    .child(
                        div()
                            .text_size(px(17.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child(title),
                    )
                    .child(
                        div()
                            .text_size(px(13.0))
                            .text_color(tokens.mute)
                            .child(subtitle),
                    ),
            )
    }

    /// Full-coverage click layer that dismisses overlays (menus, popovers).
    /// `dim` paints the modal backdrop tint used under centered dialogs;
    /// anchored popovers keep it clear so the content behind stays readable.
    pub(crate) fn overlay_scrim(&self, cx: &mut Context<Self>, dim: bool) -> AnyElement {
        let scrim = div()
            .id("overlay-scrim")
            .absolute()
            .inset_0()
            .on_any_mouse_down(cx.listener(
                |this: &mut RootView,
                 _event: &MouseDownEvent,
                 _window: &mut Window,
                 cx: &mut Context<RootView>| {
                    this.close_overlays(cx);
                },
            ));
        if !dim {
            return scrim.into_any_element();
        }
        scrim
            .bg(tokens().black.opacity(0.55))
            .with_animation(
                "scrim-dim",
                Animation::new(Duration::from_millis(140)),
                |this, delta| this.opacity(delta),
            )
            .into_any_element()
    }

    /// Shared chrome for anchored popovers (context menu, pickers): Overlay
    /// key context, focus tracking, hairline border, elevated surface.
    pub(crate) fn menu_shell(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        width: f32,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .accessibility_id(format!("optionmusic.{id}"))
            .role(Role::Menu)
            .aria_label(label.into())
            .key_context("Overlay")
            .track_focus(&self.overlay_focus)
            .on_action(cx.listener(Self::dismiss_overlay))
            .on_action(cx.listener(Self::menu_up))
            .on_action(cx.listener(Self::menu_down))
            .on_action(cx.listener(Self::menu_enter))
            .w(px(width))
            .flex()
            .flex_col()
            .p(px(5.0))
            .rounded(px(9.0))
            .border_1()
            .border_color(tokens.border_strong)
            .bg(tokens.elevated)
            .shadow(menu_shadow(tokens))
            .on_any_mouse_down(|_event, _window, cx| cx.stop_propagation())
    }

    /// Shared chrome for centered modal dialogs: same skeleton as popovers
    /// plus uniform inner padding and the deeper dialog shadow.
    pub(crate) fn dialog_shell(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        width: f32,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .accessibility_id(format!("optionmusic.{id}"))
            .role(Role::Dialog)
            .aria_label(label.into())
            .key_context("Overlay")
            .track_focus(&self.overlay_focus)
            .on_action(cx.listener(Self::dismiss_overlay))
            .w(px(width))
            .flex()
            .flex_col()
            .gap(px(14.0))
            .p(px(16.0))
            .rounded(px(12.0))
            .border_1()
            .border_color(tokens.border_strong)
            .bg(tokens.panel)
            .shadow(dialog_shadow(tokens))
            .on_any_mouse_down(|_event, _window, cx| cx.stop_propagation())
    }

    /// Mounts `panel` in a full-window centered flex layer. Wrap the panel in
    /// `overlay_enter` first for the shared entrance animation.
    pub(crate) fn dialog_layer(&self, panel: AnyElement) -> gpui::Div {
        div()
            .absolute()
            .inset_0()
            .flex()
            .items_center()
            .justify_center()
            .child(panel)
    }

    /// Entrance animation for overlay surfaces: opacity + a short downward
    /// settle. One-shot; GPUI renders the end state when reduce_motion is on.
    pub(crate) fn overlay_enter<E>(&self, element: E, id: &'static str) -> AnyElement
    where
        E: Styled + IntoElement + 'static,
    {
        element
            .with_animation(
                id,
                Animation::new(Duration::from_millis(150)).with_easing(ease_out_quint()),
                |this, delta| this.opacity(delta).mt(px(5.0 * (1.0 - delta))),
            )
            .into_any_element()
    }

    /// Title row for dialogs: semibold title, optional muted subtitle, and a
    /// trailing close button.
    pub(crate) fn dialog_header(
        &self,
        title: impl Into<SharedString>,
        subtitle: Option<SharedString>,
        close_id: &'static str,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        on_close: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let mut titles = div().flex().flex_col().gap(px(2.0)).min_w(px(0.0)).child(
            div()
                .text_size(px(13.0))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(tokens.ink)
                .child(title.into()),
        );
        if let Some(subtitle) = subtitle {
            titles = titles.child(
                div()
                    .text_size(px(11.0))
                    .text_color(tokens.mute)
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .overflow_hidden()
                    .child(subtitle),
            );
        }
        div()
            .id(format!("dialog-header-{close_id}"))
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(titles)
            .child(div().flex_1())
            .child(self.icon_button(
                close_id,
                "Close",
                icons::styled(icons::close(px(13.0)), tokens.mute, tokens.ink),
                on_close,
                tokens,
                cx,
            ))
    }

    /// Dialog footer button. `primary` is the solid white action; `selected`
    /// draws the keyboard-selection ring driven by ←/→ in confirm dialogs;
    /// `enabled` gates both the click handler and the visual state.
    pub(crate) fn dialog_button(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        primary: bool,
        selected: bool,
        enabled: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        let button = div()
            .id(id)
            .role(Role::Button)
            .aria_label(label.clone())
            .h(px(30.0))
            .px(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .border_1()
            .text_size(px(12.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .when(primary, |this| {
                this.border_color(gpui::transparent_white())
                    .bg(if enabled {
                        tokens.white
                    } else {
                        tokens.white.opacity(0.35)
                    })
                    .text_color(if enabled {
                        tokens.black
                    } else {
                        tokens.black.opacity(0.5)
                    })
            })
            .when(!primary, |this| {
                this.border_color(if selected {
                    tokens.focus
                } else {
                    tokens.border_strong
                })
                .text_color(if enabled { tokens.ink_2 } else { tokens.faint })
            });
        if !enabled {
            return button;
        }
        button
            .cursor_pointer()
            .when(selected, |this| this.shadow(focus_shadow(tokens)))
            .when(!selected && !primary, |this| {
                this.hover(|style| style.bg(tokens.lift_2).text_color(tokens.ink))
            })
            .when(primary, |this| this.hover(|style| style.bg(tokens.ink)))
            .active(|style| style.opacity(0.8))
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    handler(this, window, cx);
                },
            ))
            .child(label)
    }

    /// Physical-key chip rendered inside dialog hint rows.
    pub(crate) fn kbd_chip(&self, label: &'static str, tokens: MusicTokens) -> gpui::Div {
        div()
            .px(px(5.0))
            .py(px(1.0))
            .rounded(px(4.0))
            .border_1()
            .border_color(tokens.border_strong)
            .bg(tokens.lift)
            .text_size(px(10.0))
            .text_color(tokens.mute)
            .font_family("monospace")
            .child(label)
    }

    /// Footer row of `[key] action` hints, e.g. `[↵] confirm  [esc] cancel`.
    pub(crate) fn hint_row(
        &self,
        hints: &[(&'static str, &'static str)],
        tokens: MusicTokens,
    ) -> gpui::Div {
        let mut row = div().flex().items_center().gap(px(12.0));
        for (key, action) in hints {
            row = row.child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(self.kbd_chip(key, tokens))
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(tokens.faint)
                            .child(*action),
                    ),
            );
        }
        row
    }

    /// Framed wrapper around a `SearchInput` with a visible focused ring.
    pub(crate) fn input_shell(
        &self,
        id: &'static str,
        input: &Entity<SearchInput>,
        focused: bool,
        tokens: MusicTokens,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id(id)
            .h(px(32.0))
            .px(px(9.0))
            .rounded(px(7.0))
            .border_1()
            .border_color(if focused {
                tokens.focus
            } else {
                tokens.border_strong
            })
            .when(focused, |this| this.shadow(focus_shadow(tokens)))
            .bg(tokens.bg)
            .flex()
            .items_center()
            .child(input.clone())
    }

    /// Small caps section label used inside dialogs ("MUSIC FOLDERS", "SOUND").
    pub(crate) fn section_label(&self, text: &'static str, tokens: MusicTokens) -> gpui::Div {
        div()
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::SEMIBOLD)
            .text_color(tokens.faint)
            .child(text)
    }

    pub(crate) fn menu_item(
        &self,
        id: &'static str,
        label: impl Into<SharedString>,
        selected: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
        handler: impl Fn(&mut RootView, &mut Window, &mut Context<RootView>) + 'static,
    ) -> gpui::Stateful<gpui::Div> {
        let label: SharedString = label.into();
        div()
            .id(format!("menu-{id}"))
            .role(Role::MenuItem)
            .aria_label(label.clone())
            .aria_selected(selected)
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
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    handler(this, window, cx);
                },
            ))
            .child(label)
    }
}
