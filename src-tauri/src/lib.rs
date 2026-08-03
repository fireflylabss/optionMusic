use optionmusic::controller::{CoreController, PlaybackState, Snapshot, TrackDto};
use optionmusic::eq::EqPreset;
use serde_json::{json, Value};
use std::path::PathBuf;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

type State = Mutex<CoreController>;
fn error(e: impl std::fmt::Display) -> String {
    e.to_string()
}

const ENRICH_BATCH: usize = 32;

fn spawn_tag_enrichment(handle: AppHandle) {
    std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(5));
            let Some(state) = handle.try_state::<State>() else {
                continue;
            };
            let Ok(mut core) = state.try_lock() else {
                continue;
            };
            if !core.tags_enrichment_pending() {
                break;
            }
            let update = core.enrich_tags_batch(ENRICH_BATCH);
            drop(core);
            let _ = handle.emit("optionmusic://library-enriched", &update);
            if update.done {
                break;
            }
        }
    });
}

#[tauri::command]
fn snapshot(state: tauri::State<'_, State>) -> Result<Snapshot, String> {
    Ok(state.lock().map_err(error)?.snapshot())
}
#[tauri::command]
fn playback_state(state: tauri::State<'_, State>) -> Result<PlaybackState, String> {
    Ok(state.lock().map_err(error)?.playback_state())
}
#[tauri::command]
fn scan_music_directories(
    paths: Vec<String>,
    state: tauri::State<'_, State>,
    app: AppHandle,
) -> Result<Vec<TrackDto>, String> {
    let mut core = state.lock().map_err(error)?;
    let tracks = core
        .scan(Some(paths.into_iter().map(PathBuf::from).collect()))
        .map_err(error)?;
    let dtos: Vec<TrackDto> = tracks.iter().map(TrackDto::from).collect();
    drop(core);
    spawn_tag_enrichment(app);
    Ok(dtos)
}
#[tauri::command]
fn default_music_directory() -> String {
    optionmusic::config::default_music_dir()
        .to_string_lossy()
        .into_owned()
}
#[tauri::command]
fn play_track(id: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.play(&id).map_err(error)
}
#[tauri::command]
fn toggle_pause(state: tauri::State<'_, State>) -> Result<bool, String> {
    state.lock().map_err(error)?.toggle_pause().map_err(error)
}
#[tauri::command]
fn next(state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.next().map_err(error)
}
#[tauri::command]
fn previous(state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.previous().map_err(error)
}
#[tauri::command]
fn stop(state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.stop();
    Ok(())
}
#[tauri::command]
fn seek(seconds: f64, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.seek(seconds).map_err(error)
}
#[tauri::command]
fn set_volume(volume: u8, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.set_volume(volume);
    Ok(())
}
#[tauri::command]
fn set_excess_volume(enabled: bool, state: tauri::State<'_, State>) -> Result<(), String> {
    state
        .lock()
        .map_err(error)?
        .set_excess_volume(enabled)
        .map_err(error)
}
#[tauri::command]
fn set_ldm(enabled: bool, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.set_ldm(enabled).map_err(error)
}
#[tauri::command]
fn set_artist_source(source: String, state: tauri::State<'_, State>) -> Result<(), String> {
    let parsed = match source.to_ascii_lowercase().as_str() {
        "metadata" | "tag" | "tags" | "meta" => optionmusic::config::ArtistSource::Metadata,
        "folder" | "dir" | "directory" => optionmusic::config::ArtistSource::Folder,
        _ => return Err(format!("unknown artist_source: {source}")),
    };
    state
        .lock()
        .map_err(error)?
        .set_artist_source(parsed)
        .map_err(error)
}
#[tauri::command]
fn restore_session(state: tauri::State<'_, State>) -> Result<bool, String> {
    state
        .lock()
        .map_err(error)?
        .restore_session()
        .map_err(error)
}
#[tauri::command]
fn persist_resume(state: tauri::State<'_, State>) -> Result<(), String> {
    state
        .lock()
        .map_err(error)?
        .persist_resume(true)
        .map_err(error)
}
#[tauri::command]
fn toggle_mute(state: tauri::State<'_, State>) -> Result<bool, String> {
    Ok(state.lock().map_err(error)?.toggle_mute())
}
#[tauri::command]
fn set_eq(eq: String, state: tauri::State<'_, State>) -> Result<(), String> {
    let p = EqPreset::ALL
        .iter()
        .copied()
        .find(|p| p.label() == eq)
        .ok_or_else(|| "unknown EQ preset".to_string())?;
    state.lock().map_err(error)?.set_eq(p);
    Ok(())
}
#[tauri::command]
fn queue_add(id: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.add_queue(&id).map_err(error)
}
#[tauri::command]
fn queue_remove(id: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.remove_queue(&id);
    Ok(())
}
#[tauri::command]
fn queue_play_next(id: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.play_next(&id).map_err(error)
}
#[tauri::command]
fn toggle_favorite(id: String, state: tauri::State<'_, State>) -> Result<bool, String> {
    state
        .lock()
        .map_err(error)?
        .toggle_favorite(&id)
        .map_err(error)
}
#[tauri::command]
fn cycle_loop(state: tauri::State<'_, State>) -> Result<String, String> {
    Ok(state.lock().map_err(error)?.cycle_loop().label().into())
}
#[tauri::command]
fn shuffle(state: tauri::State<'_, State>) -> Result<(), String> {
    state.lock().map_err(error)?.shuffle();
    Ok(())
}
#[tauri::command]
fn track_cover(id: String, state: tauri::State<'_, State>) -> Result<Option<String>, String> {
    state
        .lock()
        .map_err(error)?
        .cover_data_url(&id)
        .map_err(error)
}

