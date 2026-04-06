import test from "node:test";
import assert from "node:assert/strict";

import { TaxonomyEngine } from "../src/taxonomy.ts";
import { defaultMetadata, defaultEvolutionSnapshot } from "../src/types.ts";

test("decomposes into dimensions and relations", () => {
  const engine = new TaxonomyEngine();
  const { taxonomy } = engine.decompose(
    "We decided to store project state in SQLite, expose it via MCP HTTP, and benchmark BM25 retrieval latency.",
    defaultMetadata(),
    defaultEvolutionSnapshot()
  );
  assert.ok(taxonomy.multiLabels.length >= 20);
  assert.ok(taxonomy.dimensions.length >= 3);
});

test("redacts secrets", () => {
  const engine = new TaxonomyEngine();
  const { sanitized } = engine.decompose(
    "api_key = sk_test_123456789000 secret",
    defaultMetadata(),
    defaultEvolutionSnapshot()
  );
  assert.ok(sanitized.includes("[REDACTED_SECRET]"));
});
