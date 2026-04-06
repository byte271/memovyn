use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use aho_corasick::AhoCorasick;
use regex::Regex;
use smallvec::SmallVec;

use crate::domain::{
    DimensionBreakdown, HierarchyNode, MemoryMetadata, SensitiveSpan, TaxonomyDebugView,
    TaxonomyDecomposition, TaxonomyMetadata, TaxonomyRelation, TaxonomySignal,
};
use crate::model::ModelGuidance;

pub const TAXONOMY_VERSION: &str = "2026.04.legendary";

#[derive(Debug, Clone, Default)]
pub struct TaxonomyEvolutionSnapshot {
    pub prior_labels: Vec<String>,
    pub reinforced_labels: Vec<String>,
    pub solidified_priors: Vec<String>,
    pub avoid_patterns: Vec<String>,
    pub project_terms: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Dimension {
    Semantic,
    Domain,
    Activity,
    Artifact,
    Lifecycle,
    Privacy,
    Language,
}

impl Dimension {
    fn as_str(self) -> &'static str {
        match self {
            Self::Semantic => "semantic",
            Self::Domain => "domain",
            Self::Activity => "activity",
            Self::Artifact => "artifact",
            Self::Lifecycle => "lifecycle",
            Self::Privacy => "privacy",
            Self::Language => "language",
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct CategorySeed {
    id: &'static str,
    name: &'static str,
    dimension: Dimension,
    description: &'static str,
    aliases: &'static [&'static str],
    prototype: &'static [&'static str],
    dependencies: &'static [&'static str],
    base_priority: u8,
}

macro_rules! seed {
    ($id:literal, $name:literal, $dimension:ident, $description:literal, [$($alias:literal),* $(,)?], [$($prototype:literal),* $(,)?], [$($dependency:literal),* $(,)?], $priority:literal) => {
        CategorySeed {
            id: $id,
            name: $name,
            dimension: Dimension::$dimension,
            description: $description,
            aliases: &[$($alias),*],
            prototype: &[$($prototype),*],
            dependencies: &[$($dependency),*],
            base_priority: $priority,
        }
    };
}

#[derive(Debug, Clone)]
struct RankedSeed {
    seed: &'static CategorySeed,
    score: f32,
    alias_hits: Vec<String>,
    term_hits: Vec<String>,
    path_hits: Vec<String>,
    context_hits: Vec<String>,
}

#[derive(Debug, Default)]
struct TrieNode {
    pass_count: usize,
    children: BTreeMap<char, TrieNode>,
}

impl TrieNode {
    fn insert(&mut self, token: &str) {
        self.pass_count += 1;
        let mut cursor = self;
        for ch in token.chars() {
            cursor = cursor.children.entry(ch).or_default();
            cursor.pass_count += 1;
        }
    }

    fn collect_clusters(&self, prefix: &mut String, output: &mut Vec<String>) {
        if prefix.len() >= 4 && self.pass_count >= 2 && self.children.len() >= 2 {
            output.push(format!("cluster:{prefix}*"));
        }
        for (ch, child) in &self.children {
            prefix.push(*ch);
            child.collect_clusters(prefix, output);
            prefix.pop();
        }
    }
}

#[derive(Debug)]
pub struct TaxonomyEngine {
    alias_automaton: AhoCorasick,
    alias_patterns: Vec<String>,
    alias_to_seed: Vec<usize>,
    idf: HashMap<&'static str, f32>,
    prototype_avg_len: f32,
}

impl Default for TaxonomyEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl TaxonomyEngine {
    pub fn new() -> Self {
        let mut alias_patterns = Vec::new();
        let mut alias_to_seed = Vec::new();
        for (seed_idx, seed) in seeds().iter().enumerate() {
            for alias in seed.aliases {
                alias_patterns.push(alias.to_ascii_lowercase());
                alias_to_seed.push(seed_idx);
            }
        }

        let alias_automaton = AhoCorasick::builder()
            .ascii_case_insensitive(true)
            .build(alias_patterns.clone())
            .expect("valid aho-corasick automaton");

        let mut document_frequency = HashMap::new();
        for seed in seeds() {
            let unique = seed.prototype.iter().copied().collect::<HashSet<_>>();
            for token in unique {
                *document_frequency.entry(token).or_insert(0usize) += 1;
            }
        }

        let corpus_size = seeds().len() as f32;
        let idf = document_frequency
            .into_iter()
            .map(|(token, df)| {
                let df = df as f32;
                (token, ((corpus_size - df + 0.5) / (df + 0.5) + 1.0).ln())
            })
            .collect();

        let prototype_avg_len = seeds()
            .iter()
            .map(|seed| seed.prototype.len() as f32)
            .sum::<f32>()
            / seeds().len() as f32;

        Self {
            alias_automaton,
            alias_patterns,
            alias_to_seed,
            idf,
            prototype_avg_len,
        }
    }

    pub fn decompose(
        &self,
        content: &str,
        metadata: &MemoryMetadata,
    ) -> (String, TaxonomyDecomposition) {
        self.decompose_with_context(content, metadata, &TaxonomyEvolutionSnapshot::default())
    }

    pub fn decompose_with_context(
        &self,
        content: &str,
        metadata: &MemoryMetadata,
        evolution: &TaxonomyEvolutionSnapshot,
    ) -> (String, TaxonomyDecomposition) {
        self.decompose_with_context_and_guidance(content, metadata, evolution, None)
    }

