//! `msc browse` — in-memory library browser TUI.
//!
//! Tree navigation: artists → albums → tracks, with genre/year filters,
//! unified search, favorites, and an in-memory queue the player runs through.
//! Nothing here touches the desktop; it reuses the shared [`crate::library::Library`].

use std::io::{self, Write};
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::style::{Color, Print, ResetColor, SetBackgroundColor, SetForegroundColor};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};
use crossterm::{execute, queue};

use crate::library::{AlbumGroup, ArtistGroup, Library};
use crate::player::Player;
use crate::ui::fmt_time;

// ── Palette (compact B&W, same family as the player) ─────────
const BG_FOCUS: Color = Color::Rgb {
    r: 215,
    g: 215,
    b: 215,
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
    r: 180,
    g: 180,
    b: 180,
};
const FG_MUTED: Color = Color::Rgb {
    r: 100,
    g: 100,
    b: 100,
};

enum Level {
    Artists,
    Albums,
    Tracks,
}

/// Running browse session: owns the player so `enter` can start playback and
/// the tree can keep reacting while music plays.
pub fn run(library: Library, favorites_only: bool, enable_cava: bool) -> Result<()> {
    let player = Player::new(80, 1.0, 0.0).context("cannot open player")?;

    let mut session = Session::new(library, favorites_only, player, enable_cava);
    session.enter()?;
    session.run()
}

struct Session {
    library: Library,
    player: Player,
    genre: Option<String>,
    year: Option<u32>,
    search: String,
    searching: bool,
    favorites_only: bool,
    level: Level,
    selected_artist: Option<String>,
    selected_album: Option<String>,
    cursor: usize,
    scroll: usize,
    queue: Vec<usize>,
    queue_cursor: Option<usize>,
    /// Position (index into `queue`) selected in the queue view.
    queue_sel: Option<usize>,
    queue_view: bool,
    msg: Option<String>,
    msg_since: Option<Instant>,
}

impl Session {
    fn new(library: Library, favorites_only: bool, player: Player, _cava: bool) -> Self {
        Self {
            library,
            player,
            genre: None,
            year: None,
            search: String::new(),
            searching: false,
            favorites_only,
            level: Level::Artists,
            selected_artist: None,
            selected_album: None,
            cursor: 0,
            scroll: 0,
            queue: Vec::new(),
            queue_cursor: None,
            queue_sel: None,
            queue_view: false,
            msg: None,
            msg_since: None,
        }
    }

    fn enter(&mut self) -> Result<()> {
        enable_raw_mode().context("enable raw mode")?;
        let mut out = io::stdout();
        execute!(out, EnterAlternateScreen, Hide).context("enter alt screen")?;
        Ok(())
    }

    fn set_msg(&mut self, msg: impl Into<String>) {
        self.msg = Some(msg.into());
        self.msg_since = Some(Instant::now());
    }

    fn leave(&mut self) -> Result<()> {
        let mut out = io::stdout();
        execute!(out, Show, LeaveAlternateScreen, ResetColor).ok();
        disable_raw_mode().ok();
        Ok(())
    }

    // ── Data helpers ──────────────────────────────────────────

    fn base(&self) -> Vec<usize> {
        self.library.matching(
            self.genre.as_deref(),
            self.year,
            if self.search.is_empty() {
                None
            } else {
                Some(&self.search)
            },
            self.favorites_only,
        )
    }

    fn rows(&self) -> Vec<Row> {
        let base = self.base();
        match self.level {
            Level::Artists => self
                .library
                .artists(&base)
                .into_iter()
                .map(|a| Row::Artist(a))
                .collect(),
            Level::Albums => {
                let artist = self.selected_artist.clone().unwrap_or_default();
                self.library
                    .albums_of_artist(&base, &artist)
                    .into_iter()
                    .map(|a| Row::Album(a))
                    .collect()
            }
            Level::Tracks => {
                let artist = self.selected_artist.clone().unwrap_or_default();
                let album = self.selected_album.clone().unwrap_or_default();
                self.library
                    .tracks_of_album(&base, &artist, &album)
                    .into_iter()
                    .map(|i| Row::Track(i))
                    .collect()
            }
        }
    }

