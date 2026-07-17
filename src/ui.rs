//! Terminal UI — black & white, compact, centered, zero-leak (alternate screen).

use std::io::{self, Write};
use std::time::{Duration, Instant};

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor, Stylize},
    terminal::{
        disable_raw_mode, enable_raw_mode, size, Clear, ClearType, EnterAlternateScreen,
        LeaveAlternateScreen,
    },
};

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

pub const APP_NAME: &str = "optMusic";

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
    /// When true, Drop skips terminal restore (after explicit leave()).
    detached: bool,
    /// Global clock for ambient motion.
    t0: Instant,
    /// Track identity for title fade-in.
    track_key: String,
    track_since: Instant,
}

impl SessionUi {
    pub fn enter() -> io::Result<Self> {
        enable_raw_mode()?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, Hide, Clear(ClearType::All))?;
        let now = Instant::now();
        Ok(Self {
            toast: None,
            show_list: false,
            detached: false,
            t0: now,
            track_key: String::new(),
            track_since: now,
        })
    }

    /// Restore the real terminal and detach Drop cleanup.
    pub fn leave(mut self) -> io::Result<()> {
        self.restore()?;
        self.detached = true;
        Ok(())
    }

    fn restore(&mut self) -> io::Result<()> {
        let mut out = io::stdout();
        execute!(out, Show, LeaveAlternateScreen, ResetColor)?;
        disable_raw_mode()?;
        Ok(())
    }

    pub fn toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn toggle_list(&mut self) {
        self.show_list = !self.show_list;
    }

    pub fn show_list(&self) -> bool {
        self.show_list
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

        let block_w = cols.saturating_sub(4).clamp(28, 56);
        let max_title = block_w.saturating_sub(2);

        let (icon, status) = if state.stopped {
            ("■", "stopped")
        } else if state.paused {
            ("‖", "paused")
        } else if state.muted {
            ("▶", "muted")
        } else {
            (play_icon(t), "playing")
        };

        let title = truncate(state.track_name, max_title);
        let path = truncate(state.track_path, max_title);

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
        let viz = eq_bars(t, playing, state.paused);

        let mut list_rows: Vec<(bool, String)> = Vec::new();
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
                let marker = if current {
                    list_marker(t, playing)
                } else {
                    " "
                };
                let line = format!(
                    "{marker} {:>2}  {}",
                    i + 1,
                    truncate(name, max_title.saturating_sub(6))
                );
                list_rows.push((current, line));
            }
        }

        let help = "space n/p ←→ {} m e [] ,. +/- l q";

        let mut block_h = 8usize;
        if toast.is_some() {
            block_h += 1;
        }
        if state.show_list {
            block_h += 1 + list_rows.len() + 1;
        } else {
            block_h += 1;
        }
        block_h += 1;

        let settle = ((1.0 - intro) * 1.5).round() as usize;
        let mut y = rows.saturating_sub(block_h) / 2 + settle;
        if y < 1 {
            y = 1;
        }

        let note_c = if playing {
            gray(lerp(160.0, 245.0, breath(t, 2.4)))
        } else {
            mix(BRIGHT, DARK, 1.0 - intro)
        };
        let brand_c = mix(BRIGHT, DARK, 1.0 - intro);
        paint_centered(
            &mut out,
            y as u16,
            cols,
            &[Span::fg(note_c, "♪  "), Span::fg(brand_c, APP_NAME)],
        )?;
        y += 2;

        let title_c = gray(lerp(70.0, 245.0, title_in * intro));
        paint_centered(&mut out, y as u16, cols, &[Span::fg(title_c, &title)])?;
        y += 1;

        let path_c = mix(DIM, DARK, 1.0 - intro * 0.85);
        paint_centered(&mut out, y as u16, cols, &[Span::fg(path_c, &path)])?;
        y += 2;

        let knob_c = if playing {
            gray(lerp(180.0, 255.0, breath(t, 1.6)))
        } else if state.paused {
            GRAY
        } else {
            DIM
        };
        let fill_c = mix(WHITE, DARK, 1.0 - intro);
        paint_centered(
            &mut out,
            y as u16,
            cols,
            &[
                Span::fg(DIM, &pos_s),
                Span::fg(DIM, "  "),
                Span::fg(fill_c, &filled),
                Span::fg(knob_c, &knob),
                Span::fg(DARK, &empty),
                Span::fg(DIM, "  "),
                Span::fg(DIM, &total_s),
            ],
        )?;
        y += 1;

        let st_color = if state.stopped {
            mix(GRAY, DARK, 1.0 - intro)
        } else if state.paused || state.muted {
            GRAY
        } else {
            gray(lerp(200.0, 245.0, breath(t, 2.8)))
        };
        paint_centered(
            &mut out,
            y as u16,
            cols,
            &[
                Span::fg(st_color, icon),
                Span::fg(DIM, "  "),
                Span::fg(st_color, status),
                Span::fg(DARK, "  "),
                Span::fg(if playing { GRAY } else { DARK }, &viz),
                Span::fg(DARK, "  ·  "),
                Span::fg(GRAY, &idx),
                Span::fg(DARK, "  ·  "),
                Span::fg(GRAY, &vol),
            ],
        )?;
        y += 1;

        // Speed / pitch / EQ row
        paint_centered(
            &mut out,
            y as u16,
            cols,
            &[
                Span::fg(DIM, "spd "),
                Span::fg(GRAY, &spd),
                Span::fg(DARK, "  ·  "),
                Span::fg(DIM, "ptch "),
                Span::fg(GRAY, &ptch),
                Span::fg(DARK, "  ·  "),
                Span::fg(DIM, "eq "),
                Span::fg(GRAY, eq),
            ],
        )?;
        y += 1;

        if let Some(ref msg) = toast {
            let toast_c = toast_color(&self.toast);
            paint_centered(
                &mut out,
                y as u16,
                cols,
                &[Span::fg(DARK, "·  "), Span::fg(toast_c, msg)],
            )?;
            y += 1;
        }

        if state.show_list {
            y += 1;
            for (current, line) in &list_rows {
                let color = if *current {
                    if playing {
                        gray(lerp(200.0, 245.0, breath(t, 2.2)))
                    } else {
                        BRIGHT
                    }
                } else {
                    DIM
                };
                paint_centered(&mut out, y as u16, cols, &[Span::fg(color, line)])?;
                y += 1;
            }
        }

        y += 1;
        paint_centered(
            &mut out,
            y as u16,
            cols,
            &[Span::fg(mix(DARK, Color::Black, 1.0 - intro), help)],
        )?;

        out.flush()?;
        Ok(())
    }
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

