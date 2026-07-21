import type { CSSProperties, MouseEvent } from 'react'
import { Heart, Plus } from 'lucide-react'
import type { Track } from '../types'
import { coverGlyph, folderLabel } from '../lib'

export function TrackRow({ track, index, current, playing, favorite, focused, play, addQueue, toggleFavorite, onFocusRow, onContext, wideLabel }: {
  track: Track
  index: number
  current: Track | null
  playing: boolean
  favorite: boolean
  focused: boolean
  play: (t: Track) => void
  addQueue: (t: Track) => void
  toggleFavorite: (t: Track) => void
  onFocusRow: () => void
  onContext: (e: MouseEvent) => void
  wideLabel?: string
}) {
  const active = current?.id === track.id
  return (
    <div
      role="option"
      aria-selected={active}
      tabIndex={focused ? 0 : -1}
      data-track-index={index}
      className={`row ${active ? 'selected' : ''} ${focused ? 'focused' : ''}`}
      style={{ '--i': index } as CSSProperties}
      onClick={() => play(track)}
      onContextMenu={onContext}
      onFocus={onFocusRow}
      onKeyDown={e => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); play(track) } }}
    >
      <span className="idx">
        {active && playing ? <span className="bars" aria-hidden="true"><i /><i /><i /></span> : String(index + 1).padStart(2, '0')}
      </span>
      <span className="glyph" aria-hidden="true">{coverGlyph(track.name)}</span>
      <div className="row-main">
        <strong>{track.name}</strong>
        <small title={track.path}>{track.artist || track.path}</small>
      </div>
      <span className="folder wide" title={wideLabel || track.folder}>{wideLabel || folderLabel(track.folder)}</span>
      <div className="row-actions">
        <button type="button" className={favorite ? 'liked' : ''} aria-label={favorite ? 'Remove favorite' : 'Add favorite'} onClick={e => { e.stopPropagation(); toggleFavorite(track) }}>
          <Heart size={14} fill={favorite ? 'currentColor' : 'none'} />
        </button>
        <button type="button" aria-label="Add to queue" onClick={e => { e.stopPropagation(); addQueue(track) }}><Plus size={15} /></button>
      </div>
    </div>
  )
}
