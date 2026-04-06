import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname } from "node:path";
import { createRequire } from "node:module";

import type {
  AnalyticsBucket,
  AnalyticsSnapshot,
  FeedbackOutcome,
  MemoryRecord,
  MemoryVersionSnapshot,
  RankedMemoryStat
} from "./types.ts";

type JsonProjectState = {
  createdAt: string;
  updatedAt: string;
  shareScope: boolean;
  totalQueries: number;
  totalTokenSavings: number;
};

type JsonState = {
  projects: Record<string, JsonProjectState>;
  memories: MemoryRecord[];
  memoryVersions: Record<string, MemoryVersionSnapshot[]>;
  recollections: { memoryId: string; query: string; recalledAt: string; tokensSaved: number }[];
  feedbackEvents: {
    memoryId: string;
    projectId: string;
    outcome: FeedbackOutcome;
    repeatedMistake: boolean;
    weight: number;
    note?: string;
    createdAt: string;
  }[];
};

export class Storage {
  readonly db: any | null;
  readonly stateFile: string;
  private state: JsonState;

  constructor(databasePath: string) {
    mkdirSync(dirname(databasePath), { recursive: true });
    this.stateFile = `${databasePath}.json`;
    this.db = createDatabase(databasePath);
    this.state = this.db ? createEmptyState() : loadJsonState(this.stateFile);
    if (this.db) {
      this.db.exec(`
      PRAGMA journal_mode = WAL;
      PRAGMA synchronous = NORMAL;
      PRAGMA temp_store = MEMORY;

      CREATE TABLE IF NOT EXISTS projects (
        project_id TEXT PRIMARY KEY,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        share_scope INTEGER NOT NULL DEFAULT 0,
        total_token_savings INTEGER NOT NULL DEFAULT 0,
        total_queries INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS memories (
        memory_id TEXT PRIMARY KEY,
        project_id TEXT NOT NULL,
        record_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        updated_at TEXT NOT NULL,
        access_count INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS memory_versions (
        memory_id TEXT NOT NULL,
        version INTEGER NOT NULL,
        snapshot_json TEXT NOT NULL,
        created_at TEXT NOT NULL,
        PRIMARY KEY(memory_id, version)
      );

      CREATE TABLE IF NOT EXISTS recollections (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        memory_id TEXT NOT NULL,
        query TEXT NOT NULL,
        recalled_at TEXT NOT NULL,
        tokens_saved INTEGER NOT NULL DEFAULT 0
      );

      CREATE TABLE IF NOT EXISTS feedback_events (
        id INTEGER PRIMARY KEY AUTOINCREMENT,
        memory_id TEXT NOT NULL,
        project_id TEXT NOT NULL,
        outcome TEXT NOT NULL,
        repeated_mistake INTEGER NOT NULL DEFAULT 0,
        weight REAL NOT NULL DEFAULT 1,
        note TEXT,
        created_at TEXT NOT NULL
      );
    `);
      ensureMemoryJsonColumn(this.db);
    }
  }

  upsertProject(projectId: string, shareScope: boolean): void {
    const now = nowIso();
    if (!this.db) {
      const current = this.state.projects[projectId] ?? {
        createdAt: now,
        updatedAt: now,
        shareScope,
        totalQueries: 0,
        totalTokenSavings: 0
      };
      this.state.projects[projectId] = {
        ...current,
        updatedAt: now,
        shareScope
      };
      this.flushState();
      return;
    }
    this.db
      .prepare(
        `INSERT INTO projects(project_id, created_at, updated_at, share_scope)
         VALUES (?, ?, ?, ?)
         ON CONFLICT(project_id) DO UPDATE SET updated_at=excluded.updated_at, share_scope=excluded.share_scope`
      )
      .run(projectId, now, now, shareScope ? 1 : 0);
  }