    pub fn decompose_with_context_and_guidance(
        &self,
        content: &str,
        metadata: &MemoryMetadata,
        evolution: &TaxonomyEvolutionSnapshot,
        guidance: Option<&ModelGuidance>,
    ) -> (String, TaxonomyDecomposition) {
        let (sanitized, redactions, sensitivity_tags) = redact_sensitive(content);
        let tokens = tokenize(&sanitized);
        let token_count = tokens.len().max(1);
        let term_frequency = term_frequency(&tokens);
        let content_lower = sanitized.to_ascii_lowercase();

        let mut scores = vec![0.0f32; seeds().len()];
        let mut alias_hits = vec![Vec::new(); seeds().len()];
        let mut term_hits = vec![Vec::new(); seeds().len()];
        let mut path_hits = vec![Vec::new(); seeds().len()];
        let mut context_hits = vec![Vec::new(); seeds().len()];

        for mat in self.alias_automaton.find_iter(&content_lower) {
            let pattern_idx = mat.pattern().as_usize();
            let seed_idx = self.alias_to_seed[pattern_idx];
            scores[seed_idx] += 2.6;
            alias_hits[seed_idx].push(self.alias_patterns[pattern_idx].clone());
        }

        for (idx, seed) in seeds().iter().enumerate() {
            for term in seed.prototype {
                if let Some(tf) = term_frequency.get(term) {
                    let tf = *tf as f32;
                    let idf = *self.idf.get(term).unwrap_or(&0.1);
                    let norm = 1.2
                        * (1.0 - 0.75
                            + 0.75 * (token_count as f32 / self.prototype_avg_len.max(1.0)));
                    scores[idx] += idf * ((tf * 2.25) / (tf + norm));
                    term_hits[idx].push((*term).to_string());
                }
            }

            for path in &metadata.paths {
                if seed
                    .aliases
                    .iter()
                    .any(|alias| path.to_ascii_lowercase().contains(alias))
                {
                    scores[idx] += 1.1;
                    path_hits[idx].push(path.clone());
                }
            }

            if let Some(language) = &metadata.language {
                if seed.dimension == Dimension::Language && seed.id == language {
                    scores[idx] += 2.0;
                    context_hits[idx].push(format!("metadata-language:{language}"));
                }
            }

            if evolution.prior_labels.iter().any(|label| label == seed.id) {
                scores[idx] += 0.35;
                context_hits[idx].push("project-prior".to_string());
            }
            if evolution
                .reinforced_labels
                .iter()
                .any(|label| label == seed.id)
            {
                scores[idx] += 0.55;
                context_hits[idx].push("reinforced-prior".to_string());
            }
            // Solidified priors are stronger than ordinary reinforced labels: they
            // represent patterns that have repeatedly proven useful enough to steer
            // future classification for the whole project.
            if evolution
                .solidified_priors
                .iter()
                .any(|label| label == seed.id)
            {
                scores[idx] += 1.25;
                context_hits[idx].push("solidified-project-prior".to_string());
            }
            if seed.dependencies.iter().any(|dependency| {
                evolution
                    .solidified_priors
                    .iter()
                    .any(|label| label == dependency)
            }) {
                scores[idx] += 0.35;
                context_hits[idx].push("solidified-dependency-prior".to_string());
            }
            if evolution
                .avoid_patterns
                .iter()
                .any(|label| label == seed.id)
                && matches!(seed.dimension, Dimension::Lifecycle | Dimension::Semantic)
            {
                scores[idx] += 0.4;
                context_hits[idx].push("avoid-pattern-prior".to_string());
            }
            if evolution.project_terms.iter().any(|term| {
                term_frequency.contains_key(term.as_str()) || content_lower.contains(term.as_str())
            }) {
                scores[idx] += 0.12;
                context_hits[idx].push("project-lexicon".to_string());
            }
            if let Some(guidance) = guidance {
                if guidance
                    .main_category
                    .as_deref()
                    .is_some_and(|category| category == seed.id)
                {
                    scores[idx] += 1.15;
                    context_hits[idx].push("model-main-category".to_string());
                }
                if guidance.boosted_labels.iter().any(|label| label == seed.id) {
                    scores[idx] += 0.72;
                    context_hits[idx].push("model-guidance-label".to_string());
                }
                if guidance
                    .language_hint
                    .as_deref()
                    .is_some_and(|language| language.eq_ignore_ascii_case(seed.id))
                {
                    scores[idx] += 0.8;
                    context_hits[idx].push("model-language-hint".to_string());
                }
            }
        }

        let ranked = ranked_scores(&scores, &alias_hits, &term_hits, &path_hits, &context_hits);
        let top_score = ranked
            .first()
            .map(|item| item.score)
            .unwrap_or(1.0)
            .max(0.1);
        let main_category = ranked
            .iter()
            .find(|candidate| candidate.seed.dimension == Dimension::Domain)
            .or_else(|| ranked.first())
            .map(|candidate| candidate.seed.id.to_string())
            .unwrap_or_else(|| "architecture".to_string());

        let emergent_clusters = emergent_clusters(&tokens, metadata);
        let entities = extract_entities(metadata, &tokens);
        let dimensions = build_dimensions(&ranked, top_score);
        let relations = infer_relations(&ranked);
        let signals = build_signals(&ranked, top_score);
        let headline = headline(&sanitized, &tokens);
        let summary = summary(&headline, &dimensions, &relations);
        let mut debug = build_debug_view(&ranked);
        if let Some(guidance) = guidance {
            debug
                .context_hints
                .push(format!("classifier-backend:{}", guidance.backend));
            debug.context_hints.extend(
                guidance
                    .notes
                    .iter()
                    .map(|note| format!("model-note:{note}")),
            );
        }
        let mut avoid_patterns = ranked
            .iter()
            .filter(|candidate| {
                matches!(candidate.seed.id, "avoid_pattern" | "regression" | "risk")
            })
            .take(6)
            .map(|candidate| candidate.seed.id.to_string())
            .collect::<Vec<_>>();
        if let Some(guidance) = guidance {
            avoid_patterns.extend(guidance.avoid_patterns.iter().cloned());
        }
        avoid_patterns.sort_unstable();
        avoid_patterns.dedup();

        let mut reinforce_patterns = ranked
            .iter()
            .filter(|candidate| {
                matches!(
                    candidate.seed.id,
                    "stable" | "reinforced" | "decision" | "preference" | "learned_pattern"
                )
            })
            .take(6)
            .map(|candidate| candidate.seed.id.to_string())
            .collect::<Vec<_>>();
        if let Some(guidance) = guidance {
            reinforce_patterns.extend(guidance.reinforce_patterns.iter().cloned());
        }
        reinforce_patterns.sort_unstable();
        reinforce_patterns.dedup();

        let mut labels = build_multi_labels(
            &ranked,
            &dimensions,
            &relations,
            metadata,
            &emergent_clusters,
            &sensitivity_tags,
            token_count,
        );
        labels.sort_unstable();
        labels.dedup();
        while labels.len() < 20 {
            labels.push(format!(
                "fallback:{}",
                seeds()[labels.len() % seeds().len()].id
            ));
        }
        labels.truncate(50);

        let hierarchy = build_hierarchy(&ranked, &dimensions, &emergent_clusters);
        let confidence_mean = signals
            .iter()
            .take(8)
            .map(|signal| signal.confidence)
            .sum::<f32>()
            / signals.iter().take(8).count().max(1) as f32;
        let artifact_density = tokens
            .iter()
            .filter(|token| token.contains('/') || token.contains('.') || token.contains("::"))
            .count() as f32
            / token_count as f32;

        let decomposition = TaxonomyDecomposition {
            main_category,
            confidence: confidence_mean.max(0.2),
            multi_labels: labels,
            hierarchy,
            dimensions,
            signals,
            relations: relations.clone(),
            avoid_patterns,
            reinforce_patterns,
            metadata: TaxonomyMetadata {
                headline,
                summary,
                language_hint: metadata
                    .language
                    .clone()
                    .or_else(|| guidance.and_then(|guidance| guidance.language_hint.clone()))
                    .unwrap_or_else(|| detect_language_hint(&tokens).to_string()),
                classifier_backend: guidance
                    .map(|guidance| guidance.backend.clone())
                    .unwrap_or_else(|| "algorithm".to_string()),
                classifier_notes: guidance
                    .map(|guidance| guidance.notes.clone())
                    .unwrap_or_default(),
                model_confidence: guidance.map(|guidance| guidance.confidence).unwrap_or(0.0),
                token_count,
                signal_count: ranked
                    .iter()
                    .filter(|candidate| candidate.score > 0.08)
                    .count(),
                sentence_count: sentence_count(&sanitized),
                line_count: sanitized.lines().count().max(1),
                relation_count: relations.len(),
                artifact_density,
                confidence_mean,
                sensitivity_tags,
                emergent_clusters,
                entities,
                redactions,
                taxonomy_version: TAXONOMY_VERSION.to_string(),
                compression_hint: compression_hint(token_count, confidence_mean, relations.len()),
                inferred_kinds: inferred_kinds(&ranked),
            },
            debug,
        };

        (sanitized, decomposition)
    }
}

