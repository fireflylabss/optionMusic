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