fn paint_centered(
    out: &mut impl Write,
    y: u16,
    cols: usize,
    spans: &[Span<'_>],
) -> io::Result<()> {
    let w = spans_width(spans);
    let x = cols.saturating_sub(w) / 2;
    queue!(out, MoveTo(x as u16, y))?;
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

impl Drop for SessionUi {
    fn drop(&mut self) {
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

fn play_icon(t: f64) -> &'static str {
    const FRAMES: &[&str] = &["▶", "▷", "▶", "▶"];
    let i = ((t * 2.2) as usize) % FRAMES.len();
    FRAMES[i]
}

fn list_marker(t: f64, playing: bool) -> &'static str {
    if !playing {
        return "▸";
    }
    const FRAMES: &[&str] = &["▸", "▹", "▸", "▸"];
    let i = ((t * 2.0) as usize) % FRAMES.len();
    FRAMES[i]
}

/// Tiny equalizer viz: ▁▂▃▄ — calm when paused, lively when playing.
fn eq_bars(t: f64, playing: bool, paused: bool) -> String {
    const LEVELS: &[char] = &['▁', '▂', '▃', '▄', '▅'];
    let n = 4;
    let mut s = String::with_capacity(n);
    for i in 0..n {
        let level = if playing {
            let phase = t * 3.1 + i as f64 * 0.95;
            let v = phase.sin() * 0.5 + 0.5;
            ((v * 0.85 + 0.1) * (LEVELS.len() - 1) as f64).round() as usize
        } else if paused {
            let v = breath(t, 3.2);
            ((0.15 + v * 0.25) * (LEVELS.len() - 1) as f64).round() as usize
        } else {
            0
        };
        s.push(LEVELS[level.min(LEVELS.len() - 1)]);
    }
    s
}

fn toast_color(toast: &Option<(String, Instant)>) -> Color {
    let Some((_, at)) = toast else {
        return GRAY;
    };
    let elapsed = at.elapsed().as_secs_f64();
    let fade_in = ease_out_cubic((elapsed / 0.2).clamp(0.0, 1.0));
    let fade_out = if elapsed > 1.5 {
        1.0 - ((elapsed - 1.5) / 0.7).clamp(0.0, 1.0)
    } else {
        1.0
    };
    let a = fade_in * fade_out;
    gray(lerp(40.0, 180.0, a))
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
    let mut filled = ((width as f64) * ratio).round() as usize;
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
