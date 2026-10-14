//! The admin page's overview: what needs a decision, the day's numbers, every background task and
//! every outside source; and asking a task to run now.

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::Serialize;
use ts_rs::TS;
use wf_core::admin::users::{self, UserSummary};
use wf_core::status::{self, Outcome, SourceStatus, TaskStatus};
use wf_map::disk::DiskUsage;

use super::Admin;
use crate::{ApiError, AppState};

/// Something an admin should look at; the page words it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(
    tag = "code",
    rename_all = "snake_case",
    rename_all_fields = "camelCase"
)]
#[ts(export)]
pub enum Warning {
    /// Without WF_PUBLIC_URL passkeys work on localhost only, addresses cannot be confirmed
    PublicUrlMissing,
    /// Cookies are not Secure, browsers refuse passkeys and push
    PublicUrlNotHttps,
    /// Requests came without the client-address header (WF_CLIENT_IP_HEADER): no proxy that
    /// sets it is in front, and anyone can set it to pass the limits per client
    IpHeaderMissing {
        #[ts(type = "number")]
        count: u64,
    },
    TaskFailed {
        task: String,
    },
    /// A source failed more than a few times in the last day
    SourceFailing {
        source: String,
        errors: u32,
    },
    LoopPanicked {
        name: String,
        count: u32,
    },
    /// Open-Meteo's daily budget is mostly used
    BudgetHigh {
        percent: u32,
    },
    /// Less than 2 GB free for the data folder
    DiskLow {
        #[ts(type = "number")]
        free: u64,
    },
    /// A newer Windfenster is out
    NewVersion {
        version: String,
        url: String,
    },
    /// The basemap or relief is due and waits for an admin (mode "ask")
    BasemapWaiting,
    /// A public instance with accounts and no legal notice or privacy policy
    LegalMissing,
}

/// Open-Meteo calls today against the budget
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Budget {
    pub refresh: u32,
    pub opened: u32,
    /// null: unlimited (with a key)
    pub limit: Option<u32>,
    pub opened_limit: Option<u32>,
}

/// Alarm messages today
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AlarmsToday {
    #[ts(type = "number")]
    pub push: i64,
    #[ts(type = "number")]
    pub email: i64,
    #[ts(type = "number")]
    pub failed: i64,
}

#[derive(Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AdminOverview {
    pub version: String,
    #[ts(type = "number")]
    pub started_at: i64,
    #[ts(type = "number")]
    pub schema_version: i64,
    pub warnings: Vec<Warning>,
    pub tasks: Vec<TaskStatus>,
    pub sources: Vec<SourceStatus>,
    pub budget: Budget,
    pub disk: Option<DiskUsage>,
    pub users: UserSummary,
    pub alarms: AlarmsToday,
}

/// Less free than this for the data folder is worth a warning
const DISK_LOW: u64 = 2_000_000_000;
/// A source failing this often in a day is worth a warning
const SOURCE_ERRORS: u32 = 3;

pub async fn overview(
    State(s): State<AppState>,
    _: Admin,
) -> Result<Json<AdminOverview>, ApiError> {
    let now = wf_core::time::now();
    let board = status::board();
    let tasks = board.tasks();
    let sources = board.sources();
    let usage = s.forecasts.open_meteo().usage();
    let budget = Budget {
        refresh: usage.refresh,
        opened: usage.opened,
        limit: usage.limit,
        opened_limit: usage.opened_limit,
    };
    let disk = s.disk_usage().await;
    let (push, email, failed) =
        wf_core::alarm::delivery::counts(&s.db, now - now.rem_euclid(86_400)).await?;
    let users = users::summary(&s.db, now).await?;
    let mut warnings = warnings(&s, &tasks, &sources, &budget, disk.as_ref());
    if let Some(r) = board
        .release()
        .filter(|r| status::newer(&r.version, crate::VERSION))
    {
        warnings.insert(
            0,
            Warning::NewVersion {
                version: r.version,
                url: r.url,
            },
        );
    }
    if let Some(map) = &s.map
        && map.renewal_waiting().await
    {
        warnings.insert(0, Warning::BasemapWaiting);
    }
    let settings = s.settings.get();
    let public = s
        .public_url
        .as_deref()
        .is_some_and(|u| u.starts_with("https://"));
    if public
        && users.total > 0
        && (settings.imprint.trim().is_empty() || settings.privacy.trim().is_empty())
    {
        warnings.push(Warning::LegalMissing);
    }
    Ok(Json(AdminOverview {
        version: crate::VERSION.to_owned(),
        started_at: board.started_at(),
        schema_version: wf_core::db::schema_version(&s.db).await?,
        warnings,
        tasks,
        sources,
        budget,
        disk,
        users,
        alarms: AlarmsToday {
            push,
            email,
            failed,
        },
    }))
}

fn warnings(
    s: &AppState,
    tasks: &[TaskStatus],
    sources: &[SourceStatus],
    budget: &Budget,
    disk: Option<&DiskUsage>,
) -> Vec<Warning> {
    let mut out = Vec::new();
    match s.public_url.as_deref() {
        None => out.push(Warning::PublicUrlMissing),
        Some(u) if !u.starts_with("https://") => out.push(Warning::PublicUrlNotHttps),
        Some(_) => {}
    }
    let missing = status::board().missing_ip_headers();
    if s.client_ip_header.is_some() && missing > 0 {
        out.push(Warning::IpHeaderMissing { count: missing });
    }
    for t in tasks.iter().filter(|t| t.outcome == Outcome::Failed) {
        out.push(Warning::TaskFailed { task: t.id.clone() });
    }
    for src in sources.iter().filter(|x| x.errors_day >= SOURCE_ERRORS) {
        out.push(Warning::SourceFailing {
            source: src.id.clone(),
            errors: src.errors_day,
        });
    }
    for (name, count) in status::board().panics() {
        out.push(Warning::LoopPanicked { name, count });
    }
    if let Some(limit) = budget.limit.filter(|l| *l > 0) {
        let percent = (budget.refresh + budget.opened) * 100 / limit;
        if percent >= 80 {
            out.push(Warning::BudgetHigh { percent });
        }
    }
    if let Some(d) = disk.filter(|d| d.free > 0 && d.free < DISK_LOW) {
        out.push(Warning::DiskLow { free: d.free });
    }
    out
}

/// `POST /api/admin/tasks/{id}/run`
pub async fn run(
    admin: Admin,
    State(s): State<AppState>,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if !status::board().ask(&id) {
        return Err(ApiError::NotFound);
    }
    admin.log(&s, "task.run", Some(&id)).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /api/admin/log/lines`: the server log's last warnings and errors, newest first
pub async fn log_lines(_: Admin) -> Json<Vec<status::LogLine>> {
    Json(status::board().log_lines())
}
