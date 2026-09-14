import { useEffect, useRef, useState } from 'react'
import {
  ArrowRight,
  ChevronDown,
  Download,
  Keyboard,
  Music,
  Pause,
  Play,
  Terminal,
  Volume2,
  VolumeX,
} from 'lucide-react'

import { Reveal } from '@shared/components/Reveal'
import { Badge } from '@shared/components/ui/badge'
import { Button } from '@shared/components/ui/button'
import {
  Card,
  CardDescription,
  CardHeader,
  CardTitle,
} from '@shared/components/ui/card'

const enter = (ms: number) => ({
  className: 'option-enter',
  style: { animationDelay: `${ms}ms` },
})

function DemoPlayer() {
  const videoRef = useRef<HTMLVideoElement | null>(null)
  const [playing, setPlaying] = useState(false)
  const [muted, setMuted] = useState(true)
  const [reducedMotion] = useState(
    () =>
      typeof window !== 'undefined' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches,
  )

  useEffect(() => {
    if (videoRef.current) videoRef.current.muted = true
  }, [])

  const togglePlay = () => {
    const v = videoRef.current
    if (!v) return
    if (v.paused) void v.play().catch(() => {})
    else v.pause()
  }

  const toggleMute = () => {
    const v = videoRef.current
    if (!v) return
    const next = !muted
    v.muted = next
    setMuted(next)
  }

  return (
    <div {...enter(280)} className="option-enter w-full">
      <div className="group overflow-hidden rounded-2xl border border-border bg-mono-1000 shadow-[0_2px_20px_-4px_rgba(0,0,0,0.12)] dark:shadow-[0_2px_24px_-4px_rgba(0,0,0,0.35)]">
        <div className="relative">
          <video
            ref={videoRef}
            className="block h-auto w-full outline-1 -outline-offset-1 outline-black/10 dark:outline-white/10"
            src="/demo.mp4"
            poster="/poster.jpg"
            aria-label="Demo video: optionMusic playing a local track"
            autoPlay={!reducedMotion}
            muted
            loop
            playsInline
            preload="auto"
            onClick={togglePlay}
            onPlay={() => setPlaying(true)}
            onPause={() => setPlaying(false)}
          />
          <div className="absolute inset-x-0 bottom-0 flex items-center justify-between px-3 pt-6 pb-3 text-mono-0">
            <button
              type="button"
              onClick={togglePlay}
              aria-label={playing ? 'Pause demo' : 'Play demo'}
              className="grid size-7 flex-none place-items-center rounded-md bg-mono-1000/60 backdrop-blur-sm transition-[transform,opacity] duration-(--duration-fast) ease-(--ease-option) hover:bg-mono-1000/80 active:scale-[0.96]"
            >
              {playing ? (
                <Pause
                  className="size-3.5"
                  strokeWidth={1.5}
                  fill="currentColor"
                />
              ) : (
                <Play
                  className="size-3.5 translate-x-px"
                  strokeWidth={1.5}
                  fill="currentColor"
                />
              )}
            </button>
            <button
              type="button"
              onClick={toggleMute}
              aria-label={muted ? 'Unmute demo' : 'Mute demo'}
              className="grid size-7 flex-none place-items-center rounded-md bg-mono-1000/60 backdrop-blur-sm transition-[transform,opacity,background-color] duration-(--duration-fast) ease-(--ease-option) hover:bg-mono-1000/80 active:scale-[0.96]"
            >
              {muted ? (
                <VolumeX className="size-4" strokeWidth={1.5} />
              ) : (
                <Volume2 className="size-4" strokeWidth={1.5} />
              )}
            </button>
          </div>
        </div>
      </div>
    </div>
  )
}

function Kbd({ children }: { children: React.ReactNode }) {
  return (
    <kbd className="inline-flex h-6 min-w-6 items-center justify-center rounded-md border border-border bg-card px-1.5 font-mono text-[11px] text-foreground">
      {children}
    </kbd>
  )
}

const SHORTCUTS: { keys: string[]; action: string }[] = [
  { keys: ['space'], action: 'play / pause' },
  { keys: ['j', 'k'], action: 'move' },
  { keys: ['/'], action: 'search' },
  { keys: ['q'], action: 'queue' },
  { keys: ['+', '−'], action: 'volume' },
  { keys: ['?'], action: 'all keys' },
]

