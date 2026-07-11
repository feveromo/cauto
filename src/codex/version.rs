use std::ffi::OsString;

use serde::{Deserialize, Serialize};

use crate::cache::atomic_write;
use crate::error::AppError;
use crate::paths::CautoPaths;

use super::binary::{CodexInstallation, ProcessRequest, ProcessRunner};

#[derive(Clone, Debug, Deserialize, Serialize)]
struct VersionCache {
    schema_version: u32,
    fingerprint: String,
    version: String,
}

pub fn load_or_probe(
    paths: &CautoPaths,
    installation: &CodexInstallation,
    runner: &dyn ProcessRunner,
    timeout: std::time::Duration,
    force: bool,
) -> Result<String, AppError> {
    let path = paths
        .versions_dir()
        .join(format!("{}.json", installation.fingerprint));
    if !force
        && let Ok(bytes) = std::fs::read(&path)
        && let Ok(cache) = serde_json::from_slice::<VersionCache>(&bytes)
        && cache.schema_version == 1
        && cache.fingerprint == installation.fingerprint
    {
        return Ok(cache.version);
    }
    let output = runner
        .run(&ProcessRequest {
            program: installation.binary.clone(),
            args: vec![OsString::from("--version")],
            current_dir: None,
            environment: Vec::new(),
            timeout,
        })
        .map_err(|error| AppError::CodexVersion(error.to_string()))?;
    if output.status_code != Some(0) {
        return Err(AppError::CodexVersion(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let version = String::from_utf8(output.stdout)
        .map_err(|error| AppError::CodexVersion(format!("version output was not UTF-8: {error}")))?
        .trim()
        .to_owned();
    if version.is_empty() {
        return Err(AppError::CodexVersion(
            "Codex returned an empty version string".into(),
        ));
    }
    let bytes = serde_json::to_vec(&VersionCache {
        schema_version: 1,
        fingerprint: installation.fingerprint.clone(),
        version: version.clone(),
    })
    .map_err(|error| AppError::Serialization(error.to_string()))?;
    atomic_write(&path, &bytes)?;
    Ok(version)
}
