//! Arrow-key download wizard (default for `msc dl`).
//!
//! Each setting is its own selection screen — ↑↓ move · enter confirm · esc quit.

use std::collections::HashMap;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor, Stylize};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{execute, queue};

use crate::download::{
    self, Caps, DownloadOptions, MediaItem, MediaKind, Provider, QualityPreset, SearchHit,
};
use crate::preview;
use crate::ui::{BRIGHT, GRAY, print_info, print_warn};

/// Near-white for focused text.
const FG_FOCUS: Color = Color::Rgb {
    r: 250,
    g: 250,
    b: 250,
};
/// Soft panel for selected row (still greyscale).
const BG_FOCUS: Color = Color::Rgb {
    r: 210,
    g: 210,
    b: 210,
};
const FG_ON_FOCUS: Color = Color::Rgb {
    r: 12,
    g: 12,
    b: 12,
};
const FG_TITLE: Color = Color::Rgb {
    r: 235,
    g: 235,
    b: 235,
};
const FG_BODY: Color = Color::Rgb {
    r: 175,
    g: 175,
    b: 175,
};
const FG_MUTED: Color = Color::Rgb {
    r: 95,
    g: 95,
    b: 95,
};
const FG_RULE: Color = Color::Rgb {
    r: 55,
    g: 55,
    b: 55,
};

struct TermUi;

impl TermUi {
    fn enter() -> Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, Hide).context("enter alt screen")?;
        Ok(Self)
    }

    fn paint(&self, rows: &[PaintRow]) -> Result<()> {
        let mut out = io::stdout();
        queue!(out, Clear(ClearType::All), MoveTo(0, 0))?;
        for row in rows {
            queue!(out, MoveTo(0, row.y))?;
            match row.kind {
                RowKind::Rule => {
                    queue!(
                        out,
                        SetForegroundColor(FG_RULE),
                        Print("  ────────────────────────────────────────"),
                        ResetColor
                    )?;
                }
                RowKind::Title => {
                    queue!(
                        out,
                        SetForegroundColor(FG_TITLE),
                        Print(format!("  {}", row.text)),
                        ResetColor
                    )?;
                }
                RowKind::Subtitle => {
                    queue!(
                        out,
                        SetForegroundColor(FG_BODY),
                        Print(format!("  {}", row.text)),
                        ResetColor
                    )?;
                }
                RowKind::Hint => {
                    queue!(
                        out,
                        SetForegroundColor(FG_MUTED),
                        Print(format!("  {}", row.text)),
                        ResetColor
                    )?;
                }
                RowKind::Normal => {
                    queue!(
                        out,
                        SetForegroundColor(FG_BODY),
                        Print(format!("  {}", row.text)),
                        ResetColor
                    )?;
                }
                RowKind::Focus => {
                    let padded = format!("  {}  ", row.text);
                    queue!(
                        out,
                        SetBackgroundColor(BG_FOCUS),
                        SetForegroundColor(FG_ON_FOCUS),
                        Print(padded),
                        ResetColor
                    )?;
                }
                RowKind::Checked => {
                    queue!(
                        out,
                        SetForegroundColor(FG_FOCUS),
                        Print(format!("  {}", row.text)),
                        ResetColor
                    )?;
                }
            }
        }
        out.flush().context("flush")?;
        Ok(())
    }
}

impl Drop for TermUi {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = execute!(out, Show, LeaveAlternateScreen);
        let _ = disable_raw_mode();
    }
}

#[derive(Clone, Copy)]
enum RowKind {
    Rule,
    Title,
    Subtitle,
    Hint,
    Normal,
    Focus,
    Checked,
}

struct PaintRow {
    y: u16,
    kind: RowKind,
    text: String,
}

struct Choice {
    label: &'static str,
    detail: &'static str,
}

