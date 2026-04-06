import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { resolve } from "node:path";
import { randomUUID, createHash } from "node:crypto";

import type { Config } from "./config.ts";
import { defaultMetadata, defaultSearchFilters, type AddMemoryInput, type AnalyticsSnapshot, type ArchiveInput, type ArchiveResponse, type FeedbackInput, type FeedbackResponse, type MemoryInspection, type MemoryKind, type MemoryRecord, type ProjectContext, type ReflectionInput, type ReflectionResponse, type SearchHit, type SearchInput } from "./types.ts";
import { defaultLearningState } from "./types.ts";
import { SearchIndex } from "./search.ts";
import { Storage } from "./storage.ts";
import { TaxonomyEngine } from "./taxonomy.ts";
import { ModelHook } from "./model.ts";
import { NotFoundError } from "./errors.ts";

export class Memovyn {
  readonly config: Config;
  readonly storage: Storage;
  readonly taxonomy: TaxonomyEngine;
  readonly search: SearchIndex;
  readonly modelHook: ModelHook;

  private sessionQueries = 0;
  private sessionTokenSavings = 0;

  constructor(config: Config) {
    this.config = config;
    this.storage = new Storage(config.databasePath);
    this.taxonomy = new TaxonomyEngine();
    this.search = new SearchIndex(this.storage.loadAllMemories());
    this.modelHook = new ModelHook(config);
  }

  async addMemory(input: AddMemoryInput): Promise<MemoryRecord> {
    const metadata = { ...defaultMetadata(), ...(input.metadata ?? {}) };
    this.storage.upsertProject(input.projectId, metadata.shareScope);
    const evolution = this.search.taxonomyFeedback(input.projectId);
    const guidance = await this.modelHook.classify(input.content, metadata, evolution);
    const { sanitized, taxonomy } = this.taxonomy.decompose(input.content, metadata, evolution, guidance);
    const now = nowIso();
    const memory: MemoryRecord = {
      id: randomUUID(),
      projectId: input.projectId,
      kind: input.kind ?? "observation",
      headline: taxonomy.metadata.headline,
      summary: taxonomy.metadata.summary,
      content: sanitized,
      contentHash: createHash("blake2s256").update(sanitized).digest("hex"),
      taxonomy,
      metadata,
      createdAt: now,
      updatedAt: now,
      lastAccessedAt: now,
      reinforcement: 0,
      penalty: 0,
      learning: defaultLearningState(),
      accessCount: 0,
      version: 1
    };
    this.storage.insertMemory(memory);
    this.search.insert(memory);
    return memory;
  }

  searchMemories(input: SearchInput) {
    const filters = { ...defaultSearchFilters(), ...(input.filters ?? {}) };
    const response = this.search.search(
      input.projectId,
      input.query,
      input.limit ?? 10,
      filters,
      this.storage.listSharedProjects(input.projectId)
    );
    for (const hit of response.detailLayer) {
      const tokensSaved = estimateTokenSavings(hit);
      this.storage.recordRecall(hit.memoryId, input.query, tokensSaved);
      this.sessionQueries += 1;
      this.sessionTokenSavings += tokensSaved;
    }
    return response;
  }

  async reflectMemory(input: ReflectionInput): Promise<ReflectionResponse> {
    const prior = this.search.search(
      input.projectId,
      input.taskResult,
      5,
      { ...defaultSearchFilters(), includePrivateNotes: true },
      this.storage.listSharedProjects(input.projectId)
    );
    const repeatedMistakeDetected =
      (input.outcome === "failure" || input.outcome === "regression") &&
      prior.detailLayer.some((hit) => hit.penalty > hit.reinforcement || hit.labels.some((label) => label.includes("avoid")));
    const conflictDetected = prior.detailLayer.some(
      (hit) => hit.penalty > hit.reinforcement || hit.labels.some((label) => label.includes("regression"))
    );

    const metadata = {
      ...defaultMetadata(),
      ...(input.metadata ?? {}),
      tags: [...(input.metadata?.tags ?? []), "auto-reflection"],
      extra: {
        ...(input.metadata?.extra ?? {}),
        feedback_outcome: input.outcome
      }
    };
    if (repeatedMistakeDetected) {
      metadata.tags.push("avoid_pattern");
      metadata.extra.repeat_regression = "true";
    }
    if (conflictDetected) {
      metadata.tags.push("conflict");
    }

    const memory = await this.addMemory({
      projectId: input.projectId,
      content: input.taskResult,
      metadata,
      kind: input.outcome === "failure" || input.outcome === "regression" ? "issue" : "reflection"
    });

    const feedback = this.feedbackMemory({
      memoryId: memory.id,
      outcome: input.outcome,
      repeatedMistake: repeatedMistakeDetected,
      weight: repeatedMistakeDetected ? 1.25 : 1,
      crossProjectInfluence: true,
      avoidPatterns: memory.taxonomy.avoidPatterns,
      note: "reflect_memory"
    });

    return {
      memory: feedback.memory,
      repeatedMistakeDetected,
      conflictDetected: conflictDetected || feedback.conflictDetected,
      avoidPatterns: feedback.avoidPatterns,
      interactivePrompt: {
        title: "Save this full project description + complete taxonomy to Memovyn permanent memory?",
        body: "Memovyn classified the result, updated reinforcement weights, inferred taxonomy relations, and prepared the project memory graph.",
        actions: [
          { id: "yes", label: "Yes" },
          { id: "edit", label: "Edit" },
          { id: "no", label: "No" }
        ]
      }
    };
  }

