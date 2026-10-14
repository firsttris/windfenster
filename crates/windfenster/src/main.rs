use std::io::IsTerminal;
mod admin;
mod alarms;
mod log_board;
mod settings;
mod supervise;

use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use anyhow::{Context, Result};
use clap::{Args, Parser, Subcommand};
use tokio::net::TcpListener;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{EnvFilter, Layer};
use wf_core::db::{self, Db};
use wf_core::geo::GridPoint;
use wf_ingest::forecasts::Forecasts;
use wf_ingest::geocode::Geocoder;
use wf_ingest::measurements::{Measurements, Urls};
use wf_ingest::openmeteo::{self, OpenMeteo};
use wf_ingest::scheduler::Scheduler;
use wf_ingest::stations::{self, Sources};
use wf_map::MapData;

use crate::supervise::supervise;

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
    /// Fetch the station lists (OurAirports, DWD) and the water gauges (PEGELONLINE) now.
    ImportStations(DataArgs),
    /// Ask Open-Meteo for every model at a point and print what comes back.
    Probe(ProbeArgs),
    /// Make an account an admin (`--remove` takes it back, `--list` shows the admins).
    Admin(admin::AdminArgs),
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
pub struct DataArgs {
    /// Directory for the database and the map textures.
    #[arg(long, env = "WF_DATA_DIR", default_value = "/data")]
    data_dir: PathBuf,
    /// For tests: every outside source (Open-Meteo, aviationweather.gov, the DWD, PEGELONLINE,
    /// OurAirports, the Iowa Environmental Mesonet, Protomaps, Mapterhorn) under this one address, as the browser tests'
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