pub fn run_interactive_arrows(
    music_dir_flag: &str,
    prefill_query: Option<&str>,
    prefill_provider: Option<Provider>,
    prefill_kind: Option<MediaKind>,
    prefill_output: Option<&Path>,
    _audio_format: &str,
    fallback: crate::config::DlFallbackMode,
) -> Result<()> {
    download::ensure_yt_dlp()?;
    download::purge_expired_cache();

    println!("  {} {}", "♪".with(BRIGHT), "download".with(BRIGHT).bold());
    println!(
        "  {}",
        "interactive wizard  ·  arrow keys  ·  yt-dlp".with(GRAY)
    );
    println!();

    let provider = {
        let ui = TermUi::enter()?;
        match prefill_provider {
            Some(p) => p,
            None => pick_provider(&ui)?,
        }
    };

    let raw = match prefill_query {
        Some(q) if !q.trim().is_empty() => q.trim().to_string(),
        _ => {
            let ui = TermUi::enter()?;
            let q = text_input(
                &ui,
                "Search or paste URL(s)",
                &[
                    "Type a search query for the provider you picked,",
                    "or paste one/more links separated by  ;",
                    "Example:  https://youtu.be/aaa;https://youtu.be/bbb",
                ],
                "",
            )?;
            if q.trim().is_empty() {
                bail!("empty input — nothing to download");
            }
            q
        }
    };

    let mut items = if download::input_is_urls(&raw) {
        download::items_from_urls(&raw, provider)?
    } else {
        print_info(&format!(
            "searching {} for “{}”…",
            provider.label(),
            raw.trim()
        ));
        let results = download::search(provider, &raw)?;
        let ui = TermUi::enter()?;
        browse_search_arrows(&ui, &results)?
    };

    if items.is_empty() {
        bail!("nothing selected");
    }

    // Single selection → offer optionMusic audio preview (local file + real player UI).
    if items.len() == 1 {
        let title_line = format!("“{}”", download::truncate(&items[0].title, 52));
        let want = {
            let ui = TermUi::enter()?;
            pick_yes_no(
                &ui,
                "Preview this track in optionMusic?",
                &[
                    title_line.as_str(),
                    "Quick audio listen · q returns here to continue downloading.",
                ],
                true,
            )?
        };
        if want && let Err(e) = preview::fetch_and_play(&items[0].url) {
            print_warn(&format!("preview: {e:#}"));
            println!();
        }
    }

    print_info(&format!(
        "scanning {} item(s) for formats / subs / thumbs…",
        items.len()
    ));
    download::probe_items(&mut items)?;
    let caps = download::intersect_caps(&items);
    print_info(&format!(
        "ready  ·  {} selected  ·  video={}  subs={}  thumb={}",
        items.len(),
        download::yn(caps.video),
        download::yn(caps.subs),
        download::yn(caps.thumbnail)
    ));

    let opts = {
        let ui = TermUi::enter()?;
        configure_download(
            &ui,
            &items,
            caps,
            provider,
            prefill_kind,
            prefill_output,
            music_dir_flag,
        )?
    };

    println!();
    download::summarize(&items, &opts);

    let go = {
        let ui = TermUi::enter()?;
        pick_yes_no(
            &ui,
            "Start download?",
            &[
                "Review the plan above in the terminal.",
                "This will run yt-dlp for each selected item.",
            ],
            true,
        )?
    };

    if !go {
        bail!("cancelled");
    }
    println!();
    download::run_batch(&items, &opts, fallback)
}

