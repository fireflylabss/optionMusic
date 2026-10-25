//! Session control shared by the TUI and tests.
//!
//! The TUI loop in `main.rs` used to own all of this; it lives here so both
//! drive tracks, stats, persistence and key bindings through exactly the same
//! code instead of re-deriving the semantics (3s previous-track grace,
//! once-per-track stats, resume fields).

use std::time::Duration;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MediaKeyCode};

use crate::config::RepeatMode;
use crate::lyrics::LyricsQuery;
use crate::player::Player;
use crate::playlist::{Playlist, QueueUndo, Track};
use crate::smart_shuffle::RecentWindow;
use crate::stats::{StatsStore, should_count_play};
use crate::ui::SessionUi;

/// Persist playback prefs + session (track/pos/queue) into shared config.toml.
/// One switch (`resume`) gates everything; called on quit and periodically.
/// `session_alive=false` (playlist finished naturally) clears the resume fields
/// so a completed queue doesn't reopen next launch.
#[allow(clippy::too_many_arguments)]
pub fn save_session(
    ui: &mut SessionUi,
    player: &Player,
    playlist: &Playlist,
    index: usize,
    loop_mode: LoopMode,
    smart: bool,
    session_alive: bool,
) {
    let cfg = ui.config_mut();
    if !cfg.resume {
        return;
    }
    cfg.volume = player.volume();
    cfg.eq = player.eq();
    cfg.repeat = match loop_mode {
        LoopMode::Off => RepeatMode::Off,
        LoopMode::Playlist => RepeatMode::All,
        LoopMode::Track => RepeatMode::One,
    };
    cfg.speed = player.speed();
    cfg.pitch = player.pitch();
    cfg.smart_shuffle = smart;
    if session_alive {
        cfg.resume_track = playlist
            .get(index)
            .map(|t| t.path.to_string_lossy().into_owned())
            .unwrap_or_default();
        cfg.resume_position = player.resume_position().as_secs_f64();
        cfg.resume_queue = playlist
            .tracks()
            .iter()
            .map(|t| t.path.to_string_lossy().into_owned())
            .collect();
    } else {
        cfg.resume_track.clear();
        cfg.resume_position = 0.0;
        cfg.resume_queue.clear();
    }
    let _ = cfg.save();
}

/// Nanos-based seed for smart-shuffle picks (no rng dep).
pub fn nanos_seed() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x9e3779b97f4a7c15)
}

/// Count a stats play once the track passes ~50% or 60s.
pub fn maybe_count_stats(
    store: &mut StatsStore,
    track: &Track,
    max_pos: Duration,
    dur: Option<Duration>,
    counted: &mut bool,
) {
    if *counted {
        return;
    }
    if should_count_play(max_pos.as_secs_f64(), dur.map(|d| d.as_secs_f64())) {
        let artist = track.artist.clone().unwrap_or_default();
        store.record_play(
            &track.path.to_string_lossy(),
            &track.display_name(),
            &artist,
            max_pos.as_secs(),
        );
        if let Some(sent) =
            crate::scrobble::notify_listen(&artist, &track.display_name(), track.album.as_deref())
        {
            store.record_scrobble(sent);
        }
        *counted = true;
    }
}

/// Radio 2.0 negative feedback for the outgoing track.
///
/// `disliked` (`d` key or `--dislike` flag) always implies a skip: the track
/// gets `record_feedback` (7-day similarity penalty for the track + artist)
/// and a `disliked` history line. A plain early exit (`n` / next before the
/// counting threshold) records a `skipped` history line + feedback without
/// the extra dislike weight. A track that passed the counting threshold (or
/// played to the end) records a `played` history line instead. History and
/// stats writes are best-effort — a full disk must never break playback.
pub fn record_track_outcome(
    store: &mut StatsStore,
    track: &Track,
    max_pos: Duration,
    dur: Option<Duration>,
    counted: &mut bool,
    disliked: bool,
) {
    let id = track.path.to_string_lossy().into_owned();
    if id.is_empty() {
        return;
    }
    let artist = track.artist.clone().unwrap_or_default();
    if disliked {
        // Dislike skips the play count on purpose: the track was rejected.
        store.record_feedback(&id, &track.display_name(), &artist, true);
        let _ = crate::history::record_dislike(&id);
        *counted = true;
        return;
    }
    if should_count_play(max_pos.as_secs_f64(), dur.map(|d| d.as_secs_f64())) {
        if !*counted {
            store.record_play(&id, &track.display_name(), &artist, max_pos.as_secs());
            if let Some(sent) = crate::scrobble::notify_listen(
                &artist,
                &track.display_name(),
                track.album.as_deref(),
            ) {
                store.record_scrobble(sent);
            }
            *counted = true;
        }
        let _ = crate::history::record_play(&id);
    } else {
        // Early exit before the threshold: a skip, not a completed listen.
        // The play is not counted; the track is still penalized for 7 days.
        store.record_feedback(&id, &track.display_name(), &artist, false);
        let _ = crate::history::record_skip(&id);
        *counted = true;
    }
}