  feedbackMemory(input: FeedbackInput): FeedbackResponse {
    const memory = this.storage.getMemory(input.memoryId);
    if (!memory) throw new NotFoundError(`memory ${input.memoryId}`);
    const repeated = Boolean(input.repeatedMistake);
    const weight = input.weight ?? 1;

    if (input.outcome === "success") {
      memory.reinforcement += 1.25 * weight;
      memory.learning.successScore += 1.1 * weight;
      memory.learning.reinforcementDecay = Math.max(0.78, memory.learning.reinforcementDecay * 0.94);
    } else if (input.outcome === "partial") {
      memory.reinforcement += 0.45 * weight;
      memory.penalty += 0.12 * weight;
      memory.learning.successScore += 0.35 * weight;
    } else {
      memory.penalty += (input.outcome === "regression" ? 1.4 : 0.9) * weight;
      memory.learning.failureCount += 1;
      memory.learning.conflictScore += (input.outcome === "regression" ? 1.2 : 0.85) * weight;
      memory.learning.reinforcementDecay = Math.min(2.5, memory.learning.reinforcementDecay * 1.1);
      if (repeated) {
        memory.learning.repeatedMistakeCount += 1;
        memory.taxonomy.avoidPatterns = unique([...memory.taxonomy.avoidPatterns, consolidatedAvoidPattern(memory)]);
      }
    }

    if (memory.learning.successScore >= 2) {
      memory.taxonomy.reinforcePatterns = unique([
        ...memory.taxonomy.reinforcePatterns,
        "solidified_prior",
        memory.taxonomy.mainCategory
      ]);
    }
    if ((input.avoidPatterns ?? []).length) {
      memory.taxonomy.avoidPatterns = unique([...memory.taxonomy.avoidPatterns, ...(input.avoidPatterns ?? [])]);
    }

    memory.learning.lastFeedbackAt = nowIso();
    memory.updatedAt = nowIso();
    memory.version += 1;
    this.storage.updateMemory(memory);
    this.storage.recordFeedback(memory.id, input.outcome, repeated, weight, input.note);
    this.search.refresh(memory);

    const influencedMemories: string[] = [];
    if (input.crossProjectInfluence && memory.metadata.shareScope) {
      const related = this.search.search(
        memory.projectId,
        `${memory.taxonomy.mainCategory} ${memory.taxonomy.multiLabels.slice(0, 4).join(" ")}`,
        4,
        { ...defaultSearchFilters(), includeShared: true, includePrivateNotes: true },
        this.storage.listSharedProjects(memory.projectId)
      );
      for (const hit of related.detailLayer.filter((hit) => hit.memoryId !== memory.id).slice(0, 2)) {
        influencedMemories.push(hit.memoryId);
      }
    }

    return {
      memory,
      conflictDetected: memory.learning.conflictScore > 0 || memory.penalty > memory.reinforcement,
      avoidPatterns: memory.taxonomy.avoidPatterns,
      influencedMemories,
      learningDelta: weight,
      reconciliationHints: buildReconciliationHints(memory, influencedMemories)
    };
  }

