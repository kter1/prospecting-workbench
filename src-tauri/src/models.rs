use serde::{Deserialize, Serialize};

pub const CONTRACT_VERSION: u8 = 3;
pub const MODEL_TAG: &str = "qwen3.5:9b";
pub const PROMPT_VERSION: &str = "prospect-package-v1";
pub const SEARCH_PROMPT_VERSION: &str = "search-brief-v4";

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SalesPlay {
    pub id: String,
    pub name: String,
    pub offer: String,
    pub icp_industries: Vec<String>,
    pub company_sizes: Vec<String>,
    pub target_personas: Vec<String>,
    pub exclusions: Vec<String>,
    pub proof_points: Vec<String>,
    pub tone: String,
    pub cta: String,
    pub sender_name: String,
    pub sender_company: String,
    pub postal_address: String,
    pub opt_out_text: String,
    pub approved: bool,
    pub updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferProfile {
    pub id: String,
    pub name: String,
    pub product: String,
    pub customer_outcome: String,
    pub proof_points: Vec<String>,
    pub compliance_restrictions: Vec<String>,
    pub tone: String,
    pub cta: String,
    pub sender_name: String,
    pub sender_company: String,
    pub postal_address: String,
    pub opt_out_text: String,
    pub updated_at: Option<String>,
}

impl From<&SalesPlay> for OfferProfile {
    fn from(play: &SalesPlay) -> Self {
        Self {
            id: "active".into(),
            name: play.name.clone(),
            product: play.offer.clone(),
            customer_outcome: play.offer.clone(),
            proof_points: play.proof_points.clone(),
            compliance_restrictions: play.exclusions.clone(),
            tone: play.tone.clone(),
            cta: play.cta.clone(),
            sender_name: play.sender_name.clone(),
            sender_company: play.sender_company.clone(),
            postal_address: play.postal_address.clone(),
            opt_out_text: play.opt_out_text.clone(),
            updated_at: play.updated_at.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetSegment {
    pub id: String,
    pub label: String,
    pub discovery_category: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QualificationRule {
    pub id: String,
    pub label: String,
    pub component: String,
    pub keywords: Vec<String>,
    pub points: i64,
    pub hard_gate: bool,
    /// Where the rule came from: `catalog`, `pack`, `model`, `query` or `user`.
    #[serde(default)]
    pub origin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryLane {
    pub id: String,
    pub label: String,
    pub mode: String,
    pub source: String,
    pub reason: String,
    pub limitation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchBrief {
    pub id: String,
    pub query: String,
    pub product_summary: String,
    pub customer_outcome: String,
    pub segments: Vec<TargetSegment>,
    pub geography: String,
    pub company_sizes: Vec<String>,
    pub target_personas: Vec<String>,
    pub positive_signals: Vec<QualificationRule>,
    pub exclusions: Vec<String>,
    pub discovery_lanes: Vec<DiscoveryLane>,
    pub result_limit: i64,
    pub qualification_threshold: i64,
    #[serde(default = "default_quality_band")]
    pub quality_band: i64,
    pub approved: bool,
    pub assumptions: Vec<String>,
    pub interpreter: String,
    pub model_tag: Option<String>,
    pub model_digest: Option<String>,
    pub prompt_version: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitFeature {
    pub rule_id: String,
    pub label: String,
    pub component: String,
    pub value: String,
    pub points_awarded: i64,
    pub evidence_ids: Vec<String>,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceIdentity {
    pub id: String,
    pub account_id: String,
    pub source_name: String,
    pub source_external_id: Option<String>,
    pub source_record_url: String,
    pub identity_confidence: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRunAccount {
    pub account: Account,
    pub state: String,
    pub discovery_source: String,
    pub discovery_reason: String,
    pub score: Option<i64>,
    pub evidence_confidence: String,
    pub rejection_reasons: Vec<String>,
    pub matched_features: Vec<FitFeature>,
    #[serde(default)]
    pub assessment: Option<FitAssessment>,
    #[serde(default)]
    pub rank: Option<i64>,
    #[serde(default = "default_result_tier")]
    pub result_tier: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceWarning {
    pub source: String,
    pub message: String,
    pub used_cached_results: bool,
    pub cache_age_hours: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRun {
    pub id: String,
    pub query: String,
    pub brief_id: String,
    pub status: String,
    pub progress_completed: i64,
    pub progress_total: i64,
    #[serde(default)]
    pub progress_percent: i64,
    #[serde(default = "default_progress_stage")]
    pub progress_stage: String,
    pub result_limit: i64,
    #[serde(default)]
    pub candidate_count: i64,
    #[serde(default)]
    pub researched_count: i64,
    #[serde(default)]
    pub strong_count: i64,
    #[serde(default)]
    pub near_match_count: i64,
    #[serde(default)]
    pub warnings: Vec<SourceWarning>,
    pub started_at: String,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub last_error: Option<String>,
    pub accounts: Vec<SearchRunAccount>,
}

/// Payload for the `search-run-progress` event.
///
/// Progress fires once per researched candidate. Emitting a full `SearchRun`
/// meant re-reading and re-parsing every candidate in the pool on every tick,
/// which is quadratic in pool size and sends megabytes over IPC. The UI keeps
/// its accounts and fetches pages on demand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRunProgress {
    pub id: String,
    pub status: String,
    pub progress_completed: i64,
    pub progress_total: i64,
    pub progress_percent: i64,
    pub progress_stage: String,
    pub result_limit: i64,
    pub candidate_count: i64,
    pub researched_count: i64,
    pub strong_count: i64,
    pub near_match_count: i64,
    /// True once the run reaches a terminal state, so the UI knows to reload.
    pub finished: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchRunSummary {
    pub id: String,
    pub query: String,
    pub status: String,
    pub progress_completed: i64,
    pub progress_total: i64,
    #[serde(default)]
    pub progress_percent: i64,
    #[serde(default = "default_progress_stage")]
    pub progress_stage: String,
    pub qualified_count: i64,
    pub review_count: i64,
    #[serde(default)]
    pub strong_count: i64,
    pub started_at: String,
}

fn default_progress_stage() -> String {
    "Queued".into()
}

fn default_quality_band() -> i64 {
    4
}

fn default_result_tier() -> String {
    "near_match".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSeed {
    pub company_name: String,
    pub domain: Option<String>,
    pub website_url: Option<String>,
    pub person_name: Option<String>,
    pub person_title: Option<String>,
    pub work_email: Option<String>,
    pub business_phone: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub domain: Option<String>,
    pub website_url: Option<String>,
    pub industry: Option<String>,
    pub employee_band: Option<String>,
    pub location: Option<String>,
    pub status: String,
    pub score: Option<i64>,
    pub score_label: Option<String>,
    pub researched_at: Option<String>,
    pub created_at: String,
    pub saved_at: Option<String>,
    pub favorited_at: Option<String>,
    pub trashed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Person {
    pub id: String,
    pub account_id: String,
    pub name: String,
    pub title: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContactPoint {
    pub id: String,
    pub account_id: String,
    pub person_id: Option<String>,
    pub kind: String,
    pub value: String,
    pub evidence_id: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    pub id: String,
    pub account_id: String,
    pub source_url: String,
    pub source_kind: String,
    pub title: String,
    pub excerpt: String,
    pub retrieved_at: String,
    pub published_at: Option<String>,
    pub content_hash: String,
    pub confidence: String,
    pub freshness: String,
    #[serde(default)]
    pub quality_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Signal {
    pub id: String,
    pub account_id: String,
    pub category: String,
    pub summary: String,
    pub occurred_at: Option<String>,
    pub evidence_id: String,
    pub confidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardGate {
    pub label: String,
    pub status: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CitedReason {
    pub text: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FitAssessment {
    pub hard_gates: Vec<HardGate>,
    pub icp_fit: i64,
    pub signal_relevance: i64,
    pub persona_fit: i64,
    pub evidence_contactability: i64,
    pub total: i64,
    pub reasons: Vec<CitedReason>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClaimEvidence {
    pub claim: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftArtifact {
    pub id: String,
    pub kind: String,
    pub subject: String,
    pub body: String,
    pub claim_evidence: Vec<ClaimEvidence>,
    pub model_tag: String,
    pub model_digest: String,
    pub prompt_version: String,
    pub verified: bool,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CallBrief {
    pub opener: String,
    pub reason_now: String,
    pub hypotheses: Vec<String>,
    pub discovery_questions: Vec<String>,
    pub relevant_proof: Vec<String>,
    pub likely_objections: Vec<String>,
    pub desired_outcome: String,
    pub evidence_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProspectPackage {
    pub account: Account,
    pub people: Vec<Person>,
    pub contacts: Vec<ContactPoint>,
    pub evidence: Vec<Evidence>,
    pub signals: Vec<Signal>,
    pub assessment: Option<FitAssessment>,
    pub draft: Option<DraftArtifact>,
    pub call_brief: Option<CallBrief>,
    pub suppressed: bool,
    pub latest_outcome: Option<String>,
    pub discovery_context: Option<DiscoveryContext>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryContext {
    pub search_run_id: String,
    pub query: String,
    pub discovery_source: String,
    pub discovery_reason: String,
    pub state: String,
    pub rejection_reasons: Vec<String>,
    pub matched_features: Vec<FitFeature>,
    pub evidence_confidence: String,
    pub result_tier: String,
    pub rank: Option<i64>,
    pub assessment: Option<FitAssessment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OllamaHealth {
    pub reachable: bool,
    pub model_installed: bool,
    pub model_tag: String,
    pub model_digest: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SourceCapability {
    pub id: String,
    pub name: String,
    pub modes: Vec<String>,
    pub requires_configuration: bool,
    pub enabled: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResearchJob {
    pub id: String,
    pub account_id: Option<String>,
    pub kind: String,
    pub status: String,
    pub attempts: i64,
    pub last_error: Option<String>,
    pub scheduled_at: String,
    pub completed_at: Option<String>,
    #[serde(default)]
    pub progress_percent: i64,
    #[serde(default = "default_progress_stage")]
    pub progress_stage: String,
    #[serde(default)]
    pub request: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftReadiness {
    pub ready: bool,
    pub missing_fields: Vec<String>,
    pub blockers: Vec<String>,
    pub requires_review_override: bool,
    pub requires_play_approval: bool,
    pub suggested_play: SalesPlay,
    pub ollama: OllamaHealth,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DraftGenerationRequest {
    pub account_id: String,
    pub provider: String,
    pub kind: String,
    pub override_review: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryResult {
    pub source: String,
    pub company_name: String,
    pub domain: Option<String>,
    pub website_url: Option<String>,
    pub external_id: Option<String>,
    pub reason: String,
    pub source_url: String,
    #[serde(default)]
    pub source_external_id: Option<String>,
    #[serde(default)]
    pub business_phone: Option<String>,
    #[serde(default)]
    pub location: Option<String>,
    #[serde(default)]
    pub identity_confidence: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardSnapshot {
    pub sales_play: Option<SalesPlay>,
    pub offer_profile: Option<OfferProfile>,
    pub accounts: Vec<Account>,
    pub pending_jobs: i64,
    pub due_for_daily_scan: bool,
    pub source_capabilities: Vec<SourceCapability>,
    pub search_runs: Vec<SearchRunSummary>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPackageOutput {
    pub reasons: Vec<CitedReason>,
    pub subject: String,
    pub body: String,
    pub claim_evidence: Vec<ClaimEvidence>,
    pub call_brief: CallBrief,
}