/// Next index honoring smart shuffle; `None` means the playlist ended.
pub fn advance_index(
    len: usize,
    index: usize,
    loop_mode: LoopMode,
    smart: bool,
    recent: &[usize],
    seed: u64,
) -> Option<usize> {
    if len == 0 {
        return None;
    }
    if smart {
        return Some(crate::smart_shuffle::pick_next(len, index, recent, seed));
    }
    if index + 1 < len {
        Some(index + 1)
    } else if loop_mode == LoopMode::Playlist {
        Some(0)
    } else {
        None
    }
}

/// Advance to the next track (counts stats for the old one, resets per-track
/// state). Returns `false` when the playlist ended.
///
/// `radio_mode` selects Radio 2.0 outcome accounting: an early exit via `n`
/// records a `skipped` history line + negative feedback (7-day walk penalty)
/// instead of silently dropping the event; completed listens record `played`.
/// `disliked` forces the dislike path (skip + double penalty). Non-radio
/// sessions keep the legacy behaviour (count-or-nothing).
#[allow(clippy::too_many_arguments)]
pub fn go_next(
    player: &mut Player,
    playlist: &Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    loop_mode: LoopMode,
    smart: bool,
    recent: &mut RecentWindow,
    stats_store: &mut StatsStore,
    stats_max_pos: &mut Duration,
    stats_dur: &mut Option<Duration>,
    stats_counted: &mut bool,
) -> Result<bool> {
    go_next_radio(
        player,
        playlist,
        ui,
        index,
        loop_mode,
        smart,
        recent,
        stats_store,
        stats_max_pos,
        stats_dur,
        stats_counted,
        false,
        false,
    )
}

/// [`go_next`] with Radio 2.0 outcome flags. New function (instead of a new
/// parameter on `go_next`) so existing callers keep their current shape.
#[allow(clippy::too_many_arguments)]
pub fn go_next_radio(
    player: &mut Player,
    playlist: &Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    loop_mode: LoopMode,
    smart: bool,
    recent: &mut RecentWindow,
    stats_store: &mut StatsStore,
    stats_max_pos: &mut Duration,
    stats_dur: &mut Option<Duration>,
    stats_counted: &mut bool,
    radio_mode: bool,
    disliked: bool,
) -> Result<bool> {
    if let Some(track) = playlist.get(*index) {
        if radio_mode {
            record_track_outcome(
                stats_store,
                track,
                *stats_max_pos,
                *stats_dur,
                stats_counted,
                disliked,
            );
        } else {
            maybe_count_stats(
                stats_store,
                track,
                *stats_max_pos,
                *stats_dur,
                stats_counted,
            );
        }
    }
    let Some(next) = advance_index(
        playlist.len(),
        *index,
        loop_mode,
        smart,
        &recent.as_slice(),
        nanos_seed(),
    ) else {
        return Ok(false);
    };
    recent.push(*index);
    *index = next;
    *stats_max_pos = Duration::ZERO;
    *stats_dur = None;
    *stats_counted = false;
    if let Some(t) = playlist.get(*index) {
        player.play_file(&t.path)?;
        ui.toast_track(t.display_name());
        scrobble_now_playing(t);
    }
    Ok(true)
}

/// Fire ListenBrainz `playing_now` for a track (no-op without a token).
fn scrobble_now_playing(track: &Track) {
    let artist = track.artist.clone().unwrap_or_default();
    crate::scrobble::notify_now_playing(&artist, &track.display_name(), track.album.as_deref());
}