/// Preferred cover IPC: absolute file path for `convertFileSrc` (falls back to `track_cover` data URL on the client).
#[tauri::command]
fn track_cover_url(id: String, state: tauri::State<'_, State>) -> Result<Option<String>, String> {
    state
        .lock()
        .map_err(error)?
        .cover_file_path(&id)
        .map_err(error)
}

// Kept as a compatibility boundary for the existing UI; storage is still config.toml.
#[tauri::command]
fn load_settings(state: tauri::State<'_, State>) -> Result<Value, String> {
    let c = state.lock().map_err(error)?;
    let mut settings: Value = serde_json::from_str(c.desktop_preferences()).map_err(error)?;
    if let Some(object) = settings.as_object_mut() {
        object.insert("folders".into(), json!(c.config.music_dirs));
    }
    Ok(json!({"settings": settings, "favorites": c.config.favorites}))
}
#[tauri::command]
fn save_settings(settings: Value, state: tauri::State<'_, State>) -> Result<(), String> {
    let mut c = state.lock().map_err(error)?;
    let payload = settings.get("settings").unwrap_or(&settings);
    if let Some(folders) = payload.get("folders").and_then(Value::as_array) {
        c.config.music_dirs = folders
            .iter()
            .filter_map(Value::as_str)
            .map(|path| optionmusic::config::resolve_music_dir(path))
            .collect::<Result<_, _>>()
            .map_err(error)?;
    }
    let favorites = payload
        .get("favorites")
        .or_else(|| settings.get("favorites"))
        .and_then(Value::as_array);
    if let Some(favorites) = favorites {
        // Favorites are ids, not paths supplied for playback; retain only
        // strings here and let core validation happen on playback operations.
        c.config.favorites = favorites
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
    }
    let preferences = serde_json::to_string(payload).map_err(error)?;
    c.set_desktop_preferences(preferences).map_err(error)
}
#[tauri::command]
fn reveal_in_file_manager(path: String, state: tauri::State<'_, State>) -> Result<(), String> {
    let known = state
        .lock()
        .map_err(error)?
        .known_path(&path)
        .map_err(error)?
        .to_path_buf();
    let target = &known;
    #[cfg(target_os = "windows")]
    std::process::Command::new("explorer")
        .arg("/select,")
        .arg(target)
        .spawn()
        .map_err(error)?;
    #[cfg(target_os = "macos")]
    std::process::Command::new("open")
        .arg("-R")
        .arg(target)
        .spawn()
        .map_err(error)?;
    #[cfg(all(unix, not(target_os = "macos")))]
    std::process::Command::new("xdg-open")
        .arg(target.parent().unwrap_or(target))
        .spawn()
        .map_err(error)?;
    Ok(())
}

