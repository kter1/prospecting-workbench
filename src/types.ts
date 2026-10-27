export const CONTRACT_VERSION = 3;

export type Confidence = "low" | "medium" | "high";
export type AccountStatus =
  | "new"
  | "queued"
  | "researched"
  | "qualified"
  | "needs_review"
  | "rejected"
  | "screening"
  | "researching"
  | "suppressed"
  | "failed";

export type OutcomeKind =
  | "contacted"
  | "replied"
  | "meeting"
  | "not_interested"
  | "wrong_fit"
  | "do_not_contact";

export interface SalesPlay {
  id: string;
  name: string;
  offer: string;
  icpIndustries: string[];
  companySizes: string[];
  targetPersonas: string[];
  exclusions: string[];
  proofPoints: string[];
  tone: string;
  cta: string;
  senderName: string;
  senderCompany: string;
  postalAddress: string;
  optOutText: string;
  approved: boolean;
  updatedAt?: string;
}

export interface OfferProfile {
  id: string;
  name: string;
  product: string;
  customerOutcome: string;
  proofPoints: string[];
  complianceRestrictions: string[];
  tone: string;
  cta: string;
  senderName: string;
  senderCompany: string;
  postalAddress: string;
  optOutText: string;
  updatedAt?: string;
}

export interface TargetSegment {
  id: string;
  label: string;
  discoveryCategory?: string;
}

export interface QualificationRule {
  id: string;
  label: string;
  component: "icp" | "signal" | "persona" | "evidence";
  keywords: string[];
  points: number;
  hardGate: boolean;
  /** Where the rule came from. Older stored briefs have none. */
  origin?: RuleOrigin;
}

export type RuleOrigin = "catalog" | "pack" | "model" | "query" | "user" | "";

export interface SignalSuggestions {
  rules: QualificationRule[];
  message: string;
}

export interface DiscoveryLane {
  id: string;
  label: string;
  mode: "automatic" | "guided" | "seeded" | "unavailable";
  source: string;
  reason: string;
  limitation: string;
}

export interface SearchBrief {
  id: string;
  query: string;
  productSummary: string;
  customerOutcome: string;
  segments: TargetSegment[];
  geography: string;
  companySizes: string[];
  targetPersonas: string[];
  positiveSignals: QualificationRule[];
  exclusions: string[];
  discoveryLanes: DiscoveryLane[];
  resultLimit: number;
  qualificationThreshold: number;
  qualityBand: number;
  approved: boolean;
  assumptions: string[];
  /** `browser_preview` is the placeholder brief used when no desktop backend exists. */
  interpreter: "ollama" | "manual_rules" | "browser_preview";
  modelTag?: string;
  modelDigest?: string;
  promptVersion: string;
  createdAt: string;
}

export interface FitFeature {
  ruleId: string;
  label: string;
  component: string;
  value: string;
  pointsAwarded: number;
  evidenceIds: string[];
  confidence: Confidence;
}

export interface SearchRunAccount {
  account: Account;
  state: "discovered" | "screening" | "researching" | "qualified" | "needs_review" | "rejected" | "failed";
  discoverySource: string;
  discoveryReason: string;
  score?: number;
  evidenceConfidence: Confidence | "unknown";
  rejectionReasons: string[];
  matchedFeatures: FitFeature[];
  assessment?: FitAssessment;
  rank?: number;
  resultTier: "strong" | "near_match" | "excluded";
}

export interface SourceWarning {
  source: string;
  message: string;
  usedCachedResults: boolean;
  cacheAgeHours?: number;
}

export interface SearchRun {
  id: string;
  query: string;
  briefId: string;
  status: SearchRunStatus;
  progressCompleted: number;
  progressTotal: number;
  progressPercent: number;
  progressStage: string;
  resultLimit: number;
  candidateCount: number;
  researchedCount: number;
  strongCount: number;
  nearMatchCount: number;
  warnings: SourceWarning[];
  startedAt: string;
  completedAt?: string;
  lastError?: string;
  /** How often each plan rule fired across the assessed candidates. */
  ruleHealth?: RuleHitRate[];
  accounts: SearchRunAccount[];
}

export interface RuleHitRate {
  ruleId: string;
  label: string;
  hits: number;
  assessed: number;
  verdict: "ok" | "too_few" | "never_found" | "not_discriminating";
}

export type SearchRunStatus = "running" | "complete" | "failed" | "cancelled";

/**
 * Payload of the `search-run-progress` event.
 *
 * Counters only. Progress fires once per researched candidate, so carrying the
 * full account list made each tick cost more as the pool grew. Accounts are
 * fetched separately.
 */
export interface SearchRunProgress {
  id: string;
  status: SearchRunStatus;
  progressCompleted: number;
  progressTotal: number;
  progressPercent: number;
  progressStage: string;
  resultLimit: number;
  candidateCount: number;
  researchedCount: number;
  strongCount: number;
  nearMatchCount: number;
  finished: boolean;
}

export interface SearchRunSummary {
  id: string;
  query: string;
  status: SearchRun["status"];
  progressCompleted: number;
  progressTotal: number;
  progressPercent: number;
  progressStage: string;
  qualifiedCount: number;
  reviewCount: number;
  strongCount: number;
  startedAt: string;
}

