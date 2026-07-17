//! optMusic — minimal black & white CLI music player (option music).
//!
//! Binaries: `optmusic` and short alias `msc`.
//! Engine: MPV via libmpv2.

mod cli;
mod config;
mod eq;
mod mpv;
mod player;
mod playlist;
mod ui;

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use crossterm::style::Stylize;

use cli::{Cli, Command};
use config::resolve_music_dir;
use player::Player;
use playlist::Playlist;
use ui::{
    banner, bin_name, print_info, print_success, print_warn, FrameState, SessionUi, APP_NAME,
    BRIGHT, DIM, GRAY, WHITE,
};

fn main() {
    if let Err(err) = run() {
        eprintln!("{} {err}", "error:".with(WHITE));
        for cause in err.chain().skip(1) {
            eprintln!("  {} {cause}", "↳".with(DIM));
        }
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let bin = bin_name();

    match cli.command {
        Some(Command::Play {
            paths,
            volume,
            speed,
            crossfade,
            shuffle,
            loop_playlist,
            interactive: _,
        }) => {
            let paths = resolve_play_paths(paths, &cli.music_dir)?;
            cmd_play(paths, volume, speed, crossfade, shuffle, loop_playlist)?
        }
        Some(Command::Info { path }) => {
            banner();
            cmd_info(&path)?
        }
        Some(Command::List { path, recursive }) => {
            banner();
            let path = if path.as_os_str() == "." && !cli.music_dir.is_empty() {
                resolve_music_dir(&cli.music_dir)?
            } else {
                path
            };
            cmd_list(&path, recursive)?
        }
        Some(Command::Version) => {
            println!(
                "  {} {} {}",
                "♪".with(BRIGHT),
                APP_NAME.with(BRIGHT).bold(),
                env!("CARGO_PKG_VERSION").with(DIM)
            );
            println!(
                "  {}",
                format!("optmusic · msc  ({bin})  ·  mpv").with(GRAY)
            );
        }
        None => {
            banner();
            let b = bin.as_str();
            println!("  {} play song.mp3", b.with(BRIGHT));
            println!("  {} play ./music/ -s -l -c 2", b.with(BRIGHT));
            println!("  {} play -m ~/Music", b.with(BRIGHT));
            println!("  {} list ./music -r", b.with(BRIGHT));
            println!("  {} --help", b.with(DIM));
            println!();
        }
    }

    Ok(())
}

fn resolve_play_paths(paths: Vec<PathBuf>, music_dir_flag: &str) -> Result<Vec<PathBuf>> {
    if !paths.is_empty() {
        return Ok(paths);
    }
    let dir = resolve_music_dir(music_dir_flag)?;
    if !dir.exists() {
        anyhow::bail!(
            "no paths given and music dir does not exist: {}\n  \
             create it, pass files, or use -m / --music-dir",
            dir.display()
        );
    }
    Ok(vec![dir])
}

fn cmd_play(
    paths: Vec<PathBuf>,
    volume: u8,
    speed: f64,
    crossfade: f64,
    shuffle: bool,
    loop_playlist: bool,
) -> Result<()> {
    if paths.is_empty() {
        anyhow::bail!("pass at least one file or directory to play");
    }

    let mut playlist = Playlist::from_paths(&paths)?;
    if playlist.is_empty() {
        anyhow::bail!("no playable audio files found in the given paths");
    }

    if shuffle {
        playlist.shuffle();
    }

    let mut player = Player::new(volume, speed, crossfade)?;

    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        run_session(&mut player, &mut playlist, loop_playlist, shuffle)?;
    } else {
        banner();
        print_success(&format!(
            "Loaded {} track{}",
            playlist.len(),
            if playlist.len() == 1 { "" } else { "s" }
        ));
        run_plain(&mut player, &mut playlist, loop_playlist)?;
    }

    Ok(())
}

