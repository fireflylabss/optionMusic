//! Discord Rich Presence over the local IPC socket (opt-in via `discord_rpc`).
//!
//! Purely local: talks to the Discord client over `$XDG_RUNTIME_DIR/discord-ipc-N`
//! — no network calls. Shows track title / artist-album / progress bar.

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use discord_rich_presence::{
    DiscordIpc, DiscordIpcClient,
    activity::{Activity, ActivityType, Assets, Timestamps},
};

/// Built-in Discord application id (the "optionMusic" app registered at
/// discord.dev/developers). `discord_rpc_id` in config.toml overrides it.
const BUILT_IN_CLIENT_ID: &str = "1548415534012432404";

/// Min delay between reconnect attempts while Discord is unreachable.
const RETRY_SECS: u64 = 15;
/// Refresh cadence while playing (also re-syncs the bar after seeks).
const REFRESH_SECS: u64 = 15;
/// Discord caps details/state at 128 bytes.
const FIELD_MAX: usize = 127;

/// Lazily-connected presence client. Every method no-ops while disabled, so
/// callers can call `sync` + `update` unconditionally each frame.
pub struct Rpc {
    client: Option<DiscordIpcClient>,
    client_id: String,
    enabled: bool,
    /// Last pushed activity key — dedupes per-frame `update` calls.
    last_key: String,
    last_attempt: Option<Instant>,
    warned_no_id: bool,
}

impl Default for Rpc {
    fn default() -> Self {
        Self::new()
    }
}

impl Rpc {
    pub fn new() -> Self {
        Self {
            client: None,
            client_id: String::new(),
            enabled: false,
            last_key: String::new(),
            last_attempt: None,
            warned_no_id: false,
        }
    }

    /// Follow the config toggle live. Returns a one-shot warning message when
    /// enabled but no client id is available (none registered / configured).
    pub fn sync(&mut self, enabled: bool, client_id: &str) -> Option<&'static str> {
        let id = if client_id.is_empty() {
            BUILT_IN_CLIENT_ID
        } else {
            client_id
        };
        if id != self.client_id {
            self.disconnect();
            self.client_id = id.to_string();
            self.warned_no_id = false;
        }
        if !enabled {
            self.disconnect();
            self.enabled = false;
            return None;
        }
        self.enabled = true;
        if self.client_id.is_empty() && !self.warned_no_id {
            self.warned_no_id = true;
            return Some("discord rpc · set discord_rpc_id in config.toml");
        }
        None
    }

    /// Push the current track state. Deduped internally — safe to call every
    /// frame; the Discord bar counts locally from the timestamps we send.
    /// `image` is a public art URL (e.g. YouTube thumbnail); the Discord
    /// client proxies external `https://` images itself. Falls back to the
    /// `logo` art asset registered on the app.
    pub fn update(
        &mut self,
        details: &str,
        state: &str,
        image: Option<&str>,
        position: Duration,
        duration: Option<Duration>,
        paused: bool,
    ) {
        if !self.enabled || self.client_id.is_empty() {
            return;
        }
        if self.client.is_none() {
            let cooling = self
                .last_attempt
                .map(|t| t.elapsed().as_secs() < RETRY_SECS)
                .unwrap_or(false);
            if cooling {
                return;
            }
            self.last_attempt = Some(Instant::now());
            let mut client = DiscordIpcClient::new(&self.client_id);
            match client.connect() {
                Ok(_) => {
                    self.client = Some(client);
                    self.last_key.clear();
                }
                Err(_) => return,
            }
        }

        // Position bucketed to REFRESH_SECS: track/pause changes push instantly,
        // seeks re-sync within one bucket, no per-second spam.
        let key = format!(
            "{details}|{state}|{paused}|{}|{}",
            position.as_secs() / REFRESH_SECS,
            image.unwrap_or("")
        );
        if key == self.last_key {
            return;
        }

        let details = clip(details);
        let state = clip(state);
        let assets = match image {
            Some(img) => Assets::new().large_image(img).large_text(&details),
            None => Assets::new().large_image("logo").large_text("optionMusic"),
        };
        let mut act = Activity::new().details(&details);
        if !state.is_empty() {
            act = act.state(&state);
        }
        act = act
            // Listening + start/end is what makes Discord draw the
            // Spotify-style time bar (Playing only shows "left" text).
            .activity_type(ActivityType::Listening)
            .assets(assets);
        if !paused {
            if let Some(dur) = duration {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let remaining = dur.as_secs().saturating_sub(position.as_secs()) as i64;
                act = act.timestamps(
                    Timestamps::new()
                        .start(now - position.as_secs() as i64)
                        .end(now + remaining),
                );
            }
        }

        let Some(client) = self.client.as_mut() else {
            return;
        };
        match client.set_activity(act) {
            Ok(_) => self.last_key = key,
            Err(_) => self.client = None,
        }
    }

    /// Clear the presence (keeps the socket; `sync` may re-enable later).
    pub fn clear(&mut self) {
        if let Some(client) = self.client.as_mut() {
            let _ = client.clear_activity();
        }
        self.last_key.clear();
    }

    fn disconnect(&mut self) {
        if let Some(mut client) = self.client.take() {
            let _ = client.clear_activity();
            let _ = client.close();
        }
        self.last_key.clear();
    }
}

impl Drop for Rpc {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn clip(s: &str) -> String {
    let s = s.trim();
    if s.chars().count() <= FIELD_MAX {
        return s.to_string();
    }
    s.chars().take(FIELD_MAX - 1).collect::<String>() + "…"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clip_bounds_field() {
        let long = "x".repeat(300);
        assert_eq!(clip(&long).chars().count(), FIELD_MAX);
        assert_eq!(clip(" ok "), "ok");
    }

    #[test]
    fn disabled_update_is_noop() {
        let mut rpc = Rpc::new();
        rpc.update("a", "b", None, Duration::ZERO, None, false);
        assert!(rpc.client.is_none());
    }
}
