import { BookMarked, Heart, ListMusic, Mic2, Plus, Search, Settings } from 'lucide-react'
import type { Page, Track } from '../types'
import { coverGlyph, searchChord, trackMeta } from '../lib'
import { WindowControls } from './WindowControls'

export function Sidebar({
  page,
  tracksCount,
  artistsCount,
  favoritesCount,
  currentId,
  recentTracks,
  goPage,
  play,
  openCommand,
  addFolder,
  openSettings,
}: {
  page: Page
  tracksCount: number
  artistsCount: number
  favoritesCount: number
  currentId?: string
  recentTracks: Track[]
  goPage: (p: Page) => void
  play: (t: Track) => void
  openCommand: () => void
  addFolder: () => void
  openSettings: () => void
}) {
  return (
    <div className="sidebar">
      <div className="side-top" data-tauri-drag-region>
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
          <em>{tracksCount}</em>
        </button>
        <button type="button" className={page === 'artists' ? 'side-link on' : 'side-link'} onClick={() => goPage('artists')}>
          <Mic2 size={16} strokeWidth={1.75} />
          <span>Artists</span>
          <em>{artistsCount}</em>
        </button>
        <button type="button" className={page === 'playlists' ? 'side-link on' : 'side-link'} onClick={() => goPage('playlists')}>
          <ListMusic size={16} strokeWidth={1.75} />
          <span>Playlists</span>
          <em>0</em>
        </button>
        <button type="button" className={page === 'favorites' ? 'side-link on' : 'side-link'} onClick={() => goPage('favorites')}>
          <Heart size={16} strokeWidth={1.75} />
          <span>Favorites</span>
          <em>{favoritesCount}</em>
        </button>
      </nav>

      <div className="side-recent">
        <p className="side-label">Recent</p>
        {recentTracks.length ? recentTracks.map(t => (
          <button
            type="button"
            key={t.id}
            className={`side-track${currentId === t.id ? ' current' : ''}`}
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
        <button type="button" className="side-settings" onClick={openSettings}>
          <Settings size={15} strokeWidth={1.75} />
          Settings
        </button>
      </div>
    </div>
  )
}
