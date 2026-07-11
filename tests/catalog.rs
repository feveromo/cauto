mod common;

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use cauto::cache::atomic_write;
use cauto::codex::binary;
use cauto::codex::binary::{
    CodexInstallation, ProcessError, ProcessOutput, ProcessRequest, ProcessRunner,
};
use cauto::codex::capabilities::{PresetRequest, resolve_preset};
use cauto::codex::catalog::{
    CachedCatalogSource, CatalogManager, CatalogRequest, CatalogSource, parse_debug_models,
};
use cauto::codex::version;
use cauto::paths::CautoPaths;
use cauto::routing::{CapabilitySource, FastMode, LaunchMode, ModelFamily, ReasoningLevel};
use tempfile::tempdir;

fn installation() -> CodexInstallation {
    CodexInstallation {
        binary: PathBuf::from("/fake/codex"),
        canonical_binary: PathBuf::from("/fake/codex"),
        fingerprint: "fingerprint".into(),
        codex_home: PathBuf::from("/fake/home"),
        codex_home_hash: "home-hash".into(),
        profile: None,
    }
}

fn paths(root: &std::path::Path) -> CautoPaths {
    CautoPaths {
        config_dir: root.join("config"),
        cache_dir: root.join("cache"),
        state_dir: root.join("state"),
    }
}

fn expire_catalog_cache(path: &std::path::Path) {
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    value["fetched_at_unix"] = serde_json::json!(0);
    std::fs::write(path, serde_json::to_vec(&value).unwrap()).unwrap();
}

struct CountingRunner {
    calls: AtomicUsize,
    fail_live: bool,
    timeout: bool,
}

impl CountingRunner {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            fail_live: false,
            timeout: false,
        }
    }
}

impl ProcessRunner for CountingRunner {
    fn run(&self, request: &ProcessRequest) -> Result<ProcessOutput, ProcessError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.timeout {
            return Err(ProcessError::Timeout(1));
        }
        if request.args == [OsString::from("--version")] {
            return Ok(ProcessOutput {
                status_code: Some(0),
                stdout: b"codex-cli test\n".to_vec(),
                stderr: vec![],
            });
        }
        let bundled = request.args.iter().any(|arg| arg == "--bundled");
        if self.fail_live && !bundled {
            return Ok(ProcessOutput {
                status_code: Some(1),
                stdout: vec![],
                stderr: b"live failed".to_vec(),
            });
        }
        Ok(ProcessOutput {
            status_code: Some(0),
            stdout: include_bytes!("fixtures/catalog.json").to_vec(),
            stderr: vec![],
        })
    }
}

#[test]
fn debug_catalog_accepts_additive_unknown_fields() {
    let catalog = parse_debug_models(
        include_bytes!("fixtures/catalog.json"),
        CapabilitySource::DebugModels,
        "test",
    )
    .unwrap();
    assert_eq!(catalog.models.len(), 4);
    assert!(catalog.models[0].ultra_available());
    assert!(!catalog.models[2].ultra_available());
}

#[test]
fn version_probe_is_cached_by_fingerprint() {
    let root = tempdir().unwrap();
    let paths = paths(root.path());
    let runner = CountingRunner::new();
    let install = installation();
    let first =
        version::load_or_probe(&paths, &install, &runner, Duration::from_secs(1), false).unwrap();
    let second =
        version::load_or_probe(&paths, &install, &runner, Duration::from_secs(1), false).unwrap();
    assert_eq!(first, second);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn stale_catalog_is_refreshed_while_fresh_catalog_returns_immediately() {
    let root = tempdir().unwrap();
    let paths = paths(root.path());
    let runner = CountingRunner::new();
    let manager = CatalogManager {
        paths: &paths,
        runner: &runner,
    };
    let request = CatalogRequest {
        installation: installation(),
        timeout: Duration::from_secs(1),
        max_age: Duration::from_secs(60),
        include_hidden: true,
    };
    manager.load(&request, false, false).unwrap();

    runner.calls.store(0, Ordering::SeqCst);
    let fresh = manager.load(&request, false, false).unwrap();
    assert_eq!(fresh.source, CapabilitySource::Cache);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);

    expire_catalog_cache(&manager.cache_path(&request.installation));
    let refreshed = manager.load(&request, false, false).unwrap();
    assert_eq!(refreshed.source, CapabilitySource::DebugModels);
    assert!(!refreshed.stale);
    assert!(runner.calls.load(Ordering::SeqCst) > 0);
}