const FAMILY: {
  name: string
  desc: string
  channel: 'stable' | 'beta' | 'alpha'
}[] = [
  { name: 'optionMusic', desc: 'Music player for the terminal', channel: 'stable' },
  { name: 'optionTerm', desc: 'Terminal emulator, GPU-rendered', channel: 'beta' },
  { name: 'optionFiles', desc: 'GTK file manager, mono-first', channel: 'beta' },
  { name: 'optionNotes', desc: 'Plain-text notes, local-first', channel: 'alpha' },
  { name: 'optionCalendar', desc: 'CalDAV calendar, offline-capable', channel: 'alpha' },
]

const channelVariant = {
  stable: 'success',
  beta: 'info',
  alpha: 'warning',
} as const

const STEPS: { n: string; title: string; cmd: string; desc: string }[] = [
  {
    n: '01',
    title: 'Index',
    cmd: 'optionmusic add ~/Music',
    desc: 'Point it at a folder. The library index builds once, then stays warm.',
  },
  {
    n: '02',
    title: 'Play',
    cmd: 'optionmusic play --shuffle',
    desc: 'Albums, artists, playlists or a single search — straight to audio.',
  },
  {
    n: '03',
    title: 'Stay',
    cmd: 'optionmusic',
    desc: 'The TUI stays open in a corner of your terminal. Out of the way, always on.',
  },
]

const STATS: { value: string; label: string }[] = [
  { value: '~12ms', label: 'cold start' },
  { value: '~6MB', label: 'single binary' },
  { value: '0', label: 'telemetry & accounts' },
]

const RELEASES: {
  v: string
  date: string
  channel: 'stable' | 'beta' | 'mixed'
  note: string
}[] = [
  {
    v: 'v0.2.16',
    date: '12/09/2026',
    channel: 'beta',
    note: 'Discord Rich Presence, session memory — resume where you left off',
  },
  {
    v: 'v0.2.15',
    date: '04/09/2026',
    channel: 'stable',
    note: 'Floating panels, docked karaoke lyrics, hardened downloader',
  },
  {
    v: 'v0.2.14',
    date: '29/08/2026',
    channel: 'beta',
    note: 'Library core: incremental refresh, artists → albums → tracks',
  },
  {
    v: 'v0.2.13',
    date: '22/08/2026',
    channel: 'stable',
    note: '`optmusic` alias removed — `optionmusic` and `msc` remain',
  },
  {
    v: 'v0.2.12m',
    date: '03/08/2026',
    channel: 'mixed',
    note: 'Shared path ownership, atomic playlist/config writes',
  },
]

const releaseVariant = {
  stable: 'success',
  beta: 'info',
  mixed: 'warning',
} as const

const FAQ: { q: string; a: string }[] = [
  {
    q: 'Which platforms are supported?',
    a: 'Linux today — through the AUR package or a release tarball. macOS and Windows builds land once the core player is stable.',
  },
  {
    q: 'Which audio formats does it play?',
    a: 'flac, mp3, ogg, opus and m4a out of the box. If your system decodes it, optionMusic plays it.',
  },
  {
    q: 'Is there a GUI version?',
    a: 'No — optionMusic is terminal-first by design. If you want a graphical surface, the GTK siblings in the Option family share the same design tokens.',
  },
  {
    q: 'Does it stream from Spotify or other services?',
    a: 'No. It plays your local library and nothing else — no accounts, no network calls, no telemetry.',
  },
  {
    q: 'How do I theme it?',
    a: 'You don\'t. It follows the optionDesign token layer, so it matches the rest of the family — light or warm dark, monochrome with a single accent.',
  },
  {
    q: 'Where does it keep its state?',
    a: 'Config in $XDG_CONFIG_HOME/option/music.toml, cache and library index under $XDG_DATA_HOME/option/. Plain files, nothing hidden.',
  },
]

