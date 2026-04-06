import test from "node:test";
import assert from "node:assert/strict";

import { ModelHook } from "../src/model.ts";
import { loadConfig } from "../src/config.ts";
import { defaultMetadata, defaultEvolutionSnapshot } from "../src/types.ts";

test("algorithm mode returns no model guidance", async () => {
  const hook = new ModelHook({ ...loadConfig(), classifierMode: "algorithm" });
  const guidance = await hook.classify("test memory", defaultMetadata(), defaultEvolutionSnapshot());
  assert.equal(guidance, null);
});
