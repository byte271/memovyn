use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use time::OffsetDateTime;

use crate::domain::{
    AnalyticsBucket, AnalyticsSnapshot, ExportBundle, FeedbackOutcome, LearningState, MemoryRecord,
    MemoryVersionSnapshot, ProjectCard, RankedMemoryStat, TaxonomyDecomposition,
};
use crate::error::{MemovynError, Result};

#[derive(Debug)]
pub struct Database {
    connection: Mutex<Connection>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        let connection = Connection::open(path)?;
        connection.execute_batch(
            r#"
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA temp_store = MEMORY;
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS projects (
                project_id TEXT PRIMARY KEY,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                share_scope INTEGER NOT NULL DEFAULT 0,
                total_token_savings INTEGER NOT NULL DEFAULT 0,
                total_queries INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS memories (
                row_id INTEGER PRIMARY KEY AUTOINCREMENT,
                memory_id TEXT NOT NULL UNIQUE,
                project_id TEXT NOT NULL,
                kind TEXT NOT NULL,
                headline TEXT NOT NULL,
                summary TEXT NOT NULL,
                content TEXT NOT NULL,
                content_hash TEXT NOT NULL,
                taxonomy_json TEXT NOT NULL,
                metadata_json TEXT NOT NULL,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                last_accessed_at TEXT NOT NULL,
                reinforcement REAL NOT NULL DEFAULT 0,
                penalty REAL NOT NULL DEFAULT 0,
                success_score REAL NOT NULL DEFAULT 0,
                failure_count INTEGER NOT NULL DEFAULT 0,
                repeated_mistake_count INTEGER NOT NULL DEFAULT 0,
                reinforcement_decay REAL NOT NULL DEFAULT 1,
                conflict_score REAL NOT NULL DEFAULT 0,
                last_feedback_at TEXT,
                access_count INTEGER NOT NULL DEFAULT 0,
                version INTEGER NOT NULL DEFAULT 1,
                FOREIGN KEY(project_id) REFERENCES projects(project_id)
            );

            CREATE INDEX IF NOT EXISTS idx_memories_project_created
                ON memories(project_id, created_at DESC);
            CREATE INDEX IF NOT EXISTS idx_memories_project_kind
                ON memories(project_id, kind);
            CREATE INDEX IF NOT EXISTS idx_memories_project_weight
                ON memories(project_id, reinforcement DESC, penalty ASC, success_score DESC);

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
                reinforcement_delta REAL NOT NULL DEFAULT 0,
                penalty_delta REAL NOT NULL DEFAULT 0,
                note TEXT,
                avoid_patterns_json TEXT NOT NULL DEFAULT '[]',
                created_at TEXT NOT NULL
            );
            "#,
        )?;
        ensure_memory_columns(&connection)?;
        Ok(Self {
            connection: Mutex::new(connection),
        })
    }

    pub fn upsert_project(&self, project_id: &str, share_scope: bool) -> Result<()> {
        let now = now_string();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            r#"
            INSERT INTO projects(project_id, created_at, updated_at, share_scope)
            VALUES (?1, ?2, ?2, ?3)
            ON CONFLICT(project_id) DO UPDATE SET
                updated_at = excluded.updated_at,
                share_scope = excluded.share_scope
            "#,
            params![project_id, now, share_scope as i64],
        )?;
        Ok(())
    }

    pub fn insert_memory(&self, memory: &MemoryRecord) -> Result<()> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        insert_memory_tx(&transaction, memory)?;
        transaction.commit()?;
        Ok(())
    }

    pub fn load_all_memories(&self) -> Result<Vec<MemoryRecord>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare_cached(
            r#"
            SELECT
                memory_id, project_id, kind, headline, summary, content, content_hash,
                taxonomy_json, metadata_json, created_at, updated_at, last_accessed_at,
                reinforcement, penalty, success_score, failure_count, repeated_mistake_count,
                reinforcement_decay, conflict_score, last_feedback_at, access_count, version
            FROM memories
            ORDER BY created_at ASC
            "#,
        )?;

        let rows = statement.query_map([], decode_memory_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemovynError::from)
    }

    pub fn record_recall(
        &self,
        memory_id: uuid::Uuid,
        query: &str,
        tokens_saved: usize,
    ) -> Result<()> {
        let now = now_string();
        let connection = self.connection.lock().expect("database mutex poisoned");
        connection.execute(
            "INSERT INTO recollections(memory_id, query, recalled_at, tokens_saved) VALUES (?1, ?2, ?3, ?4)",
            params![memory_id.to_string(), query, now.clone(), tokens_saved as i64],
        )?;
        connection.execute(
            r#"
            UPDATE memories
            SET access_count = access_count + 1,
                last_accessed_at = ?2
            WHERE memory_id = ?1
            "#,
            params![memory_id.to_string(), now.clone()],
        )?;
        connection.execute(
            r#"
            UPDATE projects
            SET total_queries = total_queries + 1,
                total_token_savings = total_token_savings + ?2,
                updated_at = ?3
            WHERE project_id = (
                SELECT project_id FROM memories WHERE memory_id = ?1 LIMIT 1
            )
            "#,
            params![memory_id.to_string(), tokens_saved as i64, now],
        )?;
        Ok(())
    }

    pub fn feedback_memory(
        &self,
        memory_id: uuid::Uuid,
        outcome: FeedbackOutcome,
        repeated_mistake: bool,
        weight: f32,
        activity_score: f32,
        avoid_patterns: &[String],
        note: Option<&str>,
    ) -> Result<Option<MemoryRecord>> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        let Some(mut memory) = load_memory_tx(&transaction, memory_id)? else {
            return Ok(None);
        };

        let now = OffsetDateTime::now_utc();
        let (reinforcement_delta, penalty_delta, success_delta, failure_increment, conflict_delta) =
            feedback_deltas(outcome, repeated_mistake, weight, activity_score);

        memory.reinforcement += reinforcement_delta;
        memory.penalty += penalty_delta;
        memory.learning.success_score += success_delta;
        memory.learning.failure_count += failure_increment;
        memory.learning.repeated_mistake_count += u32::from(repeated_mistake);
        memory.learning.conflict_score += conflict_delta;
        memory.learning.last_feedback_at = Some(now);
        memory.learning.reinforcement_decay =
            decay_after_feedback(memory.learning.reinforcement_decay, outcome, activity_score);
        memory.updated_at = now;
        memory.version += 1;

        if repeated_mistake {
            memory
                .taxonomy
                .avoid_patterns
                .push("repeated_mistake".to_string());
        }
        memory
            .taxonomy
            .avoid_patterns
            .extend(avoid_patterns.iter().cloned());
        memory.taxonomy.avoid_patterns.sort_unstable();
        memory.taxonomy.avoid_patterns.dedup();

        apply_learning_to_taxonomy(&mut memory.taxonomy, &memory.learning);
        update_memory_tx(&transaction, &memory)?;
        insert_memory_version_tx(&transaction, &memory)?;
        insert_feedback_event_tx(
            &transaction,
            &memory,
            outcome,
            repeated_mistake,
            reinforcement_delta,
            penalty_delta,
            avoid_patterns,
            note,
        )?;
        transaction.commit()?;
        Ok(Some(memory))
    }

    pub fn archive_memory(&self, memory_id: uuid::Uuid) -> Result<Option<MemoryRecord>> {
        let mut connection = self.connection.lock().expect("database mutex poisoned");
        let transaction = connection.transaction()?;
        let Some(mut memory) = load_memory_tx(&transaction, memory_id)? else {
            return Ok(None);
        };
        memory
            .metadata
            .extra
            .insert("archived".to_string(), "true".to_string());
        memory.updated_at = OffsetDateTime::now_utc();
        memory.version += 1;
        update_memory_tx(&transaction, &memory)?;
        insert_memory_version_tx(&transaction, &memory)?;
        transaction.commit()?;
        Ok(Some(memory))
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectCard>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare_cached(
            r#"
            SELECT
                p.project_id,
                p.share_scope,
                p.total_token_savings,
                MAX(m.updated_at) AS last_updated_at,
                COUNT(m.memory_id) AS memory_count,
                SUM(CASE WHEN m.conflict_score > 0.0 OR m.penalty > m.reinforcement THEN 1 ELSE 0 END) AS conflict_count
            FROM projects p
            LEFT JOIN memories m ON m.project_id = p.project_id
            GROUP BY p.project_id, p.share_scope, p.total_token_savings
            ORDER BY last_updated_at DESC
            "#,
        )?;

        let rows = statement.query_map([], |row| {
            let last_updated_at: Option<String> = row.get(3)?;
            Ok(ProjectCard {
                project_id: row.get(0)?,
                share_scope: row.get::<_, i64>(1)? != 0,
                total_token_savings: row.get::<_, i64>(2)? as u64,
                last_updated_at: last_updated_at.and_then(parse_time_opt),
                memory_count: row.get::<_, i64>(4)? as usize,
                conflict_count: row.get::<_, Option<i64>>(5)?.unwrap_or(0) as usize,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemovynError::from)
    }

    pub fn list_shared_projects(&self, exclude_project_id: &str) -> Result<Vec<String>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare_cached(
            "SELECT project_id FROM projects WHERE share_scope = 1 AND project_id != ?1 ORDER BY project_id ASC",
        )?;
        let rows = statement.query_map(params![exclude_project_id], |row| row.get(0))?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemovynError::from)
    }

    pub fn analytics(&self, project_id: &str) -> Result<AnalyticsSnapshot> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let summary = connection
            .query_row(
                r#"
                SELECT
                    COUNT(*) AS total_memories,
                    COALESCE((SELECT total_queries FROM projects WHERE project_id = ?1), 0),
                    COALESCE((SELECT total_token_savings FROM projects WHERE project_id = ?1), 0),
                    SUM(CASE WHEN conflict_score > 0.0 OR penalty > reinforcement THEN 1 ELSE 0 END) AS conflict_count,
                    SUM(CASE WHEN reinforcement > penalty THEN 1 ELSE 0 END) AS reinforced_memories,
                    SUM(CASE WHEN penalty > 0 THEN 1 ELSE 0 END) AS penalized_memories
                FROM memories
                WHERE project_id = ?1
                "#,
                params![project_id],
                |row| {
                    Ok((
                        row.get::<_, i64>(0)?,
                        row.get::<_, i64>(1)?,
                        row.get::<_, i64>(2)?,
                        row.get::<_, Option<i64>>(3)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(4)?.unwrap_or(0),
                        row.get::<_, Option<i64>>(5)?.unwrap_or(0),
                    ))
                },
            )
            .optional()?
            .unwrap_or((0, 0, 0, 0, 0, 0));

        Ok(AnalyticsSnapshot {
            project_id: project_id.to_string(),
            total_memories: summary.0 as usize,
            total_queries: summary.1 as u64,
            total_token_savings: summary.2 as u64,
            session_queries: 0,
            session_token_savings: 0,
            conflict_count: summary.3 as usize,
            reinforced_memories: summary.4 as usize,
            penalized_memories: summary.5 as usize,
            most_recalled: ranked_memories_query(
                &connection,
                project_id,
                r#"
                SELECT memory_id, headline, summary, access_count, reinforcement, success_score, failure_count
                FROM memories
                WHERE project_id = ?1
                ORDER BY access_count DESC, success_score DESC, updated_at DESC
                LIMIT 50
                "#,
            )?,
            most_reinforced: ranked_memories_query(
                &connection,
                project_id,
                r#"
                SELECT memory_id, headline, summary, access_count, reinforcement, success_score, failure_count
                FROM memories
                WHERE project_id = ?1
                ORDER BY reinforcement DESC, success_score DESC, updated_at DESC
                LIMIT 12
                "#,
            )?,
            most_punished: ranked_memories_query(
                &connection,
                project_id,
                r#"
                SELECT memory_id, headline, summary, access_count, penalty, success_score, failure_count
                FROM memories
                WHERE project_id = ?1
                ORDER BY penalty DESC, failure_count DESC, updated_at DESC
                LIMIT 12
                "#,
            )?,
            label_hotspots: Vec::new(),
            relation_hotspots: Vec::new(),
            conflict_heatmap: load_conflict_heatmap(&connection, project_id)?,
            growth: load_growth_series(&connection, project_id)?,
            behavior_insights: Vec::new(),
        })
    }

    pub fn project_activity(&self, project_id: &str) -> Result<(usize, u64)> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let snapshot = connection.query_row(
            r#"
            SELECT
                COUNT(*) AS total_memories,
                COALESCE((SELECT total_queries FROM projects WHERE project_id = ?1), 0)
            FROM memories
            WHERE project_id = ?1
            "#,
            params![project_id],
            |row| Ok((row.get::<_, i64>(0)? as usize, row.get::<_, i64>(1)? as u64)),
        )?;
        Ok(snapshot)
    }

    pub fn get_memory(&self, memory_id: uuid::Uuid) -> Result<Option<MemoryRecord>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        load_memory_prepared(&connection, memory_id)
    }

    pub fn memory_versions(&self, memory_id: uuid::Uuid) -> Result<Vec<MemoryVersionSnapshot>> {
        let connection = self.connection.lock().expect("database mutex poisoned");
        let mut statement = connection.prepare_cached(
            r#"
            SELECT snapshot_json
            FROM memory_versions
            WHERE memory_id = ?1
            ORDER BY version ASC
            "#,
        )?;
        let rows = statement.query_map(params![memory_id.to_string()], |row| {
            let snapshot_json: String = row.get(0)?;
            let memory: MemoryRecord =
                serde_json::from_str(&snapshot_json).map_err(sqlite_serde_err)?;
            Ok(MemoryVersionSnapshot {
                version: memory.version,
                created_at: memory.updated_at,
                headline: memory.headline,
                reinforcement: memory.reinforcement,
                penalty: memory.penalty,
            })
        })?;

        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(MemovynError::from)
    }

    pub fn export_project(&self, project_id: &str) -> Result<ExportBundle> {
        let memories = self
            .load_all_memories()?
            .into_iter()
            .filter(|memory| memory.project_id == project_id)
            .collect();
        Ok(ExportBundle {
            exported_at: OffsetDateTime::now_utc(),
            memories,
        })
    }
}

