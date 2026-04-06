use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::header::{CONTENT_TYPE, HeaderValue};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::app::Memovyn;
use crate::domain::{
    AddMemoryRequest, ArchiveRequest, FeedbackRequest, ReflectionRequest, SearchRequest,
};

pub fn router(app: Arc<Memovyn>) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/projects/:project_id", get(project_page))
        .route("/api/projects", get(list_projects))
        .route("/api/projects/:project_id/context", get(project_context))
        .route(
            "/api/projects/:project_id/analytics",
            get(project_analytics),
        )
        .route(
            "/api/projects/:project_id/analytics.csv",
            get(project_analytics_csv),
        )
        .route(
            "/api/projects/:project_id/analytics.md",
            get(project_analytics_markdown),
        )
        .route("/api/projects/:project_id/memories", get(project_memories))
        .route("/api/memories/:memory_id", get(inspect_memory))
        .route("/api/memories", post(add_memory))
        .route("/api/reflect", post(reflect_memory))
        .route("/api/feedback", post(feedback_memory))
        .route("/api/archive", post(archive_memory))
        .route("/static/app.css", get(stylesheet))
        .route("/static/app.js", get(script))
        .with_state(app)
}

async fn index(State(app): State<Arc<Memovyn>>) -> Result<Html<String>, (StatusCode, String)> {
    let projects = app.list_projects().map_err(internal_error)?;
    let cards = projects
        .into_iter()
        .map(|project| {
            let updated = project
                .last_updated_at
                .map(|ts| ts.to_string())
                .unwrap_or_else(|| "new".to_string());
            format!(
                r#"<a class="project-card" href="/projects/{id}">
                    <div class="project-card__header">
                        <strong>{id}</strong>
                        <span>{memories} memories</span>
                    </div>
                    <div class="project-card__meta">
                        <span>updated {updated}</span>
                        <span>{scope}</span>
                    </div>
                </a>"#,
                id = project.project_id,
                memories = project.memory_count,
                updated = updated,
                scope = if project.share_scope {
                    "cross-project"
                } else {
                    "project-only"
                },
            )
        })
        .collect::<Vec<_>>()
        .join("");

    Ok(Html(format!(
        r#"<!doctype html>
        <html lang="en">
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <title>Memovyn</title>
            <link rel="stylesheet" href="/static/app.css">
        </head>
        <body>
            <main class="shell">
                <section class="hero">
                    <div class="hero__controls">
                        <button id="theme-toggle" class="theme-toggle" type="button">Toggle Theme</button>
                    </div>
                    <p class="eyebrow">Memovyn v0.2</p>
                    <h1>Permanent memory for local-first coding agents.</h1>
                    <p class="lede">Taxonomy-native recall, regression avoidance, and progressive context assembly in one static binary.</p>
                </section>
                <section class="panel">
                    <div class="panel__header">
                        <h2>Projects</h2>
                        <span>SQLite-backed, MCP-native</span>
                    </div>
                    <div class="project-grid">{cards}</div>
                </section>
            </main>
        </body>
        </html>"#,
        cards = cards
    )))
}

