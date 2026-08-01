//! Command-line interface for optionMusic.

use std::path::PathBuf;

use clap::{
    Parser, Subcommand, ValueEnum,
    builder::styling::{AnsiColor, Effects, Styles},
};

fn cli_styles() -> Styles {
    Styles::styled()
        .header(AnsiColor::White.on_default() | Effects::BOLD)
        .usage(AnsiColor::White.on_default() | Effects::BOLD)
        .literal(AnsiColor::BrightWhite.on_default())
        .placeholder(AnsiColor::BrightBlack.on_default())
        .error(AnsiColor::BrightRed.on_default() | Effects::BOLD)
        .valid(AnsiColor::BrightWhite.on_default())
        .invalid(AnsiColor::BrightRed.on_default())
}

/// Download wizard UI mode (CLI override).
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliDlUi {
    /// Arrow keys, checkboxes, cycling options
    #[value(name = "arrows", alias = "arrow")]
    Arrows,
    /// Classic typed prompts
    #[value(name = "type", aliases = ["typing", "prompt"])]
    Type,
}

impl CliDlUi {
    pub fn to_mode(self) -> crate::config::DlUiMode {
        match self {
            Self::Arrows => crate::config::DlUiMode::Arrows,
            Self::Type => crate::config::DlUiMode::Type,
        }
    }
}

/// Starting EQ preset (CLI).
#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum CliEq {
    Off,
    Bass,
    Treble,
    Rock,
    Vocal,
    Lofi,
}

impl CliEq {
    pub fn to_preset(self) -> crate::eq::EqPreset {
        use crate::eq::EqPreset;
        match self {
            Self::Off => EqPreset::Off,
            Self::Bass => EqPreset::Bass,
            Self::Treble => EqPreset::Treble,
            Self::Rock => EqPreset::Rock,
            Self::Vocal => EqPreset::Vocal,
            Self::Lofi => EqPreset::Lofi,
        }
    }
}