fn configure_download(
    ui: &TermUi,
    items: &[MediaItem],
    caps: Caps,
    provider: Provider,
    prefill_kind: Option<MediaKind>,
    prefill_output: Option<&Path>,
    music_dir_flag: &str,
) -> Result<DownloadOptions> {
    let kind = match prefill_kind {
        Some(k) => {
            if k.wants_video() && !caps.video {
                MediaKind::Audio
            } else {
                k
            }
        }
        None => pick_kind(ui, provider, caps)?,
    };

    let preset = pick_preset(ui)?;
    let mut opts = download::options_from_preset(preset, kind, caps);

    // Always walk individual selection steps after preset (preset only sets defaults).
    // Custom / or "customize further" — user asked for selection steps not cycling.
    let preset_blurb = format!(
        "Preset “{}” already filled sensible defaults.",
        preset.label()
    );
    let customize = preset == QualityPreset::Custom
        || pick_yes_no(
            ui,
            "Customize formats & embeds?",
            &[
                preset_blurb.as_str(),
                "Choose Yes to pick quality, filetype, and embeds one by one.",
            ],
            preset == QualityPreset::Custom,
        )?;

    if customize {
        if kind.wants_video() {
            opts.format_selector = pick_video_quality(ui)?.into();
            opts.container = pick_video_container(ui)?.into();
        }
        if kind.wants_audio() {
            opts.audio_format = pick_audio_format(ui)?.into();
            opts.audio_quality = pick_audio_quality(ui)?.into();
        }

        if caps.music_meta {
            opts.embed_metadata = pick_yes_no(
                ui,
                "Embed full metadata into the file?",
                &[
                    "Writes everything yt-dlp can: title, artists/uploader,",
                    "album, track numbers, chapters, and the info-json blob.",
                    "Recommended for music libraries.",
                ],
                opts.embed_metadata,
            )?;
        } else {
            opts.embed_metadata = false;
        }

        if caps.thumbnail {
            opts.embed_thumbnail = pick_yes_no(
                ui,
                "Embed thumbnail as cover art?",
                &[
                    "Puts the artwork inside the audio/video file itself",
                    "(not a separate image next to it).",
                ],
                opts.embed_thumbnail,
            )?;
        } else {
            opts.embed_thumbnail = false;
        }

        if kind.wants_video() && caps.subs {
            opts.embed_subs = pick_yes_no(
                ui,
                "Download & embed subtitles?",
                &[
                    "Only en / pt / es (avoids YouTube rate-limits from “all” langs).",
                    "Embeds into the video — no loose .srt files. Failures won’t abort.",
                ],
                opts.embed_subs,
            )?;
        } else {
            opts.embed_subs = false;
        }
    }

    opts.output_dir = match prefill_output {
        Some(p) => download::resolve_output_dir(Some(p), music_dir_flag)?,
        None => pick_output_dir(ui, music_dir_flag)?,
    };

    // Soft confirm summary inside the UI before leaving alt screen.
    let _ = items;
    Ok(opts)
}

fn pick_provider(ui: &TermUi) -> Result<Provider> {
    let choices = [
        Choice {
            label: "YouTube",
            detail: "youtube.com / youtu.be — videos & audio",
        },
        Choice {
            label: "YouTube Music",
            detail: "music.youtube.com — tracks, albums, audio-first",
        },
        Choice {
            label: "SoundCloud",
            detail: "soundcloud.com — audio streams & tracks",
        },
    ];
    let idx = pick_choice(
        ui,
        "Choose a provider",
        &[
            "This decides where search runs and how links are handled.",
            "You can still paste a URL from another site later if needed.",
        ],
        &choices,
        0,
    )?;
    Ok(match idx {
        1 => Provider::YoutubeMusic,
        2 => Provider::Soundcloud,
        _ => Provider::Youtube,
    })
}

fn pick_kind(ui: &TermUi, provider: Provider, caps: Caps) -> Result<MediaKind> {
    if !caps.video || !provider.offers_video() {
        let _ = pick_choice(
            ui,
            "What to download",
            &[
                "This source only provides audio (no video stream).",
                "Continuing with an audio-only download.",
            ],
            &[Choice {
                label: "Audio only",
                detail: "Extract / download the audio track",
            }],
            0,
        )?;
        return Ok(MediaKind::Audio);
    }

    let def = match provider.default_kind() {
        MediaKind::Audio => 0,
        MediaKind::Both => 2,
        MediaKind::Video => 1,
    };
    let choices = [
        Choice {
            label: "Audio only",
            detail: "One audio file (mp3/m4a/opus/…) — no video",
        },
        Choice {
            label: "Video only",
            detail: "One video file (merged video+audio, e.g. mp4)",
        },
        Choice {
            label: "Both",
            detail: "Two files per item: a video file and a separate audio file",
        },
    ];
    let idx = pick_choice(
        ui,
        "What to download",
        &[
            "Applies to every selected item in this batch.",
            "“Both” runs two yt-dlp passes (video, then audio).",
        ],
        &choices,
        def,
    )?;
    Ok(match idx {
        0 => MediaKind::Audio,
        2 => MediaKind::Both,
        _ => MediaKind::Video,
    })
}

