use std::collections::HashSet;
use std::ffi::OsString;
use std::process::ExitCode;
use std::str::FromStr;

use crate::classifier;
use crate::cli::{GlobalArgs, RouteArgs};
use crate::codex::args::{ExplicitNativeOverrides, inspect_forwarded};
use crate::codex::capabilities::{PresetRequest, resolve_preset};
use crate::codex::launch::{InjectionPolicy, materialize_args, preview};
use crate::error::AppError;
use crate::output;
use crate::routing::{
    CapabilitySource, ClassifierMode, Confidence, EvidenceQuality, FastMode, LaunchMode,
    LaunchPlan, ModelFamily, Reason, ReasoningLevel, RuleSource, TaskType, extract_features, route,
};

use super::decision::{DecisionLogInput, write as write_decision};
use super::prompt;
use super::{catalog_for, load_context_and_config, resolve_installation};

pub(super) struct ResolvedRoute {
    pub context: crate::context::ContextSnapshot,
    pub loaded: crate::config::LoadedConfig,
    pub paths: crate::paths::CautoPaths,
    pub catalog: crate::codex::catalog::ModelCatalog,
    pub prompt: prompt::PromptInput,
    pub decision: crate::routing::RouteDecision,
    pub plan: LaunchPlan,
    pub policy: InjectionPolicy,
    pub classifier_ran: bool,
    pub classifier_outcome: String,
}

fn effective_fast(args: &RouteArgs, config: &crate::config::LoadedConfig) -> FastMode {
    if args.fast {
        FastMode::Fast
    } else if args.no_fast {
        FastMode::NoFast
    } else if args.inherit_fast {
        FastMode::Inherit
    } else {
        config.config.fast_mode
    }
}

fn effective_classifier(args: &RouteArgs, config: &crate::config::LoadedConfig) -> ClassifierMode {
    if args.no_classifier {
        ClassifierMode::Never
    } else if let Some(value) = &args.classifier {
        ClassifierMode::from_str(value).expect("clap validated classifier mode")
    } else {
        config.config.classifier
    }
}

fn count_rule_sources(matches: &[crate::routing::RuleMatch]) -> u8 {
    let mut sources = HashSet::new();
    for matched in matches {
        sources.insert(match matched.source {
            RuleSource::Builtin => 0,
            RuleSource::User => 1,
            RuleSource::Project => 2,
        });
    }
    sources.len() as u8
}

fn reconcile_explicit(args: &RouteArgs, native: &ExplicitNativeOverrides) -> Result<(), AppError> {
    if args.model.is_some() && args.family.is_some() {
        return Err(AppError::InvalidArguments(
            "--model and --family are mutually exclusive".into(),
        ));
    }
    if let (Some(cauto), Some(forwarded)) = (&args.model, &native.model)
        && cauto != forwarded
    {
        return Err(AppError::InvalidArguments(format!(
            "cauto --model {cauto:?} conflicts with forwarded model {forwarded:?}; use one override"
        )));
    }
    if args.family.is_some() && native.model.is_some() {
        return Err(AppError::InvalidArguments(
            "cauto --family conflicts with a forwarded native model; use one override".into(),
        ));
    }
    if let (Some(cauto), Some(forwarded)) = (&args.effort, native.effort)
        && ReasoningLevel::from_str(cauto).expect("clap validated effort") != forwarded
    {
        return Err(AppError::InvalidArguments(format!(
            "cauto --effort {cauto:?} conflicts with forwarded effort {}; use one override",
            forwarded.native_name()
        )));
    }
    if (args.fast || args.no_fast || args.inherit_fast) && native.service_tier.is_some() {
        return Err(AppError::InvalidArguments(
            "cauto Fast-mode flags conflict with a forwarded service_tier; use one override".into(),
        ));
    }
    Ok(())
}

