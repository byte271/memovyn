import { createHash } from "node:crypto";

import type {
  DimensionBreakdown,
  HierarchyNode,
  MemoryMetadata,
  TaxonomyDebugView,
  TaxonomyDecomposition,
  TaxonomyEvolutionSnapshot,
  TaxonomyMetadata,
  TaxonomyRelation,
  TaxonomySignal
} from "./types.ts";
import { defaultEvolutionSnapshot } from "./types.ts";
import type { ModelGuidance } from "./model.ts";

export const TAXONOMY_VERSION = "2026.04.legendary.node";

type Seed = {
  id: string;
  name: string;
  dimension: string;
  description: string;
  aliases: string[];
  prototype: string[];
  dependencies: string[];
  priority: number;
};

const seeds: Seed[] = [
  seed("decision", "Decision", "semantic", "A durable project decision.", ["decide", "decision"], ["decision", "choose", "adopt"], [], 95),
  seed("instruction", "Instruction", "semantic", "A rule the agent should follow.", ["always", "never", "remember"], ["instruction", "always", "never", "rule"], [], 94),
  seed("risk", "Risk", "semantic", "A known failure or risk pattern.", ["risk", "failure", "regression"], ["risk", "failure", "regression", "bug"], [], 96),
  seed("architecture", "Architecture", "domain", "System design and boundaries.", ["architecture", "design", "module"], ["architecture", "design", "system", "module"], [], 98),
  seed("storage", "Storage", "domain", "Persistence and indexing.", ["sqlite", "storage", "database", "persist"], ["sqlite", "database", "persist", "index"], ["architecture"], 97),
  seed("retrieval", "Retrieval", "domain", "Search, ranking, and context assembly.", ["search", "retrieval", "ranking", "bm25"], ["search", "retrieval", "ranking", "bm25", "query"], ["storage"], 97),
  seed("security", "Security", "domain", "Secrets, auth, and trust boundaries.", ["security", "secret", "jwt", "token"], ["security", "secret", "jwt", "auth"], ["architecture"], 97),
  seed("api", "API", "domain", "HTTP, MCP, and interface contracts.", ["api", "endpoint", "handler", "mcp"], ["api", "endpoint", "route", "mcp"], ["architecture"], 95),
  seed("ui", "UI", "domain", "Dashboard and operator-facing flows.", ["ui", "dashboard", "layout"], ["ui", "dashboard", "layout", "responsive"], ["api"], 91),
  seed("performance", "Performance", "domain", "Latency and scaling work.", ["performance", "latency", "optimize", "p95"], ["performance", "latency", "optimize", "throughput"], ["architecture"], 96),
  seed("implement", "Implement", "activity", "Concrete delivery work.", ["implement", "build", "create"], ["implement", "build", "create", "ship"], ["decision"], 88),
  seed("fix", "Fix", "activity", "Corrective work.", ["fix", "resolve", "repair"], ["fix", "resolve", "repair", "patch"], ["risk"], 94),
  seed("benchmark", "Benchmark", "activity", "Measured validation.", ["benchmark", "measure", "p95"], ["benchmark", "measure", "latency", "p95"], ["performance"], 91),
  seed("reflect", "Reflect", "activity", "Retrospective learning.", ["reflect", "retrospective", "learned"], ["reflect", "retrospective", "learned", "memory"], ["decision"], 90),
  seed("module", "Module", "artifact", "Code module or package.", ["module", "package", "crate"], ["module", "package", "library"], ["architecture"], 88),
  seed("database_artifact", "Database Artifact", "artifact", "Schema or index.", ["table", "schema", "index", "sqlite"], ["table", "schema", "index", "sqlite"], ["storage"], 91),
  seed("endpoint", "Endpoint", "artifact", "Route or RPC surface.", ["endpoint", "route", "json-rpc"], ["endpoint", "route", "rpc", "handler"], ["api"], 90),
  seed("query_plan", "Query Plan", "artifact", "Search scorer or postings plan.", ["bm25", "idf", "posting", "query"], ["bm25", "idf", "posting", "query"], ["retrieval"], 93),
  seed("recent", "Recent", "lifecycle", "Fresh work.", ["today", "recent", "latest"], ["recent", "latest", "fresh"], [], 78),
  seed("stable", "Stable", "lifecycle", "A trusted pattern.", ["stable", "proven", "reliable"], ["stable", "proven", "reliable"], ["decision"], 89),
  seed("avoid_pattern", "Avoid Pattern", "lifecycle", "A learned failure signature.", ["avoid", "pitfall", "mistake"], ["avoid", "pitfall", "mistake"], ["risk"], 98),
  seed("reinforced", "Reinforced", "lifecycle", "A strengthened pattern.", ["reinforced", "worked well"], ["reinforced", "worked", "success"], ["stable"], 87),
  seed("cross_project", "Cross Project", "lifecycle", "Knowledge shared across projects.", ["shared", "cross project", "portable"], ["shared", "portable", "cross"], ["architecture"], 80),
  seed("private", "Private", "privacy", "Contains private information.", ["private", "internal"], ["private", "internal", "sensitive"], ["security"], 95),
  seed("secret", "Secret", "privacy", "Contains secrets or credentials.", ["secret", "token", "password"], ["secret", "token", "password"], ["security"], 99),
  seed("rust", "Rust", "language", "Rust implementation context.", ["rust", ".rs", "cargo"], ["rust", ".rs", "cargo"], ["module"], 92),
  seed("typescript", "TypeScript", "language", "TypeScript implementation context.", ["typescript", ".ts", "node"], ["typescript", ".ts", "node"], ["module"], 91),
  seed("python", "Python", "language", "Python implementation context.", ["python", ".py"], ["python", ".py", "pip"], ["module"], 86),
  seed("sql", "SQL", "language", "SQL query context.", ["select", "from", "where", "sql"], ["select", "from", "where", "sql"], ["database_artifact"], 86)
];

