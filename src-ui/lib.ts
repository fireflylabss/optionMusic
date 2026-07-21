import { isTauri } from '@tauri-apps/api/core'
import type { ArtistSource, Snapshot, Track } from './types'

export const emptySnapshot: Snapshot = {
  library: [],
  queue: [],
  current: null,
  position: 0,
  duration: null,
  paused: true,
  stopped: true,
  volume: 80,
  muted: false,
  speed: 1,
  pitch: 0,
  eq: 'Default',
  favorites: [],
  loop_mode: 'off',
  shuffled: false,
  settings: {
    excess_volume: false,
    ldm: false,
    accent: 'default',
    artist_source: 'metadata',
    cava: {},
  },
}

export const hasTauriBridge = () =>
  isTauri() || Boolean((window as Window & { __TAURI_INTERNALS__?: unknown }).__TAURI_INTERNALS__)

export const isApple = typeof navigator !== 'undefined' && /Mac|iPhone|iPad/.test(navigator.platform || navigator.userAgent)
export const searchChord = isApple ? '⌘K' : 'Ctrl+K'

export const folderLabel = (folder: string) => {
  const parts = folder.replace(/\\/g, '/').split('/').filter(Boolean)
  return parts[parts.length - 1] || folder || '—'
}

export const coverGlyph = (name: string) => (name.trim().charAt(0) || 'o').toUpperCase()

export const formatTime = (n: number) =>
  `${Math.floor(n / 60)}:${String(Math.floor(n % 60)).padStart(2, '0')}`

export const trackArtist = (t: Track, source: ArtistSource) => {
  if (source === 'folder') return folderLabel(t.folder) || 'Unknown Artist'
  return (t.artist || '').trim() || 'Unknown Artist'
}

export const trackAlbum = (t: Track, source: ArtistSource) => {
  if (source === 'folder') return folderLabel(t.folder) || 'Unknown Album'
  return (t.album || '').trim() || 'Unknown Album'
}

export const trackMeta = (t: Track) => (t.artist || '').trim() || folderLabel(t.folder)

export const eqOptions = ['off', 'bass+', 'treble+', 'rock', 'vocal', 'lofi'] as const
