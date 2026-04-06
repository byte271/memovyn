export type ProjectId = string;
export type MemoryId = string;

export const privacyLevels = ["standard", "internal", "confidential", "secret"] as const;
export type PrivacyLevel = (typeof privacyLevels)[number];

export const memoryKinds = [
  "observation",
  "decision",
  "issue",
  "outcome",
  "note",
  "reflection",
  "context"
] as const;
export type MemoryKind = (typeof memoryKinds)[number];

export const feedbackOutcomes = ["success", "failure", "regression", "partial"] as const;
export type FeedbackOutcome = (typeof feedbackOutcomes)[number];

export type MemoryMetadata = {
  tags: string[];
  paths: string[];
  links: string[];
  source?: string;
  actor?: string;
  language?: string;
  privacy: PrivacyLevel;
  shareScope: boolean;
  extra: Record<string, string>;
};

export type SensitiveSpan = {
  start: number;
  end: number;
  label: string;
};

export type TaxonomySignal = {
  label: string;
  dimension: string;
  score: number;
  confidence: number;
  reinforcementWeight: number;
  failureCount: number;
  reinforcementDecay: number;
  reasons: string[];
};

export type TaxonomyRelation = {
  source: string;
  target: string;
  relation: string;
  weight: number;
  evidence: string;
};

export type DimensionBreakdown = {
  dimension: string;
  dominantLabel: string;
  labels: string[];
  confidence: number;
};

export type TaxonomyDebugView = {
  matchedAliases: string[];
  prototypeHits: string[];
  pathHints: string[];
  contextHints: string[];
  derivedMarkers: string[];
};

export type TaxonomyMetadata = {
  headline: string;
  summary: string;
  languageHint: string;
  classifierBackend: string;
  classifierNotes: string[];
  modelConfidence: number;
  tokenCount: number;
  signalCount: number;
  sentenceCount: number;
  lineCount: number;
  relationCount: number;
  artifactDensity: number;
  confidenceMean: number;
  sensitivityTags: string[];
  emergentClusters: string[];
  entities: string[];
  redactions: SensitiveSpan[];
  taxonomyVersion: string;
  compressionHint: string;
  inferredKinds: string[];
};

export type HierarchyNode = {
  id: string;
  name: string;
  level: number;
  description: string;
  priority: number;
  confidence: number;
  reinforcementWeight: number;
  failureCount: number;
  reinforcementDecay: number;
  dependencies: string[];
  relations: string[];
  nodeType: string;
};

export type TaxonomyDecomposition = {
  mainCategory: string;
  confidence: number;
  multiLabels: string[];
  hierarchy: HierarchyNode[];
  dimensions: DimensionBreakdown[];
  signals: TaxonomySignal[];
  relations: TaxonomyRelation[];
  avoidPatterns: string[];
  reinforcePatterns: string[];
  metadata: TaxonomyMetadata;
  debug: TaxonomyDebugView;
};

export type LearningState = {
  successScore: number;
  failureCount: number;
  repeatedMistakeCount: number;
  reinforcementDecay: number;
  conflictScore: number;
  lastFeedbackAt?: string;
};

export type MemoryRecord = {
  id: MemoryId;
  projectId: ProjectId;
  kind: MemoryKind;
  headline: string;
  summary: string;
  content: string;
  contentHash: string;
  taxonomy: TaxonomyDecomposition;
  metadata: MemoryMetadata;
  createdAt: string;
  updatedAt: string;
  lastAccessedAt: string;
  reinforcement: number;
  penalty: number;
  learning: LearningState;
  accessCount: number;
  version: number;
};

export type SearchFilters = {
  labels: string[];
  kinds: MemoryKind[];
  since?: string;
  until?: string;
  includePrivateNotes: boolean;
  includeShared: boolean;
  includeArchived: boolean;
};

export type SearchHit = {
  memoryId: MemoryId;
  score: number;
  headline: string;
  summary: string;
  content: string;
  labels: string[];
  mainCategory: string;
  confidence: number;
  relationCount: number;
  explanation: string;
  createdAt: string;
  reinforcement: number;
  penalty: number;
};

export type ProgressiveIndexCard = {
  memoryId: MemoryId;
  headline: string;
  summary: string;
  labels: string[];
  score: number;
};

export type ProgressiveSummary = {
  memoryId: MemoryId;
  mainCategory: string;
  confidence: number;
  explanation: string;
  relations: string[];
};

export type TimelineEntry = {
  memoryId: MemoryId;
  timestamp: string;
  headline: string;
  changeSignal: string;
};

export type SearchResponse = {
  projectId: ProjectId;
  query: string;
  totalHits: number;
  indexLayer: ProgressiveIndexCard[];
  summaryLayer: ProgressiveSummary[];
  timelineLayer: TimelineEntry[];
  detailLayer: SearchHit[];
};

