# Plan: registry discovery sources

Status: proposed, not started. Written without network access to any source below, so every endpoint, column name and limit marked **verify** comes from documentation or search results and must be checked against a live response before code relies on it.

## Why

Automatic discovery has two sources today, and both see a narrow slice of US businesses:

- **OpenStreetMap**: mapped premises only, sampled across about 50 metros.
- **Wikidata**: notable organizations only.

The SEC and USAspending adapters resolve names; they cannot discover. A run is capped at 100 candidates (`execute_search_run`, `candidate_budget`).

The sources below are free government or open-data registries. Each one tags its records with an industry code, so a source can be selected by code instead of by text. Most do not publish a website, so this plan also adds **verified website resolution**: without a website the research step has nothing to read, and today such candidates stop at "needs review".

## Invariants this plan keeps

- **No generated source queries.** Every query is a fixed template in code. Its only variables are industry codes stored as literals in `catalog.rs`, state codes from `regions.rs`, and integer limits and offsets. User text, model output and registry text never enter a query.
- **Provenance.** Each candidate cites its registry record URL. A location comes from the record. A website found by resolution carries an evidence row saying how it was matched.
- **Politeness.** Each source has a rate limit and uses the existing source backoff (`db::set_source_backoff`) and discovery cache. Bulk files are downloaded once per refresh period. SEC requests declare a User-Agent with a contact address, as SEC requires.
- **No personal data.** NPI uses organizational records (NPI-2) only. Records that look like individuals (a registrant name that is a person's name with a single power unit, for example) are skipped. Free-mail addresses are never stored as contacts; the existing same-domain contact rule still applies.

## 1. Source framework

New module directory `src-tauri/src/sources/`, with one file per source plus shared helpers.

### The source trait

```rust
#[async_trait]
pub trait RegistrySource: Send + Sync {
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// Whether any matched category maps to this source's codes.
    fn covers(&self, categories: &[&'static Category]) -> bool;
    async fn discover(&self, ctx: &SourceContext<'_>, request: &SourceRequest) -> AppResult<SourceBatch>;
}
```

- `SourceRequest` holds the matched categories, a `regions::Place` (state and optional metro), a page cursor and a limit.
- `SourceBatch` returns:
  - `results`;
  - `exhausted`;
  - warnings;
  - a schema check result: a parser that finds a missing column returns `SchemaMismatch { source, column }` instead of an empty list.

### `DiscoveryResult` gains structured fields

All new fields are `#[serde(default)]`, so cached and exported results still load:

- `address: Option<PostalAddress>` (street, city, state, ZIP);
- `email_domain: Option<String>` (a business domain only, never free-mail);
- `industry_codes: Vec<String>`;
- `registry_facts: Vec<RegistryFact>`, where `RegistryFact { kind, label, value, as_of }` and `kind` is one of `size`, `founded`, `funding`, `locations`, `licence`, `contracts`.

### Shared helpers

- `sources/socrata.rs` builds SoQL from a static per-dataset column map. Column names come only from the map; values are codes, state codes or integers.
  - SoQL prefix matching (`starts_with` versus `like 'code%'`): **verify**.
- `sources/bulk.rs` downloads bulk files into the app data directory, records a checksum and fetch date, and indexes the rows into a new table `registry_records(source, record_id, state, code, name, zip, payload_json, as_of)` with indexes on `(source, state, code)`.
  - Migration `004_registry_records.sql`.
  - `clear_prospects` leaves this table alone, because it is public reference data, not prospect data. `delete_all_data` clears it.

### Planner and runner

- **Planner (`ai::interpret_search_brief`).** Add an automatic lane for each source whose `covers` is true, with its reason and limitation, as the existing lanes do.
- **Runner (`execute_search_run` / `discover_candidates`).** Replace the per-source `match` with a dispatch over the registered sources. Port OpenStreetMap and Wikidata onto the trait last, once the new sources have proven the shape.
- **Capabilities.** `db::source_capabilities` lists every source, its readiness, and the reason when one is disabled (missing SEC contact, missing SAM key, failed schema check).

## 2. Industry codes in the catalog

### New `Category` fields

Each `Category` in `catalog.rs` gains literal code lists:

- `naics: &[&str]`: 2–6 digit prefixes, used by city registries and USAspending.
- `npi_taxonomies: &[&str]`: NUCC taxonomy descriptions, for healthcare categories.
  - Exact-match versus trailing-wildcard behaviour of `taxonomy_description`: **verify**.
- `ntee: &[&str]`: NTEE major groups or codes, for nonprofit categories.
- `form_d_industries: &[&str]`: Form D industry group values.
- `fmcsa: bool`: for trucking and bus companies.

### New categories

Add categories that only the new sources reach:

- trucking and freight carriers;
- bus and motorcoach operators;
- food banks, charities by type, religious organizations, private schools;
- venture-backed private companies by Form D industry group;
- federal contractors by NAICS.

`filters` (Overpass) may now be empty for these. The catalog test changes from "every category has an Overpass filter" to "every category is reachable by at least one source".

### Tests

- Every NAICS literal is 2–6 digits.
- Every taxonomy and NTEE literal matches its pattern.
- Every category is reachable by at least one source.
- `match_query` still only matches whole words.

## 3. The sources

Each source is its own commit, with a parser fixture test, a schema-mismatch test, and an `#[ignore]` live test that `scripts/check-sources.sh` runs.

### 3a. CMS NPI Registry (healthcare organizations)

- **Endpoint:** `GET https://npiregistry.cms.hhs.gov/api/?version=2.1&enumeration_type=NPI-2&taxonomy_description=<catalog literal>&state=<SS>&city=<metro city>&limit=200&skip=<0..1000>`.
  - At most 200 per request and a skip of at most 1,000, so 1,200 per query.
  - Statewide coverage iterates the metro cities `regions.rs` already knows for that state, then the remaining state query.
  - Exact value of `enumeration_type` ("NPI-2" versus "org"): **verify**.
- **Mapping:**
  - organization name, and any "doing business as" names;
  - practice location address and phone (the `LOCATION` address);
  - taxonomies, primary first.
- **Registry facts:**
  - "Registered provider type: General Practice Dentistry";
  - "N practice locations registered under this organization", counted by deduplicating NPIs on name plus phone.
- **Politeness:** no published limit, so one request per second; cache for 7 days per (taxonomy, state, city, skip).
- **Website:** none, so resolution runs on every record.

### 3b. FMCSA Company Census (trucking and bus companies)

- **Endpoint:** Socrata dataset `az4n-8mr2` on data.transportation.gov.
  - Template filters on physical state, active status, operation type and a minimum number of power units.
  - Ordered by DOT number; paged by `$offset`.
  - Column names (`phy_state`, `power_units`, `total_drivers`, `email_address`, `phone`) and status code meanings: **verify** via `columns.json`.
- **Registry facts:**
  - "42 power units, 51 drivers (MCS-150 updated 2026-08-01)";
  - interstate or intrastate operation;
  - hazmat flag.
- **Website:** the business email's domain when it is not free-mail.

### 3c. City business registries (NAICS-coded licences)

One Socrata adapter with a static dataset table of `{portal, dataset id, metro, column map, active filter}`:

| City | Dataset | Notes |
|---|---|---|
| Los Angeles | `6rrh-rzua` | reported ~634k rows |
| San Francisco | `g8m3-pdis` | NAICS on only about a third of rows (third-party figure): **verify** |
| Seattle | `wnbq-64tb` | |
| Chicago | business licences | Licence descriptions instead of NAICS, so it needs a static description map |

- **Selection:** used only when the requested geography resolves to one of these metros.
- **Registry facts:**
  - "Licensed in Los Angeles since 2014";
  - the NAICS description.
- **Licences:** the open-data licence of each portal: **verify**.

### 3d. IRS exempt-organization file (nonprofits)

- **Download:** the per-state CSV from the EO BMF page. The URL pattern (`eo_<ss>.csv` or similar) and the columns (EIN, NAME, STREET, CITY, STATE, ZIP, NTEE_CD, SUBSECTION, RULING, ASSET_AMT, INCOME_AMT, REVENUE_AMT): **verify**.
- **Indexing:** downloaded on first use per state, refreshed every 30 days, indexed into `registry_records`, and filtered locally by NTEE.
- **Data quality:** NTEE codes are reported to be noisy, so match on the major group, not the full code.
- **Registry facts:**
  - "Reported revenue $1.2M (IRS file, 2026)";
  - "Tax-exempt 501(c)(3) since 1998".
- **Dependency:** the `csv` crate.

### 3e. SEC Form D data sets (private companies that raised money)

- **Download:** quarterly ZIPs of about 3 MB. Keep the last 8 quarters and index them locally.
  - URL pattern, tab delimiter, and the columns ISSUERS (`ENTITYNAME`, street, city, `STATEORCOUNTRY`, ZIP, phone, `ENTITYTYPE`, year of incorporation) and OFFERING (`INDUSTRYGROUPTYPE`, `REVENUERANGE`, offering and sold amounts): **verify** against the Form D readme PDF.
- **Filtering:**
  - primary issuer only;
  - drop pooled investment funds;
  - match state and catalog industry group.
- **Registry facts:**
  - "Raised $2.4M of a $3M offering, filed 2026-06-12", which is also a growth signal;
  - "Revenue range: $1M–$5M";
  - "Incorporated 2021".
- **User-Agent:** requires a contact address. Use the sender email from Setup; with none, the source is disabled and the capability says why.
- **Dependency:** the `zip` crate.

### 3f. USAspending by industry (federal contractors)

- **Change:** replace the name-autocomplete adapter with `POST /api/v2/search/spending_by_award/`.
  - Filters: NAICS codes, contract award types A–D, recipient or place-of-performance state, last two fiscal years.
  - Exact names of the location filter and the NAICS filter shape: **verify**.
- **Aggregation:** group results per recipient.
- **Registry facts:** "7 federal contracts totalling $1.8M since FY2025".

### 3g. SAM.gov entity data (optional, needs a key)

- **Configuration:** a personal API key entered in Settings, stored like the SearXNG URL.
- **Value:** filters by NAICS and state, and publishes `entityURL`, the only registry here with a website.
- **Limits:** personal keys reportedly allow very few requests per day. Count usage locally and stop at the limit. API version, parameter names and daily limit: **verify**.

## 4. Verified website resolution

New module `identity.rs`. It runs in `process_search_candidate` before research, for any candidate without a website, including today's OpenStreetMap records without a `website` tag. That makes it useful before any new source ships.

### Candidate websites, in order

1. **Published by the record:** the SAM `entityURL`, or the business email domain.
2. **OpenStreetMap match:** match the record against the OpenStreetMap elements already cached for that area, by normalized name and the same ZIP or street number, and take their `website` tag. No new Overpass query.
3. **User's SearXNG:** if configured, search "name city state" and take the top public result domains. This is the user's own search instance, and the name comes from a public record.
4. **Name-derived domains:** at most 4, `.com` only.
   - Normalize the legal and trade names: drop LLC, Inc, Corp, Co, The; turn "&" into "and"; drop punctuation.
   - Try the name joined (`bartontaqueria.com`) and hyphenated (`barton-taqueria.com`).

### Verification

Every candidate, from any of the four routes, is verified before it is accepted:

- **Fetch:** robots.txt, then the homepage, then one contact or about page if needed. Use the existing safety-checked fetcher and the page cache, with at most 4 seconds per company.
- **Accept only when the site shows both:**
  - every distinctive word of the business name;
  - **and** the record's phone number (digits normalized), or its ZIP, or its street number with street name.
- **Reject:**
  - parked or for-sale pages (a static marker list);
  - directory and social hosts (a static host list);
  - pages that name a different city than the record and show no matching phone.
- **Ambiguity:** if two candidates verify, prefer the one with a phone match. If they still tie, leave the company unresolved rather than choose.

### On success

- Set the account's website.
- Store an `identity_match` evidence row quoting the matching lines, for example "Homepage names 'Barton Taqueria' and lists (512) 555-0100, matching the FMCSA record." Confidence is high for a phone match and medium for a ZIP match.
- The candidate card shows how the website was matched.

### Failures and settings

- DNS failures and definitive misses go into the page cache's negative entries, so a re-score never retries them.
- A Settings toggle, on by default, turns off name-derived domains (route 4).

### Docs

The README line "Nothing is guessed from a domain pattern" refers to contacts and stays true for contacts. Add a sentence: a website may be matched from a public record only when its homepage shows the same name and phone, ZIP or street, and the match is labelled as such.

### Tests (fixture fetcher)

- A true match on name and phone is accepted.
- The same name in another city is rejected.
- A parked domain is rejected.
- A franchise location page with a different phone is rejected.
- Two verified candidates leave the company unresolved.
- A free-mail domain is never a candidate.

## 5. Registry facts as evidence, and company size

### Evidence rows

`import_discovery_result` writes one `registry_fact` evidence row per fact. Its excerpt is built from the record and cites the record URL.

`qualification::signal_matches` treats `registry_fact` as trusted, line-level evidence, unlike the descriptive `_record` rows. Facts are authoritative and one fact is one sentence, so the one-sentence-per-rule rule still holds.

### Company size in the plan

`SearchBrief.company_sizes`, which is empty today, becomes editable in the plan as bands: 1–10, 11–50, 51–200, 201–1,000, and 1,000+.

Size proxies come from:

- FMCSA drivers;
- IRS and Form D revenue;
- NPI location count;
- city licence counts across locations.

Each proxy is labelled as a proxy.

| Outcome | Effect |
|---|---|
| A matching band | An ICP feature, "Size 11–50 (FMCSA drivers)" |
| No size evidence | Neutral |
| A size outside the bands | A hard-gate failure only when the user marks size as required; otherwise no points |

### New rule templates citing facts

- `recent_funding`: keywords "raised", "form d".
- `federal_contracts`: keywords "federal contracts".
- `fleet_size`: keyword "power units".

Add them to the relevant packs.

## 6. Larger pools and better pre-ranking

- **Pool size.** The candidate cap rises from 100 to 300 when a registry lane runs. The research time budget is unchanged, so the warning about candidates that were never researched still applies.
- **Pre-ranking (`discovery_priority`).** Before research, rank on structured facts:
  - exact industry-code match;
  - size fit;
  - recency (licence start, MCS-150 update, Form D filing date);
  - published website, then phone.
- **Lane interleaving.** Round-robin across sources, so no single source fills the pool.
- **Cursors.** A new table, `source_cursors(brief_key, source, geography, cursor)`, lets a repeat search continue where the last one stopped. This replaces the single `rotation` number for registry sources.
- **Cross-source deduplication.** Match on domain, then normalized name plus ZIP, then phone. Facts from every source merge onto one account, for example an NPI record plus a city licence.

## 7. Interface

- **Plan screen:**
  - each new lane with what it contributes;
  - a toggle to leave a lane out;
  - the size band editor.
- **Settings:**
  - SEC contact (taken from the sender identity);
  - SAM.gov key;
  - the name-derived domain toggle;
  - each source's readiness and its last schema check.
- **Candidate card:**
  - every source badge;
  - a size chip;
  - "Website matched by name and phone".

## 8. Verification and evaluation

- **Run before code relies on any field.** `scripts/check-sources.sh` calls each source once and asserts the columns its parser uses exist. It prints row counts and saves one real response per source as the parser fixture, replacing the documentation-shaped fixtures written before network access.
- **Runtime schema guard.** A parser that finds a missing column disables its lane for the run, with a warning naming the column. It never returns a silently empty result.
- **Live evaluation.** Extend `live_search_yield_eval` with one brief per source:

  | Brief | Source exercised |
  |---|---|
  | Dentists in Pennsylvania | NPI |
  | Trucking in Texas | FMCSA |
  | Food banks in Ohio | IRS |
  | Software startups in California | Form D |
  | Restaurants in Los Angeles | city registry |
  | Federal IT contractors in Virginia | USAspending |

  Report:
  - candidates per source;
  - how each website was resolved (published, OpenStreetMap, SearXNG, name-derived, unresolved);
  - a hand-checked precision of 20 name-derived website matches.
- **Offline evaluation.** Add website-resolution cases to `eval_fixtures.rs`, with a floor test: no wrong website is ever accepted.

## Sequencing

Each step is one commit and passes the CI checks.

1. Framework, catalog codes, `DiscoveryResult` fields and registry-fact evidence. No new network calls.
2. Verified website resolution. It immediately helps OpenStreetMap records without websites.
3. NPI Registry.
4. FMCSA census.
5. City registries.
6. IRS exempt-organization file (adds the `csv` crate).
7. SEC Form D (adds the `zip` crate).
8. USAspending by industry.
9. SAM.gov, optional.
10. Size bands, pre-ranking, cursors and lane interleaving.
11. Interface, README and SECURITY updates, and live evaluation runs.

## Hosts to allow before building

- `npiregistry.cms.hhs.gov`
- `data.transportation.gov`
- `www.irs.gov`
- `www.sec.gov`
- `api.usaspending.gov`
- `data.lacity.org`
- `data.sfgov.org`
- `data.seattle.gov`
- `data.cityofchicago.org`
- `api.sam.gov` (if 3g is wanted)
- `overpass-api.de`, `query.wikidata.org` and `www.wikidata.org` for the existing live evaluations

## Risks

- **Wrong website matches.** Mitigated by requiring name plus phone, ZIP or street, by refusing ties, and by the labelled evidence. Measured by the hand-checked live sample. Name-derived domains can be turned off.
- **Stale registry rows (closed businesses).** Mitigated by filtering on active status, and by registry dates driving recency in pre-ranking.
- **More candidates than research time.** Pre-ranking decides which are researched. Unresearched candidates are reported as unchecked, never as rejected.
- **Licensing.** Federal data is public domain. OpenStreetMap is ODbL and already attributed. Wikidata is CC0. City portal licences: **verify** per portal.
