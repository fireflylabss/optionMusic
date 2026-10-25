//! Terminal UI — black & white, compact, centered, zero-leak (alternate screen).

use std::collections::VecDeque;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor, Stylize},
    terminal::{
        BeginSynchronizedUpdate, Clear, ClearType, EndSynchronizedUpdate, EnterAlternateScreen,
        LeaveAlternateScreen, disable_raw_mode, enable_raw_mode, size,
    },
};

use unicode_width::UnicodeWidthStr;

use crate::cava::CavaBridge;
use crate::config::{Accent, AppConfig, CavaStyle, LyricsPos, ToastPos};
use crate::lyrics::LrcLine;
use crate::settings::{self, SETTINGS_SIDEBAR_W, SettingsAction, SettingsUi};

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

pub const APP_NAME: &str = "still";

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
/// Narrowest a rail may shrink before it collapses to a badge row.
pub const RAIL_MIN_W: usize = 12;
/// Badge shown when a rail collapses on tiny terminals.
pub const RAIL_COLLAPSED_HINT: &str = "···";

/// Resolved horizontal layout: every region owns disjoint columns that
/// always sum to exactly `cols`. Rails never overlap the player, each
/// other, or the terminal edge — on tiny screens rails shrink, then the
/// player clamps to its minimum, then rails collapse to a badge.
#[derive(Debug, Clone, Copy)]
pub struct Layout {
    pub cols: usize,
    pub left_w: usize,
    pub content_x0: usize,
    pub content_cols: usize,
    pub right_w: usize,
    /// True when the left rail collapsed (only a badge paints).
    pub left_collapsed: bool,
    /// True when the right rail collapsed.
    pub right_collapsed: bool,
}

