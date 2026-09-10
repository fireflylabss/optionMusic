import { describe, expect, it } from "bun:test";
import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";

const DIST = resolve(process.cwd(), "dist");

function dist(path: string) {
  return resolve(DIST, path);
}

function textLength(html: string): number {
  // Strip script and style contents, then tags, then count non-whitespace chars.
  const noScripts = html.replace(/<script\b[^<]*(?:(?!<\/script>)<[^<]*)*<\/script>/gi, " ");
  const noStyles = noScripts.replace(/<style\b[^<]*(?:(?!<\/style>)<[^<]*)*<\/style>/gi, " ");
  const noTags = noStyles.replace(/<[^>]+>/g, " ");
  const text = noTags.replace(/\s+/g, "");
  return text.length;
}

function textFromSelector(html: string, selector: string): string {
  const match = html.match(new RegExp(`<${selector}[\\s\\S]*?</${selector}>`));
  if (!match) return "";
  return match[0].replace(/<[^>]+>/g, " ").replace(/\s+/g, " ").trim();
}

describe("agentic readiness output", () => {
  it("builds the expected dist files", () => {
    const required = [
      "index.html",
      "index.md",
      "404.html",
      "404.md",
      "about.html",
      "about.md",
      "contact.html",
      "contact.md",
      "privacy.html",
      "privacy.md",
      "docs.html",
      "docs.md",
      "llms.txt",
      "sitemap.xml",
      "robots.txt",
      "style.css",
    ];
    for (const file of required) {
      expect(existsSync(dist(file))).toBe(true);
    }
  });

  it("homepage has a clear H1, JSON-LD, and no-JS content", () => {
    const html = readFileSync(dist("index.html"), "utf-8");
    expect(html).toContain("<h1");
    expect(html).toContain("optionMusic");
    expect(html).toContain("application/ld+json");
    expect(html).toContain('"@type":"SoftwareApplication"');
    expect(html).toContain('"@type":"Organization"');
    expect(html).toContain("<noscript");
    expect(textLength(html)).toBeGreaterThan(800);
  });

  it("homepage no-JS fallback is at least 500 characters", () => {
    const html = readFileSync(dist("index.html"), "utf-8");
    const noscript = html.match(/<noscript[^>]*>([\s\S]*?)<\/noscript>/)?.[1] ?? "";
    expect(textLength(noscript)).toBeGreaterThan(500);
  });

  it("llms.txt has a when-to-use section", () => {
    const txt = readFileSync(dist("llms.txt"), "utf-8");
    expect(txt.toLowerCase()).toContain("when to use");
    expect(txt).toContain("github.com/fireflylabss/optionMusic");
  });

  it("sitemap lists public pages", () => {
    const xml = readFileSync(dist("sitemap.xml"), "utf-8");
    for (const path of ["", "about", "contact", "privacy", "docs", "llms.txt"]) {
      expect(xml).toContain(`<loc>https://music.hory.one/${path}</loc>`);
    }
  });

  it("trust anchor pages have at least 500 characters", () => {
    for (const name of ["about", "contact", "privacy"]) {
      const html = readFileSync(dist(`${name}.html`), "utf-8");
      expect(textLength(html)).toBeGreaterThan(500);
      const h1 = textFromSelector(html, "h1");
      expect(h1.length).toBeGreaterThan(0);
    }
  });

  it("404 page links to sitemap and llms.txt", () => {
    const html = readFileSync(dist("404.html"), "utf-8");
    expect(html).toContain("sitemap.xml");
    expect(html).toContain("llms.txt");
  });

  it("homepage markdown has at least 500 characters", () => {
    const md = readFileSync(dist("index.md"), "utf-8");
    expect(md.replace(/\s/g, "").length).toBeGreaterThan(500);
  });
});
