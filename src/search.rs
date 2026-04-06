use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};
use std::sync::RwLock;

use time::OffsetDateTime;

use crate::domain::{
    MemoryKind, MemoryRecord, PrivacyLevel, ProgressiveIndexCard, ProgressiveSummary,
    SearchFilters, SearchHit, SearchResponse, TimelineEntry,
};
use crate::taxonomy::{TaxonomyEvolutionSnapshot, tokenize};

#[derive(Debug, Default)]
pub struct SearchIndex {
    projects: RwLock<HashMap<String, ProjectIndex>>,
}

#[derive(Debug, Default)]
struct ProjectIndex {
    docs: Vec<IndexedMemory>,
    doc_lookup: HashMap<uuid::Uuid, usize>,
    postings: HashMap<String, Vec<Posting>>,
    total_doc_len: usize,
    avg_doc_len: f32,
    active_doc_count: usize,
    insights: ProjectInsights,
    query_cache: RwLock<HashMap<String, CachedQuery>>,
}

#[derive(Debug, Default)]
struct ProjectInsights {
    label_counts: HashMap<String, usize>,
    reinforced_label_counts: HashMap<String, usize>,
    solidified_prior_counts: HashMap<String, usize>,
    avoid_label_counts: HashMap<String, usize>,
    relation_counts: HashMap<String, usize>,
    dimension_counts: HashMap<String, HashMap<String, usize>>,
    privacy_counts: HashMap<String, usize>,
    term_df: HashMap<String, usize>,
}

#[derive(Debug, Clone)]
struct IndexedMemory {
    record: MemoryRecord,
    doc_len: usize,
    score_hint: f32,
    label_blob: String,
    relation_blob: String,
}

#[derive(Debug, Clone, Copy)]
struct Posting {
    doc_idx: usize,
    term_freq: usize,
}

#[derive(Debug, Clone)]
struct CachedQuery {
    total_hits: usize,
    hits: Vec<SearchHit>,
}

#[derive(Debug, Clone, Copy)]
struct ScoredDoc {
    idx: usize,
    score: f32,
    created_at: OffsetDateTime,
}

#[derive(Debug, Clone, Copy, Default)]
struct QueryProfile {
    all_broad: bool,
    has_broad: bool,
}

impl SearchIndex {
    pub fn new(records: impl IntoIterator<Item = MemoryRecord>) -> Self {
        let index = Self::default();
        for record in records {
            index.insert(record);
        }
        index
    }

    pub fn insert(&self, record: MemoryRecord) {
        let project_id = record.project_id.clone();
        let mut projects = self.projects.write().expect("search index poisoned");
        projects.entry(project_id).or_default().insert(record);
    }

    pub fn refresh(&self, record: MemoryRecord) {
        let project_id = record.project_id.clone();
        let mut projects = self.projects.write().expect("search index poisoned");
        projects.entry(project_id).or_default().refresh(record);
    }

    pub fn has_project(&self, project_id: &str) -> bool {
        self.projects
            .read()
            .expect("search index poisoned")
            .contains_key(project_id)
    }

    pub fn project_summary(
        &self,
        project_id: &str,
    ) -> (
        Vec<(String, usize)>,
        Vec<(String, usize)>,
        Vec<(String, String)>,
        Vec<String>,
        Vec<String>,
        Vec<String>,
    ) {
        let projects = self.projects.read().expect("search index poisoned");
        let Some(project) = projects.get(project_id) else {
            return (
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
                Vec::new(),
            );
        };
        project.project_summary()
    }

    pub fn taxonomy_feedback(&self, project_id: &str) -> TaxonomyEvolutionSnapshot {
        let projects = self.projects.read().expect("search index poisoned");
        let Some(project) = projects.get(project_id) else {
            return TaxonomyEvolutionSnapshot::default();
        };
        project.taxonomy_feedback()
    }

    pub fn project_analytics(
        &self,
        project_id: &str,
    ) -> (Vec<(String, usize)>, Vec<(String, usize)>, usize) {
        let projects = self.projects.read().expect("search index poisoned");
        let Some(project) = projects.get(project_id) else {
            return (Vec::new(), Vec::new(), 0);
        };
        (
            top_pairs(&project.insights.label_counts, 24),
            top_pairs(&project.insights.relation_counts, 20),
            project.conflict_count(),
        )
    }

