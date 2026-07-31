import { useEffect, useState, type CSSProperties } from 'react'
import { CornersOut, Heart, Pause, Play, Playlist, X } from '@phosphor-icons/react'
import { invoke } from '@tauri-apps/api/core'
import type { Lyrics, Track } from '../types'
import { coverGlyph, formatTime, hasTauriBridge, trackMeta } from '../lib/music'

export function Stage({
  current,
  playing,
  coverSrc,
  progress,
  position,
  duration,
  favorited,
  queue,
  queueOpen,
  focusMode,
  setQueueOpen,
  setFocusMode,
  toggle,
  toggleFavorite,
  seek,
  play,
  removeFromQueue,
}: {
  current: Track | null
  playing: boolean
  coverSrc: string | null
  progress: number
  position: number
  duration: number | null
  favorited: boolean
  queue: Track[]
  queueOpen: boolean
  focusMode: boolean
  setQueueOpen: (fn: (o: boolean) => boolean) => void
  setFocusMode: (v: boolean | ((prev: boolean) => boolean)) => void
  toggle: () => void
  toggleFavorite: (t: Track) => void
  seek: (seconds: number) => void
  play: (t: Track) => void
  removeFromQueue: (id: string) => void
}) {
  const trackKey = current?.id ?? 'idle'
  const [lyrics, setLyrics] = useState<Lyrics | null>(null)
  const [lyricsOpen, setLyricsOpen] = useState(false)

  useEffect(() => {
    if (!current?.id || !hasTauriBridge()) {
      setLyrics(null)
      return
    }
    let cancelled = false
    setLyrics(null)
    invoke<Lyrics>('track_lyrics', { id: current.id })
      .then(result => { if (!cancelled) setLyrics(result) })
      .catch(() => { if (!cancelled) setLyrics(null) })
    return () => { cancelled = true }
  }, [current?.id])

  const hasLyrics = Boolean(lyrics && lyrics.kind !== 'none' && lyrics.text.trim())

  return (
    <section className={`stage${playing ? ' is-live' : ''}${current ? ' has-track' : ''}${focusMode ? ' focus-mode' : ''}`} aria-label="Now playing">
      <div key={trackKey} className={`stage-hero swap ${coverSrc ? 'has-cover' : ''}`}>
        <div className="stage-hero-art" aria-hidden="true">
          {coverSrc ? <img src={coverSrc} alt="" /> : <span>{current ? coverGlyph(current.name) : 'o'}</span>}
        </div>
        <div className="stage-hero-shade" aria-hidden="true" />
        {current && (
          <button type="button" className="stage-hero-play" aria-label={playing ? 'Pause' : 'Play'} onClick={toggle}>
            {playing ? <Pause size={22} weight="fill" /> : <Play size={22} weight="fill" />}
          </button>
        )}
        {playing && <div className="stage-hero-pulse" aria-hidden="true" />}
      </div>

      <div key={`copy-${trackKey}`} className="stage-body swap">
        {current ? (
          <>
            <div className="stage-status">
              <span className={playing ? 'live-dot' : 'idle-dot'} />
              <span>{playing ? 'Playing' : 'Paused'}</span>
              {favorited && <span className="stage-chip">Liked</span>}
              {focusMode && <span className="stage-chip">Focus</span>}
            </div>
            <h1 title={current.name}>{current.name}</h1>
            <p className="stage-artist">{trackMeta(current)}</p>
            {current.album && <p className="stage-album">{current.album}</p>}

            <div className="stage-scrub" aria-hidden={!duration}>
              <div className="stage-scrub-track">
                <div className="stage-scrub-fill" style={{ '--progress': progress / 100 } as CSSProperties} />
                <input
                  aria-label="Seek"
                  type="range"
                  min="0"
                  max={duration || 1}
                  step="0.1"
                  value={position}
                  onChange={e => seek(Number(e.target.value))}
                />
              </div>
              <div className="stage-scrub-times">
                <span>{formatTime(position)}</span>
                <span>{formatTime(duration || 0)}</span>
              </div>
            </div>

            <div className="stage-actions">
              <button
                type="button"
                className={favorited ? 'stage-action liked' : 'stage-action'}
                aria-label={favorited ? 'Remove favorite' : 'Add favorite'}
                onClick={() => toggleFavorite(current)}
              >
                <Heart size={15} weight={favorited ? 'fill' : 'regular'} />
                {favorited ? 'Liked' : 'Like'}
              </button>
              {!focusMode && (
                <button
                  type="button"
                  className={queueOpen ? 'stage-action on' : 'stage-action'}
                  aria-pressed={queueOpen}
                  onClick={() => setQueueOpen(o => !o)}
                >
                  <Playlist size={15} />
                  Queue{queue.length > 0 ? ` · ${queue.length}` : ''}
                </button>
              )}
              <button
                type="button"
                className={focusMode ? 'stage-action on' : 'stage-action'}
                aria-pressed={focusMode}
                aria-label={focusMode ? 'Exit focus mode' : 'Enter focus mode'}
                onClick={() => setFocusMode(v => !v)}
              >
                <CornersOut size={15} />
                {focusMode ? 'Exit focus' : 'Focus'}
              </button>
              {hasLyrics && (
                <button
                  type="button"
                  className={lyricsOpen ? 'stage-action on' : 'stage-action'}
                  aria-pressed={lyricsOpen}
                  onClick={() => setLyricsOpen(o => !o)}
                >
                  Lyrics
                </button>
              )}
            </div>

            {lyricsOpen && hasLyrics && (
              <div className="stage-lyrics" aria-label="Lyrics">
                <pre>{lyrics!.text}</pre>
              </div>
            )}
          </>
        ) : (
          <div className="stage-empty">
            <p className="stage-status"><span className="idle-dot" />Idle</p>
            <h1>Nothing playing</h1>
            <p className="stage-artist">Choose a track from the library.</p>
            <div className="stage-actions">
              <button
                type="button"
                className={focusMode ? 'stage-action on' : 'stage-action'}
                aria-pressed={focusMode}
                onClick={() => setFocusMode(v => !v)}
              >
                <CornersOut size={15} />
                {focusMode ? 'Exit focus' : 'Focus'}
              </button>
            </div>
          </div>
        )}
      </div>

      {queueOpen && !focusMode && (
        <div className="queue-block">
          <div className="queue-head">
            <span>Up next</span>
            <b>{queue.length}</b>
            <button type="button" className="ghost tiny" aria-label="Hide queue" onClick={() => setQueueOpen(() => false)}><X size={14} /></button>
          </div>
          <div className="queue-scroll">
            {queue.length ? queue.map((t, i) => (
              <div className={`queue-item ${current?.id === t.id ? 'current' : ''}`} key={t.id} style={{ '--i': i } as CSSProperties}>
                <span>{String(i + 1).padStart(2, '0')}</span>
                <button type="button" onClick={() => play(t)}>
                  <strong>{t.name}</strong>
                  <small>{trackMeta(t)}</small>
                </button>
                <button type="button" aria-label={`Remove ${t.name}`} onClick={() => removeFromQueue(t.id)}><X size={13} /></button>
              </div>
            )) : <p className="queue-empty">Queue is empty — add with + on any track.</p>}
          </div>
        </div>
      )}
    </section>
  )
}
