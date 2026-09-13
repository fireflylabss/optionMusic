import {
  createContext,
  useCallback,
  useEffect,
  useRef,
  useState,
  useSyncExternalStore,
} from 'react'

type Theme = 'light' | 'dark' | 'system'
type ResolvedTheme = 'light' | 'dark'

/** Anything with click coordinates — a React MouseEvent works as-is. */
interface ThemeToggleOrigin {
  clientX?: number
  clientY?: number
  currentTarget?: EventTarget | null
}

interface ThemeContext {
  theme: Theme
  resolved: ResolvedTheme
  setTheme: (theme: Theme) => void
  toggle: (origin?: ThemeToggleOrigin) => void
}

const ThemeContext = createContext<ThemeContext | null>(null)

const THEME_FADE_MS = 360

function getSystemTheme(): ResolvedTheme {
  if (typeof window === 'undefined') return 'light'
  return window.matchMedia('(prefers-color-scheme: dark)').matches
    ? 'dark'
    : 'light'
}

function subscribeToSystem(callback: () => void) {
  const mql = window.matchMedia('(prefers-color-scheme: dark)')
  mql.addEventListener('change', callback)
  return () => mql.removeEventListener('change', callback)
}

function applyResolved(resolved: ResolvedTheme) {
  const root = document.documentElement
  root.classList.add('option-disable-transitions')
  root.classList.toggle('dark', resolved === 'dark')
  void root.offsetHeight
  root.classList.remove('option-disable-transitions')
}

/** Fallback for browsers without the View Transitions API. */
function fadeTo(resolved: ResolvedTheme) {
  const root = document.documentElement
  root.classList.add('option-theme-fade')
  root.classList.toggle('dark', resolved === 'dark')
  window.setTimeout(
    () => root.classList.remove('option-theme-fade'),
    THEME_FADE_MS,
  )
}

function toggleOriginPoint(origin?: ThemeToggleOrigin): {
  x: number
  y: number
} {
  let x = origin?.clientX
  let y = origin?.clientY
  // Keyboard-triggered clicks report 0,0 — reveal from the control itself.
  if (!x && !y && origin?.currentTarget instanceof Element) {
    const rect = origin.currentTarget.getBoundingClientRect()
    x = rect.left + rect.width / 2
    y = rect.top + rect.height / 2
  }
  return {
    x: x ?? window.innerWidth / 2,
    y: y ?? window.innerHeight / 2,
  }
}

function ThemeProvider({ children }: { children: React.ReactNode }) {
  const [theme, setThemeState] = useState<Theme>(() => {
    if (typeof window === 'undefined') return 'system'
    return (localStorage.getItem('option-theme') as Theme) ?? 'system'
  })

  const systemTheme = useSyncExternalStore<ResolvedTheme>(
    subscribeToSystem,
    getSystemTheme,
    () => 'light',
  )

  const resolved = theme === 'system' ? systemTheme : theme

  const mounted = useRef(false)
  // Resolved value a manual toggle is already animating toward — keeps the
  // effect from flipping the DOM before the view transition captures it.
  const vtTarget = useRef<ResolvedTheme | null>(null)

  const transitionTo = useCallback(
    (next: ResolvedTheme, origin?: ThemeToggleOrigin) => {
      const root = document.documentElement
      const reduce = window.matchMedia(
        '(prefers-reduced-motion: reduce)',
      ).matches
      if (reduce) {
        applyResolved(next)
        return
      }
      if (typeof document.startViewTransition === 'function') {
        const { x, y } = toggleOriginPoint(origin)
        root.style.setProperty('--theme-x', `${x}px`)
        root.style.setProperty('--theme-y', `${y}px`)
        root.style.setProperty(
          '--theme-r',
          `${Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y))}px`,
        )
        vtTarget.current = next
        try {
          document
            .startViewTransition(() => applyResolved(next))
            .finished.finally(() => {
              if (vtTarget.current === next) vtTarget.current = null
            })
          return
        } catch {
          vtTarget.current = null
        }
      }
      fadeTo(next)
    },
    [],
  )

  useEffect(() => {
    const root = document.documentElement
    const domIsResolved =
      root.classList.contains('dark') === (resolved === 'dark')
    if (!domIsResolved && vtTarget.current !== resolved) {
      if (mounted.current) transitionTo(resolved)
      else applyResolved(resolved)
    }
    mounted.current = true
    localStorage.setItem('option-theme', theme)
  }, [resolved, theme, transitionTo])

  const setTheme = useCallback(
    (t: Theme) => {
      setThemeState(t)
    },
    [setThemeState],
  )

  const toggle = useCallback(
    (origin?: ThemeToggleOrigin) => {
      const next: ResolvedTheme = resolved === 'dark' ? 'light' : 'dark'
      transitionTo(next, origin)
      setThemeState(next)
    },
    [resolved, transitionTo],
  )

  return (
    <ThemeContext.Provider value={{ theme, resolved, setTheme, toggle }}>
      {children}
    </ThemeContext.Provider>
  )
}

export { ThemeContext, ThemeProvider }
export type { Theme, ThemeToggleOrigin }