  insertMemory(memory: MemoryRecord): void {
    if (!this.db) {
      const index = this.state.memories.findIndex((candidate) => candidate.id === memory.id);
      if (index >= 0) {
        this.state.memories[index] = memory;
      } else {
        this.state.memories.push(memory);
      }
      this.insertVersion(memory);
      this.flushState();
      return;
    }
    ensureProjectRow(this.db, memory.projectId, memory.metadata.shareScope);
    this.db
      .prepare(
        `INSERT OR REPLACE INTO memories(memory_id, project_id, record_json, created_at, updated_at, access_count)
         VALUES (?, ?, ?, ?, ?, ?)`
      )
      .run(
        memory.id,
        memory.projectId,
        JSON.stringify(memory),
        memory.createdAt,
        memory.updatedAt,
        memory.accessCount
      );
    this.insertVersion(memory);
  }

  updateMemory(memory: MemoryRecord): void {
    if (!this.db) {
      const index = this.state.memories.findIndex((candidate) => candidate.id === memory.id);
      if (index >= 0) {
        this.state.memories[index] = memory;
      } else {
        this.state.memories.push(memory);
      }
      this.insertVersion(memory);
      this.flushState();
      return;
    }
    ensureProjectRow(this.db, memory.projectId, memory.metadata.shareScope);
    this.db
      .prepare(
        `UPDATE memories
         SET record_json = ?, updated_at = ?, access_count = ?
         WHERE memory_id = ?`
      )
      .run(JSON.stringify(memory), memory.updatedAt, memory.accessCount, memory.id);
    this.insertVersion(memory);
  }

  private insertVersion(memory: MemoryRecord): void {
    if (!this.db) {
      const bucket = this.state.memoryVersions[memory.id] ?? [];
      bucket.push({
        version: memory.version,
        createdAt: memory.updatedAt,
        headline: memory.headline,
        reinforcement: memory.reinforcement,
        penalty: memory.penalty
      });
      this.state.memoryVersions[memory.id] = bucket;
      return;
    }
    this.db
      .prepare(
        `INSERT OR REPLACE INTO memory_versions(memory_id, version, snapshot_json, created_at)
         VALUES (?, ?, ?, ?)`
      )
      .run(memory.id, memory.version, JSON.stringify(memory), memory.updatedAt);
  }

  loadAllMemories(): MemoryRecord[] {
    if (!this.db) {
      return [...this.state.memories];
    }
    const rows = this.db.prepare(`SELECT * FROM memories ORDER BY created_at ASC`).all() as Record<string, unknown>[];
    return rows
      .map((row) => hydrateRecord(this.db, row))
      .filter((record): record is MemoryRecord => Boolean(record));
  }

  getMemory(memoryId: string): MemoryRecord | undefined {
    if (!this.db) {
      return this.state.memories.find((memory) => memory.id === memoryId);
    }
    const row = this.db
      .prepare(`SELECT * FROM memories WHERE memory_id = ?`)
      .get(memoryId) as Record<string, unknown> | undefined;
    return row ? hydrateRecord(this.db, row) : undefined;
  }

  memoryVersions(memoryId: string): MemoryVersionSnapshot[] {
    if (!this.db) {
      return this.state.memoryVersions[memoryId] ?? [];
    }
    const rows = this.db
      .prepare(`SELECT snapshot_json FROM memory_versions WHERE memory_id = ? ORDER BY version ASC`)
      .all(memoryId) as { snapshot_json: string }[];
    return rows.map((row) => {
      const memory = JSON.parse(row.snapshot_json) as MemoryRecord;
      return {
        version: memory.version,
        createdAt: memory.updatedAt,
        headline: memory.headline,
        reinforcement: memory.reinforcement,
        penalty: memory.penalty
      };
    });
  }

