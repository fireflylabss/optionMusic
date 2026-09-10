import { describe, expect, it } from "bun:test";
import { preferredMedia } from "../src/negotiate";

describe("preferredMedia", () => {
  it("prefers markdown when Accept is text/markdown", () => {
    expect(preferredMedia("text/markdown")).toBe("markdown");
  });

  it("prefers html when Accept is text/html", () => {
    expect(preferredMedia("text/html")).toBe("html");
  });

  it("prefers markdown when it has higher q", () => {
    expect(preferredMedia("text/markdown, text/html;q=0.8")).toBe("markdown");
  });

  it("prefers html when it has higher q", () => {
    expect(preferredMedia("text/html;q=0.9, text/markdown;q=0.8")).toBe("html");
  });

  it("prefers markdown when both are listed at q=1", () => {
    expect(preferredMedia("text/markdown, text/html")).toBe("markdown");
  });

  it("falls back to html for wildcard Accept", () => {
    expect(preferredMedia("*/*")).toBe("html");
  });

  it("falls back to html for text/*", () => {
    expect(preferredMedia("text/*")).toBe("html");
  });

  it("returns not-acceptable for unsupported types", () => {
    expect(preferredMedia("application/json")).toBe("not-acceptable");
  });

  it("returns not-acceptable for text/plain only", () => {
    expect(preferredMedia("text/plain")).toBe("not-acceptable");
  });

  it("defaults to html when Accept is empty", () => {
    expect(preferredMedia("")).toBe("html");
  });

  it("ignores ranges with q=0", () => {
    expect(preferredMedia("text/html;q=0, text/markdown")).toBe("markdown");
  });
});
