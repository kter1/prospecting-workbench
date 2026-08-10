export interface ScoreInput {
  icpFit: number;
  signalRelevance: number;
  personaFit: number;
  evidenceContactability: number;
  hardGateFailed?: boolean;
}
const clamp = (value: number) => Math.min(100, Math.max(0, Math.round(value)));

export function calculateFitScore(input: ScoreInput): number {
  if (input.hardGateFailed) return 0;
  return clamp(
    clamp(input.icpFit) * 0.35 +
      clamp(input.signalRelevance) * 0.3 +
      clamp(input.personaFit) * 0.2 +
      clamp(input.evidenceContactability) * 0.15
  );
}

export function scoreLabel(score?: number): string {
  if (score === undefined) return "Not scored";
  if (score >= 75) return "Strong fit";
  if (score >= 50) return "Review";
  return "Weak fit";
}