pub(super) fn resolve_route(
    global: &GlobalArgs,
    args: &RouteArgs,
    mode: LaunchMode,
    explain: bool,
    agent_session: bool,
) -> Result<ResolvedRoute, AppError> {
    if args
        .forwarded
        .first()
        .and_then(|argument| argument.to_str())
        == Some("resume")
    {
        return Err(AppError::InvalidArguments(
            "the one-shot launcher does not reroute resumed sessions; use `cauto agent --resume THREAD_ID` or native `codex resume`".into(),
        ));
    }
    let prompt = prompt::acquire(args, mode)?;
    let native = inspect_forwarded(&args.forwarded)?;
    reconcile_explicit(args, &native)?;
    let (context, loaded, paths) = load_context_and_config(global, Some(args))?;
    let installation = resolve_installation(global, &native)?;
    let catalog = catalog_for(&paths, &loaded, &installation, false, args.offline)?;
    let features = extract_features(&prompt.analysis);
    let compiled = crate::routing::CompiledRules::new(loaded.config.rules.clone())?;
    let applied = compiled.evaluate(
        &features.normalized,
        &features.explicit_paths,
        features.dimensions,
    );
    let mut constraints = applied.constraints;
    constraints.hysteresis_points = loaded.config.hysteresis_points;
    let repository_id = crate::state::repository_identifier(&context.repository.root);
    match crate::state::load_calibration(&paths.calibration(), &repository_id) {
        Ok(Some(calibration)) => constraints.calibration = calibration,
        Ok(None) => {}
        Err(error) => {
            // Calibration is optional state and must never block baseline routing.
            if global.verbose {
                eprintln!("cauto: calibration unavailable; using baseline routing: {error}");
            }
        }
    }
    if !agent_session && constraints.hysteresis_points > 0 {
        match crate::state::decision_log::latest_route(&paths.decisions(), &repository_id) {
            Ok(Some((family, effort))) => {
                constraints.prior_family = Some(family);
                constraints.prior_effort = Some(effort);
            }
            Ok(None) => {}
            Err(error) => {
                if global.verbose {
                    eprintln!("cauto: decision history unavailable for hysteresis: {error}");
                }
            }
        }
    }
    constraints.explicit_family = args
        .family
        .as_deref()
        .map(ModelFamily::from_str)
        .transpose()
        .map_err(AppError::InvalidArguments)?
        .or_else(|| {
            args.model
                .as_deref()
                .or(native.model.as_deref())
                .map(ModelFamily::from_model_id)
        });
    constraints.explicit_effort = args
        .effort
        .as_deref()
        .map(ReasoningLevel::from_str)
        .transpose()
        .map_err(AppError::InvalidArguments)?
        .or(native.effort);
    if prompt.original.is_none() {
        constraints
            .explicit_family
            .get_or_insert_with(|| ModelFamily::from_model_id(&loaded.config.default_model));
        constraints
            .explicit_effort
            .get_or_insert(loaded.config.default_effort);
    }
    let instruction_authorization = context.agents.delegation_authorized_by_instructions
        && !context.agents.delegation_requires_explicit_request;
    constraints.ultra_authorized = args.allow_ultra
        || features.delegation_requested
        || instruction_authorization
        || (!loaded.config.ultra_requires_opt_in
            && !context.agents.delegation_requires_explicit_request)
        || constraints.explicit_effort == Some(ReasoningLevel::Ultra);
    constraints.meaningful_parallel_tracks = features.meaningful_parallel_tracks
        || applied
            .matches
            .iter()
            .filter(|matched| matched.dimension_effects.parallelizability > 0)
            .count()
            >= 2;
    let mut reasons = features.reasons.clone();
    reasons.extend(applied.matches.iter().map(|matched| Reason {
        label: matched.reason.clone(),
        contribution: i16::from(
            matched.dimension_effects.scope
                + matched.dimension_effects.ambiguity
                + matched.dimension_effects.cost_of_being_wrong
                + matched.dimension_effects.runtime_dependence
                + matched.dimension_effects.architectural_depth
                + matched.dimension_effects.verification_burden,
        ) * 5,
    }));
    let evidence = EvidenceQuality {
        matched_rule_count: applied.matches.len() as u16,
        independent_rule_sources: count_rule_sources(&applied.matches),
        explicit_path_count: features.explicit_paths.len() as u16,
        clear_reproduction: features.clear_reproduction,
        clear_completion: features.clear_completion,
        known_repository: context.repository.has_git || loaded.project_loaded,
        vague_prompt: features.vague_prompt,
        unknown_catalog: catalog.source == CapabilitySource::Fallback,
        malformed_agents: context.agents.malformed_or_truncated,
        dirty_metadata: matches!(context.git.state, crate::context::GitState::Dirty),
        conflict_count: 0,
        rule_confidence_delta: applied.confidence_delta_basis_points,
    };
    let mut decision = route(
        features.task_type.clone(),
        applied.dimensions,
        loaded.config.weights,
        constraints.clone(),
        applied.matches.clone(),
        applied.conflicts.clone(),
        evidence,
        reasons.clone(),
        features.escalation_signals.clone(),
    );
    let mut classifier_ran = false;
    let mut classifier_outcome = "skipped".to_owned();
    let classifier_mode = effective_classifier(args, &loaded);
    let luna = catalog.first_family(&ModelFamily::Luna);
    let deterministic_semantic_gap = features.task_type == TaskType::Coding
        && features.reasons.is_empty()
        && features.escalation_signals.is_empty()
        && decision.matched_rules.is_empty();
    let classifier_candidate = classifier_mode != ClassifierMode::Auto
        || deterministic_semantic_gap
        || !decision.conflicts.is_empty();
    let classifier_would_run = classifier_candidate
        && classifier::should_run(
            classifier_mode,
            decision.confidence,
            loaded.config.classifier_confidence_threshold_basis_points,
            !decision.conflicts.is_empty(),
            decision.matched_rules.len(),
            prompt.valid_utf8,
            (args.model.is_some() || native.model.is_some())
                && (args.effort.is_some() || native.effort_raw.is_some()),
            args.offline,
            luna.is_some(),
        )
        && prompt.original.is_some();
    if classifier_would_run && (args.dry_run || explain) && !args.run_classifier {
        classifier_outcome = "would-run".into();
    } else if classifier_would_run && let (Some(luna), Some(_)) = (luna, prompt.original.as_ref()) {
        classifier_ran = true;
        let classifier_prompt =
            classifier::build_classifier_prompt(&prompt.analysis, &context, &decision)
                .map_err(|error| AppError::Serialization(error.to_string()))?;
        match classifier::runner::run(
            &installation,
            &luna.id,
            &classifier_prompt,
            loaded.config.classifier_timeout.duration(),
        ) {
            Ok(result) => {
                classifier_outcome = result.category;
                let dimensions =
                    classifier::blend_dimensions(decision.dimensions, &result.assessment);
                let task_type = if matches!(
                    features.task_type,
                    TaskType::Empty | TaskType::Documentation | TaskType::Mechanical
                ) {
                    features.task_type.clone()
                } else {
                    result.assessment.task_type.clone()
                };
                let mut merged_reasons = reasons;
                merged_reasons.extend(result.assessment.reasons.clone());
                let mut merged_signals = features.escalation_signals.clone();
                merged_signals.extend(result.assessment.escalation_signals.clone());
                let deterministic_confidence = decision.confidence.basis_points();
                decision = route(
                    task_type,
                    dimensions,
                    loaded.config.weights,
                    constraints.clone(),
                    applied.matches.clone(),
                    applied.conflicts.clone(),
                    evidence,
                    merged_reasons,
                    merged_signals,
                );
                let blended_confidence = (u32::from(deterministic_confidence) * 7
                    + u32::from(result.assessment.confidence.basis_points()) * 3
                    + 5)
                    / 10;
                decision.confidence = Confidence::from_basis_points(blended_confidence as u16)
                    .expect("blended confidence is bounded");
            }
            Err(error) => {
                classifier_outcome = match error {
                    classifier::ClassifierError::Timeout => "timeout",
                    classifier::ClassifierError::InvalidOutput(_) => "invalid-output",
                    classifier::ClassifierError::Exit => "nonzero-exit",
                    classifier::ClassifierError::Nested => "nested-refused",
                    classifier::ClassifierError::Temporary(_) => "temporary-error",
                    classifier::ClassifierError::Launch(_) => "launch-error",
                }
                .into();
                if global.verbose {
                    eprintln!("cauto: classifier {classifier_outcome}; using deterministic route");
                }
            }
        }
    }

    let exact_model = args
        .model
        .clone()
        .or_else(|| native.model.clone())
        .or_else(|| (prompt.original.is_none()).then(|| loaded.config.default_model.clone()));
    let explicit_model = args.model.is_some() || native.model.is_some();
    let explicit_effort = args.effort.is_some() || native.effort_raw.is_some();
    let fast_mode = if native.service_tier.is_some() {
        FastMode::Inherit
    } else {
        effective_fast(args, &loaded)
    };
    let resolved = resolve_preset(
        &catalog,
        &PresetRequest {
            model_id: exact_model,
            family: decision.recommended_family.clone(),
            effort: decision.recommended_effort,
            mode,
            fast_mode,
            explicit_model,
            explicit_effort,
            allow_downgrade: args.allow_downgrade
                || (!explicit_model && !explicit_effort && loaded.config.allow_automatic_downgrade),
        },
    )?;
    decision.ultra_selected = resolved.preset.display_level == ReasoningLevel::Ultra;
    let mut injected_args = Vec::with_capacity(2);
    if let Some(profile) = &global.profile
        && native.profile.is_none()
    {
        injected_args.push(OsString::from("--profile"));
        injected_args.push(OsString::from(profile));
    }
    let plan = LaunchPlan {
        codex_binary: installation.binary.clone(),
        working_directory: context.repository.root.clone(),
        mode,
        preset: resolved.preset,
        inherited_args: args.forwarded.clone(),
        injected_args,
        prompt: prompt.original.clone(),
        downgrade: resolved.downgrade,
    };
    let policy = InjectionPolicy {
        inject_model: native.model.is_none(),
        inject_effort: native.effort_raw.is_none(),
        inject_service_tier: native.service_tier.is_none() && plan.preset.service_tier.is_some(),
    };
    Ok(ResolvedRoute {
        context,
        loaded,
        paths,
        catalog,
        prompt,
        decision,
        plan,
        policy,
        classifier_ran,
        classifier_outcome,
    })
}