pub fn tokenize(input: &str) -> SmallVec<[String; 64]> {
    let mut tokens = SmallVec::<[String; 64]>::new();
    let mut current = String::with_capacity(24);
    for ch in input.chars() {
        if ch.is_ascii_alphanumeric() || matches!(ch, '/' | '_' | '-' | '.' | ':') {
            current.push(ch.to_ascii_lowercase());
        } else if !current.is_empty() {
            if !is_stopword(&current) {
                tokens.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if !current.is_empty() && !is_stopword(&current) {
        tokens.push(current);
    }
    tokens
}

fn term_frequency(tokens: &[String]) -> HashMap<&str, usize> {
    let mut frequency = HashMap::with_capacity(tokens.len());
    for token in tokens {
        *frequency.entry(token.as_str()).or_insert(0) += 1;
    }
    frequency
}

fn ranked_scores(
    scores: &[f32],
    alias_hits: &[Vec<String>],
    term_hits: &[Vec<String>],
    path_hits: &[Vec<String>],
    context_hits: &[Vec<String>],
) -> Vec<RankedSeed> {
    let mut ranked = seeds()
        .iter()
        .enumerate()
        .map(|(idx, seed)| RankedSeed {
            seed,
            score: scores[idx],
            alias_hits: alias_hits[idx].clone(),
            term_hits: term_hits[idx].clone(),
            path_hits: path_hits[idx].clone(),
            context_hits: context_hits[idx].clone(),
        })
        .collect::<Vec<_>>();
    ranked.sort_by(|left, right| {
        right
            .score
            .partial_cmp(&left.score)
            .unwrap_or(Ordering::Equal)
            .then(right.seed.base_priority.cmp(&left.seed.base_priority))
    });
    ranked
}

fn build_dimensions(ranked: &[RankedSeed], top_score: f32) -> Vec<DimensionBreakdown> {
    let mut grouped = HashMap::<Dimension, Vec<&RankedSeed>>::new();
    for candidate in ranked.iter().filter(|candidate| candidate.score > 0.08) {
        grouped
            .entry(candidate.seed.dimension)
            .or_default()
            .push(candidate);
    }

    let mut dimensions = Vec::new();
    for dimension in [
        Dimension::Semantic,
        Dimension::Domain,
        Dimension::Activity,
        Dimension::Artifact,
        Dimension::Lifecycle,
        Dimension::Privacy,
        Dimension::Language,
    ] {
        if let Some(entries) = grouped.get(&dimension) {
            let dominant = entries[0];
            dimensions.push(DimensionBreakdown {
                dimension: dimension.as_str().to_string(),
                dominant_label: dominant.seed.id.to_string(),
                labels: entries
                    .iter()
                    .take(4)
                    .map(|entry| entry.seed.id.to_string())
                    .collect(),
                confidence: (dominant.score / top_score).clamp(0.0, 0.99),
            });
        }
    }
    dimensions
}

fn build_signals(ranked: &[RankedSeed], top_score: f32) -> Vec<TaxonomySignal> {
    ranked
        .iter()
        .filter(|candidate| candidate.score > 0.08)
        .take(24)
        .map(|candidate| {
            let mut reasons = Vec::new();
            reasons.extend(
                candidate
                    .alias_hits
                    .iter()
                    .map(|item| format!("alias:{item}")),
            );
            reasons.extend(
                candidate
                    .term_hits
                    .iter()
                    .map(|item| format!("term:{item}")),
            );
            reasons.extend(
                candidate
                    .path_hits
                    .iter()
                    .map(|item| format!("path:{item}")),
            );
            reasons.extend(
                candidate
                    .context_hits
                    .iter()
                    .map(|item| format!("context:{item}")),
            );
            reasons.sort_unstable();
            reasons.dedup();
            TaxonomySignal {
                label: candidate.seed.id.to_string(),
                dimension: candidate.seed.dimension.as_str().to_string(),
                score: candidate.score,
                confidence: (candidate.score / top_score).clamp(0.0, 0.99),
                reinforcement_weight: 0.0,
                failure_count: 0,
                reinforcement_decay: 1.0,
                reasons,
            }
        })
        .collect()
}

fn infer_relations(ranked: &[RankedSeed]) -> Vec<TaxonomyRelation> {
    let selected = ranked
        .iter()
        .filter(|candidate| candidate.score > 0.12)
        .take(12)
        .collect::<Vec<_>>();
    let mut relations = Vec::new();
    for (left_idx, left) in selected.iter().enumerate() {
        for right in selected.iter().skip(left_idx + 1) {
            if let Some(relation) = relation_for_pair(left.seed, right.seed) {
                let weight = ((left.score + right.score) / 2.0).clamp(0.05, 4.0);
                relations.push(TaxonomyRelation {
                    source: left.seed.id.to_string(),
                    target: right.seed.id.to_string(),
                    relation: relation.to_string(),
                    weight,
                    evidence: format!("{} + {} -> {}", left.seed.name, right.seed.name, relation),
                });
            }
        }
    }
    relations.sort_by(|left, right| {
        right
            .weight
            .partial_cmp(&left.weight)
            .unwrap_or(Ordering::Equal)
    });
    relations.truncate(12);
    relations
}

fn build_multi_labels(
    ranked: &[RankedSeed],
    dimensions: &[DimensionBreakdown],
    relations: &[TaxonomyRelation],
    metadata: &MemoryMetadata,
    emergent_clusters: &[String],
    sensitivity_tags: &[String],
    token_count: usize,
) -> Vec<String> {
    let mut labels = Vec::with_capacity(64);
    for candidate in ranked
        .iter()
        .filter(|candidate| candidate.score > 0.08)
        .take(20)
    {
        labels.push(candidate.seed.id.to_string());
        labels.push(format!(
            "{}:{}",
            candidate.seed.dimension.as_str(),
            candidate.seed.name.to_ascii_lowercase().replace(' ', "-")
        ));
    }
    for dimension in dimensions {
        labels.push(format!(
            "dimension:{}:{}",
            dimension.dimension, dimension.dominant_label
        ));
    }
    for relation in relations.iter().take(8) {
        labels.push(format!(
            "relation:{}:{}:{}",
            relation.source, relation.relation, relation.target
        ));
    }
    labels.extend(emergent_clusters.iter().cloned());
    labels.extend(
        sensitivity_tags
            .iter()
            .map(|tag| format!("sensitive:{tag}")),
    );
    labels.extend(metadata.tags.iter().map(|tag| format!("tag:{tag}")));
    labels.extend(metadata.paths.iter().map(|path| format!("path:{path}")));
    labels.push(
        match token_count {
            0..=20 => "density:sparse",
            21..=80 => "density:balanced",
            _ => "density:dense",
        }
        .to_string(),
    );
    labels.push(
        metadata
            .language
            .as_ref()
            .map(|item| format!("language:{item}"))
            .unwrap_or_else(|| "language:mixed".to_string()),
    );
    labels.push(format!(
        "privacy:{}",
        match metadata.privacy {
            crate::domain::PrivacyLevel::Standard => "standard",
            crate::domain::PrivacyLevel::Internal => "internal",
            crate::domain::PrivacyLevel::Confidential => "confidential",
            crate::domain::PrivacyLevel::Secret => "secret",
        }
    ));
    labels.push(
        if metadata.share_scope {
            "scope:cross-project"
        } else {
            "scope:project-only"
        }
        .to_string(),
    );
    labels
}

fn build_hierarchy(
    ranked: &[RankedSeed],
    dimensions: &[DimensionBreakdown],
    emergent_clusters: &[String],
) -> Vec<HierarchyNode> {
    let mut nodes = Vec::new();
    nodes.push(HierarchyNode {
        id: "memory".to_string(),
        name: "Memory".to_string(),
        level: 0,
        description: "Root taxonomy node for a classified memory.".to_string(),
        priority: 100,
        confidence: 1.0,
        reinforcement_weight: 0.0,
        failure_count: 0,
        reinforcement_decay: 1.0,
        dependencies: Vec::new(),
        relations: Vec::new(),
        node_type: "root".to_string(),
    });

    for dimension in dimensions.iter().take(5) {
        nodes.push(HierarchyNode {
            id: format!("dimension/{}", dimension.dimension),
            name: format!("{} Dimension", title_case(&dimension.dimension)),
            level: 1,
            description: format!("Dominant {} signal in the memory.", dimension.dimension),
            priority: 90,
            confidence: dimension.confidence,
            reinforcement_weight: 0.0,
            failure_count: 0,
            reinforcement_decay: 1.0,
            dependencies: vec!["memory".to_string()],
            relations: vec![dimension.dominant_label.clone()],
            node_type: "dimension".to_string(),
        });
    }

    for candidate in ranked
        .iter()
        .filter(|candidate| candidate.score > 0.1)
        .take(12)
    {
        nodes.push(HierarchyNode {
            id: candidate.seed.id.to_string(),
            name: candidate.seed.name.to_string(),
            level: 2 + dimension_offset(candidate.seed.dimension),
            description: candidate.seed.description.to_string(),
            priority: candidate.seed.base_priority,
            confidence: candidate.score / ranked.first().map(|item| item.score).unwrap_or(1.0),
            reinforcement_weight: 0.0,
            failure_count: 0,
            reinforcement_decay: 1.0,
            dependencies: candidate
                .seed
                .dependencies
                .iter()
                .map(|item| item.to_string())
                .collect(),
            relations: candidate.context_hits.clone(),
            node_type: candidate.seed.dimension.as_str().to_string(),
        });
    }

    for cluster in emergent_clusters.iter().take(4) {
        nodes.push(HierarchyNode {
            id: cluster.clone(),
            name: cluster.trim_start_matches("cluster:").to_string(),
            level: 4,
            description: "Emergent prefix-cluster inferred from repeated project-specific tokens."
                .to_string(),
            priority: 65,
            confidence: 0.55,
            reinforcement_weight: 0.0,
            failure_count: 0,
            reinforcement_decay: 1.0,
            dependencies: vec!["module".to_string(), "path_artifact".to_string()],
            relations: Vec::new(),
            node_type: "cluster".to_string(),
        });
    }

    nodes
}

fn build_debug_view(ranked: &[RankedSeed]) -> TaxonomyDebugView {
    let mut debug = TaxonomyDebugView::default();
    for candidate in ranked.iter().take(12) {
        debug
            .matched_aliases
            .extend(candidate.alias_hits.iter().cloned());
        debug
            .prototype_hits
            .extend(candidate.term_hits.iter().cloned());
        debug.path_hints.extend(candidate.path_hits.iter().cloned());
        debug
            .context_hints
            .extend(candidate.context_hits.iter().cloned());
        if candidate.score > 0.1 {
            debug.derived_markers.push(format!(
                "{}:{}:{:.2}",
                candidate.seed.dimension.as_str(),
                candidate.seed.id,
                candidate.score
            ));
        }
    }
    dedup_vec(&mut debug.matched_aliases);
    dedup_vec(&mut debug.prototype_hits);
    dedup_vec(&mut debug.path_hints);
    dedup_vec(&mut debug.context_hints);
    dedup_vec(&mut debug.derived_markers);
    debug
}

fn sentence_count(content: &str) -> usize {
    content
        .split(['.', '!', '?'])
        .filter(|chunk| !chunk.trim().is_empty())
        .count()
        .max(1)
}

fn compression_hint(token_count: usize, confidence_mean: f32, relation_count: usize) -> String {
    match (token_count, confidence_mean, relation_count) {
        (0..=30, _, _) => "inject-full".to_string(),
        (_, value, _) if value >= 0.82 => "inject-summary-first".to_string(),
        (_, _, count) if count >= 6 => "inject-index-then-relations".to_string(),
        _ => "inject-index-timeline-detail".to_string(),
    }
}

fn inferred_kinds(ranked: &[RankedSeed]) -> Vec<String> {
    ranked
        .iter()
        .filter(|candidate| candidate.seed.dimension == Dimension::Semantic)
        .take(4)
        .map(|candidate| candidate.seed.id.to_string())
        .collect()
}

fn emergent_clusters(tokens: &[String], metadata: &MemoryMetadata) -> Vec<String> {
    let mut trie = TrieNode::default();
    for token in tokens.iter().filter(|token| token.len() >= 4) {
        trie.insert(token);
    }
    for path in &metadata.paths {
        for segment in path.split(['/', '\\']) {
            if segment.len() >= 4 {
                trie.insert(segment);
            }
        }
    }
    let mut output = Vec::new();
    trie.collect_clusters(&mut String::new(), &mut output);
    output.sort_unstable();
    output.dedup();
    output.truncate(10);
    output
}

fn extract_entities(metadata: &MemoryMetadata, tokens: &[String]) -> Vec<String> {
    let mut entities = Vec::new();
    entities.extend(
        metadata
            .paths
            .iter()
            .cloned()
            .map(|path| format!("path:{path}")),
    );
    entities.extend(
        tokens
            .iter()
            .filter(|token| token.contains('/') || token.contains("::") || token.ends_with(".rs"))
            .take(10)
            .cloned()
            .map(|token| format!("artifact:{token}")),
    );
    entities.truncate(16);
    entities
}

fn headline(content: &str, tokens: &[String]) -> String {
    if let Some(line) = content
        .split(['.', '\n'])
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        return line.chars().take(120).collect();
    }
    tokens
        .iter()
        .take(12)
        .cloned()
        .collect::<Vec<_>>()
        .join(" ")
}

fn summary(
    headline: &str,
    dimensions: &[DimensionBreakdown],
    relations: &[TaxonomyRelation],
) -> String {
    let dimension_summary = dimensions
        .iter()
        .take(4)
        .map(|dimension| format!("{}={}", dimension.dimension, dimension.dominant_label))
        .collect::<Vec<_>>()
        .join(", ");
    let relation_summary = relations
        .iter()
        .take(2)
        .map(|relation| {
            format!(
                "{} {} {}",
                relation.source, relation.relation, relation.target
            )
        })
        .collect::<Vec<_>>()
        .join(" | ");
    if relation_summary.is_empty() {
        format!("{headline} | {dimension_summary}")
    } else {
        format!("{headline} | {dimension_summary} | {relation_summary}")
    }
}

fn relation_for_pair(left: &CategorySeed, right: &CategorySeed) -> Option<&'static str> {
    if left.dependencies.contains(&right.id) || right.dependencies.contains(&left.id) {
        return Some("depends_on");
    }
    match (left.id, right.id) {
        ("decision", "architecture") | ("architecture", "decision") => Some("defines"),
        ("incident", "fix") | ("fix", "incident") => Some("resolved_by"),
        ("performance", "benchmark") | ("benchmark", "performance") => Some("validated_by"),
        ("api", "endpoint") | ("endpoint", "api") => Some("exposed_as"),
        ("storage", "database_artifact") | ("database_artifact", "storage") => {
            Some("materialized_as")
        }
        ("retrieval", "query_plan") | ("query_plan", "retrieval") => Some("compiled_into"),
        ("risk", "avoid_pattern") | ("avoid_pattern", "risk") => Some("elevates"),
        ("preference", "decision") | ("decision", "preference") => Some("biases"),
        ("incident", "regression") | ("regression", "incident") => Some("signals"),
        ("ui", "path_artifact") | ("path_artifact", "ui") => Some("rendered_in"),
        ("tooling", "command_artifact") | ("command_artifact", "tooling") => Some("operated_by"),
        ("testing", "test_artifact") | ("test_artifact", "testing") => Some("validated_by"),
        _ if left.dimension == Dimension::Language && right.dimension == Dimension::Artifact => {
            Some("implemented_in")
        }
        _ if left.dimension == Dimension::Semantic && right.dimension == Dimension::Domain => {
            Some("describes")
        }
        _ if left.dimension == Dimension::Lifecycle && right.dimension == Dimension::Activity => {
            Some("modulates")
        }
        _ => None,
    }
}

fn detect_language_hint(tokens: &[String]) -> &'static str {
    if tokens
        .iter()
        .any(|token| token == "cargo" || token == "fn" || token.ends_with(".rs"))
    {
        "rust"
    } else if tokens
        .iter()
        .any(|token| token == "function" || token.ends_with(".ts") || token.ends_with(".tsx"))
    {
        "typescript"
    } else if tokens
        .iter()
        .any(|token| token == "def" || token.ends_with(".py"))
    {
        "python"
    } else if tokens
        .iter()
        .any(|token| token == "select" || token == "from")
    {
        "sql"
    } else if tokens
        .iter()
        .any(|token| token.ends_with(".sh") || token == "bash")
    {
        "shell"
    } else {
        "mixed"
    }
}