#[tauri::command]
fn set_speed(speed: f64, state: tauri::State<'_, State>) -> Result<f64, String> {
    state.lock().map_err(error)?.set_speed(speed).map_err(error)
}

#[tauri::command]
fn set_pitch(pitch: f64, state: tauri::State<'_, State>) -> Result<f64, String> {
    state.lock().map_err(error)?.set_pitch(pitch).map_err(error)
}

#[tauri::command]
fn reset_speed_pitch(state: tauri::State<'_, State>) -> Result<(), String> {
    state
        .lock()
        .map_err(error)?
        .reset_speed_pitch()
        .map_err(error)
}

#[tauri::command]
fn set_replaygain(mode: String, state: tauri::State<'_, State>) -> Result<(), String> {
    let parsed = match mode.to_ascii_lowercase().as_str() {
        "off" | "no" | "none" => optionmusic::config::ReplayGainMode::Off,
        "track" => optionmusic::config::ReplayGainMode::Track,
        "album" => optionmusic::config::ReplayGainMode::Album,
        _ => return Err(format!("unknown replaygain mode: {mode}")),
    };
    state
        .lock()
        .map_err(error)?
        .set_replaygain(parsed)
        .map_err(error)
}

#[tauri::command]
fn list_playlists(
    state: tauri::State<'_, State>,
) -> Result<Vec<optionmusic::saved_playlists::SavedPlaylist>, String> {
    state.lock().map_err(error)?.list_playlists().map_err(error)
}

