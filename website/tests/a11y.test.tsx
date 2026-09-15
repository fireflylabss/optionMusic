import { describe, expect, it } from "bun:test";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { Window } from "happy-dom";
import { renderToStaticMarkup } from "react-dom/server";
import { StaticRouter } from "react-router";

import { ThemeProvider } from "../shared/components/theme/ThemeProvider";
import { Shell } from "../src/components/layout/Shell";
import { HomePage } from "../src/pages/HomePage";
import type { AxeResults, RunOptions, Spec } from "axe-core";

// Static markup carries no layout, so paint-dependent rules cannot be judged
// here; everything structural (names, roles, landmarks, headings) can.
const OPTIONS: RunOptions = {
  runOnly: { type: "tag", values: ["wcag2a", "wcag2aa", "wcag21a", "wcag21aa", "best-practice"] },
  rules: {
    "color-contrast": { enabled: false },
    "meta-viewport": { enabled: false },
  },
};

type AxeWindow = Window & {
  axe: {
    configure(spec: Spec): void;
    run(context: unknown, options: RunOptions): Promise<AxeResults>;
  };
};

const AXE_SOURCE = readFileSync(
  resolve(process.cwd(), "node_modules/axe-core/axe.min.js"),
  "utf-8",
);

async function audit(html: string): Promise<AxeResults> {
  const window = new Window({ url: "https://music.hory.one/", settings: { disableCSSFileLoading: true, disableJavaScriptFileLoading: true } });
  window.document.write(html);
  window.eval(AXE_SOURCE);
  const results = await (window as unknown as AxeWindow).axe.run(
    window.document,
    OPTIONS,
  );
  await window.happyDOM.close();
  return results;
}

function describeViolations(results: AxeResults): string {
  return results.violations
    .map((v) => {
      const targets = v.nodes.map((n) => n.target.join(" ")).join(", ");
      return `${v.id} (${v.impact}): ${v.help} — ${targets}`;
    })
    .join("\n");
}

describe("accessibility", () => {
  it("the React homepage has no axe violations", async () => {
    const body = renderToStaticMarkup(
      <StaticRouter location="/">
        <ThemeProvider>
          <Shell>
            <HomePage />
          </Shell>
        </ThemeProvider>
      </StaticRouter>,
    );
    const results = await audit(
      `<!doctype html><html lang="en"><head><title>optionMusic</title></head><body>${body}</body></html>`,
    );
    expect(describeViolations(results)).toBe("");
  }, 30_000);
});