fn insert_memory_tx(transaction: &Transaction<'_>, memory: &MemoryRecord) -> Result<()> {
    let taxonomy_json = serde_json::to_string(&memory.taxonomy)?;
    let metadata_json = serde_json::to_string(&memory.metadata)?;
    let kind_json = serde_json::to_string(&memory.kind)?;
    transaction.execute(
        r#"
        INSERT INTO memories(
            memory_id, project_id, kind, headline, summary, content, content_hash,
            taxonomy_json, metadata_json, created_at, updated_at, last_accessed_at,
            reinforcement, penalty, success_score, failure_count, repeated_mistake_count,
            reinforcement_decay, conflict_score, last_feedback_at, access_count, version
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22)
        "#,
        params![
            memory.id.to_string(),
            memory.project_id,
            kind_json,
            memory.headline,
            memory.summary,
            memory.content,
            memory.content_hash,
            taxonomy_json,
            metadata_json,
            format_time(memory.created_at),
            format_time(memory.updated_at),
            format_time(memory.last_accessed_at),
            memory.reinforcement,
            memory.penalty,
            memory.learning.success_score,
            memory.learning.failure_count as i64,
            memory.learning.repeated_mistake_count as i64,
            memory.learning.reinforcement_decay,
            memory.learning.conflict_score,
            memory.learning.last_feedback_at.map(format_time),
            memory.access_count as i64,
            memory.version as i64,
        ],
    )?;
    insert_memory_version_tx(transaction, memory)?;
    Ok(())
}