async fn project_page(
    Path(project_id): Path<String>,
    State(app): State<Arc<Memovyn>>,
) -> Result<Html<String>, (StatusCode, String)> {
    let context = app
        .get_project_context(&project_id)
        .map_err(internal_error)?;
    let analytics = app.analytics(&project_id).map_err(internal_error)?;
    let labels = context
        .taxonomy_summary
        .top_labels
        .iter()
        .map(|(label, count)| format!(r#"<li><span>{label}</span><strong>{count}</strong></li>"#))
        .collect::<Vec<_>>()
        .join("");
    let relations = context
        .taxonomy_summary
        .top_relations
        .iter()
        .map(|(relation, count)| {
            format!(r#"<li><span>{relation}</span><strong>{count}</strong></li>"#)
        })
        .collect::<Vec<_>>()
        .join("");
    let notes = context
        .debugging_notes
        .iter()
        .map(|note| format!(r#"<li>{}</li>"#, html_escape(note)))
        .collect::<Vec<_>>()
        .join("");

    Ok(Html(format!(
        r#"<!doctype html>
        <html lang="en">
        <head>
            <meta charset="utf-8">
            <meta name="viewport" content="width=device-width, initial-scale=1">
            <title>{project_id} · Memovyn</title>
            <link rel="stylesheet" href="/static/app.css">
        </head>
        <body data-project-id="{project_id}">
            <main class="shell shell--project">
                <aside class="sidebar panel">
                    <div class="sidebar__top">
                        <a href="/" class="back-link">Back</a>
                        <button id="theme-toggle" class="theme-toggle" type="button">Toggle Theme</button>
                    </div>
                    <p class="eyebrow">Project memory container</p>
                    <h1>{project_id}</h1>
                    <div class="stats-row">
                        <div class="stat-chip"><strong>{total_memories}</strong><span>memories</span></div>
                        <div class="stat-chip"><strong>{conflicts}</strong><span>conflicts</span></div>
                        <div class="stat-chip"><strong>{token_savings}</strong><span>tokens saved</span></div>
                        <div class="stat-chip"><strong>{session_tokens}</strong><span>session savings</span></div>
                        <div class="stat-chip"><strong>{health_score}</strong><span>health score</span></div>
                    </div>
                    <pre class="context-card">{ready_context}</pre>
                    <h2>Taxonomy heatmap</h2>
                    <ul class="label-list">{labels}</ul>
                    <h2>Relation graph</h2>
                    <ul class="label-list">{relations}</ul>
                    <h2>Learning leaders</h2>
                    <div id="reinforcement-panel" class="analytics-panel">
                        <p class="analytics-placeholder">Loading reinforcement leaders…</p>
                    </div>
                    <h2>Debug notes</h2>
                    <ul class="note-list">{notes}</ul>
                </aside>
                <section class="panel panel--main">
                    <div class="panel__header">
                        <div>
                            <h2>Memory stream</h2>
                            <p>Virtualized recall across large project timelines.</p>
                        </div>
                        <label class="search-box">
                            <span>Search</span>
                            <input id="memory-search" type="search" placeholder="architecture, bugfix, sqlite, regression...">
                        </label>
                    </div>
                    <section class="panel panel--analytics">
                        <div class="panel__header">
                            <div>
                                <h2>Analytics</h2>
                                <p>Visible recall, growth, and punishment state for this project.</p>
                            </div>
                            <div class="export-links">
                                <a class="export-link" href="/api/projects/{project_id}/analytics.csv">Export CSV</a>
                                <a class="export-link" href="/api/projects/{project_id}/analytics.md">Project Memory Health Report</a>
                            </div>
                        </div>
                        <div id="analytics-grid" class="analytics-grid">
                            <p class="analytics-placeholder">Loading analytics…</p>
                        </div>
                    </section>
                    <div id="memory-viewport" class="memory-viewport"></div>
                    <section id="inspection-drawer" class="inspection-drawer">
                        <h3>Memory inspector</h3>
                        <p>Click a memory card to inspect taxonomy signals, relations, and version history.</p>
                    </section>
                </section>
            </main>
            <script src="/static/app.js"></script>
        </body>
        </html>"#,
        project_id = project_id,
        total_memories = analytics.total_memories,
        conflicts = analytics.conflict_count,
        token_savings = analytics.total_token_savings,
        session_tokens = analytics.session_token_savings,
        health_score = analytics.memory_health_score,
        ready_context = html_escape(&context.ready_context),
        labels = labels,
        relations = relations,
        notes = notes
    )))
}

async fn list_projects(
    State(app): State<Arc<Memovyn>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let projects = app.list_projects().map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "projects": projects })))
}

async fn project_context(
    Path(project_id): Path<String>,
    State(app): State<Arc<Memovyn>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let context = app
        .get_project_context(&project_id)
        .map_err(internal_error)?;
    let analytics = app.analytics(&project_id).map_err(internal_error)?;
    Ok(Json(serde_json::json!({
        "context": context,
        "analytics": analytics
    })))
}

async fn project_analytics(
    Path(project_id): Path<String>,
    State(app): State<Arc<Memovyn>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let analytics = app.analytics(&project_id).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "analytics": analytics })))
}