fn redact_sensitive(content: &str) -> (String, Vec<SensitiveSpan>, Vec<String>) {
    let mut spans = Vec::new();
    let mut tags = Vec::new();
    for (label, regex) in secret_patterns() {
        for mat in regex.find_iter(content) {
            spans.push(SensitiveSpan {
                start: mat.start(),
                end: mat.end(),
                label: (*label).to_string(),
            });
            tags.push((*label).to_string());
        }
    }
    spans.sort_by_key(|span| span.start);
    spans.dedup_by(|left, right| left.start == right.start && left.end == right.end);
    tags.sort_unstable();
    tags.dedup();
    if spans.is_empty() {
        return (content.to_string(), spans, tags);
    }
    let mut redacted = String::with_capacity(content.len());
    let mut cursor = 0;
    for span in &spans {
        if span.start > cursor {
            redacted.push_str(&content[cursor..span.start]);
        }
        redacted.push_str("[REDACTED:");
        redacted.push_str(&span.label);
        redacted.push(']');
        cursor = span.end;
    }
    if cursor < content.len() {
        redacted.push_str(&content[cursor..]);
    }
    (redacted, spans, tags)
}

fn secret_patterns() -> &'static [(&'static str, Regex)] {
    static PATTERNS: OnceLock<Vec<(&'static str, Regex)>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        vec![
            (
                "api_key",
                Regex::new(r#"(?i)\b(api[-_ ]?key|token|secret|password)\b\s*[:=]\s*["']?[A-Za-z0-9_\-]{10,}["']?"#)
                    .expect("valid regex"),
            ),
            (
                "email",
                Regex::new(r"(?i)\b[A-Z0-9._%+-]+@[A-Z0-9.-]+\.[A-Z]{2,}\b").expect("valid regex"),
            ),
            (
                "pem",
                Regex::new(r"-----BEGIN [A-Z ]+-----").expect("valid regex"),
            ),
            (
                "jwt",
                Regex::new(r"[A-Za-z0-9-_]+\.[A-Za-z0-9-_]+\.[A-Za-z0-9-_]+").expect("valid regex"),
            ),
        ]
    })
}