impl Layout {
    pub fn resolve(cols: usize, left_want: usize, right_want: usize) -> Self {
        let cols = cols.max(1);
        let player_min = PLAYER_MIN_W.min(cols);
        let mut left_w = left_want.min(cols);
        let mut right_w = right_want.min(cols.saturating_sub(left_w));
        // Pass 1: fit both rails + player minimum.
        let mut free = cols.saturating_sub(left_w + right_w);
        if free < player_min {
            // Pass 2: shrink rails down to RAIL_MIN_W, biggest first.
            let mut need = player_min.saturating_sub(free);
            while need > 0 && (left_w > RAIL_MIN_W || right_w > RAIL_MIN_W) {
                if left_w >= right_w && left_w > RAIL_MIN_W {
                    left_w -= 1;
                } else if right_w > RAIL_MIN_W {
                    right_w -= 1;
                } else if left_w > RAIL_MIN_W {
                    left_w -= 1;
                } else {
                    break;
                }
                need -= 1;
            }
            free = cols.saturating_sub(left_w + right_w);
        }
        // Pass 3: still no room — collapse rails to badges, smaller first,
        // until the player fits (or nothing is left to collapse).
        let mut left_collapsed = false;
        let mut right_collapsed = false;
        while free < player_min && (left_w > 0 || right_w > 0) {
            if left_w > 0 && left_w <= right_w {
                left_w = 0;
                left_collapsed = left_want > 0;
            } else if right_w > 0 {
                right_w = 0;
                right_collapsed = right_want > 0;
            } else if left_w > 0 {
                left_w = 0;
                left_collapsed = left_want > 0;
            } else {
                break;
            }
            free = cols.saturating_sub(left_w + right_w);
        }
        let content_cols = free.max(1);
        Self {
            cols,
            left_w,
            content_x0: left_w,
            content_cols,
            right_w,
            left_collapsed,
            right_collapsed,
        }
    }
}

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
    /// Lyrics face (`y`): synced lines, plain fallback, active index, status.
    pub lyrics_open: bool,
    pub lyrics_title: &'a str,
    pub lyrics_synced: &'a [LrcLine],
    pub lyrics_plain: &'a [String],
    pub lyrics_active: Option<usize>,
    pub lyrics_status: Option<&'a str>,
    /// Provider label (`lrclib`, `netease`, `ovh`, `sidecar`, `embedded`).
    pub lyrics_source: Option<&'a str>,
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
    /// Help rail scroll offset (internal scroll when sections > height).
    help_scroll: usize,
    /// List card pop timestamps (same geometry-only pop as `?` / `c`).
    list_opened_at: Option<Instant>,
    list_closed_at: Option<Instant>,
    /// Current lyric in the gap under the title (`y` toggles).
    show_lyrics: bool,
    /// Strip pop timestamps (geometry-only, never blocks input).
    lyrics_opened_at: Option<Instant>,
    lyrics_closed_at: Option<Instant>,
    /// Manual window override (Up/Down/wheel). `None` = auto-follow.
    lyrics_manual: Option<usize>,
    /// Smooth window top (float rows, eased toward target each frame).
    lyrics_smooth: f64,
    /// Track key the lyric window follows (reset on change).
    lyrics_track_key: String,
    /// Active lyric row (terminal y) from the last paint, for click resume.
    lyrics_active_row: Option<u16>,
    /// Body rows the last lyrics card actually drew. Scroll uses this.
    lyrics_shown: usize,
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
            help_scroll: 0,
            list_opened_at: None,
            list_closed_at: None,
            show_lyrics: false,
            lyrics_opened_at: None,
            lyrics_closed_at: None,
            lyrics_manual: None,
            lyrics_smooth: 0.0,
            lyrics_track_key: String::new(),
            lyrics_active_row: None,
            lyrics_shown: 1,
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

    /// Wheel over the settings rail.
    pub fn settings_scroll_by(&mut self, delta: i32) {
        self.settings.scroll_by(delta, 0);
    }

    /// Wheel over the help rail.
    pub fn pointer_over_help(&self, col: u16, row: u16) -> bool {
        self.help_contains(col, row)
    }

    /// Wheel over any open rail (list / settings / help). Used to route
    /// wheel events to the rail instead of seeking the player.
    pub fn pointer_over_rail(&self, col: u16, row: u16) -> bool {
        self.pointer_over_list(col, row)
            || self.pointer_over_settings(col, row)
            || self.pointer_over_help(col, row)
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
        let msg = msg
            .chars()
            .map(|c| if c.is_control() { ' ' } else { c })
            .collect();
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
        if let Some(s) = self.list_side_at_open
            && (self.show_list || self.list_progress(self.config.ldm) > 0.02)
        {
            return s;
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
        if let Some(s) = self.help_side_at_open
            && (self.show_help || self.help_progress(self.config.ldm) > 0.02)
        {
            return s;
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
        self.help_scroll = 0;
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

    /// Lyrics face (`y` toggles). Hidden config turns back on.
    pub fn try_toggle_lyrics(&mut self) -> bool {
        if self.show_lyrics {
            self.close_lyrics();
            return true;
        }
        if self.config.lyrics_pos == LyricsPos::Hidden {
            self.config.lyrics_pos = LyricsPos::Right;
            let _ = self.config.save();
        }
        self.show_lyrics = true;
        let now = Instant::now();
        self.lyrics_opened_at = Some(now);
        self.lyrics_closed_at = None;
        self.lyrics_manual = None;
        true
    }

    pub fn toggle_lyrics(&mut self) {
        let _ = self.try_toggle_lyrics();
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

    /// Scroll the card by the number of body rows it last painted.
    pub fn lyrics_scroll_view(&mut self, delta: i32, total: usize) {
        self.lyrics_scroll_by(delta, total, self.lyrics_shown.max(1));
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
    fn note_lyrics_track(&mut self, key: &str) {
        if key != self.lyrics_track_key {
            self.lyrics_track_key = key.to_string();
            self.lyrics_manual = None;
            self.lyrics_smooth = 0.0;
        }
    }

    /// Click inside the lyrics card (wheel target + active-line resume).
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

    /// One lyric in the blank row under the title. The row exists either way.
    fn paint_lyric_line(
        &mut self,
        out: &mut impl Write,
        y: u16,
        region_x: usize,
        region_w: usize,
        state: &FrameState<'_>,
        max_w: usize,
    ) -> io::Result<()> {
        self.lyrics_shown = 1;
        self.hits.lyrics_head = None;
        let synced = state.lyrics_synced;
        let plain = state.lyrics_plain;
        let total = if !synced.is_empty() {
            synced.len()
        } else {
            plain.len()
        };
        let (text, bright) = if total == 0 {
            (
                state.lyrics_status.unwrap_or("no lyrics found").to_string(),
                false,
            )
        } else {
            let followed = state.lyrics_active.unwrap_or(0).min(total - 1);
            let idx = self.lyrics_manual.unwrap_or(followed).min(total - 1);
            if total <= 1 {
                self.lyrics_manual = None;
            } else if let Some(manual) = self.lyrics_manual {
                self.lyrics_manual = Some(manual.min(total - 1));
            }
            self.lyrics_smooth = idx as f64;
            let line = if !synced.is_empty() {
                synced[idx].text.clone()
            } else {
                plain[idx].clone()
            };
            (line, true)
        };
        let shown = truncate(&text, max_w.max(1));
        let color = if bright { BRIGHT } else { DIM };
        let x = paint_in_region(out, y, region_x, region_w, &[Span::fg(color, &shown)])?;
        let w = str_width(&shown).min(region_w) as u16;
        self.lyrics_active_row = Some(y);
        self.hits.lyrics_pane = if self.show_lyrics && w > 0 {
            Some(HitRect { x, y, w, h: 1 })
        } else {
            None
        };
        Ok(())
    }

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

    /// Help rail scroll (plain offset; the paint clamps to content).
    pub fn help_scroll_by(&mut self, delta: i32) {
        if delta == 0 {
            return;
        }
        if delta < 0 {
            self.help_scroll = self.help_scroll.saturating_sub((-delta) as usize);
        } else {
            self.help_scroll = self.help_scroll.saturating_add(delta as usize);
        }
    }

    pub fn help_scroll_to_top(&mut self) {
        self.help_scroll = 0;
    }

    pub fn help_scroll_to_bottom(&mut self) {
        self.help_scroll = usize::MAX;
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
        if let Some(r) = self.hits.help_pane
            && r.contains(col, row)
        {
            return HitTarget::None;
        }
        // Docked lyrics (lines + pinned header) absorb clicks so they
        // never leak through to the player controls painted around them.
        if let Some(r) = self.hits.lyrics_pane
            && r.contains(col, row)
        {
            return HitTarget::None;
        }
        if let Some(r) = self.hits.lyrics_head
            && r.contains(col, row)
        {
            return HitTarget::None;
        }
        // Playlist scrollbar absorbs hits before track rows.
        if let Some(r) = self.hits.list_bar
            && r.contains(col, row)
        {
            return HitTarget::ListScroll(r.v_ratio_at(row));
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
        if let Some(r) = self.hits.progress
            && r.contains(col, row)
        {
            return HitTarget::Progress(r.ratio_at(col));
        }
        if let Some(r) = self.hits.prev
            && r.contains(col, row)
        {
            return HitTarget::Prev;
        }
        if let Some(r) = self.hits.next
            && r.contains(col, row)
        {
            return HitTarget::Next;
        }
        if let Some(r) = self.hits.play_pause
            && r.contains(col, row)
        {
            return HitTarget::PlayPause;
        }
        if let Some(r) = self.hits.volume_down
            && r.contains(col, row)
        {
            return HitTarget::VolumeDown;
        }
        if let Some(r) = self.hits.volume_up
            && r.contains(col, row)
        {
            return HitTarget::VolumeUp;
        }
        if let Some(r) = self.hits.volume
            && r.contains(col, row)
        {
            return HitTarget::Volume;
        }
        if let Some(r) = self.hits.eq
            && r.contains(col, row)
        {
            return HitTarget::Eq;
        }
        if let Some(r) = self.hits.speed
            && r.contains(col, row)
        {
            return HitTarget::Speed;
        }
        if let Some(r) = self.hits.pitch
            && r.contains(col, row)
        {
            return HitTarget::Pitch;
        }
        if let Some(r) = self.hits.cava
            && r.contains(col, row)
        {
            return HitTarget::CavaToggle;
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
        if let Some(r) = self.hits.help_pane
            && r.contains(col, row)
        {
            return HitTarget::None;
        }
        // Same for the docked lyrics (lines + pinned header).
        if let Some(r) = self.hits.lyrics_pane
            && r.contains(col, row)
        {
            return HitTarget::None;
        }
        if let Some(r) = self.hits.lyrics_head
            && r.contains(col, row)
        {
            return HitTarget::None;
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

    fn paint_compact_player(
        &mut self,
        out: &mut impl Write,
        state: &FrameState<'_>,
        rows: usize,
        x: usize,
        width: usize,
        lyrics_on: bool,
    ) -> io::Result<()> {
        self.hits.lyrics_pane = None;
        self.hits.lyrics_head = None;
        self.lyrics_active_row = None;
        if rows == 0 || width == 0 {
            return Ok(());
        }
        let title = truncate(state.track_name, width);
        paint_in_region(out, 0, x, width, &[Span::fg(BRIGHT, &title)])?;
        let mut y = 1;
        if rows >= 3 {
            let times = format!(
                "{} / {}",
                fmt_time(state.pos),
                state
                    .duration
                    .map(fmt_time)
                    .unwrap_or_else(|| "--:--".into())
            );
            paint_in_region(out, y, x, width, &[Span::fg(DIM, &times)])?;
            y += 1;
        }
        if rows >= 4 && width >= 8 {
            let bar_w = width.min(36);
            let (filled, knob, empty) = progress_parts(state.pos, state.duration, bar_w);
            let start = paint_in_region(
                out,
                y,
                x,
                width,
                &[
                    Span::fg(BRIGHT, &filled),
                    Span::fg(BRIGHT, &knob),
                    Span::fg(DARK, &empty),
                ],
            )?;
            self.hits.progress = Some(HitRect {
                x: start,
                y,
                w: bar_w as u16,
                h: 1,
            });
            y += 1;
        }
        if rows >= 2 {
            let status = if state.stopped {
                "stopped"
            } else if state.paused {
                "paused"
            } else {
                "playing"
            };
            let controls = truncate(
                &format!(
                    "{status}  {}/{}  {}%",
                    state.index, state.total, state.volume
                ),
                width,
            );
            let start = paint_in_region(out, y, x, width, &[Span::fg(GRAY, &controls)])?;
            self.hits.play_pause = Some(HitRect {
                x: start,
                y,
                w: str_width(&controls) as u16,
                h: 1,
            });
            y += 1;
        }
        if lyrics_on && (y as usize) < rows {
            self.note_lyrics_track(if state.track_path.is_empty() {
                state.track_name
            } else {
                state.track_path
            });
            self.paint_lyric_line(out, y, x, width, state, width)?;
            y += 1;
        }
        if (y as usize) < rows {
            paint_in_region(
                out,
                y,
                x,
                width,
                &[Span::fg(DIM, "space pause · c settings · ? help")],
            )?;
        }
        Ok(())
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
        let (rice, rice_active) = crate::rice::load();
        let (accent, accent_dim) = rice.accent_for(&self.config.accent, rice_active);

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

        // Slot policy: the list floats as a compact card — the center
        // block never shifts. Lyrics take that column (`y`), or sit beside
        // it once the terminal is wide enough to read both.
        if self.config.lyrics_pos == LyricsPos::Hidden {
            self.show_lyrics = false;
        }
        let lyrics_on =
            state.lyrics_open && self.config.lyrics_pos != LyricsPos::Hidden && !self.preview;
        // Horizontal layout: one pass resolves disjoint columns for the
        // left rail, the player, and the right rail — they always sum to
        // exactly `cols`, so nothing ever paints over anything else, no
        // matter how small the terminal gets.
        let list_want = if list_visual {
            LIST_SIDEBAR_W.min(cols.saturating_sub(4)).max(20.min(cols))
        } else {
            0
        };
        let settings_want = if settings_visual {
            SETTINGS_SIDEBAR_W.min(cols).max(24.min(cols))
        } else {
            0
        };
        let help_want = if help_visual {
            HELP_SIDEBAR_W.min(cols.max(16)).max(12.min(cols))
        } else {
            0
        };
        let list_left = self.show_list && list_visual && self.list_side() == PanelSide::Left;
        let settings_left = settings_visual && self.settings_side() == PanelSide::Left;
        let help_left = help_visual && self.help_side() == PanelSide::Left;
        let left_want = (if list_left { list_want } else { 0 })
            .max(if settings_left { settings_want } else { 0 })
            .max(if help_left { help_want } else { 0 });
        let list_right_side = self.show_list && list_visual && self.list_side() == PanelSide::Right;
        let settings_right_side = settings_visual && self.settings_side() == PanelSide::Right;
        let help_right_side = help_visual && self.help_side() == PanelSide::Right;
        let right_want = (if list_right_side { list_want } else { 0 })
            .max(if settings_right_side {
                settings_want
            } else {
                0
            })
            .max(if help_right_side { help_want } else { 0 });
        let layout = Layout::resolve(cols, left_want, right_want);
        let content_x0 = layout.content_x0;
        let content_cols = layout.content_cols;
        // Overlay geometry only — never subtracted from the player region.

        // Freeze ambient motion while a card floats above the player so the
        // 60fps repaint is byte-identical (no shimmer bleeding around/under
        // the overlay). Covers the close-pop too. Resumes untouched after.
        let frozen = settings_visual || help_visual || list_visual;
        let pulse = |period: f64| {
            if frozen { 0.5 } else { breath(t, period) }
        };

        if rows < 12 || content_cols < PLAYER_MIN_W {
            self.paint_compact_player(&mut out, state, rows, content_x0, content_cols, lyrics_on)?;
        } else {
            let block_w = content_cols.saturating_sub(4).min(56);
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

            // Playlist rail — fills the free height down the side.
            // Header (title + hairline) + footer hint + divider take 5 rows.
            if self.show_list {
                let cap = rows.saturating_sub(6).max(3);
                self.list_visible = state.list_names.len().max(1).min(cap);
                self.list_total = state.list_names.len();
                self.follow_list_track(state.index);
                let vis = self.list_visible.max(1);
                self.list_scroll = clamp_list_offset(self.list_total, vis, self.list_scroll);
            }

            let show_cava_strip = cava_levels.is_some();

            if lyrics_on {
                let key = if state.track_path.is_empty() {
                    state.track_name.to_string()
                } else {
                    state.track_path.to_string()
                };
                self.note_lyrics_track(&key);
            }

            // Same height as the classic block. The lyric uses the blank row
            // under the title, so opening it does not move the player.
            let mut block_h = 8usize;
            if !self.show_path {
                block_h = block_h.saturating_sub(1);
            }
            block_h += 1; // meta
            block_h += 1; // footer

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

            let title_c = if rice_active || self.config.accent != Accent::Default {
                mix_rgb(rice.muted, accent, title_in * intro)
            } else {
                gray(lerp(70.0, 245.0, title_in * intro))
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
                y += 1;
            }
            if lyrics_on {
                self.paint_lyric_line(
                    &mut out,
                    y as u16,
                    content_x0,
                    content_cols,
                    state,
                    max_title,
                )?;
            } else {
                self.hits.lyrics_pane = None;
                self.hits.lyrics_head = None;
                self.lyrics_active_row = None;
            }
            y += 1;

            let knob_c = if playing && !ldm && !frozen {
                mix_rgb(accent_dim, accent, pulse(2.0))
            } else if state.paused {
                rice.body
            } else {
                rice.title
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
                w: str_width(prev_g) as u16,
                h: 1,
            });
            cx += str_width(prev_g) as u16 + 2;
            let pp_w = (str_width(icon) + 2 + str_width(status)) as u16;
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
                w: str_width(next_g) as u16,
                h: 1,
            });
            // Volume controls sit at the end: − vol +
            let vol_plus_w = str_width(vol_plus) as u16;
            let vol_w = str_width(&vol) as u16;
            let vol_minus_w = str_width(vol_minus) as u16;
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
            if state.smart_shuffle {
                meta_spans.push(Span::fg(DARK, "  ·  "));
                meta_spans.push(Span::fg(DIM, "smart"));
            }
            if let Some(sleep) = state.sleep_label
                && !sleep.is_empty()
            {
                meta_spans.push(Span::fg(DARK, "  ·  "));
                meta_spans.push(Span::fg(DIM, "sleep "));
                meta_spans.push(Span::fg(GRAY, sleep));
            }
            let meta_x =
                paint_in_region(&mut out, y as u16, content_x0, content_cols, &meta_spans)?;
            let mut mx = meta_x;
            self.hits.speed = Some(HitRect {
                x: mx,
                y: y as u16,
                w: (4 + str_width(&spd)) as u16,
                h: 1,
            });
            mx += 4 + str_width(&spd) as u16;
            mx += 5;
            self.hits.pitch = Some(HitRect {
                x: mx,
                y: y as u16,
                w: (5 + str_width(&ptch)) as u16,
                h: 1,
            });
            mx += 5 + str_width(&ptch) as u16;
            mx += 5;
            self.hits.eq = Some(HitRect {
                x: mx,
                y: y as u16,
                w: (3 + str_width(eq)) as u16,
                h: 1,
            });
            if spans_width(&status_spans) > content_cols {
                self.hits.prev = None;
                self.hits.play_pause = None;
                self.hits.next = None;
                self.hits.volume_down = None;
                self.hits.volume = None;
                self.hits.volume_up = None;
            }
            if spans_width(&meta_spans) > content_cols {
                self.hits.speed = None;
                self.hits.pitch = None;
                self.hits.eq = None;
            }
            y += 1;
            y += 1; // one empty breathing row before the footer bar
            let footer_dim = mix_rgb(rice.muted, Color::Black, 1.0 - intro);
            let footer_key = mix_rgb(rice.body, rice.muted, 1.0 - intro * 0.7);
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
            // Cava overlays below the player — fixed offset, does not shift the block.
            let cava_rows = self.config.cava.rows as usize;
            if show_cava_strip && y + 2 + cava_rows < rows {
                let cava_y = y + 2;
                if let Some(ref levels) = cava_levels {
                    let intensity: f64 = if playing {
                        0.85
                    } else if state.paused {
                        0.32
                    } else {
                        0.14
                    };
                    let strip_w = block_w
                        .saturating_add(4)
                        .min(content_cols.saturating_sub(4));
                    let cava_color = if rice_active || self.config.accent != Accent::Default {
                        mix_rgb(rice.muted, accent_dim, intensity.clamp(0.2, 1.0))
                    } else {
                        mix(CAVA_DIM, CAVA_SOFT, intensity * 0.95)
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
        }

        if list_visual {
            let list_anchor_right = self.list_side() == PanelSide::Right;
            // Resolved width: the rail paints exactly its Layout columns.
            let list_resolved_w = if list_anchor_right {
                layout.right_w
            } else {
                layout.left_w
            };
            // Collapsed rail: badge only, no track painting.
            if (list_anchor_right && layout.right_collapsed)
                || (!list_anchor_right && layout.left_collapsed)
            {
                paint_rail_badge(&mut out, cols, rows, list_anchor_right, rice, "list")?;
                self.hits.list.clear();
                self.hits.list_bar = None;
                self.hits.list_pane = None;
                self.list_vis_eff = 0;
            } else {
                let (track_y, track_h, thumb_y, thumb_h, vis_eff) = paint_list_sidebar(
                    &mut out,
                    &mut self.hits,
                    cols,
                    rows,
                    list_resolved_w.max(RAIL_MIN_W.min(list_want.max(1))),
                    self.list_scroll,
                    self.list_visible,
                    self.list_cursor,
                    state.index,
                    state.list_names,
                    playing,
                    t,
                    ldm,
                    frozen,
                    rice,
                    list_prog,
                    self.show_list,
                    list_anchor_right,
                )?;
                // Remember exactly what was painted so input math (wheel,
                // drag, paging) matches the pixels — even mid-pop.
                self.list_vis_eff = vis_eff;
                self.list_track_y = track_y;
                self.list_track_h = track_h;
                self.list_thumb_y = thumb_y;
                self.list_thumb_h = thumb_h;
            }
        } else {
            self.hits.list.clear();
            self.hits.list_bar = None;
            self.hits.list_pane = None;
        }

        // Settings card uses its sticky open-time side; help uses its own
        // sticky side. The two never share a side (second opener takes the
        // free one). Both paint exactly their Layout columns.
        let settings_anchor_right = self.settings_side() == PanelSide::Right;
        let settings_resolved_w = if settings_anchor_right {
            layout.right_w
        } else {
            layout.left_w
        };
        let _settings_render_w = if settings_visual
            && !((settings_anchor_right && layout.right_collapsed)
                || (!settings_anchor_right && layout.left_collapsed))
        {
            settings::paint_settings_sidebar(
                &mut out,
                cols,
                rows,
                &mut self.settings,
                &self.config,
                rice,
                settings_anchor_right,
                settings_resolved_w.max(RAIL_MIN_W.min(settings_want.max(1))),
            )?
        } else {
            if settings_visual {
                paint_rail_badge(&mut out, cols, rows, settings_anchor_right, rice, "conf")?;
            }
            self.settings.close_paint_state();
            0
        };

        let (settings_left, settings_right) = self
            .settings
            .pane_rect()
            .map(|(x, _, w, _)| (x, x + w))
            .unwrap_or((0, 0));

        let help_anchor_right = self.help_side() == PanelSide::Right;
        let help_resolved_w = if help_anchor_right {
            layout.right_w
        } else {
            layout.left_w
        };
        let (help_render_w, help_scroll) = if help_visual
            && !((help_anchor_right && layout.right_collapsed)
                || (!help_anchor_right && layout.left_collapsed))
        {
            paint_help_sidebar(
                &mut out,
                cols,
                rows,
                &mut self.hits,
                help_resolved_w.max(RAIL_MIN_W.min(help_want.max(1))),
                rice,
                self.preview,
                help_prog,
                self.show_help,
                settings_right,
                settings_left,
                help_anchor_right,
                self.help_scroll,
            )?
        } else {
            if help_visual {
                paint_rail_badge(&mut out, cols, rows, help_anchor_right, rice, "help")?;
            }
            self.hits.help_pane = None;
            (0, 0)
        };
        self.help_scroll = help_scroll;
        let _ = help_render_w;

        // Toast stack — anchored inside the player region, never over a rail.
        let stack = self.live_toasts();
        if !stack.is_empty() {
            let pos = self.config.toast_pos;
            paint_toast_stack(
                &mut out,
                cols,
                rows,
                &stack,
                pos,
                ldm,
                rice,
                content_x0,
                content_cols,
            )?;
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
            ("d", "dislike skip"),
            ("s", "stop"),
            ("l", "list"),
            ("r", "shuffle"),
            ("o", "repeat cycle"),
        ],
    ),
    (
        "queue",
        &[
            ("J K", "move track"),
            ("a", "play next"),
            ("A", "append end"),
            ("D", "clear keep cur"),
            ("u", "undo"),
            ("g", "jump current"),
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
            ("y", "lyrics"),
            ("x", "smart mix"),
            ("z", "sleep"),
            ("c", "settings"),
            ("↑↓ jk", "list / next"),
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

/// Collapsed-rail badge: when the terminal is too narrow for a rail plus
/// the player minimum, the rail shrinks to this one-cell hint at the
/// screen edge instead of overlapping anything.
fn paint_rail_badge(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    anchor_right: bool,
    theme: crate::rice::UiTheme,
    label: &str,
) -> io::Result<()> {
    let x = if anchor_right {
        cols.saturating_sub(1)
    } else {
        0
    } as u16;
    let y = rows.saturating_sub(2).max(1) as u16;
    queue!(
        out,
        MoveTo(x, y),
        SetForegroundColor(theme.muted),
        Print(truncate(label, 1)),
        ResetColor
    )?;
    Ok(())
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
    theme: crate::rice::UiTheme,
    preview: bool,
    progress: f64,
    logically_open: bool,
    avoid_left: usize,
    avoid_right_edge: usize,
    anchor_right: bool,
    scroll: usize,
) -> io::Result<(usize, usize)> {
    let e = progress.clamp(0.0, 1.0);
    if e <= 0.02 {
        hits.help_pane = None;
        return Ok((0, 0));
    }

    let sections = if preview {
        HELP_SECTIONS_PREVIEW
    } else {
        HELP_SECTIONS
    };
    let border = theme.border;
    let accent = theme.accent;

    // Rail width comes resolved from Layout — paint exactly these columns.
    // `avoid_*` only shifts the anchor so two rails never share a side.
    let box_w = sidebar_w.max(8).min(cols.max(1));
    let _ = (avoid_left, avoid_right_edge);
    // Pop geometry: pinned edge stays, width eases to full. Height is
    // always full so the rail never floats mid-screen.
    let (render_w, render_h) = if e >= 0.999 {
        (box_w, rows)
    } else {
        (
            ((box_w as f64 * (0.55 + 0.45 * e)) as usize)
                .max(12)
                .min(box_w),
            rows,
        )
    };
    let x0 = if anchor_right {
        cols.saturating_sub(render_w)
    } else {
        0usize
    };
    let ry0 = 0usize;
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

    // No solid fill: the rail is transparent over the terminal
    // background, with a single divider line at its inner edge.
    let divider_x = if anchor_right { x0 } else { x0 + render_w - 1 };
    for row in ry0..=y1.min(rows.saturating_sub(1)) {
        queue!(
            out,
            MoveTo(divider_x as u16, row as u16),
            SetForegroundColor(border),
            Print("│"),
            ResetColor
        )?;
    }
    // Body text with 1-col padding (clipped while popping).
    let render_inner = render_w.saturating_sub(4).max(8);
    let text_x = x0 + 2;
    let last = rows.saturating_sub(1);
    let mut y = 1usize;
    let mut paint_row = |y: usize, spans: &[Span<'_>]| -> io::Result<()> {
        queue!(out, MoveTo(text_x as u16, y as u16))?;
        paint_spans(out, spans)
    };

    // Scrollable window: flat rows page through `scroll`, chrome (hint)
    // always stays. Never pushes the player — clips inside the rail.
    let flat: Vec<(bool, String, String)> = sections
        .iter()
        .flat_map(|(title, rows_sec)| {
            let mut v = vec![(true, title.to_string(), String::new())];
            v.extend(
                rows_sec
                    .iter()
                    .map(|(k, a)| (false, k.to_string(), a.to_string())),
            );
            v
        })
        .collect();
    // Rows for content: everything below row 1, minus the hint line.
    let view_h = last.saturating_sub(1).saturating_sub(1).max(1);
    let scrolling = flat.len() > view_h;
    let scroll = scroll.min(flat.len().saturating_sub(view_h));
    for (is_title, a, b) in flat.iter().skip(scroll).take(view_h) {
        if y > last.saturating_sub(1) {
            break;
        }
        if *is_title {
            paint_row(y, &[Span::fg(theme.title, a)])?;
        } else {
            let key_col = format!("{a:<6}");
            let action_t = truncate(b, render_inner.saturating_sub(8));
            paint_row(
                y,
                &[
                    Span::fg(accent, &key_col),
                    Span::fg(theme.muted, " "),
                    Span::fg(theme.body, &action_t),
                ],
            )?;
        }
        y += 1;
    }
    if y <= last {
        let hint_text = if scrolling {
            format!(
                "h close · {}/{}",
                scroll + 1,
                flat.len().saturating_sub(view_h) + 1
            )
        } else {
            "h  close".to_string()
        };
        paint_row(y, &[Span::fg(theme.muted, &hint_text)])?;
    }
    Ok((render_w, scroll))
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
    theme: crate::rice::UiTheme,
    progress: f64,
    logically_open: bool,
    anchor_right: bool,
) -> io::Result<(u16, u16, u16, u16, usize)> {
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
    let border = theme.border;
    let accent = theme.accent;
    let accent_dim = theme.accent_dim;
    // Rail width comes resolved from Layout — paint exactly these columns.
    let box_w = sidebar_w.max(8).min(cols.max(1));

    // Pop geometry: pinned edge stays, width eases to full. Height is
    // always full so the rail never floats mid-screen.
    let (sidebar_w, list_h) = if e >= 0.999 {
        (box_w, rows)
    } else {
        (
            ((box_w as f64 * (0.55 + 0.45 * e)) as usize)
                .max(8)
                .min(box_w),
            rows,
        )
    };
    let x0 = if anchor_right {
        cols.saturating_sub(sidebar_w)
    } else {
        0usize
    };
    let y0 = 0usize;
    let track_x = x0 + sidebar_w.saturating_sub(2);
    let track_y = (y0 + LIST_SIDEBAR_HEADER) as u16;
    // Track area stops one row above the bottom so the pinned footer
    // hint never shares a row with a track.
    let track_h = list_h
        .saturating_sub(LIST_SIDEBAR_HEADER + LIST_SIDEBAR_FOOTER + 1)
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

    // Transparent rail over the terminal background, with a single
    // divider line at its inner edge.
    let divider_x = if anchor_right { x0 } else { x0 + sidebar_w - 1 };
    for row in y0..(y0 + list_h).min(rows) {
        queue!(
            out,
            MoveTo(divider_x as u16, row as u16),
            SetForegroundColor(border),
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
        SetForegroundColor(theme.title),
        Print(truncate(&head, inner_w)),
        ResetColor
    )?;
    queue!(
        out,
        MoveTo(tx, (y0 + 2) as u16),
        SetForegroundColor(border),
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
            SetForegroundColor(theme.muted),
            Print("(empty)"),
            ResetColor
        )?;
    } else {
        // One truncate only: marker + number + name clipped to the exact
        // free cells (`inner_w`), so wide chars can never spill past the
        // divider. Display-width aware (CJK/emoji = 2 cells).
        let num_w = total.to_string().len();
        for i in 0..vis {
            let idx = scroll + i;
            if idx >= total {
                break;
            }
            let track_n = idx + 1;
            let current = track_n == current_1based;
            let selected = idx == cursor_0based.min(total - 1);
            let marker = if current { "›" } else { " " };
            let prefix = format!("{marker}{track_n:>num_w$} ");
            let name_max = inner_w.saturating_sub(str_width(&prefix)).max(1);
            let chip = format!("{prefix}{}", truncate(&names[idx], name_max));
            let y = track_y.saturating_add(i as u16);
            let color = if current {
                if playing && !ldm && !frozen {
                    mix_rgb(accent_dim, accent, pulse(2.6))
                } else {
                    accent
                }
            } else if selected {
                // Keyboard cursor — bright so it reads apart from playback.
                theme.cursor
            } else {
                theme.body
            };
            queue!(
                out,
                MoveTo(tx, y),
                SetForegroundColor(color),
                Print(&chip),
                ResetColor
            )?;
            hits.list.push((
                HitRect {
                    x: x0 as u16,
                    y,
                    w: track_x.saturating_sub(x0) as u16,
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

    // Footer hint pins to the bottom row of the rail (never mid-list,
    // never over the player's own footer).
    let foot_y = rows.saturating_sub(1);
    queue!(
        out,
        MoveTo(tx, foot_y as u16),
        SetForegroundColor(theme.muted),
        Print(&truncate("J/K move · a next", inner_w)),
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
            theme.body
        } else {
            theme.muted
        };
        queue!(
            out,
            MoveTo(track_x as u16, track_y + i),
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
#[allow(clippy::too_many_arguments)]
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
        let w = str_width(key) as u16;
        if x as usize + w as usize > region_x + region_w {
            break;
        }
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
    spans.iter().map(|s| str_width(s.text)).sum()
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
/// Spans wider than the region are clipped (display-width aware) so a rail
/// never bleeds into the player's area on narrow terminals.
fn paint_in_region(
    out: &mut impl Write,
    y: u16,
    region_x: usize,
    region_w: usize,
    spans: &[Span<'_>],
) -> io::Result<u16> {
    let w: usize = spans.iter().map(|s| str_width(s.text)).sum();
    if w <= region_w {
        let x = region_x + (region_w - w) / 2;
        queue!(out, MoveTo(x as u16, y))?;
        paint_spans(out, spans)?;
        return Ok(x as u16);
    }
    // Too wide: clip spans to fit, keeping whole chars (never split CJK).
    let mut remaining = region_w;
    queue!(out, MoveTo(region_x as u16, y))?;
    for span in spans {
        if remaining == 0 {
            break;
        }
        let clipped = truncate(span.text, remaining);
        remaining = remaining.saturating_sub(str_width(&clipped));
        if let Some(c) = span.color {
            queue!(out, SetForegroundColor(c))?;
        } else {
            queue!(out, ResetColor)?;
        }
        queue!(out, Print(&clipped))?;
    }
    queue!(out, ResetColor)?;
    Ok(region_x as u16)
}

/// Classic vertical cava bars under the shortcut bar (default bar look).
/// Returns (start_x, rows_painted).
#[allow(clippy::too_many_arguments)]
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
        block_w.div_ceil(2).clamp(8, n.min(block_w))
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
        for (r, line) in lines.iter_mut().enumerate().take(rows) {
            let from_bottom = rows - 1 - r;
            let cell_base = from_bottom * full;
            let ch = if eighths >= cell_base + full {
                RAMP[full]
            } else if eighths > cell_base {
                RAMP[eighths - cell_base]
            } else {
                RAMP[0]
            };
            line.push(ch);
            if gap && b + 1 < bar_cols {
                line.push(' ');
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
#[allow(clippy::too_many_arguments)]
fn paint_toast_stack(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    stack: &[(&str, ToastKind, f64)],
    pos: ToastPos,
    ldm: bool,
    theme: crate::rice::UiTheme,
    region_x: usize,
    region_w: usize,
) -> io::Result<()> {
    let region_w = region_w.min(cols.saturating_sub(region_x));
    if stack.is_empty() || region_w < 5 || rows < 3 {
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
        let color = mix_rgb(theme.muted, theme.body, alpha);
        let is_err = *kind == ToastKind::Error;
        let edge_c = if is_err {
            mix_rgb(theme.muted, theme.error, alpha)
        } else {
            mix(Color::Black, theme.muted, alpha)
        };
        let symbol_c = if is_err { theme.error } else { theme.title };

        let overhead = if stacked { 4 } else { 5 };
        let short = truncate(msg, region_w.saturating_sub(overhead).min(61));
        let inner = if stacked {
            format!("{} {}", kind.symbol(), short)
        } else {
            format!("{} {} ", kind.symbol(), short)
        };
        let w = str_width(&inner);
        let box_w = w + 2;
        // Anchor inside the player region — never over a rail.
        let region_end = region_x + region_w.max(1);
        let x = match pos {
            ToastPos::BottomCenter | ToastPos::TopCenter => {
                region_x + region_w.saturating_sub(box_w) / 2
            }
            ToastPos::BottomLeft | ToastPos::TopLeft => region_x + 1,
            ToastPos::BottomRight | ToastPos::TopRight => {
                region_end.saturating_sub(box_w + 1).max(region_x)
            }
        };
        let x = x.min(region_end.saturating_sub(box_w).max(region_x));
        // 3-row box; newest hugs the edge, older ones step away.
        let base_y = if top_anchor {
            1usize
        } else {
            // Keep the bottom border off the last terminal row.
            rows.saturating_sub(4)
        };
        let offset = newest_first * step;
        if !top_anchor && offset > base_y {
            continue;
        }
        let y = if top_anchor {
            base_y + offset
        } else {
            base_y - offset
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

/// Binary name as invoked (`still` or `msc`).
pub fn bin_name() -> String {
    std::env::args()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "still".into())
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

fn str_width(s: &str) -> usize {
    UnicodeWidthStr::width(s)
}

fn truncate(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    if str_width(s) <= max {
        return s.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    // Reserve 1 cell for the ellipsis; never split a wide char.
    let mut out = String::new();
    for c in s.chars() {
        out.push(c);
        if str_width(&out) + 1 > max {
            out.pop();
            break;
        }
    }
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_paints_inside(bytes: &[u8], cols: usize, rows: usize) {
        let output = String::from_utf8_lossy(bytes);
        let mut rest = output.as_ref();
        let (mut x, mut y) = (0, 0);
        while !rest.is_empty() {
            if let Some(csi) = rest.strip_prefix("\x1b[") {
                let end = csi.find(|c: char| ('@'..='~').contains(&c)).unwrap();
                let (args, command) = csi.split_at(end);
                if command.starts_with('H') {
                    let mut parts = args.split(';').map(|n| n.parse::<usize>().unwrap());
                    y = parts.next().unwrap() - 1;
                    x = parts.next().unwrap() - 1;
                }
                rest = &command[1..];
            } else {
                let end = rest.find('\x1b').unwrap_or(rest.len());
                let text = &rest[..end];
                let width = str_width(text);
                assert!(y < rows, "row {y} outside {rows}: {text:?}");
                assert!(
                    x + width <= cols,
                    "columns {x}..{} outside {cols}: {text:?}",
                    x + width
                );
                x += width;
                rest = &rest[end..];
            }
        }
    }

    #[test]
    fn toast_bounds_and_unicode_borders() {
        assert_eq!(str_width("✓ ♪ e\u{301}"), 5);
        assert_eq!(str_width("☀\u{fe0f}"), 2);
        for cols in [1, 4, 5, 8, 20, 40, 80] {
            for rows in [1, 2, 3, 4, 8, 24] {
                for pos in [
                    ToastPos::TopLeft,
                    ToastPos::TopCenter,
                    ToastPos::TopRight,
                    ToastPos::BottomLeft,
                    ToastPos::BottomCenter,
                    ToastPos::BottomRight,
                ] {
                    for kind in [
                        ToastKind::Info,
                        ToastKind::Track,
                        ToastKind::Config,
                        ToastKind::Error,
                    ] {
                        let mut buf = Vec::new();
                        paint_toast_stack(
                            &mut buf,
                            cols,
                            rows,
                            &[("日本語 e\u{301} ☀\u{fe0f} message", kind, 0.5); 3],
                            pos,
                            true,
                            crate::rice::UiTheme::fallback(),
                            0,
                            cols,
                        )
                        .unwrap();
                        assert_paints_inside(&buf, cols, rows);
                    }
                }
            }
        }
    }

    #[test]
    fn compact_player_keeps_playback_inside_small_viewports() {
        let plain = vec!["日本語 e\u{301} ☀\u{fe0f} lyrics".to_string()];
        let state = FrameState {
            track_name: "日本語 e\u{301} ☀\u{fe0f} track title",
            track_path: "track.mp3",
            index: 1,
            total: 100,
            pos: Duration::from_secs(65),
            duration: Some(Duration::from_secs(240)),
            volume: 80,
            muted: false,
            speed: 1.0,
            pitch: 1.0,
            eq_label: "flat",
            paused: false,
            stopped: false,
            loop_label: "off",
            sleep_label: None,
            smart_shuffle: false,
            lyrics_open: true,
            lyrics_title: "lyrics",
            lyrics_synced: &[],
            lyrics_plain: &plain,
            lyrics_active: None,
            lyrics_status: None,
            lyrics_source: None,
            lyrics_now: Duration::ZERO,
            list_names: &[],
            toast: None,
        };
        for cols in [1, 4, 8, 16, 24, 33, 80] {
            for rows in 1..12 {
                let mut ui = blank_ui();
                let mut buf = Vec::new();
                ui.paint_compact_player(&mut buf, &state, rows, 0, cols, true)
                    .unwrap();
                assert_paints_inside(&buf, cols, rows);
                if rows >= 4 && cols >= 8 {
                    assert!(ui.hits.progress.is_some());
                }
            }
        }
    }

    #[test]
    fn toast_text_cannot_inject_cursor_controls() {
        let mut ui = blank_ui();
        ui.toast("saved\nnext\t\x1b[2J");
        assert_eq!(ui.toast_text(), Some("saved next  [2J"));
    }

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
    fn truncate_respects_display_width() {
        // CJK chars are 2 cells: 7 hiragana = 14 cells.
        assert_eq!(str_width("はいよろこんで"), 14);
        let t = truncate("はいよろこんで", 8);
        assert!(str_width(&t) <= 8, "spills: {t}");
        assert!(t.ends_with('…'));
        // Narrow max never splits a wide char.
        assert_eq!(truncate("はい", 2), "…");
        // Mixed content from the real library (kpop filename).
        let t2 = truncate("(여자)아이들((G)I-DLE) - 'Nxde' Official Music Video", 20);
        assert!(str_width(&t2) <= 20, "spills: {t2}");
    }

    #[test]
    fn rail_never_eats_player_minimum() {
        // Layout::resolve keeps every region disjoint at any width.
        for cols in [20, 30, 40, 50, 60, 80, 100, 120, 200] {
            for (lw, rw) in [(30, 0), (0, 26), (30, 26), (30, 30), (0, 0)] {
                let l = Layout::resolve(cols, lw, rw);
                assert_eq!(
                    l.left_w + l.content_cols + l.right_w,
                    cols,
                    "regions tile cols={cols} lw={lw} rw={rw}"
                );
                assert_eq!(l.content_x0, l.left_w);
                assert!(l.content_cols >= 1);
                if cols >= PLAYER_MIN_W + RAIL_MIN_W {
                    assert!(
                        l.content_cols >= PLAYER_MIN_W.min(cols)
                            || l.left_collapsed
                            || l.right_collapsed,
                        "player squeezed at cols={cols}"
                    );
                }
            }
        }
        // Wide terminal: rails keep full width, player gets the rest.
        let l = Layout::resolve(120, 30, 26);
        assert_eq!((l.left_w, l.right_w), (30, 26));
        assert_eq!(l.content_cols, 120 - 30 - 26);
        assert!(!l.left_collapsed && !l.right_collapsed);
        // Tiny terminal: rails collapse to badges instead of overlapping.
        let l = Layout::resolve(30, 30, 26);
        assert!(l.left_collapsed || l.right_collapsed);
        assert_eq!(l.left_w + l.content_cols + l.right_w, 30);
    }

    #[test]
    fn player_region_clips_wide_spans() {
        // Narrow free area: a 40-cell footer in a 20-cell region clips
        // instead of bleeding into the rail.
        let mut buf: Vec<u8> = Vec::new();
        let spans = [
            Span::fg(GRAY, "space"),
            Span::fg(DIM, "  ·  "),
            Span::fg(GRAY, "n/p ←→ +/− v c ? extra long tail here"),
        ];
        paint_in_region(&mut buf, 0, 30, 20, &spans).unwrap();
        let s = String::from_utf8_lossy(&buf);
        // Strip ANSI, measure the visible tail.
        let mut visible = 0usize;
        let mut in_esc = false;
        for ch in s.chars() {
            if ch == '\x1b' {
                in_esc = true;
                continue;
            }
            if in_esc {
                if ch == 'm' {
                    in_esc = false;
                }
                continue;
            }
            visible += unicode_width::UnicodeWidthChar::width(ch).unwrap_or(0);
        }
        assert!(visible <= 20, "footer bleeds {visible} cells into the rail");
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
        const { assert!(LIST_DOCK_MIN_COLS >= PLAYER_MIN_W + LIST_SIDEBAR_W) };
        // Narrow terminals must overlay instead of docking.
        const { assert!(LIST_DOCK_MIN_COLS > 60) };
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
            help_scroll: 0,
            list_opened_at: None,
            list_closed_at: None,
            show_lyrics: false,
            lyrics_opened_at: None,
            lyrics_closed_at: None,
            lyrics_manual: None,
            lyrics_smooth: 0.0,
            lyrics_track_key: String::new(),
            lyrics_active_row: None,
            lyrics_shown: 1,
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
        ui.toggle_lyrics(); // hidden turns back on
        assert_eq!(ui.config.lyrics_pos, LyricsPos::Right);
        assert!(ui.lyrics_visible());
    }
}