#[test]
fn stale_catalog_falls_back_with_warning_when_refresh_fails() {
    let root = tempdir().unwrap();
    let paths = paths(root.path());
    let initial_runner = CountingRunner::new();
    let initial_manager = CatalogManager {
        paths: &paths,
        runner: &initial_runner,
    };
    let request = CatalogRequest {
        installation: installation(),
        timeout: Duration::from_millis(10),
        max_age: Duration::from_secs(60),
        include_hidden: true,
    };
    initial_manager.load(&request, false, false).unwrap();
    expire_catalog_cache(&initial_manager.cache_path(&request.installation));

    let failing_runner = CountingRunner {
        calls: AtomicUsize::new(0),
        fail_live: false,
        timeout: true,
    };
    let manager = CatalogManager {
        paths: &paths,
        runner: &failing_runner,
    };
    let stale = manager.load(&request, false, false).unwrap();

    assert_eq!(stale.source, CapabilitySource::Cache);
    assert!(stale.stale);
    assert!(
        stale
            .warning
            .as_deref()
            .is_some_and(|warning| warning.contains("using stale cache"))
    );
    assert_eq!(failing_runner.calls.load(Ordering::SeqCst), 2);
}

#[test]
fn live_failure_uses_bundled_catalog() {
    let root = tempdir().unwrap();
    let paths = paths(root.path());
    let runner = CountingRunner {
        calls: AtomicUsize::new(0),
        fail_live: true,
        timeout: false,
    };
    let manager = CatalogManager {
        paths: &paths,
        runner: &runner,
    };
    let catalog = manager
        .load(
            &CatalogRequest {
                installation: installation(),
                timeout: Duration::from_secs(1),
                max_age: Duration::from_secs(60),
                include_hidden: true,
            },
            false,
            false,
        )
        .unwrap();
    assert_eq!(catalog.source, CapabilitySource::Bundled);
}

#[test]
fn process_timeout_uses_conservative_fallback() {
    let root = tempdir().unwrap();
    let paths = paths(root.path());
    let runner = CountingRunner {
        calls: AtomicUsize::new(0),
        fail_live: false,
        timeout: true,
    };
    let manager = CatalogManager {
        paths: &paths,
        runner: &runner,
    };
    let catalog = manager
        .load(
            &CatalogRequest {
                installation: installation(),
                timeout: Duration::from_millis(1),
                max_age: Duration::from_secs(60),
                include_hidden: true,
            },
            false,
            false,
        )
        .unwrap();
    assert_eq!(catalog.source, CapabilitySource::Fallback);
    assert!(!catalog.models[0].max_available());
}

#[test]
fn corrupt_cache_digest_is_rejected() {
    let root = tempdir().unwrap();
    let paths = paths(root.path());
    let runner = CountingRunner::new();
    let manager = CatalogManager {
        paths: &paths,
        runner: &runner,
    };
    let request = CatalogRequest {
        installation: installation(),
        timeout: Duration::from_secs(1),
        max_age: Duration::from_secs(60),
        include_hidden: true,
    };
    manager.load(&request, false, false).unwrap();
    let cache_path = manager.cache_path(&request.installation);
    let mut value: serde_json::Value =
        serde_json::from_slice(&std::fs::read(&cache_path).unwrap()).unwrap();
    value["payload_sha256"] = serde_json::Value::String("bad".into());
    std::fs::write(&cache_path, serde_json::to_vec(&value).unwrap()).unwrap();
    let source = CachedCatalogSource { path: cache_path };
    assert!(source.load(&request).is_err());
}

