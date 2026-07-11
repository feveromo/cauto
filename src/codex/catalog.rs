use std::ffi::OsString;
use std::fs::{File, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use time::{OffsetDateTime, format_description::well_known::Rfc3339};

use crate::cache::{CacheEnvelope, atomic_write, ensure_private_dir};
use crate::error::AppError;
use crate::paths::CautoPaths;
use crate::routing::{CapabilitySource, ModelFamily};

use super::binary::{CodexInstallation, ProcessRequest, ProcessRunner};
use super::version;

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

#[derive(Clone, Debug)]
pub struct CatalogRequest {
    pub installation: CodexInstallation,
    pub timeout: Duration,
    pub max_age: Duration,
    pub include_hidden: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ServiceTier {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelCapability {
    pub id: String,
    pub display_name: String,
    pub family: ModelFamily,
    pub default_reasoning_effort: String,
    #[serde(default)]
    pub supported_reasoning_efforts: Vec<String>,
    #[serde(default)]
    pub service_tiers: Vec<ServiceTier>,
    #[serde(default)]
    pub additional_speed_tiers: Vec<String>,
    #[serde(default)]
    pub input_modalities: Vec<String>,
    pub hidden: bool,
    pub supported_in_api: bool,
    pub interactive_supported: bool,
    pub exec_supported: bool,
    pub app_server_only: bool,
}

impl ModelCapability {
    #[must_use]
    pub fn supports_effort(&self, effort: &str) -> bool {
        self.supported_reasoning_efforts
            .iter()
            .any(|supported| supported.eq_ignore_ascii_case(effort))
    }

    #[must_use]
    pub fn max_available(&self) -> bool {
        self.supports_effort("max")
    }

    #[must_use]
    pub fn ultra_available(&self) -> bool {
        self.supports_effort("ultra")
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ModelCatalog {
    pub models: Vec<ModelCapability>,
    pub source: CapabilitySource,
    pub stale: bool,
    pub fetched_at_unix: u64,
    pub cache_age_seconds: Option<u64>,
    pub codex_version: String,
    pub warning: Option<String>,
}

impl ModelCatalog {
    pub fn visible_models(&self, include_hidden: bool) -> impl Iterator<Item = &ModelCapability> {
        self.models
            .iter()
            .filter(move |model| include_hidden || !model.hidden)
    }

    #[must_use]
    pub fn find(&self, id: &str) -> Option<&ModelCapability> {
        self.models
            .iter()
            .find(|model| model.id.eq_ignore_ascii_case(id))
    }

    #[must_use]
    pub fn first_family(&self, family: &ModelFamily) -> Option<&ModelCapability> {
        self.models
            .iter()
            .find(|model| !model.hidden && model.family == *family)
    }
}

#[derive(Debug, Error)]
pub enum CatalogError {
    #[error("catalog is not present")]
    Missing,
    #[error("catalog cache is incompatible: {0}")]
    Incompatible(String),
    #[error("catalog cache is corrupt: {0}")]
    Corrupt(String),
    #[error("catalog process failed: {0}")]
    Process(String),
    #[error("catalog output is invalid: {0}")]
    Parse(String),
    #[error("source is unavailable: {0}")]
    Unavailable(String),
}

pub trait CatalogSource {
    fn load(&self, request: &CatalogRequest) -> Result<ModelCatalog, CatalogError>;
}

#[derive(Debug)]
pub struct CachedCatalogSource {
    pub path: PathBuf,
}

impl CatalogSource for CachedCatalogSource {
    fn load(&self, request: &CatalogRequest) -> Result<ModelCatalog, CatalogError> {
        let bytes = std::fs::read(&self.path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::NotFound {
                CatalogError::Missing
            } else {
                CatalogError::Corrupt(error.to_string())
            }
        })?;
        let envelope = CacheEnvelope::<ModelCatalog>::parse(&bytes)
            .map_err(|error| CatalogError::Corrupt(error.to_string()))?;
        if envelope.schema_version != 1 {
            return Err(CatalogError::Incompatible(format!(
                "schema {} is not supported",
                envelope.schema_version
            )));
        }
        if envelope.codex_binary_fingerprint != request.installation.fingerprint
            || envelope.codex_home_hash != request.installation.codex_home_hash
            || envelope.profile != request.installation.profile
        {
            return Err(CatalogError::Incompatible(
                "installation fingerprint, CODEX_HOME, or profile changed".into(),
            ));
        }
        if !envelope
            .digest_is_valid()
            .map_err(|error| CatalogError::Corrupt(error.to_string()))?
        {
            return Err(CatalogError::Corrupt("payload SHA-256 mismatch".into()));
        }
        let now = unix_now();
        let age = now.saturating_sub(envelope.fetched_at_unix);
        let mut catalog = envelope.catalog;
        catalog.source = CapabilitySource::Cache;
        catalog.stale = age > request.max_age.as_secs();
        catalog.cache_age_seconds = Some(age);
        catalog.codex_version = envelope.codex_version;
        Ok(catalog)
    }
}

pub struct DebugModelsSource<'a> {
    pub runner: &'a dyn ProcessRunner,
    pub bundled: bool,
    pub codex_version: String,
}

impl CatalogSource for DebugModelsSource<'_> {
    fn load(&self, request: &CatalogRequest) -> Result<ModelCatalog, CatalogError> {
        let mut args = Vec::with_capacity(6);
        if let Some(profile) = &request.installation.profile {
            args.push(OsString::from("--profile"));
            args.push(OsString::from(profile));
        }
        args.push(OsString::from("debug"));
        args.push(OsString::from("models"));
        if self.bundled {
            args.push(OsString::from("--bundled"));
        }
        let output = self
            .runner
            .run(&ProcessRequest {
                program: request.installation.binary.clone(),
                args,
                current_dir: None,
                environment: Vec::new(),
                timeout: request.timeout,
            })
            .map_err(|error| CatalogError::Process(error.to_string()))?;
        if output.status_code != Some(0) {
            return Err(CatalogError::Process(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ));
        }
        parse_debug_models(
            &output.stdout,
            if self.bundled {
                CapabilitySource::Bundled
            } else {
                CapabilitySource::DebugModels
            },
            &self.codex_version,
        )
    }
}

/// Optional phase-two adapter marker. Version 1 does not start App Server during warm routing.
#[derive(Debug, Default)]
pub struct AppServerCatalogSource;

impl CatalogSource for AppServerCatalogSource {
    fn load(&self, _request: &CatalogRequest) -> Result<ModelCatalog, CatalogError> {
        Err(CatalogError::Unavailable(
            "App Server discovery is reserved for explicit future integration".into(),
        ))
    }
}

#[derive(Debug, Default)]
pub struct FallbackCatalogSource;

impl CatalogSource for FallbackCatalogSource {
    fn load(&self, _request: &CatalogRequest) -> Result<ModelCatalog, CatalogError> {
        Ok(fallback_catalog(
            "live and bundled catalog discovery were unavailable",
        ))
    }
}

#[derive(Debug, Deserialize)]
struct DebugCatalog {
    #[serde(default)]
    models: Vec<DebugModel>,
}

#[derive(Debug, Deserialize)]
struct DebugModel {
    slug: String,
    #[serde(default)]
    display_name: String,
    #[serde(default = "default_effort")]
    default_reasoning_level: String,
    #[serde(default)]
    supported_reasoning_levels: Vec<DebugEffort>,
    #[serde(default)]
    visibility: String,
    #[serde(default)]
    supported_in_api: bool,
    #[serde(default)]
    additional_speed_tiers: Vec<String>,
    #[serde(default)]
    service_tiers: Vec<ServiceTier>,
    #[serde(default)]
    input_modalities: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct DebugEffort {
    effort: String,
}

fn default_effort() -> String {
    "medium".into()
}

pub fn parse_debug_models(
    bytes: &[u8],
    source: CapabilitySource,
    codex_version: &str,
) -> Result<ModelCatalog, CatalogError> {
    let raw: DebugCatalog =
        serde_json::from_slice(bytes).map_err(|error| CatalogError::Parse(error.to_string()))?;
    if raw.models.is_empty() {
        return Err(CatalogError::Parse("catalog contains no models".into()));
    }
    let models = raw
        .models
        .into_iter()
        .map(|model| {
            let hidden = !model.visibility.is_empty() && model.visibility != "list";
            ModelCapability {
                family: ModelFamily::from_model_id(&model.slug),
                display_name: if model.display_name.is_empty() {
                    model.slug.clone()
                } else {
                    model.display_name
                },
                id: model.slug,
                default_reasoning_effort: model.default_reasoning_level,
                supported_reasoning_efforts: model
                    .supported_reasoning_levels
                    .into_iter()
                    .map(|effort| effort.effort)
                    .collect(),
                service_tiers: model.service_tiers,
                additional_speed_tiers: model.additional_speed_tiers,
                input_modalities: model.input_modalities,
                hidden,
                supported_in_api: model.supported_in_api,
                interactive_supported: true,
                exec_supported: true,
                app_server_only: false,
            }
        })
        .collect();
    Ok(ModelCatalog {
        models,
        source,
        stale: false,
        fetched_at_unix: unix_now(),
        cache_age_seconds: None,
        codex_version: codex_version.to_owned(),
        warning: None,
    })
}

fn fallback_model(id: &str, family: ModelFamily) -> ModelCapability {
    ModelCapability {
        id: id.into(),
        display_name: id.into(),
        family,
        default_reasoning_effort: "medium".into(),
        supported_reasoning_efforts: vec![
            "low".into(),
            "medium".into(),
            "high".into(),
            "xhigh".into(),
        ],
        service_tiers: Vec::new(),
        additional_speed_tiers: Vec::new(),
        input_modalities: vec!["text".into()],
        hidden: false,
        supported_in_api: false,
        interactive_supported: true,
        exec_supported: true,
        app_server_only: false,
    }
}

fn fallback_catalog(reason: &str) -> ModelCatalog {
    ModelCatalog {
        models: vec![
            fallback_model("gpt-5.6-sol", ModelFamily::Sol),
            fallback_model("gpt-5.6-terra", ModelFamily::Terra),
            fallback_model("gpt-5.6-luna", ModelFamily::Luna),
        ],
        source: CapabilitySource::Fallback,
        stale: false,
        fetched_at_unix: unix_now(),
        cache_age_seconds: None,
        codex_version: "unknown".into(),
        warning: Some(reason.into()),
    }
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn cache_key(installation: &CodexInstallation) -> String {
    let mut hasher = Sha256::new();
    hasher.update(installation.fingerprint.as_bytes());
    if let Some(profile) = &installation.profile {
        hasher.update([0]);
        hasher.update(profile.as_bytes());
    }
    format!("{:x}", hasher.finalize())
}

pub struct CatalogManager<'a> {
    pub paths: &'a CautoPaths,
    pub runner: &'a dyn ProcessRunner,
}

struct RefreshLock(File);

impl Drop for RefreshLock {
    fn drop(&mut self) {
        let _ = File::unlock(&self.0);
    }
}

fn acquire_refresh_lock(
    path: &std::path::Path,
    timeout: Duration,
) -> Result<Option<RefreshLock>, AppError> {
    let parent = path.parent().ok_or_else(|| AppError::Cache {
        path: path.to_path_buf(),
        message: "catalog lock has no parent".into(),
    })?;
    ensure_private_dir(parent)?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    options.mode(0o600);
    let file = options.open(path).map_err(|error| AppError::Cache {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let start = Instant::now();
    loop {
        match File::try_lock(&file) {
            Ok(()) => return Ok(Some(RefreshLock(file))),
            Err(std::fs::TryLockError::WouldBlock) if start.elapsed() < timeout => {
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(std::fs::TryLockError::WouldBlock) => return Ok(None),
            Err(std::fs::TryLockError::Error(error)) => {
                return Err(AppError::Cache {
                    path: path.to_path_buf(),
                    message: format!("catalog refresh lock failed: {error}"),
                });
            }
        }
    }
}

impl CatalogManager<'_> {
    pub fn cache_path(&self, installation: &CodexInstallation) -> PathBuf {
        self.paths
            .catalogs_dir()
            .join(format!("{}.json", cache_key(installation)))
    }

    pub fn load(
        &self,
        request: &CatalogRequest,
        refresh: bool,
        bundled_only: bool,
    ) -> Result<ModelCatalog, AppError> {
        let path = self.cache_path(&request.installation);
        let cached_source = CachedCatalogSource { path: path.clone() };
        let cached = cached_source.load(request).ok();
        if !refresh
            && !bundled_only
            && let Some(catalog) = cached.clone()
        {
            return Ok(catalog);
        }
        let original_fetched_at = cached
            .as_ref()
            .map(|catalog| catalog.fetched_at_unix)
            .unwrap_or(0);
        let refresh_lock_path = path.with_extension("refresh.lock");
        let Some(_refresh_lock) = acquire_refresh_lock(&refresh_lock_path, request.timeout)? else {
            if let Some(mut catalog) = cached {
                catalog.stale = true;
                catalog.warning = Some("another cauto process is refreshing this catalog".into());
                return Ok(catalog);
            }
            return Ok(fallback_catalog(
                "another cauto process is refreshing the missing catalog",
            ));
        };
        if !bundled_only
            && let Ok(reloaded) = cached_source.load(request)
            && (!refresh || reloaded.fetched_at_unix > original_fetched_at)
        {
            return Ok(reloaded);
        }
        let version = match version::load_or_probe(
            self.paths,
            &request.installation,
            self.runner,
            request.timeout,
            refresh,
        ) {
            Ok(version) => version,
            Err(error) => {
                if let Some(mut catalog) = cached {
                    catalog.stale = true;
                    catalog.warning = Some(format!(
                        "catalog refresh skipped because Codex version probing failed: {error}"
                    ));
                    return Ok(catalog);
                }
                return Ok(fallback_catalog(&error.to_string()));
            }
        };
        let primary = DebugModelsSource {
            runner: self.runner,
            bundled: bundled_only,
            codex_version: version.clone(),
        }
        .load(request);
        let discovered = match primary {
            Ok(catalog) => catalog,
            Err(primary_error) if !bundled_only => {
                let bundled = DebugModelsSource {
                    runner: self.runner,
                    bundled: true,
                    codex_version: version.clone(),
                }
                .load(request);
                match bundled {
                    Ok(mut catalog) => {
                        catalog.warning = Some(format!(
                            "live catalog failed; using bundled metadata: {primary_error}"
                        ));
                        catalog
                    }
                    Err(bundled_error) => {
                        if let Some(mut catalog) = cached {
                            catalog.stale = true;
                            catalog.warning = Some(format!(
                                "catalog refresh failed ({primary_error}; {bundled_error}); using stale cache"
                            ));
                            return Ok(catalog);
                        }
                        return Ok(fallback_catalog(&format!(
                            "{primary_error}; bundled fallback failed: {bundled_error}"
                        )));
                    }
                }
            }
            Err(error) => {
                if let Some(mut catalog) = cached {
                    catalog.stale = true;
                    catalog.warning = Some(format!(
                        "bundled refresh failed ({error}); using stale cache"
                    ));
                    return Ok(catalog);
                }
                return Ok(fallback_catalog(&error.to_string()));
            }
        };
        let fetched_at_unix = discovered.fetched_at_unix;
        let fetched_at = OffsetDateTime::from_unix_timestamp(fetched_at_unix as i64)
            .ok()
            .and_then(|value| value.format(&Rfc3339).ok())
            .unwrap_or_else(|| fetched_at_unix.to_string());
        let mut envelope = CacheEnvelope {
            schema_version: 1,
            cauto_version: env!("CARGO_PKG_VERSION").into(),
            codex_version: version,
            codex_binary_fingerprint: request.installation.fingerprint.clone(),
            codex_home_hash: request.installation.codex_home_hash.clone(),
            profile: request.installation.profile.clone(),
            fetched_at,
            fetched_at_unix,
            source: match discovered.source {
                CapabilitySource::DebugModels => "debug-models",
                CapabilitySource::Bundled => "bundled",
                _ => "fallback",
            }
            .into(),
            payload_sha256: String::new(),
            catalog: discovered.clone(),
        };
        envelope
            .refresh_digest()
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        let bytes = serde_json::to_vec(&envelope)
            .map_err(|error| AppError::Serialization(error.to_string()))?;
        if let Err(error) = atomic_write(&path, &bytes) {
            let mut catalog = discovered;
            catalog.warning = Some(format!("catalog cache write failed: {error}"));
            return Ok(catalog);
        }
        Ok(discovered)
    }
}
