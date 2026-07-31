import type { CSSProperties, MouseEvent, RefObject } from 'react'
import { ArrowsClockwise, FolderOpen, Heart, MagnifyingGlass, MicrophoneStage, MusicNote, Playlist, Plus, SquaresFour, X } from '@phosphor-icons/react'
import type { AlbumGroup, ArtistGroup, ArtistSource, Page, SavedPlaylist, SmartShelfKind, Track } from '../types'
import { CoverThumb } from './CoverThumb'
import { Empty } from './Empty'
import { VirtualTrackList } from './VirtualTrackList'

const SHELF_TABS: { kind: SmartShelfKind; label: string }[] = [
  { kind: 'played_week', label: 'Played this week' },
  { kind: 'no_cover', label: 'No cover' },
  { kind: 'incomplete_albums', label: 'Incomplete albums' },
]

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
  playlists,
  shelfKind,
  shelfLoading,
  setError,
  setArtistKey,
  setAlbumKey,
  setArtistMode,
  setFocusedIndex,
  setQueueOpen,
  setShelfKind,
  openCommand,
  loadLibrary,
  addFolder,
  play,
  addQueue,
  toggleFavorite,
  openContext,
  createPlaylist,
  importM3u,
  playPlaylist,
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
  playlists: SavedPlaylist[]
  shelfKind: SmartShelfKind
  shelfLoading: boolean
  setError: (v: string) => void
  setArtistKey: (v: string | null) => void
  setAlbumKey: (v: string | null) => void
  setArtistMode: (s: ArtistSource) => void
  setFocusedIndex: (i: number) => void
  setQueueOpen: (fn: (o: boolean) => boolean) => void
  setShelfKind: (k: SmartShelfKind) => void
  openCommand: () => void
  loadLibrary: () => void
  addFolder: () => void
  play: (t: Track) => void
  addQueue: (t: Track) => void
  toggleFavorite: (t: Track) => void
  openContext: (e: MouseEvent, t: Track) => void
  createPlaylist: () => void
  importM3u: () => void
  playPlaylist: (id: string) => void
}) {
  const showTrackList = page !== 'playlists' && !(page === 'artists' && !artistKey)
  const showArtistAlbums = page === 'artists' && !!artistKey && !albumKey
  const listLayout = (showTrackList && visible.length > 0) || (showArtistAlbums && visible.length > 0) || (page === 'shelves' && visible.length > 0)

  return (
    <main className="catalog">
      <div className="catalog-head swap" key={`${page}-${artistKey || ''}-${albumKey || ''}-${shelfKind}`}>
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
            {loading || (page === 'shelves' && shelfLoading) ? 'Scanning…'
              : page === 'artists' && !artistKey ? `${artists.length} artist${artists.length === 1 ? '' : 's'} · ${artistSource}`
              : page === 'artists' && artistKey && !albumKey ? `${albumsForArtist.length} album${albumsForArtist.length === 1 ? '' : 's'} · ${visible.length} tracks`
              : page === 'playlists' ? `${playlists.length} playlist${playlists.length === 1 ? '' : 's'}`
              : page === 'shelves' ? `${visible.length} track${visible.length === 1 ? '' : 's'}`
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
          {page === 'shelves' && (
            <div className="mode-toggle shelf-tabs" role="tablist" aria-label="Smart shelves">
              {SHELF_TABS.map(tab => (
                <button
                  key={tab.kind}
                  type="button"
                  role="tab"
                  aria-selected={shelfKind === tab.kind}
                  className={shelfKind === tab.kind ? 'on' : ''}
                  onClick={() => setShelfKind(tab.kind)}
                >
                  {tab.label}
                </button>
              ))}
            </div>
          )}
          {page === 'playlists' && (
            <>
              <button type="button" className="ghost" aria-label="Create playlist" onClick={createPlaylist}><Plus size={16} /></button>
              <button type="button" className="ghost" aria-label="Import M3U" onClick={importM3u}><FolderOpen size={16} /></button>
            </>
          )}
          <button type="button" className="ghost" aria-label="Search" onClick={openCommand}><MagnifyingGlass size={16} /></button>
          <button type="button" className={queueOpen ? 'ghost on' : 'ghost'} aria-label={queueOpen ? 'Hide queue' : 'Show queue'} aria-pressed={queueOpen} onClick={() => setQueueOpen(o => !o)}>
            <Playlist size={16} />
            {queueLength > 0 && <span className="dot">{queueLength}</span>}
          </button>
          <button type="button" className="ghost" aria-label="Refresh library" onClick={loadLibrary}><ArrowsClockwise size={15} className={loading ? 'spin' : ''} /></button>
        </div>
      </div>

      {error && (
        <div className="banner" role="alert">
          {error}
          <button type="button" aria-label="Dismiss error" onClick={() => setError('')}><X size={14} /></button>
        </div>
      )}

      <div className={`catalog-body${listLayout ? ' catalog-body-list' : ''}`}>
      {error && !tracks.length ? (
        <Empty icon={<FolderOpen size={28} />} title="Couldn’t open a music folder" text={error}>
          <button type="button" className="primary" onClick={loadLibrary}><ArrowsClockwise size={15} />Try again</button>
          <button type="button" className="secondary" onClick={addFolder}>Choose folder</button>
        </Empty>
      ) : loading ? (
        <Empty icon={<ArrowsClockwise className="spin" size={24} />} title="Scanning your library" text="Looking through your music folders…" />
      ) : !tracks.length && page !== 'playlists' ? (
        <Empty icon={<MusicNote size={28} />} title="Add a music folder to start" text="Drop audio into ~/Music, or choose another folder.">
          <button type="button" className="primary" onClick={addFolder}><Plus size={16} />Add music folder</button>
        </Empty>
      ) : page === 'playlists' ? (
        playlists.length ? (
          <ul className="playlist-grid" aria-label="Playlists">
            {playlists.map((pl, i) => (
              <li key={pl.id} className="playlist-grid-item">
                <button
                  type="button"
                  className="playlist-card"
                  style={{ '--i': i } as CSSProperties}
                  onClick={() => playPlaylist(pl.id)}
                >
                  <span className="playlist-glyph" aria-hidden="true"><Playlist size={22} /></span>
                  <strong>{pl.name}</strong>
                  <small>{pl.tracks.length} track{pl.tracks.length === 1 ? '' : 's'}{pl.source ? ' · imported' : ''}</small>
                </button>
              </li>
            ))}
          </ul>
        ) : (
          <Empty icon={<Playlist size={28} />} title="No playlists yet" text="Create one, or import an M3U file.">
            <button type="button" className="primary" onClick={createPlaylist}><Plus size={16} />Create playlist</button>
            <button type="button" className="secondary" onClick={importM3u}>Import M3U</button>
          </Empty>
        )
      ) : page === 'shelves' ? (
        shelfLoading ? (
          <Empty icon={<ArrowsClockwise className="spin" size={24} />} title="Loading shelf" text="Gathering tracks…" />
        ) : visible.length ? (
          <VirtualTrackList
            tracks={visible}
            wideColumn="album"
            current={current}
            playing={playing}
            favoriteIds={favoriteIds}
            focusedIndex={focusedIndex}
            listRef={listRef}
            play={play}
            addQueue={addQueue}
            toggleFavorite={toggleFavorite}
            setFocusedIndex={setFocusedIndex}
            openContext={openContext}
          />
        ) : (
          <Empty icon={<SquaresFour size={24} />} title="Nothing on this shelf" text="Try another shelf, or keep listening." />
        )
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
          <Empty icon={<MicrophoneStage size={24} />} title="No artists found" text={artistSource === 'metadata' ? 'No artist tags in this library — try Folder mode.' : 'Artists are grouped from your folder names.'} />
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
          <section className="album-section album-section-tracks">
            <h3 className="section-label">Tracks</h3>
            {visible.length ? (
              <VirtualTrackList
                tracks={visible}
                wideColumn="album"
                current={current}
                playing={playing}
                favoriteIds={favoriteIds}
                focusedIndex={focusedIndex}
                listRef={listRef}
                play={play}
                addQueue={addQueue}
                toggleFavorite={toggleFavorite}
                setFocusedIndex={setFocusedIndex}
                openContext={openContext}
              />
            ) : (
              <Empty icon={<MusicNote size={24} />} title="No tracks" text="Nothing under this artist." />
            )}
          </section>
        </div>
      ) : showTrackList && visible.length ? (
        <VirtualTrackList
          tracks={visible}
          wideColumn={page === 'artists' ? 'album' : 'folder'}
          current={current}
          playing={playing}
          favoriteIds={favoriteIds}
          focusedIndex={focusedIndex}
          listRef={listRef}
          play={play}
          addQueue={addQueue}
          toggleFavorite={toggleFavorite}
          setFocusedIndex={setFocusedIndex}
          openContext={openContext}
        />
      ) : page === 'favorites' && !visible.length ? (
        <Empty icon={<Heart size={24} />} title="No favorites yet" text="Tap the heart on a track to pin it here." />
      ) : (
        <Empty icon={<MusicNote size={24} />} title="No tracks here" text="Try another view or add a music folder." />
      )}
      </div>
    </main>
  )
}
