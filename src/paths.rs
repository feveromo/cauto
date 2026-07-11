use std::path::PathBuf;

use directories::BaseDirs;

use crate::error::AppError;

/// Platform-appropriate cauto configuration, cache, and state locations.
#[derive(Clone, Debug)]
pub struct CautoPaths {
    pub config_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub state_dir: PathBuf,
}

impl CautoPaths {
    /// Resolves XDG-aware user directories.
    pub fn discover() -> Result<Self, AppError> {
        let base = BaseDirs::new().ok_or_else(|| {
            AppError::InvalidArguments("the current user has no resolvable home directory".into())
        })?;
        let state_base = base
            .state_dir()
            .map(std::path::Path::to_path_buf)
            .unwrap_or_else(|| base.data_local_dir().to_path_buf());
        Ok(Self {
            config_dir: base.config_dir().join("cauto"),
            cache_dir: base.cache_dir().join("cauto"),
            state_dir: state_base.join("cauto"),
        })
    }

    #[must_use]
    pub fn user_config(&self) -> PathBuf {
        self.config_dir.join("config.toml")
    }

    #[must_use]
    pub fn catalogs_dir(&self) -> PathBuf {
        self.cache_dir.join("catalogs")
    }

    #[must_use]
    pub fn versions_dir(&self) -> PathBuf {
        self.cache_dir.join("versions")
    }

    #[must_use]
    pub fn decisions(&self) -> PathBuf {
        self.state_dir.join("decisions.jsonl")
    }
}
