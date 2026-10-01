//! Download sheet — the `msc dl` wizard as a GPUI overlay.
//!
//! Steps mirror the CLI flow (`src/dl_ui.rs` + `download::run_interactive`):
//! provider → query (search or url(s)) → result picker → options → run.
//! Every blocking call (yt-dlp check, search, probe, the batch itself) runs on
//! the background executor with a fresh `CoreController` — the UI's controller
//! never leaves the app thread. `DlEvent`s reach the sheet through an mpsc
//! channel pumped by a `cx.spawn` task (same shape as `watcher.rs`), so the
//! batch keeps running even if the user closes the sheet — the final toast
//! still lands.

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use gpui::{
    Animation, AnimationExt, AnyElement, Context, Entity, PathPromptOptions, Role, ScrollHandle,
    SharedString, Subscription, Task, Window, bounce, div, ease_in_out, prelude::*, px,
};
use optionmusic::config::{DlFallbackMode, default_music_dir};
use optionmusic::controller::CoreController;
use optionmusic::download::{
    Caps, DlEvent, DownloadOptions, MediaItem, MediaKind, Provider, QualityPreset, SearchHit,
};

use crate::icons;
use crate::search_input::{SearchEvent, SearchInput};
use crate::theme::*;
use crate::view::RootView;
use crate::{MenuActivate, MenuDown, MenuLeft, MenuRight, MenuUp, OpenDownloader};

/// Provider rows, in menu order.
const PROVIDERS: [(Provider, &str); 3] = [
    (Provider::Youtube, "video + audio"),
    (Provider::YoutubeMusic, "music-first, video too"),
    (Provider::Soundcloud, "audio only"),
];

/// Quality presets offered in the sheet (no Custom — the CLI's free-form
/// fields don't map to a compact row).
const PRESETS: [QualityPreset; 3] = [
    QualityPreset::Best,
    QualityPreset::Economy,
    QualityPreset::Lower,
];

const PRESET_LABELS: [&str; 3] = ["best", "economy", "lower"];

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum DlStep {
    Provider,
    Query,
    Results,
    Options,
    Run,
}

/// Per-item state inside a running batch.
#[derive(Clone, PartialEq)]
enum DlItemState {
    Queued,
    /// Currently downloading; payload is the active pass (`video`/`audio`).
    Active(&'static str),
    Done,
    Failed,
}

struct DlItemProgress {
    title: SharedString,
    state: DlItemState,
}

/// Live progress for the batch running on the background executor.
pub(crate) struct DlRunProgress {
    items: Vec<DlItemProgress>,
    finished: usize,
    failed: usize,
    warnings: Vec<SharedString>,
    log: Vec<SharedString>,
    done: bool,
    output_dir: PathBuf,
    log_scroll: ScrollHandle,
    items_scroll: ScrollHandle,
}

/// Option rows shown on the Options step — the visible list is rebuilt from
/// `caps`/`kind` so a row is never offered when no selected item supports it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum OptRow {
    Kind,
    Preset,
    Thumb,
    Meta,
    Subs,
    OutDir,
    Fallback,
    Start,
}

/// Presentation data for one option row (keeps `dl_opt_row` small).
struct OptRowUi {
    label: &'static str,
    value: SharedString,
    /// `‹ value ›` affordance on the selected row (arrows cycle the value).
    cycler: bool,
}

impl OptRowUi {
    fn cycler(label: &'static str, value: SharedString) -> Self {
        Self {
            label,
            value,
            cycler: true,
        }
    }

    fn toggle(label: &'static str, on: bool) -> Self {
        Self {
            label,
            value: if on { "on" } else { "off" }.into(),
            cycler: false,
        }
    }

    fn plain(label: &'static str, value: SharedString) -> Self {
        Self {
            label,
            value,
            cycler: false,
        }
    }
}

pub(crate) struct DlSheet {
    step: DlStep,
    provider: Provider,
    provider_sel: usize,
    query_input: Entity<SearchInput>,
    hits: Vec<SearchHit>,
    hit_sel: usize,
    /// URLs of toggled results (hit.url is the dedup key, same as the CLI).
    picked: HashSet<String>,
    /// Resolved selection feeding the run (search picks or pasted urls).
    items: Vec<MediaItem>,
    /// Intersected capabilities of `items` (post-probe when it succeeds).
    caps: Caps,
    kind: MediaKind,
    preset: QualityPreset,
    embed_thumbnail: bool,
    embed_metadata: bool,
    embed_subs: bool,
    output_dir: PathBuf,
    /// Effective fallback for runs. `config.dl_fallback == Ask` maps to Auto:
    /// the engine's ask prompt needs a TTY, which a desktop session never has.
    fallback: DlFallbackMode,
    opt_sel: usize,
    busy: Option<SharedString>,
    /// Inline notice line — search/probe errors and hints, cleared on advance.
    notice: Option<SharedString>,
    yt_dlp: Option<String>,
    run: Option<DlRunProgress>,
    results_scroll: ScrollHandle,
    _subs: Vec<Subscription>,
    _check_task: Option<Task<()>>,
    _work_task: Option<Task<()>>,
    _dir_task: Option<Task<()>>,
}

/// `Provider::base_caps` is `pub(crate)` — mirror it here.
fn provider_caps(provider: Provider) -> Caps {
    match provider {
        Provider::Youtube => Caps {
            video: true,
            audio: true,
            subs: true,
            thumbnail: true,
            music_meta: true,
        },
        Provider::YoutubeMusic => Caps {
            video: true,
            audio: true,
            subs: false,
            thumbnail: true,
            music_meta: true,
        },
        Provider::Soundcloud => Caps {
            video: false,
            audio: true,
            subs: false,
            thumbnail: true,
            music_meta: true,
        },
    }
}

/// `download::intersect_caps` is `pub(crate)` — same fold here.
fn intersect_caps(items: &[MediaItem]) -> Caps {
    let mut caps = Caps {
        video: true,
        audio: true,
        subs: true,
        thumbnail: true,
        music_meta: true,
    };
    for item in items {
        caps.video &= item.caps.video;
        caps.audio &= item.caps.audio;
        caps.subs &= item.caps.subs;
        caps.thumbnail &= item.caps.thumbnail;
        caps.music_meta &= item.caps.music_meta;
    }
    caps
}

/// `download::fmt_secs` is `pub(crate)` — m:ss / h:mm:ss.
fn fmt_dur(secs: u64) -> String {
    let (m, s) = (secs / 60, secs % 60);
    if m >= 60 {
        format!("{}:{:02}:{:02}", m / 60, m % 60, s)
    } else {
        format!("{m}:{s:02}")
    }
}

