//! optionMusic — minimal black & white CLI music player (option music).
//!
//! Binaries: `optionmusic`, short alias `msc`.
//! Engine: MPV via libmpv2.

use std::io::{self, IsTerminal};
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context, Result};
use clap::Parser;
use crossterm::event::{
    self, Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use crossterm::style::Stylize;

use optionmusic::cli::{Cli, Command, LibraryCmd, PlaylistCmd};
use optionmusic::config::{AppConfig, RepeatMode, resolve_music_dir};
use optionmusic::download::{self, DownloadRequest, MediaKind};
use optionmusic::library::Library;
use optionmusic::lyrics::{LyricsLoader, LyricsState};
use optionmusic::plain::Event as PlainEvent;
use optionmusic::player::Player;
use optionmusic::playlist::Playlist;
use optionmusic::session::{
    Action, LoopMode, go_next, go_prev, handle_key, list_nav_key, lyrics_query, maybe_count_stats,
    nanos_seed, save_session,
};
use optionmusic::settings::SettingsAction;
use optionmusic::sleep::SleepTimer;
use optionmusic::smart_shuffle::RecentWindow;
use optionmusic::stats::StatsStore;
use optionmusic::ui::{
    APP_NAME, BRIGHT, BarClick, DIM, FrameState, GRAY, HitTarget, SessionUi, WHITE, banner,
    bin_name, print_info, print_success, print_warn,
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
        if !first.starts_with('-') && !optionmusic::cli::is_subcommand(first) {
            let mut rewritten = Vec::with_capacity(raw.len() + 1);
            rewritten.push(raw[0].clone());
            rewritten.push("play".into());
            rewritten.extend(raw.into_iter().skip(1));
            return Cli::parse_from(rewritten);
        }
    }
    Cli::parse()
}

/// How much of stdout belongs to humans: `--quiet` drops the banner, `--json`
/// replaces the plain-mode lines with NDJSON and rules out the TUI entirely.
#[derive(Debug, Clone, Copy)]
struct OutputMode {
    quiet: bool,
    json: bool,
}

impl OutputMode {
    fn interactive(&self) -> bool {
        !self.json && io::stdin().is_terminal() && io::stdout().is_terminal()
    }

    /// Human lines are silenced by `--quiet` and replaced by `--json`.
    fn prose(&self) -> bool {
        !self.quiet && !self.json
    }
}

