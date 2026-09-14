//! Terminal UI — black & white, compact, centered, zero-leak (alternate screen).

use std::collections::VecDeque;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute, queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor, Stylize},
    terminal::{
        BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};

use crate::cava::CavaBridge;
use crate::config::{Accent, AppConfig, CavaStyle, LyricsPos, ToastPos};
use crate::lyrics::{
    LrcLine, active_word_index, active_word_progress, ease_lyric_offset, lyric_window_start,
    sung_letters, sung_word_count,
};
use crate::settings::{self, SettingsAction, SettingsUi};

// ── Palette ─────────────────────────────────────────────────────
pub const WHITE: Color = Color::White;
pub const BRIGHT: Color = Color::Rgb {
    r: 245,
    g: 245,
    b: 245,
};
pub const GRAY: Color = Color::Rgb {
    r: 140,
    g: 140,
    b: 140,
};
pub const DIM: Color = Color::Rgb {
    r: 80,
    g: 80,
    b: 80,
};
pub const DARK: Color = Color::Rgb {
    r: 48,
    g: 48,
    b: 48,
};
/// Soft cava wash — barely above black, content-area only.
pub const CAVA_DIM: Color = Color::Rgb {
    r: 32,
    g: 32,
    b: 32,
};
pub const CAVA_SOFT: Color = Color::Rgb {
    r: 44,
    g: 44,
    b: 44,
};

pub const APP_NAME: &str = "optionMusic";

/// Toast stack — at most 3 visible (ui.rs `TOAST_MAX`).
/// Oldest is dropped when a 4th arrives; each item expires on its own clock.
pub const TOAST_MAX: usize = 3;
/// Toast lifetime per item.
pub const TOAST_TTL_MS: u64 = 2200;

/// Minimal B&W toast kinds — symbol prefix differs, border accent only on error.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToastKind {
    /// Generic info (`·`).
    Info,
    /// New track (`♪`).
    Track,
    /// Config changed (`✓`).
    Config,
    /// Error — the only kind with a bright border (`!`).
    Error,
}

impl ToastKind {
    pub fn symbol(self) -> &'static str {
        match self {
            Self::Info => "·",
            Self::Track => "♪",
            Self::Config => "✓",
            Self::Error => "!",
        }
    }
}

#[derive(Debug, Clone)]
struct ToastItem {
    text: String,
    kind: ToastKind,
    at: Instant,
}

/// Side-panel slot. The playlist owns the left slot when docked;
/// floating cards pick the free side so they never overlap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PanelSide {
    Left,
    Right,
}

/// Minimum content width kept for the player — side panels dock only
/// when this survives; otherwise they float as overlays.
pub const PLAYER_MIN_W: usize = 34;
/// Terminal columns needed to dock the list: player min + list + margins.
pub const LIST_DOCK_MIN_COLS: usize = PLAYER_MIN_W + LIST_SIDEBAR_W + 4;

/// Clickable region resolved from the last drawn frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum HitTarget {
    /// Seek ratio along the progress bar (0.0 ..= 1.0).
    Progress(f64),
    PlayPause,
    Prev,
    Next,
    /// Mute toggle on the volume label.
    Volume,
    VolumeUp,
    VolumeDown,
    Eq,
    Speed,
    Pitch,
    CavaToggle,
    /// Footer shortcut bar (global click targets).
    Settings,
    Help,
    SeekBack,
    SeekForward,
    Quit,
    /// 1-based playlist jump.
    Jump(usize),
    /// Scroll ratio on the playlist sidebar scrollbar (0.0 ..= 1.0).
    ListScroll(f64),
    None,
}

#[derive(Debug, Clone, Copy, Default)]
struct HitRect {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
}

impl HitRect {
    fn contains(self, col: u16, row: u16) -> bool {
        let h = self.h.max(1);
        row >= self.y
            && row < self.y.saturating_add(h)
            && col >= self.x
            && col < self.x.saturating_add(self.w)
    }

    fn ratio_at(self, col: u16) -> f64 {
        if self.w <= 1 {
            return 0.0;
        }
        let inner = self.w.saturating_sub(1).max(1);
        ((col.saturating_sub(self.x)) as f64 / inner as f64).clamp(0.0, 1.0)
    }

    fn v_ratio_at(self, row: u16) -> f64 {
        if self.h <= 1 {
            return 0.0;
        }
        let inner = self.h.saturating_sub(1).max(1);
        ((row.saturating_sub(self.y)) as f64 / inner as f64).clamp(0.0, 1.0)
    }
}

#[derive(Debug, Default)]
struct HitMap {
    progress: Option<HitRect>,
    play_pause: Option<HitRect>,
    prev: Option<HitRect>,
    next: Option<HitRect>,
    volume: Option<HitRect>,
    volume_up: Option<HitRect>,
    volume_down: Option<HitRect>,
    eq: Option<HitRect>,
    speed: Option<HitRect>,
    pitch: Option<HitRect>,
    cava: Option<HitRect>,
    /// Footer shortcut chips (`space n/p ←→ +/− v c ?`) — global click targets.
    foot: Vec<(HitRect, HitTarget)>,
    /// Whole playlist sidebar (wheel scroll target).
    list_pane: Option<HitRect>,
    /// Vertical scrollbar track.
    list_bar: Option<HitRect>,
    /// Floating help overlay (swallows clicks so the player beneath stays put).
    help_pane: Option<HitRect>,
    /// Docked lyrics body (lyric lines; swallows clicks so the player around it stays put).
    lyrics_pane: Option<HitRect>,
    /// Pinned lyrics header row below the footer (wheel target too).
    lyrics_head: Option<HitRect>,
    /// (hit rect, 1-based track index)
    list: Vec<(HitRect, usize)>,
}

/// Where a playlist scrollbar click landed relative to the thumb.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarClick {
    /// On the thumb — begin a 1:1 drag.
    Thumb,
    /// Above the thumb — page up.
    Above,
    /// Below the thumb — page down.
    Below,
}

/// Max first-visible offset so the window never shows blank rows.
/// Short lists (or empty ones) pin to 0.
fn list_scroll_max_for(total: usize, vis: usize) -> usize {
    total.saturating_sub(vis.max(1))
}

/// Clamp an offset into `0..=max`.
fn clamp_list_offset(total: usize, vis: usize, offset: usize) -> usize {
    offset.min(list_scroll_max_for(total, vis))
}

/// Shift `offset` just enough to include `cursor`; never jumps otherwise.
fn ensure_cursor_visible(offset: usize, cursor: usize, vis: usize) -> usize {
    let vis = vis.max(1);
    if cursor < offset {
        cursor
    } else if cursor >= offset + vis {
        cursor + 1 - vis
    } else {
        offset
    }
}

/// Proportional thumb (y-offset, height) inside a `track_h`-tall track.
/// Never overflows: `thumb_y + thumb_h <= track_h`.
fn list_thumb_geom(track_h: u16, total: usize, vis: usize, scroll: usize) -> (u16, u16) {
    let track_h = track_h.max(1);
    if total == 0 || total <= vis.max(1) {
        return (0, track_h);
    }
    let thumb_h = ((vis as f64 / total as f64) * track_h as f64)
        .round()
        .clamp(1.0, track_h as f64) as u16;
    let max_scroll = list_scroll_max_for(total, vis);
    let thumb_max = track_h.saturating_sub(thumb_h);
    let thumb_y = if max_scroll == 0 {
        0
    } else {
        ((scroll.min(max_scroll) as f64 / max_scroll as f64) * thumb_max as f64).round() as u16
    };
    (thumb_y.min(thumb_max), thumb_h)
}

/// Snapshot of everything the player frame needs to paint.
pub struct FrameState<'a> {
    pub track_name: &'a str,
    pub track_path: &'a str,
    pub index: usize, // 1-based
    pub total: usize,
    pub pos: Duration,
    pub duration: Option<Duration>,
    pub volume: u8,
    pub muted: bool,
    pub speed: f64,
    pub pitch: f64,
    pub eq_label: &'a str,
    pub paused: bool,
    pub stopped: bool,
    /// Loop mode label: `off` · `all` · `one`
    pub loop_label: &'a str,
    /// Subtle sleep countdown (`mm:ss`) for the status line, if armed.
    pub sleep_label: Option<&'a str>,
    /// Smart shuffle indicator for the status line.
    pub smart_shuffle: bool,
    /// Lyrics docked strip: title, synced lines (with word timings),
    /// plain fallback, absolute active index, status, word clock.
    pub lyrics_open: bool,
    pub lyrics_title: &'a str,
    pub lyrics_synced: &'a [LrcLine],
    pub lyrics_plain: &'a [String],
    pub lyrics_active: Option<usize>,
    pub lyrics_status: Option<&'a str>,
    pub lyrics_now: Duration,
    pub list_names: &'a [String],
    pub toast: Option<&'a str>,
}

/// Owns terminal mode for the playback session (alternate screen + raw + hide cursor).
///
/// Alternate screen keeps scrollback clean: the real buffer is restored on leave/Drop.
pub struct SessionUi {
    toasts: VecDeque<ToastItem>,
    show_list: bool,
    show_help: bool,
    /// Sticky side snapshots taken at open time. A panel keeps the side it
    /// opened on until closed + reopened (reopen picks the free side fresh).
    /// While open — including the close-pop — geometry uses the snapshot so
    /// panels never jump live when another panel closes.
    settings_side_at_open: Option<PanelSide>,
    list_side_at_open: Option<PanelSide>,
    help_side_at_open: Option<PanelSide>,
    /// Help card pop timestamps (geometry-only animation, never blocks input).
    help_opened_at: Option<Instant>,
    help_closed_at: Option<Instant>,
    /// List card pop timestamps (same geometry-only pop as `?` / `c`).
    list_opened_at: Option<Instant>,
    list_closed_at: Option<Instant>,
    /// Lyrics docked strip (`y` toggles, `Esc`/`y` hides).
    show_lyrics: bool,
    /// Strip pop timestamps (geometry-only, never blocks input).
    lyrics_opened_at: Option<Instant>,
    lyrics_closed_at: Option<Instant>,
    /// Manual window override (Up/Down/wheel). `None` = auto-follow.
    lyrics_manual: Option<usize>,
    /// Smooth window top (float rows, eased toward target each frame).
    lyrics_smooth: f64,
    /// Last active line (line-change transition clock).
    lyrics_last_active: Option<usize>,
    lyrics_line_since: Option<Instant>,
    /// Track key the lyric window follows (reset on change).
    lyrics_track_key: String,
    /// Active lyric row (terminal y) from the last paint, for click resume.
    lyrics_active_row: Option<u16>,
    /// Path line under the track title (session-persistent; `f` toggles).
    show_path: bool,
    /// When true, Drop skips terminal restore (after explicit leave()).
    detached: bool,
    /// Global clock for ambient motion.
    t0: Instant,
    /// Track identity for title fade-in.
    track_key: String,
    track_since: Instant,
    /// First visible playlist row (0-based).
    list_scroll: usize,
    /// Selected playlist row (0-based cursor) — always kept visible.
    list_cursor: usize,
    /// Visible row count from last draw (for scroll clamping).
    list_visible: usize,
    /// Rows actually painted last frame (post pop-animation clamp, so
    /// paint and input math never disagree during the open/close pop).
    list_vis_eff: usize,
    /// Scrollbar track geometry from the last paint (thumb/drag math).
    list_track_y: u16,
    list_track_h: u16,
    list_thumb_y: u16,
    list_thumb_h: u16,
    /// Total tracks known from last draw (for scroll max).
    list_total: usize,
    /// Last followed track (1-based) for auto-scroll.
    list_follow: usize,
    /// Click targets from the last `draw`.
    hits: HitMap,
    /// Optional cava spectrum background.
    cava: Option<CavaBridge>,
    /// Persistent settings (`~/.option/music/config.toml`).
    config: AppConfig,
    settings: SettingsUi,
    /// Slim download-preview session: no list/settings sidebars.
    preview: bool,
}

impl SessionUi {
    pub fn enter(enable_cava: bool) -> io::Result<Self> {
        Self::enter_inner(enable_cava, false)
    }

    /// Lightweight player for `msc dl` audio preview (no `c` / `l`).
    pub fn enter_preview() -> io::Result<Self> {
        Self::enter_inner(false, true)
    }

