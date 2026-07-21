import React, { useEffect, useMemo, useRef, useState } from 'react'
import { createRoot } from 'react-dom/client'
import { invoke, isTauri } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { getCurrentWindow } from '@tauri-apps/api/window'
import { open } from '@tauri-apps/plugin-dialog'
import { BookMarked, Disc3, FolderOpen, Heart, ListMusic, Mic2, Music2, Pause, Play, Plus, RefreshCw, Repeat, Repeat1, Search, Settings, Shuffle, SkipBack, SkipForward, SlidersHorizontal, Volume2, VolumeX, X, ExternalLink, Minus } from 'lucide-react'
import './styles.css'

type Track = { id: string; name: string; path: string; folder: string; artist: string; album: string; mtime: number }
type ArtistSource = 'metadata' | 'folder'
type BackendSettings = { excess_volume: boolean; ldm: boolean; accent: string; artist_source?: ArtistSource; cava: { enabled?: boolean; style?: string }; folders?: string[]; music_dirs?: string[]; resume_track?: string; resume_position?: number; resume_queue?: string[] }
type LoopMode = 'off' | 'list' | 'track'
type PlaybackState = { queue: string[]; current: Track | null; position: number; duration: number | null; paused: boolean; stopped: boolean; volume: number; muted: boolean; speed: number; pitch: number; eq: string; favorites: string[]; loop_mode: LoopMode; shuffled: boolean }
type Snapshot = PlaybackState & { library: Track[]; settings: BackendSettings }
type Page = 'library' | 'artists' | 'playlists' | 'favorites'
type ContextState = { track: Track; x: number; y: number } | null
type ArtistGroup = { key: string; name: string; count: number; albumCount: number; sample: Track }
type AlbumGroup = { key: string; name: string; count: number; sample: Track }

const emptySnapshot: Snapshot = { library: [], queue: [], current: null, position: 0, duration: null, paused: true, stopped: true, volume: 80, muted: false, speed: 1, pitch: 0, eq: 'Default', favorites: [], loop_mode: 'off', shuffled: false, settings: { excess_volume: false, ldm: false, accent: 'default', artist_source: 'metadata', cava: {} } }
const hasTauriBridge = () => isTauri() || Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)
const isApple = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
const searchChord = isApple ? '⌘K' : 'Ctrl+K'
const folderLabel = (folder: string) => {
  const parts = folder.replace(/\\/g, '/').split('/').filter(Boolean)
  return parts[parts.length - 1] || folder || '—'
}
const coverGlyph = (name: string) => (name.trim().charAt(0) || 'o').toUpperCase()
const formatTime = (n: number) => `${Math.floor(n / 60)}:${String(Math.floor(n % 60)).padStart(2, '0')}`
const trackArtist = (t: Track, source: ArtistSource) => {
  if (source === 'folder') return folderLabel(t.folder) || 'Unknown Artist'
  return (t.artist || '').trim() || 'Unknown Artist'
}
const trackAlbum = (t: Track, source: ArtistSource) => {
  if (source === 'folder') return folderLabel(t.folder) || 'Unknown Album'
  return (t.album || '').trim() || 'Unknown Album'
}
const trackMeta = (t: Track) => (t.artist || '').trim() || folderLabel(t.folder)