fn truncate_label(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// `~/…` display form for the output-dir row.
fn short_dir(path: &std::path::Path) -> String {
    let shown = path.display().to_string();
    if let Some(home) = std::env::var_os("HOME") {
        let home = PathBuf::from(home).display().to_string();
        if let Some(rest) = shown.strip_prefix(&home) {
            return format!("~{rest}");
        }
    }
    shown
}

/// Kind choices allowed by the intersected caps (audio-only selections can't
/// offer video rows — matches `ask_kind` in the CLI).
fn kind_choices(caps: Caps) -> Vec<MediaKind> {
    if caps.video {
        vec![MediaKind::Audio, MediaKind::Video, MediaKind::Both]
    } else {
        vec![MediaKind::Audio]
    }
}

/// Options → `DownloadOptions`, mirroring `options_from_preset` (pub(crate)).
fn build_options(sheet: &DlSheet) -> DownloadOptions {
    let (format_selector, container, audio_format, audio_quality) = match sheet.preset {
        QualityPreset::Best | QualityPreset::Custom => ("bv*+ba/b", "mp4", "m4a", "0"),
        QualityPreset::Economy => ("bv*[height<=720]+ba/b", "mp4", "mp3", "5"),
        QualityPreset::Lower => ("bv*[height<=480]+ba/b", "mp4", "mp3", "7"),
    };
    DownloadOptions {
        kind: sheet.kind,
        format_selector: format_selector.into(),
        container: container.into(),
        audio_format: audio_format.into(),
        audio_quality: audio_quality.into(),
        embed_thumbnail: sheet.embed_thumbnail && sheet.caps.thumbnail,
        embed_metadata: sheet.embed_metadata && sheet.caps.music_meta,
        embed_subs: sheet.embed_subs && sheet.caps.subs && sheet.kind.wants_video(),
        output_dir: sheet.output_dir.clone(),
    }
}

fn dl_option_rows(sheet: &DlSheet) -> Vec<OptRow> {
    let mut rows = vec![OptRow::Kind, OptRow::Preset];
    if sheet.caps.thumbnail {
        rows.push(OptRow::Thumb);
    }
    if sheet.caps.music_meta {
        rows.push(OptRow::Meta);
    }
    if sheet.kind.wants_video() && sheet.caps.subs {
        rows.push(OptRow::Subs);
    }
    rows.push(OptRow::OutDir);
    rows.push(OptRow::Fallback);
    rows.push(OptRow::Start);
    rows
}

impl RootView {
    /// ⌘⇧D / sidebar "Download": open (or close) the download sheet.
    pub(crate) fn open_downloader(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dl_sheet.is_some() {
            self.dl_sheet = None;
            self.focus_handle.focus(window, cx);
            cx.notify();
            return;
        }
        self.context_menu = None;
        self.playlist_picker = None;
        self.pending_confirm = None;
        self.settings_open = false;
        self.about_open = false;
        if self.name_target.is_some() {
            self.name_target = None;
            self.name_input.update(cx, |input, cx| input.clear(cx));
        }

        let query_input = cx.new(|cx| {
            SearchInput::with_placeholder(cx, "search or url(s) — separate multiple with ;")
        });
        let sub = cx.subscribe(
            &query_input,
            |this, _input, event: &SearchEvent, cx| match event {
                SearchEvent::Submit | SearchEvent::TabNext => this.dl_submit(cx),
                SearchEvent::Dismiss => this.dl_dismiss(cx),
                SearchEvent::Changed => {}
                SearchEvent::TabPrev => this.dl_focus_overlay(cx),
            },
        );

        let config = self.controller.as_ref().map(|c| c.config.clone());
        let output_dir = config
            .as_ref()
            .and_then(|c| c.music_dirs.first().cloned())
            .unwrap_or_else(|| default_music_dir().join("optionmusic"));
        // Ask can't prompt on desktop — treat it as Auto unless the user
        // pinned auto/off in config.toml.
        let fallback = match config.as_ref().map(|c| c.dl_fallback) {
            Some(mode) if mode != DlFallbackMode::Ask => mode,
            _ => DlFallbackMode::Auto,
        };
        let provider = PROVIDERS[0].0;
        self.dl_sheet = Some(DlSheet {
            step: DlStep::Provider,
            provider,
            provider_sel: 0,
            query_input,
            hits: Vec::new(),
            hit_sel: 0,
            picked: HashSet::new(),
            items: Vec::new(),
            caps: provider_caps(provider),
            kind: provider.default_kind(),
            preset: QualityPreset::Best,
            embed_thumbnail: true,
            embed_metadata: true,
            embed_subs: true,
            output_dir,
            fallback,
            opt_sel: 0,
            busy: None,
            notice: None,
            yt_dlp: None,
            run: None,
            results_scroll: ScrollHandle::new(),
            _subs: vec![sub],
            _check_task: None,
            _work_task: None,
            _dir_task: None,
        });
        self.dl_check(cx);
        self.overlay_focus.focus(window, cx);
        cx.notify();
    }

    /// `OpenDownloader` action entry point.
    pub(crate) fn on_open_downloader(
        &mut self,
        _: &OpenDownloader,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_downloader(window, cx);
    }

    /// Focus the sheet's overlay handle from contexts without a `Window`
    /// (SearchInput event callbacks) — same trick as `refocus_root`.
    fn dl_focus_overlay(&mut self, cx: &mut Context<Self>) {
        let handle = self.overlay_focus.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
        cx.notify();
    }

    fn dl_focus_query(&mut self, cx: &mut Context<Self>) {
        let Some(handle) = self
            .dl_sheet
            .as_ref()
            .map(|sheet| sheet.query_input.read(cx).focus_handle.clone())
        else {
            return;
        };
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
        cx.notify();
    }

    /// Esc on the query input: close the sheet and refocus the root handle
    /// (`refocus_root` is private to view.rs — same `cx.windows()` trick).
    fn dl_dismiss(&mut self, cx: &mut Context<Self>) {
        self.close_overlays(cx);
        let handle = self.focus_handle.clone();
        for window in cx.windows() {
            let handle = handle.clone();
            let _ = window.update(cx, move |_, window, app| {
                handle.focus(window, app);
            });
        }
        cx.notify();
    }

    /// One-time yt-dlp availability check on the background executor.
    fn dl_check(&mut self, cx: &mut Context<Self>) {
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    CoreController::new()
                        .dl_available()
                        .map_err(|e| format!("{e:#}"))
                })
                .await;
            this.update(cx, |view, cx| {
                if let Some(sheet) = view.dl_sheet.as_mut() {
                    match result {
                        Ok(yt) => sheet.yt_dlp = Some(yt),
                        Err(error) => sheet.notice = Some(error.into()),
                    }
                    cx.notify();
                }
            })
            .ok();
        });
        if let Some(sheet) = self.dl_sheet.as_mut() {
            sheet._check_task = Some(task);
        }
    }

    // ── Wizard transitions ──────────────────────────────────────

    fn dl_choose_provider(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        if sheet.busy.is_some() {
            return;
        }
        let (provider, _) = PROVIDERS[index.min(PROVIDERS.len() - 1)];
        sheet.provider = provider;
        sheet.provider_sel = index.min(PROVIDERS.len() - 1);
        sheet.kind = provider.default_kind();
        sheet.notice = None;
        sheet.step = DlStep::Query;
        self.dl_focus_query(cx);
    }

    /// SearchSubmit from the query input (or the step's footer button).
    fn dl_submit(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        if sheet.busy.is_some() || sheet.step != DlStep::Query {
            return;
        }
        let raw = sheet.query_input.read(cx).content.to_string();
        if raw.trim().is_empty() {
            sheet.notice = Some("type a search or paste url(s)".into());
            cx.notify();
            return;
        }
        sheet.notice = None;

        // `input_is_urls` is pub(crate); same test via the two pub helpers.
        let parts = optionmusic::download::split_urls(&raw);
        if !parts.is_empty()
            && parts
                .iter()
                .all(|p| optionmusic::download::looks_like_url(p))
        {
            let session_provider = sheet.provider;
            sheet.items = parts
                .into_iter()
                .map(|url| {
                    let provider =
                        optionmusic::download::detect_provider(&url).unwrap_or(session_provider);
                    MediaItem {
                        title: url.clone(),
                        url,
                        provider,
                        caps: provider_caps(provider),
                    }
                })
                .collect();
            self.dl_probe(cx);
            return;
        }

        let provider = sheet.provider;
        sheet.busy = Some(format!("searching {}…", provider.label()).into());
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    CoreController::new()
                        .dl_search(provider, &raw)
                        .map_err(|e| format!("{e:#}"))
                })
                .await;
            this.update(cx, |view, cx| view.dl_search_done(result, cx))
                .ok();
        });
        if let Some(sheet) = self.dl_sheet.as_mut() {
            sheet._work_task = Some(task);
        }
        cx.notify();
    }

    fn dl_search_done(&mut self, result: Result<Vec<SearchHit>, String>, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        sheet.busy = None;
        match result {
            Ok(hits) if hits.is_empty() => {
                sheet.notice = Some("no results — try a different query".into());
                cx.notify();
            }
            Ok(hits) => {
                sheet.hits = hits;
                sheet.hit_sel = 0;
                sheet.picked.clear();
                sheet.step = DlStep::Results;
                self.dl_focus_overlay(cx);
            }
            Err(error) => {
                sheet.notice = Some(error.into());
                cx.notify();
            }
        }
    }

    fn dl_toggle_hit(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        let Some(hit) = sheet.hits.get(index) else {
            return;
        };
        let url = hit.url.clone();
        if !sheet.picked.remove(&url) {
            sheet.picked.insert(url);
        }
        sheet.hit_sel = index;
        cx.notify();
    }

    fn dl_toggle_all_hits(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        if sheet.picked.len() == sheet.hits.len() {
            sheet.picked.clear();
        } else {
            sheet.picked = sheet.hits.iter().map(|h| h.url.clone()).collect();
        }
        cx.notify();
    }

    /// Results → probe → Options. Probing is best-effort: on failure the
    /// provider base caps drive the options instead.
    fn dl_results_continue(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        if sheet.busy.is_some() {
            return;
        }
        sheet.items = sheet
            .hits
            .iter()
            .filter(|h| sheet.picked.contains(&h.url))
            .map(|h| MediaItem {
                title: h.title.clone(),
                url: h.url.clone(),
                provider: h.provider,
                caps: provider_caps(h.provider),
            })
            .collect();
        if sheet.items.is_empty() {
            sheet.notice = Some("select at least one result".into());
            cx.notify();
            return;
        }
        sheet.notice = None;
        self.dl_probe(cx);
    }

    fn dl_probe(&mut self, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        let mut items = sheet.items.clone();
        sheet.busy = Some("probing items…".into());
        let task = cx.spawn(async move |this, cx| {
            let result = cx
                .background_executor()
                .spawn(async move {
                    match CoreController::new().dl_probe(&mut items) {
                        Ok(()) => Ok(items),
                        Err(e) => Err((format!("{e:#}"), items)),
                    }
                })
                .await;
            this.update(cx, |view, cx| view.dl_probe_done(result, cx))
                .ok();
        });
        if let Some(sheet) = self.dl_sheet.as_mut() {
            sheet._work_task = Some(task);
        }
        cx.notify();
    }

    fn dl_probe_done(
        &mut self,
        result: Result<Vec<MediaItem>, (String, Vec<MediaItem>)>,
        cx: &mut Context<Self>,
    ) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        sheet.busy = None;
        let (items, probe_warning) = match result {
            Ok(items) => (items, None),
            Err((error, items)) => (items, Some(format!("probe skipped — {error}"))),
        };
        sheet.items = items;
        sheet.caps = intersect_caps(&sheet.items);
        if !sheet.caps.video && sheet.kind.wants_video() {
            sheet.kind = MediaKind::Audio;
        }
        // `options_from_preset` defaults under the Best preset.
        sheet.preset = QualityPreset::Best;
        sheet.embed_thumbnail = sheet.caps.thumbnail;
        sheet.embed_metadata = sheet.caps.music_meta;
        sheet.embed_subs = sheet.caps.subs && sheet.kind.wants_video();
        sheet.opt_sel = 0;
        sheet.step = DlStep::Options;
        sheet.notice = probe_warning.map(SharedString::from);
        cx.notify();
    }

    // ── Options step ────────────────────────────────────────────

    fn dl_cycle_kind(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        let choices = kind_choices(sheet.caps);
        let idx = choices.iter().position(|k| *k == sheet.kind).unwrap_or(0) as isize;
        let next = (idx + delta).rem_euclid(choices.len() as isize) as usize;
        sheet.kind = choices[next];
        // Subs embed only exists while a video pass runs.
        sheet.embed_subs = sheet.embed_subs && sheet.kind.wants_video();
        cx.notify();
    }

    fn dl_cycle_preset(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        let idx = PRESETS.iter().position(|p| *p == sheet.preset).unwrap_or(0) as isize;
        let next = (idx + delta).rem_euclid(PRESETS.len() as isize) as usize;
        sheet.preset = PRESETS[next];
        cx.notify();
    }

    fn dl_toggle_option(&mut self, row: OptRow, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        match row {
            OptRow::Thumb => sheet.embed_thumbnail = !sheet.embed_thumbnail,
            OptRow::Meta => sheet.embed_metadata = !sheet.embed_metadata,
            OptRow::Subs => sheet.embed_subs = !sheet.embed_subs,
            OptRow::Fallback => {
                sheet.fallback = match sheet.fallback {
                    DlFallbackMode::Auto => DlFallbackMode::Off,
                    _ => DlFallbackMode::Auto,
                };
            }
            _ => {}
        }
        cx.notify();
    }

    fn dl_pick_output_dir(&mut self, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Download output folder".into()),
        });
        let task = cx.spawn(async move |this, cx| {
            let picked = match receiver.await {
                Ok(Ok(Some(paths))) => paths,
                Ok(Ok(None)) => return,
                Ok(Err(error)) => {
                    this.update(cx, |view, cx| {
                        view.set_status(format!("Open failed: {error}"), cx);
                    })
                    .ok();
                    return;
                }
                Err(_) => return,
            };
            if let Some(path) = picked.into_iter().next() {
                this.update(cx, |view, cx| {
                    if let Some(sheet) = view.dl_sheet.as_mut() {
                        sheet.output_dir = path;
                        cx.notify();
                    }
                })
                .ok();
            }
        });
        if let Some(sheet) = self.dl_sheet.as_mut() {
            sheet._dir_task = Some(task);
        }
    }

    // ── Run step ────────────────────────────────────────────────

    fn dl_run_start(&mut self, cx: &mut Context<Self>) {
        if self._dl_task.is_some() {
            self.set_status("a download is already running", cx);
            return;
        }
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        if sheet.busy.is_some() || sheet.items.is_empty() {
            return;
        }
        let items = sheet.items.clone();
        let opts = build_options(sheet);
        let fallback = sheet.fallback;
        sheet.run = Some(DlRunProgress {
            items: items
                .iter()
                .map(|i| DlItemProgress {
                    title: truncate_label(&i.title, 64).into(),
                    state: DlItemState::Queued,
                })
                .collect(),
            finished: 0,
            failed: 0,
            warnings: Vec::new(),
            log: Vec::new(),
            done: false,
            output_dir: opts.output_dir.clone(),
            log_scroll: ScrollHandle::new(),
            items_scroll: ScrollHandle::new(),
        });
        sheet.notice = None;
        sheet.step = DlStep::Run;

        // dl_run is synchronous on its own thread; a fresh CoreController is
        // cheap and never touches the UI's player.
        let (tx, rx) = mpsc::channel::<DlEvent>();
        let run = cx.background_executor().spawn(async move {
            let mut controller = CoreController::new();
            controller.config.dl_fallback = fallback;
            controller
                .dl_run(&items, &opts, |ev| {
                    let _ = tx.send(ev);
                })
                .map_err(|e| format!("{e:#}"))
        });

        // Pump DlEvents into the view until the channel closes (run end).
        // Held on RootView so closing the sheet doesn't cancel the batch.
        let rx = Arc::new(Mutex::new(rx));
        self._dl_task = Some(cx.spawn(async move |this, cx| {
            loop {
                let ev = cx
                    .background_executor()
                    .spawn({
                        let rx = Arc::clone(&rx);
                        async move { rx.lock().ok().and_then(|rx| rx.recv().ok()) }
                    })
                    .await;
                match ev {
                    Some(ev) => {
                        this.update(cx, |view, cx| view.apply_dl_event(ev, cx)).ok();
                    }
                    None => break,
                }
            }
            let result = run.await;
            this.update(cx, |view, cx| view.dl_run_finished(result, cx))
                .ok();
        }));
        cx.notify();
    }

    fn apply_dl_event(&mut self, ev: DlEvent, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        let Some(run) = sheet.run.as_mut() else {
            return;
        };
        let log_push = |run: &mut DlRunProgress, line: String| {
            run.log.push(line.into());
            if run.log.len() > 200 {
                run.log.drain(..run.log.len() - 200);
            }
            run.log_scroll.scroll_to_item(run.log.len() - 1);
        };
        match ev {
            DlEvent::Warning { text } => {
                run.warnings.push(text.clone().into());
                log_push(run, format!("warning — {text}"));
            }
            DlEvent::BatchStarted {
                total,
                kind,
                output_dir,
            } => {
                log_push(
                    run,
                    format!("{total} item(s) · {kind} → {}", output_dir.display()),
                );
            }
            DlEvent::ItemStarted { index, title } => {
                if let Some(item) = run.items.get_mut(index) {
                    item.state = DlItemState::Active("starting");
                }
                log_push(
                    run,
                    format!(
                        "[{index}/{total}] {title}",
                        total = run.items.len(),
                        index = index + 1
                    ),
                );
            }
            DlEvent::PassStarted { index, pass } => {
                if let Some(item) = run.items.get_mut(index) {
                    item.state = DlItemState::Active(pass);
                }
                log_push(run, format!("  {pass}…"));
            }
            DlEvent::PassFailed { index, pass, error } => {
                if let Some(item) = run.items.get_mut(index) {
                    item.state = DlItemState::Active(pass);
                }
                let first = error.lines().next().unwrap_or("failed").to_string();
                log_push(
                    run,
                    format!("  {pass} failed — {}", truncate_label(&first, 96)),
                );
            }
            DlEvent::ItemFinished { index, ok } => {
                run.finished += 1;
                if !ok {
                    run.failed += 1;
                }
                if let Some(item) = run.items.get_mut(index) {
                    item.state = if ok {
                        DlItemState::Done
                    } else {
                        DlItemState::Failed
                    };
                }
            }
            DlEvent::BatchFinished { total, failed } => {
                run.done = true;
                log_push(
                    run,
                    format!("done — {} ok · {failed} failed", total - failed),
                );
            }
        }
        cx.notify();
    }

    fn dl_run_finished(&mut self, result: Result<(), String>, cx: &mut Context<Self>) {
        self._dl_task = None;
        match result {
            Ok(()) => {
                let summary = self
                    .dl_sheet
                    .as_ref()
                    .and_then(|s| s.run.as_ref())
                    .map(|run| {
                        format!(
                            "downloaded {} → {}",
                            run.items.len() - run.failed,
                            run.output_dir.display()
                        )
                    });
                if let Some(run) = self.dl_sheet.as_mut().and_then(|s| s.run.as_mut()) {
                    run.done = true;
                }
                self.set_status(summary.unwrap_or_else(|| "downloads finished".into()), cx);
            }
            Err(error) => {
                if let Some(run) = self.dl_sheet.as_mut().and_then(|s| s.run.as_mut()) {
                    run.done = true;
                    let first = error.lines().next().unwrap_or("failed").to_string();
                    run.log.push(format!("error — {first}").into());
                }
                self.set_status(truncate_label(&error.replace('\n', " · "), 120), cx);
            }
        }
        cx.notify();
    }

    // ── Keyboard contract (Overlay context on the dialog) ───────

    fn dl_menu_up(&mut self, _: &MenuUp, _w: &mut Window, cx: &mut Context<Self>) {
        self.dl_move(-1, cx);
        cx.stop_propagation();
    }

    fn dl_menu_down(&mut self, _: &MenuDown, _w: &mut Window, cx: &mut Context<Self>) {
        self.dl_move(1, cx);
        cx.stop_propagation();
    }

    fn dl_menu_left(&mut self, _: &MenuLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.dl_adjust(-1, window, cx);
        cx.stop_propagation();
    }

    fn dl_menu_right(&mut self, _: &MenuRight, window: &mut Window, cx: &mut Context<Self>) {
        self.dl_adjust(1, window, cx);
        cx.stop_propagation();
    }

    fn dl_menu_enter(&mut self, _: &MenuActivate, window: &mut Window, cx: &mut Context<Self>) {
        self.dl_activate(window, cx);
        cx.stop_propagation();
    }

    fn dl_move(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        match sheet.step {
            DlStep::Provider => {
                sheet.provider_sel = (sheet.provider_sel as isize + delta)
                    .clamp(0, PROVIDERS.len() as isize - 1)
                    as usize;
            }
            DlStep::Results => {
                if !sheet.hits.is_empty() {
                    sheet.hit_sel = (sheet.hit_sel as isize + delta)
                        .clamp(0, sheet.hits.len() as isize - 1)
                        as usize;
                    sheet.results_scroll.scroll_to_item(sheet.hit_sel);
                }
            }
            DlStep::Options => {
                let len = dl_option_rows(sheet).len();
                sheet.opt_sel =
                    (sheet.opt_sel as isize + delta).clamp(0, len as isize - 1) as usize;
            }
            _ => {}
        }
        cx.notify();
    }

    /// ←/→ on the overlay: cycle option values; on list steps right advances,
    /// left steps back.
    fn dl_adjust(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_ref() else {
            return;
        };
        let step = sheet.step;
        let opt_row = if step == DlStep::Options {
            dl_option_rows(sheet).get(sheet.opt_sel).copied()
        } else {
            None
        };
        match step {
            DlStep::Provider if delta > 0 => self.dl_choose_provider(self.dl_sheet_sel(), cx),
            DlStep::Provider => {}
            DlStep::Query => {
                if delta < 0 {
                    self.dl_go(DlStep::Provider, window, cx);
                }
            }
            DlStep::Results => {
                if delta < 0 {
                    self.dl_go(DlStep::Query, window, cx);
                } else {
                    self.dl_results_continue(cx);
                }
            }
            DlStep::Options => match opt_row {
                Some(OptRow::Kind) => self.dl_cycle_kind(delta, cx),
                Some(OptRow::Preset) => self.dl_cycle_preset(delta, cx),
                Some(OptRow::OutDir) => {
                    if delta > 0 {
                        self.dl_pick_output_dir(cx);
                    }
                }
                Some(OptRow::Start) => {
                    if delta > 0 {
                        self.dl_run_start(cx);
                    }
                }
                Some(row) => self.dl_toggle_option(row, cx),
                None => {}
            },
            DlStep::Run => {}
        }
        cx.notify();
    }

    fn dl_sheet_sel(&self) -> usize {
        self.dl_sheet.as_ref().map(|s| s.provider_sel).unwrap_or(0)
    }

    /// ↵ on the overlay: row's own action (toggle/cycle/picker/start).
    fn dl_activate(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_ref() else {
            return;
        };
        let step = sheet.step;
        let sel = sheet.hit_sel;
        let opt_row = if step == DlStep::Options {
            dl_option_rows(sheet).get(sheet.opt_sel).copied()
        } else {
            None
        };
        match step {
            DlStep::Provider => self.dl_choose_provider(self.dl_sheet_sel(), cx),
            DlStep::Query => self.dl_focus_query(cx),
            DlStep::Results => self.dl_toggle_hit(sel, cx),
            DlStep::Options => match opt_row {
                Some(OptRow::OutDir) => self.dl_pick_output_dir(cx),
                Some(OptRow::Start) => self.dl_run_start(cx),
                Some(OptRow::Kind) => self.dl_cycle_kind(1, cx),
                Some(OptRow::Preset) => self.dl_cycle_preset(1, cx),
                Some(row) => self.dl_toggle_option(row, cx),
                None => {}
            },
            DlStep::Run => {
                if self
                    .dl_sheet
                    .as_ref()
                    .and_then(|s| s.run.as_ref())
                    .is_some_and(|run| run.done)
                {
                    self.close_overlays(cx);
                    self.focus_handle.focus(window, cx);
                }
            }
        }
        cx.notify();
    }

    /// Step backwards (footer Back button); ← on list steps does the same.
    fn dl_go(&mut self, step: DlStep, window: &mut Window, cx: &mut Context<Self>) {
        let Some(sheet) = self.dl_sheet.as_mut() else {
            return;
        };
        if sheet.busy.is_some() {
            return;
        }
        match (sheet.step, step) {
            (DlStep::Query, DlStep::Provider) => {
                sheet.step = DlStep::Provider;
                self.overlay_focus.focus(window, cx);
            }
            (DlStep::Results, DlStep::Query) => {
                sheet.step = DlStep::Query;
                let focus = sheet.query_input.read(cx).focus_handle.clone();
                focus.focus(window, cx);
            }
            (DlStep::Options, _) => {
                // URL input skipped the picker — step back to wherever the
                // items came from.
                sheet.step = if sheet.hits.is_empty() {
                    let focus = sheet.query_input.read(cx).focus_handle.clone();
                    focus.focus(window, cx);
                    DlStep::Query
                } else {
                    self.overlay_focus.focus(window, cx);
                    DlStep::Results
                };
            }
            _ => {}
        }
        cx.notify();
    }
}