    fn row_tracks(&self, row: &Row) -> Vec<usize> {
        let base = self.base();
        match row {
            Row::Artist(a) => {
                let mut idx = self.library.albums_of_artist(&base, &a.name);
                let mut out = Vec::new();
                for album in idx.drain(..) {
                    out.extend(self.library.tracks_of_album(&base, &a.name, &album.name));
                }
                out
            }
            Row::Album(a) => {
                let artist = self.selected_artist.clone().unwrap_or_default();
                self.library.tracks_of_album(&base, &artist, &a.name)
            }
            Row::Track(i) => vec![*i],
        }
    }

    fn current_rows(&self) -> Vec<usize> {
        match self.level {
            Level::Tracks => {
                let artist = self.selected_artist.clone().unwrap_or_default();
                let album = self.selected_album.clone().unwrap_or_default();
                self.library
                    .tracks_of_album(&self.base(), &artist, &album)
            }
            _ => Vec::new(),
        }
    }

    fn selection_tracks(&self) -> Vec<usize> {
        let rows = self.rows();
        let Some(row) = rows.get(self.cursor) else {
            return Vec::new();
        };
        self.row_tracks(row)
    }

    // ── Playback ──────────────────────────────────────────────

    fn play_index(&mut self, i: usize) {
        if let Some(track) = self.library.get(i) {
            self.queue_cursor = Some(i);
            let _ = self.player.play_file(&track.path);
            self.set_msg(track.display_name());
        }
    }

    fn start_playing(&mut self, tracks: Vec<usize>, from: Option<usize>) {
        if tracks.is_empty() {
            return;
        }
        self.queue = tracks;
        let start = from.unwrap_or(0).min(self.queue.len().saturating_sub(1));
        if let Some(&i) = self.queue.get(start) {
            self.play_index(i);
        }
    }

    fn queue_add(&mut self, tracks: Vec<usize>) {
        if tracks.is_empty() {
            return;
        }
        let before = self.queue.len();
        let mut added = 0;
        for i in tracks {
            if !self.queue.contains(&i) {
                self.queue.push(i);
                added += 1;
            }
        }
        self.set_msg(format!(
            "+{added} queued ({} → {})",
            before,
            self.queue.len()
        ));
    }

    fn play_next(&mut self, tracks: Vec<usize>) {
        if tracks.is_empty() {
            return;
        }
        let mut added = 0;
        let pos = match self.queue_cursor {
            Some(c) => self
                .queue
                .iter()
                .position(|&i| i == c)
                .map(|p| p + 1)
                .unwrap_or(self.queue.len()),
            None => self.queue.len(),
        };
        for i in tracks {
            if !self.queue.contains(&i) {
                self.queue.insert(pos + added, i);
                added += 1;
            }
        }
        self.set_msg(format!("{added} queued next"));
    }

    fn move_queue(&mut self, delta: isize) {
        if self.queue.is_empty() {
            return;
        }
        // move selection cursor; reorder the selected track as it moves
        let sel = self.queue_sel.unwrap_or(0).min(self.queue.len() - 1);
        let target = sel as isize + delta;
        if target < 0 || target >= self.queue.len() as isize {
            return;
        }
        self.queue.swap(sel, target as usize);
        self.queue_sel = Some(target as usize);
    }

    fn queue_view_cursor(&mut self) {
        // on opening the queue view, select the currently playing position
        let pos = match self.queue_cursor {
            Some(c) => self.queue.iter().position(|&i| i == c).unwrap_or(0),
            None => 0,
        };
        if !self.queue.is_empty() {
            self.queue_sel = Some(pos.min(self.queue.len() - 1));
        } else {
            self.queue_sel = None;
        }
    }

