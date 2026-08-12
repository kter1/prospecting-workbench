#!/usr/bin/env bash
set -euo pipefail

mkdir -p artifacts
cargo test --manifest-path src-tauri/Cargo.toml real_qwen_golden_release_gate -- --ignored --nocapture 2>&1 | tee artifacts/real-model-golden.txt