export class TaxonomyEngine {
  decompose(
    content: string,
    metadata: MemoryMetadata,
    evolution: TaxonomyEvolutionSnapshot = defaultEvolutionSnapshot(),
    guidance?: ModelGuidance | null
  ): { sanitized: string; taxonomy: TaxonomyDecomposition } {
    const sanitized = redactSensitive(content);
    const tokens = tokenize(sanitized);
    const tokenSet = new Set(tokens);
    const scores = new Map<string, number>();
    const debug: TaxonomyDebugView = {
      matchedAliases: [],
      prototypeHits: [],
      pathHints: [],
      contextHints: [],
      derivedMarkers: []
    };

    for (const seedDef of seeds) {
      let score = 0;

      for (const alias of seedDef.aliases) {
        if (sanitized.toLowerCase().includes(alias)) {
          score += 2.1;
          debug.matchedAliases.push(alias);
        }
      }

      for (const term of seedDef.prototype) {
        if (tokenSet.has(term)) {
          score += 0.8;
          debug.prototypeHits.push(term);
        }
      }

      for (const path of metadata.paths) {
        if (seedDef.aliases.some((alias) => path.toLowerCase().includes(alias))) {
          score += 0.7;
          debug.pathHints.push(path);
        }
      }

      if (evolution.priorLabels.includes(seedDef.id)) {
        score += 0.35;
        debug.contextHints.push(`project-prior:${seedDef.id}`);
      }
      if (evolution.reinforcedLabels.includes(seedDef.id)) {
        score += 0.6;
        debug.contextHints.push(`reinforced-prior:${seedDef.id}`);
      }
      if (evolution.solidifiedPriors.includes(seedDef.id)) {
        score += 1.25;
        debug.contextHints.push(`solidified-prior:${seedDef.id}`);
      }
      if (seedDef.dependencies.some((dependency) => evolution.solidifiedPriors.includes(dependency))) {
        score += 0.35;
        debug.contextHints.push(`solidified-dependency:${seedDef.id}`);
      }
      if (evolution.avoidPatterns.includes(seedDef.id)) {
        score += 0.45;
        debug.contextHints.push(`avoid-prior:${seedDef.id}`);
      }

      if (guidance?.mainCategory === seedDef.id) {
        score += 1.15;
      }
      if (guidance?.boostedLabels.includes(seedDef.id)) {
        score += 0.72;
      }
      if (guidance?.languageHint?.toLowerCase() === seedDef.id) {
        score += 0.8;
      }

      scores.set(seedDef.id, score);
    }

    const ranked = [...seeds]
      .map((seedDef) => ({ seed: seedDef, score: scores.get(seedDef.id) ?? 0 }))
      .sort((a, b) => b.score - a.score || b.seed.priority - a.seed.priority);

    const mainCategory =
      guidance?.mainCategory ??
      ranked.find((candidate) => candidate.seed.dimension === "domain")?.seed.id ??
      "architecture";

    const topScore = Math.max(ranked[0]?.score ?? 1, 0.1);
    const signals: TaxonomySignal[] = ranked
      .filter((candidate) => candidate.score > 0.08)
      .slice(0, 24)
      .map((candidate) => ({
        label: candidate.seed.id,
        dimension: candidate.seed.dimension,
        score: candidate.score,
        confidence: clamp(candidate.score / topScore, 0, 0.99),
        reinforcementWeight: 0,
        failureCount: 0,
        reinforcementDecay: 1,
        reasons: [
          ...(evolution.priorLabels.includes(candidate.seed.id) ? ["project-prior"] : []),
          ...(guidance?.boostedLabels.includes(candidate.seed.id) ? ["model-guidance"] : [])
        ]
      }));

    const dimensions = buildDimensions(ranked, topScore);
    const relations = inferRelations(ranked);
    const headline = createHeadline(sanitized);
    const summary = createSummary(headline, dimensions, relations);

    const multiLabels = new Set<string>();
    for (const candidate of ranked.filter((candidate) => candidate.score > 0.08).slice(0, 20)) {
      multiLabels.add(candidate.seed.id);
      multiLabels.add(`${candidate.seed.dimension}:${candidate.seed.id}`);
    }
    for (const relation of relations) {
      multiLabels.add(`relation:${relation.source}:${relation.relation}:${relation.target}`);
    }
    for (const dimension of dimensions) {
      multiLabels.add(`dimension:${dimension.dimension}:${dimension.dominantLabel}`);
    }
    if (metadata.shareScope) multiLabels.add("scope:cross-project");
    if (metadata.language) multiLabels.add(`language:${metadata.language}`);
    if (guidance?.languageHint) multiLabels.add(`language:${guidance.languageHint}`);

    const avoidPatterns = unique([
      ...ranked
        .filter((candidate) => ["avoid_pattern", "risk"].includes(candidate.seed.id))
        .slice(0, 6)
        .map((candidate) => candidate.seed.id),
      ...(guidance?.avoidPatterns ?? [])
    ]);

    const reinforcePatterns = unique([
      ...ranked
        .filter((candidate) => ["stable", "reinforced", "decision"].includes(candidate.seed.id))
        .slice(0, 6)
        .map((candidate) => candidate.seed.id),
      ...(guidance?.reinforcePatterns ?? [])
    ]);

    while (multiLabels.size < 20) {
      multiLabels.add(`fallback:${seeds[multiLabels.size % seeds.length]!.id}`);
    }

    const taxonomy: TaxonomyDecomposition = {
      mainCategory,
      confidence: average(signals.map((signal) => signal.confidence).slice(0, 8)) || 0.2,
      multiLabels: [...multiLabels].slice(0, 50),
      hierarchy: buildHierarchy(ranked, dimensions),
      dimensions,
      signals,
      relations,
      avoidPatterns,
      reinforcePatterns,
      metadata: buildMetadata(
        headline,
        summary,
        metadata,
        tokens,
        relations,
        guidance,
        sanitized
      ),
      debug
    };

    return { sanitized, taxonomy };
  }
}

