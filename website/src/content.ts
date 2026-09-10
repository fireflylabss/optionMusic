export const SITE = {
  name: "optionMusic",
  tagline: "Local music, without the noise.",
  description:
    "A minimal local music player for Linux. Calm desktop UI and a real terminal interface, both powered by libmpv/MPV. No account, no feed, no recommendations — your files stay yours and playback works offline.",
  url: "https://music.hory.one",
  repo: "https://github.com/fireflylabss/optionMusic",
  repoName: "fireflylabss/optionMusic",
  install: "yay -S optionmusic",
  org: {
    name: "AEFireflyLabs",
    url: "https://github.com/fireflylabss",
    email: "support@hory.one",
    contactType: "customer support",
    addressCountry: "BR",
  },
  features: [
    {
      title: "Local playlists + M3U",
      body: "your lists are files, yours to keep",
    },
    {
      title: "MPV engine, cava spectrum",
      body: "serious playback, quiet surface",
    },
    {
      title: "Offline lyrics + ReplayGain",
      body: "even volume, no connection needed",
    },
  ],
  faq: [
    {
      q: "Where does optionMusic look for music?",
      a: "Your ~/Music folder by default. Point it anywhere with -m / --music-dir.",
    },
    {
      q: "Do I need an account?",
      a: "No. No account, no feed, no recommendations — your files stay yours and playback works offline.",
    },
    {
      q: "Which formats can it play?",
      a: "Whatever MPV plays — MP3, FLAC, OGG, M4A, Opus and more, powered by libmpv.",
    },
    {
      q: "Desktop or terminal?",
      a: "Both, same MPV engine underneath. Desktop when you want to see, terminal (msc) when you want to stay.",
    },
    {
      q: "How do playlists work?",
      a: "Local playlists plus M3U — your lists are files, yours to keep.",
    },
  ],
};

export type Page = {
  title: string;
  h1: string;
  lede: string;
  sections: Array<{ h2: string; body: string }>;
};

export const PAGES: Record<string, Page> = {
  about: {
    title: "About — optionMusic",
    h1: "About optionMusic",
    lede:
      "optionMusic is a local-first music player for people who still own their music. It wraps the MPV engine in a calm, black-and-white interface that stays out of the way.",
    sections: [
      {
        h2: "Why local-first",
        body:
          "We believe music files should live on your drive, not in someone else's cloud. optionMusic does not require an account, does not upload metadata, and keeps playback offline. Your library is stored exactly where you put it, and your playlists are plain M3U files you can move anywhere.",
      },
      {
        h2: "Open source",
        body:
          "The CLI, desktop UI, and website source are in the same repository under the GNU General Public License. Source code, changelogs, and packaging scripts are available on GitHub. Contributions, bug reports, and packaging help are welcome via GitHub issues and pull requests.",
      },
      {
        h2: "How it is built",
        body:
          "The core is written in Rust and uses libmpv for audio decoding and playback. The desktop surface is built with Tauri and React, while the terminal interface is a crossterm-based TUI. Both share the same playback engine and configuration format.",
      },
    ],
  },
  contact: {
    title: "Contact — optionMusic",
    h1: "Contact",
    lede:
      "Questions, bug reports, packaging requests, and feature ideas are best handled through the public GitHub repository. For anything that should stay private, use the email below.",
    sections: [
      {
        h2: "GitHub",
        body:
          "Open an issue, start a discussion, or open a pull request at github.com/fireflylabss/optionMusic. The issue tracker is the fastest way to report bugs or request features.",
      },
      {
        h2: "Email",
        body:
          "For private inquiries, reach the maintainers at support@hory.one. Response time varies, but we read every message.",
      },
      {
        h2: "Arch User Repository",
        body:
          "The AUR package is maintained in the repository under packaging/aur. Comments and votes on the AUR page also help improve the package.",
      },
    ],
  },
  privacy: {
    title: "Privacy — optionMusic",
    h1: "Privacy",
    lede:
      "optionMusic is built to keep your listening private. There are no accounts, no telemetry, and no cloud analysis of your library.",
    sections: [
      {
        h2: "Local data only",
        body:
          "Your music files, playlists, cover art, and configuration stay on your computer. Nothing is uploaded to optionMusic servers because there are no optionMusic servers. Playback works entirely offline.",
      },
      {
        h2: "No analytics in the player",
        body:
          "The desktop and CLI applications do not track usage, crashes, or playback behavior. The website is served by Vercel's edge network, which records standard HTTP request logs for operational purposes.",
      },
      {
        h2: "Open source",
        body:
          "You can inspect exactly what the application does by reading the source code at github.com/fireflylabss/optionMusic. If something in this policy is unclear, the source code is the ground truth.",
      },
    ],
  },
  docs: {
    title: "Developer resources — optionMusic",
    h1: "Developer resources",
    lede:
      "This page points agents and developers to the source, install instructions, changelog, and packaging information for optionMusic.",
    sections: [
      {
        h2: "Source code",
        body:
          "The complete source is at github.com/fireflylabss/optionMusic. It includes the Rust core, the Tauri desktop shell, the terminal TUI, and the website.",
      },
      {
        h2: "Install",
        body:
          "On Arch Linux the recommended install is `yay -S optionmusic`. For other platforms, clone the repository and run `cargo build --release` from the project root. See the README for dependencies such as libmpv-dev.",
      },
      {
        h2: "Releases and versioning",
        body:
          "Releases follow the Option-family versioning convention. See VERSIONING.md and CHANGELOG.md in the repository. Desktop and CLI version lines are independent.",
      },
      {
        h2: "AUR packaging",
        body:
          "The packaging/aur directory contains the PKGBUILD, .SRCINFO template, and the GitHub Action that publishes to the Arch User Repository.",
      },
      {
        h2: "When to use optionMusic",
        body:
          "Use optionMusic when you need an offline, local-first music player for Linux with a minimal CLI and an optional desktop UI. It is not a streaming client and does not support mobile playback.",
      },
    ],
  },
};

export const NOT_FOUND_MD = `# Not found

This path is off the record. If you are looking for optionMusic documentation or source, try:

- [Home](/)
- [About](/about)
- [Contact](/contact)
- [Privacy](/privacy)
- [Developer resources (llms.txt)](/llms.txt)
- [Sitemap](/sitemap.xml)
- [GitHub repository](https://github.com/fireflylabss/optionMusic)
`;
