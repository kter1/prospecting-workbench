# Security policy and threat model

## Data boundary

Prospecting data is stored only in the local SQLite application database. There is no application server, account system, telemetry, cloud sync, or hosted inference. Exports are written to a local `exports` directory and may contain sensitive sales research; users must protect them accordingly.

## Web content boundary

The crawler accepts only public HTTP/HTTPS content on standard ports. It blocks credentials in URLs, local/special hosts, private and non-routable IPs, unsafe redirects, active content, unsupported media, and responses over 2 MB. Redirect targets are revalidated. Robots exclusions and `Retry-After` are honored, and robots.txt is always read before the pages it governs. Raw markup is used only to fingerprint known technologies from attribute values and is then discarded. Sanitized page text (active and hidden content removed, at most 160 lines) is cached locally for 24 hours of reuse and purged after 7 days or with its account, so repeat searches and re-scoring need no new requests. Cached text is used only for deterministic matching and is never sent to the model.

DNS rebinding is a residual risk because validation and the HTTP client resolve independently. Do not research domains controlled by an adversary from a network that exposes sensitive services. A future hardening change should pin the validated address for each connection.

Configured loopback SearXNG is a narrow, explicit exception to general crawler host blocking. Its result URLs still pass the public-URL validator before import.

## Model boundary

The local model receives sanitized page excerpts enclosed as untrusted quoted evidence. It has no tool, network, or filesystem access. The model cannot construct source queries or numeric fit scores. While a plan is being edited it may suggest qualification rules from the user's own description, never from page content; suggestions are plain keyword phrases sanitized like any user edit, their points are set in code, and they apply only once the user starts the search. Technology evidence is written only from a static fingerprint table, never from page text. source categories are selected from a curated ontology and scoring is deterministic application code. Structured output, evidence IDs, word limits, hard gates, and a second verification pass are enforced before an artifact is stored. Unsupported claims are removed only when an exact draft substring can be removed safely; otherwise publication is blocked.

OpenStreetMap discovery uses bounded, curated Overpass queries. Arbitrary model-generated Overpass is rejected. Public Nominatim is not used for systematic POI enumeration.

## Reporting

Do not include real prospect data in a public report. Provide a minimal fixture, platform/version information, and the relevant error or safety behavior through the repository’s private security-reporting channel when one is available.
