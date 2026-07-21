import { useEffect, useRef, useState } from 'react'
import { Disc3, FolderOpen, Plus, Settings, SlidersHorizontal, Volume2, X } from 'lucide-react'
import type { ArtistSource, BackendSettings } from '../types'
import { eqOptions } from '../lib'

export function SettingsPanel({ settings, volume, eq, folders, defaultMusicDir, addFolder, setVolume, setEq, setExcess, setLdm, setArtistSource, close }: {
  settings: BackendSettings
  volume: number
  eq: string
  folders: string[]
  defaultMusicDir: string
  addFolder: () => void
  setVolume: (v: number) => void
  setEq: (eq: string) => void
  setExcess: (v: boolean) => void
  setLdm: (v: boolean) => void
  setArtistSource: (s: ArtistSource) => void
  close: () => void
}) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const [tab, setTab] = useState<'library' | 'playback' | 'audio'>('library')
  const selected = (eqOptions as readonly string[]).includes(eq) ? eq : 'off'
  const defaultShown = !folders.some(f => f.replace(/\/+$/, '') === defaultMusicDir.replace(/\/+$/, ''))
  const artistSource: ArtistSource = settings.artist_source === 'folder' ? 'folder' : 'metadata'
  const folderCount = folders.length + (defaultShown || !folders.length ? 1 : 0)

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

  return (
    <dialog
      ref={dialogRef}
      className="popup-dialog"
      aria-labelledby="settings-title"
    >
      <button type="button" className="dialog-dismiss" aria-label="Close settings" onClick={() => dialogRef.current?.close()} />
      <div className="popup">
        <header className="popup-head">
          <div className="popup-title">
            <span className="popup-mark" aria-hidden="true"><Settings size={16} strokeWidth={2} /></span>
            <div>
              <h2 id="settings-title">Settings</h2>
              <p>Shared with CLI · ~/option/music/config.toml</p>
            </div>
          </div>
          <button type="button" className="popup-close" aria-label="Close settings" onClick={() => dialogRef.current?.close()}><X size={18} /></button>
        </header>

        <nav className="popup-tabs" aria-label="Settings sections">
          <button type="button" className={tab === 'library' ? 'on' : ''} onClick={() => setTab('library')}>
            <FolderOpen size={15} /> Library
          </button>
          <button type="button" className={tab === 'playback' ? 'on' : ''} onClick={() => setTab('playback')}>
            <Volume2 size={15} /> Playback
          </button>
          <button type="button" className={tab === 'audio' ? 'on' : ''} onClick={() => setTab('audio')}>
            <SlidersHorizontal size={15} /> Audio
          </button>
        </nav>

        <div className="popup-body" key={tab}>
          {tab === 'library' && (
            <>
              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Music folders</h3>
                  <span>{folderCount}</span>
                </div>
                <p className="popup-note">Default ~/Music is always scanned when present.</p>
                <div className="path-list">
                  {folders.map(f => (
                    <div className="path" key={f}>
                      <FolderOpen size={15} />
                      <span title={f}>{f}</span>
                    </div>
                  ))}
                  {defaultShown && (
                    <div className="path">
                      <FolderOpen size={15} />
                      <span title={defaultMusicDir || '~/Music'}>{defaultMusicDir || '~/Music'} <em>default</em></span>
                    </div>
                  )}
                  {!folders.length && !defaultShown && <p className="popup-note">No extra folders yet.</p>}
                </div>
                <button type="button" className="popup-primary" onClick={addFolder}>
                  <Plus size={16} /> Add folder
                </button>
              </div>

              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Artists source</h3>
                </div>
                <p className="popup-note">How the Artists tab groups your library. Same setting as CLI (`c`).</p>
                <div className="seg" role="group" aria-label="Artists source">
                  <button type="button" className={artistSource === 'metadata' ? 'on' : ''} onClick={() => setArtistSource('metadata')}>
                    <Disc3 size={15} /> Metadata
                  </button>
                  <button type="button" className={artistSource === 'folder' ? 'on' : ''} onClick={() => setArtistSource('folder')}>
                    <FolderOpen size={15} /> Folder
                  </button>
                </div>
              </div>
            </>
          )}

          {tab === 'playback' && (
            <>
              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Volume</h3>
                  <span>{volume}%</span>
                </div>
                <input
                  className="popup-range"
                  type="range"
                  min="0"
                  max={settings.excess_volume ? 200 : 100}
                  value={volume}
                  onChange={e => setVolume(Number(e.target.value))}
                  aria-label="Volume"
                />
              </div>

              <label className="switch-row">
                <span>
                  <strong>Excess volume</strong>
                  <small>Allow gain up to 200%</small>
                </span>
                <input type="checkbox" checked={Boolean(settings.excess_volume)} onChange={e => setExcess(e.target.checked)} />
              </label>

              <label className="switch-row">
                <span>
                  <strong>Low detail mode</strong>
                  <small>Reduce motion across desktop + CLI</small>
                </span>
                <input type="checkbox" checked={Boolean(settings.ldm)} onChange={e => setLdm(e.target.checked)} />
              </label>
            </>
          )}

          {tab === 'audio' && (
            <div className="popup-block">
              <div className="popup-block-head">
                <h3>EQ preset</h3>
              </div>
              <p className="popup-note">Applied by the MPV desktop core.</p>
              <div className="eq-grid" role="listbox" aria-label="EQ preset">
                {eqOptions.map(o => (
                  <button
                    type="button"
                    key={o}
                    role="option"
                    aria-selected={selected === o}
                    className={selected === o ? 'on' : ''}
                    onClick={() => setEq(o)}
                  >
                    {o}
                  </button>
                ))}
              </div>
            </div>
          )}
        </div>

        <footer className="popup-foot">
          <button type="button" className="popup-done" onClick={() => dialogRef.current?.close()}>Done</button>
        </footer>
      </div>
    </dialog>
  )
}
