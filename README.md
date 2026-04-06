# Memovyn

[![Node.js](https://img.shields.io/badge/Node.js-20-black?logo=node.js)](https://nodejs.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.x-white?logo=typescript)](https://www.typescriptlang.org/)
[![License](https://img.shields.io/badge/License-MIT-black)](./LICENSE)
[![CI](https://img.shields.io/badge/CI-GitHub%20Actions-white)](./.github/workflows/ci.yml)
[![MCP](https://img.shields.io/badge/MCP-Native-black)](https://modelcontextprotocol.io/)
[![Version](https://img.shields.io/badge/Version-v0.2.0-white)](./CHANGELOG.md)

Memovyn is a local-first, MCP-native permanent memory framework for coding agents. It gives Claude Code, Cursor, Codex, and any MCP-compatible tool a persistent project brain with structured taxonomy, explainable ranking, reinforcement learning, strategic forgetting, and high-speed recall.

Memovyn v0.2 is the **Node.js 20 + TypeScript edition**. The project now runs on a stable LTS Node runtime while preserving the same high-performance philosophy:
- fearfully fast taxonomy and search
- local-first SQLite persistence
- reinforcement, priors, and avoid-pattern learning
- progressive disclosure for token efficiency
- premium dashboard and MCP-native workflows

## Why Memovyn?

Developers are tired of agents forgetting everything between sessions, repeating old mistakes, and bloating context with unusable memory.

Memovyn is built to solve that directly:
- **Local-first by default**: your memory stays on disk, in your control.
- **Taxonomy-native**: memories are decomposed into semantic, domain, activity, artifact, lifecycle, privacy, and language dimensions.
- **Explainable**: every memory can be inspected with labels, relations, provenance, and version history.
- **Learning-driven**: reinforced patterns become project priors, repeated failures become durable avoid rules, and low-value memories can be archived.
- **MCP-native**: one codebase powers CLI, MCP stdio, MCP HTTP, dashboard, and release packaging.

## Why It Beats Typical Memory Layers

| Capability | Memovyn | Mem0 / Letta / Zep / vector-only memory |
| --- | --- | --- |
| Local-first persistence | Native | Often partial or cloud-first |
| Structured taxonomy | Multi-dimensional and explainable | Usually flat or embedding-driven |
| Learning loop | Reinforcement, priors, avoid-rules, forgetting | Often weak or manual |
| Debuggability | Signals, provenance, versions, health report | Often opaque |
| Token efficiency | Progressive disclosure with explicit estimator | Frequently context-heavy |
| MCP-native workflow | First-class | Usually wrapper-level |

## Feature Highlights

### Permanent Project Memory
- isolated memory containers keyed by `project_id`
- SQLite-backed persistence across sessions and reboots
- optional cross-project sharing
- export/import and version history

### World-Class Taxonomy Engine
- 20-50 labels per memory
- 3-5 level hierarchy
- high-precision alias matching
- BM25-style prototype scoring
- prefix-trie clustering
- relation inference and debug traces
- strong project priors and solidified priors

### Optional Memovyn_0.1B Hybrid Classifier Hook
- local Ollama integration
- multi-language guidance and smarter label hints
- safe fallback to pure-algorithm mode
- deterministic taxonomy remains the final authority

### Reinforcement And Strategic Forgetting
- weighted feedback
- adaptive decay tuned by project activity
- consolidated avoid-patterns for repeated regressions
- cross-project influence for shared memories
- configurable forgetting policy:
  - `off`
  - `conservative`
  - `balanced`
  - `aggressive`

### Analytics And Operator UX
- token savings estimator
- Memory Health Score
- Learning Impact Score
- Agent Evolution Timeline
- most recalled, reinforced, punished, and impactful memories
- conflict heatmaps and proactive suggestions
- Project Memory Health Report export in Markdown
- dashboard dark mode and better loading states

### Retrieval Model
- progressive disclosure:
  - index cards
  - summary layer
  - timeline layer
  - detail layer
- broad-query fast path for large corpora
- active shortlist ranking for cold high-density queries
- cached project priors and query plans

## Installation

### Option 1: Use Release Binary

Download the latest platform artifact from GitHub Releases and place the extracted `memovyn` executable on your `PATH`.

Linux / macOS:
```bash
chmod +x memovyn
./memovyn --help
```

Windows PowerShell:
```powershell
.\memovyn.exe --help
```

### Option 2: Build From Source

Requirements:
- Node.js 20 LTS
- npm 11+

Clone and install:
```bash
git clone https://github.com/byte271/memovyn.git
cd memovyn
npm install
```

Run local validation:
```bash
npm run typecheck
npm test
npm run build
```

### Data Directory

By default Memovyn stores local state under `./.memovyn`.

Override it with:
```bash
MEMOVYN_DATA_DIR=/path/to/data
```

### Optional Ollama Hook

Enable the hybrid classifier:
```bash
MEMOVYN_CLASSIFIER_MODE=ollama
MEMOVYN_OLLAMA_BASE_URL=http://127.0.0.1:11434
MEMOVYN_OLLAMA_MODEL=memovyn_0.1b
MEMOVYN_OLLAMA_TIMEOUT_MS=350
```

If Ollama is unavailable or the model does not respond correctly, Memovyn falls back to pure-algorithm mode automatically.

### Strategic Forgetting Policy

```bash
MEMOVYN_FORGETTING_POLICY=balanced
```

Supported values:
- `off`
- `conservative`
- `balanced`
- `aggressive`

## Quick Start

Add a memory:
```bash
node --experimental-strip-types src/cli.ts add memovyn-core "We chose SQLite WAL mode and BM25 retrieval for project memory search."
```

Search memories:
```bash
node --experimental-strip-types src/cli.ts search memovyn-core "sqlite retrieval" --limit 5
```

Get ready-to-inject project context:
```bash
node --experimental-strip-types src/cli.ts context memovyn-core
```

Reflect on a result:
```bash
node --experimental-strip-types src/cli.ts reflect memovyn-core "Regression came from forgetting to update the postings index after insert." --outcome regression
```

Apply explicit feedback:
```bash
node --experimental-strip-types src/cli.ts feedback 0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe --outcome success --weight 1.25
```

Inspect a memory:
```bash
node --experimental-strip-types src/cli.ts inspect 0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe
```

Archive a low-value memory:
```bash
node --experimental-strip-types src/cli.ts archive 0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe
```

Generate a Project Memory Health Report:
```bash
node --experimental-strip-types src/cli.ts analytics memovyn-core --markdown
```

## MCP Bindings

Memovyn exposes:
- `add_memory`
- `search_memories`
- `get_project_context`
- `reflect_memory`
- `feedback_memory`
- `get_project_analytics`
- `archive_memory`

### Claude Code

Use [examples/claude-code.mcp.json](/D:/memovyn/examples/claude-code.mcp.json):

```json
{
  "mcpServers": {
    "memovyn": {
      "command": "memovyn",
      "args": ["mcp-stdio"],
      "env": {
        "MEMOVYN_DATA_DIR": ".memovyn"
      }
    }
  }
}
```

### Cursor

Use [examples/cursor.mcp.json](/D:/memovyn/examples/cursor.mcp.json).

### Codex

Use [examples/codex.mcp.json](/D:/memovyn/examples/codex.mcp.json).

### MCP HTTP Server

Run:
```bash
node --experimental-strip-types src/cli.ts serve --bind 127.0.0.1:7761
```

Endpoints:
- Dashboard: [http://localhost:7761](http://localhost:7761)
- MCP HTTP: [http://localhost:7761/mcp](http://localhost:7761/mcp)

## Dashboard

The dashboard includes:
- project cards
- taxonomy heatmaps
- relation summaries
- top recalled, reinforced, punished, and impactful memories
- conflict and evolution views
- token savings estimator
- Memory Health Score
- Learning Impact Score
- Project Memory Health Report export
- provenance-rich inspection
- one-click reinforce, punish, archive, and inspect actions
- dark mode toggle

## Architecture Overview

### Storage Layer
- SQLite via Node’s built-in `node:sqlite`
- project containers with share-scope flags
- feedback event history
- version snapshots
- recall and token-savings tracking

### Taxonomy Layer
- alias matching and prototype scoring
- relation inference
- solidified priors and avoid-pattern feedback
- optional model guidance merged into deterministic ranking

### Search Layer
- in-memory inverted index
- precomputed ranking hints
- broad-query shortlist path
- query caching
- progressive disclosure output

### Learning Layer
- weighted reinforcement
- adaptive decay
- strategic forgetting
- cross-project influence
- proactive suggestions and reconciliation hints

### Interface Layer
- CLI
- MCP stdio
- MCP HTTP
- dashboard
- Markdown health reports

## Performance

### Measured Locally During Migration

| Scenario | Add Avg | Add p95 | Search Avg | Search p95 |
| --- | ---: | ---: | ---: | ---: |
| 1,000 memories | `762us` | `1053us` | `1.199ms` | `2.907ms` |
| 5,000 memories | `1283us` | `2107us` | `1.447ms` | `6.596ms` |
| 200k simulation | n/a | n/a | `7.062ms` | `8.491ms` |

200k simulation details:
```text
insert=1.9562049s
cold_broad_ms=7.249
search_avg_ms=7.062
search_p95_ms=8.491
```

Analytics on the benchmark project:
```text
estimated_tokens_per_recall=323
memory_health_score=85
```

## Build, Test, And Release

### Local Validation

```bash
npm run typecheck
npm test
npm run build
```

### Quick Runtime Smoke Tests

```bash
node --experimental-strip-types src/cli.ts
node --experimental-strip-types src/cli.ts add demo "We decided to use SQLite and BM25 for project memory search."
node --experimental-strip-types src/cli.ts search demo sqlite --limit 5
```

### Build Release Bundle

```bash
npm run build
npm run package:bundle
```

### Cross-Platform CI

GitHub Actions CI is defined in [ci.yml](/D:/memovyn/.github/workflows/ci.yml) and builds on:
- Linux
- macOS
- Windows

Each runner:
- installs Node.js 20
- installs dependencies
- runs typecheck
- runs tests
- builds the bundle
- prunes dev dependencies
- packages a release bundle
- uploads the release artifact

## Real-World Usage Recommendations

Memovyn works best when used as the project memory system of record for an agent team.

Recommended pattern:
1. Save architecture decisions and constraints early.
2. Reflect on failures immediately with `reflect` or `feedback`.
3. Reinforce decisions that repeatedly prove correct.
4. Review the Project Memory Health Report regularly.
5. Enable the Ollama hook only when you want stronger multi-language hints or hybrid decomposition.
6. Choose a forgetting policy deliberately:
   - `off` for sensitive or very small projects
   - `conservative` for early-stage teams
   - `balanced` for most production use
   - `aggressive` for high-churn large projects

## Contributing

See [CONTRIBUTING.md](/D:/memovyn/CONTRIBUTING.md).

## Changelog

See [CHANGELOG.md](/D:/memovyn/CHANGELOG.md).

## License

MIT