    pub fn search(
        &self,
        project_id: &str,
        query: &str,
        limit: usize,
        filters: &SearchFilters,
        shared_projects: &[String],
    ) -> SearchResponse {
        let query_tokens = tokenize(query);
        let token_refs = query_tokens.iter().map(String::as_str).collect::<Vec<_>>();
        let projects = self.projects.read().expect("search index poisoned");

        let mut hits = Vec::new();
        let mut total_hits = 0usize;
        let keep_per_project = limit.saturating_mul(6).max(64);
        let mut project_names = vec![project_id.to_string()];
        if filters.include_shared {
            project_names.extend(shared_projects.iter().cloned());
        }

        for name in project_names {
            let Some(project) = projects.get(&name) else {
                continue;
            };
            let (count, mut project_hits) =
                project.search(query, &token_refs, filters, keep_per_project);
            total_hits += count;
            hits.append(&mut project_hits);
        }

        trim_to_top_k(&mut hits, limit.max(1));

        let index_layer = hits
            .iter()
            .map(|hit| ProgressiveIndexCard {
                memory_id: hit.memory_id,
                headline: hit.headline.clone(),
                summary: hit.summary.clone(),
                labels: hit.labels.iter().take(8).cloned().collect(),
                score: hit.score,
            })
            .collect::<Vec<_>>();

        let summary_layer = hits
            .iter()
            .map(|hit| ProgressiveSummary {
                memory_id: hit.memory_id,
                main_category: hit.main_category.clone(),
                confidence: hit.confidence,
                explanation: hit.explanation.clone(),
                relations: hit
                    .labels
                    .iter()
                    .filter(|label| label.starts_with("relation:"))
                    .take(4)
                    .cloned()
                    .collect(),
            })
            .collect::<Vec<_>>();

        let timeline_layer = hits
            .iter()
            .map(|hit| TimelineEntry {
                memory_id: hit.memory_id,
                timestamp: hit.created_at,
                headline: hit.headline.clone(),
                change_signal: if hit.penalty > hit.reinforcement {
                    "avoid".to_string()
                } else {
                    "reinforce".to_string()
                },
            })
            .collect::<Vec<_>>();

        SearchResponse {
            project_id: project_id.to_string(),
            query: query.to_string(),
            total_hits,
            index_layer,
            summary_layer,
            timeline_layer,
            detail_layer: hits,
        }
    }

    pub fn recent_cards(&self, project_id: &str, limit: usize) -> Vec<ProgressiveIndexCard> {
        let projects = self.projects.read().expect("search index poisoned");
        let Some(project) = projects.get(project_id) else {
            return Vec::new();
        };
        project
            .recent_docs(limit)
            .into_iter()
            .map(|doc| ProgressiveIndexCard {
                memory_id: doc.record.id,
                headline: doc.record.headline.clone(),
                summary: doc.record.summary.clone(),
                labels: doc
                    .record
                    .taxonomy
                    .multi_labels
                    .iter()
                    .take(6)
                    .cloned()
                    .collect(),
                score: ranking_bias(&doc.record),
            })
            .collect()
    }

