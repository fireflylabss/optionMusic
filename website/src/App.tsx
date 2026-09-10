import { useRef, useState } from "react";
import { SITE } from "./content";
import { DemoPlayer } from "./components/DemoPlayer";
import { NotFound } from "./components/NotFound";
import { ThemeToggle } from "./components/ThemeToggle";

function CopyInstall() {
  const [copied, setCopied] = useState(false);
  const timer = useRef<number | undefined>(undefined);

  const copy = async () => {
    try {
      await navigator.clipboard.writeText(SITE.install);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = SITE.install;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    setCopied(true);
    window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setCopied(false), 1600);
  };

  return (
    <div className="hairline-box relative mt-3 rounded-lg">
      <pre className="overflow-x-auto p-4 pr-20 font-mono text-[0.85rem]">
        <code>{SITE.install}</code>
      </pre>
      <button
        type="button"
        onClick={copy}
        aria-live="polite"
        className="absolute top-3 right-3 rounded-md px-2 py-1 font-mono text-[11px] text-[color:var(--ink-2)] transition-colors duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:text-[color:var(--color-primary)]"
      >
        {copied ? "copied ✓" : "copy"}
      </button>
    </div>
  );
}

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
              over your files.
            </h1>
            <p className="mt-5 text-[16px] leading-[26px] text-[color:var(--ink-2)]">
              No account, no feed, no recommendations. Desktop when you want
              to{" "}
              <span className="accent-serif">see</span>, terminal when you want
              to <span className="accent-serif">stay</span>. Same MPV engine
              underneath.
            </p>
            <div className="mt-6 flex flex-wrap items-center gap-4">
              <a
                className="no-underline inline-block rounded-full bg-primary text-background px-4 py-2 text-[0.85rem] font-medium transition-[opacity,scale] duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-85 active:scale-[0.96]"
                href={SITE.repo}
              >
                Get the app
              </a>
              <a
                className="no-underline inline-block rounded-full px-4 py-2 font-mono text-[13px] text-[color:var(--ink-2)] transition-[opacity,scale] duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:text-[color:var(--color-primary)] active:scale-[0.96]"
                href="#demo"
              >
                See it in action ↓
              </a>
            </div>

            <div id="demo" className="mt-8 scroll-mt-8">
              <DemoPlayer />
            </div>

            <ul className="mt-12 space-y-0 text-[0.95rem]">
              <li className="section-label list-none pb-3">features</li>
              {SITE.features.map(({ title, body }) => (
                <li
                  key={title}
                  className="hairline-top flex flex-col sm:flex-row sm:items-baseline gap-1 sm:gap-3 py-3"
                >
                  <span className="font-medium">{title}</span>
                  <span className="text-pretty text-[color:var(--ink-2)]">
                    {body}
                  </span>
                </li>
              ))}
            </ul>

            <div className="mt-12">
              <p className="section-label">faq</p>
              <div className="faq mt-3">
                {SITE.faq.map(({ q, a }) => (
                  <details key={q} className="hairline-top group py-3">
                    <summary className="flex cursor-pointer items-baseline justify-between gap-3 font-medium">
                      <span>{q}</span>
                      <span
                        aria-hidden="true"
                        className="tnum transition-transform duration-200 ease-[cubic-bezier(0,0,0.2,1)] group-open:rotate-45"
                      >
                        +
                      </span>
                    </summary>
                    <p className="mt-2 max-w-[60ch] text-[0.95rem] leading-[1.55] text-[color:var(--ink-2)]">
                      {a}
                    </p>
                  </details>
                ))}
                <div className="hairline-top" aria-hidden="true" />
              </div>
            </div>

            <div id="download" className="mt-12">
              <p className="section-label font-mono">download</p>
              <CopyInstall />
              <a
                className="no-underline mt-3 inline-block font-mono text-[13px] text-[color:var(--ink-2)] hover:text-[color:var(--color-primary)]"
                href={`${SITE.repo}#readme`}
              >
                More install options on GitHub →
              </a>
            </div>
          </div>
        </div>
      </main>

      <footer className="mx-auto mb-6 mt-12 w-full max-w-[63ch]">
        <div className="hairline-top flex flex-wrap items-center justify-between gap-4 pt-4 font-mono text-[11px] text-[color:var(--ink-2)]">
          <span className="inline-flex items-center gap-4">
            <a
              className="no-underline hover:text-[color:var(--color-primary)]"
              href="/about"
            >
              About
            </a>
            <a
              className="no-underline hover:text-[color:var(--color-primary)]"
              href="/contact"
            >
              Contact
            </a>
            <a
              className="no-underline hover:text-[color:var(--color-primary)]"
              href="/privacy"
            >
              Privacy
            </a>
          </span>
          <span className="inline-flex items-center gap-4">
            <a
              className="no-underline hover:text-[color:var(--color-primary)]"
              href="/llms.txt"
            >
              llms.txt
            </a>
            <ThemeToggle />
            <a
              className="no-underline hover:text-[color:var(--color-primary)]"
              href={SITE.repo}
            >
              GitHub →
            </a>
          </span>
        </div>
      </footer>
    </div>
  );
}
