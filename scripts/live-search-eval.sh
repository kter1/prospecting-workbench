#!/usr/bin/env bash
set -euo pipefail

mkdir -p artifacts
cargo test --manifest-path src-tauri/Cargo.toml live_search_yield_eval -- --ignored --nocapture 2>&1 | tee "artifacts/live-search-eval-$(date +%Y%m%d-%H%M%S).txt"