  archiveMemory(input: ArchiveInput): ArchiveResponse {
    const memory = this.storage.getMemory(input.memoryId);
    if (!memory) throw new NotFoundError(`memory ${input.memoryId}`);
    memory.metadata.extra.archived = "true";
    memory.updatedAt = nowIso();
    memory.version += 1;
    this.storage.updateMemory(memory);
    this.search.refresh(memory);
    return { memory };
  }

  getProjectContext(projectId: string): ProjectContext {
    const summary = this.search.projectSummary(projectId);
    const topMemories = this.search.recentCards(projectId, 8);
    return {
      projectId,
      readyContext: [
        `Project: ${projectId}`,
        `Top taxonomy labels: ${summary.topLabels.map(([label, count]) => `${label} (${count})`).join(", ")}`,
        `Dominant dimensions: ${summary.dominantDimensions.map(([dimension, label]) => `${dimension}=${label}`).join(", ")}`,
        `Key relations: ${summary.topRelations.map(([relation, count]) => `${relation} (${count})`).join(", ")}`,
        `Avoid patterns: ${summary.avoidPatterns.join(", ") || "none"}`,
        `Recent memory headlines: ${topMemories.map((memory) => memory.headline).join(" | ")}`
      ].join("\n"),
      taxonomySummary: summary,
      recentTimeline: this.search.recentTimeline(projectId, 12),
      topMemories,
      sharedRecall: this.search.recentSummaries(projectId, 6),
      debuggingNotes: buildDebugNotes(summary.activeConflicts)
    };
  }

  listProjects() {
    return this.storage.listProjects();
  }

  analytics(projectId: string): AnalyticsSnapshot {
    const analytics = this.storage.analytics(projectId);
    const insights = this.search.projectAnalytics(projectId);
    analytics.labelHotspots = insights.labelHotspots;
    analytics.relationHotspots = insights.relationHotspots;
    analytics.conflictCount = Math.max(analytics.conflictCount, insights.conflictCount);
    analytics.sessionQueries = this.sessionQueries;
    analytics.sessionTokenSavings = this.sessionTokenSavings;
    analytics.memoryHealthScore = computeHealthScore(analytics);
    analytics.learningImpactScore = computeLearningImpactScore(analytics);
    analytics.agentEvolutionTimeline = analytics.evolutionTrend;
    analytics.behaviorInsights = buildBehaviorInsights(analytics);
    analytics.proactiveSuggestions = buildProactiveSuggestions(analytics);
    return analytics;
  }

  inspectMemory(memoryId: string): MemoryInspection | undefined {
    const memory = this.storage.getMemory(memoryId);
    if (!memory) return undefined;
    return {
      memory,
      versions: this.storage.memoryVersions(memoryId),
      explanation: [
        `main_category=${memory.taxonomy.mainCategory}`,
        `confidence=${memory.taxonomy.confidence.toFixed(2)}`,
        `dimensions=${memory.taxonomy.dimensions.map((item) => `${item.dimension}=${item.dominantLabel}`).join(", ")}`,
        `relations=${memory.taxonomy.relations.map((relation) => `${relation.source} ${relation.relation} ${relation.target}`).join(" | ")}`,
        `learning=success:${memory.learning.successScore.toFixed(2)}, failures:${memory.learning.failureCount}, repeated:${memory.learning.repeatedMistakeCount}, decay:${memory.learning.reinforcementDecay.toFixed(2)}, conflict:${memory.learning.conflictScore.toFixed(2)}`
      ],
      provenance: [
        `created_at=${memory.createdAt}`,
        `updated_at=${memory.updatedAt}`,
        `content_hash=${memory.contentHash}`,
        `version=${memory.version}`,
        `classifier_backend=${memory.taxonomy.metadata.classifierBackend}`,
        `model_confidence=${memory.taxonomy.metadata.modelConfidence.toFixed(2)}`
      ]
    };
  }

  exportProject(projectId: string, path: string): void {
    const bundle = {
      exportedAt: nowIso(),
      memories: this.storage.loadAllMemories().filter((memory) => memory.projectId === projectId)
    };
    mkdirSync(resolve(path, ".."), { recursive: true });
    writeFileSync(path, JSON.stringify(bundle, null, 2));
  }

  importBundle(path: string): number {
    const bundle = JSON.parse(readFileSync(path, "utf8")) as { memories: MemoryRecord[] };
    for (const memory of bundle.memories) {
      this.storage.upsertProject(memory.projectId, memory.metadata.shareScope);
      this.storage.insertMemory(memory);
      this.search.insert(memory);
    }
    return bundle.memories.length;
  }