function buildMetadata(
  headline: string,
  summary: string,
  metadata: MemoryMetadata,
  tokens: string[],
  relations: TaxonomyRelation[],
  guidance: ModelGuidance | null | undefined,
  sanitized: string
): TaxonomyMetadata {
  return {
    headline,
    summary,
    languageHint: metadata.language ?? guidance?.languageHint ?? detectLanguage(tokens),
    classifierBackend: guidance?.backend ?? "algorithm",
    classifierNotes: guidance?.notes ?? [],
    modelConfidence: guidance?.confidence ?? 0,
    tokenCount: tokens.length,
    signalCount: tokens.length,
    sentenceCount: sanitized.split(/[.!?]+/).filter(Boolean).length || 1,
    lineCount: sanitized.split("\n").length,
    relationCount: relations.length,
    artifactDensity: tokens.filter((token) => token.includes("/") || token.includes(".")).length / Math.max(tokens.length, 1),
    confidenceMean: guidance?.confidence ?? 0.6,
    sensitivityTags: metadata.privacy === "secret" ? ["secret"] : [],
    emergentClusters: prefixClusters(tokens),
    entities: extractEntities(sanitized),
    redactions: [],
    taxonomyVersion: TAXONOMY_VERSION,
    compressionHint: tokens.length > 80 ? "summary-first" : "inline-safe",
    inferredKinds: []
  };
}

