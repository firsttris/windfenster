use std::io::IsTerminal;
mod alarms;

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
    Serve(Box<ServeArgs>),
    /// Apply pending database migrations and exit.
    Migrate(DataArgs),
    /// Fetch the station lists (OurAirports, DWD) now.
    ImportStations(DataArgs),
    /// Ask Open-Meteo for every model at a point and print what comes back.
    Probe(ProbeArgs),
}

/// `WF_BASEMAP_REGION`: a box, or none
#[derive(Debug, Clone, Copy)]
struct Region(Option<[f64; 4]>);

fn parse_region(s: &str) -> Result<Region, String> {
    if s.eq_ignore_ascii_case("none") {
        return Ok(Region(None));
    }
    let v: Vec<f64> = s
        .split(',')
        .map(|p| p.trim().parse::<f64>().map_err(|e| e.to_string()))
        .collect::<Result<_, _>>()?;
    match v[..] {
        [w, south, e, n]
            if (-180.0..=180.0).contains(&w)
                && (-180.0..=180.0).contains(&e)
                && (-90.0..=90.0).contains(&south)
                && (-90.0..=90.0).contains(&n)
                && w < e
                && south < n =>
        {
            Ok(Region(Some([w, south, e, n])))
        }
        _ => Err("expected west,south,east,north in degrees, or none".into()),
    }
}

#[derive(Args)]
struct DataArgs {
    /// Directory for the database and the map textures.
    #[arg(long, env = "WF_DATA_DIR", default_value = "/data")]
    data_dir: PathBuf,
    /// For tests: every outside source (Open-Meteo, aviationweather.gov, the DWD, PEGELONLINE,
    /// OurAirports, Protomaps, Mapterhorn) under this one address, as the browser tests'
    /// stand-in serves them (web/e2e/stand-in).
    #[arg(long, env = "WF_SOURCES")]
    sources: Option<String>,
}

impl DataArgs {
    /// The address of one outside source under `WF_SOURCES`, if set
    fn source(&self, path: &str) -> Option<String> {
        non_empty(&self.sources).map(|base| format!("{}/{path}", base.trim_end_matches('/')))
    }

    fn station_sources(&self) -> Sources {
        match self.source("") {
            Some(_) => Sources {
                ourairports_base: self.source("ourairports").unwrap_or_default(),
                awc_stations_url: self
                    .source("awc/data/cache/stations.cache.json.gz")
                    .unwrap_or_default(),
                dwd_url: self.source("dwd/stations.txt").unwrap_or_default(),
            },
            None => Sources::default(),
        }
    }

    fn measurement_urls(&self) -> Urls {
        match self.source("") {
            Some(_) => Urls {
                awc_base: self.source("awc").unwrap_or_default(),
                dwd_base: self.source("dwd").unwrap_or_default(),
                pegelonline_base: self.source("pegelonline").unwrap_or_default(),
            },
            None => Urls::default(),
        }
    }