fn update_memory_tx(transaction: &Transaction<'_>, memory: &MemoryRecord) -> Result<()> {
    let taxonomy_json = serde_json::to_string(&memory.taxonomy)?;
    let metadata_json = serde_json::to_string(&memory.metadata)?;
    let kind_json = serde_json::to_string(&memory.kind)?;
    transaction.execute(
        r#"
        UPDATE memories
        SET project_id = ?2,
            kind = ?3,
            headline = ?4,
            summary = ?5,
            content = ?6,
            content_hash = ?7,
            taxonomy_json = ?8,
            metadata_json = ?9,
            created_at = ?10,
            updated_at = ?11,
            last_accessed_at = ?12,
            reinforcement = ?13,
            penalty = ?14,
            success_score = ?15,
            failure_count = ?16,
            repeated_mistake_count = ?17,
            reinforcement_decay = ?18,
            conflict_score = ?19,
            last_feedback_at = ?20,
            access_count = ?21,
            version = ?22
        WHERE memory_id = ?1
        "#,
        params![
            memory.id.to_string(),
            memory.project_id,
            kind_json,
            memory.headline,
            memory.summary,
            memory.content,
            memory.content_hash,
            taxonomy_json,
            metadata_json,
            format_time(memory.created_at),
            format_time(memory.updated_at),
            format_time(memory.last_accessed_at),
            memory.reinforcement,
            memory.penalty,
            memory.learning.success_score,
            memory.learning.failure_count as i64,
            memory.learning.repeated_mistake_count as i64,
            memory.learning.reinforcement_decay,
            memory.learning.conflict_score,
            memory.learning.last_feedback_at.map(format_time),
            memory.access_count as i64,
            memory.version as i64,
        ],
    )?;
    Ok(())
}