fn title_case(input: &str) -> String {
    let mut output = String::new();
    for (idx, part) in input.split('_').enumerate() {
        if idx > 0 {
            output.push(' ');
        }
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            output.push(first.to_ascii_uppercase());
            output.extend(chars);
        }
    }
    output
}

fn dimension_offset(dimension: Dimension) -> u8 {
    match dimension {
        Dimension::Semantic | Dimension::Domain => 0,
        Dimension::Activity | Dimension::Artifact => 1,
        Dimension::Lifecycle | Dimension::Privacy | Dimension::Language => 2,
    }
}

fn dedup_vec(values: &mut Vec<String>) {
    values.sort_unstable();
    values.dedup();
}

fn is_stopword(token: &str) -> bool {
    matches!(
        token,
        "the"
            | "and"
            | "for"
            | "with"
            | "from"
            | "that"
            | "this"
            | "into"
            | "were"
            | "when"
            | "then"
            | "there"
            | "have"
            | "will"
            | "your"
            | "about"
            | "after"
            | "before"
            | "because"
            | "while"
            | "been"
            | "their"
            | "they"
            | "them"
            | "only"
            | "very"
            | "just"
            | "over"
            | "under"
            | "onto"
            | "than"
            | "also"
            | "same"
            | "more"
    )
}

