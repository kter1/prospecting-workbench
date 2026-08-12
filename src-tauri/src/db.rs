use std::path::Path;

use chrono::{Duration, Utc};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use sha2::{Digest, Sha256};
use url::Url;
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::*;

type SearchRunDbRow = (
    String,
    String,
    Option<String>,
    String,
    i64,
    i64,
    i64,
    String,
    i64,
    i64,
    i64,
    i64,
    i64,
    String,
    String,
    Option<String>,
    Option<String>,
);

type DiscoveryContextDbRow = (
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    String,
    Option<i64>,
    Option<String>,
);

pub struct SearchRunAccountUpdate<'a> {
    pub state: &'a str,
    pub score: Option<i64>,
    pub confidence: &'a str,
    pub rejection_reasons: &'a [String],
    pub features: &'a [FitFeature],
    pub assessment: Option<&'a FitAssessment>,
    pub rank: Option<i64>,
    pub result_tier: &'a str,
}

pub fn open(path: &Path) -> AppResult<Connection> {
    let connection = Connection::open(path)?;
    connection.busy_timeout(std::time::Duration::from_secs(5))?;
    connection.execute_batch(include_str!("../migrations/001_init.sql"))?;
    connection.execute_batch(include_str!("../migrations/002_prompt_first.sql"))?;
    let has_quality_flags = {
        let mut statement = connection.prepare("PRAGMA table_info(evidence)")?;
        let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
        columns
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|column| column == "quality_flags")
    };
    if !has_quality_flags {
        connection.execute(
            "ALTER TABLE evidence ADD COLUMN quality_flags TEXT NOT NULL DEFAULT '[]'",
            [],
        )?;
    }
    // The FTS5 index was written on every evidence insert but never queried:
    // there is no MATCH anywhere in the codebase. It was a write tax on the
    // hottest insert path and could silently desync, since a contentless FTS5
    // table gets no ON DELETE CASCADE.
    connection.execute_batch("DROP TABLE IF EXISTS evidence_fts;")?;
    migrate_search_progress_schema(&connection)?;
    migrate_quality_first_schema(&connection)?;
    migrate_prompt_first_data(&connection)?;
    Ok(connection)
}

fn table_has_column(connection: &Connection, table: &str, column: &str) -> AppResult<bool> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let columns = statement.query_map([], |row| row.get::<_, String>(1))?;
    Ok(columns
        .collect::<Result<Vec<_>, _>>()?
        .iter()
        .any(|candidate| candidate == column))
}

fn migrate_search_progress_schema(connection: &Connection) -> AppResult<()> {
    if !table_has_column(connection, "search_runs", "progress_percent")? {
        connection.execute(
            "ALTER TABLE search_runs ADD COLUMN progress_percent INTEGER NOT NULL DEFAULT 0",
            [],
        )?;
    }
    if !table_has_column(connection, "search_runs", "progress_stage")? {
        connection.execute(
            "ALTER TABLE search_runs ADD COLUMN progress_stage TEXT NOT NULL DEFAULT 'Queued'",
            [],
        )?;
    }
    connection.execute(
        "UPDATE search_runs SET progress_percent=100,progress_stage='Complete' WHERE status='complete' AND progress_percent<100",
        [],
    )?;
    Ok(())
}