fn insert_memory_version_tx(transaction: &Transaction<'_>, memory: &MemoryRecord) -> Result<()> {
    transaction.execute(
        "INSERT OR REPLACE INTO memory_versions(memory_id, version, snapshot_json, created_at) VALUES (?1, ?2, ?3, ?4)",
        params![
            memory.id.to_string(),
            memory.version as i64,
            serde_json::to_string(memory)?,
            format_time(memory.updated_at)
        ],
    )?;
    Ok(())
}

fn insert_feedback_event_tx(
    transaction: &Transaction<'_>,
    memory: &MemoryRecord,
    outcome: FeedbackOutcome,
    repeated_mistake: bool,
    reinforcement_delta: f32,
    penalty_delta: f32,
    avoid_patterns: &[String],
    note: Option<&str>,
) -> Result<()> {
    transaction.execute(
        r#"
        INSERT INTO feedback_events(
            memory_id, project_id, outcome, repeated_mistake, reinforcement_delta, penalty_delta,
            note, avoid_patterns_json, created_at
        )
        VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
        "#,
        params![
            memory.id.to_string(),
            memory.project_id,
            format!("{:?}", outcome).to_ascii_lowercase(),
            repeated_mistake as i64,
            reinforcement_delta,
            penalty_delta,
            note,
            serde_json::to_string(avoid_patterns)?,
            format_time(memory.updated_at),
        ],
    )?;
    Ok(())
}

