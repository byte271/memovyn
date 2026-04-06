import type {
  MemoryRecord,
  ProgressiveIndexCard,
  ProgressiveSummary,
  SearchFilters,
  SearchHit,
  SearchResponse,
  TaxonomyEvolutionSnapshot,
  TimelineEntry
} from "./types.ts";
import { defaultSearchFilters } from "./types.ts";
import { tokenize } from "./taxonomy.ts";

type Posting = { docIdx: number; termFreq: number };
type IndexedMemory = {
  record: MemoryRecord;
  docLen: number;
  scoreHint: number;
  labelBlob: string;
  relationBlob: string;
};

type ProjectInsights = {
  labelCounts: Map<string, number>;
  reinforcedCounts: Map<string, number>;
  solidifiedCounts: Map<string, number>;
  avoidCounts: Map<string, number>;
  relationCounts: Map<string, number>;
  dimensionCounts: Map<string, Map<string, number>>;
  privacyCounts: Map<string, number>;
  termDf: Map<string, number>;
};

type ProjectIndex = {
  docs: IndexedMemory[];
  docLookup: Map<string, number>;
  postings: Map<string, Posting[]>;
  totalDocLen: number;
  avgDocLen: number;
  activeDocCount: number;
  insights: ProjectInsights;
  queryCache: Map<string, { totalHits: number; hits: SearchHit[] }>;
};

export class SearchIndex {
  private readonly projects = new Map<string, ProjectIndex>();

  constructor(records: MemoryRecord[] = []) {
    for (const record of records) {
      this.insert(record);
    }
  }

  hasProject(projectId: string): boolean {
    return this.projects.has(projectId);
  }

  insert(record: MemoryRecord): void {
    const project = this.projects.get(record.projectId) ?? createProjectIndex();
    const docIdx = project.docs.length;
    const tokens = buildSearchTokens(record);
    const freq = new Map<string, number>();
    for (const token of tokens) {
      freq.set(token, (freq.get(token) ?? 0) + 1);
    }
    for (const [token, termFreq] of freq) {
      const bucket = project.postings.get(token) ?? [];
      bucket.push({ docIdx, termFreq });
      project.postings.set(token, bucket);
      project.insights.termDf.set(token, (project.insights.termDf.get(token) ?? 0) + 1);
    }

    project.docs.push({
      record,
      docLen: Math.max(tokens.length, 1),
      scoreHint: rankingBias(record),
      labelBlob: record.taxonomy.multiLabels.join(" "),
      relationBlob: record.taxonomy.relations
        .map((relation) => `${relation.source} ${relation.relation} ${relation.target}`)
        .join(" ")
    });
    project.docLookup.set(record.id, docIdx);
    project.totalDocLen += Math.max(tokens.length, 1);
    project.avgDocLen = project.totalDocLen / project.docs.length;
    if (!isArchived(record)) {
      project.activeDocCount += 1;
    }
    observeProjectInsights(project, record);
    project.queryCache.clear();
    this.projects.set(record.projectId, project);
  }

  refresh(record: MemoryRecord): void {
    const project = this.projects.get(record.projectId);
    if (!project) {
      this.insert(record);
      return;
    }
    const docIdx = project.docLookup.get(record.id);
    if (docIdx === undefined) {
      this.insert(record);
      return;
    }
    project.docs[docIdx] = {
      record,
      docLen: Math.max(buildSearchTokens(record).length, 1),
      scoreHint: rankingBias(record),
      labelBlob: record.taxonomy.multiLabels.join(" "),
      relationBlob: record.taxonomy.relations
        .map((relation) => `${relation.source} ${relation.relation} ${relation.target}`)
        .join(" ")
    };
    rebuildProjectIndex(project);
  }

  taxonomyFeedback(projectId: string): TaxonomyEvolutionSnapshot {
    const project = this.projects.get(projectId);
    if (!project) {
      return { priorLabels: [], reinforcedLabels: [], solidifiedPriors: [], avoidPatterns: [], projectTerms: [] };
    }
    return {
      priorLabels: topKeys(project.insights.labelCounts, 10),
      reinforcedLabels: topKeys(project.insights.reinforcedCounts, 8),
      solidifiedPriors: topKeys(project.insights.solidifiedCounts, 6),
      avoidPatterns: topKeys(project.insights.avoidCounts, 8),
      projectTerms: topKeys(project.insights.termDf, 12)
    };
  }

