//! Optional [cava](https://github.com/karlstav/cava) bridge — raw ASCII spectrum for the UI.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

const ASCII_MAX: u32 = 100;
const DEFAULT_BARS: usize = 64;

/// `(input method, source)` pairs tried in order until cava stays alive.
/// macOS: `coreaudio` `source = tap` captures the system output mix on
/// macOS 14.2+ without a loopback device; `auto` targets the default output
/// device; `portaudio`/`osx` cover builds without tap support. Linux keeps
/// the original pipewire → pulse order.
#[cfg(target_os = "macos")]
const INPUTS: &[(&str, &str)] = &[
    ("coreaudio", "tap"),
    ("coreaudio", "auto"),
    ("portaudio", "auto"),
    ("osx", "auto"),
];
#[cfg(not(target_os = "macos"))]
const INPUTS: &[(&str, &str)] = &[("pipewire", "auto"), ("pulse", "auto")];

/// Live spectrum levels in `0.0 ..= 1.0`, refreshed by a cava child process.
pub struct CavaBridge {
    child: Option<Child>,
    levels: Arc<Mutex<Vec<f32>>>,
    config_path: PathBuf,
    bars: usize,
    /// Isolated mode: feeder that pipes only this instance's stream into cava.
    feeder: Option<Feeder>,
}

/// Pipes this process's own PipeWire output stream (found by its
/// `audio-client-name`) into a fifo that cava reads, so other apps never
/// show up in the spectrum.
struct Feeder {
    stop: Arc<AtomicBool>,
    parec: Arc<Mutex<Option<Child>>>,
    fifo: PathBuf,
}

impl Feeder {
    fn shutdown(&self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Ok(mut g) = self.parec.lock()
            && let Some(mut c) = g.take()
        {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl CavaBridge {
    /// Spawn cava if the binary exists. Returns `None` when unavailable.
    /// On Linux the spectrum follows only this instance's audio when
    /// `parec` + `pactl` exist; otherwise it falls back to the system mix.
    pub fn try_start() -> Option<Self> {
        if !cava_on_path() {
            return None;
        }
        #[cfg(target_os = "linux")]
        if crate::which::on_path("pw-record")
            && crate::which::on_path("pw-link")
            && let Ok(b) = Self::start_isolated()
        {
            return Some(b);
        }
        INPUTS
            .iter()
            .find_map(|&(method, source)| Self::start_with_input(method, source).ok())
    }

    fn start_with_input(input_method: &str, source: &str) -> Result<Self> {
        let bars = DEFAULT_BARS;
        let config_path = write_cava_config(bars, input_method, source)?;
        let levels = Arc::new(Mutex::new(vec![0.0; bars]));

        let mut child = Command::new("cava")
            .arg("-p")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn cava ({input_method})"))?;

        let stdout = child.stdout.take().context("cava stdout missing")?;

        // Bail early if cava dies immediately (bad input method / missing server).
        thread::sleep(Duration::from_millis(80));
        if let Ok(Some(status)) = child.try_wait() {
            anyhow::bail!("cava exited early ({input_method}): {status}");
        }

        let shared = Arc::clone(&levels);
        thread::Builder::new()
            .name("stillmusic-cava".into())
            .spawn(move || read_loop(stdout, shared, bars))
            .context("cava reader thread")?;

        Ok(Self {
            child: Some(child),
            levels,
            config_path,
            bars,
            feeder: None,
        })
    }

    #[cfg(target_os = "linux")]
    fn start_isolated() -> Result<Self> {
        let bars = DEFAULT_BARS;
        let dir = std::env::temp_dir().join("stillmusic");
        std::fs::create_dir_all(&dir).context("temp cava dir")?;
        let pid = std::process::id();
        let fifo = dir.join(format!("cava-{pid}.pcm"));
        let _ = std::fs::remove_file(&fifo);
        let c_path =
            std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).context("fifo path")?;
        // SAFETY: valid NUL-terminated path, mode is a plain integer.
        if unsafe { libc::mkfifo(c_path.as_ptr(), 0o600) } != 0 {
            anyhow::bail!("mkfifo failed: {}", std::io::Error::last_os_error());
        }
        // O_RDWR keeps the fifo open on our side so cava never sees EOF
        // between tracks and opening never blocks.
        let sink = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&fifo)
            .context("open fifo")?;

        let config_path = write_cava_config_fifo(bars, &fifo)?;
        let levels = Arc::new(Mutex::new(vec![0.0; bars]));
        let mut child = Command::new("cava")
            .arg("-p")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .context("failed to spawn cava (fifo)")?;
        let stdout = child.stdout.take().context("cava stdout missing")?;
        thread::sleep(Duration::from_millis(80));
        if let Ok(Some(status)) = child.try_wait() {
            let _ = std::fs::remove_file(&fifo);
            anyhow::bail!("cava exited early (fifo): {status}");
        }

        let shared = Arc::clone(&levels);
        thread::Builder::new()
            .name("stillmusic-cava".into())
            .spawn(move || read_loop(stdout, shared, bars))
            .context("cava reader thread")?;

        let stop = Arc::new(AtomicBool::new(false));
        let parec: Arc<Mutex<Option<Child>>> = Arc::new(Mutex::new(None));
        let (s2, p2) = (Arc::clone(&stop), Arc::clone(&parec));
        thread::Builder::new()
            .name("stillmusic-cava-feed".into())
            .spawn(move || feed_loop(sink, s2, p2))
            .context("cava feeder thread")?;

        Ok(Self {
            child: Some(child),
            levels,
            config_path,
            bars,
            feeder: Some(Feeder { stop, parec, fifo }),
        })
    }

    pub fn snapshot(&self) -> Vec<f32> {
        self.levels
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| vec![0.0; self.bars])
    }

