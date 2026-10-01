use gpui::{Context, Role, SharedString, Window, div, prelude::*, px};

use optionmusic::update::UpdateStatus;

use crate::theme::*;
use crate::view::RootView;

impl RootView {
    /// Titlebar chip shown next to the app name when a newer release exists.
    /// Clicking it opens the GitHub release page directly.
    pub(crate) fn update_badge(
        &self,
        tag: SharedString,
        url: SharedString,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        div()
            .id("titlebar-update")
            .accessibility_id("optionmusic.titlebar.update")
            .role(Role::Button)
            .aria_label(format!("Update available: {tag}"))
            .h(px(20.0))
            .px(px(8.0))
            .flex()
            .items_center()
            .gap(px(5.0))
            .rounded(px(10.0))
            .border_1()
            .border_color(tokens.border_strong)
            .cursor_pointer()
            .text_size(px(10.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(tokens.ink_2)
            .hover(|style| style.bg(tokens.lift_2).text_color(tokens.ink))
            .active(|style| style.opacity(0.8))
            .on_click(cx.listener(
                move |_this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _w: &mut Window,
                      cx: &mut Context<RootView>| {
                    cx.open_url(&url);
                    cx.stop_propagation();
                },
            ))
            .child(div().size(px(6.0)).rounded(px(3.0)).bg(tokens.ink))
            .child(tag)
    }

    /// Settings dialog row: current check state plus the manual
    /// `CheckForUpdates` trigger and a Download link when a release is newer.
    pub(crate) fn update_row(
        &self,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let (text, color, download_url) = match &self.update_status {
            UpdateStatus::Unknown => ("Not checked yet".to_string(), tokens.faint, None),
            UpdateStatus::Checking => ("Checking…".to_string(), tokens.faint, None),
            UpdateStatus::UpToDate(tag) => (format!("Up to date · {tag}"), tokens.mute, None),
            UpdateStatus::Available(info) => (
                format!("{} available", info.tag),
                tokens.ink,
                Some(info.url.clone()),
            ),
        };
        let checking = matches!(self.update_status, UpdateStatus::Checking);

        div()
            .id("settings-updates")
            .accessibility_id("optionmusic.settings.updates")
            .role(Role::Group)
            .aria_label("Updates")
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(div().h(px(1.0)).bg(tokens.border))
            .child(self.section_label("UPDATES", tokens))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex_1()
                            .text_size(px(11.0))
                            .text_color(color)
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .overflow_hidden()
                            .child(text),
                    )
                    .child(self.dialog_button(
                        "updates-check",
                        "Check now",
                        false,
                        false,
                        !checking,
                        tokens,
                        cx,
                        |_this, window, cx| {
                            window.dispatch_action(Box::new(crate::CheckForUpdates), cx);
                        },
                    ))
                    .when_some(download_url, |this, url| {
                        this.child(self.dialog_button(
                            "updates-download",
                            "Download",
                            true,
                            false,
                            true,
                            tokens,
                            cx,
                            move |_this, _w, cx| cx.open_url(&url),
                        ))
                    }),
            )
    }
}
