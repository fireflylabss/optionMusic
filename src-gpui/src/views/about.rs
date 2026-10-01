use gpui::{AnyElement, Context, Role, Window, div, prelude::*, px};

use crate::icons;
use crate::theme::*;
use crate::view::RootView;

impl RootView {
    /// "About optionMusic" panel: app mark, version, tagline, repo link.
    /// Same chrome as the other centered dialogs (Overlay context, Esc to
    /// close, scrim click dismisses via `close_overlays`).
    pub(crate) fn about_dialog(&self, tokens: MusicTokens, cx: &mut Context<Self>) -> AnyElement {
        let panel = self
            .dialog_shell("about-dialog", "About optionMusic", 300.0, tokens, cx)
            .child(
                self.dialog_header("About", None, "about-close", tokens, cx, |this, _w, cx| {
                    this.close_overlays(cx)
                }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .w(px(44.0))
                            .h(px(44.0))
                            .rounded(px(10.0))
                            .border_1()
                            .border_color(tokens.border_strong)
                            .bg(tokens.elevated)
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(icons::music(px(22.0)).text_color(tokens.ink)),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .text_size(px(15.0))
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(tokens.ink)
                                    .child("optionMusic"),
                            )
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(tokens.mute)
                                    .child(format!("v{} · libmpv2", env!("CARGO_PKG_VERSION"))),
                            ),
                    ),
            )
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(tokens.mute)
                    .child("minimal black & white music player"),
            )
            .child(
                div()
                    .id("about-github")
                    .role(Role::Link)
                    .aria_label("github.com/fireflylabss/optionMusic")
                    .h(px(26.0))
                    .px(px(8.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .text_size(px(11.0))
                    .text_color(tokens.ink_2)
                    .hover(|style| style.bg(tokens.hover).text_color(tokens.ink))
                    .on_click(cx.listener(
                        |_this: &mut RootView,
                         _: &gpui::ClickEvent,
                         _w: &mut Window,
                         cx: &mut Context<RootView>| {
                            cx.open_url("https://github.com/fireflylabss/optionMusic");
                        },
                    ))
                    .child(
                        div()
                            .underline()
                            .child("github.com/fireflylabss/optionMusic"),
                    ),
            )
            .child(self.hint_row(&[("esc", "close")], tokens));
        self.dialog_layer(self.overlay_enter(panel, "about-enter"))
            .into_any_element()
    }
}
