//! optionMusic GPUI — minimal black & white desktop music player.

mod crash;
mod icons;
mod mini;
mod model;
mod now_playing;
mod search_input;
mod theme;
mod view;
mod views;
mod watcher;
mod widgets;

use gpui::{
    App, AppContext, Bounds, KeyBinding, Menu, MenuItem, TitlebarOptions, WindowBounds,
    WindowDecorations, WindowOptions, actions, px, size,
};
use gpui_platform::application;

use crate::view::RootView;

actions!(
    optionmusic,
    [
        PlayPause,
        Next,
        Previous,
        Stop,
        VolumeUp,
        VolumeDown,
        CycleLoop,
        Shuffle,
        ToggleQueue,
        ToggleSearch,
        ListUp,
        ListDown,
        ListFirst,
        ListLast,
        ListActivate,
        DismissOverlay,
        NewWindow,
        Quit,
        Mute,
        FavoriteCurrent,
        SeekBack,
        SeekForward,
        FocusList,
        BlurList,
        NavLibrary,
        NavArtists,
        NavAlbums,
        NavPlaylists,
        NavFavorites,
        NavShelves,
        ToggleStage,
        ToggleLyrics,
        MenuUp,
        MenuDown,
        MenuLeft,
        MenuRight,
        MenuActivate,
        SliderLeft,
        SliderRight,
        SliderHome,
        SliderEnd,
        QueueItemUp,
        QueueItemDown,
        QueueJump,
        ClearQueue,
        OpenSettings,
        About,
        NavRadio,
        Radio,
        CheckForUpdates,
        ToggleVisualizer,
        SleepTimerCycle,
        NavStats,
        ToggleMiniPlayer
    ]
);

fn open_main_window(cx: &mut App) {
    // Restore the last window size from `desktop_preferences` (JSON blob).
    let prefs = model::DesktopPrefs::default();
    let prefs = optionmusic::config::desktop_preferences_raw()
        .and_then(|raw| serde_json::from_str::<model::DesktopPrefs>(&raw).ok())
        .unwrap_or(prefs);
    let width = prefs.window_w.clamp(900.0, 3840.0);
    let height = prefs.window_h.clamp(600.0, 2160.0);
    let bounds = Bounds::centered(None, size(px(width), px(height)), cx);
    cx.open_window(
        WindowOptions {
            focus: true,
            titlebar: Some(TitlebarOptions {
                title: Some("optionMusic".into()),
                #[cfg(target_os = "macos")]
                appears_transparent: true,
                #[cfg(target_os = "macos")]
                traffic_light_position: Some(gpui::point(px(12.0), px(12.0))),
                ..Default::default()
            }),
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            is_movable: true,
            app_owns_titlebar_drag: true,
            window_decorations: Some(WindowDecorations::Client),
            window_min_size: Some(size(px(900.0), px(600.0))),
            app_id: Some("optionmusic".into()),
            ..Default::default()
        },
        |window, cx| cx.new(|cx| RootView::new(window, cx)),
    )
    .expect("open main window");
}

fn new_window(_: &NewWindow, cx: &mut App) {
    open_main_window(cx);
}

fn quit(_: &Quit, cx: &mut App) {
    cx.quit();
}