fn run_plain(player: &mut Player, playlist: &mut Playlist, loop_playlist: bool) -> Result<()> {
    loop {
        for (idx, track) in playlist.tracks().iter().enumerate() {
            print_info(&format!(
                "[{}/{}] {}",
                idx + 1,
                playlist.len(),
                track.display_name()
            ));
            player.play_file(&track.path)?;
            while !player.is_idle() {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
        if !loop_playlist {
            break;
        }
    }
    print_success("Done. Thanks for listening ♪");
    Ok(())
}

fn run_session(
    player: &mut Player,
    playlist: &mut Playlist,
    loop_playlist: bool,
    shuffled: bool,
) -> Result<()> {
    let mut ui = SessionUi::enter().context("failed to open player UI")?;
    let start_toast = if shuffled {
        format!(
            "{} track{} · shuffled",
            playlist.len(),
            if playlist.len() == 1 { "" } else { "s" }
        )
    } else {
        format!(
            "{} track{}",
            playlist.len(),
            if playlist.len() == 1 { "" } else { "s" }
        )
    };
    ui.toast(start_toast);

    let mut index: usize = 0;
    let mut held = false;
    let mut done_msg = "done — thanks for listening ♪";

    if let Some(track) = playlist.get(index) {
        player.play_file(&track.path)?;
    }

    loop {
        if !held && player.is_idle() {
            if index + 1 < playlist.len() {
                index += 1;
                if let Some(t) = playlist.get(index) {
                    player.play_file(&t.path)?;
                }
            } else if loop_playlist {
                index = 0;
                ui.toast("looping…");
                if let Some(t) = playlist.get(index) {
                    player.play_file(&t.path)?;
                }
            } else {
                break;
            }
        }

        let list_names: Vec<String> = if ui.show_list() {
            playlist
                .tracks()
                .iter()
                .map(|t| t.display_name())
                .collect()
        } else {
            Vec::new()
        };

        let track = playlist.get(index);
        let name = track.map(|t| t.display_name()).unwrap_or_else(|| "—".into());
        let path = track
            .map(|t| t.path.display().to_string())
            .unwrap_or_default();
        let toast_owned = ui.toast_text().map(|s| s.to_string());
        let eq_label = player.eq_label();

        let frame = FrameState {
            track_name: &name,
            track_path: &path,
            index: index + 1,
            total: playlist.len(),
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
            eq_label,
            paused: held || player.is_paused(),
            stopped: held,
            show_list: ui.show_list(),
            list_names: &list_names,
            toast: toast_owned.as_deref(),
        };
        ui.draw(&frame)?;

        if event::poll(Duration::from_millis(50)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match handle_key(key, player) {
                    Action::None => {}
                    Action::Quit => {
                        player.stop();
                        done_msg = "bye — thanks for listening ♪";
                        break;
                    }
                    Action::Next => {
                        held = false;
                        if index + 1 < playlist.len() {
                            index += 1;
                            if let Some(t) = playlist.get(index) {
                                player.play_file(&t.path)?;
                            }
                        } else if loop_playlist {
                            index = 0;
                            if let Some(t) = playlist.get(index) {
                                player.play_file(&t.path)?;
                            }
                        } else {
                            ui.toast("already at last track");
                        }
                    }
                    Action::Prev => {
                        held = false;
                        if !player.is_idle() && player.position() > Duration::from_secs(3) {
                            let _ = player.seek(Duration::ZERO);
                            ui.toast("restarted");
                        } else if index > 0 {
                            index -= 1;
                            if let Some(t) = playlist.get(index) {
                                player.play_file(&t.path)?;
                            }
                        } else if player.is_idle() {
                            if let Some(t) = playlist.get(index) {
                                player.play_file(&t.path)?;
                            }
                        } else {
                            let _ = player.seek(Duration::ZERO);
                        }
                    }
                    Action::List => {
                        ui.toggle_list();
                    }
                    Action::Shuffle => {
                        let current_path = playlist.get(index).map(|t| t.path.clone());
                        playlist.shuffle();
                        if let Some(path) = current_path {
                            if let Some(new_idx) =
                                playlist.tracks().iter().position(|t| t.path == path)
                            {
                                index = new_idx;
                            }
                        }
                        ui.toast("shuffled");
                    }
                    Action::Jump(n) => {
                        if n >= 1 && n <= playlist.len() {
                            held = false;
                            index = n - 1;
                            if let Some(t) = playlist.get(index) {
                                player.play_file(&t.path)?;
                            }
                        } else {
                            ui.toast("track out of range");
                        }
                    }
                    Action::Stop => {
                        player.stop();
                        held = true;
                        ui.toast("stopped");
                    }
                    Action::PlayPause => {
                        if held {
                            held = false;
                            if let Some(t) = playlist.get(index) {
                                player.play_file(&t.path)?;
                            }
                            ui.toast("playing");
                        } else {
                            let paused = player.toggle_pause();
                            ui.toast(if paused { "paused" } else { "playing" });
                        }
                    }
                    Action::Help => {
                        ui.toast("space n/p ←→ {} m e [] ,. +/- l r s q");
                    }
                    Action::VolChanged(v) => {
                        ui.toast(format!("volume {v}%"));
                    }
                    Action::Muted(m) => {
                        ui.toast(if m { "muted" } else { "unmuted" });
                    }
                    Action::SpeedChanged(s) => {
                        ui.toast(format!("speed {s:.1}x"));
                    }
                    Action::PitchChanged(p) => {
                        ui.toast(format!("pitch {p:.2}"));
                    }
                    Action::EqChanged(label) => {
                        ui.toast(format!("eq {label}"));
                    }
                    Action::ResetTempo => {
                        ui.toast("speed/pitch reset");
                    }
                    Action::Seeked => {}
                }
            }
        }
    }

    ui.leave()?;
    println!();
    print_success(done_msg);

    Ok(())
}

enum Action {
    None,
    Quit,
    Next,
    Prev,
    List,
    Shuffle,
    Stop,
    PlayPause,
    Help,
    Jump(usize),
    VolChanged(u8),
    Muted(bool),
    SpeedChanged(f64),
    PitchChanged(f64),
    EqChanged(&'static str),
    ResetTempo,
    Seeked,
}

fn handle_key(key: KeyEvent, player: &mut Player) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Action::Quit;
    }

    match key.code {
        KeyCode::Char(' ') | KeyCode::Char('t') => Action::PlayPause,
        KeyCode::Char('n') | KeyCode::Char('>') | KeyCode::Down => Action::Next,
        KeyCode::Char('p') | KeyCode::Char('<') | KeyCode::Up => Action::Prev,
        KeyCode::Char('s') => Action::Stop,
        KeyCode::Char('+') | KeyCode::Char('=') => {
            let v = player.volume_step_up();
            Action::VolChanged(v)
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            let v = player.volume_step_down();
            Action::VolChanged(v)
        }
        KeyCode::Right => {
            player.seek_short_forward();
            Action::Seeked
        }
        KeyCode::Left => {
            player.seek_short_back();
            Action::Seeked
        }
        KeyCode::Char('{') => {
            player.seek_long_back();
            Action::Seeked
        }
        KeyCode::Char('}') => {
            player.seek_long_forward();
            Action::Seeked
        }
        KeyCode::Char('m') => {
            let muted = player.toggle_mute();
            Action::Muted(muted)
        }
        KeyCode::Char('e') => {
            let eq = player.cycle_eq();
            Action::EqChanged(eq.label())
        }
        KeyCode::Char('[') => {
            let s = player.speed_down();
            Action::SpeedChanged(s)
        }
        KeyCode::Char(']') => {
            let s = player.speed_up();
            Action::SpeedChanged(s)
        }
        KeyCode::Char(',') => {
            let p = player.pitch_down();
            Action::PitchChanged(p)
        }
        KeyCode::Char('.') => {
            let p = player.pitch_up();
            Action::PitchChanged(p)
        }
        KeyCode::Char('0') => {
            player.reset_speed_pitch();
            Action::ResetTempo
        }
        KeyCode::Char('l') => Action::List,
        KeyCode::Char('r') => Action::Shuffle,
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('h') | KeyCode::Char('?') => Action::Help,
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            Action::Jump(c.to_digit(10).unwrap_or(1) as usize)
        }
        _ => Action::None,
    }
}

