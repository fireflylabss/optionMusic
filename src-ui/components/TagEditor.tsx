import { useEffect, useRef, useState } from 'react'
import { PencilSimple, X } from '@phosphor-icons/react'
import { invoke } from '@tauri-apps/api/core'
import type { AudioTags, Track } from '../types'
import { hasTauriBridge } from '../lib/music'
import { Button } from './ui/button'

export function TagEditor({
  track,
  close,
  onSaved,
}: {
  track: Track
  close: () => void
  onSaved?: (track: Track) => void
}) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const [title, setTitle] = useState(track.name)
  const [artist, setArtist] = useState(track.artist || '')
  const [album, setAlbum] = useState(track.album || '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')

  useEffect(() => {
    const el = dialogRef.current
    if (!el) return
    if (!el.open) el.showModal()
    const onClose = () => close()
    el.addEventListener('close', onClose)
    return () => {
      el.removeEventListener('close', onClose)
      if (el.open) el.close()
    }
  }, [close])

  useEffect(() => {
    if (!hasTauriBridge()) return
    let cancelled = false
    invoke<AudioTags>('get_track_tags', { id: track.id })
      .then(tags => {
        if (cancelled) return
        if (tags.title != null) setTitle(tags.title)
        if (tags.artist != null) setArtist(tags.artist)
        if (tags.album != null) setAlbum(tags.album)
      })
      .catch(() => { /* keep track fields */ })
    return () => { cancelled = true }
  }, [track.id])

  const save = async () => {
    if (!hasTauriBridge()) return
    setBusy(true)
    setError('')
    try {
      const tags: AudioTags = {
        title: title.trim() || null,
        artist: artist.trim() || null,
        album: album.trim() || null,
      }
      const updated = await invoke<Track>('set_track_tags', { id: track.id, tags })
      onSaved?.(updated)
      dialogRef.current?.close()
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ''))
    } finally {
      setBusy(false)
    }
  }

  return (
    <dialog ref={dialogRef} className="popup-dialog" aria-labelledby="tags-title">
      <button type="button" className="dialog-dismiss" aria-label="Close tag editor" onClick={() => dialogRef.current?.close()} />
      <div className="popup">
        <header className="popup-head">
          <div className="popup-title">
            <span className="popup-mark" aria-hidden="true"><PencilSimple size={16} weight="bold" /></span>
            <div>
              <h2 id="tags-title">Edit tags</h2>
              <p title={track.path}>{track.name}</p>
            </div>
          </div>
          <button type="button" className="popup-close" aria-label="Close tag editor" onClick={() => dialogRef.current?.close()}><X size={18} weight="bold" /></button>
        </header>

        <div className="popup-body">
          <label className="tag-field">
            <span>Title</span>
            <input value={title} onChange={e => setTitle(e.target.value)} disabled={busy} />
          </label>
          <label className="tag-field">
            <span>Artist</span>
            <input value={artist} onChange={e => setArtist(e.target.value)} disabled={busy} />
          </label>
          <label className="tag-field">
            <span>Album</span>
            <input value={album} onChange={e => setAlbum(e.target.value)} disabled={busy} />
          </label>
          {error && <p className="download-error" role="alert">{error}</p>}
        </div>

        <footer className="popup-foot download-foot">
          <Button type="button" variant="outline" onClick={() => dialogRef.current?.close()}>Cancel</Button>
          <Button type="button" onClick={() => void save()} disabled={busy}>Save</Button>
        </footer>
      </div>
    </dialog>
  )
}