  projectSummary(projectId: string): {
    topLabels: [string, number][];
    topRelations: [string, number][];
    dominantDimensions: [string, string][];
    avoidPatterns: string[];
    privacySignals: string[];
    activeConflicts: string[];
  } {
    const project = this.projects.get(projectId);
    if (!project) {
      return {
        topLabels: [],
        topRelations: [],
        dominantDimensions: [],
        avoidPatterns: [],
        privacySignals: [],
        activeConflicts: []
      };
    }
    const dominantDimensions = [...project.insights.dimensionCounts.entries()].map(([dimension, counts]) => {
      const top = [...counts.entries()].sort((a, b) => b[1] - a[1])[0];
      return [dimension, top?.[0] ?? "unknown"] as [string, string];
    });
    return {
      topLabels: topPairs(project.insights.labelCounts, 24),
      topRelations: topPairs(project.insights.relationCounts, 20),
      dominantDimensions,
      avoidPatterns: topKeys(project.insights.avoidCounts, 8),
      privacySignals: topKeys(project.insights.privacyCounts, 8),
      activeConflicts: project.docs
        .filter((doc) => doc.record.learning.conflictScore > 0 || doc.record.penalty > doc.record.reinforcement)
        .slice(0, 8)
        .map((doc) => doc.record.headline)
    };
  }

  projectAnalytics(projectId: string): {
    labelHotspots: [string, number][];
    relationHotspots: [string, number][];
    conflictCount: number;
  } {
    const project = this.projects.get(projectId);
    if (!project) {
      return { labelHotspots: [], relationHotspots: [], conflictCount: 0 };
    }
    return {
      labelHotspots: topPairs(project.insights.labelCounts, 24),
      relationHotspots: topPairs(project.insights.relationCounts, 20),
      conflictCount: project.docs.filter((doc) => doc.record.learning.conflictScore > 0 || doc.record.penalty > doc.record.reinforcement).length
    };
  }

  search(
    projectId: string,
    query: string,
    limit: number,
    filters: SearchFilters = defaultSearchFilters(),
    sharedProjects: string[] = []
  ): SearchResponse {
    const projectIds = [projectId, ...(filters.includeShared ? sharedProjects : [])];
    const hits: SearchHit[] = [];
    let totalHits = 0;
    for (const id of projectIds) {
      const project = this.projects.get(id);
      if (!project) continue;
      const result = searchProject(project, query, limit, filters);
      hits.push(...result.hits);
      totalHits += result.totalHits;
    }
    hits.sort((a, b) => b.score - a.score || b.createdAt.localeCompare(a.createdAt));
    const topHits = hits.slice(0, Math.max(limit, 1));
    return {
      projectId,
      query,
      totalHits,
      indexLayer: topHits.map(toIndexCard),
      summaryLayer: topHits.map(toSummary),
      timelineLayer: topHits.map(toTimeline),
      detailLayer: topHits
    };
  }

  recentCards(projectId: string, limit: number): ProgressiveIndexCard[] {
    const project = this.projects.get(projectId);
    if (!project) return [];
    return project.docs
      .filter((doc) => !isArchived(doc.record))
      .slice(-limit)
      .reverse()
      .map(toIndexCardFromDoc);
  }

  recentSummaries(projectId: string, limit: number): ProgressiveSummary[] {
    const project = this.projects.get(projectId);
    if (!project) return [];
    return project.docs
      .filter((doc) => !isArchived(doc.record))
      .slice(-limit)
      .reverse()
      .map((doc) => ({
        memoryId: doc.record.id,
        mainCategory: doc.record.taxonomy.mainCategory,
        confidence: doc.record.taxonomy.confidence,
        explanation: doc.record.taxonomy.metadata.summary,
        relations: doc.record.taxonomy.relations
          .slice(0, 4)
          .map((relation) => `${relation.source} ${relation.relation} ${relation.target}`)
      }));
  }

