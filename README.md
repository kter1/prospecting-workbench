# Prospecting Workbench

**A local-first desktop app that finds U.S. companies matching a plain-language description, gathers public evidence for each one, and refuses to state anything it cannot cite.**

Describe what you sell and who should care. The app compiles a search plan you can read and edit, discovers candidate companies from free public sources, reads their public web pages, scores fit with fixed rules in application code, and drafts outreach with a local 9B model — where every factual sentence must quote a stored evidence row or it is stripped before you see it.

Everything runs on your machine. No account, no server, no telemetry, no paid data vendor, no hosted inference.

![Discover view with a compiled search plan](docs/screenshots/01-discover.png)

---

## Why this was interesting to build

The easy version of this product is a wrapper around a paid contact database and a chat model. That version invents contact details, manufactures reasons to reach out, and cannot tell you where anything came from.

The constraint I set instead was: **the app may only assert what it can cite, and must say plainly when it finds nothing.** That one rule drove nearly every decision below and turned the project into an engineering problem rather than a prompt.

| Decision | Why |
| --- | --- |
| Scoring lives in Rust, never in the model | A model asked for a number can be argued into any number. Weights are fixed at 35/30/20/15 and auditable. |
| The model may write, never assert | A second verification pass checks each drafted claim against evidence IDs. Unsupported sentences are removed; a draft left with no cited reason is refused outright. |
| Web pages are untrusted input, not context | Fetched HTML is stripped of active and hidden elements, scanned for embedded-instruction patterns, and quoted to the model as data. |
| "Not publicly found" is a real answer | Missing contact data is shown as missing. Nothing is pattern-guessed from a domain. |
| Sources must be free and rule-abiding | OpenStreetMap and Wikidata, both with published usage policies the client honours: bounded sampling, backoff, caching, mirrors. No search-result pages are ever scraped. |

## What it does

**1 — Compile a plan you can read before anything runs.**
A deterministic compiler turns your description into target segments, qualification signals, geography, and named discovery lanes. It runs offline and instantly, and lists every assumption it made so you can correct it. Nothing touches the network until you approve.

**2 — Discover from open sources.**
Two automatic lanes with different shapes. OpenStreetMap covers businesses with a mapped premise — 94 curated verticals across all 50 states and ~50 metro areas. Wikidata covers organizations with no storefront at all: agencies, studios, firms. A third optional lane uses a SearXNG instance you run yourself.

![Ranked results split into strong and near matches](docs/screenshots/02-results.png)

**3 — Research and qualify, with citations.**
Each candidate's public pages are crawled within robots.txt, and focused excerpts become evidence rows. Fixed-weight scoring produces a fit score; only candidates within four points of the best eligible score become Strong Matches. Everything else stays visible as a near match with its reasons attached.

![Evidence ledger showing sourced excerpts](docs/screenshots/03-evidence.png)

**4 — Draft, verified.**
A pinned local `qwen3.5:9b` composes an email and call brief through Ollama's structured output. A separate pass re-checks every claim against the evidence. The compliance footer is assembled from your own approved sender identity. The app never sends anything.

## Engineering worth a look

**Untrusted input handling** — [`safety.rs`](src-tauri/src/safety.rs) enforces scheme, port, credential and private-address policy, re-validating on every redirect hop, with content-type and size ceilings. Injection markers are stripped from page text before it reaches the model.

**Source-query safety** — generated text never becomes a source query. Overpass filters are `&'static str` literals in [`catalog.rs`](src-tauri/src/catalog.rs). The SPARQL in [`wikidata.rs`](src-tauri/src/wikidata.rs) is a fixed template whose only variable is an entity id validated against `^Q[1-9][0-9]*$`. The local model contributes plain-English type names, resolved through Wikidata's own search API before anything reaches a query.

**Evidence provenance** — a mapped business with no address tags records its location as the bounded region it was found in, labelled as established by the search area rather than by a published address. Proven and inferred facts stay distinguishable in the data model.

**Honest failure reporting** — a search stopped by its time budget reports how many candidates went unresearched instead of blaming its own scoring band. Rate-limited sources fall back to cache with the cache age shown.

**Testing** — parser fixtures cover malformed, duplicate, and injection-bearing payloads for every public source. Several tests encode measured findings from the live services, so a regression fails by name rather than vaguely.

## Architecture

```
React + TypeScript (Vite)          strict TS, no state library
        │  Tauri IPC — typed commands, scoped capabilities
        ▼
Rust core
  ├── catalog.rs / regions.rs   curated verticals + U.S. bounding boxes
  ├── adapters.rs               OpenStreetMap, SEC, USAspending, SearXNG
  ├── wikidata.rs               open-ended organization discovery
  ├── safety.rs                 fetch policy, SSRF defence, sanitisation
  ├── research.rs               bounded crawl → evidence extraction
  ├── qualification.rs          fixed-weight scoring, hard gates
  ├── ai.rs                     Ollama structured output + verification
  └── db.rs                     SQLite, migrations, serialized writes
        │
        ▼
SQLite (app data dir)   ·   Ollama @ 127.0.0.1:11434
```

~11k lines of Rust, ~1.6k of TypeScript, 104 tests. CI runs both suites, a production build, `cargo fmt --check`, `clippy -D warnings`, and generates an SPDX SBOM.

## Running it

Requires macOS 13+, Node 22+, Rust 1.80+, and [Ollama](https://ollama.com/).

```sh
ollama pull qwen3.5:9b
corepack enable
pnpm install
pnpm tauri dev
```

`pnpm tauri build` produces a standalone `.app`. Discovery and research need internet access; stored records and local drafting work offline.

> **macOS note:** keep the repository outside `~/Documents`, `~/Desktop`, and `~/Downloads`. macOS tags executables in those folders with a `com.apple.provenance` attribute that blocks the hardlinks cargo uses for build scripts, and Node fails at startup with `EPERM: uv_cwd` unless the terminal has been granted access to the folder.

## Deliberate non-goals

It does not send email, place calls, guess addresses, scrape LinkedIn or search-result pages, use paid databases, collect telemetry, or fall back to hosted AI. Coverage is bounded by what free public sources actually publish, and the app says so rather than filling gaps with plausible guesses. Outreach material is prepared for a human to review, edit, and send; the operator remains responsible for complying with applicable law.

## Status and next steps

Working end to end on macOS. Known work in progress: run-view pagination for very large result sets, per-request discovery caching, and widening the qualification signal vocabulary beyond local service trades — a talent agency's website does not say "24/7 emergency service," so the scoring layer currently under-serves verticals the discovery layer can now reach.

See [SECURITY.md](SECURITY.md) for the threat model and [CONTRIBUTING.md](CONTRIBUTING.md) for development conventions. Apache-2.0.