    pub fn recent_summaries(&self, project_id: &str, limit: usize) -> Vec<ProgressiveSummary> {
        let projects = self.projects.read().expect("search index poisoned");
        let Some(project) = projects.get(project_id) else {
            return Vec::new();
        };
        project
            .recent_docs(limit)
            .into_iter()
            .map(|doc| ProgressiveSummary {
                memory_id: doc.record.id,
                main_category: doc.record.taxonomy.main_category.clone(),
                confidence: doc.record.taxonomy.confidence,
                explanation: doc.record.taxonomy.metadata.summary.clone(),
                relations: doc
                    .record
                    .taxonomy
                    .relations
                    .iter()
                    .take(4)
                    .map(|relation| {
                        format!(
                            "{} {} {}",
                            relation.source, relation.relation, relation.target
                        )
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn recent_timeline(&self, project_id: &str, limit: usize) -> Vec<TimelineEntry> {
        let projects = self.projects.read().expect("search index poisoned");
        let Some(project) = projects.get(project_id) else {
            return Vec::new();
        };
        project
            .recent_docs(limit)
            .into_iter()
            .map(|doc| TimelineEntry {
                memory_id: doc.record.id,
                timestamp: doc.record.created_at,
                headline: doc.record.headline.clone(),
                change_signal: match doc.record.kind {
                    MemoryKind::Issue => "issue",
                    MemoryKind::Decision => "decision",
                    MemoryKind::Outcome => "outcome",
                    MemoryKind::Note => "note",
                    MemoryKind::Reflection => "reflection",
                    MemoryKind::Observation | MemoryKind::Context => "context",
                }
                .to_string(),
            })
            .collect()
    }

    pub fn get_memory(&self, memory_id: uuid::Uuid) -> Option<MemoryRecord> {
        let projects = self.projects.read().expect("search index poisoned");
        for project in projects.values() {
            if let Some(idx) = project.doc_lookup.get(&memory_id) {
                return Some(project.docs[*idx].record.clone());
            }
        }
        None
    }
}

impl ProjectIndex {
    fn insert(&mut self, record: MemoryRecord) {
        let doc_idx = self.docs.len();
        let tokens = build_search_tokens(&record);
        let doc_len = tokens.len().max(1);
        let mut frequency = HashMap::<String, usize>::with_capacity(tokens.len());
        let mut unique_terms = HashSet::<String>::with_capacity(tokens.len());
        for token in tokens {
            *frequency.entry(token.clone()).or_insert(0) += 1;
            unique_terms.insert(token);
        }
        for (token, term_freq) in frequency {
            self.postings
                .entry(token)
                .or_default()
                .push(Posting { doc_idx, term_freq });
        }
        self.insights.observe_insert(&record, &unique_terms);
        self.total_doc_len += doc_len;
        self.avg_doc_len = self.total_doc_len as f32 / (doc_idx + 1) as f32;
        if !is_archived(&record) {
            self.active_doc_count += 1;
        }
        self.doc_lookup.insert(record.id, doc_idx);
        self.docs.push(IndexedMemory {
            score_hint: ranking_bias(&record),
            label_blob: build_label_blob(&record),
            relation_blob: build_relation_blob(&record),
            record,
            doc_len,
        });
        self.query_cache
            .write()
            .expect("query cache poisoned")
            .clear();
    }

    fn refresh(&mut self, record: MemoryRecord) {
        let Some(doc_idx) = self.doc_lookup.get(&record.id).copied() else {
            self.insert(record);
            return;
        };
        let old_record = self.docs[doc_idx].record.clone();
        let old_archived = is_archived(&old_record);
        let new_archived = is_archived(&record);
        match (old_archived, new_archived) {
            (false, true) => self.active_doc_count = self.active_doc_count.saturating_sub(1),
            (true, false) => self.active_doc_count += 1,
            _ => {}
        }
        self.docs[doc_idx].score_hint = ranking_bias(&record);
        self.docs[doc_idx].label_blob = build_label_blob(&record);
        self.docs[doc_idx].relation_blob = build_relation_blob(&record);
        self.docs[doc_idx].record = record;
        self.insights
            .observe_learning_refresh(&old_record, &self.docs[doc_idx].record);
        self.query_cache
            .write()
            .expect("query cache poisoned")
            .clear();
    }

    fn project_summary(
        &self,
    ) -> (
        Vec<(String, usize)>,
        Vec<(String, usize)>,
        Vec<(String, String)>,
        Vec<String>,
        Vec<String>,
        Vec<String>,
    ) {
        let top_labels = top_pairs(&self.insights.label_counts, 16);
        let top_relations = top_pairs(&self.insights.relation_counts, 12);
        let mut dominant_dimensions = self
            .insights
            .dimension_counts
            .iter()
            .filter_map(|(dimension, counts)| {
                top_pairs(counts, 1)
                    .into_iter()
                    .next()
                    .map(|(label, _)| (dimension.clone(), label))
            })
            .collect::<Vec<_>>();
        dominant_dimensions.sort_by(|left, right| left.0.cmp(&right.0));

        let avoid_patterns = top_keys(&self.insights.avoid_label_counts, 8);
        let privacy_signals = top_keys(&self.insights.privacy_counts, 8);
        let active_conflicts = self
            .docs
            .iter()
            .filter(|doc| is_conflicted(&doc.record))
            .map(|doc| doc.record.headline.clone())
            .take(8)
            .collect::<Vec<_>>();

        (
            top_labels,
            top_relations,
            dominant_dimensions,
            avoid_patterns,
            privacy_signals,
            active_conflicts,
        )
    }

    fn taxonomy_feedback(&self) -> TaxonomyEvolutionSnapshot {
        TaxonomyEvolutionSnapshot {
            prior_labels: top_keys(&self.insights.label_counts, 10),
            reinforced_labels: top_keys(&self.insights.reinforced_label_counts, 8),
            solidified_priors: top_keys(&self.insights.solidified_prior_counts, 6),
            avoid_patterns: top_keys(&self.insights.avoid_label_counts, 8),
            project_terms: top_keys(&self.insights.term_df, 12)
                .into_iter()
                .filter(|term| term.len() >= 4)
                .collect(),
        }
    }

    fn search(
        &self,
        query: &str,
        tokens: &[&str],
        filters: &SearchFilters,
        keep: usize,
    ) -> (usize, Vec<SearchHit>) {
        if let Some(cached) = self.cached_query(query, filters, keep) {
            return cached;
        }

        let profile = query_profile(tokens, &self.postings, self.docs.len());
        // Broad, high-density queries are the hardest large-scale case. Instead of
        // expanding the entire project candidate set, we score a ranked active
        // shortlist and only materialize the winning top-k documents.
        let (total_hits, mut scored) = if tokens.is_empty() {
            self.score_recent_shortlist(tokens, filters, keep, self.active_doc_count.max(keep))
        } else if profile.all_broad {
            let total_hits = if filters.is_empty() {
                self.active_doc_count
            } else {
                self.filtered_count(filters, recent_candidate_limit(self.active_doc_count, keep))
            };
            (
                total_hits,
                self.score_recent_shortlist(
                    tokens,
                    filters,
                    keep,
                    recent_candidate_limit(self.active_doc_count, keep),
                )
                .1,
            )
        } else {
            self.score_selective_candidates(tokens, filters, keep, profile)
        };
        trim_scored_to_top_k(&mut scored, keep.max(1));
        let hits = scored
            .into_iter()
            .map(|item| materialize_hit(&self.docs[item.idx], item.score))
            .collect::<Vec<_>>();

        self.store_cached_query(query, filters, total_hits, &hits);
        (total_hits, hits)
    }

    fn recent_docs(&self, limit: usize) -> Vec<&IndexedMemory> {
        self.docs
            .iter()
            .rev()
            .filter(|doc| !is_archived(&doc.record))
            .take(limit)
            .collect()
    }

    fn conflict_count(&self) -> usize {
        self.docs
            .iter()
            .filter(|doc| is_conflicted(&doc.record))
            .count()
    }

    fn cached_query(
        &self,
        query: &str,
        filters: &SearchFilters,
        keep: usize,
    ) -> Option<(usize, Vec<SearchHit>)> {
        if !is_cacheable(filters) {
            return None;
        }
        let key = cache_key(query, filters);
        let cache = self.query_cache.read().expect("query cache poisoned");
        let cached = cache.get(&key)?;
        if cached.hits.len() < keep {
            return None;
        }
        Some((
            cached.total_hits,
            cached.hits.iter().take(keep).cloned().collect(),
        ))
    }

    fn store_cached_query(
        &self,
        query: &str,
        filters: &SearchFilters,
        total_hits: usize,
        hits: &[SearchHit],
    ) {
        if !is_cacheable(filters) {
            return;
        }
        let mut cache = self.query_cache.write().expect("query cache poisoned");
        if cache.len() >= 64 {
            cache.clear();
        }
        cache.insert(
            cache_key(query, filters),
            CachedQuery {
                total_hits,
                hits: hits.iter().take(64).cloned().collect(),
            },
        );
    }

    fn score_recent_shortlist(
        &self,
        tokens: &[&str],
        filters: &SearchFilters,
        keep: usize,
        shortlist: usize,
    ) -> (usize, Vec<ScoredDoc>) {
        // ZeroText-style fast path: recent, active memories tend to dominate broad
        // operational queries, so we keep this path allocation-light and avoid
        // walking every high-frequency posting list.
        let mut total_hits = 0usize;
        let mut scored = Vec::with_capacity(shortlist.min(keep.saturating_mul(16).max(64)));
        for (idx, doc) in self.docs.iter().enumerate().rev() {
            if scored.len() >= shortlist {
                break;
            }
            if !matches_filters(doc, filters) {
                continue;
            }
            total_hits += 1;
            let broad_bonus = if tokens.is_empty() {
                0.0
            } else {
                overlap_score(tokens, &doc.label_blob) * 0.28
                    + overlap_score(tokens, &doc.relation_blob) * 0.16
                    + overlap_score(tokens, &doc.record.summary) * 0.12
            };
            scored.push(ScoredDoc {
                idx,
                score: doc.score_hint + broad_bonus + recency_boost(doc.record.created_at),
                created_at: doc.record.created_at,
            });
        }
        (total_hits, scored)
    }

    fn score_selective_candidates(
        &self,
        tokens: &[&str],
        filters: &SearchFilters,
        keep: usize,
        profile: QueryProfile,
    ) -> (usize, Vec<ScoredDoc>) {
        // Selective queries still benefit from classic inverted-index scoring, but
        // we skip extremely dense tokens and fall back to a recent shortlist when
        // a query mixes broad and specific terms.
        let mut scores = vec![0.0f32; self.docs.len()];
        let mut touched = Vec::<usize>::with_capacity(keep.saturating_mul(32).max(256));

        for token in tokens {
            let Some(postings) = self.postings.get(*token) else {
                continue;
            };
            let density = postings.len() as f32 / self.docs.len().max(1) as f32;
            if density >= 0.18 {
                continue;
            }
            let df = postings.len() as f32;
            let idf = ((self.docs.len() as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();
            for posting in postings {
                let doc = &self.docs[posting.doc_idx];
                let tf = posting.term_freq as f32;
                let doc_len = doc.doc_len as f32;
                let bm25 = idf
                    * ((tf * 2.2)
                        / (tf + 1.2 * (1.0 - 0.75 + 0.75 * (doc_len / self.avg_doc_len.max(1.0)))));
                if scores[posting.doc_idx] == 0.0 {
                    touched.push(posting.doc_idx);
                }
                scores[posting.doc_idx] += bm25;
            }
        }

        if touched.is_empty() || profile.has_broad {
            let shortlist = recent_candidate_limit(self.active_doc_count, keep);
            for (idx, doc) in self.docs.iter().enumerate().rev() {
                if touched.len() >= shortlist {
                    break;
                }
                if is_archived(&doc.record) || scores[idx] != 0.0 {
                    continue;
                }
                touched.push(idx);
            }
        }

        let mut scored = Vec::with_capacity(touched.len().min(keep.saturating_mul(16).max(64)));
        for doc_idx in touched {
            let doc = &self.docs[doc_idx];
            if !matches_filters(doc, filters) {
                continue;
            }
            let taxonomy_overlap = overlap_score(tokens, &doc.label_blob);
            let relation_overlap = overlap_score(tokens, &doc.relation_blob);
            scored.push(ScoredDoc {
                idx: doc_idx,
                score: scores[doc_idx]
                    + taxonomy_overlap * 0.45
                    + relation_overlap * 0.4
                    + doc.score_hint
                    + recency_boost(doc.record.created_at),
                created_at: doc.record.created_at,
            });
        }
        let total_hits = scored.len();
        (total_hits, scored)
    }

    fn filtered_count(&self, filters: &SearchFilters, hard_cap: usize) -> usize {
        if filters.is_empty() {
            return self.active_doc_count;
        }
        let mut count = 0usize;
        for doc in self.docs.iter().rev() {
            if matches_filters(doc, filters) {
                count += 1;
                if count >= hard_cap {
                    break;
                }
            }
        }
        count
    }
}

impl ProjectInsights {
    fn observe_insert(&mut self, record: &MemoryRecord, unique_terms: &HashSet<String>) {
        for label in &record.taxonomy.multi_labels {
            *self.label_counts.entry(label.clone()).or_insert(0) += 1;
            if is_avoid_label(label) {
                *self.avoid_label_counts.entry(label.clone()).or_insert(0) += 1;
            }
            if record.reinforcement >= record.penalty {
                *self
                    .reinforced_label_counts
                    .entry(label.clone())
                    .or_insert(0) += 1;
                if record.learning.success_score >= 2.0 || record.access_count >= 3 {
                    *self
                        .solidified_prior_counts
                        .entry(label.clone())
                        .or_insert(0) += 1;
                }
            }
            if label.starts_with("sensitive:") || label.starts_with("privacy:") {
                *self.privacy_counts.entry(label.clone()).or_insert(0) += 1;
            }
        }

        for relation in &record.taxonomy.relations {
            *self
                .relation_counts
                .entry(format!(
                    "{}:{}:{}",
                    relation.source, relation.relation, relation.target
                ))
                .or_insert(0) += 1;
        }

        for dimension in &record.taxonomy.dimensions {
            *self
                .dimension_counts
                .entry(dimension.dimension.clone())
                .or_default()
                .entry(dimension.dominant_label.clone())
                .or_insert(0) += 1;
        }

        for term in unique_terms {
            *self.term_df.entry(term.clone()).or_insert(0) += 1;
        }
    }

    fn observe_learning_refresh(&mut self, old: &MemoryRecord, new: &MemoryRecord) {
        let old_reinforced = old.reinforcement >= old.penalty;
        let new_reinforced = new.reinforcement >= new.penalty;
        for label in &new.taxonomy.multi_labels {
            if old_reinforced && !new_reinforced {
                decrement_map(&mut self.reinforced_label_counts, label);
            }
            if !old_reinforced && new_reinforced {
                *self
                    .reinforced_label_counts
                    .entry(label.clone())
                    .or_insert(0) += 1;
            }
            let old_solidified = old.learning.success_score >= 2.0 || old.access_count >= 3;
            let new_solidified = new.learning.success_score >= 2.0 || new.access_count >= 3;
            if old_solidified && !new_solidified {
                decrement_map(&mut self.solidified_prior_counts, label);
            }
            if !old_solidified && new_solidified {
                *self
                    .solidified_prior_counts
                    .entry(label.clone())
                    .or_insert(0) += 1;
            }
        }
    }
}

fn build_search_tokens(record: &MemoryRecord) -> Vec<String> {
    let mut search_blob = String::with_capacity(record.content.len() + record.summary.len() + 256);
    search_blob.push_str(&record.content);
    search_blob.push(' ');
    search_blob.push_str(&record.summary);
    search_blob.push(' ');
    search_blob.push_str(&record.taxonomy.main_category);
    for label in &record.taxonomy.multi_labels {
        search_blob.push(' ');
        search_blob.push_str(label);
    }
    for relation in &record.taxonomy.relations {
        search_blob.push(' ');
        search_blob.push_str(&relation.source);
        search_blob.push(' ');
        search_blob.push_str(&relation.relation);
        search_blob.push(' ');
        search_blob.push_str(&relation.target);
    }
    for dimension in &record.taxonomy.dimensions {
        search_blob.push(' ');
        search_blob.push_str(&dimension.dominant_label);
    }
    tokenize(&search_blob).into_vec()
}

fn build_label_blob(record: &MemoryRecord) -> String {
    let mut blob = record.taxonomy.multi_labels.join(" ");
    if !record.taxonomy.avoid_patterns.is_empty() {
        blob.push(' ');
        blob.push_str(&record.taxonomy.avoid_patterns.join(" "));
    }
    if !record.taxonomy.reinforce_patterns.is_empty() {
        blob.push(' ');
        blob.push_str(&record.taxonomy.reinforce_patterns.join(" "));
    }
    blob
}

fn build_relation_blob(record: &MemoryRecord) -> String {
    let mut blob = String::with_capacity(record.taxonomy.relations.len() * 24);
    for relation in &record.taxonomy.relations {
        blob.push_str(&relation.source);
        blob.push(' ');
        blob.push_str(&relation.relation);
        blob.push(' ');
        blob.push_str(&relation.target);
        blob.push(' ');
    }
    blob
}

fn ranking_bias(record: &MemoryRecord) -> f32 {
    record.reinforcement + record.learning.success_score * 0.45 + record.taxonomy.confidence
        - (record.penalty * record.learning.reinforcement_decay)
        - record.learning.conflict_score * 0.18
        - record.learning.failure_count as f32 * 0.12
}

fn trim_to_top_k(hits: &mut Vec<SearchHit>, keep: usize) {
    if hits.len() > keep {
        let nth = keep - 1;
        hits.select_nth_unstable_by(nth, compare_hits);
        hits.truncate(keep);
    }
    hits.sort_by(compare_hits);
}

fn compare_hits(left: &SearchHit, right: &SearchHit) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then(right.created_at.cmp(&left.created_at))
}

fn trim_scored_to_top_k(scored: &mut Vec<ScoredDoc>, keep: usize) {
    if scored.len() > keep {
        let nth = keep - 1;
        scored.select_nth_unstable_by(nth, compare_scored);
        scored.truncate(keep);
    }
    scored.sort_by(compare_scored);
}

fn compare_scored(left: &ScoredDoc, right: &ScoredDoc) -> Ordering {
    right
        .score
        .partial_cmp(&left.score)
        .unwrap_or(Ordering::Equal)
        .then(right.created_at.cmp(&left.created_at))
}

fn materialize_hit(doc: &IndexedMemory, score: f32) -> SearchHit {
    SearchHit {
        memory_id: doc.record.id,
        score,
        headline: doc.record.headline.clone(),
        summary: doc.record.summary.clone(),
        content: doc.record.content.clone(),
        labels: doc.record.taxonomy.multi_labels.clone(),
        main_category: doc.record.taxonomy.main_category.clone(),
        confidence: doc.record.taxonomy.confidence,
        relation_count: doc.record.taxonomy.relations.len(),
        explanation: doc.record.taxonomy.metadata.summary.clone(),
        created_at: doc.record.created_at,
        reinforcement: doc.record.reinforcement,
        penalty: doc.record.penalty,
    }
}

fn overlap_score(tokens: &[&str], haystack: &str) -> f32 {
    tokens
        .iter()
        .filter(|token| haystack.contains(**token))
        .count() as f32
}

fn query_profile(
    tokens: &[&str],
    postings: &HashMap<String, Vec<Posting>>,
    doc_count: usize,
) -> QueryProfile {
    if tokens.is_empty() || doc_count == 0 {
        return QueryProfile::default();
    }
    let mut all_broad = true;
    let mut has_broad = false;
    for token in tokens {
        let density = postings
            .get(*token)
            .map(|postings| postings.len() as f32 / doc_count as f32)
            .unwrap_or(0.0);
        if density >= 0.18 {
            has_broad = true;
        } else {
            all_broad = false;
        }
    }
    QueryProfile {
        all_broad: has_broad && all_broad,
        has_broad,
    }
}

fn recent_candidate_limit(active_doc_count: usize, keep: usize) -> usize {
    active_doc_count.min(keep.saturating_mul(96).max(2048))
}

fn cache_key(query: &str, filters: &SearchFilters) -> String {
    format!(
        "{}|labels={}|kinds={}|since={:?}|until={:?}|private={}|archived={}",
        query,
        filters.labels.join(","),
        filters
            .kinds
            .iter()
            .map(|kind| format!("{kind:?}"))
            .collect::<Vec<_>>()
            .join(","),
        filters.since,
        filters.until,
        filters.include_private_notes,
        filters.include_archived
    )
}

fn is_cacheable(filters: &SearchFilters) -> bool {
    !filters.include_shared
}

trait SearchFiltersExt {
    fn is_empty(&self) -> bool;
}

impl SearchFiltersExt for SearchFilters {
    fn is_empty(&self) -> bool {
        self.labels.is_empty()
            && self.kinds.is_empty()
            && self.since.is_none()
            && self.until.is_none()
            && !self.include_private_notes
            && !self.include_shared
            && !self.include_archived
    }
}

fn matches_filters(doc: &IndexedMemory, filters: &SearchFilters) -> bool {
    if !filters.include_archived && is_archived(&doc.record) {
        return false;
    }
    if !filters.kinds.is_empty() && !filters.kinds.contains(&doc.record.kind) {
        return false;
    }
    if let Some(since) = filters.since {
        if doc.record.created_at < since {
            return false;
        }
    }
    if let Some(until) = filters.until {
        if doc.record.created_at > until {
            return false;
        }
    }
    if !filters.labels.is_empty()
        && !filters.labels.iter().all(|label| {
            doc.record
                .taxonomy
                .multi_labels
                .iter()
                .any(|candidate| candidate == label)
        })
    {
        return false;
    }
    if !filters.include_private_notes
        && doc.record.kind == MemoryKind::Note
        && doc.record.metadata.privacy != PrivacyLevel::Standard
    {
        return false;
    }
    true
}

fn top_pairs(map: &HashMap<String, usize>, limit: usize) -> Vec<(String, usize)> {
    let mut items = map
        .iter()
        .map(|(key, value)| (key.clone(), *value))
        .collect::<Vec<_>>();
    items.sort_by(|left, right| right.1.cmp(&left.1).then(left.0.cmp(&right.0)));
    items.truncate(limit);
    items
}

fn top_keys(map: &HashMap<String, usize>, limit: usize) -> Vec<String> {
    top_pairs(map, limit)
        .into_iter()
        .map(|(key, _)| key)
        .collect()
}

fn decrement_map(map: &mut HashMap<String, usize>, key: &str) {
    let should_remove = match map.get_mut(key) {
        Some(value) if *value > 1 => {
            *value -= 1;
            false
        }
        Some(_) => true,
        None => false,
    };
    if should_remove {
        map.remove(key);
    }
}

fn is_avoid_label(label: &str) -> bool {
    label.contains("avoid") || label.contains("regression") || label.contains("pitfall")
}

fn is_conflicted(record: &MemoryRecord) -> bool {
    record.learning.conflict_score > 0.0 || record.penalty > record.reinforcement
}

fn is_archived(record: &MemoryRecord) -> bool {
    matches!(
        record.metadata.extra.get("archived").map(String::as_str),
        Some("true")
    )
}

fn recency_boost(created_at: OffsetDateTime) -> f32 {
    let age_hours = (OffsetDateTime::now_utc() - created_at)
        .whole_hours()
        .unsigned_abs() as f32;
    1.0 / (1.0 + age_hours / 72.0)
}

#[cfg(test)]
fn test_percentile(values: &mut [u64], percentile: f32) -> u64 {
    if values.is_empty() {
        return 0;
    }
    values.sort_unstable();
    let index = ((values.len() as f32 - 1.0) * percentile.clamp(0.0, 1.0)).round() as usize;
    values[index.min(values.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::SearchIndex;
    use crate::domain::{
        LearningState, MemoryKind, MemoryMetadata, MemoryRecord, TaxonomyDebugView,
        TaxonomyDecomposition, TaxonomyMetadata,
    };
    use std::time::Instant;
    use time::OffsetDateTime;

    fn memory(id: &str, project: &str, headline: &str, content: &str) -> MemoryRecord {
        let now = OffsetDateTime::now_utc();
        MemoryRecord {
            id: uuid::Uuid::parse_str(id).unwrap(),
            project_id: project.to_string(),
            kind: MemoryKind::Observation,
            headline: headline.to_string(),
            summary: headline.to_string(),
            content: content.to_string(),
            content_hash: id.to_string(),
            taxonomy: TaxonomyDecomposition {
                main_category: "architecture".to_string(),
                confidence: 0.82,
                multi_labels: vec![
                    "architecture".to_string(),
                    "retrieval".to_string(),
                    "avoid_pattern".to_string(),
                ],
                metadata: TaxonomyMetadata {
                    headline: headline.to_string(),
                    summary: headline.to_string(),
                    language_hint: "rust".to_string(),
                    token_count: 12,
                    signal_count: 3,
                    taxonomy_version: "test".to_string(),
                    ..Default::default()
                },
                debug: TaxonomyDebugView::default(),
                ..Default::default()
            },
            metadata: MemoryMetadata::default(),
            created_at: now,
            updated_at: now,
            last_accessed_at: now,
            reinforcement: 0.0,
            penalty: 0.0,
            learning: LearningState::default(),
            access_count: 0,
            version: 1,
        }
    }

    #[test]
    fn refresh_updates_learning_bias() {
        let index = SearchIndex::default();
        let mut record = memory(
            "0195f7f4-8aa7-7ad0-8b8d-9a6b3d5c31fe",
            "demo",
            "avoid stale bug",
            "regression bug avoid pattern",
        );
        index.insert(record.clone());
        record.reinforcement = 2.0;
        record.learning.success_score = 2.0;
        index.refresh(record);
        let summary = index.project_summary("demo");
        assert!(!summary.0.is_empty());
        assert!(!index.recent_cards("demo", 1).is_empty());
    }

    #[test]
    #[ignore = "scale simulation for manual perf validation"]
    fn search_scale_simulation_200k() {
        let index = SearchIndex::default();
        let insert_started = Instant::now();
        for idx in 0..200_000u32 {
            let mut record = memory(
                &format!("0195f7f4-8aa7-7ad0-8b8d-{:012x}", idx),
                "scale-demo",
                &format!("memory {idx}"),
                "shared brain sqlite retrieval reinforcement conflict heatmap",
            );
            record.learning.success_score = (idx % 7) as f32;
            index.insert(record);
        }
        let insert_elapsed = insert_started.elapsed();
        let cold_started = Instant::now();
        let cold_response = index.search(
            "scale-demo",
            "sqlite reinforcement conflict",
            20,
            &Default::default(),
            &[],
        );
        let cold_elapsed = cold_started.elapsed();
        assert!(cold_response.total_hits > 0);
        let _ = index.search(
            "scale-demo",
            "sqlite reinforcement conflict architecture",
            20,
            &Default::default(),
            &[],
        );
        let mut latencies = Vec::new();
        let mut last_response = None;
        for round in 0..25 {
            let started = Instant::now();
            let response = index.search(
                "scale-demo",
                if round % 5 == 0 {
                    "sqlite reinforcement conflict architecture"
                } else {
                    "sqlite reinforcement conflict"
                },
                20,
                &Default::default(),
                &[],
            );
            latencies.push(started.elapsed().as_micros() as u64);
            last_response = Some(response);
        }
        let response = last_response.expect("scale search response");
        let p95 = super::test_percentile(&mut latencies, 0.95);
        let avg = latencies.iter().sum::<u64>() / latencies.len() as u64;
        assert!(response.total_hits > 0);
        assert!(!response.detail_layer.is_empty());
        eprintln!(
            "200k scale simulation insert={:?} cold_broad_ms={} search_avg_ms={} search_p95_ms={}",
            insert_elapsed,
            cold_elapsed.as_secs_f64() * 1000.0,
            avg as f32 / 1000.0,
            p95 as f32 / 1000.0
        );
    }
}
