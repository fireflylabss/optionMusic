export type Track = {
  id: string
  name: string
  path: string
  folder: string
  artist: string
  album: string
  track_number?: number | null
  has_cover?: boolean | null
  mtime: number
}

export type ArtistSource = 'metadata' | 'folder'
export type ReplayGainMode = 'off' | 'track' | 'album'
export type SmartShelfKind = 'played_week' | 'no_cover' | 'incomplete_albums'

export type BackendSettings = {
  excess_volume: boolean
  ldm: boolean
  accent: string
  artist_source?: ArtistSource
  replaygain?: ReplayGainMode
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

export type Page = 'library' | 'artists' | 'playlists' | 'favorites' | 'shelves'

export type ContextState = { track: Track; x: number; y: number } | null

export type ArtistGroup = { key: string; name: string; count: number; albumCount: number; sample: Track }

export type AlbumGroup = { key: string; name: string; count: number; sample: Track }

export type SavedPlaylist = {
  id: string
  name: string
  tracks: string[]
  source?: string | null
}

export type AudioTags = {
  title?: string | null
  artist?: string | null
  album?: string | null
  track_number?: number | null
  disc_number?: number | null
}

export type Lyrics = {
  kind: 'embedded' | 'lrc' | 'text' | 'none'
  text: string
}

export type SearchHit = {
  id: string
  title: string
  url: string
  uploader: string
  duration: number | null
  provider: string
}

/** Partial library update from background tag enrichment. */
export type LibraryEnrichUpdate = { tracks: Track[]; done: boolean }