fn run() -> Result<()> {
    let cli = parse_cli();
    let bin = bin_name();
    let quiet = cli.quiet || cli.json;
    let out = OutputMode {
        quiet: cli.quiet,
        json: cli.json,
    };

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
            // Bare `play` (no paths, no playback flags) may resume the saved
            // session instead of starting a fresh library scan.
            let bare = paths.is_empty()
                && volume.is_none()
                && speed.is_none()
                && pitch.is_none()
                && eq.is_none()
                && !shuffle
                && !loop_playlist
                && !loop_file;
            let loop_flag = if loop_file {
                Some(LoopMode::Track)
            } else if loop_playlist {
                Some(LoopMode::Playlist)
            } else {
                None
            };
            let paths = resolve_play_paths(paths, &cli.music_dir)?;
            cmd_play(
                paths,
                volume,
                speed,
                pitch,
                eq.map(|e| e.to_preset()),
                crossfade,
                shuffle,
                loop_flag,
                cli.cava,
                out,
                bare,
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
        Some(Command::Download {
            query,
            provider,
            audio,
            video,
            both,
            output,
            audio_format,
            interactive,
            ui,
            fallback_mweb,
            no_fallback,
        }) => {
            if !quiet {
                banner();
            }
            cmd_download(
                query.as_deref(),
                provider,
                audio,
                video,
                both,
                output.as_deref(),
                &audio_format,
                interactive,
                ui.map(|u| u.to_mode()),
                fallback_mweb,
                no_fallback,
                &cli.music_dir,
            )?;
        }
        Some(Command::Playlist { action }) => {
            if !quiet {
                banner();
            }
            cmd_playlist(action)?;
        }
        Some(Command::Library { action }) => {
            if !quiet {
                banner();
            }
            cmd_library(action, &cli.music_dir)?;
        }
        Some(Command::Browse { favorites }) => {
            if !quiet {
                banner();
            }
            cmd_browse(favorites, &cli.music_dir, cli.cava)?;
        }
        Some(Command::Stats { limit }) => {
            if !quiet {
                banner();
            }
            cmd_stats(limit)?
        }
        Some(Command::Sleep { arg }) => {
            if !quiet {
                banner();
            }
            cmd_sleep(arg.as_deref())?
        }
        Some(Command::Radio {
            seed,
            artist,
            genre,
            fresh,
            volume,
            speed,
            pitch,
            eq,
            crossfade,
            loop_playlist,
            loop_file,
        }) => {
            let loop_flag = if loop_file {
                Some(LoopMode::Track)
            } else if loop_playlist {
                Some(LoopMode::Playlist)
            } else {
                None
            };
            cmd_radio(
                seed,
                artist,
                genre,
                fresh,
                volume,
                speed,
                pitch,
                eq.map(|e| e.to_preset()),
                crossfade,
                loop_flag,
                &cli.music_dir,
                cli.cava,
                out,
            )?;
        }
        Some(Command::Doctor) => {
            cmd_doctor();
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
                format!(
                    "optionmusic · msc  ({bin})  ·  mpv  ·  {}",
                    optionmusic::platform::label()
                )
                .with(GRAY)
            );
            if let Some(caveat) = optionmusic::platform::caveat() {
                print_warn(&caveat);
            }
        }
        None => {
            banner();
            let b = bin.as_str();
            println!("  {} p song.mp3", b.with(BRIGHT));
            println!("  {} play ./music/ -s -l --cava", b.with(BRIGHT));
            println!("  {} dl", b.with(BRIGHT));
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

#[allow(clippy::too_many_arguments)]
fn cmd_play(
    paths: Vec<PathBuf>,
    volume: Option<u8>,
    speed: Option<f64>,
    pitch: Option<f64>,
    eq: Option<optionmusic::eq::EqPreset>,
    crossfade: f64,
    shuffle: bool,
    loop_flag: Option<LoopMode>,
    enable_cava: bool,
    out: OutputMode,
    bare: bool,
) -> Result<()> {
    if paths.is_empty() {
        anyhow::bail!("pass at least one file or directory to play");
    }

    // Saved playback prefs are the defaults; explicit CLI flags always win.
    let prefs = AppConfig::load();
    let volume = volume.unwrap_or(prefs.volume);
    let speed = speed.unwrap_or(prefs.speed);
    let pitch = pitch.unwrap_or(prefs.pitch);
    let eq = eq.unwrap_or(prefs.eq);
    let loop_mode = loop_flag.unwrap_or(match prefs.repeat {
        RepeatMode::Off => LoopMode::Off,
        RepeatMode::All => LoopMode::Playlist,
        RepeatMode::One => LoopMode::Track,
    });

    let mut playlist = Playlist::from_paths(&paths)?;
    if playlist.is_empty() {
        anyhow::bail!("no playable audio files found in the given paths");
    }

    if shuffle {
        playlist.shuffle();
    }

    // Session resume: bare `play` restores the saved queue order and seeks
    // back to the saved track/position, paused (same idea as the old desktop).
    let mut resume_at: Option<(usize, f64)> = None;
    if bare && prefs.resume && !prefs.resume_track.is_empty() {
        resume_at = playlist
            .resume_order(&prefs.resume_queue, &prefs.resume_track)
            .map(|i| (i, prefs.resume_position));
    }

    let mut player = Player::new(
        volume.min(optionmusic::config::VOLUME_MAX_EXCESS),
        speed,
        crossfade,
    )?;
    player.set_volume_max(prefs.volume_max());
    player.set_volume(volume);
    player.set_pitch(pitch);
    player.set_eq(eq);
    player.set_loop_track(loop_mode == LoopMode::Track);

    if out.interactive() {
        run_session(
            &mut player,
            &mut playlist,
            loop_mode,
            shuffle,
            enable_cava,
            resume_at,
            prefs.smart_shuffle,
            None,
        )?;
    } else {
        if out.prose() {
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
            out,
        )?;
    }

    Ok(())
}

/// Non-TTY playback: no key loop, so a signal is the only way out. Listening
/// time is counted like the TUI does, and Ctrl-C stops MPV and saves before
/// exiting with the conventional 130.
fn run_plain(
    player: &mut Player,
    playlist: &mut Playlist,
    loop_playlist: bool,
    out: OutputMode,
) -> Result<()> {
    optionmusic::signals::install();
    let mut rpc = optionmusic::rpc::Rpc::new();
    let cfg = AppConfig::load();
    let _ = rpc.sync(cfg.discord_rpc, &cfg.discord_rpc_id);
    let mut stats_store = StatsStore::load();
    let total = playlist.len();
    let mut tracks_played = 0usize;
    let mut interrupted = false;

    emit(out, &PlainEvent::Start { tracks: total });

    'queue: loop {
        for (idx, track) in playlist.tracks().iter().enumerate() {
            // A signal between tracks must not start another one.
            if optionmusic::signals::interrupted() {
                interrupted = true;
                break 'queue;
            }
            if out.prose() {
                print_info(&format!("[{}/{}] {}", idx + 1, total, track.display_name()));
            }
            player.play_file(&track.path)?;
            let duration = track
                .duration_secs
                .map(Duration::from_secs_f64)
                .or_else(|| player.duration());
            emit(out, &PlainEvent::track(idx + 1, total, track, duration));

            let mut max_pos = Duration::ZERO;
            let mut counted = false;
            while !player.is_idle() {
                if optionmusic::signals::interrupted() {
                    interrupted = true;
                    break;
                }
                max_pos = max_pos.max(player.position());
                rpc.update(
                    &track.display_name(),
                    &track.artist_album(),
                    track.thumb_url().as_deref(),
                    player.position(),
                    player.duration(),
                    false,
                );
                maybe_count_stats(
                    &mut stats_store,
                    track,
                    max_pos,
                    player.duration(),
                    &mut counted,
                );
                std::thread::sleep(Duration::from_millis(100));
            }
            max_pos = max_pos.max(player.position());
            maybe_count_stats(
                &mut stats_store,
                track,
                max_pos,
                player.duration(),
                &mut counted,
            );
            tracks_played += 1;
            emit(
                out,
                &PlainEvent::TrackEnd {
                    index: idx + 1,
                    played: max_pos.as_secs_f64(),
                    completed: !interrupted,
                },
            );
            if interrupted {
                break 'queue;
            }
        }
        if !loop_playlist {
            break;
        }
    }

    if interrupted {
        player.stop();
    }
    let _ = stats_store.save();
    rpc.clear();

    if interrupted {
        emit(out, &PlainEvent::Interrupted { tracks_played });
        if out.prose() {
            print_info("Stopped.");
        }
        // MPV is stopped and everything is persisted, so exiting here is safe and
        // keeps the shell's `128 + signo` contract for "killed by a signal".
        io::Write::flush(&mut io::stdout()).ok();
        std::process::exit(optionmusic::signals::exit_code());
    }

    emit(out, &PlainEvent::End { tracks_played });
    if out.prose() {
        print_success("Done. Thanks for listening ♪");
    }
    Ok(())
}

/// One NDJSON line per event, flushed so a consumer sees it while the track
/// plays. Write errors (a `head`-style reader closing the pipe) are ignored
/// rather than panicking mid-playback like `println!` would.
fn emit(out: OutputMode, event: &PlainEvent) {
    if out.json {
        use io::Write;
        let mut stdout = io::stdout().lock();
        let _ = writeln!(stdout, "{}", event.to_line());
        let _ = stdout.flush();
    }
}

#[allow(clippy::too_many_arguments)]
fn run_session(
    player: &mut Player,
    playlist: &mut Playlist,
    mut loop_mode: LoopMode,
    shuffled: bool,
    enable_cava: bool,
    resume_at: Option<(usize, f64)>,
    initial_smart: bool,
    intro: Option<String>,
) -> Result<()> {
    let mut ui = SessionUi::enter(enable_cava).context("failed to open player UI")?;
    player.set_volume_max(ui.volume_max());
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
    if let Some(label) = intro {
        start_toast.push_str(&format!(" · {label}"));
    }
    if loop_mode != LoopMode::Off {
        start_toast.push_str(&format!(" · repeat {}", loop_mode.label()));
    }
    ui.toast_info(start_toast);

    let mut index: usize = resume_at.map(|(i, _)| i).unwrap_or(0);
    let mut held = false;
    let mut dragging_progress = false;
    // Scrollbar thumb drag: (grab row, scroll at grab) for 1:1 tracking.
    let mut list_drag: Option<(u16, usize)> = None;
    let mut quitting = false;
    let mut done_msg = "done — thanks for listening ♪";

    // Discord Rich Presence (opt-in) + session persistence throttle.
    let mut rpc = optionmusic::rpc::Rpc::new();
    // MPRIS on the session bus (on by default) — multimedia keys + playerctl.
    let mut mpris = optionmusic::mpris::Mpris::new();
    let mut last_save = std::time::Instant::now();

    // Smart shuffle (history-aware advance), sleep timer, stats + lyrics.
    let mut smart = initial_smart;
    let mut recent = RecentWindow::new(playlist.len());
    recent.push(index);
    let mut sleep = SleepTimer::new();
    if let Some(mins) = optionmusic::sleep::take_request()
        && mins > 0
    {
        ui.toast_config(sleep.set_minutes(mins));
    }
    let mut stats_store = StatsStore::load();
    let mut stats_max_pos = Duration::ZERO;
    let mut stats_dur: Option<Duration> = None;
    let mut stats_counted = false;
    let mut lyrics = LyricsLoader::new();

    if let Some(track) = playlist.get(index) {
        match resume_at {
            Some((_, pos)) => {
                player.play_file_paused_at(&track.path, pos)?;
                ui.toast_info(format!("resumed · {}", track.display_name()));
            }
            None => player.play_file(&track.path)?,
        }
    }

    loop {
        if !held && player.is_idle() && !player.loop_track() {
            // Repeat-one is handled by mpv (`loop-file`); here the current
            // track restarts itself, so idle only means "advance or end".
            if !go_next(
                player,
                playlist,
                &mut ui,
                &mut index,
                loop_mode,
                smart,
                &mut recent,
                &mut stats_store,
                &mut stats_max_pos,
                &mut stats_dur,
                &mut stats_counted,
                &mut lyrics,
            )? {
                break;
            }
        }

        if !held {
            let pos = player.position();
            if pos > stats_max_pos {
                stats_max_pos = pos;
            }
            if stats_dur.is_none() {
                stats_dur = player.duration();
            }
        }
        if sleep.is_expired() {
            sleep.clear();
            player.set_paused(true);
            ui.toast_config("sleep timer · stopped");
        }

        // Discord presence: follows the settings toggle live; deduped inside.
        if let Some(warn) = rpc.sync(ui.config().discord_rpc, &ui.config().discord_rpc_id) {
            ui.toast_config(warn);
        }
        if let Some(t) = playlist.get(index) {
            if held {
                // Stopped via `s`: nothing playing — drop the presence
                // instead of lingering on the last track.
                rpc.clear();
            } else {
                rpc.update(
                    &t.display_name(),
                    &t.artist_album(),
                    t.thumb_url().as_deref(),
                    player.position(),
                    player.duration(),
                    player.is_paused(),
                );
            }
        }

        // MPRIS: same live-toggle shape as the presence, then run whatever the
        // desktop (multimedia keys, panel widget, playerctl) asked for through
        // the very code paths the keyboard uses.
        if let Some(warn) = mpris.sync(ui.config().mpris) {
            ui.toast_config(warn);
        }
        for cmd in mpris.take_commands() {
            use optionmusic::mpris::Command;
            match cmd {
                Command::Play | Command::PlayPause | Command::Pause => {
                    let want_pause = matches!(cmd, Command::Pause)
                        || (matches!(cmd, Command::PlayPause) && !player.is_paused() && !held);
                    if held && !want_pause {
                        held = false;
                        if let Some(t) = playlist.get(index) {
                            player.play_file(&t.path)?;
                        }
                    } else if !held {
                        player.set_paused(want_pause);
                    }
                }
                Command::Stop => {
                    player.stop();
                    held = true;
                    ui.toast_info("stopped");
                }
                Command::Next => {
                    held = false;
                    if !go_next(
                        player,
                        playlist,
                        &mut ui,
                        &mut index,
                        loop_mode,
                        smart,
                        &mut recent,
                        &mut stats_store,
                        &mut stats_max_pos,
                        &mut stats_dur,
                        &mut stats_counted,
                        &mut lyrics,
                    )? {
                        ui.toast_info("already at last track");
                    }
                }
                Command::Previous => {
                    held = false;
                    go_prev(player, playlist, &mut ui, &mut index)?;
                }
                Command::Seek(delta, forward) => {
                    let now = player.position();
                    let target = if forward {
                        now + delta
                    } else {
                        now.saturating_sub(delta)
                    };
                    let _ = player.seek(target);
                }
                Command::SetPosition(target) => {
                    let _ = player.seek(target);
                }
                Command::SetVolume(pct) => {
                    player.set_volume(pct);
                    ui.toast_config(format!("volume {}%", player.volume()));
                }
                Command::Quit => {
                    player.stop();
                    done_msg = "bye — thanks for listening ♪";
                    dragging_progress = false;
                    list_drag = None;
                    quitting = true;
                }
            }
        }
        if quitting {
            break;
        }
        if let Some(t) = playlist.get(index) {
            mpris.update(optionmusic::mpris::Status {
                title: t.display_name(),
                artist: t.artist.clone().unwrap_or_default(),
                album: t.album.clone().unwrap_or_default(),
                path: t.path.to_string_lossy().into_owned(),
                position: if held {
                    Duration::ZERO
                } else {
                    player.position()
                },
                duration: player.duration(),
                paused: player.is_paused(),
                stopped: held,
                volume: player.volume(),
                can_next: index + 1 < playlist.len() || loop_mode == LoopMode::Playlist,
                can_prev: index > 0,
            });
        }

        // Session persistence: throttle writes while playing; final save on quit.
        if last_save.elapsed() >= Duration::from_secs(5) {
            save_session(&mut ui, player, playlist, index, loop_mode, smart, true);
            last_save = std::time::Instant::now();
        }

        let list_names: Vec<String> = if ui.list_panel_active() {
            playlist.tracks().iter().map(|t| t.display_name()).collect()
        } else {
            Vec::new()
        };

        let track = playlist.get(index);
        let name = track
            .map(|t| t.display_name())
            .unwrap_or_else(|| "—".into());
        let path = track
            .map(|t| t.path.display().to_string())
            .unwrap_or_default();
        let toast_owned = ui.toast_text().map(|s| s.to_string());
        let eq_label = player.eq_label();
        let sleep_owned = sleep.countdown_label();

        // Lyrics dock content (resolved lazily per track, then cached).
        // Synced lines carry word timings for karaoke; plain falls back
        // to a static scrollable list; empty stays `no lyrics found`.
        let empty_synced: Vec<optionmusic::lyrics::LrcLine> = Vec::new();
        let empty_plain: Vec<String> = Vec::new();
        let lyric_now = if held {
            Duration::ZERO
        } else {
            player.position()
        };
        let (lyric_synced, lyric_plain, lyric_active, lyric_status): (
            &[optionmusic::lyrics::LrcLine],
            &[String],
            Option<usize>,
            Option<String>,
        ) = if ui.lyrics_open() {
            if let Some(track) = playlist.get(index) {
                let key = track.path.to_string_lossy();
                let dur = if held { None } else { player.duration() };
                match lyrics.state(&key, || lyrics_query(track, dur)) {
                    LyricsState::Ready(r) if !r.lines.is_empty() => (
                        &r.lines[..],
                        &empty_plain[..],
                        r.active_index(lyric_now),
                        None,
                    ),
                    LyricsState::Ready(r) if !r.plain.is_empty() => {
                        (&empty_synced[..], &r.plain[..], None, None)
                    }
                    LyricsState::Loading => (
                        &empty_synced[..],
                        &empty_plain[..],
                        None,
                        Some("searching lyrics…".into()),
                    ),
                    _ => (
                        &empty_synced[..],
                        &empty_plain[..],
                        None,
                        Some("no lyrics found".into()),
                    ),
                }
            } else {
                (
                    &empty_synced,
                    &empty_plain,
                    None,
                    Some("no lyrics found".into()),
                )
            }
        } else {
            (&empty_synced, &empty_plain, None, None)
        };
        let lyric_total = if lyric_synced.is_empty() {
            lyric_plain.len()
        } else {
            lyric_synced.len()
        };

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
            sleep_label: sleep_owned.as_deref(),
            smart_shuffle: smart,
            lyrics_open: ui.lyrics_open(),
            lyrics_title: &name,
            lyrics_synced: lyric_synced,
            lyrics_plain: lyric_plain,
            lyrics_active: lyric_active,
            lyrics_status: lyric_status.as_deref(),
            lyrics_now: lyric_now,
            list_names: &list_names,
            toast: toast_owned.as_deref(),
        };
        ui.draw(&frame)?;

        // ~60 fps normally; LDM drops to ~30 fps.
        let poll_ms = if ui.ldm() { 33 } else { 16 };
        if event::poll(Duration::from_millis(poll_ms)).unwrap_or(false) {
            loop {
                match event::read() {
                    Ok(Event::Key(key)) => {
                        if key.kind == KeyEventKind::Press {
                            // Focus order, top card first: Esc closes lyrics,
                            // then help, then the list — settings only gets
                            // Esc when nothing floats above it. The open list
                            // owns its cursor keys even with settings open
                            // (single clear owner); every other
                            // settings-owned key stays with the settings
                            // card per `wants_key`. All remaining keys fall
                            // through to the global shortcuts.
                            if ui.settings_open() {
                                if key.modifiers.contains(KeyModifiers::CONTROL)
                                    && key.code == KeyCode::Char('c')
                                {
                                    player.stop();
                                    done_msg = "bye — thanks for listening ♪";
                                    dragging_progress = false;
                                    list_drag = None;
                                    quitting = true;
                                    continue;
                                }
                                // Top-of-stack close first: Esc closes the
                                // lyrics card, then help, then the list —
                                // settings only gets Esc when nothing floats.
                                if key.code == KeyCode::Esc {
                                    if ui.lyrics_open() {
                                        ui.close_lyrics();
                                        break;
                                    }
                                    if ui.show_help() {
                                        ui.toggle_help();
                                        break;
                                    }
                                    if ui.show_list() {
                                        ui.toggle_list();
                                        break;
                                    }
                                }
                                match key.code {
                                    KeyCode::Char('?') => {
                                        if !ui.try_toggle_help() {
                                            ui.toast_info("no room — close a panel");
                                        }
                                        break;
                                    }
                                    // The help card owns `h` only while it is
                                    // open (top card). Otherwise `h` belongs
                                    // to the list cursor / global shortcuts.
                                    KeyCode::Char('h') if ui.show_help() => {
                                        ui.toggle_help();
                                        break;
                                    }
                                    KeyCode::Char('q') if ui.show_help() => {
                                        ui.toggle_help();
                                        break;
                                    }
                                    _ => {}
                                }
                                // Single clear owner: the open list wins its
                                // navigation keys even with settings open.
                                // Every other settings-owned key
                                // (`c`/`q`/enter/space/`d`/`+`/`-`…) stays
                                // with the settings card per `wants_key`.
                                if ui.settings_wants_key(key.code)
                                    && !(ui.show_list() && list_nav_key(key.code))
                                {
                                    match ui.handle_settings_key(key.code) {
                                        SettingsAction::None | SettingsAction::Closed => {}
                                        SettingsAction::Applied {
                                            message,
                                            sync_volume,
                                            refresh_cava,
                                        } => {
                                            if sync_volume {
                                                player.set_volume_max(ui.volume_max());
                                            }
                                            if refresh_cava {
                                                ui.refresh_cava_if_active();
                                            }
                                            ui.toast_config(message);
                                        }
                                    }
                                    break;
                                }
                            }
                            // The open playlist owns its cursor keys so it never
                            // loses focus to the settings/help cards behind
                            // it. `l`/Esc close the list (handled globally
                            // in top-of-stack order), so they are not moves.
                            // `h` closes the top help card first when open.
                            if key.code == KeyCode::Char('h') && ui.show_help() {
                                ui.toggle_help();
                                break;
                            }
                            if ui.show_list() {
                                match key.code {
                                    KeyCode::Left
                                    | KeyCode::Up
                                    | KeyCode::Char('h')
                                    | KeyCode::Char('k') => {
                                        ui.list_move_cursor(-1);
                                        continue;
                                    }
                                    KeyCode::Right | KeyCode::Down | KeyCode::Char('j') => {
                                        ui.list_move_cursor(1);
                                        continue;
                                    }
                                    KeyCode::PageUp => {
                                        ui.list_page_by(-1);
                                        continue;
                                    }
                                    KeyCode::PageDown => {
                                        ui.list_page_by(1);
                                        continue;
                                    }
                                    KeyCode::Home => {
                                        ui.list_cursor_home();
                                        continue;
                                    }
                                    KeyCode::End => {
                                        ui.list_cursor_end();
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            // Docked lyrics strip owns Up/Down for manual scroll
                            // (auto-follow resumes on track change / Enter /
                            // pressing the active line). The open list wins
                            // first (above); settings wins via `wants_key`
                            // before this point, so this only runs with both
                            // closed. `n`/`p` still change tracks.
                            if ui.lyrics_visible() && !ui.show_list() && !ui.settings_open() {
                                match key.code {
                                    KeyCode::Up => {
                                        ui.lyrics_scroll_by(-1, lyric_total, 3);
                                        continue;
                                    }
                                    KeyCode::Down => {
                                        ui.lyrics_scroll_by(1, lyric_total, 3);
                                        continue;
                                    }
                                    KeyCode::Enter => {
                                        ui.lyrics_resume_follow();
                                        continue;
                                    }
                                    _ => {}
                                }
                            }
                            match handle_key(key, player) {
                                Action::None => {}
                                Action::List => {
                                    if !ui.try_toggle_list() {
                                        ui.toast_info("no room — close a panel");
                                    }
                                }
                                Action::Help => {
                                    if !ui.try_toggle_help() {
                                        ui.toast_info("no room — close a panel");
                                    }
                                }
                                Action::Settings => {
                                    ui.toggle_settings();
                                }
                                Action::TogglePath => {
                                    let on = ui.toggle_path();
                                    ui.toast_config(if on {
                                        "filename on"
                                    } else {
                                        "filename off"
                                    });
                                }
                                Action::Quit => {
                                    // Top-of-stack close order: lyrics, then
                                    // help, then the list, then settings.
                                    if ui.lyrics_open() {
                                        ui.close_lyrics();
                                    } else if ui.show_help() {
                                        ui.toggle_help();
                                    } else if ui.show_list() {
                                        ui.toggle_list();
                                    } else if ui.settings_open() {
                                        ui.close_settings();
                                    } else {
                                        player.stop();
                                        done_msg = "bye — thanks for listening ♪";
                                        dragging_progress = false;
                                        list_drag = None;
                                        quitting = true;
                                    }
                                }
                                Action::Next => {
                                    held = false;
                                    if !go_next(
                                        player,
                                        playlist,
                                        &mut ui,
                                        &mut index,
                                        loop_mode,
                                        smart,
                                        &mut recent,
                                        &mut stats_store,
                                        &mut stats_max_pos,
                                        &mut stats_dur,
                                        &mut stats_counted,
                                        &mut lyrics,
                                    )? {
                                        ui.toast_info("already at last track");
                                    }
                                }
                                Action::Prev => {
                                    held = false;
                                    go_prev(player, playlist, &mut ui, &mut index)?;
                                }
                                Action::Shuffle => {
                                    let current_path = playlist.get(index).map(|t| t.path.clone());
                                    playlist.shuffle();
                                    if let Some(path) = current_path
                                        && let Some(new_idx) =
                                            playlist.tracks().iter().position(|t| t.path == path)
                                    {
                                        index = new_idx;
                                    }
                                    // One-shot reorder invalidates index history.
                                    recent = RecentWindow::new(playlist.len());
                                    recent.push(index);
                                    ui.toast_config("shuffled");
                                }
                                Action::SmartShuffle => {
                                    smart = !smart;
                                    if smart {
                                        recent = RecentWindow::new(playlist.len());
                                        recent.push(index);
                                        ui.toast_config("shuffle · on");
                                    } else {
                                        ui.toast_config("shuffle · off");
                                    }
                                }
                                Action::LyricsToggle => {
                                    ui.toggle_lyrics();
                                    ui.toast_config(if ui.lyrics_open() {
                                        "lyrics · on"
                                    } else {
                                        "lyrics · off"
                                    });
                                }
                                Action::SleepCycle => {
                                    let msg = sleep.cycle();
                                    ui.toast_config(msg);
                                }
                                Action::Jump(n) => {
                                    if n >= 1 && n <= playlist.len() {
                                        held = false;
                                        index = n - 1;
                                        ui.list_set_cursor(n - 1);
                                        if let Some(t) = playlist.get(index) {
                                            player.play_file(&t.path)?;
                                            ui.toast_track(t.display_name());
                                        }
                                    } else {
                                        ui.toast_error("track out of range");
                                    }
                                }
                                Action::Stop => {
                                    player.stop();
                                    held = true;
                                    ui.toast_info("stopped");
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
                                    ui.toast_config(format!("volume {v}%"));
                                }
                                Action::Muted(m) => {
                                    ui.toast_config(if m { "muted" } else { "unmuted" });
                                }
                                Action::SpeedChanged(s) => {
                                    ui.toast_config(format!("speed {s:.1}x"));
                                }
                                Action::PitchChanged(p) => {
                                    ui.toast_config(format!("pitch {p:.2}"));
                                }
                                Action::EqChanged(label) => {
                                    ui.toast_config(format!("eq {label}"));
                                }
                                Action::ResetTempo => {
                                    ui.toast_config("speed/pitch reset");
                                }
                                Action::CavaToggle => {
                                    let msg = ui.toggle_cava();
                                    if msg == "cava unavailable" {
                                        ui.toast_error(msg);
                                    } else {
                                        ui.toast_config(msg);
                                    }
                                }
                                Action::LoopCycle => {
                                    loop_mode = loop_mode.next();
                                    player.set_loop_track(loop_mode == LoopMode::Track);
                                    ui.toast_config(format!("repeat · {}", loop_mode.label()));
                                }
                                Action::Seeked => {}
                            }
                        }
                    }
                    Ok(Event::Mouse(m)) => {
                        // Footer shortcut chips (`space n/p ←→ +/− v c ?`)
                        // are global click targets — same action as the key,
                        // even with the settings card open. The floating help
                        // card still owns its own area (checked inside).
                        if m.kind == MouseEventKind::Down(MouseButton::Left)
                            && ui.footer_hit(m.column, m.row) != HitTarget::None
                        {
                            dragging_progress = false;
                            list_drag = None;
                            match ui.footer_hit(m.column, m.row) {
                                HitTarget::PlayPause => {
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
                                    held = false;
                                    if !player.is_idle()
                                        && player.position() > Duration::from_secs(3)
                                    {
                                        let _ = player.seek(Duration::ZERO);
                                    } else if index > 0 {
                                        index -= 1;
                                        if let Some(t) = playlist.get(index) {
                                            player.play_file(&t.path)?;
                                            ui.toast_track(t.display_name());
                                        }
                                    } else {
                                        let _ = player.seek(Duration::ZERO);
                                    }
                                }
                                HitTarget::Next => {
                                    held = false;
                                    if !go_next(
                                        player,
                                        playlist,
                                        &mut ui,
                                        &mut index,
                                        loop_mode,
                                        smart,
                                        &mut recent,
                                        &mut stats_store,
                                        &mut stats_max_pos,
                                        &mut stats_dur,
                                        &mut stats_counted,
                                        &mut lyrics,
                                    )? {
                                        ui.toast_info("already at last track");
                                    }
                                }
                                HitTarget::SeekBack => {
                                    player.seek_short_back();
                                }
                                HitTarget::SeekForward => {
                                    player.seek_short_forward();
                                }
                                HitTarget::Volume => {
                                    let muted = player.toggle_mute();
                                    ui.toast_config(if muted { "muted" } else { "unmuted" });
                                }
                                HitTarget::VolumeUp => {
                                    let v = player.volume_step_up();
                                    ui.toast_config(format!("volume {v}%"));
                                }
                                HitTarget::VolumeDown => {
                                    let v = player.volume_step_down();
                                    ui.toast_config(format!("volume {v}%"));
                                }
                                HitTarget::Settings => {
                                    ui.toggle_settings();
                                }
                                HitTarget::Help if !ui.try_toggle_help() => {
                                    ui.toast_info("no room — close a panel");
                                }
                                HitTarget::Quit => {
                                    if ui.lyrics_open() {
                                        ui.close_lyrics();
                                    } else if ui.show_help() {
                                        ui.toggle_help();
                                    } else if ui.show_list() {
                                        ui.toggle_list();
                                    } else if ui.settings_open() {
                                        ui.close_settings();
                                    } else {
                                        player.stop();
                                        done_msg = "bye — thanks for listening ♪";
                                        quitting = true;
                                    }
                                }
                                _ => {}
                            }
                        } else if ui.settings_open() {
                            match m.kind {
                                MouseEventKind::Down(MouseButton::Left) => {
                                    if ui.help_contains(m.column, m.row)
                                        || ui.lyrics_contains(m.column, m.row)
                                    {
                                        // Top card consumes its own clicks —
                                        // never leak to settings/player.
                                    } else {
                                        match ui.handle_settings_click(m.column, m.row) {
                                            SettingsAction::Closed => {
                                                // Closing click is consumed.
                                                dragging_progress = false;
                                                list_drag = None;
                                            }
                                            SettingsAction::None => {
                                                // Click inside the settings card
                                                // but on no widget is consumed —
                                                // never leak to the list/player
                                                // behind. Only a true miss falls
                                                // through to the open list, which
                                                // still takes clicks + scrollbar
                                                // drags (thumb-drag, track-page).
                                                if ui.pointer_over_settings(m.column, m.row) {
                                                    dragging_progress = false;
                                                    list_drag = None;
                                                } else {
                                                    match ui.hit_target(m.column, m.row) {
                                                        HitTarget::ListScroll(_) => {
                                                            dragging_progress = false;
                                                            match ui.list_bar_click(m.row) {
                                                                BarClick::Above => {
                                                                    ui.list_page_by(-1);
                                                                }
                                                                BarClick::Below => {
                                                                    ui.list_page_by(1);
                                                                }
                                                                BarClick::Thumb => {}
                                                            }
                                                            list_drag =
                                                                Some((m.row, ui.list_offset()));
                                                        }
                                                        HitTarget::Jump(n) => {
                                                            dragging_progress = false;
                                                            list_drag = None;
                                                            if n >= 1 && n <= playlist.len() {
                                                                held = false;
                                                                index = n - 1;
                                                                ui.list_set_cursor(n - 1);
                                                                if let Some(t) = playlist.get(index)
                                                                {
                                                                    player.play_file(&t.path)?;
                                                                    ui.toast_track(
                                                                        t.display_name(),
                                                                    );
                                                                }
                                                            }
                                                        }
                                                        _ => {
                                                            dragging_progress = false;
                                                            list_drag = None;
                                                        }
                                                    }
                                                }
                                            }
                                            SettingsAction::Applied {
                                                message,
                                                sync_volume,
                                                refresh_cava,
                                            } => {
                                                if sync_volume {
                                                    player.set_volume_max(ui.volume_max());
                                                }
                                                if refresh_cava {
                                                    ui.refresh_cava_if_active();
                                                }
                                                ui.toast_config(message);
                                            }
                                        }
                                    }
                                }
                                MouseEventKind::Drag(MouseButton::Left) => {
                                    // 1:1 thumb drag keeps working with the
                                    // settings card open (same as without).
                                    if let Some((start_row, start_scroll)) = list_drag {
                                        ui.list_drag_to(start_row, start_scroll, m.row);
                                    }
                                }
                                MouseEventKind::Up(MouseButton::Left) => {
                                    dragging_progress = false;
                                    list_drag = None;
                                }
                                MouseEventKind::ScrollUp => {
                                    if ui.pointer_over_list(m.column, m.row) {
                                        ui.list_scroll_by(-3);
                                    } else if ui.lyrics_contains(m.column, m.row) {
                                        ui.lyrics_scroll_by(-1, lyric_total, 3);
                                    }
                                }
                                MouseEventKind::ScrollDown => {
                                    if ui.pointer_over_list(m.column, m.row) {
                                        ui.list_scroll_by(3);
                                    } else if ui.lyrics_contains(m.column, m.row) {
                                        ui.lyrics_scroll_by(1, lyric_total, 3);
                                    }
                                }
                                _ => {}
                            }
                            break;
                        } else {
                            match m.kind {
                                MouseEventKind::Down(MouseButton::Left) => {
                                    // Pressing the active lyric line resumes
                                    // auto-follow after a manual scroll.
                                    if ui.lyrics_hit_active_row(m.row)
                                        && ui.lyrics_contains(m.column, m.row)
                                    {
                                        ui.lyrics_resume_follow();
                                        dragging_progress = false;
                                        list_drag = None;
                                    } else {
                                        match ui.hit_target(m.column, m.row) {
                                            HitTarget::Progress(ratio) => {
                                                held = false;
                                                dragging_progress = true;
                                                list_drag = None;
                                                let _ = player.seek_ratio(ratio);
                                            }
                                            HitTarget::ListScroll(_) => {
                                                // Thumb grabs drag 1:1; clicks above
                                                // or below the thumb page a window.
                                                dragging_progress = false;
                                                match ui.list_bar_click(m.row) {
                                                    BarClick::Above => {
                                                        ui.list_page_by(-1);
                                                    }
                                                    BarClick::Below => {
                                                        ui.list_page_by(1);
                                                    }
                                                    BarClick::Thumb => {}
                                                }
                                                list_drag = Some((m.row, ui.list_offset()));
                                            }
                                            HitTarget::PlayPause => {
                                                dragging_progress = false;
                                                list_drag = None;
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
                                                list_drag = None;
                                                held = false;
                                                if !player.is_idle()
                                                    && player.position() > Duration::from_secs(3)
                                                {
                                                    let _ = player.seek(Duration::ZERO);
                                                } else if index > 0 {
                                                    index -= 1;
                                                    if let Some(t) = playlist.get(index) {
                                                        player.play_file(&t.path)?;
                                                        ui.toast_track(t.display_name());
                                                    }
                                                } else {
                                                    let _ = player.seek(Duration::ZERO);
                                                }
                                            }
                                            HitTarget::Next => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                held = false;
                                                if !go_next(
                                                    player,
                                                    playlist,
                                                    &mut ui,
                                                    &mut index,
                                                    loop_mode,
                                                    smart,
                                                    &mut recent,
                                                    &mut stats_store,
                                                    &mut stats_max_pos,
                                                    &mut stats_dur,
                                                    &mut stats_counted,
                                                    &mut lyrics,
                                                )? {
                                                    ui.toast_info("already at last track");
                                                }
                                            }
                                            HitTarget::Volume => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let muted = player.toggle_mute();
                                                ui.toast_config(if muted {
                                                    "muted"
                                                } else {
                                                    "unmuted"
                                                });
                                            }
                                            HitTarget::VolumeUp => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let v = player.volume_step_up();
                                                ui.toast_config(format!("volume {v}%"));
                                            }
                                            HitTarget::VolumeDown => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let v = player.volume_step_down();
                                                ui.toast_config(format!("volume {v}%"));
                                            }
                                            HitTarget::Eq => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let eq = player.cycle_eq();
                                                ui.toast_config(format!("eq {}", eq.label()));
                                            }
                                            HitTarget::Speed => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let s = player.speed_up();
                                                ui.toast_config(format!("speed {s:.1}x"));
                                            }
                                            HitTarget::Pitch => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let p = player.pitch_up();
                                                ui.toast_config(format!("pitch {p:.2}"));
                                            }
                                            HitTarget::CavaToggle => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                let msg = ui.toggle_cava();
                                                if msg == "cava unavailable" {
                                                    ui.toast_error(msg);
                                                } else {
                                                    ui.toast_config(msg);
                                                }
                                            }
                                            // Footer chips also resolve here when the
                                            // settings card is closed (handled globally
                                            // above when it is open).
                                            HitTarget::Settings => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                ui.toggle_settings();
                                            }
                                            HitTarget::Help => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                if !ui.try_toggle_help() {
                                                    ui.toast_info("no room — close a panel");
                                                }
                                            }
                                            HitTarget::SeekBack => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                player.seek_short_back();
                                            }
                                            HitTarget::SeekForward => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                player.seek_short_forward();
                                            }
                                            HitTarget::Quit => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                // Top-of-stack close order: lyrics,
                                                // then help, then the list, then settings.
                                                if ui.lyrics_open() {
                                                    ui.close_lyrics();
                                                } else if ui.show_help() {
                                                    ui.toggle_help();
                                                } else if ui.show_list() {
                                                    ui.toggle_list();
                                                } else if ui.settings_open() {
                                                    ui.close_settings();
                                                } else {
                                                    player.stop();
                                                    done_msg = "bye — thanks for listening ♪";
                                                    quitting = true;
                                                }
                                            }
                                            HitTarget::Jump(n) => {
                                                dragging_progress = false;
                                                list_drag = None;
                                                if n >= 1 && n <= playlist.len() {
                                                    held = false;
                                                    index = n - 1;
                                                    // Click selects AND focuses: the
                                                    // cursor follows the click.
                                                    ui.list_set_cursor(n - 1);
                                                    if let Some(t) = playlist.get(index) {
                                                        player.play_file(&t.path)?;
                                                        ui.toast_track(t.display_name());
                                                    }
                                                }
                                            }
                                            HitTarget::None => {
                                                dragging_progress = false;
                                                list_drag = None;
                                            }
                                        }
                                    }
                                }
                                MouseEventKind::Drag(MouseButton::Left) => {
                                    // 1:1 thumb drag: one row moved = one row
                                    // scrolled (grab offset preserved).
                                    if let Some((start_row, start_scroll)) = list_drag {
                                        ui.list_drag_to(start_row, start_scroll, m.row);
                                    } else if dragging_progress
                                        && let Some(ratio) = ui.progress_ratio_at_col(m.column)
                                    {
                                        let _ = player.seek_ratio(ratio);
                                    }
                                }
                                MouseEventKind::Up(MouseButton::Left) => {
                                    dragging_progress = false;
                                    list_drag = None;
                                }
                                MouseEventKind::ScrollUp => {
                                    if ui.pointer_over_list(m.column, m.row) {
                                        ui.list_scroll_by(-3);
                                    } else if ui.lyrics_contains(m.column, m.row) {
                                        ui.lyrics_scroll_by(-1, lyric_total, 3);
                                    } else {
                                        player.seek_short_back();
                                    }
                                }
                                MouseEventKind::ScrollDown => {
                                    if ui.pointer_over_list(m.column, m.row) {
                                        ui.list_scroll_by(3);
                                    } else if ui.lyrics_contains(m.column, m.row) {
                                        ui.lyrics_scroll_by(1, lyric_total, 3);
                                    } else {
                                        player.seek_short_forward();
                                    }
                                }
                                _ => {}
                            }
                        }
                    }
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

    // Final stats count for the track we quit on (past ~50% / 60s).
    if let Some(track) = playlist.get(index) {
        maybe_count_stats(
            &mut stats_store,
            track,
            stats_max_pos,
            stats_dur,
            &mut stats_counted,
        );
    }

    // Persist prefs + session for the next launch (`resume` gates inside).
    // `quitting` = user left mid-session → keep resume; natural end clears it.
    save_session(&mut ui, player, playlist, index, loop_mode, smart, quitting);
    rpc.clear();
    mpris.clear();

    ui.leave()?;
    println!();
    print_success(done_msg);

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn cmd_download(
    query: Option<&str>,
    provider: Option<download::Provider>,
    audio: bool,
    video: bool,
    both: bool,
    output: Option<&std::path::Path>,
    audio_format: &str,
    interactive: bool,
    ui_override: Option<optionmusic::config::DlUiMode>,
    fallback_mweb: bool,
    no_fallback: bool,
    music_dir_flag: &str,
) -> Result<()> {
    let kind_flag = if both {
        Some(MediaKind::Both)
    } else if audio {
        Some(MediaKind::Audio)
    } else if video {
        Some(MediaKind::Video)
    } else {
        None
    };

    let use_wizard = interactive || query.is_none_or(|q| q.trim().is_empty());
    let cfg = optionmusic::config::AppConfig::load();
    let fallback = download::resolve_fallback_policy(fallback_mweb, no_fallback, cfg.dl_fallback);
    if use_wizard {
        let ui_mode = ui_override.unwrap_or(cfg.dl_ui);
        return download::run_interactive(
            music_dir_flag,
            query,
            provider,
            kind_flag,
            output,
            audio_format,
            ui_mode,
            fallback,
        );
    }

    let query = query.unwrap().trim().to_string();
    let provider = provider
        .or_else(|| download::detect_provider(&query))
        .unwrap_or(download::Provider::Youtube);
    let kind = kind_flag.unwrap_or_else(|| provider.default_kind());
    let output_dir = download::resolve_output_dir(output, music_dir_flag)?;

    let req = DownloadRequest {
        query,
        provider,
        kind,
        output_dir,
        audio_format: audio_format.to_string(),
    };
    download::run_download(&req, fallback)
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

    match optionmusic::player::probe_duration(path) {
        Some(d) => {
            println!(
                "  {}  {}",
                "duration".with(DIM),
                optionmusic::ui::fmt_time(d).with(BRIGHT)
            );
        }
        None => {
            print_warn("duration unavailable (mpv probe failed)");
        }
    }

    Ok(())
}

