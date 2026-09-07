import { DemoPlayer } from "./components/DemoPlayer";
import { NotFound } from "./components/NotFound";
import { ThemeToggle } from "./components/ThemeToggle";

export function App() {
  const path = window.location.pathname;
  if (path !== "/" && path !== "/index.html") {
    return <NotFound />;
  }
  return (
    <div className="flex min-h-[100svh] flex-col">
      <main id="top" className="grow">
        <div className="mt-16 md:mt-24">
          <div className="mx-auto max-w-[63ch]">
            <p className="font-mono text-[13px] opacity-60">
              a local player for CLI and desktop
            </p>
            <h1 className="mt-6 -indent-[0.1rem] text-balance text-[30px] font-medium leading-[36px]">
              <em className="accent-serif">optionMusic</em> is a local player
              for people who still{" "}
              <span className="accent-serif">own music</span>. A calm surface
              over your files — no account, no feed, no recommendations.
            </h1>
            <p className="mt-5 text-[16px] leading-[26px] text-[color:var(--ink-2)]">
              Desktop when you want to{" "}
              <span className="accent-serif">see</span>, terminal when you want
              to <span className="accent-serif">stay</span>. Same MPV engine
              underneath.
            </p>
            <div className="mt-6 flex flex-wrap items-center gap-4">
              <a
                className="no-underline inline-block rounded-full bg-primary text-background px-4 py-2 text-[0.85rem] font-medium transition-[opacity,scale] duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-85 active:scale-[0.96]"
                href="https://github.com/fireflylabss/optionMusic"
              >
                Get the app →
              </a>
            </div>

            <div className="mt-12">
              <DemoPlayer />
            </div>

            <ul className="mt-12 space-y-0 text-[0.95rem]">
              <li className="section-label list-none pb-3">
                features
              </li>
              {[
                ["Local playlists + M3U", "your lists are files, yours to keep"],
                ["MPV engine, cava spectrum", "serious playback, quiet surface"],
                ["Offline lyrics + ReplayGain", "even volume, no connection needed"],
              ].map(([t, d]) => (
                <li
                  key={t}
                  className="hairline-top flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-3"
                >
                  <span className="font-medium">{t}</span>
                  <span className="text-pretty text-[color:var(--ink-2)]">{d}</span>
                </li>
              ))}
            </ul>

            <div id="download" className="mt-12">
              <p className="section-label font-mono">download</p>
              <pre
                className="hairline-box mt-3 overflow-x-auto rounded-lg p-4 font-mono text-[0.85rem]"
              >
                <code>yay -S optionmusic</code>
              </pre>
              <a
                className="no-underline mt-3 inline-block font-mono text-[13px] text-[color:var(--ink-2)] hover:text-[color:var(--color-primary)]"
                href="https://github.com/fireflylabss/optionMusic#readme"
              >
                More install options on GitHub →
              </a>
            </div>
          </div>
        </div>
      </main>

      <footer className="mx-auto mb-6 mt-12 w-full max-w-[63ch]">
        <div
          className="hairline-top flex items-center justify-between pt-4 font-mono text-[11px] text-[color:var(--ink-2)]"
        >
          <span>optionMusic</span>
          <span className="inline-flex items-center gap-4">
            <ThemeToggle />
            <a
              className="no-underline hover:text-[color:var(--color-primary)]"
              href="https://github.com/fireflylabss/optionMusic"
            >
              GitHub →
            </a>
          </span>
        </div>
      </footer>
    </div>
  );
}
