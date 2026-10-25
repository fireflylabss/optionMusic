import { describe, expect, it } from 'bun:test'

import { LATEST_CLI, parseReleases } from '../src/releases'

describe('release headings', () => {
  it('keeps beta revisions in the beta channel and preserves legacy entries', () => {
    const releases = parseReleases(`## v0.3.0-beta.1 · 25/10/2026

Corrected release attempt. This version was made for CLI with a beta release channel on 25/10/2026 (v0.3.0-beta.1).

- Dependency fixes.

## v0.3.0-beta · 24/10/2026

Initial attempt. This version was made for CLI with a beta release channel on 24/10/2026 (v0.3.0-beta).

## v0.2.12m-beta · 03/08/2026

Two surfaces. This version was made for both with a beta release channel on 03/08/2026 (v0.2.12m-beta).

## v0.1.1 · 20/07/2026

First release. This version was made for CLI on 20/07/2026 (v0.1.1).
`)
    expect(releases.map(({ v, channel }) => [v, channel])).toEqual([
      ['v0.3.0-beta.1', 'beta'],
      ['v0.3.0-beta', 'beta'],
      ['v0.2.12m-beta', 'mixed'],
      ['v0.1.1', 'stable'],
    ])
    expect(releases[0].note).toBe('Corrected release attempt.')
    expect(releases[2].surface).toBe('both')
  })

  it('recognizes alpha revisions while retaining stable tags', () => {
    const releases = parseReleases(`## v0.3.0-alpha.2 · 25/10/2026

Early work. This version was made for CLI with an alpha release channel.

## v0.2.0-stable · 24/10/2026

Daily use. This version was made for CLI with a stable release channel.
`)
    expect(releases.map(({ channel }) => channel)).toEqual(['alpha', 'stable'])
  })

  it('reads the actual current changelog as a beta revision', () => {
    expect(LATEST_CLI.v).toBe('v0.3.0-beta.1')
    expect(LATEST_CLI.channel).toBe('beta')
    expect(LATEST_CLI.date).toBe('25/10/2026')
    expect(LATEST_CLI.note).toContain('still music rebranding')
  })
})
