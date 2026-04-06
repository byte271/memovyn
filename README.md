# Memovyn

Memovyn is a local-first, MCP-native permanent memory framework for coding agents. It gives Claude Code, Cursor, Codex, and any MCP-compatible tool a project-scoped shared brain with structured recall, explainable taxonomy, issue punishment, reinforcement, and human-debuggable state.

Version `0.1.0` stays deliberately 100% LLM-free in the hot path. Classification, ranking, context assembly, conflict surfacing, and memory evolution are all algorithmic.

## What Changed In This Pass

- Taxonomy engine upgraded from a strong flat classifier into a multi-dimensional memory compiler.
- Memory records now carry:
  - semantic, domain, activity, artifact, lifecycle, privacy, and language breakdowns
  - confidence scores
  - inferred relationships
  - avoid-pattern and reinforce-pattern tracks
  - debug traces showing alias hits, prototype hits, path hints, and project priors
- Search now indexes memory content plus taxonomy structure, not just raw text.
- Project context now includes relation summaries, dominant dimensions, conflict notes, and shared recall summaries.
- Dashboard now exposes project stats, relation heatmaps, debug notes, and a memory inspection drawer.
- Storage now tracks richer project analytics, conflict counts, token savings, and version snapshots after feedback updates.
- Memovyn now persists first-class learning state on each memory:
  - `success_score`
  - `failure_count`
  - `repeated_mistake_count`
  - `reinforcement_decay`
  - `conflict_score`
  - `last_feedback_at`
- Feedback is now a first-class operation across CLI, MCP, and dashboard, not just an internal side-effect of reflection.
- Feedback now supports weighted reinforcement, adaptive decay, and cross-project influence for shared memories.
- Repeated failures are consolidated into explicit `avoid:{category}:{pattern}` rules so they stay retrievable as durable warnings.
- Analytics now expose:
  - top 50 recalled memories
  - most reinforced and most punished memories
  - project and session token savings
  - growth-over-time buckets
  - conflict heatmaps
  - behavior insights
  - CSV and Markdown export
- Dashboard now supports direct archive actions in addition to reinforce/punish/inspect.
- CLI now includes `inspect`, `feedback`, `archive`, and `analytics`.

## Why Memovyn

- Local-only persistence with zero cloud dependency.
- Per-project isolation with optional cross-project sharing.
- Pure-algorithm taxonomy and retrieval with no vector requirement.
- Reinforcement and punishment loop for learning from successes and repeated failures.
- Progressive disclosure retrieval:
  - index cards
  - summary layer
  - timeline layer
  - full detail layer
- Human-debuggable memory inspection instead of black-box scoring.
- Unified binary for CLI, MCP stdio, MCP HTTP, and dashboard.

## Repository Structure

```text
memovyn/
├── .dockerignore
├── .gitignore
├── Cargo.lock
├── Cargo.toml
├── Dockerfile
├── README.md
├── docker-compose.yml
├── examples/
│   ├── claude-code.mcp.json
│   ├── codex.mcp.json
│   └── cursor.mcp.json
├── src/
│   ├── app.rs
│   ├── config.rs
│   ├── domain.rs
│   ├── error.rs
│   ├── lib.rs
│   ├── main.rs
│   ├── search.rs
│   ├── dashboard/
│   │   └── mod.rs
│   ├── mcp/
│   │   └── mod.rs
│   ├── storage/
│   │   └── mod.rs
│   └── taxonomy/
│       └── mod.rs
└── static/
    ├── app.css
    └── app.js
```

## One-Command Docker

```bash
docker compose up --build
```

Then open:

