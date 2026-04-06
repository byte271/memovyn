use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use clap::{Parser, Subcommand};
use memovyn::dashboard;
use memovyn::mcp;
use memovyn::{
    AddMemoryRequest, ArchiveRequest, Config, FeedbackOutcome, FeedbackRequest, MemoryKind,
    Memovyn, ReflectionRequest, SearchRequest,
};
use tracing_subscriber::EnvFilter;

#[derive(Debug, Parser)]
#[command(
    name = "memovyn",
    version,
    about = "Permanent local-first memory for MCP-native coding agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = "127.0.0.1:7761")]
        bind: String,
    },
    McpStdio,
    Add {
        project_id: String,
        content: String,
        #[arg(long, value_enum, default_value_t = KindArg::Observation)]
        kind: KindArg,
    },
    Search {
        project_id: String,
        query: String,
        #[arg(long, default_value_t = 8)]
        limit: usize,
    },
    Context {
        project_id: String,
    },
    Reflect {
        project_id: String,
        task_result: String,
        #[arg(long, value_enum)]
        outcome: OutcomeArg,
    },
    Feedback {
        memory_id: String,
        #[arg(long, value_enum)]
        outcome: OutcomeArg,
        #[arg(long, default_value_t = false)]
        repeated_mistake: bool,
        #[arg(long, default_value_t = 1.0)]
        weight: f32,
    },
    Archive {
        memory_id: String,
    },
    Note {
        project_id: String,
        content: String,
    },
    Projects,
    Analytics {
        project_id: String,
        #[arg(long, default_value_t = false)]
        csv: bool,
        #[arg(long, default_value_t = false)]
        markdown: bool,
    },
    Export {
        project_id: String,
        output: PathBuf,
    },
    Import {
        input: PathBuf,
    },
    Inspect {
        memory_id: String,
    },
    Benchmark {
        project_id: String,
        #[arg(long, default_value_t = 5000)]
        memories: usize,
        #[arg(long, default_value = "sqlite bm25 dashboard")]
        query: String,
    },
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum KindArg {
    Observation,
    Decision,
    Issue,
    Outcome,
    Note,
    Reflection,
    Context,
}

impl From<KindArg> for MemoryKind {
    fn from(value: KindArg) -> Self {
        match value {
            KindArg::Observation => MemoryKind::Observation,
            KindArg::Decision => MemoryKind::Decision,
            KindArg::Issue => MemoryKind::Issue,
            KindArg::Outcome => MemoryKind::Outcome,
            KindArg::Note => MemoryKind::Note,
            KindArg::Reflection => MemoryKind::Reflection,
            KindArg::Context => MemoryKind::Context,
        }
    }
}

#[derive(Debug, Clone, Copy, clap::ValueEnum)]
enum OutcomeArg {
    Success,
    Failure,
    Regression,
    Partial,
}

impl From<OutcomeArg> for FeedbackOutcome {
    fn from(value: OutcomeArg) -> Self {
        match value {
            OutcomeArg::Success => FeedbackOutcome::Success,
            OutcomeArg::Failure => FeedbackOutcome::Failure,
            OutcomeArg::Regression => FeedbackOutcome::Regression,
            OutcomeArg::Partial => FeedbackOutcome::Partial,
        }
    }
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("memovyn=info,tower_http=info")),
        )
        .init();

    let cli = Cli::parse();
    let config = Config::from_env();
    let app = Arc::new(Memovyn::open(config)?);

    match cli.command {
        Command::Serve { bind } => serve(app, &bind).await?,
        Command::McpStdio => mcp::serve_stdio(app).await?,
        Command::Add {
            project_id,
            content,
            kind,
        } => {
            let memory = app.add_memory(AddMemoryRequest {
                project_id,
                content,
                metadata: Default::default(),
                kind: kind.into(),
            })?;
            println!("{}", serde_json::to_string_pretty(&memory)?);
        }
        Command::Search {
            project_id,
            query,
            limit,
        } => {
            let response = app.search_memories(SearchRequest {
                project_id,
                query,
                limit,
                filters: Default::default(),
            })?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        Command::Context { project_id } => {
            let context = app.get_project_context(&project_id)?;
            println!("{}", serde_json::to_string_pretty(&context)?);
        }
        Command::Reflect {
            project_id,
            task_result,
            outcome,
        } => {
            let response = app.reflect_memory(ReflectionRequest {
                project_id,
                task_result,
                outcome: outcome.into(),
                metadata: Default::default(),
            })?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        Command::Feedback {
            memory_id,
            outcome,
            repeated_mistake,
            weight,
        } => {
            let response = app.feedback_memory(FeedbackRequest {
                memory_id: uuid::Uuid::parse_str(&memory_id)?,
                outcome: outcome.into(),
                repeated_mistake,
                weight,
                cross_project_influence: true,
                avoid_patterns: Vec::new(),
                note: Some("cli-feedback".to_string()),
            })?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        Command::Archive { memory_id } => {
            let response = app.archive_memory(ArchiveRequest {
                memory_id: uuid::Uuid::parse_str(&memory_id)?,
            })?;
            println!("{}", serde_json::to_string_pretty(&response)?);
        }
        Command::Note {
            project_id,
            content,
        } => {
            let memory = app.add_memory(AddMemoryRequest {
                project_id,
                content,
                metadata: Default::default(),
                kind: MemoryKind::Note,
            })?;
            println!("{}", serde_json::to_string_pretty(&memory)?);
        }
        Command::Projects => {
            println!("{}", serde_json::to_string_pretty(&app.list_projects()?)?);
        }
        Command::Analytics {
            project_id,
            csv,
            markdown,
        } => {
            if csv {
                println!("{}", app.analytics_csv(&project_id)?);
            } else if markdown {
                println!("{}", app.analytics_markdown(&project_id)?);
            } else {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&app.analytics(&project_id)?)?
                );
            }
        }
        Command::Export { project_id, output } => {
            app.export_project(&project_id, &output)?;
            println!("exported {} to {}", project_id, output.display());
        }
        Command::Import { input } => {
            let imported = app.import_bundle(&input)?;
            println!("imported {} memories", imported);
        }
        Command::Inspect { memory_id } => {
            let memory_id = uuid::Uuid::parse_str(&memory_id)?;
            let inspection = app.inspect_memory(memory_id)?;
            println!("{}", serde_json::to_string_pretty(&inspection)?);
        }
        Command::Benchmark {
            project_id,
            memories,
            query,
        } => {
            println!("{}", app.benchmark(&project_id, memories, &query)?);
        }
    }

    Ok(())
}

async fn serve(app: Arc<Memovyn>, bind: &str) -> anyhow::Result<()> {
    let router = Router::new()
        .merge(dashboard::router(app.clone()))
        .merge(mcp::router(app));
    let address: SocketAddr = bind.parse()?;
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!("Memovyn listening on http://{}", address);
    axum::serve(listener, router)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    Ok(())
}

async fn shutdown_signal() {
    let _ = tokio::signal::ctrl_c().await;
}
