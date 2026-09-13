// libmpv is linked from the system, the TUI assumes a POSIX terminal, and the
// optional tools are looked up on `PATH`; see `platform` and the README.
#[cfg(not(unix))]
compile_error!(
    "optionMusic supports Unix targets only (Linux is tested, other Unixes are \
     best-effort). Windows is not supported yet."
);

pub mod browse;
pub mod cava;
pub mod cli;
pub mod config;
pub mod controller;
pub mod cover;
pub mod dl_ui;
pub mod download;
pub mod eq;
pub mod history;
pub mod library;
pub mod lyrics;
pub mod meta;
pub mod mpv;
pub mod platform;
pub mod player;
pub mod playlist;
pub mod preview;
pub mod radio;
pub mod rpc;
pub mod saved_playlists;
pub mod settings;
pub mod sleep;
pub mod smart_shuffle;
pub mod stats;
pub mod ui;
