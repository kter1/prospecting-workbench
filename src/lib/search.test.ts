import { describe, expect, it } from "vitest";
import { buildBrowserPreviewBrief, canRunAutomatically, coverageLabel, safePublicWebsite, searchProgress } from "./search";

describe("prompt-first search helpers", () => {
  it("does not claim automatic coverage for guided-only briefs", () => {
    const brief = { discoveryLanes: [{ id: "g", label: "Guided", mode: "guided" as const, source: "browser", reason: "", limitation: "" }] };
    expect(canRunAutomatically(brief)).toBe(false);
    expect(coverageLabel(brief)).toBe("1 guided only");
  });

  it("clamps streamed progress", () => {
    expect(searchProgress({ status: "running", progressPercent: 37 })).toBe(37);
    expect(searchProgress({ status: "running", progressPercent: 109 })).toBe(100);
    expect(searchProgress({ status: "complete", progressPercent: 40 })).toBe(100);
    expect(searchProgress()).toBe(0);
  });

  it("builds a bounded voice-intake preview without arbitrary source queries", () => {
    const brief = buildBrowserPreviewBrief("voice intake for businesses with inbound calls", "United States", 25);
    expect(canRunAutomatically(brief)).toBe(true);
    expect(brief.segments.some((item) => item.discoveryCategory === "plumber")).toBe(true);
    expect(brief.segments.length).toBeLessThanOrEqual(3);
    expect(brief.discoveryLanes.every((item) => ["overpass", "browser"].includes(item.source))).toBe(true);

    const explicit = buildBrowserPreviewBrief("voice intake for plumbing companies", "United States", 10);
    expect(explicit.segments.map((item) => item.id)).toEqual(["plumber"]);
    expect(explicit.qualificationThreshold).toBe(70);
    expect(explicit.qualityBand).toBe(4);
  });

  it("only allows explicit public web links", () => {
    expect(safePublicWebsite("https://example.com/about")).toBe("https://example.com/about");
    expect(safePublicWebsite("http://example.com")).toBe("http://example.com/");
    expect(safePublicWebsite("file:///tmp/prospect")).toBeUndefined();
    expect(safePublicWebsite("javascript:alert(1)")).toBeUndefined();
    expect(safePublicWebsite("not a url")).toBeUndefined();
  });
});
