//! Crash capture for the desktop shell.
//!
//! `install` registers a panic hook early in `main` (before GPUI starts).
//! On panic it appends a timestamped log under the cache dir and drops a
//! `.crashed` sentinel in the config dir; the next launch's
//! `take_crash_flag` clears the sentinel so the app can toast a
//! "recovered after a crash" notice. Everything here is best-effort — a
//! crash handler must never panic itself.

use std::backtrace::Backtrace;
use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// `~/.option/music/.crashed`
fn sentinel_path() -> PathBuf {
    optionmusic::config::config_dir().join(".crashed")
}

/// `~/.option/music/cache/crash`
fn log_dir() -> PathBuf {
    optionmusic::config::cache_dir().join("crash")
}

/// Install the panic hook. Runs the previous hook first so the default
/// stderr report still prints, then persists what it can.
pub(crate) fn install() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        previous(info);
        write_crash_log(info);
    }));
}

fn write_crash_log(info: &std::panic::PanicHookInfo<'_>) {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let message = info
        .payload()
        .downcast_ref::<&str>()
        .map(|s| s.to_string())
        .or_else(|| info.payload().downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string payload>".into());
    let location = info
        .location()
        .map(|l| format!("{}:{}:{}", l.file(), l.line(), l.column()))
        .unwrap_or_else(|| "<unknown>".into());
    let thread = std::thread::current();
    let body = format!(
        "optionmusic-gpui crash\nversion: {}\ntime: {} ({})\nthread: {}\nlocation: {}\n\n{}\n\nbacktrace:\n{}\n",
        env!("CARGO_PKG_VERSION"),
        secs,
        utc_stamp(secs),
        thread.name().unwrap_or("<unnamed>"),
        location,
        message,
        Backtrace::capture(),
    );
    let dir = log_dir();
    if fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = fs::write(dir.join(format!("crash-{secs}.log")), body);
    // The sentinel is just a marker — contents are informational.
    let _ = fs::write(sentinel_path(), format!("{secs}\n"));
}

/// Clear the `.crashed` sentinel. Returns the newest crash log when a
/// previous run left one behind, `None` on a clean launch.
pub(crate) fn take_crash_flag() -> Option<PathBuf> {
    let sentinel = sentinel_path();
    if !sentinel.exists() {
        return None;
    }
    let _ = fs::remove_file(&sentinel);
    newest_log()
}

fn newest_log() -> Option<PathBuf> {
    let mut newest: Option<(u64, PathBuf)> = None;
    for entry in fs::read_dir(log_dir()).ok()?.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(stamp) = name
            .strip_prefix("crash-")
            .and_then(|s| s.strip_suffix(".log"))
            .and_then(|s| s.parse::<u64>().ok())
        else {
            continue;
        };
        if newest.as_ref().is_none_or(|(best, _)| stamp >= *best) {
            newest = Some((stamp, entry.path()));
        }
    }
    newest.map(|(_, path)| path)
}

/// Epoch seconds → `YYYY-MM-DD HH:MM:SS` UTC (no datetime dependency).
fn utc_stamp(secs: u64) -> String {
    // Howard Hinnant's civil-from-days algorithm.
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    let z = days + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let doe = (z - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!("{y:04}-{mo:02}-{d:02} {h:02}:{m:02}:{s:02}Z")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_stamp_formats_known_epochs() {
        assert_eq!(utc_stamp(0), "1970-01-01 00:00:00Z");
        assert_eq!(utc_stamp(1_700_000_000), "2023-11-14 22:13:20Z");
        assert_eq!(utc_stamp(1_789_000_000), "2026-09-05 07:06:40Z");
    }
}