pub(super) fn run_route(
    global: &GlobalArgs,
    args: RouteArgs,
    mode: LaunchMode,
    explain: bool,
) -> Result<ExitCode, AppError> {
    let resolved = resolve_route(global, &args, mode, explain, false)?;
    let ResolvedRoute {
        context,
        loaded,
        paths,
        catalog,
        prompt,
        decision,
        plan,
        policy,
        classifier_ran,
        classifier_outcome,
        ..
    } = resolved;
    let command_args = materialize_args(&plan, policy);
    if global.json {
        println!(
            "{}",
            output::json::render(
                &decision,
                &plan.preset,
                plan.downgrade.as_ref(),
                mode,
                &context.repository.root.to_string_lossy(),
                classifier_ran,
                &classifier_outcome,
            )
            .map_err(|error| AppError::Serialization(error.to_string()))?
        );
    } else if !global.quiet {
        print!(
            "{}",
            output::human::render(
                &decision,
                &plan.preset,
                plan.downgrade.as_ref(),
                global.verbose || explain,
            )
        );
        if let Some(warning) = &context.git.warning {
            println!("Warning: {warning}");
        }
        if let Some(warning) = &catalog.warning {
            println!("Catalog: {warning}");
        }
        if classifier_outcome == "would-run" {
            println!("Classifier: would run; pass --run-classifier to include it in this preview");
        }
    }
    if args.print_command {
        println!(
            "Command: {}",
            preview(plan.codex_binary.as_os_str(), &command_args)
        );
    }
    let _ = write_decision(DecisionLogInput {
        paths: &paths,
        context: &context,
        catalog: &catalog,
        prompt: &prompt,
        decision: &decision,
        plan: &plan,
        policy,
        classifier_ran,
        classifier_outcome: &classifier_outcome,
        decision_mode: if args.dry_run || explain {
            "preview"
        } else {
            "launched"
        },
        strict: loaded.config.strict_logging,
        quiet: global.quiet,
    })?;
    if args.dry_run || explain {
        return Ok(ExitCode::SUCCESS);
    }
    if !global.quiet && !global.json {
        println!("Launching native Codex...");
    }
    crate::codex::launch::execute(&plan, policy)
}
