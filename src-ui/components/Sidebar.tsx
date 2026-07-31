import { BookBookmark, DownloadSimple, GearSix, Heart, MagnifyingGlass, MicrophoneStage, Playlist, Plus, SquaresFour } from '@phosphor-icons/react'
import type { Page, Track } from '../types'
import { coverGlyph, searchChord, trackMeta } from '../lib/music'
import { Button } from './ui/button'
import { WindowControls } from './WindowControls'

export function Sidebar({
  page,
  tracksCount,
  artistsCount,
  favoritesCount,
  playlistsCount,
  currentId,
  recentTracks,
  goPage,
  play,
  openCommand,
  addFolder,
  openSettings,
  openDownload,
}: {
  page: Page
  tracksCount: number
  artistsCount: number
  favoritesCount: number
  playlistsCount: number
  currentId?: string
  recentTracks: Track[]
  goPage: (p: Page) => void
  play: (t: Track) => void
  openCommand: () => void
  addFolder: () => void
  openSettings: () => void
  openDownload: () => void
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
          <BookBookmark size={16} weight="regular" />
          <span>Library</span>
          <em>{tracksCount}</em>
        </button>
        <button type="button" className={page === 'artists' ? 'side-link on' : 'side-link'} onClick={() => goPage('artists')}>
          <MicrophoneStage size={16} weight="regular" />
          <span>Artists</span>
          <em>{artistsCount}</em>
        </button>
        <button type="button" className={page === 'playlists' ? 'side-link on' : 'side-link'} onClick={() => goPage('playlists')}>
          <Playlist size={16} weight="regular" />
          <span>Playlists</span>
          <em>{playlistsCount}</em>
        </button>
        <button type="button" className={page === 'shelves' ? 'side-link on' : 'side-link'} onClick={() => goPage('shelves')}>
          <SquaresFour size={16} weight="regular" />
          <span>Shelves</span>
        </button>
        <button type="button" className={page === 'favorites' ? 'side-link on' : 'side-link'} onClick={() => goPage('favorites')}>
          <Heart size={16} weight="regular" />
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
          <MagnifyingGlass size={15} weight="regular" />
          <span>Search</span>
          <kbd>{searchChord}</kbd>
        </button>
        <Button type="button" className="w-full" onClick={openDownload}>
          <DownloadSimple data-icon="inline-start" weight="bold" />
          Download
        </Button>
        <Button type="button" className="w-full" onClick={() => void addFolder()}>
          <Plus data-icon="inline-start" weight="bold" />
          Open files
        </Button>
        <button type="button" className="side-settings" onClick={openSettings}>
          <GearSix size={15} weight="regular" />
          Settings
        </button>
      </div>
    </div>
  )
}
