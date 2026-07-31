import { convertFileSrc, invoke } from '@tauri-apps/api/core'

const MAX_CONCURRENT = 3
const cache = new Map<string, string | null>()
let active = 0
const pending: Array<{ id: string; resolve: (url: string | null) => void }> = []

async function resolveCoverUrl(id: string): Promise<string | null> {
  try {
    const path = await invoke<string | null>('track_cover_url', { id })
    if (path) return convertFileSrc(path)
  } catch { /* fall through to data URL */ }
  try {
    return await invoke<string | null>('track_cover', { id })
  } catch {
    return null
  }
}

function drain() {
  while (active < MAX_CONCURRENT && pending.length) {
    const job = pending.shift()!
    active++
    resolveCoverUrl(job.id)
      .then(url => {
        cache.set(job.id, url)
        job.resolve(url)
      })
      .catch(() => {
        cache.set(job.id, null)
        job.resolve(null)
      })
      .finally(() => {
        active--
        drain()
      })
  }
}

/** Lazy cover fetch with in-memory dedupe and max 3 concurrent IPC calls. */
export function fetchTrackCover(id: string): Promise<string | null> {
  if (!id) return Promise.resolve(null)
  if (cache.has(id)) return Promise.resolve(cache.get(id)!)
  return new Promise(resolve => {
    pending.push({ id, resolve })
    drain()
  })
}