    fn climate_urls(&self) -> wf_ingest::climate::Urls {
        match self.source("") {
            Some(_) => wf_ingest::climate::Urls {
                iem: self.source("iem").unwrap_or_default(),
                dwd_hourly: self.source("dwd-hourly").unwrap_or_default(),
            },
            None => wf_ingest::climate::Urls::default(),
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
    /// Header the proxy in front puts the client's address in, for the per-client limits on
    /// what the server fetches: `CF-Connecting-IP` behind Cloudflare, `X-Forwarded-For` behind
    /// Caddy or nginx open to the internet; `none` takes the connection's address.
    #[arg(long, env = "WF_CLIENT_IP_HEADER", default_value = "CF-Connecting-IP")]
    client_ip_header: String,
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
    /// put one in place; `ask` waits for an admin to say when on the admin page; `off` leaves it
    /// to the operator.
    #[arg(long, env = "WF_BASEMAP", default_value = "auto", value_parser = ["auto", "ask", "off"])]
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

/// What goes wrong without a public https address: said at start, since a server reached from
/// the internet needs one.
fn public_url_warning(public_url: Option<&str>) -> Option<&'static str> {
    match public_url {
        None => Some(
            "WF_PUBLIC_URL is not set: passkeys work on http://localhost:8080 only, the session \
             cookie is not marked Secure and e-mail addresses cannot be confirmed. Set it to the \
             address people open, e.g. https://windfenster.example",
        ),
        Some(u) if !u.starts_with("https://") && !u.starts_with("http://localhost") => Some(
            "WF_PUBLIC_URL is not https: the session cookie is not marked Secure and browsers \
             refuse passkeys and push messages there",
        ),
        Some(_) => None,
    }
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_ref().filter(|s| !s.trim().is_empty()).cloned()
}

#[tokio::main]
async fn main() -> Result<()> {
    // Development: read `.env` from the working directory or a parent. Variables already set
    // in the environment win, so Docker configuration is unaffected.
    let dotenv = dotenvy::dotenv();
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info,sqlx=warn"));
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::fmt::layer()
                // Colours only on a terminal, not in `docker logs`.
                .with_ansi(std::io::stdout().is_terminal())
                .with_filter(filter),
        )
        // Warnings and errors for the admin page, whatever RUST_LOG says
        .with(log_board::ToBoard.with_filter(tracing_subscriber::filter::LevelFilter::WARN))
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
            stations::import_all(&db, &http, &data.station_sources()).await?;
            // The gauges are a list of places too; without them the stations still stand.
            let gauges = Measurements::new(db, http, data.measurement_urls());
            match gauges.import_gauges().await {
                Ok(n) => tracing::info!(gauges = n, "gauges imported"),
                Err(e) => tracing::warn!(error = %e, "gauges not imported"),
            }
            Ok(())
        }
        Command::Probe(args) => probe(args).await,
        Command::Admin(args) => {
            let db = open(&args.data).await?;
            admin::run(&db, &args).await
        }
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
    // Unknown ids in WF_MODELS stop the start, as before the admin page
    wf_core::model::active(&db, args.open_meteo.models.as_deref()).await?;
    let models = wf_core::model::active(&db, None).await?;
    let settings =
        wf_core::admin::settings::SettingsStore::load(db.clone(), settings::from_env(&args))
            .await?;
    let mut cfg = args.open_meteo.config();
    if cfg.base_url.is_none() {
        cfg.base_url = args.data.source("open-meteo");
    }
    let api_key = cfg.api_key.clone();
    let forecasts = Arc::new(
        Forecasts::new(db.clone(), OpenMeteo::new(http.clone(), cfg), models)
            .with_settings(settings.clone()),
    );
    tracing::info!(
        "models: {}",
        forecasts
            .models()
            .iter()
            .map(|m| m.id.as_str())
            .collect::<Vec<_>>()
            .join(", ")
    );
    forecasts.open_meteo().load_usage().await?;
    let geocoder = Arc::new(Geocoder::new(
        http.clone(),
        non_empty(&args.geocoding_url).or_else(|| args.data.source("geocoding")),
        api_key,
    ));
    let mut climate =
        wf_ingest::climate::Climate::new(db.clone(), http.clone(), args.data.climate_urls());
    if args.data.source("").is_some() {
        // A stand-in is ours: no need to pace the requests as the IEM asks
        climate = climate.with_gap(std::time::Duration::ZERO);
    }
    let climate = Arc::new(climate);
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
            .with_settings(settings.clone()),
        )
    });
    let alarms = Arc::new(alarms::build(
        db.clone(),
        &args.data.data_dir,
        map.clone(),
        &args.mail,
        non_empty(&args.public_url),
    )?);
    if let Some(mail) = &alarms.mail {
        mail.set_per_day(settings.get().mail_per_day);
    }
    if let (true, Some(map)) = (args.scheduler, &map) {
        let m = map.clone();
        supervise("map", move || m.clone().keep_current());
        // Both follow their settings: ICON global and the basemap can be turned on and off on
        // the admin page
        let m = map.clone();
        supervise("icon_global", move || m.clone().keep_icon_global());
        let (m, db) = (map.clone(), db.clone());
        supervise("basemap", move || m.clone().keep_basemap(db.clone()));
    }
    if args.scheduler {
        let a = alarms.clone();
        supervise("alarms", move || a.clone().run());
        let (db, forecasts, measurements) = (db.clone(), forecasts.clone(), measurements.clone());
        let sources = args.data.station_sources();
        let scheduler_settings = settings.clone();
        supervise("scheduler", move || {
            Scheduler {
                db: db.clone(),
                http: http.clone(),
                forecasts: forecasts.clone(),
                measurements: measurements.clone(),
                sources: sources.clone(),
                settings: scheduler_settings.clone(),
            }
            .run()
        });
    }
    if let Some(warning) = public_url_warning(non_empty(&args.public_url).as_deref()) {
        tracing::warn!("{warning}");
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
    state = state
        .with_climate(climate)
        .with_data_dir(args.data.data_dir.clone())
        .with_settings(settings);
    state = state.with_client_ip_header(match args.client_ip_header.trim() {
        "" | "none" => None,
        h => Some(
            axum::http::HeaderName::try_from(h.to_ascii_lowercase())
                .context("WF_CLIENT_IP_HEADER is no header name")?,
        ),
    });
    let listener = TcpListener::bind(args.listen)
        .await
        .with_context(|| format!("binding {}", args.listen))?;
    tracing::info!("windfenster {} on http://{}", wf_api::VERSION, args.listen);
    axum::serve(
        listener,
        wf_api::router(state).into_make_service_with_connect_info::<SocketAddr>(),
    )
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
    fn warns_without_a_public_https_address() {
        assert!(public_url_warning(None).is_some());
        assert!(public_url_warning(Some("http://wind.example")).is_some());
        assert!(public_url_warning(Some("https://wind.example")).is_none());
        assert!(public_url_warning(Some("http://localhost:8080")).is_none());
    }

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