fn pick_preset(ui: &TermUi) -> Result<QualityPreset> {
    let choices = [
        Choice {
            label: "Best",
            detail: "Highest quality · full embeds on when available",
        },
        Choice {
            label: "Economy",
            detail: "720p / solid audio · smaller files, fewer extras",
        },
        Choice {
            label: "Lower",
            detail: "480p / compact audio · minimal size",
        },
        Choice {
            label: "Custom",
            detail: "Skip defaults — you’ll pick every option next",
        },
    ];
    let idx = pick_choice(
        ui,
        "Quality preset",
        &[
            "A starting point. You can still customize afterward",
            "(unless you only wanted the preset defaults).",
        ],
        &choices,
        0,
    )?;
    Ok(match idx {
        1 => QualityPreset::Economy,
        2 => QualityPreset::Lower,
        3 => QualityPreset::Custom,
        _ => QualityPreset::Best,
    })
}

fn pick_video_quality(ui: &TermUi) -> Result<&'static str> {
    let choices = [
        Choice {
            label: "Best / original",
            detail: "Highest resolution + best audio yt-dlp can merge",
        },
        Choice {
            label: "1080p max",
            detail: "Cap height at 1080 — good balance for most screens",
        },
        Choice {
            label: "720p max",
            detail: "Cap height at 720 — smaller download, still sharp",
        },
        Choice {
            label: "480p max",
            detail: "Cap height at 480 — smallest video size",
        },
    ];
    let idx = pick_choice(
        ui,
        "Video quality",
        &["Pick one. This sets the yt-dlp format selector for video."],
        &choices,
        0,
    )?;
    Ok(match idx {
        1 => download::video_format_for_quality("1080"),
        2 => download::video_format_for_quality("720"),
        3 => download::video_format_for_quality("480"),
        _ => download::video_format_for_quality("best"),
    })
}

fn pick_video_container(ui: &TermUi) -> Result<&'static str> {
    let choices = [
        Choice {
            label: "mp4",
            detail: "Most compatible (phones, players, browsers)",
        },
        Choice {
            label: "webm",
            detail: "Often smaller · may need a modern player",
        },
        Choice {
            label: "mkv",
            detail: "Flexible container · great for embeds/subs",
        },
    ];
    let idx = pick_choice(
        ui,
        "Video container",
        &["Final file extension after streams are merged."],
        &choices,
        0,
    )?;
    Ok(["mp4", "webm", "mkv"][idx])
}

fn pick_audio_format(ui: &TermUi) -> Result<&'static str> {
    let choices = [
        Choice {
            label: "mp3",
            detail: "Universal — works everywhere, lossy",
        },
        Choice {
            label: "m4a",
            detail: "AAC in M4A — great quality / size for music",
        },
        Choice {
            label: "opus",
            detail: "Modern lossy — efficient, needs Opus support",
        },
        Choice {
            label: "flac",
            detail: "Lossless — large files, archival quality",
        },
        Choice {
            label: "best",
            detail: "Keep the best original audio codec yt-dlp finds",
        },
    ];
    let idx = pick_choice(
        ui,
        "Audio file type",
        &["Used when extracting / converting the audio track."],
        &choices,
        1,
    )?;
    Ok(["mp3", "m4a", "opus", "flac", "best"][idx])
}

fn pick_audio_quality(ui: &TermUi) -> Result<&'static str> {
    let choices = [
        Choice {
            label: "0 — best",
            detail: "Highest bitrate / least compression (default for Best)",
        },
        Choice {
            label: "3 — high",
            detail: "Still excellent, slightly smaller",
        },
        Choice {
            label: "5 — medium",
            detail: "Good everyday listening size",
        },
        Choice {
            label: "7 — small",
            detail: "Noticeably smaller files",
        },
    ];
    let idx = pick_choice(
        ui,
        "Audio quality (yt-dlp scale)",
        &["0 = best … 10 = worst. Only applies when converting audio."],
        &choices,
        0,
    )?;
    Ok(["0", "3", "5", "7"][idx])
}