    pub fn stop(&mut self) {
        if let Some(f) = self.feeder.take() {
            f.shutdown();
            let _ = std::fs::remove_file(&f.fifo);
        }
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        let _ = std::fs::remove_file(&self.config_path);
    }
}

impl Drop for CavaBridge {
    fn drop(&mut self) {
        self.stop();
    }
}

fn cava_on_path() -> bool {
    crate::which::on_path("cava")
}

fn write_cava_config(bars: usize, input_method: &str, source: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("stillmusic");
    std::fs::create_dir_all(&dir).context("temp cava dir")?;
    let path = dir.join(format!("cava-{}.cfg", std::process::id()));

    let cfg = format!(
        r#"## still — generated cava config (do not edit)
[general]
framerate = 60
bars = {bars}
autosens = 1
sensitivity = 85
lower_cutoff_freq = 50
higher_cutoff_freq = 12000

[input]
method = {input_method}
source = {source}

[output]
method = raw
raw_target = /dev/stdout
data_format = ascii
ascii_max_range = {ASCII_MAX}
bar_delimiter = 59
frame_delimiter = 10
channels = mono

[smoothing]
monstercat = 0
noise_reduction = 70
integral = 65
gravity = 90
ignore = 0
"#
    );
    std::fs::write(&path, cfg).context("write cava config")?;
    Ok(path)
}

/// True while this process's own PipeWire output node exists (its name is
/// the `audio-client-name` set on mpv).
#[cfg(target_os = "linux")]
fn own_node_alive() -> bool {
    let want = format!("{}:", crate::mpv::client_name());
    Command::new("pw-link")
        .arg("-o")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
        .map(|o| {
            String::from_utf8_lossy(&o.stdout)
                .lines()
                .any(|l| l.trim().starts_with(&want))
        })
        .unwrap_or(false)
}