fn main() {
    crash::install();
    application().run(|cx: &mut App| {
        cx.on_action(new_window);
        cx.on_action(quit);
        cx.on_action(mini::toggle_mini_player);

        // Media shortcuts are suppressed while the search input is focused so
        // typing letters/arrows edits text instead of triggering playback.
        cx.bind_keys([
            KeyBinding::new("space", PlayPause, Some("!TextInput")),
            KeyBinding::new("right", Next, Some("!TextInput")),
            KeyBinding::new("left", Previous, Some("!TextInput")),
            KeyBinding::new("s", Stop, Some("!TextInput")),
            KeyBinding::new("l", CycleLoop, Some("!TextInput")),
            KeyBinding::new("shift-l", Shuffle, Some("!TextInput")),
            KeyBinding::new("q", ToggleQueue, Some("!TextInput")),
            KeyBinding::new("m", Mute, Some("!TextInput")),
            KeyBinding::new("f", FavoriteCurrent, Some("!TextInput")),
            KeyBinding::new("t", ToggleStage, Some("!TextInput")),
            KeyBinding::new("shift-right", SeekForward, Some("!TextInput && !TrackList")),
            KeyBinding::new("shift-left", SeekBack, Some("!TextInput && !TrackList")),
            KeyBinding::new(
                "tab",
                FocusList,
                Some("!TextInput && !TrackList && !Overlay"),
            ),
            KeyBinding::new("up", VolumeUp, Some("!TextInput && !TrackList")),
            KeyBinding::new("down", VolumeDown, Some("!TextInput && !TrackList")),
            KeyBinding::new("ctrl-1", NavLibrary, Some("!TextInput")),
            KeyBinding::new("ctrl-2", NavArtists, Some("!TextInput")),
            KeyBinding::new("ctrl-3", NavAlbums, Some("!TextInput")),
            KeyBinding::new("ctrl-4", NavPlaylists, Some("!TextInput")),
            KeyBinding::new("ctrl-5", NavFavorites, Some("!TextInput")),
            KeyBinding::new("ctrl-6", NavShelves, Some("!TextInput")),
            KeyBinding::new("ctrl-8", NavStats, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-1", NavLibrary, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-2", NavArtists, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-3", NavAlbums, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-4", NavPlaylists, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-5", NavFavorites, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-6", NavShelves, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-8", NavStats, Some("!TextInput")),
            KeyBinding::new("ctrl-f", ToggleSearch, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-f", ToggleSearch, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-n", NewWindow, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-q", Quit, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-m", ToggleMiniPlayer, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-n", NewWindow, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-q", Quit, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-m", ToggleMiniPlayer, None),
            // List navigation (any focused TrackList: tracks, artists, queue…)
            KeyBinding::new("up", ListUp, Some("TrackList")),
            KeyBinding::new("down", ListDown, Some("TrackList")),
            KeyBinding::new("k", ListUp, Some("TrackList")),
            KeyBinding::new("j", ListDown, Some("TrackList")),
            KeyBinding::new("home", ListFirst, Some("TrackList")),
            KeyBinding::new("end", ListLast, Some("TrackList")),
            KeyBinding::new("enter", ListActivate, Some("TrackList")),
            KeyBinding::new("escape", BlurList, Some("TrackList")),
            KeyBinding::new("shift-up", QueueItemUp, Some("TrackList")),
            KeyBinding::new("shift-down", QueueItemDown, Some("TrackList")),
            KeyBinding::new("ctrl-j", QueueJump, Some("!TextInput")),
            // Overlays (context menu, settings dialog, pickers)
            KeyBinding::new("escape", DismissOverlay, Some("Overlay")),
            KeyBinding::new("up", MenuUp, Some("Overlay")),
            KeyBinding::new("down", MenuDown, Some("Overlay")),
            KeyBinding::new("k", MenuUp, Some("Overlay")),
            KeyBinding::new("j", MenuDown, Some("Overlay")),
            KeyBinding::new("enter", MenuActivate, Some("Overlay")),
            KeyBinding::new("left", MenuLeft, Some("Overlay")),
            KeyBinding::new("right", MenuRight, Some("Overlay")),
            KeyBinding::new("tab", MenuRight, Some("Overlay")),
            KeyBinding::new("shift-tab", MenuLeft, Some("Overlay")),
            // Focused sliders (seek/volume) — arrow nudge + home/end.
            KeyBinding::new("left", SliderLeft, Some("Slider")),
            KeyBinding::new("right", SliderRight, Some("Slider")),
            KeyBinding::new("home", SliderHome, Some("Slider")),
            KeyBinding::new("end", SliderEnd, Some("Slider")),
            KeyBinding::new("escape", BlurList, Some("Slider")),
            // Search text input — editing contract
            KeyBinding::new("backspace", search_input::InputBackspace, Some("TextInput")),
            KeyBinding::new("delete", search_input::InputDelete, Some("TextInput")),
            KeyBinding::new("left", search_input::InputLeft, Some("TextInput")),
            KeyBinding::new("right", search_input::InputRight, Some("TextInput")),
            KeyBinding::new(
                "shift-left",
                search_input::InputSelectLeft,
                Some("TextInput"),
            ),
            KeyBinding::new(
                "shift-right",
                search_input::InputSelectRight,
                Some("TextInput"),
            ),
            KeyBinding::new("home", search_input::InputHome, Some("TextInput")),
            KeyBinding::new("end", search_input::InputEnd, Some("TextInput")),
            KeyBinding::new("enter", search_input::SearchSubmit, Some("TextInput")),
            KeyBinding::new("escape", search_input::SearchDismiss, Some("TextInput")),
            KeyBinding::new("tab", search_input::InputTabNext, Some("TextInput")),
            KeyBinding::new("shift-tab", search_input::InputTabPrev, Some("TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-a", search_input::InputSelectAll, Some("TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-v", search_input::InputPaste, Some("TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-c", search_input::InputCopy, Some("TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-x", search_input::InputCut, Some("TextInput")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-a", search_input::InputSelectAll, Some("TextInput")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-v", search_input::InputPaste, Some("TextInput")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-c", search_input::InputCopy, Some("TextInput")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-x", search_input::InputCut, Some("TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-,", OpenSettings, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-,", OpenSettings, None),
            // Radio page + quick seed-from-current action.
            KeyBinding::new("ctrl-7", NavRadio, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-7", NavRadio, Some("!TextInput")),
            KeyBinding::new("ctrl-shift-r", Radio, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-r", Radio, None),
            KeyBinding::new("v", ToggleVisualizer, Some("!TextInput")),
            // Sleep timer — `z` mirrors the TUI key.
            KeyBinding::new("z", SleepTimerCycle, Some("!TextInput")),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-z", SleepTimerCycle, Some("!TextInput")),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-shift-z", SleepTimerCycle, Some("!TextInput")),
        ]);
        cx.set_menus([
            Menu::new("optionMusic").items([
                MenuItem::action("About optionMusic", About),
                MenuItem::separator(),
                MenuItem::action("Check for Updates…", CheckForUpdates),
                MenuItem::separator(),
                MenuItem::action("Settings…", OpenSettings),
                MenuItem::separator(),
                MenuItem::action("Quit optionMusic", Quit),
            ]),
            Menu::new("File").items([MenuItem::action("New Window", NewWindow)]),
            Menu::new("Playback").items([
                MenuItem::action("Play/Pause", PlayPause),
                MenuItem::action("Stop", Stop),
                MenuItem::separator(),
                MenuItem::action("Next", Next),
                MenuItem::action("Previous", Previous),
                MenuItem::separator(),
                MenuItem::action("Loop", CycleLoop),
                MenuItem::action("Shuffle", Shuffle),
                MenuItem::separator(),
                MenuItem::action("Volume Up", VolumeUp),
                MenuItem::action("Volume Down", VolumeDown),
                MenuItem::action("Mute", Mute),
                MenuItem::separator(),
                MenuItem::action("Sleep Timer", SleepTimerCycle),
            ]),
            Menu::new("View").items([
                MenuItem::action("Library", NavLibrary),
                MenuItem::action("Artists", NavArtists),
                MenuItem::action("Albums", NavAlbums),
                MenuItem::action("Playlists", NavPlaylists),
                MenuItem::action("Favorites", NavFavorites),
                MenuItem::action("Shelves", NavShelves),
                MenuItem::action("Radio", NavRadio),
                MenuItem::action("Stats", NavStats),
                MenuItem::separator(),
                MenuItem::action("Search", ToggleSearch),
                MenuItem::action("Queue", ToggleQueue),
                MenuItem::action("Stage", ToggleStage),
                MenuItem::action("Lyrics", ToggleLyrics),
                MenuItem::action("Visualizer", ToggleVisualizer),
                MenuItem::action("Mini Player", ToggleMiniPlayer),
                MenuItem::separator(),
                MenuItem::action("Focus List", FocusList),
            ]),
        ]);
        cx.on_window_closed(|cx, _| {
            if cx.windows().is_empty() {
                cx.quit();
            }
        })
        .detach();

        open_main_window(cx);
        cx.activate(true);
    });
}