  recordRecall(memoryId: string, query: string, tokensSaved: number): void {
    const memory = this.getMemory(memoryId);
    if (!memory) return;
    const now = nowIso();
    memory.accessCount += 1;
    memory.lastAccessedAt = now;
    this.updateMemory(memory);
    if (!this.db) {
      this.state.recollections.push({
        memoryId,
        query,
        recalledAt: now,
        tokensSaved
      });
      const project = this.state.projects[memory.projectId];
      if (project) {
        project.totalQueries += 1;
        project.totalTokenSavings += tokensSaved;
        project.updatedAt = now;
      }
      this.flushState();
      return;
    }
    this.db
      .prepare(`INSERT INTO recollections(memory_id, query, recalled_at, tokens_saved) VALUES (?, ?, ?, ?)`)
      .run(memoryId, query, now, tokensSaved);
    this.db
      .prepare(
        `UPDATE projects SET total_queries = total_queries + 1, total_token_savings = total_token_savings + 1 * ?, updated_at = ? WHERE project_id = ?`
      )
      .run(tokensSaved, now, memory.projectId);
  }

  recordFeedback(
    memoryId: string,
    outcome: FeedbackOutcome,
    repeatedMistake: boolean,
    weight: number,
    note?: string
  ): void {
    const memory = this.getMemory(memoryId);
    if (!memory) return;
    if (!this.db) {
      this.state.feedbackEvents.push({
        memoryId: memory.id,
        projectId: memory.projectId,
        outcome,
        repeatedMistake,
        weight,
        note,
        createdAt: nowIso()
      });
      this.flushState();
      return;
    }
    this.db
      .prepare(
        `INSERT INTO feedback_events(memory_id, project_id, outcome, repeated_mistake, weight, note, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?)`
      )
      .run(
        memory.id,
        memory.projectId,
        outcome,
        repeatedMistake ? 1 : 0,
        weight,
        note ?? null,
        nowIso()
      );
  }

  listProjects(): { projectId: string; memoryCount: number; lastUpdatedAt?: string; shareScope: boolean; totalTokenSavings: number; conflictCount: number }[] {
    if (!this.db) {
      return Object.entries(this.state.projects).map(([projectId, project]) => ({
        projectId,
        memoryCount: this.state.memories.filter((memory) => memory.projectId === projectId).length,
        lastUpdatedAt: project.updatedAt,
        shareScope: project.shareScope,
        totalTokenSavings: project.totalTokenSavings,
        conflictCount: this.state.memories.filter(
          (memory) =>
            memory.projectId === projectId &&
            (memory.learning.conflictScore > 0 || memory.penalty > memory.reinforcement)
        ).length
      }));
    }
    const rows = this.db
      .prepare(
        `SELECT p.project_id, p.share_scope, p.total_token_savings, MAX(m.updated_at) AS last_updated_at, COUNT(m.memory_id) AS memory_count
         FROM projects p LEFT JOIN memories m ON m.project_id = p.project_id
         GROUP BY p.project_id, p.share_scope, p.total_token_savings
         ORDER BY last_updated_at DESC`
      )
      .all() as {
      project_id: string;
      share_scope: number;
      total_token_savings: number;
      last_updated_at?: string;
      memory_count: number;
    }[];

    const conflictsByProject = new Map<string, number>();
    for (const memory of this.loadAllMemories()) {
      if (memory.learning.conflictScore > 0 || memory.penalty > memory.reinforcement) {
        conflictsByProject.set(memory.projectId, (conflictsByProject.get(memory.projectId) ?? 0) + 1);
      }
    }

    return rows.map((row) => ({
      projectId: row.project_id,
      memoryCount: Number(row.memory_count),
      lastUpdatedAt: row.last_updated_at,
      shareScope: Boolean(row.share_scope),
      totalTokenSavings: Number(row.total_token_savings),
      conflictCount: conflictsByProject.get(row.project_id) ?? 0
    }));
  }

  listSharedProjects(excludeProjectId: string): string[] {
    if (!this.db) {
      return Object.entries(this.state.projects)
        .filter(([projectId, project]) => projectId !== excludeProjectId && project.shareScope)
        .map(([projectId]) => projectId);
    }
    const rows = this.db
      .prepare(`SELECT project_id FROM projects WHERE share_scope = 1 AND project_id != ? ORDER BY project_id ASC`)
      .all(excludeProjectId) as { project_id: string }[];
    return rows.map((row) => row.project_id);
  }

