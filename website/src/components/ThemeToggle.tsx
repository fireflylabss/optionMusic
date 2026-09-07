import { useEffect, useState } from "react";
import { Monitor, Moon, Sun } from "@phosphor-icons/react";

type Mode = "auto" | "light" | "dark";

const KEY = "optionmusic-theme";
const ORDER: Mode[] = ["auto", "light", "dark"];

function initialMode(): Mode {
  try {
    const v = localStorage.getItem(KEY);
    if (v === "light" || v === "dark" || v === "auto") return v;
  } catch {
    /* private mode — fall through to auto */
  }
  return "auto";
}

export function ThemeToggle() {
  const [mode, setMode] = useState<Mode>(initialMode);

  useEffect(() => {
    // Congela transições p/ a troca não borrar: injeta, troca, reflow, solta.
    const kill = document.createElement("style");
    kill.appendChild(
      document.createTextNode("*,*::before,*::after{transition:none !important}"),
    );
    document.head.appendChild(kill);
    const root = document.documentElement;
    if (mode === "auto") root.removeAttribute("data-theme");
    else root.dataset.theme = mode;
    void document.body.offsetHeight;
    requestAnimationFrame(() => requestAnimationFrame(() => kill.remove()));
    try {
      localStorage.setItem(KEY, mode);
    } catch {
      /* private mode — theme just won't persist */
    }
  }, [mode]);

  const next = () => setMode((m) => ORDER[(ORDER.indexOf(m) + 1) % ORDER.length]);

  return (
    <button
      type="button"
      onClick={next}
      title={`Theme: ${mode} — activate to change`}
      aria-label={`Color theme: ${mode}. Activate to change.`}
      className="grid h-6 w-6 place-items-center transition-transform duration-200 ease-[cubic-bezier(0,0,0.2,1)] hover:opacity-100 active:scale-[0.96]"
    >
      {mode === "light" ? <Sun size={14} /> : mode === "dark" ? <Moon size={14} /> : <Monitor size={14} />}
    </button>
  );
}