    fn map_urls(&self) -> wf_map::Urls {
        match self.source("") {
            Some(_) => wf_map::Urls {
                icon_d2: self.source("icon-d2").unwrap_or_default(),
                icon_global: self.source("icon-global").unwrap_or_default(),
                ecmwf: self.source("ecmwf").unwrap_or_default(),
                protomaps: self.source("protomaps").unwrap_or_default(),
                mapterhorn: self.source("mapterhorn").unwrap_or_default(),
            },
            None => wf_map::Urls::default(),
        }
    }
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
    /// ICON global on the map (13 km, hourly to +78 h): the DWD's 00 and 12 UTC runs, about 1.1
    /// GB each to fetch and 1.5 GB on disk; `false` leaves the world to ECMWF.
    #[arg(long, env = "WF_ICON_GLOBAL", default_value_t = true, action = clap::ArgAction::Set)]
    icon_global: bool,
    /// The map's detailed basemap and relief: `auto` cuts them from Protomaps' daily build and
    /// Mapterhorn's elevation (the world to `WF_BASEMAP_WORLD_ZOOM`, the region to
    /// `WF_BASEMAP_ZOOM`, 20 km around places people look at to zoom 14) unless the operator
    /// put one in place; `off` leaves it to the operator.
    #[arg(long, env = "WF_BASEMAP", default_value = "auto", value_parser = ["auto", "off"])]
    basemap: String,
    /// How deep the basemap covers the whole world; every level about four times more.
    #[arg(long, env = "WF_BASEMAP_WORLD_ZOOM", default_value_t = 7, value_parser = clap::value_parser!(u8).range(4..=12))]
    basemap_world_zoom: u8,
    /// The region the basemap covers deeper, `west,south,east,north` in degrees (Europe by
    /// default), or `none`.
    #[arg(long, env = "WF_BASEMAP_REGION", default_value = "-25,27,45,72", value_parser = parse_region)]
    basemap_region: Region,
    /// How deep the basemap covers the region: 8 is about 190 MB for Europe, every level about
    /// four times more (12 about 7 GB); from 14 on no spots are needed there.
    #[arg(long, env = "WF_BASEMAP_ZOOM", default_value_t = 8, value_parser = clap::value_parser!(u8).range(6..=15))]
    basemap_zoom: u8,
    /// Spots with detail to zoom 14 kept at most, about 10 MB each; 0 makes none.
    #[arg(long, env = "WF_BASEMAP_SPOTS", default_value_t = 60)]
    basemap_spots: usize,
    /// How deep the relief goes, the whole world: 7 is 948 MB, every level about three to four
    /// times more, 12 at most; 0 turns it off.
    #[arg(long, env = "WF_TERRAIN_ZOOM", default_value_t = 7, value_parser = clap::value_parser!(u8).range(0..=12))]
    terrain_zoom: u8,
    /// Vite dev server to redirect to while no SPA is built in (`pnpm dev`).
    #[arg(long, env = "WF_DEV_UI")]
    dev_ui: Option<String>,
    #[command(flatten)]
    mail: alarms::MailArgs,
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
    match cli.command.unwrap_or(Command::Serve(Box::new(cli.serve))) {
        Command::Migrate(data) => {
            open(&data).await?;
            Ok(())
        }
        Command::ImportStations(data) => {
            let db = open(&data).await?;
            let http = wf_ingest::http_client()?;
            Ok(stations::import_all(&db, &http, &data.station_sources()).await?)
        }
        Command::Probe(args) => probe(args).await,
        Command::Serve(args) => serve(*args).await,
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
    let mut cfg = args.open_meteo.config();
    if cfg.base_url.is_none() {
        cfg.base_url = args.data.source("open-meteo");
    }
    let api_key = cfg.api_key.clone();
    let forecasts = Arc::new(Forecasts::new(
        db.clone(),
        OpenMeteo::new(http.clone(), cfg),
        models,
    ));
    let geocoder = Arc::new(Geocoder::new(
        http.clone(),
        non_empty(&args.geocoding_url).or_else(|| args.data.source("geocoding")),
        api_key,
    ));
    let measurements = Arc::new(Measurements::new(
        db.clone(),
        http.clone(),
        args.data.measurement_urls(),
    ));
    let map = args.map.then(|| {
        Arc::new(
            MapData::new(
                args.data.data_dir.join("map"),
                http.clone(),
                args.data.map_urls(),
            )
            .with_depth(wf_map::basemap::Depth {
                world_zoom: args.basemap_world_zoom,
                region: args.basemap_region.0,
                region_zoom: args.basemap_zoom,
                max_spots: args.basemap_spots,
                terrain_zoom: args.terrain_zoom,
            }),
        )
    });
    let alarms = Arc::new(alarms::build(
        db.clone(),
        &args.data.data_dir,
        http.clone(),
        map.clone(),
        &args.mail,
        non_empty(&args.public_url),
    )?);
    let tracked: Vec<String> = args
        .tracked_stations
        .iter()
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    if let (true, Some(map)) = (args.scheduler, &map) {
        tokio::spawn(map.clone().keep_current());
        if args.icon_global {
            tokio::spawn(map.clone().keep_icon_global());
        }
        if args.basemap == "auto" {
            tokio::spawn(map.clone().keep_basemap(db.clone()));
        }
    }
    if args.scheduler {
        tokio::spawn(alarms.clone().run());
        tokio::spawn(
            Scheduler {
                db: db.clone(),
                http,
                forecasts: forecasts.clone(),
                measurements: measurements.clone(),
                sources: args.data.station_sources(),
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
    state = state.with_alarms(alarms);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_region() {
        assert_eq!(
            parse_region("-25,27,45,72").unwrap().0,
            Some([-25.0, 27.0, 45.0, 72.0])
        );
        assert_eq!(
            parse_region(" -10, 30 ,5,45").unwrap().0,
            Some([-10.0, 30.0, 5.0, 45.0])
        );
        assert_eq!(parse_region("none").unwrap().0, None);
        for bad in ["", "1,2,3", "45,27,-25,72", "-25,27,45,95", "a,b,c,d"] {
            assert!(parse_region(bad).is_err(), "{bad}");
        }
    }
}
