import { useEffect, useState } from 'react'
import { invoke } from '@tauri-apps/api/core'
import { coverGlyph, hasTauriBridge } from '../lib'

export function CoverThumb({ trackId, label, className }: { trackId: string; label: string; className?: string }) {
  const [src, setSrc] = useState<string | null>(null)
  useEffect(() => {
    if (!trackId || !hasTauriBridge()) { setSrc(null); return }
    let cancelled = false
    invoke<string | null>('track_cover', { id: trackId })
      .then(url => { if (!cancelled) setSrc(url) })
      .catch(() => { if (!cancelled) setSrc(null) })
    return () => { cancelled = true }
  }, [trackId])
  return (
    <span className={`${className || ''}${src ? ' has-cover' : ''}`} aria-hidden="true">
      {src ? <img src={src} alt="" /> : coverGlyph(label)}
    </span>
  )
}
