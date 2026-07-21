import { useEffect, useMemo, useRef, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { listen } from '@tauri-apps/api/event'
import { open } from '@tauri-apps/plugin-dialog'
import type { AlbumGroup, ArtistGroup, ArtistSource, ContextState, Page, PlaybackState, Snapshot, Track } from '../types'
import { emptySnapshot, hasTauriBridge, trackAlbum, trackArtist } from '../lib'

export function useOptMusic() {
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
    if (page === 'favorites') result = result.filter(t => favoriteIds.has(t.id))
    if (page === 'artists' && artistKey) {
      result = result.filter(t => trackArtist(t, artistSource).toLowerCase() === artistKey)
      if (albumKey) result = result.filter(t => trackAlbum(t, artistSource).toLowerCase() === albumKey)
    }
    if (page === 'playlists') result = []
    if (page === 'artists' && !artistKey) return []
    return result.sort((a, b) => a.name.localeCompare(b.name))
  }, [tracks, page, favoriteIds, artistKey, albumKey, artistSource])

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
  useEffect(() => { commandHitsRef.current = commandHits }, [commandHits])
  useEffect(() => { visibleRef.current = visible }, [visible])
  useEffect(() => { focusedIndexRef.current = focusedIndex }, [focusedIndex])

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
        const dirs = domain.current.settings.folders || domain.current.settings.music_dirs || []
        await invoke('scan_music_directories', { paths: [...new Set(dirs)] })
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
      const dirs = domain.current.settings.folders || domain.current.settings.music_dirs || []
      await invoke('scan_music_directories', { paths: [...new Set(dirs)] })
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
  }
}
