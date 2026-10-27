import { FormEvent, useCallback, useEffect, useRef, useState } from "react";
import { openUrl } from "@tauri-apps/plugin-opener";
import { isPermissionGranted, requestPermission, sendNotification } from "@tauri-apps/plugin-notification";
import { listen } from "@tauri-apps/api/event";
import { api, isDesktopRuntime } from "./api";
import { parseSeedInput } from "./lib/csv";
import { SignalEditor } from "./SignalEditor";
import { canRunAutomatically, coverageLabel, guidedSearchQueries, mergeSuggestedRules, safePublicWebsite, searchProgress, shortfallExplanation } from "./lib/search";
import type {
  Account,
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
  SearchRunProgress,
  SearchRunSummary,
  SalesPlay
} from "./types";

type View = "discover" | "prospects" | "setup";

const DEFAULT_PLAY: SalesPlay = {
  id: "active",
  name: "Founder-led outbound",
  offer: "",
  icpIndustries: [],
  companySizes: [],
  targetPersonas: [],
  exclusions: [],
  proofPoints: [],
  tone: "Concise, specific, and respectful",
  cta: "Ask for a short exploratory conversation",
  senderName: "",
  senderCompany: "",
  postalAddress: "",
  optOutText: "If this is not relevant, reply and I will not contact you again.",
  approved: false
};

const splitList = (value: string) =>
  value
    .split(/[,\n]/)
    .map((item) => item.trim())
    .filter(Boolean);

const formatDate = (value?: string) => {
  if (!value) return "Not yet";
  return new Intl.DateTimeFormat(undefined, { month: "short", day: "numeric", year: "numeric" }).format(
    new Date(value)
  );
};

function Badge({ children, tone = "neutral" }: { children: React.ReactNode; tone?: string }) {
  return <span className={`badge badge-${tone}`}>{children}</span>;
}

/**
 * Open a public URL in the user's browser, surfacing any refusal.
 *
 * Every call site used to be `void openUrl(...)`, which discards the promise.
 * When the opener capability rejected a URL the click did nothing at all and
 * reported nothing, which is indistinguishable from a dead button.
 */
function openExternal(url: string, onError: (message: string) => void) {
  void openUrl(url).catch((reason) => {
    onError(`Could not open ${url}. ${String(reason)}`);
  });
}

/**
 * A confirmation the app draws itself.
 *
 * `window.confirm` does nothing in a Tauri webview. WKWebView has no JS dialog
 * panel and the dialog plugin is not installed, so every destructive action
 * guarded by it silently did nothing when clicked.
 */
type ConfirmRequest = {
  title: string;
  body: string;
  confirmLabel: string;
  onConfirm: () => void;
};

function ConfirmDialog({ request, onDismiss }: { request: ConfirmRequest; onDismiss: () => void }) {
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => { if (event.key === "Escape") onDismiss(); };
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onDismiss]);
  return <div className="modal-backdrop" role="presentation" onClick={onDismiss}>
    <div className="modal panel confirm-modal" role="alertdialog" aria-modal="true" aria-labelledby="confirm-title" onClick={(event) => event.stopPropagation()}>
      <h2 id="confirm-title">{request.title}</h2>
      <p>{request.body}</p>
      <div className="receipt-actions">
        <button type="button" className="button ghost" onClick={onDismiss}>Cancel</button>
        <button type="button" className="button primary danger-solid" autoFocus onClick={() => { onDismiss(); request.onConfirm(); }}>{request.confirmLabel}</button>
      </div>
    </div>
  </div>;
}

function GlobalSearchProgress({ run, onOpen, onCancel }: { run: SearchRun; onOpen: () => void; onCancel: () => void }) {
  const progress = searchProgress(run);
  return <section className="global-search-progress" aria-label="Active search progress">
    <button type="button" onClick={onOpen} className="global-search-copy">
      <span className="status-dot" />
      <span><strong>{run.progressStage}</strong><small>{run.progressTotal ? `${run.progressCompleted} of ${run.progressTotal} candidates checked` : "Public-source discovery"}</small></span>
    </button>
    <div className="global-progress-meter"><div className="progress-track" role="progressbar" aria-label={run.progressStage} aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress}><span style={{ width: `${progress}%` }} /></div></div>
    <strong className="global-progress-percent">{progress}%</strong>
    <button type="button" className="button ghost" onClick={onCancel}>Cancel</button>
  </section>;
}

function GlobalDraftProgress({ job, onOpen }: { job: ResearchJob; onOpen: () => void }) {
  return <section className="global-search-progress draft-progress" aria-label="Local draft progress">
    <button type="button" onClick={onOpen} className="global-search-copy"><span className="status-dot" /><span><strong>{job.progressStage}</strong><small>Local Ollama drafting · {job.status}</small></span></button>
    <div className="global-progress-meter"><div className="progress-track" role="progressbar" aria-label={job.progressStage} aria-valuemin={0} aria-valuemax={100} aria-valuenow={job.progressPercent}><span style={{ width: `${job.progressPercent}%` }} /></div></div>
    <strong className="global-progress-percent">{job.progressPercent}%</strong>
  </section>;
}

