mod adapters;
mod ai;
mod catalog;
mod db;
mod error;
#[cfg(test)]
mod eval_fixtures;
mod matcher;
mod models;
mod qualification;
mod regions;
mod research;
mod safety;
mod signals;
mod wikidata;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use reqwest::redirect::Policy;
use reqwest::Client;
use rusqlite::{params, Connection};
use tauri::{AppHandle, Emitter, Manager, State};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::*;

#[derive(Clone)]
struct AppState {
    database: Arc<Mutex<Connection>>,
    client: Client,
    app_data_dir: PathBuf,
    /// Parsed robots files by host, kept in memory for a day. Robots files are
    /// tiny and per-host, so a database round trip would cost more than it saves.
    robots: Arc<Mutex<HashMap<String, (Instant, String)>>>,
}

const ROBOTS_CACHE_TTL: Duration = Duration::from_secs(24 * 60 * 60);

impl research::PageCache for AppState {
    fn robots(&self, host: &str) -> Option<String> {
        let cache = self.robots.lock().ok()?;
        cache
            .get(host)
            .filter(|(stored, _)| stored.elapsed() < ROBOTS_CACHE_TTL)
            .map(|(_, text)| text.clone())
    }

    fn store_robots(&self, host: &str, text: &str) {
        if let Ok(mut cache) = self.robots.lock() {
            cache.insert(host.to_string(), (Instant::now(), text.to_string()));
        }
    }

    fn page(&self, account_id: &str, url: &url::Url) -> Option<safety::FetchResult> {
        db::cached_page(&*self.connection().ok()?, account_id, url.as_str())
            .ok()
            .flatten()
    }

    fn store_page(&self, account_id: &str, url: &url::Url, page: &safety::FetchResult) {
        if let Ok(connection) = self.connection() {
            let _ = db::store_page(&connection, account_id, url.as_str(), page);
        }
    }
}

/// Where search-run progress goes. The desktop app emits it to the window;
/// headless evaluation records it. Searching never needs the window itself.
type ProgressSink = Arc<dyn Fn(&SearchRunProgress) + Send + Sync>;

fn window_progress_sink(app: &AppHandle) -> ProgressSink {
    let app = app.clone();
    Arc::new(move |progress| {
        let _ = app.emit("search-run-progress", progress);
    })
}

impl AppState {
    fn connection(&self) -> AppResult<std::sync::MutexGuard<'_, Connection>> {
        self.database
            .lock()
            .map_err(|_| AppError::Message("The local database lock was poisoned.".into()))
    }
}

#[tauri::command]
fn bootstrap_app(state: State<'_, AppState>) -> AppResult<DashboardSnapshot> {
    let connection = state.connection()?;
    // Freshness is a pure function of `retrieved_at`, so it is derived in the
    // read query rather than persisted. This used to rewrite the entire
    // evidence table and then recompute quality flags for every account on
    // every bootstrap, and the frontend bootstraps on mount, after each search
    // run, after each draft, after imports and deletes, and hourly. Flags are
    // recomputed on the write path, where evidence actually changes.
    db::recover_interrupted_jobs(&connection)?;
    let pending_jobs = connection.query_row(
        "SELECT count(*) FROM jobs WHERE status='pending'",
        [],
        |row| row.get(0),
    )?;
    Ok(DashboardSnapshot {
        sales_play: db::load_sales_play(&connection)?,
        offer_profile: db::load_offer_profile(&connection)?,
        accounts: db::list_accounts(&connection)?,
        pending_jobs,
        due_for_daily_scan: db::due_for_daily_scan(&connection)?,
        source_capabilities: db::source_capabilities(&connection)?,
        search_runs: db::list_search_runs(&connection)?,
    })
}

#[tauri::command]
fn save_sales_play(state: State<'_, AppState>, sales_play: SalesPlay) -> AppResult<SalesPlay> {
    db::save_sales_play(&*state.connection()?, sales_play)
}

#[tauri::command]
async fn propose_sales_play(
    state: State<'_, AppState>,
    context: String,
    current: SalesPlay,
) -> AppResult<SalesPlay> {
    let context = context.trim();
    if context.len() < 12 {
        return Err(AppError::Message(
            "Add a website URL or a short description of the offer first.".into(),
        ));
    }
    let source_text = if context.starts_with("http://") || context.starts_with("https://") {
        let target = url::Url::parse(context)?;
        if let Ok(robots_url) = target.join("/robots.txt") {
            if let Ok(robots) = safety::fetch_public(&state.client, robots_url).await {
                if !safety::robots_allows(&robots.text, target.path()) {
                    return Err(AppError::Message(
                        "The website excludes this page in robots.txt.".into(),
                    ));
                }
            }
        }
        let page = safety::fetch_public(&state.client, target).await?;
        format!(
            "Source URL: {}\nTitle: {}\nSanitized public text:\n{}",
            page.final_url, page.title, page.text
        )
    } else {
        context.chars().take(5_000).collect()
    };
    ai::propose_sales_play(&state.client, &source_text, current).await
}

