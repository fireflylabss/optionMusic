import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type {
  AlbumGroup,
  ArtistGroup,
  ArtistSource,
  ContextState,
  LibraryEnrichUpdate,
  Page,
  PlaybackState,
  ReplayGainMode,
  SavedPlaylist,
  SmartShelfKind,
  Snapshot,
  Track,
} from '../types'
import { fetchTrackCover } from '../coverQueue'
import { emptySnapshot, hasTauriBridge, trackAlbum, trackArtist } from '../lib/music'

/** Transport/queue/favorites — playback_state is enough; no full library snapshot. */
const PLAYBACK_ONLY_COMMANDS = new Set([
  'toggle_pause',
  'next',
  'previous',
  'stop',
  'seek',
  'set_volume',
  'toggle_mute',
  'set_eq',
  'cycle_loop',
  'shuffle',
  'play_track',
  'queue_add',
  'queue_remove',
  'queue_play_next',
  'toggle_favorite',
  'set_speed',
  'set_pitch',
  'reset_speed_pitch',
])

type DesktopPrefs = {
  focusMode?: boolean
  folders?: string[]
  favorites?: string[]
  [key: string]: unknown
}

export function useOptionMusic() {
  const domain = useRef<Snapshot>(emptySnapshot)
  const [, repaint] = useState(0)
  const [clock, setClock] = useState({ position: 0, duration: null as number | null })
  const [page, setPage] = useState<Page>('library')
  const [search, setSearch] = useState('')
  const [commandOpen, setCommandOpen] = useState(false)
  const [commandIndex, setCommandIndex] = useState(0)
  const [artistKey, setArtistKey] = useState<string | null>(null)
  const [albumKey, setAlbumKey] = useState<string | null>(null)
  const [settingsOpen, setSettingsOpen] = useState(false)
  const [queueOpen, setQueueOpen] = useState(true)
  const [loading, setLoading] = useState(true)
  const [error, setError] = useState('')
  const [context, setContext] = useState<ContextState>(null)
  const [status, setStatus] = useState('')
  const [defaultMusicDir, setDefaultMusicDir] = useState('')
  const [focusedIndex, setFocusedIndex] = useState(0)
  const [coverSrc, setCoverSrc] = useState<string | null>(null)
  const [playlists, setPlaylists] = useState<SavedPlaylist[]>([])
  const [shelfKind, setShelfKind] = useState<SmartShelfKind>('played_week')
  const [shelfTracks, setShelfTracks] = useState<Track[]>([])
  const [shelfLoading, setShelfLoading] = useState(false)
  const [focusMode, setFocusModeState] = useState(false)
  const [openDownload, setOpenDownload] = useState(false)
  const [tagTrack, setTagTrack] = useState<Track | null>(null)
  const prefsRef = useRef<DesktopPrefs>({})
  const listRef = useRef<HTMLDivElement>(null)
  const view = domain.current
  const tracks = view.library
  const queue = view.queue.flatMap(id => {
    const track = tracks.find(t => t.id === id)
    return track ? [track] : []
  })
  const playing = Boolean(view.current && !view.paused && !view.stopped)
  const favoriteIds = useMemo(() => new Set(view.favorites), [view.favorites])
  const folders = useMemo(
    () => view.settings.folders || view.settings.music_dirs || [],
    [view.settings.folders, view.settings.music_dirs],
  )
  const artistSource: ArtistSource = view.settings.artist_source === 'folder' ? 'folder' : 'metadata'

  const mergeLibraryTracks = (updates: Track[]) => {
    if (!updates.length) return
    const prev = domain.current
    const byId = new Map(updates.map(t => [t.id, t]))
    const library = prev.library.map(t => byId.get(t.id) ?? t)
    let current = prev.current
    if (current) {
      const enriched = byId.get(current.id)
      if (enriched) current = enriched
    }
    domain.current = { ...prev, library, current }
    repaint(n => n + 1)
  }
  const applyPlayback = (playback: PlaybackState) => {
    const prev = domain.current
    const structural =
      prev.current?.id !== playback.current?.id ||
      prev.paused !== playback.paused ||
      prev.stopped !== playback.stopped ||
      prev.volume !== playback.volume ||
      prev.muted !== playback.muted ||
      prev.eq !== playback.eq ||
      prev.loop_mode !== playback.loop_mode ||
      prev.shuffled !== playback.shuffled ||
      prev.speed !== playback.speed ||
      prev.pitch !== playback.pitch ||
      prev.queue.join('\0') !== playback.queue.join('\0') ||
      prev.favorites.join('\0') !== playback.favorites.join('\0')
    domain.current = { ...prev, ...playback, library: prev.library, settings: prev.settings }
    setClock({ position: playback.position, duration: playback.duration })
    if (structural) repaint(n => n + 1)
  }
  const hydrate = async () => {
    const snapshot = await invoke<Snapshot>('snapshot')
    domain.current = snapshot
    setClock({ position: snapshot.position, duration: snapshot.duration })
    repaint(n => n + 1)
    return snapshot
  }
  const refreshPlayback = async () => {
    const playback = await invoke<PlaybackState>('playback_state')
    applyPlayback(playback)
  }
  const command = async (name: string, args?: Record<string, unknown>) => {
    try {
      await invoke(name, args)
      if (PLAYBACK_ONLY_COMMANDS.has(name)) await refreshPlayback()
      else await hydrate()
      setError('')
    } catch (e) {
      const message = String(e).replace(/^Error:\s*/, '')
      setError(message)
      setStatus(message)
    }
  }

  const refreshPlaylists = async () => {
    if (!hasTauriBridge()) return
    try {
      const list = await invoke<SavedPlaylist[]>('list_playlists')
      setPlaylists(list)
    } catch (e) {
      setStatus(String(e).replace(/^Error:\s*/, ''))
    }
  }

  const loadShelf = async (kind: SmartShelfKind = shelfKind) => {
    if (!hasTauriBridge()) return
    setShelfLoading(true)
    try {
      const list = await invoke<Track[]>('smart_shelf', { kind })
      setShelfTracks(list)
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ''))
      setShelfTracks([])
    } finally {
      setShelfLoading(false)
    }
  }

  const loadDesktopPrefs = async () => {
    if (!hasTauriBridge()) return
    try {
      const raw = await invoke<{ settings?: DesktopPrefs }>('load_settings')
      const prefs = (raw.settings && typeof raw.settings === 'object') ? raw.settings : {}
      prefsRef.current = prefs
      if (typeof prefs.focusMode === 'boolean') setFocusModeState(prefs.focusMode)
    } catch { /* optional */ }
  }

  const persistDesktopPrefs = async (patch: DesktopPrefs) => {
    if (!hasTauriBridge()) return
    const { folders: _folders, favorites: _favorites, ...rest } = prefsRef.current
    const next = { ...rest, ...patch }
    prefsRef.current = next
    try {
      await invoke('save_settings', { settings: next })
    } catch (e) {
      setStatus(String(e).replace(/^Error:\s*/, ''))
    }
  }

  const setFocusMode = (value: boolean | ((prev: boolean) => boolean)) => {
    const next = typeof value === 'function' ? value(focusMode) : value
    setFocusModeState(next)
    void persistDesktopPrefs({ focusMode: next })
  }

  const artists = useMemo(() => {
    const map = new Map<string, ArtistGroup & { albums: Set<string> }>()
    for (const track of tracks) {
      const name = trackArtist(track, artistSource)
      const key = name.toLowerCase()
      const album = trackAlbum(track, artistSource)
      const existing = map.get(key)
      if (existing) {
        existing.count += 1
        existing.albums.add(album.toLowerCase())
        existing.albumCount = existing.albums.size
      } else {
        map.set(key, { key, name, count: 1, albumCount: 1, sample: track, albums: new Set([album.toLowerCase()]) })
      }
    }
    return [...map.values()]
      .map(({ albums: _a, ...rest }) => rest)
      .sort((a, b) => a.name.localeCompare(b.name))
  }, [tracks, artistSource])

  const albumsForArtist = useMemo(() => {
    if (!artistKey) return [] as AlbumGroup[]
    const map = new Map<string, AlbumGroup>()
    for (const track of tracks) {
      if (trackArtist(track, artistSource).toLowerCase() !== artistKey) continue
      const name = trackAlbum(track, artistSource)
      const key = name.toLowerCase()
      const existing = map.get(key)
      if (existing) existing.count += 1
      else map.set(key, { key, name, count: 1, sample: track })
    }
    return [...map.values()].sort((a, b) => a.name.localeCompare(b.name))
  }, [tracks, artistSource, artistKey])

  const recentTracks = useMemo(
    () => [...tracks].sort((a, b) => (b.mtime || 0) - (a.mtime || 0) || a.name.localeCompare(b.name)).slice(0, 5),
    [tracks],
  )

  const visible = useMemo(() => {
    if (page === 'shelves') return [...shelfTracks].sort((a, b) => a.name.localeCompare(b.name))
    let result = [...tracks]
    if (page === 'favorites') result = result.filter(t => favoriteIds.has(t.id))
    if (page === 'artists' && artistKey) {
      result = result.filter(t => trackArtist(t, artistSource).toLowerCase() === artistKey)
      if (albumKey) result = result.filter(t => trackAlbum(t, artistSource).toLowerCase() === albumKey)
    }
    if (page === 'playlists') result = []
    if (page === 'artists' && !artistKey) return []
    return result.sort((a, b) => a.name.localeCompare(b.name))
  }, [tracks, page, favoriteIds, artistKey, albumKey, artistSource, shelfTracks])

  const commandHits = useMemo(() => {
    if (!search.trim()) return tracks.slice(0, 12)
    const q = search.toLowerCase()
    return tracks
      .filter(t => `${t.name} ${t.artist} ${t.album} ${t.path} ${t.folder}`.toLowerCase().includes(q))
      .sort((a, b) => a.name.localeCompare(b.name))
      .slice(0, 40)
  }, [tracks, search])

  const commandHitsRef = useRef(commandHits)
  const visibleRef = useRef(visible)
  const focusedIndexRef = useRef(focusedIndex)
  const focusModeRef = useRef(focusMode)
  useEffect(() => { commandHitsRef.current = commandHits }, [commandHits])
  useEffect(() => { visibleRef.current = visible }, [visible])
  useEffect(() => { focusedIndexRef.current = focusedIndex }, [focusedIndex])
  useEffect(() => { focusModeRef.current = focusMode }, [focusMode])

  const play = (track: Track) => { setContext(null); setCommandOpen(false); setSearch(''); void command('play_track', { id: track.id }) }
  const openCommand = () => { setCommandOpen(true); setCommandIndex(0); setContext(null) }
  const closeCommand = () => { setCommandOpen(false); setSearch(''); setCommandIndex(0) }
  const goPage = (next: Page) => { setPage(next); setArtistKey(null); setAlbumKey(null); setContext(null) }
  const setArtistMode = async (source: ArtistSource) => {
    await command('set_artist_source', { source })
    setArtistKey(null)
    setAlbumKey(null)
  }
  const next = () => void command('next')
  const previous = () => void command('previous')
  const toggle = () => {
    if (!hasTauriBridge()) { setStatus('Browser preview: playback requires the Tauri desktop app (`bun run tauri:dev`).'); return }
    const sequence = visibleRef.current.length ? visibleRef.current : domain.current.library
    if (!domain.current.current && sequence[0]) play(sequence[0])
    else void command('toggle_pause')
  }
  const actionsRef = useRef({ play, next, previous, toggle })
  useEffect(() => { actionsRef.current = { play, next, previous, toggle } })

  useEffect(() => {
    const onClick = () => setContext(null)
    const key = (e: KeyboardEvent) => {
      const target = e.target as HTMLElement | null
      const typing = target && (target.tagName === 'INPUT' || target.tagName === 'TEXTAREA' || target.tagName === 'SELECT' || target.isContentEditable)
      if ((e.metaKey || e.ctrlKey) && e.key === 'k') {
        e.preventDefault()
        if (commandOpen) closeCommand()
        else openCommand()
        return
      }
      if (e.key === 'Escape') {
        if (commandOpen) { closeCommand(); return }
        if (settingsOpen) { setSettingsOpen(false); return }
        if (openDownload) { setOpenDownload(false); return }
        if (tagTrack) { setTagTrack(null); return }
        if (focusModeRef.current) {
          focusModeRef.current = false
          setFocusModeState(false)
          void persistDesktopPrefs({ focusMode: false })
          return
        }
        setContext(null)
        return
      }
      if (commandOpen) {
        if (e.key === 'ArrowDown') {
          e.preventDefault()
          setCommandIndex(i => Math.min(i + 1, Math.max(commandHitsRef.current.length - 1, 0)))
          return
        }
        if (e.key === 'ArrowUp') {
          e.preventDefault()
          setCommandIndex(i => Math.max(i - 1, 0))
          return
        }
        if (e.key === 'Enter') {
          const track = commandHitsRef.current[commandIndex]
          if (track) { e.preventDefault(); play(track) }
          return
        }
        return
      }
      if (typing) return
      const actions = actionsRef.current
      if (e.key === ' ' || e.code === 'Space') { e.preventDefault(); actions.toggle() }
      else if (e.key === 'ArrowRight') { e.preventDefault(); actions.next() }
      else if (e.key === 'ArrowLeft') { e.preventDefault(); actions.previous() }
      else if (e.key === 'ArrowDown') { e.preventDefault(); setFocusedIndex(i => Math.min(i + 1, Math.max(visibleRef.current.length - 1, 0))) }
      else if (e.key === 'ArrowUp') { e.preventDefault(); setFocusedIndex(i => Math.max(i - 1, 0)) }
      else if (e.key === 'Enter') {
        const track = visibleRef.current[focusedIndexRef.current]
        if (track) { e.preventDefault(); actions.play(track) }
      }
    }
    window.addEventListener('click', onClick)
    window.addEventListener('keydown', key)
    return () => { window.removeEventListener('click', onClick); window.removeEventListener('keydown', key) }
  }, [commandOpen, commandIndex, settingsOpen, openDownload, tagTrack])

  useEffect(() => { setCommandIndex(0) }, [search, commandOpen])

  useEffect(() => {
    if (!hasTauriBridge()) { setLoading(false); setStatus('Browser preview: playback requires the Tauri desktop app (`bun run tauri:dev`).'); return }
    let active = true
    const start = async () => {
      try {
        await hydrate()
        await loadDesktopPrefs()
        try { const dir = await invoke<string>('default_music_directory'); if (active) setDefaultMusicDir(dir) } catch { /* optional */ }
        const dirs = domain.current.settings.folders || domain.current.settings.music_dirs || []
        await invoke('scan_music_directories', { paths: [...new Set(dirs)] })
        if (active) {
          await hydrate()
          try {
            const restored = await invoke<boolean>('restore_session')
            if (restored) await hydrate()
          } catch { /* optional */ }
          await refreshPlaylists()
          setStatus(domain.current.library.length ? `${domain.current.library.length} tracks` : 'Library empty')
        }
      } catch (e) {
        if (active) setError(`Desktop library scan failed: ${String(e).replace(/^Error:\s*/, '')}`)
      } finally {
        if (active) setLoading(false)
      }
    }
    let unlistenState: (() => void) | undefined
    let unlistenLibrary: (() => void) | undefined
    listen<PlaybackState>('optionmusic://state', event => { if (active) applyPlayback(event.payload) }).then(fn => { unlistenState = fn })
    listen<LibraryEnrichUpdate>('optionmusic://library-enriched', event => {
      if (!active) return
      const { tracks: updates, done } = event.payload
      mergeLibraryTracks(updates)
      if (done) setStatus(`${domain.current.library.length} tracks`)
    }).then(fn => { unlistenLibrary = fn })
    start()
    return () => { active = false; unlistenState?.(); unlistenLibrary?.() }
  }, [])

  useEffect(() => {
    if (page !== 'shelves' || !hasTauriBridge()) return
    let cancelled = false
    setShelfLoading(true)
    invoke<Track[]>('smart_shelf', { kind: shelfKind })
      .then(list => {
        if (cancelled) return
        setShelfTracks(list)
      })
      .catch(e => {
        if (cancelled) return
        setError(String(e).replace(/^Error:\s*/, ''))
        setShelfTracks([])
      })
      .finally(() => {
        if (!cancelled) setShelfLoading(false)
      })
    return () => { cancelled = true }
  }, [page, shelfKind])

  const loadLibrary = async () => {
    if (!hasTauriBridge()) { setStatus('Browser preview: playback requires the Tauri desktop app (`bun run tauri:dev`).'); return }
    setLoading(true); setError('')
    try {
      const dirs = domain.current.settings.folders || domain.current.settings.music_dirs || []
      await invoke('scan_music_directories', { paths: [...new Set(dirs)] })
      await hydrate()
      await refreshPlaylists()
      if (page === 'shelves') await loadShelf()
      setStatus(`${domain.current.library.length} tracks`)
    } catch (e) {
      setError(`Desktop library scan failed: ${String(e).replace(/^Error:\s*/, '')}`)
    } finally { setLoading(false) }
  }

  const addFolder = async () => {
    if (!hasTauriBridge()) { setStatus('Browser preview: playback requires the Tauri desktop app (`bun run tauri:dev`).'); return }
    setLoading(true); setError(''); setStatus('')
    try {
      const selected = await open({ directory: true, multiple: false, title: 'Add a music folder' })
      if (typeof selected !== 'string') return
      const dirs = domain.current.settings.folders || domain.current.settings.music_dirs || []
      await invoke('scan_music_directories', { paths: [...new Set([...dirs, selected])] })
      await hydrate()
      setStatus(`${domain.current.library.length} tracks`)
    } catch (e) {
      const message = `Could not add music folder: ${String(e).replace(/^Error:\s*/, '')}`
      setError(message)
      setStatus(message)
    } finally { setLoading(false) }
  }

  const createPlaylist = async () => {
    const name = window.prompt('Playlist name')
    if (!name?.trim()) return
    try {
      await invoke('create_playlist', { name: name.trim() })
      await refreshPlaylists()
      setStatus(`Created “${name.trim()}”`)
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ''))
    }
  }

  const importM3u = async () => {
    if (!hasTauriBridge()) return
    try {
      const selected = await open({
        multiple: false,
        title: 'Import M3U playlist',
        filters: [{ name: 'Playlist', extensions: ['m3u', 'm3u8'] }],
      })
      if (typeof selected !== 'string') return
      await invoke('import_m3u', { path: selected, name: null })
      await refreshPlaylists()
      setStatus('Playlist imported')
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ''))
    }
  }

  const playPlaylist = (id: string) => {
    void command('play_playlist', { id })
  }

  const addToPlaylist = async (track: Track, playlistId?: string) => {
    setContext(null)
    let id = playlistId
    if (!id) {
      if (!playlists.length) {
        const name = window.prompt('Create playlist for this track')
        if (!name?.trim()) return
        try {
          const created = await invoke<SavedPlaylist>('create_playlist', { name: name.trim() })
          id = created.id
          await refreshPlaylists()
        } catch (e) {
          setError(String(e).replace(/^Error:\s*/, ''))
          return
        }
      } else if (playlists.length === 1) {
        id = playlists[0].id
      } else {
        const names = playlists.map((p, i) => `${i + 1}. ${p.name}`).join('\n')
        const pick = window.prompt(`Add to playlist:\n${names}\n\nEnter number or name`)
        if (!pick?.trim()) return
        const asNum = Number(pick.trim())
        if (Number.isFinite(asNum) && playlists[asNum - 1]) id = playlists[asNum - 1].id
        else {
          const match = playlists.find(p => p.name.toLowerCase() === pick.trim().toLowerCase())
          if (!match) { setStatus('Playlist not found'); return }
          id = match.id
        }
      }
    }
    try {
      await invoke('playlist_add', { id, trackId: track.id })
      await refreshPlaylists()
      setStatus('Added to playlist')
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ''))
    }
  }

  const setSpeed = (speed: number) => void command('set_speed', { speed })
  const setPitch = (pitch: number) => void command('set_pitch', { pitch })
  const resetSpeedPitch = () => void command('reset_speed_pitch')
  const setReplaygain = (mode: ReplayGainMode) => void command('set_replaygain', { mode })

  const openTagEditor = (track: Track) => {
    setContext(null)
    setTagTrack(track)
  }

  useEffect(() => { setFocusedIndex(0) }, [page, search, tracks.length, artistKey, albumKey, shelfKind, shelfTracks.length])
  useEffect(() => {
    const container = listRef.current
    if (!container) return
    const row = container.querySelector<HTMLElement>(`[data-track-index="${focusedIndex}"]`)
    if (row) {
      row.scrollIntoView({ block: 'nearest' })
      const active = document.activeElement
      if (active?.closest?.('.catalog-list') || active === document.body || active === document.documentElement) {
        row.focus({ preventScroll: true })
      }
      return
    }
    const rowHeight = 58
    const targetTop = focusedIndex * rowHeight
    const viewBottom = container.scrollTop + container.clientHeight
    if (targetTop < container.scrollTop) container.scrollTop = targetTop
    else if (targetTop + rowHeight > viewBottom) container.scrollTop = targetTop + rowHeight - container.clientHeight
  }, [focusedIndex])

  useEffect(() => {
    const id = view.current?.id
    if (!id || !hasTauriBridge()) {
      setCoverSrc(null)
      return
    }
    let cancelled = false
    setCoverSrc(null)
    fetchTrackCover(id)
      .then(url => { if (!cancelled) setCoverSrc(url) })
      .catch(() => { if (!cancelled) setCoverSrc(null) })
    return () => { cancelled = true }
  }, [view.current?.id])

  const addQueue = (t: Track) => { setContext(null); setQueueOpen(true); void command('queue_add', { id: t.id }) }
  const playNext = (t: Track) => { setContext(null); setQueueOpen(true); void command('queue_play_next', { id: t.id }) }
  const toggleFavorite = (t: Track) => { setContext(null); void command('toggle_favorite', { id: t.id }) }
  const selectedArtist = artistKey ? artists.find(a => a.key === artistKey) : null
  const selectedAlbum = albumKey ? albumsForArtist.find(a => a.key === albumKey) : null
  const shelfTitle =
    shelfKind === 'played_week' ? 'Played this week'
    : shelfKind === 'no_cover' ? 'No cover'
    : 'Incomplete albums'
  const pageTitle =
    page === 'library' ? 'Library'
    : page === 'artists' ? (selectedAlbum?.name || selectedArtist?.name || 'Artists')
    : page === 'playlists' ? 'Playlists'
    : page === 'shelves' ? shelfTitle
    : 'Favorites'
  const locationCount = useMemo(() => {
    const set = new Set(folders.map(f => f.replace(/\/+$/, '')))
    if (defaultMusicDir) set.add(defaultMusicDir.replace(/\/+$/, ''))
    else if (!folders.length) set.add('~/Music')
    return set.size || 1
  }, [folders, defaultMusicDir])

  const openContext = (e: React.MouseEvent, t: Track) => {
    e.preventDefault()
    e.stopPropagation()
    setContext({ track: t, x: Math.min(e.clientX, window.innerWidth - 240), y: Math.min(e.clientY, window.innerHeight - 280) })
  }

  const progress = clock.duration && clock.duration > 0 ? (clock.position / clock.duration) * 100 : 0
  const favorited = Boolean(view.current && favoriteIds.has(view.current.id))

  return {
    view,
    tracks,
    queue,
    playing,
    favoriteIds,
    folders,
    artistSource,
    artists,
    albumsForArtist,
    recentTracks,
    visible,
    commandHits,
    clock,
    page,
    search,
    setSearch,
    commandOpen,
    commandIndex,
    setCommandIndex,
    artistKey,
    setArtistKey,
    albumKey,
    setAlbumKey,
    settingsOpen,
    setSettingsOpen,
    queueOpen,
    setQueueOpen,
    loading,
    error,
    setError,
    context,
    status,
    defaultMusicDir,
    focusedIndex,
    setFocusedIndex,
    coverSrc,
    listRef,
    pageTitle,
    locationCount,
    progress,
    favorited,
    selectedArtistName: selectedArtist?.name,
    playlists,
    shelfKind,
    setShelfKind,
    shelfLoading,
    focusMode,
    setFocusMode,
    openDownload,
    setOpenDownload,
    tagTrack,
    setTagTrack,
    play,
    openCommand,
    closeCommand,
    goPage,
    setArtistMode,
    next,
    previous,
    toggle,
    loadLibrary,
    addFolder,
    addQueue,
    playNext,
    toggleFavorite,
    openContext,
    command,
    createPlaylist,
    importM3u,
    playPlaylist,
    addToPlaylist,
    setSpeed,
    setPitch,
    resetSpeedPitch,
    setReplaygain,
    openTagEditor,
    refreshPlaylists,
  }
}
