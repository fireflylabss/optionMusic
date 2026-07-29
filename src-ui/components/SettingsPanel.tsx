import { useEffect, useRef, useState } from 'react'
import { Disc, FolderOpen, GearSix, Plus, Sliders, SpeakerHigh, X } from '@phosphor-icons/react'
import type { ArtistSource, BackendSettings, ReplayGainMode } from '../types'
import { eqOptions } from '../lib/music'
import { Button } from './ui/button'
import { Switch } from './ui/switch'

export function SettingsPanel({
  settings,
  volume,
  eq,
  speed,
  pitch,
  folders,
  defaultMusicDir,
  addFolder,
  setVolume,
  setEq,
  setExcess,
  setLdm,
  setArtistSource,
  setSpeed,
  setPitch,
  resetSpeedPitch,
  setReplaygain,
  close,
}: {
  settings: BackendSettings
  volume: number
  eq: string
  speed: number
  pitch: number
  folders: string[]
  defaultMusicDir: string
  addFolder: () => void
  setVolume: (v: number) => void
  setEq: (eq: string) => void
  setExcess: (v: boolean) => void
  setLdm: (v: boolean) => void
  setArtistSource: (s: ArtistSource) => void
  setSpeed: (v: number) => void
  setPitch: (v: number) => void
  resetSpeedPitch: () => void
  setReplaygain: (mode: ReplayGainMode) => void
  close: () => void
}) {
  const dialogRef = useRef<HTMLDialogElement>(null)
  const [tab, setTab] = useState<'library' | 'playback' | 'audio'>('library')
  const selected = (eqOptions as readonly string[]).includes(eq) ? eq : 'off'
  const defaultShown = !folders.some(f => f.replace(/\/+$/, '') === defaultMusicDir.replace(/\/+$/, ''))
  const artistSource: ArtistSource = settings.artist_source === 'folder' ? 'folder' : 'metadata'
  const folderCount = folders.length + (defaultShown || !folders.length ? 1 : 0)
  const replaygain: ReplayGainMode = settings.replaygain === 'track' || settings.replaygain === 'album' ? settings.replaygain : 'off'

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
            <span className="popup-mark" aria-hidden="true"><GearSix size={16} weight="bold" /></span>
            <div>
              <h2 id="settings-title">Settings</h2>
              <p>Shared with CLI · ~/.option/music/config.toml</p>
            </div>
          </div>
          <button type="button" className="popup-close" aria-label="Close settings" onClick={() => dialogRef.current?.close()}><X size={18} weight="bold" /></button>
        </header>

        <nav className="popup-tabs" aria-label="Settings sections">
          <button type="button" className={tab === 'library' ? 'on' : ''} onClick={() => setTab('library')}>
            <FolderOpen size={15} weight="regular" /> Library
          </button>
          <button type="button" className={tab === 'playback' ? 'on' : ''} onClick={() => setTab('playback')}>
            <SpeakerHigh size={15} weight="regular" /> Playback
          </button>
          <button type="button" className={tab === 'audio' ? 'on' : ''} onClick={() => setTab('audio')}>
            <Sliders size={15} weight="regular" /> Audio
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
                      <FolderOpen size={15} weight="regular" />
                      <span title={f}>{f}</span>
                    </div>
                  ))}
                  {defaultShown && (
                    <div className="path">
                      <FolderOpen size={15} weight="regular" />
                      <span title={defaultMusicDir || '~/Music'}>{defaultMusicDir || '~/Music'} <em>default</em></span>
                    </div>
                  )}
                  {!folders.length && !defaultShown && <p className="popup-note">No extra folders yet.</p>}
                </div>
                <Button type="button" className="w-full" onClick={addFolder}>
                  <Plus data-icon="inline-start" weight="bold" /> Add folder
                </Button>
              </div>

              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Artists source</h3>
                </div>
                <p className="popup-note">How the Artists tab groups your library. Same setting as CLI (`c`).</p>
                <div className="seg" role="group" aria-label="Artists source">
                  <button type="button" className={artistSource === 'metadata' ? 'on' : ''} onClick={() => setArtistSource('metadata')}>
                    <Disc size={15} /> Metadata
                  </button>
                  <button type="button" className={artistSource === 'folder' ? 'on' : ''} onClick={() => setArtistSource('folder')}>
                    <FolderOpen size={15} weight="regular" /> Folder
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

              <div className="switch-row">
                <span>
                  <strong>Excess volume</strong>
                  <small>Allow gain up to 200%</small>
                </span>
                <Switch
                  checked={Boolean(settings.excess_volume)}
                  onCheckedChange={setExcess}
                  aria-label="Excess volume"
                />
              </div>

              <div className="switch-row">
                <span>
                  <strong>Low detail mode</strong>
                  <small>Reduce motion across desktop + CLI</small>
                </span>
                <Switch
                  checked={Boolean(settings.ldm)}
                  onCheckedChange={setLdm}
                  aria-label="Low detail mode"
                />
              </div>
            </>
          )}

          {tab === 'audio' && (
            <>
              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Speed</h3>
                  <span>{speed.toFixed(2)}×</span>
                </div>
                <input
                  className="popup-range"
                  type="range"
                  min="0.5"
                  max="2"
                  step="0.05"
                  value={speed}
                  onChange={e => setSpeed(Number(e.target.value))}
                  aria-label="Playback speed"
                />
              </div>

              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>Pitch</h3>
                  <span>{pitch.toFixed(2)}×</span>
                </div>
                <input
                  className="popup-range"
                  type="range"
                  min="0.5"
                  max="2"
                  step="0.05"
                  value={pitch}
                  onChange={e => setPitch(Number(e.target.value))}
                  aria-label="Playback pitch"
                />
              </div>

              <Button type="button" variant="outline" className="w-full" onClick={resetSpeedPitch}>
                Reset speed & pitch
              </Button>

              <div className="popup-block">
                <div className="popup-block-head">
                  <h3>ReplayGain</h3>
                </div>
                <p className="popup-note">Normalize loudness using embedded ReplayGain tags.</p>
                <select
                  className="popup-select"
                  value={replaygain}
                  onChange={e => setReplaygain(e.target.value as ReplayGainMode)}
                  aria-label="ReplayGain mode"
                >
                  <option value="off">Off</option>
                  <option value="track">Track</option>
                  <option value="album">Album</option>
                </select>
              </div>

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
            </>
          )}
        </div>

        <footer className="popup-foot">
          <Button type="button" className="w-full" onClick={() => dialogRef.current?.close()}>Done</Button>
        </footer>
      </div>
    </dialog>
  )
}
