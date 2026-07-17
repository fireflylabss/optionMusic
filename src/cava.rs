//! Optional [cava](https://github.com/karlstav/cava) bridge — raw ASCII spectrum for the UI.

use std::io::{BufRead, BufReader};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::{Context, Result};

const ASCII_MAX: u32 = 100;
const DEFAULT_BARS: usize = 64;

/// Live spectrum levels in `0.0 ..= 1.0`, refreshed by a cava child process.
pub struct CavaBridge {
    child: Option<Child>,
    levels: Arc<Mutex<Vec<f32>>>,
    config_path: PathBuf,
    bars: usize,
}

impl CavaBridge {
    /// Spawn cava if the binary exists. Returns `None` when unavailable.
    pub fn try_start() -> Option<Self> {
        if !cava_on_path() {
            return None;
        }
        match Self::start_with_input("pipewire").or_else(|_| Self::start_with_input("pulse")) {
            Ok(bridge) => Some(bridge),
            Err(_) => None,
        }
    }

    fn start_with_input(input_method: &str) -> Result<Self> {
        let bars = DEFAULT_BARS;
        let config_path = write_cava_config(bars, input_method)?;
        let levels = Arc::new(Mutex::new(vec![0.0; bars]));

        let mut child = Command::new("cava")
            .arg("-p")
            .arg(&config_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .with_context(|| format!("failed to spawn cava ({input_method})"))?;

        let stdout = child
            .stdout
            .take()
            .context("cava stdout missing")?;

        // Bail early if cava dies immediately (bad input method / missing server).
        thread::sleep(Duration::from_millis(80));
        if let Ok(Some(status)) = child.try_wait() {
            anyhow::bail!("cava exited early ({input_method}): {status}");
        }

        let shared = Arc::clone(&levels);
        thread::Builder::new()
            .name("optmusic-cava".into())
            .spawn(move || read_loop(stdout, shared, bars))
            .context("cava reader thread")?;

        Ok(Self {
            child: Some(child),
            levels,
            config_path,
            bars,
        })
    }

    pub fn snapshot(&self) -> Vec<f32> {
        self.levels
            .lock()
            .map(|g| g.clone())
            .unwrap_or_else(|_| vec![0.0; self.bars])
    }

    pub fn stop(&mut self) {
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
    Command::new("cava")
        .arg("-v")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn write_cava_config(bars: usize, input_method: &str) -> Result<PathBuf> {
    let dir = std::env::temp_dir().join("optmusic");
    std::fs::create_dir_all(&dir).context("temp cava dir")?;
    let path = dir.join(format!("cava-{}.cfg", std::process::id()));

    let cfg = format!(
        r#"## optMusic — generated cava config (do not edit)
[general]
framerate = 60
bars = {bars}
autosens = 1
sensitivity = 85
lower_cutoff_freq = 50
higher_cutoff_freq = 12000

[input]
method = {input_method}
source = auto

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

fn read_loop<R: std::io::Read>(reader: R, levels: Arc<Mutex<Vec<f32>>>, bars: usize) {
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    loop {
        line.clear();
        match reader.read_line(&mut line) {
            Ok(0) => break,
            Ok(_) => {
                if let Some(parsed) = parse_ascii_frame(&line, bars) {
                    if let Ok(mut g) = levels.lock() {
                        *g = parsed;
                    }
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
