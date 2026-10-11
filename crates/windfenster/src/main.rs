use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use wf_core::db::{self, Db};

/// Windfenster: wind forecast for water sports. Without a command, `serve` runs.
#[derive(Parser)]
#[command(version, args_conflicts_with_subcommands = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    serve: ServeArgs,
}

#[derive(Subcommand)]
enum Command {
    /// Migrate the database, then serve the API and the SPA.
    Serve(ServeArgs),
    /// Apply pending database migrations and exit.
    Migrate(DataArgs),
}

#[derive(Args)]
struct DataArgs {
    /// Directory for the database (and later the map textures).
    #[arg(long, env = "WF_DATA_DIR", default_value = "/data")]
    data_dir: PathBuf,
}

#[derive(Args)]
struct ServeArgs {
    #[command(flatten)]
    data: DataArgs,
    /// Address the HTTP server listens on.
    #[arg(long, env = "WF_LISTEN", default_value = "0.0.0.0:8080")]
    listen: SocketAddr,
    /// Vite dev server to redirect to while no SPA is built in (`pnpm dev`).
    #[arg(long, env = "WF_DEV_UI")]
    dev_ui: Option<String>,
}

#[tokio::main]
async fn main() -> Result<()> {
    // Development: read `.env` from the working directory or a parent. Variables already set
    // in the environment win, so Docker configuration is unaffected.
    let dotenv = dotenvy::dotenv();
    tracing_subscriber::fmt()
        // Colours only on a terminal, not in `docker logs`.
        .with_ansi(std::io::stdout().is_terminal())
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn")),
        )
        .init();
    if let Ok(path) = &dotenv {
        tracing::info!("loaded {}", path.display());
    }

    let cli = Cli::parse();
    match cli.command.unwrap_or(Command::Serve(cli.serve)) {
        Command::Migrate(data) => {
            open(&data).await?;
            Ok(())
        }
        Command::Serve(args) => {
            let db = open(&args.data).await?;
            serve(db, args.listen, args.dev_ui.filter(|v| !v.is_empty())).await
        }
    }
}

/// Opens the database in the data directory and brings it to the newest schema.
async fn open(data: &DataArgs) -> Result<Db> {
    std::fs::create_dir_all(&data.data_dir)
        .with_context(|| format!("creating {}", data.data_dir.display()))?;
    let db = db::connect(&data.data_dir.join("windfenster.db")).await?;
    let schema = db::migrate(&db).await?;
    tracing::info!("database at schema version {schema}");
    Ok(db)
}

async fn serve(db: Db, listen: SocketAddr, dev_ui: Option<String>) -> Result<()> {
    let app = wf_api::router(wf_api::AppState { db, dev_ui });
    let listener = TcpListener::bind(listen)
        .await
        .with_context(|| format!("binding {listen}"))?;
    tracing::info!("windfenster {} on http://{listen}", wf_api::VERSION);
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shut down");
    Ok(())
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    tokio::select! {
        _ = ctrl_c => {},
        _ = term => {},
    }
}
