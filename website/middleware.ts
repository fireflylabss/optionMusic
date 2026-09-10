import { next, rewrite } from "@vercel/functions";
import { preferredMedia } from "./src/negotiate";

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
  matcher: ["/((?!.*\\..*).*)/?"],
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
