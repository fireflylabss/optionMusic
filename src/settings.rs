//! In-player settings card (`c`) — floating overlay, persisted to config.toml.

use std::io::{self, Write};
use std::time::Instant;

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::config::{
    Accent, AppConfig, ArtistSource, DlFallbackMode, DlUiMode, LyricsPos, ToastPos,
};
use crate::ui::{DARK, DIM, GRAY};

pub const SETTINGS_SIDEBAR_W: usize = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsScreen {
    Main,
    Cava,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SettingsAction {
    None,
    Closed,
    /// Config mutated — apply live (toast + optional volume/cava sync).
    Applied {
        message: String,
        sync_volume: bool,
        refresh_cava: bool,
    },
}

#[derive(Debug, Clone, Copy)]
struct RowHit {
    x: u16,
    y: u16,
    w: u16,
    h: u16,
    /// Item index, or `usize::MAX` for close.
    id: usize,
}

impl RowHit {
    fn contains(self, col: u16, row: u16) -> bool {
        row >= self.y
            && row < self.y.saturating_add(self.h)
            && col >= self.x
            && col < self.x.saturating_add(self.w)
    }
}

/// One painted row inside the card: a section label, a hairline, or an
/// option row (`id` = cursor index handled by activate/nudge/reset).
enum Line {
    Head(&'static str),
    Rule,
    Opt(usize, &'static str, String),
}

pub struct SettingsUi {
    open: bool,
    screen: SettingsScreen,
    cursor: usize,
    hits: Vec<RowHit>,
    pane: Option<RowHit>,
    opened_at: Option<Instant>,
    closed_at: Option<Instant>,
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            open: false,
            screen: SettingsScreen::Main,
            cursor: 0,
            hits: Vec::new(),
            pane: None,
            opened_at: None,
            closed_at: None,
        }
    }
}

impl SettingsUi {
    pub fn is_open(&self) -> bool {
        self.open
    }

    pub fn cursor(&self) -> usize {
        self.cursor
    }

    pub fn toggle(&mut self) {
        if self.open {
            self.close();
        } else {
            self.open = true;
            self.screen = SettingsScreen::Main;
            self.cursor = 0;
            self.hits.clear();
            self.pane = None;
            self.opened_at = Some(Instant::now());
            self.closed_at = None;
        }
    }

    pub fn close(&mut self) {
        if self.open {
            self.closed_at = Some(Instant::now());
        }
        self.open = false;
        self.screen = SettingsScreen::Main;
        self.cursor = 0;
        self.hits.clear();
        self.pane = None;
    }

    /// Card pop progress 0..=1 — geometry only, never blocks input.
    /// Open: ~140ms ease-out. Close: ~120ms shrink. LDM: instant.
    pub fn anim_progress(&self, ldm: bool) -> f64 {
        if self.open {
            match self.opened_at {
                Some(t) if !ldm => ease_out_cubic(t.elapsed().as_secs_f64() / 0.14),
                _ => 1.0,
            }
        } else if ldm {
            0.0
        } else {
            match self.closed_at {
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

    /// Floating card rect (x, y, w, h) from the last paint, if any.
    pub fn pane_rect(&self) -> Option<(usize, usize, usize, usize)> {
        self.pane
            .map(|r| (r.x as usize, r.y as usize, r.w as usize, r.h as usize))
    }

    fn len(&self) -> usize {
        match self.screen {
            SettingsScreen::Main => 13,
            SettingsScreen::Cava => 3,
        }
    }

    fn move_cursor(&mut self, dir: i32) {
        let n = self.len().max(1);
        if dir < 0 {
            self.cursor = if self.cursor == 0 {
                n - 1
            } else {
                self.cursor - 1
            };
        } else {
            self.cursor = (self.cursor + 1) % n;
        }
    }

    /// True when the open card consumes this key itself (nav / edit / close).
    /// Anything else (`v l n p s m e f r o …`) must fall through to the
    /// global player shortcuts — the card has no text inputs to protect.
    pub fn wants_key(&self, code: crossterm::event::KeyCode) -> bool {
        use crossterm::event::KeyCode;
        if !self.open {
            return false;
        }
        matches!(
            code,
            KeyCode::Esc
                | KeyCode::Enter
                | KeyCode::Up
                | KeyCode::Down
                | KeyCode::Left
                | KeyCode::Right
                | KeyCode::Char('c')
                | KeyCode::Char('q')
                | KeyCode::Char('k')
                | KeyCode::Char('j')
                | KeyCode::Char('h')
                | KeyCode::Char('-')
                | KeyCode::Char('_')
                | KeyCode::Char('+')
                | KeyCode::Char('=')
                | KeyCode::Char(' ')
                | KeyCode::Char('d')
        )
    }

    pub fn handle_key(
        &mut self,
        code: crossterm::event::KeyCode,
        cfg: &mut AppConfig,
    ) -> SettingsAction {
        use crossterm::event::KeyCode;

        if !self.open {
            return SettingsAction::None;
        }

        match code {
            KeyCode::Esc | KeyCode::Char('c') | KeyCode::Char('q') => {
                if self.screen == SettingsScreen::Cava {
                    self.screen = SettingsScreen::Main;
                    self.cursor = 5;
                    return SettingsAction::None;
                }
                self.close();
                return SettingsAction::Closed;
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.move_cursor(-1);
                return SettingsAction::None;
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.move_cursor(1);
                return SettingsAction::None;
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('-') | KeyCode::Char('_') => {
                return self.nudge_value(cfg, -1);
            }
            KeyCode::Right
            | KeyCode::Char('+')
            | KeyCode::Char('=')
            | KeyCode::Enter
            | KeyCode::Char(' ') => {
                if matches!(code, KeyCode::Left) {
                    return self.nudge_value(cfg, -1);
                }
                // Right / + cycle when possible; enter / space always activate.
                if matches!(
                    code,
                    KeyCode::Right | KeyCode::Char('+') | KeyCode::Char('=')
                ) {
                    let cycled = self.nudge_value(cfg, 1);
                    if cycled != SettingsAction::None {
                        return cycled;
                    }
                }
                return self.activate(cfg);
            }
            KeyCode::Char('d') => return self.reset_selected(cfg),
            _ => {}
        }
        SettingsAction::None
    }

    pub fn handle_click(&mut self, col: u16, row: u16, cfg: &mut AppConfig) -> SettingsAction {
        if !self.open {
            return SettingsAction::None;
        }
        for hit in &self.hits {
            if hit.contains(col, row) {
                if hit.id == usize::MAX {
                    if self.screen == SettingsScreen::Cava {
                        self.screen = SettingsScreen::Main;
                        self.cursor = 5;
                        return SettingsAction::None;
                    }
                    self.close();
                    return SettingsAction::Closed;
                }
                // Click focuses; second click on same row toggles / cycles.
                if self.cursor == hit.id {
                    return self.activate(cfg);
                }
                self.cursor = hit.id;
                return SettingsAction::None;
            }
        }
        SettingsAction::None
    }

    pub fn pointer_over_pane(&self, col: u16, row: u16) -> bool {
        self.open && self.pane.map(|r| r.contains(col, row)).unwrap_or(false)
    }

    /// Clear paint-only state (hits/pane) without touching open/anim clocks.
    pub fn close_paint_state(&mut self) {
        self.hits.clear();
        self.pane = None;
    }

    fn activate(&mut self, cfg: &mut AppConfig) -> SettingsAction {
        match self.screen {
            SettingsScreen::Main => match self.cursor {
                0 => {
                    cfg.excess_volume = !cfg.excess_volume;
                    let _ = cfg.save();
                    applied(
                        if cfg.excess_volume {
                            "excess volume · on (200%)"
                        } else {
                            "excess volume · off"
                        },
                        true,
                        false,
                    )
                }
                1 => {
                    cfg.resume = !cfg.resume;
                    if !cfg.resume {
                        cfg.resume_track.clear();
                        cfg.resume_position = 0.0;
                        cfg.resume_queue.clear();
                    }
                    let _ = cfg.save();
                    applied(
                        if cfg.resume {
                            "resume · on"
                        } else {
                            "resume · off"
                        },
                        false,
                        false,
                    )
                }
                2 => {
                    cfg.discord_rpc = !cfg.discord_rpc;
                    let _ = cfg.save();
                    applied(
                        if cfg.discord_rpc {
                            "discord rpc · on"
                        } else {
                            "discord rpc · off"
                        },
                        false,
                        false,
                    )
                }
                3 => {
                    cfg.accent = cfg.accent.next_preset();
                    let _ = cfg.save();
                    applied(format!("accent · {}", cfg.accent.label()), false, false)
                }
                4 => {
                    cfg.ldm = !cfg.ldm;
                    let _ = cfg.save();
                    applied(if cfg.ldm { "ldm · on" } else { "ldm · off" }, false, false)
                }
                5 => {
                    self.screen = SettingsScreen::Cava;
                    self.cursor = 0;
                    applied("cava styles", false, false)
                }
                6 => {
                    cfg.lyrics_pos = cfg.lyrics_pos.next();
                    let _ = cfg.save();
                    applied(format!("lyrics · {}", cfg.lyrics_pos.label()), false, false)
                }
                7 => {
                    cfg.toast_pos = cfg.toast_pos.next();
                    let _ = cfg.save();
                    applied(format!("toast · {}", cfg.toast_pos.label()), false, false)
                }
                8 => {
                    cfg.toast_stack = !cfg.toast_stack;
                    let _ = cfg.save();
                    applied(
                        if cfg.toast_stack {
                            "toast stack · on (up to 3)"
                        } else {
                            "toast stack · off (newest only)"
                        },
                        false,
                        false,
                    )
                }
                9 => {
                    cfg.artist_source = cfg.artist_source.next();
                    let _ = cfg.save();
                    applied(
                        format!("artists · {}", cfg.artist_source.label()),
                        false,
                        false,
                    )
                }
                10 => {
                    cfg.dl_ui = cfg.dl_ui.next();
                    let _ = cfg.save();
                    applied(format!("dl ui · {}", cfg.dl_ui.label()), false, false)
                }
                11 => {
                    cfg.dl_fallback = cfg.dl_fallback.next();
                    let _ = cfg.save();
                    applied(
                        format!("dl fallback · {}", cfg.dl_fallback.label()),
                        false,
                        false,
                    )
                }
                12 => {
                    cfg.reset_all();
                    let _ = cfg.save();
                    applied("settings · reset defaults", true, true)
                }
                _ => SettingsAction::None,
            },
            SettingsScreen::Cava => match self.cursor {
                0 => {
                    cfg.cava.style = cfg.cava.style.next();
                    let _ = cfg.save();
                    applied(
                        format!("cava style · {}", cfg.cava.style.label()),
                        false,
                        true,
                    )
                }
                1 => {
                    cfg.cava.cycle_rows_up();
                    let _ = cfg.save();
                    applied(format!("cava height · {}", cfg.cava.rows), false, true)
                }
                2 => {
                    cfg.cava.reset_defaults();
                    let _ = cfg.save();
                    applied("cava · reset defaults", false, true)
                }
                _ => SettingsAction::None,
            },
        }
    }

    fn nudge_value(&mut self, cfg: &mut AppConfig, dir: i8) -> SettingsAction {
        match self.screen {
            SettingsScreen::Main if self.cursor == 3 => {
                cfg.accent = if dir < 0 {
                    cfg.accent.prev_preset()
                } else {
                    cfg.accent.next_preset()
                };
                let _ = cfg.save();
                applied(format!("accent · {}", cfg.accent.label()), false, false)
            }
            SettingsScreen::Main if self.cursor == 10 => {
                cfg.dl_ui = if dir < 0 {
                    cfg.dl_ui.prev()
                } else {
                    cfg.dl_ui.next()
                };
                let _ = cfg.save();
                applied(format!("dl ui · {}", cfg.dl_ui.label()), false, false)
            }
            SettingsScreen::Main if self.cursor == 9 => {
                cfg.artist_source = if dir < 0 {
                    cfg.artist_source.prev()
                } else {
                    cfg.artist_source.next()
                };
                let _ = cfg.save();
                applied(
                    format!("artists · {}", cfg.artist_source.label()),
                    false,
                    false,
                )
            }
            SettingsScreen::Main if self.cursor == 11 => {
                cfg.dl_fallback = if dir < 0 {
                    cfg.dl_fallback.prev()
                } else {
                    cfg.dl_fallback.next()
                };
                let _ = cfg.save();
                applied(
                    format!("dl fallback · {}", cfg.dl_fallback.label()),
                    false,
                    false,
                )
            }
            SettingsScreen::Main if self.cursor == 7 => {
                cfg.toast_pos = if dir < 0 {
                    cfg.toast_pos.prev()
                } else {
                    cfg.toast_pos.next()
                };
                let _ = cfg.save();
                applied(format!("toast · {}", cfg.toast_pos.label()), false, false)
            }
            SettingsScreen::Main
                if self.cursor == 0
                    || self.cursor == 1
                    || self.cursor == 2
                    || self.cursor == 4
                    || self.cursor == 8 =>
            {
                self.activate(cfg)
            }
            SettingsScreen::Main if self.cursor == 6 => {
                cfg.lyrics_pos = if dir < 0 {
                    cfg.lyrics_pos.prev()
                } else {
                    cfg.lyrics_pos.next()
                };
                let _ = cfg.save();
                applied(format!("lyrics · {}", cfg.lyrics_pos.label()), false, false)
            }
            SettingsScreen::Main if self.cursor == 5 && dir > 0 => {
                self.screen = SettingsScreen::Cava;
                self.cursor = 0;
                applied("cava styles", false, false)
            }
            SettingsScreen::Cava if self.cursor == 0 => {
                cfg.cava.style = if dir < 0 {
                    cfg.cava.style.prev()
                } else {
                    cfg.cava.style.next()
                };
                let _ = cfg.save();
                applied(
                    format!("cava style · {}", cfg.cava.style.label()),
                    false,
                    true,
                )
            }
            SettingsScreen::Cava if self.cursor == 1 => {
                if dir < 0 {
                    cfg.cava.cycle_rows_down();
                } else {
                    cfg.cava.cycle_rows_up();
                }
                let _ = cfg.save();
                applied(format!("cava height · {}", cfg.cava.rows), false, true)
            }
            _ => SettingsAction::None,
        }
    }

    fn reset_selected(&mut self, cfg: &mut AppConfig) -> SettingsAction {
        match self.screen {
            SettingsScreen::Main => match self.cursor {
                0 => {
                    cfg.excess_volume = false;
                    let _ = cfg.save();
                    applied("excess volume · off", true, false)
                }
                1 => {
                    cfg.resume = true;
                    let _ = cfg.save();
                    applied("resume · on", false, false)
                }
                2 => {
                    cfg.discord_rpc = false;
                    let _ = cfg.save();
                    applied("discord rpc · off", false, false)
                }
                3 => {
                    cfg.accent = Accent::Default;
                    let _ = cfg.save();
                    applied("accent · default", false, false)
                }
                4 => {
                    cfg.ldm = false;
                    let _ = cfg.save();
                    applied("ldm · off", false, false)
                }
                5 => {
                    cfg.cava.reset_defaults();
                    let _ = cfg.save();
                    applied("cava · reset defaults", false, true)
                }
                6 => {
                    cfg.lyrics_pos = LyricsPos::Above;
                    let _ = cfg.save();
                    applied("lyrics · above", false, false)
                }
                7 => {
                    cfg.toast_pos = ToastPos::BottomCenter;
                    let _ = cfg.save();
                    applied("toast · bottom-center", false, false)
                }
                8 => {
                    cfg.toast_stack = false;
                    let _ = cfg.save();
                    applied("toast stack · off (newest only)", false, false)
                }
                9 => {
                    cfg.artist_source = ArtistSource::Metadata;
                    let _ = cfg.save();
                    applied("artists · metadata", false, false)
                }
                10 => {
                    cfg.dl_ui = DlUiMode::Arrows;
                    let _ = cfg.save();
                    applied("dl ui · arrows", false, false)
                }
                11 => {
                    cfg.dl_fallback = DlFallbackMode::Ask;
                    let _ = cfg.save();
                    applied("dl fallback · ask", false, false)
                }
                12 => {
                    cfg.reset_all();
                    let _ = cfg.save();
                    applied("settings · reset defaults", true, true)
                }
                _ => SettingsAction::None,
            },
            SettingsScreen::Cava => {
                cfg.cava.reset_defaults();
                let _ = cfg.save();
                applied("cava · reset defaults", false, true)
            }
        }
    }
}

fn applied(message: impl Into<String>, sync_volume: bool, refresh_cava: bool) -> SettingsAction {
    SettingsAction::Applied {
        message: message.into(),
        sync_volume,
        refresh_cava,
    }
}

/// Floating settings card — same overlay language as the `?` help card:
/// solid panel, DARK hairline border, 1-col inner padding, DIM titles,
/// inverted focus row, DARK footer hint. Anchors left by default; docks
/// right when the playlist occupies the left slot (`anchor_right`).
pub fn paint_settings_sidebar(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    ui: &mut SettingsUi,
    cfg: &AppConfig,
    accent: Color,
    anchor_right: bool,
) -> io::Result<usize> {
    let prog = ui.anim_progress(cfg.ldm).clamp(0.0, 1.0);
    if !ui.open && prog <= 0.02 {
        ui.hits.clear();
        ui.pane = None;
        return Ok(0);
    }
    let closing = !ui.open;

    // Full card width (mirrors the help card's clamp language).
    let box_w = if cols <= 24 {
        cols.max(1)
    } else {
        SETTINGS_SIDEBAR_W.min(cols.saturating_sub(4)).max(24)
    };

    let title = match ui.screen {
        SettingsScreen::Main => "settings",
        SettingsScreen::Cava => "cava styles",
    };

    // Main screen is sectioned: Head rows group the option rows under them,
    // Rule is a quiet divider. Cursors index Opt rows only — headers paint
    // but never take focus or hits.
    let lines: Vec<Line> = match ui.screen {
        SettingsScreen::Main => vec![
            Line::Head("player"),
            Line::Opt(0, "Excess vol", on_off(cfg.excess_volume).into()),
            Line::Opt(1, "Resume", on_off(cfg.resume).into()),
            Line::Opt(2, "Discord", on_off(cfg.discord_rpc).into()),
            Line::Head("interface"),
            Line::Opt(3, "Accent", cfg.accent.label()),
            Line::Opt(4, "LDM", on_off(cfg.ldm).into()),
            Line::Opt(5, "Cava", "open ›".into()),
            Line::Opt(6, "Lyrics", cfg.lyrics_pos.label().into()),
            Line::Opt(7, "Toast pos", cfg.toast_pos.label().into()),
            Line::Opt(8, "Toast stack", on_off(cfg.toast_stack).into()),
            Line::Head("library & dl"),
            Line::Opt(9, "Artists", cfg.artist_source.label().into()),
            Line::Opt(10, "Dl UI", cfg.dl_ui.label().into()),
            Line::Opt(11, "Dl fallbk", cfg.dl_fallback.label().into()),
            Line::Rule,
            Line::Opt(12, "Reset all", "defaults".into()),
        ],
        SettingsScreen::Cava => vec![
            Line::Opt(0, "Style", cfg.cava.style.label().into()),
            Line::Opt(1, "Height", cfg.cava.rows.to_string()),
            Line::Opt(2, "Reset", "defaults".into()),
        ],
    };

    let hint = match ui.screen {
        SettingsScreen::Main => "↑↓ focus  enter set  c",
        SettingsScreen::Cava => "↑↓ focus  ←→ set  esc",
    };

    // Borders (2) + inner pads (2) + title + hairline + rows + gap + hint.
    let need = 4 + 1 + 1 + lines.len() + 1 + 1;
    let box_h = need.min(rows.saturating_sub(1).max(5)).max(5);

    // Pop geometry: left edge pinned, width/height ease to full.
    let e = prog;
    let (render_w, render_h) = if e >= 0.999 {
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

    let panel_bg = Color::Rgb {
        r: 22,
        g: 22,
        b: 22,
    };
    let focus_bg = accent;
    let focus_fg = Color::Black;

    // Pinned edge stable while popping; right side clears the list slot.
    let x0 = if anchor_right {
        cols.saturating_sub(render_w + 1)
    } else {
        1usize
    };
    let y0 = rows.saturating_sub(box_h) / 2;
    let ry0 = y0 + (box_h - render_h) / 2;
    let ry1 = ry0 + render_h - 1;

    ui.hits.clear();
    ui.pane = if closing {
        None
    } else {
        Some(RowHit {
            x: x0 as u16,
            y: ry0 as u16,
            w: render_w as u16,
            h: render_h as u16,
            id: 0,
        })
    };

    // Solid fill so the player never shows through.
    let fill = " ".repeat(render_w);
    for row in ry0..=ry1 {
        queue!(
            out,
            MoveTo(x0 as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(panel_bg),
            Print(&fill),
            ResetColor
        )?;
    }

    // Thin border — same DARK hairline language as the help card.
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
        MoveTo(x0 as u16, ry1 as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&bot),
        ResetColor
    )?;
    for row in (ry0 + 1)..ry1 {
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

    // Body with 1-col padding inside the border; clipped while popping.
    let text_x = x0 + 2;
    let inner_w = render_w.saturating_sub(4).max(8);
    let last = ry1.saturating_sub(1);
    let mut y = ry0 + 2;
    if y > last {
        return Ok(render_w);
    }

    paint_panel_line(
        out, text_x, y, inner_w, panel_bg, DIM, title, "", false, focus_bg, focus_fg,
    )?;
    y += 1;
    if y > last {
        return Ok(render_w);
    }

    // Quiet hairline under the title — separates chrome from options.
    queue!(
        out,
        MoveTo(text_x as u16, y as u16),
        SetBackgroundColor(panel_bg),
        SetForegroundColor(DARK),
        Print(&"─".repeat(inner_w)),
        ResetColor
    )?;
    y += 1;
    if y > last {
        return Ok(render_w);
    }

    for line in &lines {
        if y > last.saturating_sub(3) {
            break;
        }
        match line {
            Line::Head(name) => {
                // `name ──────────` hairline label in the dimmest ink.
                let rule = "─".repeat(inner_w.saturating_sub(name.len() + 3));
                paint_panel_line(
                    out,
                    text_x,
                    y,
                    inner_w,
                    panel_bg,
                    DARK,
                    &format!("{name} {rule}"),
                    "",
                    false,
                    focus_bg,
                    focus_fg,
                )?;
            }
            Line::Rule => {
                queue!(
                    out,
                    MoveTo(text_x as u16, y as u16),
                    SetBackgroundColor(panel_bg),
                    SetForegroundColor(DARK),
                    Print(&"─".repeat(inner_w)),
                    ResetColor
                )?;
            }
            Line::Opt(id, label, value) => {
                let selected = ui.cursor == *id && !closing;
                paint_panel_line(
                    out,
                    text_x,
                    y,
                    inner_w,
                    panel_bg,
                    if selected { focus_fg } else { GRAY },
                    label,
                    value,
                    selected,
                    focus_bg,
                    focus_fg,
                )?;
                if !closing {
                    ui.hits.push(RowHit {
                        x: x0 as u16,
                        y: y as u16,
                        w: render_w as u16,
                        h: 1,
                        id: *id,
                    });
                }
            }
        }
        y += 1;
    }

    y += 1;
    if y <= last {
        paint_panel_line(
            out, text_x, y, inner_w, panel_bg, DARK, hint, "", false, focus_bg, focus_fg,
        )?;
        if !closing {
            ui.hits.push(RowHit {
                x: x0 as u16,
                y: y as u16,
                w: render_w as u16,
                h: 1,
                id: usize::MAX,
            });
        }
    }

    Ok(render_w)
}

fn ease_out_cubic(t: f64) -> f64 {
    let t = t.clamp(0.0, 1.0);
    1.0 - (1.0 - t).powi(3)
}

#[allow(clippy::too_many_arguments)]
fn paint_panel_line(
    out: &mut impl Write,
    x: usize,
    y: usize,
    inner_w: usize,
    panel_bg: Color,
    fg: Color,
    label: &str,
    value: &str,
    selected: bool,
    focus_bg: Color,
    focus_fg: Color,
) -> io::Result<()> {
    let marker = if selected { "▌ " } else { "  " };
    let left = format!("{marker}{label}");
    // Value column: fit whatever space the label leaves (was a fixed 10,
    // which clipped "bottom-center").
    let right_max = inner_w.saturating_sub(left.chars().count() + 1).max(6);
    let right = if value.is_empty() {
        String::new()
    } else {
        truncate_fit(value, right_max)
    };
    let used = left.chars().count() + right.chars().count();
    let gap_n = inner_w
        .saturating_sub(used)
        .max(if right.is_empty() { 0 } else { 1 });
    let gap = " ".repeat(gap_n);
    let mut line = format!("{left}{gap}{right}");
    while line.chars().count() < inner_w {
        line.push(' ');
    }
    let line: String = line.chars().take(inner_w).collect();

    let bg = if selected { focus_bg } else { panel_bg };
    let fg_main = if selected { focus_fg } else { fg };

    queue!(
        out,
        MoveTo(x as u16, y as u16),
        SetBackgroundColor(bg),
        SetForegroundColor(fg_main),
        Print(&line),
        ResetColor
    )?;
    Ok(())
}

fn on_off(v: bool) -> &'static str {
    if v { "ON" } else { "OFF" }
}

fn truncate_fit(s: &str, max: usize) -> String {
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
    use crossterm::event::KeyCode;

    #[test]
    fn toggle_opens_and_closes() {
        let mut s = SettingsUi::default();
        assert!(!s.is_open());
        s.toggle();
        assert!(s.is_open());
        s.toggle();
        assert!(!s.is_open());
    }

    #[test]
    fn arrows_move_cursor() {
        let mut s = SettingsUi::default();
        let mut cfg = AppConfig::default();
        s.toggle();
        assert_eq!(s.cursor, 0);
        s.handle_key(KeyCode::Down, &mut cfg);
        assert_eq!(s.cursor, 1);
        s.handle_key(KeyCode::Up, &mut cfg);
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn enter_toggles_live() {
        let mut s = SettingsUi::default();
        let mut cfg = AppConfig::default();
        s.toggle();
        assert!(!cfg.excess_volume);
        let a = s.handle_key(KeyCode::Enter, &mut cfg);
        assert!(cfg.excess_volume);
        assert!(matches!(
            a,
            SettingsAction::Applied {
                sync_volume: true,
                ..
            }
        ));
    }
}
