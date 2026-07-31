import { useEffect, useRef, useState } from 'react'
import { coverGlyph, hasTauriBridge } from '../lib/music'
import { fetchTrackCover } from '../coverQueue'

type CoverThumbProps = { trackId: string; label: string; className?: string }

/** Remounts when `trackId` changes so visibility/src state reset without an effect. */
export function CoverThumb({ trackId, label, className }: CoverThumbProps) {
  return <CoverThumbInner key={trackId} trackId={trackId} label={label} className={className} />
}

function CoverThumbInner({ trackId, label, className }: CoverThumbProps) {
  const rootRef = useRef<HTMLSpanElement>(null)
  const [visible, setVisible] = useState(false)
  const [src, setSrc] = useState<string | null>(null)

  useEffect(() => {
    const el = rootRef.current
    if (!el || !trackId || !hasTauriBridge()) return
    const io = new IntersectionObserver(
      ([entry]) => {
        if (entry.isIntersecting) {
          setVisible(true)
          io.disconnect()
        }
      },
      { rootMargin: '120px' },
    )
    io.observe(el)
    return () => io.disconnect()
  }, [trackId])

  useEffect(() => {
    if (!visible || !trackId || !hasTauriBridge()) return
    let cancelled = false
    fetchTrackCover(trackId).then(url => {
      if (!cancelled) setSrc(url)
    })
    return () => { cancelled = true }
  }, [visible, trackId])

  return (
    <span ref={rootRef} className={`${className || ''}${src ? ' has-cover' : ''}`} aria-hidden="true">
      {src ? <img src={src} alt="" /> : coverGlyph(label)}
    </span>
  )
}
