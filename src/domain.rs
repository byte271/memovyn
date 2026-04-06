use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use smallvec::SmallVec;
use time::OffsetDateTime;
use uuid::Uuid;

pub type ProjectId = String;
pub type MemoryId = Uuid;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum PrivacyLevel {
    #[default]
    Standard,
    Internal,
    Confidential,
    Secret,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum MemoryKind {
    #[default]
    Observation,
    Decision,
    Issue,
    Outcome,
    Note,
    Reflection,
    Context,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FeedbackOutcome {
    Success,
    Failure,
    Regression,
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LearningState {
    #[serde(default)]
    pub success_score: f32,
    #[serde(default)]
    pub failure_count: u32,
    #[serde(default)]
    pub repeated_mistake_count: u32,
    #[serde(default = "default_reinforcement_decay")]
    pub reinforcement_decay: f32,
    #[serde(default)]
    pub conflict_score: f32,
    #[serde(default)]
    pub last_feedback_at: Option<OffsetDateTime>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MemoryMetadata {
    #[serde(default)]
    pub tags: SmallVec<[String; 8]>,
    #[serde(default)]
    pub paths: SmallVec<[String; 8]>,
    #[serde(default)]
    pub links: SmallVec<[String; 4]>,
    pub source: Option<String>,
    pub actor: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub privacy: PrivacyLevel,
    #[serde(default)]
    pub share_scope: bool,
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SensitiveSpan {
    pub start: usize,
    pub end: usize,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomySignal {
    pub label: String,
    pub dimension: String,
    pub score: f32,
    pub confidence: f32,
    #[serde(default)]
    pub reinforcement_weight: f32,
    #[serde(default)]
    pub failure_count: u32,
    #[serde(default = "default_reinforcement_decay")]
    pub reinforcement_decay: f32,
    pub reasons: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyRelation {
    pub source: String,
    pub target: String,
    pub relation: String,
    pub weight: f32,
    pub evidence: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DimensionBreakdown {
    pub dimension: String,
    pub dominant_label: String,
    pub labels: Vec<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyDebugView {
    pub matched_aliases: Vec<String>,
    pub prototype_hits: Vec<String>,
    pub path_hints: Vec<String>,
    pub context_hints: Vec<String>,
    pub derived_markers: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyMetadata {
    pub headline: String,
    pub summary: String,
    pub language_hint: String,
    #[serde(default)]
    pub classifier_backend: String,
    #[serde(default)]
    pub classifier_notes: Vec<String>,
    #[serde(default)]
    pub model_confidence: f32,
    pub token_count: usize,
    pub signal_count: usize,
    #[serde(default)]
    pub sentence_count: usize,
    #[serde(default)]
    pub line_count: usize,
    #[serde(default)]
    pub relation_count: usize,
    #[serde(default)]
    pub artifact_density: f32,
    #[serde(default)]
    pub confidence_mean: f32,
    #[serde(default)]
    pub sensitivity_tags: Vec<String>,
    #[serde(default)]
    pub emergent_clusters: Vec<String>,
    #[serde(default)]
    pub entities: Vec<String>,
    #[serde(default)]
    pub redactions: Vec<SensitiveSpan>,
    #[serde(default = "default_taxonomy_version")]
    pub taxonomy_version: String,
    #[serde(default)]
    pub compression_hint: String,
    #[serde(default)]
    pub inferred_kinds: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct HierarchyNode {
    pub id: String,
    pub name: String,
    pub level: u8,
    pub description: String,
    pub priority: u8,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub reinforcement_weight: f32,
    #[serde(default)]
    pub failure_count: u32,
    #[serde(default = "default_reinforcement_decay")]
    pub reinforcement_decay: f32,
    #[serde(default)]
    pub dependencies: Vec<String>,
    #[serde(default)]
    pub relations: Vec<String>,
    #[serde(rename = "type")]
    pub node_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaxonomyDecomposition {
    pub main_category: String,
    #[serde(default)]
    pub confidence: f32,
    #[serde(default)]
    pub multi_labels: Vec<String>,
    #[serde(default)]
    pub hierarchy: Vec<HierarchyNode>,
    #[serde(default)]
    pub dimensions: Vec<DimensionBreakdown>,
    #[serde(default)]
    pub signals: Vec<TaxonomySignal>,
    #[serde(default)]
    pub relations: Vec<TaxonomyRelation>,
    #[serde(default)]
    pub avoid_patterns: Vec<String>,
    #[serde(default)]
    pub reinforce_patterns: Vec<String>,
    #[serde(default)]
    pub metadata: TaxonomyMetadata,
    #[serde(default)]
    pub debug: TaxonomyDebugView,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryRecord {
    pub id: MemoryId,
    pub project_id: ProjectId,
    pub kind: MemoryKind,
    pub headline: String,
    pub summary: String,
    pub content: String,
    pub content_hash: String,
    pub taxonomy: TaxonomyDecomposition,
    pub metadata: MemoryMetadata,
    pub created_at: OffsetDateTime,
    pub updated_at: OffsetDateTime,
    pub last_accessed_at: OffsetDateTime,
    pub reinforcement: f32,
    pub penalty: f32,
    #[serde(default)]
    pub learning: LearningState,
    pub access_count: u64,
    pub version: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddMemoryRequest {
    pub project_id: ProjectId,
    pub content: String,
    #[serde(default)]
    pub metadata: MemoryMetadata,
    #[serde(default)]
    pub kind: MemoryKind,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SearchFilters {
    #[serde(default)]
    pub labels: Vec<String>,
    #[serde(default)]
    pub kinds: Vec<MemoryKind>,
    pub since: Option<OffsetDateTime>,
    pub until: Option<OffsetDateTime>,
    #[serde(default)]
    pub include_private_notes: bool,
    #[serde(default)]
    pub include_shared: bool,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchRequest {
    pub project_id: ProjectId,
    pub query: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub filters: SearchFilters,
}

fn default_limit() -> usize {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressiveIndexCard {
    pub memory_id: MemoryId,
    pub headline: String,
    pub summary: String,
    pub labels: Vec<String>,
    pub score: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProgressiveSummary {
    pub memory_id: MemoryId,
    pub main_category: String,
    pub confidence: f32,
    pub explanation: String,
    pub relations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimelineEntry {
    pub memory_id: MemoryId,
    pub timestamp: OffsetDateTime,
    pub headline: String,
    pub change_signal: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchHit {
    pub memory_id: MemoryId,
    pub score: f32,
    pub headline: String,
    pub summary: String,
    pub content: String,
    pub labels: Vec<String>,
    pub main_category: String,
    pub confidence: f32,
    pub relation_count: usize,
    pub explanation: String,
    pub created_at: OffsetDateTime,
    pub reinforcement: f32,
    pub penalty: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchResponse {
    pub project_id: ProjectId,
    pub query: String,
    pub total_hits: usize,
    pub index_layer: Vec<ProgressiveIndexCard>,
    pub summary_layer: Vec<ProgressiveSummary>,
    pub timeline_layer: Vec<TimelineEntry>,
    pub detail_layer: Vec<SearchHit>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaxonomySummary {
    pub top_labels: Vec<(String, usize)>,
    pub top_relations: Vec<(String, usize)>,
    pub dominant_dimensions: Vec<(String, String)>,
    pub avoid_patterns: Vec<String>,
    pub privacy_signals: Vec<String>,
    pub active_conflicts: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectContext {
    pub project_id: ProjectId,
    pub ready_context: String,
    pub taxonomy_summary: TaxonomySummary,
    pub recent_timeline: Vec<TimelineEntry>,
    pub top_memories: Vec<ProgressiveIndexCard>,
    pub shared_recall: Vec<ProgressiveSummary>,
    pub debugging_notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionRequest {
    pub project_id: ProjectId,
    pub task_result: String,
    pub outcome: FeedbackOutcome,
    #[serde(default)]
    pub metadata: MemoryMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectionResponse {
    pub memory: MemoryRecord,
    pub repeated_mistake_detected: bool,
    pub conflict_detected: bool,
    pub avoid_patterns: Vec<String>,
    pub interactive_prompt: InteractivePrompt,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackRequest {
    pub memory_id: MemoryId,
    pub outcome: FeedbackOutcome,
    #[serde(default)]
    pub repeated_mistake: bool,
    #[serde(default = "default_feedback_weight")]
    pub weight: f32,
    #[serde(default = "default_cross_project_influence")]
    pub cross_project_influence: bool,
    #[serde(default)]
    pub avoid_patterns: Vec<String>,
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeedbackResponse {
    pub memory: MemoryRecord,
    pub conflict_detected: bool,
    pub avoid_patterns: Vec<String>,
    #[serde(default)]
    pub influenced_memories: Vec<MemoryId>,
    #[serde(default)]
    pub learning_delta: f32,
    #[serde(default)]
    pub reconciliation_hints: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractiveAction {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InteractivePrompt {
    pub title: String,
    pub body: String,
    pub actions: Vec<InteractiveAction>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectCard {
    pub project_id: ProjectId,
    pub memory_count: usize,
    pub last_updated_at: Option<OffsetDateTime>,
    pub share_scope: bool,
    pub total_token_savings: u64,
    pub conflict_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsSnapshot {
    pub project_id: ProjectId,
    pub total_memories: usize,
    pub total_queries: u64,
    pub total_token_savings: u64,
    pub estimated_tokens_per_recall: u64,
    pub session_queries: u64,
    pub session_token_savings: u64,
    pub conflict_count: usize,
    pub reinforced_memories: usize,
    pub penalized_memories: usize,
    pub memory_health_score: u8,
    pub learning_impact_score: u8,
    pub most_recalled: Vec<RankedMemoryStat>,
    pub most_reinforced: Vec<RankedMemoryStat>,
    pub most_punished: Vec<RankedMemoryStat>,
    pub most_impactful: Vec<RankedMemoryStat>,
    pub label_hotspots: Vec<(String, usize)>,
    pub relation_hotspots: Vec<(String, usize)>,
    pub conflict_heatmap: Vec<AnalyticsBucket>,
    pub growth: Vec<AnalyticsBucket>,
    pub evolution_trend: Vec<AnalyticsBucket>,
    pub agent_evolution_timeline: Vec<AnalyticsBucket>,
    #[serde(default)]
    pub behavior_insights: Vec<String>,
    #[serde(default)]
    pub proactive_suggestions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RankedMemoryStat {
    pub memory_id: MemoryId,
    pub headline: String,
    pub summary: String,
    pub score: f32,
    pub access_count: u64,
    pub success_score: f32,
    pub failure_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalyticsBucket {
    pub bucket: String,
    pub memories: usize,
    pub conflicts: usize,
    pub recalls: usize,
    pub tokens_saved: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportBundle {
    pub exported_at: OffsetDateTime,
    pub memories: Vec<MemoryRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryVersionSnapshot {
    pub version: u32,
    pub created_at: OffsetDateTime,
    pub headline: String,
    pub reinforcement: f32,
    pub penalty: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryInspection {
    pub memory: MemoryRecord,
    pub versions: Vec<MemoryVersionSnapshot>,
    pub explanation: Vec<String>,
    #[serde(default)]
    pub provenance: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveRequest {
    pub memory_id: MemoryId,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArchiveResponse {
    pub memory: MemoryRecord,
}

fn default_taxonomy_version() -> String {
    "legacy".to_string()
}

fn default_reinforcement_decay() -> f32 {
    1.0
}

fn default_feedback_weight() -> f32 {
    1.0
}

fn default_cross_project_influence() -> bool {
    true
}
