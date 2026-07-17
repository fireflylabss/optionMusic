//! Terminal UI — black & white, compact, centered, zero-leak (alternate screen).

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{DisableMouseCapture, EnableMouseCapture},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor, Stylize},
    terminal::{
        disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

use crate::cava::CavaBridge;

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

pub const APP_NAME: &str = "optMusic";

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
    /// 1-based playlist jump.
    Jump(usize),
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
    /// (hit rect, 1-based track index)
    list: Vec<(HitRect, usize)>,
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
    pub show_list: bool,
    pub list_names: &'a [String],
    pub toast: Option<&'a str>,
}

/// Owns terminal mode for the playback session (alternate screen + raw + hide cursor).
///
/// Alternate screen keeps scrollback clean: the real buffer is restored on leave/Drop.
pub struct SessionUi {
    toast: Option<(String, Instant)>,
    show_list: bool,
    show_help: bool,
    /// Path line under the track title (session-persistent; `f` toggles).
    show_path: bool,
    /// When true, Drop skips terminal restore (after explicit leave()).
    detached: bool,
    /// Global clock for ambient motion.
    t0: Instant,
    /// Track identity for title fade-in.
    track_key: String,
    track_since: Instant,
    /// Click targets from the last `draw`.
    hits: HitMap,
    /// Optional cava spectrum background.
    cava: Option<CavaBridge>,
}

