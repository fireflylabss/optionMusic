import { useEffect } from "react";

export function NotFound() {
  useEffect(() => {
    document.title = "404 — Nothing playing here · optionMusic";
  }, []);

  return (
    <div className="flex min-h-[100svh] flex-col">
      <main className="grid grow place-items-center">
        <div className="mx-auto w-full max-w-[63ch]">
          <h1 className="-indent-[0.1rem] text-balance text-[2rem] leading-[1.2]">
            Nothing <em className="accent-serif">playing</em> here.
          </h1>
          <p className="mt-5 max-w-[46ch] text-[16px] leading-[26px] text-[color:var(--ink-2)]">
            Just silence on this groove — the page you&apos;re looking for is
            off the record, moved, or never pressed.
          </p>
          <div className="mt-6 flex flex-wrap items-center gap-4">
            <a
              className="no-underline inline-block rounded-full bg-primary text-background px-4 py-2 text-[0.85rem] font-medium transition-[opacity,scale] duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-85 active:scale-[0.96]"
              href="/"
            >
              Back to the music
            </a>
          </div>
        </div>
      </main>
    </div>
  );
}
