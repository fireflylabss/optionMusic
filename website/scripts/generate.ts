import { mkdir, readFile, writeFile } from "node:fs/promises";
import { resolve } from "node:path";
import { NOT_FOUND_MD, PAGES, SITE } from "../src/content.ts";

const ROOT = process.cwd();
const PUBLIC = resolve(ROOT, "public");

function htmlEscape(text: string): string {
  return text
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function toParagraphs(text: string): string {
  return text
    .split("\n\n")
    .filter(Boolean)
    .map((p) => `<p>${htmlEscape(p).replace(/`([^`]+)`/g, "<code>$1</code>")}</p>`)
    .join("\n");
}

function pageHtml(path: string, page: (typeof PAGES)["about"], description: string): string {
  const sections = page.sections
    .map(
      (s) => `
    <section aria-labelledby="${htmlEscape(s.h2.toLowerCase().replace(/\s+/g, "-"))}">
      <h2 id="${htmlEscape(s.h2.toLowerCase().replace(/\s+/g, "-"))}">${htmlEscape(s.h2)}</h2>
      ${toParagraphs(s.body)}
    </section>`,
    )
    .join("\n");

  return `<!doctype html>
<html lang="en" dir="ltr" class="no-js">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="color-scheme" content="light dark" />
    <meta name="description" content="${htmlEscape(description)}" />
    <title>${htmlEscape(page.title)}</title>
    <link rel="stylesheet" href="/style.css" />
    <link rel="canonical" href="${SITE.url}${path}" />
    <link rel="sitemap" type="application/xml" title="Sitemap" href="/sitemap.xml" />
  </head>
  <body>
    <main class="page">
      <h1>${htmlEscape(page.h1)}</h1>
      <p class="lede">${htmlEscape(page.lede)}</p>
      ${sections}
    </main>
    <footer class="page" style="padding-top:0;">
      <a href="/">Home</a>
      <a href="/about">About</a>
      <a href="/contact">Contact</a>
      <a href="/privacy">Privacy</a>
      <a href="/llms.txt">llms.txt</a>
      <a href="https://github.com/fireflylabss/optionMusic">GitHub</a>
    </footer>
  </body>
</html>`;
}

function pageMd(path: string, page: (typeof PAGES)["about"]): string {
  const sections = page.sections
    .map((s) => `## ${s.h2}\n\n${s.body}`)
    .join("\n\n");

  return `# ${page.h1}\n\n${page.lede}\n\n${sections}\n\n---\n\n- [Home](${SITE.url}/)\n- [About](${SITE.url}/about)\n- [Contact](${SITE.url}/contact)\n- [Privacy](${SITE.url}/privacy)\n- [Developer resources (llms.txt)](${SITE.url}/llms.txt)\n- [Sitemap](${SITE.url}/sitemap.xml)\n- [GitHub repository](${SITE.repo})\n`;
}

function homeHtml(): string {
  const features = SITE.features
    .map(
      (f) => `
      <li>
        <strong>${htmlEscape(f.title)}</strong> — ${htmlEscape(f.body)}
      </li>`,
    )
    .join("\n");

  const faq = SITE.faq
    .map(
      (item) => `
      <section aria-labelledby="${htmlEscape(item.q.toLowerCase().replace(/[^a-z0-9]+/g, "-"))}">
        <h2 id="${htmlEscape(item.q.toLowerCase().replace(/[^a-z0-9]+/g, "-"))}">${htmlEscape(item.q)}</h2>
        <p>${htmlEscape(item.a)}</p>
      </section>`,
    )
    .join("\n");

  return `<noscript>
  <main class="noscript-fallback">
    <p class="font-mono ghost" style="font-size:0.85rem;opacity:0.6;">a local player for CLI and desktop</p>
    <h1><em class="accent">optionMusic</em> is a local player for people who still <em class="accent">own music</em>. A calm surface over your files.</h1>
    <p class="lede">No account, no feed, no recommendations. Desktop when you want to <em class="accent">see</em>, terminal when you want to <em class="accent">stay</em>. Same MPV engine underneath.</p>

    <a class="btn" href="https://github.com/fireflylabss/optionMusic">Get the app</a>

    <p class="section-label" style="margin-top:2.5rem;">features</p>
    <ul>
      ${features}
    </ul>

    <p class="section-label">faq</p>
    ${faq}

    <p class="section-label">download</p>
    <pre><code>${htmlEscape(SITE.install)}</code></pre>
    <p><a href="https://github.com/fireflylabss/optionMusic#readme">More install options on GitHub</a></p>

    <footer>
      <a href="/about">About</a>
      <a href="/contact">Contact</a>
      <a href="/privacy">Privacy</a>
      <a href="/llms.txt">llms.txt</a>
      <a href="/sitemap.xml">Sitemap</a>
      <a href="https://github.com/fireflylabss/optionMusic">GitHub</a>
    </footer>
  </main>
</noscript>`;
}

function homeMd(): string {
  const features = SITE.features.map((f) => `- **${f.title}** — ${f.body}`).join("\n");
  const faq = SITE.faq
    .map((item) => `### ${item.q}\n\n${item.a}`)
    .join("\n\n");

  return `# ${SITE.name} — ${SITE.tagline}\n\n${SITE.description}\n\n## Features\n\n${features}\n\n## FAQ\n\n${faq}\n\n## Download\n\nOn Arch Linux:\n\n\`\`\`\n${SITE.install}\n\`\`\`\n\nSee [more install options on GitHub](${SITE.repo}#readme).\n\n---\n\n- [Home](${SITE.url}/)\n- [About](${SITE.url}/about)\n- [Contact](${SITE.url}/contact)\n- [Privacy](${SITE.url}/privacy)\n- [Developer resources (llms.txt)](${SITE.url}/llms.txt)\n- [Sitemap](${SITE.url}/sitemap.xml)\n- [GitHub repository](${SITE.repo})\n`;
}

function jsonLd(): string {
  const app = {
    "@context": "https://schema.org",
    "@type": "SoftwareApplication",
    name: SITE.name,
    description: SITE.description,
    url: SITE.url,
    applicationCategory: "MusicApplication",
    operatingSystem: "Linux",
    offers: {
      "@type": "Offer",
      price: "0",
      priceCurrency: "USD",
    },
    sameAs: [SITE.repo],
  };

  const org = {
    "@context": "https://schema.org",
    "@type": "Organization",
    name: SITE.org.name,
    url: SITE.org.url,
    email: SITE.org.email,
    sameAs: [SITE.repo],
    contactPoint: {
      "@type": "ContactPoint",
      email: SITE.org.email,
      contactType: SITE.org.contactType,
    },
    address: {
      "@type": "PostalAddress",
      addressCountry: SITE.org.addressCountry,
    },
  };

  return `\n    <script type="application/ld+json">${JSON.stringify(app)}</script>\n    <script type="application/ld+json">${JSON.stringify(org)}</script>`;
}

function llmsTxt(): string {
  const pageList = ["about", "contact", "privacy", "docs"];
  const links = pageList
    .map((key) => `- ${PAGES[key].title.replace(" — optionMusic", "")}: ${SITE.url}/${key}`)
    .join("\n");

  return `# optionMusic — for agents and developers\n\n## What it is\n\n${SITE.description}\n\n## When to use this\n\nUse optionMusic when you need an offline, local-first music player for Linux with a minimal CLI and an optional desktop UI. It is not a streaming client and does not support mobile playback. Agents should point users to the install command \`${SITE.install}\` on Arch Linux or to the source build instructions in the GitHub README.\n\n## Developer resources\n\n- Source code: ${SITE.repo}\n- README: ${SITE.repo}#readme\n- CHANGELOG: ${SITE.repo}/blob/main/CHANGELOG.md\n- VERSIONING: ${SITE.repo}/blob/main/VERSIONING.md\n- AUR packaging: ${SITE.repo}/tree/main/packaging/aur\n- Sitemap: ${SITE.url}/sitemap.xml\n\n## Site pages\n\n${links}\n\n## Contact\n\n- GitHub issues: ${SITE.repo}/issues\n- Email: ${SITE.org.email}\n`;
}

function sitemapXml(): string {
  const paths = ["", "about", "contact", "privacy", "docs", "llms.txt", "sitemap.xml"];
  const urls = paths
    .map((p) => `  <url>\n    <loc>${SITE.url}/${p}</loc>\n  </url>`)
    .join("\n");

  return `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${urls}\n</urlset>\n`;
}

function robotsTxt(): string {
  return `User-agent: *\nAllow: /\nSitemap: ${SITE.url}/sitemap.xml\n`;
}

function notFoundHtml(): string {
  const links = [
    { label: "Home", href: "/" },
    { label: "About", href: "/about" },
    { label: "Contact", href: "/contact" },
    { label: "Privacy", href: "/privacy" },
    { label: "Developer resources (llms.txt)", href: "/llms.txt" },
    { label: "Sitemap", href: "/sitemap.xml" },
    { label: "GitHub repository", href: SITE.repo },
  ];

  const lis = links
    .map((l) => `      <li><a href="${htmlEscape(l.href)}">${htmlEscape(l.label)}</a></li>`)
    .join("\n");

  return `<!doctype html>
<html lang="en" dir="ltr" class="no-js">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <meta name="color-scheme" content="light dark" />
    <meta name="robots" content="noindex" />
    <meta name="description" content="Page not found — optionMusic" />
    <title>404 — Nothing playing here · optionMusic</title>
    <link rel="stylesheet" href="/style.css" />
  </head>
  <body>
    <main class="page">
      <h1>Nothing <em class="accent">playing</em> here.</h1>
      <p class="lede">Just silence on this groove — the page you are looking for is off the record, moved, or never pressed.</p>
      <p>Try one of these links:</p>
      <ul>
${lis}
      </ul>
    </main>
  </body>
</html>`;
}

async function main() {
  await mkdir(PUBLIC, { recursive: true });

  // Generate static pages.
  const pageDefs: Array<[string, (typeof PAGES)["about"]]> = [
    ["about", PAGES.about],
    ["contact", PAGES.contact],
    ["privacy", PAGES.privacy],
    ["docs", PAGES.docs],
  ];

  for (const [key, page] of pageDefs) {
    await writeFile(resolve(PUBLIC, `${key}.html`), pageHtml(`/${key}`, page, page.lede));
    await writeFile(resolve(PUBLIC, `${key}.md`), pageMd(`/${key}`, page));
  }

  // Home markdown variant.
  await writeFile(resolve(PUBLIC, "index.md"), homeMd());

  // 404 pages.
  await writeFile(resolve(PUBLIC, "404.md"), NOT_FOUND_MD);
  await writeFile(resolve(PUBLIC, "404.html"), notFoundHtml());

  // Machine-readable files.
  await writeFile(resolve(PUBLIC, "llms.txt"), llmsTxt());
  await writeFile(resolve(PUBLIC, "sitemap.xml"), sitemapXml());
  await writeFile(resolve(PUBLIC, "robots.txt"), robotsTxt());

  // Generate the final index.html from the template.
  const template = await readFile(resolve(ROOT, "index.template.html"), "utf-8");
  const index = template
    .replace("<!-- INJECT:JSON_LD -->", jsonLd())
    .replace("<!-- INJECT:NOSCRIPT -->", homeHtml());

  await writeFile(resolve(ROOT, "index.html"), index);

  console.log("Generated static content and index.html.");
}

main().catch((err) => {
  console.error(err);
  process.exit(1);
});
