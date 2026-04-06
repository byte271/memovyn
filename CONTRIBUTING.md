# Contributing to Memovyn

Thanks for helping improve Memovyn.

Memovyn is a local-first, MCP-native memory framework for coding agents. We optimize for:
- correctness
- performance
- inspectability
- local-first trust
- developer experience

## Ground Rules

- Prefer small, reviewable pull requests.
- Preserve backward compatibility unless the change is explicitly a breaking release.
- Keep the pure-algorithm path fast and dependable even when hybrid model features are enabled.
- Treat token efficiency, search quality, and operator trust as core product features.

## Development Setup

Requirements:
- Node.js 20 LTS
- npm 11+

Install dependencies:
```bash
npm install
```

Useful commands:
```bash
npm run typecheck
npm test
npm run build
npm run package:bundle
npx tsx src/cli.ts serve --bind 127.0.0.1:7761
```

Optional Ollama setup:
```bash
export MEMOVYN_CLASSIFIER_MODE=ollama
export MEMOVYN_OLLAMA_BASE_URL=http://127.0.0.1:11434
export MEMOVYN_OLLAMA_MODEL=memovyn_0.1b
```

## Project Layout

- `src/app.ts`: application service and orchestration
- `src/taxonomy.ts`: taxonomy engine and decomposition logic
- `src/search.ts`: in-memory search index and ranking
- `src/storage.ts`: SQLite persistence layer
- `src/model.ts`: optional Ollama hybrid classifier hook
- `src/mcp.ts`: MCP JSON-RPC handler
- `src/http.ts`: dashboard and HTTP API server
- `src/cli.ts`: CLI entrypoint
- `static/`: dashboard assets
- `.github/workflows/`: CI and release automation

## Testing

Run the built-in Node test suite:
```bash
npm test
```

Recommended manual checks:
```bash
npx tsx src/cli.ts add demo "We chose SQLite and BM25."
npx tsx src/cli.ts search demo sqlite --limit 5
npx tsx src/cli.ts analytics demo --markdown
```

Large-scale benchmark:
```bash
npx tsx src/cli.ts benchmark memovyn-dev --memories 5000 --query "shared brain regression sqlite mcp"
```

## Performance Expectations

Changes that touch taxonomy, search, or storage should be benchmark-aware.

Please measure or reason about:
- add latency
- search p95
- broad-query behavior
- token-savings impact
- memory growth and strategic forgetting safety

## Pull Request Process

1. Create a focused branch.
2. Run `npm run typecheck`, `npm test`, and `npm run build`.
3. Update `README.md` or `CHANGELOG.md` when behavior changes.
4. Explain:
   - what changed
   - why it changed
   - performance impact
   - migration or compatibility risks

## Code Style

- Write clean, explicit TypeScript.
- Prefer deterministic algorithms over opaque heuristics when possible.
- Keep runtime dependencies minimal.
- Add comments only where they clarify non-obvious behavior.
- Avoid hidden magic in the learning loop or strategic forgetting logic.

## What We Value

The best Memovyn contributions make the product:
- faster
- more explainable
- more trustworthy
- easier to integrate into real agent workflows

If a change improves raw capability but makes the system harder to trust, inspect, or operate, it probably needs another pass.
