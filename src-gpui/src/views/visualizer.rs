use std::time::Duration;

use gpui::{Context, Window, div, prelude::*, px};
use optionmusic::cava::CavaBridge;

use crate::ToggleVisualizer;
use crate::theme::MusicTokens;
use crate::view::RootView;

/// Strip height and animation cadence for the spectrum strip.
const VIZ_HEIGHT: f32 = 18.0;
const VIZ_ACTIVE_MS: u64 = 33;
const VIZ_IDLE_MS: u64 = 200;

impl RootView {
    /// Poll-loop cadence: ~30fps while a live cava bridge feeds the strip,
    /// the regular 200ms playback tick otherwise.
    pub(crate) fn viz_tick_interval(&self) -> Duration {
        if self.visualizer && self.viz_bridge.is_some() {
            Duration::from_millis(VIZ_ACTIVE_MS)
        } else {
            Duration::from_millis(VIZ_IDLE_MS)
        }
    }

    /// Pull the newest frame off the bridge; true when it changed.
    pub(crate) fn viz_sync_frame(&mut self) -> bool {
        let Some(bridge) = self.viz_bridge.as_ref() else {
            return false;
        };
        let next = bridge.snapshot();
        if next == self.viz_levels {
            return false;
        }
        self.viz_levels = next;
        true
    }

    /// Start the cava child off the app thread — `try_start` writes a temp
    /// config, spawns cava and probes several input methods (~100ms each).
    pub(crate) fn start_viz(&mut self, cx: &mut Context<Self>) {
        if self.viz_bridge.is_some() || self.viz_starting {
            return;
        }
        self.viz_starting = true;
        self._viz_task = Some(cx.spawn(async move |this, cx| {
            let bridge = cx
                .background_executor()
                .spawn(async move { CavaBridge::try_start() })
                .await;
            this.update(cx, |view, cx| {
                view.viz_starting = false;
                view.viz_bridge = bridge;
                cx.notify();
            })
            .ok();
        }));
    }

    pub(crate) fn do_toggle_visualizer(&mut self, cx: &mut Context<Self>) {
        self.visualizer = !self.visualizer;
        if self.visualizer {
            self.start_viz(cx);
        } else {
            // Dropping the bridge kills the cava child and removes its
            // temp config; dropping the task cancels a probe in flight.
            self.viz_bridge = None;
            self.viz_levels.clear();
            self.viz_starting = false;
            self._viz_task = None;
        }
        cx.notify();
    }

    pub(crate) fn toggle_visualizer(
        &mut self,
        _: &ToggleVisualizer,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.do_toggle_visualizer(cx);
        self.save_desktop_prefs(window, cx);
    }

    /// Thin spectrum strip docked above the player bar. With no cava bridge
    /// (binary missing or no capturable device) it shows a hint instead —
    /// never a fake spectrum.
    pub(crate) fn visualizer_strip(&self, tokens: MusicTokens) -> gpui::Stateful<gpui::Div> {
        let strip = div()
            .id("viz-strip")
            .accessibility_id("optionmusic.visualizer")
            .h(px(VIZ_HEIGHT))
            .px(px(20.0))
            .flex();
        if self.viz_bridge.is_none() && !self.viz_starting {
            strip.items_center().justify_center().child(
                div()
                    .text_size(px(10.0))
                    .text_color(tokens.faint)
                    .child("install cava for the visualizer"),
            )
        } else {
            strip
                .items_end()
                .gap(px(2.0))
                .children(self.viz_levels.iter().copied().map(|level| {
                    let level = level.clamp(0.0, 1.0);
                    let height = 1.0 + level * (VIZ_HEIGHT - 3.0);
                    div()
                        .flex_1()
                        .min_w(px(1.0))
                        .h(px(height))
                        .rounded(px(1.0))
                        .bg(tokens.ink.opacity(0.12 + level * 0.26))
                }))
        }
    }
}