function App() {
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
  const listRef = useRef<HTMLDivElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)
  const view = domain.current
  const tracks = view.library
  const queue = view.queue.map(id => tracks.find(track => track.id === id)).filter(Boolean) as Track[]
  const playing = Boolean(view.current && !view.paused && !view.stopped)

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
  const command = async (name: string, args?: Record<string, unknown>) => {
    try {
      await invoke(name, args)
      await hydrate()
      setError('')
    } catch (e) {
      const message = String(e).replace(/^Error:\s*/, '')
      setError(message)
      setStatus(message)
    }
  }

  const artistSource: ArtistSource = view.settings.artist_source === 'folder' ? 'folder' : 'metadata'

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
    let result = [...tracks]
    if (page === 'favorites') result = result.filter(t => view.favorites.includes(t.id))
    if (page === 'artists' && artistKey) {
      result = result.filter(t => trackArtist(t, artistSource).toLowerCase() === artistKey)
      if (albumKey) result = result.filter(t => trackAlbum(t, artistSource).toLowerCase() === albumKey)
    }
    if (page === 'playlists') result = []
    if (page === 'artists' && !artistKey) return []
    return result.sort((a, b) => a.name.localeCompare(b.name))
  }, [tracks, page, view.favorites, artistKey, albumKey, artistSource])

  const commandHits = useMemo(() => {
    if (!search.trim()) return tracks.slice(0, 12)
    const q = search.toLowerCase()
    return tracks
      .filter(t => `${t.name} ${t.artist} ${t.album} ${t.path} ${t.folder}`.toLowerCase().includes(q))
      .sort((a, b) => a.name.localeCompare(b.name))
      .slice(0, 40)
  }, [tracks, search])
  const commandHitsRef = useRef(commandHits)
  commandHitsRef.current = commandHits
  const visibleRef = useRef(visible)
  const focusedIndexRef = useRef(focusedIndex)
  visibleRef.current = visible
  focusedIndexRef.current = focusedIndex

  const play = (track: Track) => { setContext(null); setCommandOpen(false); setSearch(''); void command('play_track', { id: track.id }) }
  const openCommand = () => {
    setCommandOpen(true)
    setCommandIndex(0)
    setContext(null)
    requestAnimationFrame(() => searchRef.current?.focus())
  }
  const closeCommand = () => {
    setCommandOpen(false)
    setSearch('')
    setCommandIndex(0)
  }
  const goPage = (next: Page) => {
    setPage(next)
    setArtistKey(null)
    setAlbumKey(null)
    setContext(null)
  }
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
  actionsRef.current = { play, next, previous, toggle }

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
        setContext(null)
        setSettingsOpen(false)
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
  }, [commandOpen, commandIndex])

  useEffect(() => { setCommandIndex(0) }, [search, commandOpen])

  useEffect(() => {
    if (!hasTauriBridge()) { setLoading(false); setStatus('Browser preview: playback requires the Tauri desktop app (`bun run tauri:dev`).'); return }
    let active = true
    const start = async () => {
      try {
        await hydrate()
        try { const dir = await invoke<string>('default_music_directory'); if (active) setDefaultMusicDir(dir) } catch { /* optional */ }
        const folders = domain.current.settings.folders || domain.current.settings.music_dirs || []
        await invoke('scan_music_directories', { paths: [...new Set(folders)] })
        if (active) {
          await hydrate()
          try {
            const restored = await invoke<boolean>('restore_session')
            if (restored) await hydrate()
          } catch { /* optional */ }
          setStatus(domain.current.library.length ? `${domain.current.library.length} tracks` : 'Library empty')
        }
      } catch (e) {
        if (active) setError(`Desktop library scan failed: ${String(e).replace(/^Error:\s*/, '')}`)
      } finally {
        if (active) setLoading(false)
      }
    }
    let unlisten: (() => void) | undefined
    listen<PlaybackState>('optmusic://state', event => { if (active) applyPlayback(event.payload) }).then(fn => { unlisten = fn })
    start()
    return () => { active = false; unlisten?.() }
  }, [])

  const loadLibrary = async () => {
    if (!hasTauriBridge()) { setStatus('Browser preview: playback requires the Tauri desktop app (`bun run tauri:dev`).'); return }
    setLoading(true); setError('')
    try {
      const folders = domain.current.settings.folders || domain.current.settings.music_dirs || []
      await invoke('scan_music_directories', { paths: [...new Set(folders)] })
      await hydrate()
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
      const folders = domain.current.settings.folders || domain.current.settings.music_dirs || []
      await invoke('scan_music_directories', { paths: [...new Set([...folders, selected])] })
      await hydrate()
      setStatus(`${domain.current.library.length} tracks`)
    } catch (e) {
      const message = `Could not add music folder: ${String(e).replace(/^Error:\s*/, '')}`
      setError(message)
      setStatus(message)
    } finally { setLoading(false) }
  }

  useEffect(() => { setFocusedIndex(0) }, [page, search, tracks.length, artistKey, albumKey])
  useEffect(() => {
    const row = listRef.current?.querySelector<HTMLElement>(`[data-track-index="${focusedIndex}"]`)
    if (!row) return
    row.scrollIntoView({ block: 'nearest' })
    const active = document.activeElement
    if (active?.closest?.('.catalog-list') || active === document.body || active === document.documentElement) {
      row.focus({ preventScroll: true })
    }
  }, [focusedIndex])

  useEffect(() => {
    const id = view.current?.id
    if (!id || !hasTauriBridge()) {
      setCoverSrc(null)
      return
    }
    let cancelled = false
    setCoverSrc(null)
    invoke<string | null>('track_cover', { id })
      .then(url => { if (!cancelled) setCoverSrc(url) })
      .catch(() => { if (!cancelled) setCoverSrc(null) })
    return () => { cancelled = true }
  }, [view.current?.id])

  const addQueue = (t: Track) => { setContext(null); setQueueOpen(true); void command('queue_add', { id: t.id }) }
  const playNext = (t: Track) => { setContext(null); setQueueOpen(true); void command('queue_play_next', { id: t.id }) }
  const toggleFavorite = (t: Track) => { setContext(null); void command('toggle_favorite', { id: t.id }) }
  const selectedArtist = artistKey ? artists.find(a => a.key === artistKey) : null
  const selectedAlbum = albumKey ? albumsForArtist.find(a => a.key === albumKey) : null
  const pageTitle =
    page === 'library' ? 'Library'
    : page === 'artists' ? (selectedAlbum?.name || selectedArtist?.name || 'Artists')
    : page === 'playlists' ? 'Playlists'
    : 'Favorites'
  const folders = view.settings.folders || view.settings.music_dirs || []
  const locationCount = useMemo(() => {
    const set = new Set(folders.map(f => f.replace(/\/+$/, '')))
    if (defaultMusicDir) set.add(defaultMusicDir.replace(/\/+$/, ''))
    else if (!folders.length) set.add('~/Music')
    return set.size || 1
  }, [folders, defaultMusicDir])

  const openContext = (e: React.MouseEvent, t: Track) => {
    e.preventDefault()
    e.stopPropagation()
    setContext({ track: t, x: Math.min(e.clientX, window.innerWidth - 240), y: Math.min(e.clientY, window.innerHeight - 240) })
  }

  const progress = clock.duration && clock.duration > 0 ? (clock.position / clock.duration) * 100 : 0
  const trackKey = view.current?.id ?? 'idle'
  const ldm = Boolean(view.settings.ldm)
  const showTrackList = page !== 'playlists' && !(page === 'artists' && !artistKey)
  const showArtistAlbums = page === 'artists' && !!artistKey && !albumKey

  return (
    <div className={`shell${ldm ? ' ldm' : ''}`}>
      <aside
        className="sidebar"
        onMouseDown={e => {
          if (!hasTauriBridge()) return
          const t = e.target as HTMLElement
          if (t.closest('button, input, a, .traffic, .side-nav, .side-recent, .side-actions')) return
          if (e.buttons === 1) {
            const win = getCurrentWindow()
            if (e.detail === 2) void win.toggleMaximize()
            else void win.startDragging()
          }
        }}
      >
        <div className="side-top">
          <WindowControls />
          <div className="identity">
            <span className="mark">o</span>
            <strong>optMusic</strong>
          </div>
        </div>

        <nav className="side-nav" aria-label="Library views">
          <button type="button" className={page === 'library' ? 'side-link on' : 'side-link'} onClick={() => goPage('library')}>
            <BookMarked size={16} strokeWidth={1.75} />
            <span>Library</span>
            <em>{tracks.length}</em>
          </button>
          <button type="button" className={page === 'artists' ? 'side-link on' : 'side-link'} onClick={() => goPage('artists')}>
            <Mic2 size={16} strokeWidth={1.75} />
            <span>Artists</span>
            <em>{artists.length}</em>
          </button>
          <button type="button" className={page === 'playlists' ? 'side-link on' : 'side-link'} onClick={() => goPage('playlists')}>
            <ListMusic size={16} strokeWidth={1.75} />
            <span>Playlists</span>
            <em>0</em>
          </button>
          <button type="button" className={page === 'favorites' ? 'side-link on' : 'side-link'} onClick={() => goPage('favorites')}>
            <Heart size={16} strokeWidth={1.75} />
            <span>Favorites</span>
            <em>{view.favorites.length}</em>
          </button>
        </nav>

        <div className="side-recent">
          <p className="side-label">Recent</p>
          {recentTracks.length ? recentTracks.map(t => (
            <button
              type="button"
              key={t.id}
              className={`side-track${view.current?.id === t.id ? ' current' : ''}`}
              onClick={() => play(t)}
              title={t.name}
            >
              <span className="side-thumb" aria-hidden="true">{coverGlyph(t.name)}</span>
              <span className="side-track-copy">
                <strong>{t.name}</strong>
                <small>{trackMeta(t)}</small>
              </span>
            </button>
          )) : (
            <p className="side-empty">No recent tracks yet</p>
          )}
        </div>

        <div className="side-actions">
          <button type="button" className="side-search" onClick={openCommand}>
            <Search size={15} strokeWidth={1.75} />
            <span>Search</span>
            <kbd>{searchChord}</kbd>
          </button>
          <button type="button" className="open-files" onClick={() => void addFolder()}>
            <Plus size={15} strokeWidth={2.25} />
            Open files
          </button>
          <button type="button" className="side-settings" onClick={() => setSettingsOpen(true)}>
            <Settings size={15} strokeWidth={1.75} />
            Settings
          </button>
        </div>
      </aside>

      <div className="workspace">
        <div className={`frame ${queueOpen ? 'with-queue' : ''}`}>
          <section className={`stage${playing ? ' is-live' : ''}${view.current ? ' has-track' : ''}`} aria-label="Now playing">
            <div key={trackKey} className={`stage-hero swap ${coverSrc ? 'has-cover' : ''}`}>
              <div className="stage-hero-art" aria-hidden="true">
                {coverSrc ? <img src={coverSrc} alt="" /> : <span>{view.current ? coverGlyph(view.current.name) : 'o'}</span>}
              </div>
              <div className="stage-hero-shade" aria-hidden="true" />
              {view.current && (
                <button
                  type="button"
                  className="stage-hero-play"
                  aria-label={playing ? 'Pause' : 'Play'}
                  onClick={toggle}
                >
                  {playing ? <Pause size={22} fill="currentColor" /> : <Play size={22} fill="currentColor" />}
                </button>
              )}
              {playing && <div className="stage-hero-pulse" aria-hidden="true" />}
            </div>

            <div key={`copy-${trackKey}`} className="stage-body swap">
              {view.current ? (
                <>
                  <div className="stage-status">
                    <span className={playing ? 'live-dot' : 'idle-dot'} />
                    <span>{playing ? 'Playing' : 'Paused'}</span>
                    {view.favorites.includes(view.current.id) && <span className="stage-chip">Liked</span>}
                  </div>
                  <h1 title={view.current.name}>{view.current.name}</h1>
                  <p className="stage-artist">{trackMeta(view.current)}</p>
                  {view.current.album && <p className="stage-album">{view.current.album}</p>}

                  <div className="stage-scrub" aria-hidden={!clock.duration}>
                    <div className="stage-scrub-track">
                      <div className="stage-scrub-fill" style={{ '--progress': progress / 100 } as React.CSSProperties} />
                      <input
                        aria-label="Seek"
                        type="range"
                        min="0"
                        max={clock.duration || 1}
                        step="0.1"
                        value={clock.position}
                        onChange={e => { void command('seek', { seconds: Number(e.target.value) }) }}
                      />
                    </div>
                    <div className="stage-scrub-times">
                      <span>{formatTime(clock.position)}</span>
                      <span>{formatTime(clock.duration || 0)}</span>
                    </div>
                  </div>

                  <div className="stage-actions">
                    <button
                      type="button"
                      className={view.favorites.includes(view.current.id) ? 'stage-action liked' : 'stage-action'}
                      aria-label={view.favorites.includes(view.current.id) ? 'Remove favorite' : 'Add favorite'}
                      onClick={() => toggleFavorite(view.current!)}
                    >
                      <Heart size={15} fill={view.favorites.includes(view.current.id) ? 'currentColor' : 'none'} />
                      {view.favorites.includes(view.current.id) ? 'Liked' : 'Like'}
                    </button>
                    <button
                      type="button"
                      className={queueOpen ? 'stage-action on' : 'stage-action'}
                      aria-pressed={queueOpen}
                      onClick={() => setQueueOpen(o => !o)}
                    >
                      <ListMusic size={15} />
                      Queue{queue.length > 0 ? ` · ${queue.length}` : ''}
                    </button>
                  </div>
                </>
              ) : (
                <div className="stage-empty">
                  <p className="stage-status"><span className="idle-dot" />Idle</p>
                  <h1>Nothing playing</h1>
                  <p className="stage-artist">Choose a track from the library.</p>
                </div>
              )}
            </div>

            {queueOpen && (
              <div className="queue-block">
                <div className="queue-head">
                  <span>Up next</span>
                  <b>{queue.length}</b>
                  <button className="ghost tiny" aria-label="Hide queue" onClick={() => setQueueOpen(false)}><X size={14} /></button>
                </div>
                <div className="queue-scroll">
                  {queue.length ? queue.map((t, i) => (
                    <div className={`queue-item ${view.current?.id === t.id ? 'current' : ''}`} key={t.id} style={{ '--i': i } as React.CSSProperties}>
                      <span>{String(i + 1).padStart(2, '0')}</span>
                      <button onClick={() => play(t)}>
                        <strong>{t.name}</strong>
                        <small>{trackMeta(t)}</small>
                      </button>
                      <button aria-label={`Remove ${t.name}`} onClick={() => { void command('queue_remove', { id: t.id }) }}><X size={13} /></button>
                    </div>
                  )) : <p className="queue-empty">Queue is empty — add with + on any track.</p>}
                </div>
              </div>
            )}
          </section>

          <main className="catalog">
            <div className="catalog-head swap" key={`${page}-${artistKey || ''}-${albumKey || ''}`}>
              <div>
                {page === 'artists' && artistKey && (
                  <button
                    type="button"
                    className="crumb"
                    onClick={() => {
                      if (albumKey) setAlbumKey(null)
                      else setArtistKey(null)
                    }}
                  >
                    ← {albumKey ? selectedArtist?.name || 'Artist' : 'Artists'}
                  </button>
                )}
                <h2>{pageTitle}</h2>
                <p>
                  {loading ? 'Scanning…'
                    : page === 'artists' && !artistKey ? `${artists.length} artist${artists.length === 1 ? '' : 's'} · ${artistSource}`
                    : page === 'artists' && artistKey && !albumKey ? `${albumsForArtist.length} album${albumsForArtist.length === 1 ? '' : 's'} · ${visible.length} tracks`
                    : page === 'playlists' ? 'No playlists yet'
                    : status || `${visible.length} of ${tracks.length} tracks · ${locationCount} folder${locationCount === 1 ? '' : 's'}`}
                </p>
              </div>
              <div className="catalog-tools">
                {page === 'artists' && (
                  <div className="mode-toggle" role="group" aria-label="Artist grouping">
                    <button type="button" className={artistSource === 'metadata' ? 'on' : ''} onClick={() => void setArtistMode('metadata')}>Metadata</button>
                    <button type="button" className={artistSource === 'folder' ? 'on' : ''} onClick={() => void setArtistMode('folder')}>Folder</button>
                  </div>
                )}
                <button type="button" className="ghost" aria-label="Search" onClick={openCommand}><Search size={16} /></button>
                <button className={queueOpen ? 'ghost on' : 'ghost'} aria-label={queueOpen ? 'Hide queue' : 'Show queue'} aria-pressed={queueOpen} onClick={() => setQueueOpen(o => !o)}>
                  <ListMusic size={16} />
                  {queue.length > 0 && <span className="dot">{queue.length}</span>}
                </button>
                <button className="ghost" aria-label="Refresh library" onClick={loadLibrary}><RefreshCw size={15} className={loading ? 'spin' : ''} /></button>
              </div>
            </div>

            {error && (
              <div className="banner" role="alert">
                {error}
                <button type="button" aria-label="Dismiss error" onClick={() => setError('')}><X size={14} /></button>
              </div>
            )}

            {error && !tracks.length ? (
              <Empty icon={<FolderOpen size={28} />} title="Couldn’t open a music folder" text={error}>
                <button className="primary" onClick={loadLibrary}><RefreshCw size={15} />Try again</button>
                <button className="secondary" onClick={addFolder}>Choose folder</button>
              </Empty>
            ) : loading ? (
              <Empty icon={<RefreshCw className="spin" size={24} />} title="Scanning your library" text="Looking through your music folders…" />
            ) : !tracks.length ? (
              <Empty icon={<Music2 size={28} />} title="Add a music folder to start" text="Drop audio into ~/Music, or choose another folder.">
                <button className="primary" onClick={addFolder}><Plus size={16} />Add music folder</button>
              </Empty>
            ) : page === 'playlists' ? (
              <Empty icon={<ListMusic size={28} />} title="No playlists yet" text="Playlists aren’t in optMusic yet — use Favorites or the queue for now." />
            ) : page === 'artists' && !artistKey ? (
              artists.length ? (
                <div className="artist-grid" role="list" aria-label="Artists">
                  {artists.map((a, i) => (
                    <button
                      type="button"
                      key={a.key}
                      className="artist-card"
                      style={{ '--i': i } as React.CSSProperties}
                      onClick={() => { setArtistKey(a.key); setAlbumKey(null) }}
                    >
                      <CoverThumb trackId={a.sample.id} label={a.name} className="artist-glyph" />
                      <strong>{a.name}</strong>
                      <small>{a.albumCount} album{a.albumCount === 1 ? '' : 's'} · {a.count} track{a.count === 1 ? '' : 's'}</small>
                    </button>
                  ))}
                </div>
              ) : (
                <Empty icon={<Mic2 size={24} />} title="No artists found" text={artistSource === 'metadata' ? 'No artist tags in this library — try Folder mode.' : 'Artists are grouped from your folder names.'} />
              )
            ) : showArtistAlbums ? (
              <div className="artist-detail">
                {albumsForArtist.length > 0 && (
                  <section className="album-section">
                    <h3 className="section-label">Albums</h3>
                    <div className="artist-grid album-grid" role="list" aria-label="Albums">
                      {albumsForArtist.map((a, i) => (
                        <button
                          type="button"
                          key={a.key}
                          className="artist-card"
                          style={{ '--i': i } as React.CSSProperties}
                          onClick={() => setAlbumKey(a.key)}
                        >
                          <CoverThumb trackId={a.sample.id} label={a.name} className="artist-glyph" />
                          <strong>{a.name}</strong>
                          <small>{a.count} track{a.count === 1 ? '' : 's'}</small>
                        </button>
                      ))}
                    </div>
                  </section>
                )}
                <section className="album-section">
                  <h3 className="section-label">Tracks</h3>
                  {visible.length ? (
                    <div className="catalog-list" role="listbox" aria-label="Tracks" ref={listRef}>
                      <div className="catalog-cols" aria-hidden="true">
                        <span>#</span>
                        <span />
                        <span>Title</span>
                        <span className="wide">Album</span>
                        <span />
                      </div>
                      {visible.map((t, i) => (
                        <TrackRow
                          key={t.id}
                          track={t}
                          index={i}
                          current={view.current}
                          playing={playing}
                          favorite={view.favorites.includes(t.id)}
                          focused={focusedIndex === i}
                          play={play}
                          addQueue={addQueue}
                          toggleFavorite={toggleFavorite}
                          onFocusRow={() => setFocusedIndex(i)}
                          onContext={e => openContext(e, t)}
                          wideLabel={t.album || folderLabel(t.folder)}
                        />
                      ))}
                    </div>
                  ) : (
                    <Empty icon={<Music2 size={24} />} title="No tracks" text="Nothing under this artist." />
                  )}
                </section>
              </div>
            ) : showTrackList && visible.length ? (
              <div className="catalog-list" role="listbox" aria-label="Tracks" ref={listRef} key={`${page}-${artistKey || ''}-${albumKey || ''}-${search}`}>
                <div className="catalog-cols" aria-hidden="true">
                  <span>#</span>
                  <span />
                  <span>Title</span>
                  <span className="wide">{page === 'artists' ? 'Album' : 'Folder'}</span>
                  <span />
                </div>
                {visible.map((t, i) => (
                  <TrackRow
                    key={t.id}
                    track={t}
                    index={i}
                    current={view.current}
                    playing={playing}
                    favorite={view.favorites.includes(t.id)}
                    focused={focusedIndex === i}
                    play={play}
                    addQueue={addQueue}
                    toggleFavorite={toggleFavorite}
                    onFocusRow={() => setFocusedIndex(i)}
                    onContext={e => openContext(e, t)}
                    wideLabel={page === 'artists' ? (t.album || folderLabel(t.folder)) : folderLabel(t.folder)}
                  />
                ))}
              </div>
            ) : page === 'favorites' && !visible.length ? (
              <Empty icon={<Heart size={24} />} title="No favorites yet" text="Tap the heart on a track to pin it here." />
            ) : (
              <Empty icon={<Music2 size={24} />} title="No tracks here" text="Try another view or add a music folder." />
            )}
          </main>
        </div>

        <footer className="player" aria-label="Playback">
          <div className="player-track">
            {view.current ? (
              <>
                <div key={trackKey} className={`mini-art swap ${playing ? 'live' : ''} ${coverSrc ? 'has-cover' : ''}`} aria-hidden="true">
                  {coverSrc ? <img src={coverSrc} alt="" /> : coverGlyph(view.current.name)}
                </div>
                <div key={`meta-${trackKey}`} className="player-meta swap">
                  <strong title={view.current.name}>{view.current.name}</strong>
                  <small>{trackMeta(view.current)}</small>
                </div>
                <button
                  className={view.favorites.includes(view.current.id) ? 'ghost liked' : 'ghost'}
                  aria-label={view.favorites.includes(view.current.id) ? 'Remove favorite' : 'Add favorite'}
                  onClick={() => toggleFavorite(view.current!)}
                >
                  <Heart size={15} fill={view.favorites.includes(view.current.id) ? 'currentColor' : 'none'} />
                </button>
              </>
            ) : (
              <span className="player-idle">Choose a track to start listening</span>
            )}
          </div>

          <div className="player-center">
            <div className="control-buttons">
              <button
                className={view.shuffled ? 'on' : ''}
                aria-label="Shuffle"
                aria-pressed={view.shuffled}
                onClick={() => void command('shuffle')}
              >
                <Shuffle size={18} />
              </button>
              <button aria-label="Previous track" onClick={previous}><SkipBack size={20} fill="currentColor" /></button>
              <button className="go" aria-label={playing ? 'Pause' : 'Play'} onClick={toggle}>
                {playing ? <Pause size={22} fill="currentColor" /> : <Play size={22} fill="currentColor" />}
              </button>
              <button aria-label="Next track" onClick={next}><SkipForward size={20} fill="currentColor" /></button>
              <button
                className={view.loop_mode !== 'off' ? 'on' : ''}
                aria-label={view.loop_mode === 'off' ? 'Loop off' : view.loop_mode === 'list' ? 'Loop list' : 'Loop track'}
                aria-pressed={view.loop_mode !== 'off'}
                onClick={() => void command('cycle_loop')}
              >
                {view.loop_mode === 'track' ? <Repeat1 size={18} /> : <Repeat size={18} />}
              </button>
            </div>
            <div className="scrub">
              <span>{formatTime(clock.position)}</span>
              <div className="scrub-track">
                <div className="scrub-fill" style={{ '--progress': progress / 100 } as React.CSSProperties} />
                <input
                  aria-label="Seek"
                  type="range"
                  min="0"
                  max={clock.duration || 1}
                  step="0.1"
                  value={clock.position}
                  onChange={e => { void command('seek', { seconds: Number(e.target.value) }) }}
                />
              </div>
              <span>{formatTime(clock.duration || 0)}</span>
            </div>
          </div>

          <div className="gain">
            <button type="button" aria-label={view.muted ? 'Unmute' : 'Mute'} onClick={() => void command('toggle_mute')}>
              {view.muted ? <VolumeX size={15} /> : <Volume2 size={15} />}
            </button>
            <input
              aria-label="Volume"
              type="range"
              min="0"
              max={view.settings.excess_volume ? 200 : 100}
              value={view.volume}
              onChange={e => { void command('set_volume', { volume: Number(e.target.value) }) }}
            />
            <span>{view.volume}%</span>
          </div>
        </footer>
      </div>

      {commandOpen && (
        <div className="cmd-veil" onClick={closeCommand} role="presentation">
          <div
            className="cmd"
            role="dialog"
            aria-modal="true"
            aria-label="Search library"
            onClick={e => e.stopPropagation()}
          >
            <label className="cmd-input">
              <Search size={18} />
              <input
                ref={searchRef}
                value={search}
                onChange={e => setSearch(e.target.value)}
                placeholder="Search tracks, artists, albums…"
                autoFocus
                aria-label="Search"
              />
              <kbd>{searchChord}</kbd>
            </label>
            <div className="cmd-list" role="listbox">
              {commandHits.length ? commandHits.map((t, i) => (
                <button
                  type="button"
                  key={t.id}
                  role="option"
                  aria-selected={commandIndex === i}
                  className={commandIndex === i ? 'on' : ''}
                  onMouseEnter={() => setCommandIndex(i)}
                  onClick={() => play(t)}
                >
                  <span className="cmd-glyph" aria-hidden="true">{coverGlyph(t.name)}</span>
                  <span className="cmd-copy">
                    <strong>{t.name}</strong>
                    <small>{[t.artist || folderLabel(t.folder), t.album].filter(Boolean).join(' · ')}</small>
                  </span>
                  {view.current?.id === t.id && <em>Playing</em>}
                </button>
              )) : (
                <p className="cmd-empty">{search.trim() ? 'No matches' : 'Library is empty'}</p>
              )}
            </div>
            <div className="cmd-foot">
              <span><kbd>↑↓</kbd> navigate</span>
              <span><kbd>↵</kbd> play</span>
              <span><kbd>esc</kbd> close</span>
            </div>
          </div>
        </div>
      )}
      {context && (
        <ContextMenu
          state={context}
          favorite={view.favorites.includes(context.track.id)}
          play={play}
          playNext={playNext}
          addQueue={addQueue}
          toggleFavorite={toggleFavorite}
        />
      )}
      {settingsOpen && (
        <SettingsPanel
          settings={view.settings}
          volume={view.volume}
          eq={view.eq}
          folders={folders}
          defaultMusicDir={defaultMusicDir}
          addFolder={addFolder}
          setVolume={v => { void command('set_volume', { volume: v }) }}
          setEq={eq => { void command('set_eq', { eq }) }}
          setExcess={v => { void command('set_excess_volume', { enabled: v }) }}
          setLdm={v => { void command('set_ldm', { enabled: v }) }}
          setArtistSource={source => { void setArtistMode(source) }}
          close={() => setSettingsOpen(false)}
        />
      )}
    </div>
  )
}

