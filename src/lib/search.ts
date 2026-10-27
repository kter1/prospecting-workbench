import type { QualificationRule, SearchBrief, SearchRun } from "../types";

export function searchProgress(run?: Pick<SearchRun, "status" | "progressPercent">): number {
  if (run?.status === "complete") return 100;
  return Math.min(100, Math.max(0, Math.round(run?.progressPercent ?? 0)));
}

export function canRunAutomatically(brief?: Pick<SearchBrief, "discoveryLanes">): boolean {
  return Boolean(brief?.discoveryLanes.some((lane) => lane.mode === "automatic"));
}

export function coverageLabel(brief: Pick<SearchBrief, "discoveryLanes">): string {
  const automatic = brief.discoveryLanes.filter((lane) => lane.mode === "automatic").length;
  const guided = brief.discoveryLanes.filter((lane) => lane.mode === "guided").length;
  if (automatic && guided) return `${automatic} automatic · ${guided} guided`;
  if (automatic) return `${automatic} automatic`;
  if (guided) return `${guided} guided only`;
  return "seeded only";
}

/**
 * Explain why a run returned fewer strong matches than were requested.
 *
 * Unresearched candidates were never checked, so attributing the shortfall to
 * the score band would report a rejection that never happened.
 */
export function shortfallExplanation(
  run: Pick<SearchRun, "candidateCount" | "researchedCount" | "strongCount">
): string {
  const unresearched = Math.max(0, run.candidateCount - run.researchedCount);
  if (unresearched > 0) {
    return `${unresearched} of ${run.candidateCount} discovered candidate${run.candidateCount === 1 ? "" : "s"} ${unresearched === 1 ? "was" : "were"} not researched before the search reached its time budget. Those were not rejected, they were never checked. Run the search again to continue.`;
  }
  if (run.candidateCount === 0) {
    return "No candidates were discovered from the enabled free sources for this brief.";
  }
  return "Every discovered candidate was researched. The remaining ones did not meet the score and evidence band.";
}

const QUERY_STOPWORDS = new Set([
  "a", "about", "am", "an", "and", "any", "are", "as", "at", "be", "been", "but", "by", "can",
  "do", "does", "för", "for", "from", "get", "getting", "got", "handle", "has", "have", "having",
  "help", "her", "here", "him", "his", "how", "i", "if", "in", "into", "is", "it", "its", "just",
  "like", "look", "looking", "me", "more", "my", "need", "needing", "of", "on", "or", "our", "out",
  "over", "own", "she", "should", "so", "some", "such", "than", "that", "the", "their", "them",
  "then", "there", "these", "they", "this", "those", "to", "try", "trying", "up", "us", "want",
  "wanting", "was", "we", "were", "what", "when", "where", "which", "who", "will", "with", "would",
  "you", "your"
]);

/** Salient words from a free-text query, in order, without duplicates. */
export function queryKeywords(query: string, limit = 6): string[] {
  const seen = new Set<string>();
  return query
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, " ")
    .split(/\s+/)
    .filter((word) => word.length > 2 && !QUERY_STOPWORDS.has(word))
    .filter((word) => (seen.has(word) ? false : (seen.add(word), true)))
    .slice(0, limit);
}

/**
 * Browser queries for the guided path.
 *
 * When the brief matched curated segments these lean on the segment labels.
 * When nothing matched they must still be usable: the previous version quoted
 * the user's entire sentence and emitted the internal `customerOutcome`
 * placeholder as a search string, so every link returned nothing.
 */
export function guidedSearchQueries(
  brief: Pick<SearchBrief, "query" | "segments" | "positiveSignals" | "geography">
): string[] {
  const geography = brief.geography.trim();
  const labels = brief.segments.map((segment) => segment.label).filter(Boolean);
  if (labels.length) {
    const signal = brief.positiveSignals[0]?.label;
    return [
      `"${labels[0]}" "about us" ${geography}`,
      labels[1] ? `"${labels[1]}" "about us" ${geography}` : `"${labels[0]}" directory ${geography}`,
      signal ? `"${labels[0]}" "${signal}" ${geography}` : `"${labels[0]}" association ${geography}`
    ].filter(Boolean);
  }

  const keywords = queryKeywords(brief.query);
  if (!keywords.length) return [];
  const stem = keywords.join(" ");
  return [
    `${stem} ${geography}`,
    `${stem} directory ${geography}`,
    `${stem} association members ${geography}`
  ];
}

