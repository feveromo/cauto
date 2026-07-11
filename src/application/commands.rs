use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Instant;

use crate::cli::{FeedbackArg, GlobalArgs, ModelsArgs};
use crate::codex::args::ExplicitNativeOverrides;
use crate::error::AppError;
use crate::paths::CautoPaths;
use crate::routing::{
    EvidenceQuality, ModelFamily, SelectionConstraints, TaskType, extract_features, route,
};
use crate::state::{FeedbackKind, append_feedback, build_report, repository_identifier};

use super::{catalog_for, load_context_and_config, resolve_installation};

fn feedback_kind(value: FeedbackArg) -> FeedbackKind {
    match value {
        FeedbackArg::Right => FeedbackKind::Right,
        FeedbackArg::Overkill => FeedbackKind::Overkill,
        FeedbackArg::Underpowered => FeedbackKind::Underpowered,
        FeedbackArg::FailedForOtherReason => FeedbackKind::FailedForOtherReason,
    }
}

pub(super) fn run_models(global: &GlobalArgs, args: ModelsArgs) -> Result<ExitCode, AppError> {
    let (context, loaded, paths) = load_context_and_config(global, None)?;
    let native = ExplicitNativeOverrides {
        profile: global.profile.clone(),
        ..ExplicitNativeOverrides::default()
    };
    let installation = resolve_installation(global, &native)?;
    let mut catalog = catalog_for(&paths, &loaded, &installation, args.refresh, args.bundled)?;
    if !args.include_hidden {
        catalog.models.retain(|model| !model.hidden);
    }
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&catalog)
                .map_err(|error| AppError::Serialization(error.to_string()))?
        );
    } else {
        println!(
            "Codex: {}\nCatalog: {:?}{}\nRepository: {}",
            catalog.codex_version,
            catalog.source,
            if catalog.stale { " (stale)" } else { "" },
            context.repository.root.display()
        );
        for model in &catalog.models {
            println!(
                "{}\t{}\tdefault={}\tefforts={}\ttiers={}\tinputs={}\thidden={}\tmax={}\tultra={}\tinteractive={}\texec={}\tapp-server-only={}",
                model.id,
                model.family,
                model.default_reasoning_effort,
                model.supported_reasoning_efforts.join(","),
                model
                    .service_tiers
                    .iter()
                    .map(|tier| tier.id.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                model.input_modalities.join(","),
                model.hidden,
                model.max_available(),
                model.ultra_available(),
                model.interactive_supported,
                model.exec_supported,
                model.app_server_only,
            );
        }
    }
    Ok(ExitCode::SUCCESS)
}

