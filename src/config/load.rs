use std::path::{Path, PathBuf};

use crate::error::AppError;
use crate::routing::RuleSource;

use super::schema::{RawConfig, ValidatedConfig};
use super::validate::{into_validated, validate_layer};

#[derive(Clone, Debug)]
pub struct LoadedConfig {
    pub config: ValidatedConfig,
    pub user_path: PathBuf,
    pub project_path: Option<PathBuf>,
    pub user_loaded: bool,
    pub project_loaded: bool,
}

fn read_optional(path: &Path, source: RuleSource) -> Result<Option<RawConfig>, AppError> {
    let bytes = match std::fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(AppError::ConfigRead {
                path: path.to_path_buf(),
                source,
            });
        }
    };
    let text = std::str::from_utf8(&bytes).map_err(|error| AppError::ConfigParse {
        path: path.to_path_buf(),
        message: format!("configuration must be UTF-8: {error}"),
    })?;
    let mut raw: RawConfig = toml::from_str(text).map_err(|error| AppError::ConfigParse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    for rule in &mut raw.rules {
        rule.source = Some(source.clone());
    }
    validate_layer(&raw, path)?;
    Ok(Some(raw))
}

/// Loads, validates, and explicitly merges user then project configuration.
pub fn load(user_path: &Path, project_path: Option<&Path>) -> Result<LoadedConfig, AppError> {
    let user = read_optional(user_path, RuleSource::User)?;
    let mut project = project_path
        .map(|path| read_optional(path, RuleSource::Project))
        .transpose()?
        .flatten();
    // Project policy may recommend complexity through rules, but cannot force a
    // usage-affecting service tier. A user's own Fast preference remains intact.
    if let Some(project) = &mut project {
        project.fast_mode = None;
    }
    let user_loaded = user.is_some();
    let project_loaded = project.is_some();
    let raw = user.unwrap_or_default().merge(project.unwrap_or_default());
    let effective_path = if project_loaded {
        project_path.expect("loaded project has path")
    } else {
        user_path
    };
    let config = into_validated(raw, effective_path)?;
    Ok(LoadedConfig {
        config,
        user_path: user_path.to_path_buf(),
        project_path: project_path.map(Path::to_path_buf),
        user_loaded,
        project_loaded,
    })
}