  analytics(projectId: string): AnalyticsSnapshot {
    const memories = this.loadAllMemories().filter((memory) => memory.projectId === projectId);
    const totalQueries = !this.db
      ? (this.state.projects[projectId]?.totalQueries ?? 0)
      : Number(
          (
            this.db.prepare(`SELECT total_queries FROM projects WHERE project_id = ?`).get(projectId) as {
              total_queries?: number;
            } | undefined
          )?.total_queries ?? 0
        );
    const totalTokenSavings = !this.db
      ? (this.state.projects[projectId]?.totalTokenSavings ?? 0)
      : Number(
          (
            this.db.prepare(`SELECT total_token_savings FROM projects WHERE project_id = ?`).get(projectId) as {
              total_token_savings?: number;
            } | undefined
          )?.total_token_savings ?? 0
        );

    return {
      projectId,
      totalMemories: memories.length,
      totalQueries,
      totalTokenSavings,
      estimatedTokensPerRecall: totalQueries === 0 ? 0 : Math.round(totalTokenSavings / totalQueries),
      sessionQueries: 0,
      sessionTokenSavings: 0,
      conflictCount: memories.filter((memory) => memory.learning.conflictScore > 0 || memory.penalty > memory.reinforcement).length,
      reinforcedMemories: memories.filter((memory) => memory.reinforcement > memory.penalty).length,
      penalizedMemories: memories.filter((memory) => memory.penalty > 0).length,
      memoryHealthScore: 0,
      learningImpactScore: 0,
      mostRecalled: rankMemories(memories, (memory) => memory.accessCount),
      mostReinforced: rankMemories(memories, (memory) => memory.reinforcement),
      mostPunished: rankMemories(memories, (memory) => memory.penalty),
      mostImpactful: rankMemories(
        memories,
        (memory) =>
          memory.reinforcement +
          memory.learning.successScore +
          memory.accessCount * 0.15 -
          memory.penalty -
          memory.learning.failureCount * 0.4
      ),
      labelHotspots: [],
      relationHotspots: [],
      conflictHeatmap: this.db ? loadConflictHeatmap(this.db, projectId) : loadConflictHeatmapFromState(this.state, projectId),
      growth: bucketMemories(memories),
      evolutionTrend: this.db ? loadEvolutionTrend(this.db, projectId) : loadEvolutionTrendFromState(this.state, projectId),
      agentEvolutionTimeline: this.db ? loadEvolutionTrend(this.db, projectId) : loadEvolutionTrendFromState(this.state, projectId),
      behaviorInsights: [],
      proactiveSuggestions: []
    };
  }

  projectActivity(projectId: string): { memoryCount: number; totalQueries: number } {
    if (!this.db) {
      return {
        memoryCount: this.state.memories.filter((memory) => memory.projectId === projectId).length,
        totalQueries: this.state.projects[projectId]?.totalQueries ?? 0
      };
    }
    return {
      memoryCount: this.loadAllMemories().filter((memory) => memory.projectId === projectId).length,
      totalQueries: Number(
        (
          this.db.prepare(`SELECT total_queries FROM projects WHERE project_id = ?`).get(projectId) as {
            total_queries?: number;
          } | undefined
        )?.total_queries ?? 0
      )
    };
  }

  archiveLowValueMemories(projectId: string, limit: number): MemoryRecord[] {
    const memories = this.loadAllMemories()
      .filter((memory) => memory.projectId === projectId)
      .filter(
        (memory) =>
          memory.accessCount <= 1 &&
          memory.reinforcement <= 0.2 &&
          memory.learning.successScore <= 0.25 &&
          memory.learning.conflictScore <= 0 &&
          memory.penalty <= 0.15 &&
          !memory.learning.lastFeedbackAt &&
          !memory.metadata.extra.archived
      )
      .sort((left, right) => left.createdAt.localeCompare(right.createdAt))
      .slice(0, limit);

    for (const memory of memories) {
      memory.metadata.extra.archived = "true";
      memory.metadata.extra.archive_reason = "low_value";
      memory.updatedAt = nowIso();
      memory.version += 1;
      this.updateMemory(memory);
    }
    if (!this.db) {
      this.flushState();
    }
    return memories;
  }

