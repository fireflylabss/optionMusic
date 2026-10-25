import raw from '../../CHANGELOG.md?raw'

export type Channel = 'stable' | 'beta' | 'alpha' | 'mixed'
export type Surface = 'cli' | 'desktop' | 'both'

export interface Release {
  v: string
  date: string
  channel: Channel
  surface: Surface
  note: string
}

// Includes revision tags (v0.3.0-beta.1) and legacy mixed or plain versions.
const HEADING =
  /^## (v\d+\.\d+\.\d+m?(?:-(?:(?:alpha|beta)(?:\.[1-9]\d*)?|stable))?) · (\d{2}\/\d{2}\/\d{4})$/gm

export function parseReleases(md: string): Release[] {
  const headings = [...md.matchAll(HEADING)]

  return headings.map((match, i) => {
    const [, v, date] = match
    const start = match.index + match[0].length
    const body = md.slice(start, headings[i + 1]?.index ?? md.length)

    // First paragraph is the human summary; it ends with the boilerplate
    // "This version was made for <surface> ..." sentence.
    const summary =
      body
        .split(/\n\s*\n/)
        .map((p) => p.trim())
        .find(Boolean) ?? ''
    const note = summary.split('This version was made for')[0].trim()

    const surface: Surface = /made for both/.test(summary)
      ? 'both'
      : /made for desktop/.test(summary)
        ? 'desktop'
        : 'cli'

    const channel: Channel =
      /m(?:-|$)/.test(v)
        ? 'mixed'
        : ((v.match(/-(alpha|beta|stable)(?:\.[1-9]\d*)?$/)?.[1] as Channel | undefined) ??
          'stable')

    return { v, date, channel, surface, note }
  })
}

export const RELEASES = parseReleases(raw)

/** Latest CLI-only cut — what the download button actually ships. */
export const LATEST_CLI =
  RELEASES.find((r) => r.surface === 'cli') ?? RELEASES[0]
