export type Track = {
  id: string
  name: string
  path: string
  folder: string
  artist: string
  album: string
  mtime: number
}

export type ArtistSource = 'metadata' | 'folder'

export type BackendSettings = {
  excess_volume: boolean
  ldm: boolean
  accent: string
  artist_source?: ArtistSource
  cava: { enabled?: boolean; style?: string }
  folders?: string[]
  music_dirs?: string[]
  resume_track?: string
  resume_position?: number
  resume_queue?: string[]
}

export type LoopMode = 'off' | 'list' | 'track'

export type PlaybackState = {
  queue: string[]
  current: Track | null
  position: number
  duration: number | null
  paused: boolean
  stopped: boolean
  volume: number
  muted: boolean
  speed: number
  pitch: number
  eq: string
  favorites: string[]
  loop_mode: LoopMode
  shuffled: boolean
}

export type Snapshot = PlaybackState & { library: Track[]; settings: BackendSettings }

export type Page = 'library' | 'artists' | 'playlists' | 'favorites'

export type ContextState = { track: Track; x: number; y: number } | null

export type ArtistGroup = { key: string; name: string; count: number; albumCount: number; sample: Track }

export type AlbumGroup = { key: string; name: string; count: number; sample: Track }
