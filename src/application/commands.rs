use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::{ExitCode, Stdio};
use std::time::Instant;

use crate::cli::{FeedbackArg, GlobalArgs, ModelsArgs, TuneArgs};
use crate::codex::args::ExplicitNativeOverrides;
use crate::error::AppError;
use crate::paths::CautoPaths;
use crate::routing::{
    EvidenceQuality, ModelFamily, SelectionConstraints, TaskType, extract_features, route,
};
use crate::state::{
    FeedbackKind, analyze_repository, append_feedback, build_report_with_calibrations, load_store,
    repository_identifier, reset_repository, save_recommendation,
};

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
        "routing_engine": "local-rust",
        "agent_hot_path_prepared": true,
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
        println!("Routing engine: local Rust (no model classifier)");
        println!("Agent hot path prepared: true");
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
    let paths = CautoPaths::discover()?;
    let report = build_report_with_calibrations(&paths.decisions(), &paths.calibration())?;
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&report)
                .map_err(|error| AppError::Serialization(error.to_string()))?
        );
    } else {
        println!(
            "Decisions: {} total ({} launched, including {} adaptive-agent sessions; {} preview; {} legacy/untyped)",
            report.total_decisions,
            report.total_launched_decisions,
            report.total_agent_decisions,
            report.total_preview_decisions,
            report.total_legacy_decisions,
        );
        println!(
            "Average confidence (launched): {}%",
            (u32::from(report.average_confidence_basis_points) + 50) / 100
        );
        if report.legacy_classifier_sample_count > 0 {
            println!(
                "Legacy classifier invocation/failure ({} old decisions): {}% / {}%",
                report.legacy_classifier_sample_count,
                report.legacy_classifier_invocation_rate_basis_points / 100,
                report.legacy_classifier_failure_rate_basis_points / 100
            );
        }
        println!(
            "Catalog fallback/downgrade (launched): {}% / {}%",
            report.catalog_fallback_rate_basis_points / 100,
            report.downgrade_rate_basis_points / 100
        );
        println!("Routes (launched): {:?}", report.route_distribution);
        println!("Route sources: {:?}", report.route_source_distribution);
        if report.total_agent_decisions > 0 {
            println!(
                "Native routes preserved (adaptive agent): {}%",
                report.agent_native_preserved_rate_basis_points / 100
            );
        }
        if report.routing_latency_micros.sample_count > 0 {
            println!(
                "Local routing latency ({} samples): p50={}us, p95={}us, max={}us",
                report.routing_latency_micros.sample_count,
                report.routing_latency_micros.p50,
                report.routing_latency_micros.p95,
                report.routing_latency_micros.max,
            );
        }
        if !report.agent_route_distribution.is_empty() {
            println!(
                "Routes (adaptive agent): {:?}",
                report.agent_route_distribution
            );
        }
        if !report.preview_route_distribution.is_empty() {
            println!("Routes (preview): {:?}", report.preview_route_distribution);
        }
        if !report.legacy_route_distribution.is_empty() {
            println!(
                "Routes (legacy/untyped): {:?}",
                report.legacy_route_distribution
            );
        }
        println!(
            "Unresolved generic baseline: {} / {} launched decisions ({}%)",
            report.unresolved_generic_baseline_decisions,
            report.total_launched_decisions,
            report.unresolved_generic_baseline_rate_basis_points / 100,
        );
        println!("Feedback: {:?}", report.feedback_distribution);
        println!(
            "Feedback sources: {:?}",
            report.feedback_source_distribution
        );
        println!("Feedback by route: {:?}", report.feedback_by_route);
        println!("Feedback by repository:");
        for repository in &report.feedback_by_repository {
            println!(
                "  {} [{}]: eligible={}, tuning={}, calibration={}, previews excluded={}, native-preserved excluded={}",
                repository.repository_name,
                repository.repository_identifier,
                repository.eligible_feedback_count,
                repository.status,
                repository
                    .current_calibration
                    .map_or_else(|| "none".into(), |offset| format!("{offset:+}")),
                repository.previews_excluded,
                repository.native_preserved_excluded,
            );
        }
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

pub(super) fn run_tune(global: &GlobalArgs, args: TuneArgs) -> Result<ExitCode, AppError> {
    let current = std::env::current_dir().map_err(|source| AppError::Io {
        path: PathBuf::from("."),
        source,
    })?;
    let repository = crate::context::repository::discover(global.repo.as_deref(), &current)?;
    let paths = CautoPaths::discover()?;
    let repository_id = repository_identifier(&repository.root);
    let mut store = load_store(&paths.calibration())?;

    if args.reset {
        let removed = reset_repository(&paths.calibration(), &mut store, &repository_id)?;
        if global.json {
            println!(
                "{}",
                serde_json::to_string_pretty(&serde_json::json!({
                    "schema_version": 1,
                    "repository_identifier": repository_id,
                    "reset": removed.is_some(),
                    "removed_calibration": removed,
                }))
                .map_err(|error| AppError::Serialization(error.to_string()))?
            );
        } else if !global.quiet {
            match removed {
                Some(offset) => println!(
                    "Reset calibration for {} [{}]: {offset:+} -> none",
                    repository.name, repository_id
                ),
                None => println!(
                    "No calibration was applied for {} [{}]; nothing changed",
                    repository.name, repository_id
                ),
            }
        }
        return Ok(ExitCode::SUCCESS);
    }

    let analysis = analyze_repository(
        &paths.decisions(),
        &store,
        Some((&repository_id, &repository.name)),
    )?;
    let tuning = analysis
        .repositories
        .first()
        .expect("repository filter always produces one analysis");
    let changed = if args.apply {
        save_recommendation(&paths.calibration(), &mut store, tuning)?
    } else {
        None
    };
    if global.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "schema_version": 1,
                "read_only": !args.apply,
                "analysis": analysis,
                "change": changed.map(|(before, after)| serde_json::json!({
                    "repository_identifier": repository_id,
                    "before": before,
                    "after": after,
                })),
            }))
            .map_err(|error| AppError::Serialization(error.to_string()))?
        );
    } else if !global.quiet {
        println!("Repository: {} [{}]", tuning.repository_name, repository_id);
        println!(
            "Eligible feedback: {} (right={}, underpowered={}, overkill={}; diagnostic failures={})",
            tuning.eligible_feedback_count,
            tuning.feedback.right,
            tuning.feedback.underpowered,
            tuning.feedback.overkill,
            tuning.feedback.failed_for_other_reason,
        );
        println!("Preview feedback excluded: {}", tuning.previews_excluded);
        println!(
            "Native-preserved feedback excluded: {}",
            tuning.native_preserved_excluded
        );
        println!(
            "Current calibration: {}",
            tuning
                .current_calibration
                .map_or_else(|| "none".into(), |offset| format!("{offset:+} points"))
        );
        println!(
            "Recommendation: {}",
            tuning
                .proposed_calibration
                .map_or_else(|| "none".into(), |offset| format!("{offset:+} points"))
        );
        println!("Status: {}", tuning.status);
        println!("Reason: {}", tuning.reason);
        if !args.apply {
            println!(
                "Read-only analysis; run `cauto tune --apply` to approve this repository's recommendation"
            );
        } else {
            match changed {
                Some((before, after)) => println!(
                    "Changed calibration for {} [{}]: {} -> {after:+} points",
                    tuning.repository_name,
                    repository_id,
                    before.map_or_else(|| "none".into(), |value| format!("{value:+} points")),
                ),
                None => println!("No calibration change was applied"),
            }
        }
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
