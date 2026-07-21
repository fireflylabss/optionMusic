import type { CSSProperties, MouseEvent, RefObject } from 'react'
import { FolderOpen, Heart, ListMusic, Mic2, Music2, Plus, RefreshCw, Search, X } from 'lucide-react'
import type { AlbumGroup, ArtistGroup, ArtistSource, Page, Track } from '../types'
import { folderLabel } from '../lib'
import { CoverThumb } from './CoverThumb'
import { Empty } from './Empty'
import { TrackRow } from './TrackRow'

export function Catalog({
  page,
  pageTitle,
  loading,
  error,
  status,
  tracks,
  visible,
  artists,
  albumsForArtist,
  artistSource,
  artistKey,
  albumKey,
  selectedArtistName,
  playing,
  current,
  favoriteIds,
  focusedIndex,
  queueOpen,
  queueLength,
  locationCount,
  listRef,
  setError,
  setArtistKey,
  setAlbumKey,
  setArtistMode,
  setFocusedIndex,
  setQueueOpen,
  openCommand,
  loadLibrary,
  addFolder,
  play,
  addQueue,
  toggleFavorite,
  openContext,
}: {
  page: Page
  pageTitle: string
  loading: boolean
  error: string
  status: string
  tracks: Track[]
  visible: Track[]
  artists: ArtistGroup[]
  albumsForArtist: AlbumGroup[]
  artistSource: ArtistSource
  artistKey: string | null
  albumKey: string | null
  selectedArtistName?: string
  playing: boolean
  current: Track | null
  favoriteIds: Set<string>
  focusedIndex: number
  queueOpen: boolean
  queueLength: number
  locationCount: number
  listRef: RefObject<HTMLDivElement | null>
  setError: (v: string) => void
  setArtistKey: (v: string | null) => void
  setAlbumKey: (v: string | null) => void
  setArtistMode: (s: ArtistSource) => void
  setFocusedIndex: (i: number) => void
  setQueueOpen: (fn: (o: boolean) => boolean) => void
  openCommand: () => void
  loadLibrary: () => void
  addFolder: () => void
  play: (t: Track) => void
  addQueue: (t: Track) => void
  toggleFavorite: (t: Track) => void
  openContext: (e: MouseEvent, t: Track) => void
}) {
  const showTrackList = page !== 'playlists' && !(page === 'artists' && !artistKey)
  const showArtistAlbums = page === 'artists' && !!artistKey && !albumKey

  return (
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
              ← {albumKey ? selectedArtistName || 'Artist' : 'Artists'}
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
          <button type="button" className={queueOpen ? 'ghost on' : 'ghost'} aria-label={queueOpen ? 'Hide queue' : 'Show queue'} aria-pressed={queueOpen} onClick={() => setQueueOpen(o => !o)}>
            <ListMusic size={16} />
            {queueLength > 0 && <span className="dot">{queueLength}</span>}
          </button>
          <button type="button" className="ghost" aria-label="Refresh library" onClick={loadLibrary}><RefreshCw size={15} className={loading ? 'spin' : ''} /></button>
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
          <button type="button" className="primary" onClick={loadLibrary}><RefreshCw size={15} />Try again</button>
          <button type="button" className="secondary" onClick={addFolder}>Choose folder</button>
        </Empty>
      ) : loading ? (
        <Empty icon={<RefreshCw className="spin" size={24} />} title="Scanning your library" text="Looking through your music folders…" />
      ) : !tracks.length ? (
        <Empty icon={<Music2 size={28} />} title="Add a music folder to start" text="Drop audio into ~/Music, or choose another folder.">
          <button type="button" className="primary" onClick={addFolder}><Plus size={16} />Add music folder</button>
        </Empty>
      ) : page === 'playlists' ? (
        <Empty icon={<ListMusic size={28} />} title="No playlists yet" text="Playlists aren’t in optMusic yet — use Favorites or the queue for now." />
      ) : page === 'artists' && !artistKey ? (
        artists.length ? (
          <ul className="artist-grid" aria-label="Artists">
            {artists.map((a, i) => (
              <li key={a.key} className="artist-grid-item">
                <button
                  type="button"
                  className="artist-card"
                  style={{ '--i': i } as CSSProperties}
                  onClick={() => { setArtistKey(a.key); setAlbumKey(null) }}
                >
                  <CoverThumb trackId={a.sample.id} label={a.name} className="artist-glyph" />
                  <strong>{a.name}</strong>
                  <small>{a.albumCount} album{a.albumCount === 1 ? '' : 's'} · {a.count} track{a.count === 1 ? '' : 's'}</small>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <Empty icon={<Mic2 size={24} />} title="No artists found" text={artistSource === 'metadata' ? 'No artist tags in this library — try Folder mode.' : 'Artists are grouped from your folder names.'} />
        )
      ) : showArtistAlbums ? (
        <div className="artist-detail">
          {albumsForArtist.length > 0 && (
            <section className="album-section">
              <h3 className="section-label">Albums</h3>
              <ul className="artist-grid album-grid" aria-label="Albums">
                {albumsForArtist.map((a, i) => (
                  <li key={a.key} className="artist-grid-item">
                    <button
                      type="button"
                      className="artist-card"
                      style={{ '--i': i } as CSSProperties}
                      onClick={() => setAlbumKey(a.key)}
                    >
                      <CoverThumb trackId={a.sample.id} label={a.name} className="artist-glyph" />
                      <strong>{a.name}</strong>
                      <small>{a.count} track{a.count === 1 ? '' : 's'}</small>
                    </button>
                  </li>
                ))}
              </ul>
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
                    current={current}
                    playing={playing}
                    favorite={favoriteIds.has(t.id)}
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
        <div className="catalog-list" role="listbox" aria-label="Tracks" ref={listRef} key={`${page}-${artistKey || ''}-${albumKey || ''}`}>
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
              current={current}
              playing={playing}
              favorite={favoriteIds.has(t.id)}
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
  )
}