#[test]
fn concurrent_atomic_writers_leave_one_complete_value() {
    let root = tempdir().unwrap();
    let path = root.path().join("catalog.json");
    let mut workers = Vec::new();
    for index in 0..8 {
        let path = path.clone();
        workers.push(std::thread::spawn(move || {
            let bytes = serde_json::to_vec(&serde_json::json!({
                "writer": index,
                "payload": "x".repeat(4096)
            }))
            .unwrap();
            atomic_write(&path, &bytes).unwrap();
        }));
    }
    for worker in workers {
        worker.join().unwrap();
    }
    let value: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    assert_eq!(value["payload"].as_str().unwrap().len(), 4096);
}

#[cfg(unix)]
#[test]
fn explicit_binary_discovery_and_recursion_guard_work() {
    let root = tempdir().unwrap();
    let fake = common::fake_codex(root.path());
    let found = binary::resolve(Some(&fake), None).unwrap();
    assert_eq!(found.canonical_binary, fake.canonicalize().unwrap());
    assert!(binary::resolve(Some(std::path::Path::new("/missing/codex")), None).is_err());
    let current = std::env::current_exe().unwrap();
    assert!(matches!(
        binary::resolve(Some(&current), None),
        Err(cauto::AppError::CodexRecursion(_))
    ));
}

#[test]
fn exact_unsupported_ultra_is_never_reported_as_selected() {
    let catalog = parse_debug_models(
        include_bytes!("fixtures/catalog.json"),
        CapabilitySource::DebugModels,
        "test",
    )
    .unwrap();
    let error = resolve_preset(
        &catalog,
        &PresetRequest {
            model_id: Some("gpt-5.6-luna".into()),
            family: ModelFamily::Luna,
            effort: ReasoningLevel::Ultra,
            mode: LaunchMode::Interactive,
            fast_mode: FastMode::Inherit,
            explicit_model: true,
            explicit_effort: true,
            allow_downgrade: false,
        },
    )
    .unwrap_err();
    assert!(matches!(
        error,
        cauto::AppError::ExplicitDowngradeRefused(_)
    ));
}

#[test]
fn no_fast_resolves_to_standard_not_flex() {
    let catalog = parse_debug_models(
        include_bytes!("fixtures/catalog.json"),
        CapabilitySource::DebugModels,
        "test",
    )
    .unwrap();
    let resolved = resolve_preset(
        &catalog,
        &PresetRequest {
            model_id: Some("gpt-5.6-sol".into()),
            family: ModelFamily::Sol,
            effort: ReasoningLevel::High,
            mode: LaunchMode::Interactive,
            fast_mode: FastMode::NoFast,
            explicit_model: true,
            explicit_effort: true,
            allow_downgrade: false,
        },
    )
    .unwrap();
    assert_eq!(resolved.preset.service_tier.as_deref(), Some("default"));
}

#[test]
fn allowed_automatic_downgrade_uses_strongest_proven_effort() {
    let catalog = parse_debug_models(
        include_bytes!("fixtures/catalog.json"),
        CapabilitySource::DebugModels,
        "test",
    )
    .unwrap();
    let resolved = resolve_preset(
        &catalog,
        &PresetRequest {
            model_id: Some("gpt-5.6-luna".into()),
            family: ModelFamily::Luna,
            effort: ReasoningLevel::Ultra,
            mode: LaunchMode::Exec,
            fast_mode: FastMode::Inherit,
            explicit_model: false,
            explicit_effort: false,
            allow_downgrade: true,
        },
    )
    .unwrap();
    assert_eq!(resolved.preset.display_level, ReasoningLevel::Max);
    assert!(resolved.downgrade.is_some());
}