fn load_memory_prepared(
    connection: &Connection,
    memory_id: uuid::Uuid,
) -> Result<Option<MemoryRecord>> {
    let mut statement = connection.prepare_cached(
        r#"
        SELECT
            memory_id, project_id, kind, headline, summary, content, content_hash,
            taxonomy_json, metadata_json, created_at, updated_at, last_accessed_at,
            reinforcement, penalty, success_score, failure_count, repeated_mistake_count,
            reinforcement_decay, conflict_score, last_feedback_at, access_count, version
        FROM memories
        WHERE memory_id = ?1
        "#,
    )?;
    statement
        .query_row(params![memory_id.to_string()], decode_memory_row)
        .optional()
        .map_err(MemovynError::from)
}

fn load_memory_tx(
    transaction: &Transaction<'_>,
    memory_id: uuid::Uuid,
) -> Result<Option<MemoryRecord>> {
    let mut statement = transaction.prepare(
        r#"
        SELECT
            memory_id, project_id, kind, headline, summary, content, content_hash,
            taxonomy_json, metadata_json, created_at, updated_at, last_accessed_at,
            reinforcement, penalty, success_score, failure_count, repeated_mistake_count,
            reinforcement_decay, conflict_score, last_feedback_at, access_count, version
        FROM memories
        WHERE memory_id = ?1
        "#,
    )?;
    statement
        .query_row(params![memory_id.to_string()], decode_memory_row)
        .optional()
        .map_err(MemovynError::from)
}

