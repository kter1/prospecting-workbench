# Prospecting Workbench

Prospecting Workbench is a free, Apache-2.0, local-first desktop application for evidence-backed U.S. B2B prospect research. Describe what you sell in plain language, review the targeting assumptions, and the workbench uses enabled free public sources to find and qualify candidates without pretending those sources provide exhaustive coverage.

It does **not** send email, place calls, guess addresses, scrape LinkedIn or search-result pages, use paid databases, provide telemetry, or fall back to hosted AI. Apollo.io is not used or required.

## What is implemented

- Prompt-first discovery with an editable, versioned search brief and explicit automatic/guided coverage labels.
- A stable offer profile separated from immutable per-query search runs.
- Bounded OpenStreetMap Overpass discovery across a curated catalog of roughly 90 U.S. business verticals and every U.S. state plus about 50 metro areas, with attribution and no model-generated source queries. Requests rotate across mirrors and across bounded regions, so repeating a search samples new ground.
- Progressive, cancellable, resumable search runs with run-keyed progress, overlap prevention, recovery after restart, and independent per-query assessments.
- Quality-first retrieval that discovers a broader pool before research, returns up to the requested number of strong matches, enforces the visible threshold and four-point primary score band, and separates near matches from excluded diagnostics.
- A 24-hour normalized Overpass cache, persistent source backoff, cached-result age warnings, partial-source success, and deterministic pre-ranking before batched research.
- Seed import from domains, URLs, and CSV, including conservative normalization and deduplication.
- Guided browser queries that the user opens and reviews manually.
- Free public-source discovery through SEC EDGAR and USAspending, plus optional user-operated SearXNG.
- Company-site research across permitted public company, contact, leadership, careers, news, feed, Greenhouse, and Lever pages.
- Supplemental GDELT news and exact-identity GitHub organization signals.
- Robots handling, redirect revalidation, private-address and unsafe-scheme blocking, response/type limits, and `Retry-After` errors.
- SQLite evidence ledger, serialized writes, resumable local jobs, content hashes, HTTP cache metadata, derived freshness labels, suppression, and manual outcomes.
- Saved, Favorites, All Discovered, and Trash library views with independent selection controls, transactional bulk actions, eight-second trash undo, and confirmed trash-only permanent deletion.
- Fixed 35/30/20/15 scoring in application code. Search planning is always a deterministic local compiler that selects from the curated catalog and never contacts a model. Local AI proposes the offer profile and writes cited artifacts, never numeric scores or source queries.
- Multiple focused evidence excerpts per page for workflow, hiring, location, identity, and contact facts; raw HTML is discarded.
- Pinned local `qwen3.5:9b` generation through Ollama JSON-schema output, readiness checks, persisted milestone progress, idempotent artifact saving, and a separate citation verification pass.
- Deterministic fake provider, plus fixture tests covering every public-source parser against well-formed, malformed, duplicated, and injection-bearing payloads. The fake-provider button only appears in development builds.
- Version 3 local JSON export with v1/v2 import compatibility, full deletion, source health, model health, saved-account-only daily scans while the app is open, and missed-scan catch-up at launch.

## Requirements

- macOS 13 or newer for the currently validated desktop target.
- Apple Silicon recommended, 16 GB RAM, and approximately 12 GB of free disk.
- Node.js 22+, pnpm 11+, Rust 1.77.2+, Xcode Command Line Tools.
- [Ollama](https://ollama.com/) and the local model:

```sh
ollama pull qwen3.5:9b
```

Research and discovery need internet access. Existing records and local generation remain available offline after the model is installed. Windows and Linux are not yet claimed as supported targets.

## Run locally

```sh
pnpm install
pnpm tauri dev
```

The application creates its SQLite database in the operating system’s application-data directory. It has no login, server, cloud sync, analytics, or localhost HTTP API. Ollama is contacted only at `127.0.0.1:11434`; a configured loopback SearXNG service is a separate, explicit opt-in.

## Workflow

1. Describe the product, desired customer, and business outcome on Discover.
2. Review the compact search plan, its assumptions, source coverage, geography, and qualification threshold.
3. Approve the plan. Automatic candidates stream through discovery, screening, research, and deterministic qualification.
4. Review cited evidence and contacts. Missing business contact data is displayed as “not publicly found.”
5. Save promising accounts or use seeded/guided discovery where no compliant free directory exists.
6. Add sender identity, postal address, and opt-out language before generating a ready-to-use commercial draft.
7. Review the verified package, record an outcome, or suppress the account.

Query interpretation never needs Ollama: the search plan is compiled instantly and deterministically from the curated catalog, and every rule stays visible and editable before approval. Ollama is used for the optional offer-profile proposal and for drafting. Seed import and stored records remain usable; there is never a hosted-model fallback.

The tool prepares material for human review only. It never sends or schedules outreach.

## Verification

```sh
pnpm test
pnpm build
cargo test --manifest-path src-tauri/Cargo.toml
pnpm run generate:sbom
```

With Ollama running and the pinned model installed, run the release-only real-model gate:

```sh
pnpm run eval:golden
```

It writes `artifacts/real-model-golden.txt`; attach that summary alongside the unsigned build and SBOM.

Live-adapter checks are intentionally not part of CI; source behavior and rate limits change. CI uses deterministic fixtures and the fake AI path. A release must additionally run the real-model golden evaluation on a 16 GB Apple Silicon machine before an unsigned macOS artifact is published.

## Public source notes

- [SEC EDGAR APIs](https://www.sec.gov/search-filings/edgar-application-programming-interfaces)
- [USAspending API](https://api.usaspending.gov/docs/endpoints)
- [Greenhouse job board API](https://developers.greenhouse.io/job-board.html)
- [SearXNG search API](https://docs.searxng.org/dev/search_api.html)
- [OpenStreetMap Overpass API](https://wiki.openstreetmap.org/wiki/Overpass_API)
- [OpenStreetMap copyright and ODbL attribution](https://www.openstreetmap.org/copyright)
- [Public Nominatim usage policy](https://operations.osmfoundation.org/policies/nominatim/)
- [Ollama structured outputs](https://docs.ollama.com/capabilities/structured-outputs)

OpenStreetMap results are bounded samples and require attribution. Geographies resolve against a static bounding-box table; the public Nominatim service is not used for systematic business enumeration. When a mapped business carries no address tags, its location is recorded as the bounded search area it was found in — a fact the query establishes — and labeled as such in its evidence. All coverage is bounded by public availability. “Not publicly found” is a valid result, not a prompt to infer or buy enrichment.

A search that reaches its time budget reports how many discovered candidates went unresearched. Those candidates were not rejected, and the app says so rather than attributing the shortfall to its scoring band.

## Safety and compliance boundary

- Page content is stripped of active/hidden elements and common embedded instruction patterns, then quoted to the model as untrusted evidence.
- Every contact and signal requires an evidence row. Emails are never created from patterns, and personal/mobile numbers are excluded.
- A draft cannot be created for a suppressed account.
- The required footer is appended from the approved sender identity, postal address, and opt-out wording.
- Operators remain responsible for determining whether and how to use a reviewed draft and for complying with applicable law.

See [SECURITY.md](SECURITY.md) for the threat model and [CONTRIBUTING.md](CONTRIBUTING.md) for development conventions.