    fn next_track(&mut self) {
        let Some(cur) = self.queue_cursor else {
            return;
        };
        let Some(pos) = self.queue.iter().position(|&i| i == cur) else {
            return;
        };
        if let Some(&next) = self.queue.get(pos + 1) {
            self.play_index(next);
        } else {
            self.set_msg("at last track");
        }
    }

    fn prev_track(&mut self) {
        if !self.player.is_idle() && self.player.position() > Duration::from_secs(3) {
            let _ = self.player.seek(Duration::ZERO);
            return;
        }
        let Some(cur) = self.queue_cursor else {
            return;
        };
        let Some(pos) = self.queue.iter().position(|&i| i == cur) else {
            return;
        };
        if pos > 0 {
            if let Some(&prev) = self.queue.get(pos - 1) {
                self.play_index(prev);
            }
        } else {
            let _ = self.player.seek(Duration::ZERO);
        }
    }

    fn auto_advance(&mut self) {
        if self.player.is_idle() {
            return;
        }
        if let Some(cur) = self.queue_cursor {
            if let Some(pos) = self.queue.iter().position(|&i| i == cur) {
                if let Some(&next) = self.queue.get(pos + 1) {
                    self.play_index(next);
                }
            }
        }
    }

    // ── Navigation ────────────────────────────────────────────

    fn move_cursor(&mut self, delta: isize) {
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let n = rows.len();
        self.cursor = (self.cursor as isize + delta).rem_euclid(n as isize) as usize;
        self.clamp_scroll();
    }

    fn clamp_scroll(&mut self) {
        let visible = self.visible_rows();
        if self.cursor < self.scroll {
            self.scroll = self.cursor;
        }
        if visible > 0 && self.cursor >= self.scroll + visible {
            self.scroll = self.cursor + 1 - visible;
        }
    }

    fn visible_rows(&self) -> usize {
        crossterm::terminal::size()
            .map(|(_, rows)| {
                let rows = rows as usize;
                if rows < 4 {
                    return 10;
                }
                rows - 4
            })
            .unwrap_or(10)
    }

    fn enter_selection(&mut self) {
        let rows = self.rows();
        let Some(row) = rows.get(self.cursor) else {
            return;
        };
        match self.level {
            Level::Artists => {
                if let Row::Artist(a) = row {
                    self.selected_artist = Some(a.name.clone());
                    self.selected_album = None;
                    self.level = Level::Albums;
                    self.cursor = 0;
                    self.scroll = 0;
                }
            }
            Level::Albums => {
                if let Row::Album(a) = row {
                    self.selected_album = Some(a.name.clone());
                    self.level = Level::Tracks;
                    self.cursor = 0;
                    self.scroll = 0;
                }
            }
            Level::Tracks => {
                // play now: the current track list becomes the session queue
                self.start_playing(self.current_rows(), Some(self.cursor));
            }
        }
    }

    fn back(&mut self) {
        match self.level {
            Level::Artists => {
                self.searching = false;
                self.search.clear();
            }
            Level::Albums => {
                self.level = Level::Artists;
                self.selected_artist = None;
                self.cursor = 0;
                self.scroll = 0;
            }
            Level::Tracks => {
                self.level = Level::Albums;
                self.selected_album = None;
                self.cursor = 0;
                self.scroll = 0;
            }
        }
    }

    // ── Event loop ────────────────────────────────────────────

