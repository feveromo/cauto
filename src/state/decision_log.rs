use std::ffi::{OsStr, OsString};
use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::str::FromStr;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::cache::ensure_private_dir;
use crate::error::AppError;
use crate::routing::{
    CapabilitySource, Conflict, DimensionScores, Downgrade, ModelFamily, ReasoningLevel,
    RouteSource, TaskType,
};

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;
#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DecisionRecord {
    pub schema_version: u32,
    pub record_type: String,
    /// `preview` records are retained for observability but cannot bias a later launch.
    #[serde(default = "default_decision_mode")]
    pub decision_mode: String,
    pub decision_id: String,
    pub timestamp: String,
    pub cauto_version: String,
    pub codex_version: String,
    pub repository_identifier: String,
    pub repository_name: String,
    pub git_branch: Option<String>,
    pub prompt_sha256: String,
    pub prompt_byte_length: usize,
    pub task_type: TaskType,
    pub dimensions: DimensionScores,
    pub complexity_score: u8,
    #[serde(default)]
    pub calibration: Option<crate::routing::CalibrationEffect>,
    pub confidence_basis_points: u16,
    pub matched_rule_ids: Vec<String>,
    pub raising_rule_ids: Vec<String>,
    pub lowering_rule_ids: Vec<String>,
    pub conflicts: Vec<Conflict>,
    pub selected_model: String,
    pub selected_family: ModelFamily,
    pub selected_effort: ReasoningLevel,
    pub ultra_candidate: bool,
    pub ultra_selected: bool,
    #[serde(default)]
    pub route_source: RouteSource,
    #[serde(default)]
    pub routing_elapsed_micros: u64,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub classifier_ran: bool,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub classifier_outcome: String,
    pub catalog_source: CapabilitySource,
    pub downgrade: Option<Downgrade>,
    pub sanitized_argv: Vec<String>,
    pub feedback: Option<String>,
}

fn default_decision_mode() -> String {
    "launched".into()
}

#[cfg(unix)]
fn prompt_bytes(prompt: &OsStr) -> &[u8] {
    prompt.as_bytes()
}

#[cfg(not(unix))]
fn prompt_bytes(prompt: &OsStr) -> &[u8] {
    prompt.to_string_lossy().as_bytes()
}

#[must_use]
pub fn prompt_sha256(prompt: &OsStr) -> String {
    format!("{:x}", Sha256::digest(prompt_bytes(prompt)))
}

#[must_use]
pub fn repository_identifier(path: &Path) -> String {
    #[cfg(unix)]
    let bytes = path.as_os_str().as_bytes();
    #[cfg(not(unix))]
    let binding = path.to_string_lossy();
    #[cfg(not(unix))]
    let bytes = binding.as_bytes();
    format!("{:x}", Sha256::digest(bytes))
}

