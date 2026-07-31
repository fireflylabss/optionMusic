import type { CSSProperties, MouseEvent } from 'react'
import { Heart, Plus } from '@phosphor-icons/react'
import type { Track } from '../types'
import { coverGlyph, folderLabel } from '../lib/music'

export function TrackRow({ track, index, current, playing, favorite, focused, play, addQueue, toggleFavorite, onFocusRow, onContext, wideLabel, virtualized }: {
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
  virtualized?: boolean
}) {
  const active = current?.id === track.id
  const style = {
    '--i': virtualized ? Math.min(index, 12) : index,
    ...(virtualized ? { top: index * 58, height: 58 } : {}),
  } as CSSProperties
  return (
    <div
      className={`row${virtualized ? ' row-virtual' : ''} ${active ? 'selected' : ''} ${focused ? 'focused' : ''}`}
      style={style}
      onContextMenu={onContext}
    >
      <div
        role="option"
        aria-selected={active}
        tabIndex={focused ? 0 : -1}
        data-track-index={index}
        className="row-hit"
        onClick={() => play(track)}
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
      </div>
      <div className="row-actions">
        <button type="button" className={favorite ? 'liked' : ''} aria-label={favorite ? 'Remove favorite' : 'Add favorite'} onClick={e => { e.stopPropagation(); toggleFavorite(track) }}>
          <Heart size={14} weight={favorite ? 'fill' : 'regular'} />
        </button>
        <button type="button" aria-label="Add to queue" onClick={e => { e.stopPropagation(); addQueue(track) }}><Plus size={15} /></button>
      </div>
    </div>
  )
}
