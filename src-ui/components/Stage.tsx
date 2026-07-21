import type { CSSProperties } from 'react'
import { Heart, ListMusic, Pause, Play, X } from 'lucide-react'
import type { Track } from '../types'
import { coverGlyph, formatTime, trackMeta } from '../lib'

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
  setQueueOpen,
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
  setQueueOpen: (fn: (o: boolean) => boolean) => void
  toggle: () => void
  toggleFavorite: (t: Track) => void
  seek: (seconds: number) => void
  play: (t: Track) => void
  removeFromQueue: (id: string) => void
}) {
  const trackKey = current?.id ?? 'idle'

  return (
    <section className={`stage${playing ? ' is-live' : ''}${current ? ' has-track' : ''}`} aria-label="Now playing">
      <div key={trackKey} className={`stage-hero swap ${coverSrc ? 'has-cover' : ''}`}>
        <div className="stage-hero-art" aria-hidden="true">
          {coverSrc ? <img src={coverSrc} alt="" /> : <span>{current ? coverGlyph(current.name) : 'o'}</span>}
        </div>
        <div className="stage-hero-shade" aria-hidden="true" />
        {current && (
          <button type="button" className="stage-hero-play" aria-label={playing ? 'Pause' : 'Play'} onClick={toggle}>
            {playing ? <Pause size={22} fill="currentColor" /> : <Play size={22} fill="currentColor" />}
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
                <Heart size={15} fill={favorited ? 'currentColor' : 'none'} />
                {favorited ? 'Liked' : 'Like'}
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