async fn project_analytics_csv(
    Path(project_id): Path<String>,
    State(app): State<Arc<Memovyn>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let csv = app.analytics_csv(&project_id).map_err(internal_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
    );
    Ok((headers, csv))
}

async fn project_analytics_markdown(
    Path(project_id): Path<String>,
    State(app): State<Arc<Memovyn>>,
) -> Result<impl IntoResponse, (StatusCode, String)> {
    let markdown = app
        .analytics_markdown(&project_id)
        .map_err(internal_error)?;
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/markdown; charset=utf-8"),
    );
    Ok((headers, markdown))
}

#[derive(Debug, Deserialize)]
struct MemoryQuery {
    q: Option<String>,
    offset: Option<usize>,
    limit: Option<usize>,
    include_shared: Option<bool>,
}

async fn project_memories(
    Path(project_id): Path<String>,
    Query(params): Query<MemoryQuery>,
    State(app): State<Arc<Memovyn>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let offset = params.offset.unwrap_or(0);
    let limit = params.limit.unwrap_or(40).max(1);
    let needed = offset + limit;
    let response = app
        .search_memories(SearchRequest {
            project_id,
            query: params.q.unwrap_or_default(),
            limit: needed.max(40),
            filters: crate::domain::SearchFilters {
                include_shared: params.include_shared.unwrap_or(false),
                include_private_notes: true,
                ..Default::default()
            },
        })
        .map_err(internal_error)?;
    let detail_layer = response
        .detail_layer
        .into_iter()
        .skip(offset)
        .take(limit)
        .collect::<Vec<_>>();
    Ok(Json(serde_json::json!({
        "items": detail_layer,
        "total": response.total_hits,
        "offset": offset,
        "limit": limit
    })))
}

async fn add_memory(
    State(app): State<Arc<Memovyn>>,
    Json(request): Json<AddMemoryRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let memory = app.add_memory(request).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "memory": memory })))
}

async fn inspect_memory(
    Path(memory_id): Path<String>,
    State(app): State<Arc<Memovyn>>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let memory_id = uuid::Uuid::parse_str(&memory_id).map_err(internal_error)?;
    let inspection = app.inspect_memory(memory_id).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "inspection": inspection })))
}

async fn reflect_memory(
    State(app): State<Arc<Memovyn>>,
    Json(request): Json<ReflectionRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let reflection = app.reflect_memory(request).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "reflection": reflection })))
}

async fn feedback_memory(
    State(app): State<Arc<Memovyn>>,
    Json(request): Json<FeedbackRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let feedback = app.feedback_memory(request).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "feedback": feedback })))
}

async fn archive_memory(
    State(app): State<Arc<Memovyn>>,
    Json(request): Json<ArchiveRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, String)> {
    let archived = app.archive_memory(request).map_err(internal_error)?;
    Ok(Json(serde_json::json!({ "archived": archived })))
}

async fn stylesheet() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("text/css; charset=utf-8"),
    );
    (
        headers,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/app.css")),
    )
}

async fn script() -> impl IntoResponse {
    let mut headers = HeaderMap::new();
    headers.insert(
        CONTENT_TYPE,
        HeaderValue::from_static("application/javascript; charset=utf-8"),
    );
    (
        headers,
        include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/static/app.js")),
    )
}

fn internal_error(error: impl std::fmt::Display) -> (StatusCode, String) {
    (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
}

fn html_escape(input: &str) -> String {
    input
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}