- Dashboard: [http://localhost:7761](http://localhost:7761)
- MCP HTTP endpoint: [http://localhost:7761/mcp](http://localhost:7761/mcp)

Memories persist in `./.memovyn`.

## Local Build

```bash
cargo build --release
```

Run the dashboard and HTTP MCP server:

```bash
cargo run -- serve --bind 127.0.0.1:7761
```

Run as a stdio MCP server:

```bash
cargo run -- mcp-stdio
```

## CLI Quickstart

Add a memory:

```bash
cargo run -- add memovyn-core "We chose SQLite WAL mode and BM25 retrieval for project memory search."
```

Search memories:

```bash
cargo run -- search memovyn-core "sqlite retrieval" --limit 5
```

Get ready-to-inject context:

```bash
cargo run -- context memovyn-core
```

Reflect a task result:

```bash
cargo run -- reflect memovyn-core "Regression came from forgetting to update the postings index after insert." --outcome regression
```

Apply explicit feedback to an existing memory:

```bash
cargo run -- feedback 0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe --outcome success --weight 1.25
```

Archive a memory so it leaves active recall but stays inspectable:

```bash
cargo run -- archive 0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe
```

Create a private note anchor:

```bash
cargo run -- note memovyn-core "Remember: never store raw credentials in memory entries."
```

Inspect a memory record and its version trail:

```bash
cargo run -- inspect 0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe
```

Inspect project analytics as JSON or CSV:

```bash
cargo run -- analytics memovyn-core
cargo run -- analytics memovyn-core --csv
cargo run -- analytics memovyn-core --markdown
```

Export a project:

```bash
cargo run -- export memovyn-core memovyn-core.backup.json
```

Import a project backup:

```bash
cargo run -- import memovyn-core.backup.json
```

## MCP Tools

Memovyn exposes the following MCP tools over both stdio and HTTP JSON-RPC:

1. `add_memory(project_id, content, metadata?, kind?)`
2. `search_memories(project_id, query, limit, filters?)`
3. `get_project_context(project_id)`
4. `reflect_memory(project_id, task_result, outcome, metadata?)`
5. `feedback_memory(memory_id, outcome, repeated_mistake?, weight?, cross_project_influence?, avoid_patterns?, note?)`
6. `get_project_analytics(project_id)`
7. `archive_memory(memory_id)`

`reflect_memory` returns an interactive confirmation payload with `Yes`, `Edit`, and `No` actions so a lead agent can decide whether to commit the fully classified memory.

## Connector Binding

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

## Taxonomy Engine

The taxonomy engine in [src/taxonomy/mod.rs](/D:/memovyn/src/taxonomy/mod.rs) is Memovyn’s crown jewel.

### Dimensions

- `semantic`: fact, preference, decision, risk, instruction, incident, learned pattern
- `domain`: architecture, storage, retrieval, performance, security, API, UI, tooling, testing, deployment, collaboration, documentation
- `activity`: implement, fix, benchmark, refactor, investigate, review, reflect, plan, migrate
- `artifact`: module, database artifact, endpoint, path artifact, config artifact, command artifact, query plan, test artifact, prompt artifact
- `lifecycle`: recent, stable, avoid pattern, blocked, planned, regression, reinforced, cross project, deprecated
- `privacy`: private, secret, PII
- `language`: Rust, TypeScript, Python, SQL, shell, JSON

### Classification Pipeline

- Aho-Corasick phrase matching for high-precision category triggers.
- BM25-style prototype scoring across taxonomy seeds.
- Prefix-trie emergent clustering for project-specific repeated terms.
- Project-evolution priors from existing memory containers.
- Relationship inference across top-ranked labels.
- Hierarchy emission with 3-5 levels:
  - root
  - dimension
  - category
  - lifecycle/privacy/language refinements
  - cluster nodes

### Memory Output Shape

Each memory now includes:

- `main_category`
- `confidence`
- `multi_labels` with 20-50 labels
- `dimensions`
- `signals`
- `relations`
- `avoid_patterns`
- `reinforce_patterns`
- `debug` traces for black-box debugging

## Dashboard

The dashboard in [src/dashboard/mod.rs](/D:/memovyn/src/dashboard/mod.rs) now provides:

- project cards with conflict counts and token-savings signals
- project context card
- taxonomy heatmap
- relation graph summary
- reinforcement leaders
- top recalled memories
- most reinforced vs most punished memory lists
- growth-over-time and conflict-heatmap analytics
- behavior insights
- CSV and Markdown export for analytics
- virtualized memory stream
- click-to-inspect memory drawer with taxonomy explanation, version trail, and direct reinforce/punish/archive actions

## Architecture

### 1. Storage Layer

- SQLite with WAL mode.
- Project containers with share-scope flags.
- Memory version snapshots.
- Recall logging with token-savings tracking.
- Conflict counts and recall analytics.

### 2. Search Layer

- In-memory inverted index by project.
- Search corpus includes content, summaries, labels, relations, and dimension summaries.
- Reinforcement, penalty, relation overlap, and recency-aware ranking.
- Project-insights cache for fast taxonomy feedback and project summaries.
- Search refresh path updates learning state in-memory without rebuilding postings.
- Project-evolution snapshot generation for future classification.

### 3. Reflection Loop

- `reflect_memory` auto-classifies task results.
- Detects likely repeated mistakes from prior penalized memories.
- Reinforces successes and punishes regressions.
- Emits interactive save confirmation metadata for lead-agent workflows.

### 4. Feedback Loop

- `feedback_memory` updates memory learning state directly.
- Taxonomy signals and hierarchy nodes now carry reinforcement metadata.
- Repeated failures add avoid-pattern pressure and conflict score.
- Feedback events are persisted for heatmaps, version history, and debugging.
- Feedback weights are tuned by project activity so active projects reinforce and decay more aggressively.
- Shared memories can propagate dampened reinforcement or punishment across related project containers.

### 5. Inspectability

- Every memory exposes debug traces.
- CLI `inspect` surfaces memory versions and taxonomy reasoning.
- Dashboard inspection drawer provides fast human inspection and one-click feedback.

## Benchmarks

Synthetic benchmark harness is built into the binary:

```bash
$env:MEMOVYN_DATA_DIR='D:\memovyn\.memovyn-bench-hot-1000'
cargo run --release -- benchmark memovyn-legend-hot-1000 --memories 1000 --query "shared brain regression sqlite mcp"
```

Observed locally on this repository in `release` mode:

```text
add_count=1000 add_avg_us=928 add_p95_us=1240 search_avg_ms=1.302 search_p95_ms=2.935 hits=1000
```

Additional run at 5000 memories:

```text
add_count=5000 add_avg_us=1698 add_p95_us=2542 search_avg_ms=1.417 search_p95_ms=8.876 hits=5000
```

Ignored release-scale simulation for the in-memory search layer:

```text
200k scale simulation insert=1.4885244s search_avg_ms=27.126 search_p95_ms=29.878
```

These numbers reflect the richer taxonomy compiler, structured indexing, SQLite persistence path, weighted reinforcement metadata, incremental project insights, and the new search cache plus top-k materialization path. The biggest gains came from avoiding full `SearchHit` construction for every candidate and caching repeated query plans at the project boundary.

## Why It Beats Typical Memory Layers

| Capability | Memovyn | Typical vector-only memory layer |
| --- | --- | --- |
| Project isolation | Native | Often ad hoc |
| Structured taxonomy | Multi-dimensional, hierarchical, explainable | Usually flat tags or none |
| Learning loop | Reinforcement + punishment + avoid patterns | Rare or manual |
| Shared state | Cross-project opt-in with project containers | Usually global blob |
| Debuggability | Inspectable signals, relations, versions, traces | Black-box similarity scores |
| Retrieval style | Index + summary + timeline + full detail | Usually single similarity list |
| Local-first deployment | Yes | Sometimes cloud-first |
| MCP-native workflow | Yes | Often wrapper-only |

## Comparison Snapshot

| System | Memovyn advantage |
| --- | --- |
| Mem0 | Stronger structured taxonomy, richer inspectability, no dependence on vector similarity for meaning |
| Letta / Hindsight / Pulse OS | Tighter local-first footprint and explicit punishment loop for repeated mistakes |
| Zep / Supermemory | More stateful project containers and human-readable taxonomy/debug layers |
| Cognee | Better shared-brain semantics for coding-agent workflows and stronger MCP integration |

## Validation

Validated locally with:

```bash
cargo fmt
cargo check
cargo test
```

## Notes

- The upgraded codepath is backward-compatible with earlier Memovyn taxonomy JSON by providing defaults for newly added fields.
- Memovyn now keeps incremental project insight caches and cached prepared statements in the hot path.
- The next major optimization frontier is shrinking cold broad-query latency even further without depending on warm cache effects.
