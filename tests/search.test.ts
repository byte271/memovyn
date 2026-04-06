import test from "node:test";
import assert from "node:assert/strict";

import { SearchIndex } from "../src/search.ts";
import { defaultLearningState, defaultMetadata, type MemoryRecord } from "../src/types.ts";

function memory(id: string, projectId: string, headline: string, content: string): MemoryRecord {
  const now = new Date().toISOString();
  return {
    id,
    projectId,
    kind: "observation",
    headline,
    summary: headline,
    content,
    contentHash: id,
    taxonomy: {
      mainCategory: "architecture",
      confidence: 0.82,
      multiLabels: ["architecture", "retrieval", "avoid_pattern"],
      hierarchy: [],
      dimensions: [
        { dimension: "domain", dominantLabel: "architecture", labels: ["architecture"], confidence: 0.8 }
      ],
      signals: [],
      relations: [],
      avoidPatterns: ["avoid_pattern"],
      reinforcePatterns: [],
      metadata: {
        headline,
        summary: headline,
        languageHint: "typescript",
        classifierBackend: "algorithm",
        classifierNotes: [],
        modelConfidence: 0,
        tokenCount: 12,
        signalCount: 3,
        sentenceCount: 1,
        lineCount: 1,
        relationCount: 0,
        artifactDensity: 0,
        confidenceMean: 0.82,
        sensitivityTags: [],
        emergentClusters: [],
        entities: [],
        redactions: [],
        taxonomyVersion: "test",
        compressionHint: "summary-first",
        inferredKinds: []
      },
      debug: {
        matchedAliases: [],
        prototypeHits: [],
        pathHints: [],
        contextHints: [],
        derivedMarkers: []
      }
    },
    metadata: defaultMetadata(),
    createdAt: now,
    updatedAt: now,
    lastAccessedAt: now,
    reinforcement: 0,
    penalty: 0,
    learning: defaultLearningState(),
    accessCount: 0,
    version: 1
  };
}

test("refresh updates learning bias", () => {
  const index = new SearchIndex();
  const record = memory("1", "demo", "avoid stale bug", "regression bug avoid pattern");
  index.insert(record);
  record.reinforcement = 2;
  record.learning.successScore = 2;
  index.refresh(record);
  const summary = index.projectSummary("demo");
  assert.ok(summary.topLabels.length > 0);
  assert.ok(index.recentCards("demo", 1).length > 0);
});
