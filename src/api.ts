import { invoke } from "@tauri-apps/api/core";
import { buildBrowserPreviewBrief } from "./lib/search";
import type {
  Account,
  AccountSeed,
  DashboardSnapshot,
  DiscoveryResult,
  DraftGenerationRequest,
  DraftReadiness,
  LibraryAction,
  OllamaHealth,
  OutcomeKind,
  ProspectPackage,
  ResearchJob,
  SearchBrief,
  SearchRun,
  SearchRunSummary,
  SalesPlay
} from "./types";

export const isDesktopRuntime = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

const browserSnapshot: DashboardSnapshot = {
  accounts: [],
  pendingJobs: 0,
  dueForDailyScan: false,
  sourceCapabilities: [
    { id: "overpass", name: "OpenStreetMap Overpass", modes: ["discover"], requiresConfiguration: false, enabled: true, detail: "Bounded mapped-business samples." },
    { id: "company_web", name: "Company websites", modes: ["refresh"], requiresConfiguration: false, enabled: true, detail: "Permitted public pages." },
    { id: "searxng", name: "Local SearXNG", modes: ["discover"], requiresConfiguration: true, enabled: false, detail: "Optional user-operated search." }
  ],
  searchRuns: []
};

export const api = {
  bootstrap: () => isDesktopRuntime ? invoke<DashboardSnapshot>("bootstrap_app") : Promise.resolve(browserSnapshot),
  saveSalesPlay: (salesPlay: SalesPlay) =>
    invoke<SalesPlay>("save_sales_play", { salesPlay }),
  proposeSalesPlay: (context: string, current: SalesPlay) =>
    invoke<SalesPlay>("propose_sales_play", { context, current }),
  interpretProspectingQuery: (query: string, geography: string, resultLimit: number, productWebsite?: string) =>
    isDesktopRuntime ? invoke<SearchBrief>("interpret_prospecting_query", { query, geography, resultLimit, productWebsite }) : Promise.resolve(buildBrowserPreviewBrief(query, geography, resultLimit)),
  startSearchRun: (brief: SearchBrief) =>
    isDesktopRuntime ? invoke<SearchRun>("start_search_run", { brief }) : Promise.resolve<SearchRun>({ id: crypto.randomUUID(), query: brief.query, briefId: brief.id, status: "complete", progressCompleted: 0, progressTotal: 0, progressPercent: 100, progressStage: "Complete", resultLimit: brief.resultLimit, candidateCount: 0, researchedCount: 0, strongCount: 0, nearMatchCount: 0, warnings: [], startedAt: new Date().toISOString(), completedAt: new Date().toISOString(), accounts: [] }),
  listSearchRuns: () => isDesktopRuntime ? invoke<SearchRunSummary[]>("list_search_runs") : Promise.resolve([]),
  getSearchRun: (runId: string) => invoke<SearchRun>("get_search_run", { runId }),
  cancelSearchRun: (runId: string) => invoke<SearchRun>("cancel_search_run", { runId }),
  importAccounts: (seeds: AccountSeed[]) =>
    invoke<Account[]>("import_accounts", { seeds }),
  listAccounts: () => invoke<Account[]>("list_accounts"),
  getProspectPackage: (accountId: string, searchRunId?: string) =>
    invoke<ProspectPackage>("get_prospect_package", { accountId, searchRunId }),
  applyLibraryAction: (accountIds: string[], action: LibraryAction) =>
    invoke<Account[]>("apply_library_action", { accountIds, action }),
  permanentlyDeleteAccounts: (accountIds: string[]) =>
    invoke<Account[]>("permanently_delete_accounts", { accountIds }),
  researchAccount: (accountId: string) =>
    invoke<ProspectPackage>("research_account", { accountId }),
  generatePackage: (accountId: string, provider: "ollama" | "fake" = "ollama", kind: "initial" | "follow_up" = "initial", overrideReview = false) =>
    invoke<ProspectPackage>("generate_prospect_package", { accountId, provider, kind, overrideReview }),
  getDraftReadiness: (accountId: string) =>
    invoke<DraftReadiness>("get_draft_readiness", { accountId }),
  startDraftGeneration: (request: DraftGenerationRequest) =>
    invoke<ResearchJob>("start_draft_generation", { request }),
  setOutcome: (accountId: string, outcome: OutcomeKind) =>
    invoke<ProspectPackage>("set_outcome", { accountId, outcome }),
  setSuppression: (accountId: string, suppressed: boolean) =>
    invoke<ProspectPackage>("set_suppression", { accountId, suppressed }),
  discoverPublic: (source: string, query: string) =>
    invoke<DiscoveryResult[]>("discover_public", { source, query }),
  runDailyScan: () => invoke<number>("run_daily_scan"),
  listJobs: () => invoke<ResearchJob[]>("list_jobs"),
  ollamaHealth: () => isDesktopRuntime ? invoke<OllamaHealth>("ollama_health") : Promise.resolve({ reachable: false, modelInstalled: false, modelTag: "qwen3.5:9b", message: "Browser preview; launch the Tauri desktop app to use Ollama." }),
  getSearxngUrl: () => isDesktopRuntime ? invoke<string>("get_searxng_url") : Promise.resolve(""),
  setSearxngUrl: (url: string) => isDesktopRuntime ? invoke<string>("set_searxng_url", { url }) : Promise.resolve(url),
  exportData: () => invoke<string>("export_data"),
  exportDiagnostics: () => invoke<string>("export_diagnostics"),
  importData: (payload: string) => invoke<void>("import_data", { payload }),
  deleteAllData: () => invoke<void>("delete_all_data")
};