  private flushState(): void {
    if (this.db) return;
    writeFileSync(this.stateFile, JSON.stringify(this.state, null, 2));
  }
}

function rankMemories(memories: MemoryRecord[], score: (memory: MemoryRecord) => number): RankedMemoryStat[] {
  return [...memories]
    .sort((left, right) => score(right) - score(left) || right.accessCount - left.accessCount)
    .slice(0, 12)
    .map((memory) => ({
      memoryId: memory.id,
      headline: memory.headline,
      summary: memory.summary,
      score: score(memory),
      accessCount: memory.accessCount,
      successScore: memory.learning.successScore,
      failureCount: memory.learning.failureCount
    }));
}

function bucketMemories(memories: MemoryRecord[]): AnalyticsBucket[] {
  const byDate = new Map<string, AnalyticsBucket>();
  for (const memory of memories) {
    const bucket = memory.createdAt.slice(0, 10);
    const current = byDate.get(bucket) ?? {
      bucket,
      memories: 0,
      conflicts: 0,
      recalls: 0,
      tokensSaved: 0
    };
    current.memories += 1;
    byDate.set(bucket, current);
  }
  return [...byDate.values()].sort((left, right) => left.bucket.localeCompare(right.bucket));
}

function loadConflictHeatmap(db: any, projectId: string): AnalyticsBucket[] {
  const rows = db
    .prepare(
      `SELECT substr(created_at, 1, 10) AS bucket,
              COUNT(*) AS conflicts,
              SUM(CASE WHEN repeated_mistake = 1 THEN 1 ELSE 0 END) AS repeated_conflicts
       FROM feedback_events
       WHERE project_id = ?
         AND outcome IN ('failure', 'regression')
       GROUP BY bucket
       ORDER BY bucket ASC`
    )
    .all(projectId) as { bucket: string; conflicts: number; repeated_conflicts: number }[];

  return rows.map((row) => ({
    bucket: row.bucket,
    memories: 0,
    conflicts: Number(row.conflicts),
    recalls: Number(row.repeated_conflicts),
    tokensSaved: 0
  }));
}

function loadEvolutionTrend(db: any, projectId: string): AnalyticsBucket[] {
  const rows = db
    .prepare(
      `SELECT substr(created_at, 1, 10) AS bucket,
              SUM(CASE WHEN outcome IN ('success', 'partial') THEN 1 ELSE 0 END) AS successes,
              SUM(CASE WHEN outcome IN ('failure', 'regression') THEN 1 ELSE 0 END) AS failures
       FROM feedback_events
       WHERE project_id = ?
       GROUP BY bucket
       ORDER BY bucket ASC`
    )
    .all(projectId) as { bucket: string; successes: number; failures: number }[];

  return rows.map((row) => ({
    bucket: row.bucket,
    memories: Number(row.successes),
    conflicts: Number(row.failures),
    recalls: 0,
    tokensSaved: 0
  }));
}

function loadConflictHeatmapFromState(state: JsonState, projectId: string): AnalyticsBucket[] {
  const byDay = new Map<string, AnalyticsBucket>();
  for (const event of state.feedbackEvents) {
    if (event.projectId !== projectId || !["failure", "regression"].includes(event.outcome)) continue;
    const bucket = event.createdAt.slice(0, 10);
    const current = byDay.get(bucket) ?? {
      bucket,
      memories: 0,
      conflicts: 0,
      recalls: 0,
      tokensSaved: 0
    };
    current.conflicts += 1;
    if (event.repeatedMistake) current.recalls += 1;
    byDay.set(bucket, current);
  }
  return [...byDay.values()].sort((left, right) => left.bucket.localeCompare(right.bucket));
}