impl SessionUi {
    pub fn enter(enable_cava: bool) -> io::Result<Self> {
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
        let cava = if enable_cava {
            CavaBridge::try_start()
        } else {
            None
        };
        Ok(Self {
            toast: None,
            show_list: false,
            show_help: false,
            show_path: true,
            detached: false,
            t0: now,
            track_key: String::new(),
            track_since: now,
            hits: HitMap::default(),
            cava,
        })
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

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn toggle_list(&mut self) {
        self.show_list = !self.show_list;
        if self.show_list {
            self.show_help = false;
        }
    }

    pub fn toggle_help(&mut self) {
        self.show_help = !self.show_help;
        if self.show_help {
            self.show_list = false;
        }
    }

    /// Toggle the filename/path line. Returns `true` when shown.
    pub fn toggle_path(&mut self) -> bool {
        self.show_path = !self.show_path;
        self.show_path
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

    pub fn cava_active(&self) -> bool {
        self.cava.is_some()
    }

    pub fn show_list(&self) -> bool {
        self.show_list
    }

    pub fn show_help(&self) -> bool {
        self.show_help
    }

    /// Resolve a mouse position against the last drawn frame.
    pub fn hit_target(&self, col: u16, row: u16) -> HitTarget {
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
        for (r, idx) in &self.hits.list {
            if r.contains(col, row) {
                return HitTarget::Jump(*idx);
            }
        }
        HitTarget::None
    }

    /// Progress ratio from column while dragging (ignores row).
    pub fn progress_ratio_at_col(&self, col: u16) -> Option<f64> {
        self.hits.progress.map(|r| r.ratio_at(col))
    }

    pub fn toast_text(&self) -> Option<&str> {
        match &self.toast {
            Some((msg, at)) if at.elapsed() < Duration::from_millis(2200) => Some(msg.as_str()),
            _ => None,
        }
    }

    fn expire_toast(&mut self) {
        if let Some((_, at)) = &self.toast {
            if at.elapsed() >= Duration::from_millis(2200) {
                self.toast = None;
            }
        }
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
        queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;

        let t = self.t0.elapsed().as_secs_f64();
        let playing = !state.paused && !state.stopped;

        let intro = ease_out_cubic((self.t0.elapsed().as_secs_f64() / 0.55).clamp(0.0, 1.0));
        let title_in =
            ease_out_cubic((self.track_since.elapsed().as_secs_f64() / 0.4).clamp(0.0, 1.0));

        // Help opens as a left sidebar and shifts the player right.
        let sidebar_w = if self.show_help {
            HELP_SIDEBAR_W.min(cols.saturating_sub(24))
        } else {
            0
        };
        let content_cols = cols.saturating_sub(sidebar_w);
        let content_x0 = sidebar_w;

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
        let toast = state.toast.map(|m| truncate(m, max_title));
        let cava_levels = self.cava.as_ref().map(|c| c.snapshot());

        let mut list_rows: Vec<(bool, String, usize)> = Vec::new();
        if state.show_list {
            let room = rows.saturating_sub(16).min(10).max(3);
            let start = state
                .index
                .saturating_sub(1)
                .saturating_sub(room / 2)
                .min(state.list_names.len().saturating_sub(room));
            for (i, name) in state
                .list_names
                .iter()
                .enumerate()
                .skip(start)
                .take(room)
            {
                let current = i + 1 == state.index;
                let marker = if current { "▸" } else { " " };
                let line = format!(
                    "{marker} {:>2}  {}",
                    i + 1,
                    truncate(name, max_title.saturating_sub(6))
                );
                list_rows.push((current, line, i + 1));
            }
        }

        let show_cava_strip = cava_levels.is_some();
        let cava_rows = if show_cava_strip { CAVA_BAR_ROWS } else { 0 };

        let mut block_h = 8usize;
        if !self.show_path {
            block_h = block_h.saturating_sub(1);
        }
        // Toast is a floating overlay — does not reserve layout rows.
        if state.show_list {
            block_h += 1 + list_rows.len() + 1;
        } else {
            block_h += 1;
        }
        block_h += 1; // footer
        if show_cava_strip {
            block_h += 1 + cava_rows; // gap + bar height
        }

        let settle = ((1.0 - intro) * 1.5).round() as usize;
        let mut y = rows.saturating_sub(block_h) / 2 + settle;
        if y < 1 {
            y = 1;
        }

        // Brand breathes only while playing — frozen when paused.
        let note_c = if playing {
            gray(lerp(170.0, 245.0, breath(t, 2.8)))
        } else {
            mix(BRIGHT, DARK, 1.0 - intro)
        };
        let brand_c = mix(BRIGHT, DARK, 1.0 - intro);
        paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &[Span::fg(note_c, "♪  "), Span::fg(brand_c, APP_NAME)],
        )?;
        y += 2;

        let title_c = gray(lerp(70.0, 245.0, title_in * intro));
        paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &[Span::fg(title_c, &title)],
        )?;
        y += 1;

        if self.show_path {
            let path_c = mix(DIM, DARK, 1.0 - intro * 0.85);
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

        let knob_c = if playing {
            gray(lerp(190.0, 255.0, breath(t, 2.0)))
        } else if state.paused {
            GRAY
        } else {
            DIM
        };
        let fill_c = mix(WHITE, DARK, 1.0 - intro);
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
            mix(GRAY, DARK, 1.0 - intro)
        } else if state.paused || state.muted {
            GRAY
        } else {
            gray(lerp(210.0, 245.0, breath(t, 3.2)))
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
        let status_x = paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &status_spans,
        )?;
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

        let meta_spans = [
            Span::fg(DIM, "spd "),
            Span::fg(GRAY, &spd),
            Span::fg(DARK, "  ·  "),
            Span::fg(DIM, "ptch "),
            Span::fg(GRAY, &ptch),
            Span::fg(DARK, "  ·  "),
            Span::fg(DIM, "eq "),
            Span::fg(GRAY, eq),
        ];
        let meta_x = paint_in_region(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            &meta_spans,
        )?;
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

        if state.show_list {
            y += 1;
            self.hits.list.clear();
            for (current, line, track_n) in &list_rows {
                let color = if *current {
                    if playing {
                        gray(lerp(210.0, 245.0, breath(t, 2.6)))
                    } else {
                        BRIGHT
                    }
                } else {
                    DIM
                };
                let lx = paint_in_region(
                    &mut out,
                    y as u16,
                    content_x0,
                    content_cols,
                    &[Span::fg(color, line)],
                )?;
                self.hits.list.push((
                    HitRect {
                        x: lx,
                        y: y as u16,
                        w: line.chars().count() as u16,
                        h: 1,
                    },
                    *track_n,
                ));
                y += 1;
            }
        }

        y += 1;
        let footer_dim = mix(DARK, Color::Black, 1.0 - intro);
        let footer_key = mix(GRAY, DARK, 1.0 - intro * 0.7);
        paint_key_footer(
            &mut out,
            y as u16,
            content_x0,
            content_cols,
            footer_key,
            footer_dim,
        )?;
        y += 1;

        // Cava sits under the shortcut bar — classic vertical bars.
        if show_cava_strip {
            y += 1;
            if let Some(ref levels) = cava_levels {
                let intensity = if playing {
                    0.85
                } else if state.paused {
                    0.32
                } else {
                    0.14
                };
                let strip_w = block_w.saturating_add(4).min(content_cols.saturating_sub(4));
                let (strip_x, strip_h) = paint_cava_bars(
                    &mut out,
                    y as u16,
                    content_x0,
                    content_cols,
                    strip_w,
                    levels,
                    intensity,
                )?;
                self.hits.cava = Some(HitRect {
                    x: strip_x,
                    y: y as u16,
                    w: strip_w as u16,
                    h: strip_h.max(1),
                });
            }
        }

        if self.show_help && sidebar_w > 0 {
            paint_help_sidebar(&mut out, cols, rows, sidebar_w)?;
        }

        // Floating toast — top-right corner overlay, fades in/out.
        if let Some(ref msg) = toast {
            paint_toast_overlay(&mut out, cols, msg, toast_color(&self.toast))?;
        }

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
        ],
    ),
    (
        "seek",
        &[("← →", "±5s"), ("{ }", "±60s"), ("1–9", "jump")],
    ),
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
            ("click", "ui"),
            ("?", "help"),
            ("q", "quit"),
        ],
    ),
];