/// Loads the most recent family and effort for a repository from a bounded log tail.
pub fn latest_route(
    path: &Path,
    repository_id: &str,
) -> Result<Option<(ModelFamily, ReasoningLevel)>, AppError> {
    const MAX_TAIL_BYTES: u64 = 256 * 1024;

    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(AppError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let length = file
        .metadata()
        .map_err(|source| AppError::Io {
            path: path.to_path_buf(),
            source,
        })?
        .len();
    let tail_length = length.min(MAX_TAIL_BYTES);
    file.seek(SeekFrom::End(-(tail_length as i64)))
        .map_err(|source| AppError::Io {
            path: path.to_path_buf(),
            source,
        })?;
    let mut tail = Vec::with_capacity(tail_length as usize);
    file.read_to_end(&mut tail).map_err(|source| AppError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let text = String::from_utf8_lossy(&tail);
    // A bounded tail may begin in the middle of a JSON record.
    let complete_start = if tail_length < length {
        text.find('\n').map_or(text.len(), |offset| offset + 1)
    } else {
        0
    };
    for line in text[complete_start..].lines().rev() {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
            continue;
        };
        if value.get("record_type").and_then(serde_json::Value::as_str) != Some("decision")
            || value
                .get("decision_mode")
                .and_then(serde_json::Value::as_str)
                == Some("preview")
            || value
                .get("repository_identifier")
                .and_then(serde_json::Value::as_str)
                != Some(repository_id)
        {
            continue;
        }
        let family = value
            .get("selected_family")
            .and_then(serde_json::Value::as_str)
            .map(ModelFamily::from_model_id);
        let effort = value
            .get("selected_effort")
            .and_then(serde_json::Value::as_str)
            .and_then(|effort| ReasoningLevel::from_str(effort).ok());
        return Ok(family.zip(effort));
    }
    Ok(None)
}

fn safe_config_assignment(value: &str) -> bool {
    value.split_once('=').is_some_and(|(key, _)| {
        matches!(
            key.trim(),
            "model" | "model_reasoning_effort" | "service_tier"
        )
    })
}

/// Produces log-safe argv; callers must omit the prompt before calling this function.
#[must_use]
pub fn sanitize_argv(args_without_prompt: &[OsString]) -> Vec<String> {
    let mut sanitized = Vec::with_capacity(args_without_prompt.len());
    let mut redact_next_config = false;
    for argument in args_without_prompt {
        let Some(value) = argument.to_str() else {
            sanitized.push("<non-utf8-argument>".into());
            redact_next_config = false;
            continue;
        };
        if redact_next_config {
            sanitized.push(if safe_config_assignment(value) {
                value.to_owned()
            } else {
                "<redacted-config>".into()
            });
            redact_next_config = false;
            continue;
        }
        if matches!(value, "-c" | "--config") {
            sanitized.push(value.into());
            redact_next_config = true;
        } else if let Some(assignment) = value.strip_prefix("--config=") {
            sanitized.push(if safe_config_assignment(assignment) {
                value.to_owned()
            } else {
                "--config=<redacted>".into()
            });
        } else {
            sanitized.push(value.to_owned());
        }
    }
    sanitized
}

#[must_use]
pub fn timestamp_now() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    OffsetDateTime::from_unix_timestamp(seconds as i64)
        .ok()
        .and_then(|value| value.format(&Rfc3339).ok())
        .unwrap_or_else(|| seconds.to_string())
}

pub fn append_json_line(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| AppError::State {
        path: path.to_path_buf(),
        message: "decision path has no parent".into(),
    })?;
    ensure_private_dir(parent)?;
    let mut options = OpenOptions::new();
    options.read(true).append(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    let mut file = options.open(path).map_err(|error| AppError::State {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    #[cfg(unix)]
    file.set_permissions(std::fs::Permissions::from_mode(0o600))
        .map_err(|error| AppError::State {
            path: path.to_path_buf(),
            message: format!("failed to secure decision log: {error}"),
        })?;
    let start = Instant::now();
    loop {
        match File::try_lock(&file) {
            Ok(()) => break,
            Err(std::fs::TryLockError::WouldBlock)
                if start.elapsed() < Duration::from_millis(250) =>
            {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(AppError::State {
                    path: path.to_path_buf(),
                    message: format!("decision log lock failed: {error}"),
                });
            }
            Err(error) => {
                return Err(AppError::State {
                    path: path.to_path_buf(),
                    message: format!("decision log remained locked: {error}"),
                });
            }
        }
    }
    let result = (|| {
        file.write_all(bytes)?;
        file.write_all(b"\n")?;
        file.flush()?;
        Ok::<(), std::io::Error>(())
    })();
    let _ = File::unlock(&file);
    result.map_err(|error| AppError::State {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

pub fn append_decision(path: &Path, record: &DecisionRecord) -> Result<(), AppError> {
    let bytes =
        serde_json::to_vec(record).map_err(|error| AppError::Serialization(error.to_string()))?;
    append_json_line(path, &bytes)
}