  recentTimeline(projectId: string, limit: number): TimelineEntry[] {
    const project = this.projects.get(projectId);
    if (!project) return [];
    return project.docs
      .filter((doc) => !isArchived(doc.record))
      .slice(-limit)
      .reverse()
      .map((doc) => ({
        memoryId: doc.record.id,
        timestamp: doc.record.createdAt,
        headline: doc.record.headline,
        changeSignal: doc.record.kind
      }));
  }

  getMemory(memoryId: string): MemoryRecord | undefined {
    for (const project of this.projects.values()) {
      const idx = project.docLookup.get(memoryId);
      if (idx !== undefined) {
        return project.docs[idx]?.record;
      }
    }
    return undefined;
  }
}

function searchProject(
  project: ProjectIndex,
  query: string,
  limit: number,
  filters: SearchFilters
): { totalHits: number; hits: SearchHit[] } {
  const cacheKey = `${query}|${JSON.stringify(filters)}`;
  const cached = project.queryCache.get(cacheKey);
  if (cached && cached.hits.length >= limit) {
    return { totalHits: cached.totalHits, hits: cached.hits.slice(0, limit) };
  }

  const tokens = tokenize(query);
  const scores = new Map<number, number>();
  for (const token of tokens) {
    const postings = project.postings.get(token) ?? [];
    const df = postings.length || 1;
    const idf = Math.log(((project.docs.length - df + 0.5) / (df + 0.5)) + 1);
    for (const posting of postings) {
      const doc = project.docs[posting.docIdx]!;
      if (!matchesFilters(doc.record, filters)) continue;
      const bm25 =
        idf *
        ((posting.termFreq * 2.2) /
          (posting.termFreq + 1.2 * (1 - 0.75 + 0.75 * (doc.docLen / Math.max(project.avgDocLen, 1)))));
      scores.set(posting.docIdx, (scores.get(posting.docIdx) ?? 0) + bm25);
    }
  }

  const useShortlist =
    !tokens.length ||
    tokens.every((token) => ((project.postings.get(token)?.length ?? 0) / Math.max(project.docs.length, 1)) > 0.18);

  const candidateIndices = useShortlist
    ? shortlistCandidates(project, limit, filters)
    : [...scores.keys()];

  const totalHits = useShortlist
    ? countBroadMatches(project, filters, tokens)
    : candidateIndices.filter((idx) => matchesFilters(project.docs[idx]!.record, filters)).length;

  const hits = candidateIndices
    .map((idx) => {
      const doc = project.docs[idx]!;
      if (!matchesFilters(doc.record, filters)) return undefined;
      const overlap = tokens.filter((token) => doc.labelBlob.includes(token) || doc.relationBlob.includes(token)).length;
      const score = (scores.get(idx) ?? 0) + doc.scoreHint + overlap * 0.35 + recencyBoost(doc.record.createdAt);
      return toSearchHit(doc.record, score);
    })
    .filter((hit): hit is SearchHit => Boolean(hit))
    .sort((a, b) => b.score - a.score || b.createdAt.localeCompare(a.createdAt));

  if (!hits.length && project.docs.length) {
    const fallbackHits = shortlistCandidates(project, limit, filters)
      .map((idx) => {
        const doc = project.docs[idx]!;
        const overlap = tokens.filter(
          (token) =>
            doc.labelBlob.includes(token) ||
            doc.relationBlob.includes(token) ||
            doc.record.summary.toLowerCase().includes(token) ||
            doc.record.content.toLowerCase().includes(token)
        ).length;
        return overlap > 0 || !tokens.length
          ? toSearchHit(doc.record, doc.scoreHint + overlap * 0.4 + recencyBoost(doc.record.createdAt))
          : undefined;
      })
      .filter((hit): hit is SearchHit => Boolean(hit))
      .sort((a, b) => b.score - a.score || b.createdAt.localeCompare(a.createdAt));
    if (fallbackHits.length) {
      project.queryCache.set(cacheKey, { totalHits: fallbackHits.length, hits: fallbackHits.slice(0, 64) });
      return { totalHits: fallbackHits.length, hits: fallbackHits.slice(0, Math.max(limit * 6, 64)) };
    }
  }

  project.queryCache.set(cacheKey, { totalHits, hits: hits.slice(0, 64) });
  return { totalHits, hits: hits.slice(0, Math.max(limit * 6, 64)) };
}

