import { useEffect, useRef } from 'react'
import { Search } from 'lucide-react'
import type { Track } from '../types'
import { coverGlyph, folderLabel, searchChord } from '../lib'

export function CommandPalette({
  search,
  setSearch,
  hits,
  commandIndex,
  setCommandIndex,
  currentId,
  play,
  close,
}: {
  search: string
  setSearch: (v: string) => void
  hits: Track[]
  commandIndex: number
  setCommandIndex: (i: number) => void
  currentId?: string
  play: (t: Track) => void
  close: () => void
}) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const searchRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    const el = dialogRef.current
    if (!el) return
    if (!el.open) el.showModal()
    requestAnimationFrame(() => searchRef.current?.focus())
    const onClose = () => close()
    el.addEventListener('close', onClose)
    return () => {
      el.removeEventListener('close', onClose)
      if (el.open) el.close()
    }
  }, [close])

  return (
    <dialog
      ref={dialogRef}
      className="cmd-dialog"
      aria-label="Search library"
    >
      <button type="button" className="dialog-dismiss" aria-label="Close search" onClick={() => dialogRef.current?.close()} />
      <div className="cmd">
        <label className="cmd-input">
          <Search size={18} />
          <input
            ref={searchRef}
            value={search}
            onChange={e => setSearch(e.target.value)}
            placeholder="Search tracks, artists, albums…"
            aria-label="Search"
          />
          <kbd>{searchChord}</kbd>
        </label>
        <div className="cmd-list" role="listbox">
          {hits.length ? hits.map((t, i) => (
            <button
              type="button"
              key={t.id}
              role="option"
              aria-selected={commandIndex === i}
              className={commandIndex === i ? 'on' : ''}
              onMouseEnter={() => setCommandIndex(i)}
              onClick={() => play(t)}
            >
              <span className="cmd-glyph" aria-hidden="true">{coverGlyph(t.name)}</span>
              <span className="cmd-copy">
                <strong>{t.name}</strong>
                <small>{[t.artist || folderLabel(t.folder), t.album].filter(Boolean).join(' · ')}</small>
              </span>
              {currentId === t.id && <em>Playing</em>}
            </button>
          )) : (
            <p className="cmd-empty">{search.trim() ? 'No matches' : 'Library is empty'}</p>
          )}
        </div>
        <div className="cmd-foot">
          <span><kbd>↑↓</kbd> navigate</span>
          <span><kbd>↵</kbd> play</span>
          <span><kbd>esc</kbd> close</span>
        </div>
      </div>
    </dialog>
  )
}