fn pick_output_dir(ui: &TermUi, music_dir_flag: &str) -> Result<PathBuf> {
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    let cwd_line = format!("  {}", cwd.display());
    let use_other = pick_yes_no(
        ui,
        "Save to a different folder?",
        &[
            "Default is the current directory:",
            cwd_line.as_str(),
            "Choose No to keep cwd. Choose Yes to type another path.",
        ],
        false,
    )?;
    if !use_other {
        return Ok(cwd);
    }
    let def = if music_dir_flag.is_empty() {
        String::new()
    } else {
        music_dir_flag.to_string()
    };
    let typed = text_input(
        ui,
        "Output directory",
        &[
            "Absolute path, ~/…, or relative to where you launched msc.",
            "Folder is created if it does not exist.",
        ],
        &def,
    )?;
    if typed.trim().is_empty() {
        return Ok(cwd);
    }
    download::resolve_output_dir(Some(Path::new(typed.trim())), "")
}

fn browse_search_arrows(ui: &TermUi, results: &[SearchHit]) -> Result<Vec<MediaItem>> {
    if results.is_empty() {
        bail!("no results");
    }
    let pages = results.len().div_ceil(download::PAGE_SIZE).max(1);
    let mut page = 0usize;
    let mut cursor = 0usize;
    let mut selected: HashMap<String, SearchHit> = HashMap::new();

    loop {
        let start = page * download::PAGE_SIZE;
        let end = (start + download::PAGE_SIZE).min(results.len());
        let slice = &results[start..end];
        if cursor >= slice.len() {
            cursor = slice.len().saturating_sub(1);
        }

        let mut rows = Vec::new();
        let mut y = 1u16;
        rows.push(PaintRow {
            y,
            kind: RowKind::Title,
            text: "♪  Search results".into(),
        });
        y += 1;
        rows.push(PaintRow {
            y,
            kind: RowKind::Subtitle,
            text: format!(
                "Page {}/{}  ·  {} selected  ·  space toggles  ·  preview asked after 1 pick",
                page + 1,
                pages,
                selected.len()
            ),
        });
        y += 1;
        rows.push(PaintRow {
            y,
            kind: RowKind::Rule,
            text: String::new(),
        });
        y += 1;

        for (i, hit) in slice.iter().enumerate() {
            let on = selected.contains_key(&hit.url);
            let focus = i == cursor;
            let boxc = if on { "☑" } else { "☐" };
            let dur = hit
                .duration
                .map(download::fmt_secs)
                .unwrap_or_else(|| "--:--".into());
            let text = format!(
                "{boxc}  {}   {}   {}",
                download::truncate(&hit.title, 40),
                dur,
                download::truncate(&hit.uploader, 18)
            );
            rows.push(PaintRow {
                y,
                kind: if focus {
                    RowKind::Focus
                } else if on {
                    RowKind::Checked
                } else {
                    RowKind::Normal
                },
                text,
            });
            y += 1;
        }

        y += 1;
        rows.push(PaintRow {
            y,
            kind: RowKind::Hint,
            text: "↑↓ move  ·  space check  ·  ←→ page  ·  a all  ·  enter done  ·  esc quit"
                .into(),
        });
        ui.paint(&rows)?;

        match read_key()? {
            KeyCode::Esc | KeyCode::Char('q') => bail!("cancelled"),
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = cursor
                    .checked_sub(1)
                    .unwrap_or(slice.len().saturating_sub(1));
            }
            KeyCode::Down | KeyCode::Char('j') if !slice.is_empty() => {
                cursor = (cursor + 1) % slice.len();
            }
            KeyCode::Left | KeyCode::Char('h') | KeyCode::Char('p') if page > 0 => {
                page -= 1;
                cursor = 0;
            }
            KeyCode::Right | KeyCode::Char('l') | KeyCode::Char('n') if page + 1 < pages => {
                page += 1;
                cursor = 0;
            }
            KeyCode::Char(' ') => {
                if let Some(hit) = slice.get(cursor)
                    && selected.remove(&hit.url).is_none()
                {
                    selected.insert(hit.url.clone(), hit.clone());
                }
            }
            KeyCode::Char('a') => {
                for hit in slice {
                    selected.insert(hit.url.clone(), hit.clone());
                }
            }
            KeyCode::Enter | KeyCode::Char('d') => {
                if selected.is_empty()
                    && let Some(hit) = slice.get(cursor)
                {
                    selected.insert(hit.url.clone(), hit.clone());
                }
                if !selected.is_empty() {
                    break;
                }
            }
            _ => {}
        }
    }

    Ok(selected
        .into_values()
        .map(|h| MediaItem {
            title: h.title,
            url: h.url,
            provider: h.provider,
            caps: h.provider.base_caps(),
        })
        .collect())
}