/// Left sidebar width when help is open.
const HELP_SIDEBAR_W: usize = 26;

/// Classic cava bar height (vertical columns).
const CAVA_BAR_ROWS: usize = 5;

fn help_sidebar_height() -> usize {
    let mut n = 1; // top pad
    for (i, (_title, rows)) in HELP_SECTIONS.iter().enumerate() {
        if i > 0 {
            n += 1; // blank between sections
        }
        n += 1 + rows.len(); // title + rows
    }
    n + 2 // blank + "h close"
}

fn paint_help_sidebar(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    sidebar_w: usize,
) -> io::Result<()> {
    let _ = cols;
    let x0 = 0usize;
    let rule_x = sidebar_w.saturating_sub(1);
    let inner_w = sidebar_w.saturating_sub(3).max(12);
    let h = help_sidebar_height().min(rows.saturating_sub(2));
    let mut y = rows.saturating_sub(h) / 2;
    if y < 1 {
        y = 1;
    }

    // Soft vertical rule separating help from player.
    for row in 1..rows.saturating_sub(1) {
        queue!(
            out,
            MoveTo(rule_x as u16, row as u16),
            SetForegroundColor(DARK),
            Print("│"),
            ResetColor
        )?;
    }

    let text_x = x0 + 1;

    for (si, (title, rows_sec)) in HELP_SECTIONS.iter().enumerate() {
        if si > 0 {
            y += 1;
        }
        if y >= rows.saturating_sub(1) {
            break;
        }
        paint_at(out, text_x as u16, y as u16, &[Span::fg(DIM, title)])?;
        y += 1;
        for (keys, action) in *rows_sec {
            if y >= rows.saturating_sub(1) {
                break;
            }
            let key_col = format!("{keys:<6}");
            let action_t = truncate(action, inner_w.saturating_sub(8));
            paint_at(
                out,
                text_x as u16,
                y as u16,
                &[
                    Span::fg(BRIGHT, &key_col),
                    Span::fg(DARK, " "),
                    Span::fg(GRAY, &action_t),
                ],
            )?;
            y += 1;
        }
    }
    if y + 1 < rows.saturating_sub(1) {
        y += 1;
        paint_at(out, text_x as u16, y as u16, &[Span::fg(DARK, "h  close")])?;
    }
    Ok(())
}

/// Compact footer: bright keys, dim gaps.
fn paint_key_footer(
    out: &mut impl Write,
    y: u16,
    region_x: usize,
    region_w: usize,
    key_c: Color,
    gap_c: Color,
) -> io::Result<()> {
    let chips: &[&str] = &["space", "n/p", "←→", "+/−", "v", "?"];
    let mut spans: Vec<Span<'_>> = Vec::with_capacity(chips.len() * 2);
    for (i, key) in chips.iter().enumerate() {
        if i > 0 {
            spans.push(Span::fg(gap_c, "  ·  "));
        }
        spans.push(Span::fg(key_c, key));
    }
    paint_in_region(out, y, region_x, region_w, &spans).map(|_| ())
}

struct Span<'a> {
    color: Option<Color>,
    text: &'a str,
}