fn cmd_list(path: &std::path::Path, recursive: bool) -> Result<()> {
    let tracks = optionmusic::playlist::scan_path(path, recursive)?;
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

fn cmd_library(action: LibraryCmd, music_dir_flag: &str) -> Result<()> {
    let root = resolve_music_dir(music_dir_flag)?;
    let mut library = Library::scan(root.clone())?;
    if !library.walk_errors.is_empty() {
        print_warn(&format!(
            "{} path(s) could not be read",
            library.walk_errors.len()
        ));
        for e in library.walk_errors.iter().take(3) {
            print_warn(e);
        }
    }

    match action {
        LibraryCmd::Refresh => {
            let res = library.refresh()?;
            println!(
                "  {} {}",
                format!("+{} added", res.added).with(BRIGHT),
                format!("· {} removed", res.removed).with(DIM)
            );
            println!(
                "  {} {}",
                format!("{} changed", res.changed).with(GRAY),
                format!("· {} total", res.total).with(GRAY)
            );
        }
        LibraryCmd::Ls => {
            if library.is_empty() {
                print_warn(&format!("no tracks found in {}", root.display()));
                return Ok(());
            }
            println!(
                "  {} {}",
                format!("{}", library.len()).with(BRIGHT),
                if library.len() == 1 {
                    "track".with(DIM)
                } else {
                    "tracks".with(DIM)
                }
            );
            println!();
            for (i, track) in library.tracks().iter().enumerate() {
                println!(
                    "  {}  {}",
                    format!("{:>3}", i + 1).with(DIM),
                    track.display_name().with(BRIGHT)
                );
            }
        }
        LibraryCmd::Paths => {
            for track in library.tracks() {
                println!("{}", track.path.display());
            }
        }
    }
    Ok(())
}

fn cmd_browse(favorites: bool, music_dir_flag: &str, enable_cava: bool) -> Result<()> {
    let root = resolve_music_dir(music_dir_flag)?;
    let library = Library::scan(root.clone())?;
    if library.walk_errors.is_empty() && library.is_empty() {
        print_warn(&format!("no tracks found in {}", root.display()));
        print_warn("add audio files to the folder, or pass another folder with -m / --music-dir");
        return Ok(());
    }
    optionmusic::browse::run(library, favorites, enable_cava)
}

/// `msc radio` — order the whole library as a similarity walk and play it
/// through the normal session UI. See `src/radio.rs` for the scoring.
#[allow(clippy::too_many_arguments)]
fn cmd_radio(
    seed_query: Option<String>,
    artist: Option<String>,
    genre: Option<String>,
    fresh: bool,
    volume: Option<u8>,
    speed: Option<f64>,
    pitch: Option<f64>,
    eq: Option<optionmusic::eq::EqPreset>,
    crossfade: f64,
    loop_flag: Option<LoopMode>,
    music_dir_flag: &str,
    enable_cava: bool,
    out: OutputMode,
) -> Result<()> {
    let prefs = AppConfig::load();
    let root = resolve_music_dir(music_dir_flag)?;
    let library = Library::scan(root.clone())?;
    if !library.walk_errors.is_empty() && out.prose() {
        print_warn(&format!(
            "{} path(s) could not be read",
            library.walk_errors.len()
        ));
    }
    if library.is_empty() {
        anyhow::bail!(
            "no tracks found in {} — add audio files or pass -m / --music-dir",
            root.display()
        );
    }

    let stats = StatsStore::load();
    let recent: std::collections::HashSet<String> = optionmusic::history::played_this_week()
        .into_iter()
        .collect();
    let rng_seed = nanos_seed();
    let Some(seed_idx) = optionmusic::radio::resolve_seed(
        &library,
        seed_query.as_deref(),
        artist.as_deref(),
        genre.as_deref(),
        &stats,
        rng_seed,
    ) else {
        let desc = [
            artist.as_deref().map(|a| format!("artist \"{a}\"")),
            genre.as_deref().map(|g| format!("genre \"{g}\"")),
            seed_query.as_deref().map(|q| format!("\"{q}\"")),
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" + ");
        anyhow::bail!("no match for {desc} — try an artist, album, track or genre name");
    };

    let order = optionmusic::radio::build_order(
        library.tracks(),
        |t| library.artist_name(t),
        &stats,
        &recent,
        seed_idx,
        &optionmusic::radio::RadioOpts {
            fresh,
            seed: rng_seed,
            ..Default::default()
        },
    );
    let seed_track = &library.tracks()[seed_idx];
    let seed_label = format!(
        "{} — {}",
        seed_track.display_name(),
        library.artist_name(seed_track)
    );
    let tracks: Vec<optionmusic::playlist::Track> =
        order.iter().map(|&i| library.tracks()[i].clone()).collect();
    let mut playlist = Playlist::from_tracks(tracks);

    // Saved playback prefs are the defaults; explicit CLI flags always win.
    let volume = volume.unwrap_or(prefs.volume);
    let speed = speed.unwrap_or(prefs.speed);
    let pitch = pitch.unwrap_or(prefs.pitch);
    let eq = eq.unwrap_or(prefs.eq);
    let loop_mode = loop_flag.unwrap_or(match prefs.repeat {
        RepeatMode::Off => LoopMode::Off,
        RepeatMode::All => LoopMode::Playlist,
        RepeatMode::One => LoopMode::Track,
    });

    let mut player = Player::new(
        volume.min(optionmusic::config::VOLUME_MAX_EXCESS),
        speed,
        crossfade,
    )?;
    player.set_volume_max(prefs.volume_max());
    player.set_volume(volume);
    player.set_pitch(pitch);
    player.set_eq(eq);
    player.set_loop_track(loop_mode == LoopMode::Track);

    let label = format!("radio · {seed_label}");
    if out.interactive() {
        run_session(
            &mut player,
            &mut playlist,
            loop_mode,
            false,
            enable_cava,
            None,
            prefs.smart_shuffle,
            Some(label),
        )?;
    } else {
        if out.prose() {
            banner();
            print_success(&format!(
                "Radio · {} tracks · seed {}",
                playlist.len(),
                seed_label
            ));
        }
        run_plain(
            &mut player,
            &mut playlist,
            loop_mode == LoopMode::Playlist,
            out,
        )?;
    }
    Ok(())
}

fn cmd_doctor() {
    use optionmusic::doctor::{Need, checks, downloads_ready};

    let checks = checks();
    println!("  {}", "external tools".with(BRIGHT));
    for c in &checks {
        let tag = match c.need {
            Need::Downloads => "dl",
            Need::Optional => "opt",
        };
        match &c.found {
            Some(path) => println!(
                "  {} {} {}  {}",
                "·".with(BRIGHT),
                c.tool.with(BRIGHT),
                format!("[{tag}]").with(DIM),
                path.display().to_string().with(GRAY)
            ),
            None => {
                println!(
                    "  {} {} {}  {}",
                    "·".with(DIM),
                    c.tool.with(WHITE),
                    format!("[{tag}]").with(DIM),
                    "not found".with(GRAY)
                );
                println!("    {} {}", "↳".with(DIM), c.purpose.with(GRAY));
                println!("    {} {}", "↳".with(DIM), c.hint.with(DIM));
            }
        }
    }
    println!();
    if downloads_ready(&checks) {
        print_success("playback and downloads are ready");
    } else {
        print_warn("playback works; `msc dl` needs the [dl] tools above");
    }
}

fn cmd_stats(limit: usize) -> Result<()> {
    let store = StatsStore::load();
    if store.total_plays == 0 {
        print_warn("no plays recorded yet — play something past halfway");
        return Ok(());
    }
    let limit = limit.clamp(1, 100);
    println!(
        "  {} {}  {} {}",
        format!("{}", store.total_plays).with(BRIGHT),
        if store.total_plays == 1 {
            "play".with(DIM)
        } else {
            "plays".with(DIM)
        },
        optionmusic::ui::fmt_time(Duration::from_secs(store.total_secs)).with(BRIGHT),
        "listened".with(DIM)
    );
    println!();
    println!("  {}", "top tracks".with(BRIGHT));
    for (i, t) in store.top_tracks(limit).iter().enumerate() {
        let plays = if t.plays == 1 {
            "1 play".to_owned()
        } else {
            format!("{} plays", t.plays)
        };
        println!(
            "  {}  {}  {}",
            format!("{:>2}", i + 1).with(DIM),
            t.name.as_str().with(BRIGHT),
            plays.with(GRAY)
        );
    }
    println!();
    println!("  {}", "top artists".with(BRIGHT));
    for (i, (artist, plays)) in store.top_artists(limit).iter().enumerate() {
        let label = if *plays == 1 {
            "1 play".to_owned()
        } else {
            format!("{plays} plays")
        };
        println!(
            "  {}  {}  {}",
            format!("{:>2}", i + 1).with(DIM),
            artist.as_str().with(BRIGHT),
            label.with(GRAY)
        );
    }
    Ok(())
}

fn cmd_sleep(arg: Option<&str>) -> Result<()> {
    match arg.map(str::trim) {
        None | Some("") => match optionmusic::sleep::peek_request() {
            Some(m) => print_info(&format!("sleep timer · {m} min (next session)")),
            None => print_info("sleep timer · off"),
        },
        Some(s) if s.eq_ignore_ascii_case("off") => {
            optionmusic::sleep::write_request(None)?;
            print_success("sleep timer · off");
        }
        Some(s) => {
            let mins: u64 = s
                .parse()
                .with_context(|| format!("expected minutes or `off`, got `{s}`"))?;
            if mins == 0 {
                optionmusic::sleep::write_request(None)?;
                print_success("sleep timer · off");
            } else {
                optionmusic::sleep::write_request(Some(mins))?;
                print_success(&format!("sleep timer · {mins} min (next session)"));
            }
        }
    }
    Ok(())
}

fn cmd_playlist(action: PlaylistCmd) -> Result<()> {
    match action {
        PlaylistCmd::List => {
            let list = optionmusic::saved_playlists::list()?;
            if list.is_empty() {
                print_warn("no playlists yet — try `msc playlist create \"Name\"` or `import`");
                return Ok(());
            }
            for pl in list {
                println!(
                    "  {}  {}  {}",
                    pl.id.with(DIM),
                    pl.name.with(BRIGHT),
                    format!("{} tracks", pl.tracks.len()).with(GRAY)
                );
            }
        }
        PlaylistCmd::Create { name } => {
            let pl = optionmusic::saved_playlists::create(&name)?;
            print_success(&format!("created {} ({})", pl.name, pl.id));
        }
        PlaylistCmd::Import { path, name } => {
            let pl = optionmusic::saved_playlists::import_m3u(&path, name.as_deref())?;
            print_success(&format!(
                "imported {} ({} tracks)",
                pl.name,
                pl.tracks.len()
            ));
        }
        PlaylistCmd::Export { id, path } => {
            optionmusic::saved_playlists::export_m3u(&id, &path)?;
            print_success(&format!("exported to {}", path.display()));
        }
        PlaylistCmd::Add { id, track } => {
            let path = track.canonicalize().unwrap_or(track);
            let pl = optionmusic::saved_playlists::add_track(&id, &path.to_string_lossy())?;
            print_success(&format!(
                "added to {} ({} tracks)",
                pl.name,
                pl.tracks.len()
            ));
        }
        PlaylistCmd::Delete { id } => {
            optionmusic::saved_playlists::delete(&id)?;
            print_success("playlist deleted");
        }
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