/// optionMusic — minimal black & white CLI music player
///
/// Invoke as `optionmusic`, or aliases `optmusic` / `msc`.
#[derive(Debug, Parser)]
#[command(
    name = "optionmusic",
    version,
    about = "♪ optionMusic — minimal black & white CLI music player",
    long_about = "optionMusic — play local audio from the terminal.\n\
\n\
  binaries   optionmusic · optmusic · msc\n\
  engine     MPV (libmpv)\n\
  optional   cava spectrum bars",
    after_help = "Shortcuts:\n\
  msc p song.mp3          same as  msc play song.mp3\n\
  msc pl ./album          same as  msc play ./album\n\
  msc song.mp3            bare path also plays\n\
  msc ls ~/Music -r       same as  msc list ~/Music -r\n\
  msc i song.flac         same as  msc info song.flac\n\
  msc dl                  interactive download wizard\n\
  msc dl URL --audio      direct download (aliases: download, d)\n\
\n\
Playback keys (press ? / h in the player for the full sidebar):\n\
  space        pause / resume          n / p        next / previous\n\
  ← →          seek ±5s                { }          seek ±60s\n\
  + / −        volume                  m            mute\n\
  e            cycle EQ                [ ]          speed\n\
  , / .        pitch                   0            reset speed/pitch\n\
  o            cycle loop (off→list→track)\n\
  l            playlist sidebar        r            shuffle\n\
  f            filename line           v            cava on/off\n\
  c            settings sidebar        ? / h        help\n\
  q / Esc      quit\n\
\n\
Examples:\n\
  msc p song.mp3\n\
  msc play ./music -s -l -c 2\n\
  msc play -m ~/Music --eq bass --pitch 1.05\n\
  msc play album/ --loop-file --cava\n\
  msc ls ~/Music -r\n\
  msc dl\n\
  msc dl -i\n\
  msc dl https://youtu.be/… --audio -o .\n\
  msc download -p soundcloud \"ambient\" -a\n\
  msc --help",
    styles = cli_styles(),
    propagate_version = true,
    arg_required_else_help = true,
    disable_help_subcommand = false,
)]
pub struct Cli {
    /// Default music library (used when `play` has no paths)
    #[arg(
        short = 'm',
        long = "music-dir",
        global = true,
        env = "OPTIONMUSIC_MUSIC_DIR",
        default_value = "",
        hide_default_value = true,
        value_name = "DIR",
        help = "Music library root (default: ~/Music; also OPTIONMUSIC_MUSIC_DIR)"
    )]
    pub music_dir: String,

    /// Enable cava spectrum strip (off by default; requires `cava` on PATH)
    #[arg(
        long = "cava",
        global = true,
        help = "Enable cava bars (toggle later with v)"
    )]
    pub cava: bool,

    /// Suppress the startup banner outside the TUI
    #[arg(
        short = 'q',
        long = "quiet",
        global = true,
        help = "Quiet mode (less stdout noise)"
    )]
    pub quiet: bool,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Play files or directories (interactive TUI in a terminal)
    #[command(visible_aliases = ["p", "pl"])]
    Play {
        /// Files or directories (defaults to --music-dir / ~/Music)
        #[arg(required = false, value_name = "PATH")]
        paths: Vec<PathBuf>,

        /// Volume 0–100
        #[arg(
            short,
            long,
            default_value_t = 80,
            value_parser = clap::value_parser!(u8).range(0..=100),
            value_name = "PCT"
        )]
        volume: u8,

        /// Playback speed factor
        #[arg(short = 'f', long, default_value_t = 1.0, value_name = "FACTOR")]
        speed: f64,

        /// Pitch factor (1.0 = normal)
        #[arg(long, default_value_t = 1.0, value_name = "FACTOR")]
        pitch: f64,

        /// Starting EQ preset
        #[arg(long = "eq", value_enum, default_value_t = CliEq::Off)]
        eq: CliEq,

        /// Crossfade / audio-fade duration in seconds
        #[arg(short = 'c', long, default_value_t = 0.0, value_name = "SECONDS")]
        crossfade: f64,

        /// Shuffle the playlist
        #[arg(short, long)]
        shuffle: bool,

        /// Loop the whole playlist (alias: --repeat)
        #[arg(short = 'l', long = "loop", visible_alias = "repeat")]
        loop_playlist: bool,

        /// Repeat the current track only
        #[arg(long = "loop-file", visible_alias = "repeat-one")]
        loop_file: bool,

        /// Kept for compatibility (playback is always interactive in a TTY)
        #[arg(short, long, hide = true)]
        interactive: bool,
    },

    /// Show info about an audio file
    #[command(visible_alias = "i")]
    Info {
        /// Path to the audio file
        path: PathBuf,
    },

    /// List playable audio files in a path
    #[command(visible_alias = "ls")]
    List {
        /// Directory (or file) to scan
        #[arg(default_value = ".", value_name = "PATH")]
        path: PathBuf,

        /// Scan subdirectories
        #[arg(short, long)]
        recursive: bool,
    },

    /// Download audio/video via yt-dlp (YouTube, YouTube Music, SoundCloud)
    #[command(visible_aliases = ["dl", "d"])]
    Download {
        /// URL or search query (omit for interactive wizard)
        #[arg(value_name = "QUERY")]
        query: Option<String>,

        /// Provider (auto-detected from URL when omitted)
        #[arg(short = 'p', long = "provider", value_enum, value_name = "NAME")]
        provider: Option<crate::download::Provider>,

        /// Extract audio only (mp3 by default; needs ffmpeg)
        #[arg(short = 'a', long = "audio", group = "media_kind")]
        audio: bool,

        /// Download video (merged mp4)
        #[arg(long = "video", group = "media_kind")]
        video: bool,

        /// Download both a video file and a separate audio file
        #[arg(long = "both", group = "media_kind")]
        both: bool,

        /// Output directory (default: current directory)
        #[arg(short = 'o', long = "output", value_name = "DIR")]
        output: Option<PathBuf>,

        /// Audio container when using --audio
        #[arg(long = "audio-format", default_value = "mp3", value_name = "FMT")]
        audio_format: String,

        /// Force the interactive wizard even with a query
        #[arg(short = 'i', long = "interactive")]
        interactive: bool,

        /// Download wizard UI (`arrows` default, or `type` for typed prompts)
        #[arg(
            long = "ui",
            value_enum,
            value_name = "MODE",
            help = "Wizard UI: arrows (default) or type"
        )]
        ui: Option<CliDlUi>,
    },

    /// Manage local playlists (save / import M3U)
    #[command(visible_aliases = ["plists", "pls"])]
    Playlist {
        #[command(subcommand)]
        action: PlaylistCmd,
    },

    /// Print version
    #[command(visible_alias = "ver")]
    Version,
}

