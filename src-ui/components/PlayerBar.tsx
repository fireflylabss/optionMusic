import type { CSSProperties } from 'react'
import { Heart, Pause, Play, Repeat, Repeat1, Shuffle, SkipBack, SkipForward, Volume2, VolumeX } from 'lucide-react'
import type { LoopMode, Track } from '../types'
import { coverGlyph, formatTime, trackMeta } from '../lib'

export function PlayerBar({
  current,
  playing,
  coverSrc,
  favorited,
  shuffled,
  loopMode,
  muted,
  volume,
  excessVolume,
  position,
  duration,
  progress,
  toggleFavorite,
  toggle,
  previous,
  next,
  shuffle,
  cycleLoop,
  toggleMute,
  setVolume,
  seek,
}: {
  current: Track | null
  playing: boolean
  coverSrc: string | null
  favorited: boolean
  shuffled: boolean
  loopMode: LoopMode
  muted: boolean
  volume: number
  excessVolume: boolean
  position: number
  duration: number | null
  progress: number
  toggleFavorite: (t: Track) => void
  toggle: () => void
  previous: () => void
  next: () => void
  shuffle: () => void
  cycleLoop: () => void
  toggleMute: () => void
  setVolume: (v: number) => void
  seek: (seconds: number) => void
}) {
  const trackKey = current?.id ?? 'idle'

  return (
    <footer className="player" aria-label="Playback">
      <div className="player-track">
        {current ? (
          <>
            <div key={trackKey} className={`mini-art swap ${playing ? 'live' : ''} ${coverSrc ? 'has-cover' : ''}`} aria-hidden="true">
              {coverSrc ? <img src={coverSrc} alt="" /> : coverGlyph(current.name)}
            </div>
            <div key={`meta-${trackKey}`} className="player-meta swap">
              <strong title={current.name}>{current.name}</strong>
              <small>{trackMeta(current)}</small>
            </div>
            <button
              type="button"
              className={favorited ? 'ghost liked' : 'ghost'}
              aria-label={favorited ? 'Remove favorite' : 'Add favorite'}
              onClick={() => toggleFavorite(current)}
            >
              <Heart size={15} fill={favorited ? 'currentColor' : 'none'} />
            </button>
          </>
        ) : (
          <span className="player-idle">Choose a track to start listening</span>
        )}
      </div>

      <div className="player-center">
        <div className="control-buttons">
          <button type="button" className={shuffled ? 'on' : ''} aria-label="Shuffle" aria-pressed={shuffled} onClick={shuffle}>
            <Shuffle size={18} />
          </button>
          <button type="button" aria-label="Previous track" onClick={previous}><SkipBack size={20} fill="currentColor" /></button>
          <button type="button" className="go" aria-label={playing ? 'Pause' : 'Play'} onClick={toggle}>
            {playing ? <Pause size={22} fill="currentColor" /> : <Play size={22} fill="currentColor" />}
          </button>
          <button type="button" aria-label="Next track" onClick={next}><SkipForward size={20} fill="currentColor" /></button>
          <button
            type="button"
            className={loopMode !== 'off' ? 'on' : ''}
            aria-label={loopMode === 'off' ? 'Loop off' : loopMode === 'list' ? 'Loop list' : 'Loop track'}
            aria-pressed={loopMode !== 'off'}
            onClick={cycleLoop}
          >
            {loopMode === 'track' ? <Repeat1 size={18} /> : <Repeat size={18} />}
          </button>
        </div>
        <div className="scrub">
          <span>{formatTime(position)}</span>
          <div className="scrub-track">
            <div className="scrub-fill" style={{ '--progress': progress / 100 } as CSSProperties} />
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
          <span>{formatTime(duration || 0)}</span>
        </div>
      </div>

      <div className="gain">
        <button type="button" aria-label={muted ? 'Unmute' : 'Mute'} onClick={toggleMute}>
          {muted ? <VolumeX size={15} /> : <Volume2 size={15} />}
        </button>
        <input
          aria-label="Volume"
          type="range"
          min="0"
          max={excessVolume ? 200 : 100}
          value={volume}
          onChange={e => setVolume(Number(e.target.value))}
        />
        <span>{volume}%</span>
      </div>
    </footer>
  )
}
