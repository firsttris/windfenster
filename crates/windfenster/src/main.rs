use std::io::IsTerminal;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use wf_core::db::{self, Db};
use wf_core::geo::GridPoint;
use wf_ingest::forecasts::Forecasts;
use wf_ingest::geocode::Geocoder;
use wf_ingest::measurements::{Measurements, Urls};
use wf_ingest::openmeteo::{self, OpenMeteo};
use wf_ingest::scheduler::Scheduler;
use wf_ingest::stations::{self, Sources};
use wf_map::MapData;

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
    /// Migrate the database, then serve the API and the SPA and keep the data current.
    Serve(ServeArgs),
    /// Apply pending database migrations and exit.
    Migrate(DataArgs),
    /// Fetch the station lists (OurAirports, DWD) now.
    ImportStations(DataArgs),
    /// Ask Open-Meteo for every model at a point and print what comes back.
    Probe(ProbeArgs),
}

#[derive(Args)]
struct DataArgs {
    /// Directory for the database and the map textures.
    #[arg(long, env = "WF_DATA_DIR", default_value = "/data")]
    data_dir: PathBuf,
}

#[derive(Args)]
struct OpenMeteoArgs {
    /// Open-Meteo API key; without one the free, non-commercial API is used.
    #[arg(long, env = "OPEN_METEO_API_KEY", hide_env_values = true)]
    open_meteo_api_key: Option<String>,
    /// Own Open-Meteo instance, e.g. http://open-meteo:8080.
    #[arg(long, env = "OPEN_METEO_BASE_URL")]
    open_meteo_base_url: Option<String>,
    /// Calls per day; default 9,000 without a key, unlimited with one.
    #[arg(long, env = "WF_OPEN_METEO_DAILY_BUDGET")]
    open_meteo_daily_budget: Option<u32>,
    /// Models to use, comma-separated Open-Meteo ids; default all.
    #[arg(long, env = "WF_MODELS", value_delimiter = ',')]
    models: Option<Vec<String>>,
}

impl OpenMeteoArgs {
    fn config(&self) -> openmeteo::Config {
        openmeteo::Config {
            base_url: non_empty(&self.open_meteo_base_url),
            api_key: non_empty(&self.open_meteo_api_key),
            daily_budget: self.open_meteo_daily_budget,
        }
    }
}

#[derive(Args)]
struct ServeArgs {
    #[command(flatten)]
    data: DataArgs,
    #[command(flatten)]
    open_meteo: OpenMeteoArgs,
    /// Address the HTTP server listens on.
    #[arg(long, env = "WF_LISTEN", default_value = "0.0.0.0:8080")]
    listen: SocketAddr,
    /// Public address, e.g. https://windfenster.example, for canonical links of station pages.
    #[arg(long, env = "WF_PUBLIC_URL")]
    public_url: Option<String>,
    /// Open-Meteo geocoding instance for the place search.
    #[arg(long, env = "OPEN_METEO_GEOCODING_URL")]
    geocoding_url: Option<String>,
    /// Stations (slugs) kept current and verified even when nobody opens them.
    #[arg(
        long,
        env = "WF_TRACKED_STATIONS",
        value_delimiter = ',',
        default_value = "edsb,lgrp"
    )]
    tracked_stations: Vec<String>,
    /// Background jobs (station lists, model runs, measurements); `false` serves only what is
    /// requested.
    #[arg(long, env = "WF_SCHEDULER", default_value_t = true, action = clap::ArgAction::Set)]
    scheduler: bool,
    /// The wind map: fetch ICON-D2 from the DWD (about 100 MB per run, every three hours) and
    /// keep two runs as textures in `{data_dir}/map`; `false` turns the map off.
    #[arg(long, env = "WF_MAP", default_value_t = true, action = clap::ArgAction::Set)]
    map: bool,
    /// The map's detailed basemap: `auto` cuts one from Protomaps' daily build (the map's region
    /// to zoom 8, 20 km around each tracked station to zoom 14) unless the operator put one in
    /// place; `off` leaves it to the operator.
    #[arg(long, env = "WF_BASEMAP", default_value = "auto", value_parser = ["auto", "off"])]
    basemap: String,
    /// Vite dev server to redirect to while no SPA is built in (`pnpm dev`).
    #[arg(long, env = "WF_DEV_UI")]
    dev_ui: Option<String>,
}

