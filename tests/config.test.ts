import test from "node:test";
import assert from "node:assert/strict";

import { parseClassifierMode, parseForgettingPolicy } from "../src/config.ts";

test("parses classifier mode", () => {
  assert.equal(parseClassifierMode("ollama"), "ollama");
  assert.equal(parseClassifierMode("algorithm"), "algorithm");
});

test("parses forgetting policy", () => {
  assert.equal(parseForgettingPolicy("aggressive"), "aggressive");
  assert.equal(parseForgettingPolicy("off"), "off");
});