    fn enter_inner(enable_cava: bool, preview: bool) -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(
            out,
            EnterAlternateScreen,
            EnableMouseCapture,
            Hide,
            Clear(ClearType::All)
        )?;
        let now = Instant::now();
        let config = AppConfig::load();
        let cava = if enable_cava && !preview {
            CavaBridge::try_start()
        } else {
            None
        };
        Ok(Self {
            toasts: VecDeque::new(),
            show_list: false,
            show_help: false,
            settings_side_at_open: None,
            list_side_at_open: None,
            help_side_at_open: None,
            help_opened_at: None,
            help_closed_at: None,
            list_opened_at: None,
            list_closed_at: None,
            show_lyrics: false,
            lyrics_opened_at: None,
            lyrics_closed_at: None,
            lyrics_manual: None,
            lyrics_smooth: 0.0,
            lyrics_last_active: None,
            lyrics_line_since: None,
            lyrics_track_key: String::new(),
            lyrics_active_row: None,
            show_path: false,
            detached: false,
            t0: now,
            track_key: String::new(),
            track_since: now,
            list_scroll: 0,
            list_cursor: 0,
            list_visible: 8,
            list_vis_eff: 0,
            list_track_y: 0,
            list_track_h: 1,
            list_thumb_y: 0,
            list_thumb_h: 1,
            list_total: 0,
            list_follow: 0,
            hits: HitMap::default(),
            cava,
            config,
            settings: SettingsUi::default(),
            preview,
        })
    }

    pub fn config(&self) -> &AppConfig {
        &self.config
    }

    /// Mutable access for session persistence (prefs/resume save on quit).
    pub fn config_mut(&mut self) -> &mut AppConfig {
        &mut self.config
    }

    pub fn ldm(&self) -> bool {
        self.config.ldm
    }

    pub fn volume_max(&self) -> u8 {
        self.config.volume_max()
    }

    pub fn settings_open(&self) -> bool {
        self.settings.is_open()
    }

    /// True when the open settings card consumes this key itself.
    /// Anything else (v/l/n/p/…) must fall through to global shortcuts.
    pub fn settings_wants_key(&self, code: crossterm::event::KeyCode) -> bool {
        self.settings.wants_key(code)
    }

    pub fn toggle_settings(&mut self) {
        if self.preview {
            return;
        }
        if self.settings.is_open() {
            self.settings.toggle();
        } else {
            // Snapshot the free side at open; sticky while open.
            let side = self.free_side_for_settings();
            self.settings_side_at_open = Some(side);
            self.settings.toggle();
        }
    }

    pub fn close_settings(&mut self) {
        self.settings.close();
    }

    pub fn is_preview(&self) -> bool {
        self.preview
    }

    /// Handle a key while the settings bar is open.
    pub fn handle_settings_key(&mut self, code: crossterm::event::KeyCode) -> SettingsAction {
        self.settings.handle_key(code, &mut self.config)
    }

    /// Click while the settings bar is open.
    pub fn handle_settings_click(&mut self, col: u16, row: u16) -> SettingsAction {
        self.settings.handle_click(col, row, &mut self.config)
    }

    pub fn pointer_over_settings(&self, col: u16, row: u16) -> bool {
        self.settings.pointer_over_pane(col, row)
    }

    /// Restore the real terminal and detach Drop cleanup.
    pub fn leave(mut self) -> io::Result<()> {
        if let Some(ref mut c) = self.cava {
            c.stop();
        }
        self.cava = None;
        self.restore()?;
        self.detached = true;
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut out = io::stdout();
        execute!(
            out,
            DisableMouseCapture,
            Show,
            LeaveAlternateScreen,
            ResetColor
        )?;
        disable_raw_mode()?;
        Ok(())
    }

    /// Push a generic info toast (`·`).
    pub fn toast(&mut self, msg: impl Into<String>) {
        self.push_toast(msg.into(), ToastKind::Info);
    }

    pub fn toast_info(&mut self, msg: impl Into<String>) {
        self.push_toast(msg.into(), ToastKind::Info);
    }

    /// New-track toast (`♪`).
    pub fn toast_track(&mut self, msg: impl Into<String>) {
        self.push_toast(msg.into(), ToastKind::Track);
    }

    /// Config-changed toast (`✓`).
    pub fn toast_config(&mut self, msg: impl Into<String>) {
        self.push_toast(msg.into(), ToastKind::Config);
    }

    /// Error toast (`!` + bright border) — the only accented kind.
    pub fn toast_error(&mut self, msg: impl Into<String>) {
        self.push_toast(msg.into(), ToastKind::Error);
    }

    fn push_toast(&mut self, msg: String, kind: ToastKind) {
        // Stacked toasts off (default): newest replaces — only 1 visible.
        // Stacked on: newest last, overflow drops oldest, at most TOAST_MAX.
        if !self.config.toast_stack {
            self.toasts.clear();
        }
        self.toasts.push_back(ToastItem {
            text: msg,
            kind,
            at: Instant::now(),
        });
        let max = if self.config.toast_stack {
            TOAST_MAX
        } else {
            1
        };
        while self.toasts.len() > max {
            self.toasts.pop_front();
        }
    }

    /// True while the `?` help card would collide: both side slots are
    /// taken (list on the left + settings card on the right). Callers
    /// refuse `?` with an info toast instead of opening.
    pub fn help_would_collide(&self) -> bool {
        self.show_list && self.settings.is_open()
    }

    /// Try to toggle help. Returns `false` when refused (both slots busy)
    /// — the caller should toast `no room — close a panel`.
    pub fn try_toggle_help(&mut self) -> bool {
        if !self.show_help && self.help_would_collide() {
            return false;
        }
        self.toggle_help();
        true
    }

    /// True while the `l` list card would collide: both side slots are
    /// taken (settings card + help card open). Callers refuse `l` with an
    /// info toast instead of opening — same slot policy as `?`.
    pub fn list_would_collide(&self) -> bool {
        self.settings.is_open() && self.show_help
    }

    /// Try to toggle the list. Returns `false` when refused (no free side)
    /// — the caller should toast `no room — close a panel`.
    /// Closing always succeeds.
    pub fn try_toggle_list(&mut self) -> bool {
        if !self.show_list && self.list_would_collide() {
            return false;
        }
        self.toggle_list();
        true
    }

    /// Whether the list takes layout space at this width. Below
    /// `LIST_DOCK_MIN_COLS` it floats as an overlay card instead so the
    /// player never squeezes under `PLAYER_MIN_W`.
    pub fn list_docked(&self, cols: usize) -> bool {
        self.show_list && cols >= LIST_DOCK_MIN_COLS
    }

    /// Card side for the settings panel: sticky snapshot taken at open
    /// (free side then: opposite the list, else opposite help, else left).
    /// While open — including the close-pop — the snapshot wins so the card
    /// never jumps live when another panel closes. Next open re-picks.
    pub fn settings_side(&self) -> PanelSide {
        if let Some(s) = self.settings_side_at_open {
            // Keep the snapshot while the card is open or still popping out.
            if self.settings.is_open() || self.settings.anim_progress(self.config.ldm) > 0.02 {
                return s;
            }
        }
        self.free_side_for_settings()
    }

    /// Free side for settings at open time: opposite the open list, else
    /// opposite open help, else the default left.
    fn free_side_for_settings(&self) -> PanelSide {
        if self.show_list {
            match self.list_side_at_open {
                Some(PanelSide::Left) | None => PanelSide::Right,
                Some(PanelSide::Right) => PanelSide::Left,
            }
        } else if self.show_help {
            match self.help_side_at_open {
                Some(PanelSide::Right) | None => PanelSide::Left,
                Some(PanelSide::Left) => PanelSide::Right,
            }
        } else {
            PanelSide::Left
        }
    }

    /// Card side for the playlist: sticky snapshot (default left). A second
    /// opener takes the free side so it never overlaps the settings card.
    pub fn list_side(&self) -> PanelSide {
        if let Some(s) = self.list_side_at_open {
            if self.show_list || self.list_progress(self.config.ldm) > 0.02 {
                return s;
            }
        }
        self.free_side_for_list()
    }

    fn free_side_for_list(&self) -> PanelSide {
        if self.settings.is_open() {
            match self.settings_side_at_open {
                Some(PanelSide::Left) | None => PanelSide::Right,
                Some(PanelSide::Right) => PanelSide::Left,
            }
        } else {
            PanelSide::Left
        }
    }

    /// Card side for help: sticky snapshot (default right). Picks the free
    /// side at open so it never overlaps the settings card.
    pub fn help_side(&self) -> PanelSide {
        if let Some(s) = self.help_side_at_open {
            if self.show_help || self.help_progress(self.config.ldm) > 0.02 {
                return s;
            }
        }
        self.free_side_for_help()
    }

    fn free_side_for_help(&self) -> PanelSide {
        if self.settings.is_open() {
            match self
                .settings_side_at_open
                .unwrap_or(self.free_side_for_settings())
            {
                PanelSide::Right => PanelSide::Left,
                PanelSide::Left => PanelSide::Right,
            }
        } else {
            PanelSide::Right
        }
    }

    pub fn toggle_list(&mut self) {
        if self.preview {
            return;
        }
        if !self.show_list {
            // Snapshot the free side at open; sticky while open.
            self.list_side_at_open = Some(self.free_side_for_list());
        }
        self.show_list = !self.show_list;
        let now = Instant::now();
        if self.show_list {
            self.list_follow = 0; // recenter on open
            self.list_opened_at = Some(now);
            self.list_closed_at = None;
        } else if self.hits.list_pane.is_some() || self.list_opened_at.is_some() {
            self.list_closed_at = Some(now);
        }
    }

    /// List card pop progress 0..=1 — geometry only, never blocks input.
    /// Open: ~140ms ease-out. Close: ~120ms shrink. LDM: instant.
    /// Same timing as the `?` / `c` cards.
    fn list_progress(&self, ldm: bool) -> f64 {
        if self.show_list {
            match self.list_opened_at {
                Some(t) if !ldm => ease_out_cubic(t.elapsed().as_secs_f64() / 0.14),
                _ => 1.0,
            }
        } else if ldm {
            0.0
        } else {
            match self.list_closed_at {
                Some(t) => {
                    let e = t.elapsed().as_secs_f64() / 0.12;
                    if e >= 1.0 {
                        0.0
                    } else {
                        1.0 - ease_out_cubic(e)
                    }
                }
                None => 0.0,
            }
        }
    }

    pub fn toggle_help(&mut self) {
        if !self.show_help {
            // Snapshot the free side at open; sticky while open.
            self.help_side_at_open = Some(self.free_side_for_help());
        }
        self.show_help = !self.show_help;
        let now = Instant::now();
        if self.show_help {
            self.help_opened_at = Some(now);
            self.help_closed_at = None;
        } else if self.hits.help_pane.is_some() || self.help_opened_at.is_some() {
            self.help_closed_at = Some(now);
        }
    }

    /// Help card pop progress 0..=1 — geometry only, never blocks input.
    /// Open: ~140ms ease-out. Close: ~120ms shrink. LDM: instant.
    fn help_progress(&self, ldm: bool) -> f64 {
        if self.show_help {
            match self.help_opened_at {
                Some(t) if !ldm => ease_out_cubic(t.elapsed().as_secs_f64() / 0.14),
                _ => 1.0,
            }
        } else if ldm {
            0.0
        } else {
            match self.help_closed_at {
                Some(t) => {
                    let e = t.elapsed().as_secs_f64() / 0.12;
                    if e >= 1.0 {
                        0.0
                    } else {
                        1.0 - ease_out_cubic(e)
                    }
                }
                None => 0.0,
            }
        }
    }

    /// Click inside the top help card (swallowed, never leaks through).
    pub fn help_contains(&self, col: u16, row: u16) -> bool {
        self.hits
            .help_pane
            .map(|r| r.contains(col, row))
            .unwrap_or(false)
    }

    /// Toggle the filename/path line. Returns `true` when shown.
    pub fn toggle_path(&mut self) -> bool {
        self.show_path = !self.show_path;
        self.show_path
    }

    /// Docked lyrics strip (`y` toggles, `Esc`/`y` hides).
    /// Hidden config re-opens below so `y` never dead-ends.
    pub fn toggle_lyrics(&mut self) {
        if self.show_lyrics {
            self.close_lyrics();
        } else {
            if self.config.lyrics_pos == LyricsPos::Hidden {
                self.config.lyrics_pos = LyricsPos::Below;
                let _ = self.config.save();
            }
            self.show_lyrics = true;
            let now = Instant::now();
            self.lyrics_opened_at = Some(now);
            self.lyrics_closed_at = None;
            self.lyrics_manual = None;
        }
    }

    pub fn close_lyrics(&mut self) {
        if self.show_lyrics {
            self.lyrics_closed_at = Some(Instant::now());
        }
        self.show_lyrics = false;
    }

    pub fn lyrics_open(&self) -> bool {
        self.show_lyrics
    }

    /// Strip visible this frame (open + positioned + pop alive).
    pub fn lyrics_visible(&self) -> bool {
        if self.config.lyrics_pos == LyricsPos::Hidden {
            return false;
        }
        self.show_lyrics || self.lyrics_progress(self.config.ldm) > 0.02
    }

    /// Strip pop progress 0..=1 — geometry only, never blocks input.
    /// Open: ~140ms ease-out. Close: ~120ms shrink. LDM: instant.
    /// Same timing as the `?` / `c` / `l` cards (<200ms).
    pub fn lyrics_progress(&self, ldm: bool) -> f64 {
        if self.show_lyrics {
            match self.lyrics_opened_at {
                Some(t) if !ldm => ease_out_cubic(t.elapsed().as_secs_f64() / 0.14),
                _ => 1.0,
            }
        } else if ldm {
            0.0
        } else {
            match self.lyrics_closed_at {
                Some(t) => {
                    let e = t.elapsed().as_secs_f64() / 0.12;
                    if e >= 1.0 {
                        0.0
                    } else {
                        1.0 - ease_out_cubic(e)
                    }
                }
                None => 0.0,
            }
        }
    }

    /// Line-change transition 0..=1 (~120ms slide/fade). LDM: instant.
    fn lyrics_line_progress(&self, ldm: bool) -> f64 {
        if ldm {
            return 1.0;
        }
        match self.lyrics_line_since {
            Some(t) => ease_out_cubic(t.elapsed().as_secs_f64() / 0.12),
            None => 1.0,
        }
    }

    /// Manual scroll of the lyric window (Up/Down/wheel). Sets a manual
    /// override so auto-follow pauses until track change or resume.
    pub fn lyrics_scroll_by(&mut self, delta: i32, total: usize, shown: usize) {
        if total <= shown.max(1) || delta == 0 {
            return;
        }
        let max = total.saturating_sub(shown.max(1));
        let cur = self
            .lyrics_manual
            .unwrap_or_else(|| self.lyrics_smooth.round() as usize);
        let next = (cur as i32 + delta).clamp(0, max as i32) as usize;
        self.lyrics_manual = Some(next);
    }

    /// Back to auto-follow (track change, or pressing the active line).
    pub fn lyrics_resume_follow(&mut self) {
        self.lyrics_manual = None;
    }

    /// Reset follow state on track change (auto-follow resumes).
    fn note_lyrics_track(&mut self, key: &str, active: Option<usize>) {
        if key != self.lyrics_track_key {
            self.lyrics_track_key = key.to_string();
            self.lyrics_manual = None;
            self.lyrics_smooth = 0.0;
            self.lyrics_last_active = active;
            self.lyrics_line_since = Some(Instant::now());
        } else if self.lyrics_last_active != active {
            self.lyrics_last_active = active;
            self.lyrics_line_since = Some(Instant::now());
        }
    }

    /// Click inside the docked lyrics (wheel target + active-line resume).
    /// Covers both the lyric lines (wherever `lyrics_pos` puts them) and
    /// the pinned header row below the footer.
    pub fn lyrics_contains(&self, col: u16, row: u16) -> bool {
        if self.show_lyrics {
            if self
                .hits
                .lyrics_pane
                .map(|r| r.contains(col, row))
                .unwrap_or(false)
            {
                return true;
            }
            if self
                .hits
                .lyrics_head
                .map(|r| r.contains(col, row))
                .unwrap_or(false)
            {
                return true;
            }
        }
        false
    }

    /// True when the click landed on the active lyric row (resume follow).
    pub fn lyrics_hit_active_row(&self, row: u16) -> bool {
        self.lyrics_active_row.map(|r| r == row).unwrap_or(false)
    }

    /// Pinned lyrics header row — always painted BELOW the footer chip row,
    /// whatever `lyrics_pos` says. Dim label + manual/follow hint, same B&W
    /// style. Returns the next free `y`.
    fn paint_lyrics_header(
        &mut self,
        out: &mut impl Write,
        y: usize,
        content_x0: usize,
        content_cols: usize,
        block_w: usize,
    ) -> io::Result<usize> {
        let manual = self.lyrics_manual.is_some();
        let head = if manual {
            "lyrics · scrolled · enter follow".to_string()
        } else {
            "lyrics · y hide".to_string()
        };
        let cx0 = content_x0 + content_cols.saturating_sub(block_w) / 2;
        let hw = head.chars().count().min(block_w);
        let hx = cx0 + block_w.saturating_sub(hw) / 2;
        queue!(
            out,
            MoveTo(hx as u16, y as u16),
            SetForegroundColor(DIM),
            Print(truncate(&head, block_w)),
            ResetColor
        )?;
        let strip_w = block_w.min(content_cols).max(1);
        let strip_x = cx0.min(content_x0 + content_cols.saturating_sub(1));
        self.hits.lyrics_head = Some(HitRect {
            x: strip_x as u16,
            y: y as u16,
            w: strip_w as u16,
            h: 1,
        });
        Ok(y + 1)
    }

    /// Docked lyric LINES painter (no header). Rows are centered in the
    /// player block (`block_w` wide); returns the next free `y`. `body_h`
    /// is the reserved body height (inner gaps + lyric rows, 0..=5); fixed
    /// so the player reflows without jumps. Karaoke motion is geometry-only:
    /// active-line slide-in + upcoming fade-up, frozen pulse rules kept
    /// (no ambient shimmer here). LDM: instant open, stepped highlight.
    #[allow(clippy::too_many_arguments)]
    fn paint_lyrics_body(
        &mut self,
        out: &mut impl Write,
        mut y: usize,
        content_x0: usize,
        content_cols: usize,
        block_w: usize,
        state: &FrameState<'_>,
        shown: usize,
        body_h: usize,
        line_prog: f64,
        ldm: bool,
    ) -> io::Result<usize> {
        let rows_avail = body_h;
        let cx0 = content_x0 + content_cols.saturating_sub(block_w) / 2;
        let strip_y0 = y;
        // Inner breathing room above the lyric lines (was the 2 gaps
        // between header and lines when the strip was one block).
        let gap_rows = rows_avail.min(2);
        y += gap_rows;
        let body_rows = rows_avail.saturating_sub(gap_rows).min(shown);
        let synced = state.lyrics_synced;
        let plain = state.lyrics_plain;
        let total = if synced.is_empty() {
            plain.len()
        } else {
            synced.len()
        };
        // Smooth window: eased float from draw bookkeeping.
        let start_f = self
            .lyrics_smooth
            .clamp(0.0, total.saturating_sub(body_rows.max(1)) as f64);
        let start = (start_f.round() as usize)
            .min(total.saturating_sub(body_rows.max(1).min(total.max(1))));
        let start = if total <= body_rows { 0 } else { start };
        // When idle before the first line, hold the head of the song.
        let start = if state.lyrics_active.is_none()
            && self.lyrics_manual.is_none()
            && !synced.is_empty()
        {
            0
        } else {
            start
        };
        self.lyrics_active_row = None;
        let y0_body = y;
        for i in 0..body_rows {
            let idx = start + i;
            let row_y = y0_body + i;
            if !synced.is_empty() {
                if idx >= synced.len() {
                    break;
                }
                let line = &synced[idx];
                let is_active = Some(idx) == state.lyrics_active;
                // Upcoming lines fade up: next line GRAY, deeper DIM.
                // Past lines rest at GRAY so the sung history stays readable.
                let base = if is_active {
                    BRIGHT
                } else if let Some(a) = state.lyrics_active {
                    if idx < a {
                        GRAY
                    } else if idx == a + 1 {
                        GRAY
                    } else {
                        DIM
                    }
                } else if idx == start {
                    GRAY
                } else {
                    DIM
                };
                // Slide-in on line change (~120ms, geometry only).
                let mut slide = 0usize;
                if is_active && !ldm && line_prog < 1.0 {
                    slide = ((1.0 - line_prog) * 2.0).round() as usize;
                }
                if is_active && !line.words.is_empty() {
                    let sung = sung_word_count(line, state.lyrics_now);
                    let active_w = active_word_index(line, state.lyrics_now);
                    // Next line start bounds the final word sweep.
                    let next_start = synced.get(idx + 1).map(|n| n.time);
                    // LDM degrades to per-word steps (no intra-word split).
                    let active_frac = if ldm {
                        None
                    } else {
                        active_w.and_then(|aw| {
                            active_word_progress(line, state.lyrics_now, next_start)
                                .filter(|(i, _)| *i == aw)
                                .map(|(_, f)| f)
                        })
                    };
                    let full: String = line
                        .words
                        .iter()
                        .map(|w| w.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" ");
                    let full_w = full.chars().count().min(block_w);
                    let mut wx = cx0 + block_w.saturating_sub(full_w) / 2 + slide;
                    if is_active {
                        self.lyrics_active_row = Some(row_y as u16);
                    }
                    for (wi, w) in line.words.iter().enumerate() {
                        let is_sung = wi < sung;
                        let is_cur = Some(wi) == active_w;
                        if wi > 0 {
                            queue!(
                                out,
                                MoveTo(wx as u16, row_y as u16),
                                SetForegroundColor(DIM),
                                Print(" "),
                                ResetColor
                            )?;
                            wx += 1;
                        }
                        if is_cur && !ldm {
                            // Live per-letter sweep: sung letters BRIGHT,
                            // current letter invert, rest DIM — same row,
                            // precomputed widths so no layout jitter.
                            let chars: Vec<char> = w.text.chars().collect();
                            let n = chars.len();
                            let done = sung_letters(n, active_frac.unwrap_or(0.0)).min(n);
                            let sung_s: String = chars[..done].iter().collect();
                            let cur_s: String = if done < n {
                                chars[done].to_string()
                            } else {
                                String::new()
                            };
                            let rest_s: String = if done + 1 <= n {
                                chars.iter().skip(done + 1).collect()
                            } else {
                                String::new()
                            };
                            if !sung_s.is_empty() {
                                queue!(
                                    out,
                                    MoveTo(wx as u16, row_y as u16),
                                    SetForegroundColor(BRIGHT),
                                    Print(&sung_s),
                                    ResetColor
                                )?;
                                wx += sung_s.chars().count();
                            }
                            if !cur_s.is_empty() {
                                queue!(
                                    out,
                                    MoveTo(wx as u16, row_y as u16),
                                    SetBackgroundColor(BRIGHT),
                                    SetForegroundColor(Color::Black),
                                    Print(&cur_s),
                                    ResetColor
                                )?;
                                wx += 1;
                            }
                            if !rest_s.is_empty() {
                                queue!(
                                    out,
                                    MoveTo(wx as u16, row_y as u16),
                                    SetForegroundColor(DIM),
                                    Print(&rest_s),
                                    ResetColor
                                )?;
                                wx += rest_s.chars().count();
                            }
                        } else {
                            let wt = truncate(&w.text, block_w);
                            if is_cur {
                                // LDM stepped highlight: bright, no invert.
                                queue!(
                                    out,
                                    MoveTo(wx as u16, row_y as u16),
                                    SetForegroundColor(BRIGHT),
                                    Print(&wt),
                                    ResetColor
                                )?;
                            } else if is_sung {
                                queue!(
                                    out,
                                    MoveTo(wx as u16, row_y as u16),
                                    SetForegroundColor(BRIGHT),
                                    Print(&wt),
                                    ResetColor
                                )?;
                            } else {
                                queue!(
                                    out,
                                    MoveTo(wx as u16, row_y as u16),
                                    SetForegroundColor(DIM),
                                    Print(&wt),
                                    ResetColor
                                )?;
                            }
                            wx += w.text.chars().count();
                        }
                        let _ = base;
                    }
                } else {
                    let marker = if is_active { "› " } else { "  " };
                    let text = format!(
                        "{marker}{}",
                        truncate(&line.text, block_w.saturating_sub(2))
                    );
                    let tw = text.chars().count().min(block_w);
                    let tx = cx0 + block_w.saturating_sub(tw) / 2 + slide;
                    if is_active {
                        self.lyrics_active_row = Some(row_y as u16);
                    }
                    queue!(
                        out,
                        MoveTo(tx as u16, row_y as u16),
                        SetForegroundColor(base),
                        Print(truncate(&text, block_w)),
                        ResetColor
                    )?;
                }
            } else if !plain.is_empty() {
                // Plain (untimed) lyrics: static scrollable list, same strip.
                if idx >= plain.len() {
                    break;
                }
                let text = format!("  {}", truncate(&plain[idx], block_w.saturating_sub(2)));
                let tw = text.chars().count().min(block_w);
                let tx = cx0 + block_w.saturating_sub(tw) / 2;
                queue!(
                    out,
                    MoveTo(tx as u16, row_y as u16),
                    SetForegroundColor(GRAY),
                    Print(truncate(&text, block_w)),
                    ResetColor
                )?;
            } else {
                // `no lyrics found` dim status stays in the same strip.
                if i == 0 {
                    let msg = truncate(state.lyrics_status.unwrap_or("no lyrics found"), block_w);
                    let mw = msg.chars().count().min(block_w);
                    let mx = cx0 + block_w.saturating_sub(mw) / 2;
                    queue!(
                        out,
                        MoveTo(mx as u16, row_y as u16),
                        SetForegroundColor(DIM),
                        Print(&msg),
                        ResetColor
                    )?;
                }
            }
        }
        y = y0_body + body_rows;
        // Hit rect covers the whole strip (wheel target; active-row resume).
        let strip_w = block_w.min(content_cols).max(1);
        let strip_x = cx0.min(content_x0 + content_cols.saturating_sub(1));
        self.hits.lyrics_pane = Some(HitRect {
            x: strip_x as u16,
            y: strip_y0 as u16,
            w: strip_w as u16,
            h: (y.saturating_sub(strip_y0)).max(1) as u16,
        });
        Ok(y)
    }

    /// Toggle cava background (no-op toast if binary missing).
    pub fn toggle_cava(&mut self) -> &'static str {
        if self.cava.is_some() {
            if let Some(mut c) = self.cava.take() {
                c.stop();
            }
            "cava off"
        } else {
            match CavaBridge::try_start() {
                Some(c) => {
                    self.cava = Some(c);
                    "cava on"
                }
                None => "cava unavailable",
            }
        }
    }

    /// Restart cava if it's running (style/height changed live).
    pub fn refresh_cava_if_active(&mut self) {
        if self.cava.is_some() {
            if let Some(mut c) = self.cava.take() {
                c.stop();
            }
            self.cava = CavaBridge::try_start();
        }
    }

    pub fn cava_active(&self) -> bool {
        self.cava.is_some()
    }

    pub fn show_list(&self) -> bool {
        self.show_list
    }

    pub fn show_help(&self) -> bool {
        self.show_help
    }

    /// True while the playlist sidebar is open.
    pub fn list_panel_active(&self) -> bool {
        self.show_list
    }

    /// Selected playlist row (0-based cursor). Always kept inside the
    /// visible window — cursor moves pull the window, view scrolls pull
    /// the cursor, so the selection is never stranded off-screen.
    pub fn list_cursor(&self) -> usize {
        self.list_cursor
    }

    /// Current first-visible-row offset.
    pub fn list_offset(&self) -> usize {
        self.list_scroll
    }

    /// Rows to use for input math: what was actually painted last frame
    /// (post pop-animation clamp), falling back to the draw-time cap
    /// before the first paint. Keeps paint and input in agreement so no
    /// blank rows or jumps appear mid-pop.
    fn list_vis_for_input(&self) -> usize {
        let stored = self.list_visible.max(1);
        if self.list_vis_eff == 0 {
            stored
        } else {
            self.list_vis_eff.min(stored).max(1)
        }
    }

    /// View scroll (wheel / trackpad): moves the window, then pulls the
    /// cursor back inside it so the selection stays visible.
    pub fn list_scroll_by(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        let vis = self.list_vis_for_input();
        let max = list_scroll_max_for(self.list_total, vis);
        if delta < 0 {
            self.list_scroll = self.list_scroll.saturating_sub((-delta) as usize);
        } else {
            self.list_scroll = (self.list_scroll + delta as usize).min(max);
        }
        self.clamp_cursor_to_view();
    }

    /// Jump scroll from scrollbar ratio (0 = top, 1 = bottom).
    /// Home/End use this; the cursor is pulled into view afterwards.
    pub fn list_scroll_ratio(&mut self, ratio: f64) {
        let vis = self.list_vis_for_input();
        let max = list_scroll_max_for(self.list_total, vis);
        self.list_scroll = ((ratio.clamp(0.0, 1.0) * max as f64).round() as usize).min(max);
        self.clamp_cursor_to_view();
    }

    /// Move the cursor by `delta` rows (keyboard ±1); the window follows.
    pub fn list_move_cursor(&mut self, delta: i32) {
        if self.list_total == 0 || delta == 0 {
            return;
        }
        let last = self.list_total - 1;
        if delta < 0 {
            self.list_cursor = self.list_cursor.saturating_sub((-delta) as usize);
        } else {
            self.list_cursor = (self.list_cursor + delta as usize).min(last);
        }
        let vis = self.list_vis_for_input();
        self.list_scroll = ensure_cursor_visible(self.list_scroll, self.list_cursor, vis);
        self.list_scroll = clamp_list_offset(self.list_total, vis, self.list_scroll);
    }

    /// Page the cursor by whole windows (PgUp/PgDn, scrollbar track).
    pub fn list_page_by(&mut self, pages: i32) {
        if pages == 0 {
            return;
        }
        let step = self.list_vis_for_input() as i32;
        self.list_move_cursor(pages.saturating_mul(step));
    }

    /// Cursor to the first row (Home).
    pub fn list_cursor_home(&mut self) {
        if self.list_total == 0 {
            return;
        }
        self.list_cursor = 0;
        self.list_scroll = 0;
    }

    /// Cursor to the last row (End).
    pub fn list_cursor_end(&mut self) {
        if self.list_total == 0 {
            return;
        }
        self.list_cursor = self.list_total - 1;
        let vis = self.list_vis_for_input();
        self.list_scroll = list_scroll_max_for(self.list_total, vis);
    }

    /// Click selected row `idx0`: cursor jumps there, window follows.
    pub fn list_set_cursor(&mut self, idx0: usize) {
        if self.list_total == 0 {
            return;
        }
        self.list_cursor = idx0.min(self.list_total - 1);
        let vis = self.list_vis_for_input();
        self.list_scroll = ensure_cursor_visible(self.list_scroll, self.list_cursor, vis);
        self.list_scroll = clamp_list_offset(self.list_total, vis, self.list_scroll);
    }

    /// Relative 1:1 thumb drag: one terminal row moved = one playlist
    /// row scrolled. The cursor is pulled into view afterwards so the
    /// selection stays visible while dragging.
    pub fn list_drag_to(&mut self, start_row: u16, start_scroll: usize, row: u16) {
        let vis = self.list_vis_for_input();
        let max = list_scroll_max_for(self.list_total, vis);
        let delta = row as i32 - start_row as i32;
        self.list_scroll = (start_scroll as i32 + delta).clamp(0, max as i32) as usize;
        self.clamp_cursor_to_view();
    }

    /// Classify a scrollbar click by row: on the thumb (drag) vs above
    /// / below it (page). Uses the geometry from the last paint.
    pub fn list_bar_click(&self, row: u16) -> BarClick {
        let rel = row.saturating_sub(self.list_track_y);
        if rel < self.list_thumb_y {
            BarClick::Above
        } else if rel >= self.list_thumb_y.saturating_add(self.list_thumb_h.max(1)) {
            BarClick::Below
        } else {
            BarClick::Thumb
        }
    }

    /// Pull the cursor inside `[offset, offset + vis)` and the playlist.
    fn clamp_cursor_to_view(&mut self) {
        if self.list_total == 0 {
            self.list_cursor = 0;
            self.list_scroll = 0;
            return;
        }
        let last = self.list_total - 1;
        self.list_cursor = self.list_cursor.min(last);
        let lo = self.list_scroll.min(last);
        let hi = (self.list_scroll + self.list_vis_for_input()).saturating_sub(1);
        if self.list_cursor < lo {
            self.list_cursor = lo;
        } else if self.list_cursor > hi.min(last) {
            self.list_cursor = hi.min(last);
        }
    }

    pub fn pointer_over_list(&self, col: u16, row: u16) -> bool {
        self.hits
            .list_pane
            .map(|r| r.contains(col, row))
            .unwrap_or(false)
    }

    /// Keep the current track in view; recenter when the track changes.
    /// On change the cursor follows playback (soft-centered window).
    /// Otherwise the cursor owns the window: it is only ensured visible
    /// (e.g. after a resize) and manual scrolls never snap back.
    fn follow_list_track(&mut self, index_1based: usize) {
        let total = self.list_total;
        let vis = self.list_visible.max(1);
        if index_1based == 0 || total == 0 {
            self.list_scroll = 0;
            self.list_cursor = 0;
            self.list_follow = index_1based;
            return;
        }
        let i0 = (index_1based - 1).min(total - 1);
        if self.list_follow != index_1based {
            self.list_follow = index_1based;
            // Soft-center on track change / open; cursor follows playback.
            self.list_cursor = i0;
            self.list_scroll = clamp_list_offset(total, vis, i0.saturating_sub(vis / 3));
        } else {
            // Same track: cursor owns the window — just keep it visible,
            // never drag playback back into view.
            self.list_cursor = self.list_cursor.min(total - 1);
            self.list_scroll = ensure_cursor_visible(self.list_scroll, self.list_cursor, vis);
            self.list_scroll = clamp_list_offset(total, vis, self.list_scroll);
        }
    }

    /// Resolve a mouse position against the last drawn frame.
    pub fn hit_target(&self, col: u16, row: u16) -> HitTarget {
        // Floating help overlay absorbs clicks so they never leak through
        // to the player controls painted underneath.
        if let Some(r) = self.hits.help_pane {
            if r.contains(col, row) {
                return HitTarget::None;
            }
        }
        // Docked lyrics (lines + pinned header) absorb clicks so they
        // never leak through to the player controls painted around them.
        if let Some(r) = self.hits.lyrics_pane {
            if r.contains(col, row) {
                return HitTarget::None;
            }
        }
        if let Some(r) = self.hits.lyrics_head {
            if r.contains(col, row) {
                return HitTarget::None;
            }
        }
        // Playlist scrollbar absorbs hits before track rows.
        if let Some(r) = self.hits.list_bar {
            if r.contains(col, row) {
                return HitTarget::ListScroll(r.v_ratio_at(row));
            }
        }
        for (r, idx) in &self.hits.list {
            if r.contains(col, row) {
                return HitTarget::Jump(*idx);
            }
        }
        if self.pointer_over_list(col, row) {
            return HitTarget::None;
        }
        // Footer shortcut bar — global, same actions as the keys.
        for (r, t) in &self.hits.foot {
            if r.contains(col, row) {
                return *t;
            }
        }
        if let Some(r) = self.hits.progress {
            if r.contains(col, row) {
                return HitTarget::Progress(r.ratio_at(col));
            }
        }
        if let Some(r) = self.hits.prev {
            if r.contains(col, row) {
                return HitTarget::Prev;
            }
        }
        if let Some(r) = self.hits.next {
            if r.contains(col, row) {
                return HitTarget::Next;
            }
        }
        if let Some(r) = self.hits.play_pause {
            if r.contains(col, row) {
                return HitTarget::PlayPause;
            }
        }
        if let Some(r) = self.hits.volume_down {
            if r.contains(col, row) {
                return HitTarget::VolumeDown;
            }
        }
        if let Some(r) = self.hits.volume_up {
            if r.contains(col, row) {
                return HitTarget::VolumeUp;
            }
        }
        if let Some(r) = self.hits.volume {
            if r.contains(col, row) {
                return HitTarget::Volume;
            }
        }
        if let Some(r) = self.hits.eq {
            if r.contains(col, row) {
                return HitTarget::Eq;
            }
        }
        if let Some(r) = self.hits.speed {
            if r.contains(col, row) {
                return HitTarget::Speed;
            }
        }
        if let Some(r) = self.hits.pitch {
            if r.contains(col, row) {
                return HitTarget::Pitch;
            }
        }
        if let Some(r) = self.hits.cava {
            if r.contains(col, row) {
                return HitTarget::CavaToggle;
            }
        }
        HitTarget::None
    }

    /// Progress ratio from column while dragging (ignores row).
    pub fn progress_ratio_at_col(&self, col: u16) -> Option<f64> {
        self.hits.progress.map(|r| r.ratio_at(col))
    }

    /// Footer shortcut hit only (ignores player controls).
    /// Used so `? c v …` clicks stay global even with settings open.
    pub fn footer_hit(&self, col: u16, row: u16) -> HitTarget {
        // The floating help card still owns its own area.
        if let Some(r) = self.hits.help_pane {
            if r.contains(col, row) {
                return HitTarget::None;
            }
        }
        // Same for the docked lyrics (lines + pinned header).
        if let Some(r) = self.hits.lyrics_pane {
            if r.contains(col, row) {
                return HitTarget::None;
            }
        }
        if let Some(r) = self.hits.lyrics_head {
            if r.contains(col, row) {
                return HitTarget::None;
            }
        }
        for (r, t) in &self.hits.foot {
            if r.contains(col, row) {
                return *t;
            }
        }
        HitTarget::None
    }

    /// Newest live toast text (compat helper for the frame builder).
    pub fn toast_text(&self) -> Option<&str> {
        self.toasts
            .back()
            .filter(|t| t.at.elapsed() < Duration::from_millis(TOAST_TTL_MS))
            .map(|t| t.text.as_str())
    }

    /// Live stack, oldest → newest, for the painter.
    fn live_toasts(&self) -> Vec<(&str, ToastKind, f64)> {
        self.toasts
            .iter()
            .filter_map(|t| {
                let e = t.at.elapsed();
                if e < Duration::from_millis(TOAST_TTL_MS) {
                    Some((t.text.as_str(), t.kind, e.as_secs_f64()))
                } else {
                    None
                }
            })
            .collect()
    }

    fn expire_toast(&mut self) {
        while let Some(front) = self.toasts.front() {
            if front.at.elapsed() >= Duration::from_millis(TOAST_TTL_MS) {
                self.toasts.pop_front();
            } else {
                break;
            }
        }
        // Non-front items can also age out while a newer one lives.
        self.toasts
            .retain(|t| t.at.elapsed() < Duration::from_millis(TOAST_TTL_MS));
    }

    fn note_track(&mut self, name: &str) {
        if name != self.track_key {
            self.track_key = name.to_string();
            self.track_since = Instant::now();
        }
    }

    /// Full-frame redraw — compact, centered, lightly animated.
    pub fn draw(&mut self, state: &FrameState<'_>) -> io::Result<()> {
        self.expire_toast();
        self.note_track(state.track_name);
        self.hits = HitMap::default();

        let mut out = io::stdout();
        let (cols, rows) = size().unwrap_or((80, 24));
        let cols = cols as usize;
        let rows = rows as usize;

        // Absolute redraw inside alternate screen — never touches scrollback.
        // Synchronized update avoids tear/flicker (esp. with help sidebar).
        queue!(
            out,
            BeginSynchronizedUpdate,
            Clear(ClearType::All),
            MoveTo(0, 0)
        )?;

        let t = self.t0.elapsed().as_secs_f64();
        let playing = !state.paused && !state.stopped;
        let ldm = self.config.ldm;
        let accent = accent_color(&self.config.accent);
        let accent_dim = dim_accent(accent);

        let intro = if ldm {
            1.0
        } else {
            ease_out_cubic((self.t0.elapsed().as_secs_f64() / 0.55).clamp(0.0, 1.0))
        };
        let title_in = if ldm {
            1.0
        } else {
            ease_out_cubic((self.track_since.elapsed().as_secs_f64() / 0.4).clamp(0.0, 1.0))
        };

        // Overlays float above the player — the center block never shifts.
        // (Help used to steal layout width and push the player sideways.)
        let ldm_now = self.config.ldm;
        let settings_prog = if self.preview {
            0.0
        } else {
            self.settings.anim_progress(ldm_now).clamp(0.0, 1.0)
        };
        let help_prog = self.help_progress(ldm_now).clamp(0.0, 1.0);
        let list_prog = self.list_progress(ldm_now).clamp(0.0, 1.0);
        let settings_visual = settings_prog > 0.02;
        let help_visual = help_prog > 0.02;
        let list_visual = list_prog > 0.02;
        if self.preview {
            self.show_list = false;
        }

        // Slot policy: the list floats as a compact card on the left — the
        // center block never shifts and never squeezes under `PLAYER_MIN_W`.
        // Settings/help are always overlays with fixed anchors.
        let content_x0 = 0usize;
        let content_cols = cols;
        // Overlay geometry only — never subtracted from the player region.
        let help_full_w = if help_visual {
            HELP_SIDEBAR_W.min(cols.saturating_sub(4)).max(12.min(cols))
        } else {
            0
        };

        // Freeze ambient motion while a card floats above the player so the
        // 60fps repaint is byte-identical (no shimmer bleeding around/under
        // the overlay). Covers the close-pop too. Resumes untouched after.
        // The lyrics strip is docked (part of the layout, not an overlay)
        // so it never freezes the pulse — karaoke motion is geometry-only.
        let frozen = settings_visual || help_visual || list_visual;
        let pulse = |period: f64| {
            if frozen { 0.5 } else { breath(t, period) }
        };

        let block_w = content_cols.saturating_sub(4).clamp(28, 56);
        let max_title = block_w.saturating_sub(2);

        // Stable glyphs when paused/stopped — no blinky frame swaps.
        let (icon, status) = if state.stopped {
            ("■", "stopped")
        } else if state.paused {
            ("⏸", "paused")
        } else if state.muted {
            ("▶", "muted")
        } else {
            ("▶", "playing")
        };

        let title = truncate(state.track_name, max_title);
        let path = if self.show_path {
            truncate(state.track_path, max_title)
        } else {
            String::new()
        };

        let bar_w = block_w.saturating_sub(14).clamp(12, 36);
        let (filled, knob, empty) = progress_parts(state.pos, state.duration, bar_w);
        let pos_s = fmt_time(state.pos);
        let total_s = state
            .duration
            .map(fmt_time)
            .unwrap_or_else(|| "--:--".into());

        let idx = format!("{}/{}", state.index, state.total);
        let vol = if state.muted {
            format!("mute {}%", state.volume)
        } else {
            format!("{}%", state.volume)
        };
        let spd = format!("{:.1}x", state.speed);
        let ptch = format!("{:.2}", state.pitch);
        let eq = state.eq_label;
        let loop_l = state.loop_label;
        let cava_levels = self.cava.as_ref().map(|c| c.snapshot());

        // Playlist card — compact overlay: only what fits, capped so a huge
        // queue never stretches full-height. Scrollbar covers the rest.
        if self.show_list {
            let cap = rows.saturating_sub(9).clamp(3, 10).max(1);
            self.list_visible = state.list_names.len().max(1).min(cap);
            self.list_total = state.list_names.len();
            self.follow_list_track(state.index);
            let vis = self.list_visible.max(1);
            self.list_scroll = clamp_list_offset(self.list_total, vis, self.list_scroll);
        }

        let show_cava_strip = cava_levels.is_some();

        // Docked lyrics — lyric LINES follow `lyrics_pos`, the header row
        // (`lyrics · y hide` / `scrolled · enter follow`) is always pinned
        // BELOW the footer chip row. `hidden` hides both. Fixed 6-row total
        // (1 header + 2 inner gaps + 3 rows); `above` splits it (5-row body
        // on top, 1-row header at the bottom) so the header height is never
        // double-counted. Open/close pops (~140ms/~120ms, instant in LDM);
        // the rest of the block reflows around it so nothing overlaps.
        const LYRICS_ROWS: usize = 3;
        const LYRICS_H_FULL: usize = 6; // header(1) + inner gaps(2) + rows(3)
        const LYRICS_HEAD_H: usize = 1;
        const LYRICS_BODY_FULL: usize = 5; // inner gaps(2) + rows(3)
        let lyrics_want =
            state.lyrics_open && self.config.lyrics_pos != LyricsPos::Hidden && !self.preview;
        let lyrics_prog = self.lyrics_progress(ldm).clamp(0.0, 1.0);
        let lyrics_visual = lyrics_want
            || (!self.preview && self.config.lyrics_pos != LyricsPos::Hidden && lyrics_prog > 0.02);
        let lyrics_h = if lyrics_visual {
            if ldm {
                LYRICS_H_FULL
            } else {
                ((LYRICS_H_FULL as f64 * lyrics_prog).round() as usize).clamp(1, LYRICS_H_FULL)
            }
        } else {
            0
        };
        let lyrics_above = self.config.lyrics_pos == LyricsPos::Above;
        // Split the animated total: header pops first (1 row), the body
        // (gaps + lines) takes the rest so heights never double-count.
        let lyrics_head_h = if lyrics_h > 0 { LYRICS_HEAD_H } else { 0 };
        let lyrics_body_h = lyrics_h.saturating_sub(lyrics_head_h).min(LYRICS_BODY_FULL);
        // Follow bookkeeping (track change resets manual scroll).
        if lyrics_visual || lyrics_want {
            let key = if state.track_path.is_empty() {
                state.track_name.to_string()
            } else {
                state.track_path.to_string()
            };
            let total_synced = state.lyrics_synced.len();
            let total = if total_synced > 0 {
                total_synced
            } else {
                state.lyrics_plain.len()
            };
            self.note_lyrics_track(&key, state.lyrics_active);
            let target = self
                .lyrics_manual
                .unwrap_or_else(|| lyric_window_start(state.lyrics_active, total, LYRICS_ROWS));
            // Clamp manual overrides that outlived a shorter track.
            if total <= LYRICS_ROWS {
                self.lyrics_manual = None;
            } else if let Some(m) = self.lyrics_manual {
                self.lyrics_manual = Some(m.min(total.saturating_sub(LYRICS_ROWS)));
            }
            let rate = if ldm { 1.0 } else { 0.25 };
            self.lyrics_smooth = ease_lyric_offset(self.lyrics_smooth, target as f64, rate);
        }
        let lyrics_line_prog = self.lyrics_line_progress(ldm);

        let mut block_h = 8usize;
        if !self.show_path {
            block_h = block_h.saturating_sub(1);
        }
        // Toast / cava / list are overlays or side panels — no center-block jump.
        block_h += 1; // meta gap / spacer
        block_h += 1; // footer
        block_h += lyrics_h;
        if lyrics_h > 0 {
            block_h += 1; // breathing gap between footer and pinned header
            if lyrics_above {
                block_h += 1; // breathing gap between top body and header block
            }
        }

        let settle = if ldm {
            0
        } else {
            ((1.0 - intro) * 1.5).round() as usize
        };
        let mut y = rows.saturating_sub(block_h) / 2 + settle;
        if y < 1 {
            y = 1;
        }

        // Brand breathes only while playing — frozen when paused / LDM.
        // Lyric LINES `above`: body sits ABOVE the header block (top of
        // area); the header row itself always goes below the footer.
        if lyrics_h > 0 && lyrics_above {
            if lyrics_body_h > 0 {
                y = self.paint_lyrics_body(
                    &mut out,
                    y,
                    content_x0,
                    content_cols,
                    block_w,
                    state,
                    LYRICS_ROWS,
                    lyrics_body_h,
                    lyrics_line_prog,
                    ldm,
                )?;
            }
            y += 1; // breathing gap between body and header block
        }
        let note_c = if playing && !ldm && !frozen {
            mix_rgb(accent_dim, accent, pulse(2.8))
        } else {
            mix_rgb(accent, DARK, 1.0 - intro)
        };
        let brand_c = mix_rgb(accent, DARK, 1.0 - intro);
        paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &[Span::fg(note_c, "♪  "), Span::fg(brand_c, APP_NAME)],
        )?;
        y += 2;

        let title_c = if self.config.accent == Accent::Default {
            gray(lerp(70.0, 245.0, title_in * intro))
        } else {
            mix_rgb(DARK, accent, title_in * intro)
        };
        paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &[Span::fg(title_c, &title)],
        )?;
        y += 1;

        if self.show_path {
            let path_c = mix_rgb(DIM, DARK, 1.0 - intro * 0.85);
            paint_in_region(
                &mut out,
                y as u16,
                content_x0,
                content_cols,
                &[Span::fg(path_c, &path)],
            )?;
            y += 2; // gap before progress
        } else {
            y += 1; // compact gap when path hidden
        }

        let knob_c = if playing && !ldm && !frozen {
            mix_rgb(accent_dim, accent, pulse(2.0))
        } else if state.paused {
            GRAY
        } else {
            DIM
        };
        let fill_c = mix_rgb(accent, DARK, 1.0 - intro);
        let progress_spans = [
            Span::fg(DIM, &pos_s),
            Span::fg(DIM, "  "),
            Span::fg(fill_c, &filled),
            Span::fg(knob_c, &knob),
            Span::fg(DARK, &empty),
            Span::fg(DIM, "  "),
            Span::fg(DIM, &total_s),
        ];
        let prog_x = paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &progress_spans,
        )?;
        let bar_x = prog_x + pos_s.chars().count() as u16 + 2;
        self.hits.progress = Some(HitRect {
            x: bar_x,
            y: y as u16,
            w: (filled.chars().count() + knob.chars().count() + empty.chars().count()) as u16,
            h: 1,
        });
        y += 1;

        let st_color = if state.stopped {
            mix_rgb(GRAY, DARK, 1.0 - intro)
        } else if state.paused || state.muted {
            GRAY
        } else if ldm || frozen {
            accent
        } else {
            mix_rgb(accent_dim, accent, pulse(3.2))
        };
        // ◂  icon status  ▸  ·  idx  ·  −  vol  +
        let prev_g = "◂";
        let next_g = "▸";
        let vol_minus = "−";
        let vol_plus = "+";
        let status_spans = [
            Span::fg(DIM, prev_g),
            Span::fg(DARK, "  "),
            Span::fg(st_color, icon),
            Span::fg(DIM, "  "),
            Span::fg(st_color, status),
            Span::fg(DARK, "  "),
            Span::fg(DIM, next_g),
            Span::fg(DARK, "  ·  "),
            Span::fg(GRAY, &idx),
            Span::fg(DARK, "  ·  "),
            Span::fg(DIM, vol_minus),
            Span::fg(DARK, " "),
            Span::fg(GRAY, &vol),
            Span::fg(DARK, " "),
            Span::fg(DIM, vol_plus),
        ];
        let status_x =
            paint_in_region(&mut out, y as u16, content_x0, content_cols, &status_spans)?;
        let mut cx = status_x;
        self.hits.prev = Some(HitRect {
            x: cx,
            y: y as u16,
            w: prev_g.chars().count() as u16,
            h: 1,
        });
        cx += prev_g.chars().count() as u16 + 2;
        let pp_w = (icon.chars().count() + 2 + status.chars().count()) as u16;
        self.hits.play_pause = Some(HitRect {
            x: cx,
            y: y as u16,
            w: pp_w,
            h: 1,
        });
        cx += pp_w + 2;
        self.hits.next = Some(HitRect {
            x: cx,
            y: y as u16,
            w: next_g.chars().count() as u16,
            h: 1,
        });
        // Volume controls sit at the end: − vol +
        let vol_plus_w = vol_plus.chars().count() as u16;
        let vol_w = vol.chars().count() as u16;
        let vol_minus_w = vol_minus.chars().count() as u16;
        let end = status_x + spans_width(&status_spans) as u16;
        let plus_x = end - vol_plus_w;
        let vol_x = plus_x - 1 - vol_w;
        let minus_x = vol_x - 1 - vol_minus_w;
        self.hits.volume_down = Some(HitRect {
            x: minus_x.saturating_sub(1),
            y: y as u16,
            w: vol_minus_w + 2,
            h: 1,
        });
        self.hits.volume = Some(HitRect {
            x: vol_x,
            y: y as u16,
            w: vol_w,
            h: 1,
        });
        self.hits.volume_up = Some(HitRect {
            x: plus_x.saturating_sub(1),
            y: y as u16,
            w: vol_plus_w + 2,
            h: 1,
        });
        y += 1;

        let mut meta_spans: Vec<Span<'_>> = vec![
            Span::fg(DIM, "spd "),
            Span::fg(GRAY, &spd),
            Span::fg(DARK, "  ·  "),
            Span::fg(DIM, "ptch "),
            Span::fg(GRAY, &ptch),
            Span::fg(DARK, "  ·  "),
            Span::fg(DIM, "eq "),
            Span::fg(GRAY, eq),
            Span::fg(DARK, "  ·  "),
            Span::fg(DIM, "repeat "),
            Span::fg(GRAY, loop_l),
        ];
        // Subtle session extras on the same status line — no new chrome.
        if state.smart_shuffle {
            meta_spans.push(Span::fg(DARK, "  ·  "));
            meta_spans.push(Span::fg(DIM, "smart"));
        }
        if let Some(sleep) = state.sleep_label {
            if !sleep.is_empty() {
                meta_spans.push(Span::fg(DARK, "  ·  "));
                meta_spans.push(Span::fg(DIM, "sleep "));
                meta_spans.push(Span::fg(GRAY, sleep));
            }
        }
        let meta_x = paint_in_region(&mut out, y as u16, content_x0, content_cols, &meta_spans)?;
        let mut mx = meta_x;
        self.hits.speed = Some(HitRect {
            x: mx,
            y: y as u16,
            w: (4 + spd.chars().count()) as u16,
            h: 1,
        });
        mx += 4 + spd.chars().count() as u16;
        mx += 3; // "  ·  "
        self.hits.pitch = Some(HitRect {
            x: mx,
            y: y as u16,
            w: (5 + ptch.chars().count()) as u16,
            h: 1,
        });
        mx += 5 + ptch.chars().count() as u16;
        mx += 3;
        self.hits.eq = Some(HitRect {
            x: mx,
            y: y as u16,
            w: (3 + eq.chars().count()) as u16,
            h: 1,
        });
        y += 1;
        y += 1; // one empty breathing row before the footer bar
        let footer_dim = mix_rgb(DARK, Color::Black, 1.0 - intro);
        let footer_key = mix_rgb(GRAY, DARK, 1.0 - intro * 0.7);
        paint_key_footer(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            footer_key,
            footer_dim,
            self.preview,
            &mut self.hits,
        )?;
        y += 1;
        // Pinned header: ALWAYS below the footer chip row, both positions.
        // `below` then continues with the lyric LINES under the header;
        // `above` already painted the lines on top.
        if lyrics_h > 0 {
            if lyrics_body_h == 0 {
                self.lyrics_active_row = None;
            }
            y += 1; // breathing gap between footer and pinned header
            if lyrics_head_h > 0 {
                y = self.paint_lyrics_header(&mut out, y, content_x0, content_cols, block_w)?;
            }
            if !lyrics_above && lyrics_body_h > 0 {
                y = self.paint_lyrics_body(
                    &mut out,
                    y,
                    content_x0,
                    content_cols,
                    block_w,
                    state,
                    LYRICS_ROWS,
                    lyrics_body_h,
                    lyrics_line_prog,
                    ldm,
                )?;
            }
        }

        // Cava overlays below the player — fixed offset, does not shift the block.
        let cava_rows = self.config.cava.rows as usize;
        if show_cava_strip {
            let cava_y = (y + 2).min(rows.saturating_sub(cava_rows + 1));
            if let Some(ref levels) = cava_levels {
                let intensity = if playing {
                    0.85
                } else if state.paused {
                    0.32
                } else {
                    0.14
                };
                let strip_w = block_w
                    .saturating_add(4)
                    .min(content_cols.saturating_sub(4));
                let cava_color = if self.config.accent == Accent::Default {
                    mix(CAVA_DIM, CAVA_SOFT, intensity * 0.95)
                } else {
                    mix_rgb(DARK, accent_dim, intensity.clamp(0.2, 1.0))
                };
                let (strip_x, strip_h) = paint_cava_bars(
                    &mut out,
                    cava_y as u16,
                    content_x0,
                    content_cols,
                    strip_w,
                    levels,
                    intensity,
                    self.config.cava.style,
                    cava_rows,
                    cava_color,
                )?;
                self.hits.cava = Some(HitRect {
                    x: strip_x,
                    y: cava_y as u16,
                    w: strip_w as u16,
                    h: strip_h.max(1),
                });
            }
        }

        if list_visual {
            let list_right = self.list_side() == PanelSide::Right;
            let (track_y, track_h, thumb_y, thumb_h, vis_eff) = paint_list_sidebar(
                &mut out,
                &mut self.hits,
                cols,
                rows,
                LIST_SIDEBAR_W.min(cols.saturating_sub(4)).max(20.min(cols)),
                self.list_scroll,
                self.list_visible,
                self.list_cursor,
                state.index,
                state.list_names,
                playing,
                t,
                ldm,
                frozen,
                accent,
                list_prog,
                self.show_list,
                list_right,
            )?;
            // Remember exactly what was painted so input math (wheel,
            // drag, paging) matches the pixels — even mid-pop.
            self.list_vis_eff = vis_eff;
            self.list_track_y = track_y;
            self.list_track_h = track_h;
            self.list_thumb_y = thumb_y;
            self.list_thumb_h = thumb_h;
        } else {
            self.hits.list.clear();
            self.hits.list_bar = None;
            self.hits.list_pane = None;
        }

        // Settings card uses its sticky open-time side; help uses its own
        // sticky side. The two never share a side (second opener takes the
        // free one), so they never overlap on wide terminals; on narrow
        // ones help shrinks to clear settings.
        let settings_right_side = self.settings_side() == PanelSide::Right;
        let _settings_render_w = if settings_visual {
            settings::paint_settings_sidebar(
                &mut out,
                cols,
                rows,
                &mut self.settings,
                &self.config,
                accent,
                settings_right_side,
            )?
        } else {
            self.settings.close_paint_state();
            0
        };

        let (settings_left, settings_right) = self
            .settings
            .pane_rect()
            .map(|(x, _, w, _)| (x, x + w))
            .unwrap_or((0, 0));

        let help_render_w = if help_visual {
            let help_right = self.help_side() == PanelSide::Right;
            paint_help_sidebar(
                &mut out,
                cols,
                rows,
                &mut self.hits,
                help_full_w,
                accent,
                self.preview,
                help_prog,
                self.show_help,
                settings_right,
                settings_left,
                help_right,
            )?
        } else {
            self.hits.help_pane = None;
            0
        };
        let _ = help_render_w;

        // Docked lyrics own their hit rects (set in the painters).
        // When hidden, clear them so clicks fall through to the player.
        if lyrics_h == 0 {
            self.hits.lyrics_pane = None;
            self.hits.lyrics_head = None;
            self.lyrics_active_row = None;
        }

        // Toast stack — fixed anchor + own width from `toast_pos`.
        // Panels never push or resize it; newest paints last (on top).
        let stack = self.live_toasts();
        if !stack.is_empty() {
            let pos = self.config.toast_pos;
            paint_toast_stack(&mut out, cols, rows, &stack, pos, ldm)?;
        }

        queue!(out, EndSynchronizedUpdate)?;
        out.flush()?;
        Ok(())
    }
}