function shortlistCandidates(project: ProjectIndex, limit: number, filters: SearchFilters): number[] {
  const shortlist = Math.min(project.activeDocCount, Math.max(limit * 96, 2048));
  const indices: number[] = [];
  for (let index = project.docs.length - 1; index >= 0 && indices.length < shortlist; index -= 1) {
    const doc = project.docs[index]!;
    if (!matchesFilters(doc.record, filters)) continue;
    indices.push(index);
  }
  return indices;
}

function matchesFilters(record: MemoryRecord, filters: SearchFilters): boolean {
  if (!filters.includeArchived && isArchived(record)) return false;
  if (filters.kinds.length && !filters.kinds.includes(record.kind)) return false;
  if (!filters.includePrivateNotes && record.kind === "note" && record.metadata.privacy !== "standard") return false;
  if (filters.labels.length && !filters.labels.every((label) => record.taxonomy.multiLabels.includes(label))) return false;
  if (filters.since && record.createdAt < filters.since) return false;
  if (filters.until && record.createdAt > filters.until) return false;
  return true;
}

function observeProjectInsights(project: ProjectIndex, record: MemoryRecord): void {
  for (const label of record.taxonomy.multiLabels) {
    increment(project.insights.labelCounts, label);
    if (label.includes("avoid") || label.includes("regression")) increment(project.insights.avoidCounts, label);
    if (record.reinforcement >= record.penalty) increment(project.insights.reinforcedCounts, label);
    if (record.learning.successScore >= 2 || record.accessCount >= 3) increment(project.insights.solidifiedCounts, label);
    if (label.startsWith("privacy:") || label.startsWith("sensitive:")) increment(project.insights.privacyCounts, label);
  }
  for (const relation of record.taxonomy.relations) {
    increment(project.insights.relationCounts, `${relation.source}:${relation.relation}:${relation.target}`);
  }
  for (const dimension of record.taxonomy.dimensions) {
    const bucket = project.insights.dimensionCounts.get(dimension.dimension) ?? new Map<string, number>();
    increment(bucket, dimension.dominantLabel);
    project.insights.dimensionCounts.set(dimension.dimension, bucket);
  }
}

function rebuildProjectIndex(project: ProjectIndex): void {
  project.docLookup = new Map();
  project.postings = new Map();
  project.totalDocLen = 0;
  project.avgDocLen = 1;
  project.activeDocCount = 0;
  project.insights = {
    labelCounts: new Map(),
    reinforcedCounts: new Map(),
    solidifiedCounts: new Map(),
    avoidCounts: new Map(),
    relationCounts: new Map(),
    dimensionCounts: new Map(),
    privacyCounts: new Map(),
    termDf: new Map()
  };

  project.docs = project.docs.map((doc, docIdx) => {
    const tokens = buildSearchTokens(doc.record);
    const docLen = Math.max(tokens.length, 1);
    const freq = new Map<string, number>();
    for (const token of tokens) {
      freq.set(token, (freq.get(token) ?? 0) + 1);
    }
    for (const [token, termFreq] of freq) {
      const bucket = project.postings.get(token) ?? [];
      bucket.push({ docIdx, termFreq });
      project.postings.set(token, bucket);
      project.insights.termDf.set(token, (project.insights.termDf.get(token) ?? 0) + 1);
    }
    project.docLookup.set(doc.record.id, docIdx);
    project.totalDocLen += docLen;
    if (!isArchived(doc.record)) {
      project.activeDocCount += 1;
    }
    observeProjectInsights(project, doc.record);
    return {
      record: doc.record,
      docLen,
      scoreHint: rankingBias(doc.record),
      labelBlob: doc.record.taxonomy.multiLabels.join(" "),
      relationBlob: doc.record.taxonomy.relations
        .map((relation) => `${relation.source} ${relation.relation} ${relation.target}`)
        .join(" ")
    };
  });

  project.avgDocLen = project.docs.length ? project.totalDocLen / project.docs.length : 1;
  project.queryCache.clear();
}

