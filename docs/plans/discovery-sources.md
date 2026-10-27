# Plan: a local prospect index fed by public registries

Status: proposed, not started. This was written without network access to any source below. Every endpoint, column, file name and limit marked **verify** comes from documentation or search results, and step 0 checks each one against a real response before any parser depends on it.

## The problem, measured against the code

| Limit today | Where | Effect |
|---|---|---|
| Two automatic sources: OpenStreetMap (mapped premises, ~50 sampled metros) and Wikidata (notable organizations) | `execute_search_run` | Most US businesses are in neither |
| At most 100 candidates per run, discovered live on every run | `candidate_budget` | Repeat searches re-download, and depth is capped |
| 94 hand-picked categories | `catalog.rs` | Industries outside the list fall back to "general businesses" |
| Geography matched as strings, or by bounding boxes for ~50 metros | `regions.rs`, `geography_matches` | No "within 25 miles of" search, and fragile matching |
| Research needs a website | `process_search_candidate` | Records without one stop at "needs review" |

Adding sources one by one to the live run would make each search slower and still leave it shallow. The research step reads roughly 100–500 sites per run within its time budget, so range comes from **choosing well among many companies**, not from fetching more on every search.

## The design in one paragraph

Public registries **sync into a local prospect index**: bulk files once per refresh, API slices on first use of a state and industry. Each registry record is **resolved to one entity per real business** across sources, joined by phone, name plus ZIP, or domain. A search then queries this index locally and instantly, using **one industry-code system (NAICS)** and **ZIP-based geography** with a radius. It ranks entities on structured facts, **verifies a website** only for the top-ranked ones, and researches those as it does today. Every fact cites its record and carries a date. Stale-only evidence can never qualify a company by itself.

## Invariants kept

- **No generated source queries.** Queries are fixed templates. Their variables are NAICS, taxonomy or NTEE codes that must exist in the bundled code tables, state codes, ZIP codes from the bundled ZIP table, and integers. User text, model output and registry text never enter a query.
- **Provenance.** Every fact names its source record URL and its as-of date. A website found by resolution carries evidence of how it was matched.
- **Politeness.** Each host has its own rate limit. Bulk files are fetched with resumable, checksummed downloads. The existing source backoff applies. SEC requests carry a contact User-Agent.
- **No personal data.**
  - NPI organizational records only.
  - Records whose registrant name is a person's name are skipped.
  - Demographic columns are never parsed, let alone stored: PPP race, ethnicity, gender and veteran status.
  - Related-person tables are not read (Form D `RELATEDPERSONS`).
  - Free-mail addresses are never stored as contacts.

## Sources, ranked by how much range they add

