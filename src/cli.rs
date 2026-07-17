//! Command-line interface for optMusic.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// optMusic — minimal black & white CLI music player
///
/// Short for “option music”. Invoke as `optmusic` or `msc`.
#[derive(Debug, Parser)]
#[command(
    name = "optmusic",
    version,
    about = "♪ optMusic — minimal CLI music player",
    long_about = "optMusic (option music) — play local audio from the terminal.\n\
                  Binaries: optmusic · msc\n\
                  Engine: MPV (libmpv)\n\n\
                  Keys:  space n/p ←→ {} m e [] , . +/- l r s q\n\n\
                  Examples:\n  \
                  msc play song.mp3\n  \
                  msc play ./music/ -s -l -c 2\n  \
                  msc play -m ~/Music\n  \
                  msc list ./albums -r\n  \
                  msc info song.mp3"
)]
pub struct Cli {
    /// Default music library (used when `play` has no paths)
    #[arg(
        short = 'm',
        long = "music-dir",
        global = true,
        default_value = "",
        hide_default_value = true,
        value_name = "DIR"
    )]
    pub music_dir: String,

    #[command(subcommand)]
    pub command: Option<Command>,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Play one or more audio files / directories
    Play {
        /// Files or directories to play (defaults to --music-dir / ~/Music)
        #[arg(required = false)]
        paths: Vec<PathBuf>,

        /// Volume 0–100 (default: 80)
        #[arg(short, long, default_value_t = 80, value_parser = clap::value_parser!(u8).range(0..=100))]
        volume: u8,

        /// Playback speed factor (default: 1.0)
        #[arg(short = 'f', long, default_value_t = 1.0, value_name = "FACTOR")]
        speed: f64,

        /// Crossfade / audio-fade duration in seconds (default: 0)
        #[arg(short = 'c', long, default_value_t = 0.0, value_name = "SECONDS")]
        crossfade: f64,

        /// Shuffle the playlist
        #[arg(short, long)]
        shuffle: bool,

        /// Loop the playlist forever
        #[arg(short = 'l', long = "loop")]
        loop_playlist: bool,

        /// Kept for compatibility (playback is always interactive in a TTY)
        #[arg(short, long, hide = true)]
        interactive: bool,
    },

    /// Show info about an audio file
    Info {
        /// Path to the audio file
        path: PathBuf,
    },

    /// List playable audio files in a path
    List {
        /// Directory (or file) to scan
        #[arg(default_value = ".")]
        path: PathBuf,

        /// Scan subdirectories
        #[arg(short, long)]
        recursive: bool,
    },

    /// Print version
    Version,
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_play_with_path() {
        let cli = Cli::parse_from(["optmusic", "play", "song.mp3"]);
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
            "msc",
            "-m",
            "/tmp",
            "play",
            "-c",
            "2.5",
            "-f",
            "1.25",
            "-v",
            "50",
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
        let cli = Cli::parse_from(["optmusic", "list", "./music", "-r"]);
        match cli.command {
            Some(Command::List { path, recursive }) => {
                assert_eq!(path, PathBuf::from("./music"));
                assert!(recursive);
            }
            _ => panic!("expected list"),
        }
    }
}
