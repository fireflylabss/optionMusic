import { getCurrentWindow } from '@tauri-apps/api/window'
import { Minus, Plus, X } from 'lucide-react'
import { hasTauriBridge } from '../lib'

async function runWindowAction(action: 'close' | 'minimize' | 'maximize') {
  if (!hasTauriBridge()) return
  const win = getCurrentWindow()
  if (action === 'close') await win.close()
  else if (action === 'minimize') await win.minimize()
  else await win.toggleMaximize()
}

export function WindowControls() {
  return (
    <div className="traffic" role="group" aria-label="Window controls">
      <button type="button" className="tl close" aria-label="Close" onClick={e => { e.stopPropagation(); void runWindowAction('close') }}><X size={9} strokeWidth={3} /></button>
      <button type="button" className="tl min" aria-label="Minimize" onClick={e => { e.stopPropagation(); void runWindowAction('minimize') }}><Minus size={9} strokeWidth={3} /></button>
      <button type="button" className="tl max" aria-label="Maximize" onClick={e => { e.stopPropagation(); void runWindowAction('maximize') }}><Plus size={9} strokeWidth={3} /></button>
    </div>
  )
}