fn seeds() -> &'static [CategorySeed] {
    static SEEDS: OnceLock<Vec<CategorySeed>> = OnceLock::new();
    SEEDS.get_or_init(build_seeds).as_slice()
}

fn build_seeds() -> Vec<CategorySeed> {
    let mut seeds = Vec::new();
    seeds.extend(build_semantic_seeds());
    seeds.extend(build_domain_seeds());
    seeds.extend(build_activity_seeds());
    seeds.extend(build_artifact_seeds());
    seeds.extend(build_lifecycle_seeds());
    seeds.extend(build_privacy_seeds());
    seeds.extend(build_language_seeds());
    seeds
}

fn build_semantic_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "fact",
            "Fact",
            Semantic,
            "Objective project knowledge that should be recalled as ground truth.",
            ["fact", "confirmed", "ground truth", "known"],
            ["fact", "confirmed", "known", "ground", "truth"],
            [],
            90
        ),
        seed!(
            "preference",
            "Preference",
            Semantic,
            "Stable taste, preference, or style guidance from a user or team.",
            ["prefer", "preference", "like", "want"],
            ["prefer", "preference", "style", "like", "want"],
            [],
            92
        ),
        seed!(
            "event",
            "Event",
            Semantic,
            "A project event, occurrence, or milestone with temporal meaning.",
            ["event", "happened", "landed", "rolled out"],
            ["event", "happened", "rolled", "landed", "milestone"],
            [],
            83
        ),
        seed!(
            "decision",
            "Decision",
            Semantic,
            "An explicit decision that shapes implementation or future behavior.",
            ["decision", "decide", "chose", "settled"],
            ["decision", "chose", "selected", "tradeoff", "reason"],
            [],
            96
        ),
        seed!(
            "risk",
            "Risk",
            Semantic,
            "A danger, downside, or future failure mode worth remembering.",
            ["risk", "danger", "hazard", "concern"],
            ["risk", "danger", "hazard", "concern", "failure"],
            [],
            95
        ),
        seed!(
            "constraint",
            "Constraint",
            Semantic,
            "A hard limit, invariant, or non-negotiable boundary.",
            ["constraint", "must", "cannot", "requirement"],
            ["constraint", "must", "cannot", "invariant", "requirement"],
            [],
            94
        ),
        seed!(
            "incident",
            "Incident",
            Semantic,
            "A failure or outage that should remain highly visible.",
            ["incident", "outage", "failure", "broke"],
            ["incident", "outage", "failure", "broke", "broken"],
            [],
            95
        ),
        seed!(
            "learned_pattern",
            "Learned Pattern",
            Semantic,
            "A reusable lesson or pattern extracted from outcomes.",
            ["learned", "lesson", "pattern", "remember"],
            ["learned", "lesson", "pattern", "repeat", "remember"],
            [],
            94
        ),
        seed!(
            "instruction",
            "Instruction",
            Semantic,
            "An instruction the agent should keep following over time.",
            ["instruction", "always", "never", "remember to"],
            ["instruction", "always", "never", "remember", "rule"],
            [],
            93
        ),
    ]
}

fn build_domain_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "architecture",
            "Architecture",
            Domain,
            "Cross-cutting design boundaries, structure, and trade-offs.",
            ["architecture", "design", "boundary", "modular"],
            ["architecture", "design", "system", "boundary", "module"],
            [],
            98
        ),
        seed!(
            "storage",
            "Storage",
            Domain,
            "Persistence, indexing, state management, and data retention.",
            ["sqlite", "database", "storage", "persist", "index"],
            ["sqlite", "database", "storage", "persist", "index"],
            ["architecture"],
            96
        ),
        seed!(
            "retrieval",
            "Retrieval",
            Domain,
            "Recall, ranking, search, and context assembly behavior.",
            ["retrieval", "search", "ranking", "bm25", "query"],
            ["retrieval", "search", "ranking", "bm25", "query"],
            ["storage"],
            97
        ),
        seed!(
            "performance",
            "Performance",
            Domain,
            "Latency, throughput, allocation, and scaling concerns.",
            [
                "performance",
                "latency",
                "throughput",
                "optimize",
                "hot path"
            ],
            [
                "performance",
                "latency",
                "throughput",
                "optimize",
                "allocation"
            ],
            ["architecture"],
            97
        ),
        seed!(
            "security",
            "Security",
            Domain,
            "Secrets, privacy, attack surface, and trust boundaries.",
            ["security", "secret", "credential", "privacy", "sensitive"],
            ["security", "secret", "credential", "privacy", "sensitive"],
            ["architecture"],
            97
        ),
        seed!(
            "api",
            "API",
            Domain,
            "External interfaces, MCP contracts, HTTP handlers, and protocols.",
            ["api", "endpoint", "handler", "mcp", "protocol"],
            ["api", "endpoint", "handler", "mcp", "protocol"],
            ["architecture"],
            94
        ),
        seed!(
            "ui",
            "UI",
            Domain,
            "Operator-facing flows, layout, interaction, and presentation surfaces.",
            ["ui", "dashboard", "interface", "layout", "virtualized"],
            ["ui", "dashboard", "interface", "layout", "virtualized"],
            ["api"],
            91
        ),
        seed!(
            "tooling",
            "Tooling",
            Domain,
            "CLI, automation, packaging, and operator workflow.",
            ["cli", "tooling", "docker", "command", "release"],
            ["cli", "tooling", "docker", "command", "release"],
            ["architecture"],
            90
        ),
        seed!(
            "testing",
            "Testing",
            Domain,
            "Verification, regression coverage, correctness, and safety rails.",
            ["test", "testing", "assert", "coverage", "verify"],
            ["test", "testing", "assert", "coverage", "verify"],
            ["performance"],
            92
        ),
        seed!(
            "deployment",
            "Deployment",
            Domain,
            "Shipping, runtime environment, and binary distribution behavior.",
            ["deploy", "deployment", "release", "binary", "runtime"],
            ["deploy", "deployment", "release", "binary", "runtime"],
            ["tooling"],
            88
        ),
        seed!(
            "collaboration",
            "Collaboration",
            Domain,
            "Shared state, multi-agent coordination, and handoff behavior.",
            ["collaboration", "handoff", "shared brain", "multi-agent"],
            [
                "collaboration",
                "handoff",
                "shared",
                "coordination",
                "agent"
            ],
            ["architecture"],
            93
        ),
        seed!(
            "documentation",
            "Documentation",
            Domain,
            "Human-facing docs, usage guidance, and explainability.",
            ["readme", "documentation", "docs", "guide"],
            ["readme", "documentation", "docs", "guide", "manual"],
            ["tooling"],
            85
        ),
    ]
}

