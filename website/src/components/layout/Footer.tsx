import { ArrowRight } from 'lucide-react'
import { NavLink } from 'react-router-dom'

import { Button } from '@shared/components/ui/button'

import { LATEST_CLI } from '../../releases'

type FooterLink = { label: string; href: string } | { label: string; to: string }

const groups: { title: string; links: FooterLink[] }[] = [
  {
    title: 'Product',
    links: [
      { label: 'Home', to: '/' },
      { label: 'Releases', href: '#releases' },
      { label: 'FAQ', href: '#faq' },
    ],
  },
  {
    title: 'Resources',
    links: [
      {
        label: 'GitHub ↗',
        href: 'https://github.com/fireflylabss/optionMusic',
      },
      {
        label: 'Changelog ↗',
        href: 'https://github.com/fireflylabss/optionMusic/blob/main/CHANGELOG.md',
      },
    ],
  },
]

function Footer() {
  return (
    <footer className="border-t border-border bg-background">
      <div className="mx-auto max-w-6xl px-6 py-16">
        <div className="flex flex-col gap-12 lg:flex-row lg:items-start lg:justify-between">
          <div className="max-w-sm">
            <p className="font-sans text-2xl tracking-tight">Play with focus.</p>
            <p className="mt-3 text-sm leading-relaxed text-muted-foreground">
              optionMusic is a minimal music player for the terminal. Download it
              or build from source.
            </p>
            <div className="mt-6 flex flex-wrap gap-3">
              <Button asChild size="sm">
                <a href="#download">
                  Download <ArrowRight className="ml-2 size-4" strokeWidth={1.5} />
                </a>
              </Button>
              <Button variant="outline" size="sm" asChild>
                <a
                  href="https://github.com/fireflylabss/optionMusic"
                  target="_blank"
                  rel="noreferrer"
                >
                  Source
                </a>
              </Button>
            </div>
          </div>

          <div className="grid grid-cols-2 gap-8 lg:gap-16">
            {groups.map((group) => (
              <div key={group.title} className="space-y-3">
                <p className="text-sm font-medium text-foreground">
                  {group.title}
                </p>
                <ul className="space-y-2">
                  {group.links.map((link) =>
                    'href' in link ? (
                      <li key={link.label}>
                        <a
                          href={link.href}
                          {...(link.href.startsWith('#')
                            ? {}
                            : { target: '_blank', rel: 'noreferrer' })}
                          className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                        >
                          {link.label}
                        </a>
                      </li>
                    ) : (
                      <li key={link.label}>
                        <NavLink
                          to={link.to}
                          className="text-sm text-muted-foreground transition-colors hover:text-foreground"
                        >
                          {link.label}
                        </NavLink>
                      </li>
                    ),
                  )}
                </ul>
              </div>
            ))}
          </div>
        </div>

        <div className="mt-16 flex flex-col items-start justify-between gap-4 border-t border-border pt-8 md:flex-row md:items-center">
          <div className="flex items-center gap-3">
            <span className="font-sans text-lg tracking-tight">optionMusic</span>
            <span className="text-xs text-muted-foreground">
              © 2026 Firefly Labs
            </span>
          </div>
          <div className="flex items-center gap-6">
            <a
              href="https://github.com/fireflylabss/optionMusic"
              target="_blank"
              rel="noreferrer"
              className="flex items-center gap-2 text-sm text-muted-foreground transition-colors hover:text-foreground"
            >
              <svg
                viewBox="0 0 24 24"
                className="size-4"
                fill="currentColor"
                aria-hidden="true"
              >
                <path d="M12 .297c-6.63 0-12 5.373-12 12 0 5.303 3.438 9.8 8.205 11.385.6.113.82-.258.82-.577 0-.285-.01-1.04-.015-2.04-3.338.724-4.042-1.61-4.042-1.61C4.422 18.07 3.633 17.7 3.633 17.7c-1.087-.744.084-.729.084-.729 1.205.084 1.838 1.236 1.838 1.236 1.07 1.835 2.809 1.305 3.495.998.108-.776.417-1.305.76-1.605-2.665-.3-5.466-1.332-5.466-5.93 0-1.31.465-2.38 1.235-3.22-.135-.303-.54-1.523.105-3.176 0 0 1.005-.322 3.3 1.23.96-.267 1.98-.399 3-.405 1.02.006 2.04.138 3 .405 2.28-1.552 3.285-1.23 3.285-1.23.645 1.653.24 2.873.12 3.176.765.84 1.23 1.91 1.23 3.22 0 4.61-2.805 5.625-5.475 5.92.42.36.81 1.096.81 2.22 0 1.606-.015 2.898-.015 3.293 0 .319.21.694.825.577C20.565 22.092 24 17.592 24 12.297c0-6.627-5.373-12-12-12" />
              </svg>
              Source
            </a>
            <span className="font-mono text-xs text-muted-foreground">
              {LATEST_CLI.v}
            </span>
          </div>
        </div>
      </div>
    </footer>
  )
}

export { Footer }
