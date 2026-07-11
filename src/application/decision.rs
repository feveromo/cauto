use std::ffi::OsString;

use sha2::{Digest, Sha256};

use crate::codex::catalog::ModelCatalog;
use crate::codex::launch::{InjectionPolicy, materialize_args};
use crate::context::ContextSnapshot;
use crate::error::AppError;
use crate::paths::CautoPaths;
use crate::routing::{LaunchPlan, RouteDecision};
use crate::state::{
    DecisionRecord, append_decision, prompt_sha256, repository_identifier, sanitize_argv,
};

use super::prompt::PromptInput;

pub(super) struct DecisionLogInput<'a> {
    pub paths: &'a CautoPaths,
    pub context: &'a ContextSnapshot,
    pub catalog: &'a ModelCatalog,
    pub prompt: &'a PromptInput,
    pub decision: &'a RouteDecision,
    pub plan: &'a LaunchPlan,
    pub policy: InjectionPolicy,
    pub classifier_ran: bool,
    pub classifier_outcome: &'a str,
    pub preview: bool,
    pub strict: bool,
    pub quiet: bool,
}

pub(super) fn write(input: DecisionLogInput<'_>) -> Result<(), AppError> {
    if std::env::var_os("CAUTO_DISABLE_LOG").is_some() {
        return Ok(());
    }
    let empty = OsString::new();
    let prompt_os = input.prompt.original.as_deref().unwrap_or(&empty);
    let prompt_hash = prompt_sha256(prompt_os);
    let timestamp = crate::state::decision_log::timestamp_now();
    let mut id_hasher = Sha256::new();
    id_hasher.update(timestamp.as_bytes());
    id_hasher.update(prompt_hash.as_bytes());
    id_hasher.update(std::process::id().to_le_bytes());
    let decision_id = format!("{:x}", id_hasher.finalize());
    let mut without_prompt = input.plan.clone();
    without_prompt.prompt = None;
    let argv = materialize_args(&without_prompt, input.policy);
    let raising_rule_ids = input
        .decision
        .matched_rules
        .iter()
        .filter(|matched| {
            matched.effort_floor.is_some()
                || matched.family_floor.is_some()
                || [
                    matched.dimension_effects.scope,
                    matched.dimension_effects.ambiguity,
                    matched.dimension_effects.cost_of_being_wrong,
                    matched.dimension_effects.runtime_dependence,
                    matched.dimension_effects.architectural_depth,
                    matched.dimension_effects.verification_burden,
                ]
                .into_iter()
                .any(|delta| delta > 0)
        })
        .map(|matched| matched.rule_id.clone())
        .collect();
    let lowering_rule_ids = input
        .decision
        .matched_rules
        .iter()
        .filter(|matched| {
            matched.effort_ceiling.is_some()
                || matched.family_ceiling.is_some()
                || [
                    matched.dimension_effects.scope,
                    matched.dimension_effects.ambiguity,
                    matched.dimension_effects.cost_of_being_wrong,
                    matched.dimension_effects.runtime_dependence,
                    matched.dimension_effects.architectural_depth,
                    matched.dimension_effects.verification_burden,
                ]
                .into_iter()
                .any(|delta| delta < 0)
        })
        .map(|matched| matched.rule_id.clone())
        .collect();
    let record = DecisionRecord {
        schema_version: 1,
        record_type: "decision".into(),
        decision_mode: if input.preview { "preview" } else { "launched" }.into(),
        decision_id,
        timestamp,
        cauto_version: env!("CARGO_PKG_VERSION").into(),
        codex_version: input.catalog.codex_version.clone(),
        repository_identifier: repository_identifier(&input.context.repository.root),
        repository_name: input.context.repository.name.clone(),
        git_branch: input.context.git.branch.clone(),
        prompt_sha256: prompt_hash,
        prompt_byte_length: input.prompt.byte_length,
        task_type: input.decision.task_type.clone(),
        dimensions: input.decision.dimensions,
        complexity_score: input.decision.normalized_score,
        calibration: input.decision.calibration.clone(),
        confidence_basis_points: input.decision.confidence.basis_points(),
        matched_rule_ids: input
            .decision
            .matched_rules
            .iter()
            .map(|matched| matched.rule_id.clone())
            .collect(),
        raising_rule_ids,
        lowering_rule_ids,
        conflicts: input.decision.conflicts.clone(),
        selected_model: input.plan.preset.model_id.clone(),
        selected_family: input.plan.preset.model_family.clone(),
        selected_effort: input.plan.preset.display_level,
        ultra_candidate: input.decision.ultra_candidate,
        ultra_selected: input.decision.ultra_selected,
        classifier_ran: input.classifier_ran,
        classifier_outcome: input.classifier_outcome.into(),
        catalog_source: input.plan.preset.source.clone(),
        downgrade: input.plan.downgrade.clone(),
        sanitized_argv: sanitize_argv(&argv),
        feedback: None,
    };
    match append_decision(&input.paths.decisions(), &record) {
        Ok(()) => Ok(()),
        Err(error) if input.strict => Err(error),
        Err(error) => {
            if !input.quiet {
                eprintln!("cauto: warning: decision logging failed: {error}");
            }
            Ok(())
        }
    }
}