function loadEvolutionTrendFromState(state: JsonState, projectId: string): AnalyticsBucket[] {
  const byDay = new Map<string, AnalyticsBucket>();
  for (const event of state.feedbackEvents) {
    if (event.projectId !== projectId) continue;
    const bucket = event.createdAt.slice(0, 10);
    const current = byDay.get(bucket) ?? {
      bucket,
      memories: 0,
      conflicts: 0,
      recalls: 0,
      tokensSaved: 0
    };
    if (event.outcome === "success" || event.outcome === "partial") {
      current.memories += 1;
    } else {
      current.conflicts += 1;
    }
    byDay.set(bucket, current);
  }
  return [...byDay.values()].sort((left, right) => left.bucket.localeCompare(right.bucket));
}

function nowIso(): string {
  return new Date().toISOString();
}

function ensureProjectRow(db: any, projectId: string, shareScope: boolean): void {
  const now = nowIso();
  db.prepare(
    `INSERT INTO projects(project_id, created_at, updated_at, share_scope, total_token_savings, total_queries)
     VALUES (?, ?, ?, ?, 0, 0)
     ON CONFLICT(project_id) DO UPDATE SET updated_at = excluded.updated_at, share_scope = excluded.share_scope`
  ).run(projectId, now, now, shareScope ? 1 : 0);
}

function createDatabase(databasePath: string): any | null {
  try {
    const require = createRequire(import.meta.url);
    const sqliteModuleName = `node:${["sql", "ite"].join("")}`;
    const sqlite = require(sqliteModuleName) as { DatabaseSync: new (path: string) => any };
    return new sqlite.DatabaseSync(databasePath);
  } catch {
    return null;
  }
}

function createEmptyState(): JsonState {
  return {
    projects: {},
    memories: [],
    memoryVersions: {},
    recollections: [],
    feedbackEvents: []
  };
}

function loadJsonState(stateFile: string): JsonState {
  if (!existsSync(stateFile)) {
    return createEmptyState();
  }
  try {
    return JSON.parse(readFileSync(stateFile, "utf8")) as JsonState;
  } catch {
    return createEmptyState();
  }
}

function ensureMemoryJsonColumn(db: any): void {
  const columns = db.prepare(`PRAGMA table_info(memories)`).all() as { name: string }[];
  if (!columns.some((column) => column.name === "record_json")) {
    db.exec(`ALTER TABLE memories ADD COLUMN record_json TEXT`);
  }
}

