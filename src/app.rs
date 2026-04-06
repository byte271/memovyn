use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use crate::config::Config;
use crate::domain::{
    AddMemoryRequest, AnalyticsSnapshot, ArchiveRequest, ArchiveResponse, ExportBundle,
    FeedbackOutcome, FeedbackRequest, FeedbackResponse, InteractiveAction, InteractivePrompt,
    MemoryInspection, MemoryKind, MemoryRecord, ProjectCard, ProjectContext, ReflectionRequest,
    ReflectionResponse, SearchFilters, SearchRequest, SearchResponse, TaxonomySummary,
};
use crate::error::Result;
use crate::search::SearchIndex;
use crate::storage::Database;
use crate::taxonomy::TaxonomyEngine;
use time::OffsetDateTime;
use uuid::Uuid;

#[derive(Debug)]
pub struct Memovyn {
    pub config: Config,
    database: Database,
    taxonomy: TaxonomyEngine,
    search: SearchIndex,
    session: SessionStats,
}

#[derive(Debug, Default)]
struct SessionStats {
    queries: AtomicU64,
    token_savings: AtomicU64,
}

impl Memovyn {
    pub fn open(config: Config) -> Result<Self> {
        config.ensure()?;
        let database = Database::open(config.database_path())?;
        let records = database.load_all_memories()?;
        let search = SearchIndex::new(records);
        Ok(Self {
            config,
            database,
            taxonomy: TaxonomyEngine::new(),
            search,
            session: SessionStats::default(),
        })
    }

    pub fn add_memory(&self, request: AddMemoryRequest) -> Result<MemoryRecord> {
        if !self.search.has_project(&request.project_id) {
            self.database
                .upsert_project(&request.project_id, request.metadata.share_scope)?;
        }

        let now = OffsetDateTime::now_utc();
        let evolution = self.search.taxonomy_feedback(&request.project_id);
        let (content, taxonomy) =
            self.taxonomy
                .decompose_with_context(&request.content, &request.metadata, &evolution);
        let record = MemoryRecord {
            id: Uuid::now_v7(),
            project_id: request.project_id,
            kind: request.kind,
            headline: taxonomy.metadata.headline.clone(),
            summary: taxonomy.metadata.summary.clone(),
            content_hash: blake3::hash(content.as_bytes()).to_hex().to_string(),
            content,
            taxonomy,
            metadata: request.metadata,
            created_at: now,
            updated_at: now,
            last_accessed_at: now,
            reinforcement: 0.0,
            penalty: 0.0,
            learning: Default::default(),
            access_count: 0,
            version: 1,
        };

        self.database.insert_memory(&record)?;
        self.search.insert(record.clone());
        Ok(record)
    }

    pub fn reflect_memory(&self, request: ReflectionRequest) -> Result<ReflectionResponse> {
        let shared_projects = self.database.list_shared_projects(&request.project_id)?;
        let prior_hits = self.search.search(
            &request.project_id,
            &request.task_result,
            5,
            &SearchFilters {
                include_private_notes: true,
                ..Default::default()
            },
            &shared_projects,
        );

        let repeated_mistake_detected = matches!(
            request.outcome,
            FeedbackOutcome::Failure | FeedbackOutcome::Regression
        ) && prior_hits.detail_layer.iter().any(|hit| {
            hit.penalty > hit.reinforcement
                || hit.labels.iter().any(|label| label.contains("avoid"))
        });
        let conflict_detected = prior_hits.detail_layer.iter().any(|hit| {
            hit.penalty > hit.reinforcement
                || hit.labels.iter().any(|label| label.contains("regression"))
        });

        let mut metadata = request.metadata.clone();
        metadata.tags.push("auto-reflection".to_string());
        metadata.extra.insert(
            "feedback_outcome".to_string(),
            format!("{:?}", request.outcome).to_ascii_lowercase(),
        );
        if conflict_detected {
            metadata.tags.push("conflict".to_string());
        }
        if repeated_mistake_detected {
            metadata.tags.push("avoid_pattern".to_string());
            metadata
                .extra
                .insert("repeat_regression".to_string(), "true".to_string());
        }

        let memory = self.add_memory(AddMemoryRequest {
            project_id: request.project_id.clone(),
            content: request.task_result,
            metadata,
            kind: match request.outcome {
                FeedbackOutcome::Success | FeedbackOutcome::Partial => MemoryKind::Reflection,
                FeedbackOutcome::Failure | FeedbackOutcome::Regression => MemoryKind::Issue,
            },
        })?;

        let feedback = self.feedback_memory(FeedbackRequest {
            memory_id: memory.id,
            outcome: request.outcome,
            repeated_mistake: repeated_mistake_detected,
            weight: if repeated_mistake_detected { 1.25 } else { 1.0 },
            cross_project_influence: true,
            avoid_patterns: memory.taxonomy.avoid_patterns.clone(),
            note: Some("reflect_memory".to_string()),
        })?;

        Ok(ReflectionResponse {
            memory: feedback.memory,
            repeated_mistake_detected,
            conflict_detected: conflict_detected || feedback.conflict_detected,
            avoid_patterns: feedback.avoid_patterns,
            interactive_prompt: InteractivePrompt {
                title: "Save this full project description + complete taxonomy to Memovyn permanent memory?".to_string(),
                body: "Memovyn classified the result, updated reinforcement weights, inferred taxonomy relations, and prepared the project memory graph.".to_string(),
                actions: vec![
                    InteractiveAction {
                        id: "yes".to_string(),
                        label: "Yes".to_string(),
                    },
                    InteractiveAction {
                        id: "edit".to_string(),
                        label: "Edit".to_string(),
                    },
                    InteractiveAction {
                        id: "no".to_string(),
                        label: "No".to_string(),
                    },
                ],
            },
        })
    }