fn build_activity_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "implement",
            "Implement",
            Activity,
            "Concrete construction or feature delivery work.",
            ["implement", "build", "create", "write"],
            ["implement", "build", "create", "write", "ship"],
            ["decision"],
            88
        ),
        seed!(
            "fix",
            "Fix",
            Activity,
            "A corrective action that resolves a defect or mismatch.",
            ["fix", "resolve", "repair", "correct"],
            ["fix", "resolve", "repair", "correct", "patch"],
            ["incident"],
            95
        ),
        seed!(
            "benchmark",
            "Benchmark",
            Activity,
            "Measured validation of speed, efficiency, or resource use.",
            ["benchmark", "measure", "p95", "latency", "throughput"],
            ["benchmark", "measure", "p95", "latency", "throughput"],
            ["performance"],
            90
        ),
        seed!(
            "refactor",
            "Refactor",
            Activity,
            "Structural reorganization without changing product intent.",
            ["refactor", "reshape", "reorganize", "rename"],
            ["refactor", "reshape", "reorganize", "rename", "cleanup"],
            ["architecture"],
            86
        ),
        seed!(
            "investigate",
            "Investigate",
            Activity,
            "Diagnosis, research, and root-cause analysis work.",
            ["investigate", "diagnose", "debug", "trace"],
            ["investigate", "diagnose", "debug", "trace", "research"],
            ["risk"],
            91
        ),
        seed!(
            "review",
            "Review",
            Activity,
            "Review, audit, or scrutiny of work that already exists.",
            ["review", "audit", "inspect", "check"],
            ["review", "audit", "inspect", "check", "verify"],
            ["testing"],
            84
        ),
        seed!(
            "reflect",
            "Reflect",
            Activity,
            "Retrospective learning and reinforcement behavior.",
            ["reflect", "retrospective", "lesson", "learned"],
            ["reflect", "retrospective", "lesson", "learned", "memory"],
            ["learned_pattern"],
            92
        ),
        seed!(
            "plan",
            "Plan",
            Activity,
            "Sequencing, future work, and upcoming execution intent.",
            ["plan", "next step", "todo", "roadmap"],
            ["plan", "next", "todo", "roadmap", "later"],
            ["constraint"],
            82
        ),
        seed!(
            "migrate",
            "Migrate",
            Activity,
            "Moving a system, schema, or workflow across states.",
            ["migrate", "migration", "port", "move"],
            ["migrate", "migration", "port", "move", "upgrade"],
            ["storage"],
            84
        ),
    ]
}

fn build_artifact_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "module",
            "Module",
            Artifact,
            "A code module, crate, or structural namespace.",
            ["module", "crate", "package", "mod.rs"],
            ["module", "crate", "package", "namespace", "library"],
            ["architecture"],
            88
        ),
        seed!(
            "database_artifact",
            "Database Artifact",
            Artifact,
            "Tables, schemas, rows, or concrete persistence layouts.",
            ["table", "schema", "index", "row", "sqlite"],
            ["table", "schema", "index", "row", "sqlite"],
            ["storage"],
            91
        ),
        seed!(
            "endpoint",
            "Endpoint",
            Artifact,
            "An externally callable route, handler, or RPC surface.",
            ["endpoint", "route", "json-rpc", "method", "handler"],
            ["endpoint", "route", "rpc", "method", "handler"],
            ["api"],
            89
        ),
        seed!(
            "path_artifact",
            "Path Artifact",
            Artifact,
            "A file path, source file, or workspace location.",
            ["src/", ".rs", "cargo.toml", "readme", "dockerfile"],
            ["src", "file", "path", "cargo.toml", "dockerfile"],
            ["module"],
            86
        ),
        seed!(
            "config_artifact",
            "Config Artifact",
            Artifact,
            "Configuration files, knobs, and environment surfaces.",
            ["config", "env", "setting", "toml", "yaml"],
            ["config", "env", "setting", "toml", "yaml"],
            ["tooling"],
            87
        ),
        seed!(
            "command_artifact",
            "Command Artifact",
            Artifact,
            "A shell command, CLI invocation, or operator action.",
            ["command", "cargo run", "docker compose", "cli"],
            ["command", "cargo", "docker", "cli", "run"],
            ["tooling"],
            87
        ),
        seed!(
            "query_plan",
            "Query Plan",
            Artifact,
            "An index, postings list, scorer, or search data structure.",
            ["posting", "bm25", "idf", "query", "rank"],
            ["posting", "bm25", "idf", "query", "rank"],
            ["retrieval"],
            92
        ),
        seed!(
            "test_artifact",
            "Test Artifact",
            Artifact,
            "Assertions, fixtures, cases, or coverage scaffolding.",
            ["test", "assert", "fixture", "case"],
            ["test", "assert", "fixture", "case", "coverage"],
            ["testing"],
            88
        ),
        seed!(
            "prompt_artifact",
            "Prompt Artifact",
            Artifact,
            "Instructions, prompt files, or persistent agent guidance.",
            ["memory.md", "prompt", "instruction", ".md"],
            ["prompt", "instruction", "memory.md", "markdown"],
            ["documentation"],
            84
        ),
    ]
}