  async benchmark(projectId: string, memoryCount: number, query: string): Promise<string> {
    const addLatencies: number[] = [];
    for (let index = 0; index < memoryCount; index += 1) {
      const started = performance.now();
      await this.addMemory({
        projectId,
        content: `Benchmark memory ${index}: We decided to persist SQLite state, build BM25 retrieval, virtualize the dashboard list, and reinforce shared cross-session architecture for project ${projectId}.`,
        kind: "observation"
      });
      addLatencies.push((performance.now() - started) * 1000);
    }

    const searchLatencies: number[] = [];
    let hits = 0;
    for (let round = 0; round < 25; round += 1) {
      const started = performance.now();
      const response = this.searchMemories({
        projectId,
        query: round % 5 === 0 ? `${query} architecture` : query,
        limit: 10
      });
      hits = response.totalHits;
      searchLatencies.push((performance.now() - started) * 1000);
    }

    return [
      `add_count=${memoryCount}`,
      `add_avg_us=${Math.round(average(addLatencies))}`,
      `add_p95_us=${percentile(addLatencies, 0.95)}`,
      `search_avg_ms=${(average(searchLatencies) / 1000).toFixed(3)}`,
      `search_p95_ms=${(percentile(searchLatencies, 0.95) / 1000).toFixed(3)}`,
      `hits=${hits}`
    ].join(" ");
  }

  analyticsCsv(projectId: string): string {
    const analytics = this.analytics(projectId);
    let csv = "section,key,memory_id,headline,score,access_count,success_score,failure_count,value\n";
    for (const item of analytics.mostRecalled) {
      csv += `most_recalled,,${item.memoryId},"${item.headline.replaceAll('"', "'")}",${item.score},${item.accessCount},${item.successScore},${item.failureCount},${item.accessCount}\n`;
    }
    for (const item of analytics.mostImpactful) {
      csv += `most_impactful,,${item.memoryId},"${item.headline.replaceAll('"', "'")}",${item.score},${item.accessCount},${item.successScore},${item.failureCount},${item.score}\n`;
    }
    return csv;
  }

  analyticsMarkdown(projectId: string): string {
    const analytics = this.analytics(projectId);
    const lines = [
      `# Project Memory Health Report for \`${analytics.projectId}\``,
      "",
      `- Total memories: ${analytics.totalMemories}`,
      `- Total queries: ${analytics.totalQueries}`,
      `- Project token savings: ${analytics.totalTokenSavings}`,
      `- Estimated tokens saved per recall: ${analytics.estimatedTokensPerRecall}`,
      `- Memory health score: ${analytics.memoryHealthScore}`,
      `- Learning impact score: ${analytics.learningImpactScore}`,
      ""
    ];
    if (analytics.behaviorInsights.length) {
      lines.push("## Behavior Insights");
      lines.push(...analytics.behaviorInsights.map((insight) => `- ${insight}`), "");
    }
    if (analytics.proactiveSuggestions.length) {
      lines.push("## Proactive Suggestions");
      lines.push(...analytics.proactiveSuggestions.map((suggestion) => `- ${suggestion}`), "");
    }
    lines.push("## Most Impactful Memories");
    lines.push(
      ...analytics.mostImpactful
        .slice(0, 8)
        .map(
          (memory) =>
            `- **${memory.headline}**: score ${memory.score.toFixed(2)}, failures ${memory.failureCount}, recalls ${memory.accessCount}`
        )
    );
    return lines.join("\n");
  }
}

function buildDebugNotes(activeConflicts: string[]): string[] {
  const notes = [
    "Memovyn exposes dimension breakdowns, relations, and debug traces for every memory.",
    "Use inspect mode when a memory looks wrong; provenance now includes classifier backend and model confidence."
  ];
  if (activeConflicts.length) {
    notes.push(`Active conflicts detected: ${activeConflicts.join(" | ")}`);
  }
  return notes;
}

function buildBehaviorInsights(analytics: AnalyticsSnapshot): string[] {
  const insights = [
    `Memovyn is saving an estimated ${analytics.estimatedTokensPerRecall} tokens per recall on average in this project.`,
    `Learning Impact Score is ${analytics.learningImpactScore} based on reinforcement depth, recall reuse, and conflict reduction.`
  ];
  if (analytics.labelHotspots[0]) {
    insights.push(`This project revisits \`${analytics.labelHotspots[0][0]}\` more often than other hot taxonomy labels.`);
  }
  if (analytics.mostImpactful[0]) {
    insights.push(`\`${analytics.mostImpactful[0].headline}\` is currently the most impactful memory.`);
  }
  return insights;
}

