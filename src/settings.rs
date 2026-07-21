//! In-player settings sidebar (`c`) — left overlay, persisted to config.toml.

use std::io::{self, Write};

use crossterm::{
    cursor::MoveTo,
    queue,
    style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor},
};

use crate::config::{Accent, AppConfig, ArtistSource, DlUiMode};
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

pub struct SettingsUi {
    open: bool,
    screen: SettingsScreen,
    cursor: usize,
    hits: Vec<RowHit>,
    pane: Option<RowHit>,
}

impl Default for SettingsUi {
    fn default() -> Self {
        Self {
            open: false,
            screen: SettingsScreen::Main,
            cursor: 0,
            hits: Vec::new(),
            pane: None,
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
        }
    }

    pub fn close(&mut self) {
        self.open = false;
        self.screen = SettingsScreen::Main;
        self.cursor = 0;
        self.hits.clear();
        self.pane = None;
    }

    fn len(&self) -> usize {
        match self.screen {
            SettingsScreen::Main => 7,
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
                    self.cursor = 1;
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
                        self.cursor = 1;
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
                    self.screen = SettingsScreen::Cava;
                    self.cursor = 0;
                    applied("cava styles", false, false)
                }
                2 => {
                    cfg.ldm = !cfg.ldm;
                    let _ = cfg.save();
                    applied(if cfg.ldm { "ldm · on" } else { "ldm · off" }, false, false)
                }
                3 => {
                    cfg.accent = cfg.accent.next_preset();
                    let _ = cfg.save();
                    applied(format!("accent · {}", cfg.accent.label()), false, false)
                }
                4 => {
                    cfg.dl_ui = cfg.dl_ui.next();
                    let _ = cfg.save();
                    applied(format!("dl ui · {}", cfg.dl_ui.label()), false, false)
                }
                5 => {
                    cfg.artist_source = cfg.artist_source.next();
                    let _ = cfg.save();
                    applied(
                        format!("artists · {}", cfg.artist_source.label()),
                        false,
                        false,
                    )
                }
                6 => {
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
            SettingsScreen::Main if self.cursor == 4 => {
                cfg.dl_ui = if dir < 0 {
                    cfg.dl_ui.prev()
                } else {
                    cfg.dl_ui.next()
                };
                let _ = cfg.save();
                applied(format!("dl ui · {}", cfg.dl_ui.label()), false, false)
            }
            SettingsScreen::Main if self.cursor == 5 => {
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
            SettingsScreen::Main if self.cursor == 0 || self.cursor == 2 => self.activate(cfg),
            SettingsScreen::Main if self.cursor == 1 && dir > 0 => {
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
                    cfg.cava.reset_defaults();
                    let _ = cfg.save();
                    applied("cava · reset defaults", false, true)
                }
                2 => {
                    cfg.ldm = false;
                    let _ = cfg.save();
                    applied("ldm · off", false, false)
                }
                3 => {
                    cfg.accent = Accent::Default;
                    let _ = cfg.save();
                    applied("accent · default", false, false)
                }
                4 => {
                    cfg.dl_ui = DlUiMode::Arrows;
                    let _ = cfg.save();
                    applied("dl ui · arrows", false, false)
                }
                5 => {
                    cfg.artist_source = ArtistSource::Metadata;
                    let _ = cfg.save();
                    applied("artists · metadata", false, false)
                }
                6 => {
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

/// Solid left settings panel with inverted focus row (live values).
pub fn paint_settings_sidebar(
    out: &mut impl Write,
    cols: usize,
    rows: usize,
    ui: &mut SettingsUi,
    cfg: &AppConfig,
    accent: Color,
) -> io::Result<usize> {
    if !ui.open {
        ui.hits.clear();
        ui.pane = None;
        return Ok(0);
    }

    let sidebar_w = SETTINGS_SIDEBAR_W.min(cols.saturating_sub(8)).max(24);
    let rule_x = sidebar_w.saturating_sub(1);
    let text_x = 1usize;
    let inner_w = sidebar_w.saturating_sub(3).max(12);
    let panel_bg = Color::Rgb {
        r: 22,
        g: 22,
        b: 22,
    };
    let focus_bg = accent;
    let focus_fg = Color::Black;

    ui.hits.clear();
    ui.pane = Some(RowHit {
        x: 0,
        y: 0,
        w: sidebar_w as u16,
        h: rows as u16,
        id: 0,
    });

    // Solid panel fill so options actually read as a sidebar.
    let fill = " ".repeat(sidebar_w.saturating_sub(1));
    for row in 0..rows {
        queue!(
            out,
            MoveTo(0, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(panel_bg),
            Print(&fill),
            ResetColor
        )?;
    }

    // Vertical rule
    for row in 0..rows {
        queue!(
            out,
            MoveTo(rule_x as u16, row as u16),
            SetBackgroundColor(panel_bg),
            SetForegroundColor(DARK),
            Print("│"),
            ResetColor
        )?;
    }

    let title = match ui.screen {
        SettingsScreen::Main => "settings",
        SettingsScreen::Cava => "cava styles",
    };

    let lines: Vec<(usize, &'static str, String)> = match ui.screen {
        SettingsScreen::Main => vec![
            (0, "Excess vol", on_off(cfg.excess_volume).into()),
            (1, "Cava", "open ›".into()),
            (2, "LDM", on_off(cfg.ldm).into()),
            (3, "Accent", cfg.accent.label()),
            (4, "Dl UI", cfg.dl_ui.label().into()),
            (5, "Artists", cfg.artist_source.label().into()),
            (6, "Reset all", "defaults".into()),
        ],
        SettingsScreen::Cava => vec![
            (0, "Style", cfg.cava.style.label().into()),
            (1, "Height", cfg.cava.rows.to_string()),
            (2, "Reset", "defaults".into()),
        ],
    };

    let content_h = 3 + lines.len() + 3; // title + blank + rows + preview + hint
    let mut y = rows.saturating_sub(content_h) / 2;
    if y < 1 {
        y = 1;
    }

    paint_panel_line(
        out,
        text_x,
        y,
        inner_w,
        panel_bg,
        accent,
        &format!(" {title}"),
        "",
        false,
        focus_bg,
        focus_fg,
    )?;
    y += 2;

    for (id, label, value) in &lines {
        if y >= rows.saturating_sub(3) {
            break;
        }
        let selected = ui.cursor == *id;
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
        ui.hits.push(RowHit {
            x: 0,
            y: y as u16,
            w: rule_x as u16,
            h: 1,
            id: *id,
        });
        y += 1;
    }

    // Live preview strip — always reflects current config.
    y += 1;
    if y < rows.saturating_sub(2) {
        let preview = format!(
            " live {} · ldm {} · {}",
            cfg.accent.label(),
            on_off(cfg.ldm),
            if cfg.excess_volume {
                "vol≤200"
            } else {
                "vol≤100"
            }
        );
        paint_panel_line(
            out,
            text_x,
            y,
            inner_w,
            panel_bg,
            accent,
            &truncate_fit(&preview, inner_w),
            "",
            false,
            focus_bg,
            focus_fg,
        )?;
        y += 1;
    }

    if y < rows.saturating_sub(1) {
        let hint = match ui.screen {
            SettingsScreen::Main => "↑↓ focus  enter set  c",
            SettingsScreen::Cava => "↑↓ focus  ←→ set  esc",
        };
        paint_panel_line(
            out, text_x, y, inner_w, panel_bg, DIM, hint, "", false, focus_bg, focus_fg,
        )?;
        ui.hits.push(RowHit {
            x: 0,
            y: y as u16,
            w: rule_x as u16,
            h: 1,
            id: usize::MAX,
        });
    }

    Ok(sidebar_w)
}

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
    let right = if value.is_empty() {
        String::new()
    } else {
        truncate_fit(value, 10)
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