fn feedback_deltas(
    outcome: FeedbackOutcome,
    repeated_mistake: bool,
    weight: f32,
    activity_score: f32,
) -> (f32, f32, f32, u32, f32) {
    let tuned_weight = (weight * (1.0 + activity_score * 0.14)).clamp(0.25, 4.0);
    let repeat_penalty = if repeated_mistake { 0.55 } else { 0.0 };
    match outcome {
        FeedbackOutcome::Success => (1.25 * tuned_weight, 0.0, 1.1 * tuned_weight, 0, 0.0),
        FeedbackOutcome::Partial => (
            0.45 * tuned_weight,
            0.12 * tuned_weight,
            0.35 * tuned_weight,
            0,
            0.12 * tuned_weight,
        ),
        FeedbackOutcome::Failure => (
            0.0,
            (0.9 + repeat_penalty) * tuned_weight,
            0.0,
            1,
            (0.85 + repeat_penalty) * tuned_weight,
        ),
        FeedbackOutcome::Regression => (
            0.0,
            (1.4 + repeat_penalty) * tuned_weight,
            0.0,
            1,
            (1.2 + repeat_penalty) * tuned_weight,
        ),
    }
}

fn decay_after_feedback(current: f32, outcome: FeedbackOutcome, activity_score: f32) -> f32 {
    let activity_bias = 1.0 + activity_score * 0.08;
    let adjusted = match outcome {
        FeedbackOutcome::Success => current * (0.94 / activity_bias.max(1.0)),
        FeedbackOutcome::Partial => current,
        FeedbackOutcome::Failure => current * (1.08 * activity_bias),
        FeedbackOutcome::Regression => current * (1.14 * activity_bias),
    };
    adjusted.clamp(0.78, 2.5)
}

fn apply_learning_to_taxonomy(taxonomy: &mut TaxonomyDecomposition, learning: &LearningState) {
    for signal in &mut taxonomy.signals {
        signal.reinforcement_weight = learning.success_score + learning.conflict_score;
        signal.failure_count = learning.failure_count;
        signal.reinforcement_decay = learning.reinforcement_decay;
    }
    for node in &mut taxonomy.hierarchy {
        node.reinforcement_weight = learning.success_score + learning.conflict_score;
        node.failure_count = learning.failure_count;
        node.reinforcement_decay = learning.reinforcement_decay;
        if learning.success_score > learning.conflict_score {
            node.priority = node.priority.saturating_add(1).min(100);
        } else if learning.failure_count > 0 {
            node.priority = node.priority.saturating_sub(1);
        }
    }
}

