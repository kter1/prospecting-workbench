CREATE TABLE IF NOT EXISTS offer_profile (
  id TEXT PRIMARY KEY CHECK (id = 'active'),
  json TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS search_briefs (
  id TEXT PRIMARY KEY,
  query TEXT NOT NULL,
  json TEXT NOT NULL,
  approved INTEGER NOT NULL DEFAULT 0,
  created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS search_runs (
  id TEXT PRIMARY KEY,
  brief_id TEXT REFERENCES search_briefs(id) ON DELETE SET NULL,
  query TEXT NOT NULL,
  status TEXT NOT NULL,
  progress_completed INTEGER NOT NULL DEFAULT 0,
  progress_total INTEGER NOT NULL DEFAULT 0,
  result_limit INTEGER NOT NULL,
  offer_profile_json TEXT,
  model_tag TEXT,
  model_digest TEXT,
  prompt_version TEXT,
  source_versions_json TEXT NOT NULL DEFAULT '{}',
  started_at TEXT NOT NULL,
  completed_at TEXT,
  last_error TEXT
);

CREATE TABLE IF NOT EXISTS search_run_accounts (
  search_run_id TEXT NOT NULL REFERENCES search_runs(id) ON DELETE CASCADE,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  state TEXT NOT NULL DEFAULT 'discovered',
  discovery_source TEXT NOT NULL,
  discovery_reason TEXT NOT NULL,
  score INTEGER,
  evidence_confidence TEXT NOT NULL DEFAULT 'unknown',
  rejection_reasons_json TEXT NOT NULL DEFAULT '[]',
  matched_features_json TEXT NOT NULL DEFAULT '[]',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL,
  PRIMARY KEY(search_run_id, account_id)
);

CREATE TABLE IF NOT EXISTS source_identities (
  id TEXT PRIMARY KEY,
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  source_name TEXT NOT NULL,
  source_external_id TEXT,
  source_record_url TEXT NOT NULL,
  identity_confidence TEXT NOT NULL,
  created_at TEXT NOT NULL,
  UNIQUE(account_id, source_name, source_record_url)
);

CREATE INDEX IF NOT EXISTS idx_search_runs_started ON search_runs(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_search_run_accounts_state ON search_run_accounts(search_run_id, state);
CREATE INDEX IF NOT EXISTS idx_source_identities_account ON source_identities(account_id);