impl<'a> Span<'a> {
    fn fg(color: Color, text: &'a str) -> Self {
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
fn paint_at(out: &mut impl Write, x: u16, y: u16, spans: &[Span<'_>]) -> io::Result<()> {
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
) -> io::Result<(u16, u16)> {
    if levels.is_empty() || block_w == 0 {
        return Ok(((region_x + region_w / 2) as u16, 0));
    }
    // Half-block ramp — stock cava “bars” feel.
    const RAMP: &[char] = &[' ', '▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let intensity = intensity.clamp(0.0, 1.0);
    let rows = CAVA_BAR_ROWS;
    let n = levels.len();

    // Fit as many bar columns as possible with a 1-col gap (classic look).
    // Pattern: B _ B _ B …  → each bar needs 2 cols except the last.
    let bar_cols = ((block_w + 1) / 2).clamp(8, n.min(block_w));

    let mut lines: Vec<String> = (0..rows).map(|_| String::with_capacity(block_w)).collect();

    for b in 0..bar_cols {
        // Sample across the spectrum into fewer display columns.
        let pos = if bar_cols <= 1 {
            0.0
        } else {
            b as f64 / (bar_cols - 1) as f64 * (n.saturating_sub(1)) as f64
        };
        let i0 = pos.floor() as usize;
        let i1 = (i0 + 1).min(n - 1);
        let frac = (pos - i0 as f64).clamp(0.0, 1.0) as f32;
        let raw = levels[i0] * (1.0 - frac) + levels[i1] * frac;
        // Light neighbor blend so bars don’t flicker independently.
        let left = levels[i0.saturating_sub(1)];
        let right = levels[i1.min(n - 1)];
        let level = ((left * 0.15 + raw * 0.7 + right * 0.15) as f64).clamp(0.0, 1.0) * intensity;
        // Gentle gamma — quiet audio still shows, peaks stay soft.
        let level = level.powf(0.78);

        // Height in eighths of a cell across all rows.
        let eighths = (level * (rows * (RAMP.len() - 1)) as f64).round() as usize;
        let full = RAMP.len() - 1;

        for r in 0..rows {
            // Row 0 is the top.
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
            if b + 1 < bar_cols {
                lines[r].push(' ');
            }
        }
    }

    let color = mix(CAVA_DIM, CAVA_SOFT, intensity * 0.95);
    let mut x0 = (region_x + region_w / 2) as u16;
    for (i, line) in lines.iter().enumerate() {
        let x = paint_in_region(out, y + i as u16, region_x, region_w, &[Span::fg(color, line)])?;
        if i == 0 {
            x0 = x;
        }
    }
    Ok((x0, rows as u16))
}

/// Floating toast in the top-right corner (no layout shift).
fn paint_toast_overlay(
    out: &mut impl Write,
    cols: usize,
    msg: &str,
    color: Color,
) -> io::Result<()> {
    let inner = format!(" {msg} ");
    let w = inner.chars().count();
    let box_w = w + 2; // pipes
    let margin = 1usize;
    let x = cols.saturating_sub(box_w + margin) as u16;
    let y = 1u16;
    let top = format!("┌{}┐", "─".repeat(w));
    let bot = format!("└{}┘", "─".repeat(w));
    paint_at(out, x, y, &[Span::fg(DARK, &top)])?;
    paint_at(
        out,
        x,
        y + 1,
        &[
            Span::fg(DARK, "│"),
            Span::fg(color, &inner),
            Span::fg(DARK, "│"),
        ],
    )?;
    paint_at(out, x, y + 2, &[Span::fg(DARK, &bot)])?;
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

fn toast_color(toast: &Option<(String, Instant)>) -> Color {
    let Some((_, at)) = toast else {
        return GRAY;
    };
    let elapsed = at.elapsed().as_secs_f64();
    let fade_in = ease_out_cubic((elapsed / 0.15).clamp(0.0, 1.0));
    let fade_out = if elapsed > 1.4 {
        1.0 - ((elapsed - 1.4) / 0.8).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let a = fade_in * fade_out;
    gray(lerp(55.0, 230.0, a))
}

// ── Shared helpers ──────────────────────────────────────────────

/// Binary name as invoked (`optmusic` or `msc`).
pub fn bin_name() -> String {
    std::env::args()
        .next()
        .and_then(|a| {
            std::path::Path::new(&a)
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
        })
        .unwrap_or_else(|| "optmusic".into())
}

pub fn banner() {
    println!();
    println!(
        "  {} {}",
        "♪".with(BRIGHT),
        APP_NAME.with(BRIGHT).bold()
    );
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
fn progress_parts(pos: Duration, total: Option<Duration>, width: usize) -> (String, String, String) {
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
        assert_eq!(a.chars().count() + b.chars().count() + c.chars().count(), 20);
    }
}