function buildDimensions(
  ranked: { seed: Seed; score: number }[],
  topScore: number
): DimensionBreakdown[] {
  const byDimension = new Map<string, { seed: Seed; score: number }[]>();
  for (const candidate of ranked.filter((candidate) => candidate.score > 0.08)) {
    const bucket = byDimension.get(candidate.seed.dimension) ?? [];
    bucket.push(candidate);
    byDimension.set(candidate.seed.dimension, bucket);
  }
  return [...byDimension.entries()].map(([dimension, items]) => ({
    dimension,
    dominantLabel: items[0]!.seed.id,
    labels: items.slice(0, 4).map((item) => item.seed.id),
    confidence: clamp(items[0]!.score / topScore, 0, 0.99)
  }));
}

function inferRelations(ranked: { seed: Seed; score: number }[]): TaxonomyRelation[] {
  const chosen = ranked.filter((candidate) => candidate.score > 0.12).slice(0, 10);
  const relations: TaxonomyRelation[] = [];
  for (let index = 0; index < chosen.length; index += 1) {
    const left = chosen[index]!;
    for (const right of chosen.slice(index + 1)) {
      if (left.seed.dependencies.includes(right.seed.id)) {
        relations.push({
          source: left.seed.id,
          target: right.seed.id,
          relation: "depends_on",
          weight: average([left.score, right.score]),
          evidence: `${left.seed.name} depends on ${right.seed.name}`
        });
      } else if (left.seed.dimension === right.seed.dimension) {
        relations.push({
          source: left.seed.id,
          target: right.seed.id,
          relation: "adjacent_to",
          weight: average([left.score, right.score]) * 0.5,
          evidence: `${left.seed.name} is adjacent to ${right.seed.name}`
        });
      }
    }
  }
  return relations.slice(0, 12);
}