// ── Render ──────────────────────────────────────────────────

impl RootView {
    /// The sheet's panel, mounted as a centered dialog layer by `render`.
    pub(crate) fn download_sheet(
        &self,
        window: &mut Window,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(sheet) = self.dl_sheet.as_ref() else {
            return div().into_any_element();
        };
        let body: AnyElement = match sheet.step {
            DlStep::Provider => self.dl_provider_step(sheet, tokens, cx),
            DlStep::Query => self.dl_query_step(sheet, window, tokens, cx),
            DlStep::Results => self.dl_results_step(sheet, tokens, cx),
            DlStep::Options => self.dl_options_step(sheet, tokens, cx),
            DlStep::Run => self.dl_run_step(sheet, tokens, cx),
        };

        let panel = self
            .dialog_shell("download-sheet", "Download", 470.0, tokens, cx)
            .on_action(cx.listener(Self::dl_menu_up))
            .on_action(cx.listener(Self::dl_menu_down))
            .on_action(cx.listener(Self::dl_menu_left))
            .on_action(cx.listener(Self::dl_menu_right))
            .on_action(cx.listener(Self::dl_menu_enter))
            .child(self.dialog_header(
                "Download",
                Some("powered by yt-dlp".into()),
                "download-close",
                tokens,
                cx,
                |this, _w, cx| this.close_overlays(cx),
            ))
            .child(self.dl_step_strip(sheet, tokens))
            .child(div().min_h(px(240.0)).flex().flex_col().child(body))
            .child(div().h(px(1.0)).bg(tokens.border))
            .child(self.dl_footer(sheet, tokens, cx));

        self.dialog_layer(self.overlay_enter(panel, "download-enter"))
            .into_any_element()
    }