#[tauri::command]
fn create_playlist(
    name: String,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::saved_playlists::SavedPlaylist, String> {
    state
        .lock()
        .map_err(error)?
        .create_playlist(&name)
        .map_err(error)
}

#[tauri::command]
fn rename_playlist(
    id: String,
    name: String,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::saved_playlists::SavedPlaylist, String> {
    state
        .lock()
        .map_err(error)?
        .rename_playlist(&id, &name)
        .map_err(error)
}

#[tauri::command]
fn delete_playlist(id: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state
        .lock()
        .map_err(error)?
        .delete_playlist(&id)
        .map_err(error)
}

#[tauri::command]
fn playlist_add(
    id: String,
    track_id: String,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::saved_playlists::SavedPlaylist, String> {
    state
        .lock()
        .map_err(error)?
        .playlist_add(&id, &track_id)
        .map_err(error)
}

#[tauri::command]
fn playlist_remove(
    id: String,
    track_id: String,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::saved_playlists::SavedPlaylist, String> {
    state
        .lock()
        .map_err(error)?
        .playlist_remove(&id, &track_id)
        .map_err(error)
}

#[tauri::command]
fn import_m3u(
    path: String,
    name: Option<String>,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::saved_playlists::SavedPlaylist, String> {
    state
        .lock()
        .map_err(error)?
        .import_m3u(PathBuf::from(path).as_path(), name.as_deref())
        .map_err(error)
}

#[tauri::command]
fn export_m3u(id: String, path: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state
        .lock()
        .map_err(error)?
        .export_m3u(&id, PathBuf::from(path).as_path())
        .map_err(error)
}

#[tauri::command]
fn play_playlist(id: String, state: tauri::State<'_, State>) -> Result<(), String> {
    state
        .lock()
        .map_err(error)?
        .play_playlist(&id)
        .map_err(error)
}

#[tauri::command]
fn smart_shelf(kind: String, state: tauri::State<'_, State>) -> Result<Vec<TrackDto>, String> {
    let shelf = match kind.as_str() {
        "played_week" | "played" | "week" => optionmusic::controller::SmartShelf::PlayedWeek,
        "no_cover" | "nocover" => optionmusic::controller::SmartShelf::NoCover,
        "incomplete_albums" | "incomplete" => optionmusic::controller::SmartShelf::IncompleteAlbums,
        _ => return Err(format!("unknown shelf: {kind}")),
    };
    Ok(state.lock().map_err(error)?.smart_shelf(shelf))
}

#[tauri::command]
fn get_track_tags(
    id: String,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::meta::AudioTags, String> {
    state
        .lock()
        .map_err(error)?
        .get_track_tags(&id)
        .map_err(error)
}

#[tauri::command]
fn set_track_tags(
    id: String,
    tags: optionmusic::meta::AudioTags,
    state: tauri::State<'_, State>,
) -> Result<TrackDto, String> {
    state
        .lock()
        .map_err(error)?
        .set_track_tags(&id, tags)
        .map_err(error)
}

#[tauri::command]
fn track_lyrics(
    id: String,
    state: tauri::State<'_, State>,
) -> Result<optionmusic::meta::Lyrics, String> {
    state
        .lock()
        .map_err(error)?
        .track_lyrics(&id)
        .map_err(error)
}

#[tauri::command]
fn dl_ensure_yt_dlp() -> Result<String, String> {
    optionmusic::download::ensure_yt_dlp().map_err(error)
}

#[tauri::command]
fn dl_search(
    provider: String,
    query: String,
) -> Result<Vec<optionmusic::download::SearchHit>, String> {
    let provider = match provider.to_ascii_lowercase().as_str() {
        "youtube" | "yt" => optionmusic::download::Provider::Youtube,
        "youtubemusic" | "ytm" | "youtube_music" => optionmusic::download::Provider::YoutubeMusic,
        "soundcloud" | "sc" => optionmusic::download::Provider::Soundcloud,
        _ => return Err(format!("unknown provider: {provider}")),
    };
    optionmusic::download::search(provider, &query).map_err(error)
}

#[tauri::command]
fn dl_run(
    urls: Vec<String>,
    audio: bool,
    output: Option<String>,
    audio_format: Option<String>,
) -> Result<(), String> {
    let out = optionmusic::download::resolve_output_dir(
        output.as_deref().map(std::path::Path::new),
        "",
    )
    .map_err(error)?;
    let kind = if audio {
        optionmusic::download::MediaKind::Audio
    } else {
        optionmusic::download::MediaKind::Video
    };
    let query = urls.join(";");
    let provider = optionmusic::download::detect_provider(&query)
        .unwrap_or(optionmusic::download::Provider::Youtube);
    let req = optionmusic::download::DownloadRequest {
        query,
        provider,
        kind,
        output_dir: out,
        audio_format: audio_format.unwrap_or_else(|| "mp3".into()),
    };
    optionmusic::download::run_download(&req).map_err(error)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    #[cfg(unix)]
    unsafe {
        // libmpv expects a C numeric locale for parsing and formatting values.
        libc::setlocale(libc::LC_NUMERIC, b"C\0".as_ptr().cast());
    }

    tauri::Builder::default()
        .manage(Mutex::new(CoreController::new()))
        .setup(|app| {
            #[cfg(unix)]
            unsafe {
                // GTK/WebKit may have overwritten LC_NUMERIC during init.
                libc::setlocale(libc::LC_NUMERIC, b"C\0".as_ptr().cast());
            }
            let handle = app.handle().clone();
            std::thread::spawn(move || loop {
                std::thread::sleep(std::time::Duration::from_millis(250));
                if let Some(state) = handle.try_state::<State>() {
                    if let Ok(mut core) = state.try_lock() {
                        // Playback-only payload — never re-send the full library on the ticker.
                        let _ = handle.emit("optionmusic://state", core.playback_state());
                    }
                }
            });
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            snapshot,
            playback_state,
            scan_music_directories,
            default_music_directory,
            play_track,
            toggle_pause,
            next,
            previous,
            stop,
            seek,
            set_volume,
            set_excess_volume,
            set_ldm,
            set_artist_source,
            restore_session,
            persist_resume,
            toggle_mute,
            set_eq,
            queue_add,
            queue_remove,
            queue_play_next,
            toggle_favorite,
            cycle_loop,
            shuffle,
            track_cover,
            track_cover_url,
            load_settings,
            save_settings,
            reveal_in_file_manager,
            set_speed,
            set_pitch,
            reset_speed_pitch,
            set_replaygain,
            list_playlists,
            create_playlist,
            rename_playlist,
            delete_playlist,
            playlist_add,
            playlist_remove,
            import_m3u,
            export_m3u,
            play_playlist,
            smart_shelf,
            get_track_tags,
            set_track_tags,
            track_lyrics,
            dl_ensure_yt_dlp,
            dl_search,
            dl_run
        ])
        .run(tauri::generate_context!())
        .expect("error while running optionMusic");
}