function buildHierarchy(
  ranked: { seed: Seed; score: number }[],
  dimensions: DimensionBreakdown[]
): HierarchyNode[] {
  const root: HierarchyNode = {
    id: "memory",
    name: "Memory",
    level: 0,
    description: "Root taxonomy node.",
    priority: 100,
    confidence: 1,
    reinforcementWeight: 0,
    failureCount: 0,
    reinforcementDecay: 1,
    dependencies: [],
    relations: [],
    nodeType: "root"
  };
  const nodes = [root];
  for (const dimension of dimensions) {
    nodes.push({
      id: `dimension/${dimension.dimension}`,
      name: `${dimension.dimension} dimension`,
      level: 1,
      description: `Dominant ${dimension.dimension} signal.`,
      priority: 90,
      confidence: dimension.confidence,
      reinforcementWeight: 0,
      failureCount: 0,
      reinforcementDecay: 1,
      dependencies: ["memory"],
      relations: [dimension.dominantLabel],
      nodeType: "dimension"
    });
  }
  for (const candidate of ranked.filter((candidate) => candidate.score > 0.1).slice(0, 12)) {
    nodes.push({
      id: candidate.seed.id,
      name: candidate.seed.name,
      level: 2,
      description: candidate.seed.description,
      priority: candidate.seed.priority,
      confidence: candidate.score / Math.max(ranked[0]?.score ?? 1, 1),
      reinforcementWeight: 0,
      failureCount: 0,
      reinforcementDecay: 1,
      dependencies: candidate.seed.dependencies,
      relations: [],
      nodeType: candidate.seed.dimension
    });
  }
  return nodes;
}

function prefixClusters(tokens: string[]): string[] {
  const counts = new Map<string, number>();
  for (const token of tokens) {
    const prefix = token.slice(0, 4);
    if (prefix.length < 4) continue;
    counts.set(prefix, (counts.get(prefix) ?? 0) + 1);
  }
  return [...counts.entries()]
    .filter(([, count]) => count >= 2)
    .slice(0, 6)
    .map(([prefix]) => `cluster:${prefix}*`);
}

function extractEntities(content: string): string[] {
  return [...new Set(content.match(/\b[A-Z][A-Za-z0-9_-]{2,}\b/g) ?? [])].slice(0, 12);
}

function detectLanguage(tokens: string[]): string {
  if (tokens.some((token) => token.endsWith(".ts") || token === "typescript")) return "typescript";
  if (tokens.some((token) => token.endsWith(".rs") || token === "rust")) return "rust";
  if (tokens.some((token) => token === "select" || token === "where")) return "sql";
  return "mixed";
}

export function tokenize(input: string): string[] {
  return input
    .toLowerCase()
    .split(/[^a-z0-9_./:-]+/g)
    .filter((token) => token.length >= 2 && !stopwords.has(token));
}

function redactSensitive(input: string): string {
  return input.replace(/(sk_[a-z0-9_]+)/gi, "[REDACTED_SECRET]");
}

function createHeadline(content: string): string {
  return content.trim().replace(/\s+/g, " ").slice(0, 120) || "Memory";
}

function createSummary(
  headline: string,
  dimensions: DimensionBreakdown[],
  relations: TaxonomyRelation[]
): string {
  const dimensionSummary = dimensions
    .slice(0, 4)
    .map((item) => `${item.dimension}=${item.dominantLabel}`)
    .join(", ");
  const relationSummary = relations
    .slice(0, 2)
    .map((item) => `${item.source} ${item.relation} ${item.target}`)
    .join(" | ");
  return [headline, dimensionSummary, relationSummary].filter(Boolean).join(" | ");
}

function seed(
  id: string,
  name: string,
  dimension: string,
  description: string,
  aliases: string[],
  prototype: string[],
  dependencies: string[],
  priority: number
): Seed {
  return { id, name, dimension, description, aliases, prototype, dependencies, priority };
}

function unique(values: string[]): string[] {
  return [...new Set(values)];
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function average(values: number[]): number {
  if (!values.length) return 0;
  return values.reduce((sum, value) => sum + value, 0) / values.length;
}

const stopwords = new Set([
  "the",
  "and",
  "for",
  "with",
  "this",
  "that",
  "from",
  "into",
  "your",
  "have"
]);

export function contentHash(content: string): string {
  return createHash("blake2s256").update(content).digest("hex");
}
