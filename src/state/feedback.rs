use std::path::Path;
use std::str::FromStr;

use serde::Serialize;

use crate::error::AppError;

use super::decision_log::{DecisionRecord, append_json_line, timestamp_now};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeedbackKind {
    Right,
    Overkill,
    Underpowered,
    FailedForOtherReason,
}

impl FromStr for FeedbackKind {
    type Err = String;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "right" => Ok(Self::Right),
            "overkill" => Ok(Self::Overkill),
            "underpowered" => Ok(Self::Underpowered),
            "failed-for-other-reason" => Ok(Self::FailedForOtherReason),
            _ => Err(format!("unknown feedback kind {value:?}")),
        }
    }
}

#[derive(Debug, Serialize)]
struct FeedbackRecord<'a> {
    schema_version: u32,
    record_type: &'static str,
    timestamp: String,
    decision_id: &'a str,
    repository_identifier: &'a str,
    feedback: FeedbackKind,
}

pub fn append_feedback(
    path: &Path,
    repository_id: &str,
    feedback: FeedbackKind,
) -> Result<String, AppError> {
    let bytes = std::fs::read(path).map_err(|source| AppError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let recent = bytes
        .rsplit(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
        .filter_map(|line| serde_json::from_slice::<DecisionRecord>(line).ok())
        .find(|record| {
            record.repository_identifier == repository_id && record.decision_mode != "preview"
        })
        .ok_or_else(|| {
            AppError::InvalidArguments(
                "no prior cauto decision exists for the current repository".into(),
            )
        })?;
    let event = FeedbackRecord {
        schema_version: 1,
        record_type: "feedback",
        timestamp: timestamp_now(),
        decision_id: &recent.decision_id,
        repository_identifier: repository_id,
        feedback,
    };
    let event_bytes =
        serde_json::to_vec(&event).map_err(|error| AppError::Serialization(error.to_string()))?;
    append_json_line(path, &event_bytes)?;
    Ok(recent.decision_id)
}