fn ranked_memories_query(
    connection: &Connection,
    project_id: &str,
    sql: &str,
) -> Result<Vec<RankedMemoryStat>> {
    let mut statement = connection.prepare_cached(sql)?;
    let rows = statement.query_map(params![project_id], |row| {
        Ok(RankedMemoryStat {
            memory_id: uuid::Uuid::parse_str(&row.get::<_, String>(0)?).map_err(sqlite_uuid_err)?,
            headline: row.get(1)?,
            summary: row.get(2)?,
            access_count: row.get::<_, i64>(3)? as u64,
            score: row.get::<_, f32>(4)?,
            success_score: row.get(5)?,
            failure_count: row.get::<_, i64>(6)? as u32,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(MemovynError::from)
}

fn load_growth_series(connection: &Connection, project_id: &str) -> Result<Vec<AnalyticsBucket>> {
    let mut buckets = BTreeMap::<String, AnalyticsBucket>::new();

    let mut memory_stmt = connection.prepare_cached(
        r#"
        SELECT substr(created_at, 1, 10) AS bucket, COUNT(*) AS memories
        FROM memories
        WHERE project_id = ?1
        GROUP BY bucket
        ORDER BY bucket ASC
        "#,
    )?;
    let memory_rows = memory_stmt.query_map(params![project_id], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
    })?;
    for row in memory_rows {
        let (bucket, memories) = row?;
        buckets
            .entry(bucket.clone())
            .or_insert(AnalyticsBucket {
                bucket,
                memories: 0,
                conflicts: 0,
                recalls: 0,
                tokens_saved: 0,
            })
            .memories = memories;
    }

    let mut recall_stmt = connection.prepare_cached(
        r#"
        SELECT substr(r.recalled_at, 1, 10) AS bucket, COUNT(*) AS recalls, COALESCE(SUM(r.tokens_saved), 0)
        FROM recollections r
        INNER JOIN memories m ON m.memory_id = r.memory_id
        WHERE m.project_id = ?1
        GROUP BY bucket
        ORDER BY bucket ASC
        "#,
    )?;
    let recall_rows = recall_stmt.query_map(params![project_id], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, i64>(1)? as usize,
            row.get::<_, i64>(2)? as u64,
        ))
    })?;
    for row in recall_rows {
        let (bucket, recalls, tokens_saved) = row?;
        let entry = buckets.entry(bucket.clone()).or_insert(AnalyticsBucket {
            bucket,
            memories: 0,
            conflicts: 0,
            recalls: 0,
            tokens_saved: 0,
        });
        entry.recalls = recalls;
        entry.tokens_saved = tokens_saved;
    }

    Ok(buckets.into_values().collect())
}

fn load_conflict_heatmap(
    connection: &Connection,
    project_id: &str,
) -> Result<Vec<AnalyticsBucket>> {
    let mut statement = connection.prepare_cached(
        r#"
        SELECT
            substr(created_at, 1, 10) AS bucket,
            COUNT(*) AS conflicts,
            SUM(CASE WHEN repeated_mistake = 1 THEN 1 ELSE 0 END) AS repeated_conflicts
        FROM feedback_events
        WHERE project_id = ?1
          AND outcome IN ('failure', 'regression')
        GROUP BY bucket
        ORDER BY bucket ASC
        "#,
    )?;
    let rows = statement.query_map(params![project_id], |row| {
        Ok(AnalyticsBucket {
            bucket: row.get(0)?,
            memories: 0,
            conflicts: row.get::<_, i64>(1)? as usize,
            recalls: row.get::<_, i64>(2)? as usize,
            tokens_saved: 0,
        })
    })?;
    rows.collect::<std::result::Result<Vec<_>, _>>()
        .map_err(MemovynError::from)
}