    /// `provider · query · results · options · run` progress strip.
    fn dl_step_strip(&self, sheet: &DlSheet, tokens: MusicTokens) -> gpui::Div {
        const STEPS: [&str; 5] = ["provider", "query", "results", "options", "run"];
        let mut strip = div().flex().items_center().gap(px(8.0));
        for (i, name) in STEPS.iter().enumerate() {
            let active = i == sheet.step as usize;
            strip = strip.child(
                div()
                    .text_size(px(9.0))
                    .font_weight(if active {
                        gpui::FontWeight::SEMIBOLD
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .text_color(if active { tokens.ink } else { tokens.faint })
                    .child(*name),
            );
            if i + 1 < STEPS.len() {
                strip = strip.child(div().text_size(px(9.0)).text_color(tokens.faint).child("→"));
            }
        }
        strip
            .child(div().flex_1())
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(tokens.mute)
                    .child(format!(
                        "{} · {}",
                        sheet.provider.label(),
                        sheet.yt_dlp.as_deref().map(|_| "yt-dlp ok").unwrap_or("…")
                    )),
            )
    }

    fn dl_provider_row(
        &self,
        index: usize,
        provider: Provider,
        hint: &'static str,
        selected: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let label = provider.label();
        div()
            .id(format!("dl-provider-{index}"))
            .accessibility_id(format!("optionmusic.dl.provider.{label}"))
            .role(Role::Button)
            .aria_label(format!("{label} — {hint}"))
            .aria_selected(selected)
            .h(px(40.0))
            .px(px(10.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .gap(px(10.0))
            .cursor_pointer()
            .when(selected, |this| this.bg(tokens.selected))
            .hover(|style| style.bg(tokens.selected))
            .active(|style| style.opacity(0.8))
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _w: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.dl_choose_provider(index, cx);
                },
            ))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .min_w(px(0.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_color(if selected { tokens.ink } else { tokens.ink_2 })
                            .child(label),
                    )
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(tokens.mute)
                            .child(hint),
                    ),
            )
            .child(div().flex_1())
            .when(selected, |this| {
                this.child(icons::styled(
                    icons::chevron_right(px(12.0)),
                    tokens.mute,
                    tokens.ink,
                ))
            })
    }

    fn dl_provider_step(
        &self,
        sheet: &DlSheet,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div().flex().flex_col().gap(px(2.0));
        for (index, (provider, hint)) in PROVIDERS.iter().enumerate() {
            list = list.child(self.dl_provider_row(
                index,
                *provider,
                hint,
                sheet.provider_sel == index,
                tokens,
                cx,
            ));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.section_label("PROVIDER", tokens))
            .child(list)
            .into_any_element()
    }

    fn dl_query_step(
        &self,
        sheet: &DlSheet,
        window: &Window,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let focused = sheet.query_input.read(cx).focus_handle.is_focused(window);
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(self.section_label("SEARCH", tokens))
            .child(self.input_shell("dl-query", &sheet.query_input, focused, tokens))
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(tokens.mute)
                    .child("one or more urls also work — they skip the picker"),
            )
            .child(
                div()
                    .text_size(px(10.0))
                    .text_color(tokens.faint)
                    .child(format!("provider · {}", sheet.provider.label())),
            )
            .into_any_element()
    }

    fn dl_hit_row(
        &self,
        index: usize,
        hit: &SearchHit,
        picked: bool,
        selected: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let title = hit.title.clone();
        let uploader = if hit.uploader.is_empty() {
            hit.provider.label().to_string()
        } else {
            truncate_label(&hit.uploader, 22)
        };
        let meta = format!(
            "{uploader} · {}",
            hit.duration.map(fmt_dur).unwrap_or_else(|| "--:--".into())
        );
        div()
            .id(format!("dl-hit-{index}"))
            .accessibility_id(format!("optionmusic.dl.hit.{index}"))
            .role(Role::Button)
            .aria_label(format!("{title} — {meta}"))
            .aria_selected(picked)
            .min_h(px(32.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .cursor_pointer()
            .when(selected, |this| this.bg(tokens.selected))
            .hover(|style| style.bg(tokens.selected))
            .active(|style| style.opacity(0.8))
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      _w: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.dl_toggle_hit(index, cx);
                },
            ))
            .child(
                div()
                    .w(px(14.0))
                    .flex_none()
                    .text_size(px(11.0))
                    .text_color(if picked { tokens.ink } else { tokens.faint })
                    .child(if picked { "●" } else { "○" }),
            )
            .child(
                div()
                    .flex_1()
                    .min_w(px(0.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .truncate()
                            .text_size(px(12.0))
                            .text_color(if picked { tokens.ink } else { tokens.ink_2 })
                            .child(title),
                    )
                    .child(
                        div()
                            .truncate()
                            .text_size(px(10.0))
                            .text_color(tokens.faint)
                            .child(meta),
                    ),
            )
            .child(
                div()
                    .flex_none()
                    .text_size(px(9.0))
                    .text_color(tokens.faint)
                    .child(hit.provider.label()),
            )
    }

    fn dl_results_step(
        &self,
        sheet: &DlSheet,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let mut list = div()
            .id("dl-results")
            .flex()
            .flex_col()
            .gap(px(1.0))
            .max_h(px(230.0))
            .overflow_y_scroll()
            .track_scroll(&sheet.results_scroll);
        for (index, hit) in sheet.hits.iter().enumerate() {
            list = list.child(self.dl_hit_row(
                index,
                hit,
                sheet.picked.contains(&hit.url),
                sheet.hit_sel == index,
                tokens,
                cx,
            ));
        }
        let all_label = if sheet.picked.len() == sheet.hits.len() {
            "clear"
        } else {
            "all"
        };
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(self.section_label("RESULTS", tokens))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(tokens.mute)
                            .child(format!("{} selected", sheet.picked.len())),
                    )
                    .child(
                        div()
                            .id("dl-select-all")
                            .ml(px(8.0))
                            .px(px(6.0))
                            .py(px(2.0))
                            .rounded(px(5.0))
                            .border_1()
                            .border_color(tokens.border)
                            .cursor_pointer()
                            .text_size(px(10.0))
                            .text_color(tokens.mute)
                            .hover(|style| {
                                style
                                    .text_color(tokens.ink)
                                    .border_color(tokens.border_strong)
                            })
                            .on_click(cx.listener(
                                |this: &mut RootView,
                                 _: &gpui::ClickEvent,
                                 _w: &mut Window,
                                 cx: &mut Context<RootView>| {
                                    this.dl_toggle_all_hits(cx);
                                },
                            ))
                            .child(all_label),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn dl_opt_row(
        &self,
        index: usize,
        id: &'static str,
        ui: OptRowUi,
        selected: bool,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        let OptRowUi {
            label,
            value,
            cycler,
        } = ui;
        let aria = format!("{label}: {value}");
        let mut value_row = div()
            .flex()
            .items_center()
            .gap(px(6.0))
            .text_size(px(11.0))
            .text_color(if selected { tokens.ink } else { tokens.ink_2 });
        if cycler && selected {
            value_row = value_row.child(div().text_color(tokens.faint).child("‹"));
        }
        value_row = value_row.child(value);
        if cycler && selected {
            value_row = value_row.child(div().text_color(tokens.faint).child("›"));
        }
        div()
            .id(format!("dl-opt-{id}"))
            .accessibility_id(format!("optionmusic.dl.opt.{id}"))
            .role(Role::Button)
            .aria_label(aria)
            .aria_selected(selected)
            .h(px(26.0))
            .px(px(8.0))
            .rounded(px(6.0))
            .flex()
            .items_center()
            .cursor_pointer()
            .when(selected, |this| this.bg(tokens.selected))
            .hover(|style| style.bg(tokens.selected))
            .on_click(cx.listener(
                move |this: &mut RootView,
                      _: &gpui::ClickEvent,
                      window: &mut Window,
                      cx: &mut Context<RootView>| {
                    this.dl_activate_opt(index, window, cx);
                },
            ))
            .child(
                div()
                    .text_size(px(11.0))
                    .text_color(if selected { tokens.ink_2 } else { tokens.mute })
                    .child(label),
            )
            .child(div().flex_1())
            .child(value_row)
    }

    fn dl_options_step(
        &self,
        sheet: &DlSheet,
        tokens: MusicTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let rows = dl_option_rows(sheet);
        let mut list = div().flex().flex_col().gap(px(1.0));
        for (index, row) in rows.iter().enumerate() {
            let selected = sheet.opt_sel == index;
            let (id, ui): (&'static str, OptRowUi) = match row {
                OptRow::Kind => ("kind", OptRowUi::cycler("what", sheet.kind.label().into())),
                OptRow::Preset => (
                    "preset",
                    OptRowUi::cycler(
                        "preset",
                        PRESET_LABELS[PRESETS.iter().position(|p| *p == sheet.preset).unwrap_or(0)]
                            .into(),
                    ),
                ),
                OptRow::Thumb => (
                    "thumb",
                    OptRowUi::toggle("embed thumbnail", sheet.embed_thumbnail),
                ),
                OptRow::Meta => (
                    "meta",
                    OptRowUi::toggle("embed metadata", sheet.embed_metadata),
                ),
                OptRow::Subs => (
                    "subs",
                    OptRowUi::toggle("embed subtitles", sheet.embed_subs),
                ),
                OptRow::OutDir => (
                    "outdir",
                    OptRowUi::plain("output", short_dir(&sheet.output_dir).into()),
                ),
                OptRow::Fallback => (
                    "fallback",
                    OptRowUi::cycler("retry on 403/block", sheet.fallback.label().into()),
                ),
                OptRow::Start => (
                    "start",
                    OptRowUi::plain(
                        "ready",
                        format!("download {} item(s) ↵", sheet.items.len()).into(),
                    ),
                ),
            };
            list = list.child(self.dl_opt_row(index, id, ui, selected, tokens, cx));
        }
        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(self.section_label("OPTIONS", tokens))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(tokens.mute)
                            .child(format!("{} item(s)", sheet.items.len())),
                    ),
            )
            .child(list)
            .into_any_element()
    }

    fn dl_run_step(
        &self,
        sheet: &DlSheet,
        tokens: MusicTokens,
        _cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(run) = sheet.run.as_ref() else {
            return div().into_any_element();
        };
        let mut items = div()
            .id("dl-run-items")
            .flex()
            .flex_col()
            .gap(px(1.0))
            .max_h(px(120.0))
            .overflow_y_scroll()
            .track_scroll(&run.items_scroll);
        for item in &run.items {
            let (glyph, detail): (&'static str, &'static str) = match item.state {
                DlItemState::Queued => ("·", "queued"),
                DlItemState::Active(pass) => ("↓", pass),
                DlItemState::Done => ("✓", "done"),
                DlItemState::Failed => ("✕", "failed"),
            };
            items = items.child(
                div()
                    .min_h(px(24.0))
                    .px(px(8.0))
                    .rounded(px(5.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .when(matches!(item.state, DlItemState::Active(_)), |this| {
                        this.bg(tokens.selected)
                    })
                    .child(
                        div()
                            .w(px(14.0))
                            .flex_none()
                            .text_size(px(11.0))
                            .text_color(if matches!(item.state, DlItemState::Queued) {
                                tokens.faint
                            } else {
                                tokens.ink
                            })
                            .child(glyph),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .text_size(px(11.0))
                            .text_color(if matches!(item.state, DlItemState::Queued) {
                                tokens.mute
                            } else {
                                tokens.ink_2
                            })
                            .child(item.title.clone()),
                    )
                    .child(
                        div()
                            .flex_none()
                            .text_size(px(10.0))
                            .text_color(tokens.faint)
                            .child(detail),
                    ),
            );
        }

        let mut log = div()
            .id("dl-run-log")
            .flex()
            .flex_col()
            .h(px(84.0))
            .overflow_y_scroll()
            .track_scroll(&run.log_scroll)
            .p(px(6.0))
            .rounded(px(6.0))
            .border_1()
            .border_color(tokens.border)
            .bg(tokens.bg)
            .font_family("monospace");
        for line in &run.log {
            log = log.child(
                div()
                    .truncate()
                    .text_size(px(10.0))
                    .text_color(tokens.mute)
                    .child(line.clone()),
            );
        }

        let header = if run.done {
            format!(
                "done — {} ok · {} failed → {}",
                run.items.len() - run.failed,
                run.failed,
                short_dir(&run.output_dir)
            )
        } else {
            format!(
                "downloading {} / {}{}",
                run.finished,
                run.items.len(),
                if run.failed > 0 {
                    format!(" · {} failed", run.failed)
                } else {
                    String::new()
                }
            )
        };

        div()
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .child(self.section_label("RUNNING", tokens))
                    .child(div().flex_1())
                    .child(
                        div()
                            .text_size(px(10.0))
                            .text_color(tokens.mute)
                            .child(header),
                    ),
            )
            .child(items)
            .child(log)
            .into_any_element()
    }

    /// Click on an option row: select it, then run its own action.
    fn dl_activate_opt(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
        let row = self
            .dl_sheet
            .as_ref()
            .and_then(|sheet| dl_option_rows(sheet).get(index).copied());
        if let Some(sheet) = self.dl_sheet.as_mut() {
            sheet.opt_sel = index;
        }
        match row {
            Some(OptRow::Kind) => self.dl_cycle_kind(1, cx),
            Some(OptRow::Preset) => self.dl_cycle_preset(1, cx),
            Some(OptRow::OutDir) => self.dl_pick_output_dir(cx),
            Some(OptRow::Start) => self.dl_run_start(cx),
            Some(row) => self.dl_toggle_option(row, cx),
            None => {}
        }
        cx.notify();
    }

    fn dl_footer(&self, sheet: &DlSheet, tokens: MusicTokens, cx: &mut Context<Self>) -> gpui::Div {
        let status: AnyElement = if let Some(busy) = sheet.busy.clone() {
            div()
                .text_size(px(11.0))
                .text_color(tokens.mute)
                .with_animation(
                    "dl-busy",
                    Animation::new(Duration::from_millis(900))
                        .repeat_synced()
                        .with_easing(bounce(ease_in_out)),
                    |el, delta| el.opacity(0.4 + delta * 0.6),
                )
                .child(busy)
                .into_any_element()
        } else if let Some(notice) = sheet.notice.clone() {
            div()
                .text_size(px(11.0))
                .text_color(tokens.mute)
                .child(notice)
                .into_any_element()
        } else {
            div().h(px(14.0)).into_any_element()
        };

        let busy = sheet.busy.is_some();
        let hints: &[(&'static str, &'static str)] = match sheet.step {
            DlStep::Provider => &[("↑↓", "pick"), ("↵", "choose"), ("esc", "close")],
            DlStep::Query => &[("↵", "search"), ("esc", "close")],
            DlStep::Results => &[
                ("↑↓", "move"),
                ("↵", "toggle"),
                ("→", "continue"),
                ("←", "back"),
                ("esc", "close"),
            ],
            DlStep::Options => &[("↑↓", "move"), ("← →", "adjust"), ("esc", "close")],
            DlStep::Run => &[("esc", "close")],
        };

        let mut buttons = div().flex().items_center().gap(px(6.0));
        match sheet.step {
            DlStep::Query => {
                let query = sheet.query_input.read(cx).content.clone();
                buttons = buttons.child(self.dialog_button(
                    "dl-back",
                    "back",
                    false,
                    false,
                    !busy,
                    tokens,
                    cx,
                    |this, window, cx| this.dl_go(DlStep::Provider, window, cx),
                ));
                buttons = buttons.child(self.dialog_button(
                    "dl-search",
                    "search",
                    true,
                    false,
                    !busy && !query.trim().is_empty(),
                    tokens,
                    cx,
                    |this, _w, cx| this.dl_submit(cx),
                ));
            }
            DlStep::Results => {
                buttons = buttons.child(self.dialog_button(
                    "dl-back",
                    "back",
                    false,
                    false,
                    !busy,
                    tokens,
                    cx,
                    |this, window, cx| this.dl_go(DlStep::Query, window, cx),
                ));
                buttons = buttons.child(self.dialog_button(
                    "dl-continue",
                    format!("continue · {}", sheet.picked.len()),
                    true,
                    false,
                    !busy && !sheet.picked.is_empty(),
                    tokens,
                    cx,
                    |this, _w, cx| this.dl_results_continue(cx),
                ));
            }
            DlStep::Options => {
                let back_step = if sheet.hits.is_empty() {
                    DlStep::Query
                } else {
                    DlStep::Results
                };
                buttons = buttons.child(self.dialog_button(
                    "dl-back",
                    "back",
                    false,
                    false,
                    !busy,
                    tokens,
                    cx,
                    move |this, window, cx| this.dl_go(back_step, window, cx),
                ));
                buttons = buttons.child(self.dialog_button(
                    "dl-start",
                    format!("download {}", sheet.items.len()),
                    true,
                    false,
                    !busy && !sheet.items.is_empty(),
                    tokens,
                    cx,
                    |this, _w, cx| this.dl_run_start(cx),
                ));
            }
            DlStep::Run => {
                let done = sheet.run.as_ref().is_some_and(|run| run.done);
                buttons = buttons.child(self.dialog_button(
                    "dl-close",
                    if done { "done" } else { "close" },
                    done,
                    false,
                    true,
                    tokens,
                    cx,
                    |this, _w, cx| this.close_overlays(cx),
                ));
            }
            DlStep::Provider => {}
        }

        div().flex().flex_col().gap(px(8.0)).child(status).child(
            div()
                .flex()
                .items_center()
                .child(self.hint_row(hints, tokens))
                .child(div().flex_1())
                .child(buttons),
        )
    }
}