function hydrateRecord(db: any, row: Record<string, unknown>): MemoryRecord | undefined {
  if (typeof row.record_json === "string" && row.record_json) {
    return JSON.parse(row.record_json) as MemoryRecord;
  }
  if (typeof row.memory_id !== "string" || typeof row.project_id !== "string") {
    return undefined;
  }
  const rawTaxonomy = parseJson<Record<string, unknown>>(row.taxonomy_json, {});
  const taxonomy = normalizeTaxonomy(rawTaxonomy, {
    mainCategory: "memory",
    confidence: 0.2,
    multiLabels: [],
    hierarchy: [],
    dimensions: [],
    signals: [],
    relations: [],
    avoidPatterns: [],
    reinforcePatterns: [],
    metadata: {
      headline: String(row.headline ?? "Memory"),
      summary: String(row.summary ?? ""),
      languageHint: "mixed",
      classifierBackend: "algorithm",
      classifierNotes: [],
      modelConfidence: 0,
      tokenCount: 0,
      signalCount: 0,
      sentenceCount: 1,
      lineCount: 1,
      relationCount: 0,
      artifactDensity: 0,
      confidenceMean: 0.2,
      sensitivityTags: [],
      emergentClusters: [],
      entities: [],
      redactions: [],
      taxonomyVersion: "legacy",
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
  });
  const rawMetadata = parseJson<Record<string, unknown>>(row.metadata_json, {});
  const metadata = normalizeMetadata(rawMetadata, {
    tags: [],
    paths: [],
    links: [],
    privacy: "standard",
    shareScope: false,
    extra: {}
  });
  const kind = parseJson(row.kind, "observation") as MemoryRecord["kind"];
  const memory: MemoryRecord = {
    id: String(row.memory_id),
    projectId: String(row.project_id),
    kind,
    headline: String(row.headline ?? "Memory"),
    summary: String(row.summary ?? ""),
    content: String(row.content ?? ""),
    contentHash: String(row.content_hash ?? ""),
    taxonomy,
    metadata,
    createdAt: String(row.created_at ?? nowIso()),
    updatedAt: String(row.updated_at ?? nowIso()),
    lastAccessedAt: String(row.last_accessed_at ?? row.updated_at ?? row.created_at ?? nowIso()),
    reinforcement: Number(row.reinforcement ?? 0),
    penalty: Number(row.penalty ?? 0),
    learning: {
      successScore: Number(row.success_score ?? 0),
      failureCount: Number(row.failure_count ?? 0),
      repeatedMistakeCount: Number(row.repeated_mistake_count ?? 0),
      reinforcementDecay: Number(row.reinforcement_decay ?? 1),
      conflictScore: Number(row.conflict_score ?? 0),
      lastFeedbackAt: typeof row.last_feedback_at === "string" ? row.last_feedback_at : undefined
    },
    accessCount: Number(row.access_count ?? 0),
    version: Number(row.version ?? 1)
  };
  db.prepare(`UPDATE memories SET record_json = ? WHERE memory_id = ?`).run(
    JSON.stringify(memory),
    memory.id
  );
  return memory;
}

function parseJson<T>(value: unknown, fallback: T): T {
  if (typeof value !== "string" || !value) {
    return fallback;
  }
  try {
    return JSON.parse(value) as T;
  } catch {
    return fallback;
  }
}

function normalizeTaxonomy(input: Record<string, unknown>, fallback: MemoryRecord["taxonomy"]): MemoryRecord["taxonomy"] {
  const metadataInput = (input.metadata as Record<string, unknown> | undefined) ?? {};
  const debugInput = (input.debug as Record<string, unknown> | undefined) ?? {};
  return {
    ...fallback,
    mainCategory: String(input.mainCategory ?? input.main_category ?? fallback.mainCategory),
    confidence: Number(input.confidence ?? fallback.confidence),
    multiLabels: asStringArray(input.multiLabels ?? input.multi_labels),
    hierarchy: asArray(input.hierarchy).map((item) => {
      const value = item as Record<string, unknown>;
      return {
        id: String(value.id ?? ""),
        name: String(value.name ?? ""),
        level: Number(value.level ?? 0),
        description: String(value.description ?? ""),
        priority: Number(value.priority ?? 0),
        confidence: Number(value.confidence ?? 0),
        reinforcementWeight: Number(value.reinforcementWeight ?? value.reinforcement_weight ?? 0),
        failureCount: Number(value.failureCount ?? value.failure_count ?? 0),
        reinforcementDecay: Number(value.reinforcementDecay ?? value.reinforcement_decay ?? 1),
        dependencies: asStringArray(value.dependencies),
        relations: asStringArray(value.relations),
        nodeType: String(value.nodeType ?? value.node_type ?? "")
      };
    }),
    dimensions: asArray(input.dimensions).map((item) => {
      const value = item as Record<string, unknown>;
      return {
        dimension: String(value.dimension ?? ""),
        dominantLabel: String(value.dominantLabel ?? value.dominant_label ?? ""),
        labels: asStringArray(value.labels),
        confidence: Number(value.confidence ?? 0)
      };
    }),
    signals: asArray(input.signals).map((item) => {
      const value = item as Record<string, unknown>;
      return {
        label: String(value.label ?? ""),
        dimension: String(value.dimension ?? ""),
        score: Number(value.score ?? 0),
        confidence: Number(value.confidence ?? 0),
        reinforcementWeight: Number(value.reinforcementWeight ?? value.reinforcement_weight ?? 0),
        failureCount: Number(value.failureCount ?? value.failure_count ?? 0),
        reinforcementDecay: Number(value.reinforcementDecay ?? value.reinforcement_decay ?? 1),
        reasons: asStringArray(value.reasons)
      };
    }),
    relations: asArray(input.relations).map((item) => {
      const value = item as Record<string, unknown>;
      return {
        source: String(value.source ?? ""),
        target: String(value.target ?? ""),
        relation: String(value.relation ?? ""),
        weight: Number(value.weight ?? 0),
        evidence: String(value.evidence ?? "")
      };
    }),
    avoidPatterns: asStringArray(input.avoidPatterns ?? input.avoid_patterns),
    reinforcePatterns: asStringArray(input.reinforcePatterns ?? input.reinforce_patterns),
    metadata: {
      ...fallback.metadata,
      headline: String(metadataInput.headline ?? fallback.metadata.headline),
      summary: String(metadataInput.summary ?? fallback.metadata.summary),
      languageHint: String(metadataInput.languageHint ?? metadataInput.language_hint ?? fallback.metadata.languageHint),
      classifierBackend: String(metadataInput.classifierBackend ?? metadataInput.classifier_backend ?? fallback.metadata.classifierBackend),
      classifierNotes: asStringArray(metadataInput.classifierNotes ?? metadataInput.classifier_notes),
      modelConfidence: Number(metadataInput.modelConfidence ?? metadataInput.model_confidence ?? 0),
      tokenCount: Number(metadataInput.tokenCount ?? metadataInput.token_count ?? 0),
      signalCount: Number(metadataInput.signalCount ?? metadataInput.signal_count ?? 0),
      sentenceCount: Number(metadataInput.sentenceCount ?? metadataInput.sentence_count ?? 0),
      lineCount: Number(metadataInput.lineCount ?? metadataInput.line_count ?? 0),
      relationCount: Number(metadataInput.relationCount ?? metadataInput.relation_count ?? 0),
      artifactDensity: Number(metadataInput.artifactDensity ?? metadataInput.artifact_density ?? 0),
      confidenceMean: Number(metadataInput.confidenceMean ?? metadataInput.confidence_mean ?? 0),
      sensitivityTags: asStringArray(metadataInput.sensitivityTags ?? metadataInput.sensitivity_tags),
      emergentClusters: asStringArray(metadataInput.emergentClusters ?? metadataInput.emergent_clusters),
      entities: asStringArray(metadataInput.entities),
      redactions: asArray(metadataInput.redactions) as never,
      taxonomyVersion: String(metadataInput.taxonomyVersion ?? metadataInput.taxonomy_version ?? fallback.metadata.taxonomyVersion),
      compressionHint: String(metadataInput.compressionHint ?? metadataInput.compression_hint ?? fallback.metadata.compressionHint),
      inferredKinds: asStringArray(metadataInput.inferredKinds ?? metadataInput.inferred_kinds)
    },
    debug: {
      matchedAliases: asStringArray(debugInput.matchedAliases ?? debugInput.matched_aliases),
      prototypeHits: asStringArray(debugInput.prototypeHits ?? debugInput.prototype_hits),
      pathHints: asStringArray(debugInput.pathHints ?? debugInput.path_hints),
      contextHints: asStringArray(debugInput.contextHints ?? debugInput.context_hints),
      derivedMarkers: asStringArray(debugInput.derivedMarkers ?? debugInput.derived_markers)
    }
  };
}

function normalizeMetadata(input: Record<string, unknown>, fallback: MemoryRecord["metadata"]): MemoryRecord["metadata"] {
  return {
    ...fallback,
    tags: asStringArray(input.tags),
    paths: asStringArray(input.paths),
    links: asStringArray(input.links),
    source: typeof input.source === "string" ? input.source : undefined,
    actor: typeof input.actor === "string" ? input.actor : undefined,
    language: typeof input.language === "string" ? input.language : undefined,
    privacy: (typeof input.privacy === "string" ? input.privacy : fallback.privacy) as MemoryRecord["metadata"]["privacy"],
    shareScope: Boolean(input.shareScope ?? input.share_scope),
    extra: typeof input.extra === "object" && input.extra ? (input.extra as Record<string, string>) : {}
  };
}

function asStringArray(value: unknown): string[] {
  return Array.isArray(value) ? value.map(String) : [];
}

function asArray(value: unknown): unknown[] {
  return Array.isArray(value) ? value : [];
}
