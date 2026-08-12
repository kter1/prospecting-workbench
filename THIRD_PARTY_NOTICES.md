# Third-party notices

Prospecting Workbench is Apache-2.0 licensed. It depends on open-source Rust and JavaScript packages whose exact versions are locked in `src-tauri/Cargo.lock` and `pnpm-lock.yaml`.

Principal runtime components include Tauri, React, SQLite (bundled through rusqlite), reqwest, scraper, and their transitive dependencies. Local inference uses Ollama and the separately downloaded `qwen3.5:9b` model; their licenses remain their own. No paid prospecting data service or Apollo.io component is included.

Run `pnpm run generate:sbom` to generate the complete SPDX package inventory from both lockfiles before release. The generated artifact is the authoritative version-specific notice input.

Optional automatic local-business discovery uses OpenStreetMap data, which is made available under the Open Data Commons Open Database License (ODbL). Interfaces and exports containing OpenStreetMap-derived records must credit OpenStreetMap and its contributors and make the ODbL availability clear. See https://www.openstreetmap.org/copyright.
