import type { DiscoveryLane, QualificationRule, SearchBrief, SearchRun, TargetSegment } from "../types";

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

export function safePublicWebsite(value?: string): string | undefined {
  if (!value) return undefined;
  try {
    const url = new URL(value);
    return ["http:", "https:"].includes(url.protocol) && Boolean(url.hostname) ? url.toString() : undefined;
  } catch {
    return undefined;
  }
}

export function buildBrowserPreviewBrief(query: string, geography: string, resultLimit: number): SearchBrief {
  const voiceRelated = /voice|phone|call|intake|answering/i.test(query);
  const catalog: Array<[RegExp, string, string]> = [
    [/plumb/i, "plumber", "Plumbing services"], [/electric/i, "electrician", "Electrical services"], [/hvac/i, "hvac", "HVAC services"], [/roof/i, "roofer", "Roofing services"], [/law/i, "lawyer", "Law firms"], [/property manag/i, "property_management", "Property management"], [/(?:auto|car repair)/i, "car_repair", "Auto service and repair"], [/restaurant/i, "restaurant", "Restaurants"], [/hotel/i, "hotel", "Hotels"]
  ];
  const explicitSegments = catalog.filter(([pattern]) => pattern.test(query)).map(([, id, label]) => [id, label] as [string, string]);
  const segmentTuples: Array<[string, string]> = explicitSegments.length
    ? explicitSegments
    : voiceRelated
      ? [["plumber", "Plumbing services"], ["hvac", "HVAC services"], ["lawyer", "Law firms"]]
      : [];
  const segments: TargetSegment[] = segmentTuples.map(([id, label]) => ({ id, label, discoveryCategory: id }));
  const rule = (id: string, label: string, keywords: string[], points: number, component: QualificationRule["component"] = "signal"): QualificationRule => ({ id, label, keywords, points, component, hardGate: false });
  const positiveSignals = voiceRelated
    ? [rule("after_hours", "After-hours or emergency service", ["24/7", "after hours", "emergency service"], 55), rule("call_first", "Phone-first customer intake", ["call now", "call us"], 35), rule("appointment", "Appointment or consultation workflow", ["appointment", "consultation"], 35), rule("intake_hiring", "Intake, dispatch, or receptionist hiring", ["dispatcher", "receptionist", "intake specialist"], 45)]
    : [rule("hiring", "Relevant hiring activity", ["hiring", "open positions"], 40)];
  const discoveryLanes: DiscoveryLane[] = segments.length
    ? [{ id: "local-businesses", label: "Mapped local businesses", mode: "automatic", source: "overpass", reason: "Supported business categories can be sampled within bounded regions.", limitation: "Coverage varies; results are not exhaustive." }, { id: "guided-web", label: "General private businesses", mode: "guided", source: "browser", reason: "Guided browser queries cover unsupported segments.", limitation: "The user selects results." }]
    : [{ id: "guided-web", label: "General private businesses", mode: "guided", source: "browser", reason: "No compliant automatic directory supports this broad query.", limitation: "The user selects results." }];
  return {
    id: crypto.randomUUID(), query, productSummary: query, customerOutcome: voiceRelated ? "Capture and route valuable inbound calls more consistently." : "Address the outcome described in the request.", segments, geography, companySizes: [], targetPersonas: voiceRelated ? ["Owner", "General Manager", "Operations", "Office Manager", "Intake", "Dispatch"] : ["Owner", "Operations"], positiveSignals, exclusions: voiceRelated ? ["public safety", "emergency response agency", "healthcare"] : [], discoveryLanes, resultLimit, qualificationThreshold: 70, qualityBand: 4, approved: false, assumptions: ["An instant deterministic compiler produced this editable plan; no model inference or network request was required."], interpreter: "manual_rules", promptVersion: "search-brief-v3", createdAt: new Date().toISOString()
  };
}
