-- Sanitized text of recently read public pages, so a repeat search or a
-- re-score with edited rules can match against them without re-crawling.
-- Entries are reused for 24 hours and purged after 7 days.
CREATE TABLE IF NOT EXISTS page_cache (
  account_id TEXT NOT NULL REFERENCES accounts(id) ON DELETE CASCADE,
  url TEXT NOT NULL,
  final_url TEXT NOT NULL,
  title TEXT NOT NULL,
  text TEXT NOT NULL,
  links_json TEXT NOT NULL DEFAULT '[]',
  etag TEXT,
  modified_at TEXT,
  technologies_json TEXT NOT NULL DEFAULT '[]',
  fetched_at TEXT NOT NULL,
  PRIMARY KEY(account_id, url)
);

CREATE INDEX IF NOT EXISTS idx_page_cache_fetched ON page_cache(fetched_at);