export function safePublicWebsite(value?: string): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && Boolean(url.hostname) ? url.toString() : undefined;
  } catch {
    return undefined;
  }
}

/**
 * A placeholder brief for the browser preview, where no Tauri backend exists.
 *
 * This is deliberately NOT a second targeting compiler. It used to mirror the
 * Rust category and signal tables, and the two copies had already drifted:
 * different keywords, a missing rule, a hardcoded prompt version. The real
 * compiler lives in `src-tauri/src/catalog.rs` and now covers far more
 * verticals than any duplicate here could track. The preview returns a
 * guided-only brief that says so, so nothing in the browser can imply
 * automatic coverage the desktop app would actually determine.
 */
export function buildBrowserPreviewBrief(query: string, geography: string, resultLimit: number): SearchBrief {
  return {
    id: crypto.randomUUID(),
    query,
    productSummary: query,
    customerOutcome: "Address the outcome described in the request.",
    segments: [],
    geography,
    companySizes: [],
    targetPersonas: [],
    positiveSignals: [],
    exclusions: [],
    discoveryLanes: [
      {
        id: "guided-web",
        label: "General private businesses",
        mode: "guided",
        source: "browser",
        reason: "Browser preview only; the desktop app compiles the real search plan.",
        limitation: "Automatic discovery and qualification need the Tauri desktop runtime."
      }
    ],
    resultLimit,
    qualificationThreshold: 70,
    qualityBand: 4,
    approved: false,
    assumptions: [
      "This is a browser preview. Launch the desktop app to compile a real search plan against the curated discovery catalog."
    ],
    interpreter: "browser_preview",
    promptVersion: "browser-preview",
    createdAt: new Date().toISOString()
  };
}

/**
 * Keywords from the editor's comma-separated text. Quoted phrases keep their
 * quotes, which tell the matcher to compare exact words ("locations" means
 * more than one), and may themselves contain commas.
 */
export function parseKeywords(text: string): string[] {
  const keywords: string[] = [];
  const pattern = /\s*"([^"]*)"\s*|([^,]+)/g;
  for (const match of text.matchAll(pattern)) {
    const value = match[1] !== undefined ? `"${match[1].trim()}"` : match[2].trim();
    if (value && value !== '""' && !keywords.some((existing) => existing.toLowerCase() === value.toLowerCase())) keywords.push(value);
  }
  return keywords;
}

/** The editor's text for a keyword list. */
export function formatKeywords(keywords: string[]): string {
  return keywords.join(", ");
}

/**
 * Add model suggestions to the plan's rules. A suggestion never replaces or
 * duplicates a rule already there, by id or by label, so a rule the user
 * edited is never overwritten when suggestions arrive late.
 */
export function mergeSuggestedRules(existing: QualificationRule[], suggested: QualificationRule[], limit = 10): QualificationRule[] {
  const merged = [...existing];
  for (const rule of suggested) {
    if (merged.length >= limit) break;
    const duplicate = merged.some((item) => item.id === rule.id || item.label.trim().toLowerCase() === rule.label.trim().toLowerCase());
    if (!duplicate) merged.push(rule);
  }
  return merged;
}

/** A new, empty rule the user is about to fill in. */
export function blankRule(existing: QualificationRule[]): QualificationRule {
  let index = existing.length + 1;
  while (existing.some((rule) => rule.id === `user_${index}`)) index += 1;
  return { id: `user_${index}`, label: "", component: "signal", keywords: [], points: 45, hardGate: false, origin: "user" };
}
