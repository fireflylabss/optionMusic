import { ExternalLink, Heart, Play, Plus, SkipForward } from 'lucide-react'
import { invoke } from '@tauri-apps/api/core'
import type { ContextState, Track } from '../types'
import { hasTauriBridge } from '../lib'

export function ContextMenu({ state, favorite, play, playNext, addQueue, toggleFavorite }: {
  state: NonNullable<ContextState>
  favorite: boolean
  play: (t: Track) => void
  playNext: (t: Track) => void
  addQueue: (t: Track) => void
  toggleFavorite: (t: Track) => void
}) {
  return (
    <div className="menu floating" style={{ left: state.x, top: state.y }} onClick={e => e.stopPropagation()}>
      <div className="menu-title">{state.track.name}</div>
      <button type="button" onClick={() => play(state.track)}><Play size={14} />Play now</button>
      <button type="button" onClick={() => playNext(state.track)}><SkipForward size={14} />Play next</button>
      <button type="button" onClick={() => addQueue(state.track)}><Plus size={14} />Add to queue</button>
      <button type="button" onClick={() => toggleFavorite(state.track)}><Heart size={14} />{favorite ? 'Remove favorite' : 'Add to favorites'}</button>
      <div className="menu-rule" />
      <button type="button" onClick={() => hasTauriBridge() && invoke('reveal_in_file_manager', { path: state.track.path })}><ExternalLink size={14} />Reveal in folder</button>
    </div>
  )
}
