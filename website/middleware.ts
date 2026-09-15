import { next, rewrite } from "@vercel/functions";

// NOTE: Vercel transpiles this file standalone — no bundling, no relative
// imports. Keep the content-negotiation logic self-contained here; tests
// and scripts/verify.ts import it back from this module.

export interface AcceptRange {
  type: string;
  subtype: string;
  q: number;
}

export function parseAccept(accept: string | null): AcceptRange[] {
  if (!accept) return [];

  return accept
    .split(",")
    .map((part) => part.trim())
    .filter(Boolean)
    .map((part) => {
      const [media, ...params] = part.split(";").map((s) => s.trim());
      const [rawType, rawSubtype] = media.split("/");
      const type = rawType?.trim().toLowerCase() ?? "";
      const subtype = rawSubtype?.trim().toLowerCase() ?? "";

      const qParam = params.find((p) => p.toLowerCase().startsWith("q="));
      const rawQ = qParam ? Number.parseFloat(qParam.slice(2)) : 1;
      const q = Number.isNaN(rawQ) || rawQ < 0 ? 1 : Math.min(rawQ, 1);

      return { type, subtype, q };
    })
    .filter((r) => r.q > 0 && r.type && r.subtype);
}

export type PreferredMedia = "html" | "markdown" | "not-acceptable";

const MD_MEDIA = { type: "text", subtype: "markdown" } as const;
const HTML_MEDIA = { type: "text", subtype: "html" } as const;

function qualityFor(
  ranges: AcceptRange[],
  media: { type: string; subtype: string },
): number {
  let best = -1;

  for (const r of ranges) {
    if (r.type === media.type && r.subtype === media.subtype) {
      best = Math.max(best, r.q);
      continue;
    }

    // text/* matches any text subtype, but is less specific than an exact match.
    if (r.type === media.type && r.subtype === "*") {
      best = Math.max(best, r.q * 0.9);
      continue;
    }

    // */* matches anything, but is the least specific.
    if (r.type === "*" && r.subtype === "*") {
      best = Math.max(best, r.q * 0.8);
    }
  }

  return best;
}

export function preferredMedia(accept: string | null): PreferredMedia {
  const ranges = parseAccept(accept);
  if (ranges.length === 0) return "html";

  const mdQ = qualityFor(ranges, MD_MEDIA);
  const htmlQ = qualityFor(ranges, HTML_MEDIA);

  if (mdQ < 0 && htmlQ < 0) return "not-acceptable";

  if (mdQ > htmlQ) return "markdown";
  if (htmlQ > mdQ) return "html";

  // Tie: explicit preference wins; if both are explicit, prefer markdown
  // because an agent that lists text/markdown is asking for it.
  const explicitMd = ranges.some(
    (r) => r.type === "text" && r.subtype === "markdown" && r.q > 0,
  );
  const explicitHtml = ranges.some(
    (r) => r.type === "text" && r.subtype === "html" && r.q > 0,
  );

  if (explicitMd && !explicitHtml) return "markdown";
  if (explicitHtml && !explicitMd) return "html";
  if (explicitMd && explicitHtml) return "markdown";

  // Wildcard tie — default to HTML for human browsers.
  return "html";
}

const SITE_URL = "https://music.hory.one";

const KNOWN_PAGES = new Set(["/", "/about", "/contact", "/privacy", "/docs"]);

const MARKDOWN_FILE = new Map<string, string>([
  ["/", "/index.md"],
  ["/about", "/about.md"],
  ["/contact", "/contact.md"],
  ["/privacy", "/privacy.md"],
  ["/docs", "/docs.md"],
]);

const HTML_FILE = new Map<string, string>([
  ["/", "/index.html"],
  ["/about", "/about.html"],
  ["/contact", "/contact.html"],
  ["/privacy", "/privacy.html"],
  ["/docs", "/docs.html"],
]);

const NOT_FOUND_MD = `# Not found

This path is off the record. If you are looking for optionMusic documentation or source, try:

- [Home](${SITE_URL}/)
- [About](${SITE_URL}/about)
- [Contact](${SITE_URL}/contact)
- [Privacy](${SITE_URL}/privacy)
- [Developer resources (llms.txt)](${SITE_URL}/llms.txt)
- [Sitemap](${SITE_URL}/sitemap.xml)
- [GitHub repository](https://github.com/fireflylabss/optionMusic)
`;

const ASSET_PATTERN = /\.(?:png|jpg|jpeg|gif|svg|webp|ico|mp4|webm|mov|css|js|json|xml|txt|md|woff2?)$/i;

export const config = {
  matcher: ["/((?!.*\\..*).*)"],
};

export default function middleware(request: Request) {
  const url = new URL(request.url);
  const { pathname } = url;
  const accept = request.headers.get("accept") || "";

  // Pass through static assets and machine-readable files unchanged.
  if (
    pathname.startsWith("/assets/") ||
    pathname.startsWith("/_next/") ||
    pathname === "/llms.txt" ||
    pathname === "/sitemap.xml" ||
    pathname === "/robots.txt" ||
    ASSET_PATTERN.test(pathname)
  ) {
    return next();
  }

  const known = KNOWN_PAGES.has(pathname);
  const wants = preferredMedia(accept);

  if (wants === "not-acceptable") {
    return new Response("Not Acceptable", {
      status: 406,
      headers: { Vary: "Accept" },
    });
  }

  if (!known) {
    if (wants === "markdown") {
      return new Response(NOT_FOUND_MD, {
        status: 404,
        headers: {
          "Content-Type": "text/markdown; charset=utf-8",
          Vary: "Accept, Accept-Encoding",
        },
      });
    }

    // Let Vercel's static 404 page take over with the correct 404 status.
    return next();
  }

  if (wants === "markdown") {
    const mdPath = MARKDOWN_FILE.get(pathname) ?? "/index.md";
    return rewrite(new URL(mdPath, request.url), {
      headers: {
        Vary: "Accept, Accept-Encoding",
        "Content-Type": "text/markdown; charset=utf-8",
      },
    });
  }

  const htmlPath = HTML_FILE.get(pathname) ?? "/index.html";
  return rewrite(new URL(htmlPath, request.url), {
    headers: {
      Vary: "Accept, Accept-Encoding",
    },
  });
}