#[derive(Debug, Subcommand)]
pub enum PlaylistCmd {
    /// List saved playlists
    List,
    /// Create an empty playlist
    Create {
        /// Playlist name
        name: String,
    },
    /// Import an M3U / M3U8 file
    Import {
        /// Path to the M3U file
        path: PathBuf,
        /// Optional display name
        #[arg(short, long)]
        name: Option<String>,
    },
    /// Export a playlist to M3U
    Export {
        /// Playlist id
        id: String,
        /// Output .m3u path
        path: PathBuf,
    },
    /// Add a track path to a playlist
    Add {
        /// Playlist id
        id: String,
        /// Absolute track path
        track: PathBuf,
    },
    /// Delete a playlist
    Delete {
        /// Playlist id
        id: String,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_play_with_path() {
        let cli = Cli::parse_from(["optionmusic", "play", "song.mp3"]);
        match cli.command {
            Some(Command::Play { paths, volume, .. }) => {
                assert_eq!(paths, vec![PathBuf::from("song.mp3")]);
                assert_eq!(volume, 80);
            }
            _ => panic!("expected play"),
        }
    }

    #[test]
    fn parses_music_dir_and_crossfade() {
        let cli = Cli::parse_from([
            "msc", "-m", "/tmp", "play", "-c", "2.5", "-f", "1.25", "-v", "50",
        ]);
        assert_eq!(cli.music_dir, "/tmp");
        match cli.command {
            Some(Command::Play {
                paths,
                volume,
                speed,
                crossfade,
                ..
            }) => {
                assert!(paths.is_empty());
                assert_eq!(volume, 50);
                assert!((speed - 1.25).abs() < f64::EPSILON);
                assert!((crossfade - 2.5).abs() < f64::EPSILON);
            }
            _ => panic!("expected play"),
        }
    }

    #[test]
    fn parses_list_recursive() {
        let cli = Cli::parse_from(["optionmusic", "list", "./music", "-r"]);
        match cli.command {
            Some(Command::List { path, recursive }) => {
                assert_eq!(path, PathBuf::from("./music"));
                assert!(recursive);
            }
            _ => panic!("expected list"),
        }
    }

    #[test]
    fn parses_loop_aliases_and_eq() {
        let cli = Cli::parse_from([
            "msc", "play", ".", "--repeat", "--eq", "bass", "--pitch", "1.1",
        ]);
        match cli.command {
            Some(Command::Play {
                loop_playlist,
                loop_file,
                eq,
                pitch,
                ..
            }) => {
                assert!(loop_playlist);
                assert!(!loop_file);
                assert!(matches!(eq, CliEq::Bass));
                assert!((pitch - 1.1).abs() < f64::EPSILON);
            }
            _ => panic!("expected play"),
        }
    }

    #[test]
    fn parses_loop_file() {
        let cli = Cli::parse_from(["msc", "play", "a.mp3", "--loop-file"]);
        match cli.command {
            Some(Command::Play { loop_file, .. }) => assert!(loop_file),
            _ => panic!("expected play"),
        }
    }

    #[test]
    fn parses_play_alias() {
        let cli = Cli::parse_from(["msc", "p", "a.mp3"]);
        match cli.command {
            Some(Command::Play { paths, .. }) => {
                assert_eq!(paths, vec![PathBuf::from("a.mp3")]);
            }
            _ => panic!("expected play via alias p"),
        }
    }

    #[test]
    fn parses_play_alias_pl() {
        let cli = Cli::parse_from(["msc", "pl", "a.mp3"]);
        match cli.command {
            Some(Command::Play { paths, .. }) => {
                assert_eq!(paths, vec![PathBuf::from("a.mp3")]);
            }
            _ => panic!("expected play via alias pl"),
        }
    }

    #[test]
    fn parses_download_direct() {
        let cli = Cli::parse_from([
            "msc",
            "dl",
            "https://youtu.be/abc",
            "--audio",
            "-o",
            "/tmp/out",
            "-p",
            "youtube",
        ]);
        match cli.command {
            Some(Command::Download {
                query,
                provider,
                audio,
                video,
                output,
                interactive,
                ..
            }) => {
                assert_eq!(query.as_deref(), Some("https://youtu.be/abc"));
                assert!(matches!(provider, Some(crate::download::Provider::Youtube)));
                assert!(audio);
                assert!(!video);
                assert_eq!(output, Some(PathBuf::from("/tmp/out")));
                assert!(!interactive);
            }
            _ => panic!("expected download"),
        }
    }

    #[test]
    fn parses_download_ui_type() {
        let cli = Cli::parse_from(["msc", "dl", "--ui", "type", "-i"]);
        match cli.command {
            Some(Command::Download {
                ui,
                interactive,
                ..
            }) => {
                assert!(interactive);
                assert!(matches!(ui, Some(CliDlUi::Type)));
            }
            _ => panic!("expected download"),
        }
    }

    #[test]
    fn parses_download_interactive_alias() {
        let cli = Cli::parse_from(["msc", "d", "-i"]);
        match cli.command {
            Some(Command::Download {
                query, interactive, ..
            }) => {
                assert!(query.is_none());
                assert!(interactive);
            }
            _ => panic!("expected download via alias d"),
        }
    }

    #[test]
    fn parses_download_soundcloud_search() {
        let cli = Cli::parse_from([
            "optionmusic",            "download",
            "ambient mix",
            "-p",
            "soundcloud",
            "-a",
        ]);
        match cli.command {
            Some(Command::Download {
                query,
                provider,
                audio,
                ..
            }) => {
                assert_eq!(query.as_deref(), Some("ambient mix"));
                assert!(matches!(
                    provider,
                    Some(crate::download::Provider::Soundcloud)
                ));
                assert!(audio);
            }
            _ => panic!("expected download"),
        }
    }
}
