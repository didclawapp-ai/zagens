//! Durable automation records and scheduler support.
//!
//! Automations are local-first recurring jobs that enqueue standard background
//! tasks. This module stores automation definitions and run history under
//! `~/.zagens/automations` (or `ZAGENS_AUTOMATIONS_DIR` / `DEEPSEEK_AUTOMATIONS_DIR`).

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result, bail};
use chrono::{DateTime, Datelike, Duration, Local, TimeZone, Timelike, Utc, Weekday};
use serde::{Deserialize, Serialize};
use tokio::sync::Mutex;
use tokio::time::sleep;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::task_manager::{NewTaskRequest, SharedTaskManager, TaskStatus};
use crate::utils::spawn_supervised;

const CURRENT_AUTOMATION_SCHEMA_VERSION: u32 = 2;
const CURRENT_RUN_SCHEMA_VERSION: u32 = 1;

const fn default_automation_schema_version() -> u32 {
    CURRENT_AUTOMATION_SCHEMA_VERSION
}

const fn default_run_schema_version() -> u32 {
    CURRENT_RUN_SCHEMA_VERSION
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationStatus {
    Active,
    Paused,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AutomationRunStatus {
    Queued,
    Running,
    Completed,
    Failed,
    Canceled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum AutomationTriggerKind {
    /// Enqueue a background task with conservative defaults (agent, no shell).
    #[default]
    Prompt,
    /// Enqueue a background task using stored model/mode/workspace/shell/trust flags.
    Task,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRecord {
    #[serde(default = "default_automation_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub rrule: String,
    #[serde(default)]
    pub cwds: Vec<PathBuf>,
    #[serde(default)]
    pub trigger_kind: AutomationTriggerKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allow_shell: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trust_mode: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_approve: Option<bool>,
    pub status: AutomationStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub next_run_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_run_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomationRunRecord {
    #[serde(default = "default_run_schema_version")]
    pub schema_version: u32,
    pub id: String,
    pub automation_id: String,
    pub scheduled_for: DateTime<Utc>,
    pub status: AutomationRunStatus,
    pub created_at: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub started_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ended_at: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub turn_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateAutomationRequest {
    pub name: String,
    pub prompt: String,
    pub rrule: String,
    #[serde(default)]
    pub cwds: Vec<PathBuf>,
    #[serde(default)]
    pub trigger_kind: AutomationTriggerKind,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub mode: Option<String>,
    #[serde(default)]
    pub allow_shell: Option<bool>,
    #[serde(default)]
    pub trust_mode: Option<bool>,
    #[serde(default)]
    pub auto_approve: Option<bool>,
    #[serde(default)]
    pub status: Option<AutomationStatus>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct UpdateAutomationRequest {
    pub name: Option<String>,
    pub prompt: Option<String>,
    pub rrule: Option<String>,
    pub cwds: Option<Vec<PathBuf>>,
    pub trigger_kind: Option<AutomationTriggerKind>,
    pub model: Option<String>,
    pub mode: Option<String>,
    pub allow_shell: Option<bool>,
    pub trust_mode: Option<bool>,
    pub auto_approve: Option<bool>,
    pub status: Option<AutomationStatus>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AutomationFrequency {
    Minutely,
    Hourly,
    Daily,
    Weekly,
    Monthly,
    Once,
}

#[derive(Debug, Clone)]
pub enum AutomationSchedule {
    Minutely {
        interval_minutes: u32,
        byday: Option<Vec<Weekday>>,
    },
    Hourly {
        interval_hours: u32,
        byday: Option<Vec<Weekday>>,
    },
    Daily {
        interval_days: u32,
        byhour: u32,
        byminute: u32,
    },
    Weekly {
        byday: Vec<Weekday>,
        byhour: u32,
        byminute: u32,
    },
    Monthly {
        interval_months: u32,
        bymonthday: u32,
        byhour: u32,
        byminute: u32,
    },
    Once {
        at: DateTime<Utc>,
    },
}

impl AutomationSchedule {
    pub fn parse_rrule(rrule: &str) -> Result<Self> {
        let mut parts: BTreeMap<String, String> = BTreeMap::new();
        for raw in rrule.split(';') {
            let item = raw.trim();
            if item.is_empty() {
                continue;
            }
            let Some((k, v)) = item.split_once('=') else {
                bail!("Invalid RRULE segment '{item}'");
            };
            parts.insert(k.trim().to_ascii_uppercase(), v.trim().to_ascii_uppercase());
        }

        let freq = match parts.get("FREQ").map(String::as_str) {
            Some("MINUTELY") => AutomationFrequency::Minutely,
            Some("HOURLY") => AutomationFrequency::Hourly,
            Some("DAILY") => AutomationFrequency::Daily,
            Some("WEEKLY") => AutomationFrequency::Weekly,
            Some("MONTHLY") => AutomationFrequency::Monthly,
            Some("ONCE") => AutomationFrequency::Once,
            Some(other) => bail!(
                "Unsupported RRULE FREQ '{other}'. Supported: MINUTELY, HOURLY, DAILY, WEEKLY, MONTHLY, ONCE"
            ),
            None => bail!("RRULE must include FREQ"),
        };

        match freq {
            AutomationFrequency::Minutely => {
                for key in parts.keys() {
                    if key != "FREQ" && key != "INTERVAL" && key != "BYDAY" {
                        bail!(
                            "Unsupported RRULE field '{key}' for MINUTELY. Allowed: FREQ,INTERVAL,BYDAY"
                        );
                    }
                }
                let interval_minutes = parts
                    .get("INTERVAL")
                    .map(|v| v.parse::<u32>())
                    .transpose()
                    .context("Failed to parse INTERVAL")?
                    .unwrap_or(1);
                if interval_minutes == 0 {
                    bail!("INTERVAL must be >= 1 for MINUTELY schedules");
                }
                let byday = parts.get("BYDAY").map(|v| parse_byday(v)).transpose()?;
                Ok(Self::Minutely {
                    interval_minutes,
                    byday,
                })
            }
            AutomationFrequency::Hourly => {
                for key in parts.keys() {
                    if key != "FREQ" && key != "INTERVAL" && key != "BYDAY" {
                        bail!(
                            "Unsupported RRULE field '{key}' for HOURLY. Allowed: FREQ,INTERVAL,BYDAY"
                        );
                    }
                }
                let interval_hours = parts
                    .get("INTERVAL")
                    .map(|v| v.parse::<u32>())
                    .transpose()
                    .context("Failed to parse INTERVAL")?
                    .unwrap_or(1);
                if interval_hours == 0 {
                    bail!("INTERVAL must be >= 1 for HOURLY schedules");
                }
                let byday = parts
                    .get("BYDAY")
                    .map(|value| parse_byday(value))
                    .transpose()?;
                Ok(Self::Hourly {
                    interval_hours,
                    byday,
                })
            }
            AutomationFrequency::Daily => {
                for key in parts.keys() {
                    if key != "FREQ" && key != "INTERVAL" && key != "BYHOUR" && key != "BYMINUTE" {
                        bail!(
                            "Unsupported RRULE field '{key}' for DAILY. Allowed: FREQ,INTERVAL,BYHOUR,BYMINUTE"
                        );
                    }
                }
                let interval_days = parts
                    .get("INTERVAL")
                    .map(|v| v.parse::<u32>())
                    .transpose()
                    .context("Failed to parse INTERVAL")?
                    .unwrap_or(1);
                if interval_days == 0 {
                    bail!("INTERVAL must be >= 1 for DAILY schedules");
                }
                let byhour = parts
                    .get("BYHOUR")
                    .ok_or_else(|| anyhow::anyhow!("DAILY schedules require BYHOUR"))?
                    .parse::<u32>()
                    .context("Failed to parse BYHOUR")?;
                let byminute = parts
                    .get("BYMINUTE")
                    .ok_or_else(|| anyhow::anyhow!("DAILY schedules require BYMINUTE"))?
                    .parse::<u32>()
                    .context("Failed to parse BYMINUTE")?;
                validate_hm(byhour, byminute)?;
                Ok(Self::Daily {
                    interval_days,
                    byhour,
                    byminute,
                })
            }
            AutomationFrequency::Weekly => {
                for key in parts.keys() {
                    if key != "FREQ" && key != "BYDAY" && key != "BYHOUR" && key != "BYMINUTE" {
                        bail!(
                            "Unsupported RRULE field '{key}' for WEEKLY. Allowed: FREQ,BYDAY,BYHOUR,BYMINUTE"
                        );
                    }
                }
                let byday_raw = parts
                    .get("BYDAY")
                    .ok_or_else(|| anyhow::anyhow!("WEEKLY schedules require BYDAY"))?;
                let byday = parse_byday(byday_raw)?;
                if byday.is_empty() {
                    bail!("BYDAY cannot be empty for WEEKLY schedules");
                }
                let byhour = parts
                    .get("BYHOUR")
                    .ok_or_else(|| anyhow::anyhow!("WEEKLY schedules require BYHOUR"))?
                    .parse::<u32>()
                    .context("Failed to parse BYHOUR")?;
                let byminute = parts
                    .get("BYMINUTE")
                    .ok_or_else(|| anyhow::anyhow!("WEEKLY schedules require BYMINUTE"))?
                    .parse::<u32>()
                    .context("Failed to parse BYMINUTE")?;

                if byhour > 23 {
                    bail!("BYHOUR must be between 0 and 23");
                }
                if byminute > 59 {
                    bail!("BYMINUTE must be between 0 and 59");
                }

                Ok(Self::Weekly {
                    byday,
                    byhour,
                    byminute,
                })
            }
            AutomationFrequency::Monthly => {
                for key in parts.keys() {
                    if key != "FREQ"
                        && key != "INTERVAL"
                        && key != "BYMONTHDAY"
                        && key != "BYHOUR"
                        && key != "BYMINUTE"
                    {
                        bail!(
                            "Unsupported RRULE field '{key}' for MONTHLY. Allowed: FREQ,INTERVAL,BYMONTHDAY,BYHOUR,BYMINUTE"
                        );
                    }
                }
                let interval_months = parts
                    .get("INTERVAL")
                    .map(|v| v.parse::<u32>())
                    .transpose()
                    .context("Failed to parse INTERVAL")?
                    .unwrap_or(1);
                if interval_months == 0 {
                    bail!("INTERVAL must be >= 1 for MONTHLY schedules");
                }
                let bymonthday = parts
                    .get("BYMONTHDAY")
                    .ok_or_else(|| anyhow::anyhow!("MONTHLY schedules require BYMONTHDAY"))?
                    .parse::<u32>()
                    .context("Failed to parse BYMONTHDAY")?;
                if !(1..=31).contains(&bymonthday) {
                    bail!("BYMONTHDAY must be between 1 and 31");
                }
                let byhour = parts
                    .get("BYHOUR")
                    .ok_or_else(|| anyhow::anyhow!("MONTHLY schedules require BYHOUR"))?
                    .parse::<u32>()
                    .context("Failed to parse BYHOUR")?;
                let byminute = parts
                    .get("BYMINUTE")
                    .ok_or_else(|| anyhow::anyhow!("MONTHLY schedules require BYMINUTE"))?
                    .parse::<u32>()
                    .context("Failed to parse BYMINUTE")?;
                validate_hm(byhour, byminute)?;
                Ok(Self::Monthly {
                    interval_months,
                    bymonthday,
                    byhour,
                    byminute,
                })
            }
            AutomationFrequency::Once => {
                for key in parts.keys() {
                    if key != "FREQ" && key != "DTSTART" {
                        bail!("Unsupported RRULE field '{key}' for ONCE. Allowed: FREQ,DTSTART");
                    }
                }
                let dtstart = parts
                    .get("DTSTART")
                    .ok_or_else(|| anyhow::anyhow!("ONCE schedules require DTSTART"))?;
                let at = parse_dtstart(dtstart)?;
                Ok(Self::Once { at })
            }
        }
    }

    /// Returns the next fire time after `after`, or `None` when the schedule is exhausted (ONCE).
    pub fn next_after_opt(&self, after: DateTime<Utc>) -> Result<Option<DateTime<Utc>>> {
        match self.next_after(after) {
            Ok(dt) => Ok(Some(dt)),
            Err(err) if err.to_string().contains("ONCE schedule exhausted") => Ok(None),
            Err(err) => Err(err),
        }
    }

    pub fn is_once(&self) -> bool {
        matches!(self, Self::Once { .. })
    }

    pub fn next_after(&self, after: DateTime<Utc>) -> Result<DateTime<Utc>> {
        let local_after = after.with_timezone(&Local);
        match self {
            Self::Minutely {
                interval_minutes,
                byday,
            } => {
                let mut candidate = local_after + Duration::minutes(i64::from(*interval_minutes))
                    - Duration::seconds(i64::from(local_after.second()))
                    - Duration::nanoseconds(i64::from(local_after.nanosecond()));

                if let Some(days) = byday {
                    for _ in 0..(24 * 60 * 14) {
                        if days.contains(&candidate.weekday()) {
                            return Ok(candidate.with_timezone(&Utc));
                        }
                        candidate += Duration::minutes(i64::from(*interval_minutes));
                    }
                    bail!("Unable to compute next MINUTELY run for BYDAY filter");
                }

                Ok(candidate.with_timezone(&Utc))
            }
            Self::Hourly {
                interval_hours,
                byday,
            } => {
                let mut candidate = local_after + Duration::hours(i64::from(*interval_hours))
                    - Duration::seconds(i64::from(local_after.second()))
                    - Duration::nanoseconds(i64::from(local_after.nanosecond()));

                if let Some(days) = byday {
                    for _ in 0..(24 * 21) {
                        if days.contains(&candidate.weekday()) {
                            return Ok(candidate.with_timezone(&Utc));
                        }
                        candidate += Duration::hours(i64::from(*interval_hours));
                    }
                    bail!("Unable to compute next HOURLY run for BYDAY filter");
                }

                Ok(candidate.with_timezone(&Utc))
            }
            Self::Daily {
                interval_days,
                byhour,
                byminute,
            } => next_daily(local_after, *interval_days, *byhour, *byminute),
            Self::Weekly {
                byday,
                byhour,
                byminute,
            } => {
                for day_offset in 0..15 {
                    let date = local_after.date_naive() + Duration::days(i64::from(day_offset));
                    if !byday.contains(&date.weekday()) {
                        continue;
                    }
                    if let Some(candidate) =
                        local_datetime_on_date(date, *byhour, *byminute, &local_after)
                    {
                        return Ok(candidate.with_timezone(&Utc));
                    }
                }
                bail!("Unable to compute next WEEKLY run");
            }
            Self::Monthly {
                interval_months,
                bymonthday,
                byhour,
                byminute,
            } => next_monthly(
                local_after,
                *interval_months,
                *bymonthday,
                *byhour,
                *byminute,
            ),
            Self::Once { at } => {
                if *at > after {
                    Ok(*at)
                } else {
                    bail!("ONCE schedule exhausted");
                }
            }
        }
    }
}

fn resolve_local_datetime(naive: chrono::NaiveDateTime) -> Option<DateTime<Local>> {
    Local
        .from_local_datetime(&naive)
        .single()
        .or_else(|| Local.from_local_datetime(&naive).earliest())
        .or_else(|| Local.from_local_datetime(&naive).latest())
}

fn validate_hm(byhour: u32, byminute: u32) -> Result<()> {
    if byhour > 23 {
        bail!("BYHOUR must be between 0 and 23");
    }
    if byminute > 59 {
        bail!("BYMINUTE must be between 0 and 59");
    }
    Ok(())
}

fn parse_dtstart(value: &str) -> Result<DateTime<Utc>> {
    if let Ok(dt) = DateTime::parse_from_rfc3339(value) {
        return Ok(dt.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S") {
        let local = resolve_local_datetime(naive)
            .with_context(|| format!("Ambiguous local DTSTART '{value}'"))?;
        return Ok(local.with_timezone(&Utc));
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(value, "%Y%m%dT%H%M%S") {
        let local = resolve_local_datetime(naive)
            .with_context(|| format!("Ambiguous local DTSTART '{value}'"))?;
        return Ok(local.with_timezone(&Utc));
    }
    bail!("Failed to parse DTSTART '{value}'. Use ISO-8601, e.g. 2026-06-10T09:00:00")
}

fn local_datetime_on_date(
    date: chrono::NaiveDate,
    byhour: u32,
    byminute: u32,
    after: &DateTime<Local>,
) -> Option<DateTime<Local>> {
    let naive = date.and_hms_opt(byhour, byminute, 0)?;
    let candidate = resolve_local_datetime(naive)?;
    if candidate > *after {
        Some(candidate)
    } else {
        None
    }
}

fn next_daily(
    local_after: DateTime<Local>,
    interval_days: u32,
    byhour: u32,
    byminute: u32,
) -> Result<DateTime<Utc>> {
    let interval = i64::from(interval_days.max(1));
    let mut date = local_after.date_naive();

    if let Some(naive) = date.and_hms_opt(byhour, byminute, 0) {
        if let Some(candidate) = resolve_local_datetime(naive) {
            if candidate > local_after {
                return Ok(candidate.with_timezone(&Utc));
            }
        }
    }

    date += Duration::days(interval);
    for _ in 0..400 {
        if let Some(naive) = date.and_hms_opt(byhour, byminute, 0) {
            if let Some(candidate) = resolve_local_datetime(naive) {
                return Ok(candidate.with_timezone(&Utc));
            }
        }
        date += Duration::days(interval);
    }
    bail!("Unable to compute next DAILY run");
}

fn month_day_clamped(year: i32, month: u32, bymonthday: u32) -> chrono::NaiveDate {
    let days_in = chrono::NaiveDate::from_ymd_opt(year, month, 1)
        .map(|d| {
            if month == 12 {
                chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
            } else {
                chrono::NaiveDate::from_ymd_opt(year, month + 1, 1)
            }
            .map(|next| (next - d).num_days() as u32)
            .unwrap_or(28)
        })
        .unwrap_or(28);
    let day = bymonthday.min(days_in);
    chrono::NaiveDate::from_ymd_opt(year, month, day).expect("valid month day")
}

fn advance_month(year: i32, month: u32, interval_months: u32) -> (i32, u32) {
    let total = i64::from(month - 1) + i64::from(interval_months.max(1));
    let new_year = year + (total / 12) as i32;
    let new_month = (total % 12 + 1) as u32;
    (new_year, new_month)
}

fn next_monthly(
    local_after: DateTime<Local>,
    interval_months: u32,
    bymonthday: u32,
    byhour: u32,
    byminute: u32,
) -> Result<DateTime<Utc>> {
    let interval = interval_months.max(1);
    let mut year = local_after.year();
    let mut month = local_after.month();

    for _ in 0..240 {
        let date = month_day_clamped(year, month, bymonthday);
        if let Some(candidate) = local_datetime_on_date(date, byhour, byminute, &local_after) {
            return Ok(candidate.with_timezone(&Utc));
        }
        (year, month) = advance_month(year, month, interval);
    }
    bail!("Unable to compute next MONTHLY run");
}

fn active_next_run_at(
    schedule: &AutomationSchedule,
    now: DateTime<Utc>,
) -> Result<Option<DateTime<Utc>>> {
    schedule.next_after_opt(now)
}

/// Advance `next_run_at` past `due_at`, skipping any already-elapsed slots so
/// that a sidecar that was offline for a while catches up in a single tick
/// rather than one missed slot per tick.
///
/// At most `MAX_CATCHUP_STEPS` slots are skipped to avoid burning through an
/// entire repeat-count in one tick (e.g. a MINUTELY rule offline for 1 hour).
const MAX_CATCHUP_STEPS: usize = 20;

fn apply_next_run_after_fire(
    automation: &mut AutomationRecord,
    schedule: &AutomationSchedule,
    due_at: DateTime<Utc>,
) -> Result<()> {
    let now = Utc::now();
    let mut cursor = due_at;
    let mut steps = 0;
    loop {
        match schedule.next_after_opt(cursor)? {
            Some(next) => {
                // If the next slot is still in the past (missed while offline),
                // keep advancing — up to the cap.
                if next <= now && steps < MAX_CATCHUP_STEPS {
                    cursor = next;
                    steps += 1;
                } else {
                    automation.next_run_at = Some(next);
                    return Ok(());
                }
            }
            None => {
                automation.next_run_at = None;
                automation.status = AutomationStatus::Paused;
                return Ok(());
            }
        }
    }
}

fn parse_byday(value: &str) -> Result<Vec<Weekday>> {
    let mut days = Vec::new();
    for token in value.split(',') {
        let day = match token.trim().to_ascii_uppercase().as_str() {
            "MO" => Weekday::Mon,
            "TU" => Weekday::Tue,
            "WE" => Weekday::Wed,
            "TH" => Weekday::Thu,
            "FR" => Weekday::Fri,
            "SA" => Weekday::Sat,
            "SU" => Weekday::Sun,
            other => bail!("Invalid BYDAY value '{other}'"),
        };
        if !days.contains(&day) {
            days.push(day);
        }
    }
    Ok(days)
}

#[derive(Debug, Clone)]
pub struct AutomationManager {
    automations_dir: PathBuf,
    runs_dir: PathBuf,
}

impl AutomationManager {
    pub fn open(root: PathBuf) -> Result<Self> {
        let automations_dir = root.join("automations");
        let runs_dir = root.join("runs");
        fs::create_dir_all(&automations_dir)
            .with_context(|| format!("Failed to create {}", automations_dir.display()))?;
        fs::create_dir_all(&runs_dir)
            .with_context(|| format!("Failed to create {}", runs_dir.display()))?;
        Ok(Self {
            automations_dir,
            runs_dir,
        })
    }

    pub fn default_location() -> Result<Self> {
        Self::open(default_automations_dir())
    }

    fn automation_path(&self, id: &str) -> PathBuf {
        self.automations_dir.join(format!("{id}.json"))
    }

    fn runs_dir_for(&self, automation_id: &str) -> PathBuf {
        self.runs_dir.join(automation_id)
    }

    fn run_path(&self, automation_id: &str, run_id: &str) -> PathBuf {
        self.runs_dir_for(automation_id)
            .join(format!("{run_id}.json"))
    }

    pub fn create_automation(&self, req: CreateAutomationRequest) -> Result<AutomationRecord> {
        validate_name_and_prompt(&req.name, &req.prompt)?;
        let schedule = AutomationSchedule::parse_rrule(&req.rrule)?;
        let now = Utc::now();
        let status = req.status.unwrap_or(AutomationStatus::Active);
        let next_run_at = if matches!(status, AutomationStatus::Active) {
            let next = active_next_run_at(&schedule, now)?;
            if next.is_none() {
                bail!("ONCE schedule DTSTART must be in the future");
            }
            next
        } else {
            None
        };

        let record = AutomationRecord {
            schema_version: CURRENT_AUTOMATION_SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            name: req.name.trim().to_string(),
            prompt: req.prompt.trim().to_string(),
            rrule: req.rrule.trim().to_ascii_uppercase(),
            cwds: req.cwds,
            trigger_kind: req.trigger_kind,
            model: normalize_optional_string(req.model),
            mode: normalize_optional_string(req.mode),
            allow_shell: req.allow_shell,
            trust_mode: req.trust_mode,
            auto_approve: req.auto_approve,
            status,
            created_at: now,
            updated_at: now,
            next_run_at,
            last_run_at: None,
        };

        self.save_automation(&record)?;
        Ok(record)
    }

    pub fn get_automation(&self, id: &str) -> Result<AutomationRecord> {
        let path = self.automation_path(id);
        let raw = fs::read_to_string(&path)
            .with_context(|| format!("Failed to read automation {}", path.display()))?;
        let record: AutomationRecord = serde_json::from_str(&raw)
            .with_context(|| format!("Failed to parse automation {}", path.display()))?;
        if record.schema_version > CURRENT_AUTOMATION_SCHEMA_VERSION {
            bail!(
                "Automation schema v{} is newer than supported v{}",
                record.schema_version,
                CURRENT_AUTOMATION_SCHEMA_VERSION
            );
        }
        Ok(record)
    }

    pub fn save_automation(&self, record: &AutomationRecord) -> Result<()> {
        write_json_atomic(&self.automation_path(&record.id), record)
    }

    pub fn list_automations(&self) -> Result<Vec<AutomationRecord>> {
        let mut out = Vec::new();
        for entry in fs::read_dir(&self.automations_dir)
            .with_context(|| format!("Failed to read {}", self.automations_dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = match fs::read_to_string(&path) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        target: "automations",
                        path = %path.display(),
                        error = %e,
                        "skipping unreadable automation file"
                    );
                    continue;
                }
            };
            let record: AutomationRecord = match serde_json::from_str(&raw) {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(
                        target: "automations",
                        path = %path.display(),
                        error = %e,
                        "skipping malformed automation JSON"
                    );
                    continue;
                }
            };
            if record.schema_version > CURRENT_AUTOMATION_SCHEMA_VERSION {
                tracing::warn!(
                    target: "automations",
                    path = %path.display(),
                    schema_version = record.schema_version,
                    supported = CURRENT_AUTOMATION_SCHEMA_VERSION,
                    "skipping automation with unsupported schema version"
                );
                continue;
            }
            out.push(record);
        }
        out.sort_by_key(|r| std::cmp::Reverse(r.updated_at));
        Ok(out)
    }

    pub fn update_automation(
        &self,
        id: &str,
        req: UpdateAutomationRequest,
    ) -> Result<AutomationRecord> {
        let mut existing = self.get_automation(id)?;

        if let Some(name) = req.name {
            if name.trim().is_empty() {
                bail!("Automation name cannot be empty");
            }
            existing.name = name.trim().to_string();
        }
        if let Some(prompt) = req.prompt {
            if prompt.trim().is_empty() {
                bail!("Automation prompt cannot be empty");
            }
            existing.prompt = prompt.trim().to_string();
        }
        if let Some(rrule) = req.rrule {
            let normalized = rrule.trim().to_ascii_uppercase();
            AutomationSchedule::parse_rrule(&normalized)?;
            existing.rrule = normalized;
            if matches!(existing.status, AutomationStatus::Active) {
                let schedule = AutomationSchedule::parse_rrule(&existing.rrule)?;
                existing.next_run_at = active_next_run_at(&schedule, Utc::now())?;
                if existing.next_run_at.is_none() {
                    existing.status = AutomationStatus::Paused;
                }
            }
        }
        if let Some(cwds) = req.cwds {
            existing.cwds = cwds;
        }
        if let Some(trigger_kind) = req.trigger_kind {
            existing.trigger_kind = trigger_kind;
        }
        if req.model.is_some() {
            existing.model = normalize_optional_string(req.model);
        }
        if req.mode.is_some() {
            existing.mode = normalize_optional_string(req.mode);
        }
        if let Some(v) = req.allow_shell {
            existing.allow_shell = Some(v);
        }
        if let Some(v) = req.trust_mode {
            existing.trust_mode = Some(v);
        }
        if let Some(v) = req.auto_approve {
            existing.auto_approve = Some(v);
        }
        if let Some(status) = req.status {
            existing.status = status;
            if matches!(status, AutomationStatus::Paused) {
                existing.next_run_at = None;
            } else {
                let schedule = AutomationSchedule::parse_rrule(&existing.rrule)?;
                existing.next_run_at = active_next_run_at(&schedule, Utc::now())?;
                if existing.next_run_at.is_none() {
                    bail!("Cannot resume: ONCE schedule has already passed or exhausted");
                }
            }
        }

        existing.updated_at = Utc::now();
        self.save_automation(&existing)?;
        Ok(existing)
    }

    pub fn pause_automation(&self, id: &str) -> Result<AutomationRecord> {
        self.update_automation(
            id,
            UpdateAutomationRequest {
                status: Some(AutomationStatus::Paused),
                ..UpdateAutomationRequest::default()
            },
        )
    }

    pub fn resume_automation(&self, id: &str) -> Result<AutomationRecord> {
        self.update_automation(
            id,
            UpdateAutomationRequest {
                status: Some(AutomationStatus::Active),
                ..UpdateAutomationRequest::default()
            },
        )
    }

    pub fn delete_automation(&self, id: &str) -> Result<AutomationRecord> {
        let existing = self.get_automation(id)?;
        let path = self.automation_path(id);
        fs::remove_file(&path)
            .with_context(|| format!("Failed to delete automation {}", path.display()))?;

        let runs_dir = self.runs_dir_for(id);
        if runs_dir.exists() {
            fs::remove_dir_all(&runs_dir).with_context(|| {
                format!("Failed to delete automation runs {}", runs_dir.display())
            })?;
        }

        Ok(existing)
    }

    pub fn list_runs(
        &self,
        automation_id: &str,
        limit: Option<usize>,
    ) -> Result<Vec<AutomationRunRecord>> {
        let dir = self.runs_dir_for(automation_id);
        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut out = Vec::new();
        for entry in
            fs::read_dir(&dir).with_context(|| format!("Failed to read {}", dir.display()))?
        {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|ext| ext != "json") {
                continue;
            }
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("Failed to read {}", path.display()))?;
            let run: AutomationRunRecord = serde_json::from_str(&raw)
                .with_context(|| format!("Failed to parse {}", path.display()))?;
            if run.schema_version > CURRENT_RUN_SCHEMA_VERSION {
                bail!(
                    "Automation run schema v{} is newer than supported v{}",
                    run.schema_version,
                    CURRENT_RUN_SCHEMA_VERSION
                );
            }
            out.push(run);
        }

        out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
        if let Some(limit) = limit {
            out.truncate(limit);
        }
        Ok(out)
    }

    fn save_run(&self, run: &AutomationRunRecord) -> Result<()> {
        let dir = self.runs_dir_for(&run.automation_id);
        fs::create_dir_all(&dir).with_context(|| format!("Failed to create {}", dir.display()))?;
        write_json_atomic(&self.run_path(&run.automation_id, &run.id), run)
    }

    async fn enqueue_run_task(
        &self,
        automation: &AutomationRecord,
        run: &mut AutomationRunRecord,
        task_manager: &SharedTaskManager,
    ) -> Result<()> {
        let new_task = task_request_for_automation(automation);

        match task_manager.add_task(new_task).await {
            Ok(task) => {
                run.status = AutomationRunStatus::Running;
                run.started_at = Some(Utc::now());
                run.task_id = Some(task.id.clone());
                run.thread_id = task.thread_id.clone();
                run.turn_id = task.turn_id.clone();
                run.error = None;
                Ok(())
            }
            Err(err) => {
                run.status = AutomationRunStatus::Failed;
                run.ended_at = Some(Utc::now());
                run.error = Some(format!("Failed to enqueue task: {err}"));
                Ok(())
            }
        }
    }

    pub async fn run_now(
        &self,
        automation_id: &str,
        task_manager: &SharedTaskManager,
    ) -> Result<AutomationRunRecord> {
        let mut automation = self.get_automation(automation_id)?;
        let now = Utc::now();
        let mut run = AutomationRunRecord {
            schema_version: CURRENT_RUN_SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            automation_id: automation.id.clone(),
            scheduled_for: now,
            status: AutomationRunStatus::Queued,
            created_at: now,
            started_at: None,
            ended_at: None,
            task_id: None,
            thread_id: None,
            turn_id: None,
            error: None,
        };

        self.enqueue_run_task(&automation, &mut run, task_manager)
            .await?;
        self.save_run(&run)?;

        automation.updated_at = Utc::now();
        if matches!(
            run.status,
            AutomationRunStatus::Completed
                | AutomationRunStatus::Failed
                | AutomationRunStatus::Canceled
        ) {
            automation.last_run_at = run.ended_at.or(Some(Utc::now()));
        }
        self.save_automation(&automation)?;

        Ok(run)
    }

    pub async fn scheduler_tick(&self, task_manager: &SharedTaskManager) -> Result<()> {
        let now = Utc::now();
        let mut automations = self.list_automations()?;

        for automation in &mut automations {
            if !matches!(automation.status, AutomationStatus::Active) {
                continue;
            }

            let schedule = AutomationSchedule::parse_rrule(&automation.rrule)?;
            if automation.next_run_at.is_none() {
                match active_next_run_at(&schedule, now)? {
                    Some(next) => automation.next_run_at = Some(next),
                    None => {
                        automation.status = AutomationStatus::Paused;
                        automation.updated_at = now;
                        self.save_automation(automation)?;
                        continue;
                    }
                }
                automation.updated_at = now;
                self.save_automation(automation)?;
                continue;
            }

            let due_at = automation.next_run_at.expect("checked above");
            if due_at > now {
                continue;
            }

            // Idempotency: if a run already exists for this schedule slot, skip enqueue and
            // advance next_run_at.
            let existing_for_slot = self
                .list_runs(&automation.id, Some(25))?
                .into_iter()
                .any(|run| run.scheduled_for == due_at);

            if existing_for_slot {
                apply_next_run_after_fire(automation, &schedule, due_at)?;
                automation.updated_at = now;
                self.save_automation(automation)?;
                continue;
            }

            let mut run = AutomationRunRecord {
                schema_version: CURRENT_RUN_SCHEMA_VERSION,
                id: Uuid::new_v4().to_string(),
                automation_id: automation.id.clone(),
                scheduled_for: due_at,
                status: AutomationRunStatus::Queued,
                created_at: now,
                started_at: None,
                ended_at: None,
                task_id: None,
                thread_id: None,
                turn_id: None,
                error: None,
            };

            self.enqueue_run_task(automation, &mut run, task_manager)
                .await?;
            self.save_run(&run)?;

            automation.updated_at = now;
            apply_next_run_after_fire(automation, &schedule, due_at)?;
            self.save_automation(automation)?;
        }

        Ok(())
    }

    pub async fn reconcile_run_statuses(&self, task_manager: &SharedTaskManager) -> Result<()> {
        let automations = self.list_automations()?;
        for automation in automations {
            let runs = self.list_runs(&automation.id, Some(100))?;
            for mut run in runs {
                if !matches!(
                    run.status,
                    AutomationRunStatus::Queued | AutomationRunStatus::Running
                ) {
                    continue;
                }
                let Some(task_id) = run.task_id.clone() else {
                    continue;
                };
                let task = match task_manager.get_task(&task_id).await {
                    Ok(task) => task,
                    Err(_) => {
                        // Task no longer exists (e.g. sidecar restarted and lost in-memory
                        // task state). Mark the run as failed so it doesn't stay "running"
                        // indefinitely.
                        run.status = AutomationRunStatus::Failed;
                        run.ended_at = Some(Utc::now());
                        run.error = Some("Task lost after sidecar restart".to_string());
                        self.save_run(&run)?;
                        let mut updated_automation = self.get_automation(&automation.id)?;
                        updated_automation.last_run_at = run.ended_at;
                        updated_automation.updated_at = Utc::now();
                        self.save_automation(&updated_automation)?;
                        continue;
                    }
                };

                run.thread_id = task.thread_id.clone();
                run.turn_id = task.turn_id.clone();

                let mut changed = false;
                match task.status {
                    TaskStatus::Queued => {
                        if !matches!(run.status, AutomationRunStatus::Queued) {
                            run.status = AutomationRunStatus::Queued;
                            changed = true;
                        }
                    }
                    TaskStatus::Running => {
                        if !matches!(run.status, AutomationRunStatus::Running) {
                            run.status = AutomationRunStatus::Running;
                            changed = true;
                        }
                        if run.started_at.is_none() {
                            run.started_at = Some(task.started_at.unwrap_or_else(Utc::now));
                            changed = true;
                        }
                    }
                    TaskStatus::Completed => {
                        run.status = AutomationRunStatus::Completed;
                        run.started_at = run.started_at.or(task.started_at);
                        run.ended_at = task.ended_at.or(Some(Utc::now()));
                        run.error = None;
                        changed = true;
                    }
                    TaskStatus::Failed => {
                        run.status = AutomationRunStatus::Failed;
                        run.started_at = run.started_at.or(task.started_at);
                        run.ended_at = task.ended_at.or(Some(Utc::now()));
                        run.error = task.error.clone();
                        changed = true;
                    }
                    TaskStatus::Canceled => {
                        run.status = AutomationRunStatus::Canceled;
                        run.started_at = run.started_at.or(task.started_at);
                        run.ended_at = task.ended_at.or(Some(Utc::now()));
                        changed = true;
                    }
                }

                if changed {
                    self.save_run(&run)?;
                    if matches!(
                        run.status,
                        AutomationRunStatus::Completed
                            | AutomationRunStatus::Failed
                            | AutomationRunStatus::Canceled
                    ) {
                        let mut updated_automation = self.get_automation(&automation.id)?;
                        updated_automation.last_run_at = run.ended_at.or(Some(Utc::now()));
                        updated_automation.updated_at = Utc::now();
                        self.save_automation(&updated_automation)?;
                    }
                }
            }
        }

        Ok(())
    }
}

fn normalize_optional_string(value: Option<String>) -> Option<String> {
    value
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Build the durable task payload for a scheduled automation run.
#[must_use]
pub fn task_request_for_automation(automation: &AutomationRecord) -> NewTaskRequest {
    let workspace = automation.cwds.first().cloned();
    match automation.trigger_kind {
        AutomationTriggerKind::Task => NewTaskRequest {
            prompt: automation.prompt.clone(),
            model: automation.model.clone(),
            workspace,
            mode: automation
                .mode
                .clone()
                .or_else(|| Some("agent".to_string())),
            allow_shell: automation.allow_shell,
            trust_mode: automation.trust_mode,
            auto_approve: automation.auto_approve.or(Some(true)),
        },
        AutomationTriggerKind::Prompt => NewTaskRequest {
            prompt: automation.prompt.clone(),
            model: None,
            workspace,
            mode: Some("agent".to_string()),
            allow_shell: Some(false),
            trust_mode: Some(false),
            auto_approve: Some(true),
        },
    }
}

fn validate_name_and_prompt(name: &str, prompt: &str) -> Result<()> {
    if name.trim().is_empty() {
        bail!("Automation name is required");
    }
    if prompt.trim().is_empty() {
        bail!("Automation prompt is required");
    }
    Ok(())
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    let content = serde_json::to_string_pretty(value)?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content).with_context(|| format!("Failed to write {}", tmp.display()))?;
    fs::rename(&tmp, path).with_context(|| {
        format!(
            "Failed to move temporary file {} to {}",
            tmp.display(),
            path.display()
        )
    })?;
    Ok(())
}

pub fn default_automations_dir() -> PathBuf {
    for key in ["ZAGENS_AUTOMATIONS_DIR", "DEEPSEEK_AUTOMATIONS_DIR"] {
        if let Ok(path) = std::env::var(key) {
            let trimmed = path.trim();
            if !trimmed.is_empty() {
                return PathBuf::from(trimmed);
            }
        }
    }
    zagens_config::user_data_path_or_relative("automations")
}

pub type SharedAutomationManager = Arc<Mutex<AutomationManager>>;

#[derive(Debug, Clone)]
pub struct AutomationSchedulerConfig {
    pub tick_interval_secs: u64,
}

impl Default for AutomationSchedulerConfig {
    fn default() -> Self {
        Self {
            tick_interval_secs: 15,
        }
    }
}

pub fn spawn_scheduler(
    automations: SharedAutomationManager,
    task_manager: SharedTaskManager,
    cancel: CancellationToken,
    config: AutomationSchedulerConfig,
) -> tokio::task::JoinHandle<()> {
    spawn_supervised(
        "automation-scheduler",
        std::panic::Location::caller(),
        async move {
            let interval = config.tick_interval_secs.max(5);
            loop {
                if cancel.is_cancelled() {
                    break;
                }

                {
                    let manager = automations.lock().await;
                    if let Err(err) = manager.scheduler_tick(&task_manager).await {
                        tracing::warn!("automation scheduler tick failed: {err}");
                    }
                    if let Err(err) = manager.reconcile_run_statuses(&task_manager).await {
                        tracing::warn!("automation reconcile failed: {err}");
                    }
                }

                tokio::select! {
                    _ = cancel.cancelled() => break,
                    _ = sleep(std::time::Duration::from_secs(interval)) => {}
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn parses_hourly_rrule() {
        let parsed =
            AutomationSchedule::parse_rrule("FREQ=HOURLY;INTERVAL=2;BYDAY=MO,TU").expect("parse");
        match parsed {
            AutomationSchedule::Hourly {
                interval_hours,
                byday,
            } => {
                assert_eq!(interval_hours, 2);
                assert_eq!(byday.expect("byday").len(), 2);
            }
            _ => panic!("expected hourly"),
        }
    }

    #[test]
    fn parses_weekly_rrule() {
        let parsed =
            AutomationSchedule::parse_rrule("FREQ=WEEKLY;BYDAY=MO,WE;BYHOUR=9;BYMINUTE=30")
                .expect("parse");
        match parsed {
            AutomationSchedule::Weekly {
                byday,
                byhour,
                byminute,
            } => {
                assert_eq!(byday.len(), 2);
                assert_eq!(byhour, 9);
                assert_eq!(byminute, 30);
            }
            _ => panic!("expected weekly"),
        }
    }

    #[test]
    fn parses_minutely_rrule() {
        let parsed = AutomationSchedule::parse_rrule("FREQ=MINUTELY;INTERVAL=15;BYDAY=MO,FR")
            .expect("parse");
        match parsed {
            AutomationSchedule::Minutely {
                interval_minutes,
                byday,
            } => {
                assert_eq!(interval_minutes, 15);
                assert_eq!(byday.expect("byday").len(), 2);
            }
            _ => panic!("expected minutely"),
        }
    }

    #[test]
    fn parses_daily_rrule() {
        let parsed =
            AutomationSchedule::parse_rrule("FREQ=DAILY;BYHOUR=8;BYMINUTE=15").expect("parse");
        match parsed {
            AutomationSchedule::Daily {
                interval_days,
                byhour,
                byminute,
            } => {
                assert_eq!(interval_days, 1);
                assert_eq!(byhour, 8);
                assert_eq!(byminute, 15);
            }
            _ => panic!("expected daily"),
        }
    }

    #[test]
    fn parses_monthly_rrule() {
        let parsed = AutomationSchedule::parse_rrule(
            "FREQ=MONTHLY;INTERVAL=2;BYMONTHDAY=15;BYHOUR=10;BYMINUTE=0",
        )
        .expect("parse");
        match parsed {
            AutomationSchedule::Monthly {
                interval_months,
                bymonthday,
                byhour,
                byminute,
            } => {
                assert_eq!(interval_months, 2);
                assert_eq!(bymonthday, 15);
                assert_eq!(byhour, 10);
                assert_eq!(byminute, 0);
            }
            _ => panic!("expected monthly"),
        }
    }

    #[test]
    fn parses_once_rrule() {
        let parsed = AutomationSchedule::parse_rrule("FREQ=ONCE;DTSTART=2030-06-10T09:00:00")
            .expect("parse");
        match parsed {
            AutomationSchedule::Once { at } => {
                assert!(at.year() >= 2030);
            }
            _ => panic!("expected once"),
        }
    }

    #[test]
    fn once_schedule_exhausted_after_fire() {
        let schedule = AutomationSchedule::Once {
            at: Utc.with_ymd_and_hms(2020, 1, 1, 9, 0, 0).unwrap(),
        };
        let after = Utc.with_ymd_and_hms(2020, 1, 1, 10, 0, 0).unwrap();
        assert!(schedule.next_after_opt(after).expect("ok").is_none());
    }

    #[test]
    fn daily_next_after_finds_future_slot() {
        let schedule = AutomationSchedule::Daily {
            interval_days: 1,
            byhour: 9,
            byminute: 0,
        };
        let after = Utc.with_ymd_and_hms(2026, 6, 8, 10, 0, 0).unwrap();
        let next = schedule.next_after(after).expect("next");
        assert!(next > after);
    }

    #[test]
    fn rejects_invalid_rrule_fields() {
        let err =
            AutomationSchedule::parse_rrule("FREQ=WEEKLY;BYSECOND=5").expect_err("should fail");
        assert!(err.to_string().contains("Unsupported RRULE field"));
    }

    #[test]
    fn task_request_prompt_mode_uses_conservative_defaults() {
        let automation = AutomationRecord {
            schema_version: CURRENT_AUTOMATION_SCHEMA_VERSION,
            id: "a1".to_string(),
            name: "n".to_string(),
            prompt: "do work".to_string(),
            rrule: "FREQ=HOURLY".to_string(),
            cwds: Vec::new(),
            trigger_kind: AutomationTriggerKind::Prompt,
            model: Some("ignored".to_string()),
            mode: Some("yolo".to_string()),
            allow_shell: Some(true),
            trust_mode: Some(true),
            auto_approve: Some(false),
            status: AutomationStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            next_run_at: None,
            last_run_at: None,
        };
        let req = task_request_for_automation(&automation);
        assert_eq!(req.prompt, "do work");
        assert!(req.model.is_none());
        assert_eq!(req.mode.as_deref(), Some("agent"));
        assert_eq!(req.allow_shell, Some(false));
        assert_eq!(req.trust_mode, Some(false));
        assert_eq!(req.auto_approve, Some(true));
    }

    #[test]
    fn task_request_task_mode_uses_stored_fields() {
        let automation = AutomationRecord {
            schema_version: CURRENT_AUTOMATION_SCHEMA_VERSION,
            id: "a2".to_string(),
            name: "n".to_string(),
            prompt: "audit repo".to_string(),
            rrule: "FREQ=HOURLY".to_string(),
            cwds: vec![PathBuf::from("/tmp/ws")],
            trigger_kind: AutomationTriggerKind::Task,
            model: Some("deepseek-v4-pro".to_string()),
            mode: Some("yolo".to_string()),
            allow_shell: Some(true),
            trust_mode: Some(true),
            auto_approve: Some(false),
            status: AutomationStatus::Active,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            next_run_at: None,
            last_run_at: None,
        };
        let req = task_request_for_automation(&automation);
        assert_eq!(req.model.as_deref(), Some("deepseek-v4-pro"));
        assert_eq!(req.mode.as_deref(), Some("yolo"));
        assert_eq!(req.workspace.as_deref(), Some(Path::new("/tmp/ws")));
        assert_eq!(req.allow_shell, Some(true));
        assert_eq!(req.trust_mode, Some(true));
        assert_eq!(req.auto_approve, Some(false));
    }

    #[test]
    fn deletes_automation_and_runs() {
        let tempdir = tempfile::tempdir().expect("tempdir");
        let manager = AutomationManager::open(tempdir.path().to_path_buf()).expect("manager");

        let created = manager
            .create_automation(CreateAutomationRequest {
                name: "Delete me".to_string(),
                prompt: "prompt".to_string(),
                rrule: "FREQ=HOURLY;INTERVAL=1".to_string(),
                cwds: Vec::new(),
                trigger_kind: AutomationTriggerKind::Prompt,
                model: None,
                mode: None,
                allow_shell: None,
                trust_mode: None,
                auto_approve: None,
                status: Some(AutomationStatus::Active),
            })
            .expect("create");

        let run = AutomationRunRecord {
            schema_version: CURRENT_RUN_SCHEMA_VERSION,
            id: Uuid::new_v4().to_string(),
            automation_id: created.id.clone(),
            scheduled_for: Utc::now(),
            status: AutomationRunStatus::Queued,
            created_at: Utc::now(),
            started_at: None,
            ended_at: None,
            task_id: None,
            thread_id: None,
            turn_id: None,
            error: None,
        };
        manager.save_run(&run).expect("save run");
        assert!(manager.runs_dir_for(&created.id).exists());

        manager
            .delete_automation(&created.id)
            .expect("delete automation");

        assert!(manager.get_automation(&created.id).is_err());
        assert!(!manager.runs_dir_for(&created.id).exists());
    }
}
