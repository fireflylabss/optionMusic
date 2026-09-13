import { Moon, Sun } from 'lucide-react'
import { NavLink } from 'react-router-dom'

import { Button } from '@shared/components/ui/button'
import { useTheme } from '@shared/hooks/useTheme'

const links = [
  { href: '#releases', label: 'Releases' },
  { href: '#faq', label: 'FAQ' },
  { href: '#download', label: 'Download' },
]

function Nav() {
  const { resolved, toggle } = useTheme()
  const isDark = resolved === 'dark'

  return (
    <header className="fixed inset-x-0 top-0 z-50 px-4">
      <nav className="mx-auto mt-4 flex h-14 w-full max-w-3xl items-center justify-between rounded-lg border border-border bg-background/80 px-4 backdrop-blur-md">
        <div className="flex items-center gap-8">
          <NavLink
            to="/"
            className="font-sans text-lg tracking-tight text-foreground"
          >
            optionMusic
          </NavLink>
          <div className="hidden items-center gap-6 sm:flex">
            {links.map((link) => (
              <a
                key={link.href}
                href={link.href}
                className="text-sm text-muted-foreground transition-colors duration-(--duration-fast) ease-(--ease-option) hover:text-foreground"
              >
                {link.label}
              </a>
            ))}
          </div>
        </div>

        <div className="flex items-center gap-1.5">
          <Button
            variant="ghost"
            size="icon"
            onClick={toggle}
            className="relative size-8 rounded-full"
            aria-label="Toggle dark mode"
          >
            <Sun
              strokeWidth={1.5}
              className={`absolute size-4 transition-[transform,opacity,filter] duration-(--duration-base) ease-[cubic-bezier(0.2,0,0,1)] ${isDark ? 'scale-100 opacity-100 blur-0' : 'scale-25 opacity-0 blur-[4px]'}`}
            />
            <Moon
              strokeWidth={1.5}
              className={`absolute size-4 transition-[transform,opacity,filter] duration-(--duration-base) ease-[cubic-bezier(0.2,0,0,1)] ${isDark ? 'scale-25 opacity-0 blur-[4px]' : 'scale-100 opacity-100 blur-0'}`}
            />
          </Button>
          <a
            href="https://github.com/fireflylabss/optionMusic"
            target="_blank"
            rel="noreferrer"
            className="hidden px-3 py-1.5 text-sm text-muted-foreground transition-colors duration-(--duration-fast) hover:text-foreground sm:block"
          >
            GitHub
          </a>
        </div>
      </nav>
    </header>
  )
}

export { Nav }