| # | Source | Coverage | Industry key | Size proxy | Freshness | Access |
|---|---|---|---|---|---|---|
| 1 | [SBA PPP loan data](https://data.sba.gov/dataset/ppp-foia) | Nationwide, every industry, millions of small employers | NAICS | Jobs reported, loan amount | **Dated (2020–21)** | Bulk CSVs (`public_150k_plus`, `public_up_to_150k_1..12`), public domain |
| 2 | [EPA Facility Registry Service](https://www.epa.gov/frs/frs-facilities-state-single-file-csv-download) | Nationwide regulated facilities: manufacturing, auto, dry cleaning, fuel, healthcare and more | NAICS and SIC | none | Current | Bulk per-state or national single-file CSV |
| 3 | [CMS NPI Registry](https://npiregistry.cms.hhs.gov/api-page) | Every healthcare organization | Provider type (NUCC taxonomy) | Practice-location count | Current | API: 200 per request, at most 1,200 per query, no key |
| 4 | [FMCSA Company Census](https://data.transportation.gov/Trucking-and-Motorcoaches/Company-Census-File/az4n-8mr2) | Trucking and bus companies | Operation type (maps to NAICS 484/485) | Trucks and drivers | Daily | Socrata |
| 5 | [IRS exempt-organization file](https://www.irs.gov/charities-non-profits/exempt-organizations-business-master-file-extract-eo-bmf) | Every nonprofit | NTEE | Revenue, assets | Monthly | Bulk CSV per state |
| 6 | [SEC Form D data sets](https://www.sec.gov/dera/data/form-d) | Private companies that raised money | Form D industry group | Revenue range, amount raised | Quarterly | Bulk ZIP, about 3 MB per quarter |
| 7 | [USAspending awards](https://api.usaspending.gov/docs/intro-tutorial) | Federal contractors | NAICS | Award totals | Daily | API, no key |
| 8 | City licence registries (Los Angeles `6rrh-rzua`, San Francisco `g8m3-pdis`, Seattle `wnbq-64tb`, Chicago) | Every licensed business in those cities | NAICS (Chicago: licence type) | none | Daily to monthly | Socrata |
| 9 | [SAM.gov entities](https://open.gsa.gov/api/entity-api/) | Federal registrants | NAICS | Business type | Current | Personal API key with low daily limits; **the only registry with websites** |

### What each source adds

- **PPP and EPA** are the only free sources here that are nationwide, cover every industry and carry industry codes. They are what makes discovery general instead of vertical by vertical.
- **PPP is dated.** It discovers businesses, but cannot alone show that one is still operating (see "Freshness").
- **NPI, FMCSA, IRS, Form D and the city registries** add depth and current facts in their verticals.

### Deferred

- State corporation registries (no industry codes).
- ProPublica (described by ProPublica as a work in progress, and duplicated by the IRS file).
- The full monthly NPPES bulk download (very large; API slices are enough until measured otherwise, **verify** size).

## 0. Verify sources, then build the index

### `scripts/check-sources.sh`

Run this first, with network access. For each source it:

- fetches one small slice;
- asserts that every column the parser will read exists;
- prints row counts, file sizes and the date range;
- saves the real response as a fixture under `src-tauri/tests/fixtures/sources/`.

Parsers are written against those fixtures, never against documentation alone.

### Runtime schema guard

A parser that finds a missing column returns `SchemaMismatch { source, column }`. The sync for that source stops and the source shows as unavailable in Settings, with the column name. A schema change never produces an empty result that looks like "no businesses here".

### Index schema

Migration `004_prospect_index.sql`:

- **`registry_records`**
  - Columns: source, record id, record URL, name, alternate names, street, city, state, ZIP, phone, email domain, website, codes as JSON, facts as JSON, status, as-of date, entity id.
  - Indexes on `(state, zip)`, `(source, record_id)`, phone and entity id.
  - A side table `record_codes(record, system, code)` indexed on `(system, code)`, so lookups by code stay fast at millions of rows.
- **`entities`**: id, display name, best website and its provenance, best phone, ZIP, latitude and longitude, linked account id.
- **`source_syncs`**: source, slice key, state, fetched at, row count, checksum, status.

### Sync jobs

- Syncs run on the existing `jobs` queue with per-host limits.
- Bulk syncs stream and filter rows; they never hold a whole file in memory.
- `clear_prospects` keeps the index, because it is public reference data, not your prospects. `delete_all_data` clears it.
- Settings shows the index size per source, a refresh button and a delete button.

### Storage controls

- Nationwide bulk sources (PPP and EPA national) are **opt-in**, showing the download size and the expected stored size before you accept.
- Rows are filtered while streaming. For PPP that means only businesses with 2 or more jobs whose business type is not an individual. Only the columns used are stored.
- Per-state slices are the default.

## 1. One industry-code system, one geography system

### NAICS 2022

Bundle the Census NAICS 2022 code titles and index entries as static data. It is public domain, a few MB, and the source URL and exact file are **verify**.

- **Planner.** The audience clause (`ai::audience_clause`) is matched on whole words against titles and index entries, through the existing `matcher::Text`. That yields NAICS codes from the table, which appear in the plan as editable chips, for example "722511 Full-Service Restaurants ×". You can add a code by searching the table.
- **Model suggestions.** The model suggests plain phrases that are resolved against the table. A code that is not in the table is impossible.
- **Crosswalks** are curated static tables, each with tests that every code exists:
  - NAICS to NUCC taxonomy, for 621xxx healthcare;
  - NAICS to NTEE, for nonprofits;
  - NAICS to Form D industry group (about 30 groups);
  - NAICS 484/485 to FMCSA;
  - each existing OpenStreetMap category gets NAICS codes, so OpenStreetMap results join the same system.
- **Catalog.** The existing 94 categories become shortcuts that fill in NAICS codes, so nothing users rely on today is lost.

### ZIP geography

Bundle the Census ZIP code tabulation area file: ZIP plus representative latitude and longitude, public domain, **verify** size.

- **Plan geography** gains "within N miles of a ZIP or metro" alongside state and metro. Sources that filter by state receive the state; the index then filters by ZIP distance.
- **The geography gate** (`qualification::geography_matches`) compares the ZIP distance or state when the record has a ZIP. It falls back to today's place matching only when it has none.
- **OpenStreetMap** keeps its bounding boxes. Records get a ZIP from their address tags, or from the nearest ZIP to their coordinates.

## 2. Entity resolution

When a record arrives, attach it to an existing entity when any of these hold, strongest first:

1. **Same verified domain.**
2. **Same phone** (digits normalized) and the same ZIP or the same normalized name.
3. **Same normalized name and the same ZIP and street number.** Normalizing strips legal suffixes and punctuation, and expands "&".

A weaker match (name plus ZIP only) is stored as a **candidate link** shown on the account, never merged automatically.

Each entity aggregates the facts from all its records, for example an NPI record, a city licence and a PPP loan. The account the research step creates points to its entity.

Tests:

- Franchise locations with a shared name but different phones stay separate.
- One business with a changed name but the same phone and ZIP merges.
- A ZIP collision between two different businesses never merges.

## 3. Verified website resolution

This runs only for the **top-ranked entities that are about to be researched**, not for the whole pool. It runs before research in `process_search_candidate`, at most 4 seconds per entity.

### Candidate websites, in order

1. **Published:** the SAM `entityURL`, or the business email domain from FMCSA.
2. **Joined:** a website already on the entity from another record, such as an OpenStreetMap `website` tag or a Wikidata site, linked by phone or by name and ZIP.
3. **Your SearXNG:** if configured, search "name city state" and take the top public result domains.
4. **Name-derived domains:** at most 4.
   - Joined and hyphenated forms of the normalized name.
   - `.com` for companies; `.org` first for IRS nonprofits.
   - Each needs a DNS answer before any HTTP request, using the existing `validate_dns`, so non-existent domains cost one lookup.

### Verification

Every candidate is verified the same way:

- **Fetch:** robots.txt, then the homepage, then one contact or about page if needed, through the existing safety-checked fetcher and the page cache.
- **Accept only when the site shows both:**
  - every distinctive word of the business name;
  - **and** the record's phone, or its ZIP, or its street number with street name.
- **Reject:**
  - parked or for-sale pages, from a static marker list;
  - directory, marketplace and social hosts, from a static list;
  - franchise pages that show a different phone;
  - two candidates verifying equally (the company stays unresolved).

### On success

- An `identity_match` evidence row quotes the matching lines.
- Confidence is high for a phone match and medium for a ZIP or street match.
- The card shows "Website matched by name and phone".

### Failures and settings

- DNS failures and 4xx responses are cached as definitive misses, through the existing negative page cache.
- A Settings toggle, on by default, turns off name-derived domains.

### Docs and tests

- **README:** contacts are still never guessed. A website may be matched to a public record only when its homepage shows the same name and phone, ZIP or street, and the match is labelled.
- **Tests:** a fixture fetcher covering a true match, the same name in another city, a parked domain, a franchise page, a tie, and a free-mail domain. An offline floor test asserts that no wrong website is ever accepted.

## 4. Facts, size and freshness in scoring

### Registry facts are evidence

Each fact becomes a `registry_fact` evidence row with a dated, cited excerpt, for example "FMCSA census (DOT 1234567, MCS-150 updated 2026-08-01): 42 power units, 51 drivers."

`qualification::signal_matches` treats these as trusted line-level evidence, unlike today's descriptive `_record` rows. One fact is one sentence, so the one-sentence-per-rule rule still holds.

### Company size

`SearchBrief.company_sizes`, empty today, becomes editable bands: 1–10, 11–50, 51–200, 201–1,000, and 1,000+.

Proxies map to bands:

- PPP jobs reported;
- FMCSA drivers;
- IRS revenue;
- Form D revenue range;
- NPI practice-location count.

Each proxy is labelled as a proxy, with its date. A band match is an ICP feature. A mismatch fails only when you mark size as required.

### Freshness

- **Dated facts:** a fact older than 3 years (all PPP rows) is shown and can help ranking, but is labelled "as of 2021".
- **The "Still operating" gate passes only with current evidence:**
  - a website fetched in this run;
  - or an active status in a current registry: NPI, FMCSA, IRS, an active city licence, SAM.
- **Effect:** an entity known only from dated records cannot qualify, so a closed business is never presented as a strong match.

### New rule templates citing facts

| Template | Keywords |
|---|---|
| `recent_funding` | "raised", "form d" |
| `federal_contracts` | "federal contracts" |
| `fleet_size` | "power units" |
| `growing_headcount` | |

`growing_headcount` fires when the PPP job count is below a current size proxy from another source. It is computed in code, not by keyword, and only ever confirmed, never assumed.

## 5. Search over the index

### Discovery becomes a local query

Select index entities by NAICS (and crosswalked codes), geography, size and status. Exclude entities already researched in the last 30 days unless this is a re-score.

On first use of a state and code, the planner says which syncs will run and roughly how long they take. Later searches skip that wait.

### Pre-ranking replaces `discovery_priority` for index results

Rank on:

1. exact code match over a parent-code match;
2. size fit;
3. current status and recency;
4. a published or joined website, then a phone;
5. distance from the centre of the requested area.

### Pool size and pagination

- The pool grows from 100 to 1,000 ranked entities. The research time budget is unchanged, and unresearched entities stay "not checked", never "rejected".
- Pagination is a natural property of the index: the next search continues down the same ranking. This replaces `rotation` for index-backed sources.

### Live sources

OpenStreetMap and Wikidata keep running live, and also write into the index. Their records gain entity links, and their website tags help resolve others.

## 6. Interface

- **Plan screen:**
  - NAICS chips with a code search;
  - a radius control;
  - size bands;
  - one line per source showing coverage and whether a sync will run;
  - a toggle to leave a source out.
- **Settings:**
  - per source: ready, syncing, schema mismatch, or opt-in required;
  - index size and the last sync;
  - SEC contact (from the sender identity);
  - SAM.gov key;
  - the name-derived domain toggle.
- **Candidate card:**
  - every source badge;
  - a size chip with its as-of date;
  - "Still operating: confirmed by website" or "not confirmed";
  - how the website was matched.

## 7. Success measures

Measured with `live_search_yield_eval`, extended to six briefs:

| Brief | Source exercised |
|---|---|
| Restaurants within 15 miles of 78701 | PPP, OpenStreetMap |
| Dentists in Pennsylvania | NPI |
| Trucking in Texas | FMCSA |
| Food banks in Ohio | IRS |
| Software startups in California | Form D |
| Auto body shops in Georgia | EPA FRS |

Targets, with today's value recorded first as the baseline:

| Measure | Target |
|---|---|
| In-geography candidates per brief | ≥ 1,000 (today ≤ 100) |
| Time from Start to the first ranked candidate, once synced | < 2 seconds |
| Researched candidates with a website | ≥ 60% |
| Wrong websites among 30 hand-checked name-derived matches | 0 |
| Strong matches per run | higher than baseline on every brief |

Two offline floor tests must hold:

- the existing relevance floor, precision 1.00;
- the new floor of zero wrong websites accepted in `eval_fixtures.rs`.

## Build order

Value first. Each step is one commit and passes the CI checks.

1. **Source checks, index schema, NAICS and ZIP tables.** Plan geography gains radius search. OpenStreetMap results are written into the index.
2. **Entity resolution and verified website resolution.** This immediately lifts today's OpenStreetMap records that have no website.
3. **PPP sync**, opt-in. It adds the most range, along with size and the freshness gate.
4. **EPA FRS sync.**
5. **NPI and FMCSA slices.**
6. **IRS file, Form D and USAspending by NAICS.** Adds the `csv` and `zip` crates.
7. **City registries, and SAM.gov as an optional key.**
8. **Size bands, freshness gate, index-based pre-ranking, 1,000-entity pools, and the interface.**
9. **README and SECURITY, then live evaluation against the targets.**

## Hosts to allow before step 0

- `data.sba.gov`
- `ordsext.epa.gov` and `www.epa.gov`
- `npiregistry.cms.hhs.gov`
- `data.transportation.gov`
- `www.irs.gov`
- `www.sec.gov`
- `api.usaspending.gov`
- `www.census.gov` (NAICS and ZIP files)
- `data.lacity.org`, `data.sfgov.org`, `data.seattle.gov`, `data.cityofchicago.org`
- `api.sam.gov` (optional)
- `overpass-api.de`, `query.wikidata.org` and `www.wikidata.org` for the existing live evaluations

## Decisions for you

- **PPP as an opt-in nationwide download.** The size is **verify**. Recommendation: keep it opt-in, with the size shown first.
- **Bundle the NAICS and ZIP tables in the app, or download them on first use.** Recommendation: bundle them. They are small and public domain, and keep planning offline.
- **Pool size of 1,000.** Larger pools only help with better ranking, so step 8 should be tuned on the live evaluation before raising it further.

## Risks

- **Wrong website matches.** Mitigated by requiring name plus phone, ZIP or street, by refusing ties, by labelled evidence and by the zero-wrong floor test. Name-derived domains can be turned off.
- **Closed businesses from dated data.** The freshness gate means they cannot qualify. They only rank.
- **Disk use.** Opt-in national files, streamed filtering, stored columns kept to those used, and per-source delete.
- **Entity resolution errors.** Merges need phone or address agreement. Weaker links are shown, never merged.
- **Licensing.** Federal data is public domain. OpenStreetMap is ODbL and already attributed. Wikidata is CC0. City portal licences: **verify** per portal.
