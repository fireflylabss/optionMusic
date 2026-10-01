//! Radio page — seed composer + start action, parity with `msc radio`.
//! Fields: free-text seed query, artist, genre and a `fresh` toggle.
//! An empty composer starts from a random seed (all filters `None`).

use gpui::{Context, Entity, Role, Subscription, Window, div, prelude::*, px};

use crate::icons;
use crate::model::*;
use crate::search_input::{SearchEvent, SearchInput};
use crate::theme::*;
use crate::view::RootView;
use crate::{NavRadio, Radio};

impl RootView {
    /// Wire a radio composer field: Enter starts the radio, Tab/Shift-Tab
    /// cycles the fields, Esc returns focus to the root.
    pub(crate) fn radio_field(
        cx: &mut Context<Self>,
        placeholder: &str,
    ) -> (Entity<SearchInput>, Subscription) {
        let input = cx.new(|cx| SearchInput::with_placeholder(cx, placeholder));
        let sub = cx.subscribe(&input, |this, input, event: &SearchEvent, cx| match event {
            SearchEvent::Changed => {}
            SearchEvent::Submit => this.start_radio(cx),
            SearchEvent::Dismiss => this.refocus_root(cx),
            SearchEvent::TabNext => this.radio_field_cycle(input.clone(), 1, cx),
            SearchEvent::TabPrev => this.radio_field_cycle(input.clone(), -1, cx),
        });
        (input, sub)
    }

    /// `⌘/ctrl-7` — open the Radio page and focus the seed field.
    pub(crate) fn nav_radio(&mut self, _: &NavRadio, w: &mut Window, cx: &mut Context<Self>) {
        self.navigate(Page::Radio, w, cx);
        self.focus_radio_query_detached(cx);
    }

    /// Quick radio (`cmd-shift-r`): seed from the playing track when one is
    /// playing, otherwise open the composer.
    pub(crate) fn quick_radio(&mut self, _: &Radio, window: &mut Window, cx: &mut Context<Self>) {
        let seed = if self.playback.stopped {
            None
        } else {
            self.playback.current.as_ref().map(|t| t.name.clone())
        };
        match seed {
            Some(query) => self.run_radio(Some(&query), None, None, false, cx),
            None => {
                self.navigate(Page::Radio, window, cx);
                self.focus_radio_query_detached(cx);
            }
        }
    }

    /// "Start radio" (button click or Enter in a field): read the composer,
    /// empty fields become `None` so a blank composer picks a random seed.
    pub(crate) fn start_radio(&mut self, cx: &mut Context<Self>) {
        let text = |e: &Entity<SearchInput>| {
            let value = e.read(cx).content.trim().to_string();
            if value.is_empty() { None } else { Some(value) }
        };
        let query = text(&self.radio_query);
        let artist = text(&self.radio_artist);
        let genre = text(&self.radio_genre);
        let fresh = self.radio_fresh;
        self.run_radio(
            query.as_deref(),
            artist.as_deref(),
            genre.as_deref(),
            fresh,
            cx,
        );
    }

    /// Shared entry for the composer and the quick action. `radio_start`
    /// touches the player, so it runs inline on the app thread exactly like
    /// `play_track`/`do_next`; errors surface as a toast, never a panic.
    fn run_radio(
        &mut self,
        query: Option<&str>,
        artist: Option<&str>,
        genre: Option<&str>,
        fresh: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(controller) = self.controller.as_mut() else {
            self.set_status("Library still scanning…", cx);
            return;
        };
        match controller.radio_start(query, artist, genre, fresh) {
            Ok(session) => {
                self.radio_label = Some(session.label.clone().into());
                self.stage_panel = Some(StagePanel::Queue);
                self.set_status(session.label, cx);
                self.refocus_root(cx);
            }
            Err(error) => self.set_status(format!("Radio failed: {error}"), cx),
        }
        self.update_playback_and_cover(cx);
        cx.notify();
    }

    /// Move focus between the composer's fields (Tab / Shift-Tab, wraps).
    fn radio_field_cycle(&mut self, from: Entity<SearchInput>, dir: isize, cx: &mut Context<Self>) {
        let fields = [&self.radio_query, &self.radio_artist, &self.radio_genre];
        let Some(index) = fields
            .iter()
            .position(|f| f.entity_id() == from.entity_id())
        else {
            return;
        };
        let next = (index as isize + dir).rem_euclid(fields.len() as isize) as usize;
        let handle = fields[next].read(cx).focus_handle.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
    }

    /// Focus the seed field without holding a `Window` (action handlers get
    /// `&mut Window` from GPUI, but the detached loop works from both).
    fn focus_radio_query_detached(&mut self, cx: &mut Context<Self>) {
        let handle = self.radio_query.read(cx).focus_handle.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
    }

