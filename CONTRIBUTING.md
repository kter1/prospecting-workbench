# Contributing

Prospecting Workbench accepts changes that preserve its free, local-first, evidence-only boundary.

## Setup

1. Install Node.js 22+, pnpm 11+, Rust, and the Tauri prerequisites for macOS.
2. Run `pnpm install`.
3. Run `pnpm check` before opening a change.

## Non-negotiable product rules

- No paid API, hosted inference, telemetry, analytics, login, cloud sync, native sending, or automated dialing.
- No guessed email, inferred phone classification, paywall bypass, authenticated-page access, LinkedIn scraping, or search-result-page scraping.
- Every saved company claim, signal, or contact must cite an existing evidence ID.
- Treat fetched content as untrusted. The model gets no tools, network, or filesystem access.
- Keep authoritative-source and live-adapter tests opt-in. CI must be deterministic.

## Architecture

TypeScript contracts live in `src/types.ts`; their Rust equivalents live in `src-tauri/src/models.rs`. Schema changes must update both and include a migration. SQLite is accessed through one serialized writer. Crawling belongs in `safety.rs` and `research.rs`; source-specific discovery belongs in `adapters.rs`; scoring belongs in `qualification.rs`, and artifact verification in `local_model.rs`.

New adapters must fail independently, document rate limits, cap responses, and include recorded fixtures covering malformed, stale, duplicated, rate-limited, and injection-bearing data.

## Release checklist

- Frontend tests and production build pass.
- Rust tests pass with zero warnings.
- The SPDX SBOM is regenerated with `pnpm run generate:sbom`.
- `pnpm run eval:golden` reports 100% schema-valid output and zero unsupported prospect-specific claims; publish `artifacts/real-model-golden.txt` with the build.
- The 25-account reference run yields at least 10 review-ready packages within 30 minutes on the reference Apple Silicon machine.
- Checksums are generated for unsigned macOS artifacts.