pub(super) fn run_doctor(global: &GlobalArgs) -> Result<ExitCode, AppError> {
    let (context, loaded, paths) = load_context_and_config(global, None)?;
    let native = ExplicitNativeOverrides {
        profile: global.profile.clone(),
        ..ExplicitNativeOverrides::default()
    };
    let installation = resolve_installation(global, &native)?;
    let catalog = catalog_for(&paths, &loaded, &installation, false, false)?;
    let luna = catalog.first_family(&ModelFamily::Luna);
    let sol = catalog.first_family(&ModelFamily::Sol);
    let terra = catalog.first_family(&ModelFamily::Terra);
    let report = serde_json::json!({
        "cauto_version": env!("CARGO_PKG_VERSION"),
        "rust_target": format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
        "codex_binary": installation.binary,
        "codex_version": catalog.codex_version,
        "codex_home": installation.codex_home,
        "user_config": loaded.user_path,
        "user_config_loaded": loaded.user_loaded,
        "project_config": loaded.project_path,
        "project_config_loaded": loaded.project_loaded,
        "cache_dir": paths.cache_dir,
        "state_dir": paths.state_dir,
        "catalog_source": catalog.source,
        "catalog_age_seconds": catalog.cache_age_seconds,
        "catalog_stale": catalog.stale,
        "native_unix_exec": cfg!(unix),
        "classifier_usable": luna.is_some() && std::env::var_os("CAUTO_CLASSIFIER").is_none(),
        "aliases": {
            "sol": sol.map(|model| model.id.as_str()),
            "terra": terra.map(|model| model.id.as_str()),
            "luna": luna.map(|model| model.id.as_str()),
        },
        "max": {
            "sol": sol.is_some_and(|model| model.max_available()),
            "terra": terra.is_some_and(|model| model.max_available()),
            "luna": luna.is_some_and(|model| model.max_available()),
        },
        "ultra": {
            "sol": sol.is_some_and(|model| model.ultra_available()),
            "terra": terra.is_some_and(|model| model.ultra_available()),
            "luna": luna.is_some_and(|model| model.ultra_available()),
        },
        "repository": context.repository.root,
    });
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| AppError::Serialization(error.to_string()))?
        );
    } else {
        println!("cauto {}", env!("CARGO_PKG_VERSION"));
        println!(
            "Rust target: {}-{}",
            std::env::consts::ARCH,
            std::env::consts::OS
        );
        println!("Codex binary: {}", installation.binary.display());
        println!("Codex version: {}", catalog.codex_version);
        println!("CODEX_HOME: {}", installation.codex_home.display());
        println!(
            "User config: {} ({})",
            loaded.user_path.display(),
            if loaded.user_loaded {
                "loaded"
            } else {
                "not found"
            }
        );
        let project_path = loaded
            .project_path
            .as_deref()
            .map(Path::display)
            .map(|value| value.to_string())
            .unwrap_or_else(|| "none".into());
        println!(
            "Project config: {} ({})",
            project_path,
            if loaded.project_loaded {
                "loaded"
            } else {
                "not found"
            }
        );
        println!("Cache: {}", paths.cache_dir.display());
        println!("State: {}", paths.state_dir.display());
        println!(
            "Catalog: {:?}, age={}s, stale={}",
            catalog.source,
            catalog.cache_age_seconds.unwrap_or(0),
            catalog.stale
        );
        println!("Native Unix exec: {}", cfg!(unix));
        println!("Classifier usable: {}", luna.is_some());
        println!(
            "Aliases: sol={}, terra={}, luna={}",
            sol.map(|model| model.id.as_str()).unwrap_or("unavailable"),
            terra
                .map(|model| model.id.as_str())
                .unwrap_or("unavailable"),
            luna.map(|model| model.id.as_str()).unwrap_or("unavailable")
        );
        println!(
            "Max: sol={}, terra={}, luna={}",
            sol.is_some_and(|model| model.max_available()),
            terra.is_some_and(|model| model.max_available()),
            luna.is_some_and(|model| model.max_available())
        );
        println!(
            "Ultra: sol={}, terra={}, luna={}",
            sol.is_some_and(|model| model.ultra_available()),
            terra.is_some_and(|model| model.ultra_available()),
            luna.is_some_and(|model| model.ultra_available())
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub(super) fn run_feedback(
    global: &GlobalArgs,
    feedback: FeedbackArg,
) -> Result<ExitCode, AppError> {
    let current = std::env::current_dir().map_err(|source| AppError::Io {
        path: PathBuf::from("."),
        source,
    })?;
    let repository = crate::context::repository::discover(global.repo.as_deref(), &current)?;
    let paths = CautoPaths::discover()?;
    let id = append_feedback(
        &paths.decisions(),
        &repository_identifier(&repository.root),
        feedback_kind(feedback),
    )?;
    if global.json {
        println!(
            "{}",
            serde_json::json!({"schema_version": 1, "decision_id": id, "recorded": true})
        );
    } else {
        println!("Feedback recorded for decision {id}");
    }
    Ok(ExitCode::SUCCESS)
}

pub(super) fn run_report(global: &GlobalArgs) -> Result<ExitCode, AppError> {
    let report = build_report(&CautoPaths::discover()?.decisions())?;
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| AppError::Serialization(error.to_string()))?
        );
    } else {
        println!("Decisions: {}", report.total_decisions);
        println!(
            "Average confidence: {}%",
            (u32::from(report.average_confidence_basis_points) + 50) / 100
        );
        println!(
            "Classifier invocation/failure: {}% / {}%",
            report.classifier_invocation_rate_basis_points / 100,
            report.classifier_failure_rate_basis_points / 100
        );
        println!(
            "Catalog fallback/downgrade: {}% / {}%",
            report.catalog_fallback_rate_basis_points / 100,
            report.downgrade_rate_basis_points / 100
        );
        println!("Routes: {:?}", report.route_distribution);
        println!("Feedback: {:?}", report.feedback_distribution);
        println!("Feedback by route: {:?}", report.feedback_by_route);
        println!(
            "Rules raising effort: {:?}",
            report.rules_most_often_raising_effort
        );
        println!(
            "Rules lowering effort: {:?}",
            report.rules_most_often_lowering_effort
        );
    }
    Ok(ExitCode::SUCCESS)
}

pub(super) fn run_score_benchmark(iterations: u64) -> Result<ExitCode, AppError> {
    let features = extract_features(
        "diagnose a bounded runtime bug, add a focused contract, and live-validate expected behavior",
    );
    let start = Instant::now();
    let mut checksum = 0_u64;
    for _ in 0..iterations {
        let decision = route(
            TaskType::Coding,
            std::hint::black_box(features.dimensions),
            crate::routing::Weights::default(),
            SelectionConstraints::default(),
            vec![],
            vec![],
            EvidenceQuality::default(),
            vec![],
            vec![],
        );
        checksum = checksum.wrapping_add(u64::from(decision.normalized_score));
    }
    let elapsed = start.elapsed();
    println!(
        "iterations={iterations} total_ns={} ns_per_score={} checksum={checksum}",
        elapsed.as_nanos(),
        elapsed.as_nanos() / u128::from(iterations.max(1)),
    );
    Ok(ExitCode::SUCCESS)
}

