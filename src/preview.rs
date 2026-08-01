//! Local audio preview — slim optionMusic player for `msc dl`.
//!
//! Fetches audio quietly (spinner only), then opens SessionUi without list/settings.
//! `q` / Esc returns to the download wizard.

use std::io::{self, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Stylize;

use crate::download;
use crate::player::Player;
use crate::playlist::Playlist;
use crate::ui::{DIM, FrameState, GRAY, SessionUi};

/// Fetch a single track’s audio (quiet + spinner) and open the preview player.
pub fn fetch_and_play(url: &str) -> Result<()> {
    let path = with_spinner("preparing preview", || download::fetch_preview_audio(url))?;
    run_local_preview(&path)
}

/// Play a local audio file in the slim preview TUI until the user quits.
pub fn run_local_preview(path: &Path) -> Result<()> {
    let playlist = Playlist::from_paths(&[path.to_path_buf()])
        .with_context(|| format!("cannot open preview {}", path.display()))?;
    if playlist.is_empty() {
        anyhow::bail!("preview file is not playable: {}", path.display());
    }

    let mut player = Player::new(80, 1.0, 0.0)?;
    let mut ui = SessionUi::enter_preview().context("failed to open preview player")?;
    player.set_volume_max(ui.volume_max());
    ui.toast("preview · q back");

    let index = 0usize;
    if let Some(track) = playlist.get(index) {
        player.play_file(&track.path)?;
    }

    let mut held = false;
    let mut quitting = false;

    loop {
        let track = playlist.get(index);
        let name = track
            .map(|t| t.display_name())
            .unwrap_or_else(|| "preview".into());
        let path_s = track
            .map(|t| t.path.display().to_string())
            .unwrap_or_default();
        let toast_owned = ui.toast_text().map(|s| s.to_string());

        let frame = FrameState {
            track_name: &name,
            track_path: &path_s,
            index: 1,
            total: 1,
            pos: if held {
                Duration::ZERO
            } else {
                player.position()
            },
            duration: if held { None } else { player.duration() },
            volume: player.volume(),
            muted: player.muted(),
            speed: player.speed(),
            pitch: player.pitch(),
            eq_label: player.eq_label(),
            paused: held || player.is_paused(),
            stopped: held,
            loop_label: "off",
            list_names: &[],
            toast: toast_owned.as_deref(),
        };
        ui.draw(&frame)?;

        // Preview doesn’t need 60fps — keep it light.
        if event::poll(Duration::from_millis(40)).unwrap_or(false) {
            loop {
                match event::read() {
                    Ok(Event::Key(key)) if key.kind == KeyEventKind::Press => {
                        if key.modifiers.contains(KeyModifiers::CONTROL)
                            && key.code == KeyCode::Char('c')
                        {
                            player.stop();
                            quitting = true;
                        } else {
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    player.stop();
                                    quitting = true;
                                }
                                // Intentionally ignore list / settings.
                                KeyCode::Char('l') | KeyCode::Char('c') => {}
                                KeyCode::Char(' ') | KeyCode::Char('t') => {
                                    if held {
                                        held = false;
                                        if let Some(t) = playlist.get(index) {
                                            let _ = player.play_file(&t.path);
                                        }
                                    } else {
                                        let _ = player.toggle_pause();
                                    }
                                }
                                KeyCode::Char('s') => {
                                    player.stop();
                                    held = true;
                                    ui.toast("stopped");
                                }
                                KeyCode::Left => player.seek_short_back(),
                                KeyCode::Right => player.seek_short_forward(),
                                KeyCode::Char('{') => player.seek_long_back(),
                                KeyCode::Char('}') => player.seek_long_forward(),
                                KeyCode::Char('+') | KeyCode::Char('=') => {
                                    let v = player.volume_step_up();
                                    ui.toast(format!("volume {v}%"));
                                }
                                KeyCode::Char('-') | KeyCode::Char('_') => {
                                    let v = player.volume_step_down();
                                    ui.toast(format!("volume {v}%"));
                                }
                                KeyCode::Char('m') => {
                                    let muted = player.toggle_mute();
                                    ui.toast(if muted { "muted" } else { "unmuted" });
                                }
                                KeyCode::Char('e') => {
                                    let eq = player.cycle_eq();
                                    ui.toast(format!("eq {}", eq.label()));
                                }
                                KeyCode::Char('[') => {
                                    let s = player.speed_down();
                                    ui.toast(format!("speed {s:.1}x"));
                                }
                                KeyCode::Char(']') => {
                                    let s = player.speed_up();
                                    ui.toast(format!("speed {s:.1}x"));
                                }
                                KeyCode::Char('h') | KeyCode::Char('?') => {
                                    ui.toggle_help();
                                }
                                _ => {}
                            }
                        }
                    }
                    Ok(_) => {}
                    Err(_) => break,
                }
                if quitting || !event::poll(Duration::ZERO).unwrap_or(false) {
                    break;
                }
            }
        }

        if quitting {
            break;
        }
    }

    ui.leave()?;
    Ok(())
}

fn with_spinner<T, F>(label: &str, work: F) -> Result<T>
where
    F: FnOnce() -> Result<T>,
{
    let done = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&done);
    let label = label.to_string();

    let spinner = thread::spawn(move || {
        const FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
        let mut i = 0usize;
        let mut out = io::stderr();
        while !flag.load(Ordering::Relaxed) {
            let frame = FRAMES[i % FRAMES.len()];
            let _ = write!(
                out,
                "\r  {} {} {}",
                frame.to_string().with(GRAY),
                label.as_str().with(DIM),
                "   "
            );
            let _ = out.flush();
            i = i.wrapping_add(1);
            thread::sleep(Duration::from_millis(80));
        }
        // Clear the spinner line.
        let _ = write!(out, "\r{}\r", " ".repeat(48));
        let _ = out.flush();
    });

    let result = work();
    done.store(true, Ordering::Relaxed);
    let _ = spinner.join();
    result
}