function buildProactiveSuggestions(analytics: AnalyticsSnapshot): string[] {
  const suggestions: string[] = [];
  if (analytics.memoryHealthScore < 70) {
    suggestions.push("Run the Project Memory Health Report and archive low-value notes to reduce recall noise.");
  }
  if (analytics.learningImpactScore < 60) {
    suggestions.push("Increase explicit feedback after successful tasks so reinforced patterns become decisive project priors.");
  }
  if (analytics.estimatedTokensPerRecall < 30) {
    suggestions.push("Review summary compression and prefer progressive disclosure before injecting full memory bodies.");
  }
  return suggestions;
}

function buildReconciliationHints(memory: MemoryRecord, influencedMemories: string[]): string[] {
  const hints: string[] = [];
  if (memory.learning.conflictScore > 0 || memory.penalty > memory.reinforcement) {
    hints.push(`Reconcile the active conflict by comparing this memory against the current ${memory.taxonomy.mainCategory} project prior.`);
  }
  if (memory.learning.successScore >= 2) {
    hints.push(`Solidify ${memory.taxonomy.mainCategory} as a project-level prior so future classifications inherit it automatically.`);
  }
  if (memory.taxonomy.metadata.modelConfidence >= 0.7) {
    hints.push(`Hybrid classifier confidence is ${memory.taxonomy.metadata.modelConfidence.toFixed(2)}; this is a strong candidate for Lead Agent confirmation.`);
  }
  if (influencedMemories.length) {
    hints.push(`Cross-project influence touched ${influencedMemories.length} related memories; review them before the next release.`);
  }
  return hints;
}

function computeHealthScore(analytics: AnalyticsSnapshot): number {
  if (!analytics.totalMemories) return 100;
  const reinforcedRatio = analytics.reinforcedMemories / analytics.totalMemories;
  const conflictRatio = analytics.conflictCount / analytics.totalMemories;
  const tokenRatio = Math.min(analytics.estimatedTokensPerRecall / 100, 1);
  const lowPenaltyBonus = analytics.penalizedMemories === 0 ? 10 : 0;
  const queryBonus = analytics.totalQueries > 0 ? 10 : 0;
  return Math.round(
    25 + reinforcedRatio * 25 + (1 - conflictRatio) * 25 + tokenRatio * 15 + lowPenaltyBonus + queryBonus
  );
}

function computeLearningImpactScore(analytics: AnalyticsSnapshot): number {
  if (!analytics.totalMemories) return 0;
  const impactfulRatio = Math.min(analytics.mostImpactful.length / analytics.totalMemories, 0.2);
  const reinforcedRatio = analytics.reinforcedMemories / analytics.totalMemories;
  const conflictDrag = analytics.conflictCount / analytics.totalMemories;
  return Math.round(20 + impactfulRatio * 120 + reinforcedRatio * 35 + (1 - conflictDrag) * 25);
}

function consolidatedAvoidPattern(memory: MemoryRecord): string {
  return `avoid:${memory.taxonomy.mainCategory}:${memory.headline.toLowerCase().split(/\s+/).slice(0, 4).join("-")}`;
}

function estimateTokenSavings(hit: SearchHit): number {
  const full = estimateTokens(`${hit.content} ${hit.summary} ${hit.mainCategory} ${hit.labels.join(" ")}`);
  const compressed = estimateTokens(`${hit.headline} ${hit.mainCategory} ${hit.labels.slice(0, 4).join(" ")}`);
  return Math.max(0, full - compressed);
}

function estimateTokens(text: string): number {
  return Math.ceil(text.length / 4);
}

function average(values: number[]): number {
  return values.reduce((sum, value) => sum + value, 0) / Math.max(values.length, 1);
}

function percentile(values: number[], p: number): number {
  const sorted = [...values].sort((a, b) => a - b);
  const index = Math.min(sorted.length - 1, Math.max(0, Math.round((sorted.length - 1) * p)));
  return Math.round(sorted[index] ?? 0);
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}

function nowIso(): string {
  return new Date().toISOString();
}
