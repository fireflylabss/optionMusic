import { readFileSync, statSync } from "node:fs";
import { resolve } from "node:path";
import { preferredMedia } from "../src/negotiate.ts";

const DIST = resolve(process.cwd(), "dist");
const PORT = 8124;

const MIME: Record<string, string> = {
  ".html": "text/html; charset=utf-8",
  ".md": "text/markdown; charset=utf-8",
  ".css": "text/css; charset=utf-8",
  ".js": "application/javascript; charset=utf-8",
  ".txt": "text/plain; charset=utf-8",
  ".xml": "application/xml; charset=utf-8",
  ".png": "image/png",
  ".jpg": "image/jpeg",
  ".jpeg": "image/jpeg",
  ".mp4": "video/mp4",
  ".woff2": "font/woff2",
};

const KNOWN = new Set(["/", "/about", "/contact", "/privacy", "/docs"]);

const NOT_FOUND_MD = `# Not found

This path is off the record. If you are looking for optionMusic documentation or source, try:

- [Home](/)
- [About](/about)
- [Contact](/contact)
- [Privacy](/privacy)
- [Developer resources (llms.txt)](/llms.txt)
- [Sitemap](/sitemap.xml)
- [GitHub repository](https://github.com/fireflylabss/optionMusic)
`;

function typeFor(path: string): string {
  const ext = path.slice(path.lastIndexOf("."));
  return MIME[ext] || "application/octet-stream";
}

function fileExists(path: string): boolean {
  const filePath = resolve(DIST, path);
  try {
    return statSync(filePath).isFile();
  } catch {
    return false;
  }
}

function serveFile(path: string, status = 200, extraHeaders?: Record<string, string>): Response {
  const filePath = resolve(DIST, path);
  if (!fileExists(path)) {
    return new Response("Not found", { status: 404 });
  }
  const body = readFileSync(filePath);
  return new Response(body, {
    status,
    headers: {
      "Content-Type": typeFor(path),
      "Vary": "Accept, Accept-Encoding",
      ...extraHeaders,
    },
  });
}

export default {
  port: PORT,
  fetch(req: Request) {
    const url = new URL(req.url);
    const { pathname } = url;
    const accept = req.headers.get("accept") || "";
    const wants = preferredMedia(accept);

    // Static assets and machine-readable files are served directly.
    if (pathname !== "/" && fileExists(pathname.slice(1))) {
      return serveFile(pathname.slice(1));
    }

    const known = KNOWN.has(pathname);

    if (wants === "not-acceptable") {
      return new Response("Not Acceptable", { status: 406, headers: { Vary: "Accept" } });
    }

    if (!known) {
      if (wants === "markdown") {
        return new Response(NOT_FOUND_MD, {
          status: 404,
          headers: { "Content-Type": "text/markdown; charset=utf-8", Vary: "Accept, Accept-Encoding" },
        });
      }
      return serveFile("404.html", 404);
    }

    const base = pathname === "/" ? "index" : pathname.slice(1);
    if (wants === "markdown") {
      return serveFile(`${base}.md`);
    }
    return serveFile(`${base}.html`);
  },
};

console.log(`Verification server running at http://localhost:${PORT}`);
console.log("Press Ctrl+C to stop.");