#[derive(Args)]
struct ProbeArgs {
    #[command(flatten)]
    data: DataArgs,
    #[command(flatten)]
    open_meteo: OpenMeteoArgs,
    /// Latitude, default Baden Airpark (EDSB).
    #[arg(long, default_value_t = 48.7794, allow_negative_numbers = true)]
    lat: f64,
    #[arg(long, default_value_t = 8.0805, allow_negative_numbers = true)]
    lon: f64,
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_ref().filter(|s| !s.trim().is_empty()).cloned()
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
        Command::ImportStations(data) => {
            let db = open(&data).await?;
            let http = wf_ingest::http_client()?;
            Ok(stations::import_all(&db, &http, &Sources::default()).await?)
        }
        Command::Probe(args) => probe(args).await,
        Command::Serve(args) => serve(args).await,
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

async fn serve(args: ServeArgs) -> Result<()> {
    let db = open(&args.data).await?;
    let http = wf_ingest::http_client()?;
    let models = wf_core::model::active(&db, args.open_meteo.models.as_deref()).await?;
    tracing::info!(
        "models: {}",
        models
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    let cfg = args.open_meteo.config();
    let api_key = cfg.api_key.clone();
    let forecasts = Arc::new(Forecasts::new(
        db.clone(),
        OpenMeteo::new(http.clone(), cfg),
        models,
    ));
    let geocoder = Arc::new(Geocoder::new(
        http.clone(),
        non_empty(&args.geocoding_url),
        api_key,
    ));
    let measurements = Arc::new(Measurements::new(db.clone(), http.clone(), Urls::default()));
    let map = args.map.then(|| {
        Arc::new(MapData::new(
            args.data.data_dir.join("map"),
            http.clone(),
            wf_map::Urls::default(),
        ))
    });
    let tracked: Vec<String> = args
        .tracked_stations
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if let (true, Some(map)) = (args.scheduler, &map) {
        tokio::spawn(map.clone().keep_current());
        if args.basemap == "auto" {
            tokio::spawn(map.clone().keep_basemap(db.clone()));
        }
    }
    if args.scheduler {
        tokio::spawn(
            Scheduler {
                db: db.clone(),
                http,
                forecasts: forecasts.clone(),
                measurements: measurements.clone(),
                sources: Sources::default(),
                tracked,
            }
            .run(),
        );
    }
    let mut state = wf_api::AppState::new(
        db,
        forecasts,
        Some(geocoder),
        Some(measurements),
        non_empty(&args.public_url),
        non_empty(&args.dev_ui),
    );
    if let Some(map) = map {
        state = state.with_map(map);
    }
    let listener = TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("binding {}", args.listen))?;
    tracing::info!("windfenster {} on http://{}", wf_api::VERSION, args.listen);
    axum::serve(listener, wf_api::router(state))
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    tracing::info!("shut down");
    Ok(())
}

async fn probe(args: ProbeArgs) -> Result<()> {
    let db = open(&args.data).await?;
    let models = wf_core::model::active(&db, args.open_meteo.models.as_deref()).await?;
    let p = GridPoint::new(args.lat, args.lon).context("invalid coordinates")?;
    let om = OpenMeteo::new(wf_ingest::http_client()?, args.open_meteo.config());
    println!(
        "{}",
        om.forecast_url(p, &[]).replace("&models=", "&models=…")
    );
    for m in &models {
        let run = match &m.meta_id {
            Some(id) => match om.latest_run(id).await {
                Ok(Some(t)) => format!("run {t}"),
                Ok(None) => "no run in meta.json".into(),
                Err(e) => format!("meta.json: {e}"),
            },
            None => "no meta id".into(),
        };
        let covered = if m.covers(p.lat(), p.lon()) {
            "in domain"
        } else {
            "outside domain"
        };
        println!("{:<30} {covered:<15} {run}", m.id);
    }
    let ids: Vec<&str> = models.iter().map(|m| m.id.as_str()).collect();
    let data = om.forecast(p, &ids, false).await?;
    println!(
        "timezone {:?}, elevation {:?} m",
        data.timezone, data.elevation_m
    );
    for (id, s) in &data.series {
        let first: Vec<String> = s
            .wind
            .iter()
            .take(6)
            .map(|v| v.map_or("–".into(), |v| format!("{v:.1}")))
            .collect();
        println!(
            "{id:<30} {:>4} steps of {} s from {}  wind kn: {}",
            s.len(),
            s.step_s,
            s.start,
            first.join(" ")
        );
    }
    let with_values = data.series.iter().filter(|(_, s)| !s.is_empty()).count();
    println!(
        "{with_values} of {} models have values here",
        data.series.len()
    );
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