export type TaxonomySummary = {
  topLabels: [string, number][];
  topRelations: [string, number][];
  dominantDimensions: [string, string][];
  avoidPatterns: string[];
  privacySignals: string[];
  activeConflicts: string[];
};

export type ProjectContext = {
  projectId: ProjectId;
  readyContext: string;
  taxonomySummary: TaxonomySummary;
  recentTimeline: TimelineEntry[];
  topMemories: ProgressiveIndexCard[];
  sharedRecall: ProgressiveSummary[];
  debuggingNotes: string[];
};

export type InteractiveAction = {
  id: string;
  label: string;
};

export type InteractivePrompt = {
  title: string;
  body: string;
  actions: InteractiveAction[];
};

export type ReflectionResponse = {
  memory: MemoryRecord;
  repeatedMistakeDetected: boolean;
  conflictDetected: boolean;
  avoidPatterns: string[];
  interactivePrompt: InteractivePrompt;
};

export type RankedMemoryStat = {
  memoryId: MemoryId;
  headline: string;
  summary: string;
  score: number;
  accessCount: number;
  successScore: number;
  failureCount: number;
};

export type AnalyticsBucket = {
  bucket: string;
  memories: number;
  conflicts: number;
  recalls: number;
  tokensSaved: number;
};

export type AnalyticsSnapshot = {
  projectId: ProjectId;
  totalMemories: number;
  totalQueries: number;
  totalTokenSavings: number;
  estimatedTokensPerRecall: number;
  sessionQueries: number;
  sessionTokenSavings: number;
  conflictCount: number;
  reinforcedMemories: number;
  penalizedMemories: number;
  memoryHealthScore: number;
  learningImpactScore: number;
  mostRecalled: RankedMemoryStat[];
  mostReinforced: RankedMemoryStat[];
  mostPunished: RankedMemoryStat[];
  mostImpactful: RankedMemoryStat[];
  labelHotspots: [string, number][];
  relationHotspots: [string, number][];
  conflictHeatmap: AnalyticsBucket[];
  growth: AnalyticsBucket[];
  evolutionTrend: AnalyticsBucket[];
  agentEvolutionTimeline: AnalyticsBucket[];
  behaviorInsights: string[];
  proactiveSuggestions: string[];
};

export type MemoryVersionSnapshot = {
  version: number;
  createdAt: string;
  headline: string;
  reinforcement: number;
  penalty: number;
};

export type MemoryInspection = {
  memory: MemoryRecord;
  versions: MemoryVersionSnapshot[];
  explanation: string[];
  provenance: string[];
};

export type FeedbackResponse = {
  memory: MemoryRecord;
  conflictDetected: boolean;
  avoidPatterns: string[];
  influencedMemories: MemoryId[];
  learningDelta: number;
  reconciliationHints: string[];
};

export type AddMemoryInput = {
  projectId: ProjectId;
  content: string;
  metadata?: Partial<MemoryMetadata>;
  kind?: MemoryKind;
};

export type SearchInput = {
  projectId: ProjectId;
  query: string;
  limit?: number;
  filters?: Partial<SearchFilters>;
};

export type ReflectionInput = {
  projectId: ProjectId;
  taskResult: string;
  outcome: FeedbackOutcome;
  metadata?: Partial<MemoryMetadata>;
};

export type FeedbackInput = {
  memoryId: MemoryId;
  outcome: FeedbackOutcome;
  repeatedMistake?: boolean;
  weight?: number;
  crossProjectInfluence?: boolean;
  avoidPatterns?: string[];
  note?: string;
};

export type ArchiveInput = {
  memoryId: MemoryId;
};

export type TaxonomyEvolutionSnapshot = {
  priorLabels: string[];
  reinforcedLabels: string[];
  solidifiedPriors: string[];
  avoidPatterns: string[];
  projectTerms: string[];
};

export function defaultMetadata(): MemoryMetadata {
  return {
    tags: [],
    paths: [],
    links: [],
    privacy: "standard",
    shareScope: false,
    extra: {}
  };
}

export function defaultSearchFilters(): SearchFilters {
  return {
    labels: [],
    kinds: [],
    includePrivateNotes: false,
    includeShared: false,
    includeArchived: false
  };
}

export function defaultLearningState(): LearningState {
  return {
    successScore: 0,
    failureCount: 0,
    repeatedMistakeCount: 0,
    reinforcementDecay: 1,
    conflictScore: 0
  };
}

export function defaultEvolutionSnapshot(): TaxonomyEvolutionSnapshot {
  return {
    priorLabels: [],
    reinforcedLabels: [],
    solidifiedPriors: [],
    avoidPatterns: [],
    projectTerms: []
  };
}