fn build_lifecycle_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "recent",
            "Recent",
            Lifecycle,
            "Fresh work that should be surfaced aggressively.",
            ["recent", "latest", "today", "just landed"],
            ["recent", "latest", "today", "fresh"],
            ["event"],
            79
        ),
        seed!(
            "stable",
            "Stable",
            Lifecycle,
            "A trusted, reusable pattern with good prior outcomes.",
            ["stable", "proven", "reliable", "repeat"],
            ["stable", "proven", "reliable", "repeat"],
            ["learned_pattern"],
            90
        ),
        seed!(
            "avoid_pattern",
            "Avoid Pattern",
            Lifecycle,
            "A learned failure signature to steer future agents away from.",
            ["avoid", "never again", "pitfall", "mistake"],
            ["avoid", "pitfall", "mistake", "never"],
            ["risk"],
            98
        ),
        seed!(
            "blocked",
            "Blocked",
            Lifecycle,
            "Work currently waiting on missing prerequisites.",
            ["blocked", "waiting", "cannot proceed", "depends on"],
            ["blocked", "waiting", "dependency", "pending"],
            ["constraint"],
            82
        ),
        seed!(
            "planned",
            "Planned",
            Lifecycle,
            "Intended follow-up that has not landed yet.",
            ["planned", "future", "next", "todo"],
            ["planned", "future", "next", "todo"],
            ["plan"],
            76
        ),
        seed!(
            "regression",
            "Regression",
            Lifecycle,
            "A known recurrence of a previously solved issue.",
            ["regression", "broke again", "came back"],
            ["regression", "again", "repeat", "broke"],
            ["incident"],
            97
        ),
        seed!(
            "reinforced",
            "Reinforced",
            Lifecycle,
            "A pattern strengthened by successful outcomes.",
            ["reinforced", "worked well", "keep this"],
            ["reinforced", "worked", "keep", "success"],
            ["stable"],
            88
        ),
        seed!(
            "cross_project",
            "Cross Project",
            Lifecycle,
            "Knowledge that is safe to share across project containers.",
            ["shared", "cross project", "global", "portable"],
            ["shared", "cross", "global", "portable"],
            ["collaboration"],
            78
        ),
        seed!(
            "deprecated",
            "Deprecated",
            Lifecycle,
            "An old pattern that should fade from future plans.",
            ["deprecated", "old", "legacy", "retire"],
            ["deprecated", "old", "legacy", "retire"],
            ["risk"],
            80
        ),
    ]
}

fn build_privacy_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "private",
            "Private",
            Privacy,
            "Contains private project or user information.",
            ["private", "internal", "sensitive", "user"],
            ["private", "internal", "sensitive", "user"],
            ["security"],
            96
        ),
        seed!(
            "secret",
            "Secret",
            Privacy,
            "Contains credentials or security material that must be redacted.",
            ["secret", "token", "password", "api_key"],
            ["secret", "token", "password", "api_key"],
            ["security"],
            99
        ),
        seed!(
            "pii",
            "PII",
            Privacy,
            "Contains email, identity, or directly identifying information.",
            ["email", "personally identifiable", "pii", "user data"],
            ["email", "pii", "identity", "user"],
            ["security"],
            97
        ),
    ]
}

fn build_language_seeds() -> Vec<CategorySeed> {
    vec![
        seed!(
            "rust",
            "Rust",
            Language,
            "Rust implementation or build context.",
            ["cargo", ".rs", "rust", "fn"],
            ["cargo", ".rs", "rust", "fn"],
            ["module"],
            91
        ),
        seed!(
            "typescript",
            "TypeScript",
            Language,
            "TypeScript or TSX implementation context.",
            [".ts", ".tsx", "typescript", "npm"],
            [".ts", ".tsx", "typescript", "node"],
            ["module"],
            86
        ),
        seed!(
            "python",
            "Python",
            Language,
            "Python implementation or automation context.",
            [".py", "python", "pip", "def"],
            [".py", "python", "pip", "def"],
            ["module"],
            84
        ),
        seed!(
            "sql",
            "SQL",
            Language,
            "SQL query or relational schema context.",
            ["select", "from", "where", "sql"],
            ["select", "from", "where", "sql"],
            ["database_artifact"],
            85
        ),
        seed!(
            "shell",
            "Shell",
            Language,
            "Shell scripting or terminal workflow context.",
            ["bash", ".sh", "powershell", "pwsh"],
            ["bash", ".sh", "powershell", "pwsh"],
            ["command_artifact"],
            83
        ),
        seed!(
            "json",
            "JSON",
            Language,
            "JSON or structured configuration context.",
            [".json", "json", "schema"],
            [".json", "json", "schema"],
            ["config_artifact"],
            82
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::{TaxonomyEngine, TaxonomyEvolutionSnapshot};
    use crate::domain::MemoryMetadata;

    #[test]
    fn decomposes_into_dimensions_and_relations() {
        let engine = TaxonomyEngine::new();
        let (sanitized, decomposition) = engine.decompose(
            "We decided to store project state in SQLite, expose it via MCP HTTP, and benchmark BM25 retrieval latency.",
            &MemoryMetadata::default(),
        );
        assert!(!sanitized.is_empty());
        assert!(decomposition.multi_labels.len() >= 20);
        assert!(decomposition.dimensions.len() >= 3);
        assert!(!decomposition.signals.is_empty());
        assert!(decomposition.relations.iter().any(|relation| {
            relation.relation == "depends_on" || relation.relation == "validated_by"
        }));
    }

    #[test]
    fn uses_evolution_snapshot_to_reinforce_existing_project_lexicon() {
        let engine = TaxonomyEngine::new();
        let snapshot = TaxonomyEvolutionSnapshot {
            prior_labels: vec!["collaboration".to_string()],
            reinforced_labels: vec!["cross_project".to_string()],
            solidified_priors: vec!["architecture".to_string()],
            avoid_patterns: vec!["regression".to_string()],
            project_terms: vec!["shared".to_string(), "brain".to_string()],
        };
        let (_, decomposition) = engine.decompose_with_context(
            "We need a shared brain that avoids another regression across agents.",
            &MemoryMetadata::default(),
            &snapshot,
        );
        assert!(
            decomposition
                .multi_labels
                .iter()
                .any(|label| label.contains("collaboration"))
        );
        assert!(
            decomposition
                .debug
                .context_hints
                .iter()
                .any(|hint| hint.contains("project-prior") || hint.contains("project-lexicon"))
        );
    }

    #[test]
    fn redacts_secret_material() {
        let engine = TaxonomyEngine::new();
        let (sanitized, decomposition) = engine.decompose(
            "api_key = sk_test_123456789000 secret",
            &MemoryMetadata::default(),
        );
        assert!(sanitized.contains("[REDACTED"));
        assert!(!decomposition.metadata.redactions.is_empty());
    }
}
