import { describe, expect, it } from "vitest";
import { buildBrowserPreviewBrief, canRunAutomatically, coverageLabel, guidedSearchQueries, queryKeywords, safePublicWebsite, searchProgress, shortfallExplanation } from "./search";

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

  it("never claims automatic coverage from the browser preview", () => {
    // The preview must not duplicate the Rust targeting compiler. It used to,
    // and the copies drifted; the desktop app owns category matching now.
    const brief = buildBrowserPreviewBrief("voice intake for plumbing companies", "United States", 25);
    expect(canRunAutomatically(brief)).toBe(false);
    expect(brief.segments).toEqual([]);
    expect(brief.positiveSignals).toEqual([]);
    expect(brief.interpreter).toBe("browser_preview");
    expect(brief.discoveryLanes.every((item) => item.source === "browser")).toBe(true);
    expect(brief.assumptions.join(" ")).toContain("browser preview");
    expect(brief.resultLimit).toBe(25);
    expect(brief.geography).toBe("United States");
  });

  it("does not blame the score band for candidates it never researched", () => {
    const truncated = shortfallExplanation({ candidateCount: 40, researchedCount: 12, strongCount: 3 });
    expect(truncated).toContain("28 of 40");
    expect(truncated).toContain("time budget");
    expect(truncated).toContain("never checked");
    expect(truncated).not.toContain("evidence band");

    const complete = shortfallExplanation({ candidateCount: 40, researchedCount: 40, strongCount: 3 });
    expect(complete).toContain("evidence band");
    expect(complete).not.toContain("time budget");

    expect(shortfallExplanation({ candidateCount: 0, researchedCount: 0, strongCount: 0 })).toContain(
      "No candidates were discovered"
    );
  });

  it("builds usable guided queries when no curated segment matched", () => {
    // Reported failure: "Have a script for a pilot. Looking to get
    // representation and management" matches no category, and the old fallback
    // quoted the whole sentence and emitted the internal customerOutcome
    // placeholder as a search string, so all three links returned nothing.
    const brief = {
      query: "Have a script for a pilot. Looking to get representation and management",
      segments: [],
      positiveSignals: [],
      geography: "United States"
    };
    const queries = guidedSearchQueries(brief);
    expect(queries).toHaveLength(3);
    for (const query of queries) {
      expect(query).not.toContain("Address the business outcome");
      expect(query).not.toContain("Have a script for a pilot");
      expect(query).toContain("United States");
      expect(query.length).toBeLessThan(120);
    }
    expect(queries[0]).toContain("script");
    expect(queries[0]).toContain("representation");
    expect(queries[0]).not.toContain("looking");
    expect(queries[1]).toContain("directory");
  });

  it("keeps segment labels when the brief did match a category", () => {
    const queries = guidedSearchQueries({
      query: "voice intake for plumbers",
      segments: [{ id: "plumber", label: "Plumbing services", discoveryCategory: "plumber" }],
      positiveSignals: [
        { id: "after_hours", label: "After-hours service", component: "signal", keywords: [], points: 55, hardGate: false }
      ],
      geography: "Ohio"
    });
    expect(queries[0]).toBe('"Plumbing services" "about us" Ohio');
    expect(queries[2]).toContain("After-hours service");
  });

  it("extracts salient keywords and drops filler", () => {
    expect(queryKeywords("Looking to get representation and management for my script")).toEqual([
      "representation",
      "management",
      "script"
    ]);
    expect(queryKeywords("")).toEqual([]);
    expect(queryKeywords("the and of to")).toEqual([]);
  });

  it("only allows explicit public web links", () => {
    expect(safePublicWebsite("https://example.com/about")).toBe("https://example.com/about");
    expect(safePublicWebsite("http://example.com")).toBe("http://example.com/");
    expect(safePublicWebsite("file:///tmp/prospect")).toBeUndefined();
    expect(safePublicWebsite("javascript:alert(1)")).toBeUndefined();
    expect(safePublicWebsite("not a url")).toBeUndefined();
  });
});