export interface AccountSeed {
  companyName: string;
  domain?: string;
  websiteUrl?: string;
  personName?: string;
  personTitle?: string;
  workEmail?: string;
  businessPhone?: string;
}

export interface Account {
  id: string;
  name: string;
  domain?: string;
  websiteUrl?: string;
  industry?: string;
  employeeBand?: string;
  location?: string;
  status: AccountStatus;
  score?: number;
  scoreLabel?: string;
  researchedAt?: string;
  createdAt: string;
  savedAt?: string;
  favoritedAt?: string;
  trashedAt?: string;
}

export interface Person {
  id: string;
  accountId: string;
  name: string;
  title?: string;
}

export type ContactKind =
  | "company_main"
  | "work_direct"
  | "role_email"
  | "work_email";

export interface ContactPoint {
  id: string;
  accountId: string;
  personId?: string;
  kind: ContactKind;
  value: string;
  evidenceId: string;
  confidence: Confidence;
}

export interface Evidence {
  id: string;
  accountId: string;
  sourceUrl: string;
  sourceKind: string;
  title: string;
  excerpt: string;
  retrievedAt: string;
  publishedAt?: string;
  contentHash: string;
  confidence: Confidence;
  freshness: "fresh" | "aging" | "stale" | "unknown";
  qualityFlags: Array<"stale" | "contradictory" | "incomplete">;
}

export interface Signal {
  id: string;
  accountId: string;
  category:
    | "funding"
    | "contract"
    | "leadership"
    | "hiring"
    | "product"
    | "partnership"
    | "filing"
    | "technical"
    | "other";
  summary: string;
  occurredAt?: string;
  evidenceId: string;
  confidence: Confidence;
}

export interface FitAssessment {
  hardGates: Array<{ label: string; status: "pass" | "fail" | "unknown"; reason: string }>;
  icpFit: number;
  signalRelevance: number;
  personaFit: number;
  evidenceContactability: number;
  total: number;
  reasons: Array<{ text: string; evidenceIds: string[] }>;
}

export interface DraftArtifact {
  id: string;
  kind: "initial" | "follow_up";
  subject: string;
  body: string;
  claimEvidence: Array<{ claim: string; evidenceIds: string[] }>;
  modelTag: string;
  modelDigest: string;
  promptVersion: string;
  verified: boolean;
  createdAt: string;
}

export interface CallBrief {
  opener: string;
  reasonNow: string;
  hypotheses: string[];
  discoveryQuestions: string[];
  relevantProof: string[];
  likelyObjections: string[];
  desiredOutcome: string;
  evidenceIds: string[];
}

export interface ProspectPackage {
  account: Account;
  people: Person[];
  contacts: ContactPoint[];
  evidence: Evidence[];
  signals: Signal[];
  assessment?: FitAssessment;
  draft?: DraftArtifact;
  callBrief?: CallBrief;
  suppressed: boolean;
  latestOutcome?: OutcomeKind;
  discoveryContext?: DiscoveryContext;
}

export interface DiscoveryContext {
  searchRunId: string;
  query: string;
  discoverySource: string;
  discoveryReason: string;
  state: SearchRunAccount["state"];
  rejectionReasons: string[];
  matchedFeatures: FitFeature[];
  evidenceConfidence: Confidence | "unknown";
  resultTier: "strong" | "near_match" | "excluded";
  rank?: number;
  assessment?: FitAssessment;
}

export interface OllamaHealth {
  reachable: boolean;
  modelInstalled: boolean;
  modelTag: string;
  modelDigest?: string;
  message: string;
}

export interface SourceCapability {
  id: string;
  name: string;
  modes: Array<"discover" | "refresh">;
  requiresConfiguration: boolean;
  enabled: boolean;
  detail: string;
}

export interface ResearchJob {
  id: string;
  accountId?: string;
  kind: "crawl" | "public_refresh" | "generate" | "daily_scan";
  status: "pending" | "running" | "complete" | "failed";
  attempts: number;
  lastError?: string;
  scheduledAt: string;
  completedAt?: string;
  progressPercent: number;
  progressStage: string;
  request?: DraftGenerationRequest;
}

export interface DraftGenerationRequest {
  accountId: string;
  provider: "ollama" | "fake";
  kind: "initial" | "follow_up";
  overrideReview: boolean;
}

export interface DraftReadiness {
  ready: boolean;
  missingFields: Array<"offer" | "senderName" | "senderCompany" | "postalAddress" | "optOutText">;
  blockers: string[];
  requiresReviewOverride: boolean;
  requiresPlayApproval: boolean;
  suggestedPlay: SalesPlay;
  ollama: OllamaHealth;
}

export type LibraryAction = "save" | "unsave" | "favorite" | "unfavorite" | "trash" | "restore";

export interface DiscoveryResult {
  source: string;
  companyName: string;
  domain?: string;
  websiteUrl?: string;
  externalId?: string;
  reason: string;
  sourceUrl: string;
  sourceExternalId?: string;
  businessPhone?: string;
  location?: string;
  identityConfidence?: Confidence;
}

export interface DashboardSnapshot {
  salesPlay?: SalesPlay;
  offerProfile?: OfferProfile;
  accounts: Account[];
  pendingJobs: number;
  dueForDailyScan: boolean;
  sourceCapabilities: SourceCapability[];
  searchRuns: SearchRunSummary[];
}