    pub fn feedback_memory(&self, request: FeedbackRequest) -> Result<FeedbackResponse> {
        let seed_memory = self
            .database
            .get_memory(request.memory_id)?
            .ok_or_else(|| {
                crate::error::MemovynError::NotFound(format!("memory {}", request.memory_id))
            })?;
        let (memory_count, query_count) =
            self.database.project_activity(&seed_memory.project_id)?;
        let activity_score =
            ((memory_count as f32 / 256.0) + (query_count as f32 / 2048.0)).clamp(0.0, 4.0);
        let mut avoid_patterns = request.avoid_patterns.clone();
        if request.repeated_mistake || seed_memory.learning.failure_count + 1 >= 2 {
            avoid_patterns.push(consolidated_avoid_pattern(&seed_memory));
        }
        if matches!(
            request.outcome,
            FeedbackOutcome::Failure | FeedbackOutcome::Regression
        ) {
            avoid_patterns.push(format!("avoid:{}", seed_memory.taxonomy.main_category));
        }
        avoid_patterns.sort_unstable();
        avoid_patterns.dedup();

        let memory = self
            .database
            .feedback_memory(
                request.memory_id,
                request.outcome,
                request.repeated_mistake,
                request.weight,
                activity_score,
                &avoid_patterns,
                request.note.as_deref(),
            )?
            .ok_or_else(|| {
                crate::error::MemovynError::NotFound(format!("memory {}", request.memory_id))
            })?;
        self.search.refresh(memory.clone());
        let conflict_detected =
            memory.learning.conflict_score > 0.0 || memory.penalty > memory.reinforcement;
        let influenced_memories =
            if request.cross_project_influence || seed_memory.metadata.share_scope {
                self.propagate_feedback_influence(&memory, request.outcome, request.weight * 0.32)?
            } else {
                Vec::new()
            };
        Ok(FeedbackResponse {
            avoid_patterns: memory.taxonomy.avoid_patterns.clone(),
            memory,
            conflict_detected,
            influenced_memories,
            learning_delta: request.weight * (1.0 + activity_score * 0.14),
        })
    }

    pub fn search_memories(&self, request: SearchRequest) -> Result<SearchResponse> {
        let shared_projects = self.database.list_shared_projects(&request.project_id)?;
        let response = self.search.search(
            &request.project_id,
            &request.query,
            request.limit.max(1),
            &request.filters,
            &shared_projects,
        );

        for hit in &response.detail_layer {
            let tokens_saved = hit.content.len().saturating_sub(hit.summary.len());
            let _ = self
                .database
                .record_recall(hit.memory_id, &request.query, tokens_saved);
            self.session.queries.fetch_add(1, Ordering::Relaxed);
            self.session
                .token_savings
                .fetch_add(tokens_saved as u64, Ordering::Relaxed);
        }

        Ok(response)
    }