/// Help sections — compact, grouped, minimal.
const HELP_SECTIONS: &[(&str, &[(&str, &str)])] = &[
    (
        "play",
        &[
            ("space", "pause"),
            ("n p", "next / prev"),
            ("s", "stop"),
            ("l", "list"),
            ("r", "shuffle"),
            ("o", "repeat cycle"),
        ],
    ),
    ("seek", &[("← →", "±5s"), ("{ }", "±60s"), ("1–9", "jump")]),
    (
        "sound",
        &[
            ("+ −", "volume"),
            ("m", "mute"),
            ("e", "eq"),
            ("[ ]", "speed"),
            (", .", "pitch"),
            ("0", "reset"),
        ],
    ),
    (
        "more",
        &[
            ("f", "filename"),
            ("v", "cava"),
            ("x", "smart mix"),
            ("y", "lyrics"),
            ("z", "sleep"),
            ("c", "settings"),
            ("↑↓←→", "list scroll"),
            ("?", "help"),
            ("q", "quit"),
        ],
    ),
];

/// Preview session help — no list / settings.
const HELP_SECTIONS_PREVIEW: &[(&str, &[(&str, &str)])] = &[
    (
        "play",
        &[("space", "pause"), ("s", "stop"), ("q", "back to download")],
    ),
    ("seek", &[("← →", "±5s"), ("{ }", "±60s")]),
    (
        "sound",
        &[
            ("+ −", "volume"),
            ("m", "mute"),
            ("e", "eq"),
            ("[ ]", "speed"),
        ],
    ),
    ("more", &[("?", "help")]),
];