    fn run(&mut self) -> Result<()> {
        let mut quit = false;
        loop {
            self.paint()?;
            self.auto_advance();

            if event::poll(Duration::from_millis(40)).unwrap_or(false) {
                loop {
                    match event::read() {
                        Ok(Event::Key(key)) => {
                            if key.kind == KeyEventKind::Press {
                                if self.searching {
                                    match key.code {
                                        KeyCode::Esc => {
                                            self.searching = false;
                                            self.search.clear();
                                        }
                                        KeyCode::Enter => self.searching = false,
                                        KeyCode::Backspace => {
                                            self.search.pop();
                                        }
                                        KeyCode::Char(c) if !c.is_control() => {
                                            self.search.push(c);
                                        }
                                        _ => {}
                                    }
                                } else if self.queue_view {
                                    match key.code {
                                        KeyCode::Esc => self.queue_view = false,
                                        KeyCode::Tab => self.queue_view = false,
                                        KeyCode::Up | KeyCode::Char('k') => {
                                            self.move_queue(-1);
                                        }
                                        KeyCode::Down | KeyCode::Char('j') => {
                                            self.move_queue(1);
                                        }
                                        KeyCode::Enter => {
                                            if let Some(pos) = self.queue_sel {
                                                if let Some(&i) = self.queue.get(pos) {
                                                    self.play_index(i);
                                                }
                                            }
                                        }
                                        KeyCode::Char('d') => {
                                            let Some(pos) = self.queue_sel else {
                                                break;
                                            };
                                            if pos < self.queue.len() {
                                                let removed = self.queue.remove(pos);
                                                if self.queue_cursor == Some(removed) {
                                                    self.queue_cursor = None;
                                                }
                                                if self.queue.is_empty() {
                                                    self.queue_sel = None;
                                                } else {
                                                    self.queue_sel =
                                                        Some(pos.min(self.queue.len() - 1));
                                                }
                                            }
                                        }
                                        KeyCode::Char('q') | KeyCode::Char('Q') => {
                                            self.queue_view = false;
                                        }
                                        _ => {}
                                    }
                                } else {
                                    match key.code {
                                        KeyCode::Char('q') => {
                                            self.player.stop();
                                            quit = true;
                                        }
                                        KeyCode::Esc => self.back(),
                                        KeyCode::Up | KeyCode::Char('k') => self.move_cursor(-1),
                                        KeyCode::Down | KeyCode::Char('j') => self.move_cursor(1),
                                        KeyCode::PageUp => self.move_cursor(-8),
                                        KeyCode::PageDown => self.move_cursor(8),
                                        KeyCode::Enter => self.enter_selection(),
                                        KeyCode::Char('a') => {
                                            let tracks = self.selection_tracks();
                                            self.queue_add(tracks);
                                        }
                                        KeyCode::Char('n') => {
                                            let tracks = self.selection_tracks();
                                            self.play_next(tracks);
                                        }
                                        KeyCode::Char('N') | KeyCode::Char('>') => {
                                            self.next_track();
                                        }
                                        KeyCode::Char('P') | KeyCode::Char('<') => {
                                            self.prev_track();
                                        }
                                        KeyCode::Char('f') => self.toggle_favorite_selection(),
                                        KeyCode::Char('g') => self.prompt_genre()?,
                                        KeyCode::Char('y') => self.prompt_year()?,
                                        KeyCode::Char('x') => {
                                            self.genre = None;
                                            self.year = None;
                                        }
                                        KeyCode::Char('1') => {
                                            self.favorites_only = !self.favorites_only;
                                            self.cursor = 0;
                                            self.scroll = 0;
                                        }
                                        KeyCode::Char('/') => {
                                            self.searching = true;
                                            self.search.clear();
                                        }
                                        KeyCode::Tab => {
                                            self.queue_view = true;
                                            self.queue_view_cursor();
                                        }
                                        _ => {}
                                    }
                                }
                            }
                        }
                        Ok(_) => {}
                        Err(_) => break,
                    }
                    if quit {
                        break;
                    }
                    if !event::poll(Duration::ZERO).unwrap_or(false) {
                        break;
                    }
                }
            }
            if quit {
                break;
            }
        }
        self.leave()?;
        Ok(())
    }