function countBroadMatches(project: ProjectIndex, filters: SearchFilters, tokens: string[]): number {
  let count = 0;
  for (const doc of project.docs) {
    if (!matchesFilters(doc.record, filters)) continue;
    if (
      !tokens.length ||
      tokens.some(
        (token) =>
          doc.labelBlob.includes(token) ||
          doc.relationBlob.includes(token) ||
          doc.record.summary.toLowerCase().includes(token) ||
          doc.record.content.toLowerCase().includes(token)
      )
    ) {
      count += 1;
    }
  }
  return count;
}

function createProjectIndex(): ProjectIndex {
  return {
    docs: [],
    docLookup: new Map(),
    postings: new Map(),
    totalDocLen: 0,
    avgDocLen: 1,
    activeDocCount: 0,
    insights: {
      labelCounts: new Map(),
      reinforcedCounts: new Map(),
      solidifiedCounts: new Map(),
      avoidCounts: new Map(),
      relationCounts: new Map(),
      dimensionCounts: new Map(),
      privacyCounts: new Map(),
      termDf: new Map()
    },
    queryCache: new Map()
  };
}

function buildSearchTokens(record: MemoryRecord): string[] {
  return tokenize(
    [
      record.content,
      record.summary,
      record.taxonomy.mainCategory,
      ...record.taxonomy.multiLabels,
      ...record.taxonomy.relations.map((relation) => `${relation.source} ${relation.relation} ${relation.target}`)
    ].join(" ")
  );
}

function rankingBias(record: MemoryRecord): number {
  return (
    record.reinforcement +
    record.learning.successScore * 0.45 +
    record.taxonomy.confidence -
    record.penalty * record.learning.reinforcementDecay -
    record.learning.conflictScore * 0.18 -
    record.learning.failureCount * 0.12
  );
}

function recencyBoost(createdAt: string): number {
  const ageHours = Math.abs(Date.now() - Date.parse(createdAt)) / (1000 * 60 * 60);
  return 1 / (1 + ageHours / 72);
}

function isArchived(record: MemoryRecord): boolean {
  return record.metadata.extra.archived === "true";
}

function increment(map: Map<string, number>, key: string): void {
  map.set(key, (map.get(key) ?? 0) + 1);
}

function topPairs(map: Map<string, number>, limit: number): [string, number][] {
  return [...map.entries()].sort((a, b) => b[1] - a[1] || a[0].localeCompare(b[0])).slice(0, limit);
}

function topKeys(map: Map<string, number>, limit: number): string[] {
  return topPairs(map, limit).map(([key]) => key);
}

function toSearchHit(record: MemoryRecord, score: number): SearchHit {
  return {
    memoryId: record.id,
    score,
    headline: record.headline,
    summary: record.summary,
    content: record.content,
    labels: record.taxonomy.multiLabels,
    mainCategory: record.taxonomy.mainCategory,
    confidence: record.taxonomy.confidence,
    relationCount: record.taxonomy.relations.length,
    explanation: record.taxonomy.metadata.summary,
    createdAt: record.createdAt,
    reinforcement: record.reinforcement,
    penalty: record.penalty
  };
}

function toIndexCard(hit: SearchHit): ProgressiveIndexCard {
  return {
    memoryId: hit.memoryId,
    headline: hit.headline,
    summary: hit.summary,
    labels: hit.labels.slice(0, 8),
    score: hit.score
  };
}

function toIndexCardFromDoc(doc: IndexedMemory): ProgressiveIndexCard {
  return {
    memoryId: doc.record.id,
    headline: doc.record.headline,
    summary: doc.record.summary,
    labels: doc.record.taxonomy.multiLabels.slice(0, 6),
    score: doc.scoreHint
  };
}

function toSummary(hit: SearchHit): ProgressiveSummary {
  return {
    memoryId: hit.memoryId,
    mainCategory: hit.mainCategory,
    confidence: hit.confidence,
    explanation: hit.explanation,
    relations: hit.labels.filter((label) => label.startsWith("relation:")).slice(0, 4)
  };
}

function toTimeline(hit: SearchHit): TimelineEntry {
  return {
    memoryId: hit.memoryId,
    timestamp: hit.createdAt,
    headline: hit.headline,
    changeSignal: hit.penalty > hit.reinforcement ? "avoid" : "reinforce"
  };
}
