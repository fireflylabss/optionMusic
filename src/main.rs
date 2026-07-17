//! optMusic — minimal black & white CLI music player (option music).
//!
//! Binaries: `optmusic` and short alias `msc`.
//! Engine: MPV via libmpv2.

mod cli;
mod config;
mod cava;
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
use crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::style::Stylize;

use cli::{Cli, Command};
use config::resolve_music_dir;
use player::Player;
use playlist::Playlist;
use ui::{
    banner, bin_name, print_info, print_success, print_warn, FrameState, HitTarget, SessionUi,
    APP_NAME, BRIGHT, DIM, GRAY, WHITE,
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

/// Parse CLI, rewriting bare paths to `play` (`msc song.mp3` → `msc play song.mp3`).
fn parse_cli() -> Cli {
    let raw: Vec<String> = std::env::args().collect();
    if raw.len() >= 2 {
        let first = raw[1].as_str();
        const CMDS: &[&str] = &[
            "play", "p", "pl", "info", "i", "list", "ls", "version", "ver", "help",
        ];
        if !first.starts_with('-') && !CMDS.iter().any(|c| *c == first) {
            let mut rewritten = Vec::with_capacity(raw.len() + 1);
            rewritten.push(raw[0].clone());
            rewritten.push("play".into());
            rewritten.extend(raw.into_iter().skip(1));
            return Cli::parse_from(rewritten);
        }
    }
    Cli::parse()
}

fn run() -> Result<()> {
    let cli = parse_cli();
    let bin = bin_name();
    let quiet = cli.quiet;

    match cli.command {
        Some(Command::Play {
            paths,
            volume,
            speed,
            pitch,
            eq,
            crossfade,
            shuffle,
            loop_playlist,
            loop_file,
            interactive: _,
        }) => {
            let paths = resolve_play_paths(paths, &cli.music_dir)?;
            let loop_mode = if loop_file {
                LoopMode::Track
            } else if loop_playlist {
                LoopMode::Playlist
            } else {
                LoopMode::Off
            };
            cmd_play(
                paths,
                volume,
                speed,
                pitch,
                eq.to_preset(),
                crossfade,
                shuffle,
                loop_mode,
                cli.cava,
                quiet,
            )?;
        }
        Some(Command::Info { path }) => {
            if !quiet {
                banner();
            }
            cmd_info(&path)?
        }
        Some(Command::List { path, recursive }) => {
            if !quiet {
                banner();
            }
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
            println!("  {} p song.mp3", b.with(BRIGHT));
            println!("  {} play ./music/ -s -l --cava", b.with(BRIGHT));
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
    pitch: f64,
    eq: eq::EqPreset,
    crossfade: f64,
    shuffle: bool,
    loop_mode: LoopMode,
    enable_cava: bool,
    quiet: bool,
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
    player.set_pitch(pitch);
    player.set_eq(eq);
    player.set_loop_track(loop_mode == LoopMode::Track);

    if io::stdin().is_terminal() && io::stdout().is_terminal() {
        run_session(
            &mut player,
            &mut playlist,
            loop_mode,
            shuffle,
            enable_cava,
        )?;
    } else {
        if !quiet {
            banner();
            print_success(&format!(
                "Loaded {} track{}",
                playlist.len(),
                if playlist.len() == 1 { "" } else { "s" }
            ));
        }
        run_plain(
            &mut player,
            &mut playlist,
            loop_mode == LoopMode::Playlist,
        )?;
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
    mut loop_mode: LoopMode,
    shuffled: bool,
    enable_cava: bool,
) -> Result<()> {
    let mut ui = SessionUi::enter(enable_cava).context("failed to open player UI")?;
    let mut start_toast = if shuffled {
        format!(
            "{} track{} · shuffled{}",
            playlist.len(),
            if playlist.len() == 1 { "" } else { "s" },
            if ui.cava_active() { " · cava" } else { "" }
        )
    } else {
        format!(
            "{} track{}{}",
            playlist.len(),
            if playlist.len() == 1 { "" } else { "s" },
            if ui.cava_active() { " · cava" } else { "" }
        )
    };
    if loop_mode != LoopMode::Off {
        start_toast.push_str(&format!(" · loop {}", loop_mode.label()));
    }
    ui.toast(start_toast);

    let mut index: usize = 0;
    let mut held = false;
    let mut dragging_progress = false;
    let mut dragging_list_scroll = false;
    let mut quitting = false;
    let mut done_msg = "done — thanks for listening ♪";

    if let Some(track) = playlist.get(index) {
        player.play_file(&track.path)?;
    }

    loop {
        if !held && player.is_idle() && !player.loop_track() {
            if index + 1 < playlist.len() {
                index += 1;
                if let Some(t) = playlist.get(index) {
                    player.play_file(&t.path)?;
                }
            } else if loop_mode == LoopMode::Playlist {
                index = 0;
                ui.toast("looping list…");
                if let Some(t) = playlist.get(index) {
                    player.play_file(&t.path)?;
                }
            } else {
                break;
            }
        }

        let list_names: Vec<String> = if ui.list_panel_active() {
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
            loop_label: loop_mode.label(),
            list_names: &list_names,
            toast: toast_owned.as_deref(),
        };
        ui.draw(&frame)?;

        // ~60 fps keeps progress + cava bars smooth.
        if event::poll(Duration::from_millis(16)).unwrap_or(false) {
            loop {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if key.kind == KeyEventKind::Press {
                            // Playlist sidebar scrolls with arrows / j k / page keys.
                            if ui.show_list() {
                                match key.code {
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        ui.list_scroll_by(-1);
                                        continue;
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
                                        ui.list_scroll_by(1);
                                        continue;
                                    }
                                    KeyCode::PageUp => {
                                        ui.list_scroll_by(-8);
                                        continue;
                                    }
                                    KeyCode::PageDown => {
                                        ui.list_scroll_by(8);
                                        continue;
                                    }
                                    KeyCode::Home => {
                                        ui.list_scroll_ratio(0.0);
                                        continue;
                                    }
                                    KeyCode::End => {
                                        ui.list_scroll_ratio(1.0);
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            match handle_key(key, player) {
                                Action::None => {}
                                Action::List => ui.toggle_list(),
                                Action::Help => ui.toggle_help(),
                                Action::TogglePath => {
                                    let on = ui.toggle_path();
                                    ui.toast(if on { "filename on" } else { "filename off" });
                                }
                                Action::Quit => {
                                    if ui.show_help() {
                                        ui.toggle_help();
                                    } else if ui.show_list() {
                                        ui.toggle_list();
                                    } else {
                                        player.stop();
                                        done_msg = "bye — thanks for listening ♪";
                                        dragging_progress = false;
                                        dragging_list_scroll = false;
                                        quitting = true;
                                    }
                                }
                                Action::Next => {
                                    held = false;
                                    if index + 1 < playlist.len() {
                                        index += 1;
                                        if let Some(t) = playlist.get(index) {
                                            player.play_file(&t.path)?;
                                        }
                                    } else if loop_mode == LoopMode::Playlist {
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
                                    if !player.is_idle()
                                        && player.position() > Duration::from_secs(3)
                                    {
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
                                    } else {
                                        let _ = player.toggle_pause();
                                    }
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
                                Action::CavaToggle => {
                                    let msg = ui.toggle_cava();
                                    ui.toast(msg);
                                }
                                Action::LoopCycle => {
                                    loop_mode = loop_mode.next();
                                    player.set_loop_track(loop_mode == LoopMode::Track);
                                    ui.toast(format!("loop {}", loop_mode.label()));
                                }
                                Action::Seeked => {}
                            }
                        }
                    }
                    Ok(Event::Mouse(m)) => match m.kind {
                        MouseEventKind::Down(MouseButton::Left) => match ui.hit_target(m.column, m.row)
                        {
                            HitTarget::Progress(ratio) => {
                                held = false;
                                dragging_progress = true;
                                dragging_list_scroll = false;
                                let _ = player.seek_ratio(ratio);
                            }
                            HitTarget::ListScroll(ratio) => {
                                dragging_list_scroll = true;
                                dragging_progress = false;
                                ui.list_scroll_ratio(ratio);
                            }
                            HitTarget::PlayPause => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                if held {
                                    held = false;
                                    if let Some(t) = playlist.get(index) {
                                        player.play_file(&t.path)?;
                                    }
                                } else {
                                    let _ = player.toggle_pause();
                                }
                            }
                            HitTarget::Prev => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                held = false;
                                if !player.is_idle()
                                    && player.position() > Duration::from_secs(3)
                                {
                                    let _ = player.seek(Duration::ZERO);
                                } else if index > 0 {
                                    index -= 1;
                                    if let Some(t) = playlist.get(index) {
                                        player.play_file(&t.path)?;
                                    }
                                } else {
                                    let _ = player.seek(Duration::ZERO);
                                }
                            }
                            HitTarget::Next => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                held = false;
                                if index + 1 < playlist.len() {
                                    index += 1;
                                    if let Some(t) = playlist.get(index) {
                                        player.play_file(&t.path)?;
                                    }
                                } else if loop_mode == LoopMode::Playlist {
                                    index = 0;
                                    if let Some(t) = playlist.get(index) {
                                        player.play_file(&t.path)?;
                                    }
                                } else {
                                    ui.toast("already at last track");
                                }
                            }
                            HitTarget::Volume => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let muted = player.toggle_mute();
                                ui.toast(if muted { "muted" } else { "unmuted" });
                            }
                            HitTarget::VolumeUp => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let v = player.volume_step_up();
                                ui.toast(format!("volume {v}%"));
                            }
                            HitTarget::VolumeDown => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let v = player.volume_step_down();
                                ui.toast(format!("volume {v}%"));
                            }
                            HitTarget::Eq => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let eq = player.cycle_eq();
                                ui.toast(format!("eq {}", eq.label()));
                            }
                            HitTarget::Speed => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let s = player.speed_up();
                                ui.toast(format!("speed {s:.1}x"));
                            }
                            HitTarget::Pitch => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let p = player.pitch_up();
                                ui.toast(format!("pitch {p:.2}"));
                            }
                            HitTarget::CavaToggle => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                let msg = ui.toggle_cava();
                                ui.toast(msg);
                            }
                            HitTarget::Jump(n) => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                                if n >= 1 && n <= playlist.len() {
                                    held = false;
                                    index = n - 1;
                                    if let Some(t) = playlist.get(index) {
                                        player.play_file(&t.path)?;
                                    }
                                }
                            }
                            HitTarget::None => {
                                dragging_progress = false;
                                dragging_list_scroll = false;
                            }
                        },
                        MouseEventKind::Drag(MouseButton::Left) => {
                            if dragging_list_scroll {
                                if let Some(ratio) = ui.list_scroll_ratio_at_row(m.row) {
                                    ui.list_scroll_ratio(ratio);
                                }
                            } else if dragging_progress {
                                if let Some(ratio) = ui.progress_ratio_at_col(m.column) {
                                    let _ = player.seek_ratio(ratio);
                                }
                            }
                        }
                        MouseEventKind::Up(MouseButton::Left) => {
                            dragging_progress = false;
                            dragging_list_scroll = false;
                        }
                        MouseEventKind::ScrollUp => {
                            if ui.pointer_over_list(m.column, m.row) {
                                ui.list_scroll_by(-3);
                            } else {
                                player.seek_short_back();
                            }
                        }
                        MouseEventKind::ScrollDown => {
                            if ui.pointer_over_list(m.column, m.row) {
                                ui.list_scroll_by(3);
                            } else {
                                player.seek_short_forward();
                            }
                        }
                        _ => {}
                    },
                    Ok(_) => {}
                    Err(_) => break,
                }
                if quitting {
                    break;
                }
                if !event::poll(Duration::ZERO).unwrap_or(false) {
                    break;
                }
            }
        }

        if quitting {
            break;
        }
    }

    ui.leave()?;
    println!();
    print_success(done_msg);

    Ok(())
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum LoopMode {
    Off,
    Playlist,
    Track,
}

impl LoopMode {
    fn next(self) -> Self {
        match self {
            Self::Off => Self::Playlist,
            Self::Playlist => Self::Track,
            Self::Track => Self::Off,
        }
    }

    fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Playlist => "list",
            Self::Track => "track",
        }
    }
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
    TogglePath,
    Jump(usize),
    VolChanged(u8),
    Muted(bool),
    SpeedChanged(f64),
    PitchChanged(f64),
    EqChanged(&'static str),
    ResetTempo,
    CavaToggle,
    LoopCycle,
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
        KeyCode::Char('o') => Action::LoopCycle,
        KeyCode::Char('f') => Action::TogglePath,
        KeyCode::Char('v') => Action::CavaToggle,
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