fn cmd_info(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("path does not exist: {}", path.display());
    }
    let meta = std::fs::metadata(path).context("reading file metadata")?;
    let size_h = human_bytes(meta.len());
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or_else(|| path.to_str().unwrap_or("—"));

    println!("  {}", name.with(BRIGHT).bold());
    println!("  {}", path.display().to_string().with(DIM));
    println!();

    let fmt = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_uppercase())
        .unwrap_or_else(|| "—".into());

    println!("  {}  {}", "size".with(DIM), size_h.with(GRAY));
    println!("  {}  {}", "format".with(DIM), fmt.with(GRAY));

    match player::probe_duration(path) {
        Some(d) => {
            println!(
                "  {}  {}",
                "duration".with(DIM),
                ui::fmt_time(d).with(BRIGHT)
            );
        }
        None => {
            print_warn("duration unavailable (mpv probe failed)");
        }
    }

    Ok(())
}

fn cmd_list(path: &std::path::Path, recursive: bool) -> Result<()> {
    let tracks = playlist::scan_path(path, recursive)?;
    if tracks.is_empty() {
        print_warn("no audio files found");
        return Ok(());
    }

    println!(
        "  {} {}",
        format!("{}", tracks.len()).with(BRIGHT),
        if tracks.len() == 1 {
            "track".with(DIM)
        } else {
            "tracks".with(DIM)
        }
    );
    println!();
    for (i, track) in tracks.iter().enumerate() {
        println!(
            "  {}  {}",
            format!("{:>2}", i + 1).with(DIM),
            track.display_name().with(BRIGHT)
        );
    }
    Ok(())
}

fn human_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", bytes, UNITS[unit])
    } else {
        format!("{:.1} {}", size, UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn human_bytes_formats() {
        assert_eq!(human_bytes(500), "500 B");
        assert_eq!(human_bytes(2048), "2.0 KB");
    }

    #[test]
    fn resolve_play_paths_keeps_explicit() {
        let paths = resolve_play_paths(vec![PathBuf::from("a.mp3")], "").unwrap();
        assert_eq!(paths, vec![PathBuf::from("a.mp3")]);
    }
}