function App() {
  const [view, setView] = useState<View>("discover");
  const [snapshot, setSnapshot] = useState<DashboardSnapshot>();
  const [selectedId, setSelectedId] = useState<string>();
  const [selected, setSelected] = useState<ProspectPackage>();
  const [busy, setBusy] = useState<string>();
  const [error, setError] = useState<string>();
  const [notice, setNotice] = useState<string>();
  const [activeRun, setActiveRun] = useState<SearchRun>();
  const [selectedRunId, setSelectedRunId] = useState<string>();
  const [activeDraftJob, setActiveDraftJob] = useState<ResearchJob>();
  const [draftSetup, setDraftSetup] = useState<{ readiness: DraftReadiness; kind: "initial" | "follow_up" }>();
  const [undoTrash, setUndoTrash] = useState<{ ids: string[]; until: number }>();
  const [discoverRevision, setDiscoverRevision] = useState(0);
  const [confirmRequest, setConfirmRequest] = useState<ConfirmRequest>();

  const refresh = useCallback(async () => {
    try {
      const next = await api.bootstrap();
      setSnapshot(next);
      setError(undefined);
      if (!selectedId && next.accounts[0]) setSelectedId(next.accounts[0].id);
    } catch (reason) {
      setError(String(reason));
    }
  }, [selectedId]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!isDesktopRuntime) return;
    let unlisten: (() => void) | undefined;
    // Progress carries counters only, so merge it into the run we already hold
    // instead of replacing it. Reload the full run once, on completion.
    void listen<SearchRunProgress>("search-run-progress", (event) => {
      const progress = event.payload;
      setActiveRun((current) => (current && current.id !== progress.id ? current : { ...(current as SearchRun), ...progress }));
      if (progress.finished) {
        void api.getSearchRun(progress.id).then(setActiveRun).catch(() => undefined);
        void refresh();
      }
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, [refresh]);

  useEffect(() => {
    if (!isDesktopRuntime) return;
    let unlisten: (() => void) | undefined;
    void listen<ResearchJob>("draft-job-progress", (event) => {
      setActiveDraftJob((current) => !current || current.id === event.payload.id ? event.payload : current);
      if (event.payload.status === "complete") {
        if (event.payload.accountId === selectedId) void api.getProspectPackage(selectedId!, selectedRunId).then(setSelected);
        void refresh();
        setNotice("Prospect package generated and verified locally.");
      } else if (event.payload.status === "failed") {
        setError(event.payload.lastError ?? "Local draft generation failed.");
      }
    }).then((stop) => { unlisten = stop; });
    return () => unlisten?.();
  }, [refresh, selectedId, selectedRunId]);

  useEffect(() => {
    if (!isDesktopRuntime || activeRun) return;
    const running = snapshot?.searchRuns.find((run) => run.status === "running");
    if (running) {
      void api.getSearchRun(running.id).then(setActiveRun).catch((reason) => setError(String(reason)));
    }
  }, [activeRun, snapshot?.searchRuns]);

  useEffect(() => {
    if (!isDesktopRuntime || activeDraftJob || !snapshot?.pendingJobs) return;
    void api.listJobs().then((jobs) => {
      const job = jobs.find((item) => item.kind === "generate" && ["pending", "running"].includes(item.status));
      if (job) setActiveDraftJob(job);
    }).catch((reason) => setError(String(reason)));
  }, [activeDraftJob, snapshot?.pendingJobs]);

  // Safety net only. The `search-run-progress` event drives the UI; this covers
  // the case where an emit is dropped (every emit is best-effort in the
  // backend). It refetches the whole run, so it must stay infrequent.
  useEffect(() => {
    if (!isDesktopRuntime || activeRun?.status !== "running") return;
    const timer = window.setInterval(() => {
      void api
        .getSearchRun(activeRun.id)
        .then((run) => {
          setActiveRun(run);
          if (run.status !== "running") void refresh();
        })
        .catch((reason) => setError(String(reason)));
    }, 10_000);
    return () => window.clearInterval(timer);
  }, [activeRun?.id, activeRun?.status, refresh]);

  useEffect(() => {
    const timer = window.setInterval(() => void refresh(), 60 * 60 * 1000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (!selectedId) {
      setSelected(undefined);
      return;
    }
    void api.getProspectPackage(selectedId, selectedRunId).then(setSelected).catch((reason) => setError(String(reason)));
  }, [selectedId, selectedRunId]);

  useEffect(() => {
    if (!activeDraftJob || !["pending", "running"].includes(activeDraftJob.status)) return;
    const timer = window.setInterval(() => void api.listJobs().then((jobs) => {
      const latest = jobs.find((job) => job.id === activeDraftJob.id);
      if (latest) setActiveDraftJob(latest);
    }).catch((reason) => setError(String(reason))), 10_000);
    return () => window.clearInterval(timer);
  }, [activeDraftJob?.id, activeDraftJob?.status]);

  useEffect(() => {
    if (selectedId && activeDraftJob?.status === "complete" && activeDraftJob.accountId === selectedId) {
      void api.getProspectPackage(selectedId, selectedRunId).then(setSelected).then(refresh);
    }
  }, [activeDraftJob?.id, activeDraftJob?.status, activeDraftJob?.accountId, selectedId, selectedRunId, refresh]);

  useEffect(() => {
    if (!snapshot?.dueForDailyScan) return;
    void api
      .runDailyScan()
      .then(async (count) => {
        if (!count) return;
        let granted = await isPermissionGranted();
        if (!granted) granted = (await requestPermission()) === "granted";
        if (granted) {
          sendNotification({ title: "Prospecting Workbench", body: `${count} account refreshes queued.` });
        }
        await refresh();
      })
      .catch((reason) => setError(String(reason)));
  }, [snapshot?.dueForDailyScan, refresh]);

  const withBusy = async (label: string, action: () => Promise<void>) => {
    setBusy(label);
    setError(undefined);
    setNotice(undefined);
    try {
      await action();
    } catch (reason) {
      setError(String(reason));
    } finally {
      setBusy(undefined);
    }
  };

  const loadSelected = async () => {
    if (!selectedId) return;
    setSelected(await api.getProspectPackage(selectedId, selectedRunId));
    const accounts = await api.listAccounts();
    setSnapshot((current) => (current ? { ...current, accounts } : current));
  };

  const applyLibrary = async (ids: string[], action: LibraryAction) => {
    const accounts = await api.applyLibraryAction(ids, action);
    setSnapshot((current) => current ? { ...current, accounts } : current);
    if (activeRun) setActiveRun(await api.getSearchRun(activeRun.id));
    if (selectedId && ids.includes(selectedId)) setSelected(await api.getProspectPackage(selectedId, selectedRunId));
    if (action === "trash") {
      const until = Date.now() + 8_000;
      setUndoTrash({ ids, until });
      window.setTimeout(() => setUndoTrash((current) => current?.until === until ? undefined : current), 8_000);
    }
  };

  const requestDraft = async (kind: "initial" | "follow_up") => {
    if (!selectedId) return;
    await withBusy("draft-readiness", async () => {
      const readiness = await api.getDraftReadiness(selectedId);
      if (readiness.blockers.length) throw new Error(readiness.blockers.join(" "));
      if (readiness.missingFields.length || readiness.requiresReviewOverride || readiness.requiresPlayApproval) {
        setDraftSetup({ readiness, kind });
        return;
      }
      setActiveDraftJob(await api.startDraftGeneration({ accountId: selectedId, provider: "ollama", kind, overrideReview: false }));
      setNotice("Local generation queued. You can continue using the app.");
    });
  };

  const saveSetupAndGenerate = async (play: SalesPlay, overrideReview: boolean) => {
    if (!selectedId || !draftSetup) return;
    await withBusy("draft-setup", async () => {
      const saved = await api.saveSalesPlay({ ...play, id: "active", approved: true });
      setSnapshot((current) => current ? { ...current, salesPlay: saved } : current);
      const request: DraftGenerationRequest = { accountId: selectedId, provider: "ollama", kind: draftSetup.kind, overrideReview };
      setActiveDraftJob(await api.startDraftGeneration(request));
      setDraftSetup(undefined);
      setNotice("Local generation queued. You can continue using the app.");
    });
  };

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="brand">
          <div className="brand-mark">PW</div>
          <div>
            <strong>Prospecting</strong>
            <span>Workbench</span>
          </div>
        </div>
        <nav aria-label="Primary navigation">
          {([
            ["discover", "Discover", "⌕"],
            ["prospects", "Prospects", "⌁"],
            ["setup", "Setup", "⚙"]
          ] as Array<[View, string, string]>).map(([id, label, icon]) => (
            <button className={view === id ? "nav-item active" : "nav-item"} onClick={() => setView(id)} key={id}>
              <span aria-hidden="true">{icon}</span>{label}
            </button>
          ))}
        </nav>
        <div className="local-card">
          <span className="status-dot" />
          <div><strong>Local only</strong><span>No cloud. No telemetry.</span></div>
        </div>
      </aside>

      <main>
        <header className="topbar">
          <div>
            <span className="eyebrow">{snapshot?.salesPlay?.name ?? "Local prospecting"}</span>
            <h1>{view === "discover" ? "Describe what you sell" : view === "setup" ? "Offer, compliance, and sources" : "Evidence before outreach"}</h1>
          </div>
          <div className="top-actions">
            <Badge tone={snapshot?.salesPlay?.approved ? "success" : "neutral"}>
              {snapshot?.salesPlay?.approved ? "Offer saved" : "Offer profile optional"}
            </Badge>
            <button className="button primary" onClick={() => { if (activeRun?.status !== "running") { setActiveRun(undefined); setNotice(undefined); setError(undefined); setDiscoverRevision((value) => value + 1); } setView("discover"); }}>New search</button>
          </div>
        </header>

        {activeRun?.status === "running" && <GlobalSearchProgress
          run={activeRun}
          onOpen={() => setView("discover")}
          onCancel={() => void withBusy("cancel-run", async () => setActiveRun(await api.cancelSearchRun(activeRun.id)))}
        />}
        {activeDraftJob && ["pending", "running"].includes(activeDraftJob.status) && <GlobalDraftProgress job={activeDraftJob} onOpen={() => { if (activeDraftJob.accountId) setSelectedId(activeDraftJob.accountId); setView("prospects"); }} />}

        {error && <div className="alert error"><strong>Action blocked</strong><span>{error}</span></div>}
        {notice && <div className="alert success"><strong>Status</strong><span>{notice}</span></div>}
        {undoTrash && Date.now() < undoTrash.until && <div className="alert warning"><strong>Moved to Trash</strong><span>{undoTrash.ids.length} prospect{undoTrash.ids.length === 1 ? "" : "s"} moved.</span><button className="link-button" onClick={() => void withBusy("undo-trash", async () => { await applyLibrary(undoTrash.ids, "restore"); setUndoTrash(undefined); })}>Undo</button></div>}

        {view === "setup" && (<>
          <SalesPlayEditor
            initial={snapshot?.salesPlay ?? DEFAULT_PLAY}
            busy={busy === "play"}
            proposing={busy === "propose-play"}
            onPropose={(context, current) => withBusy("propose-play", async () => {
              const proposed = await api.proposeSalesPlay(context, current);
              setSnapshot((snapshot) => snapshot ? { ...snapshot, salesPlay: proposed } : { accounts: [], pendingJobs: 0, dueForDailyScan: false, sourceCapabilities: [], searchRuns: [], salesPlay: proposed });
              setNotice("Local proposal ready. Review every field, then approve explicitly.");
            })}
            onSave={(play) => withBusy("play", async () => {
              const saved = await api.saveSalesPlay(play);
              setSnapshot((current) => current ? { ...current, salesPlay: saved } : current);
              setNotice("Sales play saved and approved.");
              setView("discover");
            })}
          />
          <SettingsView
            capabilities={snapshot?.sourceCapabilities ?? []}
            requestConfirm={setConfirmRequest}
            onExport={() => withBusy("export", async () => setNotice(`Export written to ${await api.exportData()}`))}
            onDiagnostics={() => withBusy("diagnostics", async () => setNotice(`Privacy-safe diagnostics written to ${await api.exportDiagnostics()}`))}
            onImport={(payload) => withBusy("import-data", async () => {
              await api.importData(payload);
              setSelected(undefined);
              setSelectedId(undefined);
              await refresh();
              setNotice("Local export restored.");
            })}
            onClearProspects={() => withBusy("clear-prospects", async () => {
              await api.clearProspects();
              setSelected(undefined);
              setSelectedId(undefined);
              setActiveRun(undefined);
              await refresh();
              setNotice("All prospects and search history removed. Offer profile and settings kept.");
            })}
            onDelete={() => withBusy("delete", async () => {
              await api.deleteAllData();
              setSelected(undefined);
              setSelectedId(undefined);
              setActiveRun(undefined);
              await refresh();
              setNotice("All local prospecting data deleted.");
            })}
          />
        </>)}

        {view === "discover" && (
          <DiscoverView
            key={discoverRevision}
            capabilities={snapshot?.sourceCapabilities ?? []}
            recentRuns={snapshot?.searchRuns ?? []}
            requestConfirm={setConfirmRequest}
            activeRun={activeRun}
            busy={busy}
            onInterpret={(query, geography, resultLimit, productWebsite) => api.interpretProspectingQuery(query, geography, resultLimit, productWebsite)}
            onStart={(brief, reuseFrom) => withBusy("search-run", async () => {
              const run = await api.startSearchRun(brief, reuseFrom);
              setActiveRun(run);
              setNotice(reuseFrom ? "Re-scoring the earlier candidates with your edited signals." : "Discovery started. Evidence-backed candidates will appear progressively.");
            })}
            onCancel={() => activeRun && withBusy("cancel-run", async () => setActiveRun(await api.cancelSearchRun(activeRun.id)))}
            onClearRun={() => { setActiveRun(undefined); setNotice(undefined); setError(undefined); }}
            onClearHistory={() => withBusy("clear-history", async () => {
              await api.clearSearchHistory();
              setActiveRun(undefined);
              await refresh();
              setNotice("Search history cleared. Saved prospects were kept.");
            })}
            onOpenRun={async (runId) => setActiveRun(await api.getSearchRun(runId))}
            onOpenAccount={(accountId, runId) => { setSelectedRunId(runId); setSelectedId(accountId); setView("prospects"); }}
            onLibraryAction={(ids, action) => withBusy(`library-${action}`, async () => applyLibrary(ids, action))}
            onImport={(raw) => withBusy("import", async () => {
              const seeds = parseSeedInput(raw);
              if (!seeds.length) throw new Error("Add at least one domain, URL, or CSV row.");
              const accounts = await api.importAccounts(seeds);
              setSnapshot((current) => current ? { ...current, accounts } : current);
              setSelectedId(accounts[0]?.id);
              setNotice(`${seeds.length} seed${seeds.length === 1 ? "" : "s"} imported.`);
              setView("prospects");
            })}
            onDiscover={(source, query) => api.discoverPublic(source, query)}
            onAddResults={(results) => withBusy("import", async () => {
              const accounts = await api.importAccounts(results.map((result) => ({
                companyName: result.companyName,
                domain: result.domain,
                websiteUrl: result.websiteUrl
              })));
              setSnapshot((current) => current ? { ...current, accounts } : current);
              setNotice(`${results.length} public-source result${results.length === 1 ? "" : "s"} added.`);
            })}
          />
        )}

        {view === "prospects" && (
          <ProspectsView
            accounts={snapshot?.accounts ?? []}
            selected={selected}
            selectedId={selectedId}
            busy={busy}
            draftJob={activeDraftJob}
            onError={setError}
            requestConfirm={setConfirmRequest}
            onSelect={(id) => { setSelectedRunId(undefined); setSelectedId(id); }}
            onLibraryAction={(ids, action) => withBusy(`library-${action}`, async () => applyLibrary(ids, action))}
            onPermanentDelete={(ids) => withBusy("permanent-delete", async () => {
              const accounts = await api.permanentlyDeleteAccounts(ids);
              setSnapshot((current) => current ? { ...current, accounts } : current);
              if (selectedId && ids.includes(selectedId)) { setSelectedId(undefined); setSelected(undefined); }
            })}
            onResearch={() => withBusy("research", async () => {
              if (!selectedId) return;
              setSelected(await api.researchAccount(selectedId));
              await loadSelected();
              setNotice("Public evidence refreshed.");
            })}
            onGenerate={(kind = "initial") => requestDraft(kind)}
            onOutcome={(outcome) => withBusy("outcome", async () => {
              if (!selectedId) return;
              setSelected(await api.setOutcome(selectedId, outcome));
              await loadSelected();
            })}
            onSuppress={(suppressed) => withBusy("suppress", async () => {
              if (!selectedId) return;
              setSelected(await api.setSuppression(selectedId, suppressed));
              await loadSelected();
            })}
          />
        )}

        {confirmRequest && <ConfirmDialog request={confirmRequest} onDismiss={() => setConfirmRequest(undefined)} />}

        {draftSetup && <DraftSetupDialog
          readiness={draftSetup.readiness}
          busy={busy === "draft-setup"}
          onCancel={() => setDraftSetup(undefined)}
          onSubmit={saveSetupAndGenerate}
        />}

      </main>
    </div>
  );
}

function SalesPlayEditor({ initial, busy, proposing, onPropose, onSave }: { initial: SalesPlay; busy: boolean; proposing: boolean; onPropose: (context: string, current: SalesPlay) => void; onSave: (play: SalesPlay) => void }) {
  const [play, setPlay] = useState(initial);
  const [proposalContext, setProposalContext] = useState("");
  useEffect(() => setPlay(initial), [initial]);
  const update = <K extends keyof SalesPlay>(key: K, value: SalesPlay[K]) => setPlay((current) => ({ ...current, [key]: value }));

  return (
    <form className="page-grid play-grid" onSubmit={(event) => { event.preventDefault(); onSave({ ...play, approved: true }); }}>
      <section className="panel span-2 proposal-panel"><div><span className="step">Local AI setup</span><h2>Start from a website or description</h2><p>Ollama can propose the play structure. Website text is sanitized and treated as untrusted evidence. The proposal remains unapproved until you review it.</p></div><div><textarea value={proposalContext} onChange={(event) => setProposalContext(event.target.value)} placeholder="https://your-company.example, or describe the offer, customers, and proof you can substantiate."/><button type="button" className="button secondary wide" disabled={proposing} onClick={() => onPropose(proposalContext, play)}>{proposing ? "Proposing locally…" : "Propose with Ollama"}</button></div></section>
      <section className="panel span-2">
        <div className="section-heading"><div><span className="step">01</span><h2>Offer and audience</h2></div><p>These stable offer details inform future query interpretation. Every search brief is still reviewed separately.</p></div>
        <div className="form-grid">
          <label>Play name<input value={play.name} onChange={(e) => update("name", e.target.value)} required /></label>
          <label className="span-2">What are you offering?<textarea value={play.offer} onChange={(e) => update("offer", e.target.value)} required placeholder="Describe the result you create and for whom." /></label>
          <label>ICP industries<input value={play.icpIndustries.join(", ")} onChange={(e) => update("icpIndustries", splitList(e.target.value))} placeholder="Healthcare, logistics" /></label>
          <label>Company sizes<input value={play.companySizes.join(", ")} onChange={(e) => update("companySizes", splitList(e.target.value))} placeholder="11–50, 51–200" /></label>
          <label>Target personas<input value={play.targetPersonas.join(", ")} onChange={(e) => update("targetPersonas", splitList(e.target.value))} placeholder="Founder, VP Operations" /></label>
          <label>Exclusions<input value={play.exclusions.join(", ")} onChange={(e) => update("exclusions", splitList(e.target.value))} placeholder="Agencies, non-US" /></label>
          <label className="span-2">Proof points<textarea value={play.proofPoints.join("\n")} onChange={(e) => update("proofPoints", splitList(e.target.value))} placeholder="One factual proof point per line" /></label>
        </div>
      </section>
      <section className="panel">
        <div className="section-heading"><div><span className="step">02</span><h2>Voice</h2></div></div>
        <label>Tone<input value={play.tone} onChange={(e) => update("tone", e.target.value)} required /></label>
        <label>Call to action<textarea value={play.cta} onChange={(e) => update("cta", e.target.value)} required /></label>
      </section>
      <section className="panel">
        <div className="section-heading"><div><span className="step">03</span><h2>Commercial identity</h2></div></div>
        <p className="muted">These fields are optional for research but required before a commercial draft can be generated.</p>
        <label>Sender name<input value={play.senderName} onChange={(e) => update("senderName", e.target.value)} /></label>
        <label>Company<input value={play.senderCompany} onChange={(e) => update("senderCompany", e.target.value)} /></label>
        <label>Postal address<textarea value={play.postalAddress} onChange={(e) => update("postalAddress", e.target.value)} /></label>
        <label>Opt-out wording<textarea value={play.optOutText} onChange={(e) => update("optOutText", e.target.value)} /></label>
        <button className="button primary wide" disabled={busy}>{busy ? "Saving…" : "Approve offer profile"}</button>
      </section>
    </form>
  );
}

/** Up to three signal rules a candidate matched, each with its source count. */
function ruleChips(item: SearchRun["accounts"][number]) {
  const signals = item.matchedFeatures.filter((feature) => feature.component === "signal" && feature.evidenceIds.length > 0);
  if (!signals.length) return null;
  return <div className="rule-chips">{signals.slice(0, 3).map((feature) => <span className="chip" key={feature.ruleId} title={`${feature.evidenceIds.length} quoted source${feature.evidenceIds.length === 1 ? "" : "s"}; open the candidate to read them`}>{feature.label}</span>)}{signals.length > 3 && <span className="chip muted-chip">+{signals.length - 3}</span>}</div>;
}

const RULE_VERDICT: Record<string, string> = {
  not_discriminating: "fired on almost every candidate, so it is not telling them apart",
  never_found: "was found nowhere; its keywords may not match how these sites write"
};

/** Rules that are not doing their job in this run, so the user can fix them. */
function RuleHealth({ rates }: { rates: NonNullable<SearchRun["ruleHealth"]> }) {
  const flagged = rates.filter((rate) => RULE_VERDICT[rate.verdict]);
  if (!flagged.length) return null;
  return <div className="rule-health" role="note"><strong>Signals to review</strong><ul>{flagged.map((rate) => <li key={rate.ruleId}><b>{rate.label}</b> ({rate.hits} of {rate.assessed}) {RULE_VERDICT[rate.verdict]}.</li>)}</ul><span>Use “Edit signals and re-score” to adjust them without searching again.</span></div>;
}

function DiscoverView({ capabilities, recentRuns, activeRun, busy, requestConfirm, onInterpret, onStart, onCancel, onClearRun, onClearHistory, onOpenRun, onOpenAccount, onLibraryAction, onImport, onDiscover, onAddResults }: {
  capabilities: DashboardSnapshot["sourceCapabilities"];
  recentRuns: SearchRunSummary[];
  activeRun?: SearchRun;
  busy?: string;
  onInterpret: (query: string, geography: string, resultLimit: number, productWebsite?: string) => Promise<SearchBrief>;
  onStart: (brief: SearchBrief, reuseCandidatesFrom?: string) => Promise<void>;
  onCancel: () => void;
  onClearRun: () => void;
  onClearHistory: () => void;
  requestConfirm: (request: ConfirmRequest) => void;
  onOpenRun: (runId: string) => Promise<void>;
  onOpenAccount: (accountId: string, runId: string) => void;
  onLibraryAction: (accountIds: string[], action: LibraryAction) => void;
  onImport: (raw: string) => void;
  onDiscover: (source: string, query: string) => Promise<DiscoveryResult[]>;
  onAddResults: (results: DiscoveryResult[]) => void;
}) {
  const [query, setQuery] = useState("");
  const [productWebsite, setProductWebsite] = useState("");
  const [geography, setGeography] = useState("United States");
  const [resultLimit, setResultLimit] = useState(10);
  const [brief, setBrief] = useState<SearchBrief>();
  const [interpreting, setInterpreting] = useState(false);
  const [localError, setLocalError] = useState<string>();
  const [raw, setRaw] = useState("");
  const [source, setSource] = useState("sec");
  const [sourceResults, setSourceResults] = useState<DiscoveryResult[]>([]);
  const [selected, setSelected] = useState<Set<number>>(new Set());
  const [searching, setSearching] = useState(false);
  const [clock, setClock] = useState(Date.now());
  const [selectedAccounts, setSelectedAccounts] = useState<Set<string>>(new Set());
  const [suggestionStatus, setSuggestionStatus] = useState<string>();
  const [rescoreFrom, setRescoreFrom] = useState<{ runId: string; candidates: number }>();
  const planRef = useRef<HTMLElement>(null);

  // The plan compiles instantly and offline, and renders below the prompt. On a
  // short window that put it off-screen, so submitting looked like it did
  // nothing at all. Move focus and view to the plan the moment it exists.
  useEffect(() => {
    if (!brief) return;
    planRef.current?.scrollIntoView({ behavior: "smooth", block: "start" });
    planRef.current?.focus({ preventScroll: true });
  }, [brief?.id]);

  // Ask the local model for product-specific signals once per plan. The plan is
  // usable immediately; suggestions merge in when they arrive and never
  // overwrite a rule the user has already edited.
  useEffect(() => {
    if (!brief || brief.interpreter === "browser_preview") return;
    const planId = brief.id;
    let current = true;
    setSuggestionStatus("Asking the local model for signals specific to your product…");
    api.suggestSignalRules(brief.query, brief.segments.map((segment) => segment.label), brief.positiveSignals)
      .then((result) => {
        if (!current) return;
        setBrief((latest) => latest && latest.id === planId ? { ...latest, positiveSignals: mergeSuggestedRules(latest.positiveSignals, result.rules) } : latest);
        setSuggestionStatus(result.message);
      })
      .catch((reason) => { if (current) setSuggestionStatus(`Local model suggestions are unavailable: ${String(reason)}`); });
    return () => { current = false; };
  }, [brief?.id]);

  useEffect(() => setSelectedAccounts(new Set()), [activeRun?.id]);

  useEffect(() => {
    if (activeRun?.status !== "running") return;
    setClock(Date.now());
    const timer = window.setInterval(() => setClock(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [activeRun?.id, activeRun?.status]);

  const interpret = async (event: FormEvent) => {
    event.preventDefault();
    if (activeRun?.status === "running") {
      setLocalError("Another automatic search is still running. Cancel it or wait for it to finish before starting a new one.");
      return;
    }
    setInterpreting(true);
    setLocalError(undefined);
    try {
      setBrief(await onInterpret(query, geography, resultLimit, productWebsite || undefined));
      onClearRun();
    } catch (reason) {
      setLocalError(String(reason));
    } finally {
      setInterpreting(false);
    }
  };

  const discoverSource = async (event: FormEvent) => {
    event.preventDefault();
    setSearching(true);
    setLocalError(undefined);
    try {
      const next = await onDiscover(source, query);
      setSourceResults(next);
      setSelected(new Set(next.map((_, index) => index)));
    } catch (reason) {
      setLocalError(String(reason));
    } finally {
      setSearching(false);
    }
  };

  // Re-open the finished run's plan for editing, keeping its candidates.
  const editAndRescore = async (finished: SearchRun) => {
    try {
      const previous = await api.getSearchBrief(finished.briefId);
      setRescoreFrom({ runId: finished.id, candidates: finished.accounts.length });
      setBrief({ ...previous, id: crypto.randomUUID(), approved: false });
      setSelectedAccounts(new Set());
      onClearRun();
    } catch (reason) {
      setLocalError(String(reason));
    }
  };

  const run = activeRun;
  const progress = searchProgress(run);
  const elapsedSeconds = run
    ? Math.max(0, Math.floor(((run.completedAt ? new Date(run.completedAt).getTime() : clock) - new Date(run.startedAt).getTime()) / 1_000))
    : 0;
  const elapsedLabel = elapsedSeconds < 60
    ? `${elapsedSeconds}s`
    : `${Math.floor(elapsedSeconds / 60)}m ${elapsedSeconds % 60}s`;
  const runStatus = !run
    ? ""
    : run.status === "running"
        ? run.progressStage
        : run.status === "complete"
          ? `Finished in ${elapsedLabel}`
          : run.status === "cancelled"
            ? `Cancelled after ${elapsedLabel}`
            : `Search stopped after ${elapsedLabel}`;
  const guidedQueries = brief ? guidedSearchQueries(brief) : [];
  const automatic = canRunAutomatically(brief);
  const strongMatches = run?.accounts.filter((item) => item.resultTier === "strong") ?? [];
  const nearMatches = run?.accounts.filter((item) => item.resultTier === "near_match") ?? [];
  const excluded = run?.accounts.filter((item) => item.resultTier === "excluded") ?? [];
  const selectableAccounts = run?.status === "running" ? run.accounts : [...strongMatches, ...nearMatches];
  const candidateSection = (title: string, items: SearchRun["accounts"], tone: "success" | "warning" | "neutral" = "neutral") => items.length ? <section className="candidate-section"><div className="candidate-section-heading"><h3>{title}</h3><Badge tone={tone}>{items.length}</Badge></div><div className="candidate-grid">{items.map((item) => <article key={item.account.id} className="candidate-card">
    <div className="candidate-card-top"><button type="button" className={selectedAccounts.has(item.account.id) ? "avatar selector selected" : "avatar selector"} aria-pressed={selectedAccounts.has(item.account.id)} aria-label={`Select ${item.account.name}`} onClick={() => setSelectedAccounts((current) => { const next = new Set(current); next.has(item.account.id) ? next.delete(item.account.id) : next.add(item.account.id); return next; })}>{selectedAccounts.has(item.account.id) ? "✓" : item.account.name.slice(0, 2).toUpperCase()}</button><div className="badge-row"><Badge tone={item.resultTier === "strong" ? "success" : item.resultTier === "excluded" ? "danger" : "warning"}>{item.resultTier.replaceAll("_", " ")}</Badge><Badge>{item.discoverySource}</Badge></div></div>
    <button type="button" className="candidate-body" onClick={() => onOpenAccount(item.account.id, run!.id)}><h3 title={item.account.name}>{item.account.name}</h3><p className="clamp-3" title={item.discoveryReason}>{item.discoveryReason}</p>{ruleChips(item)}{item.rejectionReasons[0] && <p className="candidate-caveat clamp-2" title={item.rejectionReasons[0]}>{item.rejectionReasons[0]}</p>}</button>
    {safePublicWebsite(item.account.websiteUrl) && <button type="button" className="website-link" onClick={() => openExternal(safePublicWebsite(item.account.websiteUrl)!, setLocalError)}>Website ↗</button>}
    <div className="candidate-footer"><span><b>{item.score ?? "—"}</b> fit</span><span>Evidence: {item.evidenceConfidence}</span></div>
  </article>)}</div></section> : null;

  return <div className="discover-workflow">
    <section className="prompt-hero panel">
      <span className="eyebrow">Local AI targeting compiler</span>
      <h2>What do you sell, and what business problem does it solve?</h2>
      <p>The search plan is compiled instantly. The workbench exposes its assumptions before it accesses any discovery source.</p>
      <form onSubmit={interpret} className="prompt-form">
        <textarea value={query} onChange={(event) => setQuery(event.target.value)} required minLength={12} placeholder="I sell a voice intake system to businesses that depend on inbound calls. Find companies showing evidence of after-hours, appointment, consultation, intake, or dispatch workflows." onKeyDown={(event) => { if (event.key === "Enter" && !event.shiftKey) { event.preventDefault(); event.currentTarget.form?.requestSubmit(); } }} />
        <div className="prompt-controls">
          <input aria-label="Optional product website" value={productWebsite} onChange={(event) => setProductWebsite(event.target.value)} placeholder="Optional product website" />
          <input aria-label="Search geography" value={geography} onChange={(event) => setGeography(event.target.value)} placeholder="United States" />
          <select aria-label="Result limit" value={resultLimit} onChange={(event) => setResultLimit(Number(event.target.value))}><option value={10}>Up to 10 strong matches</option><option value={25}>Up to 25 strong matches</option><option value={50}>Up to 50 strong matches</option></select>
          <button className="button primary" disabled={interpreting}>{interpreting ? "Building plan…" : "Build search plan"}</button>
        </div>
      </form>
      {localError && <div className="alert error"><strong>Could not continue</strong><span>{localError}</span></div>}
    </section>

    {brief && !run && <section className="search-receipt panel" ref={planRef} tabIndex={-1} aria-label="Search plan">
      <div className="section-heading"><div><span className="step">Step 2 of 2</span><h2>Review this plan, then start</h2></div><Badge tone="warning">Nothing has run yet</Badge></div>
      <p className="receipt-summary">Looking for <strong>{brief.segments.map((item) => item.label).join(", ") || "organizations matching your description"}</strong> in <strong>{brief.geography}</strong>, and keeping up to <strong>{brief.resultLimit}</strong> that public evidence actually supports.</p>
      <div className="receipt-actions primary-first">
        {automatic
          ? <button className="button primary" disabled={busy === "search-run"} onClick={() => void onStart({ ...brief, approved: true }, rescoreFrom?.runId)}>{busy === "search-run" ? "Starting…" : rescoreFrom ? `Re-score ${rescoreFrom.candidates} candidates` : `Start search for up to ${brief.resultLimit} matches`}</button>
          : <strong className="blocked-reason">This plan cannot run automatically. See below.</strong>}
        <button className="button ghost" onClick={() => { setBrief(undefined); setRescoreFrom(undefined); }}>Edit the description</button>
      </div>
      {rescoreFrom
        ? <p className="alert">Re-scoring the {rescoreFrom.candidates} candidates from your earlier search with these signals. Pages read in the last 24 hours are reused, so nothing is discovered or re-downloaded.</p>
        : <p className="muted">Sources: {coverageLabel(brief)}. Searching visits public websites; reviewing this plan does not.</p>}
      <div className="receipt-grid">
        <div><span className="eyebrow">Outcome</span><p>{brief.customerOutcome}</p></div>
        <div><span className="eyebrow">Personas</span><div className="chip-row">{brief.targetPersonas.map((item) => <span className="chip" key={item}>{item}</span>)}</div></div>
        <div><span className="eyebrow">Exclusions</span><div className="chip-row">{brief.exclusions.length ? brief.exclusions.map((item) => <span className="chip muted-chip" key={item}>{item}</span>) : <span className="muted">None proposed</span>}</div></div>
      </div>
      <SignalEditor rules={brief.positiveSignals} status={suggestionStatus} onChange={(positiveSignals) => setBrief({ ...brief, positiveSignals })} />
      <div className="lane-list">{brief.discoveryLanes.map((lane) => <div key={lane.id}><Badge tone={lane.mode === "automatic" ? "success" : "neutral"}>{lane.mode}</Badge><span><strong>{lane.label}</strong><small>{lane.reason} {lane.limitation}</small></span></div>)}</div>
      {brief.assumptions.length > 0 && <div className="assumption-box"><strong>Assumptions to review</strong><ul>{brief.assumptions.map((item) => <li key={item}>{item}</li>)}</ul></div>}
      <details className="advanced-brief"><summary>Edit boundaries</summary><div className="form-grid"><label>Geography<input value={brief.geography} onChange={(event) => setBrief({ ...brief, geography: event.target.value })} /></label><label>Qualification threshold<input type="number" min={1} max={100} value={brief.qualificationThreshold} onChange={(event) => setBrief({ ...brief, qualificationThreshold: Number(event.target.value) })} /></label><label>Maximum primary score span<input type="number" min={0} max={20} value={brief.qualityBand} onChange={(event) => setBrief({ ...brief, qualityBand: Number(event.target.value) })} /></label><label className="span-2">Exclusions<textarea value={brief.exclusions.join("\n")} onChange={(event) => setBrief({ ...brief, exclusions: splitList(event.target.value) })} /></label></div></details>
      {!automatic && <div className="coverage-callout"><strong>No curated directory covers this audience</strong><p>Automatic discovery samples OpenStreetMap, which maps physical businesses, trades, professional and healthcare practices, automotive, hospitality, retail and consumer services. Nothing in it matches this description, and the workbench will not guess at a source query rather than say so.</p><p>Two routes still work, and both feed the same evidence, scoring, and drafting: open a guided search below and paste the company websites you pick, or import a list directly. Results you add this way are researched and qualified exactly like automatic ones. No search-result page is ever scraped.</p><textarea value={raw} onChange={(event) => setRaw(event.target.value)} placeholder={'https://company-one.example\nhttps://company-two.example'} /><button className="button secondary" disabled={busy === "import"} onClick={() => onImport(raw)}>Import selected websites</button><small>For automated general-web discovery, configure your own free local SearXNG instance in Setup.</small></div>}
      {guidedQueries.length > 0 && <div className="query-list">{guidedQueries.map((item) => <button type="button" key={item} onClick={() => openExternal(`https://duckduckgo.com/?q=${encodeURIComponent(item)}`, setLocalError)}><span>{item}</span><b>Open ↗</b></button>)}</div>}
    </section>}

    {run && <section className="run-panel panel">
      <div className="section-heading"><div><span className="step">{run.status}</span><h2>Quality-first results</h2><p>{run.query}</p></div><div className="run-metric"><strong>{progress}%</strong><span>{run.researchedCount}/{run.candidateCount || run.progressTotal} researched</span></div></div>
      <div className="progress-track" role="progressbar" aria-label={runStatus} aria-valuemin={0} aria-valuemax={100} aria-valuenow={progress}><span style={{ width: `${progress}%` }} /></div>
      <div className="run-status-line" role="status"><span className={run.status === "running" ? "status-dot" : "status-dot stopped"} /><strong>{runStatus}</strong><span>Elapsed {elapsedLabel}</span></div>
      {run.status === "failed" && run.lastError && <div className="alert error"><strong>Search failed</strong><span>{run.lastError}</span></div>}
      {run.warnings.map((warning, index) => <div className="alert warning" key={`${warning.source}-${index}`}><strong>{warning.source}</strong><span>{warning.message}{warning.usedCachedResults ? ` Cached results${warning.cacheAgeHours !== undefined ? ` (${warning.cacheAgeHours}h old)` : ""} were used.` : ""}</span></div>)}
      {run.status === "running" && <button className="button ghost" disabled={busy === "cancel-run"} onClick={onCancel}>Cancel search</button>}
      {selectableAccounts.length > 0 && <button className="select-all result-select-all" onClick={() => setSelectedAccounts(selectedAccounts.size === selectableAccounts.length ? new Set() : new Set(selectableAccounts.map((item) => item.account.id)))}>{selectedAccounts.size === selectableAccounts.length ? "Clear visible selection" : "Select all visible matches"}</button>}
      {selectedAccounts.size > 0 && <div className="bulk-toolbar"><strong>{selectedAccounts.size} selected</strong><button onClick={() => onLibraryAction([...selectedAccounts], "save")}>Save</button><button onClick={() => onLibraryAction([...selectedAccounts], "favorite")}>Favorite</button><button className="danger-text" onClick={() => onLibraryAction([...selectedAccounts], "trash")}>Move to Trash</button><button onClick={() => setSelectedAccounts(new Set())}>Clear</button></div>}
      {run.status === "running" ? candidateSection("Candidates being evaluated", run.accounts) : <><div className="result-summary"><strong>{run.strongCount} strong match{run.strongCount === 1 ? "" : "es"}</strong><span>Discovered {run.candidateCount} · Researched {run.researchedCount} · Requested up to {run.resultLimit}</span>{run.strongCount < run.resultLimit && <small>{shortfallExplanation(run)}</small>}</div><RuleHealth rates={run.ruleHealth ?? []} />{candidateSection("Strong Matches", strongMatches, "success")}{candidateSection("Near Matches", nearMatches, "warning")}{excluded.length > 0 && <details className="excluded-results"><summary>Excluded or failed diagnostics ({excluded.length})</summary>{candidateSection("Excluded or Failed", excluded)}</details>}</>}
      {!run.accounts.length && run.status !== "running" && <p className="muted">No automatic candidates were returned. Use the guided or seeded routes; the workbench will not fill the list with unsupported matches.</p>}
      {run.status !== "running" && <div className="receipt-actions"><button className="button primary" onClick={() => { setBrief(undefined); setRescoreFrom(undefined); setSelectedAccounts(new Set()); onClearRun(); }}>Start another search</button>{run.briefId && run.accounts.length > 0 && <button className="button secondary" onClick={() => void editAndRescore(run)}>Edit signals and re-score</button>}</div>}
    </section>}

    {!run && recentRuns.length > 0 && <section className="panel"><div className="section-heading"><div><h3>Recent searches</h3></div><button className="button ghost danger-text" onClick={() => requestConfirm({ title: "Clear search history?", body: `This removes all ${recentRuns.length} past searches and their cached results. Saved prospects are kept.`, confirmLabel: "Clear history", onConfirm: onClearHistory })}>Clear history</button></div><div className="recent-runs">{recentRuns.slice(0, 5).map((item) => <button key={item.id} onClick={() => void onOpenRun(item.id)}><span><strong>{item.query}</strong><small>{item.strongCount} strong · {item.qualifiedCount} qualified · {item.reviewCount} review</small></span><Badge>{item.status}</Badge></button>)}</div></section>}

    <details className="alternate-routes panel"><summary>Seeded and source-specific discovery</summary><div className="page-grid discover-grid">
      <section><h3>Import domains or CSV</h3><p>Contact details remain labeled as user-supplied evidence.</p><textarea className="seed-input" value={raw} onChange={(event) => setRaw(event.target.value)} placeholder={'example.com\nacme.test\n\n, or, \nCompany,Website,Person,Job Title,Work Email,Business Phone'} /><button className="button secondary wide" disabled={busy === "import"} onClick={() => onImport(raw)}>Import seeds</button></section>
      <section><h3>Search a specific public source</h3><form className="inline-form" onSubmit={discoverSource}><select value={source} onChange={(event) => setSource(event.target.value)}><option value="sec">SEC EDGAR</option><option value="usaspending">USAspending</option><option value="searxng">Local SearXNG</option></select><input value={query} onChange={(event) => setQuery(event.target.value)} required placeholder="Company or recipient name" /><button className="button secondary" disabled={searching}>{searching ? "Searching…" : "Search"}</button></form>{sourceResults.length > 0 && <div className="result-list">{sourceResults.map((result, index) => <label key={`${result.source}-${result.externalId ?? index}`}><input type="checkbox" checked={selected.has(index)} onChange={() => setSelected((current) => { const next = new Set(current); next.has(index) ? next.delete(index) : next.add(index); return next; })} /><span><strong>{result.companyName}</strong><small>{result.reason}</small></span><Badge>{result.source}</Badge></label>)}<button className="button primary" onClick={() => onAddResults(sourceResults.filter((_, index) => selected.has(index)))}>Add selected</button></div>}</section>
    </div><div className="capability-row">{capabilities.map((capability) => <Badge key={capability.id} tone={capability.enabled ? "success" : "neutral"}>{capability.name}: {capability.enabled ? "ready" : "optional"}</Badge>)}</div></details>
    <p className="attribution">Automatic mapped-business discovery uses © OpenStreetMap contributors, available under ODbL. Coverage varies.</p>
  </div>;
}

function ProspectsView({ accounts, selected, selectedId, busy, draftJob, onError, requestConfirm, onSelect, onLibraryAction, onPermanentDelete, onResearch, onGenerate, onOutcome, onSuppress }: {
  accounts: Account[]; selected?: ProspectPackage; selectedId?: string; busy?: string; draftJob?: ResearchJob; onError: (message: string) => void; requestConfirm: (request: ConfirmRequest) => void;
  onSelect: (id: string) => void; onLibraryAction: (ids: string[], action: LibraryAction) => void; onPermanentDelete: (ids: string[]) => void;
  onResearch: () => void; onGenerate: (kind?: "initial" | "follow_up") => void; onOutcome: (outcome: OutcomeKind) => void; onSuppress: (suppressed: boolean) => void;
}) {
  const [filter, setFilter] = useState<"saved" | "favorites" | "all" | "trash">("saved");
  const [checked, setChecked] = useState<Set<string>>(new Set());
  useEffect(() => {
    if (selected?.account.trashedAt) setFilter("trash");
    else if (selected && !selected.account.savedAt) setFilter("all");
  }, [selected?.account.id, selected?.account.savedAt, selected?.account.trashedAt]);
  const visible = accounts.filter((account) => filter === "trash" ? Boolean(account.trashedAt) : !account.trashedAt && (filter === "all" || filter === "favorites" ? filter === "all" || Boolean(account.favoritedAt) : Boolean(account.savedAt)));
  const assessment = selected?.discoveryContext?.assessment ?? selected?.assessment;
  const generating = Boolean(draftJob && draftJob.accountId === selectedId && ["pending", "running"].includes(draftJob.status));
  if (!accounts.length) return <EmptyState />;
  return <div className="prospect-layout">
    <section className="account-list panel">
      <div className="library-tabs">{(["saved", "favorites", "all", "trash"] as const).map((value) => <button className={filter === value ? "active" : ""} onClick={() => { setFilter(value); setChecked(new Set()); }} key={value}>{value === "all" ? "All discovered" : value}</button>)}</div>
      <div className="list-heading"><button className="select-all" onClick={() => setChecked(checked.size === visible.length ? new Set() : new Set(visible.map((account) => account.id)))}>{checked.size === visible.length && visible.length ? "Clear all" : "Select all visible"}</button><span>{visible.length} accounts</span></div>
      {checked.size > 0 && <div className="bulk-toolbar compact"><strong>{checked.size} selected</strong>{filter === "trash" ? <><button onClick={() => onLibraryAction([...checked], "restore")}>Restore</button><button className="danger-text" onClick={() => requestConfirm({ title: "Delete permanently?", body: `${checked.size} trashed prospect${checked.size === 1 ? "" : "s"} and all related evidence will be removed. This cannot be undone.`, confirmLabel: "Delete permanently", onConfirm: () => onPermanentDelete([...checked]) })}>Delete permanently</button></> : <><button onClick={() => onLibraryAction([...checked], "save")}>Save</button><button onClick={() => onLibraryAction([...checked], "unsave")}>Unsave</button><button onClick={() => onLibraryAction([...checked], "favorite")}>Favorite</button><button onClick={() => onLibraryAction([...checked], "unfavorite")}>Unfavorite</button><button className="danger-text" onClick={() => onLibraryAction([...checked], "trash")}>Trash</button></>}</div>}
      {visible.map((account) => <article key={account.id} className={selectedId === account.id ? "account-row selected" : "account-row"}>
        <button type="button" className={checked.has(account.id) ? "avatar selector selected" : "avatar selector"} aria-pressed={checked.has(account.id)} aria-label={`Select ${account.name}`} onClick={() => setChecked((current) => { const next = new Set(current); next.has(account.id) ? next.delete(account.id) : next.add(account.id); return next; })}>{checked.has(account.id) ? "✓" : account.name.slice(0, 2).toUpperCase()}</button>
        <button type="button" className="account-open" onClick={() => onSelect(account.id)}><span className="account-meta"><strong title={account.name}>{account.name}</strong><small title={account.domain ?? account.websiteUrl}>{account.domain ?? account.websiteUrl ?? "Public-source account"}</small></span><span className="score"><b>{account.score ?? "—"}</b><small>{account.scoreLabel ?? "Not scored"}</small></span></button>
      </article>)}
      {!visible.length && <p className="muted library-empty">No prospects in this view.</p>}
    </section>
    <section className="prospect-detail">{!selected ? <div className="panel skeleton">Select a prospect to review its full record.</div> : <>
      <div className="panel detail-hero"><div><span className="eyebrow">{selected.account.domain ?? "Public record"}</span><h2>{selected.account.name}</h2>{safePublicWebsite(selected.account.websiteUrl) && <button className="website-link prominent" onClick={() => openExternal(safePublicWebsite(selected.account.websiteUrl)!, onError)}>Open website ↗</button>}<div className="badge-row"><Badge tone={selected.suppressed || selected.account.trashedAt ? "danger" : "success"}>{selected.account.trashedAt ? "In trash" : selected.suppressed ? "Do not contact" : selected.discoveryContext?.state.replaceAll("_", " ") ?? selected.account.status.replaceAll("_", " ")}</Badge><Badge>{selected.account.savedAt ? "Saved" : "Unsaved discovery"}</Badge>{selected.account.favoritedAt && <Badge tone="warning">Favorite</Badge>}{selected.account.location && <Badge>{selected.account.location}</Badge>}{selected.account.industry && <Badge>{selected.account.industry}</Badge>}<Badge>{selected.evidence.length} evidence items</Badge></div></div><div className="hero-score"><strong>{assessment?.total ?? selected.account.score ?? "—"}</strong><span>fit score</span></div></div>
      <div className="action-strip"><button className="button secondary" disabled={busy === "research" || Boolean(selected.account.trashedAt)} onClick={onResearch}>{busy === "research" ? "Researching…" : "Refresh public evidence"}</button><button className="button primary" disabled={selected.suppressed || Boolean(selected.account.trashedAt) || generating || busy === "draft-readiness" || !selected.evidence.length} onClick={() => onGenerate("initial")}>{generating ? `${draftJob?.progressPercent}% · ${draftJob?.progressStage}` : "Generate with Ollama"}</button>{selected.draft && <button className="button secondary" disabled={selected.suppressed || Boolean(selected.account.trashedAt) || generating} onClick={() => onGenerate("follow_up")}>Draft requested follow-up</button>}<button className="button ghost danger-text" disabled={Boolean(selected.account.trashedAt)} onClick={() => onSuppress(!selected.suppressed)}>{selected.suppressed ? "Remove suppression" : "Do not contact"}</button></div>
      <div className="detail-grid">
        {selected.discoveryContext && <section className="panel span-2 discovery-context"><div className="section-heading"><div><span className="step">Discovery context</span><h3>{selected.discoveryContext.resultTier.replaceAll("_", " ")}</h3></div><Badge>{selected.discoveryContext.evidenceConfidence} confidence</Badge></div><p><b>Search:</b> {selected.discoveryContext.query}</p><p><b>Source:</b> {selected.discoveryContext.discoverySource}</p><p><b>Why it appeared:</b> {selected.discoveryContext.discoveryReason}</p>{selected.discoveryContext.rejectionReasons.length > 0 && <><b>Caveats</b><ul>{selected.discoveryContext.rejectionReasons.map((reason) => <li key={reason}>{reason}</li>)}</ul></>}<b>All matched features</b>{selected.discoveryContext.matchedFeatures.length ? <ul>{selected.discoveryContext.matchedFeatures.map((feature) => <li key={feature.ruleId}>{feature.label} · {feature.pointsAwarded} points · {feature.confidence} confidence</li>)}</ul> : <p>No supported qualification features were found.</p>}</section>}
        <section className="panel"><h3>Qualification</h3>{assessment ? <><div className="score-bars">{["ICP fit", assessment.icpFit, "Signal", assessment.signalRelevance, "Persona", assessment.personaFit, "Evidence", assessment.evidenceContactability].reduce<React.ReactNode[]>((nodes, value, index, array) => { if (index % 2) return nodes; const amount = Number(array[index + 1]); nodes.push(<div key={String(value)}><span>{value}<b>{amount}</b></span><i><em style={{ width: `${amount}%` }} /></i></div>); return nodes; }, [])}</div><div className="gate-list">{assessment.hardGates.map((gate) => <div key={gate.label}><Badge tone={gate.status === "pass" ? "success" : gate.status === "fail" ? "danger" : "warning"}>{gate.status}</Badge><span><strong>{gate.label}</strong><small>{gate.reason}</small></span></div>)}</div><ul className="reason-list">{assessment.reasons.map((reason) => <li key={reason.text}>{reason.text}</li>)}</ul></> : <p className="muted">No run-specific assessment is available yet.</p>}</section>
        <section className="panel"><h3>People and business contacts</h3>{selected.people.map((person) => <div className="person-row" key={person.id}><span className="avatar">{person.name.slice(0, 2).toUpperCase()}</span><span><strong>{person.name}</strong><small>{person.title ?? "Role not publicly found"}</small></span></div>)}{selected.contacts.length ? selected.contacts.map((contact) => <div className="contact-row" key={contact.id}><span>{contact.kind.replaceAll("_", " ")}</span><strong>{contact.value}</strong><Badge>{contact.confidence}</Badge></div>) : <div className="missing">Not publicly found</div>}</section>
        <section className="panel span-2"><h3>Reason-now signals</h3>{selected.signals.length ? <div className="signal-grid">{selected.signals.map((signal) => <div key={signal.id}><Badge>{signal.category}</Badge><p>{signal.summary}</p></div>)}</div> : <p className="muted">No timely public signal found. Do not invent urgency.</p>}</section>
        <section className="panel span-2"><h3>Email draft</h3>{selected.draft ? <div className="draft"><div className="subject"><span>Subject</span><strong>{selected.draft.subject}</strong><Badge tone={selected.draft.verified ? "success" : "danger"}>{selected.draft.verified ? "Verified" : "Blocked"}</Badge></div><pre>{selected.draft.body}</pre></div> : <p className="muted">No draft yet. Research the account, then generate with the local model.</p>}</section>
        <section className="panel"><h3>Call brief</h3>{selected.callBrief ? <div className="call-brief"><p><b>Opener</b>{selected.callBrief.opener}</p><p><b>Reason now</b>{selected.callBrief.reasonNow}</p><b>Labeled hypotheses</b><ol>{selected.callBrief.hypotheses.map((hypothesis) => <li key={hypothesis}>{hypothesis}</li>)}</ol><b>Discovery questions</b><ol>{selected.callBrief.discoveryQuestions.map((question) => <li key={question}>{question}</li>)}</ol><b>Relevant proof</b><ul>{selected.callBrief.relevantProof.map((proof) => <li key={proof}>{proof}</li>)}</ul><b>Likely objections</b><ul>{selected.callBrief.likelyObjections.map((objection) => <li key={objection}>{objection}</li>)}</ul><p><b>Desired outcome</b>{selected.callBrief.desiredOutcome}</p></div> : <p className="muted">No call brief yet.</p>}</section>
        <section className="panel"><h3>Manual outcome</h3><div className="outcome-grid">{(["contacted", "replied", "meeting", "not_interested", "wrong_fit", "do_not_contact"] as OutcomeKind[]).map((outcome) => <button className={selected.latestOutcome === outcome ? "selected" : ""} onClick={() => onOutcome(outcome)} key={outcome}>{outcome.replaceAll("_", " ")}</button>)}</div></section>
        <section className="panel span-2"><h3>Evidence ledger</h3>{selected.evidence.length ? <div className="evidence-list">{selected.evidence.map((item, index) => <details key={item.id} open={index === 0}><summary><span><Badge>{item.sourceKind}</Badge><strong>{item.title}</strong>{item.qualityFlags.map((flag) => <Badge key={flag} tone={flag === "contradictory" ? "danger" : "warning"}>{flag}</Badge>)}</span><small>{formatDate(item.retrievedAt)} · {item.freshness}</small></summary><p>{item.excerpt}</p><button className="link-button" onClick={() => { const target = safePublicWebsite(item.sourceUrl); if (target) openExternal(target, onError); else onError(`This evidence has no public web address: ${item.sourceUrl}`); }}>Open source ↗</button></details>)}</div> : <p className="muted">No evidence collected.</p>}</section>
      </div>
    </>}</section>
  </div>;
}

function DraftSetupDialog({ readiness, busy, onCancel, onSubmit }: { readiness: DraftReadiness; busy: boolean; onCancel: () => void; onSubmit: (play: SalesPlay, overrideReview: boolean) => void }) {
  const [play, setPlay] = useState(readiness.suggestedPlay);
  const [overrideReview, setOverrideReview] = useState(false);
  const update = <K extends keyof SalesPlay>(key: K, value: SalesPlay[K]) => setPlay((current) => ({ ...current, [key]: value }));
  return <div className="modal-backdrop" role="presentation"><form className="modal panel" role="dialog" aria-modal="true" aria-labelledby="draft-setup-title" onSubmit={(event) => { event.preventDefault(); onSubmit(play, overrideReview); }}><div className="section-heading"><div><span className="step">Local draft setup</span><h2 id="draft-setup-title">Complete once, then generate</h2></div><button type="button" className="icon-button" aria-label="Close" onClick={onCancel}>×</button></div><p className="muted">These fields are stored only on this computer. Saving them approves the active sales play and resumes the Ollama request automatically.</p><div className="form-grid"><label className="span-2">Offer<textarea required value={play.offer} onChange={(event) => update("offer", event.target.value)} /></label><label>Sender name<input required value={play.senderName} onChange={(event) => update("senderName", event.target.value)} /></label><label>Sender company<input required value={play.senderCompany} onChange={(event) => update("senderCompany", event.target.value)} /></label><label className="span-2">Postal address<textarea required value={play.postalAddress} onChange={(event) => update("postalAddress", event.target.value)} /></label><label className="span-2">Opt-out wording<textarea required value={play.optOutText} onChange={(event) => update("optOutText", event.target.value)} /></label></div>{readiness.requiresReviewOverride && <label className="override-check"><input type="checkbox" checked={overrideReview} onChange={(event) => setOverrideReview(event.target.checked)} required /><span>I reviewed the evidence and want a draft even though this prospect is not currently a Strong Match.</span></label>}<div className="receipt-actions"><button type="button" className="button ghost" onClick={onCancel}>Cancel</button><button className="button primary" disabled={busy || readiness.requiresReviewOverride && !overrideReview}>{busy ? "Saving…" : "Save & Generate"}</button></div></form></div>;
}

function SettingsView({ capabilities, onExport, onDiagnostics, onImport, onClearProspects, onDelete, requestConfirm }: { capabilities: DashboardSnapshot["sourceCapabilities"]; onExport: () => void; onDiagnostics: () => void; onImport: (payload: string) => void; onClearProspects: () => void; onDelete: () => void; requestConfirm: (request: ConfirmRequest) => void }) {
  const [health, setHealth] = useState<OllamaHealth>();
  const [searxngUrl, setSearxngUrl] = useState("");
  const [sourceNotice, setSourceNotice] = useState("");
  useEffect(() => { void api.ollamaHealth().then(setHealth).catch(() => setHealth({ reachable: false, modelInstalled: false, modelTag: "qwen3.5:9b", message: "Ollama is not reachable." })); }, []);
  useEffect(() => { void api.getSearxngUrl().then(setSearxngUrl); }, []);
  return <div className="page-grid"><section className="panel"><h2>Local model</h2><div className="health-card"><span className={health?.modelInstalled ? "health-icon ok" : "health-icon"}>◎</span><div><strong>{health?.modelTag ?? "Checking…"}</strong><p>{health?.message}</p></div></div><p className="muted">Requires 16 GB RAM and about 12 GB of free disk. The workbench never falls back to hosted AI.</p></section><section className="panel"><h2>Data controls</h2><p>All prospecting data lives in the local application directory. Raw HTML is discarded after evidence extraction.</p><div className="stack"><button className="button secondary" onClick={onExport}>Export local data</button><button className="button secondary" onClick={onDiagnostics}>Export privacy-safe diagnostics</button><label className="button ghost file-button">Restore local export<input type="file" accept="application/json,.json" onChange={(event) => { const file = event.target.files?.[0]; if (!file) return; if (file.size > 25 * 1024 * 1024) { setSourceNotice("Import exceeds the 25 MB limit."); return; } requestConfirm({ title: "Replace all local data?", body: `Importing ${file.name} replaces every prospect, evidence row and search in this app. This cannot be undone.`, confirmLabel: "Replace everything", onConfirm: () => void file.text().then(onImport) }); event.target.value = ""; }} /></label><button className="button ghost danger-text" onClick={() => requestConfirm({ title: "Clear all prospects?", body: "Every prospect, its evidence, contacts and drafts will be removed, along with all search history. Your offer profile, sender identity and settings are kept.", confirmLabel: "Clear prospects", onConfirm: onClearProspects })}>Clear all prospects</button><button className="button ghost danger-text" onClick={() => requestConfirm({ title: "Delete everything?", body: "All prospects, evidence, searches, drafts, your offer profile and every setting will be erased. Export first if you want a copy. This cannot be undone.", confirmLabel: "Delete all data", onConfirm: onDelete })}>Delete all data</button></div></section><section className="panel span-2"><h2>Optional local search</h2><p>SearXNG is never required. Add a base URL only if you operate your own instance; clearing it disables the adapter.</p><div className="inline-form"><input aria-label="SearXNG base URL" value={searxngUrl} onChange={(event) => setSearxngUrl(event.target.value)} placeholder="http://127.0.0.1:8080" /><button className="button secondary" onClick={() => void api.setSearxngUrl(searxngUrl).then((saved) => { setSearxngUrl(saved); setSourceNotice(saved ? "SearXNG configured." : "SearXNG disabled."); })}>Save source</button></div>{sourceNotice && <p className="muted">{sourceNotice}</p>}</section><section className="panel span-2"><h2>Source capabilities</h2><div className="source-grid">{capabilities.map((item) => <div key={item.id}><Badge tone={item.enabled ? "success" : "neutral"}>{item.enabled ? "Ready" : "Optional"}</Badge><strong>{item.name}</strong><p>{item.detail}</p></div>)}</div></section></div>;
}

function EmptyState() {
  return <section className="empty panel"><div className="empty-orbit"><span>⌕</span></div><span className="eyebrow">No prospect data</span><h2>Start with companies you already understand.</h2><p>Import domains or a CSV. The workbench will collect public evidence before it attempts to rank or personalize anything.</p></section>;
}

export default App;
