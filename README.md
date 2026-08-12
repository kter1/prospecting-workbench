# Prospecting Workbench

A local-first desktop application for B2B prospect research. You describe what you sell in plain language; it finds U.S. companies that match, collects public evidence about each one, and will not state anything it cannot attribute to a source.

The app compiles a search plan you can read and edit before anything runs. It then discovers candidates from free public sources, reads their public web pages, scores fit using fixed rules in application code, and drafts outreach with a local 9B model. Every factual sentence in a draft must quote a stored evidence row, or it is removed before you see it.

Everything runs on your machine. There is no account, server, telemetry, paid data vendor, or hosted inference.

![A qualified prospect with its fit score, matched features and evidence count](docs/screenshots/qualification.png)

## Design constraints

The obvious way to build this is to wrap a paid contact database and a chat model. That approach invents contact details, manufactures reasons to reach out, and cannot show where any of it came from.

The rule I worked to instead was that the application may only assert what it can cite, and must say so plainly when it finds nothing. Most of the design follows from that.

Scoring lives in Rust rather than in the model. A model asked to produce a number can be argued into producing any number, so the weights are fixed at 35/30/20/15 and can be audited.

The model writes, but it does not assert. A second pass checks each drafted claim against evidence IDs, removes sentences it cannot support, and refuses a draft entirely if nothing cited survives.

Fetched web pages are treated as untrusted input rather than context. HTML is stripped of active and hidden elements, scanned for embedded instruction patterns, and quoted to the model as data.

Missing contact information is shown as missing. Nothing is guessed from a domain pattern.

Sources have to be free and rule-abiding. OpenStreetMap and Wikidata both publish usage policies, and the client honours them through bounded sampling, backoff, caching and mirror failover. Search result pages are never scraped.

## How it works

**Compiling a plan.** A deterministic compiler turns your description into target segments, qualification signals, geography and named discovery lanes. It runs offline and returns immediately, and it lists the assumptions it made so you can correct them. Nothing touches the network until you approve the plan.

**Discovery.** Two automatic lanes cover different shapes of company. OpenStreetMap handles businesses with a mapped physical premise, using a curated catalog of 94 verticals across all 50 states and roughly 50 metro areas. Wikidata handles organizations that have no storefront, such as agencies, studios and professional firms. A third optional lane uses a SearXNG instance you run yourself.

![A search running against public sources, streaming candidates as it goes](docs/screenshots/live-search.png)

**Research and qualification.** Each candidate's public pages are crawled within the limits of its robots.txt, and focused excerpts become evidence rows. Fixed-weight scoring produces a fit score. Only candidates within four points of the best eligible score are promoted to Strong Match; the rest stay visible as near matches with their reasons attached.

**Drafting.** A pinned local `qwen3.5:9b` composes an email and call brief using Ollama's structured output. A separate pass re-checks every claim against the evidence. The compliance footer is built from the sender identity you approved. The application never sends anything.

## Implementation notes

*Untrusted input.* [`safety.rs`](src-tauri/src/safety.rs) enforces scheme, port, credential and private-address policy, and revalidates on every redirect hop. Content type and response size are capped. Injection markers are stripped from page text before it reaches the model.

*Source query safety.* Generated text never becomes a source query. Overpass filters are `&'static str` literals in [`catalog.rs`](src-tauri/src/catalog.rs). The SPARQL in [`wikidata.rs`](src-tauri/src/wikidata.rs) is a fixed template whose only variable is an entity id validated against `^Q[1-9][0-9]*$`. The local model contributes plain English type names, which are resolved through Wikidata's own search API before anything reaches a query.

*Evidence provenance.* A mapped business with no address tags records its location as the bounded region the query proved it was in, labelled as established by the search area rather than by a published address. Facts that were proven stay distinguishable from facts that were inferred.

*Failure reporting.* A search stopped by its time budget reports how many candidates went unresearched instead of attributing the shortfall to its own scoring band. Rate-limited sources fall back to cache, with the cache age shown.

*Testing.* Parser fixtures cover malformed, duplicate and injection-bearing payloads for every public source. Several tests encode measured behaviour of the live services, so a regression fails by name rather than vaguely.

## Architecture

```
React + TypeScript (Vite)          strict TS, no state library
        |  Tauri IPC: typed commands, scoped capabilities
        v
Rust core
  |-- catalog.rs / regions.rs   curated verticals, U.S. bounding boxes
  |-- adapters.rs               OpenStreetMap, SEC, USAspending, SearXNG
  |-- wikidata.rs               open-ended organization discovery
  |-- safety.rs                 fetch policy, SSRF defence, sanitisation
  |-- research.rs               bounded crawl, evidence extraction
  |-- qualification.rs          fixed-weight scoring, hard gates
  |-- ai.rs                     Ollama structured output, verification
  |-- db.rs                     SQLite, migrations, serialized writes
        |
        v
SQLite (app data dir)      Ollama @ 127.0.0.1:11434
```

Roughly 11k lines of Rust and 1.6k of TypeScript, with 104 tests. CI runs both test suites, a production build, `cargo fmt --check` and `clippy -D warnings`, and generates an SPDX SBOM.

## Requirements and setup

macOS 13 or later, Node 22+, Rust 1.80+, and [Ollama](https://ollama.com/).

```sh
ollama pull qwen3.5:9b
corepack enable
pnpm install
pnpm tauri dev
```

`pnpm tauri build` produces a standalone `.app`. Discovery and research require internet access; stored records and local drafting work offline.

Keep the repository outside `~/Documents`, `~/Desktop` and `~/Downloads`. macOS tags executables in those directories with a `com.apple.provenance` attribute that blocks the hardlinks cargo uses for build scripts, and Node will fail at startup with `EPERM: uv_cwd` unless the terminal has been granted access to the folder.

## Scope and limitations

The application does not send email, place calls, guess addresses, scrape LinkedIn or search result pages, use paid databases, collect telemetry, or fall back to hosted AI. Coverage is bounded by what free public sources actually publish, and the app reports that rather than filling gaps with plausible guesses. Outreach material is prepared for a person to review, edit and send, and the operator remains responsible for complying with applicable law.

## Project status

Working end to end on macOS. Outstanding work includes pagination in the run view for very large result sets, per-request discovery caching, and widening the qualification signal vocabulary beyond local service trades. A talent agency's website does not advertise "24/7 emergency service," so the scoring layer currently under-serves verticals that the discovery layer can now reach.

See [SECURITY.md](SECURITY.md) for the threat model and [CONTRIBUTING.md](CONTRIBUTING.md) for development conventions. Licensed under Apache-2.0.