function WindowControls() {
  const run = async (action: 'close' | 'minimize' | 'maximize') => {
    if (!hasTauriBridge()) return
    const win = getCurrentWindow()
    if (action === 'close') await win.close()
    else if (action === 'minimize') await win.minimize()
    else await win.toggleMaximize()
  }
  return (
    <div className="traffic" role="group" aria-label="Window controls">
      <button type="button" className="tl close" aria-label="Close" onClick={e => { e.stopPropagation(); void run('close') }}><X size={9} strokeWidth={3} /></button>
      <button type="button" className="tl min" aria-label="Minimize" onClick={e => { e.stopPropagation(); void run('minimize') }}><Minus size={9} strokeWidth={3} /></button>
      <button type="button" className="tl max" aria-label="Maximize" onClick={e => { e.stopPropagation(); void run('maximize') }}><Plus size={9} strokeWidth={3} /></button>
    </div>
  )
}

function CoverThumb({ trackId, label, className }: { trackId: string; label: string; className?: string }) {
  const [src, setSrc] = useState<string | null>(null)
  useEffect(() => {
    if (!trackId || !hasTauriBridge()) { setSrc(null); return }
    let cancelled = false
    invoke<string | null>('track_cover', { id: trackId })
      .then(url => { if (!cancelled) setSrc(url) })
      .catch(() => { if (!cancelled) setSrc(null) })
    return () => { cancelled = true }
  }, [trackId])
  return (
    <span className={`${className || ''}${src ? ' has-cover' : ''}`} aria-hidden="true">
      {src ? <img src={src} alt="" /> : coverGlyph(label)}
    </span>
  )
}

