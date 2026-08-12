PRAGMA journal_mode = WAL;
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS sales_play (
  id TEXT PRIMARY KEY CHECK (id = 'active'),
  json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS accounts (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  domain TEXT UNIQUE,
  website_url TEXT,
  industry TEXT,
  employee_band TEXT,
  location TEXT,
  status TEXT NOT NULL DEFAULT 'new',
  score INTEGER,
  score_label TEXT,
  researched_at TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS people (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  name TEXT NOT NULL,
  title TEXT,
  UNIQUE(account_id, name COLLATE NOCASE)
);

CREATE TABLE IF NOT EXISTS evidence (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  source_url TEXT NOT NULL,
  source_kind TEXT NOT NULL,
  title TEXT NOT NULL,
  excerpt TEXT NOT NULL CHECK(length(excerpt) <= 500),
  retrieved_at TEXT NOT NULL,
  published_at TEXT,
  content_hash TEXT NOT NULL,
  confidence TEXT NOT NULL,
  freshness TEXT NOT NULL,
  quality_flags TEXT NOT NULL DEFAULT '[]',
  etag TEXT,
  modified_at TEXT,
  UNIQUE(account_id, source_url, content_hash)
);

CREATE TABLE IF NOT EXISTS contacts (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  person_id TEXT REFERENCES people(id) ON DELETE SET NULL,
  kind TEXT NOT NULL CHECK(kind IN ('company_main','work_direct','role_email','work_email')),
  value TEXT NOT NULL COLLATE NOCASE,
  evidence_id TEXT NOT NULL REFERENCES evidence(id) ON DELETE CASCADE,
  confidence TEXT NOT NULL,
  UNIQUE(account_id, value)
);

CREATE TABLE IF NOT EXISTS signals (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  category TEXT NOT NULL,
  summary TEXT NOT NULL,
  occurred_at TEXT,
  evidence_id TEXT NOT NULL REFERENCES evidence(id) ON DELETE CASCADE,
  confidence TEXT NOT NULL,
  UNIQUE(account_id, category, evidence_id)
);

CREATE TABLE IF NOT EXISTS packages (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  assessment_json TEXT,
  draft_json TEXT,
  call_brief_json TEXT,
  evidence_set_json TEXT NOT NULL DEFAULT '[]',
  model_tag TEXT,
  model_digest TEXT,
  prompt_version TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS draft_artifacts (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  json TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS outcomes (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  outcome TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS suppressions (
  account_id TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
  reason TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS jobs (
  id TEXT PRIMARY KEY,
  account_id TEXT REFERENCES accounts(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  status TEXT NOT NULL,
  attempts INTEGER NOT NULL DEFAULT 0,
  last_error TEXT,
  scheduled_at TEXT NOT NULL,
  completed_at TEXT
);

CREATE TABLE IF NOT EXISTS settings (
  key TEXT PRIMARY KEY,
  value TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_account ON evidence(account_id, retrieved_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_account ON signals(account_id);
CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status, scheduled_at);
CREATE INDEX IF NOT EXISTS idx_drafts_account ON draft_artifacts(account_id, created_at DESC);