function HomePage() {
  return (
    <div className="mx-auto w-full max-w-3xl px-6">
      <section className="pt-10 pb-16 md:pt-16">
        <h1
          {...enter(0)}
          className="option-enter max-w-2xl font-sans text-display-md text-foreground md:text-display"
        >
          All signal. No noise.
        </h1>
        <p
          {...enter(70)}
          className="option-enter mt-6 max-w-lg text-lg leading-relaxed text-muted-foreground"
        >
          A quiet surface over a serious player — indexed library, playlists,
          offline lyrics, a downloader. Everything's there, nothing's in the
          way.
        </p>
        <div
          {...enter(140)}
          className="option-enter mt-8 flex flex-wrap items-center gap-4"
        >
          <Button asChild>
            <a href="#download">
              Download <Download className="ml-2 size-4" strokeWidth={1.5} />
            </a>
          </Button>
          <Button variant="outline" asChild>
            <a
              href="https://github.com/fireflylabss/optionMusic"
              target="_blank"
              rel="noreferrer"
            >
              GitHub ↗
            </a>
          </Button>
        </div>
      </section>

      <DemoPlayer />

      <section className="grid gap-6 py-24 md:grid-cols-3">
        <Reveal>
          <Card className="h-full transition-[transform,border-color] duration-(--duration-base) ease-(--ease-option) hover:-translate-y-0.5 hover:border-mono-300 dark:hover:border-mono-600">
            <CardHeader>
              <Terminal className="mb-2 size-5 text-muted-foreground" strokeWidth={1.5} />
              <CardTitle as="h2" className="font-sans text-title">
                CLI-first
              </CardTitle>
              <CardDescription>
                Runs in your terminal. Starts in milliseconds and stays out of
                your way.
              </CardDescription>
            </CardHeader>
          </Card>
        </Reveal>

        <Reveal delay={90}>
          <Card className="h-full transition-[transform,border-color] duration-(--duration-base) ease-(--ease-option) hover:-translate-y-0.5 hover:border-mono-300 dark:hover:border-mono-600">
            <CardHeader>
              <Keyboard className="mb-2 size-5 text-muted-foreground" strokeWidth={1.5} />
              <CardTitle as="h2" className="font-sans text-title">
                Keyboard driven
              </CardTitle>
              <CardDescription>
                Play, pause, queue, search and jump with a minimal set of
                vim-like shortcuts.
              </CardDescription>
            </CardHeader>
          </Card>
        </Reveal>

        <Reveal delay={180}>
          <Card className="h-full transition-[transform,border-color] duration-(--duration-base) ease-(--ease-option) hover:-translate-y-0.5 hover:border-mono-300 dark:hover:border-mono-600">
            <CardHeader>
              <Music className="mb-2 size-5 text-muted-foreground" strokeWidth={1.5} />
              <CardTitle as="h2" className="font-sans text-title">
                Pure monochrome
              </CardTitle>
              <CardDescription>
                Warm grays. No gradients, no covers, no visual clutter — the
                music is the color.
              </CardDescription>
            </CardHeader>
          </Card>
        </Reveal>
      </section>

      <Reveal>
        <section className="overflow-hidden rounded-xl border border-border bg-card">
          <div className="grid divide-y divide-border sm:grid-cols-3 sm:divide-x sm:divide-y-0">
            {STATS.map((s) => (
              <div key={s.label} className="p-5">
                <p className="font-mono text-lg text-foreground">{s.value}</p>
                <p className="mt-1.5 font-mono text-[10px] uppercase tracking-[0.18em] text-muted-foreground">
                  {s.label}
                </p>
              </div>
            ))}
          </div>
          <p className="flex flex-wrap items-baseline justify-between gap-x-6 gap-y-1 border-t border-border px-5 py-4 font-mono text-xs text-muted-foreground">
            <span className="text-foreground">flac mp3 ogg opus m4a</span>
            <span>and whatever your system decodes</span>
          </p>
        </section>
      </Reveal>

      <section className="border-t border-border py-20">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          how it works
        </p>
        <h2 className="mt-3 font-sans text-display-md">Point it at a folder</h2>
        <div className="mt-10 grid gap-10 md:grid-cols-3">
          {STEPS.map((s, i) => (
            <Reveal key={s.n} delay={i * 90}>
              <div>
              <p className="font-mono text-sm text-muted-foreground">{s.n}</p>
              <h3 className="mt-2 font-sans text-title">{s.title}</h3>
              <p className="mt-2 text-sm leading-relaxed text-muted-foreground">
                {s.desc}
              </p>
              <p className="mt-4 rounded-md border border-border bg-card px-3 py-2 font-mono text-xs text-foreground">
                <span className="text-muted-foreground">$</span> {s.cmd}
              </p>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      <section className="border-t border-border py-20">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          shortcuts
        </p>
        <h2 className="mt-3 font-sans text-display-md">Ten keys, maybe</h2>
        <p className="mt-4 max-w-md leading-relaxed text-muted-foreground">
          The whole interface fits in a handful of keys. No modes to memorize —
          press <Kbd>?</Kbd> anywhere to see all of them.
        </p>
        <div className="mt-8 grid gap-3 sm:grid-cols-2">
          {SHORTCUTS.map((s, i) => (
            <Reveal key={s.action} delay={i * 50}>
              <div className="flex items-center justify-between gap-4 rounded-lg border border-border px-4 py-3 transition-colors duration-(--duration-fast) hover:border-mono-300 dark:hover:border-mono-600">
                <span className="flex items-center gap-1.5">
                  {s.keys.map((k) => (
                    <Kbd key={k}>{k}</Kbd>
                  ))}
                </span>
                <span className="text-sm text-muted-foreground">{s.action}</span>
              </div>
            </Reveal>
          ))}
        </div>
      </section>

      <section className="border-t border-border py-20">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          option family
        </p>
        <h2 className="mt-3 font-sans text-display-md">
          One palette, every surface
        </h2>
        <p className="mt-4 max-w-md leading-relaxed text-muted-foreground">
          optionMusic is part of a family of small tools that share the same
          token-driven design system — from TUI to GTK.
        </p>
        <Reveal delay={90}>
        <ul className="mt-8 divide-y divide-border border-y border-border">
          {FAMILY.map((app) => (
            <li
              key={app.name}
              className="group flex items-center justify-between gap-4 py-4 transition-colors duration-(--duration-fast) hover:bg-muted/50"
            >
              <div className="min-w-0">
                <p className="font-sans text-title">{app.name}</p>
                <p className="mt-0.5 truncate text-sm text-muted-foreground">
                  {app.desc}
                </p>
              </div>
              <div className="flex items-center gap-3">
                <Badge variant={channelVariant[app.channel]}>
                  {app.channel}
                </Badge>
                <ArrowRight
                  className="size-4 text-muted-foreground opacity-0 transition-opacity duration-(--duration-fast) group-hover:opacity-100"
                  strokeWidth={1.5}
                />
              </div>
            </li>
          ))}
        </ul>
        </Reveal>
      </section>

      <section className="border-t border-border py-20">
        <div className="grid gap-12 md:grid-cols-2 md:items-center">
          <Reveal>
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
              local-first
            </p>
            <h2 className="mt-3 font-sans text-display-md">
              Your library stays yours
            </h2>
            <p className="mt-4 leading-relaxed text-muted-foreground">
              All state lives in plain files under standard XDG paths. Works
              fully offline — no accounts, no sync service, nothing leaves the
              machine.
            </p>
            <ul className="mt-6 space-y-2 text-sm text-muted-foreground">
              <li className="flex items-baseline gap-3">
                <span className="text-foreground">—</span> Plain-text config you
                can diff and version
              </li>
              <li className="flex items-baseline gap-3">
                <span className="text-foreground">—</span> Library index is a
                local database, not a service
              </li>
              <li className="flex items-baseline gap-3">
                <span className="text-foreground">—</span> Delete the folder and
                it's gone. No account to cancel
              </li>
            </ul>
          </div>
          </Reveal>
          <Reveal delay={120}>
          <div className="overflow-hidden rounded-xl border border-border bg-card font-mono text-sm">
            <div className="border-b border-border px-4 py-3 text-xs text-muted-foreground">
              filesystem
            </div>
            <div className="space-y-1.5 p-5 text-xs text-muted-foreground">
              <p>
                <span className="text-foreground">~/.config/option/</span>
                music.toml
              </p>
              <p>
                <span className="text-foreground">~/.local/share/option/</span>
                music/library.db
              </p>
              <p>
                <span className="text-foreground">~/.local/state/option/</span>
                music/logs/
              </p>
              <p>
                <span className="text-foreground">~/Music/</span>
                **/* <span className="text-muted-foreground">← your files</span>
              </p>
            </div>
          </div>
          </Reveal>
        </div>
      </section>

      <section id="releases" className="border-t border-border py-20">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          releases
        </p>
        <h2 className="mt-3 font-sans text-display-md">Changelog</h2>
        <Reveal delay={90}>
        <div className="mt-8 border-y border-border">
          {RELEASES.map((r) => (
            <div
              key={r.v}
              className="flex items-baseline gap-4 border-b border-border py-3 font-mono text-sm last:border-b-0 sm:gap-6"
            >
              <span className="w-20 shrink-0 text-foreground">{r.v}</span>
              <span className="hidden w-24 shrink-0 text-xs text-muted-foreground sm:inline">
                {r.date}
              </span>
              <Badge variant={releaseVariant[r.channel]}>{r.channel}</Badge>
              <span className="min-w-0 text-muted-foreground">{r.note}</span>
            </div>
          ))}
        </div>
        </Reveal>
        <a
          href="https://github.com/fireflylabss/optionMusic/blob/main/CHANGELOG.md"
          target="_blank"
          rel="noreferrer"
          className="mt-6 inline-flex items-center text-sm text-muted-foreground transition-colors duration-(--duration-fast) hover:text-foreground"
        >
          Full changelog{' '}
          <ArrowRight className="ml-1.5 size-4" strokeWidth={1.5} />
        </a>
      </section>

      <section id="faq" className="border-t border-border py-20">
        <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-muted-foreground">
          faq
        </p>
        <h2 className="mt-3 font-sans text-display-md">Questions</h2>
        <Reveal delay={90}>
        <div className="mt-8">
          {FAQ.map((item) => (
            <details
              key={item.q}
              className="group border-b border-border pt-5 first:border-t"
            >
              <summary className="flex cursor-pointer list-none items-center justify-between gap-4 pb-5 font-sans text-base font-medium [&::-webkit-details-marker]:hidden">
                {item.q}
                <ChevronDown
                  className="size-4 shrink-0 text-muted-foreground transition-transform duration-(--duration-fast) ease-(--ease-option) group-open:rotate-180"
                  strokeWidth={1.5}
                />
              </summary>
              <div className="grid grid-rows-[0fr] transition-[grid-template-rows] duration-(--duration-med) ease-(--ease-option) group-open:grid-rows-[1fr]">
                <div className="overflow-hidden">
                  <p className="max-w-xl pb-5 leading-relaxed text-muted-foreground">
                    {item.a}
                  </p>
                </div>
              </div>
            </details>
          ))}
        </div>
        </Reveal>
      </section>

      <section id="download" className="border-t border-border py-20">
        <h2 className="font-sans text-display-md">Get optionMusic</h2>
        <p className="mt-4 max-w-md text-muted-foreground">
          Install from the AUR or grab a release tarball. Linux builds are
          available now.
        </p>
        <Reveal delay={90}>
        <div className="mt-8 overflow-hidden rounded-xl border border-border bg-card font-mono text-sm">
          <div className="space-y-2 p-5">
            <p>
              <span className="text-muted-foreground"># arch / AUR</span>
              <br />
              <span className="text-muted-foreground">$</span> paru -S
              optionmusic
            </p>
            <p>
              <span className="text-muted-foreground"># everywhere else</span>
              <br />
              <span className="text-muted-foreground">$</span> cargo install
              optionmusic
            </p>
            <p>
              <span className="text-muted-foreground">$</span>{' '}
              <span className="option-blink">▮</span>
            </p>
          </div>
        </div>
        </Reveal>
        <div className="mt-8 flex flex-wrap items-center gap-4">
          <Button asChild>
            <a href="#download">
              Download for Linux{' '}
              <ArrowRight className="ml-2 size-4" strokeWidth={1.5} />
            </a>
          </Button>
          <Button variant="outline" asChild>
            <a href="https://github.com/fireflylabss/optionMusic" target="_blank" rel="noreferrer">
              Build from source
            </a>
          </Button>
        </div>
      </section>
    </div>
  )
}

export { HomePage }
