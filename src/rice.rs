//! Rice-aware UI theme: read the matugen `scheme.json` when present,
//! fall back to the classic B&W palette otherwise.
//!
//! Source: `~/.local/state/rice/scheme.json` (`{mode, colours{...}}`, hex
//! without `#`). No config key, no network, never writes — a missing or
//! corrupt file silently yields the built-in greys, so playback never
//! breaks. Cached by file mtime; a `matugen --mode light` regen is picked
//! up on the next frame.

use std::sync::{Mutex, OnceLock};
use std::time::SystemTime;

use crossterm::style::Color;

use crate::config::Accent;

/// Where matugen (via the `rice-scheme` template) writes its scheme.
pub fn scheme_path() -> std::path::PathBuf {
    option_sdk::home_dir().join(".local/state/rice/scheme.json")
}

/// Full role set for every CLI surface (player, cards, browse, dl wizard).
#[derive(Debug, Clone, Copy)]
pub struct UiTheme {
    pub panel: Color,
    pub border: Color,
    pub title: Color,
    pub body: Color,
    pub muted: Color,
    pub cursor: Color,
    pub focus_bg: Color,
    pub focus_fg: Color,
    pub accent: Color,
    pub accent_dim: Color,
    pub error: Color,
    pub error_fg: Color,
    pub cava_hi: Color,
}

impl UiTheme {
    /// Classic B&W defaults — pixel-identical to the pre-rice UI.
    pub fn fallback() -> Self {
        let accent = Color::Rgb {
            r: 245,
            g: 245,
            b: 245,
        };
        Self {
            panel: Color::Rgb {
                r: 22,
                g: 22,
                b: 22,
            },
            border: Color::Rgb {
                r: 48,
                g: 48,
                b: 48,
            },
            title: Color::Rgb {
                r: 80,
                g: 80,
                b: 80,
            },
            body: Color::Rgb {
                r: 140,
                g: 140,
                b: 140,
            },
            muted: Color::Rgb {
                r: 48,
                g: 48,
                b: 48,
            },
            cursor: Color::Rgb {
                r: 245,
                g: 245,
                b: 245,
            },
            focus_bg: accent,
            focus_fg: Color::Black,
            accent,
            accent_dim: mix(
                accent,
                Color::Rgb {
                    r: 48,
                    g: 48,
                    b: 48,
                },
                0.45,
            ),
            error: Color::Rgb {
                r: 245,
                g: 245,
                b: 245,
            },
            error_fg: Color::Rgb {
                r: 245,
                g: 245,
                b: 245,
            },
            cava_hi: Color::Rgb {
                r: 44,
                g: 44,
                b: 44,
            },
        }
    }

    /// Rice accent wins; without a scheme the manual `Accent` applies.
    pub fn accent_for(&self, accent: &Accent, rice_active: bool) -> (Color, Color) {
        if rice_active {
            return (self.accent, self.accent_dim);
        }
        match accent.rgb() {
            Some((r, g, b)) => {
                let a = Color::Rgb { r, g, b };
                (a, mix(a, self.panel, 0.45))
            }
            None => (self.accent, self.accent_dim),
        }
    }
}

/// Parse `RRGGBB` with or without a leading `#`.
pub fn parse_hex(s: &str) -> Option<Color> {
    let h = s.trim().strip_prefix('#').unwrap_or(s.trim());
    if h.len() != 6 {
        return None;
    }
    let r = u8::from_str_radix(&h[0..2], 16).ok()?;
    let g = u8::from_str_radix(&h[2..4], 16).ok()?;
    let b = u8::from_str_radix(&h[4..6], 16).ok()?;
    Some(Color::Rgb { r, g, b })
}

fn mix(a: Color, b: Color, t: f64) -> Color {
    let (ar, ag, ab) = components(a);
    let (br, bg, bb) = components(b);
    let t = t.clamp(0.0, 1.0);
    Color::Rgb {
        r: (ar + (br - ar) * t).round().clamp(0.0, 255.0) as u8,
        g: (ag + (bg - ag) * t).round().clamp(0.0, 255.0) as u8,
        b: (ab + (bb - ab) * t).round().clamp(0.0, 255.0) as u8,
    }
}

fn components(c: Color) -> (f64, f64, f64) {
    match c {
        Color::Rgb { r, g, b } => (r as f64, g as f64, b as f64),
        Color::Black => (0.0, 0.0, 0.0),
        Color::White => (255.0, 255.0, 255.0),
        _ => (128.0, 128.0, 128.0),
    }
}

fn get(colours: &serde_json::Value, key: &str) -> Option<Color> {
    colours.get(key)?.as_str().and_then(parse_hex)
}