function Empty({ icon, title, text, children }: { icon: React.ReactNode; title: string; text: string; children?: React.ReactNode }) {
  return (
    <div className="empty">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      <p>{text}</p>
      {children && <div className="empty-actions">{children}</div>}
    </div>
  )
}

function TrackRow({ track, index, current, playing, favorite, focused, play, addQueue, toggleFavorite, onFocusRow, onContext, wideLabel }: {
  track: Track; index: number; current: Track | null; playing: boolean; favorite: boolean; focused: boolean
  play: (t: Track) => void; addQueue: (t: Track) => void; toggleFavorite: (t: Track) => void
  onFocusRow: () => void; onContext: (e: React.MouseEvent) => void; wideLabel?: string
}) {
  const active = current?.id === track.id
  return (
    <div
      role="option"
      aria-selected={active}
      tabIndex={focused ? 0 : -1}
      data-track-index={index}
      className={`row ${active ? 'selected' : ''} ${focused ? 'focused' : ''}`}
      style={{ '--i': index } as React.CSSProperties}
      onClick={() => play(track)}
      onContextMenu={onContext}
      onFocus={onFocusRow}
      onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); play(track) } }}
    >
      <span className="idx">
        {active && playing ? <span className="bars" aria-hidden="true"><i /><i /><i /></span> : String(index + 1).padStart(2, '0')}
      </span>
      <span className="glyph" aria-hidden="true">{coverGlyph(track.name)}</span>
      <div className="row-main">
        <strong>{track.name}</strong>
        <small title={track.path}>{track.artist || track.path}</small>
      </div>
      <span className="folder wide" title={wideLabel || track.folder}>{wideLabel || folderLabel(track.folder)}</span>
      <div className="row-actions">
        <button className={favorite ? 'liked' : ''} aria-label={favorite ? 'Remove favorite' : 'Add favorite'} onClick={e => { e.stopPropagation(); toggleFavorite(track) }}>
          <Heart size={14} fill={favorite ? 'currentColor' : 'none'} />
        </button>
        <button aria-label="Add to queue" onClick={e => { e.stopPropagation(); addQueue(track) }}><Plus size={15} /></button>
      </div>
    </div>
  )
}