    fn toggle_favorite_selection(&mut self) {
        if let Level::Tracks = self.level {
            if let Some(track) = self.library.get(self.cursor) {
                let path = track.path.clone();
                match self.library.toggle_favorite(&path) {
                    Ok(on) => self.set_msg(format!("★ favorite {}", if on { "on" } else { "off" })),
                    Err(_) => self.set_msg("could not save favorite"),
                }
            }
        } else {
            self.set_msg("favorite: enter a track first");
        }
    }

    fn prompt_genre(&mut self) -> Result<()> {
        let genres = self.library.genres();
        if genres.is_empty() {
            self.set_msg("no genres found");
            return Ok(());
        }
        let chosen = self.choose(
            "filter by genre",
            &["favorites only: 1 · clear: x"],
            &genres,
            self.genre
                .as_ref()
                .and_then(|g| genres.iter().position(|x| x == g)),
        )?;
        self.genre = chosen;
        self.cursor = 0;
        self.scroll = 0;
        Ok(())
    }

    fn prompt_year(&mut self) -> Result<()> {
        let years = self
            .library
            .years()
            .into_iter()
            .map(|y| y.to_string())
            .collect::<Vec<_>>();
        if years.is_empty() {
            self.set_msg("no year tags found");
            return Ok(());
        }
        let chosen = self.choose(
            "filter by year",
            &["favorites only: 1 · clear: x"],
            &years,
            self.year
                .map(|y| years.iter().position(|s| s == &y.to_string()))
                .flatten(),
        )?;
        self.year = chosen.and_then(|s| s.parse().ok());
        self.cursor = 0;
        self.scroll = 0;
        Ok(())
    }

    /// Small reusable choice screen (genre / year pickers).
    fn choose(&mut self, title: &str, blurb: &[&str], items: &[String], def: Option<usize>) -> Result<Option<String>> {
        let mut cursor = def.unwrap_or(0).min(items.len().saturating_sub(1));
        loop {
            let mut out = io::stdout();
            queue!(out, Clear(ClearType::All), MoveTo(0, 0)).ok();
            queue!(out, SetForegroundColor(FG_TITLE), Print(format!("♪  {title}")), ResetColor).ok();
            queue!(out, MoveTo(0, 1)).ok();
            queue!(out, SetForegroundColor(FG_MUTED), Print(format!("  {}", blurb.join(" · "))), ResetColor).ok();

            let visible = (crossterm::terminal::size().map(|(_, r)| r as usize).unwrap_or(20))
                .saturating_sub(3);
            let scroll = if cursor >= visible { cursor + 1 - visible } else { 0 };
            for (i, item) in items.iter().enumerate().skip(scroll).take(visible) {
                queue!(out, MoveTo(0, 2 + (i - scroll) as u16)).ok();
                if i == cursor {
                    let line = format!("  ▸ {}", item);
                    queue!(out, SetBackgroundColor(BG_FOCUS), SetForegroundColor(FG_ON_FOCUS), Print(line), ResetColor).ok();
                } else {
                    queue!(out, SetForegroundColor(FG_BODY), Print(format!("    {item}")), ResetColor).ok();
                }
            }
            queue!(out, MoveTo(0, 2 + visible as u16)).ok();
            queue!(out, SetForegroundColor(FG_MUTED), Print("  ↑↓ move · enter select · esc cancel"), ResetColor).ok();
            out.flush().ok();

            match read_key()? {
                KeyCode::Esc | KeyCode::Char('q') => return Ok(None),
                KeyCode::Up | KeyCode::Char('k') => {
                    cursor = cursor.checked_sub(1).unwrap_or(items.len() - 1);
                }
                KeyCode::Down | KeyCode::Char('j') => cursor = (cursor + 1) % items.len(),
                KeyCode::Enter => {
                    return Ok(items.get(cursor).cloned());
                }
                _ => {}
            }
        }
    }

    // ── Painting ──────────────────────────────────────────────