/// Build a theme from a parsed scheme value. `None` = unusable file.
fn theme_from_scheme(v: &serde_json::Value) -> Option<(UiTheme, bool)> {
    let colours = v.get("colours")?;
    let panel = get(colours, "surfaceContainerLow")?;
    let border = get(colours, "outlineVariant")?;
    let title = get(colours, "onSurfaceVariant")?;
    let body = get(colours, "onSurface")?;
    let focus_bg = get(colours, "primary")?;
    let focus_fg = get(colours, "onPrimary")?;
    let error = get(colours, "error")?;
    let surface = get(colours, "surface").unwrap_or(panel);
    let active = true;
    let accent = focus_bg;
    Some((
        UiTheme {
            panel,
            border,
            title,
            body,
            muted: border,
            cursor: body,
            focus_bg,
            focus_fg,
            accent,
            accent_dim: mix(accent, panel, 0.45),
            error,
            error_fg: get(colours, "onError").unwrap_or(body),
            cava_hi: mix(surface, body, 0.10),
        },
        active,
    ))
}

struct Cache {
    mtime: Option<SystemTime>,
    theme: UiTheme,
    rice_active: bool,
}

fn cache() -> &'static Mutex<Cache> {
    static CELL: OnceLock<Mutex<Cache>> = OnceLock::new();
    CELL.get_or_init(|| {
        Mutex::new(Cache {
            mtime: None,
            theme: UiTheme::fallback(),
            rice_active: false,
        })
    })
}

/// Current theme + whether rice is active. Reloads when `scheme.json`
/// appears, disappears, or changes (mtime). Never fails.
pub fn load() -> (UiTheme, bool) {
    let path = scheme_path();
    let mtime = std::fs::metadata(&path).and_then(|m| m.modified()).ok();
    let mut guard = cache().lock().expect("rice cache");
    if guard.mtime == mtime {
        return (guard.theme, guard.rice_active);
    }
    guard.mtime = mtime;
    let parsed = mtime
        .is_some()
        .then(|| std::fs::read_to_string(&path).ok())
        .flatten()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok())
        .and_then(|v| theme_from_scheme(&v));
    match parsed {
        Some((theme, active)) => {
            guard.theme = theme;
            guard.rice_active = active;
        }
        None => {
            guard.theme = UiTheme::fallback();
            guard.rice_active = false;
        }
    }
    (guard.theme, guard.rice_active)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_hex_with_and_without_hash() {
        assert!(matches!(
            parse_hex("#acc7ff"),
            Some(Color::Rgb {
                r: 0xac,
                g: 0xc7,
                b: 0xff
            })
        ));
        assert!(matches!(
            parse_hex("111318"),
            Some(Color::Rgb {
                r: 0x11,
                g: 0x13,
                b: 0x18
            })
        ));
        assert_eq!(parse_hex("abc"), None);
        assert_eq!(parse_hex("#zzzzzz"), None);
    }

    #[test]
    fn scheme_builds_theme() {
        let v: serde_json::Value = serde_json::from_str(
            r#"{"mode":"dark","colours":{
                "surface":"111318","surfaceContainerLow":"1a1b20",
                "outlineVariant":"44474f","onSurfaceVariant":"c4c6d0",
                "onSurface":"e2e2e9","primary":"acc7ff","onPrimary":"0f2f60",
                "error":"ffb4ab","onError":"690005"}}"#,
        )
        .unwrap();
        let (theme, active) = theme_from_scheme(&v).unwrap();
        assert!(active);
        assert!(matches!(theme.panel, Color::Rgb { r: 0x1a, .. }));
        assert!(matches!(theme.focus_bg, Color::Rgb { r: 0xac, .. }));
        assert!(matches!(theme.error, Color::Rgb { r: 0xff, .. }));
    }

    #[test]
    fn scheme_missing_keys_is_unusable() {
        let v: serde_json::Value =
            serde_json::from_str(r#"{"colours":{"primary":"acc7ff"}}"#).unwrap();
        assert!(theme_from_scheme(&v).is_none());
    }

    #[test]
    fn fallback_accent_for_default() {
        let (theme, _) = (UiTheme::fallback(), false);
        let (a, _) = theme.accent_for(&Accent::Default, false);
        assert!(matches!(a, Color::Rgb { r: 245, .. }));
        let (a2, _) = theme.accent_for(&Accent::Cyan, false);
        assert!(matches!(a2, Color::Rgb { r: 120, .. }));
    }

    #[test]
    fn rice_wins_over_manual_accent() {
        let (theme, _) = (UiTheme::fallback(), false);
        let (a, _) = theme.accent_for(&Accent::Rose, true);
        assert!(matches!(a, Color::Rgb { r: 245, .. }));
    }
}