/// Right help sidebar width when fully open.
const HELP_SIDEBAR_W: usize = 26;
/// Width of the compact playlist sidebar.
const LIST_SIDEBAR_W: usize = 30;
const LIST_SIDEBAR_HEADER: usize = 3;
const LIST_SIDEBAR_FOOTER: usize = 2;

fn help_sidebar_height(preview: bool) -> usize {
    let sections = if preview {
        HELP_SECTIONS_PREVIEW
    } else {
        HELP_SECTIONS
    };
    let mut n = 1; // top pad
    for (i, (_title, rows)) in sections.iter().enumerate() {
        if i > 0 {
            n += 1; // blank between sections
        }
        n += 1 + rows.len(); // title + rows
    }
    n + 2 // blank + "h close"
}

/// Floating help card — hovers over the player without touching its layout.
/// Sticky side from open time (`anchor_right`); `avoid_left` is the exclusive
/// right edge of the settings card and `avoid_right_edge` its left edge, so
/// the two cards never overlap brokenly on narrow terminals.
#[allow(clippy::too_many_arguments)]
fn paint_help_sidebar(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    hits: &mut HitMap,
    sidebar_w: usize,
    accent: Color,
    preview: bool,
    progress: f64,
    logically_open: bool,
    avoid_left: usize,
    avoid_right_edge: usize,
    anchor_right: bool,
) -> io::Result<usize> {
    use crossterm::style::SetBackgroundColor;

    let e = progress.clamp(0.0, 1.0);
    if e <= 0.02 {
        hits.help_pane = None;
        return Ok(0);
    }

    let sections = if preview {
        HELP_SECTIONS_PREVIEW
    } else {
        HELP_SECTIONS
    };
    let panel_bg = Color::Rgb {
        r: 22,
        g: 22,
        b: 22,
    };

    // Box geometry: 1-col margin from the pinned edge, vertically centered.
    // Shrink first to clear the settings card on narrow terminals.
    let mut box_w = sidebar_w.max(16).min(cols.saturating_sub(2).max(16));
    if anchor_right {
        let max_w = cols.saturating_sub(avoid_left + 2).max(12).min(box_w);
        box_w = max_w.max(12).min(box_w);
    } else {
        // Left-anchored: clear a right-side settings card.
        let room = avoid_right_edge.saturating_sub(3).max(12).min(box_w);
        box_w = room.max(12).min(box_w);
    }
    let need = help_sidebar_height(preview) + 4; // borders + inner pads
    let box_h = need.min(rows.saturating_sub(1).max(5)).max(5);

    // Pop geometry: pinned edge stays, width/height ease to full.
    let (render_w, render_h) = if e >= 0.999 {
        (box_w, box_h)
    } else {
        (
            ((box_w as f64 * (0.55 + 0.45 * e)) as usize)
                .max(12)
                .min(box_w),
            ((box_h as f64 * (0.6 + 0.4 * e)) as usize)
                .max(5)
                .min(box_h),
        )
    };
    let x0 = if anchor_right {
        cols.saturating_sub(render_w + 1)
    } else {
        1usize
    };
    let y0 = rows.saturating_sub(box_h) / 2;
    let ry0 = y0 + (box_h - render_h) / 2;
    let y1 = ry0 + render_h - 1;

    // Top card owns its clicks — set only while logically open so a
    // closing pop never swallows (or leaks) input meant for the player.
    hits.help_pane = if logically_open {
        Some(HitRect {
            x: x0 as u16,
            y: ry0 as u16,
            w: render_w as u16,
            h: render_h as u16,
        })
    } else {
        None
    };

    // Solid fill so the player never shows through.
    let fill = " ".repeat(render_w);
    for row in ry0..=y1 {
        queue!(
            out,
            MoveTo(x0 as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(panel_bg),
            Print(&fill),
            ResetColor
        )?;
    }

    // Thin border — same DARK hairline language as the toast card.
    let edge = "─".repeat(render_w.saturating_sub(2));
    let top = format!("┌{edge}┐");
    let bot = format!("└{edge}┘");
    queue!(
        out,
        MoveTo(x0 as u16, ry0 as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&top),
        ResetColor
    )?;
    queue!(
        out,
        MoveTo(x0 as u16, y1 as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&bot),
        ResetColor
    )?;
    for row in (ry0 + 1)..y1 {
        queue!(
            out,
            MoveTo(x0 as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(DARK),
            Print("│"),
            ResetColor
        )?;
        queue!(
            out,
            MoveTo((x0 + render_w - 1) as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(DARK),
            Print("│"),
            ResetColor
        )?;
    }

    // Body text with 1-col padding inside the border (clipped while popping).
    let render_inner = render_w.saturating_sub(4).max(8);
    let text_x = x0 + 2;
    let last = y1.saturating_sub(1); // keep bottom pad + border clear
    let mut y = ry0 + 2;
    let mut paint_row = |y: usize, spans: &[Span<'_>]| -> io::Result<()> {
        queue!(out, MoveTo(text_x as u16, y as u16))?;
        queue!(out, SetBackgroundColor(panel_bg))?;
        paint_spans(out, spans)
    };

    for (si, (title, rows_sec)) in sections.iter().enumerate() {
        if si > 0 {
            y += 1;
        }
        if y > last {
            break;
        }
        paint_row(y, &[Span::fg(DIM, title)])?;
        y += 1;
        for (keys, action) in *rows_sec {
            if y > last {
                break;
            }
            let key_col = format!("{keys:<6}");
            let action_t = truncate(action, render_inner.saturating_sub(8));
            paint_row(
                y,
                &[
                    Span::fg(accent, &key_col),
                    Span::fg(DARK, " "),
                    Span::fg(GRAY, &action_t),
                ],
            )?;
            y += 1;
        }
    }
    if y + 1 <= last {
        y += 1;
        paint_row(y, &[Span::fg(DARK, "h  close")])?;
    }
    Ok(render_w)
}

/// Playlist panel — compact floating card in the exact `?`/`c` language:
/// solid fill, DARK hairline border on all four sides, 1-col inner padding,
/// DIM title + hairline, `›` current marker, DARK footer hint.
/// Sticky side from open time (`anchor_right`); vertically centered, only as
/// tall as needed (capped); never a full-height strip. The player layout is
/// untouched.
/// Opens/closes with the same short geometry-only pop as `?`/`c`
/// (~140ms ease-out open, ~120ms shrink close); input never blocks and
/// ambient pulse stays frozen while open.
#[allow(clippy::too_many_arguments)]
fn paint_list_sidebar(
    out: &mut impl Write,
    hits: &mut HitMap,
    cols: usize,
    rows: usize,
    sidebar_w: usize,
    scroll: usize,
    visible: usize,
    cursor_0based: usize,
    current_1based: usize,
    names: &[String],
    playing: bool,
    t: f64,
    ldm: bool,
    frozen: bool,
    accent: Color,
    progress: f64,
    logically_open: bool,
    anchor_right: bool,
) -> io::Result<(u16, u16, u16, u16, usize)> {
    use crossterm::style::SetBackgroundColor;
    let e = progress.clamp(0.0, 1.0);
    if e <= 0.02 {
        hits.list.clear();
        hits.list_bar = None;
        hits.list_pane = None;
        return Ok((0, 1, 0, 1, visible.max(1)));
    }
    let pulse = |period: f64| {
        if frozen { 0.5 } else { breath(t, period) }
    };
    let panel_bg = Color::Rgb {
        r: 22,
        g: 22,
        b: 22,
    };
    let box_w = sidebar_w.max(20).min(cols.max(1));
    // Compact card: just the visible rows + title/hairline/footer + borders.
    let vis_want = visible.max(1).min(names.len().max(1));
    let want = vis_want + LIST_SIDEBAR_HEADER + LIST_SIDEBAR_FOOTER + 2; // borders
    let box_h = want.min(rows.saturating_sub(2).max(7)).max(7);

    // Pop geometry: pinned edge stays, width/height ease to full.
    let (sidebar_w, list_h) = if e >= 0.999 {
        (box_w, box_h)
    } else {
        (
            ((box_w as f64 * (0.55 + 0.45 * e)) as usize)
                .max(16)
                .min(box_w),
            ((box_h as f64 * (0.6 + 0.4 * e)) as usize)
                .max(5)
                .min(box_h),
        )
    };
    let x0 = if anchor_right {
        cols.saturating_sub(sidebar_w + 1)
    } else {
        1usize
    };
    let y0 = rows.saturating_sub(box_h) / 2 + (box_h - list_h) / 2;
    let track_x = x0 + sidebar_w.saturating_sub(2);
    let track_y = (y0 + LIST_SIDEBAR_HEADER) as u16;
    let track_h = list_h
        .saturating_sub(LIST_SIDEBAR_HEADER + LIST_SIDEBAR_FOOTER + 2)
        .max(1) as u16;

    hits.list.clear();
    hits.list_bar = None;
    // Card owns its clicks only while logically open — a closing pop
    // never swallows (or leaks) input meant for the player.
    hits.list_pane = if logically_open {
        Some(HitRect {
            x: x0 as u16,
            y: y0 as u16,
            w: sidebar_w as u16,
            h: list_h as u16,
        })
    } else {
        None
    };

    // Solid fill so the player never shows through (both modes).
    let fill = " ".repeat(sidebar_w);
    for row in y0..(y0 + list_h).min(rows) {
        queue!(
            out,
            MoveTo(x0 as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(panel_bg),
            Print(&fill),
            ResetColor
        )?;
    }
    // Hairline frame: full card on all four sides (same as `?`/`c`).
    let edge = "─".repeat(sidebar_w.saturating_sub(2));
    queue!(
        out,
        MoveTo(x0 as u16, y0 as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&format!("┌{edge}┐")),
        ResetColor
    )?;
    queue!(
        out,
        MoveTo(x0 as u16, (y0 + list_h - 1) as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&format!("└{edge}┘")),
        ResetColor
    )?;
    for row in (y0 + 1)..(y0 + list_h - 1) {
        queue!(
            out,
            MoveTo(x0 as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(DARK),
            Print("│"),
            ResetColor
        )?;
        queue!(
            out,
            MoveTo((x0 + sidebar_w - 1) as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(DARK),
            Print("│"),
            ResetColor
        )?;
    }

    let tx = (x0 + 2) as u16;
    let inner_w = sidebar_w.saturating_sub(5).max(10);
    let head = format!("playlist {}/{}", current_1based.max(1), names.len().max(1));
    queue!(
        out,
        MoveTo(tx, (y0 + 1) as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DIM),
        Print(truncate(&head, inner_w)),
        ResetColor
    )?;
    queue!(
        out,
        MoveTo(tx, (y0 + 2) as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&"─".repeat(inner_w.min(cols))),
        ResetColor
    )?;

    let total = names.len();
    let vis = visible.max(1).min(track_h as usize);
    // Same clamp the input side uses — paint never shows blank rows.
    let scroll = clamp_list_offset(total, vis, scroll);

    if names.is_empty() {
        queue!(
            out,
            MoveTo(tx, track_y),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(DARK),
            Print("(empty)"),
            ResetColor
        )?;
    } else {
        let name_w = sidebar_w.saturating_sub(9).max(8);
        for i in 0..vis {
            let idx = scroll + i;
            if idx >= total {
                break;
            }
            let track_n = idx + 1;
            let current = track_n == current_1based;
            let selected = idx == cursor_0based.min(total - 1);
            let marker = if current { "›" } else { " " };
            let chip = format!("{marker}{track_n} {}", truncate(&names[idx], name_w));
            let y = track_y.saturating_add(i as u16);
            let color = if current {
                if playing && !ldm && !frozen {
                    mix_rgb(dim_accent(accent), accent, pulse(2.6))
                } else {
                    accent
                }
            } else if selected {
                // Keyboard cursor — bright so it reads apart from playback.
                BRIGHT
            } else {
                GRAY
            };
            queue!(
                out,
                MoveTo(tx, y),
                SetBackgroundColor(panel_bg),
                SetForegroundColor(color),
                Print(&truncate(&chip, inner_w)),
                ResetColor
            )?;
            hits.list.push((
                HitRect {
                    x: x0 as u16,
                    y,
                    w: (track_x as usize).saturating_sub(x0) as u16,
                    h: 1,
                },
                track_n,
            ));
        }
    }
    // Click rows only count while logically open — a closing pop never
    // steals clicks meant for the player.
    if !logically_open {
        hits.list.clear();
    }

    queue!(
        out,
        MoveTo(tx, (y0 + list_h - 2) as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print("↑↓/jk move · l close"),
        ResetColor
    )?;

    // Vertical scrollbar. Its entire two-column hit area is intentionally generous.
    // Live only while logically open (same close-pop rule as the rows).
    hits.list_bar = if logically_open {
        Some(HitRect {
            x: track_x as u16,
            y: track_y,
            w: 2,
            h: track_h,
        })
    } else {
        None
    };

    let (thumb_y, thumb_h) = list_thumb_geom(track_h, total, vis, scroll);
    for i in 0..track_h {
        let c = if i >= thumb_y && i < thumb_y + thumb_h {
            GRAY
        } else {
            DARK
        };
        queue!(
            out,
            MoveTo(track_x as u16, track_y + i),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(c),
            Print("┃"),
            ResetColor
        )?;
    }

    Ok((track_y, track_h, thumb_y, thumb_h, vis))
}

/// Compact footer: bright keys, dim gaps.
/// Every chip also registers a footer hit rect so `space n/p ←→ +/− v c ?`
/// (and preview `q`) are clickable — same action as pressing the key.
fn paint_key_footer(
    out: &mut impl Write,
    y: u16,
    region_x: usize,
    region_w: usize,
    key_c: Color,
    gap_c: Color,
    preview: bool,
    hits: &mut HitMap,
) -> io::Result<()> {
    hits.foot.clear();
    let chips: &[&str] = if preview {
        &["space", "←→", "+/−", "?", "q"]
    } else {
        &["space", "n/p", "←→", "+/−", "v", "c", "?"]
    };
    let mut spans: Vec<Span<'_>> = Vec::with_capacity(chips.len() * 2);
    for (i, key) in chips.iter().enumerate() {
        if i > 0 {
            spans.push(Span::fg(gap_c, "  ·  "));
        }
        spans.push(Span::fg(key_c, key));
    }
    let start_x = paint_in_region(out, y, region_x, region_w, &spans)?;
    // Walk the same centered layout to place one hit rect per chip
    // (split rects for two-sided `n/p ←→ +/−` chips).
    let sep_w: u16 = 5; // "  ·  "
    let mut x = start_x;
    for (i, key) in chips.iter().enumerate() {
        if i > 0 {
            x += sep_w;
        }
        let w = key.chars().count() as u16;
        for (dx, dw, target) in foot_targets(key, w) {
            hits.foot.push((
                HitRect {
                    x: x + dx,
                    y,
                    w: dw,
                    h: 1,
                },
                target,
            ));
        }
        x += w;
    }
    Ok(())
}

/// Click target(s) for a footer chip: (x-offset, width, action).
fn foot_targets(chip: &str, w: u16) -> Vec<(u16, u16, HitTarget)> {
    match chip {
        "space" => vec![(0, w, HitTarget::PlayPause)],
        // `n`ext on the left, `p`rev on the right.
        "n/p" if w >= 3 => vec![(0, 1, HitTarget::Next), (2, 1, HitTarget::Prev)],
        "←→" if w >= 2 => vec![(0, 1, HitTarget::SeekBack), (1, 1, HitTarget::SeekForward)],
        "+/−" | "+/-" if w >= 3 => {
            vec![(0, 1, HitTarget::VolumeUp), (2, 1, HitTarget::VolumeDown)]
        }
        "v" => vec![(0, w, HitTarget::CavaToggle)],
        "c" => vec![(0, w, HitTarget::Settings)],
        "?" => vec![(0, w, HitTarget::Help)],
        "q" => vec![(0, w, HitTarget::Quit)],
        _ => vec![],
    }
}

pub(crate) struct Span<'a> {
    color: Option<Color>,
    text: &'a str,
}

impl<'a> Span<'a> {
    pub(crate) fn fg(color: Color, text: &'a str) -> Self {
        Self {
            color: Some(color),
            text,
        }
    }
}

fn spans_width(spans: &[Span<'_>]) -> usize {
    spans.iter().map(|s| s.text.chars().count()).sum()
}

fn paint_spans(out: &mut impl Write, spans: &[Span<'_>]) -> io::Result<()> {
    for span in spans {
        if let Some(c) = span.color {
            queue!(out, SetForegroundColor(c))?;
        } else {
            queue!(out, ResetColor)?;
        }
        queue!(out, Print(span.text))?;
    }
    queue!(out, ResetColor)?;
    Ok(())
}

/// Paint left-aligned at an absolute column.
pub(crate) fn paint_at(out: &mut impl Write, x: u16, y: u16, spans: &[Span<'_>]) -> io::Result<()> {
    queue!(out, MoveTo(x, y))?;
    paint_spans(out, spans)
}

/// Paint centered inside a horizontal region; returns starting column of the content.
fn paint_in_region(
    out: &mut impl Write,
    y: u16,
    region_x: usize,
    region_w: usize,
    spans: &[Span<'_>],
) -> io::Result<u16> {
    let w = spans_width(spans);
    let x = region_x + region_w.saturating_sub(w) / 2;
    queue!(out, MoveTo(x as u16, y))?;
    paint_spans(out, spans)?;
    Ok(x as u16)
}

/// Classic vertical cava bars under the shortcut bar (default bar look).
/// Returns (start_x, rows_painted).
fn paint_cava_bars(
    out: &mut impl Write,
    y: u16,
    region_x: usize,
    region_w: usize,
    block_w: usize,
    levels: &[f32],
    intensity: f64,
    style: CavaStyle,
    rows: usize,
    color: Color,
) -> io::Result<(u16, u16)> {
    if levels.is_empty() || block_w == 0 {
        return Ok(((region_x + region_w / 2) as u16, 0));
    }
    // Half-block ramp — stock cava “bars” feel.
    const RAMP: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    const DOTS: &[char] = &[' ', '·', '•', '●'];
    let intensity = intensity.clamp(0.0, 1.0);
    let rows = rows.clamp(3, 7);
    let n = levels.len();

    let gap = matches!(style, CavaStyle::Bars | CavaStyle::Mirror);
    let bar_cols = if gap {
        ((block_w + 1) / 2).clamp(8, n.min(block_w))
    } else {
        block_w.clamp(8, n.min(block_w))
    };

    let mut sampled = Vec::with_capacity(bar_cols);
    for b in 0..bar_cols {
        let pos = if bar_cols <= 1 {
            0.0
        } else {
            b as f64 / (bar_cols - 1) as f64 * (n.saturating_sub(1)) as f64
        };
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(n - 1);
        let frac = (pos - i0 as f64).clamp(0.0, 1.0) as f32;
        let raw = levels[i0] * (1.0 - frac) + levels[i1] * frac;
        let left = levels[i0.saturating_sub(1)];
        let right = levels[i1.min(n - 1)];
        let level = ((left * 0.15 + raw * 0.7 + right * 0.15) as f64).clamp(0.0, 1.0) * intensity;
        sampled.push(level.powf(0.78));
    }

    if style == CavaStyle::Mirror && bar_cols > 1 {
        let mid = bar_cols / 2;
        for i in 0..mid {
            let mirror = sampled[bar_cols - 1 - i];
            let v = sampled[i].max(mirror);
            sampled[i] = v;
            sampled[bar_cols - 1 - i] = v;
        }
    }

    let paint_rows = if style == CavaStyle::Dots { 1 } else { rows };
    let mut lines: Vec<String> = (0..paint_rows)
        .map(|_| String::with_capacity(block_w))
        .collect();

    for (b, &level) in sampled.iter().enumerate() {
        if style == CavaStyle::Dots {
            let idx = (level * (DOTS.len() - 1) as f64).round() as usize;
            lines[0].push(DOTS[idx.min(DOTS.len() - 1)]);
            if gap && b + 1 < bar_cols {
                lines[0].push(' ');
            }
            continue;
        }

        let eighths = (level * (rows * (RAMP.len() - 1)) as f64).round() as usize;
        let full = RAMP.len() - 1;
        for r in 0..rows {
            let from_bottom = rows - 1 - r;
            let cell_base = from_bottom * full;
            let ch = if eighths >= cell_base + full {
                RAMP[full]
            } else if eighths > cell_base {
                RAMP[eighths - cell_base]
            } else {
                RAMP[0]
            };
            lines[r].push(ch);
            if gap && b + 1 < bar_cols {
                lines[r].push(' ');
            }
        }
    }

    let mut x0 = (region_x + region_w / 2) as u16;
    for (i, line) in lines.iter().enumerate() {
        let x = paint_in_region(
            out,
            y + i as u16,
            region_x,
            region_w,
            &[Span::fg(color, line)],
        )?;
        if i == 0 {
            x0 = x;
        }
    }
    Ok((x0, paint_rows as u16))
}

/// Toast stack — fixed anchor from `toast_pos` with its own width.
/// Panels never push or resize it. Oldest paints first (behind), newest
/// last (front, full brightness). Older cards shift 1 row away from the
/// anchor and dim, so the stack reads as depth without new colors.
/// Only `Error` gets a bright border; every kind has its own `· ♪ ✓ !` prefix.
fn paint_toast_stack(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    stack: &[(&str, ToastKind, f64)],
    pos: ToastPos,
    ldm: bool,
) -> io::Result<()> {
    if stack.is_empty() {
        return Ok(());
    }
    let top_anchor = matches!(
        pos,
        ToastPos::TopCenter | ToastPos::TopLeft | ToastPos::TopRight
    );
    // Stacked toasts render compact: tighter padding inside the box and a
    // minimal shared-border step (2 rows) between items instead of the
    // full 3-row card gap. A lone toast keeps the roomier single padding.
    let stacked = stack.len() > 1;
    let step = if stacked { 2 } else { 3 };
    // Oldest first so the newest lands on top.
    for (depth, (msg, kind, elapsed)) in stack.iter().enumerate().take(TOAST_MAX) {
        let newest_first = stack.len() - 1 - depth;
        let base_alpha = if ldm { 1.0 } else { toast_alpha(*elapsed) };
        if base_alpha <= 0.02 {
            continue;
        }
        // Depth falloff: older cards dim and sit behind.
        let depth_dim = 1.0 - newest_first as f64 * 0.28;
        let alpha = (base_alpha * depth_dim).clamp(0.0, 1.0);
        let color = gray(lerp(40.0, 230.0, alpha));
        let is_err = *kind == ToastKind::Error;
        let edge_c = if is_err {
            gray(lerp(90.0, 245.0, alpha))
        } else {
            mix(Color::Black, DARK, alpha)
        };
        let symbol_c = if is_err { BRIGHT } else { DIM };

        let toast_max = cols.saturating_sub(10).clamp(12, 64);
        let short = msg_truncated(msg, toast_max);
        let inner = if stacked {
            format!("{} {}", kind.symbol(), short)
        } else {
            format!("{} {} ", kind.symbol(), short)
        };
        let w = inner.chars().count();
        let box_w = w + 2;
        let x = match pos {
            ToastPos::BottomCenter | ToastPos::TopCenter => cols.saturating_sub(box_w) / 2,
            ToastPos::BottomLeft | ToastPos::TopLeft => 2usize,
            ToastPos::BottomRight | ToastPos::TopRight => cols.saturating_sub(box_w + 2).max(1),
        };
        // 3-row box; newest hugs the edge, older ones step away.
        let base_y = if top_anchor {
            1usize
        } else {
            // Keep the bottom border off the last terminal row.
            rows.saturating_sub(4).max(1)
        };
        let y = if top_anchor {
            base_y + newest_first * step
        } else {
            base_y.saturating_sub(newest_first * step)
        };
        if y + 2 >= rows {
            continue;
        }
        let top = format!("┌{}┐", "─".repeat(w));
        let bot = format!("└{}┘", "─".repeat(w));
        let (x, y) = (x as u16, y as u16);
        let body = if stacked {
            format!(" {short}")
        } else {
            format!(" {short} ")
        };
        paint_at(out, x, y, &[Span::fg(edge_c, &top)])?;
        paint_at(
            out,
            x,
            y + 1,
            &[
                Span::fg(edge_c, "│"),
                Span::fg(symbol_c, kind.symbol()),
                Span::fg(color, &body),
                Span::fg(edge_c, "│"),
            ],
        )?;
        paint_at(out, x, y + 2, &[Span::fg(edge_c, &bot)])?;
    }
    Ok(())
}

fn msg_truncated(msg: &str, toast_max: usize) -> String {
    // Room taken by the `symbol + space` prefix and trailing space.
    truncate(msg, toast_max.saturating_sub(3).max(4))
}

impl Drop for SessionUi {
    fn drop(&mut self) {
        if let Some(ref mut c) = self.cava {
            c.stop();
        }
        self.cava = None;
        if !self.detached {
            let _ = self.restore();
        }
    }
}

// ── Motion helpers ──────────────────────────────────────────────

fn breath(t: f64, period_secs: f64) -> f64 {
    let p = period_secs.max(0.1);
    ((t / p) * std::f64::consts::TAU).sin() * 0.5 + 0.5
}

fn ease_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

fn gray(v: f64) -> Color {
    let v = v.round().clamp(0.0, 255.0) as u8;
    Color::Rgb { r: v, g: v, b: v }
}

fn color_level(c: Color) -> u8 {
    match c {
        Color::Rgb { r, g, b } => ((r as u16 + g as u16 + b as u16) / 3) as u8,
        Color::White => 255,
        Color::Black => 0,
        _ => 128,
    }
}

fn mix(a: Color, b: Color, t: f64) -> Color {
    gray(lerp(color_level(a) as f64, color_level(b) as f64, t))
}

fn rgb_components(c: Color) -> (u8, u8, u8) {
    match c {
        Color::Rgb { r, g, b } => (r, g, b),
        Color::White => (255, 255, 255),
        Color::Black => (0, 0, 0),
        _ => (128, 128, 128),
    }
}

fn mix_rgb(a: Color, b: Color, t: f64) -> Color {
    let (ar, ag, ab) = rgb_components(a);
    let (br, bg, bb) = rgb_components(b);
    let t = t.clamp(0.0, 1.0);
    Color::Rgb {
        r: lerp(ar as f64, br as f64, t).round().clamp(0.0, 255.0) as u8,
        g: lerp(ag as f64, bg as f64, t).round().clamp(0.0, 255.0) as u8,
        b: lerp(ab as f64, bb as f64, t).round().clamp(0.0, 255.0) as u8,
    }
}

fn accent_color(accent: &Accent) -> Color {
    match accent.rgb() {
        Some((r, g, b)) => Color::Rgb { r, g, b },
        None => BRIGHT,
    }
}

fn dim_accent(accent: Color) -> Color {
    mix_rgb(accent, DARK, 0.45)
}

fn toast_alpha(elapsed: f64) -> f64 {
    let fade_in = ease_out_cubic((elapsed / 0.18).clamp(0.0, 1.0));
    let fade_out = if elapsed > 1.55 {
        1.0 - ease_out_cubic(((elapsed - 1.55) / 0.65).clamp(0.0, 1.0))
    } else {
        1.0
    };
    fade_in * fade_out
}

// ── Shared helpers ──────────────────────────────────────────────

/// Binary name as invoked (`optionmusic` or `msc`).
pub fn bin_name() -> String {
    std::env::args()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "optionmusic".into())
}

pub fn banner() {
    println!();
    println!("  {} {}", "♪".with(BRIGHT), APP_NAME.with(BRIGHT).bold());
    println!();
}

pub fn print_info(msg: &str) {
    println!("  {} {}", "·".with(DIM), msg.with(GRAY));
}

pub fn print_success(msg: &str) {
    println!("  {} {}", "✓".with(BRIGHT), msg.with(BRIGHT));
}

pub fn print_warn(msg: &str) {
    println!("  {} {}", "!".with(GRAY), msg.with(GRAY));
}

pub fn fmt_time(d: Duration) -> String {
    let total = d.as_secs();
    let h = total / 3600;
    let m = (total % 3600) / 60;
    let s = total % 60;
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// Thin progress parts: filled ─── + ● + empty ───
fn progress_parts(
    pos: Duration,
    total: Option<Duration>,
    width: usize,
) -> (String, String, String) {
    let width = width.max(8);
    let ratio = match total {
        Some(t) if t.as_secs_f64() > 0.0 => (pos.as_secs_f64() / t.as_secs_f64()).clamp(0.0, 1.0),
        _ => 0.0,
    };
    // Floor (not round) so the knob inches forward smoothly at high refresh.
    let mut filled = (ratio * (width.saturating_sub(1)) as f64).floor() as usize;
    if filled >= width {
        filled = width.saturating_sub(1);
    }
    (
        "─".repeat(filled),
        "●".into(),
        "─".repeat(width.saturating_sub(filled + 1)),
    )
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else if max <= 1 {
        "…".into()
    } else {
        let take = max - 1;
        let mut out: String = s.chars().take(take).collect();
        out.push('…');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_time_under_hour() {
        assert_eq!(fmt_time(Duration::from_secs(65)), "1:05");
    }

    #[test]
    fn fmt_time_with_hours() {
        assert_eq!(fmt_time(Duration::from_secs(3661)), "1:01:01");
    }

    #[test]
    fn truncate_adds_ellipsis() {
        assert_eq!(truncate("hello world", 8), "hello w…");
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn progress_parts_width() {
        let (a, b, c) = progress_parts(Duration::from_secs(5), Some(Duration::from_secs(10)), 20);
        assert_eq!(
            a.chars().count() + b.chars().count() + c.chars().count(),
            20
        );
    }

    #[test]
    fn toast_stack_caps_at_three_and_drops_oldest() {
        // SessionUi::enter needs a terminal; exercise the stack logic via
        // push semantics on a bare deque instead.
        let mut q: VecDeque<ToastItem> = VecDeque::new();
        for (i, k) in [
            ToastKind::Info,
            ToastKind::Track,
            ToastKind::Config,
            ToastKind::Error,
        ]
        .into_iter()
        .enumerate()
        {
            q.push_back(ToastItem {
                text: format!("m{i}"),
                kind: k,
                at: Instant::now(),
            });
            while q.len() > TOAST_MAX {
                q.pop_front();
            }
        }
        assert_eq!(q.len(), 3);
        assert_eq!(q[0].text, "m1");
        assert_eq!(q[2].kind, ToastKind::Error);
    }

    #[test]
    fn toast_symbols_stay_minimal() {
        assert_eq!(ToastKind::Info.symbol(), "·");
        assert_eq!(ToastKind::Track.symbol(), "♪");
        assert_eq!(ToastKind::Config.symbol(), "✓");
        assert_eq!(ToastKind::Error.symbol(), "!");
    }

    #[test]
    fn list_dock_threshold_keeps_player_min() {
        assert!(LIST_DOCK_MIN_COLS >= PLAYER_MIN_W + LIST_SIDEBAR_W);
        // Narrow terminals must overlay instead of docking.
        assert!(LIST_DOCK_MIN_COLS > 60);
    }

    #[cfg(test)]
    fn blank_ui() -> SessionUi {
        let now = Instant::now();
        SessionUi {
            toasts: VecDeque::new(),
            show_list: false,
            show_help: false,
            settings_side_at_open: None,
            list_side_at_open: None,
            help_side_at_open: None,
            help_opened_at: None,
            help_closed_at: None,
            list_opened_at: None,
            list_closed_at: None,
            show_lyrics: false,
            lyrics_opened_at: None,
            lyrics_closed_at: None,
            lyrics_manual: None,
            lyrics_smooth: 0.0,
            lyrics_last_active: None,
            lyrics_line_since: None,
            lyrics_track_key: String::new(),
            lyrics_active_row: None,
            show_path: false,
            detached: true,
            t0: now,
            track_key: String::new(),
            track_since: now,
            list_scroll: 0,
            list_cursor: 0,
            list_visible: 8,
            list_vis_eff: 0,
            list_track_y: 0,
            list_track_h: 1,
            list_thumb_y: 0,
            list_thumb_h: 1,
            list_total: 0,
            list_follow: 0,
            hits: HitMap::default(),
            cava: None,
            config: AppConfig::default(),
            settings: SettingsUi::default(),
            preview: false,
        }
    }

    #[test]
    fn settings_sticks_to_open_side_after_list_closes() {
        let mut ui = blank_ui();
        ui.toggle_list(); // list opens left
        assert_eq!(ui.list_side(), PanelSide::Left);
        ui.toggle_settings(); // settings opens right (list owns left)
        assert_eq!(ui.settings_side(), PanelSide::Right);
        ui.toggle_list(); // close list — settings must stay right
        assert_eq!(ui.settings_side(), PanelSide::Right);
    }

    #[test]
    fn settings_reopen_picks_free_side() {
        let mut ui = blank_ui();
        ui.toggle_list();
        ui.toggle_settings();
        assert_eq!(ui.settings_side(), PanelSide::Right);
        ui.toggle_list(); // close list, sticky right
        ui.close_settings();
        ui.toggle_settings(); // reopen with space free -> default left
        assert_eq!(ui.settings_side(), PanelSide::Left);
    }

    #[test]
    fn list_takes_free_side_when_settings_open() {
        let mut ui = blank_ui();
        ui.toggle_settings(); // settings default left
        assert_eq!(ui.settings_side(), PanelSide::Left);
        ui.toggle_list(); // list must avoid settings -> right
        assert_eq!(ui.list_side(), PanelSide::Right);
        ui.toggle_settings(); // close settings — list stays right
        assert_eq!(ui.list_side(), PanelSide::Right);
    }

    #[test]
    fn help_takes_free_side_opposite_settings() {
        let mut ui = blank_ui();
        ui.toggle_settings();
        assert_eq!(ui.settings_side(), PanelSide::Left);
        ui.toggle_help();
        assert_eq!(ui.help_side(), PanelSide::Right);
        let mut ui2 = blank_ui();
        ui2.toggle_list();
        ui2.toggle_settings(); // settings right
        ui2.toggle_list(); // list closed, settings sticky right
        ui2.toggle_help(); // help must take free left
        assert_eq!(ui2.help_side(), PanelSide::Left);
    }

    #[test]
    fn list_offset_clamps_with_no_blank_rows() {
        assert_eq!(list_scroll_max_for(0, 8), 0);
        assert_eq!(list_scroll_max_for(3, 8), 0); // short list pins to 0
        assert_eq!(list_scroll_max_for(20, 8), 12);
        assert_eq!(clamp_list_offset(3, 8, 5), 0);
        assert_eq!(clamp_list_offset(20, 8, 99), 12);
        assert_eq!(clamp_list_offset(20, 8, 7), 7);
    }

    #[test]
    fn list_cursor_stays_visible() {
        assert_eq!(ensure_cursor_visible(0, 5, 8), 0); // inside: no jump
        assert_eq!(ensure_cursor_visible(0, 7, 8), 0); // last visible: stays
        assert_eq!(ensure_cursor_visible(6, 2, 8), 2); // above: snap up
        assert_eq!(ensure_cursor_visible(0, 8, 8), 1); // below: shift by one
        assert_eq!(ensure_cursor_visible(0, 9, 8), 2);
    }

    #[test]
    fn list_thumb_is_proportional_and_inside() {
        assert_eq!(list_thumb_geom(10, 0, 8, 0), (0, 10)); // empty: full track
        assert_eq!(list_thumb_geom(10, 5, 8, 0), (0, 10)); // short list: full
        let (y, h) = list_thumb_geom(10, 20, 5, 0);
        assert_eq!((y, h), (0, 3)); // round(5/20*10) = round(2.5) = 3
        let max = list_scroll_max_for(20, 5);
        let (y_end, h_end) = list_thumb_geom(10, 20, 5, max);
        assert_eq!(y_end + h_end, 10); // bottom: exactly flush, no overflow
        let mut prev = 0u16;
        for s in 0..=max {
            let (y, h) = list_thumb_geom(10, 20, 5, s);
            assert!(y >= prev); // monotonic while scrolling down
            assert!(y + h <= 10); // never overflows the card
            prev = y;
        }
    }

    #[test]
    fn list_cursor_moves_pull_the_window() {
        let mut ui = blank_ui();
        ui.list_total = 20;
        ui.list_visible = 8;
        ui.list_vis_eff = 8;
        ui.list_follow = 1;
        ui.list_move_cursor(5);
        assert_eq!((ui.list_cursor(), ui.list_offset()), (5, 0));
        ui.list_move_cursor(5);
        assert_eq!((ui.list_cursor(), ui.list_offset()), (10, 3));
        ui.list_move_cursor(-20);
        assert_eq!((ui.list_cursor(), ui.list_offset()), (0, 0));
        ui.list_cursor_end();
        assert_eq!((ui.list_cursor(), ui.list_offset()), (19, 12));
        ui.list_cursor_home();
        assert_eq!((ui.list_cursor(), ui.list_offset()), (0, 0));
    }

    #[test]
    fn list_view_scroll_never_strands_the_cursor() {
        let mut ui = blank_ui();
        ui.list_total = 20;
        ui.list_visible = 8;
        ui.list_vis_eff = 8;
        ui.list_follow = 1;
        ui.list_set_cursor(5);
        ui.list_scroll_by(3); // window moves, cursor stays if still visible
        assert_eq!((ui.list_cursor(), ui.list_offset()), (5, 3));
        ui.list_scroll_by(10); // window would strand it: cursor pulled along
        assert_eq!(ui.list_offset(), 12);
        assert_eq!(ui.list_cursor(), 12);
        ui.list_scroll_by(-99);
        assert_eq!((ui.list_cursor(), ui.list_offset()), (7, 0));
    }

    #[test]
    fn list_short_playlist_pins_everything() {
        let mut ui = blank_ui();
        ui.list_total = 3;
        ui.list_visible = 3;
        ui.list_vis_eff = 3;
        ui.list_follow = 1;
        ui.list_cursor_end();
        assert_eq!((ui.list_cursor(), ui.list_offset()), (2, 0));
        ui.list_scroll_by(5);
        assert_eq!((ui.list_cursor(), ui.list_offset()), (2, 0));
        ui.list_scroll_ratio(1.0);
        assert_eq!((ui.list_cursor(), ui.list_offset()), (2, 0));
    }

    #[test]
    fn list_drag_tracks_rows_one_to_one() {
        let mut ui = blank_ui();
        ui.list_total = 30;
        ui.list_visible = 10;
        ui.list_vis_eff = 10;
        ui.list_follow = 1;
        ui.list_drag_to(5, 4, 8); // three rows down = three rows scrolled
        assert_eq!(ui.list_offset(), 7);
        ui.list_drag_to(5, 4, 0); // five rows up, clamped at the top
        assert_eq!(ui.list_offset(), 0);
        ui.list_drag_to(5, 18, 9);
        assert_eq!(ui.list_offset(), 20); // clamped at max, cursor in view
        assert!(ui.list_cursor() >= ui.list_offset());
    }

    #[test]
    fn list_bar_click_classifies_thumb() {
        let mut ui = blank_ui();
        ui.list_track_y = 10;
        ui.list_thumb_y = 2;
        ui.list_thumb_h = 3;
        assert_eq!(ui.list_bar_click(9), BarClick::Above);
        assert_eq!(ui.list_bar_click(11), BarClick::Above);
        assert_eq!(ui.list_bar_click(12), BarClick::Thumb);
        assert_eq!(ui.list_bar_click(14), BarClick::Thumb);
        assert_eq!(ui.list_bar_click(15), BarClick::Below);
    }

    #[test]
    fn list_follow_recounts_on_track_change_only() {
        let mut ui = blank_ui();
        ui.list_total = 20;
        ui.list_visible = 8;
        ui.follow_list_track(6); // open on track 6: cursor follows, soft-center
        assert_eq!(ui.list_cursor(), 5);
        assert_eq!(ui.list_offset(), 5 - 8 / 3);
        ui.list_scroll_by(-2); // user looks around: sticks
        assert_eq!(ui.list_offset(), 1);
        ui.follow_list_track(6); // same track: no snap-back
        assert_eq!((ui.list_cursor(), ui.list_offset()), (5, 1));
        ui.follow_list_track(18); // new track: cursor follows again
        assert_eq!(ui.list_cursor(), 17);
    }

    #[test]
    fn lyrics_dock_toggle_and_ldm_progress() {
        let mut ui = blank_ui();
        assert!(!ui.lyrics_open());
        ui.toggle_lyrics();
        assert!(ui.lyrics_open());
        // LDM: instant open progress.
        assert_eq!(ui.lyrics_progress(true), 1.0);
        assert!(ui.lyrics_visible());
        ui.close_lyrics();
        assert!(!ui.lyrics_open());
        assert_eq!(ui.lyrics_progress(true), 0.0);
    }

    #[test]
    fn lyrics_manual_scroll_and_resume() {
        let mut ui = blank_ui();
        ui.toggle_lyrics();
        assert!(ui.lyrics_manual.is_none());
        ui.lyrics_scroll_by(2, 10, 3);
        assert_eq!(ui.lyrics_manual, Some(2));
        ui.lyrics_scroll_by(99, 10, 3);
        assert_eq!(ui.lyrics_manual, Some(7)); // clamped at max
        ui.lyrics_scroll_by(-99, 10, 3);
        assert_eq!(ui.lyrics_manual, Some(0));
        ui.lyrics_resume_follow();
        assert!(ui.lyrics_manual.is_none());
    }

    #[test]
    fn lyrics_hidden_pos_disables_strip() {
        let mut ui = blank_ui();
        ui.config.lyrics_pos = LyricsPos::Hidden;
        ui.toggle_lyrics(); // re-opens below instead of dead-ending
        assert_eq!(ui.config.lyrics_pos, LyricsPos::Below);
        assert!(ui.lyrics_visible());
    }
}