/// Previous-track behaviour shared by `p` and MPRIS `Previous`: restart the
/// current track when past the 3s grace, otherwise step back.
pub fn go_prev(
    player: &mut Player,
    playlist: &Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
) -> Result<()> {
    if !player.is_idle() && player.position() > Duration::from_secs(3) {
        let _ = player.seek(Duration::ZERO);
        ui.toast_info("restarted");
    } else if *index > 0 {
        *index -= 1;
        if let Some(t) = playlist.get(*index) {
            player.play_file(&t.path)?;
            ui.toast_track(t.display_name());
            scrobble_now_playing(t);
        }
    } else if player.is_idle() {
        if let Some(t) = playlist.get(*index) {
            player.play_file(&t.path)?;
            ui.toast_track(t.display_name());
            scrobble_now_playing(t);
        }
    } else {
        let _ = player.seek(Duration::ZERO);
    }
    Ok(())
}

/// Editable-queue helpers shared by the TUI loop. All mutations snapshot
/// into `undo` first so `u` / Ctrl+Z restores order + current + cursor.
pub fn queue_snapshot(undo: &mut QueueUndo, playlist: &Playlist, index: usize, ui: &SessionUi) {
    undo.push(playlist.tracks(), index, ui.list_cursor());
}

pub fn queue_undo(
    undo: &mut QueueUndo,
    playlist: &mut Playlist,
    index: &mut usize,
    recent: &mut RecentWindow,
    ui: &mut SessionUi,
) -> bool {
    let Some((tracks, current, cursor)) = undo.pop() else {
        return false;
    };
    playlist.replace_tracks(tracks);
    *index = current.min(playlist.len().saturating_sub(1));
    *recent = RecentWindow::new(playlist.len());
    recent.push(*index);
    ui.list_set_cursor(cursor);
    true
}

pub fn queue_reorder_step(
    playlist: &mut Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    undo: &mut QueueUndo,
    recent: &mut RecentWindow,
    delta: i32,
) -> bool {
    let cursor = ui.list_cursor();
    if cursor >= playlist.len() {
        return false;
    }
    queue_snapshot(undo, playlist, *index, ui);
    match playlist.reorder_step(cursor, delta, index) {
        Some(next_cursor) => {
            *recent = RecentWindow::new(playlist.len());
            recent.push(*index);
            ui.list_set_cursor(next_cursor);
            true
        }
        None => {
            let _ = undo.pop();
            false
        }
    }
}

pub fn queue_play_next(
    playlist: &mut Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    undo: &mut QueueUndo,
    recent: &mut RecentWindow,
) -> bool {
    let cursor = ui.list_cursor();
    if cursor >= playlist.len() || *index >= playlist.len() {
        return false;
    }
    queue_snapshot(undo, playlist, *index, ui);
    match playlist.move_to_next(cursor, *index) {
        Some((current, moved)) => {
            *index = current;
            *recent = RecentWindow::new(playlist.len());
            recent.push(*index);
            ui.list_set_cursor(moved);
            true
        }
        None => {
            let _ = undo.pop();
            false
        }
    }
}

pub fn queue_append_end(
    playlist: &mut Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    undo: &mut QueueUndo,
    recent: &mut RecentWindow,
) -> bool {
    let cursor = ui.list_cursor();
    if cursor >= playlist.len() || *index >= playlist.len() {
        return false;
    }
    queue_snapshot(undo, playlist, *index, ui);
    match playlist.move_to_end(cursor, *index) {
        Some((current, moved)) => {
            *index = current;
            *recent = RecentWindow::new(playlist.len());
            recent.push(*index);
            ui.list_set_cursor(moved);
            true
        }
        None => {
            let _ = undo.pop();
            false
        }
    }
}

pub fn queue_clear(
    playlist: &mut Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    undo: &mut QueueUndo,
    recent: &mut RecentWindow,
) -> bool {
    if *index >= playlist.len() {
        return false;
    }
    queue_snapshot(undo, playlist, *index, ui);
    if !playlist.clear_keep_current(*index) {
        let _ = undo.pop();
        return false;
    }
    *index = 0;
    *recent = RecentWindow::new(playlist.len());
    recent.push(0);
    ui.list_set_cursor(0);
    true
}