function ContextMenu({ state, favorite, play, playNext, addQueue, toggleFavorite }: {
  state: NonNullable<ContextState>; favorite: boolean
  play: (t: Track) => void; playNext: (t: Track) => void; addQueue: (t: Track) => void; toggleFavorite: (t: Track) => void
}) {
  return (
    <div className="menu floating" style={{ left: state.x, top: state.y }} onClick={e => e.stopPropagation()}>
      <div className="menu-title">{state.track.name}</div>
      <button onClick={() => play(state.track)}><Play size={14} />Play now</button>
      <button onClick={() => playNext(state.track)}><SkipForward size={14} />Play next</button>
      <button onClick={() => addQueue(state.track)}><Plus size={14} />Add to queue</button>
      <button onClick={() => toggleFavorite(state.track)}><Heart size={14} />{favorite ? 'Remove favorite' : 'Add to favorites'}</button>
      <div className="menu-rule" />
      <button onClick={() => hasTauriBridge() && invoke('reveal_in_file_manager', { path: state.track.path })}><ExternalLink size={14} />Reveal in folder</button>
    </div>
  )
}

function SettingsPanel({ settings, volume, eq, folders, defaultMusicDir, addFolder, setVolume, setEq, setExcess, setLdm, setArtistSource, close }: {
  settings: BackendSettings; volume: number; eq: string; folders: string[]; defaultMusicDir: string
  addFolder: () => void; setVolume: (v: number) => void; setEq: (eq: string) => void
  setExcess: (v: boolean) => void; setLdm: (v: boolean) => void; setArtistSource: (s: ArtistSource) => void; close: () => void
}) {
  const [tab, setTab] = useState<'library' | 'playback' | 'audio'>('library')
  const eqOptions = ['off', 'bass+', 'treble+', 'rock', 'vocal', 'lofi']
  const selected = eqOptions.includes(eq) ? eq : 'off'
  const defaultShown = !folders.some(f => f.replace(/\/+$/, '') === defaultMusicDir.replace(/\/+$/, ''))
  const artistSource: ArtistSource = settings.artist_source === 'folder' ? 'folder' : 'metadata'
  const folderCount = folders.length + (defaultShown || !folders.length ? 1 : 0)

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') close() }
    window.addEventListener('keydown', onKey)
    return () => window.removeEventListener('keydown', onKey)
  }, [close])

  return (
    <div className="popup-veil" onClick={close} role="presentation">
      <div
        className="popup"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onClick={e => e.stopPropagation()}
      >
        <header className="popup-head">
          <div className="popup-title">
            <span className="popup-mark" aria-hidden="true"><Settings size={16} strokeWidth={2} /></span>
            <div>
              <h2 id="settings-title">Settings</h2>
              <p>Shared with CLI · ~/option/music/config.toml</p>
            </div>
          </div>
          <button type="button" className="popup-close" aria-label="Close settings" onClick={close}><X size={18} /></button>
        </header>

        <nav className="popup-tabs" aria-label="Settings sections">
          <button type="button" className={tab === 'library' ? 'on' : ''} onClick={() => setTab('library')}>
            <FolderOpen size={15} /> Library
          </button>
          <button type="button" className={tab === 'playback' ? 'on' : ''} onClick={() => setTab('playback')}>
            <Volume2 size={15} /> Playback
          </button>
          <button type="button" className={tab === 'audio' ? 'on' : ''} onClick={() => setTab('audio')}>
            <SlidersHorizontal size={15} /> Audio
          </button>
        </nav>

        <div className="popup-body" key={tab}>
          {tab === 'library' && (
            <>
              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Music folders</h3>
                  <span>{folderCount}</span>
                </div>
                <p className="popup-note">Default ~/Music is always scanned when present.</p>
                <div className="path-list">
                  {folders.map(f => (
                    <div className="path" key={f}>
                      <FolderOpen size={15} />
                      <span title={f}>{f}</span>
                    </div>
                  ))}
                  {defaultShown && (
                    <div className="path">
                      <FolderOpen size={15} />
                      <span title={defaultMusicDir || '~/Music'}>{defaultMusicDir || '~/Music'} <em>default</em></span>
                    </div>
                  )}
                  {!folders.length && !defaultShown && <p className="popup-note">No extra folders yet.</p>}
                </div>
                <button type="button" className="popup-primary" onClick={addFolder}>
                  <Plus size={16} /> Add folder
                </button>
              </div>

              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Artists source</h3>
                </div>
                <p className="popup-note">How the Artists tab groups your library. Same setting as CLI (`c`).</p>
                <div className="seg" role="group" aria-label="Artists source">
                  <button type="button" className={artistSource === 'metadata' ? 'on' : ''} onClick={() => setArtistSource('metadata')}>
                    <Disc3 size={15} /> Metadata
                  </button>
                  <button type="button" className={artistSource === 'folder' ? 'on' : ''} onClick={() => setArtistSource('folder')}>
                    <FolderOpen size={15} /> Folder
                  </button>
                </div>
              </div>
            </>
          )}

          {tab === 'playback' && (
            <>
              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Volume</h3>
                  <span>{volume}%</span>
                </div>
                <input
                  className="popup-range"
                  type="range"
                  min="0"
                  max={settings.excess_volume ? 200 : 100}
                  value={volume}
                  onChange={e => setVolume(Number(e.target.value))}
                  aria-label="Volume"
                />
              </div>

              <label className="switch-row">
                <span>
                  <strong>Excess volume</strong>
                  <small>Allow gain up to 200%</small>
                </span>
                <input type="checkbox" checked={Boolean(settings.excess_volume)} onChange={e => setExcess(e.target.checked)} />
              </label>

              <label className="switch-row">
                <span>
                  <strong>Low detail mode</strong>
                  <small>Reduce motion across desktop + CLI</small>
                </span>
                <input type="checkbox" checked={Boolean(settings.ldm)} onChange={e => setLdm(e.target.checked)} />
              </label>
            </>
          )}

          {tab === 'audio' && (
            <div className="popup-block">
              <div className="popup-block-head">
                <h3>EQ preset</h3>
              </div>
              <p className="popup-note">Applied by the MPV desktop core.</p>
              <div className="eq-grid" role="listbox" aria-label="EQ preset">
                {eqOptions.map(o => (
                  <button
                    type="button"
                    key={o}
                    role="option"
                    aria-selected={selected === o}
                    className={selected === o ? 'on' : ''}
                    onClick={() => setEq(o)}
                  >
                    {o}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>

        <footer className="popup-foot">
          <button type="button" className="popup-done" onClick={close}>Done</button>
        </footer>
      </div>
    </div>
  )
}

createRoot(document.getElementById('root')!).render(<App />)