fn decode_memory_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<MemoryRecord> {
    let memory_id: String = row.get(0)?;
    let kind_json: String = row.get(2)?;
    let taxonomy_json: String = row.get(7)?;
    let metadata_json: String = row.get(8)?;
    let created_at: String = row.get(9)?;
    let updated_at: String = row.get(10)?;
    let last_accessed_at: String = row.get(11)?;
    let last_feedback_at: Option<String> = row.get(19)?;

    let taxonomy: TaxonomyDecomposition =
        serde_json::from_str(&taxonomy_json).map_err(sqlite_serde_err)?;
    let metadata = serde_json::from_str(&metadata_json).map_err(sqlite_serde_err)?;
    let kind = serde_json::from_str(&kind_json).map_err(sqlite_serde_err)?;

    Ok(MemoryRecord {
        id: uuid::Uuid::parse_str(&memory_id).map_err(sqlite_uuid_err)?,
        project_id: row.get(1)?,
        kind,
        headline: row.get(3)?,
        summary: row.get(4)?,
        content: row.get(5)?,
        content_hash: row.get(6)?,
        taxonomy,
        metadata,
        created_at: parse_time(&created_at).map_err(sqlite_time_err)?,
        updated_at: parse_time(&updated_at).map_err(sqlite_time_err)?,
        last_accessed_at: parse_time(&last_accessed_at).map_err(sqlite_time_err)?,
        reinforcement: row.get(12)?,
        penalty: row.get(13)?,
        learning: LearningState {
            success_score: row.get(14)?,
            failure_count: row.get::<_, i64>(15)? as u32,
            repeated_mistake_count: row.get::<_, i64>(16)? as u32,
            reinforcement_decay: row.get(17)?,
            conflict_score: row.get(18)?,
            last_feedback_at: last_feedback_at
                .as_deref()
                .map(parse_time)
                .transpose()
                .map_err(sqlite_time_err)?,
        },
        access_count: row.get::<_, i64>(20)? as u64,
        version: row.get::<_, i64>(21)? as u32,
    })
}

fn ensure_memory_columns(connection: &Connection) -> Result<()> {
    let columns = table_columns(connection, "memories")?;
    for (column, sql) in [
        (
            "success_score",
            "ALTER TABLE memories ADD COLUMN success_score REAL NOT NULL DEFAULT 0",
        ),
        (
            "failure_count",
            "ALTER TABLE memories ADD COLUMN failure_count INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "repeated_mistake_count",
            "ALTER TABLE memories ADD COLUMN repeated_mistake_count INTEGER NOT NULL DEFAULT 0",
        ),
        (
            "reinforcement_decay",
            "ALTER TABLE memories ADD COLUMN reinforcement_decay REAL NOT NULL DEFAULT 1",
        ),
        (
            "conflict_score",
            "ALTER TABLE memories ADD COLUMN conflict_score REAL NOT NULL DEFAULT 0",
        ),
        (
            "last_feedback_at",
            "ALTER TABLE memories ADD COLUMN last_feedback_at TEXT",
        ),
    ] {
        if !columns.contains(column) {
            connection.execute(sql, [])?;
        }
    }
    Ok(())
}

fn table_columns(
    connection: &Connection,
    table: &str,
) -> Result<std::collections::HashSet<String>> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    let columns = rows.collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(columns.into_iter().collect())
}

fn format_time(value: OffsetDateTime) -> String {
    value
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap()
}

fn now_string() -> String {
    format_time(OffsetDateTime::now_utc())
}

fn parse_time(value: &str) -> std::result::Result<OffsetDateTime, time::error::Parse> {
    OffsetDateTime::parse(value, &time::format_description::well_known::Rfc3339)
}

fn parse_time_opt(value: String) -> Option<OffsetDateTime> {
    parse_time(&value).ok()
}

fn sqlite_time_err(err: time::error::Parse) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
}

fn sqlite_uuid_err(err: uuid::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
}

fn sqlite_serde_err(err: serde_json::Error) -> rusqlite::Error {
    rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(err))
}