    fn queue_line(&self, idx: usize, pos: usize) -> String {
        let t = self.library.get(idx);
        let Some(t) = t else { return String::new() };
        let marker = if self.queue_cursor == Some(idx) { "▶" } else { " " };
        format!("  {marker} {:>3}. {}", pos + 1, t.display_name())
    }
    fn paint(&mut self) -> Result<()> {
        let rows = self.rows();
        if !rows.is_empty() && self.cursor >= rows.len() {
            self.cursor = rows.len() - 1;
        }
        self.clamp_scroll();

        let mut out = io::stdout();
        queue!(out, Clear(ClearType::All)).ok();
        let width = crossterm::terminal::size()
            .map(|(w, _)| {
                let w = w as usize;
                if w < 8 {
                    80
                } else {
                    w
                }
            })
            .unwrap_or(80);

        // header
        queue!(
            out,
            MoveTo(0, 0),
            SetForegroundColor(FG_TITLE),
            Print(format!("♪  browse")),
            SetForegroundColor(FG_MUTED),
            Print(format!(
                "  ·  {} · {} track{}",
                self.library.root().display(),
                self.library.len(),
                if self.library.len() == 1 { "" } else { "s" }
            )),
            ResetColor
        )
        .ok();

        // filter line
        let mut filters = Vec::new();
        if let Some(g) = &self.genre {
            filters.push(format!("genre:{g}"));
        }
        if let Some(y) = self.year {
            filters.push(format!("year:{y}"));
        }
        if self.favorites_only {
            filters.push("♥ favs".to_string());
        }
        if !self.search.is_empty() {
            filters.push(format!("q:\"{}\"", self.search));
        }
        let filter_line = if filters.is_empty() {
            "no filters".to_string()
        } else {
            filters.join("  ")
        };
        queue!(
            out,
            MoveTo(0, 1),
            SetForegroundColor(FG_MUTED),
            Print(format!("  {}", filter_line)),
            ResetColor
        )
        .ok();

        // transient message (queue/favorite/status toasts)
        let show_msg = self.msg.clone();
        if let Some(m) = show_msg {
            let expired = self
                .msg_since
                .map(|t| t.elapsed() > Duration::from_secs(2))
                .unwrap_or(true);
            if expired {
                self.msg = None;
                self.msg_since = None;
            } else {
                queue!(
                    out,
                    MoveTo(0, 2),
                    SetForegroundColor(FG_TITLE),
                    Print(format!("  {}", m)),
                    ResetColor
                )
                .ok();
            }
        }

        // search line
        let mut top = 2usize;
        if self.searching {
            queue!(
                out,
                MoveTo(0, 2),
                SetForegroundColor(FG_TITLE),
                Print(format!("  search › {}", self.search)),
                Print("█"),
                ResetColor
            )
            .ok();
            top = 3;
        } else if self.msg.is_some() {
            top = 3;
        }

        // body rows
        let visible = self.visible_rows();
        if self.queue_view {
            let n = self.queue.len();
            if n == 0 {
                queue!(
                    out,
                    MoveTo(0, top as u16),
                    SetForegroundColor(FG_MUTED),
                    Print("  queue empty — press `a` in the tree to add tracks"),
                    ResetColor
                )
                .ok();
            } else {
                let qscroll = self.scroll.min(n.saturating_sub(1));
                for pos in qscroll..(qscroll + visible).min(n) {
                    let line = self.queue_line(self.queue[pos], pos);
                    queue!(out, MoveTo(0, (top + pos - qscroll) as u16)).ok();
                    let selected = self.queue_sel == Some(pos);
                    if selected {
                        let padded = pad(&line, width);
                        queue!(
                            out,
                            SetBackgroundColor(BG_FOCUS),
                            SetForegroundColor(FG_ON_FOCUS),
                            Print(padded),
                            ResetColor
                        )
                        .ok();
                    } else {
                        queue!(out, SetForegroundColor(FG_BODY), Print(line), ResetColor).ok();
                    }
                }
            }
        } else if rows.is_empty() {
            queue!(
                out,
                MoveTo(0, top as u16),
                SetForegroundColor(FG_MUTED),
                Print("  nothing here"),
                ResetColor
            )
            .ok();
        } else {
            for (i, row) in rows.iter().enumerate().skip(self.scroll).take(visible) {
                let line = self.row_line(row, i);
                queue!(out, MoveTo(0, (top + i - self.scroll) as u16)).ok();
                if i == self.cursor {
                    let padded = pad(&line, width);
                    queue!(
                        out,
                        SetBackgroundColor(BG_FOCUS),
                        SetForegroundColor(FG_ON_FOCUS),
                        Print(padded),
                        ResetColor
                    )
                    .ok();
                } else {
                    queue!(out, SetForegroundColor(FG_BODY), Print(line), ResetColor).ok();
                }
            }
        }

        // footer
        let foot_top = (top + visible) as u16;
        let now = self
            .queue_cursor
            .and_then(|i| self.library.get(i))
            .map(|t| t.display_name())
            .unwrap_or_else(|| "—".into());
        queue!(
            out,
            MoveTo(0, foot_top),
            SetForegroundColor(FG_MUTED),
            Print(format!("  ▶ {}", truncate(&now, width.saturating_sub(2)))),
            ResetColor
        )
        .ok();
        let hints = if self.queue_view {
            "queue · j/k reorder · enter play · d remove · esc back"
        } else {
            "enter play · a queue · n next · f fav · g genre · y year · / search · 1 favs · tab queue · esc back · q quit"
        };
        queue!(
            out,
            MoveTo(0, foot_top + 1),
            SetForegroundColor(FG_MUTED),
            Print(format!("  {}", truncate(hints, width.saturating_sub(2)))),
            ResetColor
        )
        .ok();

        out.flush().context("flush browse")?;
        Ok(())
    }