pub fn queue_shuffle(
    playlist: &mut Playlist,
    ui: &mut SessionUi,
    index: &mut usize,
    undo: &mut QueueUndo,
    recent: &mut RecentWindow,
) {
    let current_path = playlist.get(*index).map(|t| t.path.clone());
    queue_snapshot(undo, playlist, *index, ui);
    playlist.shuffle();
    if let Some(path) = current_path
        && let Some(new_idx) = playlist.tracks().iter().position(|t| t.path == path)
    {
        *index = new_idx;
    }
    *recent = RecentWindow::new(playlist.len());
    recent.push(*index);
}

/// Lyrics lookup context from track tags (filename title fallback).
pub fn lyrics_query(track: &Track, duration: Option<Duration>) -> LyricsQuery {
    let title = track
        .title
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| track.display_name());
    LyricsQuery {
        path: track.path.clone(),
        artist: track.artist.clone().unwrap_or_default(),
        title,
        album: track.album.clone().unwrap_or_default(),
        duration: duration.map(|d| d.as_secs_f64()),
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum LoopMode {
    Off,
    Playlist,
    Track,
}

impl LoopMode {
    pub fn next(self) -> Self {
        match self {
            Self::Off => Self::Playlist,
            Self::Playlist => Self::Track,
            Self::Track => Self::Off,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Playlist => "all",
            Self::Track => "one",
        }
    }
}

pub enum Action {
    None,
    Quit,
    Next,
    Prev,
    Dislike,
    List,
    Shuffle,
    Stop,
    PlayPause,
    Help,
    Settings,
    TogglePath,
    Lyrics,
    Jump(usize),
    VolChanged(u8),
    Muted(bool),
    SpeedChanged(f64),
    PitchChanged(f64),
    EqChanged(&'static str),
    ResetTempo,
    CavaToggle,
    LoopCycle,
    SmartShuffle,
    SleepCycle,
    Seeked,
    PlayNext,
    AppendEnd,
    ClearQueue,
    Undo,
    JumpToCurrent,
    MoveUp,
    MoveDown,
}

/// Keys the open playlist owns for cursor movement. Single clear owner:
/// the list wins these even with the settings card open — every other
/// settings-owned key (`c`/`q`/enter/space/`d`/`+`/`-`…) stays with the
/// settings card per `wants_key`. `l` and Esc are excluded: they close
/// the list in top-of-stack order instead of moving.
pub fn list_nav_key(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Home
            | KeyCode::End
            | KeyCode::Char('h')
            | KeyCode::Char('j')
            | KeyCode::Char('k')
    )
}

pub fn handle_key(key: KeyEvent, player: &mut Player) -> Action {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        match key.code {
            KeyCode::Char('c') => return Action::Quit,
            KeyCode::Char('z') => return Action::Undo,
            _ => {}
        }
    }
    if key.modifiers.contains(KeyModifiers::SHIFT) {
        match key.code {
            KeyCode::Up => return Action::MoveUp,
            KeyCode::Down => return Action::MoveDown,
            KeyCode::Char('J') => return Action::MoveDown,
            KeyCode::Char('K') => return Action::MoveUp,
            _ => {}
        }
    }

    match key.code {
        KeyCode::Char(' ') | KeyCode::Char('t') => Action::PlayPause,
        KeyCode::Char('n') | KeyCode::Char('>') | KeyCode::Down => Action::Next,
        KeyCode::Char('p') | KeyCode::Char('<') | KeyCode::Up => Action::Prev,
        KeyCode::Char('s') => Action::Stop,
        // Terminals that report media keys (kitty protocol) — the desktop's
        // own multimedia keys arrive over MPRIS instead.
        KeyCode::Media(media) => match media {
            MediaKeyCode::Play | MediaKeyCode::Pause | MediaKeyCode::PlayPause => Action::PlayPause,
            MediaKeyCode::TrackNext | MediaKeyCode::FastForward => Action::Next,
            MediaKeyCode::TrackPrevious | MediaKeyCode::Rewind => Action::Prev,
            MediaKeyCode::Stop => Action::Stop,
            MediaKeyCode::LowerVolume => Action::VolChanged(player.volume_step_down()),
            MediaKeyCode::RaiseVolume => Action::VolChanged(player.volume_step_up()),
            MediaKeyCode::MuteVolume => Action::Muted(player.toggle_mute()),
            _ => Action::None,
        },
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
        KeyCode::Char('x') => Action::SmartShuffle,
        KeyCode::Char('z') => Action::SleepCycle,
        KeyCode::Char('f') => Action::TogglePath,
        KeyCode::Char('y') => Action::Lyrics,
        KeyCode::Char('J') => Action::MoveDown,
        KeyCode::Char('K') => Action::MoveUp,
        KeyCode::Char('a') => Action::PlayNext,
        KeyCode::Char('A') => Action::AppendEnd,
        KeyCode::Char('D') => Action::ClearQueue,
        KeyCode::Char('d') => Action::Dislike,
        KeyCode::Char('g') => Action::JumpToCurrent,
        KeyCode::Char('v') => Action::CavaToggle,
        KeyCode::Char('c') => Action::Settings,
        KeyCode::Char('u') => Action::Undo,
        KeyCode::Char('q') | KeyCode::Esc => Action::Quit,
        KeyCode::Char('h') | KeyCode::Char('?') => Action::Help,
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            Action::Jump(c.to_digit(10).unwrap_or(1) as usize)
        }
        _ => Action::None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeat_mode_cycles_off_all_one() {
        assert_eq!(LoopMode::Off.next(), LoopMode::Playlist);
        assert_eq!(LoopMode::Playlist.label(), "all");
        assert_eq!(LoopMode::Track.label(), "one");
        assert_eq!(LoopMode::Track.next(), LoopMode::Off);
        assert_eq!(LoopMode::Off.label(), "off");
    }

    #[test]
    fn advance_index_sequential_and_loop() {
        assert_eq!(advance_index(5, 1, LoopMode::Off, false, &[], 0), Some(2));
        assert_eq!(advance_index(5, 4, LoopMode::Off, false, &[], 0), None);
        assert_eq!(
            advance_index(5, 4, LoopMode::Playlist, false, &[], 0),
            Some(0)
        );
        assert_eq!(advance_index(0, 0, LoopMode::Off, false, &[], 0), None);
    }

    #[test]
    fn advance_index_smart_avoids_recent() {
        let recent: Vec<usize> = (1..=8).collect();
        for seed in 0..30 {
            let next = advance_index(12, 8, LoopMode::Off, true, &recent, seed).unwrap();
            assert!([0, 9, 10, 11].contains(&next));
        }
    }

    #[test]
    fn list_nav_keys_are_cursor_moves_only() {
        assert!(list_nav_key(KeyCode::Down));
        assert!(list_nav_key(KeyCode::Char('j')));
        assert!(!list_nav_key(KeyCode::Char('l')));
        assert!(!list_nav_key(KeyCode::Esc));
    }

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::empty())
    }

    fn shifted(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::SHIFT)
    }

    fn ctrl(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::CONTROL)
    }

    #[test]
    fn existing_transport_keys_unchanged() {
        let mut player = Player::new(80, 1.0, 0.0).expect("mpv");
        assert!(matches!(
            handle_key(key(KeyCode::Char('n')), &mut player),
            Action::Next
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('p')), &mut player),
            Action::Prev
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('l')), &mut player),
            Action::List
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('o')), &mut player),
            Action::LoopCycle
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('r')), &mut player),
            Action::Shuffle
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('j')), &mut player),
            Action::None
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('k')), &mut player),
            Action::None
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Down), &mut player),
            Action::Next
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Up), &mut player),
            Action::Prev
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('y')), &mut player),
            Action::Lyrics
        ));
    }

    #[test]
    fn queue_edit_keys() {
        let mut player = Player::new(80, 1.0, 0.0).expect("mpv");
        assert!(matches!(
            handle_key(key(KeyCode::Char('J')), &mut player),
            Action::MoveDown
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('K')), &mut player),
            Action::MoveUp
        ));
        assert!(matches!(
            handle_key(shifted(KeyCode::Up), &mut player),
            Action::MoveUp
        ));
        assert!(matches!(
            handle_key(shifted(KeyCode::Down), &mut player),
            Action::MoveDown
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('a')), &mut player),
            Action::PlayNext
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('A')), &mut player),
            Action::AppendEnd
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('D')), &mut player),
            Action::ClearQueue
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('g')), &mut player),
            Action::JumpToCurrent
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('u')), &mut player),
            Action::Undo
        ));
        assert!(matches!(
            handle_key(ctrl(KeyCode::Char('z')), &mut player),
            Action::Undo
        ));
        assert!(matches!(
            handle_key(key(KeyCode::Char('z')), &mut player),
            Action::SleepCycle
        ));
    }
}
