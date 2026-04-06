# Changelog

All notable changes to Memovyn will be documented in this file.

The format is based on Keep a Changelog and this project follows Semantic Versioning in spirit for its public release artifacts.

## [0.2.0] - 2026-04-06

### Added
- Optional Memovyn_0.1B hybrid classifier hook via Ollama, with graceful fallback to pure-algorithm mode.
- Configurable strategic forgetting policy: `off`, `conservative`, `balanced`, and `aggressive`.
- Learning Impact Score and Agent Evolution Timeline analytics.
- Dark mode toggle and improved loading states in the dashboard.
- Richer MCP action resources for interactive agent workflows.

### Changed
- Taxonomy decomposition can now incorporate model guidance while preserving deterministic ranking and fallback safety.
- Solidified priors have a stronger influence on future classification, including dependency-aware boosts.
- README was overhauled for v0.2 with installation, MCP bindings, benchmarks, architecture, and release guidance.
- CLI and dashboard now surface provenance and stronger operator-facing guidance.

### Performance
- 1000-memory benchmark: `add_avg_us=762`, `add_p95_us=1053`, `search_p95_ms=2.907`
- 5000-memory benchmark: `add_avg_us=1283`, `add_p95_us=2107`, `search_p95_ms=6.596`
- 200k simulation: `cold_broad_ms=7.249`, `search_p95_ms=8.491`

## [0.1.1] - 2026-04-06

### Added
- Weighted reinforcement feedback with adaptive decay and cross-project influence.
- Solidified project priors that strengthen future taxonomy decomposition.
- Strategic forgetting for low-value memories in mature, high-activity projects.
- Memory Health Score, proactive suggestions, most impactful memories, and Project Memory Health Report export.
- Provenance output in memory inspection and richer reconciliation hints for Lead Agent workflows.
- Cross-platform GitHub Actions CI for Linux, macOS, and Windows release builds.

### Changed
- Broad-query search now uses a ranked active shortlist instead of expanding the entire project candidate set.
- 200k-scale search simulation now reports cold broad-query latency as well as warmed average and p95 timings.
- Token savings estimation now models progressive disclosure rather than naive string-length deltas.
- Dashboard analytics layout and loading states were polished for a more production-ready operator experience.
- README was rewritten for release readiness with installation, MCP bindings, benchmarks, architecture, and usage guidance.

### Performance
- 1000-memory benchmark: `add_avg_us=948`, `add_p95_us=1326`, `search_p95_ms=7.814`
- 5000-memory benchmark: `add_avg_us=2105`, `add_p95_us=3710`, `search_p95_ms=10.477`
- 200k simulation: `cold_broad_ms=4.131`, `search_p95_ms=4.385`

## [0.1.0] - 2026-04-05

### Added
- Initial public release of Memovyn.
- Local-first SQLite-backed permanent memory for MCP-native coding agents.
- Multi-dimensional taxonomy engine with hierarchical decomposition.
- CLI, MCP stdio/HTTP server, dashboard, export/import, analytics, and inspection flows.
