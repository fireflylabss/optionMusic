import { useEffect, useRef, useState } from 'react'
import { DownloadSimple, MagnifyingGlass, X } from '@phosphor-icons/react'
import { invoke } from '@tauri-apps/api/core'
import type { SearchHit } from '../types'
import { formatTime, hasTauriBridge } from '../lib/music'
import { Button } from './ui/button'

const PROVIDERS = [
  { id: 'youtube', label: 'YouTube' },
  { id: 'youtubemusic', label: 'YouTube Music' },
  { id: 'soundcloud', label: 'SoundCloud' },
] as const

export function DownloadDialog({
  musicDir,
  close,
  onDownloaded,
}: {
  musicDir: string
  close: () => void
  onDownloaded?: () => void
}) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const [provider, setProvider] = useState<string>('youtube')
  const [query, setQuery] = useState('')
  const [hits, setHits] = useState<SearchHit[]>([])
  const [selected, setSelected] = useState<Set<string>>(new Set())
  const [busy, setBusy] = useState(false)
  const [status, setStatus] = useState('')
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
    void invoke('dl_ensure_yt_dlp').catch(() => { /* optional bootstrap */ })
  }, [])

  const search = async () => {
    if (!query.trim() || !hasTauriBridge()) return
    setBusy(true)
    setError('')
    setStatus('Searching…')
    setSelected(new Set())
    try {
      const results = await invoke<SearchHit[]>('dl_search', { provider, query: query.trim() })
      setHits(results)
      setStatus(results.length ? `${results.length} result${results.length === 1 ? '' : 's'}` : 'No results')
    } catch (e) {
      setHits([])
      setError(String(e).replace(/^Error:\s*/, ''))
      setStatus('')
    } finally {
      setBusy(false)
    }
  }

  const toggleHit = (url: string) => {
    setSelected(prev => {
      const next = new Set(prev)
      if (next.has(url)) next.delete(url)
      else next.add(url)
      return next
    })
  }

  const download = async () => {
    if (!selected.size || !hasTauriBridge()) return
    setBusy(true)
    setError('')
    setStatus('Downloading…')
    try {
      await invoke('dl_run', {
        urls: [...selected],
        audio: true,
        output: musicDir || null,
        audioFormat: 'mp3',
      })
      setStatus(`Downloaded ${selected.size} track${selected.size === 1 ? '' : 's'}`)
      setSelected(new Set())
      onDownloaded?.()
    } catch (e) {
      setError(String(e).replace(/^Error:\s*/, ''))
      setStatus('')
    } finally {
      setBusy(false)
    }
  }

  return (
    <dialog ref={dialogRef} className="popup-dialog" aria-labelledby="download-title">
      <button type="button" className="dialog-dismiss" aria-label="Close download" onClick={() => dialogRef.current?.close()} />
      <div className="popup popup-wide">
        <header className="popup-head">
          <div className="popup-title">
            <span className="popup-mark" aria-hidden="true"><DownloadSimple size={16} weight="bold" /></span>
            <div>
              <h2 id="download-title">Download</h2>
              <p>Search and save audio to your music folder</p>
            </div>
          </div>
          <button type="button" className="popup-close" aria-label="Close download" onClick={() => dialogRef.current?.close()}><X size={18} weight="bold" /></button>
        </header>

        <div className="popup-body download-body">
          <div className="popup-block">
            <div className="popup-block-head">
              <h3>Provider</h3>
            </div>
            <div className="seg seg-3" role="group" aria-label="Provider">
              {PROVIDERS.map(p => (
                <button
                  key={p.id}
                  type="button"
                  className={provider === p.id ? 'on' : ''}
                  onClick={() => setProvider(p.id)}
                >
                  {p.label}
                </button>
              ))}
            </div>
          </div>

          <div className="download-search">
            <input
              type="search"
              value={query}
              onChange={e => setQuery(e.target.value)}
              onKeyDown={e => { if (e.key === 'Enter') void search() }}
              placeholder="Search or paste a URL"
              aria-label="Search query"
              disabled={busy}
            />
            <Button type="button" onClick={() => void search()} disabled={busy || !query.trim()}>
              <MagnifyingGlass data-icon="inline-start" /> Search
            </Button>
          </div>

          {error && <p className="download-error" role="alert">{error}</p>}
          {status && !error && <p className="popup-note">{status}</p>}

          <div className="download-hits" role="listbox" aria-label="Search results">
            {hits.map(hit => {
              const on = selected.has(hit.url)
              return (
                <button
                  key={hit.url}
                  type="button"
                  role="option"
                  aria-selected={on}
                  className={on ? 'download-hit on' : 'download-hit'}
                  onClick={() => toggleHit(hit.url)}
                >
                  <strong>{hit.title}</strong>
                  <small>
                    {hit.uploader || 'Unknown'}
                    {hit.duration != null ? ` · ${formatTime(hit.duration)}` : ''}
                  </small>
                </button>
              )
            })}
            {!hits.length && !busy && <p className="popup-note">Search to see results.</p>}
          </div>
        </div>

        <footer className="popup-foot download-foot">
          <Button type="button" variant="outline" onClick={() => dialogRef.current?.close()}>Close</Button>
          <Button type="button" onClick={() => void download()} disabled={busy || !selected.size}>
            <DownloadSimple data-icon="inline-start" weight="bold" />
            Download{selected.size ? ` · ${selected.size}` : ''}
          </Button>
        </footer>
      </div>
    </dialog>
  )
}