    fn row_line(&self, row: &Row, _i: usize) -> String {
        match row {
            Row::Artist(a) => format!(
                "  {}",
                a.name,
            ),
            Row::Album(a) => {
                let year = a
                    .year
                    .map(|y| format!(" · {y}"))
                    .unwrap_or_default();
                format!("  {}{}", a.name, year)
            }
            Row::Track(i) => {
                let t = self.library.get(*i);
                let Some(t) = t else { return String::new() };
                let num = t
                    .track_number
                    .map(|n| format!("{:>2}. ", n))
                    .unwrap_or_default();
                let dur = crate::meta::duration_secs(&t.path)
                    .map(|d| fmt_time(Duration::from_secs_f64(d)))
                    .unwrap_or_else(|| "–:––".into());
                let mut badges = String::new();
                if t.artist.is_none() || t.album.is_none() {
                    badges.push_str(" ⚠");
                }
                if t.has_cover == Some(false) {
                    badges.push_str(" ▢");
                }
                let fav = if self.library.is_favorite(&t.path) { " ★" } else { "" };
                format!("  {num}{}{}  {dur}{badges}", t.display_name(), fav)
            }
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let mut out = io::stdout();
        let _ = execute!(out, Show, LeaveAlternateScreen, ResetColor);
        let _ = disable_raw_mode();
    }
}

enum Row {
    Artist(ArtistGroup),
    Album(AlbumGroup),
    Track(usize),
}

fn read_key() -> Result<KeyCode> {
    loop {
        let ev = event::read().context("read key")?;
        if let Event::Key(key) = ev {
            if key.kind == KeyEventKind::Press {
                return Ok(key.code);
            }
        }
    }
}

fn pad(s: &str, width: usize) -> String {
    let len = s.chars().count();
    if len >= width {
        truncate(s, width)
    } else {
        format!("{}{}", s, " ".repeat(width - len))
    }
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
