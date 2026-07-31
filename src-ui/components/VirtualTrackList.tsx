import { useEffect, useMemo, useRef, useState, type MouseEvent, type RefObject } from 'react'
import type { Track } from '../types'
import { folderLabel } from '../lib/music'
import { TrackRow } from './TrackRow'

const ROW_HEIGHT = 58
const OVERSCAN = 10

export function VirtualTrackList({
  tracks,
  wideColumn,
  current,
  playing,
  favoriteIds,
  focusedIndex,
  listRef,
  play,
  addQueue,
  toggleFavorite,
  setFocusedIndex,
  openContext,
}: {
  tracks: Track[]
  wideColumn: 'album' | 'folder'
  current: Track | null
  playing: boolean
  favoriteIds: Set<string>
  focusedIndex: number
  listRef: RefObject<HTMLDivElement | null>
  play: (t: Track) => void
  addQueue: (t: Track) => void
  toggleFavorite: (t: Track) => void
  setFocusedIndex: (i: number) => void
  openContext: (e: MouseEvent, t: Track) => void
}) {
  const scrollRef = useRef<HTMLDivElement>(null)
  const [range, setRange] = useState({ start: 0, end: 30 })

  useEffect(() => {
    const node = scrollRef.current
    if (listRef && 'current' in listRef) {
      ;(listRef as { current: HTMLDivElement | null }).current = node
    }
  })

  useEffect(() => {
    const el = scrollRef.current
    if (!el) return
    const update = () => {
      const { scrollTop, clientHeight } = el
      const start = Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN)
      const end = Math.min(tracks.length, Math.ceil((scrollTop + clientHeight) / ROW_HEIGHT) + OVERSCAN)
      setRange(prev => (prev.start === start && prev.end === end ? prev : { start, end }))
    }
    update()
    el.addEventListener('scroll', update, { passive: true })
    const ro = new ResizeObserver(update)
    ro.observe(el)
    return () => {
      el.removeEventListener('scroll', update)
      ro.disconnect()
    }
  }, [tracks.length])

  const indices = useMemo(() => {
    const out: number[] = []
    for (let i = range.start; i < range.end; i++) out.push(i)
    return out
  }, [range.start, range.end])

  const wideLabel = (t: Track) =>
    wideColumn === 'album' ? t.album || folderLabel(t.folder) : folderLabel(t.folder)

  return (
    <div className="catalog-list" role="listbox" aria-label="Tracks">
      <div className="catalog-cols" aria-hidden="true">
        <span>#</span>
        <span />
        <span>Title</span>
        <span className="wide">{wideColumn === 'album' ? 'Album' : 'Folder'}</span>
        <span />
      </div>
      <div className="catalog-list-scroll" ref={scrollRef}>
        <div className="catalog-list-virtual" style={{ height: tracks.length * ROW_HEIGHT }}>
          {indices.map(i => {
            const t = tracks[i]
            return (
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
                wideLabel={wideLabel(t)}
                virtualized
              />
            )
          })}
        </div>
      </div>
    </div>
  )
}