/// Keep a `pw-record --target <our node>` running and copy its PCM into the
/// fifo. A capture stream linked straight to our playback stream hears only
/// this instance. A watcher kills the recorder when the node goes away
/// (track change, device switch) so it re-attaches to the new one.
#[cfg(target_os = "linux")]
fn feed_loop(mut sink: std::fs::File, stop: Arc<AtomicBool>, slot: Arc<Mutex<Option<Child>>>) {
    use std::io::{Read, Write};

    let (wstop, wslot) = (Arc::clone(&stop), Arc::clone(&slot));
    let _ = thread::Builder::new()
        .name("stillmusic-cava-watch".into())
        .spawn(move || {
            while !wstop.load(Ordering::Relaxed) {
                thread::sleep(Duration::from_millis(700));
                let has_child = wslot.lock().map(|g| g.is_some()).unwrap_or(false);
                if has_child
                    && !own_node_alive()
                    && let Ok(mut g) = wslot.lock()
                    && let Some(mut c) = g.take()
                {
                    let _ = c.kill();
                    let _ = c.wait();
                }
            }
        });

    let mut buf = [0u8; 4096];
    while !stop.load(Ordering::Relaxed) {
        if !own_node_alive() {
            thread::sleep(Duration::from_millis(300));
            continue;
        }
        let spawned = Command::new("pw-record")
            .args(["--target", &crate::mpv::client_name()])
            .args(["-P", "{ node.dont-fallback = true }"])
            .args(["--format", "s16", "--rate", "44100", "--channels", "1"])
            .args(["--latency", "20ms", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn();
        let Ok(mut child) = spawned else {
            thread::sleep(Duration::from_millis(500));
            continue;
        };
        let Some(mut out) = child.stdout.take() else {
            let _ = child.kill();
            continue;
        };
        if let Ok(mut g) = slot.lock() {
            *g = Some(child);
        }
        while !stop.load(Ordering::Relaxed) {
            match out.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if sink.write_all(&buf[..n]).is_err() {
                        break;
                    }
                }
            }
        }
        if let Ok(mut g) = slot.lock()
            && let Some(mut c) = g.take()
        {
            let _ = c.kill();
            let _ = c.wait();
        }
        thread::sleep(Duration::from_millis(150));
    }
}

#[cfg(target_os = "linux")]
fn write_cava_config_fifo(bars: usize, fifo: &std::path::Path) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("stillmusic");
    let path = dir.join(format!("cava-{}.cfg", std::process::id()));
    let cfg = format!(
        r#"## still — generated cava config (do not edit)
[general]
framerate = 60
bars = {bars}
autosens = 1
sensitivity = 85
lower_cutoff_freq = 50
higher_cutoff_freq = 12000

[input]
method = fifo
source = {}
sample_rate = 44100
sample_bits = 16
channels = 1

[output]
method = raw
raw_target = /dev/stdout
data_format = ascii
ascii_max_range = {ASCII_MAX}
bar_delimiter = 59
frame_delimiter = 10
channels = mono

[smoothing]
monstercat = 0
noise_reduction = 70
integral = 65
gravity = 90
ignore = 0
"#,
        fifo.display()
    );
    std::fs::write(&path, cfg).context("write cava config")?;
    Ok(path)
}

fn read_loop<R: std::io::Read>(reader: R, levels: Arc<Mutex<Vec<f32>>>, bars: usize) {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Some(parsed) = parse_ascii_frame(&line, bars)
                    && let Ok(mut g) = levels.lock()
                {
                    *g = parsed;
                }
            }
            Err(_) => break,
        }
    }
}

/// Parse a cava ASCII frame: `12;45;3;...;\n` (semicolon-delimited, optional trailing).
pub fn parse_ascii_frame(line: &str, bars: usize) -> Option<Vec<f32>> {
    let line = line.trim();
    if line.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(bars);
    for part in line.split(';') {
        if part.is_empty() {
            continue;
        }
        let v: f32 = part.parse().ok()?;
        out.push((v / ASCII_MAX as f32).clamp(0.0, 1.0));
        if out.len() >= bars {
            break;
        }
    }
    if out.is_empty() {
        return None;
    }
    while out.len() < bars {
        out.push(0.0);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_ascii_frame() {
        let v = parse_ascii_frame("0;50;100;25;\n", 4).unwrap();
        assert_eq!(v.len(), 4);
        assert!((v[0] - 0.0).abs() < f32::EPSILON);
        assert!((v[1] - 0.5).abs() < 0.01);
        assert!((v[2] - 1.0).abs() < f32::EPSILON);
        assert!((v[3] - 0.25).abs() < 0.01);
    }

    #[test]
    fn pads_short_frame() {
        let v = parse_ascii_frame("10;20", 4).unwrap();
        assert_eq!(v.len(), 4);
        assert!(v[2] == 0.0 && v[3] == 0.0);
    }

    #[test]
    fn rejects_empty() {
        assert!(parse_ascii_frame("\n", 8).is_none());
    }
}