fn pick_choice(
    ui: &TermUi,
    title: &str,
    blurb: &[&str],
    choices: &[Choice],
    default: usize,
) -> Result<usize> {
    let mut cursor = default.min(choices.len().saturating_sub(1));
    loop {
        let mut rows = Vec::new();
        let mut y = 1u16;
        rows.push(PaintRow {
            y,
            kind: RowKind::Title,
            text: format!("♪  {title}"),
        });
        y += 1;
        for line in blurb {
            rows.push(PaintRow {
                y,
                kind: RowKind::Subtitle,
                text: (*line).into(),
            });
            y += 1;
        }
        rows.push(PaintRow {
            y,
            kind: RowKind::Rule,
            text: String::new(),
        });
        y += 1;

        for (i, c) in choices.iter().enumerate() {
            let focus = i == cursor;
            let mark = if focus { "▸" } else { " " };
            rows.push(PaintRow {
                y,
                kind: if focus {
                    RowKind::Focus
                } else {
                    RowKind::Normal
                },
                text: format!("{mark}  {}", c.label),
            });
            y += 1;
            rows.push(PaintRow {
                y,
                kind: RowKind::Hint,
                text: format!("     {}", c.detail),
            });
            y += 2; // detail + gap
        }

        rows.push(PaintRow {
            y,
            kind: RowKind::Hint,
            text: "↑↓ move  ·  enter confirm  ·  esc cancel".into(),
        });
        ui.paint(&rows)?;

        match read_key()? {
            KeyCode::Esc | KeyCode::Char('q') => bail!("cancelled"),
            KeyCode::Up | KeyCode::Char('k') => {
                cursor = cursor.checked_sub(1).unwrap_or(choices.len() - 1);
            }
            KeyCode::Down | KeyCode::Char('j') => cursor = (cursor + 1) % choices.len(),
            KeyCode::Enter | KeyCode::Char(' ') => return Ok(cursor),
            _ => {}
        }
    }
}

fn pick_yes_no(ui: &TermUi, title: &str, blurb: &[&str], default_yes: bool) -> Result<bool> {
    let choices = [
        Choice {
            label: "Yes",
            detail: "Enable this option",
        },
        Choice {
            label: "No",
            detail: "Leave this option off",
        },
    ];
    let idx = pick_choice(ui, title, blurb, &choices, if default_yes { 0 } else { 1 })?;
    Ok(idx == 0)
}

fn text_input(ui: &TermUi, title: &str, blurb: &[&str], initial: &str) -> Result<String> {
    let mut buf = initial.to_string();
    loop {
        let mut rows = Vec::new();
        let mut y = 1u16;
        rows.push(PaintRow {
            y,
            kind: RowKind::Title,
            text: format!("♪  {title}"),
        });
        y += 1;
        for line in blurb {
            rows.push(PaintRow {
                y,
                kind: RowKind::Subtitle,
                text: (*line).into(),
            });
            y += 1;
        }
        rows.push(PaintRow {
            y,
            kind: RowKind::Rule,
            text: String::new(),
        });
        y += 1;
        rows.push(PaintRow {
            y,
            kind: RowKind::Focus,
            text: format!("› {buf}█"),
        });
        y += 2;
        rows.push(PaintRow {
            y,
            kind: RowKind::Hint,
            text: "type freely  ·  backspace delete  ·  enter continue  ·  esc cancel".into(),
        });
        ui.paint(&rows)?;

        match read_key()? {
            KeyCode::Esc => bail!("cancelled"),
            KeyCode::Enter => return Ok(buf),
            KeyCode::Backspace => {
                buf.pop();
            }
            KeyCode::Char(c) if !c.is_control() => buf.push(c),
            _ => {}
        }
    }
}

fn read_key() -> Result<KeyCode> {
    loop {
        let ev = event::read().context("read key")?;
        if let Event::Key(key) = ev
            && key.kind == KeyEventKind::Press
        {
            return Ok(key.code);
        }
    }
}