#[tauri::command]
async fn interpret_prospecting_query(
    state: State<'_, AppState>,
    query: String,
    geography: String,
    result_limit: i64,
    product_website: Option<String>,
) -> AppResult<SearchBrief> {
    let clean_query = query.trim().chars().take(5_000).collect::<String>();
    let product_reference = product_website
        .filter(|value| !value.trim().is_empty())
        .map(|value| value.trim().chars().take(500).collect::<String>());
    let play = db::load_sales_play(&*state.connection()?)?;
    let mut brief = ai::interpret_search_brief(
        &clean_query,
        product_reference.as_deref(),
        &geography,
        result_limit,
        play.as_ref(),
    )
    .await?;
    if !adapters::get_searxng_url(&*state.connection()?)?.is_empty() {
        brief.discovery_lanes.push(DiscoveryLane {
            id: "local-search".into(),
            label: "User-operated web search".into(),
            mode: "automatic".into(),
            source: "searxng".into(),
            reason: "A local SearXNG instance is configured for broader public-web discovery."
                .into(),
            limitation: "Coverage depends on the engines enabled in the user's local instance."
                .into(),
        });
    }
    Ok(brief)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SignalSuggestions {
    rules: Vec<QualificationRule>,
    message: String,
}

#[tauri::command]
async fn suggest_signal_rules(
    state: State<'_, AppState>,
    query: String,
    segment_labels: Vec<String>,
    existing: Vec<QualificationRule>,
) -> AppResult<SignalSuggestions> {
    let query = query.trim().chars().take(5_000).collect::<String>();
    let segment_labels = segment_labels
        .into_iter()
        .take(8)
        .map(|label| label.chars().take(80).collect())
        .collect::<Vec<String>>();
    match ai::propose_signal_rules(&state.client, &query, &segment_labels, &existing).await {
        Ok((rules, message)) => Ok(SignalSuggestions { rules, message }),
        // A suggestion failure never blocks a search; the plan already has rules.
        Err(error) => Ok(SignalSuggestions {
            rules: Vec::new(),
            message: format!("The local model could not suggest signals: {error}"),
        }),
    }
}

#[tauri::command]
async fn start_search_run(
    app: AppHandle,
    state: State<'_, AppState>,
    mut brief: SearchBrief,
) -> AppResult<SearchRun> {
    if !brief
        .discovery_lanes
        .iter()
        .any(|lane| lane.mode == "automatic")
    {
        return Err(AppError::Message(
            "This brief has no compliant automatic discovery lane. Use its guided searches or import account seeds.".into(),
        ));
    }
    // Rules arrive from the plan editor and the local model; neither is
    // trusted to have bounded them.
    signals::sanitize_rules(&mut brief.positive_signals);
    brief.approved = true;
    let run = db::create_search_run(&*state.connection()?, &brief)?;
    let background = state.inner().clone();
    let run_id = run.id.clone();
    tauri::async_runtime::spawn(async move {
        let sink = window_progress_sink(&app);
        if let Err(error) = execute_search_run(&background, &sink, &run_id).await {
            if let Ok(connection) = background.connection() {
                let _ =
                    db::finish_search_run(&connection, &run_id, "failed", Some(&error.to_string()));
            }
            let _ = publish_search_finished(&background, &sink, &run_id);
        }
    });
    Ok(run)
}

#[tauri::command]
fn list_search_runs(state: State<'_, AppState>) -> AppResult<Vec<SearchRunSummary>> {
    db::list_search_runs(&*state.connection()?)
}

#[tauri::command]
fn get_search_run(state: State<'_, AppState>, run_id: String) -> AppResult<SearchRun> {
    db::get_search_run(&*state.connection()?, &run_id)
}

#[tauri::command]
fn cancel_search_run(state: State<'_, AppState>, run_id: String) -> AppResult<SearchRun> {
    db::cancel_search_run(&*state.connection()?, &run_id)
}

#[tauri::command]
fn import_accounts(state: State<'_, AppState>, seeds: Vec<AccountSeed>) -> AppResult<Vec<Account>> {
    db::import_accounts(&mut *state.connection()?, seeds)
}

#[tauri::command]
fn list_accounts(state: State<'_, AppState>) -> AppResult<Vec<Account>> {
    db::list_accounts(&*state.connection()?)
}

#[tauri::command]
fn get_prospect_package(
    state: State<'_, AppState>,
    account_id: String,
    search_run_id: Option<String>,
) -> AppResult<ProspectPackage> {
    db::get_package_for_run(&*state.connection()?, &account_id, search_run_id.as_deref())
}

#[tauri::command]
fn apply_library_action(
    state: State<'_, AppState>,
    account_ids: Vec<String>,
    action: String,
) -> AppResult<Vec<Account>> {
    db::apply_library_action(&mut *state.connection()?, &account_ids, &action)
}

#[tauri::command]
fn permanently_delete_accounts(
    state: State<'_, AppState>,
    account_ids: Vec<String>,
) -> AppResult<Vec<Account>> {
    db::permanently_delete_accounts(&mut *state.connection()?, &account_ids)
}

#[tauri::command]
async fn research_account(
    state: State<'_, AppState>,
    account_id: String,
) -> AppResult<ProspectPackage> {
    let app = state.inner().clone();
    let package = db::get_package(&*app.connection()?, &account_id)?;
    if package.account.trashed_at.is_some() {
        return Err(AppError::Message(
            "Restore this prospect from Trash before researching it.".into(),
        ));
    }
    let job_id = create_job(&app, Some(&account_id), "crawl")?;
    match execute_research(&app, &account_id, &job_id).await {
        Ok(()) => db::get_package(&*app.connection()?, &account_id),
        Err(error) => {
            fail_job(&app, &job_id, &error.to_string())?;
            Err(error)
        }
    }
}

fn default_sales_play(package: &ProspectPackage) -> SalesPlay {
    SalesPlay {
        id: "active".into(),
        name: "Prospect outreach".into(),
        offer: package
            .discovery_context
            .as_ref()
            .map(|context| context.query.clone())
            .unwrap_or_default(),
        icp_industries: Vec::new(),
        company_sizes: Vec::new(),
        target_personas: Vec::new(),
        exclusions: Vec::new(),
        proof_points: Vec::new(),
        tone: "Concise, specific, and respectful".into(),
        cta: "Ask for a short exploratory conversation".into(),
        sender_name: String::new(),
        sender_company: String::new(),
        postal_address: String::new(),
        opt_out_text: "If this is not relevant, reply and I will not contact you again.".into(),
        approved: false,
        updated_at: None,
    }
}

async fn draft_readiness_for(state: &AppState, account_id: &str) -> AppResult<DraftReadiness> {
    let (package, mut suggested_play, latest_state) = {
        let connection = state.connection()?;
        let package = db::get_package(&connection, account_id)?;
        let mut play =
            db::load_sales_play(&connection)?.unwrap_or_else(|| default_sales_play(&package));
        if let Some(context) = &package.discovery_context {
            if let Ok(run) = db::get_search_run(&connection, &context.search_run_id) {
                if let Ok(brief) = db::get_search_brief(&connection, &run.brief_id) {
                    if play.offer.trim().is_empty() {
                        play.offer = brief.product_summary;
                    }
                    if play.target_personas.is_empty() {
                        play.target_personas = brief.target_personas;
                    }
                    if play.exclusions.is_empty() {
                        play.exclusions = brief.exclusions;
                    }
                }
            }
        }
        let latest_state = db::latest_search_state(&connection, account_id)?
            .unwrap_or_else(|| package.account.status.clone());
        (package, play, latest_state)
    };
    if suggested_play.offer.trim().is_empty() {
        if let Some(context) = &package.discovery_context {
            suggested_play.offer = context.query.clone();
        }
    }
    let mut missing_fields = Vec::new();
    for (name, value) in [
        ("offer", suggested_play.offer.as_str()),
        ("senderName", suggested_play.sender_name.as_str()),
        ("senderCompany", suggested_play.sender_company.as_str()),
        ("postalAddress", suggested_play.postal_address.as_str()),
        ("optOutText", suggested_play.opt_out_text.as_str()),
    ] {
        if value.trim().is_empty() {
            missing_fields.push(name.into());
        }
    }
    let mut blockers = Vec::new();
    if package.account.trashed_at.is_some() {
        blockers.push("Restore this prospect from Trash before drafting.".into());
    }
    if package.suppressed {
        blockers.push("This prospect is on the local do-not-contact list.".into());
    }
    if package.evidence.is_empty() {
        blockers.push("Research must produce cited evidence before drafting.".into());
    }
    let ollama = ai::ollama_health(&state.client).await;
    if !ollama.reachable || !ollama.model_installed {
        blockers.push(ollama.message.clone());
    }
    let requires_review_override = latest_state != "qualified";
    let requires_play_approval = !suggested_play.approved;
    Ok(DraftReadiness {
        ready: missing_fields.is_empty()
            && blockers.is_empty()
            && !requires_review_override
            && !requires_play_approval,
        missing_fields,
        blockers,
        requires_review_override,
        requires_play_approval,
        suggested_play,
        ollama,
    })
}

#[tauri::command]
async fn get_draft_readiness(
    state: State<'_, AppState>,
    account_id: String,
) -> AppResult<DraftReadiness> {
    draft_readiness_for(state.inner(), &account_id).await
}

#[tauri::command]
async fn start_draft_generation(
    app: AppHandle,
    state: State<'_, AppState>,
    request: DraftGenerationRequest,
) -> AppResult<ResearchJob> {
    let readiness = draft_readiness_for(state.inner(), &request.account_id).await?;
    if !readiness.missing_fields.is_empty() {
        return Err(AppError::Message(
            "DRAFT_SETUP_REQUIRED:Complete the highlighted offer and sender fields.".into(),
        ));
    }
    if !readiness.blockers.is_empty() {
        return Err(AppError::Message(readiness.blockers.join(" ")));
    }
    if readiness.requires_review_override && !request.override_review {
        return Err(AppError::Message("Review this prospect's evidence and explicitly allow drafting for a non-qualified match.".into()));
    }
    let job_id = create_job_with_request(state.inner(), &request)?;
    let background = state.inner().clone();
    let background_app = app.clone();
    let spawned_id = job_id.clone();
    tauri::async_runtime::spawn(async move {
        if let Err(error) = execute_generation_job(&background, &background_app, &spawned_id).await
        {
            let _ = fail_job(&background, &spawned_id, &error.to_string());
            if let Ok(connection) = background.connection() {
                if let Ok(job) = db::get_job(&connection, &spawned_id) {
                    let _ = background_app.emit("draft-job-progress", job);
                }
            }
        }
    });
    db::get_job(&*state.connection()?, &job_id)
}

fn draft_status_allowed(status: &str, override_review: bool) -> bool {
    status == "qualified" || override_review
}

#[tauri::command]
fn set_outcome(
    state: State<'_, AppState>,
    account_id: String,
    outcome: String,
) -> AppResult<ProspectPackage> {
    let connection = state.connection()?;
    db::set_outcome(&connection, &account_id, &outcome)?;
    db::get_package(&connection, &account_id)
}

#[tauri::command]
fn set_suppression(
    state: State<'_, AppState>,
    account_id: String,
    suppressed: bool,
) -> AppResult<ProspectPackage> {
    let connection = state.connection()?;
    db::set_suppression(&connection, &account_id, suppressed)?;
    db::get_package(&connection, &account_id)
}

#[tauri::command]
async fn discover_public(
    state: State<'_, AppState>,
    source: String,
    query: String,
) -> AppResult<Vec<DiscoveryResult>> {
    let searxng = {
        let connection = state.connection()?;
        adapters::get_searxng_url(&connection)?
    };
    adapters::discover(
        &state.client,
        (!searxng.is_empty()).then_some(searxng),
        &source,
        &query,
    )
    .await
}

#[tauri::command]
fn run_daily_scan(state: State<'_, AppState>) -> AppResult<i64> {
    let count = db::queue_daily_scan(&*state.connection()?)?;
    let app = state.inner().clone();
    tauri::async_runtime::spawn(async move {
        let _ = process_pending_jobs(app).await;
    });
    Ok(count)
}

#[tauri::command]
fn list_jobs(state: State<'_, AppState>) -> AppResult<Vec<ResearchJob>> {
    db::list_jobs(&*state.connection()?)
}

#[tauri::command]
async fn ollama_health(state: State<'_, AppState>) -> Result<OllamaHealth, AppError> {
    Ok(ai::ollama_health(&state.client).await)
}

#[tauri::command]
fn get_searxng_url(state: State<'_, AppState>) -> AppResult<String> {
    adapters::get_searxng_url(&*state.connection()?)
}

#[tauri::command]
fn set_searxng_url(state: State<'_, AppState>, url: String) -> AppResult<String> {
    adapters::set_searxng_url(&*state.connection()?, &url)
}

#[tauri::command]
fn export_data(state: State<'_, AppState>) -> AppResult<String> {
    let payload = db::export_json(&*state.connection()?)?;
    let export_dir = state.app_data_dir.join("exports");
    std::fs::create_dir_all(&export_dir)?;
    let filename = format!(
        "prospecting-workbench-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    );
    let path = export_dir.join(filename);
    std::fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
async fn export_diagnostics(state: State<'_, AppState>) -> AppResult<String> {
    let health = ai::ollama_health(&state.client).await;
    let payload = {
        let connection = state.connection()?;
        let account_count: i64 =
            connection.query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))?;
        let evidence_count: i64 =
            connection.query_row("SELECT count(*) FROM evidence", [], |row| row.get(0))?;
        let job_counts = ["pending", "running", "complete", "failed"]
            .into_iter()
            .map(|status| {
                connection
                    .query_row(
                        "SELECT count(*) FROM jobs WHERE status=?1",
                        params![status],
                        |row| row.get::<_, i64>(0),
                    )
                    .map(|count| (status, count))
            })
            .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
        let active_jobs = db::list_jobs(&connection)?.into_iter().filter(|job| matches!(job.status.as_str(), "pending" | "running")).map(|job| serde_json::json!({"kind":job.kind,"status":job.status,"progressPercent":job.progress_percent,"progressStage":job.progress_stage})).collect::<Vec<_>>();
        serde_json::json!({
            "format":"prospecting-workbench-diagnostics","generatedAt":db::now(),"appVersion":env!("CARGO_PKG_VERSION"),
            "platform":std::env::consts::OS,"architecture":std::env::consts::ARCH,"accountCount":account_count,"evidenceCount":evidence_count,
            "jobCounts":job_counts,"activeJobs":active_jobs,"ollama":health,"sources":db::source_capabilities(&connection)?
        })
    };
    let export_dir = state.app_data_dir.join("exports");
    std::fs::create_dir_all(&export_dir)?;
    let path = export_dir.join(format!(
        "prospecting-diagnostics-{}.json",
        chrono::Utc::now().format("%Y%m%d-%H%M%S")
    ));
    std::fs::write(&path, serde_json::to_vec_pretty(&payload)?)?;
    Ok(path.to_string_lossy().into_owned())
}

#[tauri::command]
fn import_data(state: State<'_, AppState>, payload: String) -> AppResult<()> {
    db::restore_json(&mut *state.connection()?, &payload)
}

#[tauri::command]
fn clear_search_history(state: State<'_, AppState>) -> AppResult<()> {
    db::clear_search_history(&*state.connection()?)
}

#[tauri::command]
fn clear_prospects(state: State<'_, AppState>) -> AppResult<()> {
    db::clear_prospects(&*state.connection()?)
}

#[tauri::command]
fn delete_all_data(state: State<'_, AppState>) -> AppResult<()> {
    db::delete_all(&*state.connection()?)?;
    let exports = state.app_data_dir.join("exports");
    if exports.is_dir() {
        std::fs::remove_dir_all(exports)?;
    }
    Ok(())
}

fn create_job(state: &AppState, account_id: Option<&str>, kind: &str) -> AppResult<String> {
    let id = Uuid::new_v4().to_string();
    state.connection()?.execute(
        "INSERT INTO jobs(id,account_id,kind,status,scheduled_at) VALUES(?1,?2,?3,'pending',?4)",
        params![id, account_id, kind, db::now()],
    )?;
    Ok(id)
}

fn create_job_with_request(
    state: &AppState,
    request: &DraftGenerationRequest,
) -> AppResult<String> {
    if !matches!(request.provider.as_str(), "ollama" | "fake")
        || !matches!(request.kind.as_str(), "initial" | "follow_up")
    {
        return Err(AppError::Message("Unknown local draft request.".into()));
    }
    let id = Uuid::new_v4().to_string();
    state.connection()?.execute(
        "INSERT INTO jobs(id,account_id,kind,status,scheduled_at,progress_percent,progress_stage,request_json)
         VALUES(?1,?2,'generate','pending',?3,0,'Queued',?4)",
        params![id, request.account_id, db::now(), serde_json::to_string(request)?],
    )?;
    Ok(id)
}

fn publish_job_progress(
    state: &AppState,
    app: &AppHandle,
    job_id: &str,
    percent: i64,
    stage: &str,
) -> AppResult<()> {
    let job = {
        let connection = state.connection()?;
        connection.execute(
            "UPDATE jobs SET status=CASE WHEN ?2>=100 THEN 'complete' ELSE 'running' END,attempts=CASE WHEN status='pending' THEN attempts+1 ELSE attempts END,progress_percent=?2,progress_stage=?3 WHERE id=?1",
            params![job_id, percent.clamp(0, 100), stage.chars().take(160).collect::<String>()],
        )?;
        db::get_job(&connection, job_id)?
    };
    let _ = app.emit("draft-job-progress", job);
    Ok(())
}

async fn execute_generation_job(state: &AppState, app: &AppHandle, job_id: &str) -> AppResult<()> {
    let job = db::get_job(&*state.connection()?, job_id)?;
    let request: DraftGenerationRequest = job
        .request
        .ok_or_else(|| AppError::Message("Draft job is missing its persisted request.".into()))
        .and_then(|value| serde_json::from_value(value).map_err(AppError::from))?;
    publish_job_progress(state, app, job_id, 10, "Preparing and validating evidence")?;
    let (package, play, latest_state) = {
        let connection = state.connection()?;
        let package = db::get_package(&connection, &request.account_id)?;
        let play = db::require_approved_play(&connection)?;
        let latest_state = db::latest_search_state(&connection, &request.account_id)?
            .unwrap_or_else(|| package.account.status.clone());
        (package, play, latest_state)
    };
    if package.account.trashed_at.is_some() {
        return Err(AppError::Message(
            "Restore this prospect from Trash before drafting.".into(),
        ));
    }
    if !draft_status_allowed(&latest_state, request.override_review) {
        return Err(AppError::Message(
            "This prospect is not qualified; an explicit review override is required.".into(),
        ));
    }
    publish_job_progress(state, app, job_id, 20, "Composing locally")?;
    let (assessment, mut draft, brief) = ai::generate_with_progress(
        &state.client,
        &package,
        &play,
        &request.provider,
        &request.kind,
        |percent, stage| publish_job_progress(state, app, job_id, percent, stage),
    )
    .await?;
    draft.id = job_id.to_string();
    publish_job_progress(state, app, job_id, 95, "Saving verified artifacts")?;
    let evidence_ids = package
        .evidence
        .iter()
        .map(|item| item.id.clone())
        .collect::<Vec<_>>();
    db::complete_generated_job(
        &mut *state.connection()?,
        job_id,
        &request.account_id,
        &assessment,
        &draft,
        &brief,
        &evidence_ids,
    )?;
    let _ = app.emit(
        "draft-job-progress",
        db::get_job(&*state.connection()?, job_id)?,
    );
    Ok(())
}

/// Full research outside a search. It looks for evidence of the rules from the
/// most recent search that found this account, or the phone-intake defaults
/// when no search did.
async fn execute_research(state: &AppState, account_id: &str, job_id: &str) -> AppResult<()> {
    let brief = db::latest_brief_for_account(&*state.connection()?, account_id)?;
    let (focus, personas) = match brief {
        Some(brief) => (brief.positive_signals, brief.target_personas),
        None => (signals::default_focus(), Vec::new()),
    };
    execute_research_with_depth(state, account_id, job_id, false, &focus, &personas).await
}

async fn execute_quick_research(
    state: &AppState,
    account_id: &str,
    job_id: &str,
    brief: &SearchBrief,
) -> AppResult<()> {
    execute_research_with_depth(
        state,
        account_id,
        job_id,
        true,
        &brief.positive_signals,
        &brief.target_personas,
    )
    .await
}

async fn execute_research_with_depth(
    state: &AppState,
    account_id: &str,
    job_id: &str,
    quick: bool,
    focus: &[QualificationRule],
    personas: &[String],
) -> AppResult<()> {
    let account = {
        let connection = state.connection()?;
        connection.execute(
            "UPDATE jobs SET status='running',attempts=attempts+1 WHERE id=?1",
            params![job_id],
        )?;
        db::get_account(&connection, account_id)?
    };
    // A deadline rather than a timeout around the whole crawl: a slow site
    // still yields the pages it served in time, instead of nothing at all.
    let (page_budget, allowance) = if quick {
        (3, Duration::from_secs(6))
    } else {
        (8, Duration::from_secs(60))
    };
    let fetcher = research::PublicFetcher {
        client: &state.client,
    };
    let mut facts = research::Crawl {
        fetcher: &fetcher,
        cache: state,
        focus,
        personas,
        page_budget,
        deadline: Instant::now() + allowance,
    }
    .run(&account)
    .await?;
    if !quick {
        facts.extend(adapters::refresh_public_signals(&state.client, &account).await);
    }
    {
        let mut connection = state.connection()?;
        let transaction = connection.transaction()?;
        for fact in facts {
            let evidence_id = db::upsert_evidence(
                &transaction,
                account_id,
                &fact.source_url,
                &fact.source_kind,
                &fact.title,
                &fact.excerpt,
                &fact.confidence,
            )?;
            db::set_evidence_http_metadata(
                &transaction,
                &evidence_id,
                fact.etag.as_deref(),
                fact.modified_at.as_deref(),
            )?;
            for (kind, value, confidence) in fact.contacts {
                let _ = db::add_contact(
                    &transaction,
                    account_id,
                    &kind,
                    &value,
                    &evidence_id,
                    &confidence,
                );
            }
            for (category, summary, confidence) in fact.signals {
                db::add_signal(
                    &transaction,
                    account_id,
                    &category,
                    &summary,
                    &evidence_id,
                    &confidence,
                )?;
            }
        }
        db::mark_researched(&transaction, account_id)?;
        db::update_quality_flags(&transaction, account_id)?;
        transaction.execute(
            "UPDATE jobs SET status='complete',completed_at=?2,last_error=NULL WHERE id=?1",
            params![job_id, db::now()],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

fn fail_job(state: &AppState, job_id: &str, message: &str) -> AppResult<()> {
    state.connection()?.execute(
        "UPDATE jobs SET status='failed',progress_stage='Failed',last_error=?2,completed_at=?3 WHERE id=?1",
        params![
            job_id,
            message.chars().take(500).collect::<String>(),
            db::now()
        ],
    )?;
    Ok(())
}

async fn process_pending_jobs(state: AppState) -> AppResult<()> {
    loop {
        let jobs = {
            let connection = state.connection()?;
            let mut statement = connection.prepare("SELECT id,account_id FROM jobs WHERE status='pending' AND kind!='generate' AND account_id IS NOT NULL ORDER BY scheduled_at LIMIT 10")?;
            let rows = statement.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            rows.collect::<Result<Vec<_>, _>>()?
        };
        if jobs.is_empty() {
            break;
        }
        for (job_id, account_id) in jobs {
            if let Err(error) = execute_research(&state, &account_id, &job_id).await {
                let _ = fail_job(&state, &job_id, &error.to_string());
            }
        }
    }
    Ok(())
}

async fn process_pending_generation_jobs(state: AppState, app: AppHandle) -> AppResult<()> {
    let ids = {
        let connection = state.connection()?;
        let mut statement = connection.prepare(
            "SELECT id FROM jobs WHERE status='pending' AND kind='generate' ORDER BY scheduled_at",
        )?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    for job_id in ids {
        if let Err(error) = execute_generation_job(&state, &app, &job_id).await {
            let _ = fail_job(&state, &job_id, &error.to_string());
        }
    }
    Ok(())
}

async fn process_search_candidate(
    state: &AppState,
    run_id: &str,
    account_id: &str,
    brief: &SearchBrief,
) -> AppResult<()> {
    if db::search_run_is_cancelled(&*state.connection()?, run_id)? {
        return Ok(());
    }
    let account = db::get_account(&*state.connection()?, account_id)?;
    let research_error = if account.website_url.is_some() || account.domain.is_some() {
        db::update_search_run_account(
            &*state.connection()?,
            run_id,
            account_id,
            db::SearchRunAccountUpdate {
                state: "researching",
                score: None,
                confidence: "medium",
                rejection_reasons: &[],
                features: &[],
                assessment: None,
                rank: None,
                result_tier: "excluded",
            },
        )?;
        let job_id = create_job(state, Some(account_id), "quick_crawl")?;
        execute_quick_research(state, account_id, &job_id, brief)
            .await
            .err()
            .map(|error| {
                let _ = fail_job(state, &job_id, &error.to_string());
                error.to_string()
            })
    } else {
        None
    };
    if db::search_run_is_cancelled(&*state.connection()?, run_id)? {
        return Ok(());
    }
    let package = db::get_package(&*state.connection()?, account_id)?;
    let mut decision = qualification::assess_for_search(&package, brief);
    if let Some(error) = research_error {
        decision.state = "failed".into();
        decision.rejection_reasons.push(format!(
            "Public research failed: {}",
            error.chars().take(240).collect::<String>()
        ));
    } else if account.website_url.is_none() && account.domain.is_none() {
        decision.state = "needs_review".into();
        decision.rejection_reasons.push(
            "The discovery record did not provide a company website; guided identity resolution is required."
                .into(),
        );
    }
    let connection = state.connection()?;
    db::update_search_run_account(
        &connection,
        run_id,
        account_id,
        db::SearchRunAccountUpdate {
            state: &decision.state,
            score: Some(decision.assessment.total),
            confidence: &decision.evidence_confidence,
            rejection_reasons: &decision.rejection_reasons,
            features: &decision.features,
            assessment: Some(&decision.assessment),
            rank: None,
            result_tier: if matches!(decision.state.as_str(), "qualified" | "needs_review") {
                "near_match"
            } else {
                "excluded"
            },
        },
    )?;
    Ok(())
}

/// Emit a progress tick.
///
/// Deliberately carries counters only. This fires once per researched
/// candidate, so reading the full run here made the cost of each tick grow
/// with the pool already researched, quadratic overall, and megabytes of IPC
/// per tick at high candidate targets. The UI pages accounts in separately.
fn publish_search_progress(
    state: &AppState,
    sink: &ProgressSink,
    run_id: &str,
    percent: i64,
    stage: &str,
) -> AppResult<()> {
    let progress = {
        let connection = state.connection()?;
        db::set_search_run_stage(&connection, run_id, percent, stage)?;
        db::get_search_run_progress(&connection, run_id)?
    };
    sink(&progress);
    Ok(())
}

/// Emit the terminal tick for a run that has reached a final state.
fn publish_search_finished(state: &AppState, sink: &ProgressSink, run_id: &str) -> AppResult<()> {
    let progress = db::get_search_run_progress(&*state.connection()?, run_id)?;
    sink(&progress);
    Ok(())
}

/// Record a source backoff when an adapter reports `RATE_LIMIT:<source>:<secs>:`.
///
/// Both the map and reference-data lanes encode throttling in the error string,
/// and both must leave a persistent backoff so the next run does not walk
/// straight back into it.
fn record_rate_limit_backoff(state: &AppState, source: &str, error: &AppError) -> AppResult<()> {
    let message = error.to_string();
    let prefix = format!("RATE_LIMIT:{source}:");
    if let Some(rest) = message.strip_prefix(&prefix) {
        let mut parts = rest.splitn(2, ':');
        let seconds = parts
            .next()
            .and_then(|value| value.parse().ok())
            .unwrap_or(60);
        let detail = parts.next().unwrap_or(&message);
        db::set_source_backoff(&*state.connection()?, source, seconds, detail)?;
    }
    Ok(())
}

/// Resolve the description to organization classes, then collect U.S.
/// organizations of those types.
///
/// The resolved class is recorded as a run warning so the user can see exactly
/// what the search was interpreted as before reviewing any candidate.
async fn resolve_and_discover_reference_data(
    state: &AppState,
    run_id: &str,
    query: &str,
    terms: &str,
    limit: usize,
    rotation: usize,
) -> AppResult<Vec<DiscoveryResult>> {
    // A description names a need, not a directory heading. The local model
    // proposes the kinds of organization that serve it, and those go first
    // because they are far likelier to be real Wikidata classes than raw
    // keywords from the sentence. Keyword pairs remain the offline fallback.
    let mut phrases = ai::propose_audience_types(&state.client, query)
        .await
        .unwrap_or_default();
    let proposed = phrases.clone();
    phrases.extend(wikidata::candidate_phrases(terms));

    let classes = wikidata::resolve_classes(&state.client, &phrases).await?;
    if classes.is_empty() {
        // Say exactly what was tried. "No usable candidates" on its own gives
        // the user nothing to act on.
        db::add_search_warning(
            &*state.connection()?,
            run_id,
            SourceWarning {
                source: "Open reference data".into(),
                message: format!(
                    "Could not match this description to a known type of organization. Tried: {}. Naming the kind of organization you want (for example \"talent agency\" or \"literary agency\") usually resolves it.",
                    phrases.join(", ")
                ),
                used_cached_results: false,
                cache_age_hours: None,
            },
        )?;
        return Ok(Vec::new());
    }
    if !proposed.is_empty() {
        db::add_search_warning(
            &*state.connection()?,
            run_id,
            SourceWarning {
                source: "Open reference data".into(),
                message: format!(
                    "Your description was read as needing: {}. Those were looked up in open reference data.",
                    proposed.join(", ")
                ),
                used_cached_results: false,
                cache_age_hours: None,
            },
        )?;
    }
    let summary = classes
        .iter()
        .map(|class| class.label.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    db::add_search_warning(
        &*state.connection()?,
        run_id,
        SourceWarning {
            source: "Open reference data".into(),
            message: format!(
                "\"{terms}\" was interpreted as: {summary}. Organizations were collected from Wikidata on that basis; review the match before trusting the results."
            ),
            used_cached_results: false,
            cache_age_hours: None,
        },
    )?;
    // Rotation pages deeper into the same class so a repeat search surfaces
    // organizations the previous run did not reach.
    let offset = rotation.saturating_mul(limit.max(1)) % 1_000;
    wikidata::discover_instances(&state.client, &classes, limit, offset).await
}

async fn execute_search_run(state: &AppState, sink: &ProgressSink, run_id: &str) -> AppResult<()> {
    let brief = {
        let connection = state.connection()?;
        let run = db::get_search_run(&connection, run_id)?;
        db::get_search_brief(&connection, &run.brief_id)?
    };
    publish_search_progress(state, sink, run_id, 2, "Preparing enabled sources")?;
    let candidate_budget = (brief.result_limit * 5).max(30).clamp(1, 100) as usize;
    // Shifts which bounded regions this run samples first. Without it, repeat
    // searches re-sample one bounding box and return the same companies.
    let rotation = db::search_rotation(&*state.connection()?, run_id)?;
    let mut results = Vec::new();
    let automatic_lanes = brief
        .discovery_lanes
        .iter()
        .filter(|lane| lane.mode == "automatic")
        .collect::<Vec<_>>();
    let lane_total = automatic_lanes.len().max(1) as i64;
    for (lane_index, lane) in automatic_lanes.into_iter().enumerate() {
        let lane_start = 5 + (lane_index as i64 * 20 / lane_total);
        publish_search_progress(
            state,
            sink,
            run_id,
            lane_start,
            &format!("Searching {}", lane.label),
        )?;
        let lane_outcome: AppResult<Vec<DiscoveryResult>> = match lane.source.as_str() {
            "overpass" => {
                let mut categories = brief
                    .segments
                    .iter()
                    .filter_map(|segment| segment.discovery_category.clone())
                    .collect::<Vec<_>>();
                categories.sort();
                // The rotation is part of the key so a repeat search reads its
                // own sample rather than replaying the previous run's cache.
                let cache_key = format!(
                    "overpass:{}:{}:r{rotation}",
                    brief.geography.trim().to_ascii_lowercase(),
                    categories.join(",")
                );
                let cached = db::cached_discoveries(&*state.connection()?, &cache_key)?;
                let backoff = db::source_backoff(&*state.connection()?, "overpass")?;
                if let Some((cached_results, _age)) = cached.as_ref().filter(|(_, age)| *age < 24) {
                    Ok(cached_results.clone())
                } else if let Some(message) = backoff {
                    if let Some((cached_results, age)) = cached {
                        db::add_search_warning(
                            &*state.connection()?,
                            run_id,
                            SourceWarning {
                                source: "OpenStreetMap Overpass".into(),
                                message,
                                used_cached_results: true,
                                cache_age_hours: Some(age),
                            },
                        )?;
                        Ok(cached_results)
                    } else {
                        Err(AppError::Message(message))
                    }
                } else {
                    match adapters::discover_overpass(
                        &state.client,
                        &categories,
                        &brief.geography,
                        candidate_budget,
                        rotation,
                    )
                    .await
                    {
                        Ok(outcome) => {
                            // Partial throttling: some mirrors limited us but
                            // others answered. Record the backoff anyway so the
                            // next run does not walk straight back into it.
                            if let Some(seconds) = outcome.rate_limited {
                                db::set_source_backoff(
                                    &*state.connection()?,
                                    "overpass",
                                    seconds,
                                    "A public Overpass mirror rate-limited part of this search.",
                                )?;
                            }
                            // A sample too large to parse means real results
                            // were discarded. Say which ones, rather than
                            // letting the highest-yield regions vanish.
                            if !outcome.oversized_samples.is_empty() {
                                db::add_search_warning(
                                    &*state.connection()?,
                                    run_id,
                                    SourceWarning {
                                        source: "OpenStreetMap Overpass".into(),
                                        message: format!(
                                            "{} sample(s) were larger than the parse limit and were skipped: {}. Narrow the geography to cover them.",
                                            outcome.oversized_samples.len(),
                                            outcome.oversized_samples.join("; ")
                                        ),
                                        used_cached_results: false,
                                        cache_age_hours: None,
                                    },
                                )?;
                            }
                            db::cache_discoveries(
                                &*state.connection()?,
                                &cache_key,
                                "overpass",
                                &outcome.results,
                            )?;
                            Ok(outcome.results)
                        }
                        Err(error) => {
                            let message = error.to_string();
                            if let Some(rest) = message.strip_prefix("RATE_LIMIT:overpass:") {
                                let mut parts = rest.splitn(2, ':');
                                let seconds = parts
                                    .next()
                                    .and_then(|value| value.parse().ok())
                                    .unwrap_or(60);
                                let detail = parts.next().unwrap_or(&message);
                                db::set_source_backoff(
                                    &*state.connection()?,
                                    "overpass",
                                    seconds,
                                    detail,
                                )?;
                            }
                            if let Some((cached_results, age)) = cached {
                                db::add_search_warning(
                                    &*state.connection()?,
                                    run_id,
                                    SourceWarning {
                                        source: "OpenStreetMap Overpass".into(),
                                        message,
                                        used_cached_results: true,
                                        cache_age_hours: Some(age),
                                    },
                                )?;
                                Ok(cached_results)
                            } else {
                                Err(error)
                            }
                        }
                    }
                }
            }
            "wikidata" => {
                let terms = ai::reference_terms(&brief.query);
                let cache_key = format!("wikidata:{terms}:r{rotation}");
                let cached = db::cached_discoveries(&*state.connection()?, &cache_key)?;
                let backoff = db::source_backoff(&*state.connection()?, "wikidata")?;
                if let Some((cached_results, _age)) = cached.as_ref().filter(|(_, age)| *age < 24) {
                    Ok(cached_results.clone())
                } else if let Some(message) = backoff {
                    match cached {
                        Some((cached_results, age)) => {
                            db::add_search_warning(
                                &*state.connection()?,
                                run_id,
                                SourceWarning {
                                    source: "Open reference data".into(),
                                    message,
                                    used_cached_results: true,
                                    cache_age_hours: Some(age),
                                },
                            )?;
                            Ok(cached_results)
                        }
                        None => Err(AppError::Message(message)),
                    }
                } else {
                    match resolve_and_discover_reference_data(
                        state,
                        run_id,
                        &brief.query,
                        &terms,
                        candidate_budget,
                        rotation,
                    )
                    .await
                    {
                        Ok(found) => {
                            db::cache_discoveries(
                                &*state.connection()?,
                                &cache_key,
                                "wikidata",
                                &found,
                            )?;
                            Ok(found)
                        }
                        Err(error) => {
                            record_rate_limit_backoff(state, "wikidata", &error)?;
                            Err(error)
                        }
                    }
                }
            }
            source @ ("sec" | "usaspending") => {
                adapters::discover(&state.client, None, source, &brief.query).await
            }
            "searxng" => {
                let configured = adapters::get_searxng_url(&*state.connection()?)?;
                let source_query = format!(
                    "{} {} {}",
                    brief
                        .segments
                        .first()
                        .map(|item| item.label.as_str())
                        .unwrap_or(&brief.product_summary),
                    brief
                        .positive_signals
                        .first()
                        .map(|item| item.label.as_str())
                        .unwrap_or("company"),
                    brief.geography
                );
                adapters::discover(
                    &state.client,
                    (!configured.is_empty()).then_some(configured),
                    "searxng",
                    &source_query,
                )
                .await
            }
            _ => Ok(Vec::new()),
        };
        match lane_outcome {
            Ok(mut lane_results) => results.append(&mut lane_results),
            Err(error) => db::add_search_warning(
                &*state.connection()?,
                run_id,
                SourceWarning {
                    source: lane.label.clone(),
                    message: error.to_string(),
                    used_cached_results: false,
                    cache_age_hours: None,
                },
            )?,
        }
        publish_search_progress(
            state,
            sink,
            run_id,
            5 + ((lane_index as i64 + 1) * 20 / lane_total),
            &format!("Finished {}", lane.label),
        )?;
        if results.len() >= candidate_budget {
            break;
        }
    }
    results.sort_by(|left, right| {
        discovery_priority(right, &brief)
            .cmp(&discovery_priority(left, &brief))
            .then_with(|| left.company_name.cmp(&right.company_name))
    });
    let mut seen = std::collections::HashSet::new();
    results.retain(|result| {
        let key = result.domain.clone().unwrap_or_else(|| {
            format!(
                "{}|{}",
                result.company_name.to_ascii_lowercase(),
                result.source_url
            )
        });
        seen.insert(key)
    });
    results.truncate(candidate_budget);
    publish_search_progress(
        state,
        sink,
        run_id,
        30,
        &format!("Found {} candidate records", results.len()),
    )?;
    if results.is_empty() {
        return Err(AppError::Message(
            "No usable candidates were returned by the enabled free sources. Review the source warning or use guided discovery.".into(),
        ));
    }
    let total = results.len() as i64;
    db::set_search_run_counts(&*state.connection()?, run_id, total, 0)?;
    db::set_search_run_progress(&*state.connection()?, run_id, 0, total)?;
    let mut candidate_ids = Vec::with_capacity(results.len());
    let mut seen_accounts = std::collections::HashSet::new();
    for result in results {
        if db::search_run_is_cancelled(&*state.connection()?, run_id)? {
            return Ok(());
        }
        let account_id = db::import_discovery_result(&mut *state.connection()?, run_id, &result)?;
        db::update_search_run_account(
            &*state.connection()?,
            run_id,
            &account_id,
            db::SearchRunAccountUpdate {
                state: "screening",
                score: None,
                confidence: result.identity_confidence.as_deref().unwrap_or("medium"),
                rejection_reasons: &[],
                features: &[],
                assessment: None,
                rank: None,
                result_tier: "excluded",
            },
        )?;
        if seen_accounts.insert(account_id.clone()) {
            candidate_ids.push(account_id);
        }
    }
    let total = candidate_ids.len() as i64;
    db::set_search_run_progress(&*state.connection()?, run_id, 0, total)?;
    publish_search_progress(
        state,
        sink,
        run_id,
        40,
        if total == 0 {
            "No automatic candidates found"
        } else {
            "Candidates ready; checking public evidence"
        },
    )?;

    // Research a broad pre-ranked pool with a constant number of candidates in
    // flight. The requested count is the maximum number of strong matches,
    // never a pre-research truncation. Fixed batches used to wait for their
    // slowest site before starting the next, idling most workers at each
    // boundary. These are all distinct hosts; per-site politeness comes from
    // the delay inside the crawl, not from this width.
    const PARALLEL_RESEARCH: usize = 12;
    let budget = search_budget(brief.result_limit);
    let started = std::time::Instant::now();
    let mut completed = 0_i64;
    let mut stopped_by_budget = false;
    let finalize_every = brief.result_limit.max(1);
    let mut since_finalize = 0_i64;
    // Once the requested number of strong matches exists, research one more
    // request-sized slice to confirm the band, then stop starting new work.
    let mut confirmation_quota: Option<i64> = None;
    let mut pending = candidate_ids.iter().cloned();
    let mut tasks = tokio::task::JoinSet::new();
    loop {
        while tasks.len() < PARALLEL_RESEARCH && confirmation_quota != Some(0) {
            let Some(account_id) = pending.next() else {
                break;
            };
            if let Some(quota) = confirmation_quota.as_mut() {
                *quota -= 1;
            }
            let background = state.clone();
            let task_run_id = run_id.to_string();
            let task_brief = brief.clone();
            tasks.spawn(async move {
                let outcome =
                    process_search_candidate(&background, &task_run_id, &account_id, &task_brief)
                        .await;
                (account_id, outcome)
            });
        }
        if tasks.is_empty() {
            break;
        }
        if db::search_run_is_cancelled(&*state.connection()?, run_id)? {
            tasks.abort_all();
            return Ok(());
        }
        let Some(left) = budget.checked_sub(started.elapsed()) else {
            stopped_by_budget = true;
            tasks.abort_all();
            break;
        };
        let Ok(joined) = tokio::time::timeout(left, tasks.join_next()).await else {
            stopped_by_budget = true;
            tasks.abort_all();
            break;
        };
        let (account_id, outcome) = joined
            .ok_or_else(|| AppError::Message("Search research queue stopped unexpectedly.".into()))?
            .map_err(|error| {
                AppError::Message(format!(
                    "Search research worker stopped unexpectedly: {error}"
                ))
            })?;
        if let Err(error) = outcome {
            let reasons = vec![format!(
                "Candidate processing failed: {}",
                error.to_string().chars().take(240).collect::<String>()
            )];
            db::update_search_run_account(
                &*state.connection()?,
                run_id,
                &account_id,
                db::SearchRunAccountUpdate {
                    state: "failed",
                    score: None,
                    confidence: "low",
                    rejection_reasons: &reasons,
                    features: &[],
                    assessment: None,
                    rank: None,
                    result_tier: "excluded",
                },
            )?;
        }
        completed += 1;
        db::set_search_run_progress(&*state.connection()?, run_id, completed, total)?;
        db::set_search_run_counts(&*state.connection()?, run_id, total, completed)?;
        publish_search_progress(
            state,
            sink,
            run_id,
            40 + (completed * 55 / total.max(1)),
            &format!("Researched {completed} of {total} discovered candidates"),
        )?;
        since_finalize += 1;
        if since_finalize >= finalize_every {
            since_finalize = 0;
            let (strong, _) = db::finalize_search_ranking(
                &*state.connection()?,
                run_id,
                brief.qualification_threshold,
                brief.quality_band,
                brief.result_limit,
            )?;
            if confirmation_quota.is_none() && strong >= brief.result_limit {
                confirmation_quota = Some(brief.result_limit);
            }
        }
    }
    if db::search_run_is_cancelled(&*state.connection()?, run_id)? {
        return Ok(());
    }
    db::set_search_run_progress(&*state.connection()?, run_id, completed, total)?;
    publish_search_progress(state, sink, run_id, 98, "Finalizing ranked results")?;
    db::finalize_search_ranking(
        &*state.connection()?,
        run_id,
        brief.qualification_threshold,
        brief.quality_band,
        brief.result_limit,
    )?;
    // A run stopped by its time budget left candidates unresearched. Recording
    // that keeps "we ran out of time" distinct from "these did not qualify",
    // without it the UI blamed the score band for an unfinished search.
    let unresearched = (total - completed).max(0);
    if stopped_by_budget && unresearched > 0 {
        db::add_search_warning(
            &*state.connection()?,
            run_id,
            SourceWarning {
                source: "Search time budget".into(),
                message: format!(
                    "The search reached its {}-second budget with {unresearched} of {total} discovered candidates still unresearched. Those candidates were not rejected; they were never checked. Run the search again to continue, or request fewer results.",
                    budget.as_secs()
                ),
                used_cached_results: false,
                cache_age_hours: None,
            },
        )?;
    }
    db::finish_search_run(&*state.connection()?, run_id, "complete", None)?;
    publish_search_finished(state, sink, run_id)?;
    Ok(())
}

/// Rank a discovered record before research.
///
/// This decides which candidates survive truncation to the candidate budget, so
/// relevance to the approved brief leads and metadata completeness breaks ties.
/// Ordering on completeness alone kept the best-documented records rather than
/// the ones most likely to fit.
fn discovery_priority(result: &DiscoveryResult, brief: &SearchBrief) -> i64 {
    let identity = match result.identity_confidence.as_deref() {
        Some("high") => 20,
        Some("medium") => 12,
        _ => 0,
    };
    let completeness = identity
        + if result.website_url.is_some() { 45 } else { 0 }
        + if result.domain.is_some() { 20 } else { 0 }
        + if result.business_phone.is_some() {
            10
        } else {
            0
        }
        + if result.location.is_some() { 5 } else { 0 };
    relevance_score(result, brief) * 4 + completeness
}

/// Whole-word overlap between a discovered record and the approved brief.
fn relevance_score(result: &DiscoveryResult, brief: &SearchBrief) -> i64 {
    let haystack = normalized_tokens(&format!("{} {}", result.company_name, result.reason));
    let padded = format!(" {haystack} ");
    let mut score = 0;
    for segment in &brief.segments {
        if term_hit(&padded, &segment.label) {
            score += 12;
        }
        if let Some(category) = &segment.discovery_category {
            if term_hit(&padded, &category.replace('_', " ")) {
                score += 8;
            }
        }
    }
    for rule in &brief.positive_signals {
        if rule
            .keywords
            .iter()
            .any(|keyword| term_hit(&padded, keyword))
        {
            score += 5;
        }
    }
    score.min(60)
}

fn normalized_tokens(value: &str) -> String {
    value
        .to_ascii_lowercase()
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character
            } else {
                ' '
            }
        })
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// True when any word of `term` (of real length) appears in the padded haystack.
fn term_hit(padded_haystack: &str, term: &str) -> bool {
    normalized_tokens(term)
        .split(' ')
        .filter(|word| word.len() >= 4)
        .any(|word| padded_haystack.contains(&format!(" {word} ")))
}

/// Wall-clock budget for researching a run's candidate pool.
///
/// The old fixed 45 seconds could not research the pool a 25- or 50-result
/// request implies, so large requests silently returned a thin slice.
fn search_budget(result_limit: i64) -> Duration {
    Duration::from_secs((45 + result_limit.max(0) * 3).clamp(45, 240) as u64)
}

fn build_client() -> reqwest::Result<Client> {
    Client::builder()
        .user_agent("ProspectingWorkbenchBot/0.1 (+local desktop research; respects robots.txt)")
        .redirect(Policy::none())
        .connect_timeout(Duration::from_secs(8))
        .timeout(Duration::from_secs(20))
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            let app_data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&app_data_dir)?;
            let connection = db::open(&app_data_dir.join("prospecting.sqlite3"))?;
            let mut resumable_searches = db::running_search_run_ids(&connection)?;
            let resumable_search = resumable_searches.pop();
            for stale_run in resumable_searches {
                db::finish_search_run(
                    &connection,
                    &stale_run,
                    "cancelled",
                    Some("Interrupted because a newer search was active."),
                )?;
            }
            let client = build_client()?;
            let state = AppState {
                database: Arc::new(Mutex::new(connection)),
                client,
                app_data_dir,
                robots: Arc::default(),
            };
            let background = state.clone();
            let search_background = state.clone();
            let generation_background = state.clone();
            let search_app = app.handle().clone();
            let generation_app = app.handle().clone();
            app.manage(state);
            tauri::async_runtime::spawn(async move {
                let _ = process_pending_jobs(background).await;
            });
            tauri::async_runtime::spawn(async move {
                if let Some(run_id) = resumable_search {
                    let sink = window_progress_sink(&search_app);
                    if let Err(error) = execute_search_run(&search_background, &sink, &run_id).await
                    {
                        if let Ok(connection) = search_background.connection() {
                            let _ = db::finish_search_run(
                                &connection,
                                &run_id,
                                "failed",
                                Some(&error.to_string()),
                            );
                        }
                    }
                }
            });
            tauri::async_runtime::spawn(async move {
                let _ =
                    process_pending_generation_jobs(generation_background, generation_app).await;
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            bootstrap_app,
            save_sales_play,
            propose_sales_play,
            interpret_prospecting_query,
            suggest_signal_rules,
            start_search_run,
            list_search_runs,
            get_search_run,
            cancel_search_run,
            import_accounts,
            list_accounts,
            get_prospect_package,
            apply_library_action,
            permanently_delete_accounts,
            research_account,
            get_draft_readiness,
            start_draft_generation,
            set_outcome,
            set_suppression,
            discover_public,
            run_daily_scan,
            list_jobs,
            ollama_health,
            get_searxng_url,
            set_searxng_url,
            export_data,
            export_diagnostics,
            import_data,
            clear_search_history,
            clear_prospects,
            delete_all_data
        ])
        .run(tauri::generate_context!())
        .expect("error while running Prospecting Workbench");
}

#[cfg(test)]
mod command_policy_tests {
    use super::*;

    #[test]
    fn unqualified_drafts_require_an_explicit_override() {
        assert!(draft_status_allowed("qualified", false));
        assert!(!draft_status_allowed("needs_review", false));
        assert!(!draft_status_allowed("researched", false));
        assert!(draft_status_allowed("needs_review", true));
    }

    #[test]
    fn research_budget_scales_with_the_requested_result_count() {
        // A fixed 45 seconds could not research the pool that a 25- or
        // 50-result request implies, so large requests silently returned a
        // thin slice and blamed the score band for it.
        assert_eq!(search_budget(10).as_secs(), 75);
        assert_eq!(search_budget(25).as_secs(), 120);
        assert_eq!(search_budget(50).as_secs(), 195);
        // Bounded at both ends so a hand-edited brief cannot stall a search.
        assert_eq!(search_budget(0).as_secs(), 45);
        assert_eq!(search_budget(-5).as_secs(), 45);
        assert_eq!(search_budget(10_000).as_secs(), 240);
    }

    fn discovery(name: &str, reason: &str, website: bool) -> DiscoveryResult {
        DiscoveryResult {
            source: "OpenStreetMap".into(),
            company_name: name.into(),
            domain: website.then(|| "example.com".into()),
            website_url: website.then(|| "https://example.com".into()),
            external_id: None,
            reason: reason.into(),
            source_url: "https://www.openstreetmap.org/node/1".into(),
            source_external_id: None,
            business_phone: None,
            location: Some("Ohio, United States".into()),
            identity_confidence: Some("medium".into()),
        }
    }

    fn brief_for(segment: &str, label: &str) -> SearchBrief {
        SearchBrief {
            id: "b".into(),
            query: "q".into(),
            product_summary: "p".into(),
            customer_outcome: "o".into(),
            segments: vec![TargetSegment {
                id: segment.into(),
                label: label.into(),
                discovery_category: Some(segment.into()),
            }],
            geography: "Ohio".into(),
            company_sizes: vec![],
            target_personas: vec![],
            positive_signals: vec![QualificationRule {
                id: "after_hours".into(),
                label: "After-hours service".into(),
                component: "signal".into(),
                keywords: vec!["emergency service".into()],
                points: 55,
                hard_gate: false,
                origin: "pack".into(),
            }],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 10,
            qualification_threshold: 70,
            quality_band: 12,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: SEARCH_PROMPT_VERSION.into(),
            created_at: "now".into(),
        }
    }

    #[test]
    fn pre_ranking_prefers_relevance_over_metadata_completeness() {
        let brief = brief_for("plumber", "Plumbing services");
        // Ranking on completeness alone kept the best-documented record even
        // when it had nothing to do with the brief.
        let relevant_sparse = discovery(
            "Reliable Plumbing",
            "Mapped plumber in the bounded Ohio sample.",
            false,
        );
        let irrelevant_complete = discovery(
            "Bloom Florist",
            "Mapped florist in the bounded Ohio sample.",
            true,
        );
        assert!(
            discovery_priority(&relevant_sparse, &brief)
                > discovery_priority(&irrelevant_complete, &brief),
            "a relevant candidate must outrank a merely well-documented one"
        );
    }

    #[test]
    fn completeness_still_breaks_ties_between_equally_relevant_records() {
        let brief = brief_for("plumber", "Plumbing services");
        let reason = "Mapped plumber in the bounded Ohio sample.";
        assert!(
            discovery_priority(&discovery("A Plumbing", reason, true), &brief)
                > discovery_priority(&discovery("B Plumbing", reason, false), &brief)
        );
    }

    #[test]
    fn relevance_scoring_ignores_short_noise_words() {
        let brief = brief_for("plumber", "Plumbing services");
        // "in", "the" and similar must never earn relevance points.
        let noise = discovery("Generic Co", "in the of and", false);
        assert_eq!(relevance_score(&noise, &brief), 0);
    }
}

/// Live yield evaluation against the real public sources.
///
/// Ignored by default: it needs network access and takes minutes. Run it with
/// `scripts/live-search-eval.sh`, before and after a change, and compare.
#[cfg(test)]
mod live_eval {
    use super::*;

    const BRIEFS: &[(&str, &str)] = &[
        (
            "I sell payroll and shift scheduling software to restaurants with hourly staff",
            "Austin, TX",
        ),
        (
            "We sell HIPAA compliance and patient data security software to dental practices",
            "Pennsylvania",
        ),
        (
            "I am a screenwriter with a pilot script looking for literary representation",
            "United States",
        ),
    ];

    #[tokio::test]
    #[ignore]
    async fn live_search_yield_eval() {
        let directory = tempfile::tempdir().unwrap();
        let state = AppState {
            database: Arc::new(Mutex::new(
                db::open(&directory.path().join("eval.sqlite3")).unwrap(),
            )),
            client: build_client().unwrap(),
            app_data_dir: directory.path().to_path_buf(),
            robots: Arc::default(),
        };
        let sink: ProgressSink = Arc::new(|_| {});
        for (query, geography) in BRIEFS {
            let mut brief = ai::interpret_search_brief(query, None, geography, 10, None)
                .await
                .unwrap();
            brief.approved = true;
            let run = db::create_search_run(&state.connection().unwrap(), &brief).unwrap();
            let started = std::time::Instant::now();
            let outcome = execute_search_run(&state, &sink, &run.id).await;
            let elapsed = started.elapsed().as_secs();
            let run = db::get_search_run(&state.connection().unwrap(), &run.id).unwrap();
            println!("\n=== {query} ({geography})");
            if let Err(error) = outcome {
                println!("run error: {error}");
            }
            for warning in &run.warnings {
                println!("warning [{}]: {}", warning.source, warning.message);
            }
            let researched = run
                .accounts
                .iter()
                .filter(|item| !matches!(item.state.as_str(), "screening" | "researching"))
                .collect::<Vec<_>>();
            let with_evidence = researched
                .iter()
                .filter(|item| {
                    db::get_package(&state.connection().unwrap(), &item.account.id)
                        .map(|package| {
                            package
                                .evidence
                                .iter()
                                .any(|evidence| !evidence.source_kind.ends_with("_record"))
                        })
                        .unwrap_or(false)
                })
                .count();
            println!(
                "wall {elapsed}s · found {} · researched {} · with page evidence {with_evidence} · strong {} · near {}",
                run.candidate_count,
                researched.len(),
                run.strong_count,
                run.near_match_count
            );
            for rule in &brief.positive_signals {
                let hits = researched
                    .iter()
                    .filter(|item| {
                        item.matched_features
                            .iter()
                            .any(|feature| feature.rule_id == rule.id)
                    })
                    .count();
                println!(
                    "rule {:<28} hit {hits}/{}",
                    rule.id,
                    researched.len().max(1)
                );
            }
            let mut ranked = run
                .accounts
                .iter()
                .filter(|item| item.result_tier != "excluded")
                .collect::<Vec<_>>();
            ranked.sort_by_key(|item| item.rank.unwrap_or(i64::MAX));
            for item in ranked.iter().take(5) {
                println!(
                    "  #{} {} [{}] score {} · {}",
                    item.rank.unwrap_or_default(),
                    item.account.name,
                    item.result_tier,
                    item.score.unwrap_or_default(),
                    item.account.website_url.as_deref().unwrap_or("no website")
                );
                let package =
                    db::get_package(&state.connection().unwrap(), &item.account.id).unwrap();
                for feature in item
                    .matched_features
                    .iter()
                    .filter(|feature| feature.component == "signal")
                {
                    let quote = package
                        .evidence
                        .iter()
                        .find(|evidence| feature.evidence_ids.contains(&evidence.id))
                        .map(|evidence| evidence.excerpt.chars().take(140).collect::<String>())
                        .unwrap_or_default();
                    println!("      {}: \"{quote}\"", feature.label);
                }
            }
        }
    }
}
