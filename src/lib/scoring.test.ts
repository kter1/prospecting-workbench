import { describe, expect, it } from "vitest";
import { calculateFitScore, scoreLabel } from "./scoring";

describe("calculateFitScore", () => {
  it("uses the fixed, visible v1 weights", () => {
    expect(
      calculateFitScore({
        icpFit: 100,
        signalRelevance: 50,
        personaFit: 50,
        evidenceContactability: 100
      })
    ).toBe(75);
  });

  it("returns zero when a hard gate fails", () => {
    expect(
      calculateFitScore({
        icpFit: 100,
        signalRelevance: 100,
        personaFit: 100,
        evidenceContactability: 100,
        hardGateFailed: true
      })
    ).toBe(0);
  });

  it("clamps malformed component values", () => {
    expect(
      calculateFitScore({
        icpFit: 200,
        signalRelevance: -5,
        personaFit: 100,
        evidenceContactability: 100
      })
    ).toBe(70);
  });
});
describe("scoreLabel", () => {
  it("uses stable review bands", () => {
    expect(scoreLabel()).toBe("Not scored");
    expect(scoreLabel(49)).toBe("Weak fit");
    expect(scoreLabel(50)).toBe("Review");
    expect(scoreLabel(75)).toBe("Strong fit");
  });
});