fn migrate_quality_first_schema(connection: &Connection) -> AppResult<()> {
    let initializing_library = !table_has_column(connection, "accounts", "saved_at")?;
    for (column, definition) in [
        ("saved_at", "TEXT"),
        ("favorited_at", "TEXT"),
        ("trashed_at", "TEXT"),
    ] {
        if !table_has_column(connection, "accounts", column)? {
            connection.execute(
                &format!("ALTER TABLE accounts ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    for (column, definition) in [
        ("candidate_count", "INTEGER NOT NULL DEFAULT 0"),
        ("researched_count", "INTEGER NOT NULL DEFAULT 0"),
        ("strong_count", "INTEGER NOT NULL DEFAULT 0"),
        ("near_match_count", "INTEGER NOT NULL DEFAULT 0"),
        ("warnings_json", "TEXT NOT NULL DEFAULT '[]'"),
    ] {
        if !table_has_column(connection, "search_runs", column)? {
            connection.execute(
                &format!("ALTER TABLE search_runs ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    let adding_assessment_flag =
        !table_has_column(connection, "search_run_accounts", "assessment_ok")?;
    for (column, definition) in [
        ("assessment_json", "TEXT"),
        ("rank", "INTEGER"),
        ("result_tier", "TEXT NOT NULL DEFAULT 'near_match'"),
        // Precomputed from assessment_json at write time so ranking never has
        // to parse JSON. Ranking runs once per batch over the whole pool, so a
        // per-row parse there is quadratic in the candidate count.
        ("assessment_ok", "INTEGER NOT NULL DEFAULT 0"),
    ] {
        if !table_has_column(connection, "search_run_accounts", column)? {
            connection.execute(
                &format!("ALTER TABLE search_run_accounts ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    if adding_assessment_flag {
        backfill_assessment_ok(connection)?;
    }
    for (column, definition) in [
        ("progress_percent", "INTEGER NOT NULL DEFAULT 0"),
        ("progress_stage", "TEXT NOT NULL DEFAULT 'Queued'"),
        ("request_json", "TEXT"),
    ] {
        if !table_has_column(connection, "jobs", column)? {
            connection.execute(
                &format!("ALTER TABLE jobs ADD COLUMN {column} {definition}"),
                [],
            )?;
        }
    }
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS discovery_cache (
           cache_key TEXT PRIMARY KEY,
           source TEXT NOT NULL,
           payload_json TEXT NOT NULL,
           fetched_at TEXT NOT NULL
         );
         CREATE TABLE IF NOT EXISTS source_backoff (
           source TEXT PRIMARY KEY,
           not_before TEXT NOT NULL,
           last_error TEXT NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_accounts_library ON accounts(trashed_at,saved_at,favorited_at);",
    )?;
    if initializing_library {
        connection.execute("UPDATE accounts SET saved_at=created_at", [])?;
        connection.execute(
            "UPDATE search_run_accounts SET assessment_json=(SELECT assessment_json FROM packages WHERE packages.account_id=search_run_accounts.account_id) WHERE assessment_json IS NULL",
            [],
        )?;
    }
    connection.execute(
        "UPDATE search_run_accounts SET result_tier=CASE WHEN state='qualified' THEN 'near_match' ELSE 'excluded' END WHERE result_tier NOT IN ('strong','near_match','excluded') OR result_tier IS NULL",
        [],
    )?;
    if initializing_library {
        let mut statement = connection.prepare("SELECT id,brief_id,result_limit FROM search_runs WHERE status='complete' AND brief_id IS NOT NULL")?;
        let rows = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;
        let historical = rows.collect::<Result<Vec<_>, _>>()?;
        drop(statement);
        for (run_id, brief_id, limit) in historical {
            if let Ok(brief) = get_search_brief(connection, &brief_id) {
                let _ = finalize_search_ranking(
                    connection,
                    &run_id,
                    brief.qualification_threshold,
                    brief.quality_band,
                    limit,
                );
            }
        }
    }
    connection.execute(
        "DELETE FROM search_briefs WHERE approved=0 AND NOT EXISTS(SELECT 1 FROM search_runs WHERE search_runs.brief_id=search_briefs.id)",
        [],
    )?;
    Ok(())
}

/// Does this assessment clear the non-numeric half of the strong-match bar?
///
/// Split out so it is computed once when a candidate is scored rather than on
/// every ranking pass.
fn assessment_is_promotable(assessment: &FitAssessment) -> bool {
    assessment
        .hard_gates
        .iter()
        .all(|gate| gate.status == "pass")
        && !assessment.reasons.is_empty()
}

/// One-time pass to populate `assessment_ok` for rows written before the
/// column existed. Bounded by existing data and only runs on the migration.
fn backfill_assessment_ok(connection: &Connection) -> AppResult<()> {
    let rows = {
        let mut statement = connection.prepare(
            "SELECT search_run_id,account_id,assessment_json FROM search_run_accounts WHERE assessment_json IS NOT NULL",
        )?;
        let mapped = statement.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?;
        mapped.collect::<Result<Vec<_>, _>>()?
    };
    for (run_id, account_id, json) in rows {
        let promotable = serde_json::from_str::<FitAssessment>(&json)
            .map(|assessment| assessment_is_promotable(&assessment))
            .unwrap_or(false);
        connection.execute(
            "UPDATE search_run_accounts SET assessment_ok=?3 WHERE search_run_id=?1 AND account_id=?2",
            params![run_id, account_id, i64::from(promotable)],
        )?;
    }
    Ok(())
}

fn migrate_prompt_first_data(connection: &Connection) -> AppResult<()> {
    let has_profile: bool = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM offer_profile WHERE id='active')",
        [],
        |row| row.get(0),
    )?;
    if !has_profile {
        if let Some(play) = load_sales_play(connection)? {
            save_offer_profile(connection, OfferProfile::from(&play))?;
        }
    }
    let account_count: i64 =
        connection.query_row("SELECT count(*) FROM accounts", [], |row| row.get(0))?;
    let run_count: i64 =
        connection.query_row("SELECT count(*) FROM search_runs", [], |row| row.get(0))?;
    if account_count > 0 && run_count == 0 {
        let timestamp = now();
        connection.execute(
            "INSERT INTO search_runs(id,query,status,progress_completed,progress_total,result_limit,prompt_version,started_at,completed_at)
             VALUES('legacy-import','Existing accounts imported before prompt-first search','complete',?1,?1,?1,'legacy',?2,?2)",
            params![account_count, timestamp],
        )?;
        connection.execute(
            "INSERT OR IGNORE INTO search_run_accounts(search_run_id,account_id,state,discovery_source,discovery_reason,evidence_confidence,created_at,updated_at)
             SELECT 'legacy-import',id,CASE WHEN status='qualified' THEN 'qualified' WHEN status='failed' THEN 'failed' ELSE 'needs_review' END,
                    'legacy','Existing local account; original discovery query was not recorded.','unknown',?1,?1 FROM accounts",
            params![timestamp],
        )?;
    }
    Ok(())
}

pub fn now() -> String {
    Utc::now().to_rfc3339()
}

pub fn normalize_domain(value: &str) -> Option<String> {
    let trimmed = value.trim().trim_end_matches('.');
    if trimmed.is_empty() {
        return None;
    }
    let candidate = if trimmed.contains("://") {
        trimmed.to_owned()
    } else {
        format!("https://{trimmed}")
    };
    let host = Url::parse(&candidate)
        .ok()?
        .host_str()?
        .to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    if host == "localhost" || !host.contains('.') {
        return None;
    }
    Some(psl::domain_str(&host).unwrap_or(&host).to_string())
}

pub fn normalize_email(value: &str) -> Option<String> {
    let email = value.trim().to_ascii_lowercase();
    let mut pieces = email.split('@');
    let local = pieces.next()?;
    let domain = pieces.next()?;
    if pieces.next().is_some()
        || local.is_empty()
        || !domain.contains('.')
        || email.chars().any(char::is_whitespace)
    {
        None
    } else {
        Some(email)
    }
}

/// Normalize a website URL supplied by a source or a user, rejecting anything
/// that is not a public web address.
///
/// Delegates the host checks to `safety::validate_url` so a private or
/// loopback address cannot be stored as a company website. Previously this only
/// checked scheme, credentials, and port, so `http://127.0.0.1/admin` from an
/// OpenStreetMap `website` tag, an imported CSV, or a reference-data record was
/// accepted as an account's site. The crawler would still refuse to fetch it,
/// but it had no business being saved as an identity in the first place.
pub fn normalize_public_url(value: &str) -> Option<String> {
    let mut url = Url::parse(value.trim()).ok()?;
    if crate::safety::validate_url(&url).is_err() || normalize_domain(value).is_none() {
        return None;
    }
    url.set_fragment(None);
    Some(url.to_string())
}

pub fn normalize_us_phone(value: &str) -> Option<String> {
    let digits: String = value.chars().filter(char::is_ascii_digit).collect();
    match digits.len() {
        10 => Some(format!("+1{digits}")),
        11 if digits.starts_with('1') => Some(format!("+{digits}")),
        _ => None,
    }
}

pub fn score_label(score: i64) -> &'static str {
    match score {
        75.. => "Strong fit",
        55..=74 => "Promising",
        35..=54 => "Weak fit",
        _ => "Low fit",
    }
}

fn account_from_row(row: &Row<'_>) -> rusqlite::Result<Account> {
    Ok(Account {
        id: row.get(0)?,
        name: row.get(1)?,
        domain: row.get(2)?,
        website_url: row.get(3)?,
        industry: row.get(4)?,
        employee_band: row.get(5)?,
        location: row.get(6)?,
        status: row.get(7)?,
        score: row.get(8)?,
        score_label: row.get(9)?,
        researched_at: row.get(10)?,
        created_at: row.get(11)?,
        saved_at: row.get(12)?,
        favorited_at: row.get(13)?,
        trashed_at: row.get(14)?,
    })
}

const ACCOUNT_COLUMNS: &str = "id,name,domain,website_url,industry,employee_band,location,status,score,score_label,researched_at,created_at,saved_at,favorited_at,trashed_at";

pub fn save_sales_play(connection: &Connection, mut play: SalesPlay) -> AppResult<SalesPlay> {
    if play.id != "active" {
        return Err(AppError::Message(
            "Only one active sales play is supported.".into(),
        ));
    }
    if play.approved
        && [&play.name, &play.offer]
            .iter()
            .any(|value| value.trim().is_empty())
    {
        return Err(AppError::Message(
            "An approved play requires a name and product description.".into(),
        ));
    }
    let timestamp = now();
    play.updated_at = Some(timestamp.clone());
    let json = serde_json::to_string(&play)?;
    connection.execute(
        "INSERT INTO sales_play(id,json,updated_at) VALUES('active',?1,?2)
         ON CONFLICT(id) DO UPDATE SET json=excluded.json, updated_at=excluded.updated_at",
        params![json, timestamp],
    )?;
    save_offer_profile(connection, OfferProfile::from(&play))?;
    Ok(play)
}

pub fn load_sales_play(connection: &Connection) -> AppResult<Option<SalesPlay>> {
    let json: Option<String> = connection
        .query_row("SELECT json FROM sales_play WHERE id='active'", [], |row| {
            row.get(0)
        })
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(AppError::from))
        .transpose()
}

pub fn save_offer_profile(
    connection: &Connection,
    mut profile: OfferProfile,
) -> AppResult<OfferProfile> {
    if profile.id != "active" {
        return Err(AppError::Message(
            "Only one active offer profile is supported.".into(),
        ));
    }
    let timestamp = now();
    profile.updated_at = Some(timestamp.clone());
    connection.execute(
        "INSERT INTO offer_profile(id,json,updated_at) VALUES('active',?1,?2)
         ON CONFLICT(id) DO UPDATE SET json=excluded.json,updated_at=excluded.updated_at",
        params![serde_json::to_string(&profile)?, timestamp],
    )?;
    Ok(profile)
}

pub fn load_offer_profile(connection: &Connection) -> AppResult<Option<OfferProfile>> {
    let json: Option<String> = connection
        .query_row(
            "SELECT json FROM offer_profile WHERE id='active'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(AppError::from))
        .transpose()
}

pub fn require_approved_play(connection: &Connection) -> AppResult<SalesPlay> {
    load_sales_play(connection)?
        .filter(|play| play.approved)
        .ok_or_else(|| AppError::Message("Complete the offer setup before this action.".into()))
}

pub fn list_accounts(connection: &Connection) -> AppResult<Vec<Account>> {
    // The latest run score is resolved in one correlated subquery. Fetching it
    // per account was an N+1: every row ran its own search_run_accounts join,
    // and bootstrap_app called this twice. The SQL ORDER BY was also wasted,
    // since the scores it sorted on were then overwritten and re-sorted.
    let sql = format!(
        "SELECT {ACCOUNT_COLUMNS},
           (SELECT ra.score FROM search_run_accounts ra
              JOIN search_runs r ON r.id=ra.search_run_id
             WHERE ra.account_id=accounts.id AND ra.score IS NOT NULL
             ORDER BY r.started_at DESC LIMIT 1) AS latest_run_score
         FROM accounts"
    );
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map([], |row| {
        let mut account = account_from_row(row)?;
        if let Some(score) = row.get::<_, Option<i64>>(15)? {
            account.score = Some(score);
            account.score_label = Some(score_label(score).into());
        }
        Ok(account)
    })?;
    let mut accounts = rows.collect::<Result<Vec<_>, _>>()?;
    accounts.sort_by(|left, right| {
        right
            .score
            .unwrap_or(-1)
            .cmp(&left.score.unwrap_or(-1))
            .then_with(|| right.created_at.cmp(&left.created_at))
    });
    Ok(accounts)
}

pub fn get_account(connection: &Connection, account_id: &str) -> AppResult<Account> {
    let sql = format!("SELECT {ACCOUNT_COLUMNS} FROM accounts WHERE id=?1");
    let mut account = connection
        .query_row(&sql, params![account_id], account_from_row)
        .optional()?
        .ok_or_else(|| AppError::Message("Account not found.".into()))?;
    apply_latest_run_score(connection, &mut account)?;
    Ok(account)
}

fn apply_latest_run_score(connection: &Connection, account: &mut Account) -> AppResult<()> {
    if let Some(score) = connection.query_row(
        "SELECT ra.score FROM search_run_accounts ra JOIN search_runs r ON r.id=ra.search_run_id WHERE ra.account_id=?1 AND ra.score IS NOT NULL ORDER BY r.started_at DESC LIMIT 1",
        params![account.id],
        |row| row.get::<_, i64>(0),
    ).optional()? {
        account.score = Some(score);
        account.score_label = Some(score_label(score).into());
    }
    Ok(())
}

pub fn apply_library_action(
    connection: &mut Connection,
    account_ids: &[String],
    action: &str,
) -> AppResult<Vec<Account>> {
    if account_ids.is_empty() {
        return Err(AppError::Message("Select at least one prospect.".into()));
    }
    if !matches!(
        action,
        "save" | "unsave" | "favorite" | "unfavorite" | "trash" | "restore"
    ) {
        return Err(AppError::Message("Unknown prospect-library action.".into()));
    }
    let transaction = connection.transaction()?;
    let timestamp = now();
    for account_id in account_ids {
        let changed = match action {
            "save" => transaction.execute(
                "UPDATE accounts SET saved_at=COALESCE(saved_at,?2),updated_at=?2 WHERE id=?1 AND trashed_at IS NULL",
                params![account_id, timestamp],
            )?,
            "unsave" => transaction.execute(
                "UPDATE accounts SET saved_at=NULL,favorited_at=NULL,updated_at=?2 WHERE id=?1 AND trashed_at IS NULL",
                params![account_id, timestamp],
            )?,
            "favorite" => transaction.execute(
                "UPDATE accounts SET saved_at=COALESCE(saved_at,?2),favorited_at=COALESCE(favorited_at,?2),updated_at=?2 WHERE id=?1 AND trashed_at IS NULL",
                params![account_id, timestamp],
            )?,
            "unfavorite" => transaction.execute(
                "UPDATE accounts SET favorited_at=NULL,updated_at=?2 WHERE id=?1 AND trashed_at IS NULL",
                params![account_id, timestamp],
            )?,
            "trash" => transaction.execute(
                "UPDATE accounts SET trashed_at=COALESCE(trashed_at,?2),updated_at=?2 WHERE id=?1",
                params![account_id, timestamp],
            )?,
            "restore" => transaction.execute(
                "UPDATE accounts SET trashed_at=NULL,updated_at=?2 WHERE id=?1 AND trashed_at IS NOT NULL",
                params![account_id, timestamp],
            )?,
            _ => 0,
        };
        if changed == 0 && !matches!(action, "save" | "favorite" | "unsave" | "unfavorite") {
            return Err(AppError::Message(format!(
                "Prospect {account_id} could not be updated for action {action}."
            )));
        }
    }
    transaction.commit()?;
    list_accounts(connection)
}

pub fn permanently_delete_accounts(
    connection: &mut Connection,
    account_ids: &[String],
) -> AppResult<Vec<Account>> {
    if account_ids.is_empty() {
        return Err(AppError::Message(
            "Select at least one trashed prospect.".into(),
        ));
    }
    let transaction = connection.transaction()?;
    let mut affected_runs = std::collections::HashSet::new();
    for account_id in account_ids {
        let trashed: bool = transaction
            .query_row(
                "SELECT trashed_at IS NOT NULL FROM accounts WHERE id=?1",
                params![account_id],
                |row| row.get(0),
            )
            .optional()?
            .ok_or_else(|| AppError::Message("Prospect not found.".into()))?;
        if !trashed {
            return Err(AppError::Message(
                "Permanent deletion is restricted to prospects in Trash.".into(),
            ));
        }
        let mut statement = transaction
            .prepare("SELECT search_run_id FROM search_run_accounts WHERE account_id=?1")?;
        let rows = statement.query_map(params![account_id], |row| row.get::<_, String>(0))?;
        affected_runs.extend(rows.collect::<Result<Vec<_>, _>>()?);
        drop(statement);
        transaction.execute("DELETE FROM accounts WHERE id=?1", params![account_id])?;
    }
    for run_id in affected_runs {
        transaction.execute(
            "UPDATE search_runs SET candidate_count=(SELECT count(*) FROM search_run_accounts WHERE search_run_id=?1),strong_count=(SELECT count(*) FROM search_run_accounts WHERE search_run_id=?1 AND result_tier='strong'),near_match_count=(SELECT count(*) FROM search_run_accounts WHERE search_run_id=?1 AND result_tier='near_match') WHERE id=?1",
            params![run_id],
        )?;
    }
    transaction.commit()?;
    list_accounts(connection)
}

pub fn import_accounts(
    connection: &mut Connection,
    seeds: Vec<AccountSeed>,
) -> AppResult<Vec<Account>> {
    if seeds.is_empty() {
        return Err(AppError::Message(
            "No valid account seeds were provided.".into(),
        ));
    }
    let transaction = connection.transaction()?;
    for seed in seeds {
        import_seed(&transaction, seed)?;
    }
    transaction.commit()?;
    list_accounts(connection)
}

fn import_seed(transaction: &Transaction<'_>, seed: AccountSeed) -> AppResult<()> {
    let candidate_domain = seed
        .domain
        .as_deref()
        .and_then(normalize_domain)
        .or_else(|| seed.website_url.as_deref().and_then(normalize_domain));
    let is_public_index = candidate_domain.as_deref().is_some_and(|domain| {
        domain == "sec.gov"
            || domain.ends_with(".sec.gov")
            || domain == "usaspending.gov"
            || domain.ends_with(".usaspending.gov")
    });
    let domain = if is_public_index {
        None
    } else {
        candidate_domain
    };
    let supplied_url = seed.website_url.as_deref().and_then(normalize_public_url);
    let website_url = if is_public_index {
        None
    } else if supplied_url.as_deref().and_then(normalize_domain) == domain {
        supplied_url
    } else {
        domain.as_ref().map(|item| format!("https://{item}"))
    };
    let name = seed
        .company_name
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if name.is_empty() || (domain.is_none() && seed.website_url.is_none()) {
        return Err(AppError::Message(
            "Each seed needs a company name plus a valid public domain or URL.".into(),
        ));
    }
    let existing_id: Option<String> = if let Some(item) = domain.as_ref() {
        transaction
            .query_row(
                "SELECT id FROM accounts WHERE domain=?1",
                params![item],
                |row| row.get(0),
            )
            .optional()
    } else {
        transaction
            .query_row(
                "SELECT id FROM accounts WHERE domain IS NULL AND name=?1 COLLATE NOCASE",
                params![name],
                |row| row.get(0),
            )
            .optional()
    }?;
    let account_id = existing_id.unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    transaction.execute(
        "INSERT INTO accounts(id,name,domain,website_url,status,created_at,updated_at,saved_at)
         VALUES(?1,?2,?3,?4,'new',?5,?5,?5)
         ON CONFLICT(domain) DO UPDATE SET
           name=CASE WHEN excluded.name='' THEN accounts.name ELSE excluded.name END,
           website_url=COALESCE(excluded.website_url,accounts.website_url),
           saved_at=COALESCE(accounts.saved_at,excluded.saved_at), updated_at=excluded.updated_at",
        params![account_id, name, domain, website_url, timestamp],
    )?;

    if is_public_index {
        if let Some(source_url) = seed.website_url.as_deref() {
            upsert_evidence(
                transaction,
                &account_id,
                source_url,
                if source_url.contains("sec.gov") {
                    "sec_record"
                } else {
                    "usaspending_record"
                },
                "Imported authoritative public record",
                &format!("The user selected the public record for {name} as an account seed."),
                "high",
            )?;
        }
    }

    let normalized_person_name = seed
        .person_name
        .as_deref()
        .map(|name| name.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|name| !name.is_empty());
    let person_id = normalized_person_name.as_deref().map(|name| {
        let id = Uuid::new_v4().to_string();
        transaction.execute(
            "INSERT INTO people(id,account_id,name,title) VALUES(?1,?2,?3,?4)
             ON CONFLICT(account_id,name) DO UPDATE SET title=COALESCE(excluded.title,people.title)",
            params![id, account_id, name, seed.person_title],
        )?;
        let actual = transaction.query_row(
            "SELECT id FROM people WHERE account_id=?1 AND name=?2 COLLATE NOCASE",
            params![account_id, name],
            |row| row.get::<_, String>(0),
        )?;
        Ok::<String, rusqlite::Error>(actual)
    }).transpose()?;

    let imported = [
        seed.work_email
            .as_deref()
            .and_then(normalize_email)
            .map(|value| {
                let is_role = value.split('@').next().is_some_and(|local| {
                    matches!(local, "info" | "sales" | "hello" | "contact" | "support")
                });
                (value, if is_role { "role_email" } else { "work_email" })
            }),
        seed.business_phone
            .as_deref()
            .and_then(normalize_us_phone)
            .map(|value| {
                (
                    value,
                    if person_id.is_some() {
                        "work_direct"
                    } else {
                        "company_main"
                    },
                )
            }),
    ];
    for contact in imported.into_iter().flatten() {
        let source_url = format!("urn:user-import:{account_id}");
        let evidence_id = upsert_evidence(
            transaction,
            &account_id,
            &source_url,
            "user_import",
            "User-supplied seed data",
            &format!("Imported business contact: {}", contact.0),
            "medium",
        )?;
        transaction.execute(
            "INSERT OR IGNORE INTO contacts(id,account_id,person_id,kind,value,evidence_id,confidence)
             VALUES(?1,?2,?3,?4,?5,?6,'medium')",
            params![Uuid::new_v4().to_string(), account_id, person_id, contact.1, contact.0, evidence_id],
        )?;
    }
    Ok(())
}

pub fn upsert_evidence(
    connection: &Connection,
    account_id: &str,
    source_url: &str,
    source_kind: &str,
    title: &str,
    excerpt: &str,
    confidence: &str,
) -> AppResult<String> {
    let clean_excerpt: String = excerpt.chars().take(500).collect();
    let content_hash = format!("{:x}", Sha256::digest(clean_excerpt.as_bytes()));
    let existing: Option<String> = connection
        .query_row(
            "SELECT id FROM evidence WHERE account_id=?1 AND source_url=?2 AND content_hash=?3",
            params![account_id, source_url, content_hash],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let id = Uuid::new_v4().to_string();
    connection.execute(
        "INSERT INTO evidence(id,account_id,source_url,source_kind,title,excerpt,retrieved_at,content_hash,confidence,freshness)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,'fresh')",
        params![id, account_id, source_url, source_kind, title, clean_excerpt, now(), content_hash, confidence],
    )?;
    Ok(id)
}

pub fn add_signal(
    connection: &Connection,
    account_id: &str,
    category: &str,
    summary: &str,
    evidence_id: &str,
    confidence: &str,
) -> AppResult<()> {
    connection.execute(
        "INSERT OR IGNORE INTO signals(id,account_id,category,summary,evidence_id,confidence)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            Uuid::new_v4().to_string(),
            account_id,
            category,
            summary,
            evidence_id,
            confidence
        ],
    )?;
    Ok(())
}

pub fn set_evidence_http_metadata(
    connection: &Connection,
    evidence_id: &str,
    etag: Option<&str>,
    modified_at: Option<&str>,
) -> AppResult<()> {
    connection.execute(
        "UPDATE evidence SET etag=COALESCE(?2,etag),modified_at=COALESCE(?3,modified_at) WHERE id=?1",
        params![evidence_id, etag, modified_at],
    )?;
    Ok(())
}

pub fn add_contact(
    connection: &Connection,
    account_id: &str,
    kind: &str,
    value: &str,
    evidence_id: &str,
    confidence: &str,
) -> AppResult<()> {
    if !matches!(
        kind,
        "company_main" | "work_direct" | "role_email" | "work_email"
    ) {
        return Err(AppError::Message(
            "Unsupported contact classification.".into(),
        ));
    }
    let value = if kind.ends_with("email") {
        normalize_email(value)
    } else {
        normalize_us_phone(value)
    }
    .ok_or_else(|| AppError::Message("Contact value failed normalization.".into()))?;
    connection.execute(
        "INSERT OR IGNORE INTO contacts(id,account_id,kind,value,evidence_id,confidence)
         VALUES(?1,?2,?3,?4,?5,?6)",
        params![
            Uuid::new_v4().to_string(),
            account_id,
            kind,
            value,
            evidence_id,
            confidence
        ],
    )?;
    Ok(())
}

pub fn mark_researched(connection: &Connection, account_id: &str) -> AppResult<()> {
    let timestamp = now();
    connection.execute(
        "UPDATE accounts SET status='researched', researched_at=?2, updated_at=?2 WHERE id=?1",
        params![account_id, timestamp],
    )?;
    Ok(())
}

pub fn update_quality_flags(connection: &Connection, account_id: &str) -> AppResult<()> {
    let records = {
        let sql = format!(
            "SELECT id,source_kind,excerpt,{DERIVED_FRESHNESS} FROM evidence WHERE account_id=?1"
        );
        let mut statement = connection.prepare(&sql)?;
        let rows = statement.query_map(params![account_id], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, String>(3)?,
            ))
        })?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    // Compiled once; this runs for every account touched by research.
    static EMPLOYEE_PATTERN: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
        regex::Regex::new(r"(?i)\b([0-9][0-9,]{1,8})\s*\+?\s+employees\b").unwrap()
    });
    let employee_pattern = &*EMPLOYEE_PATTERN;
    let employee_values = records
        .iter()
        .filter_map(|(_, _, excerpt, _)| {
            employee_pattern
                .captures(excerpt)
                .and_then(|capture| capture.get(1))
                .map(|value| value.as_str().replace(',', ""))
        })
        .collect::<std::collections::HashSet<_>>();
    let employee_conflict = employee_values.len() > 1;
    for (id, source_kind, excerpt, freshness) in records {
        let mut flags = Vec::new();
        if freshness == "stale" {
            flags.push("stale");
        }
        if source_kind != "user_import" && excerpt.trim().chars().count() < 48 {
            flags.push("incomplete");
        }
        if employee_conflict && employee_pattern.is_match(&excerpt) {
            flags.push("contradictory");
        }
        connection.execute(
            "UPDATE evidence SET quality_flags=?2 WHERE id=?1",
            params![id, serde_json::to_string(&flags)?],
        )?;
    }
    Ok(())
}

pub fn get_package(connection: &Connection, account_id: &str) -> AppResult<ProspectPackage> {
    get_package_for_run(connection, account_id, None)
}

pub fn get_package_for_run(
    connection: &Connection,
    account_id: &str,
    search_run_id: Option<&str>,
) -> AppResult<ProspectPackage> {
    let account = get_account(connection, account_id)?;
    let people = query_people(connection, account_id)?;
    let contacts = query_contacts(connection, account_id)?;
    let evidence = query_evidence(connection, account_id)?;
    let signals = query_signals(connection, account_id)?;
    let package_json: Option<(Option<String>, Option<String>, Option<String>)> = connection
        .query_row(
            "SELECT assessment_json,draft_json,call_brief_json FROM packages WHERE account_id=?1",
            params![account_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let (assessment, draft, call_brief) = if let Some((assessment, draft, brief)) = package_json {
        (
            assessment
                .map(|json| serde_json::from_str(&json))
                .transpose()?,
            draft.map(|json| serde_json::from_str(&json)).transpose()?,
            brief.map(|json| serde_json::from_str(&json)).transpose()?,
        )
    } else {
        (None, None, None)
    };
    let suppressed = connection.query_row(
        "SELECT EXISTS(SELECT 1 FROM suppressions WHERE account_id=?1)",
        params![account_id],
        |row| row.get::<_, bool>(0),
    )?;
    let latest_outcome = connection
        .query_row(
            "SELECT outcome FROM outcomes WHERE account_id=?1 ORDER BY created_at DESC LIMIT 1",
            params![account_id],
            |row| row.get(0),
        )
        .optional()?;
    let context_row: Option<DiscoveryContextDbRow> = if let Some(run_id) = search_run_id {
        connection.query_row(
            "SELECT ra.search_run_id,r.query,ra.discovery_source,ra.discovery_reason,ra.state,ra.rejection_reasons_json,ra.matched_features_json,ra.evidence_confidence,ra.rank,ra.assessment_json
             FROM search_run_accounts ra JOIN search_runs r ON r.id=ra.search_run_id
             WHERE ra.account_id=?1 AND ra.search_run_id=?2",
            params![account_id, run_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)),
        ).optional()?
    } else {
        connection.query_row(
            "SELECT ra.search_run_id,r.query,ra.discovery_source,ra.discovery_reason,ra.state,ra.rejection_reasons_json,ra.matched_features_json,ra.evidence_confidence,ra.rank,ra.assessment_json
             FROM search_run_accounts ra JOIN search_runs r ON r.id=ra.search_run_id
             WHERE ra.account_id=?1 ORDER BY r.started_at DESC LIMIT 1",
            params![account_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?)),
        ).optional()?
    };
    let discovery_context = context_row
        .map(
            |(
                search_run_id,
                query,
                discovery_source,
                discovery_reason,
                state,
                rejections,
                features,
                evidence_confidence,
                rank,
                assessment,
            )|
             -> AppResult<DiscoveryContext> {
                let result_tier = connection.query_row(
            "SELECT result_tier FROM search_run_accounts WHERE search_run_id=?1 AND account_id=?2",
            params![search_run_id, account_id],
            |row| row.get(0),
        )?;
                Ok(DiscoveryContext {
                    search_run_id,
                    query,
                    discovery_source,
                    discovery_reason,
                    state,
                    rejection_reasons: serde_json::from_str(&rejections).unwrap_or_default(),
                    matched_features: serde_json::from_str(&features).unwrap_or_default(),
                    evidence_confidence,
                    result_tier,
                    rank,
                    assessment: assessment
                        .map(|json| serde_json::from_str(&json))
                        .transpose()?,
                })
            },
        )
        .transpose()?;
    Ok(ProspectPackage {
        account,
        people,
        contacts,
        evidence,
        signals,
        assessment,
        draft,
        call_brief,
        suppressed,
        latest_outcome,
        discovery_context,
    })
}

fn query_people(connection: &Connection, account_id: &str) -> AppResult<Vec<Person>> {
    let mut statement = connection
        .prepare("SELECT id,account_id,name,title FROM people WHERE account_id=?1 ORDER BY name")?;
    let rows = statement.query_map(params![account_id], |row| {
        Ok(Person {
            id: row.get(0)?,
            account_id: row.get(1)?,
            name: row.get(2)?,
            title: row.get(3)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn query_contacts(connection: &Connection, account_id: &str) -> AppResult<Vec<ContactPoint>> {
    let mut statement = connection.prepare("SELECT id,account_id,person_id,kind,value,evidence_id,confidence FROM contacts WHERE account_id=?1 ORDER BY kind,value")?;
    let rows = statement.query_map(params![account_id], |row| {
        Ok(ContactPoint {
            id: row.get(0)?,
            account_id: row.get(1)?,
            person_id: row.get(2)?,
            kind: row.get(3)?,
            value: row.get(4)?,
            evidence_id: row.get(5)?,
            confidence: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

/// Freshness derived from `retrieved_at`.
///
/// Kept in one place so the read path and the quality-flag pass cannot
/// disagree. The stored `freshness` column is retained only so exports written
/// by older versions remain importable; it is no longer the source of truth.
const DERIVED_FRESHNESS: &str = "CASE
   WHEN julianday('now')-julianday(retrieved_at) > 90 THEN 'stale'
   WHEN julianday('now')-julianday(retrieved_at) > 30 THEN 'aging'
   ELSE 'fresh' END";

fn query_evidence(connection: &Connection, account_id: &str) -> AppResult<Vec<Evidence>> {
    let sql = format!("SELECT id,account_id,source_url,source_kind,title,excerpt,retrieved_at,published_at,content_hash,confidence,{DERIVED_FRESHNESS},quality_flags FROM evidence WHERE account_id=?1 ORDER BY retrieved_at DESC");
    let mut statement = connection.prepare(&sql)?;
    let rows = statement.query_map(params![account_id], |row| {
        let flags: String = row.get(11)?;
        Ok(Evidence {
            id: row.get(0)?,
            account_id: row.get(1)?,
            source_url: row.get(2)?,
            source_kind: row.get(3)?,
            title: row.get(4)?,
            excerpt: row.get(5)?,
            retrieved_at: row.get(6)?,
            published_at: row.get(7)?,
            content_hash: row.get(8)?,
            confidence: row.get(9)?,
            freshness: row.get(10)?,
            quality_flags: serde_json::from_str(&flags).unwrap_or_default(),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

fn query_signals(connection: &Connection, account_id: &str) -> AppResult<Vec<Signal>> {
    let mut statement = connection.prepare("SELECT id,account_id,category,summary,occurred_at,evidence_id,confidence FROM signals WHERE account_id=?1 ORDER BY rowid DESC")?;
    let rows = statement.query_map(params![account_id], |row| {
        Ok(Signal {
            id: row.get(0)?,
            account_id: row.get(1)?,
            category: row.get(2)?,
            summary: row.get(3)?,
            occurred_at: row.get(4)?,
            evidence_id: row.get(5)?,
            confidence: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn save_generated_package(
    connection: &Connection,
    account_id: &str,
    assessment: &FitAssessment,
    draft: &DraftArtifact,
    brief: &CallBrief,
    evidence_ids: &[String],
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO draft_artifacts(id,account_id,json,created_at) VALUES(?1,?2,?3,?4)
         ON CONFLICT(id) DO UPDATE SET json=excluded.json,created_at=excluded.created_at",
        params![
            draft.id,
            account_id,
            serde_json::to_string(draft)?,
            draft.created_at
        ],
    )?;
    connection.execute(
        "INSERT INTO packages(account_id,assessment_json,draft_json,call_brief_json,evidence_set_json,model_tag,model_digest,prompt_version,updated_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)
         ON CONFLICT(account_id) DO UPDATE SET assessment_json=excluded.assessment_json,draft_json=excluded.draft_json,
         call_brief_json=excluded.call_brief_json,evidence_set_json=excluded.evidence_set_json,model_tag=excluded.model_tag,
         model_digest=excluded.model_digest,prompt_version=excluded.prompt_version,updated_at=excluded.updated_at",
        params![
            account_id,
            serde_json::to_string(assessment)?,
            serde_json::to_string(draft)?,
            serde_json::to_string(brief)?,
            serde_json::to_string(evidence_ids)?,
            draft.model_tag,
            draft.model_digest,
            draft.prompt_version,
            now()
        ],
    )?;
    connection.execute(
        "UPDATE accounts SET score=?2,score_label=?3,status='needs_review',updated_at=?4 WHERE id=?1",
        params![account_id, assessment.total, score_label(assessment.total), now()],
    )?;
    Ok(())
}

pub fn complete_generated_job(
    connection: &mut Connection,
    job_id: &str,
    account_id: &str,
    assessment: &FitAssessment,
    draft: &DraftArtifact,
    brief: &CallBrief,
    evidence_ids: &[String],
) -> AppResult<()> {
    let transaction = connection.transaction()?;
    save_generated_package(
        &transaction,
        account_id,
        assessment,
        draft,
        brief,
        evidence_ids,
    )?;
    transaction.execute(
        "UPDATE accounts SET saved_at=COALESCE(saved_at,?2),updated_at=?2 WHERE id=?1",
        params![account_id, now()],
    )?;
    transaction.execute(
        "UPDATE jobs SET status='complete',progress_percent=100,progress_stage='Complete',last_error=NULL,completed_at=?2 WHERE id=?1",
        params![job_id, now()],
    )?;
    transaction.commit()?;
    Ok(())
}

pub fn set_outcome(connection: &Connection, account_id: &str, outcome: &str) -> AppResult<()> {
    let allowed = [
        "contacted",
        "replied",
        "meeting",
        "not_interested",
        "wrong_fit",
        "do_not_contact",
    ];
    if !allowed.contains(&outcome) {
        return Err(AppError::Message("Unknown outcome.".into()));
    }
    get_account(connection, account_id)?;
    connection.execute(
        "INSERT INTO outcomes(id,account_id,outcome,created_at) VALUES(?1,?2,?3,?4)",
        params![Uuid::new_v4().to_string(), account_id, outcome, now()],
    )?;
    if outcome == "do_not_contact" {
        set_suppression(connection, account_id, true)?;
    }
    Ok(())
}

pub fn set_suppression(
    connection: &Connection,
    account_id: &str,
    suppressed: bool,
) -> AppResult<()> {
    get_account(connection, account_id)?;
    if suppressed {
        connection.execute(
            "INSERT INTO suppressions(account_id,reason,created_at) VALUES(?1,'manual',?2)
             ON CONFLICT(account_id) DO NOTHING",
            params![account_id, now()],
        )?;
        connection.execute(
            "UPDATE accounts SET status='suppressed' WHERE id=?1",
            params![account_id],
        )?;
    } else {
        connection.execute(
            "DELETE FROM suppressions WHERE account_id=?1",
            params![account_id],
        )?;
        connection.execute("UPDATE accounts SET status=CASE WHEN researched_at IS NULL THEN 'new' ELSE 'researched' END WHERE id=?1", params![account_id])?;
    }
    Ok(())
}

pub fn due_for_daily_scan(connection: &Connection) -> AppResult<bool> {
    let last: Option<String> = connection
        .query_row(
            "SELECT value FROM settings WHERE key='last_daily_scan'",
            [],
            |row| row.get(0),
        )
        .optional()?;
    let due = last
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(&value).ok())
        .map_or(true, |timestamp| {
            timestamp < Utc::now() - Duration::hours(20)
        });
    Ok(due)
}

pub fn queue_daily_scan(connection: &Connection) -> AppResult<i64> {
    let ids = {
        let mut statement =
            connection.prepare("SELECT id FROM accounts WHERE saved_at IS NOT NULL AND trashed_at IS NULL AND status != 'suppressed' AND NOT EXISTS(SELECT 1 FROM suppressions s WHERE s.account_id=accounts.id)")?;
        let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, _>>()?
    };
    let timestamp = now();
    for account_id in &ids {
        connection.execute(
            "INSERT INTO jobs(id,account_id,kind,status,scheduled_at) VALUES(?1,?2,'public_refresh','pending',?3)",
            params![Uuid::new_v4().to_string(), account_id, timestamp],
        )?;
    }
    connection.execute(
        "INSERT INTO settings(key,value) VALUES('last_daily_scan',?1) ON CONFLICT(key) DO UPDATE SET value=excluded.value",
        params![timestamp],
    )?;
    Ok(ids.len() as i64)
}

pub fn list_jobs(connection: &Connection) -> AppResult<Vec<ResearchJob>> {
    let mut statement = connection.prepare("SELECT id,account_id,kind,status,attempts,last_error,scheduled_at,completed_at,progress_percent,progress_stage,request_json FROM jobs ORDER BY scheduled_at DESC")?;
    let rows = statement.query_map([], |row| {
        let request_json: Option<String> = row.get(10)?;
        Ok(ResearchJob {
            id: row.get(0)?,
            account_id: row.get(1)?,
            kind: row.get(2)?,
            status: row.get(3)?,
            attempts: row.get(4)?,
            last_error: row.get(5)?,
            scheduled_at: row.get(6)?,
            completed_at: row.get(7)?,
            progress_percent: row.get(8)?,
            progress_stage: row.get(9)?,
            request: request_json.and_then(|json| serde_json::from_str(&json).ok()),
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_job(connection: &Connection, job_id: &str) -> AppResult<ResearchJob> {
    list_jobs(connection)?
        .into_iter()
        .find(|job| job.id == job_id)
        .ok_or_else(|| AppError::Message("Local job not found.".into()))
}

pub fn latest_search_state(connection: &Connection, account_id: &str) -> AppResult<Option<String>> {
    connection.query_row(
        "SELECT ra.state FROM search_run_accounts ra JOIN search_runs r ON r.id=ra.search_run_id WHERE ra.account_id=?1 ORDER BY r.started_at DESC LIMIT 1",
        params![account_id],
        |row| row.get(0),
    ).optional().map_err(AppError::from)
}

pub fn recover_interrupted_jobs(connection: &Connection) -> AppResult<usize> {
    Ok(connection.execute(
        "UPDATE jobs SET status='pending' WHERE status='running'",
        [],
    )?)
}

pub fn save_search_brief(connection: &Connection, brief: &SearchBrief) -> AppResult<()> {
    if brief.query.trim().len() < 12 {
        return Err(AppError::Message(
            "Describe the product and desired customer in at least twelve characters.".into(),
        ));
    }
    if !(1..=50).contains(&brief.result_limit)
        || !(1..=100).contains(&brief.qualification_threshold)
        || !(0..=20).contains(&brief.quality_band)
    {
        return Err(AppError::Message(
            "Search result limit or qualification threshold is outside the supported range.".into(),
        ));
    }
    connection.execute(
        "INSERT INTO search_briefs(id,query,json,approved,created_at) VALUES(?1,?2,?3,?4,?5)
         ON CONFLICT(id) DO UPDATE SET query=excluded.query,json=excluded.json,approved=excluded.approved",
        params![
            brief.id,
            brief.query,
            serde_json::to_string(brief)?,
            brief.approved,
            brief.created_at
        ],
    )?;
    Ok(())
}

pub fn get_search_brief(connection: &Connection, brief_id: &str) -> AppResult<SearchBrief> {
    let json: Option<String> = connection
        .query_row(
            "SELECT json FROM search_briefs WHERE id=?1",
            params![brief_id],
            |row| row.get(0),
        )
        .optional()?;
    json.map(|value| serde_json::from_str(&value).map_err(AppError::from))
        .transpose()?
        .ok_or_else(|| AppError::Message("Search brief not found.".into()))
}

pub fn list_search_briefs(connection: &Connection) -> AppResult<Vec<SearchBrief>> {
    let mut statement = connection.prepare("SELECT json FROM search_briefs ORDER BY created_at")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    rows.map(|row| {
        let json = row?;
        serde_json::from_str(&json).map_err(AppError::from)
    })
    .collect()
}

pub fn list_source_identities(connection: &Connection) -> AppResult<Vec<SourceIdentity>> {
    let mut statement = connection.prepare(
        "SELECT id,account_id,source_name,source_external_id,source_record_url,identity_confidence,created_at FROM source_identities ORDER BY created_at",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(SourceIdentity {
            id: row.get(0)?,
            account_id: row.get(1)?,
            source_name: row.get(2)?,
            source_external_id: row.get(3)?,
            source_record_url: row.get(4)?,
            identity_confidence: row.get(5)?,
            created_at: row.get(6)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn create_search_run(connection: &Connection, brief: &SearchBrief) -> AppResult<SearchRun> {
    if !brief.approved {
        return Err(AppError::Message(
            "Approve the interpreted search plan before discovery begins.".into(),
        ));
    }
    if let Some(active_id) = connection
        .query_row(
            "SELECT id FROM search_runs WHERE status='running' ORDER BY started_at DESC LIMIT 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
    {
        return Err(AppError::Message(format!(
            "SEARCH_CONFLICT:{active_id}:Another automatic search is already running."
        )));
    }
    save_search_brief(connection, brief)?;
    let id = Uuid::new_v4().to_string();
    let profile_json = load_offer_profile(connection)?
        .map(|profile| serde_json::to_string(&profile))
        .transpose()?;
    connection.execute(
        "INSERT INTO search_runs(id,brief_id,query,status,result_limit,offer_profile_json,model_tag,model_digest,prompt_version,source_versions_json,started_at)
         VALUES(?1,?2,?3,'running',?4,?5,?6,?7,?8,'{\"contracts\":3}',?9)",
        params![
            id,
            brief.id,
            brief.query,
            brief.result_limit,
            profile_json,
            brief.model_tag.as_deref().unwrap_or("manual-rules:v1"),
            brief.model_digest.as_deref(),
            brief.prompt_version.as_str(),
            now()
        ],
    )?;
    get_search_run(connection, &id)
}

pub fn list_search_runs(connection: &Connection) -> AppResult<Vec<SearchRunSummary>> {
    let mut statement = connection.prepare(
        "SELECT r.id,r.query,r.status,r.progress_completed,r.progress_total,r.progress_percent,r.progress_stage,
                SUM(CASE WHEN a.state='qualified' THEN 1 ELSE 0 END),
                SUM(CASE WHEN a.state='needs_review' THEN 1 ELSE 0 END),r.strong_count,r.started_at
         FROM search_runs r LEFT JOIN search_run_accounts a ON a.search_run_id=r.id
         GROUP BY r.id ORDER BY r.started_at DESC LIMIT 30",
    )?;
    let rows = statement.query_map([], |row| {
        Ok(SearchRunSummary {
            id: row.get(0)?,
            query: row.get(1)?,
            status: row.get(2)?,
            progress_completed: row.get(3)?,
            progress_total: row.get(4)?,
            progress_percent: row.get(5)?,
            progress_stage: row.get(6)?,
            qualified_count: row.get::<_, Option<i64>>(7)?.unwrap_or(0),
            review_count: row.get::<_, Option<i64>>(8)?.unwrap_or(0),
            strong_count: row.get(9)?,
            started_at: row.get(10)?,
        })
    })?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn running_search_run_ids(connection: &Connection) -> AppResult<Vec<String>> {
    let mut statement = connection
        .prepare("SELECT id FROM search_runs WHERE status='running' ORDER BY started_at")?;
    let rows = statement.query_map([], |row| row.get::<_, String>(0))?;
    Ok(rows.collect::<Result<Vec<_>, _>>()?)
}

pub fn get_search_run(connection: &Connection, run_id: &str) -> AppResult<SearchRun> {
    let base: Option<SearchRunDbRow> = connection
        .query_row(
            "SELECT id,query,brief_id,status,progress_completed,progress_total,progress_percent,progress_stage,result_limit,candidate_count,researched_count,strong_count,near_match_count,warnings_json,started_at,completed_at,last_error FROM search_runs WHERE id=?1",
            params![run_id],
            |row| Ok((row.get(0)?,row.get(1)?,row.get(2)?,row.get(3)?,row.get(4)?,row.get(5)?,row.get(6)?,row.get(7)?,row.get(8)?,row.get(9)?,row.get(10)?,row.get(11)?,row.get(12)?,row.get(13)?,row.get(14)?,row.get(15)?,row.get(16)?)),
        )
        .optional()?;
    let (
        id,
        query,
        brief_id,
        status,
        progress_completed,
        progress_total,
        progress_percent,
        progress_stage,
        result_limit,
        candidate_count,
        researched_count,
        strong_count,
        near_match_count,
        warnings_json,
        started_at,
        completed_at,
        last_error,
    ) = base.ok_or_else(|| AppError::Message("Search run not found.".into()))?;
    let mut statement = connection.prepare(
        "SELECT a.id,a.name,a.domain,a.website_url,a.industry,a.employee_band,a.location,a.status,a.score,a.score_label,a.researched_at,a.created_at,a.saved_at,a.favorited_at,a.trashed_at,
                ra.state,ra.discovery_source,ra.discovery_reason,ra.score,ra.evidence_confidence,ra.rejection_reasons_json,ra.matched_features_json,ra.assessment_json,ra.rank,ra.result_tier
         FROM search_run_accounts ra JOIN accounts a ON a.id=ra.account_id WHERE ra.search_run_id=?1
         ORDER BY CASE ra.result_tier WHEN 'strong' THEN 0 WHEN 'near_match' THEN 1 ELSE 2 END,COALESCE(ra.rank,9999),COALESCE(ra.score,-1) DESC,ra.created_at",
    )?;
    let rows = statement.query_map(params![run_id], |row| {
        let account = account_from_row(row)?;
        Ok((
            account,
            row.get::<_, String>(15)?,
            row.get::<_, String>(16)?,
            row.get::<_, String>(17)?,
            row.get::<_, Option<i64>>(18)?,
            row.get::<_, String>(19)?,
            row.get::<_, String>(20)?,
            row.get::<_, String>(21)?,
            row.get::<_, Option<String>>(22)?,
            row.get::<_, Option<i64>>(23)?,
            row.get::<_, String>(24)?,
        ))
    })?;
    let accounts = rows
        .map(|result| {
            let (
                account,
                state,
                discovery_source,
                discovery_reason,
                score,
                evidence_confidence,
                rejection_json,
                features_json,
                assessment_json,
                rank,
                result_tier,
            ) = result?;
            Ok(SearchRunAccount {
                account,
                state,
                discovery_source,
                discovery_reason,
                score,
                evidence_confidence,
                rejection_reasons: serde_json::from_str(&rejection_json).unwrap_or_default(),
                matched_features: serde_json::from_str(&features_json).unwrap_or_default(),
                assessment: assessment_json.and_then(|json| serde_json::from_str(&json).ok()),
                rank,
                result_tier,
            })
        })
        .collect::<Result<Vec<_>, rusqlite::Error>>()?;
    Ok(SearchRun {
        id,
        query,
        brief_id: brief_id.unwrap_or_default(),
        status,
        progress_completed,
        progress_total,
        progress_percent,
        progress_stage,
        result_limit,
        candidate_count,
        researched_count,
        strong_count,
        near_match_count,
        warnings: serde_json::from_str(&warnings_json).unwrap_or_default(),
        started_at,
        completed_at,
        last_error,
        accounts,
    })
}

/// Counters and status for a run, without touching `search_run_accounts`.
///
/// This is the read behind every progress tick, so it must stay O(1) in the
/// size of the candidate pool.
pub fn get_search_run_progress(
    connection: &Connection,
    run_id: &str,
) -> AppResult<SearchRunProgress> {
    connection
        .query_row(
            "SELECT id,status,progress_completed,progress_total,progress_percent,progress_stage,result_limit,candidate_count,researched_count,strong_count,near_match_count
             FROM search_runs WHERE id=?1",
            params![run_id],
            |row| {
                let status: String = row.get(1)?;
                Ok(SearchRunProgress {
                    id: row.get(0)?,
                    finished: matches!(status.as_str(), "complete" | "failed" | "cancelled"),
                    status,
                    progress_completed: row.get(2)?,
                    progress_total: row.get(3)?,
                    progress_percent: row.get(4)?,
                    progress_stage: row.get(5)?,
                    result_limit: row.get(6)?,
                    candidate_count: row.get(7)?,
                    researched_count: row.get(8)?,
                    strong_count: row.get(9)?,
                    near_match_count: row.get(10)?,
                })
            },
        )
        .optional()?
        .ok_or_else(|| AppError::Message("Search run not found.".into()))
}

pub fn set_search_run_progress(
    connection: &Connection,
    run_id: &str,
    completed: i64,
    total: i64,
) -> AppResult<()> {
    connection.execute(
        "UPDATE search_runs SET progress_completed=?2,progress_total=?3 WHERE id=?1 AND status='running'",
        params![run_id, completed, total],
    )?;
    Ok(())
}

pub fn set_search_run_stage(
    connection: &Connection,
    run_id: &str,
    percent: i64,
    stage: &str,
) -> AppResult<()> {
    connection.execute(
        "UPDATE search_runs SET progress_percent=?2,progress_stage=?3 WHERE id=?1 AND status='running'",
        params![
            run_id,
            percent.clamp(0, 99),
            stage.chars().take(160).collect::<String>()
        ],
    )?;
    Ok(())
}

pub fn finish_search_run(
    connection: &Connection,
    run_id: &str,
    status: &str,
    error: Option<&str>,
) -> AppResult<()> {
    if !matches!(status, "complete" | "failed" | "cancelled") {
        return Err(AppError::Message(
            "Invalid terminal search-run status.".into(),
        ));
    }
    connection.execute(
        "UPDATE search_runs SET status=?2,completed_at=?3,last_error=?4,
         progress_percent=CASE WHEN ?2='complete' THEN 100 ELSE progress_percent END,
         progress_stage=CASE WHEN ?2='complete' THEN 'Complete' WHEN ?2='failed' THEN 'Failed' ELSE progress_stage END
         WHERE id=?1",
        params![
            run_id,
            status,
            now(),
            error.map(|value| value.chars().take(500).collect::<String>())
        ],
    )?;
    Ok(())
}

pub fn cancel_search_run(connection: &Connection, run_id: &str) -> AppResult<SearchRun> {
    connection.execute(
        "UPDATE search_runs SET status='cancelled',completed_at=?2,progress_stage='Cancelled' WHERE id=?1 AND status='running'",
        params![run_id, now()],
    )?;
    get_search_run(connection, run_id)
}

/// How many searches ran before this one.
///
/// Used to rotate which bounded regions a run samples first, so repeating a
/// search explores new ground instead of re-reading one bounding box.
pub fn search_rotation(connection: &Connection, run_id: &str) -> AppResult<usize> {
    let count: i64 = connection.query_row(
        "SELECT count(*) FROM search_runs WHERE started_at < (SELECT started_at FROM search_runs WHERE id=?1)",
        params![run_id],
        |row| row.get(0),
    )?;
    Ok(count.max(0) as usize)
}

pub fn search_run_is_cancelled(connection: &Connection, run_id: &str) -> AppResult<bool> {
    Ok(connection.query_row(
        "SELECT status='cancelled' FROM search_runs WHERE id=?1",
        params![run_id],
        |row| row.get(0),
    )?)
}

pub fn import_discovery_result(
    connection: &mut Connection,
    run_id: &str,
    result: &DiscoveryResult,
) -> AppResult<String> {
    let transaction = connection.transaction()?;
    let candidate_domain = result
        .domain
        .as_deref()
        .and_then(normalize_domain)
        .or_else(|| result.website_url.as_deref().and_then(normalize_domain));
    let public_index = candidate_domain.as_deref().is_some_and(|domain| {
        matches!(domain, "sec.gov" | "usaspending.gov" | "openstreetmap.org")
    });
    let domain = (!public_index).then_some(candidate_domain).flatten();
    let company_website = result
        .website_url
        .as_deref()
        .and_then(normalize_public_url)
        .filter(|url| normalize_domain(url).is_some_and(|url_domain| Some(url_domain) == domain));
    let source_name = result.source.trim();
    let existing_by_source: Option<String> = transaction
        .query_row(
            "SELECT account_id FROM source_identities WHERE source_name=?1 AND source_record_url=?2",
            params![source_name, result.source_url],
            |row| row.get(0),
        )
        .optional()?;
    let existing_by_domain: Option<String> = domain
        .as_ref()
        .map(|value| {
            transaction
                .query_row(
                    "SELECT id FROM accounts WHERE domain=?1",
                    params![value],
                    |row| row.get(0),
                )
                .optional()
        })
        .transpose()?
        .flatten();
    let existing_by_name: Option<String> = if domain.is_none() {
        transaction
            .query_row(
                "SELECT id FROM accounts WHERE domain IS NULL AND name=?1 COLLATE NOCASE",
                params![result.company_name],
                |row| row.get(0),
            )
            .optional()?
    } else {
        None
    };
    let existing_id = existing_by_source
        .or(existing_by_domain)
        .or(existing_by_name);
    let account_id = existing_id
        .clone()
        .unwrap_or_else(|| Uuid::new_v4().to_string());
    let timestamp = now();
    if existing_id.is_some() {
        transaction.execute(
            "UPDATE accounts SET name=?2,domain=COALESCE(domain,?3),website_url=COALESCE(website_url,?4),location=COALESCE(location,?5),updated_at=?6 WHERE id=?1",
            params![account_id,result.company_name,domain,company_website,result.location,timestamp],
        )?;
    } else {
        transaction.execute(
            "INSERT INTO accounts(id,name,domain,website_url,location,status,created_at,updated_at)
             VALUES(?1,?2,?3,?4,?5,'new',?6,?6)",
            params![
                account_id,
                result.company_name,
                domain,
                company_website,
                result.location,
                timestamp
            ],
        )?;
    }
    transaction.execute(
        "INSERT OR IGNORE INTO source_identities(id,account_id,source_name,source_external_id,source_record_url,identity_confidence,created_at)
         VALUES(?1,?2,?3,?4,?5,?6,?7)",
        params![Uuid::new_v4().to_string(),account_id,source_name,result.source_external_id.as_ref().or(result.external_id.as_ref()),result.source_url,result.identity_confidence.as_deref().unwrap_or("medium"),timestamp],
    )?;
    let evidence_id = upsert_evidence(
        &transaction,
        &account_id,
        &result.source_url,
        &format!(
            "{}_record",
            source_name.to_ascii_lowercase().replace(' ', "_")
        ),
        &format!("{} discovery record", source_name),
        &format!("{}: {}", result.company_name, result.reason),
        result.identity_confidence.as_deref().unwrap_or("medium"),
    )?;
    if let Some(phone) = result.business_phone.as_deref() {
        let _ = add_contact(
            &transaction,
            &account_id,
            "company_main",
            phone,
            &evidence_id,
            "medium",
        );
    }
    transaction.execute(
        "INSERT INTO search_run_accounts(search_run_id,account_id,state,discovery_source,discovery_reason,evidence_confidence,created_at,updated_at)
         VALUES(?1,?2,'discovered',?3,?4,?5,?6,?6)
         ON CONFLICT(search_run_id,account_id) DO UPDATE SET discovery_reason=excluded.discovery_reason,updated_at=excluded.updated_at",
        params![run_id,account_id,source_name,result.reason,result.identity_confidence.as_deref().unwrap_or("medium"),timestamp],
    )?;
    transaction.commit()?;
    Ok(account_id)
}

pub fn update_search_run_account(
    connection: &Connection,
    run_id: &str,
    account_id: &str,
    update: SearchRunAccountUpdate<'_>,
) -> AppResult<()> {
    let assessment_ok = i64::from(update.assessment.is_some_and(assessment_is_promotable));
    connection.execute(
        "UPDATE search_run_accounts SET state=?3,score=?4,evidence_confidence=?5,rejection_reasons_json=?6,matched_features_json=?7,assessment_json=?8,rank=?9,result_tier=?10,assessment_ok=?11,updated_at=?12
         WHERE search_run_id=?1 AND account_id=?2",
        params![run_id,account_id,update.state,update.score,update.confidence,serde_json::to_string(update.rejection_reasons)?,serde_json::to_string(update.features)?,update.assessment.map(serde_json::to_string).transpose()?,update.rank,update.result_tier,assessment_ok,now()],
    )?;
    Ok(())
}

pub fn set_search_run_counts(
    connection: &Connection,
    run_id: &str,
    candidate_count: i64,
    researched_count: i64,
) -> AppResult<()> {
    connection.execute(
        "UPDATE search_runs SET candidate_count=?2,researched_count=?3 WHERE id=?1",
        params![run_id, candidate_count, researched_count],
    )?;
    Ok(())
}

pub fn add_search_warning(
    connection: &Connection,
    run_id: &str,
    warning: SourceWarning,
) -> AppResult<()> {
    let current: String = connection.query_row(
        "SELECT warnings_json FROM search_runs WHERE id=?1",
        params![run_id],
        |row| row.get(0),
    )?;
    let mut warnings: Vec<SourceWarning> = serde_json::from_str(&current).unwrap_or_default();
    warnings.push(warning);
    connection.execute(
        "UPDATE search_runs SET warnings_json=?2 WHERE id=?1",
        params![run_id, serde_json::to_string(&warnings)?],
    )?;
    Ok(())
}

pub fn finalize_search_ranking(
    connection: &Connection,
    run_id: &str,
    threshold: i64,
    quality_band: i64,
    result_limit: i64,
) -> AppResult<(i64, i64)> {
    // Reads only numeric and flag columns. This runs once per research batch
    // over the entire pool, so parsing every candidate's assessment here made
    // the total work quadratic in the candidate count.
    let mut statement = connection.prepare(
        "SELECT account_id,state,score,evidence_confidence,assessment_ok FROM search_run_accounts WHERE search_run_id=?1 ORDER BY COALESCE(score,-1) DESC,created_at",
    )?;
    let rows = statement.query_map(params![run_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, String>(1)?,
            row.get::<_, Option<i64>>(2)?,
            row.get::<_, String>(3)?,
            row.get::<_, i64>(4)? != 0,
        ))
    })?;
    let candidates = rows.collect::<Result<Vec<_>, _>>()?;
    let eligible = candidates
        .iter()
        .filter(|(_, state, score, confidence, assessment_ok)| {
            state == "qualified"
                && score.is_some_and(|value| value >= threshold)
                && matches!(confidence.as_str(), "medium" | "high")
                && *assessment_ok
        })
        .map(|(account_id, _, _, _, _)| account_id.clone())
        .collect::<std::collections::HashSet<_>>();
    // The band is a window on the single best eligible score, never a
    // candidate-to-candidate comparison. Chaining the comparison lets a long
    // gentle slope qualify as one cluster, 95, 84, 73 would all promote under
    // a 12-point band, a 22-point spread inside a "tight" tier. Strong Match
    // means "within `quality_band` of the best result", and README documents
    // that as a four-point span.
    let best = candidates
        .iter()
        .filter(|(account_id, _, _, _, _)| eligible.contains(account_id))
        .filter_map(|(_, _, score, _, _)| *score)
        .max();
    let mut rank = 0_i64;
    let mut strong = 0_i64;
    let mut near = 0_i64;
    for (account_id, state, score, _, _) in candidates {
        let tier = if eligible.contains(&account_id)
            && state == "qualified"
            && score.is_some_and(|value| value >= threshold)
            && best.is_some_and(|top| top - score.unwrap_or_default() <= quality_band)
            && strong < result_limit
        {
            strong += 1;
            rank += 1;
            "strong"
        } else if matches!(state.as_str(), "qualified" | "needs_review") {
            near += 1;
            rank += 1;
            "near_match"
        } else {
            "excluded"
        };
        connection.execute(
            "UPDATE search_run_accounts SET result_tier=?3,rank=?4,updated_at=?5 WHERE search_run_id=?1 AND account_id=?2",
            params![run_id, account_id, tier, (tier != "excluded").then_some(rank), now()],
        )?;
    }
    connection.execute(
        "UPDATE search_runs SET strong_count=?2,near_match_count=?3 WHERE id=?1",
        params![run_id, strong, near],
    )?;
    Ok((strong, near))
}

pub fn cache_discoveries(
    connection: &Connection,
    cache_key: &str,
    source: &str,
    results: &[DiscoveryResult],
) -> AppResult<()> {
    connection.execute(
        "INSERT INTO discovery_cache(cache_key,source,payload_json,fetched_at) VALUES(?1,?2,?3,?4)
         ON CONFLICT(cache_key) DO UPDATE SET payload_json=excluded.payload_json,fetched_at=excluded.fetched_at",
        params![cache_key, source, serde_json::to_string(results)?, now()],
    )?;
    Ok(())
}

pub fn cached_discoveries(
    connection: &Connection,
    cache_key: &str,
) -> AppResult<Option<(Vec<DiscoveryResult>, i64)>> {
    let row: Option<(String, String)> = connection
        .query_row(
            "SELECT payload_json,fetched_at FROM discovery_cache WHERE cache_key=?1",
            params![cache_key],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .optional()?;
    row.map(|(json, fetched_at)| {
        let fetched = chrono::DateTime::parse_from_rfc3339(&fetched_at)
            .map_err(|_| AppError::Message("Discovery cache timestamp is invalid.".into()))?;
        let age = Utc::now()
            .signed_duration_since(fetched.with_timezone(&Utc))
            .num_hours()
            .max(0);
        Ok((serde_json::from_str(&json)?, age))
    })
    .transpose()
}

pub fn set_source_backoff(
    connection: &Connection,
    source: &str,
    seconds: i64,
    message: &str,
) -> AppResult<()> {
    let not_before = (Utc::now() + Duration::seconds(seconds.clamp(1, 86_400))).to_rfc3339();
    connection.execute(
        "INSERT INTO source_backoff(source,not_before,last_error) VALUES(?1,?2,?3)
         ON CONFLICT(source) DO UPDATE SET not_before=excluded.not_before,last_error=excluded.last_error",
        params![source, not_before, message.chars().take(500).collect::<String>()],
    )?;
    Ok(())
}

pub fn source_backoff(connection: &Connection, source: &str) -> AppResult<Option<String>> {
    connection.query_row(
        "SELECT last_error FROM source_backoff WHERE source=?1 AND datetime(not_before)>datetime('now')",
        params![source],
        |row| row.get(0),
    ).optional().map_err(AppError::from)
}

pub fn source_capabilities(connection: &Connection) -> AppResult<Vec<SourceCapability>> {
    let searxng = connection
        .query_row(
            "SELECT value FROM settings WHERE key='searxng_url'",
            [],
            |row| row.get::<_, String>(0),
        )
        .optional()?
        .is_some();
    Ok(vec![
        capability(
            "overpass",
            "OpenStreetMap Overpass",
            &["discover"],
            false,
            true,
            "Bounded, cached samples of businesses with a mapped physical premise, with OpenStreetMap attribution.",
        ),
        capability(
            "wikidata",
            "Open reference data (Wikidata)",
            &["discover"],
            false,
            true,
            "U.S. organizations of any described type that publish a website. Covers firms with no physical premise, which the map cannot reach.",
        ),
        capability(
            "company_web",
            "Company websites",
            &["refresh"],
            false,
            true,
            "Public pages, sitemaps, feeds, careers, leadership, and press pages.",
        ),
        capability(
            "sec",
            "SEC EDGAR",
            &["discover", "refresh"],
            false,
            true,
            "Public company records and filings.",
        ),
        capability(
            "usaspending",
            "USAspending",
            &["discover", "refresh"],
            false,
            true,
            "Federal award recipients and public award records.",
        ),
        capability(
            "gdelt",
            "GDELT",
            &["refresh"],
            false,
            true,
            "Recent public news coverage.",
        ),
        capability(
            "github",
            "GitHub",
            &["refresh"],
            false,
            true,
            "Unauthenticated public organization activity; rate limits apply.",
        ),
        capability(
            "greenhouse",
            "Greenhouse",
            &["refresh"],
            false,
            true,
            "Unauthenticated public job-board pages and endpoints.",
        ),
        capability(
            "lever",
            "Lever",
            &["refresh"],
            false,
            true,
            "Public job-board pages and postings.",
        ),
        capability(
            "searxng",
            "Local SearXNG",
            &["discover"],
            true,
            searxng,
            "Optional. Configure a local instance; the core workflow does not require it.",
        ),
    ])
}

fn capability(
    id: &str,
    name: &str,
    modes: &[&str],
    requires_configuration: bool,
    enabled: bool,
    detail: &str,
) -> SourceCapability {
    SourceCapability {
        id: id.into(),
        name: name.into(),
        modes: modes.iter().map(|item| (*item).into()).collect(),
        requires_configuration,
        enabled,
        detail: detail.into(),
    }
}

pub fn export_json(connection: &Connection) -> AppResult<serde_json::Value> {
    let search_runs = list_search_runs(connection)?
        .into_iter()
        .map(|run| get_search_run(connection, &run.id))
        .collect::<AppResult<Vec<_>>>()?;
    Ok(serde_json::json!({
        "format": "prospecting-workbench-export",
        "contractVersion": CONTRACT_VERSION,
        "exportedAt": now(),
        "salesPlay": load_sales_play(connection)?,
        "offerProfile": load_offer_profile(connection)?,
        "searchBriefs": list_search_briefs(connection)?,
        "searchRuns": search_runs,
        "sourceIdentities": list_source_identities(connection)?,
        "accounts": list_accounts(connection)?,
        "packages": list_accounts(connection)?.into_iter().map(|account| get_package(connection, &account.id)).collect::<AppResult<Vec<_>>>()?,
        "jobs": list_jobs(connection)?,
    }))
}

pub fn restore_json(connection: &mut Connection, payload: &str) -> AppResult<()> {
    if payload.len() > 25 * 1024 * 1024 {
        return Err(AppError::Message(
            "Import exceeds the 25 MB safety limit.".into(),
        ));
    }
    let document: serde_json::Value = serde_json::from_str(payload)?;
    let contract_version = document
        .get("contractVersion")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    if document.get("format").and_then(serde_json::Value::as_str)
        != Some("prospecting-workbench-export")
        || !matches!(
            document
                .get("contractVersion")
                .and_then(serde_json::Value::as_u64),
            Some(1..=3)
        )
    {
        return Err(AppError::Message(
            "This is not a compatible Prospecting Workbench export.".into(),
        ));
    }
    let sales_play: Option<SalesPlay> = document
        .get("salesPlay")
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    let offer_profile: Option<OfferProfile> = document
        .get("offerProfile")
        .filter(|value| !value.is_null())
        .cloned()
        .map(serde_json::from_value)
        .transpose()?;
    let search_briefs: Vec<SearchBrief> = serde_json::from_value(
        document
            .get("searchBriefs")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let search_runs: Vec<SearchRun> = serde_json::from_value(
        document
            .get("searchRuns")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let source_identities: Vec<SourceIdentity> = serde_json::from_value(
        document
            .get("sourceIdentities")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let packages: Vec<ProspectPackage> = serde_json::from_value(
        document
            .get("packages")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    let jobs: Vec<ResearchJob> = serde_json::from_value(
        document
            .get("jobs")
            .cloned()
            .unwrap_or_else(|| serde_json::json!([])),
    )?;
    for package in &packages {
        let evidence_ids = package
            .evidence
            .iter()
            .map(|item| item.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        if package
            .contacts
            .iter()
            .any(|contact| !evidence_ids.contains(contact.evidence_id.as_str()))
            || package
                .signals
                .iter()
                .any(|signal| !evidence_ids.contains(signal.evidence_id.as_str()))
        {
            return Err(AppError::Message(
                "Import contains a contact or signal with a missing evidence reference.".into(),
            ));
        }
    }

    let transaction = connection.transaction()?;
    delete_all(&transaction)?;
    if let Some(play) = sales_play {
        save_sales_play(&transaction, play)?;
    }
    if let Some(profile) = offer_profile {
        save_offer_profile(&transaction, profile)?;
    }
    for brief in &search_briefs {
        save_search_brief(&transaction, brief)?;
    }
    for package in packages {
        let evidence_set = package
            .evidence
            .iter()
            .map(|evidence| evidence.id.clone())
            .collect::<Vec<_>>();
        let mut account = package.account;
        if contract_version < 3 && account.saved_at.is_none() {
            account.saved_at = Some(account.created_at.clone());
        }
        transaction.execute(
            "INSERT INTO accounts(id,name,domain,website_url,industry,employee_band,location,status,score,score_label,researched_at,created_at,updated_at,saved_at,favorited_at,trashed_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16)",
            params![account.id,account.name,account.domain,account.website_url,account.industry,account.employee_band,account.location,account.status,account.score,account.score_label,account.researched_at,account.created_at,now(),account.saved_at,account.favorited_at,account.trashed_at],
        )?;
        for person in package.people {
            transaction.execute(
                "INSERT INTO people(id,account_id,name,title) VALUES(?1,?2,?3,?4)",
                params![person.id, person.account_id, person.name, person.title],
            )?;
        }
        for evidence in package.evidence {
            transaction.execute(
                "INSERT INTO evidence(id,account_id,source_url,source_kind,title,excerpt,retrieved_at,published_at,content_hash,confidence,freshness,quality_flags)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)",
                params![evidence.id,evidence.account_id,evidence.source_url,evidence.source_kind,evidence.title,evidence.excerpt,evidence.retrieved_at,evidence.published_at,evidence.content_hash,evidence.confidence,evidence.freshness,serde_json::to_string(&evidence.quality_flags)?],
            )?;
        }
        for contact in package.contacts {
            transaction.execute(
                "INSERT INTO contacts(id,account_id,person_id,kind,value,evidence_id,confidence) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![contact.id,contact.account_id,contact.person_id,contact.kind,contact.value,contact.evidence_id,contact.confidence],
            )?;
        }
        for signal in package.signals {
            transaction.execute(
                "INSERT INTO signals(id,account_id,category,summary,occurred_at,evidence_id,confidence) VALUES(?1,?2,?3,?4,?5,?6,?7)",
                params![signal.id,signal.account_id,signal.category,signal.summary,signal.occurred_at,signal.evidence_id,signal.confidence],
            )?;
        }
        if package.assessment.is_some() || package.draft.is_some() || package.call_brief.is_some() {
            let model_tag = package.draft.as_ref().map(|draft| draft.model_tag.clone());
            let model_digest = package
                .draft
                .as_ref()
                .map(|draft| draft.model_digest.clone());
            let prompt_version = package
                .draft
                .as_ref()
                .map(|draft| draft.prompt_version.clone());
            if let Some(draft) = package.draft.as_ref() {
                transaction.execute(
                    "INSERT INTO draft_artifacts(id,account_id,json,created_at) VALUES(?1,?2,?3,?4)",
                    params![draft.id,account.id,serde_json::to_string(draft)?,draft.created_at],
                )?;
            }
            transaction.execute(
                "INSERT INTO packages(account_id,assessment_json,draft_json,call_brief_json,evidence_set_json,model_tag,model_digest,prompt_version,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9)",
                params![account.id,package.assessment.map(|value| serde_json::to_string(&value)).transpose()?,package.draft.map(|value| serde_json::to_string(&value)).transpose()?,package.call_brief.map(|value| serde_json::to_string(&value)).transpose()?,serde_json::to_string(&evidence_set)?,model_tag,model_digest,prompt_version,now()],
            )?;
        }
        if let Some(outcome) = package.latest_outcome {
            transaction.execute(
                "INSERT INTO outcomes(id,account_id,outcome,created_at) VALUES(?1,?2,?3,?4)",
                params![Uuid::new_v4().to_string(), account.id, outcome, now()],
            )?;
        }
        if package.suppressed {
            transaction.execute(
                "INSERT INTO suppressions(account_id,reason,created_at) VALUES(?1,'restored',?2)",
                params![account.id, now()],
            )?;
        }
    }
    for job in jobs {
        transaction.execute(
            "INSERT INTO jobs(id,account_id,kind,status,attempts,last_error,scheduled_at,completed_at,progress_percent,progress_stage,request_json) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)",
            params![job.id,job.account_id,job.kind,if job.status == "running" { "pending" } else { &job.status },job.attempts,job.last_error,job.scheduled_at,job.completed_at,job.progress_percent,job.progress_stage,job.request.map(|value| serde_json::to_string(&value)).transpose()?],
        )?;
    }
    for identity in source_identities {
        transaction.execute(
            "INSERT INTO source_identities(id,account_id,source_name,source_external_id,source_record_url,identity_confidence,created_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7)",
            params![identity.id,identity.account_id,identity.source_name,identity.source_external_id,identity.source_record_url,identity.identity_confidence,identity.created_at],
        )?;
    }
    for run in search_runs {
        let brief_id = (!run.brief_id.is_empty()).then_some(run.brief_id.clone());
        let restored_status = if run.status == "running" {
            "cancelled"
        } else {
            run.status.as_str()
        };
        let restored_percent = if restored_status == "complete" {
            100
        } else {
            run.progress_percent
        };
        let restored_stage = if run.status == "running" {
            "Cancelled during import"
        } else {
            run.progress_stage.as_str()
        };
        transaction.execute(
            "INSERT INTO search_runs(id,brief_id,query,status,progress_completed,progress_total,progress_percent,progress_stage,result_limit,prompt_version,candidate_count,researched_count,strong_count,near_match_count,warnings_json,started_at,completed_at)
             VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15,?16,?17)",
            params![run.id,brief_id,run.query,restored_status,run.progress_completed,run.progress_total,restored_percent,restored_stage,run.result_limit,SEARCH_PROMPT_VERSION,run.candidate_count,run.researched_count,run.strong_count,run.near_match_count,serde_json::to_string(&run.warnings)?,run.started_at,run.completed_at],
        )?;
        for item in run.accounts {
            transaction.execute(
                "INSERT INTO search_run_accounts(search_run_id,account_id,state,discovery_source,discovery_reason,score,evidence_confidence,rejection_reasons_json,matched_features_json,assessment_json,rank,result_tier,created_at,updated_at)
                 VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?13)",
                params![run.id,item.account.id,item.state,item.discovery_source,item.discovery_reason,item.score,item.evidence_confidence,serde_json::to_string(&item.rejection_reasons)?,serde_json::to_string(&item.matched_features)?,item.assessment.map(|value| serde_json::to_string(&value)).transpose()?,item.rank,item.result_tier,now()],
            )?;
        }
    }
    transaction.commit()?;
    migrate_prompt_first_data(connection)?;
    Ok(())
}

/// Remove every past search run, its candidate list, and its cached discoveries.
///
/// Accounts survive: a saved prospect is worth keeping even when the run that
/// found it is not.
pub fn clear_search_history(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "DELETE FROM search_run_accounts;
         DELETE FROM search_runs;
         DELETE FROM search_briefs;
         DELETE FROM discovery_cache;",
    )?;
    Ok(())
}

/// Remove every prospect and everything attached to it.
///
/// Search history goes too, because a run whose candidates no longer exist is
/// an empty row that can only confuse. The offer profile, sales play, and
/// settings are preserved, this clears work, not configuration.
pub fn clear_prospects(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "DELETE FROM search_run_accounts;
         DELETE FROM search_runs;
         DELETE FROM search_briefs;
         DELETE FROM discovery_cache;
         DELETE FROM source_identities;
         DELETE FROM outcomes;
         DELETE FROM suppressions;
         DELETE FROM jobs;
         DELETE FROM draft_artifacts;
         DELETE FROM packages;
         DELETE FROM contacts;
         DELETE FROM signals;
         DELETE FROM evidence;
         DELETE FROM people;
         DELETE FROM accounts;",
    )?;
    Ok(())
}

pub fn delete_all(connection: &Connection) -> AppResult<()> {
    connection.execute_batch(
        "DELETE FROM search_run_accounts; DELETE FROM search_runs; DELETE FROM search_briefs; DELETE FROM source_identities; DELETE FROM discovery_cache; DELETE FROM source_backoff; DELETE FROM offer_profile;
         DELETE FROM outcomes; DELETE FROM suppressions; DELETE FROM jobs; DELETE FROM draft_artifacts; DELETE FROM packages;
         DELETE FROM contacts; DELETE FROM signals; DELETE FROM evidence; DELETE FROM people; DELETE FROM accounts;
         DELETE FROM sales_play; DELETE FROM settings;",
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_domains_without_paths_or_www() {
        assert_eq!(
            normalize_domain("HTTPS://WWW.Example.com/about"),
            Some("example.com".into())
        );
        assert_eq!(normalize_domain("example.com."), Some("example.com".into()));
        assert_eq!(
            normalize_domain("https://news.example.co.uk/story"),
            Some("example.co.uk".into())
        );
        assert_eq!(normalize_domain("localhost"), None);
    }

    #[test]
    fn a_private_address_is_never_stored_as_a_company_website() {
        // A `website` tag in OpenStreetMap, a CSV column, and a reference-data
        // record are all untrusted. These used to pass, because only the
        // scheme, credentials, and port were checked.
        for hostile in [
            "http://127.0.0.1/admin",
            "http://192.168.0.5/",
            "http://10.1.2.3/",
            "http://169.254.169.254/latest/meta-data/",
            "http://[::1]/",
            "http://localhost/",
            "http://intranet.local/",
            "https://user:pass@example.com/",
            "https://example.com:8443/",
            "file:///etc/passwd",
        ] {
            assert!(
                normalize_public_url(hostile).is_none(),
                "{hostile} must not be accepted as a public website"
            );
        }
        assert_eq!(
            normalize_public_url("https://example.com/about#team").as_deref(),
            Some("https://example.com/about")
        );
    }

    #[test]
    fn normalizes_contacts_conservatively() {
        assert_eq!(
            normalize_email(" Name@Example.COM "),
            Some("name@example.com".into())
        );
        assert_eq!(normalize_email("not-an-email"), None);
        assert_eq!(
            normalize_us_phone("(212) 555-0100"),
            Some("+12125550100".into())
        );
        assert_eq!(normalize_us_phone("555-0100"), None);
        assert_eq!(
            normalize_public_url("https://www.example.com/news#team"),
            Some("https://www.example.com/news".into())
        );
        assert_eq!(normalize_public_url("file:///tmp/data"), None);
    }

    #[test]
    fn migration_drops_the_unused_full_text_index() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.sqlite3");
        let connection = open(&path).unwrap();
        let present: i64 = connection
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE name='evidence_fts'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(present, 0, "the write-only FTS index must not be recreated");
        // Reopening an existing database must stay clean and not error.
        drop(connection);
        assert!(open(&path).is_ok());
    }

    #[test]
    fn local_workflow_deduplicates_suppresses_exports_and_deletes() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("workflow.sqlite3")).unwrap();
        save_sales_play(
            &connection,
            SalesPlay {
                id: "active".into(),
                name: "Test play".into(),
                offer: "Operational research".into(),
                icp_industries: vec!["Logistics".into()],
                company_sizes: vec!["11-50".into()],
                target_personas: vec!["COO".into()],
                exclusions: vec![],
                proof_points: vec!["A cited proof point".into()],
                tone: "Concise".into(),
                cta: "Compare notes".into(),
                sender_name: "Alex".into(),
                sender_company: "Example".into(),
                postal_address: "1 Main St, New York, NY 10001".into(),
                opt_out_text: "Reply no and I will not contact you again.".into(),
                approved: true,
                updated_at: None,
            },
        )
        .unwrap();
        let seed = AccountSeed {
            company_name: "Acme".into(),
            domain: Some("www.acme.example".into()),
            website_url: None,
            person_name: Some("Dana Lee".into()),
            person_title: Some("COO".into()),
            work_email: Some("Dana@Acme.Example".into()),
            business_phone: Some("212-555-0100".into()),
        };
        import_accounts(&mut connection, vec![seed.clone(), seed]).unwrap();
        let accounts = list_accounts(&connection).unwrap();
        assert_eq!(accounts.len(), 1);
        let package = get_package(&connection, &accounts[0].id).unwrap();
        assert_eq!(package.people.len(), 1);
        assert_eq!(package.contacts.len(), 2);
        assert!(package.contacts.iter().all(|contact| package
            .evidence
            .iter()
            .any(|evidence| evidence.id == contact.evidence_id)));
        set_suppression(&connection, &accounts[0].id, true).unwrap();
        assert!(
            get_package(&connection, &accounts[0].id)
                .unwrap()
                .suppressed
        );
        let export = export_json(&connection).unwrap();
        assert_eq!(export["format"], "prospecting-workbench-export");
        delete_all(&connection).unwrap();
        assert!(list_accounts(&connection).unwrap().is_empty());
        assert!(load_sales_play(&connection).unwrap().is_none());
        restore_json(&mut connection, &export.to_string()).unwrap();
        assert_eq!(list_accounts(&connection).unwrap().len(), 1);
        assert!(load_sales_play(&connection).unwrap().unwrap().approved);
        delete_all(&connection).unwrap();
    }

    #[test]
    fn labels_stale_conflicting_and_incomplete_evidence_without_resolving_it() {
        let directory = tempfile::tempdir().unwrap();
        let connection = open(&directory.path().join("quality.sqlite3")).unwrap();
        connection.execute("INSERT INTO accounts(id,name,status,created_at,updated_at) VALUES('a','Acme','new',?1,?1)", params![now()]).unwrap();
        let first = upsert_evidence(
            &connection,
            "a",
            "https://one.example",
            "company_page",
            "One",
            "Acme reports that it has 50 employees in its United States business.",
            "medium",
        )
        .unwrap();
        let second = upsert_evidence(
            &connection,
            "a",
            "https://two.example",
            "company_page",
            "Two",
            "A second public source reports that Acme has 90 employees today.",
            "medium",
        )
        .unwrap();
        let short = upsert_evidence(
            &connection,
            "a",
            "https://three.example",
            "company_page",
            "Three",
            "Too little detail.",
            "low",
        )
        .unwrap();
        // Freshness is derived from `retrieved_at`, so age the row rather than
        // writing the label directly. This exercises the real mechanism.
        connection
            .execute(
                "UPDATE evidence SET retrieved_at=datetime('now','-100 days') WHERE id=?1",
                params![first],
            )
            .unwrap();
        update_quality_flags(&connection, "a").unwrap();
        let package = get_package(&connection, "a").unwrap();
        assert!(package
            .evidence
            .iter()
            .filter(|item| item.id == first || item.id == second)
            .all(|item| item.quality_flags.contains(&"contradictory".into())));
        assert!(package
            .evidence
            .iter()
            .find(|item| item.id == first)
            .unwrap()
            .quality_flags
            .contains(&"stale".into()));
        assert!(package
            .evidence
            .iter()
            .find(|item| item.id == short)
            .unwrap()
            .quality_flags
            .contains(&"incomplete".into()));
    }

    #[test]
    fn interrupted_jobs_are_resumable() {
        let directory = tempfile::tempdir().unwrap();
        let connection = open(&directory.path().join("jobs.sqlite3")).unwrap();
        connection
            .execute(
                "INSERT INTO jobs(id,kind,status,scheduled_at) VALUES('j','daily_scan','running',?1)",
                params![now()],
            )
            .unwrap();
        assert_eq!(recover_interrupted_jobs(&connection).unwrap(), 1);
        assert_eq!(list_jobs(&connection).unwrap()[0].status, "pending");
    }

    #[test]
    fn public_record_identity_is_not_saved_as_company_website() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("identity.sqlite3")).unwrap();
        let brief = SearchBrief {
            id: "brief".into(),
            query: "Find public companies with workflow needs".into(),
            product_summary: "Workflow system".into(),
            customer_outcome: "Improve intake".into(),
            segments: vec![],
            geography: "United States".into(),
            company_sizes: vec![],
            target_personas: vec![],
            positive_signals: vec![],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 10,
            qualification_threshold: 55,
            quality_band: 4,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: SEARCH_PROMPT_VERSION.into(),
            created_at: now(),
        };
        let run = create_search_run(&connection, &brief).unwrap();
        assert_eq!(run.progress_percent, 0);
        assert_eq!(run.progress_stage, "Queued");
        set_search_run_stage(&connection, &run.id, 37, "Checking sources").unwrap();
        let run = get_search_run(&connection, &run.id).unwrap();
        assert_eq!(run.progress_percent, 37);
        assert_eq!(run.progress_stage, "Checking sources");
        let result = DiscoveryResult {
            source: "SEC EDGAR".into(),
            company_name: "Example Registrant".into(),
            domain: None,
            website_url: Some("https://www.sec.gov/edgar/browse/?CIK=1".into()),
            external_id: Some("0000000001".into()),
            reason: "SEC registrant record".into(),
            source_url: "https://www.sec.gov/edgar/browse/?CIK=1".into(),
            source_external_id: Some("0000000001".into()),
            business_phone: None,
            location: None,
            identity_confidence: Some("high".into()),
        };
        let account_id = import_discovery_result(&mut connection, &run.id, &result).unwrap();
        let account = get_account(&connection, &account_id).unwrap();
        assert!(account.domain.is_none());
        assert!(account.website_url.is_none());
        assert_eq!(list_source_identities(&connection).unwrap().len(), 1);
        assert_eq!(
            get_search_run(&connection, &run.id).unwrap().accounts.len(),
            1
        );
    }

    fn ranking_brief(quality_band: i64) -> SearchBrief {
        SearchBrief {
            id: Uuid::new_v4().to_string(),
            query: "Find plumbing companies needing better phone intake".into(),
            product_summary: "Voice intake".into(),
            customer_outcome: "Capture calls".into(),
            segments: vec![],
            geography: "United States".into(),
            company_sizes: vec![],
            target_personas: vec![],
            positive_signals: vec![],
            exclusions: vec![],
            discovery_lanes: vec![DiscoveryLane {
                id: "mapped".into(),
                label: "Mapped".into(),
                mode: "automatic".into(),
                source: "overpass".into(),
                reason: "test".into(),
                limitation: "test".into(),
            }],
            result_limit: 10,
            qualification_threshold: 70,
            quality_band,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: SEARCH_PROMPT_VERSION.into(),
            created_at: now(),
        }
    }

    /// Import one discovered candidate already scored and assessed.
    fn seed_ranked_candidate(
        connection: &mut Connection,
        run_id: &str,
        index: usize,
        score: i64,
    ) -> String {
        let result = DiscoveryResult {
            source: "Fixture".into(),
            company_name: format!("Company {index}"),
            domain: Some(format!("company{index}.example")),
            website_url: Some(format!("https://company{index}.example")),
            external_id: None,
            reason: "Fixture discovery".into(),
            source_url: format!("https://source.example/{index}"),
            source_external_id: None,
            business_phone: None,
            location: Some("United States".into()),
            identity_confidence: Some("medium".into()),
        };
        let account_id = import_discovery_result(connection, run_id, &result).unwrap();
        let fit = FitAssessment {
            hard_gates: vec![HardGate {
                label: "Identity".into(),
                status: "pass".into(),
                reason: "Verified".into(),
            }],
            icp_fit: score,
            signal_relevance: score,
            persona_fit: score,
            evidence_contactability: score,
            total: score,
            reasons: vec![CitedReason {
                text: "Direct supported signal".into(),
                evidence_ids: vec!["evidence".into()],
            }],
        };
        update_search_run_account(
            connection,
            run_id,
            &account_id,
            SearchRunAccountUpdate {
                state: if score >= 70 {
                    "qualified"
                } else {
                    "needs_review"
                },
                score: Some(score),
                confidence: "medium",
                rejection_reasons: &[],
                features: &[],
                assessment: Some(&fit),
                rank: None,
                result_tier: "near_match",
            },
        )
        .unwrap();
        account_id
    }

    #[test]
    fn quality_first_ranking_returns_a_narrow_strong_band() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("ranking.sqlite3")).unwrap();
        let brief = SearchBrief {
            id: "quality-brief".into(),
            query: "Find plumbing companies needing better phone intake".into(),
            product_summary: "Voice intake".into(),
            customer_outcome: "Capture calls".into(),
            segments: vec![],
            geography: "United States".into(),
            company_sizes: vec![],
            target_personas: vec![],
            positive_signals: vec![],
            exclusions: vec![],
            discovery_lanes: vec![DiscoveryLane {
                id: "mapped".into(),
                label: "Mapped".into(),
                mode: "automatic".into(),
                source: "overpass".into(),
                reason: "test".into(),
                limitation: "test".into(),
            }],
            result_limit: 10,
            qualification_threshold: 70,
            quality_band: 4,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: SEARCH_PROMPT_VERSION.into(),
            created_at: now(),
        };
        let run = create_search_run(&connection, &brief).unwrap();
        let assessment = |score| FitAssessment {
            hard_gates: vec![HardGate {
                label: "Identity".into(),
                status: "pass".into(),
                reason: "Verified".into(),
            }],
            icp_fit: score,
            signal_relevance: score,
            persona_fit: score,
            evidence_contactability: score,
            total: score,
            reasons: vec![CitedReason {
                text: "Direct supported signal".into(),
                evidence_ids: vec!["evidence".into()],
            }],
        };
        for (index, score) in [82_i64, 78, 75, 48].into_iter().enumerate() {
            let result = DiscoveryResult {
                source: "Fixture".into(),
                company_name: format!("Company {index}"),
                domain: Some(format!("company{index}.example")),
                website_url: Some(format!("https://company{index}.example")),
                external_id: None,
                reason: "Fixture discovery".into(),
                source_url: format!("https://source.example/{index}"),
                source_external_id: None,
                business_phone: None,
                location: Some("United States".into()),
                identity_confidence: Some("medium".into()),
            };
            let account_id = import_discovery_result(&mut connection, &run.id, &result).unwrap();
            let fit = assessment(score);
            update_search_run_account(
                &connection,
                &run.id,
                &account_id,
                SearchRunAccountUpdate {
                    state: if score >= 70 {
                        "qualified"
                    } else {
                        "needs_review"
                    },
                    score: Some(score),
                    confidence: "medium",
                    rejection_reasons: &[],
                    features: &[],
                    assessment: Some(&fit),
                    rank: None,
                    result_tier: "near_match",
                },
            )
            .unwrap();
        }
        // The band is measured from the best eligible score: 82 is the top, so
        // only scores at or above 78 are strong. 75 is 7 below and demotes.
        assert_eq!(
            finalize_search_ranking(&connection, &run.id, 70, 4, 10).unwrap(),
            (2, 2)
        );
        let ranked = get_search_run(&connection, &run.id).unwrap();
        assert_eq!(
            ranked
                .accounts
                .iter()
                .filter(|item| item.result_tier == "strong")
                .map(|item| item.score.unwrap())
                .collect::<Vec<_>>(),
            vec![82, 78]
        );
        assert_eq!(ranked.strong_count, 2);
    }

    #[test]
    fn the_band_never_chains_from_one_candidate_to_the_next() {
        // Regression guard. Comparing each candidate against its predecessor
        // instead of the best score lets a gentle slope masquerade as a tight
        // cluster: 95 -> 84 -> 73 are 11 apart in sequence but span 22 points,
        // so promoting all three would make "Strong Match" meaningless.
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("chain.sqlite3")).unwrap();
        let brief = ranking_brief(12);
        let run = create_search_run(&connection, &brief).unwrap();
        for (index, score) in [95_i64, 84, 73].into_iter().enumerate() {
            seed_ranked_candidate(&mut connection, &run.id, index, score);
        }
        // Each step is 11, so the chaining bug promoted all three. Measured
        // from the best, 73 is 22 below and drops out.
        let (strong, near) = finalize_search_ranking(&connection, &run.id, 70, 12, 10).unwrap();
        assert_eq!(strong, 2, "73 is 22 below the top score, not 11");
        assert_eq!(near, 1);
        let ranked = get_search_run(&connection, &run.id).unwrap();
        assert_eq!(
            ranked
                .accounts
                .iter()
                .filter(|item| item.result_tier == "strong")
                .map(|item| item.score.unwrap())
                .collect::<Vec<_>>(),
            vec![95, 84]
        );

        // At the shipped four-point default the same chain yields only the top.
        let (strong, _) = finalize_search_ranking(&connection, &run.id, 70, 4, 10).unwrap();
        assert_eq!(strong, 1, "only 95 is within four points of the best");
    }

    #[test]
    fn the_strong_tier_stays_within_four_points_of_the_best() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("cliff.sqlite3")).unwrap();
        let brief = ranking_brief(4);
        let run = create_search_run(&connection, &brief).unwrap();
        for (index, score) in [95_i64, 93, 91, 74, 72].into_iter().enumerate() {
            seed_ranked_candidate(&mut connection, &run.id, index, score);
        }
        assert_eq!(
            finalize_search_ranking(&connection, &run.id, 70, 4, 10).unwrap(),
            (3, 2)
        );
        let ranked = get_search_run(&connection, &run.id).unwrap();
        assert_eq!(
            ranked
                .accounts
                .iter()
                .filter(|item| item.result_tier == "strong")
                .map(|item| item.score.unwrap())
                .collect::<Vec<_>>(),
            vec![95, 93, 91]
        );
    }

    #[test]
    fn ranking_never_returns_more_than_the_requested_limit() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("limit.sqlite3")).unwrap();
        let brief = ranking_brief(4);
        let run = create_search_run(&connection, &brief).unwrap();
        for (index, score) in [90_i64, 89, 88, 87, 86, 85].into_iter().enumerate() {
            seed_ranked_candidate(&mut connection, &run.id, index, score);
        }
        // 90..=86 are within four of the top; the limit of 3 caps them anyway.
        let (strong, near) = finalize_search_ranking(&connection, &run.id, 70, 4, 3).unwrap();
        assert_eq!(strong, 3, "the requested limit caps the strong tier");
        assert_eq!(near, 3, "the rest stay visible as near matches");
    }

    #[test]
    fn library_actions_are_idempotent_and_monitor_only_saved_accounts() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("library.sqlite3")).unwrap();
        import_accounts(
            &mut connection,
            vec![AccountSeed {
                company_name: "Acme".into(),
                domain: Some("acme.example".into()),
                website_url: None,
                person_name: None,
                person_title: None,
                work_email: None,
                business_phone: None,
            }],
        )
        .unwrap();
        let id = list_accounts(&connection).unwrap()[0].id.clone();
        apply_library_action(&mut connection, std::slice::from_ref(&id), "favorite").unwrap();
        let account = get_account(&connection, &id).unwrap();
        assert!(account.saved_at.is_some() && account.favorited_at.is_some());
        apply_library_action(&mut connection, std::slice::from_ref(&id), "unsave").unwrap();
        let account = get_account(&connection, &id).unwrap();
        assert!(account.saved_at.is_none() && account.favorited_at.is_none());
        assert_eq!(queue_daily_scan(&connection).unwrap(), 0);
        apply_library_action(&mut connection, std::slice::from_ref(&id), "save").unwrap();
        assert_eq!(queue_daily_scan(&connection).unwrap(), 1);
        apply_library_action(&mut connection, std::slice::from_ref(&id), "trash").unwrap();
        assert!(get_account(&connection, &id).unwrap().trashed_at.is_some());
        apply_library_action(&mut connection, std::slice::from_ref(&id), "restore").unwrap();
        assert!(get_account(&connection, &id).unwrap().saved_at.is_some());
    }

    #[test]
    fn sequential_runs_are_allowed_but_overlapping_runs_are_blocked() {
        let directory = tempfile::tempdir().unwrap();
        let connection = open(&directory.path().join("sequential.sqlite3")).unwrap();
        let brief = SearchBrief {
            id: "one".into(),
            query: "Find suitable public business prospects".into(),
            product_summary: "Product".into(),
            customer_outcome: "Outcome".into(),
            segments: vec![],
            geography: "United States".into(),
            company_sizes: vec![],
            target_personas: vec![],
            positive_signals: vec![],
            exclusions: vec![],
            discovery_lanes: vec![],
            result_limit: 10,
            qualification_threshold: 70,
            quality_band: 4,
            approved: true,
            assumptions: vec![],
            interpreter: "manual_rules".into(),
            model_tag: None,
            model_digest: None,
            prompt_version: SEARCH_PROMPT_VERSION.into(),
            created_at: now(),
        };
        let first = create_search_run(&connection, &brief).unwrap();
        assert!(create_search_run(
            &connection,
            &SearchBrief {
                id: "blocked".into(),
                ..brief.clone()
            }
        )
        .unwrap_err()
        .to_string()
        .starts_with("SEARCH_CONFLICT:"));
        finish_search_run(&connection, &first.id, "complete", None).unwrap();
        assert!(create_search_run(
            &connection,
            &SearchBrief {
                id: "two".into(),
                ..brief
            }
        )
        .is_ok());
    }

    #[test]
    fn normalized_discovery_cache_round_trips_without_raw_pages() {
        let directory = tempfile::tempdir().unwrap();
        let connection = open(&directory.path().join("cache.sqlite3")).unwrap();
        let result = DiscoveryResult {
            source: "OpenStreetMap".into(),
            company_name: "Acme".into(),
            domain: Some("acme.example".into()),
            website_url: Some("https://acme.example/".into()),
            external_id: Some("node/1".into()),
            reason: "Mapped plumber".into(),
            source_url: "https://openstreetmap.org/node/1".into(),
            source_external_id: Some("node/1".into()),
            business_phone: None,
            location: Some("Philadelphia, PA, United States".into()),
            identity_confidence: Some("medium".into()),
        };
        cache_discoveries(
            &connection,
            "overpass:us:plumber",
            "overpass",
            std::slice::from_ref(&result),
        )
        .unwrap();
        let (cached, age) = cached_discoveries(&connection, "overpass:us:plumber")
            .unwrap()
            .unwrap();
        assert_eq!(cached[0].company_name, "Acme");
        assert_eq!(age, 0);
        assert!(!connection
            .query_row(
                "SELECT payload_json LIKE '%<html%' FROM discovery_cache",
                [],
                |row| row.get::<_, bool>(0)
            )
            .unwrap());
    }

    #[test]
    fn version_one_exports_remain_importable() {
        let directory = tempfile::tempdir().unwrap();
        let mut connection = open(&directory.path().join("v1.sqlite3")).unwrap();
        let mut export = export_json(&connection).unwrap();
        export["contractVersion"] = serde_json::json!(1);
        export.as_object_mut().unwrap().remove("offerProfile");
        export.as_object_mut().unwrap().remove("searchBriefs");
        export.as_object_mut().unwrap().remove("searchRuns");
        export.as_object_mut().unwrap().remove("sourceIdentities");
        restore_json(&mut connection, &export.to_string()).unwrap();
        assert!(list_accounts(&connection).unwrap().is_empty());
        assert_eq!(CONTRACT_VERSION, 3);
    }
}