#[cfg(test)]
mod tests {
    use gpui::{TestAppContext, px, size};

    use super::*;
    use crate::view::RootView;
    use crate::{DismissOverlay, MenuActivate, MenuDown, OpenDownloader};

    /// The sheet opens on `OpenDownloader`, walks provider → query, and esc
    /// closes it (no yt-dlp needed for the state machine).
    #[gpui::test]
    fn sheet_opens_and_steps(cx: &mut TestAppContext) {
        let window = cx.open_window(size(px(1200.), px(760.)), |window, cx| {
            RootView::new(window, cx)
        });
        cx.run_until_parked();

        window
            .update(cx, |_view, window, cx| {
                window.dispatch_action(Box::new(OpenDownloader), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, window, cx| {
                let sheet = view.dl_sheet.as_ref().expect("sheet open");
                assert!(sheet.step == DlStep::Provider);
                // ↓ then ↵ picks youtube-music.
                window.dispatch_action(Box::new(MenuDown), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, window, cx| {
                assert!(view.dl_sheet.as_ref().unwrap().provider_sel == 1);
                window.dispatch_action(Box::new(MenuActivate), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, _w, _cx| {
                let sheet = view.dl_sheet.as_ref().unwrap();
                assert!(sheet.step == DlStep::Query);
                assert!(sheet.provider == Provider::YoutubeMusic);
            })
            .unwrap();

        window
            .update(cx, |_view, window, cx| {
                window.dispatch_action(Box::new(DismissOverlay), cx);
            })
            .unwrap();
        cx.run_until_parked();

        window
            .update(cx, |view, _w, _cx| assert!(view.dl_sheet.is_none()))
            .unwrap();
    }
}
