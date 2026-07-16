use std::collections::BTreeMap;
use std::io::BufRead;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::cache::atomic_write;
use crate::error::AppError;
use crate::routing::{RouteSource, ScoreCalibration};

use super::decision_log::{DecisionRecord, timestamp_now};

const SCHEMA_VERSION: u32 = 1;
const RECOMMENDED_OFFSET: i8 = 5;

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct FeedbackCounts {
    pub right: u64,
    pub underpowered: u64,
    pub overkill: u64,
    pub failed_for_other_reason: u64,
}

impl FeedbackCounts {
    #[must_use]
    pub const fn eligible_total(&self) -> u64 {
        self.right + self.underpowered + self.overkill
    }

    #[must_use]
    pub const fn total(&self) -> u64 {
        self.eligible_total() + self.failed_for_other_reason
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct AppliedCalibration {
    pub score_offset: i8,
    pub direction: String,
    pub eligible_feedback_count: u64,
    pub right_count: u64,
    pub underpowered_count: u64,
    pub overkill_count: u64,
    pub applied_at: String,
}

impl AppliedCalibration {
    fn validated(&self) -> Result<ScoreCalibration, AppError> {
        ScoreCalibration::new(self.score_offset).map_err(|message| AppError::State {
            path: Path::new("calibration.json").to_path_buf(),
            message,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct CalibrationStore {
    pub schema_version: u32,
    pub repositories: BTreeMap<String, AppliedCalibration>,
}

impl Default for CalibrationStore {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            repositories: BTreeMap::new(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RepositoryTuning {
    pub repository_identifier: String,
    pub repository_name: String,
    pub feedback: FeedbackCounts,
    pub eligible_feedback_count: u64,
    pub previews_excluded: u64,
    pub native_preserved_excluded: u64,
    pub eligible: bool,
    pub current_calibration: Option<i8>,
    pub proposed_calibration: Option<i8>,
    pub status: String,
    pub reason: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct TuningAnalysis {
    pub schema_version: u32,
    pub repositories: Vec<RepositoryTuning>,
}

#[derive(Clone)]
struct DecisionInfo {
    repository_identifier: String,
    repository_name: String,
    preview: bool,
    native_preserved: bool,
}

pub fn load_store(path: &Path) -> Result<CalibrationStore, AppError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(CalibrationStore::default());
        }
        Err(source) => {
            return Err(AppError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let store: CalibrationStore =
        serde_json::from_slice(&bytes).map_err(|error| AppError::State {
            path: path.to_path_buf(),
            message: format!("malformed calibration state ignored: {error}"),
        })?;
    if store.schema_version != SCHEMA_VERSION {
        return Err(AppError::State {
            path: path.to_path_buf(),
            message: format!(
                "unsupported calibration schema {}; expected {SCHEMA_VERSION}",
                store.schema_version
            ),
        });
    }
    for calibration in store.repositories.values() {
        calibration.validated().map_err(|error| AppError::State {
            path: path.to_path_buf(),
            message: error.to_string(),
        })?;
    }
    Ok(store)
}

pub fn load_calibration(
    path: &Path,
    repository_id: &str,
) -> Result<Option<ScoreCalibration>, AppError> {
    load_store(path)?
        .repositories
        .get(repository_id)
        .map(AppliedCalibration::validated)
        .transpose()
}

fn recommendation(counts: &FeedbackCounts) -> (bool, Option<i8>, String, String) {
    let eligible = counts.eligible_total();
    if eligible < 3 {
        return (
            false,
            None,
            "insufficient-feedback".into(),
            format!("{eligible} eligible events; at least 3 are required"),
        );
    }
    if counts.underpowered * 10 >= eligible * 7 {
        return (
            true,
            Some(RECOMMENDED_OFFSET),
            "recommend-increase".into(),
            format!(
                "{} of {eligible} eligible events ({:.0}%) are underpowered; propose +{RECOMMENDED_OFFSET} score points",
                counts.underpowered,
                counts.underpowered as f64 * 100.0 / eligible as f64
            ),
        );
    }
    if counts.overkill * 10 >= eligible * 7 {
        return (
            true,
            Some(-RECOMMENDED_OFFSET),
            "recommend-decrease".into(),
            format!(
                "{} of {eligible} eligible events ({:.0}%) are overkill; propose -{RECOMMENDED_OFFSET} score points",
                counts.overkill,
                counts.overkill as f64 * 100.0 / eligible as f64
            ),
        );
    }
    (
        false,
        None,
        "mixed-signal".into(),
        format!(
            "no direction reaches 70% of {eligible} eligible events; right feedback counts against a change"
        ),
    )
}

pub fn analyze_repository(
    decisions_path: &Path,
    store: &CalibrationStore,
    repository_filter: Option<(&str, &str)>,
) -> Result<TuningAnalysis, AppError> {
    let file = match std::fs::File::open(decisions_path) {
        Ok(file) => Some(file),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(source) => {
            return Err(AppError::Io {
                path: decisions_path.to_path_buf(),
                source,
            });
        }
    };
    let mut values = Vec::new();
    if let Some(file) = file {
        for line in std::io::BufReader::new(file).lines() {
            let line = line.map_err(|source| AppError::Io {
                path: decisions_path.to_path_buf(),
                source,
            })?;
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&line) {
                values.push(value);
            }
        }
    }
    let mut decisions = BTreeMap::new();
    let mut names = BTreeMap::new();
    for value in &values {
        if value.get("record_type").and_then(serde_json::Value::as_str) != Some("decision") {
            continue;
        }
        let Ok(record) = serde_json::from_value::<DecisionRecord>(value.clone()) else {
            continue;
        };
        names.insert(
            record.repository_identifier.clone(),
            record.repository_name.clone(),
        );
        decisions.insert(
            record.decision_id,
            DecisionInfo {
                repository_identifier: record.repository_identifier,
                repository_name: record.repository_name,
                preview: record.decision_mode == "preview",
                native_preserved: record.route_source == RouteSource::NativePreserved,
            },
        );
    }
    let mut counts: BTreeMap<String, (String, FeedbackCounts, u64, u64)> = BTreeMap::new();
    for value in &values {
        if value.get("record_type").and_then(serde_json::Value::as_str) != Some("feedback") {
            continue;
        }
        let Some(decision) = value
            .get("decision_id")
            .and_then(serde_json::Value::as_str)
            .and_then(|id| decisions.get(id))
        else {
            continue;
        };
        let entry = counts
            .entry(decision.repository_identifier.clone())
            .or_insert_with(|| {
                (
                    decision.repository_name.clone(),
                    FeedbackCounts::default(),
                    0,
                    0,
                )
            });
        if decision.preview {
            entry.2 += 1;
            continue;
        }
        if decision.native_preserved {
            entry.3 += 1;
            continue;
        }
        match value.get("feedback").and_then(serde_json::Value::as_str) {
            Some("right") => entry.1.right += 1,
            Some("underpowered") => entry.1.underpowered += 1,
            Some("overkill") => entry.1.overkill += 1,
            Some("failed-for-other-reason") => entry.1.failed_for_other_reason += 1,
            _ => {}
        }
    }
    if let Some((id, name)) = repository_filter {
        counts
            .entry(id.to_owned())
            .or_insert_with(|| (name.to_owned(), FeedbackCounts::default(), 0, 0));
    } else {
        for (id, calibration) in &store.repositories {
            let _ = calibration;
            counts.entry(id.clone()).or_insert_with(|| {
                (
                    names.get(id).cloned().unwrap_or_else(|| "unknown".into()),
                    FeedbackCounts::default(),
                    0,
                    0,
                )
            });
        }
    }
    let mut repositories = Vec::new();
    for (id, (name, feedback, previews_excluded, native_preserved_excluded)) in counts {
        if repository_filter.is_some_and(|(filter, _)| id != filter) {
            continue;
        }
        let (eligible, proposed, mut status, mut reason) = recommendation(&feedback);
        let current = store.repositories.get(&id).map(|value| value.score_offset);
        if eligible && current == proposed {
            status = "up-to-date".into();
            reason.push_str("; this calibration is already applied");
        }
        repositories.push(RepositoryTuning {
            repository_identifier: id,
            repository_name: name,
            eligible_feedback_count: feedback.eligible_total(),
            feedback,
            previews_excluded,
            native_preserved_excluded,
            eligible,
            current_calibration: current,
            proposed_calibration: proposed,
            status,
            reason,
        });
    }
    Ok(TuningAnalysis {
        schema_version: SCHEMA_VERSION,
        repositories,
    })
}

fn write_store(path: &Path, store: &CalibrationStore) -> Result<(), AppError> {
    let mut bytes = serde_json::to_vec_pretty(store)
        .map_err(|error| AppError::Serialization(error.to_string()))?;
    bytes.push(b'\n');
    atomic_write(path, &bytes)
}

pub fn save_recommendation(
    path: &Path,
    store: &mut CalibrationStore,
    tuning: &RepositoryTuning,
) -> Result<Option<(Option<i8>, i8)>, AppError> {
    let Some(offset) = tuning.proposed_calibration.filter(|_| tuning.eligible) else {
        return Ok(None);
    };
    ScoreCalibration::new(offset).map_err(AppError::InvalidArguments)?;
    let before = store
        .repositories
        .get(&tuning.repository_identifier)
        .map(|value| value.score_offset);
    if before == Some(offset) {
        return Ok(None);
    }
    store.repositories.insert(
        tuning.repository_identifier.clone(),
        AppliedCalibration {
            score_offset: offset,
            direction: if offset > 0 {
                "underpowered"
            } else {
                "overkill"
            }
            .into(),
            eligible_feedback_count: tuning.eligible_feedback_count,
            right_count: tuning.feedback.right,
            underpowered_count: tuning.feedback.underpowered,
            overkill_count: tuning.feedback.overkill,
            applied_at: timestamp_now(),
        },
    );
    write_store(path, store)?;
    Ok(Some((before, offset)))
}

pub fn reset_repository(
    path: &Path,
    store: &mut CalibrationStore,
    repository_id: &str,
) -> Result<Option<i8>, AppError> {
    let removed = store
        .repositories
        .remove(repository_id)
        .map(|value| value.score_offset);
    if removed.is_some() {
        write_store(path, store)?;
    }
    Ok(removed)
}