    pub fn get_project_context(&self, project_id: &str) -> Result<ProjectContext> {
        let (
            top_labels,
            top_relations,
            dominant_dimensions,
            avoid_patterns,
            privacy_signals,
            active_conflicts,
        ) = self.search.project_summary(project_id);
        let top_memories = self.search.recent_cards(project_id, 8);
        let shared_recall = self.search.recent_summaries(project_id, 6);
        let recent_timeline = self.search.recent_timeline(project_id, 12);

        let ready_context = format!(
            "Project: {project_id}\nTop taxonomy labels: {}\nDominant dimensions: {}\nKey relations: {}\nAvoid patterns: {}\nRecent memory headlines: {}",
            top_labels
                .iter()
                .map(|(label, count)| format!("{label} ({count})"))
                .collect::<Vec<_>>()
                .join(", "),
            dominant_dimensions
                .iter()
                .map(|(dimension, label)| format!("{dimension}={label}"))
                .collect::<Vec<_>>()
                .join(", "),
            top_relations
                .iter()
                .map(|(relation, count)| format!("{relation} ({count})"))
                .collect::<Vec<_>>()
                .join(", "),
            if avoid_patterns.is_empty() {
                "none".to_string()
            } else {
                avoid_patterns.join(", ")
            },
            top_memories
                .iter()
                .map(|memory| memory.headline.clone())
                .collect::<Vec<_>>()
                .join(" | ")
        );

        Ok(ProjectContext {
            project_id: project_id.to_string(),
            ready_context,
            taxonomy_summary: TaxonomySummary {
                top_labels,
                top_relations,
                dominant_dimensions,
                avoid_patterns,
                privacy_signals,
                active_conflicts: active_conflicts.clone(),
            },
            recent_timeline,
            top_memories,
            shared_recall,
            debugging_notes: build_debugging_notes(&active_conflicts),
        })
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectCard>> {
        self.database.list_projects()
    }

    pub fn analytics(&self, project_id: &str) -> Result<AnalyticsSnapshot> {
        let mut analytics = self.database.analytics(project_id)?;
        let (label_hotspots, relation_hotspots, conflict_count) =
            self.search.project_analytics(project_id);
        analytics.label_hotspots = label_hotspots;
        analytics.relation_hotspots = relation_hotspots;
        analytics.conflict_count = analytics.conflict_count.max(conflict_count);
        analytics.session_queries = self.session.queries.load(Ordering::Relaxed);
        analytics.session_token_savings = self.session.token_savings.load(Ordering::Relaxed);
        analytics.behavior_insights = build_behavior_insights(&analytics);
        Ok(analytics)
    }

    pub fn inspect_memory(&self, memory_id: Uuid) -> Result<Option<MemoryInspection>> {
        let memory = self.database.get_memory(memory_id)?;
        let Some(memory) = memory else {
            return Ok(None);
        };
        let versions = self.database.memory_versions(memory_id)?;
        let explanation = vec![
            format!("main_category={}", memory.taxonomy.main_category),
            format!("confidence={:.2}", memory.taxonomy.confidence),
            format!(
                "dimensions={}",
                memory
                    .taxonomy
                    .dimensions
                    .iter()
                    .map(|dimension| format!(
                        "{}={}",
                        dimension.dimension, dimension.dominant_label
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            format!(
                "relations={}",
                memory
                    .taxonomy
                    .relations
                    .iter()
                    .map(|relation| format!(
                        "{} {} {}",
                        relation.source, relation.relation, relation.target
                    ))
                    .collect::<Vec<_>>()
                    .join(" | ")
            ),
            format!(
                "learning=success:{:.2}, failures:{}, repeated:{}, decay:{:.2}, conflict:{:.2}",
                memory.learning.success_score,
                memory.learning.failure_count,
                memory.learning.repeated_mistake_count,
                memory.learning.reinforcement_decay,
                memory.learning.conflict_score
            ),
        ];
        Ok(Some(MemoryInspection {
            memory,
            versions,
            explanation,
        }))
    }

    pub fn analytics_csv(&self, project_id: &str) -> Result<String> {
        let analytics = self.analytics(project_id)?;
        let mut csv = String::from(
            "section,key,memory_id,headline,score,access_count,success_score,failure_count,value\n",
        );
        for item in analytics.most_recalled {
            csv.push_str(&format!(
                "most_recalled,,{},\"{}\",{:.2},{},{:.2},{},{}\n",
                item.memory_id,
                item.headline.replace('"', "'"),
                item.score,
                item.access_count,
                item.success_score,
                item.failure_count,
                item.access_count
            ));
        }
        for item in analytics.most_reinforced {
            csv.push_str(&format!(
                "most_reinforced,,{},\"{}\",{:.2},{},{:.2},{},{}\n",
                item.memory_id,
                item.headline.replace('"', "'"),
                item.score,
                item.access_count,
                item.success_score,
                item.failure_count,
                item.score
            ));
        }
        for item in analytics.most_punished {
            csv.push_str(&format!(
                "most_punished,,{},\"{}\",{:.2},{},{:.2},{},{}\n",
                item.memory_id,
                item.headline.replace('"', "'"),
                item.score,
                item.access_count,
                item.success_score,
                item.failure_count,
                item.score
            ));
        }
        for bucket in analytics.growth {
            csv.push_str(&format!(
                "growth,{},,,0,0,0,0,{}\n",
                bucket.bucket, bucket.memories
            ));
        }
        for bucket in analytics.conflict_heatmap {
            csv.push_str(&format!(
                "conflict_heatmap,{},,,0,0,0,0,{}\n",
                bucket.bucket, bucket.conflicts
            ));
        }
        Ok(csv)
    }

    pub fn analytics_markdown(&self, project_id: &str) -> Result<String> {
        let analytics = self.analytics(project_id)?;
        let mut markdown = format!(
            "# Memovyn Analytics for `{}`\n\n- Total memories: {}\n- Total queries: {}\n- Project token savings: {}\n- Session token savings: {}\n- Conflicts: {}\n- Reinforced memories: {}\n- Penalized memories: {}\n\n",
            analytics.project_id,
            analytics.total_memories,
            analytics.total_queries,
            analytics.total_token_savings,
            analytics.session_token_savings,
            analytics.conflict_count,
            analytics.reinforced_memories,
            analytics.penalized_memories
        );
        if !analytics.behavior_insights.is_empty() {
            markdown.push_str("## Behavior Insights\n");
            for insight in &analytics.behavior_insights {
                markdown.push_str(&format!("- {}\n", insight));
            }
            markdown.push('\n');
        }
        markdown.push_str("## Top Recalled Memories\n");
        for memory in analytics.most_recalled.iter().take(10) {
            markdown.push_str(&format!(
                "- **{}**: {} (recalled {} times)\n",
                memory.headline, memory.summary, memory.access_count
            ));
        }
        Ok(markdown)
    }

    pub fn archive_memory(&self, request: ArchiveRequest) -> Result<ArchiveResponse> {
        let memory = self
            .database
            .archive_memory(request.memory_id)?
            .ok_or_else(|| {
                crate::error::MemovynError::NotFound(format!("memory {}", request.memory_id))
            })?;
        self.search.refresh(memory.clone());
        Ok(ArchiveResponse { memory })
    }

    pub fn export_project(&self, project_id: &str, path: &Path) -> Result<()> {
        let bundle: ExportBundle = self.database.export_project(project_id)?;
        std::fs::write(path, serde_json::to_string_pretty(&bundle)?)?;
        Ok(())
    }

    pub fn import_bundle(&self, path: &Path) -> Result<usize> {
        let data = std::fs::read_to_string(path)?;
        let bundle: ExportBundle = serde_json::from_str(&data)?;
        let mut imported = 0usize;
        for memory in bundle.memories {
            self.database
                .upsert_project(&memory.project_id, memory.metadata.share_scope)?;
            self.database.insert_memory(&memory)?;
            self.search.insert(memory);
            imported += 1;
        }
        Ok(imported)
    }

    pub fn benchmark(&self, project_id: &str, memory_count: usize, query: &str) -> Result<String> {
        let mut add_latencies = Vec::with_capacity(memory_count);
        for idx in 0..memory_count {
            let started = Instant::now();
            self.add_memory(AddMemoryRequest {
                project_id: project_id.to_string(),
                content: format!(
                    "Benchmark memory {idx}: We decided to persist SQLite state, build BM25 retrieval, virtualize the dashboard list, and reinforce shared cross-session architecture for project {project_id}."
                ),
                metadata: Default::default(),
                kind: MemoryKind::Observation,
            })?;
            add_latencies.push(started.elapsed().as_micros() as u64);
        }
        let add_total_us = add_latencies.iter().sum::<u64>();

        let mut search_latencies = Vec::with_capacity(25);
        let mut hits = 0usize;
        for round in 0..25 {
            let search_started = Instant::now();
            let search = self.search_memories(SearchRequest {
                project_id: project_id.to_string(),
                query: if round % 5 == 0 {
                    format!("{query} architecture")
                } else {
                    query.to_string()
                },
                limit: 10,
                filters: Default::default(),
            })?;
            hits = search.total_hits;
            search_latencies.push(search_started.elapsed().as_micros() as u64);
        }

        Ok(format!(
            "add_count={memory_count} add_avg_us={} add_p95_us={} search_avg_ms={} search_p95_ms={} hits={}",
            add_total_us / memory_count.max(1) as u64,
            percentile_u64(&mut add_latencies, 0.95),
            percentile_u64(&mut search_latencies, 0.50) as f32 / 1000.0,
            percentile_u64(&mut search_latencies, 0.95) as f32 / 1000.0,
            hits
        ))
    }

    fn propagate_feedback_influence(
        &self,
        memory: &MemoryRecord,
        outcome: FeedbackOutcome,
        dampened_weight: f32,
    ) -> Result<Vec<Uuid>> {
        if dampened_weight <= 0.05 {
            return Ok(Vec::new());
        }
        let shared_projects = self.database.list_shared_projects(&memory.project_id)?;
        let query = format!(
            "{} {}",
            memory.taxonomy.main_category,
            memory
                .taxonomy
                .multi_labels
                .iter()
                .take(4)
                .cloned()
                .collect::<Vec<_>>()
                .join(" ")
        );
        let related = self.search.search(
            &memory.project_id,
            &query,
            6,
            &SearchFilters {
                include_shared: true,
                include_private_notes: true,
                ..Default::default()
            },
            &shared_projects,
        );
        let mut influenced = Vec::new();
        for hit in related
            .detail_layer
            .into_iter()
            .filter(|hit| hit.memory_id != memory.id)
            .take(3)
        {
            let Some(updated) = self.database.feedback_memory(
                hit.memory_id,
                outcome,
                false,
                dampened_weight,
                0.35,
                &memory.taxonomy.avoid_patterns,
                Some("cross_project_influence"),
            )?
            else {
                continue;
            };
            self.search.refresh(updated.clone());
            influenced.push(updated.id);
        }
        Ok(influenced)
    }
}

fn build_debugging_notes(active_conflicts: &[String]) -> Vec<String> {
    let mut notes = vec![
        "Memovyn now exposes dimension breakdowns, relations, and debug traces for every memory."
            .to_string(),
        "Use inspect mode when a memory looks wrong; the classifier now records aliases, prototype hits, path hints, and project priors."
            .to_string(),
    ];
    if !active_conflicts.is_empty() {
        notes.push(format!(
            "Active conflicts detected: {}",
            active_conflicts.join(" | ")
        ));
    }
    notes
}

fn build_behavior_insights(analytics: &AnalyticsSnapshot) -> Vec<String> {
    let mut insights = Vec::new();
    if analytics.total_queries > 0 {
        let avg_tokens = analytics.total_token_savings as f64 / analytics.total_queries as f64;
        insights.push(format!(
            "Memovyn is saving an estimated {:.0} tokens per recall on average in this project.",
            avg_tokens
        ));
    }
    if let Some((label, count)) = analytics.label_hotspots.first() {
        let baseline = analytics
            .label_hotspots
            .iter()
            .map(|(_, count)| *count)
            .sum::<usize>()
            .max(1) as f64
            / analytics.label_hotspots.len().max(1) as f64;
        let multiple = *count as f64 / baseline.max(1.0);
        insights.push(format!(
            "This project revisits `{}` {:.1}x more often than the average hot taxonomy label.",
            label, multiple
        ));
    }
    if let Some(memory) = analytics.most_punished.first() {
        insights.push(format!(
            "The most punished memory is `{}` with {} failures and score {:.2}.",
            memory.headline, memory.failure_count, memory.score
        ));
    }
    if analytics.conflict_count > 0 {
        insights.push(format!(
            "{} conflict-bearing memories are still active; avoid-pattern consolidation is keeping them visible.",
            analytics.conflict_count
        ));
    }
    insights
}

fn consolidated_avoid_pattern(memory: &MemoryRecord) -> String {
    let stem = memory
        .headline
        .split_whitespace()
        .take(4)
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join("-");
    format!("avoid:{}:{}", memory.taxonomy.main_category, stem)
}

fn percentile_u64(values: &mut [u64], percentile: f32) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() as f32 - 1.0) * percentile.clamp(0.0, 1.0)).round() as usize;
    values[index.min(values.len() - 1)]
}