    /// Composer card: seed query + artist + genre + fresh toggle, mirroring
    /// `msc radio [query] --artist --genre --fresh`.
    pub(crate) fn radio_view(
        &self,
        window: &Window,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let field = |id: &'static str, label: &'static str, input: &Entity<SearchInput>| {
            div()
                .flex()
                .items_center()
                .gap(px(10.0))
                .child(
                    div()
                        .w(px(56.0))
                        .text_size(px(11.0))
                        .text_color(tokens.mute)
                        .child(label),
                )
                .child(
                    self.input_shell(
                        id,
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
            field("radiof-seed", "Seed", &self.radio_query),
            field("radiof-artist", "Artist", &self.radio_artist),
            field("radiof-genre", "Genre", &self.radio_genre),
        ];
        let fresh = self.radio_fresh;

        let fresh_toggle = div()
            .id("radio-fresh")
            .accessibility_id("optionmusic.radio.fresh")
            .role(Role::Button)
            .aria_label("Fresh mode")
            .h(px(26.0))
            .px(px(10.0))
            .rounded(px(13.0))
            .border_1()
            .border_color(if fresh {
                tokens.ink
            } else {
                tokens.border_strong
            })
            .bg(if fresh { tokens.white } else { tokens.bg })
            .text_size(px(11.0))
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_color(if fresh { tokens.black } else { tokens.ink_2 })
            .cursor_pointer()
            .hover(|style| style.text_color(if fresh { tokens.black } else { tokens.ink }))
            .active(|style| style.opacity(0.8))
            .on_click(cx.listener(
                |this: &mut RootView,
                 _: &gpui::ClickEvent,
                 _window: &mut Window,
                 cx: &mut Context<RootView>| {
                    this.radio_fresh = !this.radio_fresh;
                    cx.notify();
                },
            ))
            .child("Fresh");

        div()
            .id("radio-page")
            .accessibility_id("optionmusic.radio")
            .role(Role::Group)
            .aria_label("Radio")
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .min_h_0()
            .child(
                div()
                    .w(px(420.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .p(px(24.0))
                    .rounded(px(14.0))
                    .border_1()
                    .border_color(tokens.border)
                    .bg(tokens.panel)
                    .shadow(vec![
                        gpui::BoxShadow::new(px(0.), px(8.), tokens.black.opacity(0.4).into())
                            .blur_radius(px(24.))
                            .spread_radius(px(-4.)),
                    ])
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .pb(px(2.0))
                            .child(icons::radio(px(15.0)).text_color(tokens.mute))
                            .child(self.section_label("SEED", tokens)),
                    )
                    .children(fields)
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .w(px(56.0))
                                    .text_size(px(11.0))
                                    .text_color(tokens.mute)
                                    .child("Mode"),
                            )
                            .child(fresh_toggle)
                            .child(
                                div()
                                    .text_size(px(11.0))
                                    .text_color(tokens.faint)
                                    .child("favor rarely played tracks"),
                            ),
                    )
                    .when_some(self.radio_label.clone(), |this, label| {
                        this.child(
                            div()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                .pt(px(2.0))
                                .child(icons::radio(px(12.0)).text_color(tokens.mute))
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w(px(0.0))
                                        .truncate()
                                        .text_size(px(11.0))
                                        .text_color(tokens.mute)
                                        .child(label),
                                ),
                        )
                    })
                    .child(self.hint_row(&[("tab", "next field"), ("↵", "start radio")], tokens))
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .justify_end()
                            .child(self.dialog_button(
                                "radio-start",
                                "Start radio",
                                true,
                                false,
                                self.controller.is_some(),
                                tokens,
                                cx,
                                |this, _w, cx| this.start_radio(cx),
                            )),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, px, size};

    use crate::model::Page;
    use crate::view::RootView;
    use crate::{NavRadio, Radio};

    /// NavRadio opens the Radio page and Tab lands on the seed field.
    #[gpui::test]
    fn radio_page_navigates_and_focuses_seed(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(1200.), px(760.)), |window, cx| {
            RootView::new(window, cx)
        });
        cx.run_until_parked();

        window
            .update(cx, |_view, window, cx| {
                window.dispatch_action(Box::new(NavRadio), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, window, cx| {
                assert!(view.page == Page::Radio);
                view.focus_current_list(window, cx);
                assert!(view.radio_query.read(cx).focus_handle.is_focused(window));
            })
            .unwrap();
    }

    /// The quick Radio action with nothing playing opens the composer
    /// instead of starting a session.
    #[gpui::test]
    fn quick_radio_without_playback_opens_page(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(1200.), px(760.)), |window, cx| {
            RootView::new(window, cx)
        });
        cx.run_until_parked();

        window
            .update(cx, |_view, window, cx| {
                window.dispatch_action(Box::new(Radio), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, _window, _cx| {
                assert!(view.page == Page::Radio);
            })
            .unwrap();
    }
}