pub(super) fn run_process_benchmark(
    program: &Path,
    args: &[OsString],
    iterations: u64,
) -> Result<ExitCode, AppError> {
    let iterations = iterations.max(1);
    let mut samples = Vec::with_capacity(iterations as usize);
    for _ in 0..iterations {
        let start = Instant::now();
        let status = std::process::Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|source| AppError::Io {
                path: program.to_path_buf(),
                source,
            })?;
        if !status.success() {
            return Err(AppError::InvalidArguments(format!(
                "benchmarked command exited with {status}"
            )));
        }
        samples.push(start.elapsed().as_nanos());
    }
    samples.sort_unstable();
    let median = samples[samples.len() / 2];
    let p95 = samples[(samples.len() * 95 / 100).min(samples.len() - 1)];
    let mean = samples.iter().sum::<u128>() / u128::from(iterations);
    println!("iterations={iterations} median_ns={median} mean_ns={mean} p95_ns={p95}");
    Ok(ExitCode::SUCCESS)
}

fn benchmark_loop<T>(mut operation: impl FnMut() -> T, iterations: u64) -> u128 {
    let iterations = iterations.max(1);
    let start = Instant::now();
    for _ in 0..iterations {
        std::hint::black_box(operation());
    }
    start.elapsed().as_nanos() / u128::from(iterations)
}

pub(super) fn run_core_benchmark(
    policy_path: &Path,
    catalog_path: &Path,
    iterations: u64,
) -> Result<ExitCode, AppError> {
    let policy_text = std::fs::read_to_string(policy_path).map_err(|source| AppError::Io {
        path: policy_path.to_path_buf(),
        source,
    })?;
    let catalog_bytes = std::fs::read(catalog_path).map_err(|source| AppError::Io {
        path: catalog_path.to_path_buf(),
        source,
    })?;
    let policy: crate::config::ProjectPolicy =
        toml::from_str(&policy_text).map_err(|error| AppError::ConfigParse {
            path: policy_path.to_path_buf(),
            message: error.to_string(),
        })?;
    let raw = crate::config::RawConfig::from(policy);
    let validated = crate::config::validate::into_validated(raw.clone(), policy_path)?;
    let compiled = crate::routing::CompiledRules::new(validated.rules.clone())?;
    let features = extract_features(
        "benchmark phrase 050 c update src/module050/file.rs with exact expected behavior",
    );
    let application = compiled.evaluate(
        &features.normalized,
        &features.explicit_paths,
        features.dimensions,
    );
    let decision = route(
        TaskType::Coding,
        application.dimensions,
        validated.weights,
        application.constraints,
        application.matches,
        application.conflicts,
        EvidenceQuality::default(),
        vec![],
        vec![],
    );
    let parse_iterations = iterations.max(10);
    let config_ns = benchmark_loop(
        || {
            let parsed: crate::config::ProjectPolicy = toml::from_str(&policy_text).unwrap();
            crate::config::RawConfig::default().merge(parsed.into())
        },
        parse_iterations,
    );
    let compile_ns = benchmark_loop(
        || crate::routing::CompiledRules::new(validated.rules.clone()).unwrap(),
        parse_iterations.min(2_000),
    );
    let match_ns = benchmark_loop(
        || {
            compiled.evaluate(
                &features.normalized,
                &features.explicit_paths,
                features.dimensions,
            )
        },
        parse_iterations * 10,
    );
    let catalog_ns = benchmark_loop(
        || {
            crate::cache::CacheEnvelope::<crate::codex::catalog::ModelCatalog>::parse(
                &catalog_bytes,
            )
            .unwrap()
        },
        parse_iterations,
    );
    let decision_json_ns = benchmark_loop(
        || serde_json::to_vec(&decision).unwrap(),
        parse_iterations * 10,
    );
    let score_ns = benchmark_loop(
        || {
            crate::routing::normalized_score(
                std::hint::black_box(features.dimensions),
                std::hint::black_box(validated.weights),
            )
        },
        parse_iterations * 1_000,
    );
    println!("config_parse_merge_ns={config_ns}");
    println!("rule_compile_ns={compile_ns}");
    println!("phrase_path_match_ns={match_ns}");
    println!("catalog_deserialize_ns={catalog_ns}");
    println!("decision_serialize_ns={decision_json_ns}");
    println!("score_ns={score_ns}");
    Ok(ExitCode::SUCCESS)
}
